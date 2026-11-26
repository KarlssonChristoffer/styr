//! Wire size: how many bytes the phone is sent for a fixed set of threads (the thread list, the
//! "now" card and one long thread's events, as the API serialises them). The fixture is built here,
//! the same every run, so a change in the number is a change in what the core sends. The test only
//! writes the number when `HUB_WIRE_OUT=<file>` is set (`scripts/monday.py` does; `scripts/pins.py`
//! turns +10 % into a red pin); without it the test checks that the fixture measures something.

use std::sync::Arc;

use hub_core::model::{EventKind, Thread, ThreadStatus};
use hub_core::{Config, Hub, Store};
use serde_json::json;

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Config::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}

/// Fixed ids and times: the bytes must not depend on the clock or on a random id.
fn thread(n: usize, status: ThreadStatus) -> Thread {
    Thread {
        id: format!("wire-{n:03}"),
        title: format!("Thread number {n} with a title of ordinary length"),
        cwd: "/home/a/hub/.hub-threads/fix-1".into(),
        engine: "claude".into(),
        status,
        engine_session_id: None,
        title_locked: true,
        archived: false,
        returned_at: None,
        seen_at: None,
        settled_at: None,
        sent_at: Some(1_700_000_000_000),
        pinned_at: None,
        snoozed_until: None,
        auto_settle: true,
        parent: None,
        reading: None,
        owner: None,
        cloud: None,
        host: None,
        project: None,
        display: None,
        created_at: 1_700_000_000_000,
        updated_at: 1_700_000_100_000,
    }
}

#[tokio::test]
async fn the_phone_wire_size_of_a_fixed_set_of_threads() {
    let hub = hub();
    for n in 0..30 {
        let status = if n % 10 == 0 { ThreadStatus::Running } else { ThreadStatus::Idle };
        hub.store.insert_thread(thread(n, status)).await.unwrap();
    }
    // One long thread: 60 turns of a prompt, streamed answer chunks and a tool call.
    let long = "wire-000";
    for turn in 0..60 {
        let s = &hub.store;
        s.append_event(long, EventKind::UserMessage, json!({ "text": format!("Please look at step {turn} and fix what is wrong with it") })).await.unwrap();
        s.append_event(long, EventKind::TurnStart, json!({ "prompt": turn })).await.unwrap();
        for chunk in 0..4 {
            s.append_event(
                long,
                EventKind::Update,
                json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": format!("Part {chunk} of the answer to step {turn}. ") } }),
            )
            .await
            .unwrap();
        }
        s.append_event(
            long,
            EventKind::Update,
            json!({ "sessionUpdate": "tool_call", "toolCallId": format!("c{turn}"), "title": "Read src/lib.rs", "kind": "read", "status": "completed", "rawInput": { "path": "src/lib.rs" } }),
        )
        .await
        .unwrap();
        s.append_event(long, EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": turn })).await.unwrap();
    }

    let list = serde_json::to_vec(&hub.all_threads().await.unwrap()).unwrap();
    let now = serde_json::to_vec(&hub_core::now::read(&hub).await.unwrap()).unwrap();
    let events = serde_json::to_vec(&hub.store.events(long, 0, 50_000).await.unwrap()).unwrap();
    let total = list.len() + now.len() + events.len();
    assert!(list.len() > 1_000 && now.len() > 10 && events.len() > 10_000, "the fixture measures nothing: {} {} {}", list.len(), now.len(), events.len());

    if let Ok(out) = std::env::var("HUB_WIRE_OUT") {
        let doc = json!({ "wire_bytes": total, "list_bytes": list.len(), "now_bytes": now.len(), "events_bytes": events.len() });
        std::fs::write(out, doc.to_string()).unwrap();
    }
}
