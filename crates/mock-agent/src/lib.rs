//! A scripted ACP agent. It echoes the prompt back word by word, and reacts to keywords:
//! "tool" reports a tool call, "permission" asks for permission first, "slow" takes a few
//! seconds (handy for testing cancel), "deaf" works on for half a minute and ignores cancel
//! (for testing the forced stop), "stray" first starts `sleep 600` in a process group of its own
//! and says its pid (a tool that outlives its engine), "crash" exits the process, "settings" reports the
//! session's model, effort and mode, "danger" asks to run `rm -rf build`,
//! "edit" reports a file edit with a diff, "notes" reports writing a new `notes.md`, "plan" reports a plan with one step in progress,
//! "write" really writes `mock.txt` in the session's folder, "erase" deletes it again, "cdn" or "homepage" writes the prompt to its
//! `index.html` (and Hub's request for proof of it gets a `page_check`), "quota" fails the turn the way the
//! Claude adapter does when a plan limit ran out, "draw" makes a picture the way Codex's image
//! generation reports one, "compile" runs a command whose output streams in as codex-acp sends it
//! (`_meta.terminal_output_delta` chunks, then `terminal_exit`; with "broken" it exits with 101). Attachments are echoed as `[image name]` or `[file uri]`.
//! "walkaway" asks for permission and ends the turn without waiting for the answer, as an agent
//! whose own approval timed out does.
//! "afterwards" ends the turn at once and goes on by itself a moment later: a tool call and a
//! sentence with no prompt behind them, as Hermes does when a background helper finishes.
//! "whereami" reports the variables named in `MOCK_ENV` as it was started with them.
//! `MOCK_MODELS=id=Name,id=Name` replaces the models it offers, to stand in for another provider.
//! `MOCK_MODES_ONLY=1` lists no settings but modes (default, accept_edits, dont_ask) taken with
//! `session/set_mode`, and refuses a new one while a turn runs, as Hermes does.
//! `MOCK_SAYS_MODEL=id` makes a new session report that model while it still runs its first.
//!
//! `serve(true)` is the inbox simulator instead (`sim.rs`): no keywords, just timed, scripted turns.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, AvailableCommand, AvailableCommandsUpdate, CancelNotification,
    ContentBlock, ContentChunk, Content, EmbeddedResourceResource, ImageContent, InitializeRequest, InitializeResponse,
    LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse, Plan, PlanEntry,
    PlanEntryPriority, PlanEntryStatus,
    PermissionOption, PermissionOptionKind, PromptCapabilities, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, SessionConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOption, SessionId, SessionNotification,
    SessionMode, SessionModeState, SessionUpdate, SetSessionConfigOptionRequest, SetSessionConfigOptionResponse,
    SetSessionModeRequest, SetSessionModeResponse, StopReason,
    Terminal, TextContent, ToolCall, ToolCallContent, ToolCallStatus, ToolKind, Diff, ToolCallUpdate, ToolCallUpdateFields, UsageUpdate,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Result, Stdio};

/// The client asked for command output as `terminal_output_delta` chunks. Without it "compile"
/// sends the output as text when the command ends, as the Claude adapter does.
static STREAM_OUTPUT: AtomicBool = AtomicBool::new(false);
/// The model last picked with `session/set_model`, which some agents (Hermes) take instead of
/// a config option; "whereami" reports it as `MODEL`.
static SET_MODEL: Mutex<String> = Mutex::new(String::new());

