//! `pty` through its public face: starting programs, the size a page asks for, the elevated
//! helper's socket (what it sends back, how it ends) and the helper's command line. The unit
//! tests inside `pty.rs` cover the bookkeeping; these cover the seams around it.
#![cfg(unix)]

use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use hub_core::model::MachineOpen;
use hub_core::pty::{Out, Terminal, Terminals, run_host, serve_host};
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

fn sh(script: &str) -> Vec<String> {
    vec!["sh".into(), "-c".into(), script.into()]
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Reads until the terminal ends, returning its exit code and everything it printed.
async fn until_exit(t: &Terminal) -> (i32, String) {
    let (mut seen, mut rx, exit) = t.follow();
    if let Some(code) = exit {
        return (code, text(&seen));
    }
    loop {
        match tokio::time::timeout(Duration::from_secs(15), rx.recv()).await.expect("the terminal ended in time") {
            Ok(Out::Bytes(b)) => seen.extend_from_slice(&b),
            Ok(Out::Exit(code)) => return (code, text(&seen)),
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(e) => panic!("{e}"),
        }
    }
}

/// The core's side of a hosted terminal, as `api::terminal_host_socket` does it.
async fn serve_hosts(terms: Arc<Terminals>) -> String {
    use axum::extract::{Path, Query, State, WebSocketUpgrade};
    use axum::response::IntoResponse;
    #[derive(serde::Deserialize)]
    struct Secret {
        secret: String,
    }
    async fn host(State(terms): State<Arc<Terminals>>, Path(id): Path<String>, Query(q): Query<Secret>, ws: WebSocketUpgrade) -> axum::response::Response {
        let Some((term, rx)) = terms.take_host(&id, &q.secret) else {
            return axum::http::StatusCode::NOT_FOUND.into_response();
        };
        ws.on_upgrade(move |socket| serve_host(term, rx, socket))
    }
    let app = axum::Router::new().route("/api/terminals/{id}/host", axum::routing::get(host)).with_state(terms);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    base
}

/// A fake helper: connects the way `run_host` does, so the test decides what it says.
async fn connect(base: &str, id: &str, secret: &str) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("{}/api/terminals/{id}/host?secret={secret}", base.replacen("http", "ws", 1));
    tokio_tungstenite::connect_async(url).await.expect("the core let the helper in").0
}

#[tokio::test]
async fn a_program_that_does_not_exist_is_refused_and_leaves_nothing_listed() {
    let terms = Terminals::default();
    let err = terms.start("here", MachineOpen::Shell, "x".into(), &["/no/such/program-hub-test".into()], None, 80, 24).unwrap_err();
    assert!(format!("{err:#}").contains("/no/such/program-hub-test"), "{err:#}");
    assert!(terms.list().is_empty());
}

#[tokio::test]
async fn what_a_terminal_was_started_with_is_what_it_lists() {
    let terms = Terminals::default();
    let argv = vec!["sh".to_owned(), "-c".to_owned(), "exit 0".to_owned(), "my arg".to_owned()];
    let info = terms.start("box", MachineOpen::Attach, "Take over".into(), &argv, Some("thread-1".into()), 80, 24).unwrap();
    assert_eq!((info.machine.as_str(), info.title.as_str()), ("box", "Take over"));
    assert_eq!(info.thread.as_deref(), Some("thread-1"));
    assert_eq!(info.admin, None, "a terminal of its own is not an administrator one");
    assert_eq!(info.command, "sh -c 'exit 0' 'my arg'");
    assert_eq!(info.exit, None);
    assert!(info.started_at > 0);
    let listed = terms.list();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, info.id);
    until_exit(&terms.get(&info.id).unwrap()).await;
}

#[tokio::test]
async fn the_size_a_page_asks_for_reaches_the_program() {
    let terms = Terminals::default();
    // `stty size` prints "rows cols" once the program has had time to be resized.
    let info = terms.start("here", MachineOpen::Shell, "here".into(), &sh("sleep 1; stty size"), None, 80, 24).unwrap();
    let t = terms.get(&info.id).unwrap();
    t.resize(132, 43);
    let (code, seen) = until_exit(&t).await;
    assert_eq!(code, 0);
    assert!(seen.contains("43 132"), "{seen}");
}

