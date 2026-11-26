//! The GitHub tab: a repository's pull requests, one of them in full, its diff, and what you can
//! do to it (merge, review, comment, draft, close). Everything goes through the GitHub CLI the
//! machine is logged in with, in the project folder, so `gh` picks the repository from its remote.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::git;
use crate::locked::Ignore;
use serde_json::Value;

use crate::model::{BranchList, BranchRow, IssueAction, IssueDetail, IssueList, IssueRow, PullAction, PullCard, PullCheck, PullComment, PullCommit, PullDetail, PullFile, PullList, PullRow};
use crate::proc::{GhError, gh_within};
use crate::triage::parse_time;

const ROW: &str = "number,title,url,state,isDraft,author,headRefName,baseRefName,additions,deletions,updatedAt,reviewDecision,statusCheckRollup,mergeable,labels,isCrossRepository";
const DETAIL: &str = "body,createdAt,mergedAt,comments,reviews,commits,files,reviewRequests,mergeStateStatus,autoMergeRequest";
const REPO: &str = "squashMergeAllowed,mergeCommitAllowed,rebaseMergeAllowed,deleteBranchOnMerge,viewerDefaultMergeMethod";
/// A diff bigger than this is cut, so a huge pull request cannot stall the page.
const MAX_DIFF: usize = 4 * 1024 * 1024;

fn gh_message(e: GhError) -> String {
    match e {
        GhError::Missing => "Install the GitHub CLI (gh) and run `gh auth login` to see pull requests here.".into(),
        GhError::Failed(msg) => msg,
    }
}

/// Open, merged or closed pull requests, newest activity first.
pub async fn pulls(root: &Path, state: &str) -> PullList {
    let state = match state {
        "merged" | "closed" | "all" => state,
        _ => "open",
    };
    let args = ["pr", "list", "--state", state, "--limit", "60", "--json", ROW];
    match gh_within(root, &args, Duration::from_secs(15)).await {
        Ok(out) => match serde_json::from_str::<Vec<Value>>(&out) {
            Ok(list) => {
                let mut pulls: Vec<PullRow> = list.iter().map(row).collect();
                pulls.sort_by_key(|x| std::cmp::Reverse(x.updated_at));
                PullList { message: None, pulls }
            }
            Err(e) => PullList { message: Some(format!("Could not read what gh answered: {e}")), pulls: Vec::new() },
        },
        Err(e) => PullList { message: Some(gh_message(e)), pulls: Vec::new() },
    }
}

/// One pull request with its description, checks, conversation, commits and files.
pub async fn pull(root: &Path, number: u64) -> Result<PullDetail> {
    let n = number.to_string();
    let fields = format!("{ROW},{DETAIL}");
    let comments = format!("repos/{{owner}}/{{repo}}/pulls/{n}/comments?per_page=100");
    let (view_args, lines_args) = (["pr", "view", &n, "--json", &fields], ["api", comments.as_str()]);
    let (view, lines, repo) = tokio::join!(
        gh_within(root, &view_args, Duration::from_secs(20)),
        gh_within(root, &lines_args, Duration::from_secs(20)),
        gh_within(root, &["repo", "view", "--json", REPO], Duration::from_secs(10)),
    );
    let view: Value = serde_json::from_str(&view.map_err(|e| anyhow::anyhow!(gh_message(e)))?)?;
    let lines: Value = lines.ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
    let repo: Value = repo.ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
    Ok(detail(&view, &lines, &repo))
}

const CARD: &str = "number,title,url,state,isDraft,author,additions,deletions,changedFiles,updatedAt,mergedAt,statusCheckRollup";

/// What a pull request's hover card shows. `repo` is `owner/name` when the chat named another
/// repository than the one in `root`; otherwise `gh` takes it from `root`'s remote.
pub async fn card(root: &Path, repo: &str, number: u64) -> Result<PullCard> {
    let n = number.to_string();
    let mut args = vec!["pr", "view", n.as_str(), "--json", CARD];
    let valid = !repo.is_empty() && repo.split('/').count() == 2 && repo.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c));
    if valid {
        args.extend(["--repo", repo]);
    }
    let out = gh_within(root, &args, Duration::from_secs(12)).await.map_err(|e| anyhow::anyhow!(gh_message(e)))?;
    Ok(card_from(&serde_json::from_str(&out)?))
}

