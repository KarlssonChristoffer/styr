//! The ACP session on an open connection, from initialize to the first turn: Hub's tools as an
//! MCP server, opening or loading the session (copying in what was said in a terminal meanwhile),
//! a warm engine claimed by its thread, the thread's settings applied, and an engine taken over
//! from the core before. Then it hands over to `turns.rs`.

use crate::locked::ResultExt;
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn session(
    hub: Arc<Hub>,
    thread: Thread,
    (command, start_model, route_option): (String, Option<String>, Option<serde_json::Value>),
    meta: Option<serde_json::Map<String, serde_json::Value>>,
    (replaying, replayed, unprompted): (Arc<AtomicBool>, Replayed, Arc<crate::engine::unprompted::Unprompted>),
    commands: &mut mpsc::UnboundedReceiver<Command>,
    queue: Parked,
    (attached, began): (Option<crate::keeper::Attached>, std::time::Instant),
    conn: ConnectionTo<Agent>,
) -> Result<()> {
    // An engine taken over from the core before this one is already up, in its session.
    if let Some(attached) = attached {
        return session_taken_over(hub, thread, route_option, attached, unprompted, commands, queue, conn).await;
    }
    let init = conn
        .send_request(InitializeRequest::new(ProtocolVersion::V1).client_capabilities(capabilities()))
        .block_task()
        .await
        .map_err(acp_err)?;
    let initialized = began.elapsed();

    let mcp = hub_tools_server(&hub, &thread, &init);
    // Extra folders, where the agent takes them in the session request (Claude Code gets them
    // from its options instead, see `connections::add_dirs`).
    let extra_dirs = if init.agent_capabilities.session_capabilities.additional_directories.is_some() {
        crate::reach::directories(&crate::reach::get(&hub.ctx(), &thread.id).await.unwrap_or_default(), &thread.cwd)
    } else {
        Vec::new()
    };
    let import = match &thread.engine_session_id {
        Some(_) => crate::terminal::pending_import(&hub.store.events_of(&thread.id, 0, &SESSION_LOG).await?),
        None => None,
    };
    let (session_id, mut state, resumed) = open_session(&conn, &thread, &init, import.is_some(), (mcp, extra_dirs, meta), (&replaying, &replayed)).await?;
    // The agent lost the session (another provider could not load it, or it was gone): the
    // first message carries the conversation instead, as when another agent takes over.
    let lost = thread.engine_session_id.is_some() && !resumed && import.is_none();
    if let Some(known) = import {
        copy_in_terminal(&hub, &thread, known, resumed, &replayed).await?;
    }
    if route_option.is_some()
        && let Some(model) = &start_model
    {
        set_harness_model(&conn, &thread, &session_id, model).await?;
    }

    let ready = began.elapsed();

    // Started ahead of its thread: the session is open, so wait here for the thread's first
    // message. One that came out different from what the engine started with goes to a fresh
    // engine instead (the prompt is left over, see `run`).
    let Some(thread) = claim_warm(&hub, thread, commands, &queue).await? else { return Ok(()) };
    if !resumed {
        let id = session_id.0.to_string();
        hub.update_thread(&thread.id, move |t| t.engine_session_id = Some(id)).await?;
    }

    // Up and not yet prompted: whatever runs under the engine now is the engine.
    let id = thread.id.clone();
    tokio::task::spawn_blocking(move || crate::workers::note_engine(&id)).await.warn_on_err("note the engine's processes");

    let started = SessionStartedData {
        driver: "acp".into(),
        command,
        protocol_version: Some(json!(init.protocol_version)),
        agent: Some(json!(init.agent_info)),
        session_id: session_id.0.to_string(),
        resumed: Some(resumed),
        // How long the engine took to come up, in ms (`timings.rs`).
        start_ms: Some(StartTiming { initialize: initialized.as_millis() as i64, session: (ready - initialized).as_millis() as i64, total: ready.as_millis() as i64 }),
    };
    hub.emit_typed(&thread.id, &started).await?;

    apply_settings(&hub, &thread, &conn, &session_id, &mut state, resumed, (&start_model, route_option)).await?;
    let offered = SessionStateData {
        config_options: Some(json!(state.config)),
        modes: Some(json!(state.modes)),
        prompt_capabilities: Some(json!(init.agent_capabilities.prompt_capabilities)),
    };
    hub.emit_typed(&thread.id, &offered).await?;

    turns(&hub, &thread, &session_id, &mut state, commands, &queue, &conn, &unprompted, lost, None).await
}

