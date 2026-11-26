//! The ACP driver: a thread's engine from `run` until its command channel closes or the agent
//! exits. Here is what the steps share (the capabilities Hub announces, the settings the session
//! offers, prompts as ACP content, the agent's updates logged, its errors put in words); the steps
//! are `connect.rs`, `session.rs` and `turns.rs`, and calls outside a thread are `probing.rs`.

use crate::locked::LockExt;
use crate::locked::ResultExt;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    BooleanConfigOptionCapabilities, CancelNotification, ClientCapabilities,
    ClientSessionCapabilities, NoticeCapabilities, SessionConfigOptionsCapabilities, ContentBlock, InitializeRequest, LoadSessionRequest, HttpHeader, McpServer, McpServerHttp, NewSessionRequest,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigOption, SessionConfigOptionValue, SessionId,
    SessionModeId, SessionModeState, SessionNotification, SetSessionConfigOptionRequest,
    SetSessionModeRequest, TextContent,
};
use agent_client_protocol::{AcpAgent, Agent, Client, ConnectionTo, LineDirection, Responder};
use anyhow::{Context, Result};
use serde_json::json;
use tokio::sync::mpsc;

use super::{Command, Prompt};

/// The kinds `terminal::session_log_state` and `carry_on_after_terminal` read, so an engine start
/// does not parse the whole log.
const SESSION_LOG: [EventKind; 5] =
    [EventKind::UserMessage, EventKind::TurnEnd, EventKind::Error, EventKind::SessionStarted, EventKind::Terminal];
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);
use crate::config::EngineConfig;
use crate::hub::Hub;
use crate::model::{Attachment, Event, EventKind, QuotaSwitchData, SessionStartedData, SessionStateData, SettingData, StartTiming, TerminalData, Thread, ThreadStatus};

/// Runs one thread's engine until its command channel closes or the agent exits.
pub(crate) async fn run(
    hub: Arc<Hub>,
    thread: Thread,
    engine: EngineConfig,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    leftover: &mut Vec<Prompt>,
    link: Option<crate::keeper::Link>,
) -> Result<()> {
    let parked: Parked = Arc::default();
    let result = connect(hub, thread, engine, commands, parked.clone(), link).await;
    leftover.extend(parked.locked().drain(..));
    result
}

/// Prompts a session holds behind the running turn; what is left when it ends goes back to the
/// caller (`run`).
type Parked = Arc<std::sync::Mutex<VecDeque<Prompt>>>;

/// How long an engine gets to end its turn after `session/cancel` before Hub stops it itself.
const CANCEL_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// How a turn ended: the engine answered (boxed: the answer is far larger than `Forced`), or Hub
/// stopped it for not stopping.
enum Turn {
    Done(Box<Result<agent_client_protocol::schema::v1::PromptResponse, agent_client_protocol::Error>>),
    Forced,
}

/// What a turn comes to: the engine's answer to the prompt.
type TurnFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Result<agent_client_protocol::schema::v1::PromptResponse, agent_client_protocol::Error>> + Send>>;

pub(crate) use crate::model::ENGINE_GONE;

/// What is known about an engine that went away: ACP's own words and, when the engine or its
/// keeper wrote one, the last line of its log (an exit code, a crash, why the keeper ended it).
fn closed_detail(hub: &Hub, thread_id: &str, err: &agent_client_protocol::Error) -> String {
    let mut detail = format!("The connection to the engine closed ({}).", err.message);
    let last = std::fs::read_to_string(engine_log(hub, thread_id))
        .ok()
        .and_then(|log| log.lines().rev().find(|l| !l.trim().is_empty()).map(|l| l.chars().take(400).collect::<String>()));
    match last {
        Some(line) => detail.push_str(&format!(" Last line of its log: {line}")),
        None => detail.push_str(" It printed nothing before it went."),
    }
    detail
}

/// Updates replayed while a session loads, kept when they are to be copied into the log.
type Replayed = Arc<std::sync::Mutex<Option<Vec<serde_json::Value>>>>;

