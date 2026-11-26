//! Folders and switches, slips, modes and instructions.

use super::*;

#[tokio::test]
async fn a_thread_keeps_the_folders_and_switches_it_started_with_and_a_change_restarts_its_engine() {
    use hub_core::model::{ReachFolder, ThreadReach};
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let extra = hub_core::janitor::test_dir().to_string_lossy().into_owned();
    let reach = ThreadReach { folders: vec![ReachFolder { path: extra.clone(), name: Some("extra".into()) }], connections_off: vec!["Trello".into()] };
    let thread = hub.create_thread(NewThread { reach: Some(reach.clone()), ..new_thread("hello") }).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    assert_eq!(hub_core::reach::get(&hub.ctx(), &thread.id).await.unwrap(), reach);
    assert!(hub.engines().contains(&thread.id));

    // Changed while idle: the engine stops, and the next message starts it again.
    let none = ThreadReach { connections_off: vec!["Trello".into()], ..Default::default() };
    hub.set_reach(&thread.id, none.clone()).await.unwrap();
    assert!(!hub.engines().contains(&thread.id));
    assert_eq!(hub_core::reach::get(&hub.ctx(), &thread.id).await.unwrap(), none);
    // A folder that is not there is refused and changes nothing.
    let missing = ThreadReach { folders: vec![ReachFolder { path: format!("{extra}/nope"), name: None }], ..Default::default() };
    assert!(hub.set_reach(&thread.id, missing).await.is_err());
    assert_eq!(hub_core::reach::get(&hub.ctx(), &thread.id).await.unwrap(), none);
    hub.send(&thread.id, "again".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    assert!(hub.engines().contains(&thread.id));
}

/// The slips of a thread, once `count` of them have been written (they are written in the
/// background, a moment after the message goes).
async fn slips_of(hub: &Hub, thread: &str, count: usize) -> Vec<hub_core::model::Slip> {
    for _ in 0..500 {
        let mut got = hub_core::package::ledger::list(
            &hub.store,
            hub_core::package::ledger::Filter { thread: Some(thread.to_owned()), ..Default::default() },
        )
        .await
        .unwrap();
        if got.len() >= count {
            got.reverse();
            return got;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("fewer than {count} slips were written");
}

#[tokio::test]
async fn every_message_to_an_engine_gets_a_slip_and_a_handover_says_so() {
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

    // A plain message: only the person's words.
    let first = slips_of(&hub, &thread.id, 1).await;
    assert_eq!(first[0].purpose, "message");
    assert_eq!(first[0].engine, "mock");
    assert_eq!(first[0].recipe, "legacy");
    assert!(first[0].session.as_deref().is_some_and(|s| !s.is_empty()), "the slip says which engine session it went into");
    let id = hub.store.thread(&thread.id).await.unwrap().unwrap().engine_session_id;
    assert_eq!(first[0].session, id, "it is the session the thread keeps");
    assert_eq!(first[0].sections.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["you"]);
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(first[0].seq, Some(events[0].seq), "the slip points at the message it was for");

    // A switch hands the conversation over with the next message, and the slip shows how much
    // of what the engine got was the handover.
    hub.switch_engine(&thread.id, "other".into()).await.unwrap();
    hub.send(&thread.id, "what was the word".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let all = slips_of(&hub, &thread.id, 2).await;
    let switched = &all[1];
    assert_eq!(switched.purpose, "switch");
    assert_eq!(switched.engine, "other");
    let ids: Vec<&str> = switched.sections.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.starts_with(&["handover"]) && ids.contains(&"you"), "{ids:?}");
    let handover = switched.sections.iter().find(|s| s.id == "handover").unwrap();
    let you = switched.sections.iter().find(|s| s.id == "you").unwrap();
    assert_eq!(you.chars as usize, "what was the word".chars().count());
    assert!(handover.chars > you.chars, "the conversation is bigger than the question");
    assert!(switched.sections.iter().map(|s| s.tokens).sum::<u32>() >= switched.tokens);
    assert!(!switched.fallback && switched.estimated);

    // The handover goes once: the next message is a plain one again.
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.send(&thread.id, "thanks".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let all = slips_of(&hub, &thread.id, 3).await;
    assert_eq!(all[2].purpose, "message");
    assert!(all[2].tokens < switched.tokens);
}

/// Hermes lists modes and no settings, has no mode called auto, and takes a new mode only when its
/// session is idle. A new thread still starts without asking about edits (its "dont_ask"), and a
/// mode picked while a turn runs is not dropped: it is kept and sent when the turn ends.
#[tokio::test]
async fn a_mode_picked_during_a_turn_of_a_modes_only_engine_waits_for_the_turn() {
    let mut config = test_config();
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig {
            command: mock_agent().display().to_string(),
            env: [("MOCK_MODES_ONLY".to_string(), "1".to_string())].into(),
            traits: hub_core::config::EngineTraits::preset("claude"),
            ..Default::default()
        },
    );
    config.default_engine = "mock".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("settings")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(reply_text(&events).ends_with("mode=dont_ask"), "a new thread starts in the engine's own auto mode: {}", reply_text(&events));

    // The engine refuses a mode while the turn runs; the pick must still end up applied.
    hub.send(&thread.id, "slow hold on".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnStart).await;
    hub.set_setting(&thread.id, "mode".into(), "accept_edits".into()).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let state = next_event(&mut rx, EventKind::SessionState).await;
    assert_eq!(state.data["modes"]["currentModeId"], "accept_edits", "{state:#?}");
    hub.send(&thread.id, "settings".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(reply_text(&events).ends_with("mode=accept_edits"), "{}", reply_text(&events));
}

/// Questions the last core asked cannot be answered by this one: they are closed when it starts,
/// and a card that is still on screen from before says so instead of "no open request".
#[tokio::test]
async fn questions_left_open_by_the_last_core_are_closed_when_this_one_starts() {
    let hub = hub(Store::in_memory().unwrap());
    no_proof(&hub).await;
    let mut rx = hub.subscribe();
    let ask = |id: &str| (EventKind::PermissionRequest, serde_json::json!({ "id": id, "options": [], "tool_call": { "title": "Approve edit: a.html" } }));
    let id = cut_off(&hub, &[ask("a"), ask("b"), (EventKind::PermissionResolved, serde_json::json!({ "id": "a", "option_id": "allow" }))]).await;
    assert_eq!(hub_core::restart::resume(&hub).await.unwrap(), 1);
    turn_ends(&mut rx, 1).await;
    let events = hub.store.events(&id, 0, 1000).await.unwrap();
    let closed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventKind::PermissionResolved && e.data["restart"] == true)
        .map(|e| e.data["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(closed, ["b"], "only the question still open is closed");

    // A card from before the restart that the log still shows open: answering closes it and says why.
    let id = cut_off(&hub, &[ask("c")]).await;
    let err = hub.answer_permission(&id, "c", Some("allow".into())).await.unwrap_err().to_string();
    assert!(err.contains("before Hub restarted"), "{err}");
    let events = hub.store.events(&id, 0, 1000).await.unwrap();
    assert!(events.iter().any(|e| e.kind == EventKind::PermissionResolved && e.data["id"] == "c" && e.data["stale"] == true));
    let err = hub.answer_permission(&id, "nope", None).await.unwrap_err().to_string();
    assert!(err.contains("no open permission request"), "{err}");
}

#[tokio::test]
async fn an_engine_that_does_not_read_mcp_instructions_gets_them_with_its_first_message() {
    let mut config = test_config();
    config.engines.clear();
    let command = mock_agent().display().to_string();
    // One engine that puts MCP instructions in front of the model, one that does not say.
    let reads = hub_core::config::EngineTraits { mcp_instructions: Some(true), ..Default::default() };
    for (name, traits) in [("reads", reads), ("silent", Default::default())] {
        let engine = hub_core::config::EngineConfig { command: command.clone(), traits, ..Default::default() };
        config.engines.insert(name.into(), engine);
    }
    config.default_engine = "reads".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let mut said = Vec::new();
    for name in ["reads", "silent"] {
        let mut rx = hub.subscribe();
        let thread = hub
            .create_thread(NewThread { engine: Some(name.into()), ..new_thread("hello there") })
            .await
            .unwrap();
        next_event(&mut rx, EventKind::TurnEnd).await;
        hub.send(&thread.id, "and again".into(), vec![], None).await.unwrap();
        next_event(&mut rx, EventKind::TurnEnd).await;
        let events = hub.store.events(&thread.id, 0, 1_000_000).await.unwrap();
        // The log keeps the user's own words, never Hub's text.
        let user: Vec<_> = events.iter().filter(|e| e.kind == EventKind::UserMessage).map(|e| e.data["text"].as_str().unwrap().to_owned()).collect();
        assert_eq!(user, ["hello there", "and again"], "{name}");
        said.push(reply_text(&events));
    }
    assert!(!said[0].contains("<hub_instructions>"), "an engine that reads them is not told twice: {}", said[0]);
    let echo = &said[1];
    assert_eq!(echo.matches("<hub_instructions>").count(), 1, "told once per session: {echo}");
    assert!(echo.contains("A diagram, flowchart, schema or chart is not a picture"), "{echo}");
    // The person's words come first: an engine names the session after how the message begins.
    assert!(echo.find("hello there").unwrap() < echo.find("<hub_instructions>").unwrap(), "{echo}");
}

/// Hermes goes on by itself when a background helper finishes: updates arrive after the turn ended,
/// with no prompt of Hub's behind them. The thread must show as working for that, log a turn of its
/// own (`unprompted`), and end it when the engine goes quiet, instead of reading as idle all along.
#[tokio::test]
async fn a_turn_the_engine_starts_on_its_own_shows_as_working_and_ends() {
    let hub = hub(Store::in_memory().unwrap());
    // This Hub's own: the other tests' engines keep the usual quiet. Long enough that a loaded
    // machine never stalls that long between the two updates the agent sends together at the end,
    // which would split its turn in two.
    hub_core::engine::unprompted::set_quiet(&hub, Duration::from_secs(2));
    no_proof(&hub).await;
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("afterwards please")).await.unwrap();
    let first = next_event(&mut rx, EventKind::TurnEnd).await;
    assert!(first.data["unprompted"].is_null());

    let start = next_event(&mut rx, EventKind::TurnStart).await;
    assert_eq!(start.data["unprompted"], true, "{start:#?}");
    assert!(start.data["prompt"].is_null());
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().status, ThreadStatus::Running);

    let end = next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(end.data["unprompted"], true, "{end:#?}");
    assert_eq!(end.data["stop_reason"], "end_turn");
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(reply_text(&events).ends_with("Helper done."), "{}", reply_text(&events));
    assert!(
        events.iter().filter(|e| e.kind == EventKind::TurnEnd).count() == 2,
        "one turn of Hub's and one of the engine's own"
    );
}
