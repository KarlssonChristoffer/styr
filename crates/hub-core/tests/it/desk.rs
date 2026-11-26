//! A project's desk (`desk.rs`): one thread per project that steers the others. Its threads'
//! turns come back to it as quiet notes, and it may start threads in its own project only.
//! Against the scripted `mock-agent` engine.

use std::sync::Arc;
use std::time::Duration;

use hub_core::model::{EventKind, NewThread, ThreadStatus};
use hub_core::{Config, Hub, Store};
use serde_json::{Value, json};
use crate::support::{mock_agent, wait_status, turn_ended};

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
fn ok(answer: Value) -> Value {
    let result = &answer["result"];
    assert_eq!(result["isError"], false, "{answer}");
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn a_desk_hears_from_its_threads_and_steers_its_own_project_only() {
    let hub = hub();
    let root = hub_core::janitor::test_dir();
    let folder = root.join("atlas");
    let elsewhere = root.join("other");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::create_dir_all(&elsewhere).unwrap();
    let folder = folder.display().to_string();

    // Made once, then the same thread again, also by another spelling of the folder.
    let desk = hub.open_desk(&folder).await.unwrap();
    assert_eq!(desk.title, "atlas desk");
    assert!(desk.title_locked && !desk.auto_settle);
    let again = hub.open_desk(&format!("{folder}/")).await.unwrap();
    assert_eq!(again.id, desk.id);
    assert_eq!(hub_core::desk::ids(&hub.ctx()).await.unwrap(), [desk.id.clone()]);

    // A thread of the project finishes a turn: the desk gets a note, and no turn of its own.
    let worker = hub
        .create_thread(NewThread {
            cwd: folder.clone(),
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
    turn_ended(&hub, &worker.id).await;
    wait_status(&hub, &worker.id, ThreadStatus::Idle).await;
    let mut note = None;
    for _ in 0..200 {
        let events = hub.store.events(&desk.id, 0, 1000).await.unwrap();
        note = events.into_iter().find(|e| e.kind == EventKind::DeskNote);
        if note.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let note = note.expect("no note in the desk");
    assert_eq!(note.data["thread"], worker.id.as_str());
    assert_eq!(note.data["state"], "done");
    assert!(note.data["text"].as_str().unwrap().contains("count the files"), "{}", note.data);
    let desk_now = hub.store.thread(&desk.id).await.unwrap().unwrap();
    assert_eq!(desk_now.status, ThreadStatus::Idle);
    let events = hub.store.events(&desk.id, 0, 1000).await.unwrap();
    assert!(!events.iter().any(|e| e.kind == EventKind::UserMessage), "a note never starts a turn");

    // The desk's agent is told it is the desk.
    let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} });
    let answer = hub_core::mcp::handle(&hub, &desk.id, init).await.unwrap();
    assert!(answer["result"]["instructions"].as_str().unwrap().contains("this project's desk"));
    let plain = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} });
    let answer = hub_core::mcp::handle(&hub, &worker.id, plain).await.unwrap();
    assert!(!answer["result"]["instructions"].as_str().unwrap().contains("this project's desk"));

    // It starts threads in its own project, and not in another.
    let away = call("start_thread", json!({ "message": "hi", "folder": elsewhere.display().to_string() }));
    let refused = hub_core::mcp::handle(&hub, &desk.id, away).await.unwrap();
    assert_eq!(refused["result"]["isError"], true, "{refused}");
    let here = ok(hub_core::mcp::handle(&hub, &desk.id, call("start_thread", json!({ "message": "list the files" }))).await.unwrap());
    assert_eq!(here["thread"]["folder"], folder.as_str());
}