/// `lock()` that survives a poisoned mutex, as `hub_core::locked::LockExt` (mock-agent stays
/// free of hub-core, so it keeps its own copy).
trait LockExt<T> {
    fn locked(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn locked(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A discard on purpose with its reason, as `hub_install::ignore::Ignore` (mock-agent depends on no
/// workspace crate, so it keeps its own copy).
trait Ignore {
    fn ignore(self, why: &'static str);
}

impl<T, E> Ignore for Result<T, E> {
    fn ignore(self, why: &'static str) {
        debug_assert!(!why.trim().is_empty(), "ignore() needs a reason");
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, agent_client_protocol::JsonRpcRequest)]
#[request(method = "session/set_model", response = serde_json::Value)]
#[serde(rename_all = "camelCase")]
struct SetModelRequest {
    session_id: String,
    model_id: String,
}

/// `MOCK_MODES_ONLY`: the agent has modes and no settings, as Hermes has.
fn modes_only() -> bool {
    std::env::var("MOCK_MODES_ONLY").is_ok()
}

/// The mode of a modes-only agent.
static MODE: Mutex<String> = Mutex::new(String::new());
/// A turn is running.
static BUSY: AtomicBool = AtomicBool::new(false);

fn modes() -> SessionModeState {
    let current = MODE.locked().clone();
    let modes = [("default", "Default"), ("accept_edits", "Accept Edits"), ("dont_ask", "Don't Ask")]
        .into_iter()
        .map(|(id, name)| SessionMode::new(id, name))
        .collect();
    SessionModeState::new(if current.is_empty() { "default".to_owned() } else { current }, modes)
}

/// Current values of the session settings: model, effort, mode.
type Settings = Arc<Mutex<[String; 3]>>;

const SETTINGS: [(&str, &str, SessionConfigOptionCategory, &[&str]); 3] = [
    ("model", "Model", SessionConfigOptionCategory::Model, &["default", "opus", "haiku"]),
    ("effort", "Effort", SessionConfigOptionCategory::ThoughtLevel, &["low", "medium", "high"]),
    ("mode", "Mode", SessionConfigOptionCategory::Mode, &["default", "acceptEdits", "plan"]),
];

/// The values setting `i` offers, as id and name.
fn choices(i: usize) -> Vec<(String, String)> {
    if i == 0
        && let Ok(models) = std::env::var("MOCK_MODELS")
    {
        return models
            .split(',')
            .map(|m| m.split_once('=').unwrap_or((m, m)))
            .map(|(id, name)| (id.trim().to_owned(), name.trim().to_owned()))
            .collect();
    }
    SETTINGS[i].3.iter().map(|v| (v.to_string(), v.to_string())).collect()
}

fn config_options(settings: &Settings) -> Vec<SessionConfigOption> {
    let current = settings.locked().clone();
    SETTINGS
        .iter()
        .zip(current)
        .enumerate()
        .map(|(i, ((id, name, category, _), current))| {
            let options: Vec<SessionConfigSelectOption> =
                choices(i).into_iter().map(|(v, n)| SessionConfigSelectOption::new(v, n)).collect();
            SessionConfigOption::select(*id, *name, current, options).category(category.clone())
        })
        .collect()
}

fn announce_commands(cx: &ConnectionTo<Client>, session: SessionId) -> Result<()> {
    let commands = vec![
        AvailableCommand::new("compact", "Summarise the conversation to free context"),
        AvailableCommand::new("review", "Review the current changes"),
    ];
    cx.send_notification(SessionNotification::new(
        session,
        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(commands)),
    ))
}

mod replay;
mod sim;

pub use replay::serve as replay;

/// Speaks ACP on stdio until the client goes. `sim` plays the simulator's scripts instead of
/// reacting to keywords.
pub async fn serve(sim: bool) -> Result<()> {
    let sim = sim.then(|| Arc::new(sim::Sim::default()));
    let settings = fresh_settings();
    let cancelled = Arc::new(AtomicBool::new(false));
    let folder: Arc<Mutex<std::path::PathBuf>> = Arc::default();
    let on_cancel = {
        let cancelled = cancelled.clone();
        async move |_: CancelNotification, _cx| {
            cancelled.store(true, Ordering::SeqCst);
            Ok(())
        }
    };
    Agent
        .builder()
        .name("mock-agent")
        .on_receive_request(
            async move |init: InitializeRequest, responder, _cx| {
                responder.respond(initialize_reply(init))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let (settings, folder) = (settings.clone(), folder.clone());
                async move |req: NewSessionRequest, responder, cx: ConnectionTo<Client>| {
                    *folder.locked() = req.cwd.clone();
                    let id = SessionId::new(uuid::Uuid::now_v7().to_string());
                    responder.respond(new_session_reply(id.clone(), &settings))?;
                    announce_commands(&cx, id)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let (settings, folder, sim) = (settings.clone(), folder.clone(), sim.clone());
                async move |req: LoadSessionRequest, responder, cx: ConnectionTo<Client>| {
                    *folder.locked() = req.cwd.clone();
                    if let Some(sim) = &sim {
                        sim.resumed();
                    }
                    replay_history(&cx, &req)?;
                    responder.respond(load_session_reply(&settings))?;
                    announce_commands(&cx, req.session_id)
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: SetModelRequest, responder: agent_client_protocol::Responder<serde_json::Value>, _cx| {
                remember_model(req);
                responder.respond(serde_json::json!({}))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: SetSessionModeRequest, responder: agent_client_protocol::Responder<SetSessionModeResponse>, _cx| {
                match change_mode(&req) {
                    Ok(done) => responder.respond(done),
                    Err(err) => responder.respond_with_error(err),
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            {
                let (settings, sim) = (settings.clone(), sim.clone());
                async move |req: SetSessionConfigOptionRequest, responder, _cx| {
                    match set_option(&req, &settings, sim.as_deref()) {
                        Ok(done) => responder.respond(done),
                        Err(err) => responder.respond_with_error(err),
                    }
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(on_cancel, agent_client_protocol::on_receive_notification!())
        .on_receive_request(
            async move |req: PromptRequest, responder, cx: ConnectionTo<Client>| {
                let sim = sim.clone();
                let cancelled = cancelled.clone();
                cancelled.store(false, Ordering::SeqCst);
                let (settings, folder) = (turn_settings(&settings), folder.locked().clone());
                let task_cx = cx.clone();
                BUSY.store(true, Ordering::SeqCst);
                cx.spawn(async move {
                    let session = req.session_id.clone();
                    let stop = match &sim {
                        Some(sim) => sim.turn(req, &task_cx, &cancelled).await,
                        None => turn(req, &task_cx, &cancelled, &settings, &folder).await,
                    };
                    BUSY.store(false, Ordering::SeqCst);
                    finish_prompt(stop, session, &task_cx, responder)
                })
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(Stdio::new())
        .await
}

/// What this agent can do; also notes whether the client wants terminal output streamed.
fn initialize_reply(init: InitializeRequest) -> InitializeResponse {
    let asked = init.client_capabilities.meta.as_ref().and_then(|m| m.get("terminal_output_delta"));
    STREAM_OUTPUT.store(asked == Some(&serde_json::Value::Bool(true)), Ordering::SeqCst);
    InitializeResponse::new(init.protocol_version).agent_capabilities(
        AgentCapabilities::new()
            .load_session(true)
            .prompt_capabilities(PromptCapabilities::new().image(true).embedded_context(true)),
    )
}

/// Answers the prompt: the error as it came, or the usage and then the stop reason.
fn finish_prompt(
    stop: Result<StopReason>,
    session: SessionId,
    cx: &ConnectionTo<Client>,
    responder: agent_client_protocol::Responder<PromptResponse>,
) -> Result<()> {
    let stop = match stop {
        Ok(stop) => stop,
        Err(err) => return responder.respond_with_error(err),
    };
    cx.send_notification(SessionNotification::new(session, SessionUpdate::UsageUpdate(UsageUpdate::new(12_000, 200_000))))?;
    responder.respond(PromptResponse::new(stop))
}

/// The reply to a new session: the settings it offers, or the modes when this agent only has modes.
fn new_session_reply(id: SessionId, settings: &Settings) -> NewSessionResponse {
    // `MOCK_SAYS_MODEL`: a new session reports that model while it runs its first,
    // as claude-agent-acp reports the model in settings.json its CLI was told to skip.
    let reported = match std::env::var("MOCK_SAYS_MODEL") {
        Ok(said) => {
            let mut current = settings.locked().clone();
            current[0] = said;
            config_options(&Arc::new(Mutex::new(current)))
        }
        Err(_) => config_options(settings),
    };
    if modes_only() {
        NewSessionResponse::new(id).modes(modes())
    } else {
        NewSessionResponse::new(id).config_options(reported)
    }
}

/// The reply to a loaded session, shaped like the one for a new session.
fn load_session_reply(settings: &Settings) -> LoadSessionResponse {
    if modes_only() {
        LoadSessionResponse::new().modes(modes())
    } else {
        LoadSessionResponse::new().config_options(config_options(settings))
    }
}

/// The settings a session starts with: the first model, medium effort, the default mode.
fn fresh_settings() -> Settings {
    Arc::new(Mutex::new([choices(0)[0].0.clone(), "medium".into(), "default".into()]))
}

/// The model Hub last asked for, which "whereami" tells back.
fn remember_model(req: SetModelRequest) {
    *SET_MODEL.locked() = req.model_id;
}

/// The settings a turn reports: the mode is the session's own when this agent only has modes.
fn turn_settings(settings: &Settings) -> [String; 3] {
    let mut settings = settings.locked().clone();
    if modes_only() {
        settings[2] = modes().current_mode_id.0.to_string();
    }
    settings
}

/// A mode change, which only applies while no turn is running.
fn change_mode(req: &SetSessionModeRequest) -> Result<SetSessionModeResponse> {
    if BUSY.load(Ordering::SeqCst) {
        return Err(agent_client_protocol::Error::internal_error()
            .data("Mode changes apply when the session is idle; try again after this turn."));
    }
    *MODE.locked() = req.mode_id.0.to_string();
    Ok(SetSessionModeResponse::new())
}

/// Replay the history a test wrote, as an agent replays its session file
/// (`{"user": ..}` or `{"agent": ..}` per line), or something to ignore.
fn replay_history(cx: &ConnectionTo<Client>, req: &LoadSessionRequest) -> Result<()> {
    let history = std::fs::read_to_string(req.cwd.join(".mock-history.jsonl")).unwrap_or_default();
    if history.trim().is_empty() {
        cx.send_notification(SessionNotification::new(
            req.session_id.clone(),
            SessionUpdate::AgentMessageChunk(text("(replayed history)")),
        ))?;
    }
    for line in history.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()) {
        let update = match (line["user"].as_str(), line["agent"].as_str()) {
            (Some(t), _) => SessionUpdate::UserMessageChunk(text(t)),
            (_, Some(t)) => SessionUpdate::AgentMessageChunk(text(t)),
            _ => continue,
        };
        cx.send_notification(SessionNotification::new(req.session_id.clone(), update))?;
    }
    Ok(())
}

/// Change one setting, or the simulator's scenario; an unknown setting or value is invalid params.
fn set_option(req: &SetSessionConfigOptionRequest, settings: &Settings, sim: Option<&sim::Sim>) -> Result<SetSessionConfigOptionResponse> {
    // The simulator's script is a setting it takes without offering it, so the
    // composer has nothing extra to show.
    if let Some(sim) = sim
        && &*req.config_id.0 == "scenario"
        && let Some(name) = req.value.as_value_id()
    {
        sim.pick(&name.0);
        return Ok(SetSessionConfigOptionResponse::new(config_options(settings)));
    }
    let Some(i) = SETTINGS.iter().position(|(id, ..)| *id == &*req.config_id.0) else {
        return Err(agent_client_protocol::Error::invalid_params());
    };
    let Some(value) = req.value.as_value_id().filter(|v| choices(i).iter().any(|(id, _)| *id == *v.0)) else {
        return Err(agent_client_protocol::Error::invalid_params());
    };
    settings.locked()[i] = value.0.to_string();
    Ok(SetSessionConfigOptionResponse::new(config_options(settings)))
}

/// The prompt without the `<hub_instructions>` block Hub puts after a session's first message.
fn without_hub_note(prompt: &str) -> String {
    const END: &str = "</hub_instructions>";
    match (prompt.find("<hub_instructions>"), prompt.find(END)) {
        (Some(start), Some(end)) if start < end => format!("{}{}", &prompt[..start], &prompt[end + END.len()..]),
        _ => prompt.to_owned(),
    }
}

/// What the user sent, flattened to the one line of text the keywords are looked for in.
fn prompt_text(req: &PromptRequest) -> String {
    req.prompt
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.clone()),
            ContentBlock::Image(i) => Some(format!("[image {}]", i.mime_type)),
            ContentBlock::ResourceLink(l) => Some(format!("[file {}]", l.uri)),
            ContentBlock::Resource(r) => Some(match &r.resource {
                EmbeddedResourceResource::TextResourceContents(t) => format!("[file {}]", t.uri),
                EmbeddedResourceResource::BlobResourceContents(b) => format!("[file {}]", b.uri),
                _ => "[file]".to_owned(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The text a turn answers: the prompt, or for "settings" / "whereami" what the agent knows about itself.
fn answered_text(prompt: String, settings: &[String; 3]) -> String {
    // Hub's instructions ride after the first message of a session for an engine that does
    // not show MCP instructions to the model; their words are not what the user asked.
    let asked = without_hub_note(&prompt);
    if asked.contains("settings") {
        format!("model={} effort={} mode={}", settings[0], settings[1], settings[2])
    } else if asked.contains("whereami") {
        // The environment it was started with, for the variables named in MOCK_ENV.
        let names = std::env::var("MOCK_ENV").unwrap_or_default();
        let mut said: Vec<String> = names.split(',').filter(|n| !n.is_empty()).map(|n| format!("{n}={}", std::env::var(n).unwrap_or_default())).collect();
        let model = SET_MODEL.locked().clone();
        if !model.is_empty() {
            said.push(format!("MODEL={model}"));
        }
        said.join(" ")
    } else {
        prompt
    }
}

/// What one turn talks to: the connection, the session and the folder the agent works in.
struct Turn<'a> {
    cx: &'a ConnectionTo<Client>,
    session: SessionId,
    cancelled: &'a AtomicBool,
    folder: &'a std::path::Path,
}

impl Turn<'_> {
    fn say(&self, update: SessionUpdate) -> Result<()> {
        self.cx.send_notification(SessionNotification::new(self.session.clone(), update))
    }

    /// The keywords that end a turn at once: a crash, a hit limit, a failing command.
    fn abrupt(&self, cue: &str) -> Result<Option<StopReason>> {
        if cue.contains("crash") {
            std::process::exit(3);
        }
        if cue.contains("quota") {
            self.say(SessionUpdate::AgentMessageChunk(text("Working on it. ")))?;
            return Err(agent_client_protocol::Error::new(-32603, "Internal error: You've hit your limit · resets 3am (Europe/Stockholm)"));
        }
        // A command that fails, as Claude Code reports one.
        if cue.contains("toolfail") {
            let call = ToolCall::new("fail-1", "Run `cargo build`")
                .kind(ToolKind::Execute)
                .status(ToolCallStatus::InProgress)
                .raw_input(serde_json::json!({ "command": "cd app && cargo build" }));
            self.say(SessionUpdate::ToolCall(call))?;
            let out = ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new("Exit code 127\nbash: cargo: command not found"))));
            self.say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new("fail-1", ToolCallUpdateFields::new().status(ToolCallStatus::Failed).content(vec![out]))))?;
            self.say(SessionUpdate::AgentMessageChunk(text("The build could not run. ")))?;
            return Ok(Some(StopReason::EndTurn));
        }
        Ok(None)
    }

    /// A tool call, with a permission question when the cue asks for one (or the command is dangerous).
    async fn tool_call(&self, cue: &str) -> Result<Option<StopReason>> {
        let danger = cue.contains("danger");
        let title = if danger { "Run `rm -rf build`" } else { "Run `cargo test`" };
        if !(cue.contains("tool") || cue.contains("permission") || danger) {
            return Ok(None);
        }
        let call = ToolCall::new("call-1", title).status(ToolCallStatus::InProgress);
        self.say(SessionUpdate::ToolCall(call))?;
        if cue.contains("permission") || danger {
            let ask = RequestPermissionRequest::new(
                self.session.clone(),
                ToolCallUpdate::new("call-1", ToolCallUpdateFields::new().title(title)),
                vec![
                    PermissionOption::new("allow", "Allow", PermissionOptionKind::AllowOnce),
                    PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
                ],
            );
            if cue.contains("walkaway") {
                let _unanswered = self.cx.send_request(ask);
                self.say(SessionUpdate::AgentMessageChunk(text("Gave up waiting. ")))?;
                return Ok(Some(StopReason::EndTurn));
            }
            let answer = self.cx.send_request(ask).block_task().await?;
            let allowed = matches!(&answer.outcome,
                RequestPermissionOutcome::Selected(s) if &*s.option_id.0 == "allow");
            if !allowed {
                self.say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    "call-1",
                    ToolCallUpdateFields::new().status(ToolCallStatus::Failed),
                )))?;
                return Ok(Some(if self.cancelled.load(Ordering::SeqCst) {
                    StopReason::Cancelled
                } else {
                    StopReason::EndTurn
                }));
            }
        }
        self.say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "call-1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
        )))?;
        Ok(None)
    }

    /// The keywords that only add something to the turn: a connector call, a plan, files written.
    fn extras(&self, cue: &str, prompt: &str) -> Result<()> {
        // A claude.ai connector's tool, named the way Claude Code names it.
        if cue.contains("connector") {
            let name = "mcp__claude_ai_ElevenLabs__creative_generate_image";
            let mut meta = serde_json::Map::new();
            meta.insert("claudeCode".into(), serde_json::json!({ "toolName": name }));
            self.say(SessionUpdate::ToolCall(ToolCall::new("mcp-1", name).status(ToolCallStatus::InProgress).meta(meta)))?;
            self.say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new("mcp-1", ToolCallUpdateFields::new().status(ToolCallStatus::Completed))))?;
        }
        if cue.contains("plan") {
            let step = |s: &str, status| PlanEntry::new(s, PlanEntryPriority::Medium, status);
            self.say(SessionUpdate::Plan(Plan::new(vec![
                step("Read the current code", PlanEntryStatus::Completed),
                step("Make the change", PlanEntryStatus::InProgress),
                step("Run the tests", PlanEntryStatus::Pending),
            ])))?;
        }
        if cue.contains("write") {
            std::fs::write(self.folder.join("mock.txt"), format!("{prompt}\n")).ignore("a fake edit; the test that cues it looks for the file and fails there");
        }
        if cue.contains("erase") {
            std::fs::remove_file(self.folder.join("mock.txt")).ignore("a fake delete; the test that cues it checks the file is gone");
        }
        if cue.contains("cdn") || cue.contains("homepage") {
            std::fs::write(self.folder.join("index.html"), format!("{prompt}\n")).ignore("a fake edit; the test that cues it looks for the file and fails there");
        }
        if cue.contains("edit") {
            let diff = Diff::new("src/lib.rs", "fn main() {\n    println!(\"hello hub\");\n}\n")
                .old_text("fn main() {\n    println!(\"hello\");\n}\n".to_owned());
            let call = ToolCall::new("edit-1", "Edit src/lib.rs")
                .kind(ToolKind::Edit)
                .status(ToolCallStatus::Completed)
                .content(vec![ToolCallContent::Diff(diff)]);
            self.say(SessionUpdate::ToolCall(call))?;
        }
        if cue.contains("notes") {
            let diff = Diff::new("notes.md", NOTES);
            let call = ToolCall::new("notes-1", "Write notes.md")
                .kind(ToolKind::Edit)
                .status(ToolCallStatus::Completed)
                .content(vec![ToolCallContent::Diff(diff)]);
            self.say(SessionUpdate::ToolCall(call))?;
        }
        if cue.contains("draw") {
            self.draw()?;
        }
        Ok(())
    }

    /// As codex-acp reports Codex's `image_gen`: the revised prompt, then the PNG with the path
    /// Codex saved it to.
    fn draw(&self) -> Result<()> {
        self.say(SessionUpdate::ToolCall(ToolCall::new("image-1", "Image generation").status(ToolCallStatus::InProgress)))?;
        let picture = ImageContent::new(PICTURE, "image/png").uri(self.folder.join("generated_images/sunset.png").display().to_string());
        self.say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            "image-1",
            ToolCallUpdateFields::new().status(ToolCallStatus::Completed).content(vec![
                ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(
                    "Revised prompt: A low sun over a calm blue landscape, flat pixel style",
                )))),
                ToolCallContent::Content(Content::new(ContentBlock::Image(picture))),
            ]),
        )))
    }

    /// Answers to the questions Hub itself asks an agent (`focus.rs`, `gist.rs`).
    fn hub_question(&self, cue: &str) -> Result<Option<StopReason>> {
        // Hub's focus check (`focus.rs`): the thread is off the stage when its message says so.
        if let (Some(start), Some(end)) = (cue.find("<message>"), cue.find("</message>"))
            && cue.contains("<stage>")
        {
            let fits = !cue[start..end].contains("offstage");
            let answer = serde_json::json!({ "fits": fits, "why": if fits { "Part of the stage" } else { "A new idea for a later stage" } });
            self.say(SessionUpdate::AgentMessageChunk(text(&answer.to_string())))?;
            return Ok(Some(StopReason::EndTurn));
        }
        // Hub's hover card TL;DR (`gist.rs`): three points and what it needs.
        if cue.contains("Write a TL;DR") {
            let answer = "- Two sends to the same machine clobbered each other's temp file, so chmod hit a missing path.\n\
                - Fixed in `remote.rs` with a folder per send; the new test passes.\n\
                - The full suite has not run since the last change.\nNEEDS: Should I run the full suite before handing it off?";
            self.say(SessionUpdate::AgentMessageChunk(text(answer)))?;
            return Ok(Some(StopReason::EndTurn));
        }
        // Hub asking for proof of a page (`proof.rs`): looks at it with page_check, named the way
        // codex-acp names an MCP tool, and the page has an error when its file says "oops".
        if cue.contains("Hub: your last reply changed") && cue.contains("page_check") {
            let page = std::fs::read_to_string(self.folder.join("index.html")).unwrap_or_default();
            let seen = if page.contains("oops") {
                "Rendered at 720×40 px. 1 problem:\n- console.error: oops"
            } else {
                "Rendered at 720×40 px. No script errors, console errors or blocked loads."
            };
            let input = serde_json::json!({ "server": "hub", "tool": "page_check", "arguments": { "file": "index.html" } });
            self.say(SessionUpdate::ToolCall(ToolCall::new("look-1", "Tool: hub/page_check").status(ToolCallStatus::InProgress).raw_input(input)))?;
            let done = ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .content(vec![ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(seen))))]);
            self.say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new("look-1", done)))?;
            self.say(SessionUpdate::AgentMessageChunk(text("Looked at the page.")))?;
            return Ok(Some(StopReason::EndTurn));
        }
        Ok(None)
    }

    /// A `cargo build` that prints the compile lines, all at once or streamed as terminal output.
    async fn compile(&self, cue: &str) -> Result<Option<StopReason>> {
        // A new id each run, as real agents use, so a later run is its own command.
        static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = format!("compile-{}", RUNS.fetch_add(1, Ordering::SeqCst) + 1);
        let meta = |m: serde_json::Value| m.as_object().cloned();
        let call = ToolCall::new(id.clone(), "cargo build --release")
            .kind(ToolKind::Execute)
            .status(ToolCallStatus::InProgress)
            .raw_input(serde_json::json!({ "command": "cargo build --release" }))
            .content(vec![ToolCallContent::Terminal(Terminal::new(id.clone()))])
            .meta(meta(serde_json::json!({ "terminal_info": { "terminal_id": id } })));
        self.say(SessionUpdate::ToolCall(call))?;
        let pause = if cue.contains("slow") { 400 } else { 5 };
        if !STREAM_OUTPUT.load(Ordering::SeqCst) {
            let output = format!("```console\n{}\n```", COMPILE.join("\n"));
            let done = ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .content(vec![ToolCallContent::Content(Content::new(ContentBlock::Text(TextContent::new(output))))]);
            self.say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id.clone(), done)))?;
        }
        for line in COMPILE.iter().filter(|_| STREAM_OUTPUT.load(Ordering::SeqCst)) {
            if self.cancelled.load(Ordering::SeqCst) {
                return Ok(Some(StopReason::Cancelled));
            }
            let chunk = serde_json::json!({ "terminal_output_delta": { "terminal_id": id, "data": format!("{line}\r\n") } });
            self.say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id.clone(), ToolCallUpdateFields::new()).meta(meta(chunk))))?;
            tokio::time::sleep(Duration::from_millis(pause)).await;
        }
        if STREAM_OUTPUT.load(Ordering::SeqCst) {
            let code = if cue.contains("broken") { 101 } else { 0 };
            if code != 0 {
                let line = "\x1b[1m\x1b[31merror\x1b[0m\x1b[1m: could not compile `hub-core` (lib) due to 1 previous error\x1b[0m\r\n";
                let chunk = serde_json::json!({ "terminal_output_delta": { "terminal_id": id, "data": line } });
                self.say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id.clone(), ToolCallUpdateFields::new()).meta(meta(chunk))))?;
            }
            let exit = serde_json::json!({ "terminal_exit": { "terminal_id": id, "exit_code": code, "signal": null } });
            self.say(SessionUpdate::ToolCallUpdate(
                ToolCallUpdate::new(id.clone(), ToolCallUpdateFields::new().status(ToolCallStatus::Completed)).meta(meta(exit)),
            ))?;
        }
        Ok(None)
    }

    /// The keywords about a turn that does not end the ordinary way: a stray process, a deaf agent,
    /// a helper that finishes afterwards.
    async fn odd_endings(&self, cue: &str) -> Result<Option<StopReason>> {
        if cue.contains("stray") {
            // A tool that never ends and sits in a process group of its own, as a shell command of the
            // Claude adapter does: ending the engine's group does not reach it. Says its pid.
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                // Unix only, so no console window to hide (`tests/no_window.rs` is about Windows).
                use std::process::Command as Spawn;
                if let Ok(child) = Spawn::new("sleep").arg("600").process_group(0).spawn() {
                    self.say(SessionUpdate::AgentMessageChunk(text(&format!("Started {}. ", child.id()))))?;
                }
            }
        }
        if cue.contains("deaf") {
            // An agent that does not honour session/cancel: Hub has to stop it itself.
            self.say(SessionUpdate::AgentMessageChunk(text("Not listening. ")))?;
            tokio::time::sleep(Duration::from_secs(30)).await;
            return Ok(Some(StopReason::EndTurn));
        }
        if cue.contains("afterwards") {
            let (cx, session) = (self.cx.clone(), self.session.clone());
            tokio::spawn(async move {
                let say = |update: SessionUpdate| cx.send_notification(SessionNotification::new(session.clone(), update));
                tokio::time::sleep(Duration::from_millis(300)).await;
                const LATE: &str = "a late update after the turn; Hub may have closed the session by now";
                say(SessionUpdate::ToolCall(ToolCall::new("later-1", "Run `cargo test`").status(ToolCallStatus::InProgress))).ignore(LATE);
                tokio::time::sleep(Duration::from_millis(600)).await;
                say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new("later-1", ToolCallUpdateFields::new().status(ToolCallStatus::Completed)))).ignore(LATE);
                say(SessionUpdate::AgentMessageChunk(text("Helper done."))).ignore(LATE);
            });
            return Ok(Some(StopReason::EndTurn));
        }
        Ok(None)
    }

    /// The plain answer: the prompt echoed back word by word.
    async fn echo(&self, cue: &str, prompt: &str) -> Result<StopReason> {
        let delay = if cue.contains("slow") { 500 } else { 15 };
        for word in format!("Echo: {prompt}").split_inclusive(' ') {
            if self.cancelled.load(Ordering::SeqCst) {
                return Ok(StopReason::Cancelled);
            }
            self.say(SessionUpdate::AgentMessageChunk(text(word)))?;
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        Ok(StopReason::EndTurn)
    }
}