#[tokio::test]
async fn it_starts_at_the_size_it_was_asked_for_and_keeps_it_sane() {
    let terms = Terminals::default();
    let info = terms.start("here", MachineOpen::Shell, "here".into(), &sh("stty size"), None, 100, 30).unwrap();
    let (_, seen) = until_exit(&terms.get(&info.id).unwrap()).await;
    assert!(seen.contains("30 100"), "{seen}");
    // Nonsense sizes are clamped instead of refused.
    let info = terms.start("here", MachineOpen::Shell, "here".into(), &sh("stty size"), None, 0, 0).unwrap();
    let (_, seen) = until_exit(&terms.get(&info.id).unwrap()).await;
    assert!(seen.contains("4 10"), "{seen}");
}

#[tokio::test]
async fn with_no_program_a_terminal_is_the_machines_shell() {
    let terms = Terminals::default();
    let info = terms.start("here", MachineOpen::Shell, "here".into(), &[], None, 80, 24).unwrap();
    assert_eq!(info.command, "shell");
    let t = terms.get(&info.id).unwrap();
    t.write(b"echo sum=$((40+2))\rexit 7\r".to_vec());
    let (code, seen) = until_exit(&t).await;
    assert_eq!(code, 7, "{seen}");
    assert!(seen.contains("sum=42"), "{seen}");
}

#[tokio::test]
async fn the_environment_says_what_kind_of_terminal_it_is() {
    let terms = Terminals::default();
    let info = terms.start("here", MachineOpen::Shell, "here".into(), &sh("echo term=$TERM color=$COLORTERM"), None, 80, 24).unwrap();
    let (_, seen) = until_exit(&terms.get(&info.id).unwrap()).await;
    assert!(seen.contains("term=xterm-256color color=truecolor"), "{seen}");
}

#[tokio::test]
async fn a_helper_that_failed_to_start_says_so_and_cannot_be_let_in() {
    let terms = Terminals::default();
    let (info, secret) = terms.start_hosted("here", MachineOpen::Shell, "admin".into(), &[], 80, 24).unwrap();
    assert_eq!(info.admin, Some(true));
    assert_eq!(info.thread, None);
    terms.host_failed(&info.id, "Windows said no");
    let t = terms.get(&info.id).unwrap();
    let (code, seen) = until_exit(&t).await;
    assert_eq!(code, -1);
    assert!(seen.contains("Windows said no"), "{seen}");
    assert!(terms.take_host(&info.id, &secret).is_none(), "an ended terminal takes no helper");
    // Saying so about a terminal that is gone is harmless.
    terms.host_failed("nothing", "x");
    terms.host_never_came("nothing");
}

#[tokio::test]
async fn a_helper_that_connected_is_not_called_late() {
    let terms = Terminals::default();
    let (info, secret) = terms.start_hosted("here", MachineOpen::Shell, "admin".into(), &[], 80, 24).unwrap();
    assert!(terms.take_host(&info.id, &secret).is_some());
    terms.host_never_came(&info.id);
    assert_eq!(terms.list()[0].exit, None, "it came, so it is not ended for being late");
}