/// Writes what the engine prints on stderr to `<data dir>/engines/<thread>.log`, which is where
/// to look when an engine fails to start. A log that grew past a few megabytes starts over.
/// Keys are redacted as in the thread's log (`leak::redact`): an engine that prints its
/// environment or "invalid key sk-…" must not leave the key on disk.
///
/// With `trace_engines` on, every JSON-RPC line both ways goes to `<thread>.trace.jsonl` as
/// `{ from: "hub" | "agent", ts, msg }`, redacted the same way: a real session recorded this way
/// is what a contract test against the real adapter replays.
fn stderr_log(hub: &Hub, thread_id: &str) -> impl Fn(&str, LineDirection) + Send + Sync + 'static {
    const LIMIT: u64 = 4 << 20;
    let vault = hub.secrets.cached();
    let path = engine_log(hub, thread_id);
    let open = |path: &std::path::Path, cap: bool| {
        std::fs::create_dir_all(path.parent().unwrap_or(path))
            .and_then(|_| {
                let big = cap && std::fs::metadata(path).is_ok_and(|m| m.len() > LIMIT);
                std::fs::OpenOptions::new().create(true).append(!big).write(true).truncate(big).open(path)
            })
            .map_err(|err| tracing::warn!("no engine log at {}: {err}", path.display()))
            .ok()
            .map(std::sync::Mutex::new)
    };
    let file = open(&path, true);
    let trace = hub.config.trace_engines.then(|| open(&engine_trace(hub, thread_id), false)).flatten();
    move |line, direction| {
        use std::io::Write;
        let clean = crate::leak::redact(line, &vault).unwrap_or_else(|| line.to_owned());
        if direction == LineDirection::Stderr {
            if let Some(file) = &file
                && let Ok(mut file) = file.lock()
            {
                (writeln!(file, "{clean}")).warn_on_err("writing the log");
            }
        } else if let Some(trace) = &trace
            && let Ok(mut trace) = trace.lock()
        {
            let from = if direction == LineDirection::Stdin { "hub" } else { "agent" };
            let msg = serde_json::from_str::<serde_json::Value>(&clean).unwrap_or(serde_json::Value::String(clean));
            (writeln!(trace, "{}", json!({ "from": from, "ts": crate::model::now_ms(), "msg": msg }))).warn_on_err("writing the log");
        }
    }
}

/// Where an engine's stderr goes for a thread.
pub(crate) fn engine_log(hub: &Hub, thread_id: &str) -> std::path::PathBuf {
    hub.config.data_dir.join("engines").join(format!("{thread_id}.log"))
}

/// Where a thread's JSON-RPC trace goes when `trace_engines` is on.
pub(crate) fn engine_trace(hub: &Hub, thread_id: &str) -> std::path::PathBuf {
    hub.config.data_dir.join("engines").join(format!("{thread_id}.trace.jsonl"))
}

/// Waits for the first message of the thread a warm engine started for (`warm.rs`), and parks
/// it to go first. False when none came in time or the channel closed.
async fn claim(warm: &crate::warm::Warm, commands: &mut mpsc::UnboundedReceiver<Command>, queue: &Parked) -> bool {
    let mut until = tokio::time::Instant::from_std(warm.at + crate::warm::WARM_FOR);
    let mut extended = false;
    loop {
        match tokio::time::timeout_at(until, commands.recv()).await {
            Ok(Some(Command::Prompt(prompt))) => {
                queue.locked().push_front(prompt);
                return true;
            }
            // Settings are read from the thread's log once it has one.
            Ok(Some(_)) => continue,
            Ok(None) => return false,
            // Its thread is being created right now: give the message a moment to arrive.
            Err(_) if !extended && crate::warm::pending(&warm.id).is_some_and(|w| w.claimed) => {
                extended = true;
                until = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
            }
            Err(_) => return false,
        }
    }
}

/// Moves the thread to a cheaper model before a message when the quota guard says so.
async fn guard(
    hub: &Arc<Hub>,
    thread_id: &str,
    engine: &str,
    conn: &ConnectionTo<Agent>,
    session_id: &SessionId,
    state: &mut SessionState,
) -> Result<()> {
    let Some((wanted, limit)) = hub.quota_switch(thread_id, engine).await else { return Ok(()) };
    let Some((config_id, value)) = model_choice(state, &wanted.to_lowercase()) else { return Ok(()) };
    let from = state.current(&config_id);
    if from.as_ref() == Some(&json!(value)) {
        return Ok(());
    }
    hub.emit_typed(thread_id, &SettingData::new(config_id.clone(), value.clone())).await?;
    state.set(conn, session_id, &config_id, json!(value)).await;
    state.publish(hub, thread_id).await?;
    let data = QuotaSwitchData { limit: limit.label, percent: json!(limit.percent), from: Some(json!(from)), to: value };
    hub.emit_typed(thread_id, &data).await?;
    Ok(())
}

