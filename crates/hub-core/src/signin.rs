//! Keeping engines signed in on every machine.
//!
//! An engine's normal login (Claude Code's `/login`, `codex login`) is a short access token and
//! a refresh token that changes each time it is used. It stays alive only while the engine runs
//! now and then on that machine, and a copy of it on a second machine dies the moment either one
//! refreshes. On a machine nobody sits at, that ends as a thread failing with "OAuth session
//! expired and could not be refreshed".
//!
//! So Hub does three things, all from the engine's traits (`EngineTraits`), naming no engine:
//! - An engine error that matches `auth_phrases` marks the engine's sign-in on this machine as
//!   lost: the thread gets a `sign_in_lost` event with a button, and the machine's "Ready for
//!   work" shows it until a turn works again.
//! - `hubd sign-in <engine>`, opened in a terminal on the machine, runs the engine's `sign_in`
//!   command there. Where that prints a long-lived token (`sign_in_token`, Claude Code's
//!   `claude setup-token`: a year, on the subscription), Hub reads it from the output (or asks
//!   for it) and keeps it in that machine's vault.
//! - Every engine Hub starts on that machine gets the kept token in its environment, so there is
//!   nothing left to refresh. Readiness warns a month before it runs out.

use crate::locked::ResultExt;
use crate::locked::Ignore;
use std::collections::BTreeMap;
use std::io::{BufRead as _, Write as _};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::{Config, SignInToken};
use crate::hub::Hub;
use crate::model::{Event, SignInDone, now_ms};

pub use crate::hubd_args::{CLI_ARG, CLI_MARKER, ROOT_ARG, ROOT_MARKER, SIGN_IN_ARG};
const LOST: &str = "signin.lost";
const KEPT: &str = "signin.kept";
const DAY_MS: i64 = 86_400_000;
/// A kept sign-in this close to its end shows as a warning.
pub const SOON_MS: i64 = 30 * DAY_MS;
/// Wide enough that the engine prints a token on one line, so it can be read back whole.
const COLUMNS: u32 = 200;

/// The engine's sign-in on this machine stopped working.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lost {
    pub at: i64,
    pub message: String,
}

/// A long-lived sign-in Hub keeps for an engine on this machine. The token is in the vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kept {
    pub at: i64,
    pub until: i64,
    /// The token's end, as the vault shows keys.
    pub hint: String,
    /// The machine it was made on, when it came from another one (`take_sign_ins`). Only the
    /// ones made here are sent on, so two machines never send one back and forth.
    #[serde(default)]
    pub from: Option<String>,
}

/// A lasting sign-in sent to the other machines, so signing in once on one does them all. Only
/// tokens Hub keeps (`sign_in_token`, a year on the subscription) travel: an engine's own login
/// refreshes itself and a copy of it dies the moment either machine refreshes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedSignIn {
    pub engine: String,
    pub token: String,
    pub at: i64,
    pub until: i64,
}

/// What this machine knows of its engines' sign-ins, by engine.
#[derive(Debug, Clone, Default)]
pub struct SignIns {
    pub lost: BTreeMap<String, Lost>,
    pub kept: BTreeMap<String, Kept>,
}


/// The vault name of an engine's kept token.
pub fn vault_name(engine: &str) -> String {
    let name: String = engine.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    format!("HUB_SIGNIN_{name}")
}


/// Whether an engine's error says its sign-in is gone.
pub fn is_auth_failure(message: &str, phrases: &[String]) -> bool {
    let message = message.to_lowercase();
    phrases.iter().any(|p| !p.is_empty() && message.contains(p.as_str()))
}

/// The token `prefix` starts, in what a terminal showed: colours and cursor moves dropped.
pub fn find_token(output: &str, prefix: &str) -> Option<String> {
    let text = crate::text::strip_ansi(output);
    let token_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    // The last one: an engine that shows the token twice shows the same one.
    text.rmatch_indices(prefix)
        .map(|(i, _)| text[i..].chars().take_while(|c| token_char(*c)).collect::<String>())
        .find(|t| t.len() >= prefix.len() + 20)
}


