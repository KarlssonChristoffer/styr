//! The dev preview: the clone follows the updater's branch with every open pull request merged on
//! top (or just one, or in Live mode the ones that change only the pages), rebuilt as they are
//! pushed, so the dev window shows a thread's work seconds after it pushes. Threads mostly work
//! elsewhere (in the cloud, on other machines) and end in pull requests, which reach this machine
//! only once released; the preview closes that gap.
//!
//! Top to bottom: turning it on and off (`dev_preview`), the loop that keeps it current
//! (`follow_preview`, `refresh_preview`), what GitHub says (open and merged pull requests, what is
//! new since the last build), the git steps that rebuild the clone (`build_preview`,
//! `fetch_preview`, `leave_preview`, `park`), and the check that Vite serves the rebuilt pages
//! (`check_served`).

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::build::{install_packages, packages_stale};
use super::{PORT, PREVIEW_KEY, git, log_path, serving};
use crate::dev_state::{MergedPull, Preview, Probe};
use crate::hub::Hub;
use crate::locked::ResultExt;
use crate::model::{DevPreviewCheck, DevPreviewPr, DevPreviewSet, DevStatus, now_ms};
use crate::proc::{GhError, gh_within};

/// The local branch the preview is built on. Rebuilt from scratch on every refresh.
pub(super) const PREVIEW_BRANCH: &str = "hub-preview";
/// How often the preview looks for new pushes while someone is looking at it.
pub(super) const PREVIEW_EVERY: Duration = Duration::from_secs(30);
/// How often it looks when nobody has asked for the status lately; a look wakes it at once.
pub(super) const PREVIEW_IDLE_EVERY: Duration = Duration::from_secs(300);
/// How long a look at the status counts as someone watching.
pub(super) const WATCHING_MS: i64 = 2 * 60_000;
/// How long the merged list is trusted while the base and the open pull requests stay as they were.
const MERGED_FRESH_MS: i64 = 5 * 60_000;

/// The preview value of Live mode.
pub(super) const LIVE: &str = "live";

/// What a pull request may change and still be shown live: anything but the programs, whose
/// paths are the ones the updater's fast lane looks at (`update::native_stamp_at`).
const NATIVE_PATHS: [&str; 7] = ["crates", "Cargo.toml", "Cargo.lock", "app/src-tauri", ".cargo", "rust-toolchain.toml", "rust-toolchain"];

pub(super) fn preview_wait_after(looked: i64, now: i64) -> Duration {
    if now - looked < WATCHING_MS { PREVIEW_EVERY } else { PREVIEW_IDLE_EVERY }
}

/// `None` for every open pull request, or the one in `pr:<number>`.
pub(super) fn preview_only(value: &str) -> Option<u32> {
    value.strip_prefix("pr:").and_then(|n| n.parse().ok())
}

impl Hub {
    /// Turns the preview on (all open pull requests, or one), refreshes it now, or turns it off,
    /// which puts the clone back on the updater's branch.
    pub async fn dev_preview(self: &Arc<Self>, set: DevPreviewSet) -> Result<DevStatus> {
        if set.on {
            // The preview is in the clone, so the dev window goes back to showing that.
            if self.dev_shown().await?.1.is_some() {
                self.dev_show(crate::model::DevShow::default()).await?;
            }
            let live = set.live == Some(true);
            let value = if live { LIVE.to_owned() } else { set.only.map_or_else(|| "all".to_owned(), |n| format!("pr:{n}")) };
            self.store.set_value(PREVIEW_KEY, &value).await?;
            // Live mode is one click in the main window: it sets dev mode up when it is not.
            if live && !self.dev_dir().await?.join("app/node_modules").is_dir() && self.dev.lock().busy.is_none() {
                self.dev_setup(None).await?;
            }
            self.dev.lock().preview.force |= set.refresh;
            self.follow_preview();
            if !serving() && self.dev_dir().await?.join("app/node_modules").is_dir() {
                self.dev_start().await.warn_on_err("start the preview");
            }
        } else if self.store.take_value(PREVIEW_KEY).await?.is_some() {
            self.dev.kick.notify_one();
            let dir = self.dev_dir().await?;
            let (remote, base) = (self.config.update.remote.clone(), self.config.update.branch.clone());
            let _moving = self.dev.moving.lock().await;
            let parked = tokio::task::spawn_blocking(move || leave_preview(&dir, &remote, &base, now_ms())).await??;
            if let Some(branch) = parked {
                tracing::info!("dev preview: changed files parked on {branch}");
            }
        }
        self.dev_status().await
    }

