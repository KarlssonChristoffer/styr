//! Proof asked for, customs, attempts and taking a thread over in a terminal.

use super::*;

#[tokio::test]
async fn a_reply_that_changed_code_without_proof_is_asked_for_it_once() {
    let dir = repo();
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let mut new = new_thread("write please");
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new).await.unwrap();

    let asked = next_event(&mut rx, EventKind::Proof).await;
    assert_eq!((asked.data["state"].as_str(), asked.data["files"].as_i64()), (Some("asked"), Some(1)), "{}", asked.data);
    let request = next_event(&mut rx, EventKind::UserMessage).await;
    assert_eq!(request.data["proof"], asked.data["reply"]);
    // Still none after asking: recorded, and not asked again.
    let missing = next_event(&mut rx, EventKind::Proof).await;
    assert_eq!((missing.data["state"].as_str(), &missing.data["reply"]), (Some("missing"), &asked.data["reply"]));
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // A test run that passes is proof.
    hub.send(&thread.id, "write and run the tool".into(), vec![], None).await.unwrap();
    let shown = next_event(&mut rx, EventKind::Proof).await;
    assert_eq!(shown.data["state"], "shown", "{}", shown.data);
    assert_eq!((shown.data["kind"].as_str(), shown.data["command"].as_str()), (Some("tests"), Some("cargo test")));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::UserMessage).count(), 3, "no second request");
}

/// A page is asked to be looked at with `page_check`, and that look is the proof, whichever way
/// it went. Code other than pages still gets the question about tests.
#[tokio::test]
async fn a_page_without_proof_is_asked_to_be_looked_at_and_the_look_counts() {
    let dir = repo();
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let mut new = new_thread("make a homepage please");
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new).await.unwrap();

    let asked = next_event(&mut rx, EventKind::Proof).await;
    assert_eq!(asked.data["state"], "asked", "{}", asked.data);
    let request = next_event(&mut rx, EventKind::UserMessage).await;
    let text = request.data["text"].as_str().unwrap();
    assert!(text.contains("look at the page with page_check (`file`: index.html;"), "{text}");
    assert!(!text.contains("run the project's tests"), "{text}");
    let shown = next_event(&mut rx, EventKind::Proof).await;
    assert_eq!((shown.data["state"].as_str(), &shown.data["reply"]), (Some("shown"), &asked.data["reply"]), "{}", shown.data);
    assert_eq!((shown.data["kind"].as_str(), shown.data["command"].as_str()), (Some("screenshot"), Some("page_check index.html")));
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // A look that finds an error in the page is a failed proof, not a missing one.
    hub.send(&thread.id, "homepage again, oops".into(), vec![], None).await.unwrap();
    assert_eq!(next_event(&mut rx, EventKind::Proof).await.data["state"], "asked");
    let failed = next_event(&mut rx, EventKind::Proof).await;
    assert_eq!(failed.data["state"], "failed", "{}", failed.data);
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // Code that is not a page gets the question about tests, as before.
    hub.send(&thread.id, "write it".into(), vec![], None).await.unwrap();
    assert_eq!(next_event(&mut rx, EventKind::Proof).await.data["state"], "asked");
    let code = next_event(&mut rx, EventKind::UserMessage).await;
    let text = code.data["text"].as_str().unwrap();
    assert!(text.contains("run the project's tests or checks") && !text.contains("page_check"), "{text}");
}

#[tokio::test]
async fn no_proof_is_asked_for_when_told_not_to_or_when_code_was_only_taken_out() {
    let dir = repo();
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let mut new = new_thread("write it, no tests needed");
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new).await.unwrap();

    let waived = next_event(&mut rx, EventKind::Proof).await;
    assert_eq!((waived.data["state"].as_str(), waived.data["why"].as_str()), (Some("skipped"), Some("waived")), "{}", waived.data);
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // Deleting what an earlier turn wrote leaves nothing to show working.
    hub.send(&thread.id, "erase that file again".into(), vec![], None).await.unwrap();
    let removed = next_event(&mut rx, EventKind::Proof).await;
    assert_eq!((removed.data["state"].as_str(), removed.data["why"].as_str()), (Some("skipped"), Some("removed")), "{}", removed.data);
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::UserMessage).count(), 2, "Hub asked for nothing");
}

