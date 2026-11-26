//! The hover card on a thread in the lists: the TL;DR the agent wrote itself (`write_tldr`) and
//! when, and what the card shows beside it (the agent's last reply, its quick replies, a waiting
//! permission question, the latest proof). `peek` answers at once from the log. Hub writes no
//! TL;DR of its own: an agent that does not write one leaves the card to offer asking for one.

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::ctx::Ctx;
use crate::replies::last_reply;
use crate::text::tail;
use crate::model::{Event, EventKind, GistData, PeekReplies, PeekReply, RepliesData, ThreadPeek, ThreadStatus};

/// What the card shows, read from the log without asking any model.
pub async fn peek(ctx: &Ctx<'_>, thread_id: &str) -> Result<ThreadPeek> {
    let thread = ctx.store.thread(thread_id).await?.context("no such thread")?;
    let events = ctx.store.events(thread_id, 0, 100_000).await?;
    Ok(read(&events, thread.status))
}

/// The points and the ask from `write_tldr`'s arguments.
pub(crate) fn from_args(args: &Value) -> Result<(Vec<String>, Option<String>), String> {
    let line = |v: &Value, max: usize| -> String { v.as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect() };
    let points: Vec<String> = args["points"].as_array().map(|a| a.iter().map(|p| line(p, 400)).filter(|p| !p.is_empty()).take(3).collect()).unwrap_or_default();
    if points.is_empty() {
        return Err("Give one to three short points.".into());
    }
    let ask = Some(line(&args["needs"], 300)).filter(|a| !a.is_empty() && !matches!(a.trim_end_matches('.').to_lowercase().as_str(), "nothing" | "none" | "inget"));
    Ok((points, ask))
}

/// The agent's TL;DR as the event to log. It covers the turn it is written in, so the turn end
/// that follows does not make it stale (`read`). The caller logs it (`mcp/talk.rs`).
pub async fn note(ctx: &Ctx<'_>, thread_id: &str, points: Vec<String>, ask: Option<String>) -> Result<GistData> {
    if points.is_empty() {
        bail!("no points");
    }
    let upto = ctx.store.events(thread_id, 0, 100_000).await?.last().map(|e| e.seq).unwrap_or(0);
    Ok(GistData { upto, points, ask: Some(ask), by: Some("agent".into()) })
}

/// The card's contents from the log as it stands.
pub(crate) fn read(events: &[Event], status: ThreadStatus) -> ThreadPeek {
    let gist = events.iter().rev().find(|e| e.kind == EventKind::Gist);
    let written_at = gist.map(|g| g.ts);
    // A gist that is not an object says nothing, but is still the thread's gist.
    let said = gist.and_then(Event::data_as::<GistData>).unwrap_or_default();
    let upto = said.upto;
    let by_agent = said.by.as_deref() == Some("agent");
    // An agent writes its TL;DR inside the turn it sums up, so the first turn end after it is covered.
    let mut covered = !by_agent;
    let mut since = 0;
    for e in events.iter().filter(|e| e.seq > upto && moves(e.kind)) {
        if !covered && e.kind == EventKind::TurnEnd {
            covered = true;
            continue;
        }
        if matches!(e.kind, EventKind::UserMessage | EventKind::TurnEnd) {
            since += 1;
        }
        covered = true;
        since = since.max(1);
    }
    let stale = gist.is_none() || since > 0;
    let last_user = events.iter().rev().find(|e| e.kind == EventKind::UserMessage).map(|e| e.seq).unwrap_or(0);
    // Quick replies count until the person says something, as in the thread.
    let replies = events.iter().rev().find(|e| e.kind == EventKind::Replies && e.seq > last_user).and_then(Event::data_as::<RepliesData>).and_then(|offer| {
        let options: Vec<PeekReply> = offer
            .options
            .into_iter()
            .filter_map(|o| {
                let label = o.label.trim().to_owned();
                (!label.is_empty()).then(|| PeekReply { message: Some(o.message).filter(|m| !m.trim().is_empty()).unwrap_or_else(|| label.clone()), label })
            })
            .collect();
        (options.len() >= 2).then_some(PeekReplies { question: offer.question, options })
    });
    let permission = (status == ThreadStatus::WaitingOnYou)
        .then(|| {
            events.iter().rev().find(|e| e.kind == EventKind::PermissionRequest).filter(|req| {
                !events.iter().any(|e| e.seq > req.seq && e.data_as::<crate::model::PermissionResolvedData>().is_some_and(|a| Some(a.id) == req.data_as::<crate::model::PermissionRequestData>().map(|r| r.id)))
            })
        })
        .flatten()
        .map(|req| req.tool_call()["title"].as_str().unwrap_or("use a tool").to_owned());
    let error = (status == ThreadStatus::Error)
        .then(|| events.iter().rev().find(|e| e.kind == EventKind::Error))
        .flatten()
        .map(|e| e.failure().unwrap_or("The engine failed").to_owned());
    let proof = events.iter().rev().find(|e| e.kind == EventKind::Proof).and_then(Event::data_as::<crate::model::ProofData>);
    ThreadPeek {
        points: said.points,
        ask: said.ask.flatten(),
        upto,
        written_at,
        by: by_agent.then(|| "agent".to_owned()),
        since,
        stale,
        last_reply: tail(last_reply(events).trim(), 1_500),
        replies,
        permission,
        error,
        files: proof.as_ref().and_then(|p| p.files.flatten()),
        tests: proof.as_ref().and_then(|p| match (p.kind.as_ref().and_then(|k| k.as_deref()), p.state.as_str()) {
            (Some("tests"), "shown") => Some("passed".to_owned()),
            (_, "failed") => Some("failed".to_owned()),
            _ => None,
        }),
        turns: events.iter().filter(|e| e.kind == EventKind::UserMessage).count() as i64,
    }
}