/// A day as people write it, in UTC.
fn day(ms: i64) -> String {
    crate::activity::date_of(ms, 0)
}

impl Hub {
    pub async fn sign_ins(&self) -> SignIns {
        SignIns { lost: self.read_map(LOST).await, kept: self.read_map(KEPT).await }
    }

    async fn read_map<T: serde::de::DeserializeOwned>(&self, key: &str) -> BTreeMap<String, T> {
        match self.store.get_value(key).await {
            Ok(Some(v)) => serde_json::from_str(&v).unwrap_or_default(),
            _ => BTreeMap::new(),
        }
    }

    async fn write_map<T: Serialize>(&self, key: &str, map: &BTreeMap<String, T>) -> Result<()> {
        self.store.set_value(key, &serde_json::to_string(map)?).await
    }

    /// Hands the kept tokens to engines started from now on. At start and after each change.
    pub async fn load_sign_ins(&self) {
        let kept: BTreeMap<String, Kept> = self.read_map(KEPT).await;
        let vault = self.secrets.cached();
        let mut env = BTreeMap::new();
        for engine in kept.keys() {
            let (Some(config), Some(token)) = (self.config.engines.get(engine), self.config.traits(engine).sign_in_token) else { continue };
            if let Some(value) = vault.get(&vault_name(engine)) {
                env.insert(config.command.clone(), (token.env, value.clone()));
            }
        }
        crate::secrets::set_signed_in_env(env);
    }

    /// The kept sign-in for a thread's engine, for its environment.
    pub(crate) fn sign_in_env(&self, engine: &str) -> Vec<(String, String)> {
        self.config.engines.get(engine).map(|e| crate::secrets::signed_in_env(&e.command)).unwrap_or_default()
    }

    /// After an engine error: when it says the sign-in is gone, mark it so, tell the thread and
    /// stop its engine, so the next message starts one that reads the sign-in afresh.
    pub(crate) async fn after_sign_in_error(self: &Arc<Self>, error: &Event) -> Result<()> {
        let message = error.failure().unwrap_or_default();
        let detail = error.detail().as_str().unwrap_or_default();
        let Some(thread) = self.store.thread(&error.thread_id).await? else { return Ok(()) };
        let phrases = self.config.traits(&thread.engine).auth_phrases.unwrap_or_default();
        if !is_auth_failure(message, &phrases) && !is_auth_failure(detail, &phrases) {
            return Ok(());
        }
        let said = if message.is_empty() { detail } else { message };
        let mut lost: BTreeMap<String, Lost> = self.read_map(LOST).await;
        lost.insert(thread.engine.clone(), Lost { at: now_ms(), message: said.chars().take(300).collect() });
        self.write_map(LOST, &lost).await?;
        let kept = self.read_map::<Kept>(KEPT).await.contains_key(&thread.engine);
        let label = crate::prefs::accounts(&self.config)
            .engines
            .into_iter()
            .find(|a| a.engine == thread.engine)
            .map(|a| a.label)
            .or_else(|| self.config.engines.get(&thread.engine).and_then(|e| e.label.clone()))
            .unwrap_or_else(|| thread.engine.clone());
        let data = crate::model::SignInLostData {
            engine: thread.engine.clone(),
            label,
            machine: self.remotes.local_name().to_owned(),
            message: said.to_owned(),
            kept,
            can_keep: Some(self.config.traits(&thread.engine).sign_in.is_some()),
            // On a Mac the CLI's own login is in the Keychain, out of reach of hubd's service, so
            // it never "ran out": it was never readable here.
            keychain: Some(cfg!(target_os = "macos") && !kept && self.config.traits(&thread.engine).sign_in_token.is_some()),
        };
        self.emit_typed(&thread.id, &data).await?;
        self.shutdown(&thread.id, false).await
    }

