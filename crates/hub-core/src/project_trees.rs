//! A git worktree per thread, for any project that asks for it. With the switch on (the project
//! page), a new thread in a project's repository works on a copy of the latest main in its own
//! folder and branch, `<repo>/.hub-threads/<id>` on `hub/<title>-<stamp>`, so two threads never
//! edit the same files. It is the same idea as the worktrees of threads on Hub itself
//! (`dev_threads.rs`, which also share packages and a build cache), without those.
//!
//! The clean-up after agent work (`janitor.rs`) already looks in `.hub-threads` of every
//! project's repository: a worktree goes once everything in it is pushed, and a thread written
//! to again gets it back on its branch (`bring_back_folder`).

use crate::locked::ResultExt;
use crate::locked::Ignore;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::dev::{self, git};
use crate::dev_threads::{self, FOLDER, canon};
use crate::hub::Hub;
use crate::model::{ProjectTrees, ProjectTreesSet};

/// The projects with worktrees on, by their repository's folder.
const KEY: &str = "trees.projects";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Pref {
    on: bool,
    /// What new threads are cut from, as `remote/branch`; Hub finds one when it is not set.
    #[serde(default)]
    base: Option<String>,
}

/// The repository `dir` is in, when it is the repository itself and not a linked worktree of one.
fn repo_root(dir: &Path) -> Option<PathBuf> {
    let root = PathBuf::from(git(dir, &["rev-parse", "--show-toplevel"]).ok()?);
    let (dotgit, common) = (git(&root, &["rev-parse", "--absolute-git-dir"]).ok()?, git(&root, &["rev-parse", "--git-common-dir"]).ok()?);
    let common = if Path::new(&common).is_absolute() { PathBuf::from(common) } else { root.join(common) };
    let same = canon(&dotgit).ok()? == canon(&common).ok()?;
    same.then(|| canon(&root).unwrap_or(root))
}

/// The branch the remote calls its default, as `remote/branch`, when this repository knows it.
fn default_base(root: &Path) -> Option<String> {
    git(root, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]).ok().filter(|s| !s.is_empty())
}

/// Where a new worktree starts: the base as the remote has it (fetched first), else what the
/// remote's default branch was when last fetched, else where the folder is now.
fn start_at(root: &Path, base: Option<&str>) -> String {
    let tries: Vec<String> = base.map(str::to_owned).into_iter().chain(default_base(root)).collect();
    for start in tries {
        if let Some((remote, branch)) = start.split_once('/') {
            git(root, &["fetch", "--quiet", "--no-tags", remote, &format!("+refs/heads/{branch}:refs/remotes/{remote}/{branch}")]).warn_on_err("fetch the base branch");
        }
        if git(root, &["rev-parse", "--verify", "--quiet", &format!("{start}^{{commit}}")]).is_ok() {
            return start;
        }
    }
    "HEAD".to_owned()
}

/// Adds the worktree of thread `id` on a new `branch`.
fn add(root: &Path, base: Option<&str>, id: &str, branch: &str) -> Result<PathBuf> {
    let parent = dev_threads::folder(root);
    std::fs::create_dir_all(&parent)?;
    dev_threads::exclude(root, &format!("/{FOLDER}/"))?;
    let tree = parent.join(crate::attempts::branch_stem(id));
    let start = start_at(root, base);
    git(root, &["worktree", "prune"]).warn_on_err("git worktree prune");
    git(root, &["worktree", "add", "--quiet", "-b", branch, &tree.display().to_string(), &start])?;
    git(&tree, &["branch", "--quiet", "--unset-upstream"]).ignore("a branch started from HEAD has no upstream to unset");
    Ok(tree)
}