async fn turn(
    req: PromptRequest,
    cx: &ConnectionTo<Client>,
    cancelled: &AtomicBool,
    settings: &[String; 3],
    folder: &std::path::Path,
) -> Result<StopReason> {
    let prompt = answered_text(prompt_text(&req), settings);
    let t = Turn { cx, session: req.session_id.clone(), cancelled, folder };
    // Like the Claude adapter, /usage is answered locally with the plan limits.
    if prompt.trim() == "/usage" {
        t.say(SessionUpdate::AgentMessageChunk(text(USAGE)))?;
        return Ok(StopReason::EndTurn);
    }
    // Keywords count in what was asked now, not in a conversation Hub handed over with it.
    let asked = without_hub_note(&prompt);
    let cue = match (asked.find("<conversation>"), asked.rfind("</conversation>")) {
        (Some(start), Some(end)) if start < end => format!("{}{}", &asked[..start], &asked[end..]),
        _ => asked.clone(),
    };
    if let Some(stop) = t.abrupt(&cue)? {
        return Ok(stop);
    }
    if let Some(stop) = t.tool_call(&cue).await? {
        return Ok(stop);
    }
    t.extras(&cue, &prompt)?;
    if let Some(stop) = t.hub_question(&cue)? {
        return Ok(stop);
    }
    if cue.contains("compile")
        && let Some(stop) = t.compile(&cue).await?
    {
        return Ok(stop);
    }
    if let Some(stop) = t.odd_endings(&cue).await? {
        return Ok(stop);
    }
    t.echo(&cue, &prompt).await
}

