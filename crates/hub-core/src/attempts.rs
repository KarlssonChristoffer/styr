//! Several attempts at once: the same task in two to four git worktrees, each its own thread
//! (often on a different model), then the person picks the best one and it lands in their folder.
//!
//! Worktrees go in the repository's own git folder, `.git/hub-attempts/<group>-<n>`, on branch
//! `hub/attempt-<group>-<n>`, so nothing shows up next to the project and the disk is the one the
//! project is on. Each starts from the folder as it was, uncommitted changes included (the
//! snapshot taken for it), so an attempt sees what the person sees.

use crate::locked::ResultExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::checkpoint::{self, git};

/// One attempt's worktree.
#[derive(Debug, Clone)]
pub struct Worktree {
    /// The worktree's top folder.
    pub path: PathBuf,
    /// Where the thread works: the same folder inside the worktree as it had in the repository.
    pub cwd: PathBuf,
    pub branch: String,
}

/// Where attempts start from: the repository `cwd` is in, snapshotted now.
pub struct Base {
    pub root: PathBuf,
    /// The snapshot every attempt starts from and is compared with.
    pub commit: String,
}

/// Snapshots the repository `cwd` is in, for a group of attempts, kept as `refs/hub/attempts/<group>`.
pub fn base(cwd: &Path, group: &str) -> Result<Base> {
    let Some(s) = checkpoint::take(cwd, &format!("attempts/{group}"), &format!("Hub: attempts {group} start here"))? else {
        bail!("attempts need a git repository; this folder is not in one");
    };
    if s.head.is_none() {
        bail!("attempts need a first commit in the repository");
    }
    Ok(Base { root: s.root, commit: s.commit })
}

/// Adds worktree `n` of `group`, starting from `base` with the snapshot's files in place.
pub fn add(cwd: &Path, base: &Base, group: &str, n: usize) -> Result<Worktree> {
    let common = git(&base.root, &["rev-parse", "--git-common-dir"])?;
    let common = base.root.join(common.trim());
    let name = format!("{}-{n}", branch_stem(group));
    let path = common.join("hub-attempts").join(&name);
    let branch = format!("hub/attempt-{name}");
    let at = path.display().to_string();
    git(&base.root, &["worktree", "add", "-q", "-b", &branch, &at, "HEAD"])?;
    // The person's uncommitted changes too, left uncommitted.
    git(&path, &["read-tree", "-u", "--reset", &base.commit])?;
    git(&path, &["reset", "-q"])?;
    let rel = cwd.strip_prefix(&base.root).unwrap_or(Path::new(""));
    let cwd = if rel.as_os_str().is_empty() { path.clone() } else { path.join(rel) };
    Ok(Worktree { cwd, path, branch })
}

/// What an attempt has changed since `base`: files, lines added and lines removed.
pub fn stat(worktree: &Path, base: &str) -> Result<(u32, u32, u32)> {
    let files = checkpoint::diff_now(worktree, base)?;
    let (mut added, mut removed) = (0, 0);
    for f in &files {
        for l in f.diff.lines() {
            if l.starts_with('+') && !l.starts_with("+++") {
                added += 1;
            } else if l.starts_with('-') && !l.starts_with("---") {
                removed += 1;
            }
        }
    }
    Ok((files.len() as u32, added, removed))
}

/// Brings what the attempt in `worktree` changed since `base` into the repository at `root`, as
/// uncommitted changes. The attempt's state is kept under `refs/hub/<keep>` first.
pub fn apply(root: &Path, worktree: &Path, base: &str, keep: &str) -> Result<usize> {
    let done = checkpoint::snapshot(worktree, "Hub: picked attempt")?;
    git(root, &["update-ref", &format!("refs/hub/{keep}"), &done.commit])?;
    let patch = git(root, &["diff", "--binary", "--no-color", "--no-ext-diff", base, &done.commit])?;
    if patch.trim().is_empty() {
        return Ok(0);
    }
    let files = patch.lines().filter(|l| l.starts_with("diff --git ")).count();
    // Straight onto the files when it fits; otherwise merge where the folder changed meanwhile.
    if checkpoint::run(root, &["apply", "--whitespace=nowarn", "-"], None, Some(&patch)).is_err() {
        checkpoint::run(root, &["apply", "--3way", "--whitespace=nowarn", "-"], None, Some(&patch))
            .context("the attempt does not apply to the folder as it is now")?;
    }
    Ok(files)
}