    /// A turn worked, so the engine is signed in again.
    pub(crate) async fn signed_in_again(&self, thread_id: &str) {
        let Ok(Some(thread)) = self.store.thread(thread_id).await else { return };
        let mut lost: BTreeMap<String, Lost> = self.read_map(LOST).await;
        if lost.remove(&thread.engine).is_some() {
            self.write_map(LOST, &lost).await.warn_on_err("save the lost sign-ins");
        }
    }

    /// `POST /api/engines/{engine}/sign-in`: `hubd sign-in` finished. With a token, keeps it for
    /// the engine's threads here; without, the engine saved its own login.
    pub async fn signed_in(&self, engine: &str, token: Option<&str>) -> Result<SignInDone> {
        if !self.config.engines.contains_key(engine) {
            bail!("no engine called {engine}");
        }
        let until = match token.map(str::trim).filter(|t| !t.is_empty()) {
            Some(token) => {
                let Some(how) = self.config.traits(engine).sign_in_token else {
                    bail!("{engine} signs in by itself; Hub keeps no token for it");
                };
                check_token(token, &how)?;
                let now = now_ms();
                let until = now + how.days as i64 * DAY_MS;
                self.keep(engine, token, now, until, None).await?;
                // The other machines get it with the engines, at once.
                crate::engines::share_soon();
                Some(until)
            }
            None => {
                self.signed_in_now(engine).await?;
                None
            }
        };
        Ok(SignInDone { engine: engine.to_owned(), until })
    }

    /// Keeps a lasting sign-in for the engine's threads here, from `from` when another machine
    /// sent it.
    async fn keep(&self, engine: &str, token: &str, at: i64, until: i64, from: Option<&str>) -> Result<()> {
        self.secrets.set(&vault_name(engine), token).await?;
        let mut kept: BTreeMap<String, Kept> = self.read_map(KEPT).await;
        kept.insert(engine.to_owned(), Kept { at, until, hint: crate::prefs::hint(token), from: from.map(str::to_owned) });
        self.write_map(KEPT, &kept).await?;
        self.signed_in_now(engine).await
    }

    /// The engine is signed in again: no longer lost, and idle engines that started with the old
    /// sign-in start with the new one at the next message.
    async fn signed_in_now(&self, engine: &str) -> Result<()> {
        let mut lost: BTreeMap<String, Lost> = self.read_map(LOST).await;
        if lost.remove(engine).is_some() {
            self.write_map(LOST, &lost).await?;
        }
        self.load_sign_ins().await;
        for id in self.engines() {
            let idle = self
                .store
                .thread(&id)
                .await
                .ok()
                .flatten()
                .is_some_and(|t| t.engine == engine && !matches!(t.status, crate::model::ThreadStatus::Running | crate::model::ThreadStatus::WaitingOnYou));
            if idle {
                self.shutdown(&id, false).await.warn_on_err("stop the idle thread");
            }
        }
        Ok(())
    }

    /// The lasting sign-ins made on this machine, to send to the others.
    pub async fn outgoing_sign_ins(&self) -> Vec<SharedSignIn> {
        let kept: BTreeMap<String, Kept> = self.read_map(KEPT).await;
        let vault = self.secrets.cached();
        let now = now_ms();
        kept.into_iter()
            .filter(|(_, k)| (k.from.is_none() || self.config.relay_keys) && k.until > now)
            .filter_map(|(engine, k)| {
                let token = vault.get(&vault_name(&engine))?.clone();
                Some(SharedSignIn { engine, token, at: k.at, until: k.until })
            })
            .collect()
    }