const USAGE: &str = "## Usage\n\n> Claude max subscription usage\n\n### Limits\n\n\
    **5-hour limit** — **9%** · Resets Oct 30, 1:50 AM GMT+1\n\n\
    **Weekly · all models** — **10%** · Resets Nov 1, 4:00 PM GMT+1\n";

/// A 96×96 PNG of a sun over a landscape.
const PICTURE: &str = "iVBORw0KGgoAAAANSUhEUgAAAGAAAABgCAIAAABt+uBvAAACTklEQVR42u3QxW6bYRSE4bmrpmFOubZjl5kpTA4z50YaZma+nF5CK3nT1fyKv2PpLEZ69jN6kZ65FgIdM9dCoH36Sgi0TV8JgdapSyHQMnUpBJonL4RA0+SFEGicOBcCDRPnQqB+/EwI1I2fCYFfY6dC4OfYqRD4MXoiBL6PngiBbyPHQuDryLEQ+DJ8JAQ+Dx8JgU9Dh0Lg49ChEPgweCAE3g8e+PfnOs3lbhrvBvY9i0zzv1wcwNuBfbduVCfD/APe9O/5lEWdDNsbeN2/51DWdTIMn+BV3643gXUyrM7gZd+uKyZ1Mkz+4EXvjiuGgUz+4Hnvjh+GdTLCL+FZz7Yf5oHCL+Fpz7Yf5oHCL+FJ95Yf5oHCLyHVveWHeaDwS0h2bfphHij8Emq7Nv0wDxR+CYnODT/MA4VfQrxzww/zQOGXEEuv+2EeKPwSHqfX/TAPFH4JjzrWXDGsY/IHDzvWXDEMZPIHD9pXvTGpY3UG99tXHQqsY/gE99pWfMq6ju0N3G1bcSuLOuYfcKd12bMb1cnFAdS0LvsXmSZ306huWRICVS1LQqCyeVEIVDQvCoHypgUhUNa0IARKG+eFQEnjvBAobpgTAkUNc0KgsH5WCBTUzwqB/LrfQuBWaUwI5JXGhEBeWUyIf4HiQihQVKDbZXEhFCgyUHlcCAWKCpRfnhBCgRQoNFBFQggUVCSEUKDoQLVCKFBUoMLKWiEUSIEUKLeBiiqTQqCoKimEAilQYKDiqqQQChQZqDolhAJFBSqpTgmhQAoUGqgmJcRfa0+Bh8DrmwAAAAAASUVORK5CYII=";

