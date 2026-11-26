//! The update manager: which version of hubd and of each agent CLI (Claude Code, Codex, Hermes,
//! OpenCode, Gemini CLI) every machine runs, and updating them everywhere at once.
//!
//! Each hubd answers for itself (`GET /api/clis`) and updates its own CLIs
//! (`POST /api/clis/update`), with the package manager that owns each one: an npm install is
//! updated with npm, anything else with the CLI's own update command. Which CLIs there are, their
//! npm package and their update command is the harness catalog in `engines.rs`, data only.
//!
//! The machine you use (the PC) puts the machines together (`GET /api/updates`), reads the newest
//! version of each npm package once an hour, and "Install everywhere" updates a CLI, or hubd, on
//! every machine online (or on one, when asked). `news` reads a CLI's own release notes, as the
//! harness catalog's `changelog` names them, for Settings > Updates. With follow on (the default), a CLI that got newer here, by that button
//! or by updating itself, is brought to the same version on the others; hubd already follows this
//! machine's version (`remote.rs`).

use crate::locked::LockExt;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::engines::{Harness, harnesses};
use crate::hub::Hub;
use crate::model::{HostState, UpdateStatus, UpdateTrack, now_ms};
use crate::text::parse_version;

/// Store key: "no" when other machines should not follow this one's CLI versions.
const FOLLOW_KEY: &str = "updates.follow";
const UPDATE_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const KEEP_LINES: usize = 40;
/// How often the newest versions are read from npm.
const LATEST_EVERY: Duration = Duration::from_secs(3600);

/// One agent CLI on one machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Tool {
    /// The harness id, e.g. `claude`.
    pub id: String,
    pub label: String,
    pub installed: bool,
    /// What `--version` says, as a version number.
    pub version: Option<String>,
    /// The newest version, when Hub can read it (npm packages); filled in by the machine you use.
    #[serde(default)]
    pub latest: Option<String>,
    /// The command that updates it there.
    pub update: Option<String>,
    /// The update running or last run there since its core started.
    pub job: Option<ToolJob>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolJob {
    pub running: bool,
    /// Done and the version moved or stayed newest (true), or it failed (false). None while it runs.
    pub ok: Option<bool>,
    /// One line on how it went, e.g. "2.1.3 → 2.1.5".
    pub message: String,
    pub lines: Vec<String>,
    #[ts(type = "number")]
    pub started: i64,
}

/// What one hubd answers about its own CLIs.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolsHere {
    pub machine: String,
    pub tools: Vec<Tool>,
}

/// One machine in Settings > Updates.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MachineUpdates {
    pub machine: String,
    pub local: bool,
    pub online: bool,
    /// Its hubd version.
    pub hubd: Option<String>,
    /// Its own updater: building, ready, failed and why.
    pub hubd_state: Option<UpdateStatus>,
    /// What this machine did last to bring its hubd up to date, or why it could not.
    pub hubd_note: Option<String>,
    /// The update this machine sent it, step by step (feature `update.look`). Absent when none
    /// was sent, and from a version of Hub before it.
    #[serde(default)]
    #[ts(optional)]
    pub hubd_track: Option<UpdateTrack>,
    pub tools: Vec<Tool>,
    /// Why its CLIs are not listed, e.g. offline or a hubd from before the update manager.
    pub error: Option<String>,
}

/// Every machine's versions, as Settings > Updates shows them.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Fleet {
    /// The hubd version the other machines are brought to: this machine's.
    pub hubd: Option<String>,
    /// Other machines follow this one's CLI versions.
    pub follow: bool,
    pub machines: Vec<MachineUpdates>,
    #[ts(type = "number")]
    pub at: i64,
}

static JOBS: LazyLock<Mutex<BTreeMap<String, ToolJob>>> = LazyLock::new(Default::default);
/// A failed ask to another machine, by (machine, tool), shown in that cell until it answers.
static ASKS: LazyLock<Mutex<BTreeMap<(String, String), ToolJob>>> = LazyLock::new(Default::default);
static LATEST: LazyLock<Mutex<(Option<Instant>, BTreeMap<String, String>)>> = LazyLock::new(Default::default);
/// The last fleet read, so the page opens at once.
static LAST: LazyLock<Mutex<Option<Fleet>>> = LazyLock::new(Default::default);
/// (machine, tool, version) already asked to follow, so a machine that cannot update is not asked
/// again every round.
static FOLLOWED: LazyLock<Mutex<BTreeSet<(String, String, String)>>> = LazyLock::new(Default::default);
static FOLLOW_NOW: tokio::sync::Notify = tokio::sync::Notify::const_new();