fn card_from(v: &Value) -> PullCard {
    let checks = checks(&v["statusCheckRollup"]);
    let live: Vec<&PullCheck> = checks.iter().filter(|c| c.state != "skipped").collect();
    let rollup = if live.is_empty() {
        "none"
    } else if live.iter().any(|c| c.state == "fail") {
        "fail"
    } else if live.iter().any(|c| c.state == "pending") {
        "pending"
    } else {
        "pass"
    };
    let url = s(&v["url"]);
    let repo = url.split("github.com/").nth(1).map(|r| r.split('/').take(2).collect::<Vec<_>>().join("/")).unwrap_or_default();
    let state = match (s(&v["state"]).to_lowercase().as_str(), v["isDraft"].as_bool().unwrap_or(false)) {
        ("open", true) => "draft".to_owned(),
        (other, _) => other.to_owned(),
    };
    let when = if state == "merged" { time(&v["mergedAt"]) } else { 0 };
    PullCard {
        repo,
        number: v["number"].as_u64().unwrap_or(0),
        title: s(&v["title"]),
        url,
        state,
        author: login(&v["author"]),
        additions: v["additions"].as_u64().unwrap_or(0),
        deletions: v["deletions"].as_u64().unwrap_or(0),
        files: v["changedFiles"].as_u64().unwrap_or(0),
        checks: rollup.into(),
        checks_total: live.len() as u64,
        checks_done: live.iter().filter(|c| c.state != "pending").count() as u64,
        updated_at: if when > 0 { when } else { time(&v["updatedAt"]) },
    }
}

/// The pull request's diff, as `git diff` prints it.
pub async fn diff(root: &Path, number: u64) -> Result<String> {
    let mut text = gh_within(root, &["pr", "diff", &number.to_string()], Duration::from_secs(30))
        .await
        .map_err(|e| anyhow::anyhow!(gh_message(e)))?;
    if text.len() > MAX_DIFF {
        let mut cut = MAX_DIFF;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }
    Ok(text)
}

/// Does what the GitHub tab asked. Returns nothing; the tab reads the pull request again.
pub async fn act(a: &PullAction) -> Result<()> {
    let root = Path::new(&a.path);
    let n = a.number.to_string();
    let body = a.body.as_deref().map(str::trim).unwrap_or_default();
    let method = match a.method.as_deref().unwrap_or("squash") {
        "merge" => "--merge",
        "rebase" => "--rebase",
        _ => "--squash",
    };
    let args: Vec<&str> = match a.action.as_str() {
        "merge" => vec!["pr", "merge", &n, method],
        "auto_merge" => vec!["pr", "merge", &n, "--auto", method],
        "cancel_auto_merge" => vec!["pr", "merge", &n, "--disable-auto"],
        "comment" if body.is_empty() => bail!("write something first"),
        "comment" => vec!["pr", "comment", &n, "--body", body],
        "approve" => vec!["pr", "review", &n, "--approve", "--body", body],
        "request_changes" if body.is_empty() => bail!("say what should change"),
        "request_changes" => vec!["pr", "review", &n, "--request-changes", "--body", body],
        "ready" => vec!["pr", "ready", &n],
        "draft" => vec!["pr", "ready", &n, "--undo"],
        "close" => vec!["pr", "close", &n],
        "reopen" => vec!["pr", "reopen", &n],
        "update_branch" => vec!["pr", "update-branch", &n],
        "checkout" => vec!["pr", "checkout", &n],
        "delete_branch" => return delete_branch(root, &n).await,
        other => bail!("unknown action {other}"),
    };
    gh_within(root, &args, Duration::from_secs(90)).await.map_err(|e| anyhow::anyhow!(gh_message(e)))?;
    // gh's own --delete-branch also deletes the local branch and switches the folder away from
    // it, under any thread working there; only the branch on GitHub goes.
    if a.action == "merge" && a.delete_branch {
        delete_branch(root, &n).await?;
    }
    Ok(())
}