impl Hub {
    async fn tree_prefs(&self) -> HashMap<String, Pref> {
        let text = self.store.get_value(KEY).await.ok().flatten().unwrap_or_default();
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Whether the dev clone is this repository: its threads have worktrees by the dev setting.
    async fn is_dev_clone(&self, root: &Path) -> bool {
        match self.dev_dir().await.ok().and_then(|d| canon(d).ok()) {
            Some(clone) => clone == root,
            None => false,
        }
    }

    /// What a project's page shows: whether new threads in it get a worktree each.
    pub async fn project_trees(&self, path: &str) -> Result<ProjectTrees> {
        let at = PathBuf::from(path);
        let Some(root) = tokio::task::spawn_blocking(move || repo_root(&at)).await.ok().flatten() else {
            return Ok(ProjectTrees { root: None, on: false, base: None, default_base: None });
        };
        let prefs = self.tree_prefs().await;
        let key = root.display().to_string();
        let on = if self.is_dev_clone(&root).await { self.dev_worktrees_on().await } else { prefs.get(&key).is_some_and(|p| p.on) };
        let default = tokio::task::spawn_blocking({
            let root = root.clone();
            move || default_base(&root)
        })
        .await
        .ok()
        .flatten();
        Ok(ProjectTrees { root: Some(key.clone()), on, base: prefs.get(&key).and_then(|p| p.base.clone()), default_base: default })
    }

    /// Turns worktrees on or off for a project, and sets what its threads are cut from.
    pub async fn set_project_trees(self: &Arc<Self>, set: ProjectTreesSet) -> Result<ProjectTrees> {
        let at = PathBuf::from(&set.path);
        let Some(root) = tokio::task::spawn_blocking(move || repo_root(&at)).await.ok().flatten() else {
            bail!("{} is not a git repository", set.path);
        };
        if self.is_dev_clone(&root).await {
            self.dev_set_worktrees(set.on).await?;
        } else {
            let mut prefs = self.tree_prefs().await;
            let base = set.base.clone().filter(|b| !b.trim().is_empty());
            prefs.insert(root.display().to_string(), Pref { on: set.on, base });
            self.store.set_value(KEY, &serde_json::to_string(&prefs)?).await?;
        }
        self.project_trees(&set.path).await
    }

    /// For a new thread in `cwd`: when its project has worktrees on, a worktree of its own,
    /// answered as the folder the thread works in. Nothing when they are off, the folder is not
    /// a repository (or is already a worktree), or the worktree could not be made, and the
    /// thread then works in `cwd`. Threads on Hub itself are `dev_thread_folder`'s.
    pub(crate) async fn project_thread_folder(&self, cwd: &str, id: &str, title: &str) -> Option<String> {
        let cwd = canon(cwd).ok()?;
        let root = {
            let at = cwd.clone();
            tokio::task::spawn_blocking(move || repo_root(&at)).await.ok()??
        };
        if self.is_dev_clone(&root).await {
            return None;
        }
        let pref = self.tree_prefs().await.remove(&root.display().to_string()).filter(|p| p.on)?;
        let rel = cwd.strip_prefix(&root).ok()?.to_owned();
        if rel.starts_with(".git") || rel.starts_with(FOLDER) {
            return None;
        }
        let branch = dev::branch_name(title, crate::model::now_ms()).replacen("hub-dev/", "hub/", 1);
        let (at, id) = (root.clone(), id.to_owned());
        let made = tokio::time::timeout(Duration::from_secs(30), tokio::task::spawn_blocking(move || add(&at, pref.base.as_deref(), &id, &branch))).await;
        match made {
            Ok(Ok(Ok(tree))) => Some(if rel.as_os_str().is_empty() { tree } else { tree.join(rel) }.display().to_string()),
            Ok(Ok(Err(e))) => {
                tracing::warn!("no worktree for the new thread, it works in the project's folder: {e:#}");
                None
            }
            _ => None,
        }
    }

    /// Where a removed worktree's thread starts again: what the project says, else the remote's
    /// default branch, else the folder as it is.
    pub(crate) async fn tree_base(&self, root: &Path) -> Option<String> {
        self.tree_prefs().await.remove(&root.display().to_string()).and_then(|p| p.base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        git(dir, args).unwrap();
    }

    #[test]
    fn a_worktree_starts_from_the_remotes_branch_and_leaves_the_folder_alone() {
        let origin = crate::janitor::test_dir();
        let repo = crate::janitor::test_dir();
        sh(&origin, &["init", "--quiet", "--bare", "-b", "main"]);
        sh(&repo, &["init", "--quiet", "-b", "main"]);
        sh(&repo, &["config", "user.email", "t@t"]);
        sh(&repo, &["config", "user.name", "t"]);
        std::fs::write(repo.join("a.txt"), "one").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "--quiet", "-m", "one"]);
        sh(&repo, &["remote", "add", "origin", &origin.display().to_string()]);
        sh(&repo, &["push", "--quiet", "origin", "main"]);
        sh(&repo, &["remote", "set-head", "origin", "main"]);
        // A local commit that is not pushed stays out of the new thread's copy.
        std::fs::write(repo.join("a.txt"), "two").unwrap();
        sh(&repo, &["commit", "--quiet", "-am", "two"]);

        let root = repo_root(&repo).unwrap();
        assert_eq!(default_base(&root).as_deref(), Some("origin/main"));
        let tree = add(&root, None, "0190aaaa-bbbb", "hub/try-1").unwrap();
        assert_eq!(std::fs::read_to_string(tree.join("a.txt")).unwrap(), "one");
        assert_eq!(git(&tree, &["branch", "--show-current"]).unwrap(), "hub/try-1");
        assert!(tree.starts_with(root.join(FOLDER)));
        // The worktree is not a repository root of its own for another thread to nest in.
        assert!(repo_root(&tree).is_none());
        assert!(git(&root, &["status", "--porcelain"]).unwrap().is_empty(), ".hub-threads is ignored without a change to .gitignore");
    }
}
