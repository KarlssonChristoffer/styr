//! What happens to an event once it is in the log. Every event goes through [`ALL`], whichever way
//! it was logged (`Hub::emit` or `Hub::emit_once`), so a new "do something when X is logged" feature
//! adds a function to the list here and does not touch `hub.rs` (this list is part of Hub: `hub/observers.rs`). An observer gets the hub and the
//! logged event, does its quick synchronous part (counting, noting, queueing) and returns; slow work
//! goes to the after-turn queue (`hub/after_turn.rs`, `look_at`).

use std::path::Path;

use super::Hub;
use crate::model::{Event, EventKind};

pub(crate) type Observer = fn(&Hub, &Event);

/// In the order they run. Observers must not log events themselves (they run inside `emit`).
pub(crate) const ALL: &[Observer] = &[status_shadow, first_token_clock, after_turn_queue, #[cfg(test)] counted];

/// Follows the status the log implies, to compare with the real one (`status_shadow.rs`).
fn status_shadow(hub: &Hub, event: &Event) {
    hub.shadow.note(&hub.store, event);
}

/// Times the wait from a turn's start to the engine's first answer (`problems/metrics.rs`).
fn first_token_clock(_hub: &Hub, event: &Event) {
    match event.kind {
        EventKind::TurnStart => crate::metric_samples::turn_started(&event.thread_id),
        EventKind::Update => crate::metric_samples::engine_said(&event.thread_id, &event.data),
        _ => {}
    }
}

/// Queues the events that need a look: edits (clashes), tool calls worth counting, turn ends
/// (leaks, proof) and errors (plan limits).
fn after_turn_queue(hub: &Hub, event: &Event) {
    let update = event.kind == EventKind::Update;
    let edits = update && !crate::clash::edited(&event.data, Path::new("")).is_empty();
    let tool = update && crate::model::worth_a_look(&event.data);
    if edits || tool || event.kind == EventKind::TurnEnd || event.kind == EventKind::Error {
        hub.queue_after(event.clone());
    }
}

#[cfg(test)]
static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Test-only observer: remembers the thread of every event, so a test counts its own thread's.
#[cfg(test)]
fn counted(_hub: &Hub, event: &Event) {
    SEEN.lock().unwrap_or_else(|p| p.into_inner()).push(event.thread_id.clone());
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    /// `emit` and `emit_once` logged different things before (`emit_once` skipped the clock, the
    /// shadow and the queue); both now run the same list.
    #[tokio::test]
    async fn emit_and_emit_once_run_the_same_observers() {
        let config = Config { data_dir: crate::janitor::test_dir(), ..Config::default() };
        let hub = Hub::new(Store::in_memory().unwrap(), config);
        let t = crate::model::Thread {
            id: "observers-test".into(), title: "t".into(), cwd: "/tmp".into(), engine: "claude".into(), status: crate::model::ThreadStatus::Idle,
            engine_session_id: None, title_locked: false, archived: false, returned_at: None, seen_at: None,
            settled_at: None, sent_at: None, pinned_at: None, snoozed_until: None, auto_settle: true,
            parent: None, reading: None, owner: None, cloud: None, host: None, project: None, display: None, created_at: 1, updated_at: 1,
        };
        let thread = hub.store.insert_thread(t).await.unwrap();
        let seen = || SEEN.lock().unwrap_or_else(|p| p.into_inner()).iter().filter(|id| *id == "observers-test").count();
        hub.emit(&thread.id, EventKind::UserMessage, json!({ "text": "one" })).await.unwrap();
        assert_eq!(seen(), 1);
        let once = hub.emit_once(&thread.id, EventKind::UserMessage, json!({ "text": "two" }), "c1").await.unwrap();
        assert!(once.is_some());
        assert_eq!(seen(), 2);
        // A repeated request is not logged again, so nobody observes it.
        assert!(hub.emit_once(&thread.id, EventKind::UserMessage, json!({ "text": "two" }), "c1").await.unwrap().is_none());
        assert_eq!(seen(), 2);
    }
}
