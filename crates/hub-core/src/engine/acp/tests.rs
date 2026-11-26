//! Tests of the ACP session.

#[test]
fn a_failed_admitted_turn_is_put_in_words() {
    let said = super::plain_detail(r#"{"details":"admitted_turn_failed"}"#.into());
    assert!(said.contains("model provider") && said.contains("admitted_turn_failed"), "{said}");
    assert_eq!(super::plain_detail("disk full".into()), "disk full");
}

/// "Internal error" alone says nothing; what went wrong inside it is what the thread shows.
#[test]
fn an_internal_error_says_what_went_wrong() {
    use agent_client_protocol::Error;
    let wrapped = super::internal(anyhow::anyhow!("no DeepSeek key here"));
    assert_eq!(super::acp_err(wrapped).to_string(), "no DeepSeek key here");
    assert_eq!(super::acp_err(Error::internal_error()).to_string(), "Internal error");
    let other = Error::invalid_params().data(serde_json::json!("model is unknown"));
    assert_eq!(super::acp_err(other).to_string(), "Invalid params: model is unknown");
}

/// A saved setting goes to the engine only when the session lists its id and offers its value:
/// one saved for another engine (an unknown id, a value it does not have) is left out.
#[test]
fn only_settings_the_session_offers_are_sent() {
    use serde_json::json;
    let config = json!([
        { "id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": "fast",
          "options": [{ "value": "fast", "name": "Fast" }, { "value": "deep", "name": "Deep" }] },
        { "id": "fast-mode", "name": "Fast mode", "type": "boolean", "currentValue": false },
    ]);
    let state = super::SessionState {
        config: Some(serde_json::from_value(config).unwrap()),
        modes: serde_json::from_value(json!({ "currentModeId": "default", "availableModes": [{ "id": "default", "name": "Default" }] })).ok(),
        route: None,
        deferred: Vec::new(),
    };
    // Unknown id: another engine's option.
    assert!(!state.offers("collaboration_mode", &json!("default")));
    // Known id, value it does not have.
    assert!(!state.offers("model", &json!("gpt-6-luna")));
    // Known id and value.
    assert!(state.offers("model", &json!("deep")));
    // A boolean option takes a boolean, not a choice.
    assert!(state.offers("fast-mode", &json!(true)));
    assert!(!state.offers("fast-mode", &json!("on")));
    // An agent with modes but no mode option: the mode must be one it lists.
    assert!(state.offers("mode", &json!("default")));
    assert!(!state.offers("mode", &json!("agent")));
}

/// A pick by name finds the model or level the session offers; a new thread's mode is the
/// engine's auto one, whether the agent lists it as a setting or only as a mode.
#[test]
fn a_name_finds_the_choice_and_auto_is_the_default_mode() {
    use serde_json::json;
    let config = json!([
        { "id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": "gpt-6.1-sol",
          "options": [{ "value": "gpt-6.1-sol", "name": "Sol 6.1" }, { "value": "gpt-6.1-terra", "name": "Terra 6.1" }] },
        { "id": "reasoning_effort", "name": "Reasoning", "category": "thought_level", "type": "select", "currentValue": "low",
          "options": [{ "value": "low", "name": "Low" }, { "value": "medium", "name": "Medium" }, { "value": "high", "name": "High" }] },
        { "id": "mode", "name": "Approvals", "category": "mode", "type": "select", "currentValue": "manual",
          "options": [{ "value": "manual", "name": "Manual" }, { "value": "auto-review", "name": "Auto review" }, { "value": "full", "name": "Full access" }] },
    ]);
    let state = super::SessionState { config: Some(serde_json::from_value(config).unwrap()), modes: None, route: None, deferred: Vec::new() };
    assert_eq!(super::by_name(&state, "model", "terra"), Some(("model".into(), "gpt-6.1-terra".into())));
    assert_eq!(super::by_name(&state, "reasoning_effort", "medium"), Some(("reasoning_effort".into(), "medium".into())));
    assert_eq!(super::by_name(&state, "model", "opus"), None, "another engine's name finds nothing");
    let words: Vec<String> = super::DEFAULT_MODE.iter().map(|w| (*w).to_owned()).collect();
    assert_eq!(super::preferred(&state, "mode", &words), Some(("mode".into(), "auto-review".into())));

    // An agent with modes only (no settings) counts as the "mode" setting.
    let modes = super::SessionState {
        config: None,
        modes: serde_json::from_value(json!({ "currentModeId": "default", "availableModes": [
            { "id": "default", "name": "Default" }, { "id": "acceptEdits", "name": "Accept Edits" }, { "id": "auto", "name": "Auto" }] })).ok(),
        route: None,
        deferred: Vec::new(),
    };
    assert_eq!(super::preferred(&modes, "mode", &words), Some(("mode".into(), "auto".into())));
    // An engine with no auto mode keeps its own default.
    let none = super::SessionState {
        config: None,
        modes: serde_json::from_value(json!({ "currentModeId": "default", "availableModes": [{ "id": "default", "name": "Default" }] })).ok(),
        route: None,
        deferred: Vec::new(),
    };
    assert_eq!(super::preferred(&none, "mode", &words), None);
    // An engine has no mode named auto: its "dont_ask" is the one that does not ask about edits.
    let odd = super::SessionState {
        config: None,
        modes: serde_json::from_value(json!({ "currentModeId": "default", "availableModes": [
            { "id": "default", "name": "Default" }, { "id": "accept_edits", "name": "Accept Edits" }, { "id": "dont_ask", "name": "Don't Ask" }] })).ok(),
        route: None,
        deferred: Vec::new(),
    };
    assert_eq!(super::preferred(&odd, "mode", &words), Some(("mode".into(), "dont_ask".into())));
}

/// A thread deleted while its engine winds down still gets updates. Logging them fails (the
/// thread is gone), which once went back to the SDK as a bare "Internal error" warning.
#[tokio::test]
async fn updates_for_a_thread_that_is_gone_are_dropped() {
    let data = tempfile::tempdir().unwrap();
    let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
    let hub = crate::Hub::new(crate::Store::in_memory().unwrap(), config);
    hub.store.insert_thread(crate::model::Thread {
        id: "t1".into(), title: "t".into(), cwd: "/tmp".into(), engine: "e1".into(),
        status: crate::model::ThreadStatus::Running, engine_session_id: None, title_locked: false,
        archived: false, returned_at: None, seen_at: None, settled_at: None, sent_at: None,
        pinned_at: None, snoozed_until: None, auto_settle: true, parent: None, reading: None, owner: None, cloud: None, host: None, project: None, display: None,
        created_at: 1, updated_at: 1,
    }).await.unwrap();
    let chunk = serde_json::json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "hi" } });
    super::take_update(&hub, "t1", chunk.clone()).await;
    assert_eq!(hub.store.events("t1", 0, 10).await.unwrap().len(), 1);

    hub.store.delete_thread("t1").await.unwrap();
    let title = serde_json::json!({ "sessionUpdate": "session_info_update", "title": "late" });
    assert!(super::record_update(&hub, "t1", chunk.clone()).await.is_err(), "the log refuses it");
    assert!(super::record_update(&hub, "t1", title.clone()).await.is_err());
    super::take_update(&hub, "t1", chunk).await;
    super::take_update(&hub, "t1", title).await;
    assert!(hub.store.thread("t1").await.unwrap().is_none());
}