    /// Takes in the lasting sign-ins another machine sent. A newer one made here stays.
    pub async fn take_sign_ins(&self, from: &str, shipped: Vec<SharedSignIn>) -> Result<()> {
        let now = now_ms();
        for s in shipped {
            let Some(how) = self.config.traits(&s.engine).sign_in_token else { continue };
            if !self.config.engines.contains_key(&s.engine) || s.until <= now || check_token(&s.token, &how).is_err() {
                continue;
            }
            let kept: BTreeMap<String, Kept> = self.read_map(KEPT).await;
            if let Some(k) = kept.get(&s.engine) {
                let same = self.secrets.cached().get(&vault_name(&s.engine)) == Some(&s.token);
                let newer_here = k.from.is_none() && k.at >= s.at && k.until > now;
                if same || newer_here {
                    continue;
                }
            }
            self.keep(&s.engine, &s.token, s.at, s.until, Some(from)).await?;
            tracing::info!(engine = %s.engine, %from, "signed in with the sign-in made on {from}");
        }
        Ok(())
    }

    /// `DELETE /api/engines/{engine}/sign-in`: back to the engine's own login.
    pub async fn forget_sign_in(&self, engine: &str) -> Result<()> {
        let mut kept: BTreeMap<String, Kept> = self.read_map(KEPT).await;
        kept.remove(engine);
        self.write_map(KEPT, &kept).await?;
        if self.secrets.cached().contains_key(&vault_name(engine)) {
            self.secrets.delete(&vault_name(engine)).await?;
        }
        self.load_sign_ins().await;
        Ok(())
    }
}