/// Deletes the pull request's branch on GitHub, unless it lives in a fork or is the default branch.
async fn delete_branch(root: &Path, n: &str) -> Result<()> {
    let out = gh_within(root, &["pr", "view", n, "--json", "headRefName,isCrossRepository"], Duration::from_secs(15))
        .await
        .map_err(|e| anyhow::anyhow!(gh_message(e)))?;
    let v: Value = serde_json::from_str(&out)?;
    // GitHub's own word on the default branch; the local origin/HEAD can be stale or missing.
    let default = match gh_within(root, &["repo", "view", "--json", "defaultBranchRef", "--jq", ".defaultBranchRef.name"], Duration::from_secs(15)).await {
        Ok(name) if !name.trim().is_empty() => name.trim().to_owned(),
        _ => default_branch(root),
    };
    let branch = deletable_head(&v, &default)?;
    let path = format!("repos/{{owner}}/{{repo}}/git/refs/heads/{branch}");
    match gh_within(root, &["api", "-X", "DELETE", &path], Duration::from_secs(20)).await {
        Ok(_) => Ok(()),
        // Already gone, e.g. the repository deletes merged branches by itself.
        Err(GhError::Failed(msg)) if msg.contains("Reference does not exist") || msg.contains("422") => Ok(()),
        Err(e) => bail!(gh_message(e)),
    }
}

/// The pull request's head branch, if it is one Hub may delete on GitHub.
fn deletable_head<'a>(v: &'a Value, default: &str) -> Result<&'a str> {
    let branch = v["headRefName"].as_str().unwrap_or_default();
    if branch.is_empty() || v["isCrossRepository"].as_bool().unwrap_or(false) {
        bail!("the branch is in someone else's fork");
    }
    if branch == default || branch == "main" || branch == "master" {
        bail!("{branch} is the default branch and stays");
    }
    Ok(branch)
}

const ISSUE: &str = "number,title,url,state,author,labels,comments,updatedAt";

/// Open or closed issues, newest activity first.
pub async fn issues(root: &Path, state: &str) -> IssueList {
    let state = if state == "closed" { "closed" } else { "open" };
    let args = ["issue", "list", "--state", state, "--limit", "60", "--json", ISSUE];
    match gh_within(root, &args, Duration::from_secs(15)).await {
        Ok(out) => match serde_json::from_str::<Vec<Value>>(&out) {
            Ok(list) => {
                let mut issues: Vec<IssueRow> = list.iter().map(issue_row).collect();
                issues.sort_by_key(|x| std::cmp::Reverse(x.updated_at));
                IssueList { message: None, issues }
            }
            Err(e) => IssueList { message: Some(format!("Could not read what gh answered: {e}")), issues: Vec::new() },
        },
        Err(e) => IssueList { message: Some(gh_message(e).replace("pull requests", "issues")), issues: Vec::new() },
    }
}

pub async fn issue(root: &Path, number: u64) -> Result<IssueDetail> {
    let fields = format!("{ISSUE},body,createdAt");
    let out = gh_within(root, &["issue", "view", &number.to_string(), "--json", &fields], Duration::from_secs(20))
        .await
        .map_err(|e| anyhow::anyhow!(gh_message(e)))?;
    let v: Value = serde_json::from_str(&out)?;
    let comments = v["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| PullComment {
            kind: "comment".into(),
            author: login(&c["author"]),
            body: s(&c["body"]),
            at: time(&c["createdAt"]),
            url: c["url"].as_str().map(str::to_owned),
            verdict: None,
            path: None,
            line: None,
        })
        .collect();
    Ok(IssueDetail { issue: issue_row(&v), body: s(&v["body"]), created_at: time(&v["createdAt"]), comments })
}

