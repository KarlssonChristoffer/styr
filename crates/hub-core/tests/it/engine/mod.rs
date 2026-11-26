//! End-to-end tests of threads against the scripted `mock-agent` engine; the tests sit in the modules below, the shared helpers here.

use std::sync::Arc;
use std::time::Duration;

use hub_core::model::{Attachment, CustomsEdit, CustomsPrefs, CustomsStatus, Event, EventKind, NewProject, NewThread, StreamMsg, ThreadStatus};
use hub_core::{Config, Hub, Store};
use tokio::sync::broadcast;
use crate::support::{hubd, mock_agent, wait_status};

/// Hub's defaults with a data folder of the test's own. Never the real one: a test core that
/// starts there takes over the keepers of the engines working on this machine, and ends them
/// when it lets go (threads died with "Incoming transport closed" while the tests ran).
fn test_config() -> Config {
    Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() }
}

fn hub(store: Store) -> Arc<Hub> {
    let mut config = test_config();
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig { command: mock_agent().display().to_string(), driver: Default::default(), secrets: Vec::new(), terminal: None, traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() },
    );
    config.default_engine = "mock".into();
    Hub::new(store, config)
}

/// Waits for the next event of a kind. Generous, because the updater runs these on a machine
/// still hot from the build, where a mock agent can take seconds to start; a test that is wrong
/// fails all the same, only later.
async fn next_event(rx: &mut broadcast::Receiver<StreamMsg>, kind: EventKind) -> Event {
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

/// Waits until the agent has said its first words: some other update (a command list, a mode)
/// can come before them, and a test that goes on after the first `Update` of any kind may act
/// while the words are still on their way to the log.
async fn next_words(rx: &mut broadcast::Receiver<StreamMsg>) {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            if let StreamMsg::Event { event } = rx.recv().await.unwrap()
                && event.kind == EventKind::Update && event.data["sessionUpdate"] == "agent_message_chunk" {
                    return;
                }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for the agent's first words"))
}

fn reply_text(events: &[Event]) -> String {
    events
        .iter()
        .filter(|e| e.kind == EventKind::Update && e.data["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|e| e.data["content"]["text"].as_str())
        .collect()
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

/// Proof before done asks for more turns; tests about something else turn it off.
async fn no_proof(hub: &Hub) {
    let prefs = hub_core::usage::GuardPrefs { proof: false, ..hub.guard_prefs().await.unwrap() };
    hub.set_guard_prefs(prefs).await.unwrap();
}

/// A repository with one commit, as a thread's folder.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(dir.path().join("readme.md"), "hi\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"]);
    dir
}

fn git(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Waits for every process that carries `thread_id` to be gone. A stopped engine has been told to
/// die when its task ends, but the system takes a moment to tear the process down (longer on a
/// busy Windows machine), so this gives it time and fails only if one lives on.
async fn no_processes_left(thread_id: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let id = thread_id.to_owned();
        let left = tokio::task::spawn_blocking(move || hub_core::workers::thread_processes(&id)).await.unwrap();
        if left.is_empty() {
            return;
        }
        assert!(tokio::time::Instant::now() < deadline, "processes still carry the thread: {left:?}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Turn ends seen on the stream until `n` have come.
async fn turn_ends(rx: &mut broadcast::Receiver<StreamMsg>, n: usize) {
    for _ in 0..n {
        next_event(rx, EventKind::TurnEnd).await;
    }
}

/// A thread whose log ends as a crash left it.
async fn cut_off(hub: &Arc<Hub>, extra: &[(EventKind, serde_json::Value)]) -> String {
    let thread = hub.create_thread(NewThread { message: None, ..new_thread("") }).await.unwrap();
    let said = hub.store.append_event(&thread.id, EventKind::UserMessage, serde_json::json!({ "text": "first" })).await.unwrap();
    hub.store.append_event(&thread.id, EventKind::TurnStart, serde_json::json!({ "prompt": said.seq })).await.unwrap();
    for (kind, data) in extra {
        hub.store.append_event(&thread.id, *kind, data.clone()).await.unwrap();
    }
    thread.id
}

async fn engine_gone(hub: &Hub, id: &str) {
    for _ in 0..800 {
        if !hub.engines().iter().any(|e| e == id) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the engine never stopped");
}

mod turns;
mod projects_and_undo;
mod proof_and_takeover;
mod keepers;
mod worktrees_and_widgets;
mod slips;