/// `hubd cli <engine>`, in a terminal on the machine (the Claude and Codex buttons): runs the
/// engine's CLI with the sign-in Hub keeps here, the way its threads run, and without keys that
/// would move it off the subscription. Over SSH a Mac's own login is locked in the Keychain, so
/// without this the CLI there said "Not logged in" while the machine's threads worked.
pub async fn cli(config: &Config, args: &[String]) -> Result<()> {
    std::hint::black_box(CLI_MARKER);
    let Some(engine) = args.iter().find(|a| !a.starts_with("--")) else {
        bail!("usage: hubd {CLI_ARG} <engine>");
    };
    let Some(e) = config.engines.get(engine) else { bail!("there is no engine called {engine} here") };
    let program = e
        .terminal
        .as_deref()
        .and_then(crate::machine::engine_cli)
        .with_context(|| format!("{engine} has no CLI of its own"))?
        .to_owned();
    let mut env: Vec<(String, String)> = Vec::new();
    if let Some(how) = config.traits(engine).sign_in_token {
        let store = crate::store::Store::open(&config.data_dir.join("hub.db"))?;
        let vault = crate::secrets::Vault::new(store, &config.data_dir);
        if let Some((_, token)) = vault.env(&[vault_name(engine)]).await?.pop() {
            env.push((how.env, token));
        }
    }
    // Not signed in here: sign in first, in this same window, instead of opening a CLI that only
    // says so. A sign-in that fails is told and the CLI opens anyway.
    if env.is_empty() && needs_sign_in(config, engine) {
        println!("{engine} is not signed in on this machine. Hub signs it in first.\n");
        if let Err(e) = sign_in(config, std::slice::from_ref(engine)).await {
            println!("\n{e:#}\nOpening {program} anyway.\n");
        } else if let Some(how) = config.traits(engine).sign_in_token {
            // The core now keeps a token for it: this CLI gets it too.
            let store = crate::store::Store::open(&config.data_dir.join("hub.db"))?;
            let vault = crate::secrets::Vault::new(store, &config.data_dir);
            if let Some((_, token)) = vault.env(&[vault_name(engine)]).await?.pop() {
                env.push((how.env, token));
            }
        }
    }
    let path = crate::engines::locate(&program).with_context(|| format!("{program} is not installed here"))?;
    let mut cmd = crate::proc::command_with_console(path);
    for key in crate::config::ENGINE_KEYS {
        cmd.env_remove(key);
    }
    cmd.envs(env);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(cmd.exec()).with_context(|| format!("starting {program}"))
    }
    #[cfg(not(unix))]
    {
        let status = cmd.status().with_context(|| format!("starting {program}"))?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// The lasting sign-ins Hub keeps on this machine, as the variables the engines read them from
/// (`CLAUDE_CODE_OAUTH_TOKEN`), for every engine that has one. Read from the vault here, so it
/// works from a terminal without the core running.
async fn kept_env(config: &Config) -> Result<Vec<(String, String)>> {
    let mut wanted: Vec<(String, String)> = Vec::new();
    for engine in config.engines.keys() {
        if let Some(how) = config.traits(engine).sign_in_token {
            wanted.push((vault_name(engine), how.env));
        }
    }
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let store = crate::store::Store::open(&config.data_dir.join("hub.db"))?;
    let vault = crate::secrets::Vault::new(store, &config.data_dir);
    let names: Vec<String> = wanted.iter().map(|(v, _)| v.clone()).collect();
    let found = vault.env(&names).await?;
    Ok(found.into_iter().filter_map(|(v, token)| wanted.iter().find(|(n, _)| *n == v).map(|(_, env)| (env.clone(), token))).collect())
}

/// What `hubd root` runs: `sudo` with the kept sign-ins named in `--preserve-env` (so they reach
/// root through the environment, never through a command line `ps` shows), a login shell for
/// root, and the login user's folders in front of root's PATH so `claude` and `codex` are found.
fn root_command(env_names: &[String], path: &std::ffi::OsStr) -> Vec<std::ffi::OsString> {
    let mut argv: Vec<std::ffi::OsString> = vec!["sudo".into()];
    if !env_names.is_empty() {
        argv.push(format!("--preserve-env={}", env_names.join(",")).into());
    }
    let mut env_path = std::ffi::OsString::from("PATH=");
    env_path.push(path);
    argv.extend(["-i".into(), "env".into(), env_path, "bash".into()]);
    argv
}

/// `hubd root`, in a terminal on the machine (the Root shell button): `sudo -i` makes a different
/// user with an empty home, so the sign-in Claude Code keeps for the login user is not there and
/// `claude` asked to log in again. Hub's lasting sign-in goes with it into root's environment, the
/// way it goes to the machine's threads and `hubd cli`; nothing is written under `/root`. Keys
/// that would move an engine off the subscription are left out.
pub async fn root(config: &Config) -> Result<()> {
    std::hint::black_box(ROOT_MARKER);
    if !cfg!(unix) {
        bail!("a root shell is for Linux and Mac");
    }
    let env = kept_env(config).await?;
    let home = crate::folders::home();
    let mut dirs = vec![home.join(".local").join("bin"), config.data_dir.join("tools").join("bin"), config.data_dir.join("node").join("bin")];
    dirs.extend(["/usr/local/sbin", "/usr/local/bin", "/usr/sbin", "/usr/bin", "/sbin", "/bin"].map(std::path::PathBuf::from));
    let path = std::env::join_paths(dirs).context("building root's PATH")?;
    let names: Vec<String> = env.iter().map(|(n, _)| n.clone()).collect();
    let argv = root_command(&names, &path);
    let mut cmd = crate::proc::command_with_console(&argv[0]);
    cmd.args(&argv[1..]);
    for key in crate::config::ENGINE_KEYS {
        cmd.env_remove(key);
    }
    cmd.envs(env);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(cmd.exec()).context("starting sudo")
    }
    #[cfg(not(unix))]
    {
        let _unused_off_unix = cmd;
        Ok(())
    }
}

/// An engine that can sign in from Hub (it has a sign-in command), whose login on this machine
/// is not there. An engine whose login Hub cannot read counts as signed in: not ours to judge.
fn needs_sign_in(config: &Config, engine: &str) -> bool {
    config.traits(engine).sign_in.is_some()
        && crate::prefs::accounts(config).engines.iter().find(|a| a.engine == engine).is_some_and(|a| !a.logged_in)
}

fn check_token(token: &str, how: &SignInToken) -> Result<()> {
    if !token.starts_with(&how.prefix) || token.len() < how.prefix.len() + 20 {
        bail!("that is not a sign-in token: it should start with {}", how.prefix);
    }
    if !token.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("the token has characters a token never has; copy it again");
    }
    Ok(())
}

