//! Engines that outlive the core.
//!
//! An engine used to be a child of the core, so every update or restart of the core killed it
//! in the middle of its turn, and the next core had to ask the agent to carry on (`restart.rs`).
//! Now each engine runs under a keeper, `hubd keep`: a small process the core starts on its own,
//! apart from itself, which starts the engine and holds its stdio. The core talks to the keeper
//! over a loopback socket, line for line as it would over the engine's stdio.
//!
//! A core that hands over (`Hub::park`) first tells each keeper to hold on (`detach`) and then
//! lets go. The keeper keeps the engine working, keeps what it says, and waits for the next core
//! (at most [`HOLD`]). The next core finds the keepers by the card each writes to
//! `<data dir>/keepers/<thread>.json`, takes them over (`attach`), and the turn goes on: what the
//! agent said meanwhile arrives first, questions it asked meanwhile are asked again, and the end
//! of a turn the old core started comes to the new one (`_hub/wait_turn`).
//!
//! A keeper speaks enough ACP for that. Requests from the core get ids of the keeper's own, since
//! every core counts from the start and the old core's prompt may still be open; answers go back
//! under the core's id, or are kept when the core that asked is gone. It also notes the agent's
//! answer to `initialize` and the session id, which a core that takes over does not ask again.
//!
//! A core that goes away without `detach` (Stop, an idle engine, a forced stop, the panic button,
//! a crash) ends the engine and everything it started at once, as when it was a child.

use std::collections::{BTreeMap, HashMap, VecDeque};
use crate::locked::ResultExt;
use crate::locked::Ignore;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

/// The `hubd` subcommand that keeps an engine.
pub const KEEP_ARG: &str = "keep";

/// How keeper and core talk; a core takes over only keepers that speak its version.
const PROTO: u32 = 1;

/// How long a keeper told to hold on waits for a core before it ends the engine.
pub const HOLD: Duration = Duration::from_secs(10 * 60);

/// The method a core that took over a turn in progress asks to hear that turn's end with. It never
/// reaches the agent.
pub const WAIT_TURN: &str = "_hub/wait_turn";

/// What a keeper answers a core that asks to attach while another core holds it.
const BUSY: &str = "held by another core";

/// Lines kept for a core while none is attached; past it the oldest go.
const BUFFER: usize = 50_000;

/// What the core gives a keeper on its stdin: the engine to start and where to write.
#[derive(Serialize, Deserialize)]
pub struct Spec {
    pub thread: String,
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    /// Where the engine's stderr goes (`acp::engine_log`).
    pub log: PathBuf,
    pub data_dir: PathBuf,
}

/// What a keeper writes to `<data dir>/keepers/<thread>.json`, so the next core finds it.
#[derive(Serialize, Deserialize)]
struct Card {
    proto: u32,
    pid: u32,
    port: u16,
    token: String,
    thread: String,
}

/// What a keeper tells a core that takes it over.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Attached {
    /// The agent's answer to `initialize`.
    pub init: Option<Value>,
    pub session: Option<String>,
    /// A turn an earlier core started has not ended yet, or ended unheard: ask with [`WAIT_TURN`].
    pub busy: bool,
}

#[derive(Serialize, Deserialize)]
struct Hello {
    proto: u32,
    token: String,
    /// `attach`, `detach` or `stop`.
    op: String,
}

pub(crate) fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("keepers")
}

fn card_path(data_dir: &Path, thread: &str) -> PathBuf {
    dir(data_dir).join(format!("{thread}.json"))
}

/// The program that keeps engines: the one set in the config, else this one when it is `hubd`.
/// None means engines run as children of the core, as before.
pub(crate) fn program(config: &crate::Config) -> Option<PathBuf> {
    if !config.keep_engines || config.trace_engines {
        return None;
    }
    if let Some(p) = &config.keeper {
        return Some(p.clone());
    }
    std::env::current_exe().ok().filter(|p| p.file_stem().is_some_and(|s| s == "hubd"))
}

