//! Keeps each project's default branch current, after T3 Code's "Automatically pull": every few
//! minutes a project folder that sits on its default branch, clean, with nothing of its own to
//! push, and no thread working in it, is fast-forwarded to its upstream. Anything else is left
//! alone, so new threads start on fresh code and no work is ever touched.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::hub::Hub;
use crate::model::ThreadStatus;

const EVERY: Duration = Duration::from_secs(10 * 60);

pub async fn keep_current(hub: Arc<Hub>) {
    if !hub.config.auto_pull {
        return;
    }
    // Not in the first minutes: startup has enough to do.
    tokio::time::sleep(Duration::from_secs(90)).await;
    loop {
        let projects = hub.store.projects().await.unwrap_or_default();
        let busy: Vec<String> = hub
            .store
            .threads()
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|t| matches!(t.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou | ThreadStatus::InTerminal))
            .map(|t| t.cwd)
            .collect();
        for p in projects {
            if busy.iter().any(|cwd| Path::new(cwd).starts_with(&p.path)) {
                continue;
            }
            let path = p.path.clone();
            if let Ok(Pulled::Moved(n)) = tokio::task::spawn_blocking(move || pull(Path::new(&path))).await {
                tracing::info!(project = %p.path, commits = n, "fast-forwarded the default branch");
            }
        }
        tokio::time::sleep(EVERY).await;
    }
}

#[derive(Debug, PartialEq)]
pub enum Pulled {
    /// Moved forward this many commits.
    Moved(u32),
    /// Already current.
    Current,
    /// Left alone: why, for tests and logs.
    Skipped(&'static str),
}

/// Fast-forwards `root` to its upstream when it is on the default branch with nothing of its own.
pub fn pull(root: &Path) -> Pulled {
    if !root.join(".git").exists() {
        return Pulled::Skipped("not a repository");
    }
    let Some(branch) = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"]) else {
        return Pulled::Skipped("detached");
    };
    if branch.trim() != crate::github::default_branch(root) {
        return Pulled::Skipped("not on the default branch");
    }
    if git(root, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).is_none() {
        return Pulled::Skipped("no upstream");
    }
    if !git(root, &["status", "--porcelain=v1", "--untracked-files=normal"]).unwrap_or_default().trim().is_empty() {
        return Pulled::Skipped("local changes");
    }
    if git(root, &["fetch", "--quiet", "--no-tags"]).is_none() {
        return Pulled::Skipped("fetch failed");
    }
    let count = |range: &str| git(root, &["rev-list", "--count", range]).and_then(|n| n.trim().parse::<u32>().ok());
    if count("@{u}..HEAD").unwrap_or(1) > 0 {
        return Pulled::Skipped("local commits");
    }
    match count("HEAD..@{u}") {
        Some(0) => Pulled::Current,
        Some(n) if git(root, &["merge", "--ff-only", "--quiet", "@{u}"]).is_some() => Pulled::Moved(n),
        _ => Pulled::Skipped("could not fast-forward"),
    }
}

/// Git that never asks for anything and never hangs: see [`crate::git::output`].
fn git(cwd: &Path, args: &[&str]) -> Option<String> {
    crate::git::run(cwd, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").args(args).current_dir(dir).output().unwrap().status.success();
        assert!(ok, "git {args:?}");
    }

    fn commit(dir: &Path, file: &str) {
        std::fs::write(dir.join(file), file).unwrap();
        sh(dir, &["add", "."]);
        sh(dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", file]);
    }

    /// An "origin" with one commit, and a clone of it on main.
    fn pair() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        sh(&origin, &["init", "-q", "-b", "main"]);
        commit(&origin, "a");
        sh(dir.path(), &["clone", "-q", "origin", "clone"]);
        let clone = dir.path().join("clone");
        (dir, origin, clone)
    }

    #[test]
    fn fast_forwards_a_clean_default_branch() {
        let (_dir, origin, clone) = pair();
        assert_eq!(pull(&clone), Pulled::Current);
        commit(&origin, "b");
        commit(&origin, "c");
        assert_eq!(pull(&clone), Pulled::Moved(2));
        assert!(clone.join("c").exists());
    }

    #[test]
    fn leaves_work_alone() {
        let (_dir, origin, clone) = pair();
        commit(&origin, "b");
        std::fs::write(clone.join("draft"), "x").unwrap();
        assert_eq!(pull(&clone), Pulled::Skipped("local changes"));
        std::fs::remove_file(clone.join("draft")).unwrap();
        commit(&clone, "mine");
        assert_eq!(pull(&clone), Pulled::Skipped("local commits"));
        sh(&clone, &["checkout", "-q", "-b", "feature"]);
        assert_eq!(pull(&clone), Pulled::Skipped("not on the default branch"));
    }
}
