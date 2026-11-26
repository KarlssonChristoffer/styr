//! Worktrees, recipes, harness keys, widgets, problems, warming and unattended picks.

use super::*;

#[tokio::test]
async fn threads_on_hub_in_dev_mode_each_work_in_a_worktree_of_their_own() {
    let dir = repo();
    let hub = hub(Store::in_memory().unwrap());
    no_proof(&hub).await;
    // The folder is the dev clone.
    hub.store.set_value("dev.dir", dir.path().to_str().unwrap()).await.unwrap();
    let mut rx = hub.subscribe();
    let mut new = new_thread("write it");
    new.cwd = dir.path().display().to_string();
    let thread = hub.create_thread(new.clone()).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    let tree = std::path::PathBuf::from(&thread.cwd);
    assert!(tree.join("mock.txt").exists(), "the thread worked in its worktree");
    assert!(!dir.path().join("mock.txt").exists(), "and left the clone alone");
    assert!(thread.cwd.contains(".hub-threads"));
    assert!(git(&tree, &["branch", "--show-current"]).starts_with("hub-dev/write-it-"));

    let status = hub.dev_status().await.unwrap();
    assert_eq!(status.threads.len(), 1);
    assert_eq!((status.threads[0].thread_id.as_str(), status.threads[0].changed), (thread.id.as_str(), 1));
    assert!(status.worktrees && status.shown.is_none());

    // The dev window shows it: the folder's state is the worktree's.
    let status = hub.dev_show(hub_core::model::DevShow { thread: Some(thread.id.clone()) }).await.unwrap();
    assert_eq!(status.shown.as_deref(), Some(thread.id.as_str()));
    assert_eq!(status.changed, 1);

    // Removing it keeps the work, and the dev window goes back to the clone.
    let status = hub.dev_remove_worktree(&thread.id).await.unwrap();
    assert!(status.threads.is_empty() && status.shown.is_none());
    assert!(!tree.exists());
    assert!(git(dir.path(), &["cat-file", "-e", &format!("refs/hub/dev-threads/{}:mock.txt", hub_core::attempts::branch_stem(&thread.id))]).is_empty());

    // Turned off, threads work in the clone itself.
    hub.dev_set_worktrees(false).await.unwrap();
    new.message = None;
    let plain = hub.create_thread(new).await.unwrap();
    assert_eq!(plain.cwd, dir.path().display().to_string());
}

