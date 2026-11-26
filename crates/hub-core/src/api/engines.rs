//! Routes for the engines area: engines and harnesses, prefs, connections and plugins.

use super::*;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
        .route("/api/prefs/about", get(prefs_about))
        .route("/api/prefs/accounts", get(prefs_accounts))
        .route("/api/prefs/guard", get(guard_prefs).put(set_guard_prefs))
        .route("/api/prefs/permissions", get(permission_prefs).put(set_permission_prefs))
        .route("/api/prefs/threads", get(thread_prefs).put(set_thread_prefs))
        .route("/api/prefs/chatdetail", get(chat_detail).put(set_chat_detail))
        .route("/api/looks", get(looks).put(set_looks))
        .route("/api/prefs/config", get(prefs_config).put(edit_config))
        .route("/api/engines", get(engine_catalog).post(add_engine))
        .route("/api/engines/test", post(test_engine))
        .route("/api/providers/{id}/test", post(test_provider))
        .route("/api/engines/install", get(install_jobs).post(install_harness))
        .route("/api/harnesses/{id}/settings", get(harness_settings).put(set_harness_setting))
        .route("/api/engines/receive", post(receive_engines))
        .route("/api/clis", get(tools_here))
        .route("/api/clis/update", post(update_tool))
        .route("/api/engines/machines", get(engine_machines))
        .route("/api/engines/share", post(share_engines))
        .route("/api/engines/{name}", axum::routing::delete(remove_engine))
        .route("/api/prefs/restart", post(restart_core))
        .route("/api/prefs/disk", get(disk_usage).post(free_disk))
        .route("/api/prefs/notes", get(list_notes).post(write_notes))
        .route("/api/prefs/tools", get(|| async { blocking(|| Ok(crate::prefs::tools())).await.map(Json) }))
        .route("/api/prefs/ssh", get(|| async { blocking(|| Ok(crate::prefs::ssh())).await.map(Json) }))
        .route("/api/models", get(model_table))
        .route("/api/connections", get(connections))
        .route("/api/connections/prefs", get(connection_prefs).put(set_connection_prefs))
        .route("/api/connections/doctor", post(engine_doctor))
        .route("/api/agent-prefs", get(agent_prefs).put(set_agent_prefs))
        .route("/api/handoff-prefs", get(handoff_prefs).put(set_handoff_prefs))
        .route("/api/local-models", get(|| async { Json(crate::agent_prefs::local_models().await) }))
        .route("/api/threads/{id}/connections", get(thread_connections))
        .route("/api/engines/clis", get(engine_clis))
        .route("/api/engines/{name}/sign-in", post(engine_signed_in).delete(forget_sign_in))
        .route("/api/engines/{name}/login", post(start_login).get(login_job))
        .route("/api/sign-ins/pending", get(pending_sign_ins))
        .route("/api/plugins/discover", get(plugins_discover))
        .route("/api/plugins/search", get(plugins_search))
        .route("/api/plugins/repo", get(plugins_repo))
        .route("/api/plugins/install", post(plugins_install))
        .route("/api/plugins/ready", post(plugins_ready))
}

/// The engines with a CLI of their own, which a machine's page can open there.
async fn engine_clis(State(hub): State<Arc<Hub>>) -> impl IntoResponse {
    let clis: Vec<crate::model::EngineCli> = hub
        .config
        .engines
        .iter()
        .filter_map(|(engine, config)| {
            let cli = config.terminal.as_deref().and_then(crate::machine::engine_cli)?;
            Some(crate::model::EngineCli { engine: engine.clone(), cli: cli.to_owned() })
        })
        .collect();
    Json(clis)
}

async fn plugins_discover(State(hub): State<Arc<Hub>>, Query(q): Query<PluginsQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::plugins::discover(&hub.ctx(), q.fresh).await))
}

async fn plugins_search(State(hub): State<Arc<Hub>>, Query(q): Query<PluginsQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::plugins::search(&hub.ctx(), &q.q).await))
}

async fn plugins_repo(State(hub): State<Arc<Hub>>, Query(q): Query<PluginsQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::plugins::detail(&hub.ctx(), &q.name).await?))
}

async fn prefs_about(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let version = hub.update_status().version;
    Ok(Json(blocking(move || Ok(crate::prefs::about(&hub.config, &version))).await?))
}

async fn permission_prefs(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.permission_prefs().await?))
}