#[tokio::test]
async fn code_a_doc_covers_gets_its_doc_checked_once_and_reverts_are_left_alone() {
    let dir = repo();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/notes.md"), "# Notes\n\n## The mock file\n<!-- covers: mock.txt -->\nIt holds the prompt.\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "docs"]);
    let hub = hub(Store::in_memory().unwrap());
    no_proof(&hub).await;
    let mut rx = hub.subscribe();
    let mut new = new_thread("write please");
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new).await.unwrap();

    let asked = next_event(&mut rx, EventKind::Docs).await;
    assert_eq!(asked.data["state"], "asked", "{}", asked.data);
    assert_eq!((asked.data["sections"][0]["doc"].as_str(), asked.data["sections"][0]["heading"].as_str()), (Some("docs/notes.md"), Some("The mock file")));
    let request = next_event(&mut rx, EventKind::UserMessage).await;
    assert_eq!(request.data["docs"], asked.data["reply"]);
    assert!(request.data["text"].as_str().unwrap().contains("docs/notes.md › The mock file (lines 3-5), for mock.txt"));
    // The mock does not open the doc, so the answer is noted as ignored, and not asked again.
    let answer = next_event(&mut rx, EventKind::Docs).await;
    assert_eq!((answer.data["state"].as_str(), &answer.data["reply"]), (Some("ignored"), &asked.data["reply"]));
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // Taking the file out again puts it back as it was: nothing for the docs.
    hub.send(&thread.id, "erase that file again".into(), vec![], None).await.unwrap();
    let reverted = next_event(&mut rx, EventKind::Docs).await;
    assert_eq!((reverted.data["state"].as_str(), reverted.data["why"].as_str()), (Some("skipped"), Some("reverted")), "{}", reverted.data);
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::UserMessage).count(), 3, "one request in all");
}

#[tokio::test]
async fn keys_a_turn_wrote_are_reported_to_the_agent_and_kept_out_of_the_log() {
    let dir = repo();
    let key = "sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789";
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let mut new = new_thread(&format!("write {key}"));
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new).await.unwrap();

    let leak = next_event(&mut rx, EventKind::Leak).await;
    let found = &leak.data["findings"][0];
    assert_eq!((found["path"].as_str(), found["line"].as_i64(), found["kind"].as_str()), (Some("mock.txt"), Some(1), Some("Anthropic API key")));
    assert_eq!(found["hint"], "sk-a…6789");
    let sent = next_event(&mut rx, EventKind::UserMessage).await;
    assert_eq!(sent.data["leak"], leak.data["seq"]);
    assert!(sent.data["text"].as_str().unwrap().contains("mock.txt:1"));
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // The engine got the key; the log has only a hint of it.
    assert!(std::fs::read_to_string(dir.path().join("mock.txt")).unwrap().contains(key));
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let log = serde_json::to_string(&events).unwrap();
    assert!(!log.contains(key) && log.contains("[Anthropic API key sk-a…6789]"));
    // One message about it, and no proof request on top.
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::UserMessage).count(), 2);
}

#[tokio::test]
async fn what_a_turn_brings_in_without_clearing_customs_is_held_and_reported() {
    let dir = repo();
    let hub = hub(Store::in_memory().unwrap());
    let quiet = CustomsPrefs { auto_review: false, ..CustomsPrefs::default() };
    hub_core::customs::set_prefs(&hub.ctx(), &quiet).await.unwrap();
    let mut rx = hub.subscribe();
    let link = r#"cdn <script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.min.js"></script>"#;
    let mut new = new_thread(link);
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new).await.unwrap();

    let stopped = next_event(&mut rx, EventKind::Customs).await;
    let found = &stopped.data["found"][0];
    assert_eq!((found["kind"].as_str(), found["name"].as_str(), found["path"].as_str()), (Some("cdn"), Some("bootstrap"), Some("index.html")));
    assert_eq!((found["status"].as_str(), stopped.data["sent"].as_bool()), (Some("held"), Some(true)));
    // A held item never wakes the agent: no message from Customs goes to it.
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(!events.iter().any(|e| e.kind == EventKind::UserMessage && e.data.get("customs").is_some()));
    let held = hub_core::customs::list(&hub.ctx()).await.unwrap();
    assert_eq!((held[0].name.as_str(), held[0].status, held[0].decided_by.as_str()), ("bootstrap", CustomsStatus::Held, "found"));

    // Once cleared as the npm package it is, the same link passes.
    hub_core::customs::remove(&hub.ctx(), "cdn", "bootstrap").await.unwrap();
    let npm = CustomsEdit { kind: "npm".into(), name: "bootstrap".into(), status: Some(CustomsStatus::Cleared), category: None, summary: None, versions: None, reason: None };
    hub_core::customs::edit(&hub.ctx(), npm).await.unwrap();
    hub.send(&thread.id, format!("{link} <!-- again -->"), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::Customs).count(), 1);
}

