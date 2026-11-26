//! Routes for the machines area: this machine's host view, the other machines (hosts, tailnet, LAN),
//! terminals, the tests another machine sends here (checks), readiness and the iPhone simulators.

use crate::locked::ResultExt;
use super::*;
use axum::http::HeaderName;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
        .route("/api/sim", post(|State(hub): State<Arc<Hub>>| async move { hub.start_sim().map(Json).map_err(ApiError::from) }).delete(|State(hub): State<Arc<Hub>>| async move { hub.clear_sim().await.map(Json).map_err(ApiError::from) }))
        .route("/api/host", get(host))
        .route("/api/dist", get(dist))
        .route("/api/host/about", get(host_about))
        .route("/api/host/processes/{pid}/stop", post(stop_process))
        .route("/api/host/processes/{pid}/pause", post(pause_process))
        .route("/api/host/processes/{pid}/resume", post(resume_process))
        .route("/api/threads/{id}/terminal", post(open_terminal))
        .route("/api/hosts", get(list_hosts).post(add_host))
        .route("/api/hosts/{name}", axum::routing::delete(remove_host).patch(change_host))
        .route("/api/hosts/{name}/reconnect", post(reconnect_host))
        .route("/api/hosts/{name}/key", post(let_in_host))
        .route("/api/hosts/{name}/install", post(install_host))
        .route("/api/hosts/{name}/update", post(update_host))
        .route("/api/checks", get(checks_status))
        .route("/api/checks/prepare", post(checks_prepare))
        .route("/api/checks/source", post(checks_source))
        .route("/api/checks/run", post(checks_run))
        .route("/api/tailnet", get(tailnet))
        .route("/api/tailnet/{name}/dismiss", post(dismiss_tailnet))
        .route("/api/tailnet/setup", post(set_up_host))
        .route("/api/lan", get(lan))
        .route("/api/hosts/{name}/tailnet", post(join_tailnet))
        .route("/api/hosts/{name}/open", post(open_on_machine))
        .route("/api/hosts/{name}/terminals", post(open_terminal_on_machine))
        .route("/api/terminals", get(terminals))
        .route("/api/terminals/{id}", axum::routing::delete(close_terminal))
        .route("/api/terminals/{id}/ws", get(terminal_socket))
        .route("/api/terminals/{id}/host", get(terminal_host_socket))
        .route("/api/machine/routes", get(machine_routes))
        .route("/api/machine/ready", get(machine_ready))
        .route("/api/machine/credentials/check", post(credentials_check))
        .route("/api/simview", get(simview))
        .route("/api/simview/frame", get(simview_frame))
        .route("/api/simview/live", post(simview_live))
        .route("/api/simview/boot", post(simview_boot))
        .route("/api/simview/input", post(simview_input))
        .route("/api/simview/app", post(simview_app))
        .route("/api/simview/shutdown", post(simview_shutdown))
        .route("/api/machine/git-identity", post(set_git_identity))
}

async fn list_hosts(State(hub): State<Arc<Hub>>) -> Json<Vec<crate::model::HostInfo>> {
    Json(hub.remotes.list())
}

async fn add_host(State(hub): State<Arc<Hub>>, Json(new): Json<NewHost>) -> ApiResult<impl IntoResponse> {
    Ok((StatusCode::CREATED, Json(hub.remotes.add(new).await?)))
}

async fn remove_host(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    hub.remotes.remove(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn change_host(State(hub): State<Arc<Hub>>, Path(name): Path<String>, Json(settings): Json<crate::model::HostSettings>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.remotes.change(&name, settings).await?))
}

async fn reconnect_host(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    hub.remotes.reconnect(&name)?;
    Ok(StatusCode::ACCEPTED)
}

async fn let_in_host(
    State(hub): State<Arc<Hub>>,
    Path(name): Path<String>,
    Json(body): Json<crate::model::LetInRequest>,
) -> ApiResult<impl IntoResponse> {
    hub.remotes.let_in(&name, &body.password).await?;
    Ok(StatusCode::ACCEPTED)
}

/// The machines on the tailnet, for setting up new ones.
async fn tailnet(State(hub): State<Arc<Hub>>) -> Json<crate::model::Tailnet> {
    Json(hub.remotes.tailnet(&hub.store).await)
}

/// Adds a machine found on the tailnet and installs hubd there when it has none.
async fn set_up_host(State(hub): State<Arc<Hub>>, Json(new): Json<NewHost>) -> ApiResult<Json<crate::model::SetUpAnswer>> {
    Ok(Json(hub.remotes.set_up(new).await?))
}