#[tokio::test]
async fn what_the_helper_sends_is_shown_and_its_exit_code_is_kept() {
    let terms = Arc::new(Terminals::default());
    let base = serve_hosts(terms.clone()).await;
    let (info, secret) = terms.start_hosted("here", MachineOpen::Shell, "admin".into(), &[], 80, 24).unwrap();
    let t = terms.get(&info.id).unwrap();
    let mut ws = connect(&base, &info.id, &secret).await;
    ws.send(Message::Binary(b"from the helper".to_vec().into())).await.unwrap();
    ws.send(Message::Text(r#"{"exit": 5}"#.into())).await.unwrap();
    let (code, seen) = until_exit(&t).await;
    assert_eq!(code, 5);
    assert!(seen.contains("from the helper"), "{seen}");
    // A second helper with the same secret is turned away.
    let url = format!("{}/api/terminals/{}/host?secret={secret}", base.replacen("http", "ws", 1), info.id);
    assert!(tokio_tungstenite::connect_async(url).await.is_err());
}

#[tokio::test]
async fn a_helper_that_goes_away_without_a_word_ends_the_terminal_with_minus_one() {
    let terms = Arc::new(Terminals::default());
    let base = serve_hosts(terms.clone()).await;
    let (info, secret) = terms.start_hosted("here", MachineOpen::Shell, "admin".into(), &[], 80, 24).unwrap();
    let t = terms.get(&info.id).unwrap();
    let mut ws = connect(&base, &info.id, &secret).await;
    ws.send(Message::Binary(b"last words".to_vec().into())).await.unwrap();
    drop(ws);
    let (code, seen) = until_exit(&t).await;
    assert_eq!(code, -1);
    assert!(seen.contains("last words"), "{seen}");
}

#[tokio::test]
async fn keys_size_and_a_close_are_passed_on_to_the_helper_in_order() {
    let terms = Arc::new(Terminals::default());
    let base = serve_hosts(terms.clone()).await;
    let (info, secret) = terms.start_hosted("here", MachineOpen::Shell, "admin".into(), &[], 90, 33).unwrap();
    let t = terms.get(&info.id).unwrap();
    t.write(b"abc".to_vec());
    t.resize(120, 40);
    let mut ws = connect(&base, &info.id, &secret).await;
    let mut got = Vec::new();
    while got.len() < 3 {
        match tokio::time::timeout(Duration::from_secs(10), ws.next()).await.expect("the core sent in time") {
            Some(Ok(Message::Close(_))) | None => break,
            Some(Ok(m)) => got.push(m),
            Some(Err(e)) => panic!("{e}"),
        }
    }
    // The size it started with comes first, then what was typed meanwhile, then the later size.
    assert_eq!(got[0], Message::Text(r#"{"resize":[90,33]}"#.into()));
    assert_eq!(got[1], Message::Binary(b"abc".to_vec().into()));
    assert_eq!(got[2], Message::Text(r#"{"resize":[120,40]}"#.into()));
    assert!(terms.close(&info.id));
    let mut kill = None;
    while let Ok(Some(Ok(m))) = tokio::time::timeout(Duration::from_secs(10), ws.next()).await {
        if let Message::Text(t) = m {
            kill = Some(t.to_string());
            break;
        }
    }
    assert_eq!(kill.as_deref(), Some(r#"{"kill":true}"#));
}

#[tokio::test]
async fn the_helper_gives_the_program_the_size_the_core_asked_for() {
    let terms = Arc::new(Terminals::default());
    let base = serve_hosts(terms.clone()).await;
    let argv = sh("sleep 1; stty size");
    let (info, secret) = terms.start_hosted("here", MachineOpen::Shell, "admin".into(), &argv, 110, 35).unwrap();
    let t = terms.get(&info.id).unwrap();
    let helper = tokio::spawn({
        let (base, id) = (base.clone(), info.id.clone());
        async move { run_host(&base, None, &id, &secret, 80, 24, &argv).await.unwrap() }
    });
    let (code, seen) = until_exit(&t).await;
    assert_eq!(code, 0);
    assert!(seen.contains("35 110"), "{seen}");
    assert_eq!(helper.await.unwrap(), 0);
}

#[tokio::test]
async fn a_helper_without_a_core_to_answer_gives_up_with_an_error() {
    // Nothing listens on port 1.
    let err = run_host("http://127.0.0.1:1", None, "id", "secret", 80, 24, &sh("true")).await.unwrap_err();
    assert!(format!("{err:#}").contains("Hub"), "{err:#}");
}

#[tokio::test]
async fn the_helpers_command_line_must_be_complete() {
    for args in [vec![], vec!["127.0.0.1:1".to_owned()], vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()]] {
        let err = hub_core::pty::host(&args, hub_core::auth::token).await.unwrap_err();
        assert!(err.to_string().contains("usage: hubd admin-host"), "{err}");
    }
    // Arguments after `--` belong to the program, not to the helper.
    let args: Vec<String> = ["a", "b", "c", "d", "e", "--", "prog"].iter().map(|s| s.to_string()).collect();
    let err = hub_core::pty::host(&args, hub_core::auth::token).await.unwrap_err();
    assert!(err.to_string().contains("usage: hubd admin-host"), "{err}");
}
