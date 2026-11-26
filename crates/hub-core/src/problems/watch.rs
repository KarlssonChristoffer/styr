//! What the clock does to problems, every few minutes (from [`super::vakten::run`]):
//!
//! - follows the pull request of each problem's thread: notes it when it opens, and resolves the
//!   problem for the next release when it merges, so a hit from a newer release brings it back
//!   as regressed;
//! - brings archived problems back when their time is up;
//! - takes "escalating" off what calmed down;
//! - looks for quiet sources and slower releases (`quiet`, `metrics`).

use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::params;

use super::{ProblemPatch, ProblemPr, Resolve};
use crate::hub::Hub;
use crate::model::{StreamMsg, now_ms};

const DAY_MS: i64 = 86_400_000;
static PRUNED: Mutex<i64> = Mutex::new(0);

pub async fn sweep(hub: &Arc<Hub>) -> Result<()> {
    follow_prs(hub).await?;
    let now = now_ms();
    // Archives whose time is up, and escalations that calmed down.
    let changed: Vec<i64> = hub
        .store
        .with(move |c| {
            let mut changed = Vec::new();
            let mut q = c.prepare("SELECT id, archive FROM problems WHERE status = 'archived' AND archive LIKE '%\"time\"%'")?;
            let due: Vec<i64> = q
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|r| r.ok())
                .filter(|(_, a)| serde_json::from_str::<super::Archive>(a).is_ok_and(|a| matches!(a, super::Archive::Time { at } if at <= now)))
                .map(|(id, _)| id)
                .collect();
            for id in due {
                c.execute("UPDATE problems SET status = 'open', archive = NULL WHERE id = ?1", [id])?;
                super::log(c, id, "Hub", "status", "Back from the archive: its time was up")?;
                changed.push(id);
            }
            let today = now.div_euclid(DAY_MS);
            let mut q = c.prepare(
                "SELECT p.id FROM problems p WHERE p.escalating = 1 AND
                   (SELECT COALESCE(SUM(count), 0) FROM problem_days d WHERE d.problem_id = p.id AND d.day >= ?1) = 0",
            )?;
            let calm: Vec<i64> = q.query_map([today - 1], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            for id in calm {
                c.execute("UPDATE problems SET escalating = 0 WHERE id = ?1", [id])?;
                super::log(c, id, "Hub", "escalating", "Calmed down: no hits since yesterday")?;
                changed.push(id);
            }
            Ok(changed)
        })
        .await?;
    for id in changed {
        if let Some(problem) = super::get(hub, id).await? {
            hub.publish(StreamMsg::Problem { problem });
        }
    }
    super::quiet::sweep(hub).await?;
    super::metrics::sweep(hub).await?;
    let prune = {
        let mut at = PRUNED.lock().unwrap_or_else(|p| p.into_inner());
        let due = now - *at > DAY_MS;
        if due {
            *at = now;
        }
        due
    };
    if prune {
        hub.store.with(|c| Ok(super::rules::prune(c)?)).await?;
    }
    Ok(())
}

