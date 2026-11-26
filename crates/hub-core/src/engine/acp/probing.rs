//! ACP calls outside a thread, and picking among what a session offers. `complete` runs one
//! prompt in a throwaway session (nothing logged, every permission refused); `probe` asks an
//! engine what it is and which models it offers without a prompt, for trying it before it is
//! saved. Then the choosers a thread's session uses: model, mode (auto by default) and effort.

use super::*;

/// Runs one prompt in a throwaway session and returns the reply. Nothing is logged and every
/// permission request is rejected, so the agent can only answer in text. `model` picks the first
/// model choice whose id or name contains it, when the engine offers model choices.
pub(crate) async fn complete(
    purpose: &'static str,
    engine: &EngineConfig,
    cwd: &str,
    model: Option<&str>,
    prompt: String,
) -> Result<String> {
    let shared = crate::secrets::shared_engine_env(engine);
    let signed_in = crate::secrets::signed_in_env(&engine.command);
    let config = agent(&engine.command, &[])?
        .into_config()
        .envs(without_inherited_paid_keys(engine, [&shared, &signed_in]))
        .envs(engine.env.clone())
        .envs(shared)
        .envs(signed_in);
    let agent = AcpAgent::new(config);
    let reply = Arc::new(std::sync::Mutex::new(String::new()));

    let on_update = {
        let reply = reply.clone();
        async move |n: SessionNotification, _cx| {
            let data = serde_json::to_value(&n.update).unwrap_or_default();
            if crate::model::update_kind(&data) == Some("agent_message_chunk")
                && let Some(text) = data["content"]["text"].as_str()
            {
                reply.locked().push_str(text);
            }
            Ok(())
        }
    };
    let on_permission = async move |_req: RequestPermissionRequest,
                                    responder: Responder<RequestPermissionResponse>,
                                    _cx: ConnectionTo<Agent>| {
        responder.respond(RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled))
    };

    // What the slip needs, kept for the moment the session exists.
    let (slip_engine, slip_model, prompt_text) = (engine.clone(), model.map(str::to_owned), prompt.clone());
    let (cwd, model) = (cwd.to_owned(), model.map(str::to_lowercase));
    let start = engine.model.as_deref().map(str::to_lowercase);
    Client
        .builder()
        .name("hub")
        .on_receive_notification(on_update, agent_client_protocol::on_receive_notification!())
        .on_receive_request(on_permission, agent_client_protocol::on_receive_request!())
        .connect_with(agent, async move |conn: ConnectionTo<Agent>| {
            // Notices advertised so an engine's warnings stay out of the reply text.
            conn.send_request(InitializeRequest::new(ProtocolVersion::V1).client_capabilities(capabilities()))
                .block_task()
                .await?;
            let created = conn.send_request(NewSessionRequest::new(&cwd)).block_task().await?;
            // The slip goes in once the session exists, so it can say which one it was.
            crate::package::asked(purpose, &slip_engine, slip_model.as_deref(), &created.session_id.0, &prompt_text);
            let mut state = SessionState { config: created.config_options, modes: created.modes, route: None, deferred: Vec::new() };
            // The engine's own model first; a helper's model wins when the engine offers it.
            for model in [start, model].into_iter().flatten() {
                if let Some((id, value)) = model_choice(&state, &model) {
                    state.set(&conn, &created.session_id, &id, json!(value)).await;
                }
            }
            let prompt = PromptRequest::new(
                created.session_id,
                vec![ContentBlock::Text(TextContent::new(prompt))],
            );
            conn.send_request(prompt).block_task().await?;
            Ok(())
        })
        .await
        .map_err(acp_err)?;
    let reply = reply.locked().trim().to_owned();
    if reply.is_empty() {
        anyhow::bail!("the engine gave no answer");
    }
    Ok(reply)
}

