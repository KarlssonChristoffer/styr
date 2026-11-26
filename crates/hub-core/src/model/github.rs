//! Model types: github.

use super::*;

/// The small card shown when a pull request is hovered in a chat (`github::card`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullCard {
    /// `owner/name`, from the pull request's own address.
    pub repo: String,
    #[ts(type = "number")]
    pub number: u64,
    pub title: String,
    pub url: String,
    /// "open", "draft", "merged" or "closed".
    pub state: String,
    pub author: String,
    #[ts(type = "number")]
    pub additions: u64,
    #[ts(type = "number")]
    pub deletions: u64,
    #[ts(type = "number")]
    pub files: u64,
    /// "pass", "fail", "pending" or "none", and how many checks there are and how many are done.
    pub checks: String,
    #[ts(type = "number")]
    pub checks_total: u64,
    #[ts(type = "number")]
    pub checks_done: u64,
    /// When it last changed (a merged one: when it merged), milliseconds since 1970.
    #[ts(type = "number")]
    pub updated_at: i64,
}

/// One pull request in the GitHub tab's list.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullRow {
    #[ts(type = "number")]
    pub number: u64,
    pub title: String,
    pub url: String,
    /// "open", "merged" or "closed".
    pub state: String,
    pub draft: bool,
    pub author: String,
    /// The branch it brings in, and the one it goes into.
    pub head: String,
    pub base: String,
    #[ts(type = "number")]
    pub additions: u64,
    #[ts(type = "number")]
    pub deletions: u64,
    #[ts(type = "number")]
    pub updated_at: i64,
    /// All checks together: "pass", "fail", "pending" or "none".
    pub checks: String,
    /// "approved", "changes_requested", "review_required", or empty when no review is needed.
    pub review: String,
    /// Whether it merges into its base without conflicts: "yes", "conflicts" or "unknown".
    pub mergeable: String,
    pub labels: Vec<String>,
    /// Its branch lives in a fork.
    #[serde(default)]
    pub cross_repo: bool,
}

/// The merge queue, removed. Only the shape stays, so `/api/mergeq` still answers (always
/// "off") for apps that have not updated yet.
// compat-until: 2026-12-09 (with the /api/mergeq route in api/github.rs)
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MergeQueue {
    /// "off", "test" (test each pull request, merge nothing) or "merge" (merge the green ones).
    pub mode: String,
    /// Why the queue cannot run here, or what went wrong last time.
    pub note: Option<String>,
    /// What runs right now, e.g. "Testing #12, #13: step 2 of 3: npm run check".
    pub step: Option<String>,
    #[ts(type = "number | null")]
    pub checked_at: Option<i64>,
    /// The file the checks write their output to.
    pub log: Option<String>,
    /// Open ones in the order they are taken, then the last few merged.
    pub rows: Vec<QueueRow>,
}

/// One pull request in the merge queue.
// compat-until: 2026-12-09 (with the /api/mergeq route in api/github.rs)
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct QueueRow {
    #[ts(type = "number")]
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: String,
    pub branch: String,
    /// The Hub thread whose branch it is, when one is.
    pub thread_id: Option<ThreadId>,
    /// "queued", "testing", "passed", "failed", "conflict", "merging", "merged" or "held".
    pub state: String,
    /// Why it failed or conflicts, or what it waits for.
    pub note: Option<String>,
    /// The commit of the branch that was looked at last.
    pub head: String,
    #[ts(type = "number")]
    pub since: i64,
    #[ts(type = "number | null")]
    pub merged_at: Option<i64>,
    /// Merged and part of the version this machine runs.
    pub in_version: bool,
}

/// A change to the merge queue: its mode, or one pull request held, let go or tried again.
// compat-until: 2026-12-09 (with the /api/mergeq route in api/github.rs)
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct QueueAsk {
    pub mode: Option<String>,
    #[ts(type = "number | null")]
    pub number: Option<u64>,
    pub hold: Option<bool>,
    pub retry: bool,
    /// Look at the pull requests now instead of at the next tick.
    pub check: bool,
}

/// A repository on GitHub the signed-in account can reach (`GET /api/github/repos`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GithubRepo {
    /// `owner/name`.
    pub full: String,
    pub name: String,
    pub owner: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub private: bool,
    #[serde(default)]
    pub archived: bool,
    /// Last push, in ms; 0 when unknown.
    #[ts(type = "number")]
    pub pushed: i64,
    pub url: String,
}

/// The repositories of the GitHub account `gh` is signed in as on this machine, or why there are
/// none to show.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GithubRepos {
    /// The account's login.
    #[serde(default)]
    pub login: Option<String>,
    /// Why nothing is shown, e.g. the GitHub CLI is missing or not signed in.
    #[serde(default)]
    pub message: Option<String>,
    pub repos: Vec<GithubRepo>,
}

/// Where this machine stands with GitHub (`/api/github/connect`). `state` is `ready` (signed in),
/// `lost` (gh has a sign-in that GitHub rejects),
/// `waiting` (the sign-in runs and wants `code` typed at `url`), `none` (not signed in), `missing`
/// (no `gh` here) or `failed` (the last sign-in did not finish; `message` says why). A client
/// that reads another value treats it as `none`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GithubConnect {
    pub state: String,
    #[serde(default)]
    pub login: Option<String>,
    /// The one-time code to type on GitHub while `waiting`.
    #[serde(default)]
    pub code: Option<String>,
    /// Where to type it (`https://github.com/login/device`).
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    /// The account's display name, when `ready`.
    #[serde(default)]
    pub name: Option<String>,
    /// The account's picture, when `ready`.
    #[serde(default)]
    pub avatar: Option<String>,
    /// What the sign-in on this machine may do (`repo`, `read:org`, ...), when `ready`.
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// The pull requests of a repository, or why there are none to show.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullList {
    /// Why nothing is shown, e.g. the GitHub CLI is missing or not logged in.
    pub message: Option<String>,
    pub pulls: Vec<PullRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullCheck {
    pub name: String,
    /// "pass", "fail", "pending" or "skipped".
    pub state: String,
    pub url: Option<String>,
}

