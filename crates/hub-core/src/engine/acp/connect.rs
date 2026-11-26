//! Starting a thread's engine and opening the ACP connection to it: the provider route, launch
//! arguments and environment (keys, connections switched off, paid keys kept out), the installed
//! command, a keeper so the engine outlives a core restart (`keeper.rs`), and the handlers for the
//! agent's updates and permission asks. The session on that connection is `session.rs`.

use super::*;

pub(super) async fn connect(
    hub: Arc<Hub>,
    thread: Thread,
    engine: EngineConfig,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    parked: Parked,
    link: Option<crate::keeper::Link>,
) -> Result<()> {
    // When the engine's start began, for `SessionStarted.start_ms` (`timings.rs`).
    let began = std::time::Instant::now();
    let (route_option, by_set_model, engine) = pick_route(&hub, &thread, engine).await?;
    let launch = launch_for(&hub, &thread, &engine).await;
    let command = installed_command(&hub, &thread, &engine).await?;
    let agent = agent(&command, &launch.args)?;
    // Commands' output while they run, for engines that send it only at the end.
    let live = crate::live_output::Live::new(&hub.config, &thread.id);
    let config = engine_config(&hub, &thread, &engine, &launch, agent, &live).await?;
    let attached = link.as_ref().map(|l| l.attached.clone());
    let link = keeper_link(&hub, &thread, &config, link).await;
    hub.note_kept(&thread.id, link.is_some());
    let agent = AcpAgent::new(crate::supervise::contained(config)).with_debug(stderr_log(&hub, &thread.id));
    // While a session is loaded the agent replays its history, which is already in the log,
    // unless the thread was in a terminal: then the replay is kept to copy in what was said there.
    let replaying = Arc::new(AtomicBool::new(false));
    let replayed: Replayed = Arc::default();
    let thread_id = thread.id.clone();
    let command = engine.command.clone();
    let start_model = engine.model.clone().filter(|_| route_option.is_none() || by_set_model);

    let watching = tokio::spawn(live.clone().watch({
        let (hub, thread_id) = (hub.clone(), thread.id.clone());
        move |data| {
            let (hub, thread_id) = (hub.clone(), thread_id.clone());
            async move { hub.emit_typed(&thread_id, &data).await.map(|_| ()) }
        }
    }));
    let _watching = AbortOnDrop(watching);
    // Turns the engine starts on its own, with no prompt of Hub's (`unprompted.rs`).
    let unprompted = crate::engine::unprompted::Unprompted::new();
    // An engine taken over mid-turn goes on with the old core's prompt: what it says before
    // `turns` waits for that prompt's end is part of it, not a turn of its own.
    unprompted.prompting(attached.as_ref().is_some_and(|a| a.busy));
    let on_update = {
        let (hub, replaying, replayed, thread_id, live, unprompted) =
            (hub.clone(), replaying.clone(), replayed.clone(), thread_id.clone(), live.clone(), unprompted.clone());
        async move |n: SessionNotification, _cx| {
            if replaying.load(Ordering::Acquire) {
                if let Some(kept) = replayed.locked().as_mut() {
                    kept.push(serde_json::to_value(&n.update).unwrap_or_default());
                }
                return Ok(());
            }
            let data = serde_json::to_value(&n.update).unwrap_or_default();
            live.saw(&data);
            unprompted.saw(&hub, &thread_id, &data).await;
            take_update(&hub, &thread_id, data).await;
            Ok(())
        }
    };

    let on_permission = {
        let (hub, thread_id) = (hub.clone(), thread_id.clone());
        async move |req: RequestPermissionRequest,
                    responder: Responder<RequestPermissionResponse>,
                    cx: ConnectionTo<Agent>| {
            let data = json!({ "tool_call": req.tool_call, "options": req.options });
            let answer = hub.ask_permission(&thread_id, data).await.map_err(internal)?;
            // Waiting inline would stall every other message from the agent.
            cx.spawn(async move {
                let outcome = match answer.await.ok().flatten() {
                    Some(option) => {
                        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option))
                    }
                    None => RequestPermissionOutcome::Cancelled,
                };
                responder.respond(RequestPermissionResponse::new(outcome))
            })
        }
    };

    let builder = Client
        .builder()
        .name("hub")
        .on_receive_notification(on_update, agent_client_protocol::on_receive_notification!())
        .on_receive_request(on_permission, agent_client_protocol::on_receive_request!());
    let main = async move |conn: ConnectionTo<Agent>| {
        session(hub, thread, (command, start_model, route_option), launch.session_meta, (replaying, replayed, unprompted), commands, parked, (attached, began), conn).await.map_err(internal)
    };
    match link {
        Some(link) => builder.connect_with(link.into_lines(), main).await,
        None => builder.connect_with(agent, main).await,
    }
    .map_err(acp_err)
}

