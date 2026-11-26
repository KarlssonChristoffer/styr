//! Waiting for a thread to answer a message: until the turn that answers it ended (or failed),
//! the thread needs the user, or the time is up. Hub's tool `wait_for_thread` (`hub_tools`) and
//! the bench's runner (`bench/run.rs`) both wait this way; it sits apart from both so neither
//! has to name the other.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{Value, json};

use crate::hub::Hub;
use crate::locked::Ignore;
use crate::model::{Event, EventKind, ThreadStatus};
use crate::replies::reply;

/// Waits until message `asked` has been answered (or failed), or the thread needs the user.
pub(crate) async fn wait(hub: &Arc<Hub>, thread_id: &str, asked: i64, timeout: Duration) -> Result<Value> {
    let until = tokio::time::Instant::now() + timeout;
    let mut stream = hub.subscribe();
    loop {
        // How the turn ended and what it said; not its tool output.
        let kinds = [EventKind::TurnEnd, EventKind::Error];
        let events = hub.store.events_with_updates(thread_id, asked, &kinds, &["agent_message_chunk"]).await?;
        let thread = hub.store.thread(thread_id).await?.context("the thread was deleted")?;
        if let Some(state) = answered(&events, asked) {
            let (state, end) = state;
            return Ok(json!({ "thread_id": thread_id, "state": state, "reply": reply(&events, end) }));
        }
        if thread.status == ThreadStatus::WaitingOnYou {
            return Ok(json!({
                "thread_id": thread_id,
                "state": "needs_user",
                "note": "it is waiting for the user to answer a permission question in Hub",
                "reply": reply(&events, i64::MAX),
            }));
        }
        if matches!(thread.status, ThreadStatus::Error | ThreadStatus::InTerminal) {
            return Ok(json!({ "thread_id": thread_id, "state": thread.status, "reply": reply(&events, i64::MAX) }));
        }
        if tokio::time::Instant::now() >= until {
            return Ok(json!({ "thread_id": thread_id, "state": "still_working", "note": "wait again or read it later" }));
        }
        // Woken by any change, or a second later at the latest.
        tokio::time::timeout_at(until.min(tokio::time::Instant::now() + Duration::from_secs(1)), stream.recv()).await.ignore("a timeout or a lagged stream only means look again");
    }
}

/// How the turn that answered message `asked` ended, and where.
pub(crate) fn answered(events: &[Event], asked: i64) -> Option<(&'static str, i64)> {
    events.iter().find_map(|e| {
        let answers = match e.prompt() {
            Some(p) => p == asked,
            // Turns logged before turn ends named their message.
            None => e.kind == EventKind::TurnEnd,
        };
        match e.kind {
            EventKind::TurnEnd if answers => {
                Some((if e.cancelled() { "stopped" } else { "done" }, e.seq))
            }
            EventKind::Error if answers && e.has("prompt") => Some(("failed", e.seq)),
            _ => None,
        }
    })
}