// ---- This machine ----

/// The CLIs Hub knows, as they are on this machine.
pub async fn here() -> Vec<Tool> {
    let mut out = Vec::new();
    for h in harnesses() {
        let Some(cli) = h.cli.clone() else { continue };
        let found = {
            let cli = cli.clone();
            tokio::task::spawn_blocking(move || crate::engines::locate(&cli)).await.ok().flatten()
        };
        let version = match &found {
            Some(path) => version_of(path).await,
            None => None,
        };
        out.push(Tool {
            id: h.id.clone(),
            label: h.label.clone(),
            installed: found.is_some(),
            version,
            latest: None,
            update: found.as_deref().and_then(|p| update_command(&h, p, cfg!(windows))),
            job: JOBS.locked().get(&h.id).cloned(),
        });
    }
    out
}

/// The version a program says it is, from `--version`.
async fn version_of(path: &Path) -> Option<String> {
    let mut cmd = crate::proc::tokio_command(path);
    cmd.arg("--version").stdin(std::process::Stdio::null()).kill_on_drop(true);
    cmd.env("NO_COLOR", "1").env("CI", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.as_std_mut().creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = tokio::time::timeout(Duration::from_secs(20), cmd.output()).await.ok()?.ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout)).or_else(|| parse_version(&String::from_utf8_lossy(&out.stderr)))
}

/// Compares two version numbers by their numeric parts: `0.10.2` is newer than `0.9.7`. A
/// pre-release (`1.2.0-beta`) is older than the release.
pub fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |v: &str| {
        let (main, pre) = v.split_once('-').map_or((v, None), |(m, p)| (m, Some(p.to_owned())));
        (main.split('.').map(|n| n.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>(), pre)
    };
    let ((mut na, pa), (mut nb, pb)) = (parts(a), parts(b));
    let len = na.len().max(nb.len());
    na.resize(len, 0);
    nb.resize(len, 0);
    na.cmp(&nb).then_with(|| match (pa, pb) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(x), Some(y)) => x.cmp(&y),
    })
}

/// How to update a CLI found at `path`, with the package manager that owns it: npm for an npm
/// install, Homebrew for a Homebrew one, else its own update command, else npm.
pub fn update_command(h: &Harness, path: &Path, windows: bool) -> Option<String> {
    let p = path.to_string_lossy().to_lowercase().replace('\\', "/");
    let npm = ["/node_modules/", "/npm/", "/.npm-global/", "/nvm/", "/fnm/", "/volta/", "/pnpm/"].iter().any(|w| p.contains(w));
    let brew = p.contains("/homebrew/") || p.contains("/linuxbrew/") || p.contains("/cellar/");
    let program = if windows { format!("& '{}'", path.display()) } else { format!("'{}'", path.display()) };
    match (&h.package, &h.self_update) {
        (Some(pkg), _) if npm => Some(format!("npm install -g {pkg}@latest")),
        (_, Some(own)) => Some(format!("{program} {own}")),
        _ if brew => h.cli.as_ref().map(|c| format!("brew upgrade {c}")),
        (Some(pkg), None) => Some(format!("npm install -g {pkg}@latest")),
        (None, None) => None,
    }
}