/// A keeper the core is attached to: the socket, past the greeting.
pub(crate) struct Link {
    pub reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    pub writer: tokio::net::tcp::OwnedWriteHalf,
    pub attached: Attached,
}

impl Link {
    /// The socket as a line transport for the ACP crate.
    pub fn into_lines(
        self,
    ) -> agent_client_protocol::Lines<
        impl futures::Sink<String, Error = std::io::Error> + Send + 'static,
        impl futures::Stream<Item = std::io::Result<String>> + Send + 'static,
    > {
        let incoming = futures::stream::unfold(self.reader, |mut reader| async move {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => None,
                Ok(_) => Some((Ok(line.trim_end_matches(['\r', '\n']).to_owned()), reader)),
                Err(e) => Some((Err(e), reader)),
            }
        });
        let outgoing = futures::sink::unfold(self.writer, |mut writer, line: String| async move {
            writer.write_all(line.as_bytes()).await?;
            if !line.ends_with('\n') {
                writer.write_all(b"\n").await?;
            }
            writer.flush().await?;
            Ok::<_, std::io::Error>(writer)
        });
        agent_client_protocol::Lines::new(outgoing, incoming)
    }
}

/// Starts a keeper for `spec` and attaches to it.
pub(crate) async fn start(program: &Path, spec: &Spec) -> Result<Link> {
    let mut cmd = crate::proc::tokio_command(program);
    cmd.arg(KEEP_ARG)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(false);
    apart(&mut cmd);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        // A job that does not let its processes break away: start it inside, which still keeps
        // the engine through a restart of the core within the same job.
        #[cfg(windows)]
        Err(_) => {
            let mut cmd = crate::proc::tokio_command(program);
            cmd.arg(KEEP_ARG).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
            cmd.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000);
            cmd.spawn().with_context(|| format!("starting {}", program.display()))?
        }
        #[cfg(not(windows))]
        Err(e) => return Err(e).with_context(|| format!("starting {}", program.display())),
    };
    let mut stdin = child.stdin.take().context("no stdin")?;
    stdin.write_all(serde_json::to_string(spec)?.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    drop(stdin);
    let stdout = child.stdout.take().context("no stdout")?;
    let mut line = String::new();
    let read = tokio::time::timeout(Duration::from_secs(20), BufReader::new(stdout).read_line(&mut line)).await;
    // The keeper runs on its own from here; nothing waits for it.
    drop(child);
    match read {
        Ok(Ok(n)) if n > 0 => {}
        _ => bail!("the engine keeper did not start"),
    }
    let card: Card = serde_json::from_str(line.trim()).context("the engine keeper's card")?;
    attach_to(&card).await
}

/// Starts the keeper in a process group of its own (Unix) or outside the core's console and
/// job (Windows), so it lives on when the core ends.
fn apart(cmd: &mut tokio::process::Command) {
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    {
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB
        cmd.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000 | 0x0100_0000);
    }
}

async fn greet(card: &Card, op: &str) -> Result<(BufReader<tokio::net::tcp::OwnedReadHalf>, tokio::net::tcp::OwnedWriteHalf, Value)> {
    let stream = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(("127.0.0.1", card.port))).await.context("the keeper did not answer")??;
    stream.set_nodelay(true).warn_on_err("set TCP nodelay");
    let (read, mut write) = stream.into_split();
    let hello = Hello { proto: PROTO, token: card.token.clone(), op: op.into() };
    write.write_all(format!("{}\n", serde_json::to_string(&hello)?).as_bytes()).await?;
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line)).await.context("the keeper did not answer")??;
    let answer: Value = serde_json::from_str(line.trim()).context("the keeper's answer")?;
    if let Some(err) = answer["error"].as_str() {
        return Err(Refused(err.to_owned()).into());
    }
    Ok((reader, write, answer))
}

/// A keeper's no, with its reason.
#[derive(Debug)]
struct Refused(String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the keeper said no: {}", self.0)
    }
}

impl std::error::Error for Refused {}

