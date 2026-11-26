//! Hub's own tools (MCP over HTTP) as an engine reaches them, past the connection token guard.

use std::sync::Arc;

use hub_core::model::{EventKind, NewThread};
use hub_core::{Config, Hub, Store, api};
use serde_json::json;

async fn serve(hub: Arc<Hub>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, api::router(hub)).await.unwrap() });
    base
}

fn initialize() -> serde_json::Value {
    json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } })
}

#[tokio::test]
async fn an_engine_reaches_the_tools_with_what_it_is_given() {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let thread = hub
        .create_thread(NewThread {
            cwd: std::env::temp_dir().display().to_string(),
            title: Some("tools".into()),
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
        })
        .await
        .unwrap();
    let base = serve(hub.clone()).await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();

    // What the engine is handed in `session/new`: the path and the token it must show.
    let (url, token) = hub_core::auth::tools_endpoint(&hub.config, &thread.id);
    let path = url.split_once("/mcp/").map(|(_, id)| format!("/mcp/{id}")).unwrap();
    let ok = http
        .post(format!("{base}{path}"))
        .header("authorization", format!("Bearer {}", token.unwrap()))
        .json(&initialize())
        .send()
        .await
        .unwrap();
    assert_eq!(ok.status(), 200);
    let answer: serde_json::Value = ok.json().await.unwrap();
    assert_eq!(answer["result"]["serverInfo"]["name"], "hub");

    // Without it the engine is refused, and the thread says why (once), since the engine only
    // reports that the server "failed to start".
    for _ in 0..2 {
        let refused = http.post(format!("{base}{path}")).json(&initialize()).send().await.unwrap();
        assert_eq!(refused.status(), 401);
    }
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let errors: Vec<_> = events.iter().filter(|e| e.kind == EventKind::Error).collect();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].data["message"].as_str().unwrap().contains("connection token"));
}

#[test]
fn a_core_bound_to_every_interface_is_dialled_over_loopback() {
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.bind = "0.0.0.0:7878".parse().unwrap();
    assert_eq!(hub_core::auth::tools_endpoint(&config, "t1").0, "http://127.0.0.1:7878/mcp/t1");
}

#[tokio::test]
async fn an_agent_shows_a_page_that_is_served_sandboxed_and_cannot_call_the_api() {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let thread = hub
        .create_thread(NewThread {
            cwd: std::env::temp_dir().display().to_string(),
            title: Some("pages".into()),
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
        })
        .await
        .unwrap();
    let base = serve(hub.clone()).await;
    let http = reqwest::Client::builder().no_proxy().build().unwrap();

    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    let tools = hub_core::mcp::handle(&hub, &thread.id, list).await.unwrap();
    assert!(tools["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "show_page"), "{tools}");

    let html = "<!doctype html><html><body><canvas id=c></canvas><script>c.width=200</script></body></html>";
    let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "show_page", "arguments": { "html": html, "title": "Demo", "height": 5000 } } });
    let answer = hub_core::mcp::handle(&hub, &thread.id, call).await.unwrap();
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let page = events.iter().find(|e| e.kind == EventKind::Page).expect("a page event");
    assert_eq!((page.data["title"].as_str(), page.data["height"].as_i64()), (Some("Demo"), Some(1200)));
    assert!(page.data.get("html").is_none(), "the page stays out of the log");

    // The frame and the browser show no token: the page's random id lets them in, sandboxed.
    let id = page.data["id"].as_str().unwrap();
    let served = http.get(format!("{base}/api/threads/{}/pages/{id}", thread.id)).send().await.unwrap();
    assert_eq!(served.status(), 200);
    let csp = served.headers()["content-security-policy"].to_str().unwrap().to_owned();
    assert!(csp.starts_with("sandbox allow-scripts;") && !csp.contains("allow-same-origin"), "{csp}");
    assert!(csp.contains("connect-src 'none'"), "{csp}");
    assert!(served.text().await.unwrap().contains("<canvas id=c>"));

    // Nothing else opens that way, and a sandboxed page (origin "null") is refused by the API.
    let other = http.get(format!("{base}/api/threads/{}/pages/nope", thread.id)).send().await.unwrap();
    assert_eq!(other.status(), 404);
    let events = http.get(format!("{base}/api/threads/{}/events", thread.id)).send().await.unwrap();
    assert_eq!(events.status(), 401);
    let from_page = http.get(format!("{base}/api/threads")).header("origin", "null").send().await.unwrap();
    assert_eq!(from_page.status(), 401);
    let posted = http.post(format!("{base}/api/threads/{}/pages/{id}", thread.id)).send().await.unwrap();
    assert!(posted.status().is_client_error());

    // The page goes with its thread.
    hub.delete_thread(&thread.id).await.unwrap();
    let gone = http.get(format!("{base}/api/threads/{}/pages/{id}", thread.id)).send().await.unwrap();
    assert_eq!(gone.status(), 404);
}