/// Starts updating one CLI on this machine, in the background.
pub async fn start_update(tool: &str) -> Result<ToolJob> {
    let h = harnesses().into_iter().find(|h| h.id == tool).with_context(|| format!("Hub knows no tool {tool:?}"))?;
    let cli = h.cli.clone().with_context(|| format!("{} has no program to update", h.label))?;
    let path = {
        let cli = cli.clone();
        tokio::task::spawn_blocking(move || crate::engines::locate(&cli)).await?
    }
    .with_context(|| format!("{} is not installed here", h.label))?;
    let command = update_command(&h, &path, cfg!(windows)).with_context(|| format!("Hub doesn't know how to update {}", h.label))?;
    let before = version_of(&path).await;
    let job = {
        let mut jobs = JOBS.locked();
        if jobs.get(&h.id).is_some_and(|j| j.running) {
            return Ok(jobs[&h.id].clone());
        }
        let job = ToolJob { running: true, ok: None, message: format!("Updating {}…", h.label), lines: vec![format!("$ {command}")], started: now_ms() };
        jobs.insert(h.id.clone(), job.clone());
        job
    };
    tokio::spawn(async move {
        let id = h.id.clone();
        let sink = move |line: String| {
            if let Some(j) = JOBS.locked().get_mut(&id) {
                j.lines.push(line);
                let extra = j.lines.len().saturating_sub(KEEP_LINES);
                j.lines.drain(..extra);
            }
        };
        let mut ran = tokio::time::timeout(UPDATE_TIMEOUT, crate::engines::run_shell(&command, sink.clone())).await;
        // npm said no: read why, and when Hub can get round it (a cache root owns, a global folder
        // sudo opens without a password), try once more that way.
        let mut failure = None;
        if matches!(ran, Ok(Err(_))) {
            let lines = JOBS.locked().get(&h.id).map(|j| j.lines.clone()).unwrap_or_default();
            failure = npm_failure(&lines);
            if let Some(again) = failure.as_ref().and_then(|f| retry_command(&command, f)).filter(|_| !cfg!(windows)) {
                let again = match again {
                    Retry::Run(c) => Some(c),
                    Retry::Sudo(c) => sudo_without_password().await.then_some(c),
                };
                if let Some(again) = again {
                    sink(format!("$ {again}"));
                    ran = tokio::time::timeout(UPDATE_TIMEOUT, crate::engines::run_shell(&again, sink)).await;
                    if matches!(ran, Ok(Err(_))) {
                        let lines = JOBS.locked().get(&h.id).map(|j| j.lines.clone()).unwrap_or_default();
                        failure = npm_failure(&lines).or(failure);
                    }
                }
            }
        }
        crate::engines::forget_lookup();
        let after = match crate::engines::locate(&cli) {
            Some(p) => version_of(&p).await,
            None => None,
        };
        let (ok, message) = match ran {
            Err(_) => (false, format!("The update was still going after {} minutes and was stopped.", UPDATE_TIMEOUT.as_secs() / 60)),
            Ok(Err(e)) => match &failure {
                Some(f) => (false, f.explain(&command)),
                None => (false, format!("The update failed: {e}")),
            },
            Ok(Ok(())) => match (&before, &after) {
                (Some(b), Some(a)) if b != a => (true, format!("{b} → {a}")),
                (_, Some(a)) => (true, format!("{a}, the newest it found")),
                (_, None) => (false, format!("The update ran, but {} no longer answers --version", h.label)),
            },
        };
        if let Some(j) = JOBS.locked().get_mut(&h.id) {
            j.running = false;
            j.ok = Some(ok);
            j.message = message;
        }
        // A newer version here is one the others follow.
        if ok {
            FOLLOW_NOW.notify_one();
        }
    });
    Ok(job)
}

/// Why npm stopped, read from what it printed: its error code and the path it was refused.
#[derive(Debug, Clone, PartialEq)]
pub struct NpmFailure {
    /// `EACCES`, `ENOTEMPTY`, `E404`, …
    pub code: String,
    pub path: Option<String>,
    /// npm's own one-line summary (`Error: EACCES: permission denied, mkdir '…'`).
    pub summary: Option<String>,
}

impl NpmFailure {
    fn denied(&self) -> bool {
        matches!(self.code.as_str(), "EACCES" | "EPERM")
    }

    /// npm's cache (`~/.npm`), which a past `sudo npm` can leave owned by root.
    fn in_cache(&self) -> bool {
        self.path.as_deref().is_some_and(|p| p.replace('\\', "/").contains("/.npm/") || p.replace('\\', "/").ends_with("/.npm"))
    }

    /// What to say on the machine's row: the real reason and what fixes it.
    pub fn explain(&self, command: &str) -> String {
        let path = self.path.as_deref().unwrap_or("its folder");
        match (self.denied(), self.in_cache()) {
            (true, true) => format!(
                "npm may not write to its cache ({path}): files there belong to root, left by an earlier sudo npm. Fix once on that machine: sudo chown -R $(id -un) ~/.npm"
            ),
            (true, false) => format!(
                "npm may not write to {path}: the global npm folder belongs to root and sudo asks for a password. Run once on that machine: sudo {command}"
            ),
            _ => match &self.summary {
                Some(s) => format!("npm stopped with {}: {s}", self.code),
                None => format!("npm stopped with {}", self.code),
            },
        }
    }
}

