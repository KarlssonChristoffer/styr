//! Taking a thread over in a terminal, and handing it back.
//!
//! Engines speak ACP over stdio, so there is no screen to attach to while Hub drives one. Taking
//! over is a handover instead: the core stops the thread's engine, and `hubd attach` runs the
//! engine's own CLI resuming the same session (`terminal` in the engine's config, e.g.
//! `claude --resume {session}`) in the thread's folder. When the CLI exits the thread is handed
//! back; the core loads the session again and copies what was said in the terminal into the log
//! (the `terminal` event says how).
//!
//! `hubd attach <thread>` works in any terminal, also over SSH to a host and on Windows. Until
//! Enter is pressed it follows the thread live from the log; with `--if-idle` (Hub's Take over) it
//! takes over at once when the thread is not working, and never stops a turn by itself. A turn
//! that Enter does stop is carried on when the thread comes back, unless the terminal finished it. Where tmux is installed, every thread
//! with a running engine also gets a tmux session `hub-<id>` running it, so `tmux attach -t
//! hub-<id>` reaches any of them and they outlive the core's restarts.

use std::io::Write as _;
use crate::locked::ResultExt;
use crate::locked::Ignore;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::config::Config;
use crate::model::{Event, EventKind, Takeover, TerminalData, Thread, ThreadStatus};

pub use crate::hubd_args::{ATTACH_ARG, THREADS_ARG};
/// `hubd pair [--tailscale]`: a one-time code that lets another device in.
pub const PAIR_ARG: &str = "pair";

/// The short name of a thread on the command line and in tmux: the last eight hex digits of its
/// id, which are random (the first ones are a timestamp).
pub fn short_id(thread_id: &str) -> String {
    let hex: String = thread_id.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    hex[hex.len().saturating_sub(8)..].to_owned()
}

pub fn tmux_session(thread_id: &str) -> String {
    format!("hub-{}", short_id(thread_id))
}

/// This program, when it is `hubd`; tests and other hosts of the library have no `attach`.
pub(crate) fn hubd_exe() -> Option<PathBuf> {
    std::env::current_exe().ok().filter(|p| p.file_stem().is_some_and(|s| s == "hubd"))
}

pub(crate) fn tmux_available() -> bool {
    !cfg!(windows)
        && crate::proc::command("tmux")
            .arg("-V")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
}

