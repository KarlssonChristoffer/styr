//! Dev mode, step 2: a git worktree per thread. A thread started in Hub (dev) gets a worktree of
//! the clone of its own, `<clone>/.hub-threads/<id>`, on a branch of its own that starts from
//! the branch the updater follows, so two threads on Hub never edit the same files and the
//! preview never has to park anyone's work. The dev window (and the dev core) can show any one
//! thread's worktree, live, instead of the clone.
//!
//! The worktrees share the clone's packages (`app/node_modules` is a link to the clone's) and
//! build cache (`target/`, set for them in `.hub-threads/.cargo/config.toml`), so a new thread
//! costs no install and no build from scratch. The cache has a cap: past it, the oldest parts go.

use crate::locked::ResultExt;
use crate::locked::Ignore;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::dev::{self, git};
use crate::hub::Hub;
use crate::model::{DevShow, DevStatus, DevThread, Thread};

/// `off` when new threads in Hub (dev) work in the clone itself.
const WORKTREES_KEY: &str = "dev.worktrees";
/// The thread whose worktree the dev window shows.
pub(crate) const SHOW_KEY: &str = "dev.show";
/// The build cache's cap in GB, when not the default.
const CAP_KEY: &str = "dev.cache_gb";
const DEFAULT_CAP_GB: u64 = 10;
pub(crate) const FOLDER: &str = ".hub-threads";

/// Where the threads' worktrees go: in the clone, which git is told to leave alone, and not in
/// `.git`, which Vite neither serves nor watches.
pub fn folder(clone: &Path) -> PathBuf {
    clone.join(FOLDER)
}

impl Hub {
    pub(crate) async fn dev_worktrees_on(&self) -> bool {
        !matches!(self.store.get_value(WORKTREES_KEY).await, Ok(Some(v)) if v == "off")
    }

    pub async fn dev_set_worktrees(self: &Arc<Self>, on: bool) -> Result<DevStatus> {
        if on {
            self.store.take_value(WORKTREES_KEY).await?;
        } else {
            self.store.set_value(WORKTREES_KEY, "off").await?;
        }
        self.dev_status().await
    }

    /// Keeps the clone's build cache under its cap; also run when the core starts.
    pub(crate) async fn trim_dev_cache(&self) {
        let Ok(clone) = self.dev_dir().await else { return };
        let (target, cap) = (clone.join("target"), self.dev_cache_cap().await);
        let worktrees = folder(&clone);
        tokio::task::spawn_blocking(move || {
            // A worktree's own `target` is a build that missed the shared cache (a thread that
            // set its own, or a worktree from before the config): only gigabytes.
            for e in std::fs::read_dir(&worktrees).into_iter().flatten().flatten() {
                let own = e.path().join("target");
                if own.join("CACHEDIR.TAG").is_file() {
                    tracing::info!("dev: removing the separate build cache {}", own.display());
                    crate::locked::clean_dir(&own);
                }
            }
            if target.is_dir() {
                crate::dev::trim_cache(&target, cap);
            }
        })
        .await.warn_on_err("trim the build caches");
    }

    pub(crate) async fn dev_cache_cap(&self) -> u64 {
        let gb = self.store.get_value(CAP_KEY).await.ok().flatten().and_then(|v| v.trim().parse().ok()).unwrap_or(DEFAULT_CAP_GB);
        gb.max(2) << 30
    }