/// Whether `err` is a keeper saying another core holds it.
pub(crate) fn held_elsewhere(err: &anyhow::Error) -> bool {
    err.downcast_ref::<Refused>().is_some_and(|r| r.0 == BUSY)
}

async fn attach_to(card: &Card) -> Result<Link> {
    let (reader, writer, answer) = greet(card, "attach").await?;
    let attached: Attached = serde_json::from_value(answer["attached"].clone()).context("the keeper's state")?;
    Ok(Link { reader, writer, attached })
}

/// Tells the keeper of `thread` to end its engine and everything it started, whether or not a
/// core is attached to it. For a thread that is deleted: the keeper would end the engine when the
/// core lets go, but only if the core's connection closes cleanly, and an engine found by its card
/// may have no core at all. Nothing to do (and no error) when there is no keeper.
pub(crate) async fn stop(data_dir: &Path, thread: &str) {
    let Some(card) = read_card(&card_path(data_dir, thread)) else { return };
    greet(&card, "stop").await.ignore("a card can outlive its keeper");
}

/// The pid of `thread`'s keeper, from its card.
pub(crate) fn pid(data_dir: &Path, thread: &str) -> Option<u32> {
    read_card(&card_path(data_dir, thread)).map(|c| c.pid)
}

fn read_card(path: &Path) -> Option<Card> {
    serde_json::from_slice::<Card>(&std::fs::read(path).ok()?).ok()
}

/// Tells the keepers of `threads` to hold their engines for the next core.
pub(crate) async fn detach(data_dir: &Path, threads: &[String]) {
    for thread in threads {
        let Some(card) = read_card(&card_path(data_dir, thread)) else { continue };
        if let Err(err) = greet(&card, "detach").await {
            tracing::warn!(thread = %thread, "the engine's keeper did not take the handover: {err:#}");
        }
    }
}