/// What an engine says about itself when started and asked for a session, without a prompt:
/// nothing is spent. For trying an engine before it is saved.
pub(crate) struct Probe {
    /// The agent's own name and version, if it gives them.
    pub agent: Option<String>,
    /// Every model it offers, as (value, name).
    pub models: Vec<(String, String)>,
    /// The model it starts on, after the engine's `model` was picked.
    pub current: Option<String>,
}

/// Blanks for the pay-per-token keys hubd's environment would hand an engine, for the short
/// side calls (`complete`, `probe`) that build their own environment (`engines::inherited_paid_keys`).
pub(super) fn without_inherited_paid_keys<'a>(
    engine: &EngineConfig,
    given: impl IntoIterator<Item = &'a Vec<(String, String)>>,
) -> Vec<(String, String)> {
    let explicit: Vec<&str> =
        engine.env.keys().map(String::as_str).chain(given.into_iter().flatten().map(|(k, _)| k.as_str())).collect();
    crate::engines::inherited_paid_keys(&explicit, |k| std::env::var(k).ok())
}

pub(crate) async fn probe(engine: &EngineConfig, cwd: &str, secrets: Vec<(String, String)>) -> Result<Probe> {
    let config = agent(&engine.command, &[])?
        .into_config()
        .envs(without_inherited_paid_keys(engine, [&secrets]))
        .envs(engine.env.clone())
        .envs(secrets);
    let agent = AcpAgent::new(config);
    let on_permission = async move |_req: RequestPermissionRequest,
                                    responder: Responder<RequestPermissionResponse>,
                                    _cx: ConnectionTo<Agent>| {
        responder.respond(RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled))
    };
    let (cwd, wanted) = (cwd.to_owned(), engine.model.as_deref().map(str::to_lowercase));
    let found = Arc::new(std::sync::Mutex::new(None));
    let out = found.clone();
    Client
        .builder()
        .name("hub")
        .on_receive_request(on_permission, agent_client_protocol::on_receive_request!())
        .connect_with(agent, async move |conn: ConnectionTo<Agent>| {
            let init = conn.send_request(InitializeRequest::new(ProtocolVersion::V1)).block_task().await?;
            let created = conn.send_request(NewSessionRequest::new(&cwd)).block_task().await?;
            let mut state = SessionState { config: created.config_options, modes: created.modes, route: None, deferred: Vec::new() };
            if let Some(wanted) = wanted
                && let Some((id, value)) = model_choice(&state, &wanted)
            {
                state.set(&conn, &created.session_id, &id, json!(value)).await;
            }
            let agent = serde_json::to_value(&init.agent_info).ok().and_then(|a| {
                let name = a["title"].as_str().or(a["name"].as_str())?.to_owned();
                Some(match a["version"].as_str() {
                    Some(v) => format!("{name} {v}"),
                    None => name,
                })
            });
            let (models, current) = match model_options(&state) {
                Some((id, offered)) => (offered, state.current(&id).and_then(|v| v.as_str().map(str::to_owned))),
                None => (Vec::new(), None),
            };
            *out.locked() = Some(Probe { agent, models, current });
            Ok(())
        })
        .await
        .map_err(acp_err)?;
    let probe = found.locked().take();
    probe.context("the engine closed before it answered")
}

/// The model option and the first of its choices that matches `wanted`.
/// The setting that picks the model, and every model it offers as (value, name).
pub(super) fn model_options(state: &SessionState) -> Option<(String, Vec<(String, String)>)> {
    state.config.iter().flatten().find_map(|o| {
        let o = serde_json::to_value(o).ok()?;
        if o["category"] != "model" {
            return None;
        }
        let flat = o["options"].as_array()?.iter().flat_map(|c| match c["options"].as_array() {
            Some(group) => group.clone(),
            None => vec![c.clone()],
        });
        let offered = flat
            .filter_map(|c| Some((c["value"].as_str()?.to_owned(), c["name"].as_str().unwrap_or_default().to_owned())))
            .collect();
        Some((o["id"].as_str()?.to_owned(), offered))
    })
}