/// `hubd sign-in <engine>`, in a terminal on the machine: runs the engine's sign-in command and
/// tells the running core how it went. Waits for Enter at the end, so the window says what
/// happened before it closes.
pub async fn run(config: &Config, args: &[String]) -> Result<()> {
    let result = sign_in(config, args).await;
    if let Err(e) = &result {
        println!("\n{e:#}");
    }
    print!("\nPress Enter to close. ");
    std::io::stdout().flush().warn_on_err("flush stdout");
    std::io::stdin().lock().read_line(&mut String::new()).ignore("a closed window instead of Enter is fine");
    result
}

async fn sign_in(config: &Config, args: &[String]) -> Result<()> {
    let Some(engine) = args.iter().find(|a| !a.starts_with("--")) else {
        bail!("usage: hubd {SIGN_IN_ARG} <engine>");
    };
    if !config.engines.contains_key(engine) {
        bail!("there is no engine called {engine} here");
    }
    let traits = config.traits(engine);
    let Some(command) = traits.sign_in.clone() else {
        bail!("Hub does not know how {engine} signs in; open its own CLI and sign in there");
    };
    let client = crate::terminal::Client::new(config)?;
    let mut words = crate::project::split_command(&command);
    if words.is_empty() {
        bail!("{engine}'s sign-in command is empty");
    }
    // A machine set up for threads may lack the CLI that signs in: install it first.
    if crate::proc::find_program(&words[0]).is_none() {
        let Some(install) = crate::engines::install_command(config, engine, &command) else {
            bail!("{} is not installed here, and Hub doesn't know how to install it", words[0]);
        };
        println!("{} is not installed here, so Hub installs it first, in its own folder (Node.js too if there is none).\n", words[0]);
        let shown = |line: String| println!("  {line}");
        crate::engines::run_shell(&crate::engines::with_path(&install), shown).await.with_context(|| format!("installing {}", words[0]))?;
        println!();
    }
    words[0] = crate::proc::find_program(&words[0])
        .with_context(|| format!("{} is still not found after installing it", words[0]))?
        .display()
        .to_string();

    let Some(how) = traits.sign_in_token else {
        println!("Signing {engine} in with `{command}`. Follow what it says.\n");
        let status = crate::proc::command_with_console(&words[0]).args(&words[1..]).status().context("could not start the sign-in")?;
        if !status.success() {
            bail!("the sign-in did not finish ({status})");
        }
        let _: SignInDone = client.post(&format!("/api/engines/{engine}/sign-in"), json!({})).await?;
        println!("\nSigned in. Threads here use it from their next message.");
        return Ok(());
    };

    println!("Signing {engine} in for {} days with `{command}`.", how.days);
    println!("Open the link it shows on any device, approve, and paste the code back here.\n");
    let printed = run_captured(&words, &config.data_dir)?;
    let token = match printed.as_deref().and_then(|o| find_token(o, &how.prefix)) {
        Some(t) => t,
        None => {
            println!("\nPaste the token it printed (it starts with {}), then Enter:", how.prefix);
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            let token = line.trim().to_owned();
            if token.is_empty() {
                bail!("no token, so nothing changed");
            }
            token
        }
    };
    let done: SignInDone = client.post(&format!("/api/engines/{engine}/sign-in"), json!({ "token": token })).await?;
    let until = done.until.map(day).unwrap_or_default();
    println!("\nDone. Hub keeps {engine} signed in here until {until} and reminds you a month before.");
    println!("Threads here use it from their next message.");
    Ok(())
}

