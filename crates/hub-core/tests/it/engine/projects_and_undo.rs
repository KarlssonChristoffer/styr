//! Settings, attachments, projects, rollback, second opinion and plan limits.

use super::turns::Warnings;
use super::*;

/// Threads started together each get their own log (the first reading of problem #13).
#[tokio::test]
async fn threads_started_at_once_all_answer() {
    let warnings = Arc::new(std::sync::Mutex::new(Vec::new()));
    let _logging = tracing::subscriber::set_default(Warnings(warnings.clone()));
    let hub = hub(Store::in_memory().unwrap());
    let (a, b, c) = tokio::join!(
        hub.create_thread(new_thread("one")),
        hub.create_thread(new_thread("two")),
        hub.create_thread(new_thread("three")),
    );
    for thread in [a.unwrap(), b.unwrap(), c.unwrap()] {
        wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
        let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
        assert!(events.iter().any(|e| e.kind == EventKind::TurnEnd));
        assert!(!events.iter().any(|e| e.kind == EventKind::Error), "{events:#?}");
    }
    let warnings = warnings.lock().unwrap();
    assert!(warnings.is_empty(), "{warnings:#?}");
}

#[tokio::test]
async fn settings_apply_now_and_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let id = {
        let hub = hub(Store::open(&path).unwrap());
        let mut rx = hub.subscribe();
        let mut new = new_thread("settings");
        new.settings.insert("model".into(), "opus".into());
        let thread = hub.create_thread(new).await.unwrap();
        next_event(&mut rx, EventKind::TurnEnd).await;

        // While the engine runs, a change is applied right away and reported.
        hub.set_setting(&thread.id, "effort".into(), "high".into()).await.unwrap();
        let state = next_event(&mut rx, EventKind::SessionState).await;
        let effort = state.data["config_options"].as_array().unwrap().iter().find(|o| o["id"] == "effort").unwrap();
        assert_eq!(effort["currentValue"], "high");
        // A setting the engine refuses is ignored rather than breaking the thread.
        hub.set_setting(&thread.id, "colour".into(), "red".into()).await.unwrap();
        hub.send(&thread.id, "settings".into(), vec![], None).await.unwrap();
        next_event(&mut rx, EventKind::TurnEnd).await;
        thread.id
    };
    let hub = hub(Store::open(&path).unwrap());
    let mut rx = hub.subscribe();
    hub.send(&id, "settings".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let text = reply_text(&hub.store.events(&id, 0, 1000).await.unwrap());
    assert_eq!(
        text,
        "Echo: model=opus effort=medium mode=default\
         Echo: model=opus effort=high mode=default\
         Echo: model=opus effort=high mode=default"
    );
}

#[tokio::test]
async fn attachments_reach_the_engine() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("look")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let attachments = vec![
        Attachment { name: "shot.png".into(), mime: "image/png".into(), data: Some("iVBORw0KGgo=".into()), text: None, path: None, url: None },
        Attachment { name: "notes.txt".into(), mime: "text/plain".into(), data: None, text: Some("hi".into()), path: None, url: None },
        Attachment { name: "main.rs".into(), mime: "text/x-rust".into(), data: None, text: None, path: Some(r"F:\Dev\hub\main.rs".into()), url: None },
        Attachment { name: "Docs".into(), mime: "text/uri-list".into(), data: None, text: None, path: None, url: Some("https://example.com/docs".into()) },
    ];
    hub.send(&thread.id, "see".into(), attachments, None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let text = reply_text(&events);
    assert!(
        text.ends_with("Echo: see [image image/png] [file file:///notes.txt] [file file:///F:/Dev/hub/main.rs] [file https://example.com/docs]"),
        "{text}"
    );
    let sent = events.iter().rfind(|e| e.kind == EventKind::UserMessage).unwrap();
    assert_eq!(sent.data["attachments"][0]["name"], "shot.png");
    assert!(hub.send(&thread.id, " ".into(), vec![], None).await.is_err());
}

