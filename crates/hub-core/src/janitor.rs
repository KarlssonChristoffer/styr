//! Cleaning up after agent work. Threads leave worktrees, branches, temporary folders and logs
//! behind; every half hour (and from Settings > Updates, Clean up now) Hub removes what is safe
//! to lose and lists what it had to leave, with why.
//!
//! Safe to lose means nothing in it exists only on this machine:
//! - a worktree Hub made (`<clone>/.hub-threads/<id>`, `.git/hub-attempts/<n>`) whose files are
//!   all committed and whose commits are all on a remote, and whose threads have been quiet for
//!   an hour. A thread written to again gets its worktree back, on the same branch.
//! - a folder in `.git/hub-attempts` that picking an attempt could not delete, an hour after Hub
//!   noted it as such, its work kept under `refs/hub/` (see `sweep_attempt_leftovers`).
//! - a local branch that no worktree has out, is not the default branch, and has no commit that
//!   is not on a remote. The remote branch and the pull request stay; so does git's history.
//! - Hub's own temporary files (`hub-…` in the system's temp folder) older than a day, and engine
//!   logs that stayed empty.
//!
//! Everything else stays: uncommitted changes, unpushed commits, a thread still on it, and
//! anything Hub did not make (listed so it can be looked at, never touched).

use crate::locked::ResultExt;
use crate::locked::Ignore;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::Result;

use crate::dev::{self, git};
use crate::dev_threads;
use crate::hub::Hub;
use crate::model::{Cleaned, Cleanup, Thread, ThreadStatus, now_ms};

const EVERY: Duration = Duration::from_secs(30 * 60);
/// How long a thread has to be quiet before its worktree goes.
const QUIET_MS: i64 = 60 * 60 * 1000;
/// How old Hub's temporary files have to be: none of them is needed for longer than a run.
const TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const LAST_KEY: &str = "cleanup.last";
/// The branch a thread's worktree was on when it was cleaned up, to bring it back on.
const BRANCH_KEY: &str = "dev.branch.";
/// What Hub (and its tests) name their temporary files and folders.
const TEMP_PREFIXES: &[&str] = &[
    "hub-test-",
    "hub-bench-",
    "hub-page-check-",
    "hub-carry-",
    "hub-engine-test",
    "hub-pages-",
    "hub-library-",
    "hub-usage-detail-",
    "hub-resolve-",
    "hub-auto-",
    "hub-draft-check-",
];

/// Cleans up shortly after the core starts and then every half hour. A dev core leaves it to
/// the real one: they share the clone.
pub async fn often(hub: Arc<Hub>) {
    if hub.config.bind.port() == dev::CORE_PORT {
        return;
    }
    tokio::time::sleep(Duration::from_secs(180)).await;
    loop {
        if let Err(e) = hub.clean_up().await {
            tracing::warn!("cleanup: {e:#}");
        }
        tokio::time::sleep(EVERY).await;
    }
}

impl Hub {
    /// The last clean-up, if one ran.
    pub async fn last_cleanup(&self) -> Option<Cleanup> {
        let text = self.store.get_value(LAST_KEY).await.ok().flatten()?;
        serde_json::from_str(&text).ok()
    }

