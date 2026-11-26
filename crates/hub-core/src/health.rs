//! The weekly health check: every project looked over without running anything in it. Is CI
//! green, is work left uncommitted or unpushed, are branches lying around, has it gone quiet.
//! Building and testing is left to a thread the user starts from the report, since that runs
//! the project's own code and takes tokens.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::git;
use crate::ctx::Ctx;
use crate::config::Config;
use crate::store::Store;
use crate::model::{now_ms, Health, HealthCheck, HealthReport, ProjectHealth};

const REPORT: &str = "health.report";
const WEEK_MS: i64 = 7 * 86_400_000;
const DAY_MS: i64 = 86_400_000;
/// A branch without a commit for this long is stale.
const STALE_DAYS: i64 = 60;
/// A project without a commit for this long has gone quiet.
const QUIET_DAYS: i64 = 30;

/// Runs the check when the last one is a week old, looking every hour. Takes the database and
/// the settings it was started with, not the Hub, so this module names nothing above it.
pub async fn weekly(store: Store, config: Config) {
    let ctx = Ctx::new(&store, &config);
    tokio::time::sleep(Duration::from_secs(300)).await;
    loop {
        let due = match last(&ctx).await {
            Ok(r) => now_ms() - r.checked_at >= WEEK_MS,
            Err(_) => true,
        };
        if due {
            match check_all(&ctx).await {
                Ok(r) => tracing::info!("health check of {} projects done", r.projects.len()),
                Err(err) => tracing::warn!("health check: {err:#}"),
            }
        }
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

/// The latest report, empty when none has run.
pub async fn last(ctx: &Ctx<'_>) -> Result<HealthReport> {
    Ok(match ctx.store.get_value(REPORT).await? {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => HealthReport::default(),
    })
}

/// The projects the last check found broken or worth a look, when it ran at or after `since`
/// (for "While you were away"); empty when it did not.
pub async fn troubled_since(ctx: &Ctx<'_>, since: i64) -> Result<Vec<ProjectHealth>> {
    let report = last(ctx).await?;
    if report.checked_at < since {
        return Ok(Vec::new());
    }
    Ok(report.projects.into_iter().filter(|p| matches!(p.health, Health::Warn | Health::Bad)).collect())
}

/// Checks every project now and keeps the report.
pub async fn check_all(ctx: &Ctx<'_>) -> Result<HealthReport> {
    let mut projects = Vec::new();
    for (name, path) in roots(ctx).await? {
        projects.push(check(&name, Path::new(&path)).await);
    }
    projects.sort_by(|a, b| b.health.cmp(&a.health).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    let report = HealthReport { checked_at: now_ms(), projects };
    ctx.store.set_value(REPORT, &serde_json::to_string(&report)?).await?;
    Ok(report)
}

/// Stored projects, and the repositories threads have worked in, each once.
async fn roots(ctx: &Ctx<'_>) -> Result<Vec<(String, String)>> {
    let mut folders: Vec<(Option<String>, String)> =
        ctx.store.projects().await?.into_iter().map(|p| (Some(p.name), p.path)).collect();
    folders.extend(ctx.store.threads().await?.into_iter().filter(|t| t.host.is_none()).map(|t| (None, t.cwd)));
    tokio::task::spawn_blocking(move || {
        let mut out: Vec<(String, String)> = Vec::new();
        for (name, folder) in folders {
            let path = Path::new(&folder);
            // A thread's folder counts by its repository; a stored project even when gone.
            let top = git::run(path, &["rev-parse", "--show-toplevel"]).map(|t| t.trim().to_owned());
            let root = match (&name, top) {
                (_, Some(top)) => top,
                (Some(_), None) => folder.clone(),
                (None, None) => continue,
            };
            let same = |p: &str| p.replace('\\', "/").to_lowercase() == root.replace('\\', "/").to_lowercase();
            if out.iter().any(|(_, p)| same(p)) {
                continue;
            }
            let name = name.unwrap_or_else(|| {
                Path::new(&root).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.clone())
            });
            out.push((name, root));
        }
        out
    })
    .await
    .map_err(Into::into)
}

async fn check(name: &str, root: &Path) -> ProjectHealth {
    let mut checks = Vec::new();
    if !root.is_dir() {
        checks.push(item("Folder", Health::Bad, "The folder is gone", None));
    } else {
        let dir = root.to_path_buf();
        let local = tokio::task::spawn_blocking(move || local_checks(&dir, now_ms())).await.unwrap_or_default();
        checks.extend(local);
        if checks.iter().any(|c| c.label != "Git") {
            checks.extend(ci(root).await);
        }
    }
    let health = checks.iter().map(|c| c.health).max().unwrap_or_default();
    ProjectHealth { name: name.to_owned(), path: root.display().to_string(), health, checks }
}

fn item(label: &str, health: Health, detail: impl Into<String>, url: Option<String>) -> HealthCheck {
    HealthCheck { label: label.to_owned(), health, detail: detail.into(), url }
}

fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// What git says without asking the network.
fn local_checks(root: &Path, now: i64) -> Vec<HealthCheck> {
    let run = |args: &[&str]| git::run(root, args).map(|s| s.trim().to_owned());
    let Some(branch) = run(&["rev-parse", "--abbrev-ref", "HEAD"]) else {
        return vec![item("Git", Health::Warn, "Not in git, so nothing is backed up", None)];
    };
    let mut out = Vec::new();
    let changed = run(&["status", "--porcelain"]).map_or(0, |s| s.lines().count() as i64);
    out.push(if changed > 0 {
        item("Uncommitted", Health::Warn, format!("{} not committed", plural(changed, "file", "files")), None)
    } else {
        item("Uncommitted", Health::Ok, "Everything is committed", None)
    });
    out.push(match run(&["rev-list", "--count", "@{upstream}..HEAD"]).and_then(|n| n.parse::<i64>().ok()) {
        Some(0) => item("Pushed", Health::Ok, format!("{branch} is pushed"), None),
        Some(n) => item("Pushed", Health::Warn, format!("{} on {branch} not pushed", plural(n, "commit", "commits")), None),
        None if run(&["remote"]).is_some_and(|r| r.is_empty()) => item("Pushed", Health::Warn, "No remote: the work lives only here", None),
        None => item("Pushed", Health::Warn, format!("{branch} has no upstream branch"), None),
    });
    if let Some(last) = run(&["log", "-1", "--format=%ct"]).and_then(|t| t.parse::<i64>().ok()) {
        let days = (now - last * 1000) / DAY_MS;
        if days >= QUIET_DAYS {
            out.push(item("Activity", Health::Warn, format!("No commit in {days} days"), None));
        }
    }
    let default = run(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .and_then(|r| r.strip_prefix("origin/").map(str::to_owned))
        .unwrap_or_else(|| if run(&["rev-parse", "--verify", "-q", "main"]).is_some() { "main".into() } else { "master".into() });
    let merged: Vec<String> = run(&["branch", "--merged", &default, "--format=%(refname:short)"])
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .filter(|b| b != &default && b != &branch)
        .collect();
    let stale = run(&["for-each-ref", "refs/heads", "--format=%(refname:short)%09%(committerdate:unix)"])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(b, t)| *b != default && *b != branch && !merged.iter().any(|m| m == b) && t.parse::<i64>().is_ok_and(|t| now - t * 1000 > STALE_DAYS * DAY_MS))
        .count() as i64;
    let mut dead = Vec::new();
    if !merged.is_empty() {
        dead.push(format!("{} merged into {default}", plural(merged.len() as i64, "branch", "branches")));
    }
    if stale > 0 {
        dead.push(format!("{} untouched for {STALE_DAYS}+ days", plural(stale, "branch", "branches")));
    }
    if !dead.is_empty() {
        out.push(item("Branches", Health::Warn, dead.join(", "), None));
    }
    out
}

/// The latest CI run and open pull requests, when the GitHub CLI can tell.
async fn ci(root: &Path) -> Vec<HealthCheck> {
    let gh = crate::project_github::github(root).await;
    if gh.message.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(run) = gh.ci {
        let on = if run.branch.is_empty() { String::new() } else { format!(" on {}", run.branch) };
        out.push(match (run.status.as_str(), run.conclusion.as_str()) {
            ("completed", "success") => item("CI", Health::Ok, format!("{} passed{on}", run.name), Some(run.url)),
            ("completed", "failure" | "timed_out" | "startup_failure") => {
                item("CI", Health::Bad, format!("{} failed{on}", run.name), Some(run.url))
            }
            ("completed", other) => item("CI", Health::Unknown, format!("{} {other}{on}", run.name), Some(run.url)),
            _ => item("CI", Health::Unknown, format!("{} is running{on}", run.name), Some(run.url)),
        });
    }
    if !gh.prs.is_empty() {
        let n = gh.prs.len() as i64;
        out.push(item("Pull requests", Health::Ok, format!("{} open", plural(n, "pull request", "pull requests")), None));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        assert!(git::run(dir, args).is_some(), "git {args:?}");
    }

    #[test]
    fn a_fresh_repository_shows_what_is_left_undone() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["config", "user.email", "me@example.com"]);
        git(dir, &["config", "user.name", "Me"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git(dir, &["branch", "done"]);
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        let checks = local_checks(dir, now_ms());
        let find = |l: &str| checks.iter().find(|c| c.label == l).unwrap_or_else(|| panic!("{l}: {checks:?}"));
        assert_eq!((find("Uncommitted").health, find("Uncommitted").detail.as_str()), (Health::Warn, "1 file not committed"));
        assert_eq!(find("Pushed").detail, "No remote: the work lives only here");
        assert_eq!(find("Branches").detail, "1 branch merged into main");
        assert!(checks.iter().all(|c| c.label != "Activity"));
        // A year later it has gone quiet.
        let later = local_checks(dir, now_ms() + 365 * DAY_MS);
        assert!(later.iter().any(|c| c.label == "Activity" && c.detail.starts_with("No commit in 36")), "{later:?}");
    }

    #[test]
    fn a_folder_outside_git_is_said_so() {
        let tmp = crate::janitor::tempdir_outside_git();
        let checks = local_checks(tmp.path(), now_ms());
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].label, "Git");
    }

    #[test]
    fn the_worst_check_wins() {
        assert!(Health::Bad > Health::Warn && Health::Warn > Health::Ok && Health::Ok > Health::Unknown);
    }
}
