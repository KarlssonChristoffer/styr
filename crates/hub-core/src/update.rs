//! Self-update. The core follows a branch in its own clone of the repository, runs the tests,
//! builds new commits in release mode, installs them side by side under `bin/<version>` and then
//! hands over to the new version once no thread is working. Engines resume their sessions
//! afterwards.
//!
//! The handover can undo itself. The old core copies the database, lets go of the port and
//! starts the new one, which points `current` at itself once it has opened the database and
//! bound the port. Until then the old core waits; if the new one exits or does not come up in
//! time, the old one stops it, restores the copy when the database was already migrated, takes
//! the port back and remembers the commit as failed.

use std::fs::OpenOptions;
use crate::locked::ResultExt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::config::{BuildMode, UpdateConfig};
use crate::hub::Hub;
use crate::model::{UpdateState, UpdateWait, WaitingChange, now_ms};
mod round;
use round::{Built, Next, build_remote, latest_remote, on_failed, on_ready, show_nothing_new};

/// How long a new version gets to open the database and bind the port.
pub const TAKEOVER_TIMEOUT: Duration = Duration::from_secs(90);

/// Set in the environment of a `hubd` that takes over from an older one.
pub const TAKEOVER_VAR: &str = "HUB_TAKEOVER";

/// A built and installed version, ready to take over.
pub struct Ready {
    pub version: String,
    pub hubd: PathBuf,
}

/// Follows the branch until a new version is installed and nothing is running, then returns it
/// to start in this one's place.
pub async fn run(hub: Arc<Hub>, cfg: UpdateConfig, data: PathBuf) -> Ready {
    let src = cfg.source_in(&data);
    {
        let (cfg, src) = (cfg.clone(), src.clone());
        blocking(move || {
            trim(&cfg, &src);
            Ok(())
        })
        .await.warn_on_err("trim the update folders");
    }
    // A commit that failed once is not tried again, also after a restart.
    let mut failed = std::fs::read_to_string(failed_file(&data)).ok().map(|v| v.trim().to_owned());
    if hub.update_status().state != UpdateState::Failed {
        hub.set_update(|s| s.state = UpdateState::Idle);
    }
    // The version last put on offer, so a freshly built one is followed by a check at once.
    let mut offered: Option<String> = None;
    loop {
        match check(&hub, &cfg, &data, &src, failed.as_deref()).await {
            Ok(Some(version)) => match on_ready(&hub, &cfg, &data, version, &mut offered).await {
                Next::Return(ready) => return ready,
                _ => continue,
            },
            Ok(None) => {}
            Err(err) => match on_failed(&hub, &cfg, &data, &err, &mut failed).await {
                Next::Return(ready) => return ready,
                Next::Again => continue,
                Next::Wait => {}
            },
        }
        tokio::select! {
            _ = hub.retry_requested() => {
                failed = None;
                crate::locked::clean_file(failed_file(&data));
            }
            _ = crate::update_signals::look().notified() => {}
            _ = hub.build_now_requested() => {}
            _ = tokio::time::sleep(nap(&hub, &cfg)) => {}
        }
    }
}

/// How long to wait before looking again: the usual interval, or less when a version waiting for
/// a quiet branch is due sooner.
fn nap(hub: &Hub, cfg: &UpdateConfig) -> Duration {
    let every = Duration::from_secs(cfg.interval_secs.max(10));
    match hub.update_status().waiting_until {
        Some(until) => every.min(Duration::from_millis((until - now_ms()).max(1_000) as u64)),
        None => every,
    }
}