/// The error npm printed, from an update's output (`npm ERR! code EACCES`, or `npm error code
/// EACCES` from npm 10 on). None when the output holds no npm error.
pub fn npm_failure(lines: &[String]) -> Option<NpmFailure> {
    let field = |name: &str| {
        lines.iter().rev().find_map(|l| {
            let rest = l.trim().strip_prefix("npm ERR!").or_else(|| l.trim().strip_prefix("npm error"))?.trim();
            rest.strip_prefix(name).filter(|v| v.starts_with(' ')).map(|v| v.trim().to_owned())
        })
    };
    let code = field("code")?;
    let summary = lines.iter().find_map(|l| {
        let rest = l.trim().strip_prefix("npm ERR!").or_else(|| l.trim().strip_prefix("npm error"))?.trim();
        rest.strip_prefix("Error: ").map(str::to_owned).or_else(|| rest.starts_with(&format!("{code}:")).then(|| rest.to_owned()))
    });
    Some(NpmFailure { path: field("path"), code, summary })
}

/// A second way to run a failed npm update.
#[derive(Debug, PartialEq)]
pub enum Retry {
    /// Run this instead.
    Run(String),
    /// Run this if sudo needs no password here.
    Sudo(String),
}

/// How to get round `failure` for the npm `command`: a cache root owns is swapped for Hub's own,
/// a global folder root owns is tried with sudo. None for anything else.
pub fn retry_command(command: &str, failure: &NpmFailure) -> Option<Retry> {
    if !command.starts_with("npm ") || !failure.denied() {
        return None;
    }
    if failure.in_cache() {
        let cache = crate::folders::home().join(".cache").join("hub").join("npm");
        return Some(Retry::Run(format!("{command} --cache '{}'", cache.display())));
    }
    Some(Retry::Sudo(format!("sudo -n {command}")))
}

/// Whether sudo runs here without asking for a password.
async fn sudo_without_password() -> bool {
    let mut cmd = crate::proc::tokio_command("sudo");
    cmd.args(["-n", "true"]).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    matches!(tokio::time::timeout(Duration::from_secs(10), cmd.status()).await, Ok(Ok(s)) if s.success())
}

// ---- All machines ----

/// The newest version of each npm package, read at most once an hour.
async fn latest(client: &reqwest::Client) -> BTreeMap<String, String> {
    {
        let cached = LATEST.locked();
        if cached.0.is_some_and(|at| at.elapsed() < LATEST_EVERY) {
            return cached.1.clone();
        }
    }
    let mut found = BTreeMap::new();
    for h in harnesses() {
        let Some(pkg) = h.package else { continue };
        let url = format!("https://registry.npmjs.org/{pkg}/latest");
        let answer = client.get(&url).timeout(Duration::from_secs(15)).send().await;
        if let Ok(answer) = answer
            && answer.status().is_success()
            && let Ok(body) = answer.json::<serde_json::Value>().await
            && let Some(v) = body["version"].as_str()
        {
            found.insert(h.id, v.to_owned());
        }
    }
    let mut cached = LATEST.locked();
    // A failed read keeps what was known and tries again in ten minutes.
    if found.is_empty() {
        cached.0 = Some(Instant::now() - LATEST_EVERY + Duration::from_secs(600));
    } else {
        *cached = (Some(Instant::now()), found);
    }
    cached.1.clone()
}

/// One release of an agent CLI, from its own release notes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CliRelease {
    pub version: String,
    /// Markdown, as the CLI's makers wrote it.
    pub text: String,
}

/// What changed in an agent CLI between two versions, for Settings > Updates.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CliNews {
    pub tool: String,
    /// Newest first, at most `NEWS_MAX`.
    pub releases: Vec<CliRelease>,
    /// The page with all its release notes, to open in a browser.
    pub url: Option<String>,
    /// Why there is nothing: no notes known for it, or they could not be read.
    pub error: Option<String>,
}

const NEWS_MAX: usize = 12;
/// Each CLI's release notes as last read, by tool id; read again after `LATEST_EVERY`.
static NOTES: LazyLock<Mutex<BTreeMap<String, (Instant, Vec<CliRelease>)>>> = LazyLock::new(Default::default);