    /// For a new thread in `cwd`: when that is the clone (or a folder in it), a worktree of its
    /// own, answered as the folder the thread works in. Nothing when worktrees are off, it is
    /// another folder, or the worktree could not be made (the thread then works in `cwd`).
    pub(crate) async fn dev_thread_folder(&self, cwd: &str, id: &str, title: &str) -> Option<String> {
        // A dev core shares the clone with this core; its threads are for trying things out.
        if self.config.bind.port() == dev::CORE_PORT || !self.dev_worktrees_on().await {
            return None;
        }
        let clone = self.dev_dir().await.ok()?;
        let (clone, cwd) = (canon(&clone).ok()?, canon(cwd).ok()?);
        let rel = cwd.strip_prefix(&clone).ok()?.to_owned();
        if rel.starts_with(".git") || rel.starts_with(FOLDER) || !clone.join(".git").is_dir() {
            return None;
        }
        let (remote, base) = (self.config.update.remote.clone(), self.config.update.branch.clone());
        // The newest base, if the remote answers quickly; otherwise the one fetched last.
        {
            let (clone, remote, base) = (clone.clone(), remote.clone(), base.clone());
            let fetch = tokio::task::spawn_blocking(move || {
                git(&clone, &["fetch", "--quiet", "--no-tags", &remote, &format!("+refs/heads/{base}:refs/remotes/{remote}/{base}")])
            });
            tokio::time::timeout(Duration::from_secs(15), fetch).await.ignore("offline or slow, the worktree starts from what was last fetched");
        }
        let branch = dev::branch_name(title, crate::model::now_ms());
        let (at, id) = (clone.clone(), id.to_owned());
        let made = tokio::task::spawn_blocking(move || add(&at, &remote, &base, &id, &branch)).await;
        // Every worktree builds the workspace crates again into the shared cache; it was 28 GB
        // on the PC. Kept under its cap before the new thread starts building.
        self.trim_dev_cache().await;
        match made {
            Ok(Ok(tree)) => {
                let folder = if rel.as_os_str().is_empty() { tree } else { tree.join(rel) };
                Some(folder.display().to_string())
            }
            Ok(Err(e)) => {
                tracing::warn!("dev: no worktree for the new thread, it works in the clone: {e:#}");
                None
            }
            Err(_) => None,
        }
    }

    /// The threads working in worktrees of the clone, newest first.
    pub(crate) async fn dev_threads(&self) -> Result<Vec<DevThread>> {
        let clone = self.dev_dir().await?;
        let Ok(root) = canon(folder(&clone)) else { return Ok(Vec::new()) };
        let upstream = format!("{}/{}", self.config.update.remote, self.config.update.branch);
        let mut threads: Vec<Thread> = self.store.threads().await?.into_iter().filter(|t| t.host.is_none() && Path::new(&t.cwd).starts_with(&root)).collect();
        threads.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        tokio::task::spawn_blocking(move || {
            threads
                .into_iter()
                .filter_map(|t| {
                    let tree = worktree_of(&root, Path::new(&t.cwd))?;
                    if !tree.join(".git").exists() {
                        return None;
                    }
                    let (branch, changed, ahead) = crate::dev::git_state(&tree, &upstream);
                    Some(DevThread {
                        thread_id: t.id,
                        title: t.title,
                        status: t.status,
                        path: tree.display().to_string(),
                        branch: branch.unwrap_or_default(),
                        changed,
                        ahead,
                    })
                })
                .collect()
        })
        .await
        .context("reading the threads' worktrees")
    }

    /// The folder the dev window shows: the worktree of the thread picked, or the clone. The
    /// thread's id comes along when it is a worktree.
    pub(crate) async fn dev_shown(&self) -> Result<(PathBuf, Option<String>)> {
        let clone = self.dev_dir().await?;
        if let Some(id) = self.store.get_value(SHOW_KEY).await?
            && let Some(thread) = self.store.thread(&id).await?
            && let Ok(root) = canon(folder(&clone))
            && let Some(tree) = worktree_of(&root, Path::new(&thread.cwd))
            && tree.join(".git").exists()
        {
            return Ok((tree, Some(id)));
        }
        Ok((clone, None))
    }

    /// Shows a thread's worktree in the dev window, or the clone again. The live UI restarts in
    /// that folder and the dev core, when on, rebuilds from it.
    pub async fn dev_show(self: &Arc<Self>, show: DevShow) -> Result<DevStatus> {
        match show.thread {
            Some(id) => {
                let clone = self.dev_dir().await?;
                let thread = self.store.thread(&id).await?.context("no such thread")?;
                let root = canon(folder(&clone)).context("no thread in Hub (dev) has a worktree yet")?;
                let tree = worktree_of(&root, Path::new(&thread.cwd)).context("that thread does not work in a worktree of the clone")?;
                let (c, t) = (clone.clone(), tree.clone());
                tokio::task::spawn_blocking(move || link_modules(&c, &t)).await??;
                self.store.set_value(SHOW_KEY, &id).await?;
            }
            None => {
                self.store.take_value(SHOW_KEY).await?;
            }
        }
        self.dev_restart_ui().await?;
        self.dev_status().await
    }