#[tokio::test]
async fn the_engine_log_hides_keys() {
    use agent_client_protocol::LineDirection;
    let data = tempfile::tempdir().unwrap();
    let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
    let hub = crate::Hub::new(crate::Store::in_memory().unwrap(), config);
    hub.secrets.set("DEEPSEEK", "ds-abcdefghijklmnop1234567890").await.unwrap();
    let log = super::stderr_log(&hub, "t1");
    log("key ds-abcdefghijklmnop1234567890 refused", LineDirection::Stderr);
    log("token sk-ant-abcdefghijklmnopqrstuvwxyz0123 too", LineDirection::Stderr);
    log("not stderr sk-ant-abcdefghijklmnopqrstuvwxyz0123", LineDirection::Stdout);
    let text = std::fs::read_to_string(super::engine_log(&hub, "t1")).unwrap();
    assert_eq!(text, "key [secret DEEPSEEK] refused\ntoken [Anthropic API key sk-a…0123] too\n");
    assert!(!super::engine_trace(&hub, "t1").exists(), "no trace unless asked for");
}

#[tokio::test]
async fn a_trace_keeps_every_line_both_ways_without_keys() {
    use agent_client_protocol::LineDirection;
    let data = tempfile::tempdir().unwrap();
    let mut config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
    config.trace_engines = true;
    let hub = crate::Hub::new(crate::Store::in_memory().unwrap(), config);
    hub.secrets.set("DEEPSEEK", "ds-abcdefghijklmnop1234567890").await.unwrap();
    let log = super::stderr_log(&hub, "t1");
    log(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1}}"#, LineDirection::Stdin);
    log(r#"{"jsonrpc":"2.0","id":1,"result":{"key":"ds-abcdefghijklmnop1234567890"}}"#, LineDirection::Stdout);
    log("starting up", LineDirection::Stderr);
    let text = std::fs::read_to_string(super::engine_trace(&hub, "t1")).unwrap();
    let lines: Vec<serde_json::Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert_eq!(lines[0]["from"], "hub");
    assert_eq!(lines[0]["msg"]["method"], "initialize");
    assert_eq!(lines[1]["from"], "agent");
    assert_eq!(lines[1]["msg"]["result"]["key"], "[secret DEEPSEEK]");
    assert!(lines.iter().all(|l| l["ts"].is_i64()));
    assert_eq!(std::fs::read_to_string(super::engine_log(&hub, "t1")).unwrap(), "starting up\n");
}