/// The composer shows on/off settings as toggles, so engines may send them as booleans.
/// Stops a task when the engine run that started it ends, however it ends.
struct AbortOnDrop(tokio::task::JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// `terminal_output_delta` (the Zed convention both adapters read) asks for a command's output as
/// appended chunks under the tool call's `_meta`, with its exit code: Codex streams them while the
/// command runs, Claude sends the output in one chunk when it ends. The thread's terminal shows them.
/// `notices` lets an engine send its warnings ("Exceeded skills context budget…") as `notice`
/// updates, which the chat shows as a quiet line, instead of as text in the agent's reply.
fn capabilities() -> ClientCapabilities {
    let options = SessionConfigOptionsCapabilities::new().boolean(BooleanConfigOptionCapabilities::new());
    let meta = json!({ "terminal_output_delta": true }).as_object().cloned();
    let session = ClientSessionCapabilities::new().config_options(options).notices(NoticeCapabilities::new());
    ClientCapabilities::new().session(session).meta(meta)
}

/// What the engine says can be set on the session, kept current as settings change.
#[derive(Default)]
struct SessionState {
    config: Option<Vec<SessionConfigOption>>,
    modes: Option<SessionModeState>,
    /// Hub's provider and model pick (`routes.rs`), kept in `config` in place of the agent's
    /// own model setting however often the agent lists its settings again.
    route: Option<SessionConfigOption>,
    /// Settings the engine refused while a turn ran (Hermes changes its mode only when the session
    /// is idle), tried again once the turn has ended. The latest pick of each setting.
    deferred: Vec<(String, serde_json::Value)>,
}

/// What came of sending one setting to the engine.
#[derive(PartialEq)]
enum Set {
    /// The engine took it.
    Done,
    /// Not sent: Hub's own pick, or one the session does not offer.
    Left,
    /// The engine said no.
    Refused,
}

impl SessionState {
    /// The model the session is on now, as the engine or Hub's route names it, for a slip.
    fn model_now(&self) -> Option<String> {
        let name = |v: serde_json::Value| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned);
        self.current(crate::routes::ROUTE).and_then(name).filter(|r| r != crate::routes::OWN).or_else(|| self.current("model").and_then(name))
    }

    fn current(&self, config_id: &str) -> Option<serde_json::Value> {
        let option = self.config.iter().flatten().find(|o| &*o.id.0 == config_id);
        match option {
            Some(o) => serde_json::to_value(o).ok().map(|v| v["currentValue"].clone()),
            None if config_id == "mode" => {
                self.modes.as_ref().map(|m| json!(m.current_mode_id.0.to_string()))
            }
            None => None,
        }
    }

    /// Whether the session lists `config_id` (or, for an agent with modes only, `mode`) and offers
    /// `value` among its choices.
    fn offers(&self, config_id: &str, value: &serde_json::Value) -> bool {
        if let Some(option) = self.config.iter().flatten().find(|o| &*o.id.0 == config_id) {
            let Ok(option) = serde_json::to_value(option) else { return false };
            if option["type"] == "boolean" {
                return value.is_boolean();
            }
            return option["options"].as_array().into_iter().flatten().any(|c| {
                let choices = c["options"].as_array().cloned().unwrap_or_else(|| vec![c.clone()]);
                choices.iter().any(|c| c["value"] == *value)
            });
        }
        config_id == "mode" && self.modes.as_ref().is_some_and(|m| {
            m.available_modes.iter().any(|mode| json!(mode.id.0.to_string()) == *value)
        })
    }

    /// Applies one setting. A failure is logged and leaves the state as it was, so a setting the
    /// engine no longer offers (an old model) does not stop the thread.
    async fn set(
        &mut self,
        conn: &ConnectionTo<Agent>,
        session_id: &SessionId,
        config_id: &str,
        value: serde_json::Value,
    ) -> Set {
        // Hub's own provider pick is not the agent's to set.
        if config_id == crate::routes::ROUTE {
            return Set::Left;
        }
        // A setting saved while another engine ran (or a choice this session no longer lists) is
        // not the engine's to refuse: the engine would only say no, so it is left out here.
        if !self.offers(config_id, &value) {
            tracing::debug!(config_id, "not sending a setting the session does not offer: {value}");
            return Set::Left;
        }
        let offered = self.config.iter().flatten().any(|o| &*o.id.0 == config_id);
        let result = if !offered && config_id == "mode" && self.modes.is_some() {
            // Agents without config options still have modes.
            let Some(mode) = value.as_str().map(|m| SessionModeId::new(m.to_owned())) else {
                return Set::Left;
            };
            conn.send_request(SetSessionModeRequest::new(session_id.clone(), mode.clone()))
                .block_task()
                .await
                .map(|_| {
                    if let Some(modes) = &mut self.modes {
                        modes.current_mode_id = mode;
                    }
                })
        } else {
            let value = match value {
                serde_json::Value::Bool(b) => SessionConfigOptionValue::boolean(b),
                serde_json::Value::String(s) => SessionConfigOptionValue::value_id(s),
                _ => return Set::Left,
            };
            let request = SetSessionConfigOptionRequest::new(session_id.clone(), config_id.to_owned(), value);
            conn.send_request(request).block_task().await.map(|r| {
                self.config = Some(r.config_options);
                self.keep_route();
            })
        };
        match result {
            Ok(()) => Set::Done,
            Err(e) => {
                tracing::warn!(config_id, "could not change setting: {}", e.message);
                Set::Refused
            }
        }
    }

    /// A setting the user changed while a turn ran. Hermes takes a new mode only when its session
    /// is idle and says no in the meantime; the pick waits for the turn to end rather than
    /// showing and then dropping it. True when the engine took it (or had nothing to refuse).
    async fn set_during_turn(
        &mut self,
        conn: &ConnectionTo<Agent>,
        session_id: &SessionId,
        config_id: String,
        value: serde_json::Value,
    ) -> bool {
        self.deferred.retain(|(id, _)| *id != config_id);
        if self.set(conn, session_id, &config_id, value.clone()).await == Set::Refused {
            self.deferred.push((config_id, value));
            return false;
        }
        true
    }

    /// Tries the settings the engine refused during the turn again, now that it is idle.
    async fn apply_deferred(&mut self, conn: &ConnectionTo<Agent>, session_id: &SessionId) -> bool {
        let deferred = std::mem::take(&mut self.deferred);
        let any = !deferred.is_empty();
        for (config_id, value) in deferred {
            self.set(conn, session_id, &config_id, value).await;
        }
        any
    }

    /// Puts Hub's provider pick back in place of the agent's own model setting, after the agent
    /// listed its settings again.
    fn keep_route(&mut self) {
        let Some(route) = self.route.clone() else { return };
        let options = self.config.get_or_insert_with(Vec::new);
        options.retain(|o| {
            o.id.0 != route.id.0 && serde_json::to_value(o).map(|v| v["category"] != "model").unwrap_or(true)
        });
        options.insert(0, route);
    }

    async fn publish(&self, hub: &Hub, thread_id: &str) -> Result<()> {
        let data = SessionStateData { config_options: Some(json!(self.config)), modes: Some(json!(self.modes)), prompt_capabilities: None };
        hub.emit_typed(thread_id, &data).await?;
        Ok(())
    }
}