/// How often removing a worktree is tried, and how long apart: a process that just ended can
/// hold its folder for a moment on Windows.
const REMOVE_TRIES: u32 = 5;
const REMOVE_PAUSE: std::time::Duration = std::time::Duration::from_millis(200);

/// Removes an attempt's worktree and branch; its work stays reachable through `refs/hub/`.
/// Answers whether the folder is gone. One that cannot be deleted yet (Windows does not delete a
/// folder a process still works in) is left for the clean-up (`janitor::sweep_attempt_leftovers`),
/// so picking never fails on it.
pub fn remove(root: &Path, worktree: &Worktree, keep: &str) -> bool {
    remove_with(root, worktree, keep, &|path| {
        // Git forgets the worktree even when it could not delete every file in it.
        match git(root, &["worktree", "remove", "--force", &path.display().to_string()]) {
            Err(e) if path.exists() => std::fs::remove_dir_all(path).with_context(|| format!("{e:#}")),
            _ => Ok(()),
        }
    })
}

/// The note `remove` leaves in a folder it could not delete: the ref the attempt's work is kept
/// under and the attempt's thread, one per line (`janitor::sweep_attempt_leftovers` reads it).
pub const LEFTOVER: &str = ".hub-leftover";

/// The kept ref and the thread from a `LEFTOVER` note.
pub fn read_leftover(text: &str) -> Option<(String, String)> {
    let mut lines = text.lines().map(str::trim);
    let kept_at = lines.next().filter(|l| l.starts_with("refs/hub/"))?;
    let owner = lines.next().filter(|l| !l.is_empty())?;
    Some((kept_at.to_owned(), owner.to_owned()))
}

/// `remove`, with `delete` taking the worktree's folder away.
fn remove_with(root: &Path, worktree: &Worktree, keep: &str, delete: &dyn Fn(&Path) -> Result<()>) -> bool {
    let mut gone = true;
    if worktree.path.exists() {
        let kept_at = format!("refs/hub/{keep}");
        let kept = checkpoint::snapshot(&worktree.path, "Hub: attempt as it was").and_then(|s| git(root, &["update-ref", &kept_at, &s.commit]));
        if let Err(e) = &kept {
            tracing::warn!("keep the attempt as a ref failed: {e:#}");
        }
        let mut deleted = delete(&worktree.path);
        for _ in 1..REMOVE_TRIES {
            if deleted.is_ok() {
                break;
            }
            std::thread::sleep(REMOVE_PAUSE);
            deleted = delete(&worktree.path);
        }
        gone = deleted.is_ok();
        if let Err(e) = deleted {
            // The note is what lets the clean-up take the folder; without a kept ref it stays.
            let owner = keep.split('/').next().unwrap_or(keep);
            let noted = kept.is_ok() && std::fs::write(worktree.path.join(LEFTOVER), format!("{kept_at}\n{owner}\n")).is_ok();
            let then = if noted { "the clean-up takes it later" } else { "it stays until removed by hand" };
            tracing::warn!("could not remove the attempt's worktree {}, {then}: {e:#}", worktree.path.display());
        }
    }
    git(root, &["branch", "-q", "-D", &worktree.branch]).warn_on_err("delete the attempt's branch");
    git(root, &["worktree", "prune"]).warn_on_err("git worktree prune");
    gone
}