#[test]
fn windows_paths_survive_as_engine_commands() {
    let agent = super::agent(r#""C:\Program Files\Agent\agent.exe" --acp C:\work"#, &[]).unwrap();
    let config = agent.config();
    assert_eq!(config.command().display().to_string(), r"C:\Program Files\Agent\agent.exe");
    assert_eq!(config.arguments(), ["--acp", r"C:\work"]);
}

/// An engine told Hub's rules in its first message named threads after them (13 threads called
/// "Hub coding agent workflow" in a week, 2026-11). A title about what the person asked stays.
/// An engine told Hub's rules in its first message names the session after them; the thread
/// keeps its title then, and takes one that is about what the person asked.
#[tokio::test]
async fn an_engine_told_the_rules_in_a_message_cannot_name_the_thread_after_them() {
    let data = tempfile::tempdir().unwrap();
    let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
    let hub = crate::Hub::new(crate::Store::in_memory().unwrap(), config);
    hub.store.insert_thread(crate::model::Thread {
        id: "t1".into(), title: "Zlatan".into(), cwd: "/tmp".into(), engine: "e1".into(),
        status: crate::model::ThreadStatus::Running, engine_session_id: None, title_locked: false,
        archived: false, returned_at: None, seen_at: None, settled_at: None, sent_at: None,
        pinned_at: None, snoozed_until: None, auto_settle: true, parent: None, reading: None, owner: None, cloud: None, host: None, project: None, display: None,
        created_at: 1, updated_at: 1,
    }).await.unwrap();
    hub.emit("t1", crate::model::EventKind::UserMessage, serde_json::json!({ "text": "Skapa ett diagram på hur många mål Zlatan har gjort" })).await.unwrap();
    let title = |t: &str| serde_json::json!({ "sessionUpdate": "session_info_update", "title": t });
    super::record_update(&hub, "t1", title("Hub coding agent workflow")).await.unwrap();
    assert_eq!(hub.store.thread("t1").await.unwrap().unwrap().title, "Zlatan");
    super::record_update(&hub, "t1", title("Zlatan goals chart")).await.unwrap();
    assert_eq!(hub.store.thread("t1").await.unwrap().unwrap().title, "Zlatan goals chart");
}

#[test]
fn a_title_about_hubs_own_rules_is_not_taken() {
    use super::turns::names_hubs_rules;
    assert!(names_hubs_rules("Hub coding agent workflow", &["Skapa ett diagram på hur många mål Zlatan har gjort"]));
    assert!(names_hubs_rules("Hub Thread Management Instructions", &["hej"]));
    assert!(!names_hubs_rules("Zlatan goals chart", &["Skapa ett diagram på hur många mål Zlatan har gjort"]));
    assert!(!names_hubs_rules("Hub inbox auto-settle", &["Varför har trådar från hubben i inkorgen inte settlat?"]));
    assert!(!names_hubs_rules("Fix Hub updater", &["uppdateringen hänger sig"]), "work on Hub itself is named so");
    assert!(!names_hubs_rules("GitHub Actions setup", &["sätt upp CI"]), "a word that only holds 'hub' is not Hub");
}