/// Does what was asked to an issue. Returns its number, which is new for "create".
pub async fn act_on_issue(a: &IssueAction) -> Result<u64> {
    let root = Path::new(&a.path);
    let n = a.number.to_string();
    let body = a.body.as_deref().map(str::trim).unwrap_or_default();
    let title = a.title.as_deref().map(str::trim).unwrap_or_default();
    let args: Vec<&str> = match a.action.as_str() {
        "create" if title.is_empty() => bail!("give it a title"),
        "create" => vec!["issue", "create", "--title", title, "--body", body],
        "comment" if body.is_empty() => bail!("write something first"),
        "comment" => vec!["issue", "comment", &n, "--body", body],
        "close" => vec!["issue", "close", &n],
        "reopen" => vec!["issue", "reopen", &n],
        other => bail!("unknown action {other}"),
    };
    let out = gh_within(root, &args, Duration::from_secs(60)).await.map_err(|e| anyhow::anyhow!(gh_message(e)))?;
    if a.action == "create" {
        // gh prints the new issue's URL, which ends in its number.
        return out.lines().rev().find_map(|l| l.trim().rsplit('/').next()?.parse().ok()).context("gh did not say which issue it made");
    }
    Ok(a.number)
}

fn issue_row(v: &Value) -> IssueRow {
    IssueRow {
        number: v["number"].as_u64().unwrap_or(0),
        title: s(&v["title"]),
        url: s(&v["url"]),
        state: s(&v["state"]).to_lowercase(),
        author: login(&v["author"]),
        labels: v["labels"].as_array().map(|l| l.iter().map(|l| s(&l["name"])).collect()).unwrap_or_default(),
        comments: v["comments"].as_array().map(|c| c.len() as u64).or_else(|| v["comments"].as_u64()).unwrap_or(0),
        updated_at: time(&v["updatedAt"]),
    }
}

/// The branches here and on GitHub, after a quick fetch, newest first.
pub fn branches(root: &Path) -> Result<BranchList> {
    // Best effort: offline, the list is what was last fetched.
    git::run(root, &["fetch", "--prune", "--quiet", "origin"]).ignore("offline, the list is what was last fetched");
    let default = default_branch(root);
    let base = if git::run(root, &["rev-parse", "--verify", "--quiet", &format!("origin/{default}")]).is_some() {
        format!("origin/{default}")
    } else {
        default.clone()
    };
    let current = git::run(root, &["branch", "--show-current"]).map(|s| s.trim().to_owned()).unwrap_or_default();
    let refs = git::run(root, &["for-each-ref", "--format=%(refname)%09%(committerdate:unix)%09%(subject)", "refs/heads", "refs/remotes/origin"])
        .context("not a git repository")?;
    let merged: std::collections::HashSet<String> = git::run(root, &["for-each-ref", "--format=%(refname)", "--merged", &base, "refs/heads", "refs/remotes/origin"])
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    let mut out: Vec<BranchRow> = Vec::new();
    for line in refs.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(full), Some(at), subject) = (parts.next(), parts.next(), parts.next().unwrap_or_default()) else { continue };
        let (name, remote) = match (full.strip_prefix("refs/heads/"), full.strip_prefix("refs/remotes/origin/")) {
            (Some(n), _) => (n, false),
            (None, Some("HEAD")) => continue,
            (None, Some(n)) => (n, true),
            _ => continue,
        };
        let at = at.parse::<i64>().unwrap_or(0) * 1000;
        let is_merged = merged.contains(full);
        match out.iter_mut().find(|b| b.name == name) {
            Some(b) => {
                b.local |= !remote;
                b.remote |= remote;
                b.merged &= is_merged;
                if at > b.at {
                    b.at = at;
                    b.subject = subject.to_owned();
                }
            }
            None => out.push(BranchRow {
                name: name.to_owned(),
                at,
                subject: subject.to_owned(),
                current: name == current,
                local: !remote,
                remote,
                merged: is_merged,
                ahead: 0,
            }),
        }
    }
    for b in &mut out {
        let tip = if b.local { b.name.clone() } else { format!("origin/{}", b.name) };
        b.ahead = git::run(root, &["rev-list", "--count", &format!("{base}..{tip}")]).and_then(|n| n.trim().parse().ok()).unwrap_or(0);
    }
    out.sort_by(|a, b| (b.name == default).cmp(&(a.name == default)).then(b.at.cmp(&a.at)));
    Ok(BranchList { default, branches: out })
}