    /// Cleans up now; see the module.
    pub async fn clean_up(self: &Arc<Self>) -> Result<Cleanup> {
        let threads: Vec<Thread> = self.store.threads().await?.into_iter().filter(|t| t.host.is_none()).collect();
        let shown = self.store.get_value(dev_threads::SHOW_KEY).await?.unwrap_or_default();
        let now = now_ms();
        // The folders threads are in, and whether each still needs it.
        let folders: Vec<(PathBuf, bool, String)> = threads
            .iter()
            .map(|t| {
                let busy = matches!(t.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou | ThreadStatus::InTerminal)
                    || now - t.updated_at < QUIET_MS
                    || t.id == shown;
                (PathBuf::from(&t.cwd), busy, t.id.clone())
            })
            .collect();
        let mut repos: Vec<PathBuf> = self.store.projects().await?.into_iter().map(|p| PathBuf::from(p.path)).collect();
        if let Ok(clone) = self.dev_dir().await {
            repos.push(clone);
        }
        let defaults = vec![self.config.update.branch.clone(), "main".to_owned(), "master".to_owned()];
        let (data, temp) = (self.config.data_dir.clone(), std::env::temp_dir());
        let (mut done, branches) = tokio::task::spawn_blocking(move || {
            let mut done = Cleanup { at: now, ..Default::default() };
            let mut branches = Vec::new();
            let mut seen = Vec::new();
            for repo in repos {
                let Ok(root) = git(&repo, &["rev-parse", "--show-toplevel"]).map(PathBuf::from) else { continue };
                let root = dev_threads::canon(&root).unwrap_or(root);
                if seen.contains(&root) {
                    continue;
                }
                seen.push(root.clone());
                branches.extend(sweep_repo(&root, &folders, &defaults, &mut done));
            }
            let (n, bytes) = sweep_temp(&temp, TEMP_AGE);
            if n > 0 {
                done.removed.push(Cleaned { kind: "temp".into(), path: temp.display().to_string(), why: "Hub's temporary files older than a day".into(), count: n });
                done.freed += bytes;
            }
            let (n, bytes) = sweep_empty_logs(&data.join("engines"), TEMP_AGE);
            if n > 0 {
                done.removed.push(Cleaned { kind: "log".into(), path: data.join("engines").display().to_string(), why: "empty engine logs".into(), count: n });
                done.freed += bytes;
            }
            (done, branches)
        })
        .await?;
        // Remember the branch a thread's worktree was on, to bring it back on.
        for (id, branch) in branches {
            self.store.set_value(&format!("{BRANCH_KEY}{id}"), &branch).await?;
        }
        done.at = now_ms();
        for c in &done.removed {
            tracing::info!(kind = %c.kind, path = %c.path, count = c.count, "cleanup: removed ({})", c.why);
        }
        self.store.set_value(LAST_KEY, &serde_json::to_string(&done)?).await?;
        Ok(done)
    }

    /// Brings back the worktree of a thread that the clean-up removed, before the thread is
    /// written to: on its branch when that is still here or on the remote, otherwise (the work
    /// was merged and the branch deleted) on a new branch from the updater's branch for Hub (dev),
    /// or from the project's base for any other project with worktrees on.
    pub(crate) async fn bring_back_folder(&self, thread: &Thread) {
        if thread.host.is_some() || Path::new(&thread.cwd).exists() {
            return;
        }
        // The thread's folder is `<repository>/.hub-threads/<id>`, as made.
        let cwd = PathBuf::from(&thread.cwd);
        let parts: Vec<_> = cwd.components().collect();
        let Some(at) = parts.iter().position(|c| c.as_os_str() == dev_threads::FOLDER) else { return };
        let Some(name) = parts.get(at + 1).map(|c| c.as_os_str().to_owned()) else { return };
        let repo: PathBuf = parts[..at].iter().collect();
        if !repo.join(".git").exists() {
            return;
        }
        let Ok(repo) = dev_threads::canon(&repo) else { return };
        let tree = repo.join(dev_threads::FOLDER).join(name);
        let key = format!("{BRANCH_KEY}{}", thread.id);
        let is_dev = self.dev_dir().await.ok().and_then(|d| dev_threads::canon(d).ok()).is_some_and(|d| d == repo);
        let branch = match self.store.get_value(&key).await {
            Ok(Some(b)) => b,
            _ if is_dev => dev::branch_name(&thread.title, now_ms()),
            _ => dev::branch_name(&thread.title, now_ms()).replacen("hub-dev/", "hub/", 1),
        };
        let (remote, base, link) = if is_dev {
            (self.config.update.remote.clone(), self.config.update.branch.clone(), true)
        } else {
            // The project's base, `remote/branch`, or its remote's default; without one, HEAD.
            let base = self.tree_base(&repo).await.or_else(|| git(&repo, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]).ok());
            match base.as_deref().and_then(|b| b.split_once('/')) {
                Some((r, b)) => (r.to_owned(), b.to_owned(), false),
                None => ("origin".to_owned(), "main".to_owned(), false),
            }
        };
        let made = tokio::task::spawn_blocking(move || restore(&repo, &tree, &branch, &remote, &base, link)).await;
        match made {
            Ok(Ok(())) => {
                self.store.take_value(&key).await.warn_on_err("forget the restore key");
                tracing::info!(thread = %thread.id, "cleanup: brought the thread's worktree back");
            }
            Ok(Err(e)) => tracing::warn!(thread = %thread.id, "cleanup: could not bring the worktree back: {e:#}"),
            Err(_) => {}
        }
    }
}

