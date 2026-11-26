//! A repository's open pull requests and latest CI run, asked of the GitHub CLI. Below
//! `project.rs` (which shows it on the project page) so the weekly health check (`health.rs`) can
//! ask the same without naming the project page.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::proc::{GhError, gh_within};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct GithubInfo {
    /// Why nothing is shown, e.g. the GitHub CLI is missing or not logged in.
    pub message: Option<String>,
    pub prs: Vec<PullRequest>,
    pub ci: Option<CiRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullRequest {
    #[ts(type = "number")]
    pub number: u64,
    pub title: String,
    pub url: String,
    #[serde(alias = "isDraft", default)]
    pub draft: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CiRun {
    pub name: String,
    /// queued, in_progress or completed.
    pub status: String,
    /// success, failure, cancelled, ... once completed.
    #[serde(default)]
    pub conclusion: String,
    pub url: String,
    #[serde(alias = "headBranch", default)]
    pub branch: String,
}

/// Open pull requests and the latest CI run, from the GitHub CLI when it is installed and
/// logged in. Never takes longer than a few seconds.
pub async fn github(root: &Path) -> GithubInfo {
    let none = |message: &str| GithubInfo { message: Some(message.to_owned()), prs: Vec::new(), ci: None };
    let prs = gh(root, &["pr", "list", "--state", "open", "--limit", "5", "--json", "number,title,url,isDraft"]);
    let runs = gh(root, &["run", "list", "--limit", "1", "--json", "name,status,conclusion,url,headBranch"]);
    let (prs, runs) = tokio::join!(prs, runs);
    let prs = match prs {
        Ok(out) => serde_json::from_str::<Vec<PullRequest>>(&out).unwrap_or_default(),
        Err(GhError::Missing) => return none("Install the GitHub CLI (gh) and run `gh auth login` to see PRs and CI here."),
        Err(GhError::Failed(msg)) => return none(&msg),
    };
    let ci = runs.ok().and_then(|out| serde_json::from_str::<Vec<CiRun>>(&out).ok()).and_then(|mut r| r.pop());
    GithubInfo { message: None, prs, ci }
}

async fn gh(root: &Path, args: &[&str]) -> std::result::Result<String, GhError> {
    gh_within(root, args, Duration::from_secs(8)).await
}