/// Settings > Safety. A phone may only make it stricter (`auth::loosens`): a lost phone must not be
/// able to turn the questions off and then have an agent run anything.
async fn set_permission_prefs(
    State(hub): State<Arc<Hub>>,
    narrow: Option<axum::Extension<crate::auth::Narrow>>,
    Json(prefs): Json<crate::model::PermissionPrefs>,
) -> ApiResult<impl IntoResponse> {
    if narrow.is_some() && crate::auth::loosens(&hub.permission_prefs().await?, &prefs) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "a phone can only make Safety stricter; loosen it on the computer".into()));
    }
    Ok(Json(hub.set_permission_prefs(prefs).await?))
}

async fn guard_prefs(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.guard_prefs().await?))
}

async fn set_guard_prefs(
    State(hub): State<Arc<Hub>>,
    Json(prefs): Json<crate::usage::GuardPrefs>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.set_guard_prefs(prefs).await?))
}

async fn thread_prefs(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.thread_prefs().await?))
}

async fn set_thread_prefs(
    State(hub): State<Arc<Hub>>,
    Json(prefs): Json<crate::triage::ThreadPrefs>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.set_thread_prefs(prefs).await?))
}

async fn chat_detail(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.chat_detail().await?))
}

async fn set_chat_detail(
    State(hub): State<Arc<Hub>>,
    Json(detail): Json<crate::triage::ChatDetail>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.set_chat_detail(detail).await?))
}

async fn looks(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.looks().await?))
}

async fn set_looks(State(hub): State<Arc<Hub>>, Json(looks): Json<crate::model::Looks>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.set_looks(looks).await?))
}

async fn prefs_config(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let busy = hub.busy().await?;
    let version = hub.update_status().version;
    Ok(Json(blocking(move || crate::prefs::core_settings(&hub.config, &version, busy)).await?))
}

async fn edit_config(
    State(hub): State<Arc<Hub>>,
    Json(edit): Json<crate::model::ConfigEdit>,
) -> ApiResult<impl IntoResponse> {
    let busy = hub.busy().await?;
    let version = hub.update_status().version;
    // Dictation reads its mute key on each use, so this one needs no restart.
    if edit.key == "voice.mute_key" {
        hub.voice.set_mute_key(edit.value.as_str().map(str::to_owned));
    }
    if edit.key == "voice.polish" {
        hub.voice.set_polish(edit.value.as_bool().unwrap_or(false));
    }
    if edit.key == "voice.language" {
        hub.voice.set_language(edit.value.as_str().unwrap_or_default());
    }
    let settings = blocking(move || {
        crate::prefs::edit_config(&edit)?;
        crate::prefs::core_settings(&hub.config, &version, busy)
    })
    .await?;
    Ok(Json(settings))
}

/// The harnesses and providers engines are made of, and the engines there are (`engines.rs`).
async fn engine_catalog(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let keys = hub.secrets.list().await?;
    Ok(Json(blocking(move || crate::engines::catalog(&hub.config, &keys)).await?))
}

/// Tries an engine before it is added: the key with its provider, then a session with the agent.
/// Asks a provider's API for its models with the key the vault has for it.
async fn test_provider(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::routes::test_provider(&hub.secrets, &id).await?))
}

async fn test_engine(
    State(hub): State<Arc<Hub>>,
    Json(draft): Json<crate::engines::EngineDraft>,
) -> ApiResult<impl IntoResponse> {
    let (name, engine, given) = crate::engines::build(&draft)?;
    let mut secrets = hub.secrets.engine_env(&engine).await?;
    if let Some((key, value)) = given {
        secrets.retain(|(n, _)| *n != key);
        secrets.push((key, value));
    }
    Ok(Json(crate::engine_probe::test(&name, &engine, secrets).await))
}

/// Adds an engine to `hub.json`, its key to the vault. It runs after the core restarts.
async fn add_engine(
    State(hub): State<Arc<Hub>>,
    Json(draft): Json<crate::engines::EngineDraft>,
) -> ApiResult<impl IntoResponse> {
    let (name, engine, given) = crate::engines::build(&draft)?;
    if let Some((key, value)) = given {
        hub.secrets.set(&key, &value).await?;
    }
    blocking(move || crate::engines::save(&name, &engine)).await?;
    crate::engines::share_soon();
    engine_catalog(State(hub)).await
}

/// A harness's own settings on this machine, read through its own program (`harness_settings.rs`).
async fn harness_settings(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::harness_settings::read(&hub.store, crate::engines::harness_program, &id).await))
}