/// Machines on the home network that answer SSH.
async fn lan(State(hub): State<Arc<Hub>>) -> ApiResult<Json<Vec<crate::model::LanMachine>>> {
    let ssh = tokio::task::spawn_blocking(|| crate::prefs::ssh().hosts).await.unwrap_or_default();
    let hosts = hub.remotes.identities(&ssh);
    Ok(Json(crate::lan::scan(&hosts).await?))
}

/// Installs Tailscale on a machine SSH reaches and starts it; the sign-in link shows on its row.
async fn join_tailnet(
    State(hub): State<Arc<Hub>>,
    Path(name): Path<String>,
    Json(req): Json<crate::model::InstallRequest>,
) -> ApiResult<Json<crate::model::InstallAnswer>> {
    let sudo = hub.remotes.join_tailnet(&name, req.password).await?;
    Ok(Json(crate::model::InstallAnswer { sudo, build: None }))
}

/// What opening `what` on machine `name` runs here, and its title: ssh to the machine's alias, or
/// the program itself for this computer.
fn machine_plan(hub: &Hub, name: &str, what: crate::model::MachineOpen, engine: Option<&str>, thread: Option<&str>) -> anyhow::Result<(String, Vec<String>)> {
    let alias = if name.eq_ignore_ascii_case(hub.remotes.local_name()) {
        None
    } else {
        let alias = hub.remotes.ssh_alias(name).ok_or_else(|| anyhow::anyhow!("no machine called {name}"))?;
        Some(alias.ok_or_else(|| anyhow::anyhow!("{name} has no SSH alias, so no terminal can log in there"))?)
    };
    if what == crate::model::MachineOpen::Attach {
        let thread = thread.ok_or_else(|| anyhow::anyhow!("which thread to take over?"))?;
        let argv = crate::machine::attach_command(alias.as_deref(), thread)?;
        return Ok((format!("take over {} on {name}", crate::terminal::short_id(thread)), argv));
    }
    let cli = match engine {
        // `hubd sign-in` takes the engine's name; the machine there checks it is one of its own.
        Some(engine) if what == crate::model::MachineOpen::SignIn => Some(engine),
        Some(engine) => {
            let config = hub.config.engines.get(engine).ok_or_else(|| anyhow::anyhow!("no engine called {engine}"))?;
            config.terminal.as_deref().and_then(crate::machine::engine_cli)
        }
        None => None,
    };
    // An engine's CLI runs through `hubd cli <engine>`, signed in the way threads are.
    let engine = engine.filter(|_| what == crate::model::MachineOpen::Engine);
    let argv = crate::machine::command_for(alias.as_deref(), what, cli, engine)?;
    let title = match (what, cli) {
        (crate::model::MachineOpen::Engine, Some(cli)) => format!("{cli} on {name}"),
        (crate::model::MachineOpen::Root, _) => format!("root on {name}"),
        (crate::model::MachineOpen::SignIn, Some(engine)) => format!("sign {engine} in on {name}"),
        _ => name.to_owned(),
    };
    Ok((title, argv))
}

/// As administrator is for a shell or an engine's CLI on this computer; another machine's shell
/// is already its own user's.
fn check_admin_open(hub: &Hub, name: &str, what: crate::model::MachineOpen) -> anyhow::Result<()> {
    if !name.eq_ignore_ascii_case(hub.remotes.local_name()) {
        anyhow::bail!("administrator is for this computer only; another machine's shell is already its own user's");
    }
    if !matches!(what, crate::model::MachineOpen::Shell | crate::model::MachineOpen::Engine) {
        anyhow::bail!("only a shell or an engine's CLI opens as administrator");
    }
    Ok(())
}

/// Opens a terminal window on this computer, on that machine: a shell, a root shell or an
/// engine's CLI. This computer's own name opens it here.
async fn open_on_machine(
    State(hub): State<Arc<Hub>>,
    Path(name): Path<String>,
    Json(req): Json<crate::model::OpenOnMachine>,
) -> ApiResult<impl IntoResponse> {
    let (title, argv) = machine_plan(&hub, &name, req.what, req.engine.as_deref(), None)?;
    if req.admin == Some(true) {
        check_admin_open(&hub, &name, req.what)?;
        return Ok(Json(blocking(move || crate::machine::open_admin(&title, &argv)).await?));
    }
    Ok(Json(blocking(move || crate::machine::open(&title, &argv)).await?))
}

