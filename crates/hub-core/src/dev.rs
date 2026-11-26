//! Dev mode: working on Hub from inside Hub. Hub's own repository is cloned to a folder of its
//! own (the updater's clone under `src/` stays untouched) and added as a project, and its UI is
//! served there by Vite with hot reload. The app opens that in a second window, which talks to
//! this core like the main one, so a change a thread makes to the UI shows as it is saved, and a
//! change that breaks the UI only breaks the dev window. Shipping commits and pushes to the branch
//! the updater follows, which tests, builds and offers it like any other version.
//!
//! This file owns the clone and the live UI: where the clone is, its status, setting it up,
//! starting and stopping Vite, and the git and process helpers the parts below share. The parts:
//! `preview.rs` (the clone following the open pull requests), `ship.rs` (shipping the clone's work
//! as a pull request), `core.rs` (the dev core on port 7879) and `build.rs` (packages and the
//! build cache).

mod build;
mod core;
mod preview;
mod ship;

use build::install_packages;
pub(crate) use build::trim_cache;
use preview::{LIVE, preview_only};

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::hub::Hub;
pub use crate::dev_state::Dev;
use crate::model::{
    DevCoreState, DevPreview, DevShip, DevShipped, DevState, DevStatus, GitDraft, Project, StreamMsg,
    now_ms,
};
use crate::proc::{GhError, gh_within};

/// The live UI's port. Not Vite's usual 1420, so `npm run tauri dev` can run beside it.
pub const PORT: u16 = 1421;

/// Origins the dev window's requests come from.
pub const ORIGINS: [&str; 2] = ["http://127.0.0.1:1421", "http://localhost:1421"];

/// Where the clone is, when moved from the default `<projects root>/hub`.
const DIR_KEY: &str = "dev.dir";
/// The branch last shipped and its pull request, `<branch> <url>`.
const PR_KEY: &str = "dev.pr";
/// What the preview shows, `all`, `live` (every one that changes only the pages, for the main
/// window's Live mode) or `pr:<number>`; no preview when unset.
const PREVIEW_KEY: &str = "dev.preview";
/// `on` while the dev window should use a dev core, so it comes back after this core restarts.
const CORE_KEY: &str = "dev.core";

pub fn url() -> String {
    format!("http://127.0.0.1:{PORT}")
}

fn serving() -> bool {
    TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], PORT)), Duration::from_millis(200)).is_ok()
}

fn log_path(hub: &Hub) -> PathBuf {
    hub.config.data_dir.join("dev.log")
}

/// The dev core's port, next to this core's 7878.
pub const CORE_PORT: u16 = 7879;

pub fn core_url() -> String {
    format!("http://127.0.0.1:{CORE_PORT}")
}

fn core_serving() -> bool {
    TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], CORE_PORT)), Duration::from_millis(200)).is_ok()
}

/// The dev core's own data folder: its database (a copy of this one), config, token and logs.
fn core_dir(hub: &Hub) -> PathBuf {
    hub.config.data_dir.join("dev-core")
}

fn core_log(hub: &Hub) -> PathBuf {
    core_dir(hub).join("build.log")
}