fn has_tmux_session(name: &str) -> bool {
    crate::proc::command("tmux")
        .args(["has-session", "-t", &format!("={name}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Starts `hubd attach` for the thread in a detached tmux session, unless one is there. Under
/// systemd it goes in a scope of its own, so restarting the service does not end it.
pub(crate) fn start_tmux(thread: &Thread, now: bool) -> Result<String> {
    let exe = hubd_exe().context("only hubd can open a terminal")?;
    let name = tmux_session(&thread.id);
    if has_tmux_session(&name) {
        return Ok(name);
    }
    let mut args: Vec<String> = ["new-session", "-d", "-s", &name, "-c", &thread.cwd].map(String::from).to_vec();
    // A tmux server that was already running has its own environment; `attach` needs to find
    // the same config as this core.
    if let Some(config) = std::env::var_os("HUB_CONFIG") {
        args.extend(["-e".into(), format!("HUB_CONFIG={}", config.to_string_lossy())]);
    }
    args.push(exe.display().to_string());
    args.extend([ATTACH_ARG.to_owned(), thread.id.clone()]);
    if now {
        args.push("--if-idle".into());
    }
    let status = crate::services::outside_service("tmux", &args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("starting tmux")?;
    if !status.success() {
        bail!("tmux could not start a session for the thread");
    }
    Ok(name)
}

/// Gives a thread whose engine is starting its tmux session, where tmux is installed. Never fails
/// the engine.
pub(crate) fn watch_in_tmux(thread: &Thread) {
    if thread.host.is_some() || hubd_exe().is_none() || !tmux_available() {
        return;
    }
    let thread = thread.clone();
    std::thread::spawn(move || {
        if let Err(err) = start_tmux(&thread, false) {
            tracing::debug!(thread = %thread.id, "no tmux session: {err:#}");
        }
    });
}

pub(crate) fn close_tmux(thread_id: &str) {
    if cfg!(windows) || hubd_exe().is_none() {
        return;
    }
    let name = tmux_session(thread_id);
    std::thread::spawn(move || {
        crate::proc::command("tmux")
            .args(["kill-session", "-t", &format!("={name}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ignore("the thread may have no tmux session, or tmux is not installed");
    });
}

pub(crate) fn alive(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    let pid = Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing());
    sys.process(pid).is_some()
}

/// The process that has the thread in a terminal, from the log.
pub(crate) fn taken_by(events: &[Event]) -> Option<u32> {
    let last = events.iter().rev().find(|e| e.kind == EventKind::Terminal)?.data_as::<TerminalData>()?;
    (last.state == "taken").then(|| last.pid.flatten().map(|p| p as u32)).flatten()
}


/// What the log says of the thread's current session: how many prompts the engine has had in it,
/// and whether the thread is back from a terminal with what was said there not copied yet.
pub(crate) fn session_log_state(events: &[Event]) -> (usize, bool) {
    let mut known = 0;
    let mut unanswered = 0;
    let mut back = false;
    for e in events {
        match e.kind {
            EventKind::UserMessage => {
                known += 1;
                unanswered += 1;
            }
            EventKind::TurnEnd | EventKind::Error => unanswered = 0,
            // A fresh session has seen only the messages that were waiting for it.
            EventKind::SessionStarted if e.data_as::<crate::model::SessionStartedData>().is_some_and(|s| s.resumed == Some(false)) => known = unanswered,
            EventKind::Terminal => match e.data_as::<TerminalData>().unwrap_or_default() {
                t if t.state == "back" => back = true,
                t if t.state == "imported" => {
                    back = false;
                    known = t.replay_turns.unwrap_or(known as u64) as usize;
                }
                _ => {}
            },
            _ => {}
        }
    }
    (known, back)
}

/// How many prompts the engine has had in the thread's current session, by the log, when the
/// thread came back from a terminal and what was said there is not copied yet.
pub(crate) fn pending_import(events: &[Event]) -> Option<usize> {
    let (known, back) = session_log_state(events);
    back.then_some(known)
}

/// How many prompts the engine has had in the thread's current session, by the log. A session at
/// none has no conversation for an engine's CLI to resume.
pub(crate) fn session_prompts(events: &[Event]) -> usize {
    session_log_state(events).0
}

/// The engine's own CLI resuming a session, with `{session}`: `terminal` in its configuration, or
/// its harness's when the configuration was saved without one (an engine added before Hub knew how
/// to resume that harness).
pub(crate) fn terminal_command(name: &str, engine: &crate::config::EngineConfig) -> Option<String> {
    let id = engine.harness.as_deref().unwrap_or(name);
    engine
        .terminal
        .clone()
        .or_else(|| crate::engines::harnesses().into_iter().find(|h| h.id == id).and_then(|h| h.terminal))
        .filter(|t| !t.trim().is_empty())
}

/// What the agent is told when a turn that taking over stopped is carried on.
pub const CARRY_ON_TERMINAL: &str = "Your last turn was stopped when the thread was taken over in a terminal, and it is back in Hub now. Carry on where you left off, taking into account anything said in the terminal. Commands you started may or may not have finished: check what is already done before you run anything again that must not run twice (pushes, migrations, messages sent).";

/// Whether a thread just back from a terminal should carry on the turn taking over stopped:
/// the seq of that `taken` event when so. `tail` is what was said in the terminal (the copied
/// updates). Carried on when nothing was said there, or when the last thing said was the CLI's
/// own note that the user interrupted it; a request the terminal saw through is left alone.
pub(crate) fn carry_on_after_terminal(events: &[Event], tail: &[Value]) -> Option<i64> {
    let (taken, data) = events.iter().rev().find_map(|e| Some((e, e.data_as::<TerminalData>().filter(|t| t.state == "taken")?)))?;
    data.cut?;
    // Asked once per takeover, also when the session is loaded again.
    if events.iter().any(|e| e.kind == EventKind::UserMessage && e.mark("terminal").as_i64() == Some(taken.seq)) {
        return None;
    }
    let mut last = None::<String>;
    let mut previous_user = false;
    for u in tail {
        let user = crate::model::update_kind(u) == Some("user_message_chunk");
        if user {
            let text = u["content"]["text"].as_str().unwrap_or_default();
            if !previous_user {
                last = Some(String::new());
            }
            if let Some(last) = &mut last {
                last.push_str(text);
            }
        }
        previous_user = user;
    }
    match last {
        None => Some(taken.seq),
        Some(said) if said.trim_start().starts_with("[Request interrupted") => Some(taken.seq),
        Some(_) => None,
    }
}

/// What a loaded session replayed after the first `known` prompts, keeping only what a thread
/// shows, and how many prompts the replay had in all.
pub(crate) fn terminal_tail(replayed: &[Value], known: usize) -> (Vec<Value>, usize) {
    const SHOWN: [&str; 6] =
        ["user_message_chunk", "agent_message_chunk", "agent_thought_chunk", "tool_call", "tool_call_update", "plan"];
    let mut turns = 0;
    let mut start = None;
    let mut previous_user = false;
    for (i, u) in replayed.iter().enumerate() {
        let user = crate::model::update_kind(u) == Some("user_message_chunk");
        if user && !previous_user {
            if turns == known {
                start = Some(i);
            }
            turns += 1;
        }
        previous_user = user;
    }
    let tail = start
        .map(|i| replayed[i..].iter().filter(|u| SHOWN.iter().any(|k| crate::model::update_kind(u) == Some(*k))).cloned().collect())
        .unwrap_or_default();
    (tail, turns)
}

/// Prompts in a run of replayed updates.
pub(crate) fn turns_in(updates: &[Value]) -> usize {
    let mut previous_user = false;
    let mut turns = 0;
    for u in updates {
        let user = crate::model::update_kind(u) == Some("user_message_chunk");
        if user && !previous_user {
            turns += 1;
        }
        previous_user = user;
    }
    turns
}

// ---- `hubd attach` and `hubd threads` ----

pub(crate) struct Client {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl Client {
    pub(crate) fn new(config: &Config) -> Result<Self> {
        let token = crate::auth::token(&config.data_dir)?;
        Ok(Self { base: format!("http://{}", config.bind), token, http: reqwest::Client::new() })
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let res = self.http.get(format!("{}{path}", self.base)).bearer_auth(&self.token).send().await.context("Hub is not answering")?;
        Self::read(res).await
    }

    pub(crate) async fn post<T: serde::de::DeserializeOwned>(&self, path: &str, body: Value) -> Result<T> {
        let res = self
            .http
            .post(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .context("Hub is not answering")?;
        Self::read(res).await
    }

    async fn read<T: serde::de::DeserializeOwned>(res: reqwest::Response) -> Result<T> {
        if !res.status().is_success() {
            let body: Value = res.json().await.unwrap_or_default();
            bail!("{}", body["error"].as_str().unwrap_or("Hub refused"));
        }
        Ok(res.json().await?)
    }

    async fn thread(&self, query: &str) -> Result<Thread> {
        let threads: Vec<Thread> = self.get("/api/threads").await?;
        let q = query.to_lowercase();
        if let Some(t) = threads.iter().find(|t| t.id == query) {
            return Ok(t.clone());
        }
        let by_id: Vec<&Thread> = threads.iter().filter(|t| t.id.starts_with(query) || short_id(&t.id) == q).collect();
        let found = if by_id.is_empty() {
            threads.iter().filter(|t| t.host.is_none() && t.title.to_lowercase().contains(&q)).collect()
        } else {
            by_id
        };
        match found.as_slice() {
            [one] if one.host.is_some() => {
                bail!("{} runs on {}; attach there: ssh {0} -t hubd attach {}", one.title, one.host.as_deref().unwrap_or_default(), short_id(&one.id))
            }
            [one] => Ok((*one).clone()),
            [] => bail!("no thread matches {query:?}; `hubd threads` lists them"),
            many => bail!(
                "{} threads match {query:?}: {}",
                many.len(),
                many.iter().map(|t| format!("{} ({})", short_id(&t.id), t.title)).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

/// `hubd threads`: the threads on this machine with the short ids `attach` takes.
pub async fn list(config: &Config) -> Result<()> {
    let client = Client::new(config)?;
    let threads: Vec<Thread> = client.get("/api/threads").await?;
    for t in threads.iter().filter(|t| !t.archived && t.host.is_none()) {
        let status = serde_json::to_value(t.status).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
        println!("{}  {:<14}  {}  ({})", short_id(&t.id), status, t.title, t.cwd);
    }
    Ok(())
}

/// `hubd pair [--tailscale]`: prints a one-time code for another device, and where to reach this
/// machine. `--tailscale` first lets Tailscale serve hubd over HTTPS to the tailnet.
pub async fn pair(config: &Config, args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "--tailscale") {
        let url = crate::pairing::tailscale_serve(config.bind.port()).await?;
        println!("Tailscale serves this machine's Hub at {url}");
    }
    let client = Client::new(config)?;
    let code: crate::model::PairCode = client.post("/api/pair", Value::Null).await?;
    println!();
    println!("  Pairing code: {}", code.code);
    println!("  It works once, for 10 minutes.");
    println!();
    match &code.url {
        Some(url) => {
            println!("On the PC: Hub > Settings > Machines > Add by URL, with {url} and the code.");
        }
        None => {
            println!("No Tailscale address for this machine yet. To reach it over Tailscale, run:");
            println!("  hubd pair --tailscale");
            println!("Or add it by its SSH alias in Settings > Machines, which needs no code.");
        }
    }
    Ok(())
}

/// `hubd attach <thread> [--now | --if-idle]`: follows the thread live; Enter (or `--now`) takes
/// it over and runs the engine's CLI on its session, and when that exits the thread goes back to
/// Hub and is followed again. `--if-idle` takes over at once only when the thread is not working,
/// so opening it never stops a turn. Ctrl+C leaves.
pub async fn attach(config: &Config, args: &[String]) -> Result<()> {
    let now = args.iter().any(|a| a == "--now");
    let if_idle = args.iter().any(|a| a == "--if-idle");
    let Some(query) = args.iter().find(|a| !a.starts_with("--")) else {
        bail!("usage: hubd attach <thread> [--now | --if-idle]   (`hubd threads` lists them)");
    };
    let client = Client::new(config)?;
    let thread = client.thread(query).await?;
    let id = thread.id.clone();
    let mut out = std::io::stdout();
    (writeln!(out, "{} · {}", thread.title, thread.cwd)).warn_on_err("writing the log");

    let events: Vec<Event> = client.get(&format!("/api/threads/{id}/events?limit=50000")).await?;
    let mut view = View::default();
    // Only the latest exchange, so the screen starts with what is going on now.
    let from = events.iter().rposition(|e| e.kind == EventKind::UserMessage).unwrap_or(0);
    for e in &events[from..] {
        (write!(out, "{}", view.render(e))).warn_on_err("writing the log");
    }
    let mut after = events.last().map(|e| e.seq).unwrap_or(0);
    let mut working = matches!(thread.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou);
    let mut take = now || (if_idle && !working);
    // Empty polls in a row: a thread nobody writes to is looked at less and less often, so many
    // attached terminals do not keep hubd awake (a new event, or a running turn, is back to 700 ms).
    let mut quiet = 0u32;

    loop {
        if take {
            if let Err(err) = take_over(&client, &id, &mut view).await {
                (writeln!(out, "\n! {err:#}")).warn_on_err("writing the log");
            }
            working = false;
            // The log since, including what was copied from the terminal, is followed below.
        }
        (writeln!(out, "\n{}", hint(working))).warn_on_err("writing the log");
        (out.flush()).warn_on_err("writing to the connection");
        // One line only: once the CLI runs, the terminal's input is all its own.
        let (tx, mut enter) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let mut line = String::new();
            if std::io::stdin().read_line(&mut line).is_ok_and(|n| n > 0) {
                tx.send(()).ignore("the watch may have ended before Enter");
            }
        });
        loop {
            tokio::select! {
                pressed = &mut enter => {
                    if pressed.is_ok() {
                        take = true;
                        break;
                    }
                    // No input to read (not a terminal): only follow.
                    enter = tokio::sync::oneshot::channel().1;
                }
                _ = tokio::signal::ctrl_c() => {
                    (writeln!(out)).warn_on_err("writing the log");
                    return Ok(());
                }
                _ = tokio::time::sleep(Duration::from_millis(if working || quiet < 10 { 700 } else if quiet < 40 { 2000 } else { 5000 })) => {
                    let Ok(new) = client.get::<Vec<Event>>(&format!("/api/threads/{id}/events?after={after}")).await else { continue };
                    quiet = if new.is_empty() { quiet.saturating_add(1) } else { 0 };
                    for e in &new {
                        (write!(out, "{}", view.render(e))).warn_on_err("writing the log");
                        after = e.seq;
                        match e.kind {
                            EventKind::TurnStart => working = true,
                            // The way out again, under what just finished.
                            EventKind::TurnEnd | EventKind::Error => {
                                working = false;
                                (writeln!(out, "{}", hint(false))).warn_on_err("writing the log");
                            }
                            EventKind::Terminal if e.data_as::<TerminalData>().is_some_and(|t| t.state == "imported") => {
                                (writeln!(out, "{}", hint(working))).warn_on_err("writing the log");
                            }
                            _ => {}
                        }
                    }
                    (out.flush()).warn_on_err("writing to the connection");
                }
            }
        }
    }
}

/// The line that says what Enter does now.
fn hint(working: bool) -> &'static str {
    if working {
        "[Hub is working; you can watch it here · Enter stops the turn and takes it over (Hub carries on when you hand back, if it was left unfinished) · Ctrl+C leaves]"
    } else {
        "[Enter takes it over · Ctrl+C leaves]"
    }
}

async fn take_over(client: &Client, id: &str, view: &mut View) -> Result<()> {
    let taken: Takeover = client.post(&format!("/api/threads/{id}/takeover"), json!({ "pid": std::process::id() })).await?;
    let stopped = if taken.stopped_turn {
        "Hub's turn was stopped; when you exit the CLI, the thread goes back to Hub and carries on if the work was left unfinished."
    } else {
        "Hub's engine is stopped; exit the CLI to hand the thread back."
    };
    println!("\nTaking over {}. {stopped}\n$ {}\n", taken.title, taken.command);
    if taken.fresh {
        println!("Nothing was said in this session yet, so the CLI starts a new conversation. What came before is in Hub's log, not in the CLI.\n");
    }
    view.mid_line = false;

    #[cfg(windows)]
    let mut cmd = {
        let mut c = crate::proc::tokio_command_with_console("cmd");
        c.arg("/C").raw_arg(&taken.command);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = crate::proc::tokio_command_with_console("sh");
        c.arg("-c").arg(&taken.command);
        c
    };
    // What it starts belongs to the thread's project, as with Hub's own engine.
    cmd.current_dir(&taken.cwd)
        .envs(&taken.env)
        .env(crate::process_marks::THREAD_VAR, id)
        .env(crate::process_marks::PROJECT_VAR, &taken.cwd)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    let ran = cmd.status().await;
    if let Err(err) = &ran {
        println!("\n! could not start it: {err}");
    }

    // The core may be restarting for an update; keep asking for a while.
    let mut tries = 0;
    loop {
        match client.post::<Thread>(&format!("/api/threads/{id}/handback"), json!({})).await {
            Ok(_) => break,
            Err(err) if tries < 60 => {
                if tries == 0 {
                    println!("\nWaiting for Hub to take the thread back ({err:#})…");
                }
                tries += 1;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(err) => bail!("Hub did not take the thread back: {err:#}. Open it in Hub and press Take back."),
        }
    }
    println!("\nBack in Hub.");
    Ok(())
}

/// Draws log events as plain lines for a terminal.
#[derive(Default)]
struct View {
    tools: std::collections::HashSet<String>,
    /// The last thing written did not end its line.
    mid_line: bool,
}

impl View {
    fn line(&mut self, text: &str) -> String {
        let lead = if self.mid_line { "\n" } else { "" };
        self.mid_line = false;
        format!("{lead}{text}\n")
    }

    fn render(&mut self, e: &Event) -> String {
        let d = &e.data;
        match e.kind {
            EventKind::UserMessage if e.mark("review").is_null() => {
                let text = e.said().unwrap_or_default().trim();
                self.line(&format!("\n› {}", text.replace('\n', "\n  ")))
            }
            EventKind::Update => match crate::model::update_kind(d) {
                Some("agent_message_chunk") => {
                    let text = d["content"]["text"].as_str().unwrap_or_default();
                    if text.is_empty() {
                        return String::new();
                    }
                    self.mid_line = !text.ends_with('\n');
                    text.to_owned()
                }
                Some("user_message_chunk") => {
                    let text = d["content"]["text"].as_str().unwrap_or_default().trim().to_owned();
                    self.line(&format!("\n› {text}  (in the terminal)"))
                }
                Some("tool_call") => {
                    let id = d["toolCallId"].as_str().unwrap_or_default().to_owned();
                    if !self.tools.insert(id) {
                        return String::new();
                    }
                    self.line(&format!("  · {}", d["title"].as_str().unwrap_or("tool")))
                }
                _ => String::new(),
            },
            EventKind::PermissionRequest => {
                let title = e.tool_call()["title"].as_str().unwrap_or("the agent wants to continue");
                self.line(&format!("  ? {title}: answer in Hub, or take the thread over"))
            }
            EventKind::TurnEnd => self.line("  — done"),
            EventKind::Error => self.line(&format!("  ! {}", e.failure().unwrap_or("the engine failed"))),
            EventKind::Terminal => match e.data_as::<TerminalData>().unwrap_or_default() {
                t if t.state == "taken" => self.line("  — taken over in a terminal"),
                t if t.state == "imported" => {
                    let n = t.turns.unwrap_or(0);
                    self.line(&format!("  — back in Hub, {n} {} from the terminal copied in", if n == 1 { "prompt" } else { "prompts" }))
                }
                _ => String::new(),
            },
            _ => String::new(),
        }
    }
}

#[cfg(test)]
mod tests;