#[tokio::test]
async fn a_changed_page_is_a_new_version_and_the_agent_can_look_at_a_draft() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.bind = listener.local_addr().unwrap();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let thread = hub
        .create_thread(NewThread {
            cwd: std::env::temp_dir().display().to_string(),
            title: Some("versions".into()),
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
        })
        .await
        .unwrap();
    let router = api::router(hub.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let call = |name: &str, args: serde_json::Value| {
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": name, "arguments": args } })
    };

    let first = hub_core::mcp::handle(&hub, &thread.id, call("show_page", json!({ "html": "<p>v1</p>", "title": "Chart" }))).await.unwrap();
    assert_eq!(first["result"]["isError"], false, "{first}");
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let v1 = events.iter().find(|e| e.kind == EventKind::Page).unwrap().data["id"].as_str().unwrap().to_owned();
    assert!(first["result"]["content"][0]["text"].as_str().unwrap().contains(&v1), "the agent learns the id to replace");

    // A new version names the one it replaces; a page that is not there is refused.
    let bad = hub_core::mcp::handle(&hub, &thread.id, call("show_page", json!({ "html": "<p>v2</p>", "title": "Chart", "replaces": "nope" }))).await.unwrap();
    assert_eq!(bad["result"]["isError"], true, "{bad}");
    let second = hub_core::mcp::handle(&hub, &thread.id, call("show_page", json!({ "html": "<p>v2</p>", "title": "Chart", "replaces": v1 }))).await.unwrap();
    assert_eq!(second["result"]["isError"], false, "{second}");
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let pages: Vec<_> = events.iter().filter(|e| e.kind == EventKind::Page).collect();
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[1].data["replaces"], json!(v1));

    // Checking needs nothing but the arguments; with no browser on the machine it says so.
    let none = hub_core::mcp::handle(&hub, &thread.id, call("page_check", json!({}))).await.unwrap();
    assert_eq!(none["result"]["isError"], true, "{none}");
    let gone = hub_core::mcp::handle(&hub, &thread.id, call("page_check", json!({ "file": "hub-no-such-page.html" }))).await.unwrap();
    assert_eq!(gone["result"]["isError"], true, "{gone}");
    // The rest drives a real browser, which the updater's test run on the PC cannot count on
    // (Chrome there has ended before it was ready under load): only with HUB_TEST_BROWSER set.
    if hub_core::page_check::find_browser().is_none() || std::env::var_os("HUB_TEST_BROWSER").is_none() {
        return;
    }
    let html = "<body style='height:420px'><h1>Draft</h1><script>fetch('http://127.0.0.1:1/x');missing()</script></body>";
    let seen = hub_core::mcp::handle(&hub, &thread.id, call("page_check", json!({ "html": html }))).await.unwrap();
    let content = &seen["result"]["content"];
    assert_eq!(seen["result"]["isError"], false, "{seen}");
    let text = content[0]["text"].as_str().unwrap();
    assert!(text.contains("Rendered at 720×") && text.contains("missing is not defined"), "{text}");
    assert!(text.contains("connect-src"), "the sandbox's block is told: {text}");
    assert_eq!((content[1]["type"].as_str(), content[1]["mimeType"].as_str()), (Some("image"), Some("image/png")));
    let after = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(after.iter().filter(|e| e.kind == EventKind::Page).count(), 2, "a draft is not shown");
    let shown = hub_core::mcp::handle(&hub, &thread.id, call("page_check", json!({ "page": v1 }))).await.unwrap();
    assert_eq!(shown["result"]["isError"], false, "{shown}");
    // A page the agent wrote to a file in its folder, looked at by its path.
    let name = format!("hub-page-check-{}.html", std::process::id());
    let file = std::env::temp_dir().join(&name);
    std::fs::write(&file, "<!doctype html><h1>Hello world</h1>").unwrap();
    let on_disk = hub_core::mcp::handle(&hub, &thread.id, call("page_check", json!({ "file": name }))).await.unwrap();
    std::fs::remove_file(&file).unwrap();
    assert_eq!(on_disk["result"]["isError"], false, "{on_disk}");
    assert!(on_disk["result"]["content"][0]["text"].as_str().unwrap().contains("No script errors"), "{on_disk}");
}