/// The releases of `tool` newer than `from` and up to `to`, newest first.
pub async fn news(tool: &str, from: Option<&str>, to: Option<&str>) -> CliNews {
    let source = harnesses().into_iter().find(|h| h.id == tool).and_then(|h| h.changelog);
    let mut out = CliNews { tool: tool.to_owned(), releases: Vec::new(), url: source.as_deref().map(notes_page), error: None };
    let Some(source) = source else {
        out.error = Some("Hub knows no release notes for it".into());
        return out;
    };
    let cached = NOTES.locked().get(tool).filter(|(at, _)| at.elapsed() < LATEST_EVERY).map(|(_, r)| r.clone());
    let all = match cached {
        Some(all) => all,
        None => match read_notes(&source).await {
            Ok(all) => {
                NOTES.locked().insert(tool.to_owned(), (Instant::now(), all.clone()));
                all
            }
            Err(e) => {
                out.error = Some(format!("could not read its release notes: {e:#}"));
                return out;
            }
        },
    };
    out.releases = between(all, from, to);
    out
}

/// Releases newer than `from` and not newer than `to`, newest first, at most `NEWS_MAX`.
pub fn between(mut all: Vec<CliRelease>, from: Option<&str>, to: Option<&str>) -> Vec<CliRelease> {
    all.retain(|r| from.is_none_or(|f| compare(&r.version, f).is_gt()) && to.is_none_or(|t| compare(&r.version, t).is_le()));
    all.sort_by(|a, b| compare(&b.version, &a.version));
    all.dedup_by(|a, b| a.version == b.version);
    all.truncate(NEWS_MAX);
    all
}

/// Where a person reads the notes: the file on GitHub rather than its raw copy, or the releases.
fn notes_page(source: &str) -> String {
    if let Some(repo) = source.strip_prefix("github:") {
        return format!("https://github.com/{repo}/releases");
    }
    let url = source.strip_prefix("md:").unwrap_or(source);
    match url.strip_prefix("https://raw.githubusercontent.com/") {
        Some(rest) => {
            let mut parts = rest.splitn(4, '/');
            match (parts.next(), parts.next(), parts.next(), parts.next()) {
                (Some(owner), Some(repo), Some(branch), Some(path)) => format!("https://github.com/{owner}/{repo}/blob/{branch}/{path}"),
                _ => url.to_owned(),
            }
        }
        None => url.to_owned(),
    }
}

async fn read_notes(source: &str) -> Result<Vec<CliRelease>> {
    let client = &*NPM;
    if let Some(repo) = source.strip_prefix("github:") {
        let url = format!("https://api.github.com/repos/{repo}/releases?per_page=40");
        let answer = client.get(&url).header("User-Agent", "hub").header("Accept", "application/vnd.github+json").timeout(Duration::from_secs(20)).send().await?;
        if !answer.status().is_success() {
            bail!("GitHub answered {}", answer.status());
        }
        return Ok(github_releases(&answer.json::<serde_json::Value>().await?));
    }
    let url = source.strip_prefix("md:").unwrap_or(source);
    let answer = client.get(url).header("User-Agent", "hub").timeout(Duration::from_secs(20)).send().await?;
    if !answer.status().is_success() {
        bail!("{url} answered {}", answer.status());
    }
    Ok(markdown_releases(&answer.text().await?))
}

/// A changelog file with one `## <version>` heading per release.
pub fn markdown_releases(text: &str) -> Vec<CliRelease> {
    let mut out: Vec<CliRelease> = Vec::new();
    let mut open = false;
    for line in text.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            open = match parse_version(head) {
                Some(version) => {
                    out.push(CliRelease { version, text: String::new() });
                    true
                }
                None => false,
            };
        } else if line.starts_with("# ") {
            open = false;
        } else if open && let Some(last) = out.last_mut() {
            last.text.push_str(line);
            last.text.push('\n');
        }
    }
    for r in &mut out {
        r.text = r.text.trim().to_owned();
    }
    out
}

/// GitHub's list of releases, without drafts and pre-releases.
pub fn github_releases(list: &serde_json::Value) -> Vec<CliRelease> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false) && !r["prerelease"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            let version = parse_version(r["tag_name"].as_str()?)?;
            Some(CliRelease { version, text: r["body"].as_str().unwrap_or_default().trim().to_owned() })
        })
        .collect()
}

