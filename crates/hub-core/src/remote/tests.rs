//! Guards reaching and updating other machines: the URL before the tunnel, a refused token
//! noticed, what the SSH probe and check say, the installer's line ends and the Tailscale link,
//! and sending versions (back-off after a failed send, one version per quiet gap, sends that do
//! not clobber each other, a self-building host updating from the source sent to it).

#[test]
fn a_failed_send_waits_longer_each_time_up_to_six_hours() {
    let mins: Vec<u64> = (1..=8).map(|n| retry_after(n).as_secs() / 60).collect();
    assert_eq!(mins, [10, 20, 40, 80, 160, 320, 320, 320]);
    assert!(retry_after(99) <= Duration::from_secs(6 * 3600));
}


/// A host whose hubd got a new token still answers health, and refuses the old token on the
/// first real call; `token_accepted` is how the connection notices before using it.
#[tokio::test]
async fn a_token_the_host_refuses_is_noticed() {
    use axum::{Router, http::HeaderMap, routing::get};
    let app = Router::new().route(
        "/api/host",
        get(|headers: HeaderMap| async move {
            if headers.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer new") { StatusCode::OK } else { StatusCode::UNAUTHORIZED }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();
    let with = |token: Option<&str>| Conn { base: base.clone(), token: token.map(str::to_owned) };
    assert!(token_accepted(&client, &with(Some("new"))).await);
    assert!(!token_accepted(&client, &with(Some("old"))).await);
    assert!(!token_accepted(&client, &with(None)).await);
    // A host that does not answer is not said to have refused.
    let gone = Conn { base: "http://127.0.0.1:1".into(), token: None };
    assert!(token_accepted(&client, &gone).await);
}
use super::*;
use super::link::token_accepted;
use super::bundle::version_bundle;
use super::ship::{hubd_put, pushed_config, retry_after, source_put, without_test_step};

#[test]
fn finds_the_tailscale_sign_in_link() {
    let log = "Installing Tailscale\nStarting Tailscale\n\nTo authenticate, visit:\n\n\thttps://login.tailscale.com/a/1a2b3c4d\n\n";
    assert_eq!(login_url(log).as_deref(), Some("https://login.tailscale.com/a/1a2b3c4d"));
    assert_eq!(login_url("Starting Tailscale\n"), None);
}

fn sh(dir: &std::path::Path, script: &str, env: &[(&str, &std::path::Path)], input: Option<&[u8]>) -> String {
    use std::io::Write;
    // On Windows a bare `bash` can be WSL's, which does not understand these paths; Git's is.
    let git_bash = std::path::Path::new(r"C:\Program Files\Git\bin\bash.exe");
    let mut cmd = std::process::Command::new(if cfg!(windows) && git_bash.exists() { git_bash } else { std::path::Path::new("bash") });
    cmd.arg("-c").arg(script).current_dir(dir).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.unwrap_or_default()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{script} failed");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[tokio::test]
async fn a_host_that_builds_for_itself_updates_from_the_source_sent_to_it() {
    let tmp = tempfile::tempdir().unwrap();
    let pc = tmp.path().join("pc");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&pc).unwrap();
    let commit = "git -c user.name=t -c user.email=t@t commit --quiet --allow-empty -m";
    sh(&pc, &format!("git init --quiet -b main && {commit} one"), &[], None);
    let first = sh(&pc, "git rev-parse --short=12 HEAD", &[], None);
    // The host's clone, installed from GitHub before: its origin is unreachable from here.
    let src = data.join("hub").join("src");
    std::fs::create_dir_all(&src).unwrap();
    sh(&src, "git init --quiet && git remote add origin https://invalid.example/hub", &[], None);

    let xdg = [("XDG_DATA_HOME", data.as_path())];
    for version in [first.clone(), { sh(&pc, &format!("{commit} two"), &[], None); sh(&pc, "git rev-parse --short=12 HEAD", &[], None) }] {
        let (bundle, name) = version_bundle(&pc, &version).await.unwrap();
        assert!(!bundle.is_empty() && name.starts_with("refs/hub-ship/"));
        // The rest is what a Linux host runs; a Windows bash (WSL or Git's) is not one.
        if cfg!(windows) {
            continue;
        }
        sh(tmp.path(), &source_put(&name, "main"), &xdg, Some(&bundle));
        // As the updater does.
        sh(&src, "git fetch --quiet origin main", &[], None);
        assert_eq!(sh(&src, "git rev-parse --short=12 origin/main", &[], None), version);
    }
    // The PC's clone keeps only its own refs.
    assert_eq!(sh(&pc, "git for-each-ref --format='%(refname)'", &[], None), "refs/heads/main");
    assert!(!data.join("hub").join("source.bundle").exists());
}

#[test]
fn two_sends_of_hubd_to_one_host_do_not_clobber_each_other() {
    use std::io::Write;
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let bin = data.join("hub").join("bin");
    let xdg = [("XDG_DATA_HOME", data.as_path())];
    let git_bash = std::path::Path::new(r"C:\Program Files\Git\bin\bash.exe");
    let bash = if cfg!(windows) && git_bash.exists() { git_bash } else { std::path::Path::new("bash") };
    // The first send is still writing hubd...
    let mut first = std::process::Command::new(bash)
        .arg("-c")
        .arg(hubd_put("abc"))
        .env("XDG_DATA_HOME", &data)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = first.stdin.take().unwrap();
    stdin.write_all(b"first ").unwrap();
    stdin.flush().unwrap();
    let writing = || std::fs::read_dir(&bin).into_iter().flatten().flatten().any(|e| e.file_name().to_string_lossy().contains(".partial") && e.path().join("hubd").exists());
    while !writing() {
        std::thread::sleep(Duration::from_millis(20));
    }
    // ...when a second one comes and goes.
    sh(tmp.path(), &hubd_put("abc"), &xdg, Some(b"second"));
    assert_eq!(std::fs::read_to_string(bin.join("abc").join("hubd")).unwrap(), "second");
    stdin.write_all(b"half").unwrap();
    drop(stdin);
    let out = first.wait_with_output().unwrap();
    assert!(out.status.success(), "the first send failed: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(bin.join("abc").join("hubd")).unwrap(), "first half");
    assert_eq!(std::fs::read_to_string(bin.join("abc").join("VERSION")).unwrap(), "abc");
    assert_eq!(std::fs::read_to_string(data.join("hub").join("pushed")).unwrap(), "abc");
    let names: Vec<String> = std::fs::read_dir(&bin).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["abc"]);
}

#[test]
fn a_send_of_nothing_leaves_the_host_as_it_was() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let git_bash = std::path::Path::new(r"C:\Program Files\Git\bin\bash.exe");
    let bash = if cfg!(windows) && git_bash.exists() { git_bash } else { std::path::Path::new("bash") };
    let status = std::process::Command::new(bash).arg("-c").arg(hubd_put("abc")).env("XDG_DATA_HOME", &data).stdin(std::process::Stdio::null()).status().unwrap();
    assert!(!status.success());
    let bin = data.join("hub").join("bin");
    assert_eq!(std::fs::read_dir(&bin).unwrap().count(), 0);
    assert!(!data.join("hub").join("pushed").exists());
}

#[test]
fn a_host_that_built_for_itself_keeps_its_settings_when_it_takes_builds() {
    let before = r#"{"update":{"enabled":true,"steps":[{"dir":"","command":["cargo","test"]}],"artifacts":["target/release/hubd"],"min_free_gb":3},"engines":{"x":{"command":"hermes acp"}}}"#;
    let after: serde_json::Value = serde_json::from_str(&pushed_config(before).unwrap()).unwrap();
    assert_eq!(after["update"], serde_json::json!({"enabled": true, "ask": false, "pushed": true}));
    assert_eq!(after["engines"]["x"]["command"], "hermes acp");
    let empty: serde_json::Value = serde_json::from_str(&pushed_config("").unwrap()).unwrap();
    assert_eq!(empty["update"]["pushed"], true);
}

#[test]
fn a_host_that_builds_drops_the_tests_the_pc_already_ran() {
    let old = r#"{"update":{"steps":[{"dir":"","command":["cargo","test","-p","hub-core"]},{"dir":"","command":["cargo","build","--release","-p","hubd"]}]},"engines":{}}"#;
    let new: serde_json::Value = serde_json::from_str(&without_test_step(old).unwrap()).unwrap();
    assert_eq!(new["update"]["steps"].as_array().unwrap().len(), 1);
    assert_eq!(new["update"]["steps"][0]["command"][1], "build");
    assert!(new.get("engines").is_some());
    assert!(without_test_step(&new.to_string()).is_none());
    assert!(without_test_step("not json").is_none());
}

#[test]
fn the_host_parameter_is_read_and_dropped() {
    let uri: Uri = "/api/project?path=%2Fsrv%2Fgame&host=tin%20can&rel=".parse().unwrap();
    assert_eq!(host_param(&uri).as_deref(), Some("tin can"));
    assert_eq!(without_host_param(&uri), "/api/project?path=%2Fsrv%2Fgame&rel=");
    let uri: Uri = "/api/threads?host=tincan".parse().unwrap();
    assert_eq!(without_host_param(&uri), "/api/threads");
    assert_eq!(host_param(&"/api/threads".parse().unwrap()), None);
}

#[test]
fn the_installer_goes_out_with_unix_line_ends() {
    assert!(!install_script().contains('\r'));
    assert!(install_script().starts_with("#!/usr/bin/env bash\n"));
}

#[test]
fn the_check_says_what_the_host_lacks() {
    let lacks = read_check("package git\npackage build-essential\npackage `rm -rf`\nlinger\n");
    assert_eq!(lacks.packages, ["git", "build-essential"]);
    assert!(lacks.linger && !lacks.sudo && lacks.needs_root());
    assert_eq!(lacks.sudo_for("clanker"), "Hub needs sudo on clanker once to install git, build-essential and let hubd run without a login.");
    let lacks = read_check("problem systemd is missing\nsudo\n");
    assert_eq!(lacks.problems, ["systemd is missing"]);
    assert!(lacks.sudo && !lacks.needs_root());
    assert_eq!(read_check(""), Lacks::default());
}

#[test]
fn the_probe_says_what_the_host_has() {
    assert_eq!(read_probe("missing\n"), Probe::Missing);
    assert_eq!(read_probe("installed\nhubd abc is running\n"), Probe::Installed);
    assert_eq!(read_probe("installing\nBuilding hubd\n"), Probe::Installing("Building hubd".into()));
    assert_eq!(read_probe("installing\n"), Probe::Installing("starting".into()));
    assert_eq!(read_probe("failed\ncargo is missing.\n"), Probe::Failed("cargo is missing.".into()));
    assert_eq!(read_probe(""), Probe::Unknown);
}

#[test]
fn the_url_comes_before_the_tunnel() {
    let config = HostConfig {
        name: "tincan".into(),
        ssh: Some("tincan".into()),
        url: Some("https://tincan.tail1.ts.net".into()),
        port: 7878,
        token: None,
        push_quiet_mins: None,
    };
    assert_eq!(routes(&config), ["https://tincan.tail1.ts.net", "ssh tincan"]);
}

#[test]
fn a_host_with_a_quiet_gap_gets_at_most_one_version_per_gap() {
    use super::ship::quiet_left;
    let min = 60_000;
    let now = 1_000 * min;
    // No gap set: today's behaviour, always now.
    assert_eq!(quiet_left(None, Some(now - min), true, now), None);
    // Sent 10 minutes ago with a 30-minute gap: 20 minutes to go, and the check skips it.
    assert_eq!(quiet_left(Some(30), Some(now - 10 * min), true, now), Some(20));
    // The gap has passed: sent.
    assert_eq!(quiet_left(Some(30), Some(now - 30 * min), true, now), None);
    // A host never sent to, and one with no version at all: at once. (Update now, `Remotes::update`,
    // does not ask the gap.)
    assert_eq!(quiet_left(Some(30), None, true, now), None);
    assert_eq!(quiet_left(Some(30), Some(now - min), false, now), None);
    // A clock ahead of now (set on another clock) holds no longer than the gap.
    assert_eq!(quiet_left(Some(30), Some(now + 999 * min), true, now), Some(30));
}

#[test]
fn only_thread_answers_are_stamped() {
    assert!(answers_with_thread(&Method::POST, "/api/threads"));
    assert!(answers_with_thread(&Method::GET, "/api/threads/abc"));
    assert!(answers_with_thread(&Method::PATCH, "/api/threads/abc"));
    assert!(!answers_with_thread(&Method::GET, "/api/threads"));
    assert!(!answers_with_thread(&Method::GET, "/api/threads/abc/events"));
    assert!(!answers_with_thread(&Method::DELETE, "/api/threads/abc"));
}