/// Commits on `rev` that no remote has. Without a remote, all of them.
fn unpushed(repo: &Path, rev: &str) -> Option<u32> {
    git(repo, &["rev-list", "--count", rev, "--not", "--remotes"]).ok()?.trim().parse().ok()
}

/// Why `tree` cannot go without losing something, or `None` when everything in it is on a remote.
pub(crate) fn keeps(tree: &Path) -> Option<String> {
    let Ok(status) = git(tree, &["status", "--porcelain"]) else { return Some("git cannot read it".into()) };
    let changed = status.lines().filter(|l| !l.trim().is_empty()).count();
    if changed > 0 {
        return Some(format!("{changed} uncommitted {}", if changed == 1 { "change" } else { "changes" }));
    }
    match unpushed(tree, "HEAD") {
        Some(0) => None,
        Some(n) => Some(format!("{n} {} not pushed", if n == 1 { "commit" } else { "commits" })),
        None => Some("git cannot tell what is pushed".into()),
    }
}

/// One worktree of a repository, as `git worktree list --porcelain` tells it.
struct Listed {
    path: PathBuf,
    branch: Option<String>,
    prunable: bool,
}

/// The repository's worktrees, the repository itself first; `None` when git could not list them
/// (a lock, a timeout), which callers must not read as "there are none".
fn worktrees(repo: &Path) -> Option<Vec<Listed>> {
    let text = git(repo, &["worktree", "list", "--porcelain"]).ok()?;
    let mut out: Vec<Listed> = Vec::new();
    for line in text.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            out.push(Listed { path: PathBuf::from(p), branch: None, prunable: false });
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            if let Some(l) = out.last_mut() {
                l.branch = Some(b.to_owned());
            }
        } else if line.starts_with("prunable")
            && let Some(l) = out.last_mut()
        {
            l.prunable = true;
        }
    }
    // Git always lists the repository itself.
    (!out.is_empty()).then_some(out)
}