/// The engine's command as found on this machine, or an error that says what is missing: a
/// harness that is not installed fails to spawn with only "Internal error". One installed after
/// hubd started is found where a login shell would.
async fn installed_command(hub: &Hub, thread: &Thread, engine: &EngineConfig) -> Result<String> {
    let given = engine.command.clone();
    tokio::task::spawn_blocking(move || crate::engines::resolve(&given)).await?.ok_or_else(|| {
        let program = crate::project::split_command(&engine.command).into_iter().next().unwrap_or_default();
        anyhow::anyhow!(
            "{program} is not installed on {}, so {} cannot start here. Install it there (Settings > Engines), or switch to an engine this machine has.",
            hub.remotes.local_name(),
            thread.engine
        )
    })
}

/// A harness that reads keys from a file of its own over its environment would take an old key
/// from there (Hermes's ~/.hermes/.env): Hub's key wins.
async fn clear_stale_harness_keys(thread: &Thread, engine: &EngineConfig, keys: &[(String, String)]) -> Result<()> {
    if let (Some(file), Some(home)) =
        (crate::engines::harness(&thread.engine, engine).and_then(|h| h.key_file), crate::folders::home_dir())
        && !keys.is_empty()
    {
        let file = home.join(file);
        let given = keys.to_vec();
        match tokio::task::spawn_blocking(move || crate::engines::clear_stale_keys(&file, &given).map(|c| (c, file))).await? {
            Ok((true, file)) => tracing::info!(thread = %thread.id, "set aside an older key in {}", file.display()),
            Ok(_) => {}
            Err(e) => tracing::warn!(thread = %thread.id, "could not check the harness's own keys: {e:#}"),
        }
    }
    Ok(())
}

/// Pay-per-token keys in hubd's own environment that must not reach the engine and bill the API
/// instead of the subscription; one Hub sets on purpose still wins (`engines.rs`). Returned
/// as the pairs to blank out.
fn paid_keys_kept_out(thread: &Thread, engine: &EngineConfig, set: [&[(String, String)]; 4]) -> Vec<(String, String)> {
    let explicit: Vec<&str> = engine
        .env
        .keys()
        .map(String::as_str)
        .chain(set.iter().flat_map(|env| env.iter()).map(|(k, _)| k.as_str()))
        .collect();
    let stripped = crate::engines::inherited_paid_keys(&explicit, |k| std::env::var(k).ok());
    if !stripped.is_empty() {
        let names: Vec<&str> = stripped.iter().map(|(k, _)| k.as_str()).collect();
        tracing::info!(thread = %thread.id, "kept {} out of the engine's environment (subscriptions only)", names.join(", "));
    }
    stripped
}

/// A keeper this core took over from the one before, or a new one (`keeper.rs`), so the engine
/// lives through the next restart of the core; `None` when the engine is this core's child.
async fn keeper_link(hub: &Hub, thread: &Thread, config: &agent_client_protocol::AcpAgentConfig, link: Option<crate::keeper::Link>) -> Option<crate::keeper::Link> {
    match (link, crate::keeper::program(&hub.config)) {
        (Some(link), _) => Some(link),
        (None, Some(program)) => {
            let spec = crate::keeper::Spec {
                thread: thread.id.clone(),
                command: config.command().to_owned(),
                args: config.arguments().to_vec(),
                env: config.environment().clone(),
                log: engine_log(hub, &thread.id),
                data_dir: hub.config.data_dir.clone(),
            };
            match crate::keeper::start(&program, &spec).await {
                Ok(link) => Some(link),
                Err(err) => {
                    tracing::warn!(thread = %thread.id, "the engine runs without a keeper, so a restart cuts its turn off: {err:#}");
                    None
                }
            }
        }
        (None, None) => None,
    }
}

/// The provider and model the thread runs on, for a harness that runs on many (`routes.rs`):
/// Hub's pick as a config option (when there is one), whether the agent is also told the model
/// over ACP, and the engine as it starts.
async fn pick_route(hub: &Hub, thread: &Thread, engine: EngineConfig) -> Result<(Option<serde_json::Value>, bool, EngineConfig)> {
    // A harness that runs on many providers starts on the one the thread picked (`routes.rs`).
    let vault = hub.secrets.list().await.unwrap_or_default();
    // An engine started ahead of its thread (`warm.rs`) starts on what the composer showed.
    let settings = match crate::warm::pending(&thread.id) {
        Some(warm) => warm.settings,
        None => hub.store.settings(&thread.id).await?,
    };
    let route = crate::routes::picked(&settings).unwrap_or_else(|| crate::routes::default_route(&engine));
    let live = crate::routes::live_models(&hub.secrets, &thread.engine, &engine, &vault).await;
    let route_option = crate::routes::option(&thread.engine, &engine, &route, &vault, &live);
    // The pick as offered: an old model name gives way to one the provider lists now.
    let route = route_option.as_ref().and_then(|o| o["currentValue"].as_str().map(str::to_owned)).unwrap_or(route);
    // Whether the agent is also told the model over ACP (Hermes takes it no other way; Claude
    // Code on a provider reads it from its environment and would be sent a name it does not list).
    let by_set_model = crate::routes::sets_model(&thread.engine, &engine);
    let engine = if route_option.is_some() { crate::routes::apply(&thread.engine, &engine, &route, &vault) } else { engine };
    Ok((route_option, by_set_model, engine))
}