#[tokio::test]
async fn the_customs_review_asks_the_engine_and_holds_what_it_cannot_verify() {
    let hub = hub(Store::in_memory().unwrap());
    let quiet = CustomsPrefs { auto_review: false, ..CustomsPrefs::default() };
    hub_core::customs::set_prefs(&hub.ctx(), &quiet).await.unwrap();
    // No registry knows an MCP server, so it goes straight to the model. The mock engine only
    // echoes, which is not the verdict asked for: held, saying why.
    hub_core::customs::request(&hub.ctx(), "mcp", "weather-server", "forecasts", "", "agent", None).await.unwrap();
    let item = hub_core::customs_review::review(&hub, "mcp", "weather-server").await.unwrap().unwrap();
    assert_eq!((item.status, item.decided_by.as_str()), (CustomsStatus::Held, "review"));
    assert!(item.reason.starts_with("Review: yellow. The checks passed, but the model review did not run"), "{}", item.reason);
    assert!(item.reviewed_at.is_some());

    // A person's clearing stands through a review.
    let mine = CustomsEdit { kind: "mcp".into(), name: "weather-server".into(), status: Some(CustomsStatus::Cleared), category: None, summary: None, versions: None, reason: None };
    hub_core::customs::edit(&hub.ctx(), mine).await.unwrap();
    let item = hub_core::customs_review::review(&hub, "mcp", "weather-server").await.unwrap().unwrap();
    assert_eq!((item.status, item.decided_by.as_str()), (CustomsStatus::Cleared, "you"));
}

#[tokio::test]
async fn two_threads_editing_the_same_file_are_both_warned() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let mut new = new_thread("edit");
    new.cwd = dir.path().display().to_string();
    let first = hub.create_thread(new.clone()).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let second = hub.create_thread(new).await.unwrap();
    let a = next_event(&mut rx, EventKind::Clash).await;
    let b = next_event(&mut rx, EventKind::Clash).await;
    let mut seen = [(a.thread_id.clone(), a.data["other"].as_str().unwrap().to_owned()), (b.thread_id.clone(), b.data["other"].as_str().unwrap().to_owned())];
    seen.sort();
    let mut want = [(first.id.clone(), second.id.clone()), (second.id.clone(), first.id.clone())];
    want.sort();
    assert_eq!(seen, want);
    assert!(a.data["path"].as_str().unwrap().ends_with("src/lib.rs"));
    wait_status(&hub, &second.id, ThreadStatus::Idle).await;

    // The next message tells the agent.
    hub.send(&first.id, "hello".into(), vec![], None).await.unwrap();
    while next_event(&mut rx, EventKind::TurnEnd).await.thread_id != first.id {}
    let events = hub.store.events(&first.id, 0, 1000).await.unwrap();
    assert!(reply_text(&events).contains("is also changing"), "{}", reply_text(&events));
    // Each pair is warned once per file.
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::Clash).count(), 1);
}