/// The pull request of each problem's thread: noted when it shows up or changes, and a merge
/// resolves the problem for the next release.
pub async fn follow_prs(hub: &Hub) -> Result<()> {
    let prs = hub.thread_prs();
    let open: Vec<(i64, String, Option<String>, String, bool)> = hub
        .store
        .with(|c| {
            // `settled`: a resolution is already in its log, so a reopened problem is left alone.
            let mut q = c.prepare(
                "SELECT id, thread_id, pr, status, regressed = 0 AND NOT EXISTS
                   (SELECT 1 FROM problem_activity a WHERE a.problem_id = problems.id AND a.kind = 'status' AND a.text LIKE 'Resolved%')
                 FROM problems WHERE thread_id IS NOT NULL AND merged_into IS NULL",
            )?;
            let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await?;
    for (id, thread, kept, status, never_resolved) in open {
        let before: Option<ProblemPr> = kept.as_deref().and_then(|k| serde_json::from_str(k).ok());
        let now = match prs.get(&thread) {
            Some(pr) => ProblemPr { number: pr.number, url: pr.url.clone(), state: pr.state.clone() },
            // The thread is archived or its worktree is gone, so its branch finds no pull request:
            // ask GitHub about the number kept on the problem instead.
            None => match before.as_ref().filter(|b| matches!(b.state.as_str(), "open" | "draft")) {
                Some(b) => match look_up(b).await {
                    Some(now) => now,
                    None => continue,
                },
                None => before.clone().filter(|b| b.state == "merged").unwrap_or_else(|| ProblemPr { number: 0, url: String::new(), state: String::new() }),
            },
        };
        if now.number == 0 {
            continue;
        }
        let unchanged = before.as_ref().is_some_and(|b| b.number == now.number && b.state == now.state);
        // A merge noted before the problem was resolved (the old miss) is caught up once.
        let catch_up = unchanged && now.state == "merged" && status == "open" && never_resolved;
        if unchanged && !catch_up {
            continue;
        }
        let json = serde_json::to_string(&now)?;
        let text = match now.state.as_str() {
            "merged" => format!("Pull request #{} merged", now.number),
            "closed" => format!("Pull request #{} closed without merging", now.number),
            _ => format!("Pull request #{} opened: {}", now.number, now.url),
        };
        if !catch_up {
            hub.store
                .with(move |c| {
                    c.execute("UPDATE problems SET pr = ?2 WHERE id = ?1", params![id, json])?;
                    super::log(c, id, "Hub", "pr", &text)?;
                    Ok(())
                })
                .await?;
        }
        let is_code = super::get(hub, id).await?.is_some_and(|p| p.source == super::analysis::SOURCE);
        if now.state == "merged" && status == "open" && !is_code {
            let patch = ProblemPatch { resolve: Some(Resolve::NextRelease), why: Some(format!("PR #{} merged", now.number)), ..Default::default() };
            super::change(hub, id, patch, &format!("PR #{}", now.number)).await?;
        } else if let Some(problem) = super::get(hub, id).await? {
            hub.publish(StreamMsg::Problem { problem });
        }
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhState {
    number: u32,
    url: String,
    state: String,
    #[serde(default)]
    is_draft: bool,
}

/// One pull request by its link, from anywhere: the link names the repository.
async fn look_up(pr: &ProblemPr) -> Option<ProblemPr> {
    let args = ["pr", "view", pr.url.as_str(), "--json", "number,url,state,isDraft"];
    let out = crate::proc::gh_within(&std::env::temp_dir(), &args, std::time::Duration::from_secs(15)).await.ok()?;
    let got: GhState = serde_json::from_str(&out).ok()?;
    let state = match (got.state.as_str(), got.is_draft) {
        ("MERGED", _) => "merged",
        ("CLOSED", _) => "closed",
        (_, true) => "draft",
        _ => "open",
    };
    Some(ProblemPr { number: got.number, url: got.url, state: state.into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::problems::{self, ProblemStatus};
    use crate::{Config, Store};

    fn report(release: &str) -> problems::Incoming {
        problems::Incoming::from_report(
            serde_json::from_value(serde_json::json!({ "project": "Knegarn", "level": "error", "message": "old failure", "release": release })).unwrap(),
            "app",
        )
    }

    #[tokio::test]
    async fn a_merge_noted_on_a_problem_resolves_it_even_when_its_thread_is_gone() {
        let config = Config { data_dir: crate::janitor::test_dir(), ..Default::default() };
        let hub = Hub::new(Store::in_memory().unwrap(), config);
        let p = problems::record(&hub, report("1.0")).await.unwrap();
        // The thread is archived, so no pull request is found by branch; the number is kept.
        let id = p.id;
        hub.store
            .with(move |c| {
                c.execute(
                    "UPDATE problems SET thread_id = 'gone', pr = ?2 WHERE id = ?1",
                    params![id, r#"{"number":9,"url":"https://github.com/o/r/pull/9","state":"merged"}"#],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        follow_prs(&hub).await.unwrap();
        assert_eq!(problems::get(&hub, id).await.unwrap().unwrap().status, ProblemStatus::Resolved);
        // A hit from a newer release reopens it; the same old merge does not resolve it again.
        problems::record(&hub, report("2.0")).await.unwrap();
        follow_prs(&hub).await.unwrap();
        assert_eq!(problems::get(&hub, id).await.unwrap().unwrap().status, ProblemStatus::Open);
    }
}
