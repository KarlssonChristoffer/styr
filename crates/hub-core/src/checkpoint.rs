//! Checkpoints: a snapshot of a repository's working tree, taken before each message a thread
//! sends to its engine, so everything the thread did can be rolled back.
//!
//! A snapshot is a commit made with a throwaway index (`git add -A` into a copy of the real one),
//! so tracked and untracked files are both in it and the user's index, branch and stash are left
//! alone. Ignored files are not. Each snapshot is kept alive by a ref under `refs/hub/`.

use crate::locked::ResultExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::model::{UndoFile, UndoPreview};

/// A snapshot of the repository `root`, with where its branch pointed when it was taken.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub root: PathBuf,
    pub commit: String,
    pub head: Option<String>,
    pub branch: Option<String>,
}

/// Snapshots the repository `cwd` is in and keeps it under `refs/hub/<name>`. None when `cwd`
/// is not in a git repository, or git is missing.
pub fn take(cwd: &Path, name: &str, message: &str) -> Result<Option<Snapshot>> {
    let Ok(root) = git(cwd, &["rev-parse", "--show-toplevel"]) else {
        return Ok(None);
    };
    let root = PathBuf::from(root.trim());
    let snapshot = snapshot(&root, message)?;
    git(&root, &["update-ref", &format!("refs/hub/{name}"), &snapshot.commit])?;
    Ok(Some(snapshot))
}

pub(crate) fn snapshot(root: &Path, message: &str) -> Result<Snapshot> {
    let head = git(root, &["rev-parse", "-q", "--verify", "HEAD^{commit}"]).ok().map(|h| h.trim().to_owned());
    let branch = git(root, &["symbolic-ref", "-q", "--short", "HEAD"]).ok().map(|b| b.trim().to_owned());
    let tree = with_index(root, None, |index| {
        git_with(root, &["add", "-A", "--", ":/"], index, None)?;
        git_with(root, &["write-tree"], index, None)
    })?;
    let mut args = vec!["commit-tree", tree.trim(), "-m", message];
    if let Some(head) = &head {
        args.extend(["-p", head.as_str()]);
    }
    let commit = git(root, &args)?.trim().to_owned();
    Ok(Snapshot { root: root.to_owned(), commit, head, branch })
}

/// What rolling the repository back to `target` would change.
pub fn preview(root: &Path, target: &Target) -> Result<UndoPreview> {
    let now = snapshot(root, "Hub: now")?;
    plan(root, target, &now)
}

/// A snapshot to go back to, as recorded in the log.
#[derive(Debug, Clone)]
pub struct Target {
    pub commit: String,
    pub head: Option<String>,
    pub branch: Option<String>,
}

fn plan(root: &Path, target: &Target, now: &Snapshot) -> Result<UndoPreview> {
    let out = git(
        root,
        &["diff-tree", "-r", "-z", "--no-renames", "--name-status", &target.commit, &now.commit],
    )?;
    let mut files = Vec::new();
    let mut parts = out.split('\0').filter(|p| !p.is_empty());
    while let (Some(status), Some(path)) = (parts.next(), parts.next()) {
        let action = match status {
            // Made after the checkpoint.
            "A" => "remove",
            // Gone since the checkpoint.
            "D" => "recreate",
            _ => "restore",
        };
        files.push(UndoFile { path: path.to_owned(), action: action.into() });
    }

    // Commits made since are undone too, unless they left this machine or the branch changed.
    let mut commits = Vec::new();
    let mut kept = None;
    if now.head != target.head {
        let (Some(old), Some(new)) = (&target.head, &now.head) else {
            return Ok(UndoPreview { root: root.display().to_string(), files, commits, kept_commits: None });
        };
        let range = format!("{old}..{new}");
        commits = git(root, &["log", "--format=%h %s", &range])
            .map(|l| l.lines().map(str::to_owned).collect())
            .unwrap_or_default();
        let ancestor = git(root, &["merge-base", "--is-ancestor", old, new]).is_ok();
        let unpushed = git(root, &["rev-list", "--count", new, "--not", "--remotes", old])
            .ok()
            .and_then(|n| n.trim().parse::<usize>().ok());
        if now.branch != target.branch {
            kept = Some(format!("the branch changed to {}", now.branch.as_deref().unwrap_or("a detached HEAD")));
        } else if !ancestor {
            kept = Some("the branch was rewritten".into());
        } else if unpushed != Some(commits.len()) {
            kept = Some("some of them are pushed".into());
        }
    }
    Ok(UndoPreview { root: root.display().to_string(), files, commits, kept_commits: kept })
}