#[tokio::test]
async fn attempts_run_side_by_side_and_the_picked_one_lands_in_the_folder() {
    let dir = repo();
    let hub = hub(Store::in_memory().unwrap());
    no_proof(&hub).await;
    let mut rx = hub.subscribe();
    let mut new = new_thread("write it");
    new.cwd = dir.path().display().to_string();
    new.attempts = vec![
        [("model".to_owned(), serde_json::json!("opus"))].into(),
        [("model".to_owned(), serde_json::json!("haiku"))].into(),
    ];
    let first = hub.create_thread(new).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    next_event(&mut rx, EventKind::TurnEnd).await;

    let group = hub.attempts(&first.id).await.unwrap();
    assert_eq!(group.attempts.len(), 2);
    let models: Vec<_> = group.attempts.iter().map(|a| a.model.clone().unwrap_or_default()).collect();
    assert_eq!(models, ["opus", "haiku"]);
    assert!(group.attempts.iter().all(|a| a.files == 1 && a.title.ends_with(&"/2".to_string())));
    assert!(!dir.path().join("mock.txt").exists(), "attempts work in their own worktrees");
    let second = group.attempts[1].thread_id.clone();
    wait_status(&hub, &second, ThreadStatus::Idle).await;
    wait_status(&hub, &first.id, ThreadStatus::Idle).await;

    let after = hub.pick_attempt(&second).await.unwrap();
    assert!(after.done);
    assert_eq!(after.attempts.iter().map(|a| a.state.clone().unwrap()).collect::<Vec<_>>(), ["dropped", "picked"]);
    assert!(dir.path().join("mock.txt").exists());
    let picked = hub.store.thread(&second).await.unwrap().unwrap();
    assert_eq!((picked.cwd.as_str(), picked.title.as_str()), (dir.path().to_str().unwrap(), "write it"));
    assert!(hub.store.thread(&first.id).await.unwrap().unwrap().archived);
    assert!(!git(dir.path(), &["worktree", "list"]).contains("hub-attempts"));
    assert!(hub.pick_attempt(&first.id).await.is_err(), "once per group");

    // Picking can be undone like any rollback.
    let events = hub.store.events(&second, 0, 1000).await.unwrap();
    let safety = events.iter().rev().find(|e| e.kind == EventKind::Attempt).unwrap().data["safety"].as_str().unwrap().to_owned();
    hub.undo(&second, &safety).await.unwrap();
    assert!(!dir.path().join("mock.txt").exists());
}

/// Taking a thread over stops its engine and refuses Hub's messages; handing it back loads the
/// session again and copies in what was said in the terminal.
#[tokio::test]
async fn a_thread_taken_over_in_a_terminal_comes_back_with_what_was_said_there() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub_config = test_config();
    hub_config.engines.clear();
    hub_config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig {
            command: mock_agent().display().to_string(),
            driver: Default::default(),
            secrets: Vec::new(),
            terminal: Some("mock-cli --resume {session}".into()),
            traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default()
        },
    );
    hub_config.default_engine = "mock".into();
    hub_config.tmux = false;
    let hub = Hub::new(Store::in_memory().unwrap(), hub_config);
    let mut rx = hub.subscribe();
    let mut new = new_thread("first from hub");
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    let taken = hub.take_over(&thread.id, Some(std::process::id())).await.unwrap();
    let session = hub.store.thread(&thread.id).await.unwrap().unwrap().engine_session_id.unwrap();
    assert_eq!(taken.command, format!("mock-cli --resume {session}"));
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().status, ThreadStatus::InTerminal);
    assert!(hub.engines().is_empty(), "Hub's engine lets go of the session");
    let refused = hub.send(&thread.id, "from hub meanwhile".into(), vec![], None).await;
    assert!(refused.unwrap_err().to_string().contains("terminal"));

    // What the CLI wrote to the session while it had it.
    std::fs::write(
        dir.path().join(".mock-history.jsonl"),
        concat!(
            r#"{"user":"first from hub"}"#, "\n",
            r#"{"agent":"Echo: first from hub"}"#, "\n",
            r#"{"user":"typed in the terminal"}"#, "\n",
            r#"{"agent":"done in the terminal"}"#, "\n",
        ),
    )
    .unwrap();
    hub.hand_back(&thread.id).await.unwrap();
    let imported = loop {
        let e = next_event(&mut rx, EventKind::Terminal).await;
        if e.data["state"] == "imported" {
            break e;
        }
    };
    assert_eq!(imported.data["turns"], 1);
    assert_eq!(imported.data["replay_turns"], 2);
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let copied: Vec<&str> = events
        .iter()
        .filter(|e| e.kind == EventKind::Update && e.seq < imported.seq)
        .filter_map(|e| e.data["content"]["text"].as_str())
        .collect();
    assert!(copied.ends_with(&["typed in the terminal", "done in the terminal"]), "{copied:?}");
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().status, ThreadStatus::Idle);

    // Hub carries on in the same session, and a second load copies nothing twice.
    hub.send(&thread.id, "back in hub".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.take_over(&thread.id, None).await.unwrap();
    hub.hand_back(&thread.id).await.unwrap();
    let again = loop {
        let e = next_event(&mut rx, EventKind::Terminal).await;
        if e.data["state"] == "imported" {
            break e;
        }
    };
    // The mock replays only its file, which lacks "back in hub": nothing new after the known three.
    assert_eq!(again.data["turns"], 0);
}

