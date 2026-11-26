//! Terminals inside Hub. A machine's page opens a shell, a root shell or an engine's CLI there in
//! a pseudo-terminal this core runs (ConPTY on Windows), logged in over SSH like the window
//! `machine::open` would open. The app draws it with xterm.js over a WebSocket
//! (`GET /api/terminals/{id}/ws`): what the terminal prints goes out as binary frames, keys come
//! back as binary frames, and `{"resize": [cols, rows]}` as text.
//!
//! A terminal outlives the page that shows it: it keeps the last of its output, so going back to
//! the machine's page shows it as it was and it carries on. It ends when what runs in it exits
//! (it stays listed, with its exit code, until closed) or when it is closed.
//!
//! **As administrator** (Windows). A process that is not elevated cannot start an elevated child
//! of its own or reach the one Windows starts, so the pseudo-terminal cannot live in this core.
//! Instead `Terminals::start_hosted` makes a terminal with no process, and the person is asked
//! (UAC) to start `hubd admin-host`, which runs elevated, owns the pseudo-terminal and the shell
//! and connects back to `GET /api/terminals/{id}/host`. What it prints goes into the same backlog
//! and out to the same pages; keys, sizes and "close" go the other way. Whoever may type in a
//! terminal may type in this one: a phone's token may not (`phone::may`), and the host presents
//! the connection token plus a secret that only this terminal's launch was given.

use crate::locked::ResultExt;
use crate::locked::LockExt;
use crate::locked::Ignore;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::tungstenite;

use crate::model::{MachineOpen, TerminalSession};

/// How much of a terminal's output is kept for a page that opens it again.
const BACKLOG: usize = 512 * 1024;
/// At most this many terminals at once, so a page stuck reopening cannot fill the machine.
const MAX: usize = 24;

/// What a terminal sends the pages that show it.
#[derive(Debug, Clone)]
pub enum Out {
    Bytes(Arc<[u8]>),
    Exit(i32),
}

/// The first argument of `hubd admin-host`, the elevated helper that runs a terminal as
/// administrator.
pub const HOST_ARG: &str = "admin-host";
/// How long a helper has to connect after Windows started it.
pub const HOST_WAITS: std::time::Duration = std::time::Duration::from_secs(30);

/// What this core asks of the helper that runs a hosted terminal.
pub enum Control {
    Keys(Vec<u8>),
    Resize(u16, u16),
    Kill,
}

/// A terminal whose process runs in an elevated helper (see the top of this file).
struct Hosted {
    secret: String,
    to_host: mpsc::UnboundedSender<Control>,
    /// Taken by the one helper that connects.
    from_pages: Mutex<Option<mpsc::UnboundedReceiver<Control>>>,
    connected: std::sync::atomic::AtomicBool,
}

pub struct Terminal {
    pub info: Mutex<TerminalSession>,
    backlog: Mutex<VecDeque<u8>>,
    out: broadcast::Sender<Out>,
    input: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    /// Dropped once what runs in it exits: on Windows the output only ends when ConPTY closes.
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    killer: Mutex<Option<Box<dyn portable_pty::ChildKiller + Send + Sync>>>,
    hosted: Option<Hosted>,
}

impl Terminal {
    /// What it printed so far and what it prints from now on, with no gap between the two.
    pub fn follow(&self) -> (Vec<u8>, broadcast::Receiver<Out>, Option<i32>) {
        let backlog = self.backlog.locked();
        let rx = self.out.subscribe();
        let exit = self.info.locked().exit;
        (backlog.iter().copied().collect(), rx, exit)
    }