/// Something said on a pull request: a comment, a review, or a comment on a line of a file.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullComment {
    /// "comment", "review" or "line".
    pub kind: String,
    pub author: String,
    pub body: String,
    #[ts(type = "number")]
    pub at: i64,
    pub url: Option<String>,
    /// A review's verdict: "approved", "changes_requested" or "commented".
    pub verdict: Option<String>,
    /// The file and line a line comment is on.
    pub path: Option<String>,
    #[ts(type = "number | null")]
    pub line: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullCommit {
    pub oid: String,
    pub headline: String,
    pub author: String,
    #[ts(type = "number")]
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullFile {
    pub path: String,
    #[ts(type = "number")]
    pub additions: u64,
    #[ts(type = "number")]
    pub deletions: u64,
}

/// One pull request opened in the GitHub tab.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullDetail {
    pub pull: PullRow,
    pub body: String,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number | null")]
    pub merged_at: Option<i64>,
    pub checks: Vec<PullCheck>,
    /// Comments, reviews and line comments, oldest first.
    pub comments: Vec<PullComment>,
    pub commits: Vec<PullCommit>,
    pub files: Vec<PullFile>,
    /// People and teams asked to review who have not yet.
    pub reviewers: Vec<String>,
    /// GitHub merges it by itself once the checks and reviews it needs are in.
    pub auto_merge: bool,
    /// GitHub's word for whether it can merge now: "clean", "blocked", "behind", "dirty",
    /// "unstable", "draft" or "unknown".
    pub merge_state: String,
    /// The merge methods the repository allows, the default first: "squash", "merge", "rebase".
    pub methods: Vec<String>,
    /// The repository deletes the branch by itself after a merge.
    pub deletes_branch: bool,
}

/// Something to do to a pull request from the GitHub tab.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PullAction {
    /// The project folder, which tells which repository.
    pub path: String,
    #[ts(type = "number")]
    pub number: u64,
    /// "merge", "auto_merge", "cancel_auto_merge", "comment", "approve", "request_changes",
    /// "ready", "draft", "close", "reopen", "update_branch", "checkout" or "delete_branch".
    pub action: String,
    /// For a merge: "squash", "merge" or "rebase".
    #[serde(default)]
    pub method: Option<String>,
    /// For a comment or a review.
    #[serde(default)]
    pub body: Option<String>,
    /// For a merge: delete the branch on GitHub afterwards.
    #[serde(default)]
    pub delete_branch: bool,
}

/// One issue in the GitHub tab.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IssueRow {
    #[ts(type = "number")]
    pub number: u64,
    pub title: String,
    pub url: String,
    /// "open" or "closed".
    pub state: String,
    pub author: String,
    pub labels: Vec<String>,
    #[ts(type = "number")]
    pub comments: u64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IssueList {
    pub message: Option<String>,
    pub issues: Vec<IssueRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IssueDetail {
    pub issue: IssueRow,
    pub body: String,
    #[ts(type = "number")]
    pub created_at: i64,
    /// Oldest first.
    pub comments: Vec<PullComment>,
}

/// Something to do to an issue, or a new one.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IssueAction {
    pub path: String,
    /// 0 when creating one.
    #[serde(default)]
    #[ts(type = "number")]
    pub number: u64,
    /// "create", "comment", "close" or "reopen".
    pub action: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

/// A branch of the repository, here and on GitHub.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BranchRow {
    pub name: String,
    /// When its newest commit was made.
    #[ts(type = "number")]
    pub at: i64,
    pub subject: String,
    /// Checked out in the folder.
    pub current: bool,
    pub local: bool,
    /// On GitHub (origin).
    pub remote: bool,
    /// Everything on it is in the default branch already.
    pub merged: bool,
    /// Commits it has that the default branch has not.
    #[ts(type = "number")]
    pub ahead: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BranchList {
    pub default: String,
    pub branches: Vec<BranchRow>,
}

/// Where a thread's folder stands with git, for its Git button.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitState {
    pub branch: Option<String>,
    /// main or master, whichever origin points at.
    pub default: String,
    /// Files with uncommitted changes.
    pub files: Vec<String>,
    /// The branch exists on GitHub.
    pub upstream: bool,
    /// Commits not yet pushed, and commits on GitHub not yet pulled.
    #[ts(type = "number")]
    pub ahead: u32,
    #[ts(type = "number")]
    pub behind: u32,
    /// The folder has a GitHub remote.
    pub github: bool,
}

/// What the Git button should do: any of commit, push and open a pull request, in that order,
/// or pull.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitShip {
    pub cwd: String,
    #[serde(default)]
    pub commit: bool,
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub pr: bool,
    #[serde(default)]
    pub pull: bool,
    /// The commit message.
    #[serde(default)]
    pub message: Option<String>,
    /// The pull request's title and description.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitShipped {
    /// The commit made, short.
    pub commit: Option<String>,
    pub pushed: bool,
    /// The pull request opened.
    pub pr_url: Option<String>,
}

/// Words for the Git button, written by a model from the changes and the thread.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitDraft {
    pub message: String,
    pub title: String,
    pub body: String,
}