/// Taking over in the middle of a turn stops it; when the terminal only interrupted it, the thread
/// carries on by itself once it is handed back, once.
#[tokio::test]
async fn a_turn_stopped_by_taking_over_carries_on_when_handed_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut hub_config = test_config();
    hub_config.engines.clear();
    hub_config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig {
            command: mock_agent().display().to_string(),
            driver: Default::default(),
            secrets: Vec::new(),
            terminal: Some("mock-cli --resume {session}".into()),
            traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default()
        },
    );
    hub_config.default_engine = "mock".into();
    hub_config.tmux = false;
    let hub = Hub::new(Store::in_memory().unwrap(), hub_config);
    let mut rx = hub.subscribe();
    let mut new = new_thread("slow one two three four five six");
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new).await.unwrap();
    next_event(&mut rx, EventKind::TurnStart).await;
    // The session is open once the turn is under way.
    while hub.store.thread(&thread.id).await.unwrap().unwrap().engine_session_id.is_none() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let taken = hub.take_over(&thread.id, None).await.unwrap();
    assert!(taken.stopped_turn);
    std::fs::write(
        dir.path().join(".mock-history.jsonl"),
        concat!(r#"{"user":"slow one two three four five six"}"#, "\n", r#"{"user":"[Request interrupted by user for tool use]"}"#, "\n"),
    )
    .unwrap();
    hub.hand_back(&thread.id).await.unwrap();
    let imported = loop {
        let e = next_event(&mut rx, EventKind::Terminal).await;
        if e.data["state"] == "imported" {
            break e;
        }
    };
    assert_eq!(imported.data["carry_on"], true);
    let asked = next_event(&mut rx, EventKind::UserMessage).await;
    assert!(asked.data["terminal"].is_i64(), "{:?}", asked.data);
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // Taken over while idle: nothing to carry on.
    let taken = hub.take_over(&thread.id, None).await.unwrap();
    assert!(!taken.stopped_turn);
    hub.hand_back(&thread.id).await.unwrap();
    let again = loop {
        let e = next_event(&mut rx, EventKind::Terminal).await;
        if e.data["state"] == "imported" {
            break e;
        }
    };
    assert!(again.data["carry_on"].is_null());
    let events = hub.store.events(&thread.id, 0, 10_000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::UserMessage && !e.data["terminal"].is_null()).count(), 1);
}

#[tokio::test]
async fn a_thread_without_a_terminal_command_or_session_is_not_taken_over() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("hello")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let err = hub.take_over(&thread.id, None).await.unwrap_err();
    assert!(err.to_string().contains("terminal command"), "{err}");
}