#[tokio::test]
async fn a_new_thread_off_the_stage_now_gets_a_focus_note() {
    let hub = hub(Store::in_memory().unwrap());
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("docs")).unwrap();
    std::fs::write(dir.path().join("docs/FOCUS.md"), "## Stage 1: Proof of concept (active)\nDelivered when:\n- [ ] Real threads\n").unwrap();
    let cwd = dir.path().display().to_string();
    let start = |message: &str| NewThread { cwd: cwd.clone(), message: None, ..new_thread(message) };
    let focus = |id: String| {
        let hub = hub.clone();
        async move { hub.store.events(&id, 0, 1000).await.unwrap().into_iter().filter(|e| e.kind == EventKind::Focus).collect::<Vec<_>>() }
    };

    let off = hub.create_thread(start("")).await.unwrap();
    hub.clone().focus_check(off.id.clone(), cwd.clone(), "an offstage idea: add a 3D world".into()).await;
    let notes = focus(off.id.clone()).await;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].data["stage"], "Proof of concept");
    assert_eq!(notes[0].data["fits"], false);

    let on = hub.create_thread(start("")).await.unwrap();
    hub.clone().focus_check(on.id.clone(), cwd.clone(), "make real threads run on the PC".into()).await;
    assert!(focus(on.id.clone()).await.is_empty(), "fits, so nothing is logged");
    hub.clone().focus_check(on.id.clone(), std::env::temp_dir().display().to_string(), "an offstage idea: add a 3D world".into()).await;
    assert!(focus(on.id).await.is_empty(), "no focus file, no check");
}

#[tokio::test]
async fn tidies_with_a_small_model_and_keeps_the_original() {
    let hub = hub(Store::in_memory().unwrap());
    // The mock agent answers "settings" with the session's model, so this shows haiku was picked.
    let tidy = hub.tidy("uh so settings, the login thing, you know", None).await.unwrap();
    assert!(tidy.contains("model=haiku"), "{tidy}");
    assert!(hub.tidy("  ", None).await.is_err());

    let thread = hub.create_thread(NewThread { message: None, ..new_thread("") }).await.unwrap();
    let mut rx = hub.subscribe();
    hub.send(&thread.id, "Task: fix login".into(), vec![], Some("uh fix the login".into()))
        .await
        .unwrap();
    let sent = next_event(&mut rx, EventKind::UserMessage).await;
    assert_eq!(sent.data["text"], "Task: fix login");
    assert_eq!(sent.data["original"], "uh fix the login");
}

#[tokio::test]
async fn saves_rules_to_claude_md_once_under_their_own_heading() {
    let hub = hub(Store::in_memory().unwrap());
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "# Project\n\nSome notes.\n\n## Later\n\nMore.\n").unwrap();
    let thread = hub
        .create_thread(NewThread { cwd: dir.path().display().to_string(), message: None, ..new_thread("") })
        .await
        .unwrap();
    hub.save_rule(&thread.id, "Always run  the tests", 1).await.unwrap();
    hub.save_rule(&thread.id, "Never push to main", 2).await.unwrap();
    hub.save_rule(&thread.id, "Always run the tests", 3).await.unwrap();
    let body = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert_eq!(
        body,
        "# Project\n\nSome notes.\n\n## Later\n\nMore.\n\n## Rules from Hub\n\n- Always run the tests\n- Never push to main\n"
    );
    let saved = hub.store.events(&thread.id, 0, 100).await.unwrap();
    assert_eq!(saved.iter().filter(|e| e.kind == EventKind::RuleSaved).count(), 3);
}

#[tokio::test]
async fn an_engine_s_traits_come_from_its_config_and_not_its_name() {
    let mut config = test_config();
    config.engines.clear();
    let traits = hub_core::config::EngineTraits { rules_file: Some("AGENTS.md".into()), ..Default::default() };
    let command = mock_agent().display().to_string();
    let engine = hub_core::config::EngineConfig { command, driver: Default::default(), secrets: Vec::new(), terminal: None, traits, ..Default::default() };
    config.engines.insert("plain".into(), engine);
    config.default_engine = "plain".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let dir = tempfile::tempdir().unwrap();
    let thread = hub
        .create_thread(NewThread { cwd: dir.path().display().to_string(), engine: Some("plain".into()), ..new_thread("hello") })
        .await
        .unwrap();
    // Rules go to the file the engine reads.
    hub.save_rule(&thread.id, "Never push to main", 1).await.unwrap();
    assert!(std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap().contains("- Never push to main"));
    assert!(!dir.path().join("CLAUDE.md").exists());
    // An engine without a usage probe is never asked for plan limits, after a turn or when the
    // limits are asked for.
    for _ in 0..400 {
        let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
        if events.iter().any(|e| e.kind == EventKind::TurnEnd) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let report = hub.usage_report(true).await.unwrap();
    assert!(!report.refreshing && report.plan.is_none());
    assert_eq!(report.plan_error.as_deref(), Some("The default engine does not report plan limits"));
}

#[tokio::test]
async fn writes_a_handoff_from_the_log() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("fix the login bug")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    // The mock echoes the prompt, so the handoff shows what it was built from.
    let text = hub.handoff(&thread.id).await.unwrap();
    assert!(text.contains("User: fix the login bug"), "{text}");
    assert!(text.contains("Agent: Echo: fix the login bug"), "{text}");
}

