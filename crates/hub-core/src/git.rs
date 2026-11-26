//! How Hub runs git (no prompt, no stdin unless given, a time limit; sync or async), and just
//! enough git for the thread header and the dev page: the branch, how many files have changes,
//! and answers kept for pages that ask every few seconds.

use crate::locked::ResultExt;
use crate::locked::LockExt;
use crate::locked::Ignore;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Git state of a thread's folder.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitInfo {
    /// Branch name, or the short commit when detached. None outside a repository.
    pub branch: Option<String>,
    /// Files with changes. None when git is not installed.
    #[ts(type = "number | null")]
    pub changed: Option<u32>,
}

/// How long [`run_cached`] reuses an answer while the repository's index and HEAD stay as they were.
const FRESH: Duration = Duration::from_secs(30);

/// Size and time of the index, HEAD and HEAD's reflog: a commit, checkout, reset or `git add`
/// changes at least one of them.
type Stamp = [Option<(u64, SystemTime)>; 3];

struct Cached {
    at: Instant,
    stamp: Stamp,
    out: Option<String>,
}

static CACHE: LazyLock<Mutex<HashMap<(PathBuf, Vec<String>), Cached>>> = LazyLock::new(Default::default);

pub fn info(cwd: &Path) -> GitInfo {
    let Some(git_dir) = find_git_dir(cwd) else {
        return GitInfo { branch: None, changed: None };
    };
    GitInfo { branch: branch(&git_dir), changed: changed_files(cwd) }
}

/// Just the branch, which reads one file and runs no git.
pub fn branch_of(cwd: &Path) -> Option<String> {
    branch(&find_git_dir(cwd)?)
}

/// The branch checked out in `cwd` as `git branch --show-current` prints it, so empty when HEAD
/// is detached; None outside a repository. Reads one file and runs no git.
pub fn current_branch(cwd: &Path) -> Option<String> {
    let head = std::fs::read_to_string(find_git_dir(cwd)?.join("HEAD")).ok()?;
    Some(head.trim().strip_prefix("ref: refs/heads/").unwrap_or_default().to_owned())
}

/// The `.git` folder for `cwd`, following the `gitdir:` file that worktrees use.
fn find_git_dir(cwd: &Path) -> Option<PathBuf> {
    for dir in cwd.ancestors() {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git);
        }
        if dot_git.is_file() {
            let text = std::fs::read_to_string(&dot_git).ok()?;
            let target = text.strip_prefix("gitdir:")?.trim();
            return Some(dir.join(target));
        }
    }
    None
}

fn branch(git_dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref: refs/heads/") {
        Some(name) => Some(name.to_owned()),
        // Detached: show the short commit like git does.
        None => Some(head.chars().take(7).collect()),
    }
}

/// Number of files with changes, or None when git is not installed.
fn changed_files(cwd: &Path) -> Option<u32> {
    let out = run(cwd, &["status", "--porcelain=v1", "--untracked-files=normal"])?;
    Some(out.lines().count() as u32)
}

/// How long any git Hub starts may take before it is killed. A fetch on a bad network or a
/// credential helper waiting for a person must not hold a thread for ever.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(60);

/// For git that moves a lot of data on purpose: clone, pull, push, bundles, checkouts.
pub(crate) const LONG: Duration = Duration::from_secs(600);

/// The one way Hub starts git: no prompt of any kind (terminal, sign-in window, askpass), no
/// stdin, no console window, and killed after `timeout`. Returns the raw output, or an error
/// when git is missing or the time ran out.
pub(crate) fn output(cwd: &Path, args: &[&str], timeout: Duration) -> std::io::Result<std::process::Output> {
    output_with(cwd, args, &[], None, timeout)
}

/// [`output`] with environment variables of the caller's own (a checkpoint's author and index)
/// and, when `stdin` is given, those bytes as git's input instead of none.
pub(crate) fn output_with(cwd: &Path, args: &[&str], env: &[(&str, &OsStr)], stdin: Option<&[u8]>, timeout: Duration) -> std::io::Result<std::process::Output> {
    use std::io::{Read, Write};
    use std::process::Stdio;
    let mut cmd = crate::proc::command("git");
    cmd.args(["-c", "credential.interactive=never"]).args(args).current_dir(cwd);
    cmd.env("GIT_TERMINAL_PROMPT", "0").env("GCM_INTERACTIVE", "never").env("GIT_ASKPASS", "").env("SSH_ASKPASS", "");
    cmd.envs(env.iter().copied());
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
        // Written beside the readers, so git filling its output pipe cannot stall the write.
        let input = input.to_vec();
        std::thread::spawn(move || pipe.write_all(&input).warn_on_err("write git's input"));
    }
    let mut out = child.stdout.take().expect("piped");
    let mut err = child.stderr.take().expect("piped");
    let out_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        out.read_to_end(&mut b).ignore("git's output is what could be read before the pipe broke");
        b
    });
    let err_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        err.read_to_end(&mut b).ignore("git's output is what could be read before the pipe broke");
        b
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= timeout {
            child.kill().warn_on_err("kill the timed-out git");
            child.wait().warn_on_err("wait for the killed git");
            // Not joined: a grandchild (ssh, a credential helper) may hold the pipes open.
            drop((out_t, err_t));
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("git {} took longer than {}s", args.first().unwrap_or(&""), timeout.as_secs())));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Ok(std::process::Output { status, stdout: out_t.join().unwrap_or_default(), stderr: err_t.join().unwrap_or_default() })
}