/// Cleans one repository: its worktrees Hub made, then its branches. Answers the branches of
/// thread worktrees removed, by thread id.
fn sweep_repo(root: &Path, folders: &[(PathBuf, bool, String)], defaults: &[String], done: &mut Cleanup) -> Vec<(String, String)> {
    git(root, &["worktree", "prune"]).warn_on_err("git worktree prune");
    let common = git(root, &["rev-parse", "--git-common-dir"]).map(PathBuf::from).unwrap_or_else(|_| root.join(".git"));
    let common = if common.is_absolute() { common } else { root.join(common) };
    let ours = [dev_threads::folder(root), common.join("hub-attempts")];
    let ours: Vec<PathBuf> = ours.iter().map(|p| dev_threads::canon(p).unwrap_or_else(|_| p.clone())).collect();
    let mut gone = Vec::new();
    for (i, w) in worktrees(root).into_iter().flatten().enumerate() {
        // The first is the repository itself.
        if i == 0 || w.prunable {
            continue;
        }
        let path = dev_threads::canon(&w.path).unwrap_or(w.path.clone());
        let shown = path.display().to_string();
        let Some(under) = ours.iter().find(|o| path.starts_with(o)) else {
            done.kept.push(Cleaned { kind: "worktree".into(), path: shown, why: "not made by Hub; look at it yourself".into(), count: 1 });
            continue;
        };
        let attempt = under.ends_with("hub-attempts");
        let mine: Vec<&(PathBuf, bool, String)> =
            folders.iter().filter(|(cwd, ..)| dev_threads::canon(cwd).unwrap_or_else(|_| cwd.clone()).starts_with(&path)).collect();
        if mine.iter().any(|(_, busy, _)| *busy) {
            done.kept.push(Cleaned { kind: "worktree".into(), path: shown, why: "a thread is still on it".into(), count: 1 });
            continue;
        }
        if let Some(why) = keeps(&path) {
            done.kept.push(Cleaned { kind: "worktree".into(), path: shown, why, count: 1 });
            continue;
        }
        // An attempt's thread has no way back to a removed worktree; it goes once picked or
        // archived, which removes it already. One with a thread stays.
        if attempt && !mine.is_empty() {
            continue;
        }
        dev_threads::unlink_modules(&path);
        if let Err(e) = git(root, &["worktree", "remove", "--force", &shown]) {
            done.kept.push(Cleaned { kind: "worktree".into(), path: shown, why: format!("could not remove it: {e:#}"), count: 1 });
            continue;
        }
        let branch = w.branch.clone().unwrap_or_default();
        for (_, _, id) in &mine {
            if !branch.is_empty() {
                gone.push(((*id).clone(), branch.clone()));
            }
        }
        let why = if branch.is_empty() { "everything in it is pushed".to_owned() } else { format!("{branch} is pushed") };
        done.removed.push(Cleaned { kind: "worktree".into(), path: shown, why, count: 1 });
    }
    git(root, &["worktree", "prune"]).warn_on_err("git worktree prune");
    sweep_attempt_leftovers(root, &common.join("hub-attempts"), folders, Duration::from_millis(QUIET_MS as u64), done);
    sweep_branches(root, defaults, done);
    gone
}

/// Removes what picking an attempt could not delete yet (`attempts::remove`) from `dir`, the
/// repository's `.git/hub-attempts`. A folder goes only when all of these hold:
/// - git no longer has it as a worktree: it is not in git's list (read here; when git cannot list
///   the worktrees nothing goes, a failed listing says nothing) and its `.git` link is gone or points at an admin folder under `.git/worktrees/` that no
///   longer exists, which is what a `git worktree remove` that failed half-way leaves;
/// - Hub wrote its leftover note (`attempts::LEFTOVER`) there, naming the ref its work is kept
///   under and the attempt's thread, and that ref is in the repository;
/// - no thread's folder is in it, other than the attempt's own thread while that is quiet (a
///   dropped attempt keeps its folder name when archived);
/// - the note is older than `age`: written once, when the folder was given up, so a live folder
///   never looks old.
///
/// Anything else in `dir` that is not a worktree is listed under `done.kept` with why.
pub(crate) fn sweep_attempt_leftovers(root: &Path, dir: &Path, folders: &[(PathBuf, bool, String)], age: Duration, done: &mut Cleanup) {
    let canon = |p: &Path| dev_threads::canon(p).unwrap_or_else(|_| p.to_path_buf());
    let Some(listed) = worktrees(root) else { return };
    let listed: Vec<PathBuf> = listed.iter().map(|w| canon(&w.path)).collect();
    let mut n = 0;
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = canon(&e.path());
        if !e.file_type().is_ok_and(|t| t.is_dir()) || listed.contains(&path) || linked(&path) {
            continue;
        }
        let shown = path.display().to_string();
        let mut keep = |why: String| done.kept.push(Cleaned { kind: "worktree".into(), path: shown.clone(), why, count: 1 });
        let note = path.join(crate::attempts::LEFTOVER);
        let Some((kept_at, owner)) = std::fs::read_to_string(&note).ok().and_then(|t| crate::attempts::read_leftover(&t)) else {
            keep("not a worktree, and Hub left no note of where its work is; look at it yourself".into());
            continue;
        };
        if git(root, &["rev-parse", "--verify", "--quiet", &kept_at]).is_err() {
            keep(format!("its work is not under {kept_at}"));
            continue;
        }
        let on_it = folders.iter().any(|(cwd, busy, id)| canon(cwd).starts_with(&path) && (*busy || *id != owner));
        if on_it {
            keep("a thread is still on it".into());
            continue;
        }
        if !std::fs::metadata(&note).is_ok_and(|m| older_than(&m, age)) {
            continue;
        }
        match std::fs::remove_dir_all(&path) {
            Ok(()) => n += 1,
            Err(e) => keep(format!("could not remove it: {e}")),
        }
    }
    if n > 0 {
        let why = "left by picking an attempt; its work is under refs/hub/".into();
        done.removed.push(Cleaned { kind: "worktree".into(), path: dir.display().to_string(), why, count: n });
    }
}

