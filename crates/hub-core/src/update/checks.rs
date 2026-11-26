//! Tests on another machine. The PC's update builds Hub's release in a few GB and used to run the
//! tests (`npm run check`, `cargo test`) first, in the same memory. With `update.tests_on` set to a
//! machine's name, the PC sends that machine the commit, the machine runs the tests there, and the
//! PC builds only the release meanwhile. The version is installed only once the machine says the
//! tests passed.
//!
//! Two halves in one file. The machine that runs the tests (a Linux server such as tincan) keeps
//! its own repository in `<data>/checks/src`, receives commits into it as git bundles (it has no
//! GitHub login), checks one out and runs the same steps the merge queue runs, into
//! `<data>/checks.log`. The PC asks it what it has (`GET /api/checks`), sends the commit
//! (`POST /api/checks/source`), starts the run (`POST /api/checks/run`) and polls the verdict.
//!
//! A machine that cannot do it (offline, hubd too old, no Rust, too little disk) never costs a
//! version: the PC runs the tests itself, as before.

use crate::locked::{LockExt, ResultExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{BuildStep, UpdateConfig, is_build_step};
use crate::hub::Hub;
use crate::model::now_ms;
use crate::problems::{Hit, Incoming, Level};
use crate::update::{self, blocking, git};

/// How a run of the tests stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Running,
    Passed,
    Failed,
    /// The machine could not finish the run (memory stayed short through every retry): says
    /// nothing about the commit. An older PC reads it as `Unknown`, which it treats the same way.
    Stopped,
    /// A state a newer hubd has that this one does not know.
    #[serde(other)]
    Unknown,
}

/// One run of the tests, as the machine that runs them tells it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobView {
    pub version: String,
    pub state: JobState,
    /// The step it is on, while it runs.
    #[serde(default)]
    pub step: Option<String>,
    pub started_at: i64,
    #[serde(default)]
    pub finished_at: Option<i64>,
    /// Why it failed: the end of the failing step's output.
    #[serde(default)]
    pub tail: Option<String>,
}

/// A step that ran out of memory through every retry: the machine's fault, not the commit's.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct MachineShort(pub String);

/// What a machine answers to "can you run the tests?".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub ready: bool,
    /// What stops it, in words, when it is not ready.
    #[serde(default)]
    pub missing: Vec<String>,
    /// The commit it received last, so the next one can be sent as the difference.
    #[serde(default)]
    pub have: Option<String>,
    /// The run it has or had last.
    #[serde(default)]
    pub job: Option<JobView>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunAsk {
    pub version: String,
}

/// How many installed versions' tests this machine still waits for; the machine runs one test run
/// at a time, so a run for a pull request (`prci`) must not start meanwhile and stop them.
static WATCHING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Whether the tests of an installed version are still running on another machine.
pub fn watching() -> bool {
    WATCHING.load(Ordering::SeqCst) > 0
}

static JOB: Mutex<Option<JobView>> = Mutex::new(None);
static STOP: Mutex<Option<Arc<Halt>>> = Mutex::new(None);

/// The tail of a run this machine stopped to install a ready version. A run whose tail starts with
/// "stopped " ended `stopped`: the PC posts it as "waiting" and tries again (`outcome_of`).
pub const STOPPED_FOR_INSTALL: &str = "stopped to install the update";
/// The tail of a run stopped because the tests of another version took the machine's one slot.
pub const STOPPED_FOR_ANOTHER: &str = "stopped for the tests of another version";
/// How long a run may hold the install of a ready version on this machine. The PC gives up waiting
/// for a pull request's run after as long (`prci`), so a longer hold would keep nothing.
pub const RUN_LIMIT: Duration = Duration::from_secs(40 * 60);

/// The stop of one run, and why, so its end says why instead of failing the commit.
struct Halt {
    flag: AtomicBool,
    why: Mutex<&'static str>,
}