/// Removes a worktree, trying again for a few seconds while Windows says "Permission denied": a
/// file the agent just wrote is held for a moment by the virus scanner or the indexer, and the
/// folder cannot go until they let go. The engine itself is already gone by now. git forgets the
/// worktree before it fails on such a file, so the next try finds "not a working tree": then the
/// folder is removed here and git's list is pruned.
fn remove_worktree(repo: &std::path::Path, tree: &str) {
    for _ in 0..60 {
        let out = std::process::Command::new("git").args(["worktree", "remove", "--force", tree]).current_dir(repo).output().unwrap();
        if out.status.success() {
            return;
        }
        let why = String::from_utf8_lossy(&out.stderr).into_owned();
        if why.contains("not a working tree") {
            let _ = std::fs::remove_dir_all(tree);
            git(repo, &["worktree", "prune"]);
            if !std::path::Path::new(tree).exists() {
                return;
            }
        } else {
            assert!(why.contains("Permission denied") || why.contains("Access is denied"), "git worktree remove: {why}");
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    panic!("the worktree {tree} could not be removed");
}

#[tokio::test]
async fn a_project_with_worktrees_on_gives_each_new_thread_a_worktree_of_its_own() {
    use hub_core::model::ProjectTreesSet;
    let dir = repo();
    let hub = hub(Store::in_memory().unwrap());
    no_proof(&hub).await;
    let path = dir.path().display().to_string();

    // Off by default: a thread works in the project's folder.
    assert!(!hub.project_trees(&path).await.unwrap().on);
    let mut new = new_thread("plain");
    new.cwd = path.clone();
    let plain = hub.create_thread(new.clone()).await.unwrap();
    assert_eq!(plain.cwd, path);

    // On: each new thread gets a folder and a branch of its own, and the project's folder is left alone.
    let set = hub.set_project_trees(ProjectTreesSet { path: path.clone(), on: true, base: None }).await.unwrap();
    assert!(set.on && set.root.is_some());
    let mut rx = hub.subscribe();
    new.message = Some("write it".into());
    let thread = hub.create_thread(new.clone()).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let tree = std::path::PathBuf::from(&thread.cwd);
    assert!(thread.cwd.contains(".hub-threads"), "{}", thread.cwd);
    assert!(tree.join("mock.txt").exists() && !dir.path().join("mock.txt").exists());
    assert!(git(&tree, &["branch", "--show-current"]).starts_with("hub/write-it-"));
    assert!(git(dir.path(), &["status", "--porcelain"]).is_empty(), "the project's folder shows no change, and .hub-threads needs no .gitignore");

    // Cleaned up while idle, the worktree comes back on its branch when the thread is written to.
    let branch = git(&tree, &["branch", "--show-current"]).trim().to_owned();
    // Its engine stops first, as the idle timeout does: on Windows a process running in the
    // folder keeps it from being removed ("Permission denied").
    hub.shutdown(&thread.id, true).await.unwrap();
    engine_gone(&hub, &thread.id).await;
    remove_worktree(dir.path(), &thread.cwd);
    hub.store.set_value(&format!("dev.branch.{}", thread.id), &branch).await.unwrap(); // what the clean-up leaves
    assert!(!tree.exists());
    hub.send(&thread.id, "again".into(), vec![], None).await.unwrap();
    for _ in 0..100 {
        let ended = hub.store.events(&thread.id, 0, 1000).await.unwrap().iter().filter(|e| e.kind == EventKind::TurnEnd).count();
        if ended >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(tree.join("readme.md").exists());
    assert_eq!(git(&tree, &["branch", "--show-current"]).trim(), branch);

    // A folder inside the repository maps to the same place in the worktree.
    std::fs::create_dir_all(dir.path().join("sub")).unwrap();
    new.cwd = dir.path().join("sub").display().to_string();
    new.message = None;
    let sub = hub.create_thread(new.clone()).await.unwrap();
    assert!(sub.cwd.contains(".hub-threads") && sub.cwd.ends_with("sub"), "{}", sub.cwd);

    // Off again: back to the folder itself.
    hub.set_project_trees(ProjectTreesSet { path: path.clone(), on: false, base: None }).await.unwrap();
    new.cwd = path.clone();
    assert_eq!(hub.create_thread(new).await.unwrap().cwd, path);
    // A folder that is no repository cannot have it.
    let not = tempfile::tempdir().unwrap();
    assert!(hub.set_project_trees(ProjectTreesSet { path: not.path().display().to_string(), on: true, base: None }).await.is_err());
}

#[tokio::test]
async fn a_recipe_runs_on_the_picked_project_and_agents_only_suggest_them() {
    use hub_core::hub_tools::{Caller, call};
    use hub_core::model::{AgentAccess, RecipeDraft};
    let hub = hub(Store::in_memory().unwrap());
    let folder = std::env::temp_dir().display().to_string();
    let draft = RecipeDraft { name: "Fix tests".into(), about: None, prompt: "fix them".into(), engine: None, folder: None, template: None };
    let recipe = hub_core::recipes::create(&hub.ctx(), draft.clone()).await.unwrap();
    assert!(hub_core::recipes::create(&hub.ctx(), RecipeDraft { name: "fix TESTS ".into(), ..draft }).await.is_err());

    // A recipe for any project needs one picked.
    assert!(hub.start_recipe(&recipe.id, None, Default::default()).await.is_err());
    let how = hub_core::recipes::Start { note: Some("only the api".into()), ..Default::default() };
    let thread = hub.start_recipe(&recipe.id, Some(&folder), how).await.unwrap();
    let events = hub.store.events(&thread, 0, 1000).await.unwrap();
    let first = events.iter().find(|e| e.kind == EventKind::UserMessage).unwrap();
    assert_eq!(first.data["recipe"]["name"], "Fix tests");
    let text = first.data["text"].as_str().unwrap();
    assert!(text.contains("fix them") && text.ends_with("This time, also: only the api"));
    let list = hub_core::recipes::list(&hub.ctx()).await.unwrap();
    assert_eq!(list.recipes[0].runs[0].thread, thread);

    // An agent's recipe waits for the user; it cannot be run until kept.
    let agent = Caller::Agent { name: "terminal".into(), access: AgentAccess::Supervised };
    let args = serde_json::json!({ "name": "Bump version", "prompt": "bump it", "why": "you did it twice" });
    call(&hub, &agent, "suggest_recipe", &args).await.unwrap().unwrap();
    let run = serde_json::json!({ "recipe": "Bump version", "folder": folder });
    assert!(call(&hub, &agent, "run_recipe", &run).await.unwrap().is_err());
    let suggested = hub_core::recipes::list(&hub.ctx()).await.unwrap().recipes.into_iter().find(|r| r.suggested.is_some()).unwrap();
    assert_eq!(suggested.suggested.as_ref().unwrap().by, "terminal");
    hub_core::recipes::keep(&hub.ctx(), &suggested.id).await.unwrap();
    let ran = call(&hub, &agent, "run_recipe", &run).await.unwrap().unwrap();
    let ran: serde_json::Value = serde_json::from_str(&ran).unwrap();
    let id = ran["thread"]["id"].as_str().unwrap();
    let events = hub.store.events(id, 0, 1000).await.unwrap();
    let first = events.iter().find(|e| e.kind == EventKind::UserMessage).unwrap();
    assert_eq!(first.data["recipe"]["by"]["agent"], "terminal");
    // Suggesting a name the user already keeps is refused, so the agent runs that one instead.
    assert!(call(&hub, &agent, "suggest_recipe", &args).await.unwrap().is_err());
}

/// An engine added from a harness and a provider hands the agent its environment and starts on
/// its model, and trying it spends nothing.
#[tokio::test]
async fn an_added_engine_starts_on_its_model_with_its_environment() {
    let engine = hub_core::config::EngineConfig {
        command: mock_agent().display().to_string(),
        model: Some("deepseek:deepseek-reasoner".into()),
        env: [("MOCK_MODELS".to_string(), "deepseek:deepseek-chat=Chat,deepseek:deepseek-reasoner=Reasoner".to_string())].into(),
        ..Default::default()
    };
    let test = hub_core::engine_probe::test("mock-deepseek", &engine, Vec::new()).await;
    assert!(test.ok, "{}", test.message);
    assert!(test.model_found);
    assert_eq!(test.current.as_deref(), Some("deepseek:deepseek-reasoner"));
    assert_eq!(test.models.len(), 2);

    let missing = hub_core::config::EngineConfig { model: Some("kimi".into()), ..engine.clone() };
    let test = hub_core::engine_probe::test("mock-deepseek", &missing, Vec::new()).await;
    assert!(test.ok && !test.model_found, "{}", test.message);

    let gone = hub_core::config::EngineConfig { command: "/nonexistent/hermes acp".into(), ..Default::default() };
    let test = hub_core::engine_probe::test("gone", &gone, Vec::new()).await;
    assert!(!test.ok && test.message.contains("not installed"), "{}", test.message);
}

#[tokio::test]
async fn a_harness_thread_switches_provider_and_keeps_its_session() {
    // The mock stands in for Hermes: Hub hands it the provider and model in its environment.
    let mut config = test_config();
    config.engines.clear();
    let mut env = std::collections::BTreeMap::new();
    env.insert("MOCK_ENV".to_owned(), "HERMES_INFERENCE_PROVIDER,HERMES_MODEL".to_owned());
    config.engines.insert(
        "hermes".into(),
        hub_core::config::EngineConfig {
            command: mock_agent().display().to_string(),
            harness: Some("hermes".into()),
            provider: Some("deepseek".into()),
            model: Some("deepseek:deepseek-v4-pro".into()),
            env,
            ..Default::default()
        },
    );
    config.default_engine = "hermes".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("whereami")).await.unwrap();
    // The model setting is Hub's, grouped by provider, and stands in for the agent's own.
    let state = next_event(&mut rx, EventKind::SessionState).await;
    let models: Vec<_> = state.data["config_options"].as_array().unwrap().iter().filter(|o| o["category"] == "model").collect();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["id"], hub_core::routes::ROUTE);
    assert_eq!(models[0]["currentValue"], "deepseek:deepseek-v4-pro");
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let session = hub.store.thread(&thread.id).await.unwrap().unwrap().engine_session_id;

    // Another model restarts the agent on it, in the same session.
    hub.set_setting(&thread.id, hub_core::routes::ROUTE.into(), "deepseek:deepseek-flash".into()).await.unwrap();
    hub.send(&thread.id, "whereami".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let text = reply_text(&hub.store.events(&thread.id, 0, 1000).await.unwrap());
    assert_eq!(
        text,
        "Echo: HERMES_INFERENCE_PROVIDER=deepseek HERMES_MODEL=deepseek-v4-pro MODEL=deepseek:deepseek-v4-pro\
         Echo: HERMES_INFERENCE_PROVIDER=deepseek HERMES_MODEL=deepseek-flash MODEL=deepseek:deepseek-flash"
    );
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().engine_session_id, session);
    let started = hub.store.events(&thread.id, 0, 1000).await.unwrap().into_iter().filter(|e| e.kind == EventKind::SessionStarted).collect::<Vec<_>>();
    assert_eq!(started.last().unwrap().data["resumed"], true);
}

#[tokio::test]
async fn a_replaced_key_reaches_a_thread_whose_engine_is_already_running() {
    // An engine keeps the environment it started with; a new key must not wait for a restart.
    let mut config = test_config();
    config.engines.clear();
    config.data_dir = hub_core::janitor::test_dir();
    let mut env = std::collections::BTreeMap::new();
    env.insert("MOCK_ENV".to_owned(), "WEATHER_KEY".to_owned());
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig {
            command: mock_agent().display().to_string(),
            secrets: vec!["WEATHER_KEY".into()],
            env,
            ..Default::default()
        },
    );
    config.default_engine = "mock".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    hub.secrets.set("WEATHER_KEY", "old-key-abcdefghij").await.unwrap();
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("whereami")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    assert!(hub.engines().contains(&thread.id));

    hub.secrets.set("WEATHER_KEY", "new-key-abcdefghij").await.unwrap();
    assert_eq!(hub_core::engine_renew::renew_once(&hub).await, vec![thread.id.clone()]);
    // Nothing else changed: the next pass leaves it alone.
    assert!(hub_core::engine_renew::renew_once(&hub).await.is_empty());

    hub.send(&thread.id, "whereami".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    // The engine was started again (on the new key; the log hides both), in the same session.
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let started: Vec<_> = events.iter().filter(|e| e.kind == EventKind::SessionStarted).collect();
    assert_eq!(started.len(), 2);
    assert_eq!(started[1].data["resumed"], true);
}

#[tokio::test]
async fn a_widget_is_built_by_a_thread_and_follows_its_page() {
    let mut config = test_config();
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig { command: mock_agent().display().to_string(), driver: Default::default(), secrets: Vec::new(), terminal: None, traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() },
    );
    config.default_engine = "mock".into();
    config.data_dir = hub_core::janitor::test_dir();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let mut rx = hub.subscribe();

    let made = hub_core::widget_builds::create(&hub, "A countdown to Friday").await.unwrap();
    assert_eq!(made.state, hub_core::model::WidgetState::Building);
    assert_eq!(made.title, "A countdown to Friday");
    let thread = made.thread.clone().unwrap();
    assert!(hub.config.data_dir.join("widgets").join(&made.id).is_dir(), "it gets a folder of its own");

    // The mock agent only echoes, so the turn ends without a page: the widget failed, with why.
    next_event(&mut rx, EventKind::TurnEnd).await;
    let mut failed = None;
    for _ in 0..200 {
        let w = hub_core::widgets::list(&hub.ctx()).await.unwrap().into_iter().find(|w| w.id == made.id).unwrap();
        if w.state == hub_core::model::WidgetState::Failed {
            failed = Some(w);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let failed = failed.expect("the widget never failed");
    assert!(!failed.error.is_empty());

    // Its thread shows a page: that is the card now, with the page's title.
    hub_core::widgets::page_shown(&hub.ctx(), &thread, "p1", "Friday countdown").await.unwrap();
    let w = hub_core::widgets::list(&hub.ctx()).await.unwrap().into_iter().find(|w| w.id == made.id).unwrap();
    assert_eq!(w.state, hub_core::model::WidgetState::Ready);
    assert_eq!(w.page.as_deref(), Some("p1"));
    assert_eq!(w.title, "Friday countdown");

    // A change goes to the same thread and shows building until it ends; with no new page the
    // old one stays.
    let asked = hub_core::widget_builds::ask_change(&hub, &made.id, "bigger numbers").await.unwrap();
    assert_eq!(asked.state, hub_core::model::WidgetState::Building);
    next_event(&mut rx, EventKind::TurnEnd).await;
    let mut back = false;
    for _ in 0..200 {
        let w = hub_core::widgets::list(&hub.ctx()).await.unwrap().into_iter().find(|w| w.id == made.id).unwrap();
        if w.state == hub_core::model::WidgetState::Ready {
            assert_eq!(w.page.as_deref(), Some("p1"));
            back = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(back, "a change with no new page keeps the old one");

    hub_core::widgets::delete(&hub.ctx(), &made.id).await.unwrap();
    assert!(hub_core::widgets::list(&hub.ctx()).await.unwrap().is_empty());
}

/// Waits until `check` finds something in the thread's log.
async fn wait_for_event(hub: &Hub, id: &str, check: impl Fn(&Event) -> bool) -> Event {
    for _ in 0..2400 {
        if let Some(e) = hub.store.events(id, 0, 100_000).await.unwrap().into_iter().find(|e| check(e)) {
            return e;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the event never came");
}

#[tokio::test]
async fn a_failed_tool_call_is_counted_and_kept_as_a_problem() {
    let hub = hub(Store::in_memory().unwrap());
    hub_core::problems::install(&hub);
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("toolfail please")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;

    let mut found = Vec::new();
    for _ in 0..400 {
        let filter = hub_core::problems::ProblemFilter { view: Some(hub_core::problems::View::All), ..Default::default() };
        found = hub_core::problems::list(&hub, filter).await.unwrap().into_iter().filter(|p| p.source == "tool").collect();
        if !found.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(found.len(), 1, "one problem for the failed command");
    let p = &found[0];
    assert_eq!(p.title, "Command · cargo failed: bash: cargo: command not found");
    assert_eq!(p.culprit.as_deref(), Some("mock · Command"));
    assert_eq!(p.level, hub_core::problems::Level::Warning);
    let detail = hub_core::problems::detail(&hub, p.id, 1).await.unwrap().unwrap();
    assert_eq!(detail.hits[0].extra["thread"], thread.id.as_str());
    assert!(detail.hits[0].crumbs.iter().any(|c| c.message.contains("toolfail please")), "what came before goes along");

    let stats = hub_core::problems::tools::stats(&hub, 7).await.unwrap();
    let command = stats.tools.iter().find(|t| t.tool == "Command").expect("the command is counted");
    assert_eq!((command.engine.as_str(), command.calls, command.failed), ("mock", 1, 1));
    assert_eq!(command.problems[0].id, p.id);
    assert_eq!(*command.days.last().unwrap(), 1);
}

#[tokio::test]
async fn a_failure_in_a_thread_gets_a_card_and_fix_now_reports_back() {
    let mut config = test_config();
    config.engines.clear();
    config.engines.insert("mock".into(), hub_core::config::EngineConfig { command: mock_agent().display().to_string(), traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() });
    config.engines.insert("broken".into(), hub_core::config::EngineConfig { command: "/nonexistent/engine".into(), ..Default::default() });
    config.default_engine = "mock".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    hub_core::problems::install(&hub);
    let mut rx = hub.subscribe();
    let mut new = new_thread("hello there");
    new.engine = Some("broken".into());
    let thread = hub.create_thread(new).await.unwrap();
    let error = next_event(&mut rx, EventKind::Error).await;
    let card = wait_for_event(&hub, &thread.id, |e| e.kind == EventKind::Reported).await;
    assert_eq!((card.data["about"].as_i64(), card.data["state"].as_str()), (Some(error.seq), Some("sent")));
    let problem = card.data["problem"].as_i64().unwrap();
    // The cards do not keep a broken engine trying: it stops after a few, as it did before.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let errors = hub.store.events(&thread.id, 0, 10_000).await.unwrap().iter().filter(|e| e.kind == EventKind::Error).count();
    assert!((1..=3).contains(&errors), "{errors} errors");

    let fix = hub_core::problems::fixnow::fix_now(&hub, &thread.id, error.seq).await.unwrap();
    assert_ne!(fix.id, thread.id);
    let again = hub_core::problems::fixnow::fix_now(&hub, &thread.id, error.seq).await.unwrap();
    assert_eq!(again.id, fix.id, "pressing it twice does not start a second fix");
    let log = hub.store.events(&fix.id, 0, 10).await.unwrap();
    assert_eq!(log[0].kind, EventKind::Unattended, "a fix thread runs in the background");
    let first = log.iter().find(|e| e.kind == EventKind::UserMessage).unwrap();
    let brief = first.data["text"].as_str().unwrap();
    assert!(brief.contains(error.data["message"].as_str().unwrap()) && brief.contains(&thread.id), "the fix thread knows what failed where");
    assert_eq!(hub_core::problems::get(&hub, problem).await.unwrap().unwrap().thread.as_deref(), Some(fix.id.as_str()));

    let done = wait_for_event(&hub, &thread.id, |e| e.kind == EventKind::Reported && e.data["state"] != "sent" && e.data["state"] != "fixing").await;
    assert_eq!(done.data["state"], "fixed");
    assert_eq!(done.data["fix"], fix.id.as_str());
    assert_eq!(done.data["about"].as_i64(), Some(error.seq));
    assert!(!done.data["text"].as_str().unwrap().is_empty(), "the fix thread's last words come back");
}

#[tokio::test]
async fn vakten_reviews_the_tools_that_fail_most_once_a_day() {
    let mut config = test_config();
    config.data_dir = hub_core::janitor::test_dir();
    config.engines.clear();
    config.engines.insert("mock".into(), hub_core::config::EngineConfig { command: mock_agent().display().to_string(), traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() });
    config.default_engine = "mock".into();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    hub_core::problems::install(&hub);
    let mut rx = hub.subscribe();
    for n in 0..5 {
        hub.create_thread(new_thread(&format!("toolfail {n}"))).await.unwrap();
        next_event(&mut rx, EventKind::TurnEnd).await;
    }
    for _ in 0..400 {
        let stats = hub_core::problems::tools::stats(&hub, 7).await.unwrap();
        if stats.failed >= 5 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let found = hub_core::problems::tools::review_candidates(&hub).await;
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].rate.tool.as_str(), found[0].rate.failed), ("Command", 5));

    hub_core::problems::vakten::set_prefs(&hub, hub_core::problems::vakten::VaktenPrefs { on: true, ..Default::default() }).await.unwrap();
    hub_core::problems::vakten::look(&hub).await.unwrap();
    let vakten = hub.store.threads().await.unwrap().into_iter().find(|t| t.title == "Vakten").expect("Vakten's thread");
    let asked = hub.store.events(&vakten.id, 0, 100).await.unwrap();
    let text = asked.iter().find(|e| e.kind == EventKind::UserMessage).unwrap().data["text"].as_str().unwrap().to_owned();
    assert!(text.contains("Tool-call review") && text.contains("mock · Command: 5 of 5 calls failed (100%), new this week"), "{text}");

    // Vakten saves what it found; the Tool calls tab reads it.
    let finding: hub_core::problems::tools::ToolFinding = serde_json::from_value(serde_json::json!({
        "engine": "mock", "tool": "Command", "cause": "environment", "action": "Install cargo", "verdict": "waits"
    }))
    .unwrap();
    hub_core::problems::tools::save_review(&hub, "cargo is missing on this machine", vec![finding]).await.unwrap();
    let review = hub_core::problems::tools::review(&hub).await.unwrap().unwrap();
    assert_eq!((review.text.as_str(), review.findings[0].cause.as_str()), ("cargo is missing on this machine", "environment"));
}

#[tokio::test]
async fn a_thread_asked_for_on_auto_starts_on_the_default_engine() {
    // Auto is gone; an app that still remembers it gets the default engine and no model moves.
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let mut new = new_thread("what are the settings?");
    new.engine = Some("auto".into());
    let thread = hub.create_thread(new).await.unwrap();
    assert_eq!(thread.engine, "mock");
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(events.iter().all(|e| e.kind != EventKind::Auto));
    assert!(reply_text(&events).contains("model=default"), "{}", reply_text(&events));
}

fn warm_ask(cwd: &std::path::Path) -> hub_core::model::WarmAsk {
    hub_core::model::WarmAsk { cwd: cwd.display().to_string(), engine: None, settings: Default::default() }
}

#[tokio::test]
async fn an_engine_warmed_while_typing_takes_the_new_thread() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let dir = tempfile::tempdir().unwrap();
    let id = hub.warm_new(warm_ask(dir.path())).await.unwrap().expect("an engine starts");
    // Asking again while it waits gives the same one.
    assert_eq!(hub.warm_new(warm_ask(dir.path())).await.unwrap().as_deref(), Some(id.as_str()));
    // Nothing is logged for a thread that does not exist yet.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(hub.store.thread(&id).await.unwrap().is_none());

    let new = NewThread { cwd: dir.path().display().to_string(), warm: Some(id.clone()), ..new_thread("hello warm") };
    let thread = hub.create_thread(new).await.unwrap();
    assert_eq!(thread.id, id);
    // Working from the moment the message is in, before the engine has taken it.
    assert_eq!(hub.store.thread(&id).await.unwrap().unwrap().status, ThreadStatus::Running);
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &id, ThreadStatus::Idle).await;

    let events = hub.store.events(&id, 0, 1000).await.unwrap();
    assert_eq!(reply_text(&events), "Echo: hello warm");
    assert_eq!(events.iter().filter(|e| e.kind == EventKind::SessionStarted).count(), 1);
    assert!(hub.store.thread(&id).await.unwrap().unwrap().engine_session_id.is_some());
    assert!(hub_core::warm::pending(&id).is_none());
}

#[tokio::test]
async fn a_thread_unlike_its_warm_engine_gets_a_fresh_one() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let warmed = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let id = hub.warm_new(warm_ask(warmed.path())).await.unwrap().unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let new = NewThread { cwd: other.path().display().to_string(), warm: Some(id.clone()), ..new_thread("somewhere else") };
    let thread = hub.create_thread(new).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(reply_text(&events), "Echo: somewhere else");
    let started: Vec<_> = events.iter().filter(|e| e.kind == EventKind::SessionStarted).collect();
    assert_eq!(started.len(), 1, "only the fresh engine says it started");
    assert!(!events.iter().any(|e| e.kind == EventKind::Error));
}

#[tokio::test]
async fn auto_and_unknown_engines_are_not_warmed() {
    let hub = hub(Store::in_memory().unwrap());
    let dir = tempfile::tempdir().unwrap();
    let auto = hub_core::model::WarmAsk { engine: Some("auto".into()), ..warm_ask(dir.path()) };
    assert_eq!(hub.warm_new(auto).await.unwrap(), None);
    let unknown = hub_core::model::WarmAsk { engine: Some("nope".into()), ..warm_ask(dir.path()) };
    assert_eq!(hub.warm_new(unknown).await.unwrap(), None);
    let gone = hub_core::model::WarmAsk { cwd: dir.path().join("missing").display().to_string(), ..warm_ask(dir.path()) };
    assert_eq!(hub.warm_new(gone).await.unwrap(), None);
}

/// A thread that runs without the user (a fix thread from Problems) picks its model and mode by
/// words from what the engine offers, keeps a setting it chose itself, and Hub answers its
/// permission questions instead of the user.
#[tokio::test]
async fn an_unattended_thread_picks_by_words_and_never_asks() {
    let hub = hub(Store::in_memory().unwrap());
    let quiet = |words: &[&str]| hub_core::model::Unattended {
        by: "Problems".into(),
        prefer: [
            ("model".to_owned(), words.iter().map(|w| w.to_string()).collect()),
            ("mode".to_owned(), vec!["auto".into(), "accept".into()]),
        ]
        .into_iter()
        .collect(),
    };
    let mut rx = hub.subscribe();
    // "sonnet" is not offered, so the next word wins; "auto" is not, so "accept" finds acceptEdits.
    let thread = hub.create_thread(NewThread { unattended: Some(quiet(&["sonnet", "HAIKU"])), ..new_thread("settings") }).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events[0].kind, EventKind::Unattended);
    let said = reply_text(&events);
    assert!(said.contains("model=haiku") && said.contains("mode=acceptEdits"), "{said}");
    assert!(events.iter().any(|e| e.kind == EventKind::Setting && e.data["by"] == "prefer"));

    // A setting the thread chose itself stays.
    let mut new = NewThread { unattended: Some(quiet(&["haiku"])), ..new_thread("settings") };
    new.settings.insert("model".into(), serde_json::json!("opus"));
    let chose = hub.create_thread(new).await.unwrap();
    loop {
        let end = next_event(&mut rx, EventKind::TurnEnd).await;
        if end.thread_id == chose.id {
            break;
        }
    }
    let said = reply_text(&hub.store.events(&chose.id, 0, 1000).await.unwrap());
    assert!(said.contains("model=opus"), "{said}");

    // Its permission question never reaches the user.
    let asks = hub.create_thread(NewThread { unattended: Some(quiet(&["haiku"])), ..new_thread("needs permission") }).await.unwrap();
    let auto = next_event(&mut rx, EventKind::PermissionAuto).await;
    assert_eq!(auto.thread_id, asks.id);
    assert_eq!(auto.data["allowed"], true);
    loop {
        let end = next_event(&mut rx, EventKind::TurnEnd).await;
        if end.thread_id == asks.id {
            break;
        }
    }
    let events = hub.store.events(&asks.id, 0, 1000).await.unwrap();
    assert!(!events.iter().any(|e| e.kind == EventKind::PermissionRequest));
}