/// Events after which an earlier TL;DR no longer says where the thread stands.
fn moves(kind: EventKind) -> bool {
    matches!(kind, EventKind::UserMessage | EventKind::TurnEnd | EventKind::Error | EventKind::PermissionRequest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(seq: i64, kind: EventKind, data: Value) -> Event {
        Event { thread_id: "t".into(), seq, ts: seq, kind, data }
    }

    #[test]
    fn tldr_arguments_become_points_and_an_ask() {
        let (points, ask) = from_args(&json!({ "points": ["Fixed `remote.rs`.", " ", "#332 is open."], "needs": "Re-run the suite?" })).unwrap();
        assert_eq!(points, vec!["Fixed `remote.rs`.", "#332 is open."]);
        assert_eq!(ask.as_deref(), Some("Re-run the suite?"));
        assert_eq!(from_args(&json!({ "points": ["Done."], "needs": "nothing." })).unwrap().1, None);
        assert!(from_args(&json!({ "points": [] })).is_err());
    }

    #[test]
    fn an_agents_tldr_covers_its_own_turn_end() {
        let mut events = vec![
            ev(1, EventKind::UserMessage, json!({ "text": "fix it" })),
            ev(2, EventKind::Gist, json!({ "upto": 1, "points": ["Fixed it."], "ask": null, "by": "agent" })),
            ev(3, EventKind::TurnEnd, json!({})),
        ];
        let peek = read(&events, ThreadStatus::Idle);
        assert!(!peek.stale);
        assert_eq!(peek.by.as_deref(), Some("agent"));
        events.push(ev(4, EventKind::UserMessage, json!({ "text": "and the test?" })));
        let peek = read(&events, ThreadStatus::Running);
        assert!(peek.stale);
        assert_eq!(peek.since, 1);
    }

    #[test]
    fn a_gist_goes_stale_when_a_turn_ends_after_it() {
        let mut events = vec![
            ev(1, EventKind::UserMessage, json!({ "text": "fix it" })),
            ev(2, EventKind::TurnEnd, json!({})),
            ev(3, EventKind::Gist, json!({ "upto": 2, "points": ["Fixed it."], "ask": null })),
        ];
        let peek = read(&events, ThreadStatus::Idle);
        assert!(!peek.stale);
        assert_eq!(peek.points, vec!["Fixed it."]);
        events.push(ev(4, EventKind::UserMessage, json!({ "text": "and the test?" })));
        assert!(read(&events, ThreadStatus::Running).stale);
        assert!(read(&events[..2], ThreadStatus::Idle).stale, "no gist yet is stale");
        // A gist that is not an object is still the gist, and says nothing.
        let odd = read(&[ev(1, EventKind::Gist, json!(null))], ThreadStatus::Idle);
        assert!(odd.points.is_empty() && odd.written_at.is_some());
    }

    #[test]
    fn quick_replies_count_until_the_person_writes() {
        let offer = json!({ "question": "Run it?", "options": [{ "label": "Yes" }, { "label": "No", "message": "No, hand it off" }] });
        let mut events = vec![ev(1, EventKind::UserMessage, json!({ "text": "go" })), ev(2, EventKind::Replies, offer)];
        let replies = read(&events, ThreadStatus::Idle).replies.expect("offered");
        assert_eq!(replies.options[0].message, "Yes");
        assert_eq!(replies.options[1].message, "No, hand it off");
        events.push(ev(3, EventKind::UserMessage, json!({ "text": "yes" })));
        assert!(read(&events, ThreadStatus::Idle).replies.is_none());
    }
}