/// Whether `tree` is still linked as a worktree: its `.git` is a folder, or a `gitdir:` file
/// pointing at an admin folder that exists.
fn linked(tree: &Path) -> bool {
    let dot = tree.join(".git");
    if dot.is_dir() {
        return true;
    }
    let Ok(text) = std::fs::read_to_string(&dot) else { return false };
    text.lines().find_map(|l| l.strip_prefix("gitdir:")).is_some_and(|d| {
        let d = Path::new(d.trim());
        if d.is_absolute() { d.is_dir() } else { tree.join(d).is_dir() }
    })
}

/// Deletes local branches whose every commit is on a remote, unless a worktree has them out or
/// they are the default branch.
fn sweep_branches(root: &Path, defaults: &[String], done: &mut Cleanup) {
    // Without the list Hub cannot tell which branches a worktree has out.
    let Some(listed) = worktrees(root) else { return };
    let out: Vec<String> = listed.into_iter().filter_map(|w| w.branch).collect();
    let head = git(root, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"]).unwrap_or_default();
    let head = head.strip_prefix("origin/").unwrap_or_default().to_owned();
    let Ok(list) = git(root, &["for-each-ref", "--format=%(refname:short)", "refs/heads/"]) else { return };
    let repo = root.display().to_string();
    for branch in list.lines().map(str::trim).filter(|b| !b.is_empty()) {
        if out.iter().any(|o| o == branch) || defaults.iter().any(|d| d == branch) || branch == head || branch == "hub-preview" {
            continue;
        }
        match unpushed(root, &format!("refs/heads/{branch}")) {
            Some(0) => {
                if git(root, &["branch", "--quiet", "-D", branch]).is_ok() {
                    done.removed.push(Cleaned { kind: "branch".into(), path: format!("{branch} in {repo}"), why: "every commit is on the remote".into(), count: 1 });
                }
            }
            Some(n) if branch.starts_with("hub-dev/") || branch.starts_with("hub/") || crate::engine_home::AGENT_BRANCHES.iter().any(|p| branch.starts_with(p)) => {
                let why = format!("{n} {} not pushed", if n == 1 { "commit" } else { "commits" });
                done.kept.push(Cleaned { kind: "branch".into(), path: format!("{branch} in {repo}"), why, count: 1 });
            }
            _ => {}
        }
    }
}

/// Whether `tree` is already a worktree of `clone`, as `git worktree list` has it.
fn is_worktree(clone: &Path, tree: &Path) -> bool {
    let want = dev_threads::canon(tree).unwrap_or_else(|_| tree.to_path_buf());
    worktrees(clone).into_iter().flatten().any(|w| dev_threads::canon(&w.path).unwrap_or_else(|_| w.path.clone()) == want)
}

/// Puts a removed thread worktree back at `tree` on `branch`. A folder already back is left as it is,
/// and a branch made since the check (another restore, or the thread's own start) is taken as it is.
fn restore(clone: &Path, tree: &Path, branch: &str, remote: &str, base: &str, link: bool) -> Result<()> {
    git(clone, &["worktree", "prune"]).warn_on_err("git worktree prune");
    let at = tree.display().to_string();
    let local = format!("refs/heads/{branch}");
    if !is_worktree(clone, tree) {
        let made = if git(clone, &["rev-parse", "--verify", "--quiet", &local]).is_ok() {
            git(clone, &["worktree", "add", "--quiet", &at, branch]).map(drop)
        } else {
            let theirs = format!("{remote}/{branch}");
            git(clone, &["fetch", "--quiet", "--no-tags", remote, &format!("+refs/heads/{branch}:refs/remotes/{theirs}")]).warn_on_err("fetch the thread's branch");
            let start = if git(clone, &["rev-parse", "--verify", "--quiet", &theirs]).is_ok() { theirs } else { format!("{remote}/{base}") };
            let start = if git(clone, &["rev-parse", "--verify", "--quiet", &start]).is_ok() { start } else { "HEAD".to_owned() };
            match git(clone, &["worktree", "add", "--quiet", "-b", branch, &at, &start]) {
                Ok(_) => {
                    git(tree, &["branch", "--quiet", "--unset-upstream"]).ignore("a branch started from HEAD has no upstream to unset");
                    Ok(())
                }
                Err(e) if git(clone, &["rev-parse", "--verify", "--quiet", &local]).is_ok() => {
                    git(clone, &["worktree", "add", "--quiet", &at, branch]).map(drop).or(Err(e))
                }
                Err(e) => Err(e),
            }
        };
        if let Err(e) = made
            && !is_worktree(clone, tree)
        {
            return Err(e);
        }
    }
    if link && let Err(e) = dev_threads::link_modules(clone, tree) {
        tracing::warn!("cleanup: the worktree has no packages yet: {e:#}");
    }
    Ok(())
}

fn older_than(meta: &std::fs::Metadata, age: Duration) -> bool {
    meta.modified().ok().and_then(|m| SystemTime::now().duration_since(m).ok()).is_some_and(|d| d >= age)
}

/// Removes Hub's own files and folders in the temp folder `dir` older than `age`. Answers how
/// many went and their bytes.
pub fn sweep_temp(dir: &Path, age: Duration) -> (u32, u64) {
    let (mut n, mut bytes) = (0, 0);
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let ours = TEMP_PREFIXES.iter().any(|p| name.starts_with(p)) || name.starts_with("hub-") && name.ends_with(".bundle");
        let Ok(meta) = e.metadata() else { continue };
        if !ours || !older_than(&meta, age) {
            continue;
        }
        let size = crate::disk::size(&e.path());
        let gone = if meta.is_dir() { std::fs::remove_dir_all(e.path()) } else { std::fs::remove_file(e.path()) };
        if gone.is_ok() {
            n += 1;
            bytes += size;
        }
    }
    (n, bytes)
}