impl Hub {
    pub async fn dev_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = self.store.get_value(DIR_KEY).await? {
            return Ok(PathBuf::from(dir));
        }
        Ok(Path::new(&self.projects_root().await?).join("hub"))
    }

    pub async fn dev_status(&self) -> Result<DevStatus> {
        let dir = self.dev_dir().await?;
        let (shown_dir, shown) = self.dev_shown().await?;
        let log = log_path(self);
        let (branch, changed, ahead) = {
            let (dir, shown_dir) = (dir.clone(), shown_dir.clone());
            let remote = format!("{}/{}", self.config.update.remote, self.config.update.branch);
            tokio::task::spawn_blocking(move || {
                let clone = git_state(&dir, &remote);
                // No branch still means no clone, whatever is shown.
                if clone.0.is_some() && shown_dir != dir { git_state(&shown_dir, &remote) } else { clone }
            })
            .await?
        };
        let threads = self.dev_threads().await.unwrap_or_default();
        let worktrees = self.dev_worktrees_on().await;
        let cache_cap = self.dev_cache_cap().await;
        let pr = self.store.get_value(PR_KEY).await?.and_then(|v| {
            let (shipped, url) = v.split_once(' ')?;
            (Some(shipped) == branch.as_deref()).then(|| url.to_owned())
        });
        let only = self.store.get_value(PREVIEW_KEY).await?.map(|v| (preview_only(&v), v == LIVE)).filter(|_| shown.is_none());
        let serving = serving();
        let mut inner = self.dev.lock();
        // A server that exited on its own failed, unless it was stopped from here.
        if let Some(code) = inner.server.as_mut().and_then(|c| c.try_wait().ok().flatten()) {
            inner.server = None;
            inner.busy = None;
            inner.failed = true;
            inner.message = Some(format!("The live UI stopped ({code}):\n{}", tail(&log)));
        }
        if let Some(code) = inner.core.as_mut().and_then(|c| c.try_wait().ok().flatten()) {
            inner.core = None;
            inner.core_failed = true;
            inner.core_message = Some(format!("The dev core stopped ({code}):\n{}", tail(&core_dir(self).join("core.log"))));
        }
        let state = match inner.busy {
            Some(DevState::SettingUp) => DevState::SettingUp,
            _ if branch.is_none() => {
                if inner.failed { DevState::Failed } else { DevState::Missing }
            }
            _ if serving => DevState::Running,
            Some(state) => state,
            None if inner.failed => DevState::Failed,
            None => DevState::Stopped,
        };
        if state == DevState::Running {
            inner.busy = None;
            inner.failed = false;
            inner.message = None;
        }
        let core = if inner.core_building {
            DevCoreState::Building
        } else if core_serving() {
            DevCoreState::Running
        } else if inner.core_failed {
            DevCoreState::Failed
        } else {
            DevCoreState::Off
        };
        Ok(DevStatus {
            dir: dir.display().to_string(),
            state,
            url: url(),
            branch,
            changed,
            ahead,
            message: inner.message.clone(),
            pr,
            core,
            core_url: core_url(),
            core_message: inner.core_message.clone(),
            preview: only.map(|(only, live)| {
                let p = &inner.preview;
                DevPreview {
                    only,
                    live,
                    base: p.base.clone(),
                    included: p.included.clone(),
                    skipped: p.skipped.clone(),
                    open: p.open.clone(),
                    core_changed: p.core_changed,
                    refreshing: p.refreshing,
                    refreshed_at: p.refreshed_at,
                    message: p.message.clone(),
                    merged: p.merged.clone(),
                    check: p.check.clone(),
                }
            }),
            worktrees,
            threads,
            shown,
            cache_bytes: inner.cache_bytes,
            cache_cap,
        })
    }

    /// Clones Hub, installs its packages, adds it as a project and starts the live UI, in the
    /// background. Each step is skipped when it is done already.
    pub async fn dev_setup(self: &Arc<Self>, dir: Option<String>) -> Result<DevStatus> {
        if let Some(dir) = dir.as_deref().map(|d| d.trim().trim_end_matches(['/', '\\'])).filter(|d| !d.is_empty()) {
            self.store.set_value(DIR_KEY, dir).await?;
        }
        {
            let mut inner = self.dev.lock();
            if inner.busy.is_some() {
                bail!("dev mode is already being set up");
            }
            inner.busy = Some(DevState::SettingUp);
            inner.failed = false;
            inner.message = Some("Cloning Hub".into());
        }
        let hub = self.clone();
        tokio::spawn(async move {
            if let Err(e) = hub.set_up_dev().await {
                hub.dev.fail(format!("{e:#}"));
                return;
            }
            hub.dev.lock().busy = None;
            if let Err(e) = hub.dev_start().await {
                hub.dev.fail(format!("{e:#}"));
            }
            // A preview asked for before there was a clone can start now.
            hub.dev.kick.notify_one();
        });
        self.dev_status().await
    }

    async fn set_up_dev(self: &Arc<Self>) -> Result<()> {
        let dir = self.dev_dir().await?;
        let source = self.config.update.source_in(&self.config.data_dir);
        let (remote, branch) = (self.config.update.remote.clone(), self.config.update.branch.clone());
        let log = log_path(self);
        let clone_dir = dir.clone();
        tokio::task::spawn_blocking(move || clone(&source, &clone_dir, &remote, &branch, &log)).await??;

        if !dir.join("app/node_modules").is_dir() {
            self.dev.lock().message = Some("Installing packages".into());
            let (app, log) = (dir.join("app"), log_path(self));
            tokio::task::spawn_blocking(move || install_packages(&app, &log)).await??;
        }

        let path = dir.display().to_string();
        let project = self.store.insert_project(Project { path, name: "Hub (dev)".into(), created_at: now_ms() }).await?;
        self.publish(StreamMsg::Project { project });
        Ok(())
    }

    /// Starts the live UI server in the clone and waits for it to answer.
    pub async fn dev_start(self: &Arc<Self>) -> Result<DevStatus> {
        if serving() {
            return self.dev_status().await;
        }
        let clone = self.dev_dir().await?;
        if !clone.join("app/node_modules").is_dir() {
            bail!("dev mode is not set up in {} yet", clone.display());
        }
        let (dir, _) = self.dev_shown().await?;
        let core = self.dev.lock().core_wanted.then(core_url);
        let child = spawn_server(&dir, &clone, &log_path(self), core.as_deref())?;
        {
            let mut inner = self.dev.lock();
            inner.server = Some(child);
            inner.busy = Some(DevState::Starting);
            inner.failed = false;
            inner.message = None;
        }
        let hub = self.clone();
        tokio::spawn(async move {
            for _ in 0..120 {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let status = hub.dev_status().await;
                if status.as_ref().is_ok_and(|s| s.state != DevState::Starting) {
                    return;
                }
            }
            let log = log_path(&hub);
            hub.dev.fail(format!("The live UI did not come up within a minute:\n{}", tail(&log)));
        });
        self.dev_status().await
    }

    /// Stops the live UI server, also one an earlier core started.
    pub async fn dev_stop(&self) -> Result<DevStatus> {
        let child = {
            let mut inner = self.dev.lock();
            inner.busy = None;
            inner.failed = false;
            inner.message = None;
            inner.server.take()
        };
        tokio::task::spawn_blocking(move || stop(child, PORT, serving)).await??;
        self.dev_status().await
    }

    /// Restarts the live UI, so it talks to the core it should; the page reloads by itself.
    pub(crate) async fn dev_restart_ui(self: &Arc<Self>) -> Result<()> {
        // Without a set-up clone the server on the port is not dev mode's (a test's, say).
        if serving() && self.dev_dir().await?.join("app/node_modules").is_dir() {
            self.dev_stop().await?;
            self.dev_start().await?;
        }
        Ok(())
    }

}

