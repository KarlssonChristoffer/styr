//! An agent's reply read from a thread's log: the message chunks of one turn. Plain reading of
//! events, below the tools that hand it to agents (`hub_tools.rs`), the hover card (`gist.rs`)
//! and the desk (`desk.rs`), so they share it without naming each other.

use crate::model::{Event, EventKind};
use crate::store::Store;
use crate::text::clip;

/// Most of a reply handed on in one piece.
pub const READ_CHARS: usize = 24_000;

/// The updates a conversation is told in: the agent's words and the tools it called.
pub const SAID: &[&str] = &["agent_message_chunk", "tool_call"];

/// The part of a thread's log that `conversation_log` and `handover_log` (`hub/send.rs`) read:
/// the user's messages, the agent's words and tool calls, and the rewinds that cut some out;
/// with the kinds in `more` too. Not the tool output that is most of a log.
pub async fn conversation(store: &Store, thread_id: &str, more: &[EventKind]) -> anyhow::Result<Vec<Event>> {
    let kinds: Vec<EventKind> = [EventKind::UserMessage, EventKind::Rewind].into_iter().chain(more.iter().copied()).collect();
    store.events_with_updates(thread_id, 0, &kinds, SAID).await
}

/// The agent's reply in the last turn that ended.
pub fn last_reply(events: &[Event]) -> String {
    let end = events.iter().rev().find(|e| e.kind == EventKind::TurnEnd).map(|e| e.seq).unwrap_or(i64::MAX);
    reply(events, end)
}

/// What the agent said in the turn ending at `end`: its message chunks since the turn before.
pub fn reply(events: &[Event], end: i64) -> String {
    let mut text = String::new();
    for e in events.iter().take_while(|e| e.seq <= end) {
        match e.kind {
            EventKind::TurnEnd | EventKind::Error if e.seq < end => text.clear(),
            EventKind::Update if crate::model::update_kind(&e.data) == Some("agent_message_chunk") => {
                text.push_str(e.data["content"]["text"].as_str().unwrap_or_default())
            }
            _ => {}
        }
    }
    clip(text.trim(), READ_CHARS)
}