/// Removes engine logs that stayed empty for longer than `age`; ones with something in them
/// stay, they are how a thread's end is traced.
pub fn sweep_empty_logs(dir: &Path, age: Duration) -> (u32, u64) {
    let mut n = 0;
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_file() && meta.len() == 0 && older_than(&meta, age) && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    (n, 0)
}

/// A temporary folder for a test, `hub-test-…` in the system's temp folder. Tests that need a
/// folder to outlive a `TempDir` use this instead of `tempdir().keep()`, which nothing ever
/// removed: the first call in a run removes the ones runs left more than a few hours ago, and
/// Hub's clean-up removes them after a day.
pub fn test_dir() -> PathBuf {
    static SWEPT: std::sync::Once = std::sync::Once::new();
    let temp = std::env::temp_dir();
    SWEPT.call_once(|| {
        for e in std::fs::read_dir(&temp).into_iter().flatten().flatten() {
            if e.file_name().to_string_lossy().starts_with("hub-test-") && e.metadata().is_ok_and(|m| older_than(&m, Duration::from_secs(6 * 3600))) {
                crate::locked::clean_dir(e.path());
            }
        }
    });
    let dir = temp.join(format!("hub-test-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).expect("making a test folder");
    dir
}

/// A stand-in program for a test: the shell script `body` as an executable `dir/name`. `cp`
/// writes it in a process of its own, so this one never holds it open for writing: a test
/// beside this one that forks while such a handle is open carries it into its child, and running
/// the script then fails with "Text file busy" until that child has exec'd.
#[cfg(all(test, unix))]
pub(crate) fn test_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let source = dir.join(format!("{name}.src"));
    std::fs::write(&source, body).unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o755)).unwrap();
    let script = dir.join(name);
    let copied = std::process::Command::new("cp").arg(&source).arg(&script).status().unwrap();
    assert!(copied.success(), "cp could not write {}", script.display());
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