static NPM: LazyLock<reqwest::Client> = LazyLock::new(|| reqwest::Client::builder().build().expect("building the HTTP client"));

pub async fn follow_on(hub: &Hub) -> bool {
    hub.store.get_value(FOLLOW_KEY).await.ok().flatten().as_deref() != Some("no")
}

pub async fn set_follow(hub: &Hub, on: bool) -> Result<()> {
    hub.store.set_value(FOLLOW_KEY, if on { "yes" } else { "no" }).await?;
    if on {
        FOLLOW_NOW.notify_one();
    }
    Ok(())
}

/// The last fleet read, or a fresh one when there is none yet.
pub async fn cached(hub: &Hub) -> Fleet {
    let last = LAST.locked().clone();
    match last {
        Some(f) => f,
        None => fleet(hub).await,
    }
}

/// Every machine's hubd and CLI versions, read now.
pub async fn fleet(hub: &Hub) -> Fleet {
    let remotes = &hub.remotes;
    let target = remotes.ship_version().await.ok();
    let newest = latest(&NPM).await;
    let mut machines = vec![MachineUpdates {
        machine: remotes.local_name().to_owned(),
        local: true,
        online: true,
        hubd: Some(hub_install::running_version()),
        hubd_state: Some(hub.update_status()),
        hubd_note: None,
        hubd_track: None,
        tools: here().await,
        error: None,
    }];
    for host in remotes.list().into_iter().filter(|h| !h.local) {
        let online = host.state == HostState::Online;
        let mut m = MachineUpdates {
            machine: host.name.clone(),
            local: false,
            online,
            hubd: host.version.clone(),
            hubd_state: None,
            hubd_note: remotes.hubd_note(&host.name),
            hubd_track: None,
            tools: Vec::new(),
            error: None,
        };
        if !online {
            m.error = Some("offline; it is brought up to date when it is back".into());
        } else {
            m.hubd_state = remotes.get_from::<UpdateStatus>(&host.name, "/api/version").await.ok();
            remotes.observe(&host.name, true, m.hubd_state.as_ref(), host.version.as_deref());
            match remotes.get_from::<ToolsHere>(&host.name, "/api/clis").await {
                Ok(t) => m.tools = t.tools,
                Err(e) if crate::remote::too_old(&e) => {
                    remotes.wake_push();
                    m.error = Some("its hubd is from before the update manager, so it can't list its CLIs; Hub is updating its hubd first".into());
                }
                Err(e) => m.error = Some(format!("{e:#}")),
            }
        }
        if !online {
            remotes.observe(&host.name, false, None, host.version.as_deref());
        }
        m.hubd_track = remotes.track(&host.name);
        machines.push(m);
    }
    // The newest each CLI could be: what npm says, or the newest any machine runs.
    for m in &mut machines {
        for t in &mut m.tools {
            t.latest = newest.get(&t.id).cloned();
            if let Some(ask) = ASKS.locked().get(&(m.machine.clone(), t.id.clone()))
                && t.job.is_none()
            {
                t.job = Some(ask.clone());
            }
        }
    }
    let fleet = Fleet { hubd: target, follow: follow_on(hub).await, machines, at: now_ms() };
    *LAST.locked() = Some(fleet.clone());
    fleet
}

