//! Threads on another machine: a second core stands in for tincan, reached at a URL.

use std::sync::Arc;
use std::time::Duration;

use hub_core::config::HostConfig;
use hub_core::model::{EventKind, HostInfo, HostState, StreamMsg, Thread};
use hub_core::{Config, Hub, Store, api};
use serde_json::json;
use crate::support::mock_agent;

/// A core's config with its own data folder, so each has its own connection token.
fn config(name: &str, hosts: Vec<HostConfig>) -> Config {
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig {
            command: mock_agent().display().to_string(),
            driver: Default::default(),
            secrets: Vec::new(),
            terminal: None,
            traits: hub_core::config::EngineTraits::preset("claude"), ..Default::default()
        },
    );
    config.default_engine = "mock".into();
    config.name = Some(name.into());
    config.hosts = hosts;
    config
}

/// Serves a core's API on a free loopback port.
async fn serve(hub: Arc<Hub>) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, api::router(hub)).await.unwrap();
    });
    (base, task)
}

/// A client that shows a core's connection token, as programs other than the app must.
fn client(hub: &Hub) -> reqwest::Client {
    let token = hub_core::auth::token(&hub.config.data_dir).unwrap();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(reqwest::header::AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
    reqwest::Client::builder().no_proxy().default_headers(headers).build().unwrap()
}

async fn host_state(client: &reqwest::Client, base: &str, name: &str) -> Option<HostState> {
    let hosts: Vec<HostInfo> = client.get(format!("{base}/api/hosts")).send().await.ok()?.json().await.ok()?;
    hosts.into_iter().find(|h| h.name == name).map(|h| h.state)
}

async fn wait_host(client: &reqwest::Client, base: &str, name: &str, state: HostState) {
    for _ in 0..200 {
        if host_state(client, base, name).await == Some(state) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{name} never became {state:?}");
}

#[tokio::test]
async fn a_thread_started_for_another_host_runs_there_and_shows_here() {
    let tincan = Hub::new(Store::in_memory().unwrap(), config("tincan", vec![]));
    let (tincan_url, _tincan_server) = serve(tincan.clone()).await;
    let token = hub_core::auth::token(&tincan.config.data_dir).unwrap();
    let host = HostConfig { name: "tincan".into(), ssh: None, url: Some(tincan_url), port: 7878, token: Some(token), push_quiet_mins: None };
    let pc = Hub::new(Store::in_memory().unwrap(), config("PC", vec![host]));
    let (pc_url, _pc_server) = serve(pc.clone()).await;
    let client = client(&pc);

    wait_host(&client, &pc_url, "tincan", HostState::Online).await;
    let hosts: Vec<HostInfo> = client.get(format!("{pc_url}/api/hosts")).send().await.unwrap().json().await.unwrap();
    assert_eq!(hosts[0].name, "PC");
    assert!(hosts[0].local);

    let mut rx = pc.subscribe();
    let cwd = std::env::temp_dir().display().to_string();
    let created = client
        .post(format!("{pc_url}/api/threads?host=tincan"))
        .json(&json!({ "cwd": cwd, "message": "hello there" }))
        .send()
        .await
        .unwrap();
    assert!(created.status().is_success(), "{}", created.text().await.unwrap());
    let thread: Thread = created.json().await.unwrap();
    assert_eq!(thread.host.as_deref(), Some("tincan"));

    // It lives on tincan, not here.
    assert!(tincan.store.thread(&thread.id).await.unwrap().is_some());
    assert!(pc.store.thread(&thread.id).await.unwrap().is_none());

    // tincan's stream is relayed into this core's stream.
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(StreamMsg::Event { event }) = rx.recv().await
                && event.thread_id == thread.id
                && event.kind == EventKind::TurnEnd
            {
                return;
            }
        }
    })
    .await
    .expect("the turn end was relayed");

    // Listing and the log go through this core.
    let list: Vec<Thread> = client.get(format!("{pc_url}/api/threads")).send().await.unwrap().json().await.unwrap();
    let listed = list.iter().find(|t| t.id == thread.id).expect("listed here");
    assert_eq!(listed.host.as_deref(), Some("tincan"));
    let events: Vec<hub_core::model::Event> = client
        .get(format!("{pc_url}/api/threads/{}/events", thread.id))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let reply: String = events
        .iter()
        .filter(|e| e.kind == EventKind::Update && e.data["sessionUpdate"] == "agent_message_chunk")
        .filter_map(|e| e.data["content"]["text"].as_str())
        .collect();
    assert_eq!(reply, "Echo: hello there");
    let one: Thread =
        client.get(format!("{pc_url}/api/threads/{}", thread.id)).send().await.unwrap().json().await.unwrap();
    assert_eq!(one.host.as_deref(), Some("tincan"));

    // An unknown host is refused rather than run here.
    let nowhere = client
        .post(format!("{pc_url}/api/threads?host=nowhere"))
        .json(&json!({ "cwd": std::env::temp_dir().display().to_string() }))
        .send()
        .await
        .unwrap();
    assert_eq!(nowhere.status(), reqwest::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn hosts_added_from_the_app_are_kept_and_can_be_removed() {
    let store = Store::in_memory().unwrap();
    let pc = Hub::new(store.clone(), config("PC", vec![]));
    let (pc_url, _server) = serve(pc.clone()).await;
    let client = client(&pc);

    let added = client
        .post(format!("{pc_url}/api/hosts"))
        .json(&json!({ "name": "nowhere", "url": "http://127.0.0.1:9" }))
        .send()
        .await
        .unwrap();
    assert_eq!(added.status(), reqwest::StatusCode::CREATED);
    let same = client.post(format!("{pc_url}/api/hosts")).json(&json!({ "name": "PC", "ssh": "pc" })).send().await.unwrap();
    assert_eq!(same.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(store.get_value("hosts").await.unwrap().unwrap().contains("nowhere"));
    wait_host(&client, &pc_url, "nowhere", HostState::Offline).await;

    let removed = client.delete(format!("{pc_url}/api/hosts/nowhere")).send().await.unwrap();
    assert_eq!(removed.status(), reqwest::StatusCode::NO_CONTENT);
    assert_eq!(host_state(&client, &pc_url, "nowhere").await, None);
    assert!(!store.get_value("hosts").await.unwrap().unwrap().contains("nowhere"));
}

#[tokio::test]
async fn the_gap_between_versions_of_a_machine_added_here_is_set_kept_and_used() {
    let store = Store::in_memory().unwrap();
    let fixed = HostConfig { name: "fixed".into(), ssh: None, url: Some("http://127.0.0.1:9".into()), port: 7878, token: None, push_quiet_mins: Some(20) };
    let pc = Hub::new(store.clone(), config("PC", vec![fixed]));
    let (pc_url, _server) = serve(pc.clone()).await;
    let client = client(&pc);
    let added = client.post(format!("{pc_url}/api/hosts")).json(&json!({ "name": "nowhere", "url": "http://127.0.0.1:9" })).send().await.unwrap();
    assert_eq!(added.status(), reqwest::StatusCode::CREATED);
    let patch = |name: &str, body: serde_json::Value| client.patch(format!("{pc_url}/api/hosts/{name}")).json(&body).send();

    // Set: answered, listed, kept with the machines added here, and what the ten-minute check reads.
    let set = patch("nowhere", json!({ "push_quiet_mins": 30 })).await.unwrap();
    assert_eq!(set.status(), reqwest::StatusCode::OK);
    assert_eq!(set.json::<HostInfo>().await.unwrap().push_quiet_mins, Some(30));
    let hosts: Vec<HostInfo> = client.get(format!("{pc_url}/api/hosts")).send().await.unwrap().json().await.unwrap();
    assert_eq!(hosts.iter().find(|h| h.name == "nowhere").unwrap().push_quiet_mins, Some(30));
    assert!(store.get_value("hosts").await.unwrap().unwrap().contains("\"push_quiet_mins\":30"));
    assert_eq!(pc.remotes.push_gap("nowhere"), Some(30));
    // A field left out stays; 0 clears it.
    assert_eq!(patch("nowhere", json!({})).await.unwrap().json::<HostInfo>().await.unwrap().push_quiet_mins, Some(30));
    assert_eq!(patch("nowhere", json!({ "push_quiet_mins": 0 })).await.unwrap().json::<HostInfo>().await.unwrap().push_quiet_mins, None);
    assert_eq!(pc.remotes.push_gap("nowhere"), None);

    // An unknown machine, and one from hub.json, are refused and say why.
    let unknown = patch("elsewhere", json!({ "push_quiet_mins": 30 })).await.unwrap();
    assert_eq!(unknown.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(unknown.text().await.unwrap().contains("no machine called elsewhere"));
    let configured = patch("fixed", json!({ "push_quiet_mins": 30 })).await.unwrap();
    assert_eq!(configured.status(), reqwest::StatusCode::BAD_REQUEST);
    assert!(configured.text().await.unwrap().contains("hub.json"));
    // Its own gap from hub.json is on its info and used all the same.
    assert_eq!(pc.remotes.push_gap("fixed"), Some(20));
}

#[tokio::test]
async fn a_machine_says_who_it_is_and_keeps_saying_it() {
    let tincan = Hub::new(Store::in_memory().unwrap(), config("tincan", vec![]));
    let data = tincan.config.data_dir.clone();
    let client = client(&tincan);
    let (base, task) = serve(tincan).await;
    // Health is open, so a client can check who answers before it shows a token.
    let health: serde_json::Value = reqwest::get(format!("{base}/api/health")).await.unwrap().json().await.unwrap();
    let id = health["machine"].as_str().unwrap().to_owned();
    assert_eq!(health["name"], "tincan");
    let routes: hub_core::model::MachineRoutes = client.get(format!("{base}/api/machine/routes")).send().await.unwrap().json().await.unwrap();
    assert_eq!(routes.id, id);
    assert_eq!(routes.name, "tincan");
    // A phone code is only for those with the machine's token.
    let refused = reqwest::Client::builder().no_proxy().build().unwrap().post(format!("{base}/api/pair/phone-code")).send().await.unwrap();
    assert_eq!(refused.status(), 401);
    let code: hub_core::model::PairCode = client.post(format!("{base}/api/pair/phone-code")).send().await.unwrap().json().await.unwrap();
    assert_eq!(code.code.len(), 9);
    task.abort();
    // The same machine after a restart.
    assert_eq!(hub_core::identity::machine_id(&data).unwrap(), id);
}

#[tokio::test(flavor = "multi_thread")]
async fn another_machine_runs_the_tests_of_a_commit_sent_to_it_and_says_why_they_failed() {
    use hub_core::update::checks::{Outcome, Status, start, wait};

    let mut tincan_config = config("tincan", vec![]);
    tincan_config.update.min_free_gb = 0;
    let tincan = Hub::new(Store::in_memory().unwrap(), tincan_config);
    let (tincan_url, _tincan_server) = serve(tincan.clone()).await;
    let token = hub_core::auth::token(&tincan.config.data_dir).unwrap();
    let host = HostConfig { name: "tincan".into(), ssh: None, url: Some(tincan_url.clone()), port: 7878, token: Some(token), push_quiet_mins: None };
    let pc = Hub::new(Store::in_memory().unwrap(), config("PC", vec![host]));
    let (pc_url, _pc_server) = serve(pc.clone()).await;
    wait_host(&client(&pc), &pc_url, "tincan", HostState::Online).await;

    // A repository with one commit and nothing to test: the first step cannot even start there,
    // which is the way a failing commit reaches the PC.
    let repo = hub_core::janitor::test_dir();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=Hub", "-c", "user.email=hub@localhost"])
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    git(&["init", "--quiet", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "one"]);
    let full = git(&["rev-parse", "HEAD"]);
    let short = full[..12].to_owned();

    let outcome = {
        let (pc, repo, short) = (pc.clone(), repo.clone(), short.clone());
        tokio::task::spawn_blocking(move || {
            let session = start(&pc, "tincan", &short, &repo, &|_| {}).expect("tincan takes the commit and starts the tests");
            wait(&pc, &session, Duration::from_secs(120), &|_| {}, &Default::default()).unwrap()
        })
        .await
        .unwrap()
    };
    match outcome {
        Outcome::Failed(why) => assert!(why.contains("npm"), "{why}"),
        other => panic!("expected the tests to fail there, got {other:?}"),
    }

    // The commit is kept there, so the next one is sent as the difference.
    let status: Status = client(&tincan).get(format!("{tincan_url}/api/checks")).send().await.unwrap().json().await.unwrap();
    assert_eq!(status.have.as_deref(), Some(full.as_str()));
    let _ = std::fs::remove_dir_all(&repo);
}

#[tokio::test]
async fn a_host_answers_the_ask_to_look_for_a_sent_update_with_its_updater_state() {
    let tincan = Hub::new(Store::in_memory().unwrap(), config("tincan", vec![]));
    let (url, _server) = serve(tincan.clone()).await;
    let client = client(&tincan);
    // With and without the version: an older sender asks with nothing.
    for body in [json!({ "version": "762a45f" }), json!({})] {
        let answer = client.post(format!("{url}/api/update/look")).json(&body).send().await.unwrap();
        assert!(answer.status().is_success());
        let status: hub_core::model::UpdateStatus = answer.json().await.unwrap();
        // A core with no updater says so; it does not claim to have looked.
        assert_eq!(status.state, hub_core::model::UpdateState::Disabled);
    }
    let health: serde_json::Value = client.get(format!("{url}/api/health")).send().await.unwrap().json().await.unwrap();
    assert!(health["features"].as_array().unwrap().iter().any(|f| f == "update.look"), "{health}");
}

#[tokio::test]
async fn tests_that_fail_after_an_install_become_one_problem_that_counts_each_time() {
    use hub_core::problems::{self, Level, ProblemFilter, View};

    let hub = Hub::new(Store::in_memory().unwrap(), config("PC", vec![]));
    let hub = Arc::new(hub);
    for _ in 0..2 {
        let incoming = hub_core::update::checks::problem("0123456789abcdef", "tincan", Level::Error, "The tests of the installed version 0123456789ab failed on tincan", Some("`cargo test` failed"));
        problems::record(&hub, incoming).await.unwrap();
    }
    let all = problems::list(&hub, ProblemFilter { view: Some(View::All), ..Default::default() }).await.unwrap();
    let ours: Vec<_> = all.iter().filter(|p| p.source == "update tests").collect();
    assert_eq!(ours.len(), 1, "{all:?}");
    assert_eq!(ours[0].level, Level::Error);
}

#[tokio::test]
async fn host_tokens_move_from_the_database_to_the_vault() {
    let store = Store::in_memory().unwrap();
    let legacy = r#"[{"name":"old box","ssh":null,"url":"http://127.0.0.1:9","port":7878,"token":"secret-token-1"}]"#;
    store.set_value("hosts", legacy).await.unwrap();
    let config = config("PC", vec![]);
    // A copy of the database made by an older version holds the token too.
    let backups = hub_core::backup::dir(&config.data_dir);
    std::fs::create_dir_all(&backups).unwrap();
    let copy = backups.join("daily-1.db");
    {
        let c = rusqlite::Connection::open(&copy).unwrap();
        c.execute_batch("CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT)").unwrap();
        c.execute("INSERT INTO kv VALUES ('hosts', ?1)", [legacy]).unwrap();
    }
    let pc = Hub::new(store.clone(), config);
    let (pc_url, _server) = serve(pc.clone()).await;
    let client = client(&pc);
    wait_host(&client, &pc_url, "old box", HostState::Offline).await;

    assert_eq!(pc.secrets.value("HUB_HOST_OLD_BOX").await.as_deref(), Some("secret-token-1"));
    assert!(!store.get_value("hosts").await.unwrap().unwrap().contains("secret-token-1"));
    let kept = || {
        let c = rusqlite::Connection::open(&copy).unwrap();
        let v: String = c.query_row("SELECT value FROM kv WHERE key = 'hosts'", [], |r| r.get(0)).unwrap();
        v.contains("secret-token-1")
    };
    for _ in 0..50 {
        if !kept() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(!kept());

    // Removing the machine takes its token out of the vault.
    client.delete(format!("{pc_url}/api/hosts/old%20box")).send().await.unwrap();
    assert_eq!(pc.secrets.value("HUB_HOST_OLD_BOX").await, None);
}