/// Every keeper this core can take over, by thread. Cards of keepers that are gone, or that speak
/// another version, are cleared (an old one is told to stop: its turn is carried on instead).
pub(crate) async fn take_over(data_dir: &Path) -> Vec<(String, Link)> {
    let Ok(entries) = std::fs::read_dir(dir(data_dir)) else { return Vec::new() };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Some(card) = read_card(&path) else {
            crate::locked::clean_file(&path);
            continue;
        };
        if card.proto != PROTO {
            greet(&card, "stop").await.ignore("a keeper of an older protocol may be gone or not understand");
            crate::locked::clean_file(&path);
            continue;
        }
        match attach_to(&card).await {
            Ok(link) => found.push((card.thread.clone(), link)),
            // Another core's engine, working: not ours to take, and its card stays.
            Err(err) if held_elsewhere(&err) => {
                tracing::info!(thread = %card.thread, "left an engine another core holds");
            }
            Err(err) => {
                tracing::info!(thread = %card.thread, "no engine to take over: {err:#}");
                crate::locked::clean_file(&path);
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------------------------
// The keeper itself.

/// What the relay hands on: a line for the agent's stdin or for the attached core.
#[derive(Debug, PartialEq)]
enum Out {
    Agent(String),
    Core(String),
}

/// A request the core sent, under the keeper's id.
struct Asked {
    epoch: u64,
    id: Value,
    method: String,
    /// `session/load`'s session id, taken as the session once it succeeds.
    session: Option<String>,
}

/// The ACP bookkeeping between one engine and whichever core is attached.
#[derive(Default)]
struct Relay {
    /// Counts attaches; a request from an earlier one is answered to nobody.
    epoch: u64,
    connected: bool,
    next_id: i64,
    asked: HashMap<i64, Asked>,
    /// Requests from the agent no core has answered yet, by id, in the order they came.
    asks: Vec<(Value, String)>,
    /// What the agent said while no core was attached.
    buffer: VecDeque<String>,
    init: Option<Value>,
    session: Option<String>,
    /// The end of a prompt whose core is gone, kept for the next one's [`WAIT_TURN`].
    unheard: Option<Value>,
    /// A [`WAIT_TURN`] waiting for that end: its epoch and id.
    waiting: Option<(u64, Value)>,
}

impl Relay {
    fn attach(&mut self) -> (Attached, Vec<String>) {
        self.epoch += 1;
        self.connected = true;
        self.waiting = None;
        let busy = self.unheard.is_some() || self.asked.values().any(|a| a.method == "session/prompt");
        let mut replay: Vec<String> = self.buffer.drain(..).collect();
        replay.extend(self.asks.iter().map(|(_, line)| line.clone()));
        (Attached { init: self.init.clone(), session: self.session.clone(), busy }, replay)
    }

    fn detached(&mut self) {
        self.connected = false;
        self.waiting = None;
    }

    fn push_core(&mut self, line: String, out: &mut Vec<Out>) {
        if self.connected {
            out.push(Out::Core(line));
        } else {
            if self.buffer.len() >= BUFFER {
                self.buffer.pop_front();
            }
            self.buffer.push_back(line);
        }
    }

    fn on_core(&mut self, line: &str) -> Vec<Out> {
        let mut out = Vec::new();
        let Ok(mut msg) = serde_json::from_str::<Value>(line) else {
            out.push(Out::Agent(line.to_owned()));
            return out;
        };
        let id = msg.get("id").cloned().filter(|v| !v.is_null());
        let method = msg.get("method").and_then(Value::as_str).map(str::to_owned);
        match (id, method) {
            (Some(id), Some(method)) if method == WAIT_TURN => {
                if let Some(end) = self.unheard.take() {
                    out.push(Out::Core(answer(&id, end)));
                } else if self.asked.values().any(|a| a.method == "session/prompt") {
                    self.waiting = Some((self.epoch, id));
                } else {
                    out.push(Out::Core(answer(&id, json!({ "result": { "stopReason": "end_turn" } }))));
                }
            }
            (Some(id), Some(method)) => {
                self.next_id += 1;
                let session = (method == "session/load").then(|| msg["params"]["sessionId"].as_str().map(str::to_owned)).flatten();
                self.asked.insert(self.next_id, Asked { epoch: self.epoch, id, method, session });
                msg["id"] = json!(self.next_id);
                out.push(Out::Agent(msg.to_string()));
            }
            // The core's answer to a question of the agent's.
            (Some(id), None) => {
                self.asks.retain(|(asked, _)| *asked != id);
                out.push(Out::Agent(line.to_owned()));
            }
            (None, _) => out.push(Out::Agent(line.to_owned())),
        }
        out
    }

    fn on_agent(&mut self, line: &str) -> Vec<Out> {
        let mut out = Vec::new();
        let Ok(mut msg) = serde_json::from_str::<Value>(line) else {
            self.push_core(line.to_owned(), &mut out);
            return out;
        };
        let id = msg.get("id").cloned().filter(|v| !v.is_null());
        let has_method = msg.get("method").is_some();
        match (id, has_method) {
            (Some(id), false) => {
                let Some(asked) = id.as_i64().and_then(|n| self.asked.remove(&n)) else { return out };
                if let Some(result) = msg.get("result") {
                    match asked.method.as_str() {
                        "initialize" => self.init = Some(result.clone()),
                        "session/new" => self.session = result["sessionId"].as_str().map(str::to_owned).or(self.session.take()),
                        "session/load" => self.session = asked.session.clone().or(self.session.take()),
                        _ => {}
                    }
                }
                if asked.epoch == self.epoch && self.connected {
                    msg["id"] = asked.id;
                    out.push(Out::Core(msg.to_string()));
                } else if asked.method == "session/prompt" {
                    let mut end = msg;
                    if let Some(o) = end.as_object_mut() {
                        o.remove("id");
                        o.remove("jsonrpc");
                    }
                    match self.waiting.take() {
                        Some((epoch, id)) if epoch == self.epoch && self.connected => out.push(Out::Core(answer(&id, end))),
                        _ => self.unheard = Some(end),
                    }
                }
            }
            (Some(id), true) => {
                self.asks.push((id, line.to_owned()));
                if self.connected {
                    out.push(Out::Core(line.to_owned()));
                }
            }
            (None, _) => self.push_core(line.to_owned(), &mut out),
        }
        out
    }
}

/// A JSON-RPC answer under `id`, from `end`'s `result` or `error`.
fn answer(id: &Value, end: Value) -> String {
    let mut msg = json!({ "jsonrpc": "2.0", "id": id });
    if let Some(e) = end.get("error") {
        msg["error"] = e.clone();
    } else {
        msg["result"] = end.get("result").cloned().unwrap_or(Value::Null);
    }
    msg.to_string()
}

/// Values of the engine's environment that look like keys, to blank out of its log.
fn secrets(env: &BTreeMap<String, String>) -> Vec<String> {
    env.iter()
        .filter(|(k, v)| {
            let k = k.to_ascii_uppercase();
            v.len() >= 12 && ["KEY", "TOKEN", "SECRET", "PASSWORD", "AUTH"].iter().any(|w| k.contains(w))
        })
        .map(|(_, v)| v.clone())
        .collect()
}

enum Event {
    AgentLine(String),
    AgentGone,
    Stderr(String),
    Core(u64, Option<String>),
    Accepted(TcpStream),
}

/// Starts the engine as the keeper's child, in a group (or job) of its own that ends with the keeper.
fn spawn_engine(spec: &Spec) -> Result<tokio::process::Child> {
    // Windows: everything the engine starts sits in a job that ends with this process.
    #[cfg(windows)]
    {
        let mut limits = win32job::ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let job = win32job::Job::create_with_limit_info(&limits).context("creating a job object")?;
        job.assign_current_process().context("joining the job object")?;
        std::mem::forget(job);
    }
    let mut cmd = crate::proc::tokio_command(&spec.command);
    cmd.args(&spec.args)
        .envs(&spec.env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().with_context(|| format!("starting {}", spec.command.display()))
}

/// Opens the port the core attaches on and writes the card that says where, to the data folder and
/// to stdout. Returns the listener, the token a core must present and the card's path.
async fn publish_card(spec: &Spec) -> Result<(tokio::net::TcpListener, String, PathBuf)> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let token = format!("{}", uuid::Uuid::new_v4().simple());
    let card = Card { proto: PROTO, pid: std::process::id(), port: listener.local_addr()?.port(), token: token.clone(), thread: spec.thread.clone() };
    let path = card_path(&spec.data_dir, &spec.thread);
    std::fs::create_dir_all(dir(&spec.data_dir))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&card)?)?;
    std::fs::rename(&tmp, &path)?;
    // The core reads the card from stdout, then stops listening.
    {
        let mut stdout = tokio::io::stdout();
        stdout.write_all(format!("{}\n", serde_json::to_string(&card)?).as_bytes()).await?;
        stdout.flush().await?;
    }
    Ok((listener, token, path))
}

/// Turns the engine's output, its stderr and the connections to the port into events.
fn pipe_events(
    agent_out: tokio::process::ChildStdout,
    agent_err: tokio::process::ChildStderr,
    listener: tokio::net::TcpListener,
    tx: &tokio::sync::mpsc::UnboundedSender<Event>,
) {
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(agent_out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tx.send(Event::AgentLine(line)).ignore("the keeper's loop has ended");
            }
            tx.send(Event::AgentGone).ignore("the keeper's loop has ended");
        });
    }
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(agent_err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tx.send(Event::Stderr(line)).ignore("the keeper's loop has ended");
            }
        });
    }
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tx.send(Event::Accepted(stream)).ignore("the keeper's loop has ended");
            }
        });
    }
}