#[tokio::test]
async fn usage_reports_plan_limits_and_thread_context() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = hub(Store::in_memory().unwrap()).config.clone();
    config.data_dir = dir.path().to_path_buf();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("hello")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;

    // Usually still refreshing here, but on a busy machine the turn above can already have
    // brought fresh limits, so only the outcome is asserted.
    let mut report = hub.usage_report(true).await.unwrap();
    for _ in 0..400 {
        report = hub.usage_report(false).await.unwrap();
        if !report.refreshing {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let plan = report.plan.expect("plan limits");
    assert_eq!(plan.limits[0].label, "5-hour limit");
    assert_eq!(plan.limits[1].percent, 10.0);
    let t = report.threads.iter().find(|t| t.thread_id == thread.id).unwrap();
    assert_eq!((t.used, t.size), (12_000, 200_000));
    // Fresh limits are not fetched again.
    assert!(!hub.usage_report(true).await.unwrap().refreshing);
}

#[tokio::test]
async fn a_project_from_a_template_has_working_files_and_a_setup_thread_only_when_described() {
    let hub = hub(Store::in_memory().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let new = |folder: &str, about: Option<&str>, ci: bool| NewProject {
        path: dir.path().join(folder).display().to_string(),
        create: Some(true),
        name: Some("Tiny Api".into()),
        github: None,
        setup: Some(true),
        about: about.map(str::to_owned),
        template: Some("fastapi".into()),
        public: None,
        ci: Some(ci),
    };

    // Nothing said about it: the starter is all there is to set up, so no thread.
    let quiet = hub.create_project(new("quiet", None, false)).await.unwrap();
    assert!(quiet.thread_id.is_none(), "{:?}", quiet.notes);
    let root = dir.path().join("quiet");
    for file in ["app/main.py", "tests/test_health.py", "pyproject.toml", "README.md", "docs/plan.md", "CLAUDE.md", "AGENTS.md"] {
        assert!(root.join(file).is_file(), "{file}");
    }
    let claude = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap();
    assert!(claude.contains("`uv run pytest`") && claude.contains("## Rules from Hub"), "{claude}");
    assert_eq!(claude, std::fs::read_to_string(root.join("AGENTS.md")).unwrap());
    assert!(!root.join(".github").exists(), "no workflow unless asked for");

    // A description gets a thread that is told not to start over.
    let described = hub.create_project(new("described", Some("an API for my recipes"), false)).await.unwrap();
    let thread = hub.store.thread(described.thread_id.as_deref().unwrap()).await.unwrap().unwrap();
    assert_eq!(thread.title, "Set up Tiny Api");
    let events = hub.store.events(&thread.id, 0, 50).await.unwrap();
    let first = events.iter().find(|e| e.kind == EventKind::UserMessage).map(|e| e.data["text"].as_str().unwrap_or_default().to_owned()).unwrap_or_default();
    assert!(first.contains("Do not scaffold again") && first.contains("an API for my recipes"), "{first}");

    // CI belongs to the GitHub repository: without one nothing is written, and the project is still made.
    hub.create_project(new("ci", None, true)).await.unwrap();
    assert!(dir.path().join("ci/app/main.py").is_file());
    assert!(!dir.path().join("ci/.github").exists());
}

#[tokio::test]
async fn projects_are_added_created_and_removed() {
    let hub = hub(Store::in_memory().unwrap());
    let dir = tempfile::tempdir().unwrap();

    let existing = dir.path().join("atlas");
    std::fs::create_dir_all(&existing).unwrap();
    let added = hub.create_project(NewProject { path: existing.display().to_string(), create: None, name: None, github: None, setup: None, about: None, template: None, public: None, ci: None }).await.unwrap().project;
    assert_eq!(added.name, "atlas");
    assert!(!existing.join("docs/plan.md").exists(), "adding an existing folder changes nothing in it");

    let fresh = dir.path().join("game");
    let created = hub
        .create_project(NewProject { path: format!("{}/", fresh.display()), create: Some(true), name: Some("Untitled game".into()), github: None, setup: Some(true), about: Some("A small platformer".into()), template: None, public: None, ci: None })
        .await
        .unwrap();
    assert!(created.notes.is_empty(), "{:?}", created.notes);
    let setup = hub.store.thread(created.thread_id.as_deref().unwrap()).await.unwrap().unwrap();
    assert_eq!(setup.title, "Set up Untitled game");
    assert_eq!(setup.cwd, fresh.display().to_string());
    let created = created.project;
    assert_eq!(created.name, "Untitled game");
    assert_eq!(created.path, fresh.display().to_string());
    assert!(fresh.join("docs/plan.md").is_file());
    assert!(std::fs::read_to_string(fresh.join("CLAUDE.md")).unwrap().contains("docs/plan.md"));
    assert!(hub.create_project(NewProject { path: fresh.display().to_string(), create: Some(true), name: None, github: None, setup: None, about: None, template: None, public: None, ci: None }).await.is_err(), "a folder with files is not created over");
    assert!(hub.create_project(NewProject { path: dir.path().join("nope").display().to_string(), create: None, name: None, github: None, setup: None, about: None, template: None, public: None, ci: None }).await.is_err());

    // Adding the same folder again keeps the first entry.
    hub.create_project(NewProject { path: existing.display().to_string(), create: None, name: None, github: None, setup: None, about: None, template: None, public: None, ci: None }).await.unwrap();
    assert_eq!(hub.store.projects().await.unwrap().len(), 2);
    hub.remove_project(&added.path).await.unwrap();
    assert_eq!(hub.store.projects().await.unwrap().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["Untitled game"]);
    assert!(existing.is_dir(), "removing a project keeps its folder");
}

#[tokio::test]
async fn a_retried_new_thread_is_created_once() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let request = NewThread { client_id: Some("tap-1".into()), ..new_thread("hello") };
    let first = hub.create_thread(request.clone()).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let again = hub.create_thread(request).await.unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(hub.store.threads().await.unwrap().len(), 1);
    let messages = hub.store.events(&first.id, 0, 1000).await.unwrap();
    assert_eq!(messages.iter().filter(|e| e.kind == EventKind::UserMessage).count(), 1);
}

#[tokio::test]
async fn everything_a_thread_did_can_be_rolled_back_and_back_again() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("readme.md"), "hi\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"]);

    let hub = hub(Store::in_memory().unwrap());
    no_proof(&hub).await;
    let mut rx = hub.subscribe();
    let mut new = new_thread("write please");
    new.cwd = repo.display().to_string();
    let thread = hub.create_thread(new).await.unwrap();
    let checkpoint = next_event(&mut rx, EventKind::Checkpoint).await;
    assert_eq!(checkpoint.data["seq"], 1);
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    assert!(repo.join("mock.txt").exists());

    let to = checkpoint.data["commit"].as_str().unwrap();
    let preview = hub.undo_preview(&thread.id, to).await.unwrap();
    assert_eq!(preview.files.len(), 1);
    assert_eq!((preview.files[0].path.as_str(), preview.files[0].action.as_str()), ("mock.txt", "remove"));

    hub.undo(&thread.id, to).await.unwrap();
    assert!(!repo.join("mock.txt").exists());
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let restored = events.iter().rfind(|e| e.kind == EventKind::Restored).unwrap();
    assert_eq!(restored.data["seq"], 1);

    // Undoing the rollback brings the thread's work back.
    let safety = restored.data["safety"].as_str().unwrap();
    hub.undo(&thread.id, safety).await.unwrap();
    assert!(repo.join("mock.txt").exists());
    assert!(hub.undo(&thread.id, "0000000").await.is_err(), "only this thread's snapshots");
}