/// Hub's own tools as an MCP server, when the agent can reach one over HTTP.
fn hub_tools_server(hub: &Hub, thread: &Thread, init: &agent_client_protocol::schema::v1::InitializeResponse) -> Vec<McpServer> {
    if hub.config.agent_tools && (init.agent_capabilities.mcp_capabilities.http || hub.config.traits(&thread.engine).mcp_http.unwrap_or(false)) {
        let (mut url, token) = crate::auth::tools_endpoint(&hub.config, &thread.id);
        let headers = match token {
            Ok(token) => {
                // An engine that drops the headers shows the token in the address instead.
                if hub.config.traits(&thread.engine).mcp_token_in_url.unwrap_or(false) {
                    url = crate::auth::with_token(&url, &token);
                }
                vec![HttpHeader::new("Authorization", format!("Bearer {token}"))]
            }
            Err(err) => {
                tracing::error!(thread = %thread.id, "Hub's tools get no connection token: {err:#}");
                Vec::new()
            }
        };
        vec![McpServer::Http(McpServerHttp::new("hub", url).headers(headers))]
    } else {
        Vec::new()
    }
}

/// What was said in the terminal, copied in after the log's own prompts.
async fn copy_in_terminal(hub: &Arc<Hub>, thread: &Thread, known: usize, resumed: bool, replayed: &Replayed) -> Result<()> {
    let kept = replayed.locked().take().unwrap_or_default();
    let (tail, total) = if resumed { crate::terminal::terminal_tail(&kept, known) } else { (Vec::new(), 0) };
    let turns = crate::terminal::turns_in(&tail);
    // A turn taking over stopped goes on, unless the terminal saw the work through. Only on
    // the same session: a fresh one would not know what it was doing.
    let carry = if resumed && hub.config.resume_turns {
        crate::terminal::carry_on_after_terminal(&hub.store.events_of(&thread.id, 0, &SESSION_LOG).await?, &tail)
    } else {
        None
    };
    for update in tail {
        hub.emit(&thread.id, EventKind::Update, update).await?;
    }
    let imported = TerminalData { turns: Some(turns as u64), replay_turns: Some(total as u64), carry_on: carry.is_some().then_some(true), ..TerminalData::state("imported") };
    hub.emit_typed(&thread.id, &imported).await?;
    if let Some(taken) = carry {
        hub.carry_on_after_takeover(&thread.id, taken);
    }
    Ok(())
}

/// A thread whose engine was started ahead of it: waits for the thread's first message. `None`
/// when the thread came out different from what the engine started with (or never came), so a
/// fresh engine takes it instead.
async fn claim_warm(hub: &Hub, thread: Thread, commands: &mut mpsc::UnboundedReceiver<Command>, queue: &Parked) -> Result<Option<Thread>> {
    let Some(warm) = crate::warm::pending(&thread.id) else { return Ok(Some(thread)) };
    let claimed = claim(&warm, commands, queue).await;
    crate::warm::forget(&warm.id);
    if !claimed {
        return Ok(None);
    }
    let Some(real) = hub.store.thread(&warm.id).await? else { return Ok(None) };
    let route = crate::routes::picked(&hub.store.settings(&real.id).await?);
    if real.engine != warm.engine || real.cwd != warm.cwd || route != warm.route() {
        tracing::info!(thread = %real.id, "the thread is not what its warm engine started for; starting a fresh one");
        return Ok(None);
    }
    Ok(Some(real))
}