/// The engine's command as a program and its arguments. Not `AcpAgent::from_str`: its shell
/// rules read `\` as an escape, which turns `C:\Tools\agent.exe` into `C:Toolsagent.exe`.
fn agent(command: &str, extra: &[String]) -> Result<AcpAgent> {
    let words = crate::project::split_command(command);
    // A package npx already has starts without npx looking for it again.
    let words = crate::npx::direct(&words).unwrap_or(words);
    let (program, args) = words.split_first().with_context(|| format!("bad engine command {command:?}"))?;
    Ok(AcpAgent::new(agent_client_protocol::AcpAgentConfig::new(program).args(args.iter().chain(extra).cloned())))
}

/// The ACP content of a message: its text, then each attachment.
fn content(prompt: Prompt) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    if !prompt.text.is_empty() {
        blocks.push(ContentBlock::Text(TextContent::new(prompt.text)));
    }
    blocks.extend(prompt.attachments.iter().filter_map(attachment_block));
    blocks
}

fn attachment_block(a: &Attachment) -> Option<ContentBlock> {
    let block = if let Some(url) = &a.url {
        json!({ "type": "resource_link", "name": a.name, "uri": url, "mimeType": a.mime })
    } else if let Some(path) = &a.path {
        json!({ "type": "resource_link", "name": a.name, "uri": file_uri(path), "mimeType": a.mime })
    } else if let Some(text) = &a.text {
        json!({ "type": "resource", "resource": { "uri": file_uri(&a.name), "mimeType": a.mime, "text": text } })
    } else if let Some(data) = &a.data {
        if a.mime.starts_with("image/") {
            json!({ "type": "image", "mimeType": a.mime, "data": data })
        } else {
            json!({ "type": "resource", "resource": { "uri": file_uri(&a.name), "mimeType": a.mime, "blob": data } })
        }
    } else {
        return None;
    };
    serde_json::from_value(block).ok()
}

