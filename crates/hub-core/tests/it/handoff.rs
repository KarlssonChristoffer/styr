//! Handoffs: who does which kind of task (Settings > Handoffs), read by Hub's own helper questions,
//! by Customs and by side jobs, and told to the agents.

use std::sync::Arc;

use hub_core::config::{EngineConfig, EngineTraits};
use hub_core::hub_tools::{Caller, call};
use hub_core::model::{
    CustomsPrefs, CustomsStatus, EventKind, HandoffPick, HandoffPrefs, HandoffTask, NewThread,
};
use hub_core::{Config, Hub, Store};
use serde_json::json;

fn hub() -> Arc<Hub> {
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.engines.clear();
    for name in ["main", "cheap"] {
        // Never started: these tests only look at which one is asked for.
        config.engines.insert(
            name.into(),
            EngineConfig {
                command: "no-such-engine".into(),
                driver: Default::default(),
                secrets: Vec::new(),
                terminal: None,
                traits: EngineTraits::preset("claude"),
                ..Default::default()
            },
        );
    }
    config.default_engine = "main".into();
    Hub::new(Store::in_memory().unwrap(), config)
}

fn pick(engine: &str, model: Option<&str>) -> Option<HandoffPick> {
    Some(HandoffPick {
        engine: engine.into(),
        model: model.map(str::to_owned),
    })
}

fn table(entries: &[(&str, Option<HandoffPick>, Option<HandoffPick>)]) -> HandoffPrefs {
    HandoffPrefs {
        tasks: entries
            .iter()
            .map(|(id, chat, dev)| {
                (
                    (*id).to_owned(),
                    HandoffTask {
                        chat: chat.clone(),
                        dev: dev.clone(),
                    },
                )
            })
            .collect(),
    }
}

fn new_thread(cwd: &str) -> NewThread {
    NewThread {
        cwd: cwd.into(),
        title: Some("t".into()),
        engine: None,
        message: None,
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

#[tokio::test]
async fn the_table_keeps_what_can_be_used_and_nothing_else() {
    let hub = hub();
    let saved = hub_core::handoff::set_prefs(&hub.ctx(),
        table(&[
            ("vetting", pick(" cheap ", Some("  ")), None),
            ("not_a_task", pick("cheap", None), None),
            ("quick", pick("  ", None), None),
        ]),
    )
    .await
    .unwrap();
    assert_eq!(saved.tasks.len(), 1, "{saved:?}");
    assert_eq!(saved.tasks["vetting"].chat, pick("cheap", None));
    assert_eq!(hub_core::handoff::prefs(&hub.ctx()).await.unwrap(), saved);
}

#[tokio::test]
async fn chats_and_dev_threads_can_have_different_picks() {
    let hub = hub();
    let root = tempfile::tempdir().unwrap();
    hub.set_projects_root(&root.path().display().to_string())
        .await
        .unwrap();
    let chat = root
        .path()
        .join("chats")
        .join("20261103-1")
        .display()
        .to_string();
    let dev = root.path().join("atlas").display().to_string();
    hub_core::handoff::set_prefs(&hub.ctx(),
        table(&[("research", pick("cheap", Some("flash")), pick("main", None))]),
    )
    .await
    .unwrap();
    assert_eq!(
        hub_core::handoff::pick(&hub.ctx(), "research", Some(&chat)).await,
        pick("cheap", Some("flash"))
    );
    assert_eq!(
        hub_core::handoff::pick(&hub.ctx(), "research", Some(&dev)).await,
        pick("main", None)
    );
    assert_eq!(
        hub_core::handoff::pick(&hub.ctx(), "research", None).await,
        pick("main", None),
        "no folder is a dev thread"
    );
    assert_eq!(
        hub_core::handoff::pick(&hub.ctx(), "quick", Some(&chat)).await,
        None,
        "nothing chosen for it"
    );
    // A pick for an engine this machine has not got is not used here.
    hub_core::handoff::set_prefs(&hub.ctx(), table(&[("research", pick("elsewhere", None), None)]))
        .await
        .unwrap();
    assert_eq!(
        hub_core::handoff::pick(&hub.ctx(), "research", Some(&chat)).await,
        None
    );
}

#[tokio::test]
async fn helper_questions_follow_the_housekeeping_pick_and_else_the_tidy_engine() {
    let hub = hub();
    assert_eq!(
        hub.tidy_target(None).await,
        ("main".to_owned(), Some("haiku".to_owned()))
    );
    hub_core::handoff::set_prefs(&hub.ctx(),
        table(&[("housekeeping", None, pick("cheap", Some("flash")))]),
    )
    .await
    .unwrap();
    assert_eq!(
        hub.tidy_target(None).await,
        ("cheap".to_owned(), Some("flash".to_owned()))
    );
}

#[tokio::test]
async fn an_agent_is_told_which_tasks_to_hand_over_and_may_not_name_another_engine() {
    let hub = hub();
    let folder = tempfile::tempdir().unwrap();
    let cwd = folder.path().display().to_string();
    assert_eq!(
        hub_core::handoff::instructions(&hub.ctx(), &cwd).await,
        "",
        "nothing chosen, nothing said"
    );
    hub_core::handoff::set_prefs(&hub.ctx(),
        table(&[("research", None, pick("cheap", Some("flash")))]),
    )
    .await
    .unwrap();
    let told = hub_core::handoff::instructions(&hub.ctx(), &cwd).await;
    assert!(
        told.contains("kind \"research\"") && told.contains("cheap, flash"),
        "{told}"
    );
    assert!(!told.contains("review"), "{told}");

    let parent = hub.create_thread(new_thread(&cwd)).await.unwrap();
    let start = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "start_side_job",
        "arguments": { "title": "Look", "task": "find the date", "kind": "research", "engine": "main" } } });
    let answer = hub_core::mcp::handle(&hub, &parent.id, start)
        .await
        .unwrap();
    assert_eq!(answer["result"]["isError"], true, "{answer}");
    assert!(
        answer
            .to_string()
            .contains("the user chose cheap for Research"),
        "{answer}"
    );
}