/// Opens the same inside Hub: a terminal this core runs, which the page shows (`pty.rs`).
async fn open_terminal_on_machine(
    State(hub): State<Arc<Hub>>,
    Path(name): Path<String>,
    Json(req): Json<crate::model::OpenTerminal>,
) -> ApiResult<Json<crate::model::TerminalSession>> {
    let thread = req.thread.filter(|_| req.what == crate::model::MachineOpen::Attach);
    let (title, argv) = machine_plan(&hub, &name, req.what, req.engine.as_deref(), thread.as_deref())?;
    if req.admin == Some(true) {
        check_admin_open(&hub, &name, req.what)?;
        let title = format!("{title} · administrator");
        let (session, secret) = hub.terminals.start_hosted(&name, req.what, title, &argv, req.cols, req.rows)?;
        let id = session.id.clone();
        let mut host = vec![
            crate::pty::HOST_ARG.to_owned(),
            hub.config.bind.to_string(),
            hub.config.data_dir.display().to_string(),
            id.clone(),
            secret,
            req.cols.to_string(),
            req.rows.to_string(),
        ];
        if !argv.is_empty() {
            host.push("--".to_owned());
            host.extend(argv);
        }
        // Windows asks the person here; a "No" ends the terminal with what Windows said.
        let asked = blocking(move || crate::machine::start_admin_host(&host)).await;
        match asked {
            Ok(()) => {
                let hub = hub.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(crate::pty::HOST_WAITS).await;
                    hub.terminals.host_never_came(&id);
                });
            }
            Err(err) => {
                hub.terminals.close(&id);
                return Err(err);
            }
        }
        return Ok(Json(session));
    }
    // The machine's name as the page knows it, also for this computer.
    Ok(Json(hub.terminals.start(&name, req.what, title, &argv, thread, req.cols, req.rows)?))
}

async fn terminals(State(hub): State<Arc<Hub>>) -> Json<Vec<crate::model::TerminalSession>> {
    Json(hub.terminals.list())
}

async fn close_terminal(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> StatusCode {
    if hub.terminals.close(&id) { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND }
}

/// Shows a terminal: its output so far, then live; keys and sizes come back.
async fn terminal_socket(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Browsers do not apply CORS to WebSockets (see `stream`). No phone page may type here.
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok())
        && !crate::guard::origin_allowed(&hub.config, origin)
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(term) = hub.terminals.get(&id) else {
        return (StatusCode::NOT_FOUND, "that terminal is closed").into_response();
    };
    ws.on_upgrade(move |socket| show_terminal(term, socket))
}

#[derive(Deserialize)]
struct HostSecret {
    secret: String,
}

/// The elevated helper of a terminal as administrator connects here (`pty::serve_host`).
async fn terminal_host_socket(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(HostSecret { secret }): Query<HostSecret>,
    ws: WebSocketUpgrade,
) -> Response {
    // A program, never a web page.
    if headers.get(header::ORIGIN).is_some() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some((term, from_pages)) = hub.terminals.take_host(&id, &secret) else {
        return (StatusCode::NOT_FOUND, "no terminal waits for that").into_response();
    };
    ws.on_upgrade(move |socket| crate::pty::serve_host(term, from_pages, socket))
}

#[derive(Deserialize)]
struct TerminalControl {
    resize: Option<(u16, u16)>,
}

async fn show_terminal(term: Arc<crate::pty::Terminal>, mut socket: WebSocket) {
    use crate::pty::Out;
    let (backlog, mut rx, exit) = term.follow();
    if !backlog.is_empty() && socket.send(Message::Binary(backlog.into())).await.is_err() {
        return;
    }
    let exit_msg = |code: i32| Message::Text(serde_json::json!({ "exit": code }).to_string().into());
    if let Some(code) = exit {
        socket.send(exit_msg(code)).await.warn_on_err("send the exit to the terminal page");
    }
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Ok(Out::Bytes(bytes)) => {
                    if socket.send(Message::Binary(bytes.to_vec().into())).await.is_err() {
                        return;
                    }
                }
                Ok(Out::Exit(code)) => {
                    if socket.send(exit_msg(code)).await.is_err() {
                        return;
                    }
                }
                // Fell behind: the page reconnects and draws it again from the backlog.
                Err(RecvError::Lagged(_)) => {
                    socket.send(Message::Close(None)).await.warn_on_err("close the lagging terminal page");
                    return;
                }
                Err(RecvError::Closed) => return,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Binary(bytes))) => term.write(bytes.to_vec()),
                Some(Ok(Message::Text(text))) => {
                    if let Ok(TerminalControl { resize: Some((cols, rows)) }) = serde_json::from_str(&text) {
                        term.resize(cols, rows);
                    }
                }
                Some(Ok(_)) => {}
                _ => return,
            },
        }
    }
}