/// A session started ahead of its first message has no conversation for the engine's CLI to
/// resume (`claude --resume` says "No conversation found"), so the CLI starts one under the id;
/// an engine that cannot do that says to send a message first.
#[tokio::test]
async fn a_session_with_nothing_said_is_started_fresh_in_the_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let engine = |new: Option<&str>| hub_core::config::EngineConfig {
        command: mock_agent().display().to_string(),
        driver: Default::default(),
        secrets: Vec::new(),
        terminal: Some("mock-cli --resume {session}".into()),
        traits: hub_core::config::EngineTraits { terminal_new: new.map(String::from), ..Default::default() },
        ..Default::default()
    };
    let mut hub_config = test_config();
    hub_config.engines.clear();
    hub_config.engines.insert("mock".into(), engine(Some("mock-cli --session-id {session}")));
    hub_config.engines.insert("plain".into(), engine(None));
    hub_config.default_engine = "mock".into();
    hub_config.tmux = false;
    let hub = Hub::new(Store::in_memory().unwrap(), hub_config);
    let mut rx = hub.subscribe();

    for (name, fresh) in [("mock", true), ("plain", false)] {
        let mut new = new_thread("");
        new.message = None;
        new.engine = Some(name.into());
        new.cwd = dir.path().display().to_string();
        let thread = hub.create_thread(new).await.unwrap();
        hub.warm(&thread.id).await.unwrap();
        let session = loop {
            match hub.store.thread(&thread.id).await.unwrap().unwrap().engine_session_id {
                Some(s) => break s,
                None => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        };
        let taken = hub.take_over(&thread.id, None).await;
        if fresh {
            let taken = taken.unwrap();
            assert!(taken.fresh);
            assert_eq!(taken.command, format!("mock-cli --session-id {session}"));
            hub.hand_back(&thread.id).await.unwrap();
        } else {
            let err = taken.unwrap_err().to_string();
            assert!(err.contains("nothing has been said"), "{err}");
            assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().status, ThreadStatus::Idle);
        }
    }

    // Once the engine has had a prompt, the session is resumed as before.
    let mut new = new_thread("hello");
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let taken = hub.take_over(&thread.id, None).await.unwrap();
    assert!(!taken.fresh);
    assert!(taken.command.starts_with("mock-cli --resume "), "{}", taken.command);
}

#[tokio::test]
async fn a_turn_cut_off_by_an_update_carries_on_in_the_next_core() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let old = hub(Store::open(&path).unwrap());
    no_proof(&old).await;
    let mut rx = old.subscribe();
    let thread = old.create_thread(new_thread("slow one two three four five six")).await.unwrap();
    next_event(&mut rx, EventKind::TurnStart).await;
    next_event(&mut rx, EventKind::Update).await;
    // Sent while the turn works: it waits in Hub's queue, not in the engine.
    old.send_once(&thread.id, "queued".into(), vec![], None, Some("tap-1".into()), false).await.unwrap();

    old.park("update").await;
    assert!(old.engines().is_empty());
    no_processes_left(&thread.id).await;
    // Nothing logged an end: the turn was cut off, not finished.
    let events = old.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(!events.iter().any(|e| matches!(e.kind, EventKind::TurnEnd | EventKind::Error)));
    // A parked core starts no engine; what is sent meanwhile waits in the log.
    assert!(old.send(&thread.id, "while parked".into(), vec![], None).await.is_err());

    let new = hub(Store::open(&path).unwrap());
    no_proof(&new).await;
    let mut rx = new.subscribe();
    assert_eq!(hub_core::restart::resume(&new).await.unwrap(), 1);
    turn_ends(&mut rx, 3).await;

    let events = new.store.events(&thread.id, 0, 1000).await.unwrap();
    let restart = events.iter().find(|e| e.kind == EventKind::Restart).expect("restart logged");
    assert_eq!(restart.data["state"], "continued");
    assert_eq!(restart.data["by"], "update");
    assert_eq!(restart.data["prompt"], events[0].seq);
    assert_eq!(restart.data["queued"].as_array().unwrap().len(), 2);
    let text = reply_text(&events);
    let carried = text.find("Echo: Hub restarted").expect("asked to carry on");
    let queued = text.find("Echo: queued").expect("queued message sent");
    let parked = text.find("Echo: while parked").expect("message from the parked core sent");
    assert!(carried < queued && queued < parked, "{text}");
    // The continued session was resumed, not started over.
    assert!(events.iter().rfind(|e| e.kind == EventKind::SessionStarted).unwrap().data["resumed"] == true);

    // The app retrying the queued message after the restart does nothing.
    new.send_once(&thread.id, "queued".into(), vec![], None, Some("tap-1".into()), false).await.unwrap();
    // And starting again finds nothing left to do.
    assert_eq!(hub_core::restart::resume(&new).await.unwrap(), 0);
    let again = new.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(again.iter().filter(|e| e.kind == EventKind::UserMessage).count(), 4);
    assert_eq!(again.iter().filter(|e| e.kind == EventKind::Restart).count(), 1);
}