/// The Markdown file "notes" writes, with most of what a reader has to draw.
const NOTES: &str = "---\ntitle: Hello\n---\n# Hello World\n\nA short note with **bold**, *italic* and `code`.\n\n## List\n\n- one\n- two\n  - nested\n\n## Table\n\n| Name | State |\n| --- | --- |\n| Hub | running |\n\n> A quote.\n\n```rust\nfn main() {}\n```\n";

fn text(s: &str) -> ContentChunk {
    ContentChunk::new(ContentBlock::Text(TextContent::new(s)))
}

/// What "compile" prints, colours and all.
const COMPILE: &[&str] = &[
    "\x1b[1m\x1b[32m   Compiling\x1b[0m proc-macro2 v1.0.92",
    "\x1b[1m\x1b[32m   Compiling\x1b[0m unicode-ident v1.0.14",
    "\x1b[1m\x1b[32m   Compiling\x1b[0m serde v1.0.215",
    "\x1b[1m\x1b[32m   Compiling\x1b[0m quote v1.0.37",
    "\x1b[1m\x1b[32m   Compiling\x1b[0m syn v2.0.90",
    "\x1b[1m\x1b[32m   Compiling\x1b[0m tokio v1.42.0",
    "\x1b[1m\x1b[32m   Compiling\x1b[0m serde_json v1.0.133",
    "\x1b[1m\x1b[32m   Compiling\x1b[0m rusqlite v0.32.1",
    "\x1b[1m\x1b[32m   Compiling\x1b[0m hub-core v0.1.0 (/work/hub/crates/hub-core)",
    "\x1b[1m\x1b[33mwarning\x1b[0m\x1b[1m: unused variable: `seq`\x1b[0m",
    "  \x1b[1m\x1b[34m-->\x1b[0m crates/hub-core/src/store.rs:212:13",
    "\x1b[1m\x1b[32m   Compiling\x1b[0m hubd v0.1.0 (/work/hub/crates/hubd)",
    "\x1b[1m\x1b[32m    Finished\x1b[0m `release` profile [optimized] target(s) in 41.07s",
];