/// How the engine is launched: its arguments and environment, with the connections switched off
/// for Hub's threads and for this one, the extra folders, and the environment that says which
/// work a thread is.
async fn launch_for(hub: &Hub, thread: &Thread, engine: &EngineConfig) -> crate::connections::Launch {
    // Connections and plugins switched off for Hub's threads (`connections.rs`).
    let mut prefs = crate::connections::prefs(&hub.ctx()).await.unwrap_or_default();
    // What this one thread reaches: more folders, and connections switched off for it (`reach.rs`).
    let reach = crate::reach::get(&hub.ctx(), &thread.id).await.unwrap_or_default();
    if !reach.connections_off.is_empty() {
        let off = prefs.off.entry(thread.engine.clone()).or_default();
        off.extend(reach.connections_off.iter().cloned());
        off.sort();
        off.dedup();
    }
    let mut launch = crate::connections::launch(&prefs, &thread.engine, &engine.command);
    crate::connections::add_dirs(&mut launch, &engine.command, &crate::reach::directories(&reach, &thread.cwd));
    // Compaction and output style from Settings > Engines (`agent_prefs.rs`).
    let agent_prefs = crate::agent_prefs::for_folder(&crate::agent_prefs::prefs(&hub.ctx()).await.unwrap_or_default(), &thread.cwd);
    crate::connections::apply_agent_prefs(&mut launch, &agent_prefs, &engine.command);
    // An everyday chat starts Claude without the user's plugins and MCP servers, so it does not
    // wait for them before the first answer (`connections::lean`).
    if let Ok(root) = hub.projects_root().await
        && crate::connections::is_chat_folder(&root, &thread.cwd)
    {
        crate::connections::lean(&mut launch, &engine.command, crate::connections::settings_model().as_deref());
    } else {
        // A development thread tells Aftermath which work it is; an everyday chat has none, and
        // Aftermath is not asked to follow it.
        launch.env.push((crate::runtime::WORK_REF_VAR.into(), crate::runtime::work_ref(&thread.id)));
    }
    launch
}

/// The engine's launch config: its command and arguments with the environment every process it
/// starts inherits, so the project page can tell what a thread started without anyone tagging it
/// (see `runtime.rs`).
async fn engine_config(
    hub: &Hub,
    thread: &Thread,
    engine: &EngineConfig,
    launch: &crate::connections::Launch,
    agent: AcpAgent,
    live: &crate::live_output::Live,
) -> Result<agent_client_protocol::AcpAgentConfig> {
    // Every process the engine starts inherits these, so the project page can tell what a
    // thread started without anyone tagging it (see `runtime.rs`).
    // With Customs' registry on, the engine's package managers fetch through it.
    let registry = match crate::customs::prefs(&hub.ctx()).await {
        Ok(p) if p.registry => crate::registry_address::env(&crate::registry_address::base(&hub.config)),
        _ => Vec::new(),
    };
    let keys = hub.secrets.engine_env(engine).await?;
    clear_stale_harness_keys(thread, engine, &keys).await?;
    let api_env = hub.api_env(&thread.cwd).await;
    // Kept so a key replaced while the engine runs starts it again on the new one.
    let started_on = hub.secrets.used_in(&keys.iter().chain(&api_env).cloned().collect::<Vec<_>>());
    hub.engine_keys.locked().insert(thread.id.clone(), started_on);
    let signed_in = hub.sign_in_env(&thread.engine);
    let stripped = paid_keys_kept_out(thread, engine, [&keys, &launch.env, &api_env, &signed_in]);
    Ok(agent
        .into_config()
        .envs(stripped)
        .envs(live.env())
        .envs(registry)
        .envs(engine.env.clone())
        .envs(keys)
        // A sign-in Hub keeps for this engine here (`signin.rs`).
        .envs(signed_in)
        .envs(launch.env.clone())
        .envs(api_env)
        .env(crate::process_marks::THREAD_VAR, &thread.id)
        .env(crate::process_marks::PROJECT_VAR, &thread.cwd))
}