/// Deletes a branch here, on GitHub, or both. Never the default branch or the one checked out.
pub fn delete_branches(root: &Path, names: &[String], local: bool, remote: bool) -> Result<Vec<String>> {
    let default = default_branch(root);
    let current = git::run(root, &["branch", "--show-current"]).map(|s| s.trim().to_owned()).unwrap_or_default();
    let mut failed = Vec::new();
    for name in names {
        if *name == default || name.starts_with('-') {
            failed.push(format!("{name}: the default branch stays"));
            continue;
        }
        if local && *name == current {
            failed.push(format!("{name}: checked out in the folder"));
            continue;
        }
        let has_local = git::run(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")]).is_some();
        if local && has_local && git::run(root, &["branch", "-D", name]).is_none() {
            failed.push(format!("{name}: git would not delete it here"));
        }
        let has_remote = git::run(root, &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/origin/{name}")]).is_some();
        if remote && has_remote && git::run(root, &["push", "origin", "--delete", name]).is_none() {
            failed.push(format!("{name}: GitHub would not delete it"));
        }
    }
    Ok(failed)
}

/// main or master: whatever origin's HEAD points at, else whichever exists.
pub fn default_branch(root: &Path) -> String {
    if let Some(head) = git::run(root, &["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"])
        && let Some(name) = head.trim().strip_prefix("origin/") {
            return name.to_owned();
        }
    for name in ["main", "master"] {
        if git::run(root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{name}")]).is_some() {
            return name.into();
        }
    }
    "main".into()
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}

fn time(v: &Value) -> i64 {
    v.as_str().and_then(parse_time).unwrap_or(0)
}

fn login(v: &Value) -> String {
    v["login"].as_str().or_else(|| v["name"].as_str()).unwrap_or("ghost").to_owned()
}

fn row(v: &Value) -> PullRow {
    let checks = checks(&v["statusCheckRollup"]);
    let rollup = if checks.is_empty() {
        "none"
    } else if checks.iter().any(|c| c.state == "fail") {
        "fail"
    } else if checks.iter().any(|c| c.state == "pending") {
        "pending"
    } else {
        "pass"
    };
    PullRow {
        number: v["number"].as_u64().unwrap_or(0),
        title: s(&v["title"]),
        url: s(&v["url"]),
        state: s(&v["state"]).to_lowercase(),
        draft: v["isDraft"].as_bool().unwrap_or(false),
        author: login(&v["author"]),
        head: s(&v["headRefName"]),
        base: s(&v["baseRefName"]),
        additions: v["additions"].as_u64().unwrap_or(0),
        deletions: v["deletions"].as_u64().unwrap_or(0),
        updated_at: time(&v["updatedAt"]),
        checks: rollup.into(),
        review: s(&v["reviewDecision"]).to_lowercase(),
        mergeable: match v["mergeable"].as_str() {
            Some("MERGEABLE") => "yes",
            Some("CONFLICTING") => "conflicts",
            _ => "unknown",
        }
        .into(),
        labels: v["labels"].as_array().map(|l| l.iter().map(|l| s(&l["name"])).collect()).unwrap_or_default(),
        cross_repo: v["isCrossRepository"].as_bool().unwrap_or(false),
    }
}

/// Check runs and commit statuses, the newest of each name.
fn checks(v: &Value) -> Vec<PullCheck> {
    let mut out: Vec<PullCheck> = Vec::new();
    for c in v.as_array().into_iter().flatten() {
        let (name, state, url) = if c["__typename"] == "StatusContext" {
            let state = match c["state"].as_str().unwrap_or_default() {
                "SUCCESS" => "pass",
                "FAILURE" | "ERROR" => "fail",
                _ => "pending",
            };
            (s(&c["context"]), state, c["targetUrl"].as_str())
        } else {
            let state = match (c["status"].as_str().unwrap_or_default(), c["conclusion"].as_str().unwrap_or_default()) {
                ("COMPLETED", "SUCCESS" | "NEUTRAL") => "pass",
                ("COMPLETED", "SKIPPED" | "STALE") => "skipped",
                ("COMPLETED", _) => "fail",
                _ => "pending",
            };
            let name = match (c["workflowName"].as_str(), c["name"].as_str()) {
                (Some(w), Some(n)) if !w.is_empty() && w != n => format!("{w} / {n}"),
                (_, Some(n)) => n.to_owned(),
                (Some(w), None) => w.to_owned(),
                _ => String::new(),
            };
            (name, state, c["detailsUrl"].as_str())
        };
        let check = PullCheck { name, state: state.into(), url: url.filter(|u| !u.is_empty()).map(str::to_owned) };
        match out.iter_mut().find(|o| o.name == check.name) {
            Some(o) => *o = check,
            None => out.push(check),
        }
    }
    out
}

fn detail(v: &Value, lines: &Value, repo: &Value) -> PullDetail {
    let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
    let mut comments: Vec<PullComment> = Vec::new();
    for c in list(&v["comments"]) {
        comments.push(PullComment {
            kind: "comment".into(),
            author: login(&c["author"]),
            body: s(&c["body"]),
            at: time(&c["createdAt"]),
            url: c["url"].as_str().map(str::to_owned),
            verdict: None,
            path: None,
            line: None,
        });
    }
    for r in list(&v["reviews"]) {
        let verdict = s(&r["state"]).to_lowercase();
        // A review with no words is only worth showing for its verdict; a bare "commented" is
        // the wrapper around line comments, which show on their own.
        if r["body"].as_str().unwrap_or_default().trim().is_empty() && verdict == "commented" {
            continue;
        }
        comments.push(PullComment {
            kind: "review".into(),
            author: login(&r["author"]),
            body: s(&r["body"]),
            at: time(&r["submittedAt"]),
            url: None,
            verdict: Some(verdict),
            path: None,
            line: None,
        });
    }
    for c in list(lines) {
        comments.push(PullComment {
            kind: "line".into(),
            author: login(&c["user"]),
            body: s(&c["body"]),
            at: time(&c["created_at"]),
            url: c["html_url"].as_str().map(str::to_owned),
            verdict: None,
            path: c["path"].as_str().map(str::to_owned),
            line: c["line"].as_u64().or_else(|| c["original_line"].as_u64()),
        });
    }
    comments.sort_by_key(|c| c.at);

    let commits = list(&v["commits"])
        .iter()
        .map(|c| PullCommit {
            oid: s(&c["oid"]),
            headline: s(&c["messageHeadline"]),
            author: c["authors"].as_array().and_then(|a| a.first()).map(login).unwrap_or_default(),
            at: time(&c["committedDate"]),
        })
        .collect();
    let files = list(&v["files"])
        .iter()
        .map(|f| PullFile { path: s(&f["path"]), additions: f["additions"].as_u64().unwrap_or(0), deletions: f["deletions"].as_u64().unwrap_or(0) })
        .collect();
    let reviewers = list(&v["reviewRequests"]).iter().map(login).collect();

    let mut methods: Vec<String> = [("squash", "squashMergeAllowed"), ("merge", "mergeCommitAllowed"), ("rebase", "rebaseMergeAllowed")]
        .into_iter()
        .filter(|(_, key)| repo[key].as_bool().unwrap_or(true))
        .map(|(m, _)| m.to_owned())
        .collect();
    let default = s(&repo["viewerDefaultMergeMethod"]).to_lowercase();
    if let Some(i) = methods.iter().position(|m| *m == default) {
        let m = methods.remove(i);
        methods.insert(0, m);
    }

    PullDetail {
        pull: row(v),
        body: s(&v["body"]),
        created_at: time(&v["createdAt"]),
        merged_at: v["mergedAt"].as_str().and_then(parse_time),
        checks: checks(&v["statusCheckRollup"]),
        comments,
        commits,
        files,
        reviewers,
        auto_merge: v["autoMergeRequest"].is_object(),
        merge_state: s(&v["mergeStateStatus"]).to_lowercase(),
        methods,
        deletes_branch: repo["deleteBranchOnMerge"].as_bool().unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "number": 58, "title": "Folder picker per machine", "url": "https://github.com/o/r/pull/58",
            "state": "OPEN", "isDraft": false, "author": { "login": "KarlssonChristoffer" },
            "headRefName": "claude/x", "baseRefName": "main", "additions": 812, "deletions": 94,
            "updatedAt": "2026-10-30T15:00:00Z", "reviewDecision": "APPROVED", "mergeable": "MERGEABLE",
            "labels": [{ "name": "ui" }],
            "statusCheckRollup": [
                { "__typename": "CheckRun", "name": "test", "workflowName": "CI", "status": "COMPLETED", "conclusion": "SUCCESS", "detailsUrl": "https://x/1" },
                { "__typename": "StatusContext", "context": "deploy", "state": "PENDING", "targetUrl": "" }
            ],
            "body": "What changed", "createdAt": "2026-10-30T12:00:00Z", "mergedAt": null,
            "comments": [{ "author": { "login": "a" }, "body": "Looks good", "createdAt": "2026-10-30T13:00:00Z", "url": "https://c/1" }],
            "reviews": [
                { "author": { "login": "b" }, "body": "", "state": "COMMENTED", "submittedAt": "2026-10-30T13:30:00Z" },
                { "author": { "login": "b" }, "body": "", "state": "APPROVED", "submittedAt": "2026-10-30T14:00:00Z" }
            ],
            "commits": [{ "oid": "abc", "messageHeadline": "Pick folders", "committedDate": "2026-10-30T11:00:00Z", "authors": [{ "login": "c", "name": "C" }] }],
            "files": [{ "path": "src/a.rs", "additions": 10, "deletions": 2 }],
            "reviewRequests": [{ "__typename": "User", "login": "d" }, { "__typename": "Team", "name": "core" }],
            "mergeStateStatus": "BLOCKED", "autoMergeRequest": { "mergeMethod": "SQUASH" }
        })
    }

    #[test]
    fn reads_a_row() {
        let r = row(&sample());
        assert_eq!((r.number, r.state.as_str(), r.author.as_str(), r.head.as_str()), (58, "open", "KarlssonChristoffer", "claude/x"));
        assert_eq!((r.checks.as_str(), r.review.as_str(), r.mergeable.as_str()), ("pending", "approved", "yes"));
        assert_eq!(r.labels, ["ui"]);
        assert!(r.updated_at > 0);
    }

    #[test]
    fn checks_roll_up() {
        let fail = json!([{ "__typename": "CheckRun", "name": "a", "status": "COMPLETED", "conclusion": "FAILURE" }, { "__typename": "CheckRun", "name": "b", "status": "IN_PROGRESS", "conclusion": "" }]);
        assert_eq!(row(&json!({ "statusCheckRollup": fail })).checks, "fail");
        assert_eq!(row(&json!({ "statusCheckRollup": [] })).checks, "none");
        // A re-run replaces the earlier run of the same name.
        let rerun = json!([{ "__typename": "CheckRun", "name": "a", "status": "COMPLETED", "conclusion": "FAILURE" }, { "__typename": "CheckRun", "name": "a", "status": "COMPLETED", "conclusion": "SUCCESS" }]);
        assert_eq!(row(&json!({ "statusCheckRollup": rerun })).checks, "pass");
        assert_eq!(checks(&sample()["statusCheckRollup"])[0].name, "CI / test");
    }

    #[test]
    fn reads_a_pull_request() {
        let lines = json!([{ "user": { "login": "e" }, "body": "Why?", "created_at": "2026-10-30T12:30:00Z", "path": "src/a.rs", "line": 88, "html_url": "https://c/2" }]);
        let repo = json!({ "squashMergeAllowed": true, "mergeCommitAllowed": true, "rebaseMergeAllowed": false, "deleteBranchOnMerge": true, "viewerDefaultMergeMethod": "MERGE" });
        let d = detail(&sample(), &lines, &repo);
        let kinds: Vec<&str> = d.comments.iter().map(|c| c.kind.as_str()).collect();
        // Oldest first; the empty "commented" review is left out, the approval stays.
        assert_eq!(kinds, ["line", "comment", "review"]);
        assert_eq!(d.comments[0].line, Some(88));
        assert_eq!(d.comments[2].verdict.as_deref(), Some("approved"));
        assert_eq!(d.methods, ["merge", "squash"]);
        assert_eq!(d.reviewers, ["d", "core"]);
        assert!(d.auto_merge && d.deletes_branch);
        assert_eq!((d.merge_state.as_str(), d.commits[0].author.as_str(), d.files[0].additions), ("blocked", "c", 10));
    }

    #[test]
    fn a_card_says_merged_draft_and_checks() {
        let merged = json!({ "number": 695, "title": "T", "url": "https://github.com/o/hub/pull/695", "state": "MERGED", "isDraft": false, "author": { "login": "k" },
            "additions": 1, "deletions": 1, "changedFiles": 1, "updatedAt": "2026-11-04T19:00:00Z", "mergedAt": "2026-11-04T19:30:00Z",
            "statusCheckRollup": [{ "__typename": "CheckRun", "name": "a", "status": "COMPLETED", "conclusion": "SUCCESS" }, { "__typename": "CheckRun", "name": "b", "status": "COMPLETED", "conclusion": "SKIPPED" }] });
        let c = card_from(&merged);
        assert_eq!((c.state.as_str(), c.repo.as_str(), c.checks.as_str(), c.checks_total, c.files), ("merged", "o/hub", "pass", 1, 1));
        assert!(c.updated_at > card_from(&json!({ "updatedAt": "2026-11-04T19:00:00Z" })).updated_at);
        let draft = json!({ "number": 1, "state": "OPEN", "isDraft": true, "statusCheckRollup": [{ "__typename": "CheckRun", "name": "a", "status": "IN_PROGRESS" }] });
        let d = card_from(&draft);
        assert_eq!((d.state.as_str(), d.checks.as_str(), d.checks_done), ("draft", "pending", 0));
    }

    fn git(dir: &Path, args: &[&str]) {
        assert!(crate::git::run(dir, args).is_some(), "git {args:?}");
    }

    #[test]
    fn lists_branches_and_what_is_merged() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["config", "user.email", "t@t"]);
        git(dir, &["config", "user.name", "t"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git(dir, &["branch", "done"]);
        git(dir, &["switch", "-q", "-c", "work"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", "more"]);
        let list = branches(dir).unwrap();
        assert_eq!(list.default, "main");
        let names: Vec<&str> = list.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names[0], "main");
        let work = list.branches.iter().find(|b| b.name == "work").unwrap();
        assert!(work.current && !work.merged && work.ahead == 1 && work.subject == "more");
        assert!(list.branches.iter().find(|b| b.name == "done").unwrap().merged);
        let failed = delete_branches(dir, &["main".into(), "work".into(), "done".into()], true, false).unwrap();
        assert_eq!(failed.len(), 2);
        assert!(!branches(dir).unwrap().branches.iter().any(|b| b.name == "done"));
    }

    #[test]
    fn reads_an_issue_row() {
        let i = issue_row(&json!({ "number": 3, "title": "Crash", "state": "OPEN", "author": { "login": "a" }, "comments": [{}, {}], "labels": [{ "name": "bug" }], "updatedAt": "2026-10-30T10:00:00Z" }));
        assert_eq!((i.number, i.state.as_str(), i.comments, i.labels[0].as_str()), (3, "open", 2, "bug"));
    }

    #[test]
    fn never_deletes_the_default_branch() {
        let pr = |head: &str, fork: bool| json!({ "headRefName": head, "isCrossRepository": fork });
        assert_eq!(deletable_head(&pr("claude/x", false), "main").unwrap(), "claude/x");
        assert!(deletable_head(&pr("main", false), "main").is_err());
        assert!(deletable_head(&pr("develop", false), "develop").is_err());
        assert!(deletable_head(&pr("master", false), "trunk").is_err());
        assert!(deletable_head(&pr("claude/x", true), "main").is_err());
        assert!(deletable_head(&pr("", false), "main").is_err());
    }
}
