//! Keepers, updates, engine switches, side jobs, routines and the panic button.

use super::*;

/// A hub whose engines run under keepers that keep them in `data`.
fn kept_hub(store: Store, data: &std::path::Path) -> Arc<Hub> {
    let mut config = test_config();
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig { command: mock_agent().display().to_string(), traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() },
    );
    config.default_engine = "mock".into();
    config.data_dir = data.to_owned();
    config.keeper = Some(hubd().clone());
    config.tmux = false;
    Hub::new(store, config)
}

/// Another core on the same data folder (Hub's own tests on the machine Hub runs on) must not
/// take a working engine from the core it works for: taking it over and letting go ended every
/// engine on the machine at once ("Incoming transport closed").
#[tokio::test]
async fn a_second_core_leaves_a_working_engine_alone() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let hub = kept_hub(Store::open(&dir.path().join("hub.db")).unwrap(), &data);
    no_proof(&hub).await;
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("slow one two three four five six")).await.unwrap();
    next_event(&mut rx, EventKind::TurnStart).await;
    next_event(&mut rx, EventKind::Update).await;

    // A stranger with its own database starts on the same folder and looks for engines to take.
    let stranger = kept_hub(Store::open(&dir.path().join("other.db")).unwrap(), &data);
    assert_eq!(hub_core::restart::resume(&stranger).await.unwrap(), 0);
    drop(stranger);
    assert!(data.join("keepers").join(format!("{}.json", thread.id)).exists(), "the card stays");

    // The turn goes on and ends where it started, and the engine takes the next message.
    let end = next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(end.data["stop_reason"], "end_turn");
    hub.send(&thread.id, "after".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(!events.iter().any(|e| e.kind == EventKind::Error), "{events:#?}");
    assert!(reply_text(&events).ends_with("Echo: after"));
}

/// An engine that goes away mid-turn says so in plain words, with why from its keeper, and the
/// next message starts a fresh one instead of failing at once on the dead connection.
#[tokio::test]
async fn an_engine_that_dies_under_its_keeper_says_why_and_the_next_message_works() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let hub = kept_hub(Store::open(&dir.path().join("hub.db")).unwrap(), &data);
    no_proof(&hub).await;
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("first")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;

    hub.send(&thread.id, "crash".into(), vec![], None).await.unwrap();
    let err = next_event(&mut rx, EventKind::Error).await;
    assert!(err.data["message"].as_str().unwrap().starts_with("The engine stopped in the middle of this turn"), "{err:?}");
    assert!(err.data["detail"].as_str().unwrap().contains("exited by itself (exit code 3)"), "{err:?}");
    assert!(err.data["prompt"].is_i64());
    wait_status(&hub, &thread.id, ThreadStatus::Error).await;

    hub.send(&thread.id, "again".into(), vec![], None).await.unwrap();
    let end = next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(end.data["stop_reason"], "end_turn");
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::Error).count(), 1, "{events:#?}");
    assert!(reply_text(&events).ends_with("Echo: again"));
}

#[tokio::test]
async fn an_engine_under_a_keeper_goes_on_through_an_update() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let data = dir.path().join("data");
    let old = kept_hub(Store::open(&path).unwrap(), &data);
    no_proof(&old).await;
    let mut rx = old.subscribe();
    let thread = old.create_thread(new_thread("slow one two three four five six")).await.unwrap();
    next_event(&mut rx, EventKind::TurnStart).await;
    next_words(&mut rx).await;
    old.send_once(&thread.id, "queued".into(), vec![], None, Some("tap-1".into()), false).await.unwrap();
    assert!(old.busy_unkept().await.is_ok_and(|b| !b), "an update need not wait for a kept engine");

    old.park("update").await;
    assert!(old.engines().is_empty());
    // The engine still runs, held for the next core.
    assert!(data.join("keepers").join(format!("{}.json", thread.id)).exists());
    let id = thread.id.clone();
    assert!(!tokio::task::spawn_blocking(move || hub_core::workers::thread_processes(&id)).await.unwrap().is_empty());

    let new = kept_hub(Store::open(&path).unwrap(), &data);
    no_proof(&new).await;
    let mut rx = new.subscribe();
    assert_eq!(hub_core::restart::resume(&new).await.unwrap(), 1);
    turn_ends(&mut rx, 2).await;

    let events = new.store.events(&thread.id, 0, 1000).await.unwrap();
    // Nothing was cut off: no restart, no request to carry on, one session.
    assert!(!events.iter().any(|e| e.kind == EventKind::Restart), "{events:#?}");
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::SessionStarted).count(), 1);
    let ends: Vec<_> = events.iter().filter(|e| e.kind == EventKind::TurnEnd).collect();
    assert_eq!(ends[0].data["prompt"], events[0].seq, "the first turn ended in the new core");
    let text = reply_text(&events);
    assert_eq!(text.matches("Echo: slow one two three four five six").count(), 1, "{text}");
    assert!(text.contains("Echo: queued"), "{text}");
    // The status follows the last TurnEnd a moment later.
    wait_status(&new, &thread.id, ThreadStatus::Idle).await;
}