/// One round: fetch, and build and install when the branch moved. Returns the version that is
/// ready to start.
async fn check(
    hub: &Arc<Hub>,
    cfg: &UpdateConfig,
    data: &Path,
    src: &Path,
    failed: Option<&str>,
) -> Result<Option<String>> {
    let cfg = &live(cfg);
    let previous = hub.update_status().state;
    // A version built and waiting for you stays on offer while newer ones are built behind it.
    let ready = (previous == UpdateState::Ready)
        .then(|| hub.update_status().target)
        .flatten()
        .filter(|v| installed(data, v));
    // A failure or a version on offer stays shown while checking again.
    if !matches!(previous, UpdateState::Failed | UpdateState::Ready) {
        hub.set_update(|s| s.state = UpdateState::Checking);
    }
    let Some(remote) = latest_remote(hub, cfg, data, src).await? else { return Ok(None) };

    let running = hub.update_status().version;
    if let Some(ready) = ready.clone().filter(|_| remote == running || Some(remote.as_str()) == failed) {
        return Ok(Some(ready));
    }
    if remote == running || Some(remote.as_str()) == failed {
        // Nothing new, or the commit that failed last time: keep showing what happened.
        show_nothing_new(hub, remote == running);
        return Ok(None);
    }

    if cfg.pushed && !installed(data, &remote) {
        bail!("{remote} was pushed without its files");
    }
    if !installed(data, &remote) {
        // A version that builds the programs waits for a quiet branch, so Rust changes merged one
        // after another become one version; "build now" skips that.
        let held = if hub.take_build_now() || cfg.pushed {
            None
        } else {
            let (data2, src2, remote2, running2, cfg3) = (data.to_owned(), src.to_owned(), remote.clone(), running.clone(), cfg.clone());
            let built_at = LAST_BUILD_END.load(Ordering::Relaxed);
            blocking(move || Ok(hold(&data2, &src2, &remote2, &running2, &cfg3, built_at, now_ms()))).await?
        };
        let until = held.as_ref().and_then(|h| h.until);
        hub.set_update(|s| {
            if ready.is_some() {
                s.building = Some(remote.clone());
            } else {
                s.state = UpdateState::Building;
                s.target = Some(remote.clone());
                s.message = None;
            }
            s.step = held.as_ref().map(|h| h.why.clone());
            s.waiting_until = until;
            s.waiting = held.as_ref().map(|h| h.wait.clone());
        });
        if held.is_some() {
            return Ok(ready);
        }
        if let Built::Stop(answer) = build_remote(hub, cfg, data, src, &remote, ready.as_ref()).await? {
            return Ok(answer);
        }
    }
    // Patch notes for everything since the running version, read after the restart.
    // A machine the PC pushes to has no clone to read them from.
    if !cfg.pushed && hub.store.get_value(&format!("notes:{remote}")).await.ok().flatten().is_none() {
        // The model is asked at most once an hour; in between the commits are the notes.
        let (from, write) = crate::notes::spaced(&hub.store, (running != "dev").then_some(running.as_str()), now_ms()).await;
        let ask = |p: String| async move { if write { crate::engine::ask_tidy(&hub.config, "notes", p, crate::notes::WAIT).await } else { Ok(String::new()) } };
        if let Err(err) = crate::notes::record(&hub.store, src, from.as_deref(), &remote, &remote, ask).await {
            tracing::warn!("patch notes failed: {err:#}");
        }
    }
    // `current` moves only once the new version is up; see `took_over`.
    hub.set_update(|s| {
        s.state = UpdateState::Ready;
        s.target = Some(remote.clone());
        s.message = None;
        s.building = None;
        s.step = None;
        s.waiting_until = None;
        s.waiting = None;
        s.checked_at = Some(now_ms());
    });
    Ok(Some(remote))
}

/// How long an update waits for threads to stop working before it restarts anyway.
const IDLE_WAIT: Duration = Duration::from_secs(120);

/// Builds the checked out source and installs it as `version`, telling `progress` as each step
/// starts. Also used by `hubd install`.
/// Setting `stop` stops the step running and fails the build.
pub use crate::update_signals::building;