    /// Removes a thread's worktree. Uncommitted work is kept under `refs/hub/dev-threads/`, and
    /// the branch stays when it has commits the updater's branch lacks.
    pub async fn dev_remove_worktree(self: &Arc<Self>, id: &str) -> Result<DevStatus> {
        let clone = self.dev_dir().await?;
        let thread = self.store.thread(id).await?.context("no such thread")?;
        if thread.status == crate::model::ThreadStatus::Running {
            bail!("the thread is still working; stop it first");
        }
        let root = canon(folder(&clone)).context("there are no worktrees")?;
        let tree = worktree_of(&root, Path::new(&thread.cwd)).context("that thread does not work in a worktree of the clone")?;
        if self.store.get_value(SHOW_KEY).await?.as_deref() == Some(id) {
            self.store.take_value(SHOW_KEY).await?;
            self.dev_restart_ui().await?;
        }
        let upstream = format!("{}/{}", self.config.update.remote, self.config.update.branch);
        let keep = format!("dev-threads/{}", crate::attempts::branch_stem(id));
        let branch = tokio::task::spawn_blocking(move || remove(&clone, &tree, &upstream, &keep)).await??;
        // Written to again, the thread gets its worktree back on this branch.
        if !branch.is_empty() {
            self.store.set_value(&format!("dev.branch.{id}"), &branch).await?;
        }
        self.dev_status().await
    }
}

/// The folder's full path, without the `\\?\` Windows puts before it: that kind of path is
/// the thread's folder, and tools like npm and cmd do not take it.
pub(crate) fn canon(path: impl AsRef<Path>) -> std::io::Result<PathBuf> {
    let full = std::fs::canonicalize(path)?;
    let text = full.display().to_string();
    Ok(match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => full,
    })
}

/// The worktree a folder under `root` (the canonical `.hub-threads`) is in.
fn worktree_of(root: &Path, cwd: &Path) -> Option<PathBuf> {
    let cwd = canon(cwd).unwrap_or_else(|_| cwd.to_owned());
    let name = cwd.strip_prefix(root).ok()?.components().next()?;
    Some(root.join(name))
}

/// Adds the worktree for thread `id` on a new `branch` from `remote/base` (or the clone's HEAD
/// when that is missing), with the clone's packages and build cache.
fn add(clone: &Path, remote: &str, base: &str, id: &str, branch: &str) -> Result<PathBuf> {
    let root = folder(clone);
    std::fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
    exclude(clone, &format!("/{FOLDER}/"))?;
    cargo_config(clone)?;
    let tree = root.join(crate::attempts::branch_stem(id));
    let upstream = format!("{remote}/{base}");
    let start = if git(clone, &["rev-parse", "--verify", "--quiet", &upstream]).is_ok() { upstream } else { "HEAD".to_owned() };
    git(clone, &["worktree", "prune"]).warn_on_err("git worktree prune");
    git(clone, &["worktree", "add", "--quiet", "-b", branch, &tree.display().to_string(), &start])?;
    git(&tree, &["branch", "--quiet", "--unset-upstream"]).ignore("a branch started from HEAD has no upstream to unset");
    if let Err(e) = link_modules(clone, &tree) {
        tracing::warn!("dev: the worktree has no packages yet: {e:#}");
    }
    Ok(tree)
}

/// Cargo in any worktree builds into the clone's `target/`, which a config beside the worktrees
/// says: cargo looks for one in every folder above where it runs.
fn cargo_config(clone: &Path) -> Result<()> {
    lean_cargo_config(&folder(clone), "the threads' worktrees share the clone's build cache.", &clone.join("target"))
}