/// Changes one of a harness's settings here; the other machines get it with the engines.
async fn set_harness_setting(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(edit): Json<HarnessSettingEdit>,
) -> ApiResult<impl IntoResponse> {
    crate::harness_settings::write(&hub.store, crate::engines::harness_program, &id, &edit.key, edit.value).await?;
    crate::engines::share_soon();
    Ok(Json(crate::harness_settings::read(&hub.store, crate::engines::harness_program, &id).await))
}

/// Engines another machine added, sent here to keep the list the same everywhere.
async fn receive_engines(
    State(hub): State<Arc<Hub>>,
    Json(shipment): Json<crate::engine_share::Shipment>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::engine_share::receive(&hub, shipment).await?))
}

/// How the engines stand on each of the other machines, as they last answered.
async fn engine_machines(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::engine_share::machines(&hub)))
}

/// Sends the engines to the other machines now and answers how they stand there.
async fn share_engines(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    crate::engine_share::share_once(&hub).await;
    Ok(Json(crate::engine_share::machines(&hub)))
}

/// This machine's agent CLIs, their versions and how each is updated here.
async fn tools_here(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::updates::ToolsHere { machine: hub.remotes.local_name().to_owned(), tools: crate::updates::here().await }))
}

/// Updates one CLI on this machine, in the background.
async fn update_tool(Json(ask): Json<ToolAsk>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::updates::start_update(&ask.tool).await?))
}

/// Installs and the ones running, so the settings page can show how far each got.
async fn install_jobs() -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::engines::install_jobs()))
}

/// Runs a harness's own installer in the background; the page follows it with `install_jobs`.
async fn install_harness(Json(ask): Json<InstallAsk>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::engines::start_install(&ask.harness)?))
}

async fn remove_engine(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    blocking(move || crate::engines::remove(&name)).await?;
    crate::engines::share_soon();
    engine_catalog(State(hub)).await
}

/// Starts a fresh core so changed settings apply. Threads that are working carry on in it.
async fn restart_core() -> ApiResult<impl IntoResponse> {
    crate::prefs::request_restart();
    Ok(StatusCode::ACCEPTED)
}

/// What Hub keeps on disk and how much room is left.
async fn disk_usage(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let (data, cfg) = (hub.config.data_dir.clone(), hub.config.update.clone());
    let usage = blocking(move || {
        let src = cfg.source_in(&data);
        Ok(crate::disk::usage(&data, &src, &crate::update::target_dir(&cfg, &src)))
    })
    .await?;
    Ok(Json(usage))
}

/// Empties what the build folder can do without; the next update rebuilds it.
async fn free_disk(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    if hub.update_status().state == crate::model::UpdateState::Building {
        return Err(anyhow::anyhow!("an update is building right now; try again when it is done").into());
    }
    let (data, cfg) = (hub.config.data_dir.clone(), hub.config.update.clone());
    let usage = blocking(move || {
        let src = cfg.source_in(&data);
        let target = crate::update::target_dir(&cfg, &src);
        crate::disk::trim_build(&target, 0);
        Ok(crate::disk::usage(&data, &src, &target))
    })
    .await?;
    Ok(Json(usage))
}

async fn list_notes(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::notes::list(&hub.store).await?))
}

/// Writes patch notes for the running version now, e.g. when it was installed by hand.
async fn write_notes(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let src = crate::prefs::source_dir(&hub.config).ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no source checkout found".into()))?;
    let version = hub.update_status().version;
    let head = if version == "dev" { "HEAD".to_owned() } else { version.clone() };
    // From the last notes the model wrote, so versions that kept only their commits are told too.
    let written = crate::notes::last_written(&hub.store, Some(&version)).await.map(|n| n.version);
    let from = match written.or(crate::notes::previous(&hub.store, &version).await) {
        Some(v) => Some(v),
        // Nothing to compare with: the last day of work.
        None => crate::notes::start_of_recent(&src, &head),
    };
    Ok(Json(crate::notes::record(&hub.store, &src, from.as_deref(), &head, &version, |p| crate::engine::ask_tidy(&hub.config, "notes", p, crate::notes::WAIT)).await?))
}

async fn prefs_accounts(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || Ok(crate::prefs::accounts(&hub.config))).await?))
}

/// `hubd sign-in <engine>` finished in a terminal here (`signin.rs`).
async fn engine_signed_in(
    State(hub): State<Arc<Hub>>,
    Path(name): Path<String>,
    Json(body): Json<SignedIn>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.signed_in(&name, body.token.as_deref()).await?))
}

/// Signs an engine in here with a link and a code the app shows (`login.rs`).
async fn start_login(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::login::start(hub, &name)?))
}