#[tokio::test]
async fn a_core_that_dies_again_does_not_ask_twice() {
    let hub = hub(Store::in_memory().unwrap());
    no_proof(&hub).await;
    let mut rx = hub.subscribe();
    // The last core logged its request to carry on, then died before the engine got it.
    let id = cut_off(&hub, &[
        (EventKind::Restart, serde_json::json!({ "state": "continued", "by": "crash", "prompt": 1, "queued": [] })),
        (EventKind::UserMessage, serde_json::json!({ "text": hub_core::restart::CARRY_ON, "restart": 1 })),
    ])
    .await;
    assert_eq!(hub_core::restart::resume(&hub).await.unwrap(), 1);
    turn_ends(&mut rx, 1).await;
    let events = hub.store.events(&id, 0, 1000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::Restart).count(), 1);
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::UserMessage).count(), 2);
    assert!(reply_text(&events).starts_with("Echo: Hub restarted"));
}

#[tokio::test]
async fn a_turn_that_keeps_getting_cut_off_is_let_go() {
    let hub = hub(Store::in_memory().unwrap());
    let id = cut_off(&hub, &[]).await;
    // Three times Hub asked to carry on, and each time the core died before the turn ended.
    let mut prompt = 1;
    for _ in 0..3 {
        let log = |kind, data| hub.store.append_event(&id, kind, data);
        log(EventKind::Restart, serde_json::json!({ "state": "continued", "by": "crash", "prompt": prompt, "queued": [] })).await.unwrap();
        prompt = log(EventKind::UserMessage, serde_json::json!({ "text": "carry on", "restart": prompt })).await.unwrap().seq;
        log(EventKind::TurnStart, serde_json::json!({ "prompt": prompt })).await.unwrap();
    }
    assert_eq!(hub_core::restart::resume(&hub).await.unwrap(), 0);
    let events = hub.store.events(&id, 0, 1000).await.unwrap();
    let last = events.last().unwrap();
    assert_eq!(last.kind, EventKind::Restart);
    assert_eq!(last.data["state"], "stopped");
    assert_eq!(last.data["why"], "loop");
    assert!(hub.engines().is_empty());
    // Let go means let go: the next start leaves it alone too.
    assert_eq!(hub_core::restart::resume(&hub).await.unwrap(), 0);
    assert_eq!(hub.store.events(&id, 0, 1000).await.unwrap().len(), events.len());
}