/// Writes a line the engine printed on stderr to the keeper's log, with the secrets in the
/// engine's environment blanked out.
fn log_stderr(log: &mut Option<std::fs::File>, hide: &[String], line: String) {
        if let Some(file) = log.as_mut() {
            let mut clean = line;
            for s in hide {
                clean = clean.replace(s.as_str(), "[redacted]");
            }
            use std::io::Write;
            (writeln!(file, "{clean}")).warn_on_err("writing the log");
        }
}

/// Passes what the relay decided on: lines to the engine, lines to the core that is attached.
async fn forward(outs: Vec<Out>, agent_in: &mut tokio::process::ChildStdin, core: &mut Option<tokio::net::tcp::OwnedWriteHalf>) {
    for out in outs {
        match out {
            Out::Agent(line) => {
                if agent_in.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                    break;
                }
                (agent_in.flush().await).warn_on_err("writing to the connection");
            }
            Out::Core(line) => {
                let Some(w) = core.as_mut() else { continue };
                if w.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                    // The reader sees the same and says so.
                    *core = None;
                }
            }
        }
    }
}

/// What a connection to the keeper's port came to.
enum Accepted {
    /// A core attached: what to send it from the replay.
    Outs(Vec<Out>),
    /// Nothing more to do with it.
    Skip,
    /// The keeper was told to stop.
    Stop,
}