    /// A surface asked for the dev status: the preview is being watched. After a quiet spell
    /// the loop slept long, so this wakes it for a look now.
    pub fn dev_looked(&self) {
        let before = self.dev.looked.swap(now_ms(), std::sync::atomic::Ordering::Relaxed);
        if self.dev.lock().preview.following && now_ms() - before > WATCHING_MS {
            self.dev.kick.notify_one();
        }
    }

    /// How long the preview loop waits before looking again: short while the status was asked
    /// for lately, long when nobody looks.
    fn preview_wait(&self, now: i64) -> Duration {
        preview_wait_after(self.dev.looked.load(std::sync::atomic::Ordering::Relaxed), now)
    }

    /// Starts the loop that keeps the preview current, or wakes it when it runs.
    pub(super) fn follow_preview(self: &Arc<Self>) {
        {
            let mut inner = self.dev.lock();
            if inner.preview.following {
                drop(inner);
                self.dev.kick.notify_one();
                return;
            }
            inner.preview.following = true;
        }
        let hub = self.clone();
        tokio::spawn(async move {
            loop {
                let (only, live) = match hub.store.get_value(PREVIEW_KEY).await {
                    Ok(Some(value)) => (preview_only(&value), value == LIVE),
                    _ => break,
                };
                if let Err(e) = hub.refresh_preview(only, live).await {
                    let mut inner = hub.dev.lock();
                    inner.preview.refreshing = false;
                    inner.preview.message = Some(format!("{e:#}"));
                    inner.preview.check = Some(DevPreviewCheck { at: now_ms(), pushed: Vec::new(), base_moved: false, error: Some(format!("{e:#}")) });
                }
                tokio::select! {
                    _ = hub.dev.kick.notified() => {}
                    _ = tokio::time::sleep(hub.preview_wait(now_ms())) => {}
                }
            }
            hub.dev.lock().preview = Preview::default();
        });
    }