/// Runs the sign-in on this terminal and returns what it printed. Where `script` can record a
/// terminal session (Linux, macOS) the output is read from its recording, in a file only you can
/// read that is removed straight after; elsewhere it runs as it is and nothing is read.
fn run_captured(words: &[String], data_dir: &Path) -> Result<Option<String>> {
    let script = if cfg!(windows) { None } else { crate::proc::find_program("script") };
    let Some(script) = script else {
        let status = crate::proc::command_with_console(&words[0]).args(&words[1..]).status().context("could not start the sign-in")?;
        if !status.success() {
            bail!("the sign-in did not finish ({status})");
        }
        return Ok(None);
    };
    let file = data_dir.join(format!("sign-in-{}.log", std::process::id()));
    std::fs::write(&file, "")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600))?;
    }
    let line = words.iter().map(|w| shell_quote(w)).collect::<Vec<_>>().join(" ");
    let inner = format!("stty cols {COLUMNS} 2>/dev/null; exec {line}");
    let mut cmd = crate::proc::command_with_console(script);
    if cfg!(target_os = "macos") {
        cmd.arg("-q").arg(&file).args(["sh", "-c", &inner]);
    } else {
        cmd.args(["-q", "-f", "-e", "-c", &inner]).arg(&file);
    }
    let status = cmd.status();
    let printed = std::fs::read(&file).map(|b| String::from_utf8_lossy(&b).into_owned());
    crate::locked::clean_file(&file);
    let status = status.context("could not start the sign-in")?;
    if !status.success() {
        bail!("the sign-in did not finish ({status})");
    }
    Ok(printed.ok())
}