/// Picks the preview back up when the core starts, if it was on. A dev core leaves it alone: it
/// shares the clone with this one.
pub async fn follow(hub: Arc<Hub>) {
    if hub.config.bind.port() == CORE_PORT {
        return;
    }
    hub.trim_dev_cache().await;
    if matches!(hub.store.get_value(PREVIEW_KEY).await, Ok(Some(_))) {
        // A server an earlier core left running has been through every rewrite of the clone since,
        // and one that missed a file keeps serving the old one for good (the old Where it goes
        // stayed on screen for hours). A new core starts it afresh; the page reloads by itself.
        if let Err(e) = hub.dev_restart_ui().await {
            tracing::warn!("the live UI did not restart with this core: {e:#}");
        }
        hub.follow_preview();
    }
    // The dev core and the loop that rebuilds it, if they were on.
    if matches!(hub.store.get_value(CORE_KEY).await, Ok(Some(_)))
        && let Err(e) = hub.dev_core(true).await {
            tracing::warn!("dev core did not come back: {e:#}");
        }
}

/// Stops `child`, or whatever listens on `port` when this core did not start it, with
/// everything it started, and waits for the port to close.
fn stop(child: Option<Child>, port: u16, up: fn() -> bool) -> Result<()> {
    let pid = match child {
        Some(child) => Some(child.id()),
        None => crate::host::snapshot(&[], &[PORT, CORE_PORT]).listeners.into_iter().find(|l| l.port == port).and_then(|l| l.pid),
    };
    if let Some(pid) = pid {
        crate::workers::act(pid, crate::workers::Action::Stop)?;
    }
    // So the status that follows does not still see it.
    for _ in 0..20 {
        if !up() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

/// When the core's code in the clone last changed: Rust files and manifests under `crates/`,
/// and the lock file.
fn newest_source(dir: &Path) -> Option<std::time::SystemTime> {
    fn walk(dir: &Path, newest: &mut Option<std::time::SystemTime>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if name != "target" && name != "node_modules" && !name.starts_with('.') {
                    walk(&path, newest);
                }
            } else if (name.ends_with(".rs") || name == "Cargo.toml")
                && let Ok(time) = entry.metadata().and_then(|m| m.modified())
            {
                *newest = (*newest).max(Some(time));
            }
        }
    }
    let mut newest = std::fs::metadata(dir.join("Cargo.lock")).and_then(|m| m.modified()).ok();
    walk(&dir.join("crates"), &mut newest);
    newest
}