/// Writes `<dir>/.cargo/config.toml` so cargo anywhere below `dir` builds into `target`, with no
/// incremental cache and no debug info: they made most of the gigabytes, and the test builds of a
/// thread or of a conflict fix gain little from them.
pub(crate) fn lean_cargo_config(dir: &Path, why: &str, target: &Path) -> Result<()> {
    let dir = dir.join(".cargo");
    std::fs::create_dir_all(&dir)?;
    let target = target.display().to_string().replace('\\', "/");
    let text = format!(
        "# Written by Hub: {why}\n[build]\ntarget-dir = '{target}'\nincremental = false\n\n[profile.dev]\ndebug = 0\n"
    );
    let path = dir.join("config.toml");
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Points the worktree's `app/node_modules` at the clone's, unless its packages differ (then
/// it gets its own, installed). Git is told to leave the link alone.
pub(crate) fn link_modules(clone: &Path, tree: &Path) -> Result<()> {
    let (link, modules) = (tree.join("app/node_modules"), clone.join("app/node_modules"));
    if link.exists() || !modules.is_dir() {
        return Ok(());
    }
    let lock = |dir: &Path| std::fs::read(dir.join("app/package-lock.json")).ok();
    if lock(clone) != lock(tree) {
        let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
        let log = folder(clone).join("npm.log");
        return dev::run(&tree.join("app"), npm, &["install"], &log);
    }
    exclude(clone, "/app/node_modules")?;
    make_link(&modules, &link)
}

/// Tells git in the clone and all its worktrees to leave `pattern` alone, without a change to
/// `.gitignore`.
pub(crate) fn exclude(clone: &Path, pattern: &str) -> Result<()> {
    let path = PathBuf::from(git(clone, &["rev-parse", "--git-common-dir"])?);
    let path = if path.is_absolute() { path } else { clone.join(path) }.join("info/exclude");
    let had = std::fs::read_to_string(&path).unwrap_or_default();
    if !had.lines().any(|l| l.trim() == pattern) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let sep = if had.is_empty() || had.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&path, format!("{had}{sep}{pattern}\n")).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

#[cfg(unix)]
fn make_link(to: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(to, link).with_context(|| format!("linking {} to {}", link.display(), to.display()))
}

/// A junction, which needs no admin rights.
#[cfg(windows)]
fn make_link(to: &Path, link: &Path) -> Result<()> {
    // A junction keeps its target as written, and `/` is no separator there.
    let back = |p: &Path| p.display().to_string().replace('/', "\\");
    let out = dev::command("cmd", link.parent().unwrap_or(link))
        .args(["/C", "mklink", "/J"])
        .arg(back(link))
        .arg(back(to))
        .output()
        .context("running mklink")?;
    if !out.status.success() {
        bail!("mklink /J failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

/// Takes the packages link away without touching what it points at.
pub(crate) fn unlink_modules(tree: &Path) {
    let link = tree.join("app/node_modules");
    if std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink() || cfg!(windows) && is_junction(&link)) {
        std::fs::remove_file(&link).or_else(|_| std::fs::remove_dir(&link)).warn_on_err("remove the node_modules link");
    }
}

#[cfg(windows)]
fn is_junction(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const REPARSE_POINT: u32 = 0x400;
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_attributes() & REPARSE_POINT != 0)
}

#[cfg(not(windows))]
fn is_junction(_: &Path) -> bool {
    false
}

fn remove(clone: &Path, tree: &Path, upstream: &str, keep: &str) -> Result<String> {
    let branch = crate::git::run(tree, &["branch", "--show-current"]).map(|b| b.trim().to_owned()).unwrap_or_default();
    if let Ok(s) = crate::checkpoint::snapshot(tree, "Hub: thread's worktree as it was") {
        git(clone, &["update-ref", &format!("refs/hub/{keep}"), &s.commit]).warn_on_err("keep the worktree as a ref");
    }
    // First, so removing the worktree cannot reach into the clone's packages.
    unlink_modules(tree);
    git(clone, &["worktree", "remove", "--force", &tree.display().to_string()])?;
    git(clone, &["worktree", "prune"]).warn_on_err("git worktree prune");
    if !branch.is_empty() && git(clone, &["rev-list", "--count", &format!("{upstream}..{branch}")]).is_ok_and(|n| n == "0") {
        git(clone, &["branch", "--quiet", "-D", &branch]).warn_on_err("delete the merged branch");
    }
    Ok(branch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        git(dir, args).unwrap();
    }

    fn clone_with_remote(tmp: &Path) -> PathBuf {
        let (remote, clone) = (tmp.join("remote.git"), tmp.join("hub"));
        std::fs::create_dir_all(&remote).unwrap();
        sh(&remote, &["init", "--quiet", "--bare", "-b", "main"]);
        sh(tmp, &["clone", "--quiet", &remote.display().to_string(), "hub"]);
        std::fs::create_dir_all(clone.join("app/node_modules/pkg")).unwrap();
        std::fs::write(clone.join("app/node_modules/pkg/index.js"), "x").unwrap();
        std::fs::write(clone.join(".gitignore"), "node_modules/\n/target\n").unwrap();
        std::fs::write(clone.join("app/package-lock.json"), "{}").unwrap();
        sh(&clone, &["add", "-A"]);
        sh(&clone, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet", "-m", "first"]);
        sh(&clone, &["push", "--quiet", "origin", "main"]);
        clone
    }

    #[test]
    fn a_thread_gets_a_worktree_with_the_clones_packages_and_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let clone = clone_with_remote(tmp.path());
        // The clone is busy with something else: the thread starts from the remote's main.
        std::fs::write(clone.join("half-done.txt"), "x").unwrap();

        let tree = add(&clone, "origin", "main", "0192-abcd-ef01-2345", "hub-dev/make-it-green-1").unwrap();
        assert_eq!(tree, folder(&clone).join("ef012345"));
        assert_eq!(git(&tree, &["branch", "--show-current"]).unwrap(), "hub-dev/make-it-green-1");
        assert!(!tree.join("half-done.txt").exists());
        assert!(tree.join("app/node_modules/pkg/index.js").exists(), "packages are linked");
        assert_eq!(git(&tree, &["status", "--porcelain"]).unwrap(), "", "git leaves the link alone");
        assert_eq!(git(&clone, &["status", "--porcelain"]).unwrap(), "?? half-done.txt", "and the worktrees in the clone");
        let config = std::fs::read_to_string(folder(&clone).join(".cargo/config.toml")).unwrap();
        assert!(config.contains(&clone.join("target").display().to_string().replace('\\', "/")));
        assert!(config.contains("incremental = false") && config.contains("debug = 0"));

        let root = canon(folder(&clone)).unwrap();
        assert_eq!(worktree_of(&root, &tree.join("app")), Some(canon(&tree).unwrap()));
        assert_eq!(worktree_of(&root, &clone), None);
    }

    #[test]
    fn removing_keeps_work_and_never_touches_the_clones_packages() {
        let tmp = tempfile::tempdir().unwrap();
        let clone = clone_with_remote(tmp.path());
        let tree = add(&clone, "origin", "main", "t1", "hub-dev/empty-1").unwrap();
        remove(&clone, &tree, "origin/main", "dev-threads/t1").unwrap();
        assert!(!tree.exists());
        assert!(clone.join("app/node_modules/pkg/index.js").exists());
        assert!(git(&clone, &["rev-parse", "--verify", "--quiet", "hub-dev/empty-1"]).is_err(), "a branch with nothing on it goes");

        let tree = add(&clone, "origin", "main", "t2", "hub-dev/work-2").unwrap();
        std::fs::write(tree.join("work.txt"), "done").unwrap();
        sh(&tree, &["add", "work.txt"]);
        sh(&tree, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--quiet", "-m", "work"]);
        std::fs::write(tree.join("more.txt"), "half").unwrap();
        remove(&clone, &tree, "origin/main", "dev-threads/t2").unwrap();
        assert!(git(&clone, &["rev-parse", "--verify", "--quiet", "hub-dev/work-2"]).is_ok(), "a branch with commits stays");
        assert!(git(&clone, &["cat-file", "-e", "refs/hub/dev-threads/t2:more.txt"]).is_ok(), "uncommitted work is kept");
        assert!(clone.join("app/node_modules/pkg/index.js").exists());
    }
}
