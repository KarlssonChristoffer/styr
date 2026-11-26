//! Your GitHub contributions, through the `gh` CLI you are logged in with: the same calendar
//! GitHub draws on your profile, across every repository, plus which repositories and pull
//! requests each day's work went to.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::proc::{GhError, gh_within};

#[derive(Debug, Clone, Default)]
pub struct Github {
    /// Contributions (commits, pull requests, issues, reviews) per `YYYY-MM-DD`.
    pub days: BTreeMap<String, u32>,
    /// `(date, repository, commits)`.
    pub commits: Vec<(String, String, u32)>,
    /// `(date, repository, title, url)` of pull requests opened.
    pub pulls: Vec<(String, String, String, String)>,
}

const QUERY: &str = "query($from: DateTime!) { viewer { contributionsCollection(from: $from) {
  contributionCalendar { weeks { contributionDays { date contributionCount } } }
  commitContributionsByRepository(maxRepositories: 100) {
    repository { nameWithOwner }
    contributions(first: 100) { nodes { occurredAt commitCount } }
  }
  pullRequestContributions(first: 100) {
    nodes { occurredAt pullRequest { title url repository { nameWithOwner } } }
  }
} } }";

/// The year up to now. GitHub allows at most a year per question. `from` is `YYYY-MM-DD`.
pub async fn year(from: &str) -> Result<Github, String> {
    let args = ["api", "graphql", "-f", &format!("query={QUERY}"), "-f", &format!("from={from}T00:00:00Z")];
    let dir = std::env::temp_dir();
    match gh_within(Path::new(&dir), &args, Duration::from_secs(20)).await {
        Ok(out) => serde_json::from_str::<Value>(&out).map(|v| parse(&v)).map_err(|e| e.to_string()),
        Err(GhError::Missing) => Err("Install the GitHub CLI (gh) and run `gh auth login` to include GitHub.".into()),
        Err(GhError::Failed(msg)) => Err(format!("GitHub: {msg}")),
    }
}

pub fn parse(v: &Value) -> Github {
    let c = &v["data"]["viewer"]["contributionsCollection"];
    let list = |v: &Value| v.as_array().cloned().unwrap_or_default();
    let mut out = Github::default();
    for week in list(&c["contributionCalendar"]["weeks"]) {
        for day in list(&week["contributionDays"]) {
            let (Some(date), Some(n)) = (day["date"].as_str(), day["contributionCount"].as_u64()) else { continue };
            if n > 0 {
                out.days.insert(date.to_owned(), n as u32);
            }
        }
    }
    for repo in list(&c["commitContributionsByRepository"]) {
        let name = repo["repository"]["nameWithOwner"].as_str().unwrap_or_default();
        for node in list(&repo["contributions"]["nodes"]) {
            let (Some(at), Some(n)) = (node["occurredAt"].as_str(), node["commitCount"].as_u64()) else { continue };
            out.commits.push((at.chars().take(10).collect(), name.to_owned(), n as u32));
        }
    }
    for node in list(&c["pullRequestContributions"]["nodes"]) {
        let pr = &node["pullRequest"];
        let (Some(at), Some(title), Some(url)) = (node["occurredAt"].as_str(), pr["title"].as_str(), pr["url"].as_str()) else {
            continue;
        };
        let repo = pr["repository"]["nameWithOwner"].as_str().unwrap_or_default();
        out.pulls.push((at.chars().take(10).collect(), repo.to_owned(), title.to_owned(), url.to_owned()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_calendar_repositories_and_pull_requests_are_read() {
        let v = serde_json::json!({ "data": { "viewer": { "contributionsCollection": {
            "contributionCalendar": { "weeks": [ { "contributionDays": [
                { "date": "2026-10-29", "contributionCount": 7 },
                { "date": "2026-10-30", "contributionCount": 0 }
            ] } ] },
            "commitContributionsByRepository": [ { "repository": { "nameWithOwner": "me/hub" },
                "contributions": { "nodes": [ { "occurredAt": "2026-10-29T07:00:00Z", "commitCount": 5 } ] } } ],
            "pullRequestContributions": { "nodes": [ { "occurredAt": "2026-10-29T08:00:00Z",
                "pullRequest": { "title": "Labs", "url": "https://github.com/me/hub/pull/2", "repository": { "nameWithOwner": "me/hub" } } } ] }
        } } } });
        let g = parse(&v);
        assert_eq!(g.days.len(), 1);
        assert_eq!(g.days["2026-10-29"], 7);
        assert_eq!(g.commits, vec![("2026-10-29".into(), "me/hub".into(), 5)]);
        assert_eq!(g.pulls[0].2, "Labs");
    }
}