#[tokio::test]
async fn another_model_reviews_what_the_thread_changed() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("readme.md"), "hi\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"]);

    let hub = hub(Store::in_memory().unwrap());
    no_proof(&hub).await;
    let mut rx = hub.subscribe();
    let mut new = new_thread("hello");
    new.cwd = repo.display().to_string();
    let thread = hub.create_thread(new).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    assert!(hub.review(&thread.id, Default::default()).await.is_err(), "nothing changed yet");

    hub.send(&thread.id, "write it".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let ask = hub_core::model::ReviewRequest { model: Some("haiku".into()), ..Default::default() };
    let started = hub.review(&thread.id, ask).await.unwrap();
    assert_eq!(started.model, "haiku");
    let running = next_event(&mut rx, EventKind::Review).await;
    assert_eq!(running.data["state"], "running");
    assert_eq!(running.data["files"][0], "mock.txt");
    let done = finished(&mut rx).await;
    assert_eq!(done.data["id"], started.id.as_str());
    assert_eq!(done.data["state"], "done", "{}", done.data);
    // The mock echoes its prompt: the diff and the request reached the reviewer.
    let text = done.data["text"].as_str().unwrap();
    assert!(text.contains("+write it") && text.contains("<request>"), "{text}");

    // Its findings go back to the agent, marked as the review's.
    assert_eq!(done.data["sent"], true);
    let sent = next_event(&mut rx, EventKind::UserMessage).await;
    assert_eq!(sent.data["review"], started.id.as_str());
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // The next review starts where this one ended; the agent can ask for one itself.
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    std::fs::write(repo.join("b.txt"), "b\n").unwrap();
    let call = serde_json::json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call",
        "params": { "name": "second_opinion", "arguments": { "reason": "touches auth" } } });
    let answer = hub_core::mcp::handle(&hub, &thread.id, call).await.unwrap();
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    let running = next_event(&mut rx, EventKind::Review).await;
    assert_eq!((running.data["by"].as_str(), running.data["reason"].as_str()), (Some("agent"), Some("touches auth")));
    let mut files: Vec<&str> = running.data["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
    files.sort();
    assert_eq!(files, ["a.txt", "b.txt", "mock.txt"], "mock.txt changed again when the agent echoed the review");
    let e = finished(&mut rx).await;
    assert_eq!(e.data["state"], "done", "{}", e.data);
    // Three files are cut into parts; every file is in one.
    let parts = e.data["chunks"].as_array().unwrap();
    let covered: usize = parts.iter().map(|c| c["files"].as_array().unwrap().len()).sum();
    assert_eq!(covered, 3);
}

