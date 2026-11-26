//! Speech to text for dictation in the composer. Runs whisper.cpp locally, so nothing leaves
//! the machine and Swedish works as well as English. The first use downloads the whisper.cpp
//! binary (on Windows) and the model into `<data_dir>/voice`.

mod install;
mod text;

use crate::locked::{LockExt, ResultExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use install::{fetch, find_cli, run, server_binary, walk};
use text::{VOCABULARY, clean, echoes, folder_line, pinned_language, prompt, why_it_stopped};

pub use crate::voice_config::VoiceConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum VoiceState {
    /// Not set up yet; the first recording sets it up.
    Missing,
    /// Downloading the binary or the model.
    Preparing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VoiceStatus {
    pub state: VoiceState,
    pub detail: Option<String>,
    /// The voice chat's mute key, when one is set up.
    pub mute_key: Option<String>,
    /// Whether dictated text is tidied by a small model when dictation stops.
    pub polish: bool,
}

/// How to transcribe one recording.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Hints {
    /// Text that comes just before the recording, such as what is already in the box. Whisper
    /// continues it, which keeps names, spelling and punctuation consistent.
    pub prompt: Option<String>,
    /// The phrase's final text rather than a live preview, worth a slower, better decode.
    #[serde(rename = "final")]
    pub last: bool,
    /// The thread's folder: its name, branch and changed files help whisper spell them.
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Transcript {
    pub text: String,
}

pub struct Voice {
    dir: PathBuf,
    config: VoiceConfig,
    status: Mutex<VoiceStatus>,
    /// One setup at a time; later callers wait for it and then find everything in place.
    setup: tokio::sync::Mutex<()>,
    /// The graphics card download, apart from `setup` so dictation carries on while it runs.
    upgrading: tokio::sync::Mutex<()>,
    /// whisper-server with the model loaded, so live dictation does not reload it for every
    /// few seconds of speech. Stopped after a while without use.
    /// One per model: the preview's and, on a graphics card, the final text's.
    servers: std::sync::Arc<tokio::sync::Mutex<Vec<Server>>>,
    /// The final model would not run here (too little graphics memory, say); medium does both.
    final_broken: std::sync::atomic::AtomicBool,
    /// From `voice.polish`, and changed by Settings without a restart.
    polish: std::sync::atomic::AtomicBool,
    /// Folder hints for whisper's prompt, per folder, with when they were read.
    hints: Mutex<std::collections::HashMap<String, (std::time::Instant, String)>>,
    /// Whether there is an Nvidia card, asked once.
    nvidia: tokio::sync::OnceCell<bool>,
    mute: crate::mute::Mute,
    /// From `voice.mute_key`, and changed by Settings without a restart.
    mute_key: Mutex<Option<String>>,
    /// From `voice.language`, and changed by Settings without a restart.
    language: Mutex<String>,
}

struct Server {
    /// The binary it runs, so a switch to the graphics card build starts a new one.
    binary: PathBuf,
    model: PathBuf,
    port: u16,
    last_used: std::time::Instant,
    _child: tokio::process::Child,
}

/// A loaded model costs memory; give it back after this long without dictation.
const SERVER_IDLE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

impl Voice {
    pub fn new(data_dir: &Path, config: VoiceConfig) -> Self {
        let mute_key = config.mute_key.clone();
        let polish = config.polish;
        let voice = Self {
            dir: data_dir.join("voice"),
            config,
            status: Mutex::new(VoiceStatus { state: VoiceState::Missing, detail: None, mute_key: None, polish: false }),
            setup: tokio::sync::Mutex::new(()),
            upgrading: tokio::sync::Mutex::new(()),
            servers: Default::default(),
            final_broken: Default::default(),
            polish: std::sync::atomic::AtomicBool::new(polish),
            hints: Default::default(),
            nvidia: Default::default(),
            mute: Default::default(),
            mute_key: Mutex::new(mute_key),
            language: Mutex::new(String::new()),
        };
        voice.set_language(&voice.config.language.clone());
        voice.forgive_gpu();
        if voice.installed().is_some() {
            voice.set(VoiceState::Ready, None);
        }
        // Servers a crashed or replaced core left running hold gigabytes each.
        let dir = voice.dir.clone();
        std::thread::spawn(move || {
            for name in ["whisper-server", "whisper-server.exe"] {
                for found in walk(&dir, name) {
                    crate::supervise::end_leftovers(&found);
                }
            }
        });
        voice
    }

    /// Mutes the voice chat while dictating, when a mute key is set; called again every few
    /// seconds to keep it muted.
    pub fn mute(&self, on: bool) -> Result<()> {
        let key = self.mute_key.locked().clone();
        match (key, on) {
            (Some(key), true) => self.mute.hold(&key),
            _ => {
                self.mute.release();
                Ok(())
            }
        }
    }

    pub fn set_mute_key(&self, key: Option<String>) {
        self.mute.release();
        *self.mute_key.locked() = key;
    }

    pub fn set_polish(&self, on: bool) {
        self.polish.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_language(&self, language: &str) {
        *self.language.locked() = pinned_language(language);
    }

    fn language(&self) -> String {
        self.language.locked().clone()
    }

    /// Gets everything ready for a recording that is starting now: the files, and whisper-server
    /// with the model loaded, so the first live words do not wait seconds for a cold start.
    pub async fn warm(&self) -> Result<()> {
        touch();
        let (cli, model) = self.prepare().await?;
        let Some(server) = server_binary(&cli) else { return Ok(()) };
        self.server_port(&server, &model).await?;
        // The final model too, so the first phrase's final text does not wait for it to load.
        if let Some(big) = self.final_model(&cli)
            && self.server_port(&server, &big).await.is_err()
        {
            self.give_up_final();
        }
        Ok(())
    }

    pub fn status(&self) -> VoiceStatus {
        VoiceStatus {
            mute_key: self.mute_key.locked().clone(),
            polish: self.polish.load(std::sync::atomic::Ordering::Relaxed),
            ..self.status.locked().clone()
        }
    }

    fn set(&self, state: VoiceState, detail: Option<String>) {
        *self.status.locked() = VoiceStatus { state, detail, mute_key: None, polish: false };
    }

    fn model_path(&self) -> PathBuf {
        self.dir.join(&self.config.model)
    }

    fn binary(&self) -> Option<PathBuf> {
        if let Some(command) = &self.config.command {
            return Some(command.clone());
        }
        find_cli(&self.dir.join("bin"))
    }

    /// The CUDA build and its bigger model, when both are in place and have not failed.
    fn gpu_installed(&self) -> Option<(PathBuf, PathBuf)> {
        if self.config.command.is_some() || self.config.gpu == Some(false) || self.dir.join(GPU_FAILED).exists() {
            return None;
        }
        let model = self.dir.join(&self.config.gpu_model);
        Some((find_cli(&self.dir.join("bin-gpu"))?, model)).filter(|(_, m)| m.is_file())
    }

    /// The binary and model to use, when both are in place: the graphics card's when it has them.
    fn installed(&self) -> Option<(PathBuf, PathBuf)> {
        if let Some(found) = self.gpu_installed() {
            return Some(found);
        }
        self.cpu_installed()
    }

    /// The processor's binary and model, to fall back on when the graphics card's fail.
    fn cpu_installed(&self) -> Option<(PathBuf, PathBuf)> {
        let model = self.model_path();
        Some((self.binary()?, model)).filter(|(_, m)| m.is_file())
    }

    /// Earlier cores blamed the card for a whisper-server that died under them (curl could no
    /// longer reach it), which happens when a background build takes the memory, and moved the
    /// PC to the small model for good. Such a mark is dropped so the card gets another go.
    fn forgive_gpu(&self) {
        let failed = self.dir.join(GPU_FAILED);
        if std::fs::read_to_string(&failed).is_ok_and(|why| why.contains("Failed to connect")) {
            crate::locked::clean_file(failed);
        }
    }

    fn on_gpu(&self, cli: &Path) -> bool {
        cli.starts_with(self.dir.join("bin-gpu"))
    }

    /// The biggest model, for final text, when the graphics card runs whisper and it is in place.
    fn final_model(&self, cli: &Path) -> Option<PathBuf> {
        if !self.on_gpu(cli) || self.final_broken.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        Some(self.dir.join(&self.config.final_model)).filter(|m| m.is_file())
    }

    fn give_up_final(&self) {
        tracing::warn!("{} would not run here; final text stays on {}", self.config.final_model, self.config.gpu_model);
        self.final_broken.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    fn vad_model(&self) -> Option<PathBuf> {
        Some(self.dir.join(&self.config.vad_model)).filter(|m| m.is_file())
    }

    async fn has_nvidia(&self) -> bool {
        *self.nvidia.get_or_init(|| async {
            let mut cmd = crate::proc::tokio_command("nvidia-smi");
            cmd.arg("-L");
            run(&mut cmd).await.is_ok_and(|out| out.contains("GPU"))
        }).await
    }

    /// Fetches the CUDA build and the bigger model when there is an Nvidia card to run them, while
    /// dictation carries on with the small model. The next recording after it finishes uses them.
    pub async fn upgrade(&self) -> Result<()> {
        self.fetch_extras().await;
        if !cfg!(windows) || self.config.command.is_some() || self.config.gpu == Some(false) {
            return Ok(());
        }
        if self.gpu_installed().is_some() || self.dir.join(GPU_FAILED).exists() || !self.has_nvidia().await {
            return Ok(());
        }
        // Every recording asks for this; one download is enough.
        let Ok(_guard) = self.upgrading.try_lock() else { return Ok(()) };
        if self.gpu_installed().is_some() {
            return Ok(());
        }
        let result = self.download_gpu().await;
        if let Err(e) = &result {
            // Not tried again: the small model on the processor still works.
            tokio::fs::write(self.dir.join(GPU_FAILED), format!("{e:#}")).await.warn_on_err("write file");
        }
        let state = if self.installed().is_some() { VoiceState::Ready } else { VoiceState::Missing };
        self.set(state, None);
        result
    }

    /// The voice activity model (under 1 MB), and the final model once the graphics card runs
    /// whisper. Neither is needed to dictate, so a failed download only waits for the next try.
    async fn fetch_extras(&self) {
        let Ok(_guard) = self.upgrading.try_lock() else { return };
        let vad = self.dir.join(&self.config.vad_model);
        if self.config.command.is_none() && self.installed().is_some() && !vad.is_file() {
            let partial = vad.with_extension("partial");
            match fetch(&self.config.vad_model_url, &partial).await {
                Ok(()) => {
                    tokio::fs::rename(&partial, &vad).await.warn_on_err("move the downloaded model into place");
                }
                Err(e) => tracing::warn!("voice activity model: {e:#}"),
            }
        }
        let big = self.dir.join(&self.config.final_model);
        if self.gpu_installed().is_some() && !big.is_file() {
            self.set(VoiceState::Ready, Some(format!("Downloading the biggest speech model ({})", self.config.final_model)));
            let partial = big.with_extension("partial");
            match fetch(&self.config.final_model_url, &partial).await {
                Ok(()) => {
                    tokio::fs::rename(&partial, &big).await.warn_on_err("move the downloaded model into place");
                }
                Err(e) => tracing::warn!("final speech model: {e:#}"),
            }
            self.set(VoiceState::Ready, None);
        }
    }

    async fn download_gpu(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.dir).await?;
        let bin = self.dir.join("bin-gpu");
        if find_cli(&bin).is_none() {
            self.set(VoiceState::Ready, Some("Downloading whisper.cpp for the graphics card".into()));
            let zip = self.dir.join("whisper-gpu.zip");
            let mut fetched = Err(anyhow::anyhow!("no download address for whisper.cpp with CUDA"));
            for url in &self.config.gpu_binary_urls {
                fetched = fetch(url, &zip).await;
                if fetched.is_ok() {
                    break;
                }
                tracing::warn!("{:#}", fetched.as_ref().unwrap_err());
            }
            fetched?;
            crate::locked::clean_dir_async(&bin).await;
            tokio::fs::create_dir_all(&bin).await?;
            run(crate::proc::tokio_command("tar").arg("-xf").arg(&zip).arg("-C").arg(&bin)).await?;
            crate::locked::clean_file_async(&zip).await;
            if find_cli(&bin).is_none() {
                bail!("the whisper.cpp CUDA download has no whisper-cli");
            }
        }
        let model = self.dir.join(&self.config.gpu_model);
        if !model.is_file() {
            self.set(VoiceState::Ready, Some(format!("Downloading the bigger speech model ({})", self.config.gpu_model)));
            let partial = model.with_extension("partial");
            fetch(&self.config.gpu_model_url, &partial).await?;
            tokio::fs::rename(&partial, &model).await?;
        }
        Ok(())
    }

    /// Downloads whatever is missing. Safe to call any number of times.
    pub async fn prepare(&self) -> Result<(PathBuf, PathBuf)> {
        if let Some(found) = self.installed() {
            if self.status().state != VoiceState::Ready {
                self.set(VoiceState::Ready, None);
            }
            return Ok(found);
        }
        let _guard = self.setup.lock().await;
        if let Some(found) = self.installed() {
            self.set(VoiceState::Ready, None);
            return Ok(found);
        }
        let result = self.download().await;
        match &result {
            Ok(_) => self.set(VoiceState::Ready, None),
            Err(e) => self.set(VoiceState::Failed, Some(format!("{e:#}"))),
        }
        result
    }

    async fn download(&self) -> Result<(PathBuf, PathBuf)> {
        tokio::fs::create_dir_all(&self.dir).await?;
        if self.binary().is_none() {
            if !cfg!(windows) {
                bail!("install whisper.cpp and set voice.command to its whisper-cli");
            }
            self.set(VoiceState::Preparing, Some("Downloading whisper.cpp".into()));
            let zip = self.dir.join("whisper-bin.zip");
            let mut fetched = Err(anyhow::anyhow!("no download address for whisper.cpp"));
            for url in &self.config.binary_urls {
                fetched = fetch(url, &zip).await;
                if fetched.is_ok() {
                    break;
                }
                tracing::warn!("{:#}", fetched.as_ref().unwrap_err());
            }
            fetched?;
            let bin = self.dir.join("bin");
            crate::locked::clean_dir_async(&bin).await;
            tokio::fs::create_dir_all(&bin).await?;
            // tar.exe ships with Windows 10 and later and reads zip files.
            run(crate::proc::tokio_command("tar").arg("-xf").arg(&zip).arg("-C").arg(&bin)).await?;
            crate::locked::clean_file_async(&zip).await;
            if self.binary().is_none() {
                bail!("the whisper.cpp download has no whisper-cli");
            }
        }
        let model = self.model_path();
        if !model.is_file() {
            self.set(VoiceState::Preparing, Some(format!("Downloading the speech model ({})", self.config.model)));
            let partial = model.with_extension("partial");
            fetch(&self.config.model_url, &partial).await?;
            tokio::fs::rename(&partial, &model).await?;
            // A model from an earlier default is a few hundred MB nobody uses any more.
            let keep = [&self.config.gpu_model, &self.config.final_model, &self.config.vad_model].map(|m| self.dir.join(m));
            if let Ok(entries) = std::fs::read_dir(&self.dir) {
                for old in entries.flatten().map(|e| e.path()) {
                    if old != model && !keep.contains(&old) && old.extension().is_some_and(|e| e == "bin") {
                        crate::locked::clean_file(old);
                    }
                }
            }
        }
        self.installed().context("voice setup finished but files are missing")
    }

    /// Turns a 16 kHz mono WAV recording into text.
    pub async fn transcribe(&self, wav: &[u8], hints: &Hints) -> Result<String> {
        touch();
        let (cli, model) = self.prepare().await?;
        let file = self.dir.join(format!("rec-{}.wav", uuid::Uuid::now_v7()));
        tokio::fs::write(&file, wav).await?;
        let gpu = self.on_gpu(&cli);
        let context = match hints.cwd.as_deref() {
            Some(cwd) => self.folder_hints(cwd).await,
            None => String::new(),
        };
        let fresh = hints.prompt.as_deref().unwrap_or_default().trim().is_empty();
        let opts = Decode {
            prompt: prompt(hints.prompt.as_deref(), &context),
            beam: if hints.last { self.config.beam_size.max(1) } else { 1 },
            // The processor is too slow to redo the preview every second over whisper's full 30 s
            // window, so it only looks at as much as was recorded. Final text gets the full window.
            audio_ctx: if hints.last || gpu { 0 } else { audio_ctx(wav.len()) },
        };
        let big = if hints.last { self.final_model(&cli) } else { None };
        let mut output = match &big {
            Some(big) => match self.decode(&cli, big, &file, &opts).await {
                Ok(text) => Ok(text),
                Err(e) => {
                    tracing::warn!("{e:#}");
                    self.give_up_final();
                    self.drop_server(big).await;
                    self.decode(&cli, &model, &file, &opts).await
                }
            },
            None => self.decode(&cli, &model, &file, &opts).await,
        };
        let worked = self.dir.join(GPU_WORKED);
        if gpu && output.is_ok() && !worked.exists() {
            tokio::fs::write(&worked, "").await.warn_on_err("write file");
        }
        if gpu && let Err(e) = &output {
            let reason = format!("{e:#}");
            if worked.exists() {
                // It has run here before, so this is a passing trouble (memory taken by a build,
                // say): this recording goes to the processor, the next tries the card again.
                tracing::warn!("whisper.cpp on the graphics card failed this time, using the processor: {reason}");
            } else {
                // The CUDA build would not run here (an old driver, say): back to the processor for good.
                tracing::warn!("whisper.cpp on the graphics card failed, using the processor: {reason}");
                tokio::fs::write(self.dir.join(GPU_FAILED), reason).await.warn_on_err("write file");
            }
            self.servers.lock().await.clear();
            if let Some((cli, model)) = self.cpu_installed() {
                output = self.decode(&cli, &model, &file, &opts).await;
            }
        }
        crate::locked::clean_file_async(&file).await;
        let text = clean(&output?);
        Ok(if fresh && echoes(&opts.prompt, &text) { String::new() } else { text })
    }

    /// The folder's name, its branch and the files changed in it, as a line for whisper's prompt,
    /// so it spells them as written. Read at most every 30 seconds per folder.
    async fn folder_hints(&self, cwd: &str) -> String {
        if let Some((at, line)) = self.hints.locked().get(cwd)
            && at.elapsed() < std::time::Duration::from_secs(30)
        {
            return line.clone();
        }
        let line = folder_line(cwd).await;
        self.hints.locked().insert(cwd.to_owned(), (std::time::Instant::now(), line.clone()));
        line
    }

    /// Decodes with whisper-server when the build has one, else (or when the server will not
    /// start or answer) with whisper-cli, which loads the model for this one file.
    async fn decode(&self, cli: &Path, model: &Path, file: &Path, opts: &Decode) -> Result<String> {
        let Some(server) = server_binary(cli) else { return self.via_cli(cli, model, file, opts).await };
        match self.via_server(&server, model, file, opts).await {
            Ok(text) => Ok(text),
            Err(server_err) => {
                tracing::warn!("{server_err:#}; decoding with whisper-cli instead");
                self.via_cli(cli, model, file, opts).await.map_err(|e| e.context(format!("{server_err:#}")))
            }
        }
    }

    async fn via_cli(&self, cli: &Path, model: &Path, file: &Path, opts: &Decode) -> Result<String> {
        let mut cmd = crate::proc::tokio_command(cli);
        cmd.arg("-m").arg(model)
            .arg("-f").arg(file)
            .args(["-l", &self.language(), "-t", &threads().to_string(), "-nt", "-np"])
            .args(["-bs", &opts.beam.to_string(), "-ac", &opts.audio_ctx.to_string(), "--prompt", &opts.prompt]);
        run(&mut cmd).await
    }

    /// The port of a running whisper-server with `model` loaded, started when there is none.
    async fn server_port(&self, binary: &Path, model: &Path) -> Result<u16> {
        let mut servers = self.servers.lock().await;
        // A switch to or from the graphics card build leaves the old ones unused.
        servers.retain(|s| s.binary == binary);
        if let Some(s) = servers.iter_mut().find(|s| s.model == model) {
            s.last_used = std::time::Instant::now();
            return Ok(s.port);
        }
        let started = self.start_server(binary, model).await?;
        let port = started.port;
        servers.push(started);
        if servers.len() == 1 {
            self.reap_when_idle();
        }
        Ok(port)
    }

    async fn drop_server(&self, model: &Path) {
        self.servers.lock().await.retain(|s| s.model != model);
    }

    async fn via_server(&self, server: &Path, model: &Path, file: &Path, opts: &Decode) -> Result<String> {
        let port = self.server_port(server, model).await?;
        match self.ask_server(port, file, opts).await {
            Ok(text) => Ok(text),
            Err(e) => {
                // A crashed server is started again on the next request.
                self.drop_server(model).await;
                Err(e)
            }
        }
    }

    async fn ask_server(&self, port: u16, file: &Path, opts: &Decode) -> Result<String> {
        let url = format!("http://127.0.0.1:{port}/inference");
        let mut cmd = crate::proc::tokio_command("curl");
        cmd.args(["--silent", "--show-error", "--fail", "-F"])
            .arg(format!("file=@{}", file.display()))
            .args(["-F", "response_format=json"])
            .arg("-F").arg(format!("language={}", self.language()))
            .arg("-F").arg(format!("beam_size={}", opts.beam))
            .arg("-F").arg(format!("audio_ctx={}", opts.audio_ctx))
            // -F would read a prompt starting with @ or < as a file name.
            .arg("--form-string").arg(format!("prompt={}", opts.prompt))
            .arg(&url);
        let body = run(&mut cmd).await?;
        let json: serde_json::Value = serde_json::from_str(&body).context("whisper-server answered oddly")?;
        Ok(json["text"].as_str().unwrap_or_default().to_owned())
    }

    /// Runs one throwaway decode of a second of silence. The first decode after a start is slow
    /// (the graphics card sets up its kernels, the processor pages the model in), and it should
    /// not be the user's first words.
    async fn first_pass(&self, port: u16) -> Result<()> {
        let file = self.dir.join(format!("warm-{}.wav", uuid::Uuid::now_v7()));
        tokio::fs::write(&file, silence_wav(16_000)).await?;
        let opts = Decode { prompt: VOCABULARY.to_owned(), beam: 1, audio_ctx: audio_ctx(44 + 32_000) };
        let result = self.ask_server(port, &file, &opts).await;
        crate::locked::clean_file_async(&file).await;
        result.map(|_| ())
    }

    /// Starts whisper-server and waits for the model to load. Tried a few times on fresh ports:
    /// the port is picked by binding and letting go, and something else can take it in between.
    async fn start_server(&self, server: &Path, model: &Path) -> Result<Server> {
        let mut last = None;
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            // The last try leaves voice activity detection out, in case this build cannot do it.
            match self.start_server_once(server, model, attempt < 2).await {
                Ok(s) => return Ok(s),
                Err(e) => {
                    tracing::warn!("{e:#}");
                    last = Some(e);
                }
            }
        }
        Err(last.expect("tried at least once"))
    }

    async fn start_server_once(&self, server: &Path, model: &Path, vad: bool) -> Result<Server> {
        let port = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        // What it prints is kept, so a failed start can say why (out of memory, a bad model).
        let log_path = self.dir.join(SERVER_LOG);
        let stderr = std::fs::File::create(&log_path).map(Stdio::from).unwrap_or_else(|_| Stdio::null());
        let mut cmd = crate::proc::tokio_command(server);
        cmd.arg("-m").arg(model)
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["-l", &self.language(), "-t", &threads().to_string()]);
        if let Some(silero) = self.vad_model().filter(|_| vad) {
            // A short pause is kept inside a phrase; the composer already ends phrases at 1.2 s.
            cmd.arg("--vad").arg("-vm").arg(silero).args(["-vsd", "300", "-vp", "200"]);
        }
        cmd.current_dir(server.parent().unwrap_or(Path::new(".")))
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(stderr)
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd.spawn().context("starting whisper-server")?;
        crate::supervise::tie(&child);
        // Loading the model takes a moment.
        let started = std::time::Instant::now();
        for _ in 0..300 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                let loaded = started.elapsed();
                if let Err(e) = self.first_pass(port).await {
                    tracing::warn!("whisper-server's first decode failed: {e:#}");
                }
                tracing::info!(
                    "whisper-server ready: model loaded in {:.1} s, first decode {:.1} s ({})",
                    loaded.as_secs_f32(),
                    (started.elapsed() - loaded).as_secs_f32(),
                    model.file_name().unwrap_or_default().to_string_lossy()
                );
                return Ok(Server {
                    binary: server.to_path_buf(),
                    model: model.to_path_buf(),
                    port,
                    last_used: std::time::Instant::now(),
                    _child: child,
                });
            }
            if let Some(status) = child.try_wait()? {
                let said = std::fs::read_to_string(&log_path).unwrap_or_default();
                match why_it_stopped(&said) {
                    Some(why) => bail!("whisper-server exited ({status}): {why}"),
                    None => bail!("whisper-server exited: {status}"),
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        bail!("whisper-server did not start")
    }

    fn reap_when_idle(&self) {
        let servers = self.servers.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                let mut s = servers.lock().await;
                s.retain(|server| server.last_used.elapsed() <= SERVER_IDLE);
                if s.is_empty() {
                    return;
                }
            }
        });
    }
}