pub use crate::tempdirs::temp_outside_git;
#[cfg(test)]
pub(crate) use crate::tempdirs::tempdir_outside_git;

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        git(dir, args).unwrap();
    }

    fn commit(dir: &Path, file: &str) {
        std::fs::write(dir.join(file), file).unwrap();
        sh(dir, &["add", file]);
        sh(dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet", "-m", file]);
    }

    /// A clone of a bare remote with one commit on main, pushed.
    fn clone(tmp: &Path) -> PathBuf {
        let (remote, clone) = (tmp.join("remote.git"), tmp.join("hub"));
        std::fs::create_dir_all(&remote).unwrap();
        sh(&remote, &["init", "--quiet", "--bare", "-b", "main"]);
        sh(tmp, &["clone", "--quiet", &remote.display().to_string(), "hub"]);
        commit(&clone, "first");
        sh(&clone, &["push", "--quiet", "origin", "main"]);
        clone
    }

    fn worktree(clone: &Path, name: &str, branch: &str) -> PathBuf {
        let tree = dev_threads::folder(clone).join(name);
        std::fs::create_dir_all(tree.parent().unwrap()).unwrap();
        sh(clone, &["worktree", "add", "--quiet", "-b", branch, &tree.display().to_string(), "main"]);
        dev_threads::canon(&tree).unwrap()
    }

    #[test]
    fn only_what_is_all_pushed_goes() {
        let tmp = tempfile::tempdir().unwrap();
        let clone = dev_threads::canon(clone(tmp.path())).unwrap();
        let pushed = worktree(&clone, "aaaa0001", "hub-dev/pushed-1");
        commit(&pushed, "a.txt");
        sh(&pushed, &["push", "--quiet", "origin", "hub-dev/pushed-1"]);
        let dirty = worktree(&clone, "aaaa0002", "hub-dev/dirty-2");
        std::fs::write(dirty.join("half.txt"), "x").unwrap();
        let local = worktree(&clone, "aaaa0003", "hub-dev/local-3");
        commit(&local, "b.txt");
        let busy = worktree(&clone, "aaaa0004", "hub-dev/busy-4");
        // A branch pushed and left, and one with work only here.
        sh(&clone, &["branch", "claude/old", "main"]);
        sh(&clone, &["branch", "claude/mine", "hub-dev/local-3"]);

        let folders = vec![(pushed.clone(), false, "t1".to_owned()), (busy.clone(), true, "t4".to_owned())];
        let mut done = Cleanup::default();
        let gone = sweep_repo(&clone, &folders, &["main".to_owned()], &mut done);

        assert!(!pushed.exists(), "pushed and quiet: gone");
        assert_eq!(gone, vec![("t1".to_owned(), "hub-dev/pushed-1".to_owned())]);
        assert!(git(&clone, &["rev-parse", "--verify", "--quiet", "refs/heads/hub-dev/pushed-1"]).is_err(), "its branch too");
        assert!(git(&clone, &["rev-parse", "--verify", "--quiet", "refs/remotes/origin/hub-dev/pushed-1"]).is_ok(), "the remote one stays");
        assert!(dirty.join("half.txt").exists(), "uncommitted work stays");
        assert!(local.join("b.txt").exists(), "unpushed commits stay");
        assert!(busy.exists(), "a thread still on it");
        assert!(git(&clone, &["rev-parse", "--verify", "--quiet", "refs/heads/claude/old"]).is_err(), "a pushed branch goes");
        assert!(git(&clone, &["rev-parse", "--verify", "--quiet", "refs/heads/claude/mine"]).is_ok(), "an unpushed one stays");
        assert!(git(&clone, &["rev-parse", "--verify", "--quiet", "refs/heads/main"]).is_ok(), "the default branch stays");
        let kept: Vec<&str> = done.kept.iter().map(|k| k.why.as_str()).collect();
        assert!(kept.contains(&"1 uncommitted change") && kept.contains(&"1 commit not pushed") && kept.contains(&"a thread is still on it"), "{kept:?}");

        // The thread is written to again: its worktree comes back on its branch, from the remote.
        restore(&clone, &pushed, "hub-dev/pushed-1", "origin", "main", true).unwrap();
        assert_eq!(git(&pushed, &["branch", "--show-current"]).unwrap(), "hub-dev/pushed-1");
        assert!(pushed.join("a.txt").exists());
    }

    #[test]
    fn a_worktree_hub_did_not_make_is_only_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let clone = dev_threads::canon(clone(tmp.path())).unwrap();
        let other = tmp.path().join("clean");
        sh(&clone, &["worktree", "add", "--quiet", "--detach", &other.display().to_string(), "main"]);
        let mut done = Cleanup::default();
        sweep_repo(&clone, &[], &["main".to_owned()], &mut done);
        assert!(other.exists());
        assert!(done.kept.iter().any(|k| k.why.starts_with("not made by Hub")));
    }

    #[test]
    fn a_merged_branch_deleted_on_the_remote_comes_back_from_main() {
        let tmp = tempfile::tempdir().unwrap();
        let clone = dev_threads::canon(clone(tmp.path())).unwrap();
        let tree = dev_threads::folder(&clone).join("bbbb0001");
        restore(&clone, &tree, "hub-dev/gone-1", "origin", "main", true).unwrap();
        assert_eq!(git(&tree, &["branch", "--show-current"]).unwrap(), "hub-dev/gone-1");
        assert!(tree.join("first").exists());
    }

    #[test]
    fn restoring_a_worktree_that_is_back_is_a_no_op() {
        let tmp = tempfile::tempdir().unwrap();
        let clone = dev_threads::canon(clone(tmp.path())).unwrap();
        let tree = dev_threads::folder(&clone).join("cccc0001");
        restore(&clone, &tree, "hub-dev/twice-1", "origin", "main", true).unwrap();
        restore(&clone, &tree, "hub-dev/twice-1", "origin", "main", true).unwrap();
        assert_eq!(git(&tree, &["branch", "--show-current"]).unwrap(), "hub-dev/twice-1");
        assert!(tree.join("first").exists());
    }

    #[test]
    fn a_branch_with_no_worktree_is_restored_onto_that_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let clone = dev_threads::canon(clone(tmp.path())).unwrap();
        sh(&clone, &["branch", "hub-dev/existing-1", "main"]);
        let tree = dev_threads::folder(&clone).join("dddd0001");
        restore(&clone, &tree, "hub-dev/existing-1", "origin", "main", true).unwrap();
        assert_eq!(git(&tree, &["branch", "--show-current"]).unwrap(), "hub-dev/existing-1");
    }

    #[test]
    fn only_hubs_old_temporary_files_go() {
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path();
        std::fs::create_dir_all(t.join("hub-page-check-1/Default")).unwrap();
        std::fs::write(t.join("hub-page-check-1/Default/x"), "abc").unwrap();
        std::fs::write(t.join("hub-carry-1.bundle"), "ab").unwrap();
        std::fs::write(t.join("hub-1.bundle"), "a").unwrap();
        std::fs::create_dir_all(t.join("someone-else")).unwrap();
        std::fs::write(t.join("hub.db"), "").unwrap();
        assert_eq!(sweep_temp(t, Duration::from_secs(3600)), (0, 0), "nothing old yet");
        assert_eq!(sweep_temp(t, Duration::ZERO), (3, 6));
        assert!(t.join("someone-else").exists() && t.join("hub.db").exists());

        let logs = t.join("engines");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("a.log"), "").unwrap();
        std::fs::write(logs.join("b.log"), "ended").unwrap();
        assert_eq!(sweep_empty_logs(&logs, Duration::ZERO).0, 1);
        assert!(logs.join("b.log").exists());
    }

    #[test]
    fn test_folders_are_named_for_the_clean_up() {
        let dir = test_dir();
        assert!(dir.file_name().unwrap().to_string_lossy().starts_with("hub-test-"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