/// The build folder is shared by the update and the tests (`checks.rs`). Two
/// cargos in it at once lock each other's test programs (LNK1104 on Windows) and their builds,
/// so they take turns: whoever builds holds this.
static BUILD_FOLDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Waits for the build folder, saying so once through `waiting`. Ends early when `stop` is set.
pub(crate) fn hold_build_folder(waiting: &dyn Fn(), stop: &AtomicBool) -> Result<std::sync::MutexGuard<'static, ()>> {
    let mut told = false;
    loop {
        match BUILD_FOLDER.try_lock() {
            Ok(held) => return Ok(held),
            // A build that panicked left nothing to guard but the folder itself.
            Err(std::sync::TryLockError::Poisoned(p)) => return Ok(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        if stop.load(Ordering::Relaxed) {
            bail!("stopped while waiting for the build folder");
        }
        if !told {
            waiting();
            told = true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Clears the building flag however the build ends.
struct Building;

impl Drop for Building {
    fn drop(&mut self) {
        crate::update_signals::set_building(false);
    }
}

pub fn build_and_install(
    cfg: &UpdateConfig,
    data: &Path,
    src: &Path,
    version: &str,
    progress: &dyn Fn(String),
    stop: &AtomicBool,
) -> Result<()> {
    build_and_install_with(cfg, data, src, version, progress, stop, None)
}

/// `build_and_install`, with the hub that lets the tests run on another machine
/// (`update.tests_on`, see `checks`).
pub(crate) fn build_and_install_with(
    cfg: &UpdateConfig,
    data: &Path,
    src: &Path,
    version: &str,
    progress: &dyn Fn(String),
    stop: &AtomicBool,
    hub: Option<&Arc<Hub>>,
) -> Result<()> {
    crate::update_signals::set_building(true);
    let _building = Building;
    // Only the pages changed since a build whose programs can be kept: no Rust is tested or built.
    if let Some(donor) = pages_only_donor(cfg, data, src, version) {
        progress("Only the pages changed: the programs of the version before are kept".into());
        match build_pages_only(cfg, data, src, version, &donor, progress, stop) {
            Ok(()) => return Ok(()),
            // A check or the build of the pages failing is the commit's fault; a fast lane that
            // could not be taken (files missing from the version before) falls back to the full build.
            Err(err) if err.downcast_ref::<PagesUnavailable>().is_none() => return Err(err),
            Err(err) => tracing::warn!("the pages-only update was not possible, building everything: {err:#}"),
        }
    }
    let _folder = hold_build_folder(&|| progress("Waits for the tests to finish with the build folder".into()), stop)?;
    let steps = cfg.steps.clone().unwrap_or_else(UpdateConfig::default_steps);
    let target = target_dir(cfg, src);
    // The tests on another machine, started now so they run while this one builds. Only with the
    // default steps: a build with steps of its own says what runs where.
    let default_steps = cfg.steps.is_none();
    // Install first, test after: the version is installed when it is built and the tests' verdict
    // arrives later as a problem. Only where another machine runs them; with none, they do not run.
    let tests_after = cfg.tests_after && cfg.tests_on.is_some() && default_steps && hub.is_some();
    let elsewhere = match (hub, cfg.tests_on.as_deref(), default_steps) {
        (Some(hub), Some(host), true) => match checks::start(hub, host, version, src, progress) {
            Ok(session) => Some((hub, session)),
            Err(err) => {
                let note = if tests_after {
                    format!("{host} cannot run the tests, so they do not run for {version}: {err:#}")
                } else {
                    format!("{host} cannot run the tests, so they run here: {err:#}")
                };
                tracing::warn!("{note}");
                note_in_log(data, &note);
                progress(note.clone());
                if tests_after {
                    checks::report(version, host, crate::problems::Level::Warning, &note, None);
                }
                None
            }
        },
        _ => None,
    };
    let here: Vec<_> = if elsewhere.is_some() || tests_after { steps.iter().filter(|s| !checks::is_check_step(s)).cloned().collect() } else { steps };
    run_steps(cfg, data, src, &target, &here, progress, stop)?;
    if let Some((hub, session)) = elsewhere {
        let limit = Duration::from_secs(cfg.step_timeout_mins.max(1) * 60 * 3);
        if tests_after {
            install(cfg, data, src, version)?;
            trim(cfg, src);
            checks::watch_after(hub, session, limit);
            return Ok(());
        }
        match checks::wait(hub, &session, limit, progress, stop)? {
            checks::Outcome::Passed => note_in_log(data, &format!("the tests passed on {}", session.host)),
            checks::Outcome::Failed(tail) => {
                note_in_log(data, &format!("the tests failed on {}:\n{tail}", session.host));
                bail!("the tests failed on {}:\n{tail}", session.host);
            }
            // The machine lost the run: not this commit's fault, so the tests run here after all.
            checks::Outcome::Lost(why) => {
                let note = format!("{why}; the tests run here");
                note_in_log(data, &note);
                progress(note);
                let tests: Vec<_> = UpdateConfig::default_steps().into_iter().filter(checks::is_check_step).collect();
                checks::checks_in(cfg, checks::CheckPaths { gate: src, log: &data.join("update.log"), target: &target }, &tests, &[], progress, stop)?;
            }
        }
    }
    install(cfg, data, src, version)?;
    trim(cfg, src);
    Ok(())
}

/// A line in `update.log` between the steps, for the record.
fn note_in_log(data: &Path, text: &str) {
    if let Ok(mut log) = OpenOptions::new().create(true).append(true).open(data.join("update.log")) {
        (writeln!(log, "\n== {text}")).warn_on_err("writing the log");
    }
}

fn install(cfg: &UpdateConfig, data: &Path, src: &Path, version: &str) -> Result<()> {
    if version == hub_install::running_version() {
        bail!("{version} is the running version; it cannot be replaced while it runs");
    }
    let partial = hub_install::bin_dir(data).join(format!("{version}.partial"));
    assemble(cfg, src, &target_dir(cfg, src), &partial, version)?;
    place(data, &partial, version)
}

/// Collects a built version's files from `src` into the fresh folder `dest`.
fn assemble(cfg: &UpdateConfig, src: &Path, target: &Path, dest: &Path, version: &str) -> Result<()> {
    crate::locked::clean_dir(dest);
    std::fs::create_dir_all(dest)?;
    for artifact in cfg.artifacts.clone().unwrap_or_else(UpdateConfig::default_artifacts) {
        // Artifacts under `target/` follow the build folder when it was moved.
        let from = match artifact.strip_prefix("target") {
            Ok(rest) => target.join(rest),
            Err(_) => src.join(&artifact),
        };
        let name = artifact.file_name().context("artifact without a file name")?;
        std::fs::copy(&from, dest.join(name)).with_context(|| format!("copying {}", from.display()))?;
    }
    // The app's pages and its shell's stamp, so an open window can take this version's pages
    // without restarting (`crate::shell`). Without them it restarts, as before.
    let pages = src.join("app/dist");
    if pages.join("index.html").is_file() {
        match crate::shell::stamp(src) {
            Ok(stamp) => {
                copy_dir(&pages, &dest.join(hub_install::UI_DIR)).context("copying the app's pages")?;
                std::fs::write(dest.join(hub_install::SHELL), stamp)?;
            }
            Err(err) => tracing::warn!("no shell stamp, so the window restarts for this version: {err:#}"),
        }
    }
    if let Some(native) = native_stamp(src) {
        std::fs::write(dest.join(hub_install::NATIVE), native)?;
    }
    hub_install::probation::stamp_version(dest, version)?;
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest).with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Keeps the new version and the running one, and the ones going back needs (the last good version
/// and what a version on probation replaced); older builds go. A folder that cannot go yet
/// (Windows: a program started from it still runs, or still has it as its working folder) is
/// left for the next prune, and said so in the log, so leftovers are not silent.
pub(crate) fn prune(data: &Path, keep: &str) {
    let running = hub_install::running_version();
    let back = hub_install::probation::keep(data);
    let current = hub_install::current_version(data);
    let Ok(entries) = std::fs::read_dir(hub_install::bin_dir(data)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == keep || name == running || current.as_deref() == Some(name.as_str()) || back.contains(&name) {
            continue;
        }
        let path = entry.path();
        // A build is a folder. A file here is the shell wrapper (`live_output::wrapper`), which
        // engines that outlive this update still run every command through.
        if !path.is_dir() {
            continue;
        }
        if let Err(err) = std::fs::remove_dir_all(&path) {
            tracing::info!("old build {} stays for now: {err}", path.display());
        }
    }
}

fn installed(data: &Path, version: &str) -> bool {
    hub_install::bin_dir(data).join(version).join("VERSION").exists()
}

/// `git fetch` of the branch. Git never opens a sign-in window from here (see `git::output`), so
/// a repository that needs a sign-in the PC does not have fails; then it tries once more with the
/// GitHub CLI's sign-in, which Hub already asks for to show pull requests, before saying what to do.
fn fetch(dir: &Path, remote: &str, branch: &str) -> Result<()> {
    let first = match git(dir, &["fetch", "--quiet", remote, branch]) {
        Ok(_) => return Ok(()),
        Err(e) => e,
    };
    if !is_sign_in_failure(&format!("{first:#}")) {
        return Err(first);
    }
    let with_gh = ["-c", "credential.helper=", "-c", "credential.helper=!gh auth git-credential", "fetch", "--quiet", remote, branch];
    if git(dir, &with_gh).is_ok() {
        return Ok(());
    }
    Err(first.context("GitHub did not accept the saved sign-in, so Hub cannot look for updates. Run `gh auth login` once on this computer"))
}

/// Whether git failed because GitHub wanted a sign-in it did not get, rather than the network.
fn is_sign_in_failure(message: &str) -> bool {
    let message = message.to_lowercase();
    ["authentication failed", "could not read username", "invalid username or token", "terminal prompts disabled", "interactivity has been disabled", "user cancelled dialog", "failed to execute prompt script"]
        .iter()
        .any(|needle| message.contains(needle))
}

pub(crate) use crate::git::checked as git;

/// Runs one build step with its output going straight into the log, as it comes, so the log
/// shows how far a long step got. A step still running after `limit` is stopped with everything
/// it started.
pub(crate) fn run_logged(dir: &Path, cmd: &[String], env: &[(&str, String)], log_path: &Path, limit: Duration, stop: &AtomicBool) -> Result<()> {
    let (program, args) = cmd.split_first().context("empty build step")?;
    let mut log = OpenOptions::new().create(true).append(true).open(log_path)?;
    writeln!(log, "\n== {} $ {}", dir.display(), cmd.join(" "))?;
    let from = log.metadata()?.len();
    let mut proc = command(program, dir);
    // Below normal priority, which cargo and rustc inherit: a build from scratch takes every
    // core it is given, and your windows and Hub itself must still win over it.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        proc.creation_flags(0x0800_0000 | 0x0000_4000); // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS
    }
    // Its own process group, so stopping it reaches everything it started (`kill_tree`).
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut proc, 0);
    let mut child = proc
        .args(args)
        .envs(env.iter().map(|(k, v)| (*k, v.as_str())))
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log.try_clone()?)
        .spawn()
        .with_context(|| format!("starting {program}"))?;
    let start = std::time::Instant::now();
    let mut low_since: Option<std::time::Instant> = None;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if stop.load(Ordering::Relaxed) {
            kill_tree(&mut child);
            (writeln!(log, "\n== stopped: installing the version that was ready")).warn_on_err("writing the log");
            bail!("stopped to install the version that was ready");
        }
        // Dictation needs room to load its model, and a build took it (whisper-server exited).
        if crate::voice::dictating()
            && headroom().is_some_and(|left| left < DICTATION_MEMORY)
        {
            kill_tree(&mut child);
            (writeln!(log, "\n== stopped: making room for dictation; the step runs again after")).warn_on_err("writing the log");
            return Err(GaveWay.into());
        }
        // Hub and your threads matter more than the build: when memory runs short, the build goes.
        // But not for a dip: rustc and the linker swing by gigabytes, and a stop throws away the
        // minutes the step had compiled (the PC lost a hub-core compile to a dip of a second or two).
        // Only memory that stays short for `LOW_MEMORY_GRACE` ends the step.
        let left = headroom();
        if memory_short(&mut low_since, left, std::time::Instant::now()) {
            let left = left.unwrap_or(0);
            kill_tree(&mut child);
            (writeln!(log, "\n== memory when it stopped, biggest first: {}", biggest_processes())).warn_on_err("writing the log");
            (writeln!(log, "\n== stopped: the computer was running out of memory ({} MB left)", left >> 20)).warn_on_err("writing the log");
            return Err(MemoryShort(format!("`{}` was stopped because the computer was running out of memory (full log in {})", cmd.join(" "), log_path.display())).into());
        }
        if start.elapsed() > limit {
            kill_tree(&mut child);
            (writeln!(log, "\n== stopped after {} minutes", limit.as_secs() / 60)).warn_on_err("writing the log");
            bail!(
                "`{}` was still running after {} minutes and was stopped (full log in {}):\n{}",
                cmd.join(" "),
                limit.as_secs() / 60,
                log_path.display(),
                tail(log_path, from)
            );
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    if !status.success() {
        // A crash prints a long backtrace, so the cause is often above the last lines: say it.
        let whole = std::fs::read(log_path).unwrap_or_default();
        let whole = String::from_utf8_lossy(whole.get(from as usize..).unwrap_or_default());
        let why = if out_of_memory(&whole) {
            " because it ran out of memory"
        } else if broken_build_folder(&whole) {
            " because the build folder is broken (crates rustc cannot load)"
        } else {
            ""
        };
        // The names of the tests that fell are far above the last lines when a test's output is
        // long; they go first, in libtest's own words, so an older PC's `prci` finds them too.
        let fell: String = failed_tests(&whole).iter().take(20).map(|name| format!("test {name} ... FAILED\n")).collect();
        bail!("`{}` failed{why} (full log in {}):\n{fell}{}", cmd.join(" "), log_path.display(), tail(log_path, from));
    }
    Ok(())
}

/// A step stopped to leave memory to dictation. It runs again when dictation is quiet.
#[derive(Debug, thiserror::Error)]
#[error("stepped aside for dictation")]
pub(crate) struct GaveWay;

/// A step stopped because memory stayed short: the machine's state, not the commit's fault.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct MemoryShort(pub String);

/// A build that failed because the build folder holds crates rustc cannot load, not because of the code.
pub(crate) fn broken_build_folder(output: &str) -> bool {
    ["build folder is broken", "required to be available in rlib format", "found possibly newer version of crate", "found invalid metadata files", "error[e0463]", "error[e0786]"]
        .iter()
        .any(|m| output.to_lowercase().contains(m))
}

/// The tests that fell in cargo test's output, each once, in order: `test a::b ... FAILED` lines and
/// the names listed under libtest's closing `failures:`.
pub(crate) fn failed_tests(output: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut listing = false;
    for line in output.lines() {
        let name = if line.trim_end() == "failures:" {
            listing = true;
            continue;
        } else if listing && line.trim().is_empty() {
            continue;
        } else if listing && line.starts_with("    ") && !line.trim().contains(' ') {
            Some(line.trim())
        } else {
            listing = false;
            line.trim().strip_prefix("test ").and_then(|l| l.strip_suffix(" ... FAILED"))
        };
        // A test's name has no spaces; a line two writers mixed is no name.
        if let Some(name) = name.filter(|n| !n.contains(' ') && !names.iter().any(|x| x == n)) {
            names.push(name.to_owned());
        }
    }
    names
}

/// The last lines a step wrote to the log, from byte `from` on.
fn tail(log_path: &Path, from: u64) -> String {
    let text = std::fs::read(log_path).unwrap_or_default();
    let text = String::from_utf8_lossy(text.get(from as usize..).unwrap_or_default());
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(12)..].join("\n")
}

/// Stops a step and what it started: cargo runs the tests and npm runs node as children of their own.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        command("taskkill", Path::new(".")).args(["/T", "/F", "/PID", &child.id().to_string()]).output().warn_on_err("taskkill the step");
    }
    #[cfg(unix)]
    {
        // The whole process group (`run_logged` gave the step its own): cargo's rustc and test
        // binaries and npm's node are grandchildren, which `pkill -P` would not reach.
        crate::proc::command("kill").args(["-KILL", "--", &format!("-{}", child.id())]).output().warn_on_err("kill the step's process group");
    }
    child.kill().warn_on_err("kill the step");
    child.wait().warn_on_err("wait for the killed step");
}

pub(crate) fn command(program: &str, dir: &Path) -> Command {
    let mut cmd = crate::proc::command(program);
    cmd.current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await?
}

mod memory;
pub(crate) use memory::*;

mod pages;
pub(crate) use pages::*;

mod takeover;
pub use takeover::*;

mod steps;
pub use steps::*;

mod quiet;
pub use quiet::*;

/// The tests on another machine (`update.tests_on`); they share the build folder and the step
/// runner with the update, so they live under it.
pub mod checks;

#[cfg(test)]
mod tests;