/// Stops the run that is going with `why` as its end and gives it a minute to let go of the folder.
fn halt(why: &'static str) {
    let Some(halt) = STOP.locked().take() else { return };
    *halt.why.locked() = why;
    halt.flag.store(true, Ordering::Relaxed);
    for _ in 0..120 {
        if JOB.locked().as_ref().is_none_or(|j| j.state != JobState::Running) {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Whether the run `job` still holds an install that has waited since `began` (ms): while it runs,
/// and neither it nor the wait has passed `RUN_LIMIT`.
fn holds_install(job: Option<&JobView>, began: i64, now: i64) -> bool {
    let limit = RUN_LIMIT.as_millis() as i64;
    job.is_some_and(|j| j.state == JobState::Running && now - j.started_at < limit) && now - began < limit
}

/// Before this machine installs a ready version (the install restarts hubd, and a run lives in it):
/// the run that is going (a pull request's tests or the PC's update's) finishes first, up to
/// `RUN_LIMIT`. One still running then is stopped as `STOPPED_FOR_INSTALL`, which the PC posts as
/// "waiting" and tries again after the restart. `say` hears once what it waits for.
pub async fn finish_before_install(say: impl Fn(String)) {
    let began = now_ms();
    let mut told = false;
    loop {
        let job = JOB.locked().clone();
        let Some(job) = job.filter(|j| holds_install(Some(j), began, now_ms())) else { break };
        if !told {
            say(format!("Waits for the tests of {} to finish before it installs (at most {} minutes)", crate::text::short_version(&job.version), RUN_LIMIT.as_secs() / 60));
            told = true;
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    if JOB.locked().as_ref().is_some_and(|j| j.state == JobState::Running) {
        blocking(|| {
            halt(STOPPED_FOR_INSTALL);
            Ok(())
        })
        .await
        .warn_on_err("stop the tests before the install");
    }
}

fn dir(data: &Path) -> PathBuf {
    data.join("checks")
}

fn repo(data: &Path) -> PathBuf {
    dir(data).join("src")
}

fn log_file(data: &Path) -> PathBuf {
    data.join("checks.log")
}

/// The build folder of the tests: inside the configured build folder (so it can sit on a big
/// drive), else in the checks folder.
fn target(cfg: &UpdateConfig, data: &Path) -> PathBuf {
    cfg.target_dir.as_ref().map_or_else(|| dir(data).join("target"), |d| d.join("checks"))
}

/// The steps of the tests: the update's, up to the release build.
pub fn steps() -> Vec<BuildStep> {
    crate::config::default_checks()
}

/// Whether a step of the update is one of the tests (not the packages, not the release build).
pub fn is_check_step(step: &BuildStep) -> bool {
    !is_build_step(step) && !step.command.iter().any(|a| a == "install")
}

/// The environment of the test steps: cargo's folder (where rustup puts it) and Hub's own Node
/// are found even when the service that runs hubd has a short PATH.
fn tool_env() -> Vec<(&'static str, String)> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        dirs.push(PathBuf::from(home).join(".cargo").join("bin"));
    }
    dirs.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    match std::env::join_paths(dirs) {
        Ok(path) => vec![("PATH", path.to_string_lossy().into_owned())],
        Err(_) => Vec::new(),
    }
}

fn tool_ok(program: &str) -> bool {
    let mut cmd = crate::proc::command(program);
    cmd.arg("--version").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    for (k, v) in tool_env() {
        cmd.env(k, v);
    }
    cmd.status().is_ok_and(|s| s.success())
}

fn is_sha(text: &str) -> bool {
    (7..=64).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

// ---- the machine that runs the tests ----

/// What this machine can do for a PC that asks.
pub async fn status(hub: &Hub) -> Result<Status> {
    let (data, cfg) = (hub.config.data_dir.clone(), hub.config.update.clone());
    blocking(move || Ok(status_in(&data, &cfg))).await
}

fn status_in(data: &Path, cfg: &UpdateConfig) -> Status {
    let mut missing = Vec::new();
    for tool in ["git", "node", "cargo"] {
        if !tool_ok(tool) {
            missing.push(tool.to_owned());
        }
    }
    if let Err(err) = update::check_space(cfg, &target(cfg, data)) {
        missing.push(format!("{err}"));
    }
    let src = repo(data);
    let have = git(&src, &["rev-parse", "--verify", "--quiet", "refs/hub/received"]).ok().filter(|v| is_sha(v));
    Status { ready: missing.is_empty(), missing, have, job: JOB.locked().clone() }
}

/// Installs Rust (rustup, the minimal profile, no sudo) when it is the only thing missing. It is
/// the one tool Hub does not bring along with its own installer.
pub async fn prepare(hub: &Hub) -> Result<()> {
    let data = hub.config.data_dir.clone();
    blocking(move || {
        if tool_ok("cargo") {
            return Ok(());
        }
        if !cfg!(unix) {
            bail!("Rust is missing; install it from https://rustup.rs");
        }
        std::fs::create_dir_all(dir(&data))?;
        let log = std::fs::OpenOptions::new().create(true).append(true).open(log_file(&data))?;
        let status = crate::proc::command("sh")
            .args(["-c", "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --no-modify-path"])
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .status()
            .context("starting the Rust installer; is curl installed?")?;
        if !status.success() || !tool_ok("cargo") {
            bail!("installing Rust failed; see {}", log_file(&data).display());
        }
        Ok(())
    })
    .await
}

/// Takes a git bundle (`remote::version_bundle`) into this machine's repository and notes the
/// commit it brought as the latest it has. Returns that commit.
pub async fn receive(hub: &Hub, bytes: Vec<u8>) -> Result<String> {
    let data = hub.config.data_dir.clone();
    blocking(move || receive_in(&data, &bytes)).await
}

fn receive_in(data: &Path, bytes: &[u8]) -> Result<String> {
    let src = repo(data);
    std::fs::create_dir_all(&src)?;
    if !src.join(".git").exists() {
        git(&src, &["init", "--quiet"])?;
    }
    let file = dir(data).join("in.bundle");
    std::fs::write(&file, bytes)?;
    let file_text = file.to_string_lossy().into_owned();
    let fetched = git(&src, &["fetch", "--quiet", &file_text, "+refs/hub-ship/*:refs/hub-in/*"]);
    crate::locked::clean_file(&file);
    fetched.context("taking the commit in")?;
    let tip = git(&src, &["for-each-ref", "--format=%(objectname)", "refs/hub-in"])?;
    let tip = tip.lines().next().map(str::to_owned).filter(|t| is_sha(t)).context("the bundle held no commit")?;
    git(&src, &["update-ref", "refs/hub/received", &tip])?;
    // The ref was only the way in; the commit is kept by `received`.
    for line in git(&src, &["for-each-ref", "--format=%(refname)", "refs/hub-in"])?.lines() {
        git(&src, &["update-ref", "-d", line]).warn_on_err("dropping a received ref");
    }
    Ok(tip)
}

/// Starts the tests of `version` (a commit it received). The same version asked again is the
/// same run; another version stops the run that is going and takes its place.
pub async fn run(hub: &Hub, version: &str) -> Result<JobView> {
    if !is_sha(version) {
        bail!("{version} is not a commit");
    }
    let (data, cfg) = (hub.config.data_dir.clone(), hub.config.update.clone());
    let version = version.to_owned();
    blocking(move || start_in(&data, &cfg, &version, steps(), tool_env())).await
}

fn start_in(data: &Path, cfg: &UpdateConfig, version: &str, steps: Vec<BuildStep>, env: Vec<(&'static str, String)>) -> Result<JobView> {
    let src = repo(data);
    let full = git(&src, &["rev-parse", "--verify", &format!("{version}^{{commit}}")]).with_context(|| format!("{version} was not sent here"))?;
    let short = full.chars().take(version.len().max(12)).collect::<String>();
    {
        let job = JOB.locked();
        if let Some(job) = job.as_ref().filter(|j| j.version == short || j.version == version)
            && !matches!(job.state, JobState::Failed | JobState::Stopped)
        {
            return Ok(job.clone());
        }
    }
    // Another version: the run that goes on is stopped, and gets a minute to let go of the folder.
    halt(STOPPED_FOR_ANOTHER);
    let view = JobView { version: short.clone(), state: JobState::Running, step: Some("starting".into()), started_at: now_ms(), finished_at: None, tail: None };
    *JOB.locked() = Some(view.clone());
    let stop = Arc::new(Halt { flag: AtomicBool::new(false), why: Mutex::new(STOPPED_FOR_ANOTHER) });
    *STOP.locked() = Some(stop.clone());
    let (data, cfg) = (data.to_owned(), cfg.clone());
    std::thread::spawn(move || {
        let outcome = checkout_and_check(&data, &cfg, &full, &steps, &env, &stop.flag, &|step| {
            if let Some(job) = JOB.locked().as_mut().filter(|j| j.version == short) {
                job.step = Some(step);
            }
        });
        let mut job = JOB.locked();
        // A run that was stopped for a newer one has been replaced; its end says nothing.
        let Some(job) = job.as_mut().filter(|j| j.version == short) else { return };
        job.finished_at = Some(now_ms());
        job.step = None;
        match outcome {
            Ok(()) => job.state = JobState::Passed,
            // Stopped (for an install, another version's tests, or memory) says nothing about the
            // commit: `stopped`, never `failed` (a stop read as failed painted #1083 red).
            Err(_) if stop.flag.load(Ordering::Relaxed) => {
                job.state = JobState::Stopped;
                job.tail = Some((*stop.why.locked()).to_owned());
            }
            Err(err) => {
                job.state = if err.downcast_ref::<MachineShort>().is_some() { JobState::Stopped } else { JobState::Failed };
                let text = format!("{err:#}");
                job.tail = Some(text.chars().take(4000).collect());
            }
        }
    });
    Ok(view)
}

fn checkout_and_check(
    data: &Path,
    cfg: &UpdateConfig,
    full: &str,
    steps: &[BuildStep],
    env: &[(&str, String)],
    stop: &AtomicBool,
    progress: &dyn Fn(String),
) -> Result<()> {
    let src = repo(data);
    git(&src, &["checkout", "--quiet", "--force", "--detach", full])?;
    // What an earlier run left that git tracks nothing of; ignored folders (node_modules) stay.
    git(&src, &["clean", "--quiet", "-fd"])?;
    let target = target(cfg, data);
    update::check_space(cfg, &target)?;
    let log = log_file(data);
    update::cap_log(&log);
    let temp = data.join("test-temp");
    crate::locked::clean_dir(&temp);
    std::fs::create_dir_all(&temp)?;
    let mut env = env.to_vec();
    for key in ["TEMP", "TMP", "TMPDIR"] {
        env.push((key, temp.display().to_string()));
    }
    let result = checks_in(cfg, CheckPaths { gate: &src, log: &log, target: &target }, steps, &env, progress, stop);
    crate::locked::clean_dir(&temp);
    result
}

// ---- the machine that wants them run ----

/// A run of the tests on another machine, started for one version.
pub struct Session {
    pub host: String,
    pub version: String,
}

/// What came of waiting for the tests.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Passed,
    /// The tests ran and failed: the commit's fault.
    Failed(String),
    /// The machine lost or stopped the run (it restarted, went offline, installed an update, ran
    /// another version's tests, ran short of memory): not the commit's fault, run them here.
    Lost(String),
}

/// Whether `full` (a commit in `src`) is already in the history of `have`, the commit the host
/// reports as its latest: then nothing is sent, since a bundle of what `have` already has is empty
/// and git refuses to make one. `have` not in `src` (or none) means it is sent.
fn already_there(src: &Path, full: &str, have: Option<&str>) -> bool {
    have.filter(|h| is_sha(h)).is_some_and(|h| h == full || git(src, &["merge-base", "--is-ancestor", full, h]).is_ok())
}

/// Sends `version` (a commit in `src`) to `host` and starts its tests there. An error means the
/// host cannot do it now and says why; the caller runs the tests itself.
pub fn start(hub: &Arc<Hub>, host: &str, version: &str, src: &Path, progress: &dyn Fn(String)) -> Result<Session> {
    let handle = tokio::runtime::Handle::current();
    let remotes = &hub.remotes;
    progress(format!("Asks {host} to run the tests"));
    let mut status: Status = handle.block_on(remotes.get_from(host, "/api/checks")).with_context(|| format!("{host} cannot run the tests"))?;
    if !status.ready && status.missing.len() == 1 && status.missing[0] == "cargo" {
        // Rust is the one tool it can bring itself.
        progress(format!("Rust is missing on {host}; it installs it (a few minutes, once)"));
        handle.block_on(remotes.post_to::<serde_json::Value>(host, "/api/checks/prepare", &serde_json::json!({}), Duration::from_secs(900)))?;
        status = handle.block_on(remotes.get_from(host, "/api/checks")).with_context(|| format!("{host} cannot run the tests"))?;
    }
    if !status.ready {
        bail!("{host} cannot run the tests yet: {}", status.missing.join(", "));
    }
    let full = git(src, &["rev-parse", "--verify", &format!("{version}^{{commit}}")])?;
    if !already_there(src, &full, status.have.as_deref()) {
        progress(format!("Sends {version} to {host}"));
        let since = status.have.as_deref().filter(|h| is_sha(h));
        let (bundle, _) = handle.block_on(crate::remote::version_bundle_since(src, version, since))?;
        handle.block_on(remotes.post_bytes_to::<serde_json::Value>(host, "/api/checks/source", bundle, Duration::from_secs(600)))?;
    }
    let job: JobView = handle.block_on(remotes.post_to(host, "/api/checks/run", &RunAsk { version: full.clone() }, Duration::from_secs(120)))?;
    Ok(Session { host: host.to_owned(), version: job.version })
}

/// Waits for the verdict, saying how far it got.
pub fn wait(hub: &Arc<Hub>, session: &Session, limit: Duration, progress: &dyn Fn(String), stop: &AtomicBool) -> Result<Outcome> {
    let handle = tokio::runtime::Handle::current();
    let began = std::time::Instant::now();
    let mut silent = 0;
    loop {
        if stop.load(Ordering::Relaxed) {
            bail!("stopped to install the version that was ready");
        }
        match handle.block_on(hub.remotes.get_from::<Status>(&session.host, "/api/checks")) {
            Ok(status) => {
                silent = 0;
                match status.job {
                    Some(job) if job.version == session.version => match outcome_of(&session.host, &job) {
                        Some(outcome) => return Ok(outcome),
                        None => progress(format!("Tests on {}: {}", session.host, job.step.unwrap_or_else(|| "running".into()))),
                    },
                    // The update's own tests come first and take the machine's one slot.
                    Some(_) => return Ok(Outcome::Lost(format!("{} is testing another version", session.host))),
                    None => return Ok(Outcome::Lost(format!("{} no longer has the run", session.host))),
                }
            }
            Err(err) => {
                silent += 1;
                if silent >= 36 {
                    return Ok(Outcome::Lost(format!("{} did not answer for three minutes: {err:#}", session.host)));
                }
            }
        }
        if began.elapsed() > limit {
            return Ok(Outcome::Lost(format!("{} was still testing after {} minutes", session.host, limit.as_secs() / 60)));
        }
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// What a run's state says, as `host` reports it; `None` while it runs. Only a failed test is the
/// commit's fault: a stop (install, another version, memory) is `Lost`, so the PC tries again.
pub(crate) fn outcome_of(host: &str, job: &JobView) -> Option<Outcome> {
    let tail = job.tail.as_deref().unwrap_or_default().trim();
    Some(match job.state {
        JobState::Running => return None,
        JobState::Passed => Outcome::Passed,
        // compat-until: 2026-12-11. An older machine ended a run it stopped for another version's
        // tests as failed, with the stop's words as the reason (#1083 went red so).
        JobState::Failed if tail == "stopped to install the version that was ready" => Outcome::Lost(STOPPED_FOR_ANOTHER.into()),
        JobState::Failed if tail.is_empty() => Outcome::Failed("no reason given".into()),
        JobState::Failed => Outcome::Failed(tail.to_owned()),
        // A newer machine says why it stopped; an older one stopped only for memory.
        JobState::Stopped if tail.starts_with("stopped ") => Outcome::Lost(tail.to_owned()),
        JobState::Stopped => Outcome::Lost(format!("{host} ran out of memory: {tail}")),
        JobState::Unknown => Outcome::Lost(format!("{host} answered with a state this version does not know")),
    })
}

/// Waits for the verdict of tests that run after the version was installed, and says it: a
/// failure or a run that could not finish becomes a problem. Does not hold up anything.
pub fn watch_after(hub: &Arc<Hub>, session: Session, limit: Duration) {
    let (hub, handle) = (hub.clone(), tokio::runtime::Handle::current());
    WATCHING.fetch_add(1, Ordering::SeqCst);
    std::thread::spawn(move || {
        struct Done;
        impl Drop for Done {
            fn drop(&mut self) {
                WATCHING.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
        let _done = Done;
        let _in = handle.enter();
        let stop = AtomicBool::new(false);
        match wait(&hub, &session, limit, &|_| {}, &stop) {
            Ok(Outcome::Passed) => tracing::info!("the tests of {} passed on {}", session.version, session.host),
            Ok(Outcome::Failed(tail)) => report(&session.version, &session.host, Level::Error, &format!("The tests of the installed version {} failed on {}", crate::text::short_version(&session.version), session.host), Some(&tail)),
            Ok(Outcome::Lost(why)) => report(&session.version, &session.host, Level::Warning, &format!("The tests of the installed version {} did not finish: {why}", crate::text::short_version(&session.version)), None),
            Err(err) => report(&session.version, &session.host, Level::Warning, &format!("The tests of the installed version {} did not finish: {err:#}", crate::text::short_version(&session.version)), None),
        }
    });
}

/// Tells about the tests of an installed version as a problem, one per kind: Problems groups
/// them, counts how often and lets a thread fix a failing one.
pub fn report(version: &str, host: &str, level: Level, message: &str, tail: Option<&str>) {
    crate::problems::capture(problem(version, host, level, message, tail));
}

/// The problem `report` hands in.
pub fn problem(version: &str, host: &str, level: Level, message: &str, tail: Option<&str>) -> Incoming {
    let kind = if level == Level::Error { "failed" } else { "not-run" };
    Incoming {
        project: "Hub".into(),
        source: "update tests".into(),
        level,
        folder: None,
        fingerprint: Some(vec!["update-tests".into(), kind.into()]),
        hit: Hit {
            ts: now_ms(),
            message: message.to_owned(),
            kind: Some("update tests".into()),
            frames: Vec::new(),
            trace: tail.map(str::to_owned),
            crumbs: Vec::new(),
            release: Some(version.to_owned()),
            environment: None,
            extra: serde_json::json!({ "version": version, "machine": host }),
        },
    }
}

/// Where a check run happens: the checked-out source, the log it writes and the build folder.
#[derive(Clone, Copy)]
pub(crate) struct CheckPaths<'a> {
    pub gate: &'a Path,
    pub log: &'a Path,
    pub target: &'a Path,
}

/// Runs `checks` in `gate` with the update's build settings into `log`: the one runner the update,
/// the merge queue and the test machine share. A step stopped for dictation or memory runs again once
/// it is quiet; `stop` ends it.
pub(crate) fn checks_in(
    cfg: &crate::config::UpdateConfig,
    at: CheckPaths<'_>,
    checks: &[crate::config::BuildStep],
    extra_env: &[(&str, String)],
    progress: &dyn Fn(String),
    stop: &AtomicBool,
) -> Result<()> {
    let CheckPaths { gate, log, target } = at;
    // The same build folder as the updater: the dependencies are built once for both.
    let mut env = vec![
        ("CARGO_TARGET_DIR", target.display().to_string()),
        ("TS_RS_EXPORT_DIR", target.join("ts-bindings").display().to_string()),
        ("CARGO_INCREMENTAL", "0".to_owned()),
        ("CARGO_PROFILE_DEV_DEBUG", "0".to_owned()),
        ("CARGO_BUILD_JOBS", update::build_jobs().to_string()),
    ];
    env.extend(extra_env.iter().map(|(k, v)| (*k, v.clone())));
    let limit = Duration::from_secs(cfg.step_timeout_mins.max(1) * 60);
    for (i, step) in checks.iter().enumerate() {
        let what = format!("step {} of {}: {}", i + 1, checks.len(), step.command.join(" "));
        progress(what.clone());
        let mut env = env.clone();
        let (mut short, mut cleared) = (0, false);
        loop {
            update::wait_for_dictation(&|| progress(format!("{what} waits while you dictate")), stop)?;
            update::wait_for_memory(&|gb| progress(format!("{what} waits for {gb} GB of free memory")), stop)?;
            let Err(err) = update::run_logged(&gate.join(&step.dir), &step.command, &env, log, limit, stop) else { break };
            if stop.load(Ordering::Relaxed) {
                return Err(err);
            }
            // The step was stopped to leave memory to dictation: the same step again once it
            // is quiet, as the updater does. It says nothing about the pull requests.
            if err.downcast_ref::<update::GaveWay>().is_some() {
                continue;
            }
            let text = format!("{err:#}");
            // Out of memory (stopped by the guard, or killed by the system) is the machine's state,
            // not the commit's fault: again once memory is back, one job at a time. A red status
            // for it sent every thread to fix a test that was fine. Still short after three tries
            // says so (`MachineShort`) instead of failing the commit.
            if err.downcast_ref::<update::MemoryShort>().is_some() || update::out_of_memory(&text) {
                short += 1;
                if short > 3 {
                    return Err(MachineShort(text).into());
                }
                progress(format!("{what} again, one job at a time, after running out of memory"));
                env.retain(|(k, _)| *k != "CARGO_BUILD_JOBS");
                env.push(("CARGO_BUILD_JOBS", "1".to_owned()));
            } else if update::broken_build_folder(&text) && !cleared {
                cleared = true;
                crate::locked::clean_dir(target.join("debug"));
            } else {
                return Err(err);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) -> String {
        let mut all = vec!["-c", "user.name=Hub", "-c", "user.email=hub@localhost"];
        all.extend_from_slice(args);
        git(dir, &all).unwrap()
    }

    fn step(command: &[&str]) -> BuildStep {
        BuildStep { dir: PathBuf::new(), command: command.iter().map(|s| s.to_string()).collect() }
    }

    /// A clone with two commits and the bundle of the second, as the PC would send it.
    fn clone_with_bundle(root: &Path) -> (PathBuf, String, String) {
        let pc = root.join("pc");
        std::fs::create_dir_all(&pc).unwrap();
        sh(&pc, &["init", "--quiet", "-b", "main"]);
        std::fs::write(pc.join("a.txt"), "one\n").unwrap();
        sh(&pc, &["add", "."]);
        sh(&pc, &["commit", "--quiet", "-m", "one"]);
        let first = sh(&pc, &["rev-parse", "HEAD"]);
        std::fs::write(pc.join("a.txt"), "two\n").unwrap();
        sh(&pc, &["commit", "--quiet", "-am", "two"]);
        let second = sh(&pc, &["rev-parse", "HEAD"]);
        (pc, first, second)
    }

    fn bundle_of(pc: &Path, commit: &str, since: Option<&str>) -> Vec<u8> {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(crate::remote::version_bundle_since(pc, commit, since)).unwrap().0
    }

    fn wait_for_end() -> JobView {
        for _ in 0..200 {
            if let Some(job) = JOB.locked().clone().filter(|j| j.state != JobState::Running) {
                return job;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the run did not end");
    }

    #[test]
    fn a_commit_is_received_as_a_difference_and_its_tests_pass_or_fail_with_the_reason() {
        // The runs share one process-wide slot, so the cases below run one after the other.
        let root = crate::janitor::test_dir();
        let (pc, first, second) = clone_with_bundle(&root);
        let data = root.join("tincan");
        std::fs::create_dir_all(&data).unwrap();
        // The test does not build anything, so it needs no room.
        let cfg = UpdateConfig { min_free_gb: 0, ..Default::default() };

        // The whole history first, then only what is new.
        assert_eq!(receive_in(&data, &bundle_of(&pc, &first, None)).unwrap(), first);
        assert_eq!(status_in(&data, &cfg).have.as_deref(), Some(first.as_str()));
        let next = bundle_of(&pc, &second, Some(&first));
        assert!(next.len() < bundle_of(&pc, &second, None).len() + 1);
        assert_eq!(receive_in(&data, &next).unwrap(), second);
        assert_eq!(status_in(&data, &cfg).have.as_deref(), Some(second.as_str()));

        // A passing step passes.
        let job = start_in(&data, &cfg, &second, vec![step(&["git", "status"])], Vec::new()).unwrap();
        assert_eq!(job.state, JobState::Running);
        let done = wait_for_end();
        assert_eq!(done.state, JobState::Passed, "{:?}", done.tail);
        // Asked again for the same version, the answer is the run that was.
        assert_eq!(start_in(&data, &cfg, &second, vec![step(&["git", "status"])], Vec::new()).unwrap().state, JobState::Passed);

        // A failing step fails and says which command.
        start_in(&data, &cfg, &first, vec![step(&["git", "no-such-command"])], Vec::new()).unwrap();
        let failed = wait_for_end();
        assert_eq!(failed.state, JobState::Failed);
        assert!(failed.tail.unwrap().contains("no-such-command"));
        assert!(failed.finished_at.is_some());

        // A run stopped to install a ready version ends `stopped` and says so, never `failed`.
        let hang = if cfg!(windows) { step(&["cmd", "/C", "ping -n 30 127.0.0.1 >NUL"]) } else { step(&["sh", "-c", "sleep 30"]) };
        start_in(&data, &cfg, &first, vec![hang], Vec::new()).unwrap();
        halt(STOPPED_FOR_INSTALL);
        let stopped = wait_for_end();
        assert_eq!((stopped.state, stopped.tail.as_deref()), (JobState::Stopped, Some(STOPPED_FOR_INSTALL)));

        // A commit that was never sent is refused, not run.
        assert!(start_in(&data, &cfg, "0123456789abcdef0123456789abcdef01234567", vec![step(&["git", "status"])], Vec::new()).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_step_that_keeps_running_out_of_memory_stops_the_run_without_failing_the_commit() {
        let root = crate::janitor::test_dir();
        let log = root.join("log");
        let cfg = UpdateConfig::default();
        let tries = root.join("tries");
        let script = format!("echo x >> {}; echo 'memory allocation of 8 bytes failed'; exit 1", tries.display());
        let err = checks_in(&cfg, CheckPaths { gate: &root, log: &log, target: &root.join("t") }, &[step(&["sh", "-c", &script])], &[], &|_| {}, &AtomicBool::new(false)).unwrap_err();
        assert!(err.downcast_ref::<MachineShort>().is_some(), "{err:#}");
        // The first run and three more.
        assert_eq!(std::fs::read_to_string(&tries).unwrap().lines().count(), 4);
        // A plain failure is still the commit's, and is not tried again.
        let err = checks_in(&cfg, CheckPaths { gate: &root, log: &log, target: &root.join("t") }, &[step(&["sh", "-c", "echo broken; exit 1"])], &[], &|_| {}, &AtomicBool::new(false)).unwrap_err();
        assert!(err.downcast_ref::<MachineShort>().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_the_test_steps_move_to_the_other_machine() {
        let all = UpdateConfig::default_steps();
        let kept: Vec<_> = all.iter().filter(|s| !is_check_step(s)).map(|s| s.command.join(" ")).collect();
        // The packages and the two builds stay; the check and the two kinds of tests go.
        assert_eq!(kept.len(), all.len() - 3, "{kept:?}");
        assert!(kept.iter().all(|c| !c.contains("cargo test") && !c.contains("run check") && !c.ends_with("npm test") && !c.ends_with("npm.cmd test")));
        assert_eq!(steps().len(), 4);
        assert!(steps().iter().filter(|s| is_check_step(s)).count() == 3);
    }

    fn job(state: JobState, started_at: i64, tail: Option<&str>) -> JobView {
        JobView { version: "abc".into(), state, step: None, started_at, finished_at: None, tail: tail.map(str::to_owned) }
    }

    #[test]
    fn a_running_test_run_holds_the_install_until_it_ends_or_runs_too_long() {
        let limit = RUN_LIMIT.as_millis() as i64;
        let now = 10 * limit;
        // A run that is going holds it; nothing running, or a run that ended, does not.
        assert!(holds_install(Some(&job(JobState::Running, now - 60_000, None)), now, now));
        assert!(!holds_install(None, now, now));
        for ended in [JobState::Passed, JobState::Failed, JobState::Stopped] {
            assert!(!holds_install(Some(&job(ended, now - 60_000, None)), now, now));
        }
        // A run older than the limit, or a wait that has lasted the limit, no longer holds it.
        assert!(!holds_install(Some(&job(JobState::Running, now - limit, None)), now, now));
        assert!(!holds_install(Some(&job(JobState::Running, now - 1, None)), now - limit, now));
    }

    #[test]
    fn a_stopped_run_is_not_the_commits_failure() {
        let lost = |j: &JobView| match outcome_of("tincan", j) {
            Some(Outcome::Lost(why)) => why,
            other => panic!("{other:?}"),
        };
        assert_eq!(lost(&job(JobState::Stopped, 1, Some(STOPPED_FOR_INSTALL))), "stopped to install the update");
        assert_eq!(lost(&job(JobState::Stopped, 1, Some(STOPPED_FOR_ANOTHER))), STOPPED_FOR_ANOTHER);
        // An older machine stopped only for memory, and its tail says how.
        assert!(lost(&job(JobState::Stopped, 1, Some("`cargo test` was stopped because the computer was running out of memory"))).starts_with("tincan ran out of memory: "));
        // The tail an older machine gave a run it stopped for another version (#1083, head 2e6491aa).
        assert_eq!(lost(&job(JobState::Failed, 1, Some("stopped to install the version that was ready"))), STOPPED_FOR_ANOTHER);
        // A failed test stays the commit's; a run that goes on says nothing yet.
        assert_eq!(outcome_of("tincan", &job(JobState::Failed, 1, Some("`cargo test` failed"))), Some(Outcome::Failed("`cargo test` failed".into())));
        assert_eq!(outcome_of("tincan", &job(JobState::Running, 1, None)), None);
        assert_eq!(outcome_of("tincan", &job(JobState::Passed, 1, None)), Some(Outcome::Passed));
    }

    #[test]
    fn a_state_a_newer_hubd_has_reads_as_unknown() {
        let job: JobView = serde_json::from_str(r#"{"version":"abc","state":"queued","started_at":1}"#).unwrap();
        assert_eq!(job.state, JobState::Unknown);
        let stopped: JobView = serde_json::from_str(r#"{"version":"abc","state":"stopped","started_at":1}"#).unwrap();
        assert_eq!(stopped.state, JobState::Stopped);
    }

    #[test]
    fn already_there_says_which_versions_the_host_has() {
        let root = crate::janitor::test_dir();
        let (pc, first, second) = clone_with_bundle(&root);
        // The host's latest is `second`: `second` and its ancestor `first` are there, so nothing goes.
        assert!(already_there(&pc, &second, Some(&second)), "the tip itself");
        assert!(already_there(&pc, &first, Some(&second)), "an ancestor of the tip");
        // A newer commit is not: it goes as the difference, which is not empty.
        assert!(!already_there(&pc, &second, Some(&first)), "a newer version");
        assert!(!bundle_of(&pc, &second, Some(&first)).is_empty());
        // The host's latest unknown to this clone, or no latest: send it whole.
        assert!(!already_there(&pc, &first, Some("0123456789abcdef0123456789abcdef01234567")));
        assert!(!already_there(&pc, &first, None));
        // The empty range that used to fail the send: an ancestor against the tip that has it.
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert!(rt.block_on(crate::remote::version_bundle_since(&pc, &first, Some(&second))).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