fn now_secs() -> u64 {
    (crate::model::now_ms() / 1000) as u64
}

/// When dictation was last used, in seconds since 1970, so builds can give way to it.
static LAST_USE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn touch() {
    LAST_USE.store(now_secs(), std::sync::atomic::Ordering::Relaxed);
}

/// Whether someone has dictated in the last 90 seconds. Hub's own update builds wait for it, and
/// step aside when memory runs short, since a build took the memory whisper needed to start.
pub fn dictating() -> bool {
    let last = LAST_USE.load(std::sync::atomic::Ordering::Relaxed);
    last > 0 && now_secs().saturating_sub(last) < 90
}

/// A 16 kHz mono WAV of `samples` silent samples.
fn silence_wav(samples: u32) -> Vec<u8> {
    let data = samples * 2;
    let mut wav = Vec::with_capacity(44 + data as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    for v in [16u32.to_le_bytes(), [1, 0, 1, 0], 16_000u32.to_le_bytes(), 32_000u32.to_le_bytes(), [2, 0, 16, 0]] {
        wav.extend_from_slice(&v);
    }
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data.to_le_bytes());
    wav.resize(44 + data as usize, 0);
    wav
}

/// Marks that the CUDA build failed here, with why, so it is not fetched again.
const GPU_FAILED: &str = "gpu-failed.txt";

/// Marks that the CUDA build has transcribed here, so a later failure is not the card's fault.
const GPU_WORKED: &str = "gpu-worked.txt";

/// What the last whisper-server printed while starting.
const SERVER_LOG: &str = "whisper-server.log";

struct Decode {
    prompt: String,
    beam: u32,
    /// How much of whisper's 30 s window to encode, in its 20 ms steps; 0 is all of it.
    audio_ctx: u32,
}

/// Enough of whisper's window for a recording of this many WAV bytes, with a second to spare,
/// since cutting it close makes whisper repeat itself or trail off.
fn audio_ctx(wav_bytes: usize) -> u32 {
    let seconds = wav_bytes.saturating_sub(44) as f64 / (16_000.0 * 2.0);
    let steps = ((seconds + 1.0) * 50.0).ceil() as u32;
    (steps.div_ceil(64) * 64).clamp(256, 1500)
}

fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4)
}

#[cfg(test)]
mod tests;