/// Answers a connection to the keeper's port: a core attaching, detaching for a handover, or
/// stopping the engine. `holding` and `alone_since` are the keeper's state about being handed over.
async fn accept(
    stream: TcpStream,
    token: &str,
    relay: &mut Relay,
    (holding, alone_since): (&mut bool, &mut Option<tokio::time::Instant>),
    core: &mut Option<tokio::net::tcp::OwnedWriteHalf>,
    tx: &tokio::sync::mpsc::UnboundedSender<Event>,
) -> Accepted {
        let (read, mut write) = stream.into_split();
        let mut reader = BufReader::new(read);
        let mut hello = String::new();
        let ok = tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut hello)).await.is_ok_and(|r| r.is_ok());
        let hello: Option<Hello> = ok.then(|| serde_json::from_str(hello.trim()).ok()).flatten();
        let Some(hello) = hello.filter(|h| h.token == token) else {
            (write.write_all(b"{\"error\":\"not let in\"}\n").await).warn_on_err("writing to the connection");
            return Accepted::Skip;
        };
        match hello.op.as_str() {
            "detach" => {
                *holding = true;
                (write.write_all(b"{\"ok\":true}\n").await).warn_on_err("writing to the connection");
                Accepted::Skip
            }
            "stop" => {
                (write.write_all(b"{\"ok\":true}\n").await).warn_on_err("writing to the connection");
                Accepted::Stop
            }
            // A core is attached and has not handed over: whoever else asks is not the
            // next core (a test's core on this machine's data folder, a second hubd),
            // and taking the engine from it would end the turn it is in.
            "attach" if relay.connected && !*holding => {
                (write.write_all(format!("{}\n", json!({ "error": BUSY })).as_bytes()).await).warn_on_err("writing to the connection");
                Accepted::Skip
            }
            "attach" if hello.proto == PROTO => {
                let (attached, replay) = relay.attach();
                *holding = false;
                *alone_since = None;
                let greeting = format!("{}\n", json!({ "attached": attached }));
                if write.write_all(greeting.as_bytes()).await.is_err() {
                    relay.detached();
                    return Accepted::Skip;
                }
                let epoch = relay.epoch;
                let tx = tx.clone();
                tokio::spawn(async move {
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        tx.send(Event::Core(epoch, Some(line))).ignore("the keeper's loop has ended");
                    }
                    tx.send(Event::Core(epoch, None)).ignore("the keeper's loop has ended");
                });
                *core = Some(write);
                Accepted::Outs(replay.into_iter().map(Out::Core).collect())
            }
            _ => {
                (write.write_all(b"{\"error\":\"unknown\"}\n").await).warn_on_err("writing to the connection");
                Accepted::Skip
            }
        }
}