/// Eight characters of a group id, enough to tell groups apart in branch names.
pub fn branch_stem(group: &str) -> String {
    group.chars().rev().filter(|c| c.is_ascii_alphanumeric()).take(8).collect::<Vec<_>>().into_iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git").args(args).current_dir(dir).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    #[test]
    fn attempts_start_from_the_folder_and_the_picked_one_lands_in_it() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        sh(repo, &["init", "-q", "-b", "main"]);
        // Git for Windows checks files out with CRLF by default; this test compares contents.
        sh(repo, &["config", "core.autocrlf", "false"]);
        std::fs::create_dir(repo.join("app")).unwrap();
        std::fs::write(repo.join("app/a.txt"), "one\n").unwrap();
        sh(repo, &["add", "."]);
        sh(repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"]);
        // Uncommitted work the attempts should see.
        std::fs::write(repo.join("app/wip.txt"), "wip\n").unwrap();

        let base = base(&repo.join("app"), "g-0123456789").unwrap();
        let one = add(&repo.join("app"), &base, "g-0123456789", 1).unwrap();
        let two = add(&repo.join("app"), &base, "g-0123456789", 2).unwrap();
        assert!(one.cwd.ends_with("app") && one.cwd.join("wip.txt").exists());
        assert_eq!(one.branch, "hub/attempt-23456789-1");

        std::fs::write(one.cwd.join("a.txt"), "one\nfrom attempt one\n").unwrap();
        std::fs::write(two.cwd.join("b.txt"), "attempt two\n").unwrap();
        assert_eq!(stat(&one.path, &base.commit).unwrap(), (1, 1, 0));

        let files = apply(&base.root, &one.path, &base.commit, "t/attempt").unwrap();
        assert_eq!(files, 1);
        assert_eq!(std::fs::read_to_string(repo.join("app/a.txt")).unwrap(), "one\nfrom attempt one\n");
        assert!(repo.join("app/wip.txt").exists() && !repo.join("app/b.txt").exists());

        assert!(remove(&base.root, &one, "t/attempt-1"));
        assert!(remove(&base.root, &two, "t/attempt-2"));
        assert!(!two.path.exists());
        assert!(!sh(repo, &["branch"]).contains("hub/attempt"));
        assert!(sh(repo, &["for-each-ref", "refs/hub/t/"]).contains("attempt-2"), "dropped work is kept");
    }

    /// A worktree whose folder cannot be deleted (on Windows, a process that is still ending works
    /// in it) is left behind without failing: its work is kept, git forgets it, and the clean-up
    /// removes the folder later.
    #[test]
    fn a_folder_that_cannot_be_deleted_yet_is_left_for_the_clean_up() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        sh(repo, &["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        sh(repo, &["add", "."]);
        sh(repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"]);
        let base = base(repo, "g-0123456789").unwrap();
        let one = add(repo, &base, "g-0123456789", 1).unwrap();
        std::fs::write(one.cwd.join("b.txt"), "work\n").unwrap();

        // As `git worktree remove` does when a file stays locked: the link goes, the folder stays.
        let tries = std::cell::Cell::new(0);
        let locked = |path: &Path| {
            tries.set(tries.get() + 1);
            crate::locked::clean_file(path.join(".git"));
            Err(anyhow::anyhow!("failed to delete '{}': Permission denied", path.display()))
        };
        assert!(!remove_with(repo, &one, "t/attempt-1", &locked), "not gone, and no error");
        assert_eq!(tries.get(), REMOVE_TRIES, "tried again before giving up");
        assert!(one.path.join("b.txt").exists());
        assert!(sh(repo, &["for-each-ref", "refs/hub/t/"]).contains("attempt-1"), "the work is kept");
        assert!(!sh(repo, &["worktree", "list"]).contains("hub-attempts"), "git no longer has it");

        assert_eq!(read_leftover(&std::fs::read_to_string(one.path.join(LEFTOVER)).unwrap()), Some(("refs/hub/t/attempt-1".into(), "t".into())));

        // Beside it: a live attempt (still linked), and a leftover another thread works in.
        let live = add(repo, &base, "g-0123456789", 2).unwrap();
        let three = add(repo, &base, "g-0123456789", 3).unwrap();
        assert!(!remove_with(repo, &three, "u/attempt-3", &locked));
        let attempts = one.path.parent().unwrap();
        let other = (three.cwd.clone(), false, "someone".to_owned());
        // The attempt's own archived thread does not hold its leftover.
        let owner = (one.cwd.clone(), false, "t".to_owned());
        let folders = [other, owner];
        let sweep = |root: &Path, age| {
            let mut done = crate::model::Cleanup::default();
            crate::janitor::sweep_attempt_leftovers(root, attempts, &folders, age, &mut done);
            done
        };
        // Git could not list the worktrees (here: not a repository): nothing goes.
        let elsewhere = tempfile::tempdir().unwrap();
        let none = sweep(elsewhere.path(), std::time::Duration::ZERO);
        assert!(none.removed.is_empty() && one.path.exists() && three.path.exists());
        assert!(sweep(repo, std::time::Duration::from_secs(3600)).removed.is_empty(), "not before it is old");

        let done = sweep(repo, std::time::Duration::ZERO);
        assert_eq!(done.removed.iter().map(|c| c.count).sum::<u32>(), 1, "{done:?}");
        assert!(!one.path.exists(), "the clean-up took the leftover");
        assert!(live.path.join(".git").exists() && live.path.exists(), "a live attempt stays");
        assert!(three.path.exists(), "a leftover a thread is on stays");
        assert!(done.kept.iter().any(|k| k.why == "a thread is still on it"), "{done:?}");
    }
}