#[tokio::test]
async fn customs_tells_the_thread_when_the_chosen_engine_vets_what_was_declared() {
    let hub = hub();
    hub_core::customs::set_prefs(
        &hub.ctx(),
        &CustomsPrefs {
            auto_review: false,
            ..CustomsPrefs::default()
        },
    )
    .await
    .unwrap();
    let folder = tempfile::tempdir().unwrap();
    let thread = hub
        .create_thread(new_thread(&folder.path().display().to_string()))
        .await
        .unwrap();
    let declare =
        |name: &str| json!({ "kind": "npm", "name": name, "why": "needed", "summary": "" });

    // Nothing chosen: Customs works as before and the thread gets no note.
    let caller = Caller::Thread(thread.clone());
    call(&hub, &caller, "customs_declare", &declare("left-pad"))
        .await
        .unwrap()
        .unwrap();
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(events.iter().all(|e| e.kind != EventKind::Task));

    hub_core::handoff::set_prefs(&hub.ctx(),
        table(&[("vetting", None, pick("cheap", Some("flash")))]),
    )
    .await
    .unwrap();
    let answer = call(&hub, &caller, "customs_declare", &declare("repo-reverse"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&answer).unwrap()["status"],
        json!(CustomsStatus::Held)
    );
    let notes: Vec<_> = hub
        .store
        .events(&thread.id, 0, 1000)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventKind::Task)
        .collect();
    assert_eq!(notes.len(), 2, "{notes:?}");
    let (first, last) = (&notes[0].data, &notes[1].data);
    assert_eq!(
        (
            first["state"].as_str(),
            first["task"].as_str(),
            first["engine"].as_str(),
            first["model"].as_str()
        ),
        (
            Some("working"),
            Some("vetting"),
            Some("cheap"),
            Some("flash")
        )
    );
    assert_eq!(
        (
            last["state"].as_str(),
            last["verdict"].as_str(),
            last["name"].as_str()
        ),
        (Some("done"), Some("held"), Some("repo-reverse"))
    );
    assert_eq!(first["id"], last["id"], "one id pairs the lines");
}