    pub fn write(&self, bytes: Vec<u8>) {
        if let Some(h) = &self.hosted {
            h.to_host.send(Control::Keys(bytes)).ignore("the helper that hosts the terminal may have ended");
        } else if let Some(input) = &self.input {
            input.send(bytes).ignore("the shell may have exited");
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        if let Some(h) = &self.hosted {
            h.to_host.send(Control::Resize(cols, rows)).ignore("the helper that hosts the terminal may have ended");
        } else if let Some(master) = self.master.locked().as_ref() {
            master.resize(size(cols, rows)).warn_on_err("resize the terminal");
        }
    }

    fn kill(&self) {
        if let Some(h) = &self.hosted {
            h.to_host.send(Control::Kill).ignore("the helper that hosts the terminal may have ended");
        } else if let Some(killer) = self.killer.locked().as_mut() {
            killer.kill().ignore("the shell may have exited already");
        }
    }

    /// Output from the process: kept for pages that open the terminal later, and sent to those
    /// that show it now.
    fn push(&self, bytes: &[u8]) {
        let mut backlog = self.backlog.locked();
        backlog.extend(bytes);
        let over = backlog.len().saturating_sub(BACKLOG);
        backlog.drain(..over);
        self.out.send(Out::Bytes(Arc::from(bytes))).ignore("nobody may be watching; the backlog keeps it");
    }

    /// What ran in it has ended. Only the first end counts.
    fn finish(&self, code: i32) {
        let _guard = self.backlog.locked();
        let mut info = self.info.locked();
        if info.exit.is_none() {
            info.exit = Some(code);
            self.out.send(Out::Exit(code)).ignore("nobody may be watching; info keeps the exit code");
        }
    }
}

#[derive(Default)]
pub struct Terminals {
    open: Mutex<HashMap<String, Arc<Terminal>>>,
}

impl Terminals {
    pub fn list(&self) -> Vec<TerminalSession> {
        let mut list: Vec<TerminalSession> = self.open.locked().values().map(|t| t.info.locked().clone()).collect();
        list.sort_by_key(|t| t.started_at);
        list
    }

    pub fn get(&self, id: &str) -> Option<Arc<Terminal>> {
        self.open.locked().get(id).cloned()
    }

    /// Ends it if it still runs, and forgets it.
    pub fn close(&self, id: &str) -> bool {
        let gone = self.open.locked().remove(id);
        if let Some(t) = &gone {
            t.kill();
        }
        gone.is_some()
    }

    fn make_room(&self) -> Result<()> {
        let mut open = self.open.locked();
        if open.len() >= MAX {
            // Make room by forgetting the oldest that has ended.
            let ended = open.values().filter(|t| t.info.locked().exit.is_some()).min_by_key(|t| t.info.locked().started_at).map(|t| t.info.locked().id.clone());
            match ended {
                Some(id) => {
                    open.remove(&id);
                }
                None => bail!("{MAX} terminals are open already; close one first"),
            }
        }
        Ok(())
    }