/// Rolls the repository back to `target`. A snapshot of how it was is kept first under
/// `refs/hub/<safety>`, so the rollback can be undone. Returns that snapshot and what changed.
pub fn restore(root: &Path, target: &Target, safety: &str) -> Result<(Snapshot, UndoPreview)> {
    let now = snapshot(root, "Hub: before rolling back")?;
    git(root, &["update-ref", &format!("refs/hub/{safety}"), &now.commit])?;
    let plan = plan(root, target, &now)?;

    if !plan.commits.is_empty() && plan.kept_commits.is_none()
        && let Some(old) = &target.head
    {
        // Moves the branch back; the files are put right below.
        git(root, &["reset", "-q", "--mixed", old])?;
    }
    for f in plan.files.iter().filter(|f| f.action == "remove") {
        let path = root.join(&f.path);
        match std::fs::remove_file(&path) {
            Ok(()) => remove_empty_parents(root, &path),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("removing {}", path.display())),
        }
    }
    let back: Vec<&str> = plan.files.iter().filter(|f| f.action != "remove").map(|f| f.path.as_str()).collect();
    if !back.is_empty() {
        with_index(root, Some(&target.commit), |index| {
            let mut paths = back.join("\0");
            paths.push('\0');
            git_with(root, &["checkout-index", "-f", "-q", "-z", "--stdin"], index, Some(&paths))
        })?;
    }
    Ok((now, plan))
}

/// Removes folders left empty by removing `file`, up to the repository root.
fn remove_empty_parents(root: &Path, file: &Path) {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d == root || !d.starts_with(root) || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// Runs `f` with a throwaway index file: a copy of the real index, or `tree` read into it.
fn with_index<T>(root: &Path, tree: Option<&str>, f: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let git_dir = PathBuf::from(git(root, &["rev-parse", "--absolute-git-dir"])?.trim());
    let index = git_dir.join(format!("hub-index-{}", uuid::Uuid::now_v7()));
    let result = (|| {
        match tree {
            Some(tree) => {
                git_with(root, &["read-tree", tree], &index, None)?;
            }
            None => {
                let real = git_dir.join("index");
                if real.exists() {
                    std::fs::copy(&real, &index).context("copying the git index")?;
                    // Git rereads a file changed in the same tick the index was written, but only
                    // if the index keeps its time; a fresh copy would hide such an edit.
                    if let Ok(modified) = std::fs::metadata(&real).and_then(|m| m.modified()) {
                        std::fs::File::options().write(true).open(&index).and_then(|f| f.set_modified(modified)).warn_on_err("keep the index's modified time");
                    }
                }
            }
        }
        f(&index)
    })();
    crate::locked::clean_file(&index);
    result
}

pub(crate) fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    run(cwd, args, None, None)
}

fn git_with(cwd: &Path, args: &[&str], index: &Path, stdin: Option<&str>) -> Result<String> {
    run(cwd, args, Some(index), stdin)
}

