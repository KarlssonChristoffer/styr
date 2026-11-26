//! Shipping the clone's work: everything changed goes to a branch of its own, rebased on the
//! updater's branch and pushed, with a pull request opened (or added to) by `gh`. `dev_ship`
//! drives it; the git steps (commit, rebase, push) are `push_branch` at the bottom.

use super::*;
use crate::locked::ResultExt;

impl Hub {
    /// Commits everything changed in the clone to a branch of its own, pushes it and opens a
    /// pull request against the branch the updater follows, or adds to the one already open. It
    /// never pushes to that branch itself: Hub's merge queue merges it.
    pub async fn dev_ship(self: &Arc<Self>, ship: DevShip) -> Result<DevShipped> {
        let (dir, shown) = self.dev_shown().await?;
        if shown.is_none() && self.store.get_value(PREVIEW_KEY).await?.is_some() {
            bail!("the clone is showing the preview of pull requests; turn the preview off to ship work of its own");
        }
        let (remote, base) = (self.config.update.remote.clone(), self.config.update.branch.clone());
        let current = crate::git::run(&dir, &["branch", "--show-current"]).map(|b| b.trim().to_owned()).unwrap_or_default();
        // The branch's pull request: one still open gets the new commits; after one was merged
        // or closed the next ship starts a new branch.
        let open = if current.is_empty() || current == base { None } else { pr_for(&dir, &current).await };
        let reuse = open.as_ref().is_some_and(|(state, _)| state == "OPEN");

        let staged = {
            let dir = dir.clone();
            tokio::task::spawn_blocking(move || -> Result<String> {
                git(&dir, &["add", "-A"])?;
                git(&dir, &["diff", "--cached", "--stat"])
            })
            .await??
        };
        let typed = ship.message.as_deref().map(str::trim).filter(|m| !m.is_empty()).map(str::to_owned);
        // The words: what was typed, or a draft from the small model, whose commit body becomes
        // the release note.
        let draft = match &typed {
            Some(text) => GitDraft {
                message: text.clone(),
                title: text.lines().next().unwrap_or_default().to_owned(),
                body: text.clone(),
            },
            None => match self.git_draft(&dir.display().to_string(), None).await {
                Ok(draft) => draft,
                Err(_) => {
                    let line = self.commit_message(&dir, &staged).await;
                    GitDraft { message: line.clone(), title: line, body: "Made in Hub's dev mode.".into() }
                }
            },
        };

        let name = if reuse { current.clone() } else { branch_name(&draft.title, now_ms()) };

        let commit = {
            let (dir, name, base, remote, message) = (dir.clone(), name.clone(), base.clone(), remote.clone(), draft.message.clone());
            let commit_now = !staged.is_empty();
            tokio::task::spawn_blocking(move || push_branch(&dir, &remote, &base, &name, reuse, commit_now.then_some(message.as_str())))
            .await??
        };
        let (hash, subject) = commit.split_once(' ').unwrap_or((&commit, ""));

        let pr = if reuse {
            open.map(|(_, url)| url)
        } else {
            let args = ["pr", "create", "--base", &base, "--head", &name, "--title", &draft.title, "--body", &draft.body];
            let out = gh_within(&dir, &args, Duration::from_secs(60)).await.map_err(|e| match e {
                GhError::Missing => anyhow::anyhow!("pushed {name}, but opening the pull request needs the GitHub CLI (gh)"),
                GhError::Failed(msg) => anyhow::anyhow!("pushed {name}, but the pull request was not opened: {msg}"),
            })?;
            out.lines().map(str::trim).rfind(|l| l.starts_with("https://")).map(str::to_owned)
        };
        if let Some(url) = &pr {
            self.store.set_value(PR_KEY, &format!("{name} {url}")).await?;
        }
        Ok(DevShipped { commit: hash.to_owned(), message: subject.to_owned(), branch: name, pr, updated: reuse })
    }

    /// A commit message from the small model, or a plain one when it cannot answer.
    pub(super) async fn commit_message(&self, dir: &Path, stat: &str) -> String {
        let dir = dir.to_owned();
        let diff = tokio::task::spawn_blocking(move || git(&dir, &["diff", "--cached"]).unwrap_or_default())
            .await
            .unwrap_or_default();
        let diff: String = diff.chars().take(12_000).collect();
        let prompt = format!(
            "Write the commit message for this change to Hub, a desktop app. Answer with the message \
             only: one line of at most 72 characters saying what the user gets, in plain English, \
             no prefix, no quotes, no trailing period.\n\n{stat}\n\n{diff}"
        );
        let written = crate::engine::ask_tidy(&self.config, "notes", prompt, crate::notes::WAIT).await.unwrap_or_default();
        let line: String = written.lines().next().unwrap_or_default().trim().trim_matches('"').chars().take(100).collect();
        if line.is_empty() { "Changes from dev mode".to_owned() } else { line }
    }
}

/// Commits what is staged (with `message`) to `name`, a new branch unless `reuse`, rebases it
/// on `remote/base` and pushes it. Answers `<short hash> <subject>`.
pub(super) fn push_branch(dir: &Path, remote: &str, base: &str, name: &str, reuse: bool, message: Option<&str>) -> Result<String> {
    let upstream = format!("{remote}/{base}");
    git(dir, &["fetch", remote, base])?;
    if !reuse {
        // Uncommitted changes come along to the new branch.
        git(dir, &["switch", "-c", name])?;
    }
    if let Some(message) = message {
        git(dir, &["commit", "-m", message])?;
    }
    if git(dir, &["rev-list", "--count", &format!("{upstream}..HEAD")])? == "0" {
        bail!("nothing to ship: no changes and no commits that {upstream} lacks");
    }
    if git(dir, &["rebase", &upstream]).is_err() {
        git(dir, &["rebase", "--abort"]).warn_on_err("git rebase --abort");
        bail!("{upstream} moved and does not combine cleanly with these changes; ask a thread in Hub (dev) to rebase on it");
    }
    // The clone's own copy of the base branch goes back to where the remote is, in case commits
    // were made on it before.
    if git(dir, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{base}")]).is_ok() {
        git(dir, &["branch", "-f", base, &upstream])?;
    }
    git(dir, &["push", "--force-with-lease", "-u", remote, &format!("HEAD:refs/heads/{name}")])?;
    git(dir, &["log", "-1", "--format=%h %s"])
}