/// The setting of `category` ("model", "mode", ...) the engine offers and its choice that
/// `words` name first: for each word in order, a choice whose id or name is the word, else one
/// that contains it. Modes an agent lists apart from its settings count as the "mode" setting.
pub(super) fn preferred(state: &SessionState, category: &str, words: &[String]) -> Option<(String, String)> {
    // (config id, choice id, choice name) of every choice of that category.
    let mut choices: Vec<(String, String, String)> = Vec::new();
    for o in state.config.iter().flatten() {
        let Ok(o) = serde_json::to_value(o) else { continue };
        if o["category"] != category {
            continue;
        }
        let Some(id) = o["id"].as_str() else { continue };
        for c in o["options"].as_array().into_iter().flatten() {
            for c in c["options"].as_array().cloned().unwrap_or_else(|| vec![c.clone()]) {
                if let Some(value) = c["value"].as_str() {
                    choices.push((id.to_owned(), value.to_owned(), c["name"].as_str().unwrap_or_default().to_owned()));
                }
            }
        }
    }
    if choices.is_empty() && category == "mode" && let Some(modes) = &state.modes {
        for m in &modes.available_modes {
            choices.push(("mode".into(), m.id.0.to_string(), m.name.clone()));
        }
    }
    words.iter().map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).find_map(|w| {
        let is = |(_, value, name): &&(String, String, String)| value.to_lowercase() == w || name.to_lowercase() == w;
        let has = |(_, value, name): &&(String, String, String)| value.to_lowercase().contains(&w) || name.to_lowercase().contains(&w);
        choices.iter().find(is).or_else(|| choices.iter().find(has)).map(|(id, value, _)| (id.clone(), value.clone()))
    })
}

/// The words a new thread's permission mode is picked by: the engine's auto mode.
///
/// "auto" is Claude's and Codex's; "dont_ask" is Hermes's, which has no mode named auto and whose
/// other two ask before edits.
pub(super) const DEFAULT_MODE: &[&str] = &["auto", "dont_ask"];

/// The mode a thread starts in when nobody picked one: the engine's auto mode, for an ordinary
/// thread. Threads whose permission answers Hub decides itself (design mode, Vakten, a routine's
/// run, a fix thread that picks its own words) keep the engine's default.
pub(super) async fn default_mode(hub: &Hub, thread_id: &str, state: &SessionState) -> Result<Option<(String, String)>> {
    if hub.store.unattended(thread_id).await?.is_some()
        || crate::design::is_on(&hub.ctx(), thread_id).await
        || crate::vakten_thread::is_vakten(&hub.store, thread_id).await
        || matches!(crate::routines::working_run(&hub.ctx(), thread_id).await, Ok(Some(_)))
    {
        return Ok(None);
    }
    let words: Vec<String> = DEFAULT_MODE.iter().map(|w| (*w).to_owned()).collect();
    Ok(preferred(state, "mode", &words))
}

/// The choice of the setting `config_id` that `word` names: a model for `model`, a thinking level
/// for the effort settings.
pub(super) fn by_name(state: &SessionState, config_id: &str, word: &str) -> Option<(String, String)> {
    let category = if config_id == "model" { "model" } else { "thought_level" };
    preferred(state, category, &[word.to_owned()])
}

pub(super) fn model_choice(state: &SessionState, wanted: &str) -> Option<(String, String)> {
    state.config.iter().flatten().find_map(|o| {
        let o = serde_json::to_value(o).ok()?;
        if o["category"] != "model" {
            return None;
        }
        let choices = o["options"].as_array()?;
        let flat = choices.iter().flat_map(|c| match c["options"].as_array() {
            Some(group) => group.clone(),
            None => vec![c.clone()],
        });
        flat.into_iter()
            .find(|c| {
                [&c["value"], &c["name"]]
                    .iter()
                    .any(|s| s.as_str().is_some_and(|s| s.to_lowercase().contains(wanted)))
            })
            .and_then(|c| Some((o["id"].as_str()?.to_owned(), c["value"].as_str()?.to_owned())))
    })
}
