//! Hub's tools: one thread's agent starting, messaging and reading another through Hub's tools, and
//! an agent outside Hub reaching them with a key, against the scripted `mock-agent` engine.

use std::sync::Arc;
use std::time::Duration;

use hub_core::model::{AgentAccess, EventKind, NewAgentKey, NewThread, ThreadStatus};
use hub_core::{Config, Hub, Store, api};
use serde_json::{Value, json};
use crate::support::{mock_agent, wait_status, turn_ended};

fn hub() -> Arc<Hub> {
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig { command: mock_agent().display().to_string(), driver: Default::default(), secrets: Vec::new(), terminal: None, traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default() },
    );
    config.default_engine = "mock".into();
    Hub::new(Store::in_memory().unwrap(), config)
}

fn call(name: &str, arguments: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

/// A tool's answer as JSON, failing the test on a tool error.
fn ok(answer: Value) -> Value {
    let result = &answer["result"];
    assert_eq!(result["isError"], false, "{answer}");
    serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
}

fn names(answer: &Value) -> Vec<String> {
    answer["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect()
}

#[tokio::test]
async fn a_thread_hands_work_to_another_and_reads_the_answer() {
    let hub = hub();
    let folder = std::env::temp_dir().display().to_string();
    let boss = hub
        .create_thread(NewThread {
            cwd: folder.clone(),
            title: Some("Boss".into()),
            engine: None,
            message: Some("hello".into()),
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
        })
        .await
        .unwrap();
    turn_ended(&hub, &boss.id).await;
    wait_status(&hub, &boss.id, ThreadStatus::Idle).await;
    hub.set_setting(&boss.id, "effort".into(), json!("high")).await.unwrap();

    let list = json!({ "jsonrpc": "2.0", "id": 0, "method": "tools/list" });
    let tools = names(&hub_core::mcp::handle(&hub, &boss.id, list).await.unwrap());
    for tool in ["hub_brief", "where_am_i", "hub_status", "delegate", "list_threads", "read_thread", "start_thread", "send_to_thread", "wait_for_thread", "stop_thread", "second_opinion"] {
        assert!(tools.contains(&tool.to_owned()), "{tool} missing from {tools:?}");
    }

    // The engines and their models, as the boss's session reported them.
    let status = ok(hub_core::mcp::handle(&hub, &boss.id, call("hub_status", json!({}))).await.unwrap());
    let models = status["engines"][0]["models"].as_array().unwrap();
    assert!(models.iter().any(|m| m["value"] == "haiku"), "{status}");

    // Where the boss is: its machine, folder and programs.
    let here = ok(hub_core::mcp::handle(&hub, &boss.id, call("where_am_i", json!({}))).await.unwrap());
    assert_eq!(here["thread"]["id"], boss.id.as_str(), "{here}");
    assert_eq!(here["project"]["folder"], folder.as_str(), "{here}");
    assert!(here["machine"]["os"].is_string() && here["programs_on_path"].is_array(), "{here}");

    // A new thread in the same folder, on the model asked for and otherwise the boss's settings.
    let start = call("start_thread", json!({ "message": "count the files", "model": "hai", "wait": true, "timeout_s": 30 }));
    let started = ok(hub_core::mcp::handle(&hub, &boss.id, start).await.unwrap());
    let worker = started["thread"]["id"].as_str().unwrap().to_owned();
    assert_eq!(started["thread"]["folder"], folder.as_str());
    assert_eq!(started["result"]["state"], "done", "{started}");
    assert!(started["result"]["reply"].as_str().unwrap().contains("count the files"), "{started}");
    let settings = hub.store.settings(&worker).await.unwrap();
    assert!(settings.contains(&("effort".into(), json!("high"))), "{settings:?}");
    assert!(settings.contains(&("model".into(), json!("haiku"))), "{settings:?}");

    // Its message is marked as the boss's, not the user's.
    let events = hub.store.events(&worker, 0, 1000).await.unwrap();
    let first = events.iter().find(|e| e.kind == EventKind::UserMessage).unwrap();
    assert_eq!(first.data["from"]["thread"], boss.id.as_str());

    // A follow-up, then the conversation as the boss reads it.
    let send = call("send_to_thread", json!({ "thread_id": worker, "message": "now sort them", "wait": true }));
    let sent = ok(hub_core::mcp::handle(&hub, &boss.id, send).await.unwrap());
    assert_eq!(sent["result"]["state"], "done", "{sent}");
    assert!(sent["result"]["reply"].as_str().unwrap().contains("now sort them"));
    assert!(!sent["result"]["reply"].as_str().unwrap().contains("count the files"), "only this turn's reply: {sent}");

    let read = ok(hub_core::mcp::handle(&hub, &boss.id, call("read_thread", json!({ "thread_id": worker }))).await.unwrap());
    let items = read["items"].as_array().unwrap();
    let who: Vec<&str> = items.iter().map(|i| i["who"].as_str().unwrap()).collect();
    assert_eq!(who, ["agent", "reply", "agent", "reply"], "{read}");
    assert_eq!(items[0]["from"], "Boss");

    // Waiting on a thread that already answered returns at once.
    let waited = ok(hub_core::mcp::handle(&hub, &boss.id, call("wait_for_thread", json!({ "thread_id": worker }))).await.unwrap());
    assert_eq!(waited["state"], "done");

    // A thread cannot message or stop itself.
    let own = call("send_to_thread", json!({ "thread_id": boss.id, "message": "hi" }));
    assert_eq!(hub_core::mcp::handle(&hub, &boss.id, own).await.unwrap()["result"]["isError"], true);
}

#[tokio::test]
async fn a_delegation_waits_out_a_plan_limit_and_is_kept_until_it_reports() {
    let hub = hub();
    let boss = hub
        .create_thread(NewThread {
            cwd: std::env::temp_dir().display().to_string(),
            title: Some("Boss".into()),
            engine: None,
            message: Some("hello".into()),
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
        })
        .await
        .unwrap();
    turn_ended(&hub, &boss.id).await;
    wait_status(&hub, &boss.id, ThreadStatus::Idle).await;

    // The worker hits a plan limit on its first turn.
    let delegated = call("delegate", json!({ "message": "quota job" }));
    let delegated = ok(hub_core::mcp::handle(&hub, &boss.id, delegated).await.unwrap());
    let worker = delegated["thread"]["id"].as_str().unwrap().to_owned();
    assert!(hub.store.get_value("delegations").await.unwrap().unwrap().contains(&worker), "kept until it reports");
    for _ in 0..400 {
        let events = hub.store.events(&worker, 0, 1000).await.unwrap();
        if events.iter().any(|e| e.kind == EventKind::QuotaWait && e.data["state"] == "waiting") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    wait_status(&hub, &worker, ThreadStatus::Idle).await;
    // It is not reported as failed while Hub waits for the limit to reset.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let told = |events: &[hub_core::model::Event]| events.iter().any(|e| e.kind == EventKind::UserMessage && e.data["from"]["thread"] == worker.as_str());
    assert!(!told(&hub.store.events(&boss.id, 0, 1000).await.unwrap()), "reported while it waited for the limit");

    // Carried on after the reset, the same job reports its reply.
    hub.carry_on(&worker, "you").await.unwrap();
    let mut back = None;
    for _ in 0..800 {
        let events = hub.store.events(&boss.id, 0, 1000).await.unwrap();
        back = events.into_iter().find(|e| e.kind == EventKind::UserMessage && e.data["from"]["thread"] == worker.as_str());
        if back.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let text = back.expect("the delegated thread never reported back").data["text"].as_str().unwrap().to_owned();
    assert!(text.contains("is done") && text.contains("plan limit that stopped you has reset"), "{text}");
    assert!(!hub.store.get_value("delegations").await.unwrap().unwrap_or_default().contains(&worker), "forgotten once reported");
}

#[tokio::test]
async fn a_brief_says_what_is_going_on_and_delegated_work_reports_back() {
    let hub = hub();
    let boss = hub
        .create_thread(NewThread {
            cwd: std::env::temp_dir().display().to_string(),
            title: Some("Boss".into()),
            engine: None,
            message: Some("hello".into()),
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
        })
        .await
        .unwrap();
    turn_ended(&hub, &boss.id).await;
    wait_status(&hub, &boss.id, ThreadStatus::Idle).await;

    // Delegating returns at once with the new thread.
    let delegated = call("delegate", json!({ "message": "tidy the readme" }));
    let delegated = ok(hub_core::mcp::handle(&hub, &boss.id, delegated).await.unwrap());
    let worker = delegated["thread"]["id"].as_str().unwrap().to_owned();
    assert!(delegated.get("result").is_none(), "{delegated}");

    // Its reply comes back to the boss by itself, marked as from the worker.
    let mut back = None;
    for _ in 0..400 {
        let events = hub.store.events(&boss.id, 0, 1000).await.unwrap();
        back = events.into_iter().find(|e| e.kind == EventKind::UserMessage && e.data["from"]["thread"] == worker.as_str());
        if back.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let back = back.expect("the delegated thread never reported back");
    let text = back.data["text"].as_str().unwrap();
    assert!(text.contains("is done") && text.contains("tidy the readme"), "{text}");

    // The brief lists both threads as finished, with the end of their replies.
    for _ in 0..400 {
        if hub.store.thread(&boss.id).await.unwrap().unwrap().status == ThreadStatus::Idle
            && hub.store.events(&boss.id, 0, 1000).await.unwrap().iter().filter(|e| e.kind == EventKind::TurnEnd).count() >= 2
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let brief = ok(hub_core::mcp::handle(&hub, &boss.id, call("hub_brief", json!({}))).await.unwrap());
    let finished = brief["finished"].as_array().unwrap();
    let ids: Vec<&str> = finished.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&worker.as_str()) && ids.contains(&boss.id.as_str()), "{brief}");
    let item = finished.iter().find(|t| t["id"] == worker.as_str()).unwrap();
    assert!(item["reply"].as_str().unwrap().contains("tidy the readme"), "{brief}");
    assert!(brief["summary"].as_str().unwrap().starts_with("0 need the user, 0 working, 2 finished"), "{brief}");

    // An outside agent has no thread to report back to.
    let outside = hub_core::hub_tools::Caller::Agent { name: "bot".into(), access: AgentAccess::Full };
    let refused = hub_core::hub_tools::call(&hub, &outside, "delegate", &json!({ "message": "x", "folder": std::env::temp_dir() })).await.unwrap();
    assert!(refused.is_err());
}

async fn serve(hub: Arc<Hub>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, api::router(hub)).await.unwrap() });
    base
}

#[tokio::test]
async fn an_outside_agent_gets_what_its_key_allows() {
    let hub = hub();
    let base = serve(hub.clone()).await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let post = |token: &str, body: Value| {
        http.post(format!("{base}/mcp")).header("authorization", format!("Bearer {token}")).json(&body).send()
    };

    let made = hub_core::hub_tools::create_key(&hub, NewAgentKey { name: "terminal".into(), access: AgentAccess::Read, days: None })
        .await
        .unwrap();
    assert!(made.command.starts_with("claude mcp add --transport http hub http://"), "{}", made.command);
    assert!(made.command.contains(&made.token));
    // Only its hash is kept.
    let kept = hub.store.get_value("agent_keys").await.unwrap().unwrap();
    assert!(!kept.contains(&made.token));

    let list = json!({ "jsonrpc": "2.0", "id": 0, "method": "tools/list" });
    let answer: Value = post(&made.token, list.clone()).await.unwrap().json().await.unwrap();
    let tools = names(&answer);
    assert!(tools.contains(&"read_thread".to_owned()));
    assert!(!tools.contains(&"start_thread".to_owned()) && !tools.contains(&"second_opinion".to_owned()), "{tools:?}");
    let start = call("start_thread", json!({ "message": "hi", "folder": std::env::temp_dir() }));
    let refused: Value = post(&made.token, start).await.unwrap().json().await.unwrap();
    assert_eq!(refused["result"]["isError"], true);
    assert!(hub.store.threads().await.unwrap().is_empty());
    assert!(hub_core::hub_tools::keys(&hub).await.unwrap()[0].last_used_at.is_some());

    // A wrong key, a web page and a revoked key are turned away.
    assert_eq!(post("hub_wrong", list.clone()).await.unwrap().status(), 401);
    let page = http
        .post(format!("{base}/mcp"))
        .header("origin", "https://evil.example")
        .header("authorization", format!("Bearer {}", made.token))
        .json(&list)
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), 401);
    hub_core::hub_tools::revoke_key(&hub, &made.key.id).await.unwrap();
    assert_eq!(post(&made.token, list.clone()).await.unwrap().status(), 401);

    // A supervised key starts threads, which run with the engine's defaults.
    let made = hub_core::hub_tools::create_key(&hub, NewAgentKey { name: "bot".into(), access: AgentAccess::Supervised, days: Some(1) })
        .await
        .unwrap();
    let start = call("start_thread", json!({ "message": "hello there", "folder": std::env::temp_dir(), "settings": { "mode": "yolo" } }));
    let refused: Value = post(&made.token, start).await.unwrap().json().await.unwrap();
    assert_eq!(refused["result"]["isError"], true, "settings need full access");
    let start = call("start_thread", json!({ "message": "hello there", "folder": std::env::temp_dir(), "wait": true }));
    let answer: Value = post(&made.token, start).await.unwrap().json().await.unwrap();
    let started = ok(answer);
    assert_eq!(started["result"]["state"], "done", "{started}");
    let id = started["thread"]["id"].as_str().unwrap();
    let events = hub.store.events(id, 0, 1000).await.unwrap();
    let first = events.iter().find(|e| e.kind == EventKind::UserMessage).unwrap();
    assert_eq!(first.data["from"]["agent"], "bot");
}

#[tokio::test]
async fn ask_jev_is_a_quiet_tool_that_says_when_it_is_off() {
    let hub = hub();
    let names: Vec<String> = hub_core::hub_tools::tools(AgentAccess::Read).iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect();
    assert!(names.contains(&"ask_jev".to_owned()));
    assert!(hub_core::permit::QUIET_TOOLS.contains(&"ask_jev"));
    // No connection: the agent is told to decide itself, never given an error to get stuck on.
    let answer = hub.ask_for_agent("t1", "Is it done?", "text", Vec::new(), Vec::new()).await;
    assert_eq!(answer["answered"], false);
    assert_eq!(answer["note"], "Decide yourself.");
    // The per-thread switch reads back (it also needs a connection to be on at all).
    hub.set_agent_jev("t1", false).await.unwrap();
    assert!(!hub.agent_jev_on("t1").await);
    hub.set_agent_jev("t1", true).await.unwrap();
}