/// The next review event that is no longer running.
async fn finished(rx: &mut broadcast::Receiver<StreamMsg>) -> Event {
    loop {
        let e = next_event(rx, EventKind::Review).await;
        if e.data["state"] != "running" {
            return e;
        }
    }
}

#[tokio::test]
async fn the_quota_guard_moves_threads_to_a_cheaper_model_until_you_pick_one() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let mut prefs = hub.guard_prefs().await.unwrap();
    assert_eq!(prefs.switch_at, None, "off until asked for");
    prefs.switch_at = Some(10.0);
    prefs.fallback_model = "haiku".into();
    hub.set_guard_prefs(prefs).await.unwrap();
    // The mock reports the weekly limit at 10%.
    hub.usage_report(true).await.unwrap();
    for _ in 0..200 {
        if hub.usage.plan().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let thread = hub.create_thread(new_thread("settings")).await.unwrap();
    let switched = next_event(&mut rx, EventKind::QuotaSwitch).await;
    assert_eq!(switched.data["to"], "haiku");
    assert_eq!(switched.data["limit"], "Weekly · all models");
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(reply_text(&events).contains("model=haiku"), "{}", reply_text(&events));

    // A model the user picks afterwards stays.
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.set_setting(&thread.id, "model".into(), "opus".into()).await.unwrap();
    hub.send(&thread.id, "settings".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::QuotaSwitch).count(), 1);
    assert!(reply_text(&events).ends_with("model=opus effort=medium mode=default"), "{}", reply_text(&events));
}

