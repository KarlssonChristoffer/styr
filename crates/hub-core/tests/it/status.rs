//! `status::replay` against the logs real threads write: the status it works out from the log must
//! be the one the thread ended with. This is the check the shadow mode (D3 step 3) runs live.

use std::sync::Arc;
use std::time::Duration;

use hub_core::model::{EventKind, NewThread, StreamMsg, ThreadStatus};
use hub_core::status::replay;
use hub_core::{Config, Hub, Store};
use tokio::sync::broadcast;
use crate::support::{mock_agent, wait_status};

fn hub() -> Arc<Hub> {
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig {
            command: mock_agent().display().to_string(),
            driver: Default::default(),
            secrets: Vec::new(),
            terminal: Some("mock-cli --resume {session}".into()),
            traits: hub_core::config::EngineTraits::preset("claude"),
            ..Default::default()
        },
    );
    config.default_engine = "mock".into();
    Hub::new(Store::in_memory().unwrap(), config)
}

async fn next_event(rx: &mut broadcast::Receiver<StreamMsg>, kind: EventKind) -> hub_core::model::Event {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let StreamMsg::Event { event } = rx.recv().await.unwrap()
                && event.kind == kind {
                    return event;
                }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {kind:?}"))
}

fn new_thread(message: &str) -> NewThread {
    NewThread {
        cwd: std::env::temp_dir().display().to_string(),
        title: None,
        engine: None,
        message: Some(message.into()),
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
    }
}

/// The log of `id` played from the start must give the status the thread has now.
async fn log_agrees(hub: &Hub, id: &str) {
    let events = hub.store.events(id, 0, 10_000).await.unwrap();
    let stored = hub.store.thread(id).await.unwrap().unwrap().status;
    assert_eq!(replay(ThreadStatus::Idle, &events), stored, "log: {:?}", events.iter().map(|e| e.kind).collect::<Vec<_>>());
}

#[tokio::test]
async fn the_log_of_a_plain_turn_gives_the_status_the_thread_ended_with() {
    let hub = hub();
    let thread = hub.create_thread(new_thread("hello world")).await.unwrap();
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    log_agrees(&hub, &thread.id).await;
}

#[tokio::test]
async fn the_log_of_a_question_gives_waiting_and_then_idle() {
    let hub = hub();
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("needs permission")).await.unwrap();
    let ask = next_event(&mut rx, EventKind::PermissionRequest).await;
    wait_status(&hub, &thread.id, ThreadStatus::WaitingOnYou).await;
    log_agrees(&hub, &thread.id).await;

    hub.answer_permission(&thread.id, ask.data["id"].as_str().unwrap(), Some("allow".into())).await.unwrap();
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    log_agrees(&hub, &thread.id).await;
}

#[tokio::test]
async fn the_log_of_a_sleeping_session_gives_waiting_and_then_idle() {
    let hub = hub();
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("needs permission")).await.unwrap();
    let ask = next_event(&mut rx, EventKind::PermissionRequest).await;
    wait_status(&hub, &thread.id, ThreadStatus::WaitingOnYou).await;
    hub.sleep_waiting(Duration::ZERO).await;
    log_agrees(&hub, &thread.id).await;

    hub.answer_permission(&thread.id, ask.data["id"].as_str().unwrap(), Some("allow".into())).await.unwrap();
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    log_agrees(&hub, &thread.id).await;
}

#[tokio::test]
async fn the_log_of_a_question_left_open_gives_idle() {
    let hub = hub();
    let thread = hub.create_thread(new_thread("permission walkaway")).await.unwrap();
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    log_agrees(&hub, &thread.id).await;
}

#[tokio::test]
async fn the_log_of_a_stopped_turn_gives_idle() {
    let hub = hub();
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("needs permission")).await.unwrap();
    next_event(&mut rx, EventKind::PermissionRequest).await;
    hub.cancel(&thread.id).await.unwrap();
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    log_agrees(&hub, &thread.id).await;
}

#[tokio::test]
async fn the_log_of_a_terminal_taken_and_handed_back_gives_the_status() {
    let hub = hub();
    let thread = hub.create_thread(new_thread("hello world")).await.unwrap();
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.take_over(&thread.id, None).await.unwrap();
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().status, ThreadStatus::InTerminal);
    log_agrees(&hub, &thread.id).await;

    hub.hand_back(&thread.id).await.unwrap();
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    log_agrees(&hub, &thread.id).await;
    let markers: Vec<_> = hub.store.events(&thread.id, 0, 10_000).await.unwrap().into_iter().filter(|e| e.kind == EventKind::Status).collect();
    assert_eq!(markers.iter().map(|e| e.data["why"].as_str().unwrap()).collect::<Vec<_>>(), ["terminal", "terminal_back"]);
}

#[tokio::test]
async fn a_core_that_starts_logs_why_it_set_a_cut_off_thread_idle() {
    let dir = hub_core::janitor::test_dir();
    let path = dir.join("hub.db");
    let now = hub_core::model::now_ms();
    let id = {
        let store = Store::open(&path).unwrap();
        let thread = store
            .insert_thread(hub_core::model::Thread {
                id: uuid::Uuid::now_v7().to_string(),
                title: "cut off".into(),
                cwd: dir.display().to_string(),
                engine: "claude".into(),
                status: ThreadStatus::Running,
                engine_session_id: None,
                title_locked: true,
                archived: false,
                returned_at: None,
                seen_at: None,
                settled_at: None,
                sent_at: None,
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
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        thread.id
    };
    let store = Store::open(&path).unwrap();
    assert_eq!(store.thread(&id).await.unwrap().unwrap().status, ThreadStatus::Idle);
    let events = store.events(&id, 0, 100).await.unwrap();
    assert_eq!(events.last().unwrap().kind, EventKind::Status);
    assert_eq!(events.last().unwrap().data["why"], "core_started");
    assert_eq!(replay(ThreadStatus::Running, &events), ThreadStatus::Idle);
}