/// Trimmed stdout of git in `dir`, or an error that says what failed in git's own words
/// (`git <args> failed: <stderr>`), for code that shows it or matches on it (a sign-in failure).
/// Under [`LONG`], since callers fetch, push, rebase and add worktrees with it.
pub(crate) fn checked(dir: &Path, args: &[&str]) -> anyhow::Result<String> {
    use anyhow::Context;
    let out = output(dir, args, LONG).context("running git; is it installed?")?;
    if !out.status.success() {
        anyhow::bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// [`output`] for async code: git runs on a blocking thread, so the task never waits on it.
pub(crate) async fn output_async(cwd: &Path, args: &[&str], timeout: Duration) -> std::io::Result<std::process::Output> {
    let cwd = cwd.to_path_buf();
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    tokio::task::spawn_blocking(move || output(&cwd, &args.iter().map(String::as_str).collect::<Vec<_>>(), timeout)).await.map_err(std::io::Error::other)?
}

/// The commit `rev` names in `dir`, shortened to 12 characters the way Hub names a version
/// (`git rev-parse --short=12`). None outside a repository, for an unknown `rev` or without git.
pub fn short_commit(dir: &Path, rev: &str) -> Option<String> {
    run(dir, &["rev-parse", "--short=12", rev]).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// Stdout of a git command in `cwd`, or None when git is missing, the command fails or it times out.
pub(crate) fn run(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = output(cwd, args, TIMEOUT).ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// [`run`] for a page that asks every few seconds. The same folder and arguments get the last
/// answer for up to [`FRESH`] while the index and HEAD are unchanged, so a commit, checkout or
/// `git add` shows at once and other edits within half a minute. Every git is a process start:
/// the dev page asking every two seconds about ten worktrees ran 17 a second, and on Windows each
/// start leaves kernel memory behind in some drivers, which grew by gigabytes a day.
/// `--no-optional-locks` keeps `git status` from rewriting the index under a thread's own git.
pub(crate) fn run_cached(cwd: &Path, args: &[&str]) -> Option<String> {
    let key = (cwd.to_path_buf(), args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>());
    if let Some(hit) = CACHE.locked().get(&key)
        && hit.at.elapsed() < FRESH
        && hit.stamp == stamp(cwd)
    {
        return hit.out.clone();
    }
    let out = run(cwd, &[&["--no-optional-locks"], args].concat());
    let mut cache = CACHE.locked();
    cache.retain(|_, c| c.at.elapsed() < FRESH * 10);
    cache.insert(key, Cached { at: Instant::now(), stamp: stamp(cwd), out: out.clone() });
    out
}

/// Who last changed one line of a file, from `git blame`.
#[derive(Debug, Clone, Default)]
pub struct Blamed {
    /// Unix seconds of the commit (the time of the blame for a line not committed yet).
    pub time: i64,
    /// Short commit id; empty for a line not committed yet.
    pub commit: String,
    pub summary: String,
}

/// The last change of every line of `file` (relative to `cwd`), first line first. None outside a
/// repository, for an untracked file or when git is missing.
pub fn blame(cwd: &Path, file: &str) -> Option<Vec<Blamed>> {
    let out = run(cwd, &["blame", "--line-porcelain", "--", file])?;
    let mut lines = Vec::new();
    let mut now = Blamed::default();
    for l in out.lines() {
        if l.starts_with('\t') {
            lines.push(std::mem::take(&mut now));
        } else if let Some(t) = l.strip_prefix("author-time ") {
            now.time = t.parse().unwrap_or(0);
        } else if let Some(s) = l.strip_prefix("summary ") {
            now.summary = s.to_owned();
        } else if now.commit.is_empty()
            && let Some(id) = l.split(' ').next()
            && id.len() == 40
            && id.bytes().all(|b| b.is_ascii_hexdigit())
            && id.bytes().any(|b| b != b'0')
        {
            now.commit = id[..8].to_owned();
        }
    }
    Some(lines)
}

fn stamp(cwd: &Path) -> Stamp {
    let Some(dir) = find_git_dir(cwd) else { return [None; 3] };
    let of = |p: PathBuf| std::fs::metadata(p).ok().and_then(|m| Some((m.len(), m.modified().ok()?)));
    [of(dir.join("index")), of(dir.join("HEAD")), of(dir.join("logs").join("HEAD"))]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_branch_and_follows_worktree_links() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/claude/atlas-pc-build\n").unwrap();
        std::fs::create_dir_all(repo.join("src/deep")).unwrap();
        assert_eq!(branch(&find_git_dir(&repo.join("src/deep")).unwrap()).as_deref(), Some("claude/atlas-pc-build"));

        let wt = dir.path().join("wt");
        let wt_git = dir.path().join("repo/.git/worktrees/wt");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::create_dir_all(&wt_git).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        std::fs::write(wt_git.join("HEAD"), "0123456789abcdef\n").unwrap();
        assert_eq!(branch(&find_git_dir(&wt).unwrap()).as_deref(), Some("0123456"));
    }

    #[test]
    fn no_repository_means_no_branch() {
        let dir = crate::tempdirs::tempdir_outside_git();
        assert!(info(dir.path()).branch.is_none());
        assert!(current_branch(dir.path()).is_none());
    }

    #[test]
    fn current_branch_is_empty_when_detached_like_git() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/hub-dev/x\n").unwrap();
        assert_eq!(current_branch(dir.path()).as_deref(), Some("hub-dev/x"));
        std::fs::write(dir.path().join(".git/HEAD"), "0123456789abcdef\n").unwrap();
        assert_eq!(current_branch(dir.path()).as_deref(), Some(""));
    }

    #[test]
    fn caller_env_and_input_reach_git() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        assert!(run(repo, &["init", "-b", "main"]).is_some());
        let who = output_with(repo, &["var", "GIT_AUTHOR_IDENT"], &[("GIT_AUTHOR_NAME", OsStr::new("Hub")), ("GIT_AUTHOR_EMAIL", OsStr::new("hub@localhost"))], None, TIMEOUT).unwrap();
        assert!(String::from_utf8_lossy(&who.stdout).starts_with("Hub <hub@localhost>"), "{who:?}");
        let hashed = output_with(repo, &["hash-object", "--stdin"], &[], Some(b"hello\n"), TIMEOUT).unwrap();
        // `printf 'hello\n' | git hash-object --stdin`
        assert_eq!(String::from_utf8_lossy(&hashed.stdout).trim(), "ce013625030ba8dba906f756967f9e9ca394464a");
        // Without input git reads none, so it does not wait for one.
        let empty = output(repo, &["hash-object", "--stdin"], TIMEOUT).unwrap();
        assert_eq!(String::from_utf8_lossy(&empty.stdout).trim(), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    }

    #[test]
    fn short_commit_names_a_version_and_async_output_reports_failure() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        assert!(run(repo, &["init", "-b", "main"]).is_some());
        assert_eq!(short_commit(repo, "HEAD"), None, "no commit yet");
        assert!(run(repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "one"]).is_some());
        let short = short_commit(repo, "HEAD").unwrap();
        assert_eq!(short.len(), 12);
        assert!(run(repo, &["rev-parse", "HEAD"]).unwrap().starts_with(&short));

        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let bad = rt.block_on(output_async(repo, &["rev-parse", "--verify", "nope"], TIMEOUT)).unwrap();
        assert!(!bad.status.success());
        assert!(!bad.stderr.is_empty(), "git's own words come back for the caller to show");
    }

    #[test]
    fn a_cached_answer_lasts_until_the_index_or_head_moves() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let git = |args: &[&str]| assert!(run(repo, args).is_some(), "git {args:?}");
        git(&["init", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "a").unwrap();
        let count = || run_cached(repo, &["status", "--porcelain"]).map(|s| s.lines().count());
        assert_eq!(count(), Some(1));

        // A new file alone moves neither the index nor HEAD: the page keeps the answer it had.
        std::fs::write(repo.join("b.txt"), "b").unwrap();
        assert_eq!(count(), Some(1), "reused within FRESH");

        // `git add` writes the index, so the next ask runs git again.
        git(&["add", "b.txt"]);
        assert_eq!(count(), Some(2));

        // A commit moves HEAD's reflog even when nothing else changed.
        let ahead = || run_cached(repo, &["rev-list", "--count", "HEAD"]).map(|s| s.trim().to_owned());
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "one"]);
        assert_eq!(ahead().as_deref(), Some("1"));
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "--allow-empty", "-m", "two"]);
        assert_eq!(ahead().as_deref(), Some("2"));
    }
}