/// A new session can report a model it does not run (claude-agent-acp reads `model` from
/// settings.json while a chat's CLI was told to skip that file): the thread's pick is sent anyway.
#[tokio::test]
async fn a_picked_model_is_sent_even_when_the_new_session_says_it_has_it() {
    let mut config = test_config();
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig {
            command: mock_agent().display().to_string(),
            env: [("MOCK_SAYS_MODEL".to_string(), "opus".to_string())].into(),
            traits: hub_core::config::EngineTraits::preset("claude"),
            ..Default::default()
        },
    );
    config.default_engine = "mock".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let mut rx = hub.subscribe();
    let mut new = new_thread("settings");
    new.settings.insert("model".into(), "opus".into());
    let thread = hub.create_thread(new).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(reply_text(&events).contains("model=opus"), "{}", reply_text(&events));
}

#[tokio::test]
async fn a_thread_stopped_by_a_plan_limit_waits_for_it_and_carries_on() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let started = hub_core::model::now_ms();
    let thread = hub.create_thread(new_thread("quota")).await.unwrap();
    let error = next_event(&mut rx, EventKind::Error).await;
    assert!(error.data["message"].as_str().unwrap().contains("You've hit your limit"));
    // The mock's /usage puts the weekly limit highest, resetting Nov 1 at 4 PM.
    let wait = next_event(&mut rx, EventKind::QuotaWait).await;
    assert_eq!(wait.data["state"], "waiting");
    assert_eq!(wait.data["known"], true);
    assert_eq!(wait.data["limit"], "Weekly · all models");
    assert_eq!(wait.data["resets"], "Nov 1, 4:00 PM GMT+1");
    assert!(wait.data["at"].as_i64().unwrap() > started);
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // Carrying on sends the agent a message marked with the wait.
    hub.carry_on(&thread.id, "you").await.unwrap();
    let resumed = next_event(&mut rx, EventKind::QuotaWait).await;
    assert_eq!(resumed.data["state"], "resumed");
    assert_eq!(resumed.data["wait"], wait.seq);
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let said = events.iter().rfind(|e| e.kind == EventKind::UserMessage).unwrap();
    assert_eq!(said.data["quota"], wait.seq);
    assert!(reply_text(&events).contains("plan limit that stopped you has reset"), "{}", reply_text(&events));
    assert!(hub.carry_on(&thread.id, "you").await.is_err(), "no longer waiting");

    // What you write while it waits takes the place of carrying on.
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.send(&thread.id, "quota again".into(), vec![], None).await.unwrap();
    assert_eq!(next_event(&mut rx, EventKind::QuotaWait).await.data["state"], "waiting");
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub.send(&thread.id, "hello there".into(), vec![], None).await.unwrap();
    let cancelled = next_event(&mut rx, EventKind::QuotaWait).await;
    assert_eq!(cancelled.data["state"], "cancelled");
    assert_eq!(cancelled.data["by"], "message");

    // Switched off in the guard, a limit is only an error.
    let prefs = hub_core::usage::GuardPrefs { resume: false, ..hub.guard_prefs().await.unwrap() };
    hub.set_guard_prefs(prefs).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    hub.send(&thread.id, "quota once more".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::Error).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::QuotaWait && e.data["state"] == "waiting").count(), 2);
}

fn two_engines() -> Arc<Hub> {
    let mut config = test_config();
    config.engines.clear();
    for name in ["mock", "other"] {
        let command = mock_agent().display().to_string();
        config.engines.insert(name.into(), hub_core::config::EngineConfig { command, driver: Default::default(), secrets: Vec::new(), terminal: None, traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() });
    }
    // Pays per use: no plan limits, so no phrases for one.
    let command = mock_agent().display().to_string();
    config.engines.insert("paid".into(), hub_core::config::EngineConfig { command, driver: Default::default(), secrets: vec!["KEY".into()], terminal: None, traits: Default::default(), ..Default::default() });
    config.default_engine = "mock".into();
    Hub::new(Store::in_memory().unwrap(), config)
}