fn file_uri(path: &str) -> String {
    let path = path.replace('\\', "/");
    if path.starts_with('/') { format!("file://{path}") } else { format!("file:///{path}") }
}

/// Logs one `session/update` in the thread. Never an error back to the agent: a notification has
/// no reply, so the SDK would only log a bare "Internal error" and lose the cause. Updates still
/// arriving for a thread that was deleted while its engine wound down are dropped quietly.
async fn take_update(hub: &Hub, thread_id: &str, data: serde_json::Value) {
    let Err(err) = record_update(hub, thread_id, data).await else { return };
    // The thread was removed while the agent was still talking; the engine is on its way out
    // (`Hub::runtime_with`).
    if crate::store::gone(&err) || matches!(hub.store.thread(thread_id).await, Ok(None)) {
        tracing::debug!(thread = %thread_id, "dropped an engine update for a thread that is gone: {err:#}");
    } else {
        tracing::warn!(thread = %thread_id, "could not log the engine's update: {err:#}");
    }
}

async fn record_update(hub: &Hub, thread_id: &str, data: serde_json::Value) -> Result<()> {
    if crate::model::update_kind(&data) == Some("session_info_update")
        && let Some(title) = data["title"].as_str()
        && !titled_after_hubs_rules(hub, thread_id, title).await
    {
        hub.suggest_title(thread_id, title.to_owned()).await?;
    }
    hub.emit(thread_id, EventKind::Update, data).await?;
    Ok(())
}

/// Whether the engine named the session after Hub's rules, which it got in a message
/// (`turns::names_hubs_rules`). The title the person's own words gave stays then.
async fn titled_after_hubs_rules(hub: &Hub, thread_id: &str, title: &str) -> bool {
    let Ok(Some(thread)) = hub.store.thread(thread_id).await else { return false };
    if !hub.config.agent_tools || hub.config.traits(&thread.engine).mcp_instructions.unwrap_or(false) {
        return false;
    }
    let said = hub.store.events_of(thread_id, 0, &[EventKind::UserMessage]).await.unwrap_or_default();
    let said: Vec<&str> = said.iter().filter_map(Event::said).collect();
    turns::names_hubs_rules(title, &said)
}

fn internal(err: anyhow::Error) -> agent_client_protocol::Error {
    agent_client_protocol::Error::internal_error().data(json!(format!("{err:#}")))
}

/// The error as one line. "Internal error" is only the JSON-RPC wrapper around what went wrong
/// (Hub's own failures inside a session, a harness that could not start), so the detail alone
/// is said then: a bare "Internal error" in a thread tells nobody anything.
fn acp_err(err: agent_client_protocol::Error) -> anyhow::Error {
    match error_detail(&err) {
        Some(detail) if err.message.eq_ignore_ascii_case("internal error") => anyhow::anyhow!("{detail}"),
        Some(detail) => anyhow::anyhow!("{}: {detail}", err.message),
        None => anyhow::anyhow!("{}", err.message),
    }
}

/// What the agent added to an error beyond its one-line message: often the real cause, since
/// "Internal error" alone says nothing.
fn error_detail(err: &agent_client_protocol::Error) -> Option<String> {
    let detail = match err.data.as_ref()? {
        serde_json::Value::Null => return None,
        serde_json::Value::String(s) => s.trim().to_owned(),
        other => other.to_string(),
    };
    (!detail.is_empty() && detail != err.message).then(|| detail.chars().take(2000).collect())
}

/// An engine's own code for a turn that failed inside it, put in words. Hermes answers "Internal
/// error" with `admitted_turn_failed` when its model provider did not answer after its retries
/// (several minutes of silence in the thread); the code alone tells nobody that.
fn plain_detail(detail: String) -> String {
    if detail.contains("admitted_turn_failed") {
        return format!(
            "The engine's turn failed inside the engine ({detail}). That is usually its model provider not answering after several retries; the engine's own log has the exact reason."
        );
    }
    detail
}

mod connect;
use connect::*;

mod session;
use session::*;

mod turns;
use turns::*;

mod probing;
pub(crate) use probing::*;

#[cfg(test)]
mod tests;