    /// Starts `argv` (empty: the default shell) in the home folder. `thread` is the thread it
    /// takes over, for `MachineOpen::Attach`.
    #[allow(clippy::too_many_arguments)]
    pub fn start(&self, machine: &str, what: MachineOpen, title: String, argv: &[String], thread: Option<String>, cols: u16, rows: u16) -> Result<TerminalSession> {
        self.make_room()?;
        let pair = native_pty_system().openpty(size(cols, rows)).context("opening a terminal")?;
        let mut cmd = command(argv);
        cmd.cwd(crate::folders::home());
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let child = pair.slave.spawn_command(cmd).with_context(|| format!("starting {}", shown(argv)))?;
        // The terminal's own end stays with the child only, so its output ends when it exits.
        drop(pair.slave);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;

        let info = TerminalSession {
            id: uuid::Uuid::new_v4().simple().to_string(),
            machine: machine.to_owned(),
            what,
            title,
            command: shown(argv),
            started_at: crate::model::now_ms(),
            exit: None,
            thread,
            admin: None,
        };
        let (out, _) = broadcast::channel(1024);
        let (input, keys) = std::sync::mpsc::channel::<Vec<u8>>();
        let term = Arc::new(Terminal {
            info: Mutex::new(info.clone()),
            backlog: Mutex::new(VecDeque::new()),
            out,
            input: Some(input),
            master: Mutex::new(Some(pair.master)),
            killer: Mutex::new(Some(killer)),
            hosted: None,
        });

        // Keys go through a thread of their own: a write blocks while the program does not read.
        std::thread::Builder::new().name("pty-in".into()).spawn(move || {
            for bytes in keys {
                if writer.write_all(&bytes).and_then(|()| writer.flush()).is_err() {
                    return;
                }
            }
        })?;

        let t = term.clone();
        std::thread::Builder::new().name("pty-out".into()).spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            loop {
                let n = match reader.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => n,
                };
                t.push(&buf[..n]);
            }
        })?;

        let t = term.clone();
        let mut child = child;
        std::thread::Builder::new().name("pty-wait".into()).spawn(move || {
            let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
            // Let the last output through before ConPTY closes.
            std::thread::sleep(std::time::Duration::from_millis(300));
            t.master.locked().take();
            t.finish(code);
        })?;

        self.open.locked().insert(info.id.clone(), term);
        Ok(info)
    }

    /// A terminal as administrator: listed and shown like any, with no process yet. The secret is
    /// for the helper that Windows starts elevated (`hubd admin-host`), which runs `argv`.
    pub fn start_hosted(&self, machine: &str, what: MachineOpen, title: String, argv: &[String], cols: u16, rows: u16) -> Result<(TerminalSession, String)> {
        self.make_room()?;
        let info = TerminalSession {
            id: uuid::Uuid::new_v4().simple().to_string(),
            machine: machine.to_owned(),
            what,
            title,
            command: shown(argv),
            started_at: crate::model::now_ms(),
            exit: None,
            thread: None,
            admin: Some(true),
        };
        let secret = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let (to_host, from_pages) = mpsc::unbounded_channel();
        // The size the helper starts with is asked for first, ahead of the keys typed meanwhile.
        to_host.send(Control::Resize(cols, rows)).ignore("from_pages is held just above");
        let (out, _) = broadcast::channel(1024);
        let term = Arc::new(Terminal {
            info: Mutex::new(info.clone()),
            backlog: Mutex::new(VecDeque::new()),
            out,
            input: None,
            master: Mutex::new(None),
            killer: Mutex::new(None),
            hosted: Some(Hosted { secret: secret.clone(), to_host, from_pages: Mutex::new(Some(from_pages)), connected: std::sync::atomic::AtomicBool::new(false) }),
        });
        self.open.locked().insert(info.id.clone(), term);
        Ok((info, secret))
    }

    /// The helper did not start (it was refused, or never came): say so in the terminal and end it.
    pub fn host_failed(&self, id: &str, why: &str) {
        if let Some(t) = self.get(id) {
            t.push(format!("\r\n{why}\r\n").as_bytes());
            t.finish(-1);
        }
    }

    /// Called `HOST_WAITS` after Windows started the helper: ends the terminal if it never came.
    pub fn host_never_came(&self, id: &str) {
        let waiting = self.get(id).is_some_and(|t| t.hosted.as_ref().is_some_and(|h| !h.connected.load(std::sync::atomic::Ordering::SeqCst)));
        if waiting {
            self.host_failed(id, "The administrator helper did not start, so there is no shell here. Close this and try again.");
        }
    }

    /// Lets the helper in, once, when it shows the terminal's secret.
    pub fn take_host(&self, id: &str, secret: &str) -> Option<(Arc<Terminal>, mpsc::UnboundedReceiver<Control>)> {
        let term = self.get(id)?;
        let hosted = term.hosted.as_ref()?;
        if hosted.secret != secret || term.info.locked().exit.is_some() {
            return None;
        }
        let rx = hosted.from_pages.locked().take()?;
        hosted.connected.store(true, std::sync::atomic::Ordering::SeqCst);
        Some((term, rx))
    }
}

