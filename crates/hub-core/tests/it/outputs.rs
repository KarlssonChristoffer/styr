//! A project's library (`outputs.rs`): what its threads made, read from their logs, with where
//! each thing is now. Against the scripted `mock-agent` engine.

use std::path::PathBuf;
use std::sync::Arc;

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

#[tokio::test]
async fn the_library_lists_pages_with_versions_and_pictures_with_their_place() {
    let hub = hub();
    let root = hub_core::janitor::test_dir().join("atlas");
    std::fs::create_dir_all(root.join("docs")).unwrap();
    let folder = root.display().to_string();
    let thread = hub
        .create_thread(NewThread {
            cwd: folder.clone(),
            title: Some("Sketch the board".into()),
            engine: None,
            message: Some("draw the board".into()),
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

    // A page, then a second version of it.
    let show = |html: &str, replaces: Option<&str>| {
        let mut args = json!({ "title": "Status board", "html": html });
        if let Some(r) = replaces {
            args["replaces"] = json!(r);
        }
        call("show_page", args)
    };
    let first = hub_core::mcp::handle(&hub, &thread.id, show("<p>one</p>", None)).await.unwrap();
    let said = first["result"]["content"][0]["text"].as_str().unwrap().to_owned();
    let id = said.rsplit("Its id is ").next().unwrap().trim_end_matches('.').to_owned();
    hub_core::mcp::handle(&hub, &thread.id, show("<p>two</p>", Some(&id))).await.unwrap();
    let pages = hub.store.events(&thread.id, 0, 1000).await.unwrap().iter().filter(|e| e.kind == EventKind::Page).count();
    assert_eq!(pages, 2);
    // A page event that names no page is not listed.
    hub.store.append_event(&thread.id, EventKind::Page, json!({ "id": "", "title": "Nameless" })).await.unwrap();
    // A side job that is done lists what it drew; one still working lists nothing.
    let job = |state: &str, image: &str| json!({ "id": state, "title": "Draw it", "state": state, "images": [image] });
    hub.store.append_event(&thread.id, EventKind::Child, job("done", "drawn.png")).await.unwrap();
    hub.store.append_event(&thread.id, EventKind::Child, job("working", "pending.png")).await.unwrap();

    let made = hub_core::outputs::list(&hub, &folder).await.unwrap();
    assert_eq!(made.iter().filter(|o| o.kind == "page").count(), 1);
    let page = made.iter().find(|o| o.kind == "page").expect("no page");
    assert_eq!(page.title, "Status board");
    assert_eq!(page.versions, 2);
    assert_eq!(page.places[0].kind, "hub");
    assert_eq!(page.places[0].state, "kept");
    assert_eq!(page.open.thread.as_deref(), Some(thread.id.as_str()));
    assert_eq!(page.life.len(), 2, "{:?}", page.life);

    // The mock says it saved the picture but never does: the library says it is gone.
    let drawn = made.iter().find(|o| o.title == "drawn.png").expect("the side job's picture is not listed");
    assert_eq!(drawn.kind, "picture");
    assert!(drawn.life.iter().any(|step| step.what == "Drawn by side job Draw it"), "{:?}", drawn.life);
    assert!(made.iter().all(|o| o.title != "pending.png"));
    let picture = made.iter().find(|o| o.title == "sunset.png").expect("no picture");
    assert_eq!(picture.title, "sunset.png");
    assert_eq!(picture.places[0].kind, "folder");
    assert_eq!(picture.places[0].state, "gone");
    assert_eq!(picture.thread_title.as_deref(), Some("Sketch the board"));
    assert!(made.iter().all(|o| !o.live));

    // Hub shows a thing by its library id, and only things the threads made.
    let path = PathBuf::from(picture.open.path.as_deref().unwrap());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"png").unwrap();
    match hub_core::outputs::content(&hub, &folder, &picture.id).await.unwrap() {
        hub_core::outputs::Content::File(p) => assert_eq!(std::fs::read(p).unwrap(), b"png"),
        _ => panic!("the picture is a file"),
    }
    let elsewhere = format!("file:{}", std::env::temp_dir().join("x.md").display());
    assert!(hub_core::outputs::content(&hub, &folder, &elsewhere).await.is_err());
    assert!(hub_core::outputs::content(&hub, &folder, &page.id).await.is_err(), "a page is shown from Hub, not read as a file");
}