/// `hubd keep`: reads its [`Spec`] from stdin, starts the engine and keeps it. Returns the exit
/// code.
pub async fn serve() -> Result<i32> {
    let mut line = String::new();
    BufReader::new(tokio::io::stdin()).read_line(&mut line).await?;
    let spec: Spec = serde_json::from_str(line.trim()).context("reading what to keep")?;
    let mut engine = spawn_engine(&spec)?;
    let engine_pid = engine.id();
    let mut agent_in = engine.stdin.take().context("no stdin")?;
    let agent_out = engine.stdout.take().context("no stdout")?;
    let agent_err = engine.stderr.take().context("no stderr")?;

    let (listener, token, path) = publish_card(&spec).await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    pipe_events(agent_out, agent_err, listener, &tx);

    let hide = secrets(&spec.env);
    let mut log = std::fs::create_dir_all(spec.log.parent().unwrap_or(Path::new(".")))
        .and_then(|_| std::fs::OpenOptions::new().create(true).append(true).open(&spec.log))
        .ok();
    let mut relay = Relay::default();
    let mut core: Option<tokio::net::tcp::OwnedWriteHalf> = None;
    // Told to hold on, and since when nobody is attached.
    let mut holding = false;
    let mut alone_since: Option<tokio::time::Instant> = None;

    let code = loop {
        let deadline = alone_since.map(|t| t + HOLD).unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(3600));
        let event = tokio::select! {
            e = rx.recv() => e,
            _ = tokio::time::sleep_until(deadline), if alone_since.is_some() => {
                note(&mut log, "ended the engine: no core took it over in time after a handover");
                break 0;
            }
        };
        let Some(event) = event else { break 0 };
        let outs = match event {
            Event::AgentLine(line) => relay.on_agent(&line),
            Event::AgentGone => {
                let code = engine.wait().await.ok().and_then(|s| s.code()).unwrap_or(1);
                note(&mut log, &format!("the engine exited by itself (exit code {code})"));
                break code;
            }
            Event::Stderr(line) => {
                log_stderr(&mut log, &hide, line);
                continue;
            }
            Event::Core(epoch, line) => {
                if epoch != relay.epoch {
                    continue;
                }
                match line {
                    Some(line) => relay.on_core(&line),
                    None => {
                        core = None;
                        relay.detached();
                        if !holding {
                            note(&mut log, "ended the engine: the core it worked for let go without handing over");
                            break 0;
                        }
                        alone_since = Some(tokio::time::Instant::now());
                        continue;
                    }
                }
            }
            Event::Accepted(stream) => match accept(stream, &token, &mut relay, (&mut holding, &mut alone_since), &mut core, &tx).await {
                Accepted::Outs(outs) => outs,
                Accepted::Skip => continue,
                Accepted::Stop => break 0,
            },
        };
        forward(outs, &mut agent_in, &mut core).await;
    };
    // Only its own card: a fresh keeper for the same thread (a new engine right after this one,
    // e.g. a warm engine that stepped aside) may have written its card there already.
    if read_card(&path).is_some_and(|c| c.token == token) {
        crate::locked::clean_file(&path);
    }
    end_engine(&mut engine, engine_pid).await;
    Ok(code)
}

/// Says in the engine's log why the keeper ends, so a turn that died has a reason on disk even
/// when the engine itself printed nothing.
fn note(log: &mut Option<std::fs::File>, why: &str) {
    if let Some(file) = log.as_mut() {
        use std::io::Write;
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        (writeln!(file, "[hub, unix time {secs}] {why}")).warn_on_err("writing the log");
    }
}

/// Ends the engine and what it started: its process group on Unix; on Windows the job ends it
/// when this process exits.
async fn end_engine(engine: &mut tokio::process::Child, pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        // SAFETY: a plain signal to the group the engine leads.
        unsafe extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        unsafe { kill(-(pid as i32), 15) };
        if tokio::time::timeout(Duration::from_secs(1), engine.wait()).await.is_err() {
            unsafe { kill(-(pid as i32), 9) };
        }
    }
    #[cfg(not(unix))]
    let _unused_off_unix = pid;
    engine.start_kill().ignore("the engine may have ended already");
    engine.wait().await.warn_on_err("wait for the engine to end");
}

#[cfg(test)]
mod tests;