/// Speaks with the helper on its WebSocket: what it prints in binary frames, `{"exit": code}`
/// when its process ended; keys go back in binary frames, `{"resize": [cols, rows]}` and
/// `{"kill": true}` as text.
pub async fn serve_host(term: Arc<Terminal>, mut from_pages: mpsc::UnboundedReceiver<Control>, mut socket: WebSocket) {
    loop {
        tokio::select! {
            control = from_pages.recv() => {
                let sent = match control {
                    Some(Control::Keys(bytes)) => socket.send(Message::Binary(bytes.into())).await,
                    Some(Control::Resize(cols, rows)) => socket.send(Message::Text(serde_json::json!({ "resize": [cols, rows] }).to_string().into())).await,
                    Some(Control::Kill) => {
                        socket.send(Message::Text(serde_json::json!({ "kill": true }).to_string().into())).await.warn_on_err("tell the helper to kill");
                        break;
                    }
                    None => break,
                };
                if sent.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Binary(bytes))) => term.push(&bytes),
                Some(Ok(Message::Text(text))) => {
                    if let Some(code) = serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| v["exit"].as_i64()) {
                        term.finish(code as i32);
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
    // A helper that went without saying how it ended.
    term.finish(-1);
}

/// `hubd admin-host <bind> <data folder> <terminal> <secret> <cols> <rows> [-- program args…]`:
/// runs elevated (Windows asked the person), runs the program in a pseudo-terminal of its own and
/// hands it to the core that asked. Ends with the program, or when the core closes the terminal.
/// `token` reads the core's API token from its data folder (hubd passes `auth::token`), so this
/// module does not name `auth`.
pub async fn host(args: &[String], token: impl FnOnce(&std::path::Path) -> Result<String>) -> Result<i32> {
    let (head, argv) = match args.iter().position(|a| a == "--") {
        Some(at) => (&args[..at], &args[at + 1..]),
        None => (args, &args[args.len()..]),
    };
    let [bind, data_dir, id, secret, cols, rows] = head else {
        bail!("usage: hubd admin-host <bind> <data folder> <terminal> <secret> <cols> <rows> [-- program args…]");
    };
    let token = token(std::path::Path::new(data_dir))?;
    run_host(&format!("http://{bind}"), Some(&token), id, secret, cols.parse().unwrap_or(100), rows.parse().unwrap_or(30), argv).await
}

/// What the helper's loop is told by its threads.
enum Ran {
    Output(Vec<u8>),
    Exited(i32),
}

type HostSocket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Opens the socket a hosted terminal talks to Hub over.
async fn connect_host(base: &str, token: Option<&str>, id: &str, secret: &str) -> Result<HostSocket> {
    use tungstenite::client::IntoClientRequest;
    let url = format!("{}/api/terminals/{id}/host?secret={secret}", base.replacen("http", "ws", 1));
    let mut request = url.into_client_request()?;
    if let Some(token) = token {
        request.headers_mut().insert(axum::http::header::AUTHORIZATION, axum::http::HeaderValue::from_str(&format!("Bearer {token}"))?);
    }
    let (ws, _) = tokio::time::timeout(std::time::Duration::from_secs(20), tokio_tungstenite::connect_async(request))
        .await
        .map_err(|_| anyhow!("Hub did not answer"))?
        .context("connecting to Hub")?;
    Ok(ws)
}

/// Starts the threads that carry a hosted terminal's output out, its keys in, and its end; the
/// returned sender takes what is typed.
fn host_threads(
    mut reader: Box<dyn Read + Send>,
    mut writer: Box<dyn Write + Send>,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    ran_tx: mpsc::UnboundedSender<Ran>,
) -> Result<std::sync::mpsc::Sender<Vec<u8>>> {
    let tx = ran_tx.clone();
    std::thread::Builder::new().name("host-out".into()).spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    if tx.send(Ran::Output(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
            }
        }
    })?;
    let (keys, typed) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::Builder::new().name("host-in".into()).spawn(move || {
        for bytes in typed {
            if writer.write_all(&bytes).and_then(|()| writer.flush()).is_err() {
                return;
            }
        }
    })?;
    std::thread::Builder::new().name("host-wait".into()).spawn(move || {
        let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
        // Let the last output through before ConPTY closes.
        std::thread::sleep(std::time::Duration::from_millis(300));
        ran_tx.send(Ran::Exited(code)).ignore("the link to the core may have closed first");
    })?;
    Ok(keys)
}