/// Git with Hub as author and committer and, given `index`, that index instead of the user's.
/// Under [`crate::git::LONG`]: the first snapshot of a big folder hashes every file.
pub(crate) fn run(cwd: &Path, args: &[&str], index: Option<&Path>, stdin: Option<&str>) -> Result<String> {
    use std::ffi::OsStr;
    // Snapshots are Hub's, whoever the repository says the user is.
    let mut env: Vec<(&str, &OsStr)> = vec![
        ("GIT_AUTHOR_NAME", OsStr::new("Hub")),
        ("GIT_AUTHOR_EMAIL", OsStr::new("hub@localhost")),
        ("GIT_COMMITTER_NAME", OsStr::new("Hub")),
        ("GIT_COMMITTER_EMAIL", OsStr::new("hub@localhost")),
    ];
    if let Some(index) = index {
        env.push(("GIT_INDEX_FILE", index.as_os_str()));
    }
    let out = crate::git::output_with(cwd, args, &env, stdin.map(str::as_bytes), crate::git::LONG).context("running git")?;
    if !out.status.success() {
        bail!("git {}: {}", args.first().unwrap_or(&""), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What changed from `base` to the working tree now, untracked files included, for a review.
pub struct Changes {
    /// Each file's diff, in git's order.
    pub files: Vec<FileDiff>,
    /// The snapshot of now, kept under `refs/hub/<keep>`, so the next review starts here.
    pub head: String,
}

pub struct FileDiff {
    pub path: String,
    pub diff: String,
    /// Lines added plus lines removed.
    pub lines: usize,
    pub added: usize,
    pub removed: usize,
}

pub fn diff_since(root: &Path, base: &str, keep: &str) -> Result<Changes> {
    let now = snapshot(root, "Hub: reviewed")?;
    git(root, &["update-ref", &format!("refs/hub/{keep}"), &now.commit])?;
    let diff = git(root, &["diff", "--no-color", "--no-ext-diff", "-M", base, &now.commit])?;
    Ok(Changes { files: split(&diff), head: now.commit })
}

/// Whether `root` is inside a git repository right now.
pub fn is_repo(root: &Path) -> bool {
    git(root, &["rev-parse", "--git-dir"]).is_ok()
}

/// What changed from `base` to the working tree now, untracked files included, without keeping
/// anything: what a turn did, for the leak guard and proof before done.
pub fn diff_now(root: &Path, base: &str) -> Result<Vec<FileDiff>> {
    let now = snapshot(root, "Hub: after a turn")?;
    let diff = git(root, &["diff", "--no-color", "--no-ext-diff", "-M", base, &now.commit])?;
    Ok(split(&diff))
}

/// Whether `paths` in the working tree now are just as they were at one of `earlier` (oldest
/// first): a turn that put back what an earlier turn did. Returns the first one that matches.
pub fn put_back(root: &Path, earlier: &[String], paths: &[String]) -> Result<Option<String>> {
    if paths.is_empty() || earlier.is_empty() {
        return Ok(None);
    }
    let now = snapshot(root, "Hub: after a turn")?;
    for base in earlier {
        let mut args = vec!["diff", "--no-ext-diff", "--name-only", base.as_str(), now.commit.as_str(), "--"];
        args.extend(paths.iter().map(String::as_str));
        if git(root, &args)?.trim().is_empty() {
            return Ok(Some(base.clone()));
        }
    }
    Ok(None)
}

/// A whole diff cut into one diff per file.
fn split(diff: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    for line in diff.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest.trim_end().rsplit_once(" b/").map(|(_, b)| b).unwrap_or(rest.trim_end());
            files.push(FileDiff { path: path.to_owned(), diff: String::new(), lines: 0, added: 0, removed: 0 });
        }
        let Some(file) = files.last_mut() else { continue };
        file.diff.push_str(line);
        let added = line.starts_with('+') && !line.starts_with("+++");
        let removed = line.starts_with('-') && !line.starts_with("---");
        file.added += usize::from(added);
        file.removed += usize::from(removed);
        file.lines += usize::from(added || removed);
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        git(dir, args).unwrap();
    }

    fn repo() -> tempfile::TempDir {
        let dir = crate::janitor::tempdir_outside_git();
        let p = dir.path();
        sh(p, &["init", "-q", "-b", "main"]);
        // Git for Windows checks files out with CRLF by default; these tests compare contents.
        sh(p, &["config", "core.autocrlf", "false"]);
        std::fs::write(p.join("keep.txt"), "one\n").unwrap();
        std::fs::write(p.join(".gitignore"), "target/\n").unwrap();
        sh(p, &["add", "."]);
        sh(p, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"]);
        dir
    }

    fn target(s: &Snapshot) -> Target {
        Target { commit: s.commit.clone(), head: s.head.clone(), branch: s.branch.clone() }
    }

    #[test]
    fn rolls_back_edits_new_files_deletions_and_commits() {
        let dir = repo();
        let p = dir.path();
        // Work in progress before the thread starts is part of the checkpoint.
        std::fs::write(p.join("draft.txt"), "mine\n").unwrap();
        let cp = take(p, "t/1", "before").unwrap().unwrap();

        std::fs::write(p.join("keep.txt"), "changed\n").unwrap();
        std::fs::create_dir_all(p.join("src/deep")).unwrap();
        std::fs::write(p.join("src/deep/new.rs"), "new\n").unwrap();
        std::fs::remove_file(p.join("draft.txt")).unwrap();
        std::fs::create_dir_all(p.join("target")).unwrap();
        std::fs::write(p.join("target/out"), "built\n").unwrap();
        sh(p, &["add", "keep.txt"]);
        sh(p, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "agent commit"]);

        let preview = preview(p, &target(&cp)).unwrap();
        let mut files: Vec<_> = preview.files.iter().map(|f| (f.path.as_str(), f.action.as_str())).collect();
        files.sort();
        assert_eq!(files, [("draft.txt", "recreate"), ("keep.txt", "restore"), ("src/deep/new.rs", "remove")]);
        assert_eq!(preview.commits.len(), 1);
        assert!(preview.kept_commits.is_none());

        let (safety, _) = restore(p, &target(&cp), "t/undo").unwrap();
        assert_eq!(std::fs::read_to_string(p.join("keep.txt")).unwrap(), "one\n");
        assert_eq!(std::fs::read_to_string(p.join("draft.txt")).unwrap(), "mine\n");
        assert!(!p.join("src").exists(), "empty folders go too");
        assert!(p.join("target/out").exists(), "ignored files are left alone");
        assert_eq!(git(p, &["rev-parse", "HEAD"]).unwrap().trim(), cp.head.as_deref().unwrap());

        // And the rollback itself can be rolled back.
        restore(p, &target(&safety), "t/redo").unwrap();
        assert_eq!(std::fs::read_to_string(p.join("keep.txt")).unwrap(), "changed\n");
        assert!(p.join("src/deep/new.rs").exists());
        assert!(!p.join("draft.txt").exists());
    }

    #[test]
    fn keeps_commits_on_another_branch() {
        let dir = repo();
        let p = dir.path();
        let cp = take(p, "t/1", "before").unwrap().unwrap();
        sh(p, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(p.join("keep.txt"), "feature\n").unwrap();
        sh(p, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-am", "on feature"]);
        let preview = preview(p, &target(&cp)).unwrap();
        assert!(preview.kept_commits.as_deref().unwrap().contains("feature"));
        restore(p, &target(&cp), "t/undo").unwrap();
        assert_eq!(std::fs::read_to_string(p.join("keep.txt")).unwrap(), "one\n");
        assert_eq!(git(p, &["symbolic-ref", "--short", "HEAD"]).unwrap().trim(), "feature");
    }

    #[test]
    fn outside_a_repository_there_is_no_checkpoint() {
        let dir = crate::janitor::tempdir_outside_git();
        assert!(take(dir.path(), "t/1", "before").unwrap().is_none());
        assert!(!is_repo(dir.path()));
    }

    #[test]
    fn a_folder_that_stops_being_a_repository_is_seen() {
        let dir = repo();
        assert!(is_repo(dir.path()));
        std::fs::remove_dir_all(dir.path().join(".git")).unwrap();
        assert!(!is_repo(dir.path()));
    }

    #[test]
    fn diff_includes_untracked_files_one_per_file() {
        let dir = repo();
        let p = dir.path();
        let cp = take(p, "t/1", "before").unwrap().unwrap();
        std::fs::write(p.join("fresh.txt"), "hello\n").unwrap();
        std::fs::write(p.join("keep.txt"), "two\n").unwrap();
        let changes = diff_since(p, &cp.commit, "t/review").unwrap();
        let files: Vec<_> = changes.files.iter().map(|f| (f.path.as_str(), f.lines)).collect();
        assert_eq!(files, [("fresh.txt", 1), ("keep.txt", 2)]);
        assert!(changes.files[0].diff.contains("+hello"));
        // The next review starts where this one ended.
        assert!(diff_since(p, &changes.head, "t/review2").unwrap().files.is_empty());
    }

    #[test]
    fn a_turn_that_puts_back_earlier_work_is_seen() {
        let dir = repo();
        let p = dir.path();
        let first = take(p, "t/1", "before 1").unwrap().unwrap();
        std::fs::write(p.join("keep.txt"), "one\nadded\n").unwrap();
        std::fs::write(p.join("new.rs"), "fn x() {}\n").unwrap();
        let second = take(p, "t/2", "before 2").unwrap().unwrap();
        let earlier = [first.commit.clone(), second.commit.clone()];
        let paths = ["keep.txt".to_owned(), "new.rs".to_owned()];
        // Still the second turn's work: nothing put back.
        assert_eq!(put_back(p, &earlier[..1], &paths).unwrap(), None);
        // The third turn takes it all out again.
        std::fs::write(p.join("keep.txt"), "one\n").unwrap();
        std::fs::remove_file(p.join("new.rs")).unwrap();
        assert_eq!(put_back(p, &earlier, &paths).unwrap(), Some(first.commit.clone()));
        // Half of it is not a revert.
        std::fs::write(p.join("new.rs"), "fn y() {}\n").unwrap();
        assert_eq!(put_back(p, &earlier, &paths).unwrap(), None);
    }
}