    /// Rebuilds the preview when the updater's branch or a pull request in it moved since the
    /// last time. Files changed in the clone hold it back, unless a refresh was asked for: then
    /// they are parked on a branch of their own first. `live` leaves out the ones that change
    /// the programs.
    async fn refresh_preview(self: &Arc<Self>, only: Option<u32>, live: bool) -> Result<()> {
        let dir = self.dev_dir().await?;
        if !dir.join(".git").exists() {
            bail!("Set up dev mode first: there is no clone at {} yet", dir.display());
        }
        let (remote, base) = (self.config.update.remote.clone(), self.config.update.branch.clone());
        let pulls = open_pulls(&dir, &base).await?;
        let head = {
            let (dir, remote, base) = (dir.clone(), remote.clone(), base.clone());
            tokio::task::spawn_blocking(move || git(&dir, &["ls-remote", &remote, &format!("refs/heads/{base}")])).await??
        };
        let head = head.split_whitespace().next().unwrap_or_default().to_owned();
        let wanted: Vec<&OpenPull> = pulls.iter().filter(|p| only.is_none_or(|n| p.number == n)).collect();
        let from = std::iter::once(head.clone())
            .chain(live.then(|| LIVE.to_owned()))
            .chain(wanted.iter().map(|p| format!("{}:{}", p.number, p.head_ref_oid)))
            .collect::<Vec<_>>()
            .join(" ");
        let as_pr = |p: &OpenPull| DevPreviewPr { number: p.number, title: p.title.clone(), url: p.url.clone(), why: None, files: None };
        // Merged pull requests change when the base moves or a pull request closes, and both show
        // in what was just read; so the second `gh` call waits until one does.
        let merged = self.merged_since(&dir, &base, &head, &pulls).await;

        let _moving = self.dev.moving.lock().await;
        let (current, changed) = clone_state(&dir).await?;
        let already = {
            let mut inner = self.dev.lock();
            inner.preview.open = pulls.iter().map(as_pr).collect();
            if only.is_some_and(|n| wanted.is_empty() && !pulls.is_empty() && pulls.iter().all(|p| p.number != n)) {
                inner.preview.message = Some(format!("#{} is not open any more; showing {base} only", only.unwrap_or_default()));
            }
            // What this look found: how the pushes now differ from the ones the preview was built from.
            let (pushed, base_moved) = news(inner.preview.built_from.as_deref(), &from);
            inner.preview.check = Some(DevPreviewCheck { at: now_ms(), pushed, base_moved, error: None });
            let current_already = !inner.preview.force
                && current == PREVIEW_BRANCH
                && changed == 0
                && inner.preview.built_from.as_deref() == Some(from.as_str());
            if !current_already && changed > 0 && !inner.preview.force {
                inner.preview.message = Some(format!(
                    "Waiting: {changed} file{} changed in the clone. Refresh keeps them on a branch of their own and carries on.",
                    if changed == 1 { "" } else { "s" }
                ));
                return Ok(());
            }
            if !current_already {
                inner.preview.refreshing = true;
                inner.preview.force = false;
            }
            current_already
        };
        if already {
            self.note_merged(&dir, merged).await;
            // Nothing was rebuilt; the pages Vite was last given are still the ones to read back.
            self.check_served().await;
            return Ok(());
        }

        let prs: Vec<DevPreviewPr> = wanted.iter().map(|p| as_pr(p)).collect();
        let built = {
            let (dir, remote, base) = (dir.clone(), remote.clone(), base.clone());
            tokio::task::spawn_blocking(move || build_preview(&dir, &remote, &base, &prs, live, now_ms())).await??
        };
        // New or changed packages go in before the preview counts as ready: Live mode moves the
        // window the moment it is, and pages that import a package the clone does not have yet
        // (katex, after a merge that added it) do not start.
        if built.packages_changed {
            self.dev.lock().preview.message = Some("Installing packages".into());
            let (app, log) = (dir.join("app"), log_path(self));
            let installed = tokio::task::spawn_blocking(move || install_packages(&app, &log)).await?;
            if let Err(err) = installed {
                self.dev.lock().preview.refreshing = false;
                return Err(err);
            }
        }
        // Vite is told to start over whenever the pages in the clone changed, not only the packages:
        // its file watcher is not to be trusted with a branch switch (a push showed the old file
        // for good), and a server that starts again makes the page reload by itself.
        if built.packages_changed || built.app_changed {
            self.dev_restart_ui().await?;
        }
        {
            let mut inner = self.dev.lock();
            let p = &mut inner.preview;
            p.refreshing = false;
            p.built_from = Some(from);
            p.base = Some(built.base);
            p.included = built.included;
            p.skipped = built.skipped;
            p.core_changed = built.core_changed;
            p.refreshed_at = Some(now_ms());
            p.message = built.parked.map(|b| format!("What was changed in the clone is kept on {b}"));
            if built.app_changed {
                p.probe = built.probe;
            }
        }
        self.note_merged(&dir, merged).await;
        self.check_served().await;
        Ok(())
    }

    /// The merged pull requests, asked of `gh` again only when the base or an open pull request
    /// moved since the last answer.
    async fn merged_since(&self, dir: &Path, base: &str, head: &str, pulls: &[OpenPull]) -> Option<Vec<MergedPull>> {
        let asked = format!("{head} {}", pulls.iter().map(|p| format!("{}:{}", p.number, p.head_ref_oid)).collect::<Vec<_>>().join(" "));
        let cached = self.dev.lock().preview.merged_asked.clone().filter(|(k, at, _)| *k == asked && now_ms() - at < MERGED_FRESH_MS);
        match cached {
            Some((_, _, list)) => Some(list),
            None => {
                let fresh = merged_pulls(dir, base).await;
                if let Some(list) = &fresh {
                    self.dev.lock().preview.merged_asked = Some((asked, now_ms(), list.clone()));
                }
                fresh
            }
        }
    }