fn shell_quote(word: &str) -> String {
    if !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:@".contains(c)) {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn a_sign_in_made_once_reaches_another_machine_and_never_comes_back() {
        let hub = |dir: &std::path::Path| {
            let config = crate::Config { data_dir: dir.to_owned(), ..Default::default() };
            crate::Hub::new(crate::Store::in_memory().unwrap(), config)
        };
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (pc, tincan) = (hub(a.path()), hub(b.path()));
        let token = "sk-ant-oat01-abcdefghijklmnopqrstuvwxyz";
        pc.signed_in("claude", Some(token)).await.unwrap();

        let sent = pc.outgoing_sign_ins().await;
        assert_eq!(sent.len(), 1);
        tincan.take_sign_ins("pc", sent).await.unwrap();
        assert_eq!(tincan.secrets.cached().get(&vault_name("claude")).map(String::as_str), Some(token));
        let kept = tincan.sign_ins().await.kept["claude"].clone();
        assert_eq!(kept.from.as_deref(), Some("pc"));
        // Engines started there from now on get it.
        assert_eq!(tincan.sign_in_env("claude"), vec![("CLAUDE_CODE_OAUTH_TOKEN".to_owned(), token.to_owned())]);
        // What came from elsewhere is not sent on.
        assert!(tincan.outgoing_sign_ins().await.is_empty());

        // A newer sign-in made on tincan itself stays when the older one comes again.
        let newer = "sk-ant-oat01-zyxwvutsrqponmlkjihgfedcba";
        tincan.signed_in("claude", Some(newer)).await.unwrap();
        tincan.take_sign_ins("pc", pc.outgoing_sign_ins().await).await.unwrap();
        assert_eq!(tincan.secrets.cached().get(&vault_name("claude")).map(String::as_str), Some(newer));
        // Something that is no token is never kept.
        let bad = vec![SharedSignIn { engine: "claude".into(), token: "nope".into(), at: now_ms() + 1, until: now_ms() + DAY_MS }];
        tincan.take_sign_ins("pc", bad).await.unwrap();
        assert_eq!(tincan.secrets.cached().get(&vault_name("claude")).map(String::as_str), Some(newer));
    }

    #[tokio::test]
    async fn a_relaying_machine_hands_on_sign_ins_it_was_sent_and_the_maker_keeps_its_own() {
        let hub = |dir: &std::path::Path, relay: bool| {
            let mut config = crate::Config { data_dir: dir.to_owned(), ..Default::default() };
            config.relay_keys = relay;
            crate::Hub::new(crate::Store::in_memory().unwrap(), config)
        };
        let (a, b, c) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (pc, tincan, vps) = (hub(a.path(), false), hub(b.path(), true), hub(c.path(), false));
        let token = "sk-ant-oat01-abcdefghijklmnopqrstuvwxyz";
        pc.signed_in("claude", Some(token)).await.unwrap();
        tincan.take_sign_ins("pc", pc.outgoing_sign_ins().await).await.unwrap();
        // Tincan relays it; the PC, which made it, is not changed by it coming back.
        let relayed = tincan.outgoing_sign_ins().await;
        assert_eq!(relayed.len(), 1);
        vps.take_sign_ins("tincan", relayed.clone()).await.unwrap();
        assert_eq!(vps.secrets.cached().get(&vault_name("claude")).map(String::as_str), Some(token));
        pc.take_sign_ins("tincan", relayed).await.unwrap();
        assert_eq!(pc.sign_ins().await.kept["claude"].from, None);
        // A machine that does not relay still hands nothing on.
        assert!(vps.outgoing_sign_ins().await.is_empty());
    }

    use super::*;

    #[test]
    fn claude_and_codex_errors_about_sign_in_are_recognised() {
        let claude = crate::config::EngineTraits::preset("claude").auth_phrases.unwrap();
        assert!(is_auth_failure("Failed to authenticate: OAuth session expired and could not be refreshed", &claude));
        assert!(is_auth_failure("Authentication required", &claude));
        assert!(!is_auth_failure("You've hit your limit · resets 3am", &claude));
        let codex = crate::config::EngineTraits::preset("codex").auth_phrases.unwrap();
        assert!(is_auth_failure("Your refresh token was already used. Please log in again.", &codex));
        assert!(!is_auth_failure("anything", &[]));
    }

    #[test]
    fn a_token_is_read_whole_from_a_coloured_terminal() {
        let token = format!("sk-ant-oat01-{}", "AbC_d-9".repeat(14));
        let out = format!(
            "\u{1b}[?25l\u{1b}]0;claude\u{7}✓ Long-lived authentication token created\r\n\r\n\u{1b}[1m{token}\u{1b}[22m\r\n\r\nexport CLAUDE_CODE_OAUTH_TOKEN=<token>\r\n"
        );
        assert_eq!(find_token(&out, "sk-ant-oat").as_deref(), Some(token.as_str()));
        assert_eq!(find_token("Your token: sk-ant-oat\r\n", "sk-ant-oat"), None);
        assert_eq!(find_token("nothing here", "sk-ant-oat"), None);
    }

    #[test]
    fn only_tokens_are_kept() {
        let how = crate::config::EngineTraits::preset("claude").sign_in_token.unwrap();
        assert!(check_token(&format!("sk-ant-oat01-{}", "a".repeat(80)), &how).is_ok());
        assert!(check_token("sk-ant-api03-abcdefghijklmnopqrstuvwxyz", &how).is_err());
        assert!(check_token(&format!("sk-ant-oat01-{} x", "a".repeat(80)), &how).is_err());
    }

    #[test]
    fn the_root_shell_keeps_the_sign_in_out_of_the_command_line() {
        let with = root_command(&["CLAUDE_CODE_OAUTH_TOKEN".to_owned()], std::ffi::OsStr::new("/a:/b"));
        let with: Vec<String> = with.iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(with, ["sudo", "--preserve-env=CLAUDE_CODE_OAUTH_TOKEN", "-i", "env", "PATH=/a:/b", "bash"]);
        let without: Vec<String> = root_command(&[], std::ffi::OsStr::new("/a")).iter().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(without, ["sudo", "-i", "env", "PATH=/a", "bash"]);
    }

    #[test]
    fn vault_names_are_like_variables() {
        assert_eq!(vault_name("claude"), "HUB_SIGNIN_CLAUDE");
        assert_eq!(vault_name("claude-2"), "HUB_SIGNIN_CLAUDE_2");
    }
}