#[tokio::test]
async fn switching_engine_hands_the_conversation_to_the_new_one() {
    let mut config = test_config();
    config.engines.clear();
    for name in ["mock", "other"] {
        config.engines.insert(
            name.into(),
            hub_core::config::EngineConfig { command: mock_agent().display().to_string(), driver: Default::default(), secrets: Vec::new(), terminal: None, traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() },
        );
    }
    config.default_engine = "mock".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("remember the word walrus")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.set_setting(&thread.id, "model".into(), serde_json::json!("old-model")).await.unwrap();

    assert!(hub.switch_engine(&thread.id, "missing".into()).await.is_err());
    let switched = hub.switch_engine(&thread.id, "other".into()).await.unwrap();
    assert_eq!(switched.engine, "other");
    assert!(switched.engine_session_id.is_none());
    let stored = hub.store.thread(&thread.id).await.unwrap().unwrap();
    assert_eq!(stored.engine, "other");
    // The old engine's settings stay behind with it.
    assert!(hub.store.settings(&thread.id).await.unwrap().is_empty());
    // The new engine is already starting, before anything is sent to it.
    assert!(hub.engines().contains(&thread.id));
    next_event(&mut rx, EventKind::SessionStarted).await;

    // The first turn on the new engine fails; the handoff is not used up by it.
    hub.send(&thread.id, "crash".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::Error).await;
    wait_status(&hub, &thread.id, ThreadStatus::Error).await;

    hub.send(&thread.id, "what was the word".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let switch = events.iter().find(|e| e.kind == EventKind::EngineSwitch).unwrap();
    let crashed = events.iter().find(|e| e.kind == EventKind::UserMessage && e.data["text"] == "crash").unwrap();
    assert_eq!(crashed.data["handoff"], switch.seq);
    assert_eq!(switch.data["from"], "mock");
    assert_eq!(switch.data["to"], "other");
    let asked = events.iter().rfind(|e| e.kind == EventKind::UserMessage).unwrap();
    assert_eq!(asked.data["handoff"], switch.seq);
    assert_eq!(asked.data["text"], "what was the word");
    // The new engine heard the earlier conversation before the new message.
    let after: Vec<Event> = events.iter().filter(|e| e.seq > asked.seq).cloned().collect();
    let reply = reply_text(&after);
    assert!(reply.contains("walrus") && reply.ends_with("what was the word"), "{reply}");

    // The handoff goes once.
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.send(&thread.id, "thanks".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let last = events.iter().rfind(|e| e.kind == EventKind::UserMessage).unwrap();
    assert!(last.data.get("handoff").is_none());
    let after: Vec<Event> = events.iter().filter(|e| e.seq > last.seq).cloned().collect();
    assert_eq!(reply_text(&after), "Echo: thanks");
    // The new engine started a session of its own for it.
    let started = events.iter().find(|e| e.kind == EventKind::SessionStarted && e.seq > switch.seq).unwrap();
    assert!(!started.data["resumed"].as_bool().unwrap());
}

#[tokio::test]
async fn editing_from_a_message_leaves_what_followed_behind() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    // Outside git, so there is no snapshot of the files.
    let dir = tempfile::tempdir_in(hub_core::janitor::temp_outside_git()).unwrap();
    let thread = hub.create_thread(NewThread { cwd: dir.path().display().to_string(), ..new_thread("remember the word walrus") }).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.send(&thread.id, "and the colour teal".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let teal = events.iter().find(|e| e.kind == EventKind::UserMessage && e.data["text"] == "and the colour teal").unwrap().seq;

    // No snapshot to roll the files back to outside git, and nothing to edit that is not a message.
    assert!(hub.edit_from(&thread.id, teal, "x".into(), vec![], true).await.is_err());
    assert!(hub.edit_from(&thread.id, teal + 1, "x".into(), vec![], false).await.is_err());

    hub.edit_from(&thread.id, teal, "what was the word".into(), vec![], false).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let rewind = events.iter().find(|e| e.kind == EventKind::Rewind).unwrap();
    assert_eq!(rewind.data["from"], teal);
    assert_eq!(rewind.data["files"], false);
    let asked = events.iter().rfind(|e| e.kind == EventKind::UserMessage).unwrap();
    assert_eq!(asked.data["rewind"], rewind.seq);
    // A fresh session heard what came before the edited message, and nothing after it.
    let after: Vec<Event> = events.iter().filter(|e| e.seq > asked.seq).cloned().collect();
    let reply = reply_text(&after);
    assert!(reply.contains("walrus") && !reply.contains("teal") && reply.ends_with("what was the word"), "{reply}");
    let started = events.iter().find(|e| e.kind == EventKind::SessionStarted && e.seq > rewind.seq).unwrap();
    assert!(!started.data["resumed"].as_bool().unwrap());
    // What was left behind cannot be edited again.
    assert!(hub.edit_from(&thread.id, teal, "x".into(), vec![], false).await.is_err());

    // Once answered, the session carries on as usual.
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.send(&thread.id, "thanks".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let last = events.iter().rfind(|e| e.kind == EventKind::UserMessage).unwrap();
    assert!(last.data.get("rewind").is_none());
    let after: Vec<Event> = events.iter().filter(|e| e.seq > last.seq).cloned().collect();
    assert_eq!(reply_text(&after), "Echo: thanks");
}

#[tokio::test]
async fn warming_starts_a_stopped_engine_ahead_of_the_message() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("hello")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.shutdown(&thread.id, false).await.unwrap();
    assert!(!hub.engines().contains(&thread.id));

    hub.warm(&thread.id).await.unwrap();
    let started = next_event(&mut rx, EventKind::SessionStarted).await;
    assert!(started.data["resumed"].as_bool().unwrap());
    // Warming a running engine leaves it be.
    hub.warm(&thread.id).await.unwrap();
    hub.send(&thread.id, "again".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::SessionStarted).count(), 2);
    // The thread turns idle just after its TurnEnd goes out, so wait for it as above.
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
}

#[tokio::test]
async fn a_side_job_draws_on_another_engine_and_reports_back() {
    let mut config = test_config();
    config.engines.clear();
    for name in ["mock", "drawer"] {
        let command = mock_agent().display().to_string();
        config.engines.insert(name.into(), hub_core::config::EngineConfig { command, driver: Default::default(), secrets: Vec::new(), terminal: None, traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() });
    }
    config.default_engine = "mock".into();
    config.side_jobs.insert("image".into(), "drawer".into());
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let mut rx = hub.subscribe();
    let mut parent = new_thread("");
    parent.message = None;
    let parent = hub.create_thread(parent).await.unwrap();

    let call = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "start_side_job",
        "arguments": { "title": "Sunset", "task": "draw a sunset, flat pixel style", "kind": "image" } } });
    let answer = hub_core::mcp::handle(&hub, &parent.id, call).await.unwrap();
    assert_eq!(answer["result"]["isError"], false, "{answer}");

    // The parent notes the job at once, then its result.
    let started = next_event(&mut rx, EventKind::Child).await;
    assert_eq!((started.data["state"].as_str(), started.data["engine"].as_str()), (Some("working"), Some("drawer")));
    let child_id = started.data["id"].as_str().unwrap().to_owned();
    let child = hub.store.thread(&child_id).await.unwrap().unwrap();
    assert_eq!((child.parent.as_deref(), child.engine.as_str(), child.title.as_str()), (Some(parent.id.as_str()), "drawer", "Sunset"));
    let done = next_event(&mut rx, EventKind::Child).await;
    assert_eq!(done.thread_id, parent.id);
    assert_eq!(done.data["state"], "done", "{}", done.data);
    assert!(done.data["images"][0].as_str().unwrap().ends_with("sunset.png"), "{}", done.data);
    assert!(done.data["reply"].as_str().unwrap().contains("draw a sunset"));

    // The parent's agent gets it as a marked message, which starts its turn.
    let told = next_event(&mut rx, EventKind::UserMessage).await;
    assert_eq!((told.thread_id.as_str(), told.data["child"].as_str()), (parent.id.as_str(), Some(child_id.as_str())));
    assert!(told.data["text"].as_str().unwrap().contains("sunset.png"));

    // A side job has no side jobs of its own.
    let list = serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" });
    let names = |answer: serde_json::Value| answer["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    assert!(names(hub_core::mcp::handle(&hub, &parent.id, list.clone()).await.unwrap()).contains(&"start_side_job".to_owned()));
    assert!(!names(hub_core::mcp::handle(&hub, &child_id, list).await.unwrap()).contains(&"start_side_job".to_owned()));
}