/// The helper's side of a hosted terminal, against the core at `base`.
pub async fn run_host(base: &str, token: Option<&str>, id: &str, secret: &str, cols: u16, rows: u16, argv: &[String]) -> Result<i32> {
    let mut ws = connect_host(base, token, id, secret).await?;

    let pair = native_pty_system().openpty(size(cols, rows)).context("opening a terminal")?;
    let mut cmd = command(argv);
    cmd.cwd(crate::folders::home());
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    let child = pair.slave.spawn_command(cmd).with_context(|| format!("starting {}", shown(argv)))?;
    drop(pair.slave);
    let mut killer = child.clone_killer();
    let reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    let mut master = Some(pair.master);

    let (ran_tx, mut ran) = mpsc::unbounded_channel::<Ran>();
    let keys = host_threads(reader, writer, child, ran_tx)?;

    let mut code = -1;
    loop {
        tokio::select! {
            got = ran.recv() => match got {
                Some(Ran::Output(bytes)) => {
                    if ws.send(tungstenite::Message::Binary(bytes.into())).await.is_err() {
                        killer.kill().ignore("the shell may have exited already");
                        return Ok(-1);
                    }
                }
                Some(Ran::Exited(c)) => {
                    code = c;
                    break;
                }
                None => break,
            },
            msg = ws.next() => match msg {
                Some(Ok(tungstenite::Message::Binary(bytes))) => {
                    keys.send(bytes.to_vec()).ignore("the shell may have exited");
                }
                Some(Ok(tungstenite::Message::Text(text))) => {
                    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
                    if let Some(a) = v["resize"].as_array()
                        && let (Some(c), Some(r)) = (a.first().and_then(|x| x.as_u64()), a.get(1).and_then(|x| x.as_u64()))
                        && let Some(m) = &master
                    {
                        m.resize(size(c as u16, r as u16)).warn_on_err("resize the terminal");
                    }
                    if v["kill"].as_bool() == Some(true) {
                        killer.kill().ignore("the shell may have exited already");
                        return Ok(-1);
                    }
                }
                // The core went away or closed the terminal: nothing here should outlive it.
                Some(Ok(tungstenite::Message::Close(_))) | Some(Err(_)) | None => {
                    killer.kill().ignore("the shell may have exited already");
                    return Ok(-1);
                }
                Some(Ok(_)) => {}
            },
        }
    }
    // The program ended: let go of the pseudo-terminal, send what is left, then how it ended.
    master.take();
    while let Ok(Some(Ran::Output(bytes))) = tokio::time::timeout(std::time::Duration::from_millis(300), ran.recv()).await {
        ws.send(tungstenite::Message::Binary(bytes.into())).await.warn_on_err("writing to the connection");
    }
    ws.send(tungstenite::Message::Text(serde_json::json!({ "exit": code }).to_string().into())).await.warn_on_err("writing to the connection");
    ws.close(None).await.warn_on_err("close the connection");
    Ok(code)
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { cols: cols.clamp(10, 1000), rows: rows.clamp(4, 500), pixel_width: 0, pixel_height: 0 }
}

/// The program to run: `argv`, or this machine's shell when it is empty.
fn command(argv: &[String]) -> CommandBuilder {
    if let Some((program, args)) = argv.split_first() {
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        return cmd;
    }
    if cfg!(windows) {
        let mut cmd = CommandBuilder::new("powershell.exe");
        cmd.arg("-NoLogo");
        return cmd;
    }
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/sh".to_owned());
    let mut cmd = CommandBuilder::new(shell);
    cmd.arg("-l");
    cmd
}

