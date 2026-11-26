//! The thread's Git button, after T3 Code's Git actions: where the folder stands, switching
//! branch, and commit, push and open a pull request in one go, with the words written by a model
//! from the changes and the thread.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::git;
use crate::hub::Hub;
use crate::model::{GitDraft, GitShip, GitShipped, GitState};
use crate::proc::{GhError, gh_within};

pub fn state(cwd: &Path) -> GitState {
    let Some(status) = git::run(cwd, &["status", "--porcelain=v1", "--branch", "--untracked-files=normal"]) else {
        return GitState::default();
    };
    let mut st = GitState { default: crate::github::default_branch(cwd), ..Default::default() };
    for line in status.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            parse_head(head, &mut st);
        } else if line.len() > 3 {
            // A rename reads `old -> new`; the new name is the file now.
            let name = &line[3..];
            st.files.push(name.rsplit(" -> ").next().unwrap_or(name).trim_matches('"').to_owned());
        }
    }
    st.github = git::run(cwd, &["remote", "get-url", "origin"]).is_some_and(|u| u.contains("github.com"));
    st
}

/// `main...origin/main [ahead 1, behind 2]`, `No commits yet on main`, or `HEAD (no branch)`.
fn parse_head(head: &str, st: &mut GitState) {
    if head.starts_with("HEAD (no branch)") {
        return;
    }
    if let Some(name) = head.strip_prefix("No commits yet on ") {
        st.branch = Some(name.trim().to_owned());
        return;
    }
    let (names, counts) = head.split_once(" [").map(|(n, c)| (n, c.trim_end_matches(']'))).unwrap_or((head, ""));
    let (branch, upstream) = names.split_once("...").map(|(b, u)| (b, Some(u))).unwrap_or((names, None));
    st.branch = Some(branch.to_owned());
    st.upstream = upstream.is_some() && counts != "gone";
    for part in counts.split(", ") {
        if let Some(n) = part.strip_prefix("ahead ") {
            st.ahead = n.parse().unwrap_or(0);
        } else if let Some(n) = part.strip_prefix("behind ") {
            st.behind = n.parse().unwrap_or(0);
        }
    }
}

/// Local branches, newest first.
pub fn local_branches(cwd: &Path) -> Vec<String> {
    git::run(cwd, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/heads"])
        .map(|s| s.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// Checks out `branch`, made from where the folder is now when `create`. Uncommitted changes
/// come along, as with `git switch`; git refuses when they would be lost.
pub fn switch(cwd: &Path, branch: &str, create: bool) -> Result<()> {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') || branch.contains(char::is_whitespace) {
        bail!("not a branch name: {branch:?}");
    }
    let mut args = vec!["switch"];
    if create {
        args.push("-c");
    }
    args.push(branch);
    run_git(cwd, &args)
}

/// Runs git and says what it printed on failure.
fn run_git(cwd: &Path, args: &[&str]) -> Result<()> {
    let out = crate::git::output(cwd, args, crate::git::LONG).context("git is not installed, or took too long")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = if err.trim().is_empty() { String::from_utf8_lossy(&out.stdout) } else { err };
        let first: Vec<&str> = err.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("hint:")).take(3).collect();
        bail!("git {}: {}", args[0], if first.is_empty() { "failed".into() } else { first.join(" ") });
    }
    Ok(())
}

/// Pull, or commit, push and open a pull request: the steps asked for, in that order. Stops at
/// the first that fails, and says how far it got.
pub async fn ship(s: GitShip) -> Result<GitShipped> {
    let cwd = std::path::PathBuf::from(&s.cwd);
    let mut done = GitShipped::default();
    if s.pull {
        let dir = cwd.clone();
        tokio::task::spawn_blocking(move || run_git(&dir, &["pull", "--ff-only"])).await??;
        return Ok(done);
    }
    if s.commit {
        let message = s.message.as_deref().map(str::trim).unwrap_or_default().to_owned();
        if message.is_empty() {
            bail!("write a commit message first");
        }
        let dir = cwd.clone();
        done.commit = tokio::task::spawn_blocking(move || -> Result<Option<String>> {
            run_git(&dir, &["add", "-A"])?;
            run_git(&dir, &["commit", "-q", "-m", &message])?;
            Ok(git::run(&dir, &["rev-parse", "--short", "HEAD"]).map(|h| h.trim().to_owned()))
        })
        .await??;
    }
    if s.push {
        let dir = cwd.clone();
        let upstream = state(&cwd).upstream;
        tokio::task::spawn_blocking(move || if upstream { run_git(&dir, &["push", "-q"]) } else { run_git(&dir, &["push", "-q", "-u", "origin", "HEAD"]) })
            .await?
            .with_context(|| if done.commit.is_some() { "committed, but the push failed" } else { "the push failed" })?;
        done.pushed = true;
    }
    if s.pr {
        let title = s.title.as_deref().map(str::trim).unwrap_or_default();
        if title.is_empty() {
            bail!("give the pull request a title");
        }
        let body = s.body.as_deref().unwrap_or_default();
        let mut args = vec!["pr", "create", "--title", title, "--body", body];
        if s.draft {
            args.push("--draft");
        }
        let out = gh_within(&cwd, &args, Duration::from_secs(60)).await.map_err(|e| match e {
            GhError::Missing => anyhow::anyhow!("install the GitHub CLI (gh) to open pull requests"),
            GhError::Failed(msg) => anyhow::anyhow!("{msg}"),
        })?;
        done.pr_url = out.lines().map(str::trim).rfind(|l| l.starts_with("https://")).map(str::to_owned);
    }
    Ok(done)
}