/// Updates one CLI, or hubd (`tool` = "hubd"), on every machine online that has it, or only on
/// `only` when it names one.
pub async fn everywhere(hub: &Arc<Hub>, tool: &str, only: Option<&str>) -> Result<Fleet> {
    let mut fleet = fleet(hub).await;
    if let Some(name) = only {
        if !fleet.machines.iter().any(|m| m.machine == name) {
            bail!("no machine named {name:?}");
        }
        fleet.machines.retain(|m| m.machine == name);
    }
    if tool == "hubd" {
        let Some(target) = fleet.hubd.clone() else { bail!("this machine can't say which hubd version to send") };
        let mut jobs = Vec::new();
        for m in fleet.machines.iter().filter(|m| m.online) {
            let ready = m.hubd_state.as_ref().is_some_and(|s| s.state == crate::model::UpdateState::Ready);
            if m.local {
                // Built and waiting for your yes: this is it.
                if ready {
                    hub.approve_install()?;
                }
                continue;
            }
            if m.hubd.as_deref() == Some(target.as_str()) {
                continue;
            }
            let ready_target = m.hubd_state.as_ref().and_then(|s| s.target.as_deref()) == Some(target.as_str());
            let (remotes, machine) = (hub.remotes.clone(), m.machine.clone());
            jobs.push(async move {
                let done = if ready && ready_target {
                    remotes.approve_install(&machine).await.map(|_| None)
                } else {
                    remotes.update(&machine).await.map(|answer| answer.sudo.map(|_| "its sudo password").or(answer.build.map(|_| "whether this PC builds hubd for it")))
                };
                match done {
                    Ok(None) => {}
                    // Update Center cannot ask these; the machine's cell says where they are asked.
                    Ok(Some(ask)) => remotes.say(&machine, format!("not started: it needs {ask} first. Answer in Settings > Machines, then press Update again")),
                    Err(e) => {
                        tracing::warn!("updating hubd on {machine}: {e:#}");
                        remotes.say(&machine, format!("could not start the update: {e:#}"));
                    }
                }
            });
        }
        // Side by side: one machine that answers slowly (SSH resetting, a Mac asleep) must not
        // keep the others waiting behind it, as it did when they were asked one after the other.
        futures::future::join_all(jobs).await;
        return Ok(self::fleet(hub).await);
    }
    if !harnesses().iter().any(|h| h.id == tool && h.cli.is_some()) {
        bail!("Hub knows no tool {tool:?}");
    }
    for m in fleet.machines.iter().filter(|m| m.online) {
        if !m.tools.iter().any(|t| t.id == tool && t.installed) {
            continue;
        }
        ask(hub, &m.machine, m.local, tool).await;
    }
    // Give the jobs a moment to start, so the page shows them running.
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(self::fleet(hub).await)
}

/// Starts updating `tool` on one machine; a failure to ask shows in that machine's cell.
async fn ask(hub: &Hub, machine: &str, local: bool, tool: &str) {
    let asked = if local {
        start_update(tool).await.map(|_| ())
    } else {
        hub.remotes
            .post_to::<ToolJob>(machine, "/api/clis/update", &serde_json::json!({ "tool": tool }), Duration::from_secs(60))
            .await
            .map(|_| ())
    };
    let key = (machine.to_owned(), tool.to_owned());
    match asked {
        Ok(()) => {
            ASKS.locked().remove(&key);
        }
        Err(e) => {
            ASKS.locked().insert(key, ToolJob { running: false, ok: Some(false), message: format!("{e:#}"), lines: Vec::new(), started: now_ms() });
        }
    }
}

/// The machines whose `tool` is older than here, to bring along: (machine, tool, version here).
pub fn behind(fleet: &Fleet) -> Vec<(String, String, String)> {
    let Some(local) = fleet.machines.iter().find(|m| m.local) else { return Vec::new() };
    let mut out = Vec::new();
    for mine in local.tools.iter().filter(|t| t.installed) {
        let Some(v) = &mine.version else { continue };
        for m in fleet.machines.iter().filter(|m| !m.local && m.online) {
            let Some(theirs) = m.tools.iter().find(|t| t.id == mine.id && t.installed) else { continue };
            let running = theirs.job.as_ref().is_some_and(|j| j.running);
            let older = theirs.version.as_deref().is_some_and(|tv| compare(tv, v).is_lt());
            if older && !running && theirs.update.is_some() {
                out.push((m.machine.clone(), mine.id.clone(), v.clone()));
            }
        }
    }
    out
}

/// Keeps the list fresh and, with follow on, the other machines' CLIs on this machine's versions:
/// every fifteen minutes, and soon after an update here finishes.
pub async fn follow(hub: Arc<Hub>) {
    tokio::time::sleep(Duration::from_secs(120)).await;
    loop {
        let fleet = fleet(&hub).await;
        if fleet.follow {
            for (machine, tool, version) in behind(&fleet) {
                if !FOLLOWED.locked().insert((machine.clone(), tool.clone(), version.clone())) {
                    continue;
                }
                tracing::info!("updates: bringing {tool} on {machine} to {version}");
                ask(&hub, &machine, false, &tool).await;
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(15 * 60)) => {}
            // An update here: let npm settle a moment, then look.
            _ = FOLLOW_NOW.notified() => tokio::time::sleep(Duration::from_secs(5)).await,
        }
    }
}

#[cfg(test)]
mod tests;