/// A picture in an everyday chat shows itself: the chat's agent is not woken to say it is done.
#[tokio::test]
async fn a_picture_drawn_for_a_chat_does_not_wake_the_chats_agent() {
    let mut config = test_config();
    config.engines.clear();
    for name in ["mock", "drawer"] {
        let command = mock_agent().display().to_string();
        config.engines.insert(name.into(), hub_core::config::EngineConfig { command, driver: Default::default(), secrets: Vec::new(), terminal: None, traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() });
    }
    config.default_engine = "mock".into();
    config.side_jobs.insert("image".into(), "drawer".into());
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let root = tempfile::tempdir().unwrap();
    hub.set_projects_root(&root.path().display().to_string()).await.unwrap();
    let chat = root.path().join("chats").join("20261110-1");
    std::fs::create_dir_all(&chat).unwrap();
    let mut rx = hub.subscribe();
    let mut parent = new_thread("");
    parent.message = None;
    parent.cwd = chat.display().to_string();
    let parent = hub.create_thread(parent).await.unwrap();

    let call = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "start_side_job",
        "arguments": { "title": "Sunset", "task": "draw a sunset, flat pixel style", "kind": "image" } } });
    let answer = hub_core::mcp::handle(&hub, &parent.id, call).await.unwrap();
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    assert!(answer.to_string().contains("You are not called back"), "{answer}");
    let done = loop {
        let e = next_event(&mut rx, EventKind::Child).await;
        if e.data["state"] == "done" {
            break e;
        }
    };
    assert_eq!((done.thread_id.as_str(), done.data["quiet"].as_bool()), (parent.id.as_str(), Some(true)), "{}", done.data);
    let child_id = done.data["id"].as_str().unwrap().to_owned();
    wait_status(&hub, &child_id, ThreadStatus::Idle).await;
    // Marked just after the event goes out.
    let mut marked = false;
    for _ in 0..100 {
        marked = hub.store.thread(&parent.id).await.unwrap().unwrap().returned_at.is_some();
        if marked {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(marked, "the chat has something new to see");

    // It hears of it with the next message, once.
    for (text, told) in [("thanks", true), ("and again", false)] {
        let mut rx = hub.subscribe();
        hub.send(&parent.id, text.into(), Vec::new(), None).await.unwrap();
        next_event(&mut rx, EventKind::TurnEnd).await;
        let events = hub.store.events_after_last(&parent.id, EventKind::UserMessage, &[]).await.unwrap();
        assert_eq!(reply_text(&events).contains("the side job \"Sunset\" finished"), told, "{text}: {}", reply_text(&events));
    }
    // Only the person's two messages reached the chat's agent: none of Hub's about the job.
    let said = hub.store.events_of(&parent.id, 0, &[EventKind::UserMessage]).await.unwrap();
    assert_eq!(said.iter().map(|e| (e.data["text"].as_str().unwrap_or_default(), e.data["child"].is_null())).collect::<Vec<_>>(), [("thanks", true), ("and again", true)]);
}

#[tokio::test]
async fn an_engine_that_cannot_start_stops_retrying_and_a_side_job_on_it_fails_once() {
    let mut config = test_config();
    config.engines.clear();
    config.engines.insert("mock".into(), hub_core::config::EngineConfig { command: mock_agent().display().to_string(), driver: Default::default(), secrets: Vec::new(), terminal: None, traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() });
    let broken = hub_core::config::EngineConfig { command: "/nonexistent/engine".into(), driver: Default::default(), secrets: Vec::new(), terminal: None, traits: Default::default(), ..Default::default() };
    config.engines.insert("broken".into(), broken);
    config.default_engine = "mock".into();
    config.side_jobs.insert("image".into(), "broken".into());
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let mut parent = new_thread("");
    parent.message = None;
    let parent = hub.create_thread(parent).await.unwrap();

    let job = hub_core::children::SideJob { title: "Picture".into(), task: "a picture".into(), kind: "image".into(), context: "brief".into(), engine: None, model: None };
    let child = hub.start_side_job(&parent.id, job).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;

    let errors = hub.store.events(&child.id, 0, 1000).await.unwrap().iter().filter(|e| e.kind == EventKind::Error).count();
    assert!((1..=3).contains(&errors), "{errors} errors");
    let events = hub.store.events(&parent.id, 0, 1000).await.unwrap();
    let failed = events.iter().filter(|e| e.kind == EventKind::Child && e.data["state"] == "failed").count();
    let told = events.iter().filter(|e| e.kind == EventKind::UserMessage && e.data["child"].is_string()).count();
    assert_eq!((failed, told), (1, 1));
}

#[tokio::test]
async fn a_routine_only_reports_and_its_thread_is_yours_after() {
    use hub_core::model::{RoutineDraft, RunState, Schedule};
    let hub = hub(Store::in_memory().unwrap());
    let prefs = hub_core::model::PermissionPrefs { allow: vec!["*".into()], ..Default::default() };
    hub.set_permission_prefs(prefs).await.unwrap();
    let draft = RoutineDraft {
        name: "Sweep".into(),
        folder: std::env::temp_dir().display().to_string(),
        engine: None,
        prompt: "danger".into(),
        schedule: Schedule::default(),
        enabled: true,
        template: None,
    };
    let routine = hub_core::routines::create(&hub.ctx(), draft).await.unwrap();
    let mut rx = hub.subscribe();
    let thread = hub_core::routine_runs::start(&hub, &routine.id, "you").await.unwrap();

    // Your rules allow everything, but the run only reports.
    let auto = next_event(&mut rx, EventKind::PermissionAuto).await;
    assert_eq!(auto.data["allowed"], false);
    assert!(auto.data["why"].as_str().unwrap().contains("routine"));
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread, 0, 1000).await.unwrap();
    assert_eq!(events[0].data["routine"]["name"], "Sweep");

    let mut run = None;
    for _ in 0..200 {
        let list = hub_core::routines::list(&hub.ctx()).await.unwrap();
        let first = list.routines[0].runs[0].clone();
        if first.state != RunState::Working {
            run = Some(first);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let run = run.expect("the run never ended");
    assert_eq!(run.state, RunState::Done);
    assert_eq!(run.thread.as_deref(), Some(thread.as_str()));

    // Answering in the thread is ordinary work again.
    wait_status(&hub, &thread, ThreadStatus::Idle).await;
    hub.send(&thread, "danger".into(), vec![], None).await.unwrap();
    let auto = next_event(&mut rx, EventKind::PermissionAuto).await;
    assert_eq!(auto.data["allowed"], true);
    next_event(&mut rx, EventKind::TurnEnd).await;
}

#[tokio::test]
async fn the_panic_button_pauses_every_agent_and_resume_carries_them_on() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let hub1 = hub(Store::open(&path).unwrap());
    no_proof(&hub1).await;
    let mut rx = hub1.subscribe();
    let thread = hub1.create_thread(new_thread("slow one two three four five six")).await.unwrap();
    next_event(&mut rx, EventKind::TurnStart).await;

    let halt = hub_core::halt::pause(&hub1, Default::default()).await.unwrap();
    assert_eq!(halt.threads, vec![thread.id.clone()]);
    assert!(hub1.engines().is_empty());
    let events = hub1.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(!events.iter().any(|e| matches!(e.kind, EventKind::TurnEnd | EventKind::Error)));
    assert!(events.iter().any(|e| e.kind == EventKind::Paused));
    assert_eq!(hub1.store.thread(&thread.id).await.unwrap().unwrap().status, ThreadStatus::Idle);
    // While paused a message is kept, not sent, and no engine starts, here or in a new thread.
    hub1.send(&thread.id, "while paused".into(), vec![], None).await.unwrap();
    hub1.create_thread(new_thread("another")).await.unwrap();
    assert!(hub1.engines().is_empty());

    // A core started while paused stays paused and carries nothing on.
    drop(hub1);
    let hub2 = hub(Store::open(&path).unwrap());
    no_proof(&hub2).await;
    hub_core::halt::load(&hub2).await.unwrap();
    assert!(hub2.halted());
    assert_eq!(hub_core::restart::resume(&hub2).await.unwrap(), 0);
    assert!(hub2.engines().is_empty());

    let mut rx = hub2.subscribe();
    let resumed = hub_core::halt::resume(&hub2, Default::default()).await.unwrap();
    assert_eq!(resumed.threads, 2);
    assert!(!hub2.halted());
    turn_ends(&mut rx, 3).await;
    let events = hub2.store.events(&thread.id, 0, 1000).await.unwrap();
    let restart = events.iter().find(|e| e.kind == EventKind::Restart).expect("carried on");
    assert_eq!(restart.data["by"], "pause");
    assert_eq!(restart.data["state"], "continued");
    let text = reply_text(&events);
    let carried = text.find("Echo: Hub paused every agent").expect("asked to carry on");
    let waited = text.find("Echo: while paused").expect("the waiting message went out");
    assert!(carried < waited, "{text}");
}