#[tokio::test]
async fn a_thread_is_told_to_hand_pictures_to_a_side_job_unless_none_is_set() {
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = Hub::new(Store::in_memory().unwrap(), config.clone());
    let new = || NewThread {
        cwd: std::env::temp_dir().display().to_string(),
        title: Some("pictures".into()),
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
    };
    let thread = hub.create_thread(new()).await.unwrap();
    let said = hub_core::mcp::handle(&hub, &thread.id, initialize()).await.unwrap();
    assert!(said["result"]["instructions"].as_str().unwrap().contains("start_side_job"), "{said}");

    config.side_jobs.clear();
    let free = Hub::new(Store::in_memory().unwrap(), config);
    let thread = free.create_thread(new()).await.unwrap();
    let said = hub_core::mcp::handle(&free, &thread.id, initialize()).await.unwrap();
    assert!(!said["result"]["instructions"].as_str().unwrap().contains("start_side_job"), "{said}");
}

#[tokio::test]
async fn an_engine_warmed_before_its_thread_exists_still_reaches_the_tools() {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let id = "01a19cb2-0000-7000-8000-warmtools001";

    // Nothing is waiting for the id: the engine is turned away.
    let none = hub_core::mcp::handle(&hub, id, initialize()).await.unwrap();
    assert_eq!(none["error"]["code"], -32602, "{none}");

    // An engine starts ahead of its thread and connects to Hub's tools as it starts. Claude Code
    // reports the server as failed for the whole session when it is refused then.
    let cwd = std::env::temp_dir().display().to_string();
    hub_core::warm::add(hub_core::warm::Warm { id: id.into(), engine: "claude".into(), cwd, settings: Vec::new(), at: std::time::Instant::now(), claimed: false });
    let answer = hub_core::mcp::handle(&hub, id, initialize()).await.unwrap();
    assert_eq!(answer["result"]["serverInfo"]["name"], "hub", "{answer}");
    let list = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
    let tools = hub_core::mcp::handle(&hub, id, list).await.unwrap();
    let names: Vec<_> = tools["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"hub_brief") && names.contains(&"show_page"), "{names:?}");

    // Once the engine has its thread or gives up, the id is unknown again.
    hub_core::warm::forget(id);
    let gone = hub_core::mcp::handle(&hub, id, initialize()).await.unwrap();
    assert_eq!(gone["error"]["code"], -32602, "{gone}");
}

#[tokio::test]
async fn a_question_to_the_user_answers_the_agent_at_once_and_waits_in_the_chat() {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let thread = hub
        .create_thread(NewThread {
            cwd: std::env::temp_dir().display().to_string(),
            title: Some("ask".into()),
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
        })
        .await
        .unwrap();
    let call = |args: serde_json::Value| json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "ask_user", "arguments": args } });
    let before = hub.store.thread(&thread.id).await.unwrap().unwrap().status;
    let answer = hub_core::mcp::handle(&hub, &thread.id, call(json!({
        "question": "Which name?",
        "options": [{ "label": "alpha" }, { "label": "beta" }],
        "assumed": "alpha",
    })))
    .await
    .unwrap();
    assert_eq!(answer["result"]["isError"], false, "{answer}");
    let text: serde_json::Value = serde_json::from_str(answer["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!((text["answered"].clone(), text["assuming"].as_str()), (json!(false), Some("alpha")));
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let ask = events.iter().find(|e| e.kind == EventKind::Ask).expect("the question is logged");
    assert_eq!((ask.data["question"].as_str(), ask.data["assumed"].as_str()), (Some("Which name?"), Some("alpha")));
    assert!(ask.data["id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().status, before, "a question that does not wait never makes the thread wait");

    // No assumption, no question.
    let refused = hub_core::mcp::handle(&hub, &thread.id, call(json!({ "question": "Which?", "options": [{ "label": "a" }, { "label": "b" }] }))).await.unwrap();
    assert_eq!(refused["result"]["isError"], true);
}
