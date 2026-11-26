//! Reactions (`reactions.rs`): an emoji on a message from the user or the agent, kept in the log
//! as `reaction` events. Against the scripted `mock-agent` engine.

use std::sync::Arc;

use hub_core::model::{EventKind, NewThread, ThreadStatus};
use hub_core::{Config, Hub, Store};
use serde_json::{Value, json};

use crate::support::{mock_agent, turn_ended, wait_status};

fn hub() -> Arc<Hub> {
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig { command: mock_agent().display().to_string(), driver: Default::default(), secrets: Vec::new(), terminal: None, traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() },
    );
    config.default_engine = "mock".into();
    Hub::new(Store::in_memory().unwrap(), config)
}

fn call(name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

/// A tool's answer as JSON, failing the test on a tool error.
fn said(answer: &Value) -> String {
    answer["result"]["content"][0]["text"].as_str().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn you_and_the_agent_react_and_it_changes_nothing_else() {
    let hub = hub();
    let folder = hub_core::janitor::test_dir().join("atlas");
    std::fs::create_dir_all(&folder).unwrap();
    let thread = hub
        .create_thread(NewThread {
            cwd: folder.display().to_string(),
            title: Some("Count files".into()),
            engine: None,
            message: Some("count the files".into()),
            attachments: vec![],
            settings: Default::default(),
            original: None,
            client_id: None,
            attempts: vec![],
            warm: None,
            design: false,
            unattended: None,
            reach: None,
            cloud: false,
        })
        .await
        .unwrap();
    turn_ended(&hub, &thread.id).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let user = events.iter().find(|e| e.kind == EventKind::UserMessage).unwrap().seq;
    let end = events.iter().find(|e| e.kind == EventKind::TurnEnd).unwrap().seq;
    let before = hub.store.thread(&thread.id).await.unwrap().unwrap();

    // You put a thumbs up on your message and a rocket on the reply; a second one changes nothing.
    assert!(hub.react(&thread.id, user, "👍", true, "you").await.unwrap().is_some());
    assert!(hub.react(&thread.id, user, "👍", true, "you").await.unwrap().is_none());
    assert!(hub.react(&thread.id, end, "🚀", true, "you").await.unwrap().is_some());
    // Not on a tool call, not on a message that is not there, not on something that is no emoji.
    let tool = events.iter().find(|e| e.kind == EventKind::Update).unwrap().seq;
    assert!(hub.react(&thread.id, tool, "👍", true, "you").await.is_err());
    assert!(hub.react(&thread.id, 9999, "👍", true, "you").await.is_err());
    assert!(hub.react(&thread.id, user, "thumbs", true, "you").await.is_err());
    assert!(hub.react(&thread.id, user, ":nope:", true, "you").await.is_err());
    // Taking it away is another event, and what you use counts.
    assert!(hub.react(&thread.id, user, "👍", false, "you").await.unwrap().is_some());
    assert!(hub.react(&thread.id, user, "👍", false, "you").await.unwrap().is_none());
    let state = hub_core::reactions::state(&hub.ctx()).await.unwrap();
    assert_eq!(&state.quick[..2], ["👍", "🚀"], "{:?}", state.quick);

    // The agent's own tool reacts to the user's latest message, once.
    let caller = hub_core::hub_tools::Caller::Thread(hub.store.thread(&thread.id).await.unwrap().unwrap());
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    let tools = hub_core::mcp::handle_as(&hub, &caller, list).await.unwrap();
    assert!(tools["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "react"));
    let answer = hub_core::mcp::handle_as(&hub, &caller, call("react", json!({ "emoji": "👀" }))).await.unwrap();
    assert!(said(&answer).contains("👀"), "{answer}");
    let again = hub_core::mcp::handle_as(&hub, &caller, call("react", json!({ "emoji": "👀" }))).await.unwrap();
    assert!(said(&again).contains("already"), "{again}");
    let bad = hub_core::mcp::handle_as(&hub, &caller, call("react", json!({ "emoji": "yes" }))).await.unwrap();
    assert_eq!(bad["result"]["isError"], true, "{bad}");

    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let mine: Vec<_> = events.iter().filter(|e| e.kind == EventKind::Reaction).collect();
    assert_eq!(mine.len(), 4);
    assert!(mine.iter().any(|e| e.data["by"] == "agent" && e.data["emoji"] == "👀" && e.data["seq"] == user));
    // No reaction started a turn or touched the thread.
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::UserMessage).count(), 1);
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::TurnStart).count(), 1);
    let after = hub.store.thread(&thread.id).await.unwrap().unwrap();
    assert_eq!((after.status, after.updated_at), (before.status, before.updated_at));
}
