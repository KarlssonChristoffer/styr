//! A recorded ACP session played back to Hub (`mock-agent replay`, `crates/mock-agent/src/replay.rs`).
//!
//! `fixtures/acp-tool-turn.trace.jsonl` is a `trace_engines` recording of one turn with a tool
//! call, made against `mock-agent`. `acp-claude-*` and `acp-codex-*` are real sessions recorded on
//! the PC (10 Nov 2026) against claude-agent-acp 0.99.1 and codex-acp 2.5.0, with the scratch
//! folder, e-mail and tokens rewritten and the command lists cut to three. To pin Hub against an
//! adapter, record a thread with `"trace_engines": true` in hub.json and copy
//! `<data>/engines/<thread>.trace.jsonl` into `fixtures/`; a test like the ones below then holds
//! Hub to whatever that session said. A new adapter version is a new recording, not an edit.

use std::path::PathBuf;
use std::time::Duration;

use hub_core::model::{EventKind, NewThread, ThreadStatus, TurnUsage};
use hub_core::{Config, Hub, Store};
use crate::support::mock_agent;

fn fixture(name: &str) -> PathBuf {
    hub_core::source_tree::hub_core().join("tests/fixtures").join(name)
}

/// Plays `fixture` to a fresh Hub as the only engine and returns the thread's events once the turn
/// ended and the thread is idle again. The bench checks read the same recordings (`bench.rs`).
pub(crate) async fn play(fixture_name: &str, harness: &str) -> Vec<hub_core::model::Event> {
    let trace = fixture(fixture_name);
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.keep_engines = false;
    config.engines.clear();
    config.engines.insert(
        "replay".into(),
        hub_core::config::EngineConfig {
            command: format!("\"{}\" replay \"{}\"", mock_agent().display(), trace.display()),
            driver: Default::default(),
            secrets: Vec::new(),
            terminal: None,
            traits: hub_core::config::EngineTraits::preset(harness),
            ..Default::default()
        },
    );
    config.default_engine = "replay".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);

    let new: NewThread = serde_json::from_value(serde_json::json!({
        "cwd": hub_core::janitor::test_dir().display().to_string(),
        "message": "anything: the recording answers whatever is sent",
    }))
    .unwrap();
    let thread = hub.create_thread(new).await.unwrap();

    let mut events = Vec::new();
    for _ in 0..400 {
        events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
        if events.iter().any(|e| e.kind == EventKind::TurnEnd) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let kinds: Vec<_> = events.iter().map(|e| &e.kind).collect();
    assert!(kinds.contains(&&EventKind::TurnEnd), "the turn never ended: {kinds:?}");
    assert!(!kinds.contains(&&EventKind::Error), "Hub failed on the recording: {events:?}");

    for _ in 0..200 {
        if hub.store.thread(&thread.id).await.unwrap().unwrap().status == ThreadStatus::Idle {
            return events;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the thread did not go back to idle");
}

fn updates(events: &[hub_core::model::Event]) -> Vec<&str> {
    events.iter().filter(|e| e.kind == EventKind::Update).filter_map(|e| e.data["sessionUpdate"].as_str()).collect()
}

fn said(events: &[hub_core::model::Event]) -> String {
    events
        .iter()
        .filter(|e| e.kind == EventKind::Update && e.data["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|e| e.data["content"]["text"].as_str())
        .collect()
}

/// The tokens the turn's end carried.
fn tokens(events: &[hub_core::model::Event]) -> Option<TurnUsage> {
    events.iter().find(|e| e.kind == EventKind::TurnEnd)?.data_as::<hub_core::model::TurnEndData>()?.usage
}

/// Tokens in, out, read from the cache and written to it.
fn usage(input: i64, output: i64, cache_read: i64, cache_write: i64) -> Option<TurnUsage> {
    Some(TurnUsage { input: Some(input), output: Some(output), cache_read: Some(cache_read), cache_write: Some(cache_write) })
}

fn stop_reason(events: &[hub_core::model::Event]) -> &str {
    events.iter().find(|e| e.kind == EventKind::TurnEnd).and_then(|e| e.data["stop_reason"].as_str()).unwrap_or_default()
}

#[tokio::test]
async fn hub_understands_a_recorded_session() {
    let events = play("acp-tool-turn.trace.jsonl", "claude").await;
    let updates = updates(&events);
    assert!(updates.contains(&"tool_call") && updates.contains(&"tool_call_update"), "{updates:?}");
    assert_eq!(said(&events), "Echo: tool please");
    assert_eq!(stop_reason(&events), "end_turn");
    // The mock agent's answer has no `usage`, so the turn's end has none.
    assert_eq!(tokens(&events), None);
}

#[tokio::test]
async fn hub_understands_a_real_claude_adapter_turn() {
    let events = play("acp-claude-tool-turn.trace.jsonl", "claude").await;
    let updates = updates(&events);
    assert!(updates.contains(&"tool_call") && updates.contains(&"tool_call_update"), "{updates:?}");
    assert!(!said(&events).trim().is_empty(), "the adapter's answer never reached the thread");
    assert_eq!(stop_reason(&events), "end_turn");
    // The prompt response's `usage`: inputTokens, outputTokens, cachedReadTokens, cachedWriteTokens.
    assert_eq!(tokens(&events), usage(4, 105, 58_290, 58_397));
}

#[tokio::test]
async fn hub_understands_a_real_codex_adapter_turn() {
    let events = play("acp-codex-tool-turn.trace.jsonl", "codex").await;
    let updates = updates(&events);
    assert!(updates.contains(&"tool_call") && updates.contains(&"tool_call_update"), "{updates:?}");
    assert!(!said(&events).trim().is_empty(), "the adapter's answer never reached the thread");
    assert_eq!(stop_reason(&events), "end_turn");
    // The same protocol fields; this adapter writes its cache writes as 0.
    assert_eq!(tokens(&events), usage(7_401, 67, 34_304, 0));
}

#[tokio::test]
async fn hub_asks_when_the_real_claude_adapter_wants_permission() {
    let events = play("acp-claude-permission.trace.jsonl", "claude").await;
    let ask = events.iter().find(|e| e.kind == EventKind::PermissionRequest).expect("the recording's permission question never reached the thread");
    let options: Vec<&str> = ask.data["options"].as_array().map(|o| o.iter().filter_map(|o| o["optionId"].as_str()).collect()).unwrap_or_default();
    assert!(options.contains(&"allow-once") && options.contains(&"reject"), "{options:?}");
    assert!(updates(&events).contains(&"tool_call"));
    assert_eq!(tokens(&events), usage(4, 110, 89_071, 27_688));
}