/// Sign-ins waiting for a code on the other machines (`autologin.rs`).
async fn pending_sign_ins() -> impl IntoResponse {
    Json(crate::autologin::pending())
}

async fn login_job(Path(name): Path<String>) -> impl IntoResponse {
    Json(crate::login::job(&name))
}

async fn forget_sign_in(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    hub.forget_sign_in(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn plugins_install(State(hub): State<Arc<Hub>>, Json(ask): Json<WorkerInstallAsk>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::plugins::install_worker(&hub.ctx(), &ask.repo).await?))
}

/// A Worker's ready check again, for a folder that holds one of your tools.
async fn plugins_ready(State(hub): State<Arc<Hub>>, Json(ask): Json<ReadyAsk>) -> ApiResult<impl IntoResponse> {
    let wanted = std::fs::canonicalize(&ask.folder).unwrap_or_else(|_| std::path::PathBuf::from(&ask.folder));
    let known = crate::own_tools::folders(&hub.ctx()).await.into_iter().any(|f| std::fs::canonicalize(&f).unwrap_or(f) == wanted);
    if !known {
        return Err(anyhow::anyhow!("no tool of yours in {}", ask.folder).into());
    }
    Ok(Json(crate::plugins::ready(&wanted).await))
}

/// Every engine's MCP servers and connectors, and which of their tools threads called.
async fn connections(State(hub): State<Arc<Hub>>, Query(q): Query<ModelsQuery>) -> ApiResult<Json<crate::model::Connections>> {
    Ok(Json(crate::connections::all(&hub.ctx(), q.refresh).await?))
}

async fn connection_prefs(State(hub): State<Arc<Hub>>) -> ApiResult<Json<crate::model::ConnectionPrefs>> {
    Ok(Json(crate::connections::prefs(&hub.ctx()).await?))
}

/// What Hub switches off in the engines it starts; engines already running keep what they had.
async fn set_connection_prefs(
    State(hub): State<Arc<Hub>>,
    Json(prefs): Json<crate::model::ConnectionPrefs>,
) -> ApiResult<Json<crate::model::ConnectionPrefs>> {
    Ok(Json(crate::connections::set_prefs(&hub.ctx(), prefs).await?))
}

async fn agent_prefs(State(hub): State<Arc<Hub>>) -> ApiResult<Json<crate::model::AgentPrefs>> {
    Ok(Json(crate::agent_prefs::prefs(&hub.ctx()).await?))
}

/// How Hub starts agents from now on; engines already running keep what they had.
async fn set_agent_prefs(
    State(hub): State<Arc<Hub>>,
    Json(prefs): Json<crate::model::AgentPrefs>,
) -> ApiResult<Json<crate::model::AgentPrefs>> {
    Ok(Json(crate::agent_prefs::set_prefs(&hub.ctx(), prefs).await?))
}

async fn handoff_prefs(State(hub): State<Arc<Hub>>) -> ApiResult<Json<crate::model::HandoffPrefs>> {
    Ok(Json(crate::handoff::prefs(&hub.ctx()).await?))
}

/// Who does which kind of task from now on (`handoff.rs`); work already started keeps its engine.
async fn set_handoff_prefs(
    State(hub): State<Arc<Hub>>,
    Json(prefs): Json<crate::model::HandoffPrefs>,
) -> ApiResult<Json<crate::model::HandoffPrefs>> {
    Ok(Json(crate::handoff::set_prefs(&hub.ctx(), prefs).await?))
}

/// The engine's own check of its installation, such as `claude doctor`.
async fn engine_doctor(State(hub): State<Arc<Hub>>, Json(q): Json<DoctorQuery>) -> ApiResult<Json<crate::model::DoctorReport>> {
    let command = hub
        .config
        .engines
        .get(&q.engine)
        .map(|e| e.command.clone())
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("no engine {}", q.engine)))?;
    let home = crate::prefs::home().unwrap_or_default();
    Ok(Json(blocking(move || crate::connections::doctor(&command, &home)).await?))
}

async fn thread_connections(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Query(q): Query<ModelsQuery>,
) -> ApiResult<Json<crate::model::ThreadConnections>> {
    Ok(Json(crate::connections::of_thread(&hub.ctx(), &id, q.refresh).await?))
}

/// What each model the engines offer can do, from the public model list.
async fn model_table(State(hub): State<Arc<Hub>>, Query(q): Query<ModelsQuery>) -> ApiResult<Json<crate::model::ModelTable>> {
    Ok(Json(hub.model_table(q.refresh).await?))
}