#[tokio::test]
async fn a_thread_stopped_by_a_plan_limit_goes_on_with_another_subscription_never_a_paid_one() {
    let hub = two_engines();
    let mut rx = hub.subscribe();
    // By default Hub moves it by itself, to the other subscription.
    let thread = hub.create_thread(new_thread("quota")).await.unwrap();
    let wait = next_event(&mut rx, EventKind::QuotaWait).await;
    assert_eq!((wait.data["state"].as_str(), wait.data["offer"].as_str()), (Some("waiting"), Some("other")), "{}", wait.data);
    let moved = next_event(&mut rx, EventKind::QuotaWait).await;
    assert_eq!((moved.data["state"].as_str(), moved.data["to"].as_str(), moved.data["by"].as_str()), (Some("switched"), Some("other"), Some("hub")), "{}", moved.data);
    assert_eq!(moved.data["wait"], wait.seq);
    next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().engine, "other");
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(events.iter().any(|e| e.kind == EventKind::EngineSwitch && e.data["to"] == "other"));
    let said = events.iter().rfind(|e| e.kind == EventKind::UserMessage).unwrap();
    assert_eq!(said.data["quota"], wait.seq);
    assert!(said.data.get("handoff").is_some(), "the new engine is told the conversation: {}", said.data);
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    // Both subscriptions out: it waits for the reset. The paid engine is never the way out.
    hub.send(&thread.id, "quota again".into(), vec![], None).await.unwrap();
    let wait = next_event(&mut rx, EventKind::QuotaWait).await;
    assert_eq!(wait.data["state"], "waiting");
    assert!(wait.data.get("offer").is_none(), "{}", wait.data);
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    assert!(hub.continue_elsewhere(&thread.id, None, "you").await.is_err(), "no other subscription is free");
    let refused = hub.continue_elsewhere(&thread.id, Some("paid"), "you").await.unwrap_err();
    assert!(format!("{refused:#}").contains("bills per use"), "{refused:#}");
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().engine, "other");
}

#[tokio::test]
async fn moving_on_after_a_plan_limit_can_be_asked_first_or_switched_off() {
    let hub = two_engines();
    let mut rx = hub.subscribe();
    let prefs = hub_core::usage::GuardPrefs { limit_switch: "ask".into(), ..hub.guard_prefs().await.unwrap() };
    hub.set_guard_prefs(prefs).await.unwrap();
    let thread = hub.create_thread(new_thread("quota")).await.unwrap();
    let wait = next_event(&mut rx, EventKind::QuotaWait).await;
    assert_eq!((wait.data["state"].as_str(), wait.data["offer"].as_str()), (Some("waiting"), Some("other")), "{}", wait.data);
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    // Nothing moved until the click.
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().engine, "mock");
    hub.continue_elsewhere(&thread.id, None, "you").await.unwrap();
    let moved = next_event(&mut rx, EventKind::QuotaWait).await;
    assert_eq!((moved.data["state"].as_str(), moved.data["by"].as_str()), (Some("switched"), Some("you")), "{}", moved.data);
    next_event(&mut rx, EventKind::TurnEnd).await;

    let prefs = hub_core::usage::GuardPrefs { limit_switch: "off".into(), ..hub.guard_prefs().await.unwrap() };
    hub.set_guard_prefs(prefs).await.unwrap();
    let second = hub.create_thread(new_thread("quota again")).await.unwrap();
    let wait = next_event(&mut rx, EventKind::QuotaWait).await;
    assert!(wait.data.get("offer").is_none(), "{}", wait.data);
    wait_status(&hub, &second.id, ThreadStatus::Idle).await;
    assert_eq!(hub.store.thread(&second.id).await.unwrap().unwrap().engine, "mock");
}

#[tokio::test]
async fn a_side_job_can_be_given_an_engine_and_an_unknown_one_is_refused() {
    let hub = two_engines();
    let mut rx = hub.subscribe();
    let mut parent = new_thread("");
    parent.message = None;
    let parent = hub.create_thread(parent).await.unwrap();
    let start = |args: serde_json::Value| serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "start_side_job", "arguments": args } });

    let answer = hub_core::mcp::handle(&hub, &parent.id, start(serde_json::json!({ "title": "Look it up", "task": "find the date", "kind": "research", "engine": "other" }))).await.unwrap();
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    let started = next_event(&mut rx, EventKind::Child).await;
    assert_eq!((started.data["state"].as_str(), started.data["engine"].as_str()), (Some("working"), Some("other")));

    let answer = hub_core::mcp::handle(&hub, &parent.id, start(serde_json::json!({ "title": "Look it up", "task": "find the date", "kind": "research", "engine": "nowhere" }))).await.unwrap();
    assert_eq!(answer["result"]["isError"], true, "{answer}");
    assert!(answer.to_string().contains("no engine"), "{answer}");
}
