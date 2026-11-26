//! What is going on right now, as small as a widget or a lock screen can carry (`GET
//! /api/phone/now`): the threads that work, the ones that wait on you with the question they ask,
//! and the plan limits. One call, one small answer, so a widget extension with a few megabytes to
//! spend does not have to read the whole thread list. It is the same picture as the Now and Limit
//! cards on the Overview, from the same data.
//!
//! Only plain facts: whether a question is safe to answer from a lock screen is for the client to
//! judge, and one it cannot judge is one to open, not to answer.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::hub::Hub;
use crate::model::{EventKind, Thread, ThreadStatus, now_ms};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Now {
    /// When this was read, unix milliseconds, so a widget can say how old it is.
    #[ts(type = "number")]
    pub at: i64,
    /// Threads that wait on you, the oldest wait first.
    pub waiting: Vec<NowThread>,
    /// Threads that work, the longest running first.
    pub working: Vec<NowThread>,
    /// The plan limits of every subscription Hub reads, Claude's first.
    pub limits: Vec<NowLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NowThread {
    pub id: String,
    pub title: String,
    /// The project's folder name.
    pub project: String,
    /// The machine, when it is not this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub host: Option<String>,
    /// Since when it works (the last message you sent it), or since when it waits. Unix
    /// milliseconds, so a clock can run from it without asking again.
    #[ts(type = "number")]
    pub since: i64,
    /// The question it waits on, when it is a permission question on this machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ask: Option<NowAsk>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NowAsk {
    /// The request to answer: `POST /api/threads/{id}/permissions/{request}`.
    pub request: String,
    /// What the agent wants to do, as it words it.
    pub what: String,
    /// The command, when it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub command: Option<String>,
    pub options: Vec<NowOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NowOption {
    /// What to send as `option_id`.
    pub id: String,
    pub name: String,
    /// "allow_once", "allow_always", "reject_once" or "reject_always".
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NowLimit {
    /// Whose it is: "Claude", "Codex".
    pub account: String,
    /// "5-hour limit", "Weekly · all models", ...
    pub label: String,
    pub percent: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null", optional)]
    pub resets_at: Option<i64>,
}

/// The project a folder belongs to, as its name: an attempt's or a dev thread's worktree counts as
/// the repository it was made from (`thread_display::project_root`).
fn project_name(cwd: &str) -> String {
    let root = crate::thread_display::project_root(cwd);
    root.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(&root).to_owned()
}

/// Whether the thread is one the lists show: not archived or settled, and a thread Hub started
/// itself or a side job only counts while it waits on you.
fn counts(t: &Thread) -> bool {
    !t.archived && t.settled_at.is_none() && ((t.owner.is_none() && t.parent.is_none()) || t.status == ThreadStatus::WaitingOnYou)
}

pub async fn read(hub: &Arc<Hub>) -> Result<Now> {
    let threads = hub.all_threads().await?;
    // The open permission question of each thread here: the latest request with no answer after it.
    let requests = hub.store.latest_of_kind(EventKind::PermissionRequest).await?;
    let answered: HashMap<String, i64> =
        hub.store.latest_of_kind(EventKind::PermissionResolved).await?.into_iter().map(|e| (e.thread_id, e.seq)).collect();
    let mut asks: HashMap<String, NowAsk> = HashMap::new();
    for req in requests {
        if answered.get(&req.thread_id).is_some_and(|seq| *seq > req.seq) {
            continue;
        }
        let call = req.tool_call();
        let options = req.options()
            .as_array()
            .map(|all| {
                all.iter()
                    .filter_map(|o| {
                        Some(NowOption { id: o["optionId"].as_str()?.to_owned(), name: o["name"].as_str()?.to_owned(), kind: o["kind"].as_str().unwrap_or_default().to_owned() })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ask = NowAsk {
            request: req.request_id().as_str().map(str::to_owned).unwrap_or_else(|| req.request_id().to_string()),
            what: call["title"].as_str().unwrap_or("use a tool").to_owned(),
            command: call["rawInput"]["command"].as_str().map(str::to_owned),
            options,
        };
        asks.insert(req.thread_id, ask);
    }
    let now_thread = |t: &Thread, since: i64, ask: Option<NowAsk>| NowThread {
        id: t.id.clone(),
        title: t.title.clone(),
        project: project_name(&t.cwd),
        host: t.host.clone(),
        since,
        ask,
    };
    let mut waiting: Vec<NowThread> = threads
        .iter()
        .filter(|t| counts(t) && t.status == ThreadStatus::WaitingOnYou)
        .map(|t| now_thread(t, t.updated_at, if t.host.is_none() { asks.remove(&t.id) } else { None }))
        .collect();
    waiting.sort_by_key(|t| t.since);
    let mut working: Vec<NowThread> = threads
        .iter()
        .filter(|t| counts(t) && t.status == ThreadStatus::Running)
        .map(|t| now_thread(t, t.sent_at.unwrap_or(t.created_at), None))
        .collect();
    working.sort_by_key(|t| t.since);

    let report = hub.usage_report(false).await?;
    let mut limits = Vec::new();
    let mut push = |account: &str, plan: &crate::usage::PlanUsage| {
        for l in &plan.limits {
            limits.push(NowLimit { account: account.to_owned(), label: l.label.clone(), percent: l.percent, resets_at: l.resets_at });
        }
    };
    if let Some(plan) = &report.plan {
        push("Claude", plan);
    }
    for other in &report.others {
        if let Some(plan) = &other.plan {
            push(&other.label, plan);
        }
    }
    Ok(Now { at: now_ms(), waiting, working, limits })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worktree_counts_as_its_project() {
        assert_eq!(project_name("D:\\Dev\\hub\\.hub-threads\\fix-1"), "hub");
        assert_eq!(project_name("/home/a/hub/.git/hub-attempts/x"), "hub");
        assert_eq!(project_name("/home/a/atlas/"), "atlas");
    }

    #[test]
    fn the_snapshot_keeps_its_keys() {
        let now = Now { at: 1, waiting: vec![], working: vec![], limits: vec![] };
        let json = serde_json::to_value(&now).unwrap();
        for key in ["at", "waiting", "working", "limits"] {
            assert!(json.get(key).is_some(), "{key}");
        }
    }
}