#[derive(Deserialize)]
struct DistQuery {
    platform: String,
}

/// This machine's installed Hub as a package for a new machine of the same kind (`dist.rs`).
async fn dist(State(hub): State<Arc<Hub>>, Query(q): Query<DistQuery>) -> Result<Response, ApiError> {
    let (file, version) = crate::dist::package(&hub.config.data_dir, &q.platform).await.map_err(|e| ApiError::new(StatusCode::NOT_FOUND, format!("{e:#}")))?;
    let size = tokio::fs::metadata(&file).await.map_err(|e| ApiError::from(anyhow::Error::from(e)))?.len();
    let opened = tokio::fs::File::open(&file).await.map_err(|e| ApiError::from(anyhow::Error::from(e)))?;
    let chunks = futures::stream::unfold(opened, |mut f| async move {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 256 * 1024];
        match f.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok::<_, std::io::Error>(buf), f))
            }
            Err(e) => Some((Err(e), f)),
        }
    });
    Ok((
        [(header::CONTENT_TYPE, "application/gzip".to_owned()), (header::CONTENT_LENGTH, size.to_string()), (HeaderName::from_static("x-hub-version"), version)],
        axum::body::Body::from_stream(chunks),
    )
        .into_response())
}

async fn host_about() -> impl IntoResponse {
    Json(crate::host::about().await)
}

async fn dismiss_tailnet(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    crate::tailnet::dismiss(&hub.store, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn install_host(
    State(hub): State<Arc<Hub>>,
    Path(name): Path<String>,
    Json(body): Json<crate::model::InstallRequest>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.remotes.install(&name, body).await?))
}

/// Whether this machine can run another machine's tests, and how its last run went.
async fn checks_status(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::update::checks::status(&hub).await?))
}

/// Installs Rust here when it is the one thing missing for the tests.
async fn checks_prepare(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    crate::update::checks::prepare(&hub).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Takes the commit another machine sends as a git bundle.
async fn checks_source(State(hub): State<Arc<Hub>>, body: axum::body::Bytes) -> ApiResult<impl IntoResponse> {
    let tip = crate::update::checks::receive(&hub, body.to_vec()).await?;
    Ok(Json(serde_json::json!({ "have": tip })))
}

/// Runs the tests of a commit that was sent here.
async fn checks_run(State(hub): State<Arc<Hub>>, Json(ask): Json<crate::update::checks::RunAsk>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::update::checks::run(&hub, &ask.version).await?))
}

async fn update_host(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.remotes.update(&name).await?))
}

async fn machine_ready(State(hub): State<Arc<Hub>>) -> impl IntoResponse {
    let sign_ins = hub.sign_ins().await;
    Json(crate::ready::check(&hub.config, sign_ins).await)
}

/// Tries this machine's engine sign-ins and keys and API connections for real (`credcheck.rs`).
async fn credentials_check(State(hub): State<Arc<Hub>>) -> impl IntoResponse {
    Json(crate::credcheck::check(&hub).await)
}

/// The iPhone simulators of this Mac and whether one is watched live (`simview.rs`).
async fn simview(State(hub): State<Arc<Hub>>) -> impl IntoResponse {
    Json(hub.simview.state().await)
}

#[derive(serde::Deserialize)]
struct SimFrameQuery {
    udid: String,
}

/// The newest picture of a simulator as a JPEG: the live one while it is watched, else a
/// screenshot. 204 while the live picture has none yet.
async fn simview_frame(State(hub): State<Arc<Hub>>, Query(q): Query<SimFrameQuery>) -> ApiResult<axum::response::Response> {
    let frame = hub.simview.frame(&q.udid).await.map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    Ok(match frame {
        Some(bytes) => ([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "no-store")], bytes).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}

#[derive(serde::Deserialize)]
struct SimLiveRequest {
    #[serde(default)]
    udid: Option<String>,
    on: bool,
}

/// Starts or stops the live picture of a simulator.
async fn simview_live(State(hub): State<Arc<Hub>>, Json(req): Json<SimLiveRequest>) -> ApiResult<impl IntoResponse> {
    if req.on {
        let udid = req.udid.ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "which simulator?".into()))?;
        hub.simview.start(&hub.config, &udid).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
    } else {
        hub.simview.stop();
    }
    Ok(Json(hub.simview.state().await))
}