/// Puts the thread's settings into the session: what it chose, a default mode, what a thread that
/// runs without the user prefers, and Hub's provider pick.
async fn apply_settings(
    hub: &Arc<Hub>,
    thread: &Thread,
    conn: &ConnectionTo<Agent>,
    session_id: &SessionId,
    state: &mut SessionState,
    resumed: bool,
    (start_model, route_option): (&Option<String>, Option<serde_json::Value>),
) -> Result<()> {
    // Settings chosen while the engine was stopped, or before the thread had a session.
    let chosen = crate::routes::without(hub.store.settings(&thread.id).await?);
    // What a new session says it runs is not taken on trust: claude-agent-acp reports the model
    // named in settings.json while a chat's Claude Code was told to skip that file and runs its
    // default (`connections::lean`). So a new session is sent every pick; a resumed one only
    // what it does not have.
    let has = |state: &SessionState, config_id: &str, value: &serde_json::Value| resumed && state.current(config_id).as_ref() == Some(value);
    // An engine made of a harness and a provider starts on its model, unless the thread picked one.
    if let Some(wanted) = start_model
        && route_option.is_none()
        && let Some((config_id, value)) = model_choice(state, &wanted.to_lowercase())
        && !chosen.iter().any(|(id, _)| *id == config_id)
        && !has(state, &config_id, &json!(value))
    {
        state.set(conn, session_id, &config_id, json!(value)).await;
    }
    // A pick made by name (`~terra`, `~medium`: what a phone sends when it only knows the engine by
    // its family) is looked up among what the session offers and logged as the real choice, so the
    // log, the composer and later starts all see the id the engine knows.
    let mut named = Vec::new();
    for (config_id, value) in chosen {
        match value.as_str().and_then(|v| v.strip_prefix('~')) {
            Some(word) => {
                if let Some((id, real)) = by_name(state, &config_id, word) {
                    hub.emit_typed(&thread.id, &SettingData::new(id.clone(), real.clone()).by("name")).await?;
                    named.push((id, json!(real)));
                }
            }
            None => named.push((config_id, value)),
        }
    }
    let chosen = named;
    let chose: Vec<String> = chosen.iter().filter(|(id, v)| state.offers(id, v)).map(|(id, _)| id.clone()).collect();
    for (config_id, value) in chosen {
        if !has(state, &config_id, &value) {
            state.set(conn, session_id, &config_id, value).await;
        }
    }
    // A new thread runs in the engine's auto mode unless its start or its user chose another: Hub
    // answers the questions that still come, and the person is asked less.
    if !resumed && let Some((config_id, value)) = default_mode(hub, &thread.id, state).await? && !chose.contains(&config_id) {
        hub.emit_typed(&thread.id, &SettingData::new(config_id.clone(), value.clone()).by("default")).await?;
        if state.current(&config_id).as_ref() != Some(&json!(value)) {
            state.set(conn, session_id, &config_id, json!(value)).await;
        }
    }
    // A thread that runs without the user picks its settings by words, once: the pick is logged
    // as a setting, so later starts find it among the chosen ones.
    if let Some(unattended) = hub.store.unattended(&thread.id).await? {
        for (category, words) in &unattended.prefer {
            // Hub's provider pick stands in for the model; it was chosen already.
            if category == "model" && route_option.is_some() {
                continue;
            }
            let Some((config_id, value)) = preferred(state, category, words) else { continue };
            if chose.contains(&config_id) {
                continue;
            }
            hub.emit_typed(&thread.id, &SettingData::new(config_id.clone(), value.clone()).by("prefer")).await?;
            if !has(state, &config_id, &json!(value)) {
                state.set(conn, session_id, &config_id, json!(value)).await;
            }
        }
    }
    // Hub's provider and model pick stands in for whatever model setting the agent has.
    if let Some(option) = route_option {
        match serde_json::from_value::<SessionConfigOption>(option) {
            Ok(option) => {
                state.route = Some(option);
                state.keep_route();
            }
            Err(e) => tracing::warn!(thread = %thread.id, "the provider pick is not a config option: {e}"),
        }
    }
    Ok(())
}

