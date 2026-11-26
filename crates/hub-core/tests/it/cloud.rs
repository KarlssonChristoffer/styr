//! Threads that run in the cloud (`cloud_threads.rs`): what is refused before a thread exists,
//! what is kept of the run, and that a cloud thread never counts as work this machine does.

use std::sync::Arc;
use std::time::Duration;

use hub_core::model::{NewThread, ThreadStatus};
use hub_core::{Config, Hub, Store};

fn hub() -> (Arc<Hub>, String) {
    let dir = hub_core::janitor::test_dir();
    let config = Config { data_dir: dir.join("data"), ..Default::default() };
    std::fs::create_dir_all(&config.data_dir).unwrap();
    let cwd = dir.join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    (Hub::new(Store::in_memory().unwrap(), config), cwd.display().to_string())
}

fn cloud(cwd: &str, engine: &str, message: Option<&str>) -> NewThread {
    NewThread {
        cwd: cwd.into(),
        title: None,
        engine: Some(engine.into()),
        message: message.map(Into::into),
        attachments: Vec::new(),
        settings: Default::default(),
        original: None,
        client_id: None,
        attempts: Vec::new(),
        warm: None,
        design: false,
        unattended: None,
        reach: None,
        cloud: true,
    }
}

#[tokio::test]
async fn a_cloud_thread_that_cannot_start_leaves_no_thread() {
    let (hub, cwd) = hub();
    // No task, and an engine with no cloud.
    assert!(hub.create_thread(cloud(&cwd, "claude", None)).await.is_err());
    assert!(hub.create_thread(cloud(&cwd, "claude", Some("  "))).await.is_err());
    assert!(hub.create_thread(cloud(&cwd, "hermes", Some("do it"))).await.is_err());
    assert!(hub.store.threads().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_folder_without_github_fails_in_the_thread_and_says_why() {
    let (hub, cwd) = hub();
    let made = hub.create_thread(cloud(&cwd, "claude", Some("fix the build"))).await.unwrap();
    assert_eq!(made.cloud.as_ref().map(|c| c.provider.as_str()), Some("claude"));
    // Starting runs in the background; the thread ends up failed with the reason on its card.
    let mut thread = made;
    for _ in 0..100 {
        thread = hub.store.thread(&thread.id).await.unwrap().unwrap();
        if thread.status == ThreadStatus::Error {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(thread.status, ThreadStatus::Error);
    let run = thread.cloud.unwrap();
    assert_eq!(run.state, "failed");
    assert!(run.note.unwrap().contains("GitHub"));
    // The words went into the log as the user's message, and no engine was started for it.
    assert!(hub.engines().is_empty());
}

#[tokio::test]
async fn a_cloud_thread_that_works_does_not_hold_this_machine() {
    let (hub, cwd) = hub();
    let made = hub.create_thread(cloud(&cwd, "claude", Some("fix the build"))).await.unwrap();
    hub.store.update_thread(&made.id, |t| t.status = ThreadStatus::Running).await.unwrap();
    assert!(!hub.busy().await.unwrap());
    assert!(!hub.busy_unkept().await.unwrap());
}

#[tokio::test]
async fn the_run_survives_the_store() {
    let (hub, cwd) = hub();
    let made = hub.create_thread(cloud(&cwd, "claude", Some("fix the build"))).await.unwrap();
    hub.store
        .update_thread(&made.id, |t| {
            let c = t.cloud.as_mut().unwrap();
            c.session_id = "session_01X".into();
            c.state = "working".into();
            c.branch = Some("claude/fix".into());
        })
        .await
        .unwrap();
    let back = hub.store.thread(&made.id).await.unwrap().unwrap().cloud.unwrap();
    assert_eq!((back.session_id.as_str(), back.state.as_str(), back.branch.as_deref()), ("session_01X", "working", Some("claude/fix")));
    // A thread on a machine has none, and says nothing of it on the wire.
    let plain = hub.create_thread(NewThread { cloud: false, engine: Some("claude".into()), message: None, ..cloud(&cwd, "claude", None) }).await.unwrap();
    assert!(plain.cloud.is_none());
    assert!(serde_json::to_value(&plain).unwrap().get("cloud").is_none());
}