const DRAFT: &str = "Write the git commit message and the pull request for the changes below, made in \
the coding conversation that follows them. Answer with JSON only, no code fence: \
{\"message\": \"...\", \"title\": \"...\", \"body\": \"...\"}. \
message: a subject line under 70 characters in the imperative mood, a blank line, then a short body \
saying what changed and why, in plain words a user of the program understands (it becomes the \
release note). title: the pull request's title, like the subject. body: a short markdown \
description: what changed, why, and how it was checked (the tests or commands the conversation ran). \
Write in English. Do not use tools.";

impl Hub {
    /// Words for the Git button: a commit message, and a pull request title and description.
    pub async fn git_draft(self: &Arc<Self>, cwd: &str, thread_id: Option<&str>) -> Result<GitDraft> {
        let dir = std::path::PathBuf::from(cwd);
        let changes = tokio::task::spawn_blocking(move || {
            let mut text = crate::files::uncommitted(&dir).unwrap_or_default();
            // Nothing to commit: the pull request covers what the branch has over the default one.
            if text.trim().is_empty() {
                let base = format!("origin/{}", crate::github::default_branch(&dir));
                let log = git::run(&dir, &["log", "--format=%s%n%b", &format!("{base}..HEAD")]).unwrap_or_default();
                let diff = git::run(&dir, &["diff", "--no-color", &format!("{base}...HEAD")]).unwrap_or_default();
                text = format!("Commits:\n{log}\n\n{diff}");
            }
            text
        })
        .await?;
        let changes: String = changes.chars().take(40_000).collect();
        let log = match thread_id {
            Some(id) => {
                let log = crate::hub::conversation_log(&self.store.events(id, 0, 100_000).await?);
                let chars: Vec<char> = log.chars().collect();
                chars[chars.len().saturating_sub(12_000)..].iter().collect()
            }
            None => String::new(),
        };
        let (name, tidy_model) = self.tidy_target(Some(cwd)).await;
        let engine = self.config.engines.get(&name).with_context(|| format!("unknown engine {name:?}"))?;
        let prompt = format!("{DRAFT}\n\n<changes>\n{changes}\n</changes>\n\n<conversation>{log}\n</conversation>");
        let answer = crate::engine::complete("commit_draft", engine, cwd, tidy_model.as_deref(), prompt);
        let answer = tokio::time::timeout(Duration::from_secs(120), answer).await.context("writing took too long")??;
        parse_draft(&answer).context("the model's answer was not the JSON asked for")
    }
}

fn parse_draft(answer: &str) -> Option<GitDraft> {
    let json = &answer[answer.find('{')?..=answer.rfind('}')?];
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let get = |k: &str| v[k].as_str().unwrap_or_default().trim().to_owned();
    let draft = GitDraft { message: get("message"), title: get("title"), body: get("body") };
    (!draft.message.is_empty()).then_some(draft)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_branch_line() {
        let mut st = GitState::default();
        parse_head("claude/x...origin/claude/x [ahead 2, behind 1]", &mut st);
        assert_eq!((st.branch.as_deref(), st.upstream, st.ahead, st.behind), (Some("claude/x"), true, 2, 1));
        let mut st = GitState::default();
        parse_head("work", &mut st);
        assert_eq!((st.branch.as_deref(), st.upstream), (Some("work"), false));
        let mut st = GitState::default();
        parse_head("No commits yet on main", &mut st);
        assert_eq!(st.branch.as_deref(), Some("main"));
    }

    #[test]
    fn reads_a_draft() {
        let d = parse_draft("Here:\n{\"message\": \"Add X\\n\\nWhy\", \"title\": \"Add X\", \"body\": \"- x\"}").unwrap();
        assert_eq!((d.title.as_str(), d.body.as_str()), ("Add X", "- x"));
        assert!(parse_draft("no").is_none());
    }

    #[test]
    fn commits_and_switches() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        for args in [&["init", "-q", "-b", "main"][..], &["config", "user.email", "t@t"], &["config", "user.name", "t"]] {
            run_git(dir, args).unwrap();
        }
        std::fs::write(dir.join("a.txt"), "hi").unwrap();
        let st = state(dir);
        assert_eq!((st.branch.as_deref(), st.files.as_slice(), st.github), (Some("main"), &["a.txt".to_owned()][..], false));
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let ship = GitShip { cwd: dir.display().to_string(), commit: true, push: false, pr: false, pull: false, message: Some("Add a".into()), title: None, body: None, draft: false };
        assert!(rt.block_on(ship_ok(ship)).commit.is_some());
        assert!(state(dir).files.is_empty());
        switch(dir, "feature", true).unwrap();
        assert_eq!(state(dir).branch.as_deref(), Some("feature"));
        assert_eq!(local_branches(dir).len(), 2);
        assert!(switch(dir, "-x", false).is_err());
    }

    async fn ship_ok(s: GitShip) -> GitShipped {
        ship(s).await.unwrap()
    }
}