/// Resumes the thread's session when the agent can and has it, else starts a new one. Also says
/// whether it resumed. The agent replays what was said while a session loads, which `replaying`
/// tells the update handler to set aside.
async fn open_session(
    conn: &ConnectionTo<Agent>,
    thread: &Thread,
    init: &agent_client_protocol::schema::v1::InitializeResponse,
    terminal_import: bool,
    (mcp, extra_dirs, meta): (Vec<McpServer>, Vec<std::path::PathBuf>, Option<serde_json::Map<String, serde_json::Value>>),
    (replaying, replayed): (&Arc<AtomicBool>, &Replayed),
) -> Result<(SessionId, SessionState, bool)> {
    if let Some(previous) = thread.engine_session_id.clone()
        && init.agent_capabilities.load_session
    {
        if terminal_import {
            *replayed.locked() = Some(Vec::new());
        }
        replaying.store(true, Ordering::Release);
        let loaded = conn
            .send_request(LoadSessionRequest::new(SessionId::new(previous.clone()), &thread.cwd).mcp_servers(mcp.clone()).additional_directories(extra_dirs.clone()).meta(meta.clone()))
            .block_task()
            .await;
        replaying.store(false, Ordering::Release);
        match loaded {
            Ok(loaded) => {
                let state = SessionState { config: loaded.config_options, modes: loaded.modes, route: None, deferred: Vec::new() };
                return Ok((SessionId::new(previous), state, true));
            }
            Err(e) => tracing::warn!(thread = %thread.id, "could not resume session: {}", e.message),
        }
    }
    let created = conn
        .send_request(NewSessionRequest::new(&thread.cwd).mcp_servers(mcp).additional_directories(extra_dirs).meta(meta))
        .block_task()
        .await
        .map_err(acp_err)?;
    let state = SessionState { config: created.config_options, modes: created.modes, route: None, deferred: Vec::new() };
    Ok((created.session_id, state, false))
}

/// A harness on a provider Hub picked: agents such as Hermes take the model only through
/// ACP's `session/set_model`, as `provider:model`, and not from their environment.
async fn set_harness_model(conn: &ConnectionTo<Agent>, thread: &Thread, session_id: &SessionId, model: &str) -> Result<()> {
    let ask = agent_client_protocol::UntypedMessage::new(
        "session/set_model",
        json!({ "sessionId": session_id.0.to_string(), "modelId": model }),
    )
    .map_err(acp_err)?;
    if let Err(e) = conn.send_request(ask).block_task().await {
        tracing::warn!(thread = %thread.id, "the agent did not take model {model}: {}", e.message);
    }
    Ok(())
}

/// Carries on with an engine a keeper held through a restart of the core (`keeper.rs`): no
/// `initialize` and no session to load, since both are done; the settings are the last ones the
/// log has. A turn the old core started is waited for, then the messages that were sent meanwhile
/// go in order.
pub(super) async fn session_taken_over(
    hub: Arc<Hub>,
    thread: Thread,
    route_option: Option<serde_json::Value>,
    attached: crate::keeper::Attached,
    unprompted: Arc<crate::engine::unprompted::Unprompted>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    queue: Parked,
    conn: ConnectionTo<Agent>,
) -> Result<()> {
    let session_id = SessionId::new(attached.session.clone().context("the keeper knows no session")?);
    let events = hub.store.events_of(&thread.id, 0, &[EventKind::SessionState]).await?;
    let last = events.last().and_then(Event::data_as::<SessionStateData>).unwrap_or_default();
    let mut state = SessionState {
        config: last.config_options.and_then(|options| serde_json::from_value(options).ok()),
        modes: last.modes.and_then(|modes| serde_json::from_value(modes).ok()),
        route: None,
        deferred: Vec::new(),
    };
    if let Some(option) = route_option.and_then(|o| serde_json::from_value::<SessionConfigOption>(o).ok()) {
        state.route = Some(option);
        state.keep_route();
    }
    let resumed = if attached.busy { hub.store.last_turn(&thread.id).await?.cut_off.or(Some(0)) } else { None };
    if resumed.is_none() {
        hub.set_status_because(&thread.id, ThreadStatus::Idle, "engine_taken_over").await?;
    }
    tracing::info!(thread = %thread.id, busy = attached.busy, "took the engine over from the core before");
    turns(&hub, &thread, &session_id, &mut state, commands, &queue, &conn, &unprompted, false, resumed).await
}