fn shown(argv: &[String]) -> String {
    if argv.is_empty() {
        return "shell".to_owned();
    }
    argv.iter()
        .map(|a| if !a.is_empty() && a.chars().all(|c| c.is_ascii_alphanumeric() || "-_./@:=".contains(c)) { a.clone() } else { format!("'{a}'") })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::Duration;

    async fn until_exit(rx: &mut broadcast::Receiver<Out>, seen: &mut Vec<u8>) -> i32 {
        loop {
            match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.expect("the terminal ended in time") {
                Ok(Out::Bytes(b)) => seen.extend_from_slice(&b),
                Ok(Out::Exit(code)) => return code,
                Err(e) => panic!("{e}"),
            }
        }
    }

    #[tokio::test]
    async fn a_terminal_runs_a_program_and_keeps_what_it_printed() {
        let terms = Terminals::default();
        let argv = ["sh".to_owned(), "-c".to_owned(), "printf 'hello from a pty'; exit 3".to_owned()];
        let info = terms.start("here", MachineOpen::Shell, "here".into(), &argv, None, 80, 24).unwrap();
        let t = terms.get(&info.id).unwrap();
        let (mut seen, mut rx, _) = t.follow();
        let code = until_exit(&mut rx, &mut seen).await;
        assert_eq!(code, 3);
        assert!(String::from_utf8_lossy(&seen).contains("hello from a pty"));
        // A page that opens it again gets the same output, and its end.
        let (again, _, exit) = t.follow();
        assert!(String::from_utf8_lossy(&again).contains("hello from a pty"));
        assert_eq!(exit, Some(3));
        assert_eq!(terms.list()[0].exit, Some(3));
        assert!(terms.close(&info.id));
        assert!(terms.list().is_empty());
    }

    #[tokio::test]
    async fn keys_reach_the_program() {
        let terms = Terminals::default();
        let argv = ["sh".to_owned(), "-c".to_owned(), "read line; echo \"got $line\"".to_owned()];
        let info = terms.start("here", MachineOpen::Shell, "here".into(), &argv, None, 80, 24).unwrap();
        let t = terms.get(&info.id).unwrap();
        let (mut seen, mut rx, _) = t.follow();
        t.resize(120, 40);
        t.write(b"typed\r".to_vec());
        assert_eq!(until_exit(&mut rx, &mut seen).await, 0);
        assert!(String::from_utf8_lossy(&seen).contains("got typed"), "{}", String::from_utf8_lossy(&seen));
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

    #[tokio::test]
    async fn a_terminal_run_by_a_helper_shows_its_output_takes_keys_and_ends() {
        let terms = Arc::new(Terminals::default());
        let base = serve_hosts(terms.clone()).await;
        let argv = ["sh".to_owned(), "-c".to_owned(), "read line; echo \"got $line\"; exit 4".to_owned()];
        let (info, secret) = terms.start_hosted("here", MachineOpen::Shell, "here".into(), &argv, 80, 24).unwrap();
        assert_eq!(info.admin, Some(true));
        let t = terms.get(&info.id).unwrap();
        // Typed before the helper is there: it arrives once the helper is.
        t.write(b"early\r".to_vec());
        let (mut seen, mut rx, _) = t.follow();
        let helper = tokio::spawn({
            let (base, id, secret, argv) = (base.clone(), info.id.clone(), secret.clone(), argv.clone());
            async move { run_host(&base, None, &id, &secret, 80, 24, &argv).await.unwrap() }
        });
        let code = until_exit(&mut rx, &mut seen).await;
        assert_eq!(code, 4);
        assert!(String::from_utf8_lossy(&seen).contains("got early"), "{}", String::from_utf8_lossy(&seen));
        assert_eq!(helper.await.unwrap(), 4);
        assert_eq!(terms.list()[0].exit, Some(4));
    }

    #[tokio::test]
    async fn closing_a_hosted_terminal_ends_the_helpers_program() {
        let terms = Arc::new(Terminals::default());
        let base = serve_hosts(terms.clone()).await;
        let argv = ["sleep".to_owned(), "30".to_owned()];
        let (info, secret) = terms.start_hosted("here", MachineOpen::Shell, "here".into(), &argv, 80, 24).unwrap();
        let helper = tokio::spawn({
            let (base, id) = (base.clone(), info.id.clone());
            async move { run_host(&base, None, &id, &secret, 80, 24, &argv).await }
        });
        // Wait until the helper has connected.
        for _ in 0..100 {
            if terms.get(&info.id).unwrap().hosted.as_ref().unwrap().connected.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(terms.close(&info.id));
        let ended = tokio::time::timeout(Duration::from_secs(10), helper).await.expect("the helper ended in time");
        assert!(ended.unwrap().is_ok());
    }

    #[tokio::test]
    async fn a_helper_needs_the_secret_and_only_one_gets_in() {
        let terms = Arc::new(Terminals::default());
        let (info, secret) = terms.start_hosted("here", MachineOpen::Shell, "here".into(), &[], 80, 24).unwrap();
        assert!(terms.take_host(&info.id, "wrong").is_none());
        assert!(terms.take_host("nothing", &secret).is_none());
        assert!(terms.take_host(&info.id, &secret).is_some());
        assert!(terms.take_host(&info.id, &secret).is_none());
        // Never came: it ends, saying so.
        let (other, _) = terms.start_hosted("here", MachineOpen::Shell, "here".into(), &[], 80, 24).unwrap();
        terms.host_never_came(&other.id);
        let (shown, _, exit) = terms.get(&other.id).unwrap().follow();
        assert_eq!(exit, Some(-1));
        assert!(String::from_utf8_lossy(&shown).contains("did not start"));
    }

    /// A terminal with no process, to try the bookkeeping on.
    fn bare(terms: &Terminals) -> (TerminalSession, Arc<Terminal>) {
        let (info, _) = terms.start_hosted("here", MachineOpen::Shell, "t".into(), &[], 80, 24).unwrap();
        let t = terms.get(&info.id).unwrap();
        (info, t)
    }

    #[test]
    fn the_backlog_keeps_the_last_of_the_output_and_follow_has_no_gap() {
        let terms = Terminals::default();
        let (_, t) = bare(&terms);
        t.push(&vec![b'a'; BACKLOG]);
        t.push(b"tail");
        let (seen, mut rx, exit) = t.follow();
        assert_eq!(seen.len(), BACKLOG);
        assert!(seen.ends_with(b"tail"));
        assert!(seen.starts_with(b"a"));
        assert_eq!(exit, None);
        // Output after follow() goes to the receiver, not into what was returned.
        t.push(b"more");
        match rx.try_recv() {
            Ok(Out::Bytes(b)) => assert_eq!(&*b, b"more"),
            other => panic!("{other:?}"),
        }
        assert_eq!(seen.len(), BACKLOG);
    }

    #[test]
    fn only_the_first_end_counts() {
        let terms = Terminals::default();
        let (info, t) = bare(&terms);
        let (_, mut rx, _) = t.follow();
        t.finish(7);
        t.finish(0);
        assert_eq!(terms.list().iter().find(|s| s.id == info.id).unwrap().exit, Some(7));
        assert!(matches!(rx.try_recv(), Ok(Out::Exit(7))));
        assert!(rx.try_recv().is_err(), "a second end is not announced");
        assert_eq!(t.follow().2, Some(7));
    }

    #[test]
    fn a_full_machine_forgets_the_oldest_ended_terminal_and_refuses_when_all_run() {
        let terms = Terminals::default();
        let made: Vec<_> = (0..MAX).map(|_| bare(&terms)).collect();
        assert!(terms.make_room().is_err(), "all running: nothing to forget");
        assert!(terms.start_hosted("here", MachineOpen::Shell, "x".into(), &[], 80, 24).is_err());
        assert_eq!(terms.list().len(), MAX);

        // Two ended; the one that started first goes.
        for (n, (_, t)) in made.iter().enumerate() {
            t.info.locked().started_at = 1000 + n as i64;
        }
        made[5].1.finish(1);
        made[3].1.finish(0);
        terms.make_room().unwrap();
        assert!(terms.get(&made[3].0.id).is_none());
        assert!(terms.get(&made[5].0.id).is_some());
        assert_eq!(terms.list().len(), MAX - 1);
    }

    #[test]
    fn closing_an_unknown_terminal_says_so() {
        let terms = Terminals::default();
        let (info, _) = bare(&terms);
        assert!(!terms.close("nope"));
        assert!(terms.close(&info.id));
        assert!(!terms.close(&info.id));
    }

    #[test]
    fn sizes_are_kept_sane_and_commands_shown_quoted() {
        let s = size(0, 0);
        assert_eq!((s.cols, s.rows), (10, 4));
        let s = size(u16::MAX, u16::MAX);
        assert_eq!((s.cols, s.rows), (1000, 500));
        assert_eq!(shown(&[]), "shell");
        let argv = ["ssh".to_owned(), "-t".to_owned(), "my host".to_owned(), "".to_owned(), "a=b/c.d".to_owned()];
        assert_eq!(shown(&argv), "ssh -t 'my host' '' a=b/c.d");
    }

    #[tokio::test]
    async fn closing_ends_what_runs() {
        let terms = Terminals::default();
        let argv = ["sleep".to_owned(), "30".to_owned()];
        let info = terms.start("here", MachineOpen::Shell, "here".into(), &argv, None, 80, 24).unwrap();
        let t = terms.get(&info.id).unwrap();
        let (mut seen, mut rx, _) = t.follow();
        assert!(terms.close(&info.id));
        until_exit(&mut rx, &mut seen).await;
    }
}