#[derive(serde::Deserialize)]
struct SimInputRequest {
    udid: String,
    #[serde(default)]
    touches: Vec<crate::simview::SimTouch>,
    #[serde(default)]
    button: Option<String>,
}

/// Touches and a button press for the simulator that is watched live. 400 when it is not live.
async fn simview_input(State(hub): State<Arc<Hub>>, Json(req): Json<SimInputRequest>) -> ApiResult<impl IntoResponse> {
    if req.touches.len() > 64 {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "too many touches at once".into()));
    }
    hub.simview.input(&req.udid, &req.touches, req.button.as_deref()).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
struct SimAppRequest {
    udid: String,
    /// `start`, `thread` or `tab`.
    action: String,
    #[serde(default)]
    thread: Option<String>,
    #[serde(default)]
    tab: Option<u8>,
}

/// Starts Hub's own iPhone app in the simulator already paired with this machine, opens a thread
/// in it, or starts it on one of its tabs. Makes the pairing code itself, so nobody types one.
async fn simview_app(State(hub): State<Arc<Hub>>, Json(req): Json<SimAppRequest>) -> ApiResult<impl IntoResponse> {
    let action = crate::simview::AppAction::parse(&req.action, req.thread.as_deref(), req.tab).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
    // An ordinary code: a phone code only redeems over the tailnet, and the simulator reaches this
    // machine on its loopback address.
    let code = hub.pairing.code(None);
    crate::simview::app(&req.udid, &action, &code.code, hub.config.bind.port()).await.map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize)]
struct SimBootRequest {
    udid: String,
}

/// Boots a simulator and answers when it is ready.
async fn simview_boot(State(hub): State<Arc<Hub>>, Json(req): Json<SimBootRequest>) -> ApiResult<impl IntoResponse> {
    crate::simview::boot(&req.udid).await.map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    Ok(Json(hub.simview.state().await))
}

/// Shuts a simulator down. If it is the one watched live, the picture stops first.
async fn simview_shutdown(State(hub): State<Arc<Hub>>, Json(req): Json<SimBootRequest>) -> ApiResult<impl IntoResponse> {
    if hub.simview.state().await.live.udid.as_deref() == Some(req.udid.as_str()) {
        hub.simview.stop();
    }
    crate::simview::shutdown(&req.udid).await.map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    Ok(Json(hub.simview.state().await))
}

async fn set_git_identity(Json(id): Json<crate::model::GitIdentity>) -> ApiResult<impl IntoResponse> {
    blocking(move || crate::git_identity::set_identity(&id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn host(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let threads = hub.store.threads().await?;
    let snapshot = tokio::task::spawn_blocking(move || crate::host::snapshot(&threads, &[crate::dev::PORT, crate::dev::CORE_PORT]))
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(snapshot))
}

async fn stop_process(Path(pid): Path<u32>) -> ApiResult<impl IntoResponse> {
    process_action(pid, crate::workers::Action::Stop).await
}

async fn pause_process(Path(pid): Path<u32>) -> ApiResult<impl IntoResponse> {
    process_action(pid, crate::workers::Action::Pause).await
}

async fn resume_process(Path(pid): Path<u32>) -> ApiResult<impl IntoResponse> {
    process_action(pid, crate::workers::Action::Resume).await
}

/// Acts on the process and everything it started.
async fn process_action(pid: u32, action: crate::workers::Action) -> ApiResult<StatusCode> {
    blocking(move || crate::workers::act(pid, action)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn open_terminal(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.open_terminal(&id).await?))
}

/// This machine's id, name and the address Tailscale serves it at, for clients that reach every
/// machine themselves.
async fn machine_routes(State(hub): State<Arc<Hub>>) -> ApiResult<Json<crate::model::MachineRoutes>> {
    Ok(Json(crate::model::MachineRoutes {
        id: crate::identity::machine_id(&hub.config.data_dir)?,
        name: hub.remotes.local_name().to_owned(),
        url: crate::pairing::tailscale_url(hub.config.bind.port()).await,
    }))
}