/// A new branch for shipping: `hub-dev/` and the change's title, made unique by the time.
pub(crate) fn branch_name(title: &str, ms: i64) -> String {
    let mut slug = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 40 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    let stamp = format!("{:x}", (ms / 1000) & 0xffffff);
    if slug.is_empty() { format!("hub-dev/{stamp}") } else { format!("hub-dev/{slug}-{stamp}") }
}

/// The state (`OPEN`, `MERGED`, `CLOSED`) and address of the newest pull request from `branch`.
async fn pr_for(dir: &Path, branch: &str) -> Option<(String, String)> {
    let args = ["pr", "list", "--head", branch, "--state", "all", "--limit", "1", "--json", "state,url"];
    let out = gh_within(dir, &args, Duration::from_secs(20)).await.ok()?;
    let v: serde_json::Value = serde_json::from_str(&out).ok()?;
    let pr = v.get(0)?;
    Some((pr["state"].as_str()?.to_owned(), pr["url"].as_str()?.to_owned()))
}

/// The branch, uncommitted files and commits ahead of `upstream` of the clone or a thread's
/// worktree. No branch when there is no clone. The dev page asks every two seconds for the clone
/// and every worktree, so the branch is read from HEAD and the counts are kept a while
/// (`git::run_cached`) instead of starting three gits each time.
pub(crate) fn git_state(dir: &Path, upstream: &str) -> (Option<String>, u32, u32) {
    if !dir.join(".git").exists() {
        return (None, 0, 0);
    }
    let branch = crate::git::current_branch(dir);
    let changed = crate::git::run_cached(dir, &["status", "--porcelain"]).map_or(0, |s| s.lines().count() as u32);
    let ahead = crate::git::run_cached(dir, &["rev-list", "--count", &format!("{upstream}..HEAD")])
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0);
    (Some(branch.unwrap_or_default()), changed, ahead)
}

/// Clones Hub into `dir` from the updater's clone, which is already on disk and allowed to fetch,
/// then points it at the same remote and checks out the branch the updater follows.
fn clone(source: &Path, dir: &Path, remote: &str, branch: &str, log: &Path) -> Result<()> {
    if dir.join(".git").exists() {
        return Ok(());
    }
    if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
        bail!("{} exists and is not a clone of Hub; pick another folder", dir.display());
    }
    if !source.join(".git").exists() {
        bail!("there is no clone of Hub at {} to start from; clone it to {} yourself", source.display(), dir.display());
    }
    let url = git(source, &["remote", "get-url", remote])?;
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    run(
        source,
        "git",
        &["clone", "--no-checkout", &source.display().to_string(), &dir.display().to_string()],
        log,
    )?;
    git(dir, &["remote", "set-url", "origin", &url])?;
    if remote != "origin" {
        git(dir, &["remote", "rename", "origin", remote])?;
    }
    run(dir, "git", &["fetch", remote, branch], log)?;
    git(dir, &["checkout", "-B", branch, "--track", &format!("{remote}/{branch}")])?;
    Ok(())
}

/// Vite in `dir`'s app (the clone or a thread's worktree). `core` is the core the page talks to,
/// when not this one. A worktree's packages are a link to the clone's, which Vite is allowed to
/// serve from.
fn spawn_server(dir: &Path, clone: &Path, log: &Path, core: Option<&str>) -> Result<Child> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let out = File::create(log).with_context(|| format!("creating {}", log.display()))?;
    let port = PORT.to_string();
    let mut cmd = command(npm, &dir.join("app"));
    cmd.args(["run", "dev", "--", "--port", &port, "--strictPort"])
        // The server belongs to the project in Workers and on its page.
        .env("HUB_PROJECT", dir)
        .env("HUB_DEV_MODULES", clone.join("app/node_modules"))
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out);
    if let Some(core) = core {
        cmd.env("VITE_HUB_URL", core);
    }
    cmd.spawn().with_context(|| format!("starting {npm} run dev; is Node installed?"))
}

pub(crate) fn run(dir: &Path, program: &str, args: &[&str], log: &Path) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(file, "\n== {} $ {program} {}", dir.display(), args.join(" "))?;
    let out = command(program, dir).args(args).stdin(Stdio::null()).output().with_context(|| format!("starting {program}"))?;
    file.write_all(&out.stdout)?;
    file.write_all(&out.stderr)?;
    if !out.status.success() {
        bail!("`{program} {}` failed (log in {}):\n{}", args.join(" "), log.display(), tail(log));
    }
    Ok(())
}

pub(crate) use crate::git::checked as git;

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

/// The last lines of a log.
fn tail(log: &Path) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let lines: Vec<&str> = text.lines().rev().take(12).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests;