    /// Where the update stands, for a pull request merged but not installed yet.
    fn merged_says(&self) -> &'static str {
        use crate::model::UpdateState;
        let update = self.update_status();
        match update.state {
            UpdateState::Building | UpdateState::Checking if update.target.is_some() || update.building.is_some() => "merged, being built",
            UpdateState::Ready | UpdateState::Restarting => "merged, built, installs next",
            _ => "merged, the update starts soon",
        }
    }

    /// Updates the list of merged-but-not-installed pull requests from what GitHub answered.
    async fn note_merged(self: &Arc<Self>, dir: &Path, merged: Option<Vec<MergedPull>>) {
        let Some(merged) = merged else { return };
        let (dir, running, says) = (dir.to_owned(), self.update_status().version, self.merged_says());
        if let Ok(list) = tokio::task::spawn_blocking(move || not_installed(&dir, &running, &merged, says)).await {
            self.dev.lock().preview.merged = list;
        }
    }

    /// Asks Vite for the pages the last rebuild changed and compares them with the disk. A page
    /// it hands out as it was gets Vite restarted, and if it is still old after that the menu says
    /// so, so a live view that lags is not found by looking at it hours later.
    async fn check_served(self: &Arc<Self>) {
        let probe = self.dev.lock().preview.probe.clone();
        if probe.is_empty() || !serving() {
            return;
        }
        let stale = stale_pages(&probe).await;
        if stale.is_empty() {
            return;
        }
        tracing::warn!("the live UI served old copies of {}; restarting it", stale.join(", "));
        if self.dev_restart_ui().await.is_ok() {
            for _ in 0..30 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if serving() {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let still = stale_pages(&probe).await;
        let mut inner = self.dev.lock();
        inner.preview.message = Some(if still.is_empty() {
            format!("The live UI was serving an old copy of {}; restarted it", stale.join(", "))
        } else {
            format!("The live UI still serves an old copy of {} after a restart; see Labs > Dev mode", still.join(", "))
        });
        // Said once and restarted once: nothing more to compare until the next rebuild, so a phrase
        // the compiler changes does not restart Vite every half minute.
        inner.preview.probe.clear();
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenPull {
    number: u32,
    title: String,
    url: String,
    head_ref_oid: String,
}

/// The open pull requests into `base`, oldest number first.
async fn open_pulls(dir: &Path, base: &str) -> Result<Vec<OpenPull>> {
    let args = ["pr", "list", "--state", "open", "--base", base, "--limit", "50", "--json", "number,title,url,headRefOid"];
    let out = gh_within(dir, &args, Duration::from_secs(30)).await.map_err(|e| match e {
        GhError::Missing => anyhow::anyhow!("The preview needs the GitHub CLI (gh) to see the open pull requests"),
        GhError::Failed(msg) => anyhow::anyhow!("Could not list the open pull requests: {msg}"),
    })?;
    let mut pulls: Vec<OpenPull> = serde_json::from_str(&out).context("reading what gh answered")?;
    pulls.sort_by_key(|p| p.number);
    Ok(pulls)
}

/// The branch the clone is on and how many files in it changed.
async fn clone_state(dir: &Path) -> Result<(String, usize)> {
    let dir = dir.to_owned();
    Ok(tokio::task::spawn_blocking(move || {
        let current = crate::git::run(&dir, &["branch", "--show-current"]).map(|b| b.trim().to_owned()).unwrap_or_default();
        let changed = crate::git::run(&dir, &["status", "--porcelain"]).map_or(0, |s| s.lines().count());
        (current, changed)
    })
    .await?)
}

/// The pull requests merged into `base` lately, newest first. A failed look answers `None`, so
/// the list shown before stays.
async fn merged_pulls(dir: &Path, base: &str) -> Option<Vec<MergedPull>> {
    let args = ["pr", "list", "--state", "merged", "--base", base, "--limit", "12", "--json", "number,title,url,mergeCommit"];
    let out = gh_within(dir, &args, Duration::from_secs(30)).await.ok()?;
    serde_json::from_str(&out).ok()
}

/// What is new since a preview was built from `before` (see `from` in `refresh_preview`: the
/// branch head, `live` and `<number>:<head>` for each pull request): the pull requests whose head
/// is new, and whether the branch itself moved. Nothing is new for a preview not built yet.
pub(super) fn news(before: Option<&str>, now: &str) -> (Vec<u32>, bool) {
    let Some(before) = before else { return (Vec::new(), false) };
    let heads = |from: &str| -> Vec<(u32, String)> {
        from.split_whitespace().filter_map(|t| t.split_once(':')).filter_map(|(n, h)| Some((n.parse().ok()?, h.to_owned()))).collect()
    };
    let old = heads(before);
    let pushed = heads(now).into_iter().filter(|h| !old.contains(h)).map(|(n, _)| n).collect();
    (pushed, before.split_whitespace().next() != now.split_whitespace().next())
}

/// The ones of `merged` that the installed version `running` does not contain, as the live menu
/// lists them: oldest first, at most eight. `says` is where the update stands. A version this
/// clone does not know (a development build, or one from before the clone's last fetch) leaves
/// nothing to say, so the list is empty rather than guessed.
pub(super) fn not_installed(dir: &Path, running: &str, merged: &[MergedPull], says: &str) -> Vec<DevPreviewPr> {
    if running == "dev" || git(dir, &["rev-parse", "--verify", "--quiet", &format!("{running}^{{commit}}")]).is_err() {
        return Vec::new();
    }
    let mut out: Vec<DevPreviewPr> = merged
        .iter()
        .filter(|m| m.merge_commit.as_ref().is_some_and(|c| git(dir, &["merge-base", "--is-ancestor", &c.oid, running]).is_err()))
        .take(8)
        .map(|m| DevPreviewPr { number: m.number, title: m.title.clone(), url: m.url.clone(), why: Some(says.to_owned()), files: None })
        .collect();
    out.sort_by_key(|p| p.number);
    out
}

/// A rebuilt preview.
pub(super) struct Built {
    pub(super) base: String,
    pub(super) included: Vec<DevPreviewPr>,
    pub(super) skipped: Vec<DevPreviewPr>,
    pub(super) packages_changed: bool,
    /// Something under `app/` is not what it was before: what Vite serves has to be read again.
    pub(super) app_changed: bool,
    /// Up to three of the changed pages, to check Vite serves them as they are now.
    pub(super) probe: Vec<Probe>,
    pub(super) core_changed: bool,
    pub(super) parked: Option<String>,
}

/// Puts the clone on a fresh `hub-preview` branch from `remote/base` and merges `prs` into it in
/// order. One that does not merge cleanly on top of the others is left out, and with
/// `pages_only` one that changes the programs too. Changed files are parked first, and nothing
/// is pushed.
pub(super) fn build_preview(dir: &Path, remote: &str, base: &str, prs: &[DevPreviewPr], pages_only: bool, now: i64) -> Result<Built> {
    let before = git(dir, &["rev-parse", "HEAD"]).ok();
    let parked = park(dir, now)?;
    let missing = fetch_preview(dir, remote, base, prs)?;
    let upstream = format!("{remote}/{base}");
    git(dir, &["checkout", "--quiet", "-f", "-B", PREVIEW_BRANCH, &upstream])?;

    let (mut included, mut skipped) = (Vec::new(), Vec::new());
    for pr in prs {
        if missing.contains(&pr.number) {
            skipped.push(DevPreviewPr { why: Some("GitHub has no head for it yet".into()), files: None, ..pr.clone() });
            continue;
        }
        let head = format!("refs/hub-preview/{}", pr.number);
        let mut pr = pr.clone();
        if pages_only {
            let changed = git(dir, &["diff", "--name-only", &format!("{upstream}...{head}")])?;
            let files: Vec<String> = changed.lines().map(str::to_owned).collect();
            let native = files.iter().any(|f| NATIVE_PATHS.iter().any(|n| f == n || f.starts_with(&format!("{n}/"))));
            if native {
                skipped.push(DevPreviewPr { why: Some("changes Rust, so it shows once it is merged and built".into()), ..pr });
                continue;
            }
            pr.files = Some(files);
        }
        let message = format!("Preview #{} {}", pr.number, pr.title);
        let merged = git(
            dir,
            &[
                "-c", "user.name=Hub", "-c", "user.email=hub@localhost",
                "merge", "--quiet", "--no-ff", "--no-edit", "-m", &message, &head,
            ],
        );
        if merged.is_ok() {
            included.push(pr);
        } else {
            git(dir, &["merge", "--abort"]).warn_on_err("git merge --abort");
            let why = if included.is_empty() {
                format!("does not merge cleanly with {base}")
            } else {
                format!("does not merge cleanly with {base} and the ones before it")
            };
            skipped.push(DevPreviewPr { why: Some(why), files: None, ..pr });
        }
    }

    let packages_changed = packages_stale(&dir.join("app"));
    let core_changed = !git(dir, &["diff", "--name-only", &upstream, "HEAD", "--", "crates", "Cargo.lock"])?.is_empty();
    // Files kept aside count too: they were what the server showed until now.
    let app_changed = parked.is_some()
        || before.as_ref().is_none_or(|before| git(dir, &["diff", "--name-only", before, "HEAD", "--", "app"]).map_or(true, |changed| !changed.is_empty()));
    let probe = before.as_deref().map(|before| probes(dir, before)).unwrap_or_default();
    Ok(Built { base: git(dir, &["rev-parse", "--short", &upstream])?, included, skipped, packages_changed, app_changed, probe, core_changed, parked })
}

/// Fetches `base` and the head of each of `prs`, and answers the pull requests whose head the
/// remote does not have. One such head (a pull request listed open whose `refs/pull/<n>/head` is
/// gone, as #422's was) makes git refuse the whole fetch, the base included. Then the base is
/// fetched alone, a failure there is still an error, and each head on its own.
fn fetch_preview(dir: &Path, remote: &str, base: &str, prs: &[DevPreviewPr]) -> Result<Vec<u32>> {
    let fetch = |specs: &[String]| {
        let mut args = vec!["fetch", "--quiet", "--no-tags", remote];
        args.extend(specs.iter().map(String::as_str));
        git(dir, &args).map(|_| ())
    };
    let base_spec = format!("+refs/heads/{base}:refs/remotes/{remote}/{base}");
    let head_spec = |n: u32| format!("+refs/pull/{n}/head:refs/hub-preview/{n}");
    let mut specs = vec![base_spec.clone()];
    specs.extend(prs.iter().map(|p| head_spec(p.number)));
    if fetch(&specs).is_ok() {
        return Ok(Vec::new());
    }
    fetch(&[base_spec])?;
    let mut missing = Vec::new();
    for pr in prs {
        // A head fetched on an earlier refresh must not stand in for one the remote no longer has.
        drop_ref(dir, &format!("refs/hub-preview/{}", pr.number)).warn_on_err("drop an old preview head");
        if fetch(&[head_spec(pr.number)]).is_err() {
            missing.push(pr.number);
        }
    }
    Ok(missing)
}

/// Back from the preview to the updater's branch, as the remote has it. Changed files are parked.
pub(super) fn leave_preview(dir: &Path, remote: &str, base: &str, now: i64) -> Result<Option<String>> {
    if crate::git::run(dir, &["branch", "--show-current"]).map(|b| b.trim().to_owned()).unwrap_or_default() != PREVIEW_BRANCH {
        return Ok(None);
    }
    let parked = park(dir, now)?;
    let upstream = format!("{remote}/{base}");
    git(dir, &["fetch", "--quiet", "--no-tags", remote, &format!("+refs/heads/{base}:refs/remotes/{upstream}")])?;
    git(dir, &["checkout", "--quiet", "-f", "-B", base, &upstream])?;
    git(dir, &["branch", "--quiet", "--set-upstream-to", &upstream]).warn_on_err("set the branch's upstream");
    Ok(parked)
}

/// Commits whatever is changed in the clone to a new local branch, so moving the clone loses
/// nothing. Answers the branch, when there was something to keep.
pub(super) fn park(dir: &Path, now: i64) -> Result<Option<String>> {
    // A merge a refresh was cut off in the middle of is not anyone's work.
    abort_merge(dir).warn_on_err("git merge --abort");
    if git(dir, &["status", "--porcelain"])?.is_empty() {
        return Ok(None);
    }
    let name = format!("hub-dev/parked-{:x}", now / 1000);
    git(dir, &["switch", "--quiet", "-c", &name])?;
    git(dir, &["add", "-A"])?;
    git(
        dir,
        &["-c", "user.name=Hub", "-c", "user.email=hub@localhost", "commit", "--quiet", "--no-verify", "-m", "Kept when the dev preview took over the clone"],
    )?;
    Ok(Some(name))
}

/// Aborts the merge in progress in `dir`, if there is one: no `MERGE_HEAD` (git resolves it in
/// the right git dir, also for a linked worktree) is nothing to do, not a failure.
pub(super) fn abort_merge(dir: &Path) -> Result<()> {
    if git(dir, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_err() {
        return Ok(());
    }
    git(dir, &["merge", "--abort"]).map(|_| ())
}

/// Deletes the ref `name` in `dir`, if it is there; a ref that is gone already is not a failure.
pub(super) fn drop_ref(dir: &Path, name: &str) -> Result<()> {
    if git(dir, &["rev-parse", "-q", "--verify", name]).is_err() {
        return Ok(());
    }
    git(dir, &["update-ref", "-d", name]).map(|_| ())
}

/// The pages (`.svelte`, `.ts` under `app/src`) the clone changed since `before`, up to three,
/// each with phrases from the lines the change added.
fn probes(dir: &Path, before: &str) -> Vec<Probe> {
    let Ok(files) = git(dir, &["diff", "--name-only", "--diff-filter=AM", before, "HEAD", "--", "app/src"]) else { return Vec::new() };
    let mut out = Vec::new();
    for file in files.lines().filter(|f| f.ends_with(".svelte") || f.ends_with(".ts")) {
        let Ok(diff) = git(dir, &["diff", "-U0", before, "HEAD", "--", file]) else { continue };
        let added: String = diff.lines().filter_map(|l| l.strip_prefix('+')).filter(|l| !l.starts_with("++")).collect::<Vec<_>>().join("\n");
        let needles = phrases(&added);
        if !needles.is_empty() {
            out.push(Probe { url_path: format!("/{}", file.trim_start_matches("app/")), needles });
        }
        if out.len() == 3 {
            break;
        }
    }
    out
}

/// Plain phrases of text in `source`, as they come out of the compiler unchanged: at least two
/// words, ten letters, no code or comment. They are what is looked for in the served page.
pub(super) fn phrases(source: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in source.lines() {
        let line = line.trim();
        if line.starts_with("//") || line.starts_with('*') || line.starts_with("/*") || line.starts_with("<!--") {
            continue;
        }
        let mut run = String::new();
        for c in line.chars().chain(std::iter::once('\n')) {
            if c.is_ascii_alphabetic() || c == ' ' || c == ',' || c == '.' || c == '-' {
                run.push(c);
                continue;
            }
            let phrase = run.split_whitespace().collect::<Vec<_>>().join(" ");
            if phrase.len() >= 10 && phrase.len() <= 60 && phrase.contains(' ') && !out.contains(&phrase) {
                out.push(phrase);
            }
            run.clear();
        }
        if out.len() >= 6 {
            break;
        }
    }
    out
}

/// Whether the served text has none of the phrases the file has on disk: Vite is handing out an
/// old copy.
pub(super) fn serves_old(served: &str, needles: &[String]) -> bool {
    let served = served.split_whitespace().collect::<Vec<_>>().join(" ");
    !needles.is_empty() && !needles.iter().any(|n| served.contains(n.as_str()))
}

/// The pages of `probes` that Vite serves as they were, not as they are.
async fn stale_pages(probes: &[Probe]) -> Vec<String> {
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(5)).build() else { return Vec::new() };
    let mut stale = Vec::new();
    for probe in probes {
        let Ok(response) = client.get(format!("http://127.0.0.1:{PORT}{}", probe.url_path)).send().await else { continue };
        if !response.status().is_success() {
            continue;
        }
        if let Ok(text) = response.text().await
            && serves_old(&text, &probe.needles)
        {
            stale.push(probe.url_path.clone());
        }
    }
    stale
}
