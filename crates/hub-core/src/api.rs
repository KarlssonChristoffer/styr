//! HTTP + WebSocket API that every surface talks to.

use crate::locked::LockExt;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, Request, State};
use axum::middleware::{self, Next};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::error_code::ErrorCode;
use crate::hub::Hub;
use crate::model::{
    NewHost, NewThread, PermissionAnswer, ReviewRequest, SaveRule, SendMessage, SetSetting, Shutdown, ThreadPatch,
    Tidied, TidyRequest, UndoTarget,
};

mod bench;
mod engines;
mod github;
mod keys;
mod machines;
mod phone;
mod projects;
mod threads;
mod usage;

use threads::{create_thread, warm_new};

pub fn router(hub: Arc<Hub>) -> Router {
    let origins: Vec<HeaderValue> =
        hub.config.allowed_origins.iter().map(String::as_str).chain(crate::dev::ORIGINS).filter_map(|o| o.parse().ok()).collect();
    // The app's own origins, and the phone page of another machine on the same tailnet, which
    // follows this machine with a token of its own (`phone::sibling_page`).
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, parts| {
            origins.contains(origin) || origin.to_str().is_ok_and(|o| {
                let host = parts.headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or_default();
                crate::phone::siblings(o, host)
            })
        }))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    Router::new()
        // --- projects ---
.route(crate::promptlink::PATH, get(projects::prompt_link))
        // --- machines ---
.route("/api/desk", get(desk_folder))
.route("/api/desks", get(desks).post(open_desk))
        // --- github ---
.route("/api/threads/{id}/gitstate", get(thread_git_state))
.route("/api/threads/{id}/branches", get(thread_branches))
.route("/api/threads/{id}/switch", post(thread_switch))
.route("/api/threads/{id}/ship", post(thread_ship))
.route("/api/threads/{id}/gitdraft", post(thread_git_draft))
.route("/api/threads/{id}/watch", post(watch_pr).delete(unwatch_pr))
        // --- engines ---
        // --- keys ---
.route("/api/threads/{id}/jev", get(thread_jev).put(set_thread_jev))
.route(crate::gateway::CALLBACK, get(crate::gateway::callback))
.route("/gw/{*rest}", axum::routing::any(crate::gateway::forward))
.route("/mcp/{id}", post(mcp))
.route("/mcp", post(agent_mcp))
        // --- usage ---
        .route("/api/health", get(health))
.merge(crate::customs_registry::router())
.merge(crate::problems::router())
        // Each area's routes, as it is moved out of this file (see api/).
        .merge(threads::routes())
        .merge(projects::routes())
        .merge(phone::routes())
        .merge(machines::routes())
        .merge(github::routes())
        .merge(engines::routes())
        .merge(keys::routes())
        .merge(usage::routes())
        .merge(bench::routes())
        .layer(middleware::from_fn_with_state(hub.clone(), to_host))
        // Attachments travel as base64 in the message body.
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(crate::guard::Guard::new(hub.clone()), crate::guard::check))
        .layer(cors)
        .with_state(hub)
}

/// An error at the border: the HTTP status, the text and the kind (`error_code.rs`).
struct ApiError {
    status: StatusCode,
    message: String,
    code: ErrorCode,
}

impl ApiError {
    /// The kind follows from the status.
    fn new(status: StatusCode, message: String) -> Self {
        Self { status, message, code: ErrorCode::of_status(status.as_u16()) }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(status = self.status.as_u16(), "api error: {}", self.message);
        }
        (self.status, Json(serde_json::json!({ "error": self.message, "code": self.code }))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        Self { status: status_of(&err), message: format!("{err:#}"), code: ErrorCode::of_error(&err) }
    }
}

/// What an error from a handler means to the caller. Most `anyhow` errors here are refusals the
/// person can act on (400), but a database failure is Hub's own fault: 500. The body keeps its
/// shape (`{ "error": … }`) and clients only branch on 404/405 (`ApiError.tooOld`), so older apps
/// and phones read the new codes as the same error text.
fn status_of(err: &anyhow::Error) -> StatusCode {
    if err.chain().any(|cause| cause.downcast_ref::<rusqlite::Error>().is_some()) {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::BAD_REQUEST
    }
}

type ApiResult<T> = Result<T, ApiError>;

/// Requests about another machine go to its hubd: `?host=<name>` (starting a thread there,
/// its files, its projects), or any `/api/threads/{id}/...` of a thread that runs there.
async fn to_host(State(hub): State<Arc<Hub>>, req: Request, next: Next) -> Response {
    match hub.remotes.route(req.uri()) {
        None => {
            // How long the core takes to answer, for the per-release baseline (`problems::metrics`).
            // Waits that are long on purpose are left out.
            let timed = req.method() == axum::http::Method::GET && !req.uri().path().contains("wait");
            let started = std::time::Instant::now();
            let response = next.run(req).await;
            if timed {
                crate::metric_samples::sample("api_ms", started.elapsed().as_secs_f64() * 1000.0, None, "Hub");
            }
            response
        }
        Some(Ok(remote)) => remote.forward(hub.remotes.client(), req).await,
        Some(Err(message)) => ApiError::new(StatusCode::NOT_FOUND, message).into_response(),
    }
}

/// `GET`/`PUT /api/threads/{id}/jev`: whether this thread's agents may ask the decision model.
#[derive(serde::Serialize, Deserialize)]
struct ThreadJev {
    agents: bool,
}

async fn thread_jev(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(ThreadJev { agents: hub.agent_jev_on(&id).await }))
}

async fn set_thread_jev(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(b): Json<ThreadJev>) -> ApiResult<impl IntoResponse> {
    hub.set_agent_jev(&id, b.agents).await?;
    Ok(Json(ThreadJev { agents: hub.agent_jev_on(&id).await }))
}

#[derive(Deserialize)]
struct TuneAnswer {
    id: String,
    #[serde(rename = "use")]
    used: bool,
}

fn default_days() -> u32 {
    371
}

fn thirty() -> u32 {
    30
}

fn seven() -> u32 {
    7
}

fn fourteen() -> u32 {
    14
}

/// Hub's tools for a thread's agent (see `mcp.rs`).
async fn mcp(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(msg): Json<serde_json::Value>) -> Response {
    match crate::mcp::handle(&hub, &id, msg).await {
        Some(answer) => Json(answer).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// Hub's tools for an agent outside Hub, by the agent key it shows. The connection token
/// works too, with full access, for scripts on this machine.
async fn agent_mcp(State(hub): State<Arc<Hub>>, headers: HeaderMap, Json(msg): Json<serde_json::Value>) -> Response {
    let Some(given) = crate::auth::bearer(&headers) else {
        return ApiError::new(StatusCode::UNAUTHORIZED, "an agent key is needed; make one in Hub under Settings > Agents".into())
            .into_response();
    };
    let caller = if crate::auth::is_token(&hub.config, given) {
        Some(crate::hub_tools::Caller::Agent { name: "Hub's connection token".into(), access: crate::model::AgentAccess::Full })
    } else {
        match crate::hub_tools::agent(&hub, given).await {
            Ok(caller) => caller,
            Err(e) => return ApiError::from(e).into_response(),
        }
    };
    let Some(caller) = caller else {
        return ApiError::new(StatusCode::UNAUTHORIZED, "this agent key is unknown, revoked or expired".into()).into_response();
    };
    match crate::mcp::handle_as(&hub, &caller, msg).await {
        Some(answer) => Json(answer).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

#[derive(serde::Deserialize)]
struct PluginsQuery {
    #[serde(default)]
    q: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    fresh: bool,
}

#[derive(Deserialize)]
struct CustomsRef {
    kind: String,
    name: String,
}

#[derive(Deserialize)]
struct ImportBody {
    folder: String,
}

#[derive(Deserialize)]
struct CustomsRepoBody {
    /// `owner/name` or a link to the repository.
    repo: String,
}

#[derive(Deserialize)]
struct CustomsActivityQuery {
    kind: Option<String>,
    name: Option<String>,
}

/// The folder a thread asked about all of Hub works in, made ready with its brief.
async fn desk_folder(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let folder = crate::hub_tools::folder(&hub.config.data_dir)?;
    Ok(Json(serde_json::json!({ "folder": folder.display().to_string() })))
}

/// The ids of every project's desk thread, so surfaces can show them apart.
async fn desks(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(serde_json::json!({ "ids": crate::desk::ids(&hub.ctx()).await? })))
}

#[derive(serde::Deserialize)]
struct OpenDesk {
    folder: String,
}

/// A project's desk, made the first time.
async fn open_desk(State(hub): State<Arc<Hub>>, Json(body): Json<OpenDesk>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.open_desk(&body.folder).await?))
}

async fn search(cwd: String, q: String) -> ApiResult<Json<Vec<crate::files::FileMatch>>> {
    let found = tokio::task::spawn_blocking(move || crate::files::search(std::path::Path::new(&cwd), &q, 40))
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(found))
}

#[derive(Deserialize)]
struct ProjectQuery {
    path: String,
    /// A folder or file inside the project, relative to it.
    #[serde(default)]
    rel: String,
}

/// Runs blocking project work on the blocking pool.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(ApiError::from)
}

#[derive(Deserialize)]
struct HarnessSettingEdit {
    key: String,
    value: serde_json::Value,
}

#[derive(serde::Deserialize)]
struct ToolAsk {
    tool: String,
    /// Only on this machine; every machine online when missing.
    #[serde(default)]
    machine: Option<String>,
}

#[derive(serde::Deserialize)]
struct InstallAsk {
    harness: String,
}

async fn bring(hub: &Arc<Hub>, body: crate::model::BringProject) -> ApiResult<crate::model::Brought> {
    let wait = std::time::Duration::from_secs(600);
    let here = |h: &Option<String>| h.as_deref().is_none_or(|h| h.eq_ignore_ascii_case(hub.remotes.local_name()));
    let pack: crate::model::ProjectPack = if here(&body.from) {
        let path = body.path.clone();
        blocking(move || crate::carry::pack(&path)).await?
    } else {
        let from = body.from.as_deref().unwrap_or_default();
        hub.remotes.post_to(from, "/api/folders/pack", &serde_json::json!({ "path": body.path }), wait).await?
    };
    let unpack = crate::model::UnpackProject { path: body.target.clone(), pack, sync: false, resume: false };
    let brought: crate::model::Brought = if here(&body.to) {
        blocking(move || crate::carry::unpack(unpack)).await?
    } else {
        hub.remotes.post_to(body.to.as_deref().unwrap_or_default(), "/api/folders/unpack", &unpack, wait).await?
    };
    Ok(brought)
}

#[derive(serde::Deserialize)]
struct SignedIn {
    #[serde(default)]
    token: Option<String>,
}

async fn thread_cwd(hub: &Hub, id: &str) -> ApiResult<String> {
    Ok(hub.store.thread(id).await?.ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("no thread {id}")))?.cwd)
}

async fn thread_git_state(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    let cwd = thread_cwd(&hub, &id).await?;
    Ok(Json(blocking(move || Ok(crate::gitops::state(std::path::Path::new(&cwd)))).await?))
}

async fn thread_branches(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    let cwd = thread_cwd(&hub, &id).await?;
    Ok(Json(blocking(move || Ok(crate::gitops::local_branches(std::path::Path::new(&cwd)))).await?))
}

#[derive(Deserialize)]
struct SwitchBody {
    branch: String,
    #[serde(default)]
    create: bool,
}

async fn thread_switch(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(b): Json<SwitchBody>) -> ApiResult<impl IntoResponse> {
    let cwd = thread_cwd(&hub, &id).await?;
    blocking(move || crate::gitops::switch(std::path::Path::new(&cwd), &b.branch, b.create)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn thread_ship(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(mut s): Json<crate::model::GitShip>) -> ApiResult<impl IntoResponse> {
    s.cwd = thread_cwd(&hub, &id).await?;
    Ok(Json(crate::gitops::ship(s).await?))
}

async fn thread_git_draft(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    let cwd = thread_cwd(&hub, &id).await?;
    Ok(Json(hub.git_draft(&cwd, Some(&id)).await?))
}

#[derive(Deserialize)]
struct WatchBody {
    #[serde(default)]
    number: Option<u64>,
}

async fn watch_pr(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(b): Json<WatchBody>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.watch_pr(&id, b.number).await?))
}

async fn unwatch_pr(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    hub.unwatch_pr(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct WorkerInstallAsk {
    repo: String,
}

#[derive(Deserialize)]
struct ReadyAsk {
    folder: String,
}

/// Open to anyone: that this is Hub, which build, and what it speaks.
async fn health(State(hub): State<Arc<Hub>>) -> Json<serde_json::Value> {
    // Who answers here, so a client can tell this machine from another at the same address before
    // it shows a token (`identity.rs`).
    let machine = crate::identity::machine_id(&hub.config.data_dir).ok();
    Json(serde_json::json!({
        "ok": true,
        "version": hub_install::running_version(),
        // Kept at 1 for the older clients that read it; nothing reads it any more. A new call is a
        // `compat::FEATURES` entry, never a bump of this.
        "protocol": 1,
        "core": crate::compat::CORE,
        "features": crate::compat::feature_names(),
        "schema": crate::store::schema_version(),
        "machine": machine,
        "name": hub.remotes.local_name(),
    }))
}

async fn stream(
    State(hub): State<Arc<Hub>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Browsers do not apply CORS to WebSockets, so the origin is checked here. Clients that
    // are not browsers (CLI, tests) send no Origin header.
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok())
        && !crate::guard::origin_allowed(&hub.config, origin)
        && !crate::phone::own_page(&headers)
        && !crate::phone::sibling_page(&headers) {
            return StatusCode::FORBIDDEN.into_response();
        }
    ws.on_upgrade(move |socket| forward(hub, socket))
}

async fn forward(hub: Arc<Hub>, mut socket: WebSocket) {
    let mut rx = hub.subscribe();
    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(msg) => {
                    let Ok(text) = serde_json::to_string(&msg) else { continue };
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                // The client fell behind; tell it to refetch instead of silently dropping events.
                Err(RecvError::Lagged(_)) => {
                    let Ok(resync) = serde_json::to_string(&crate::model::StreamMsg::Resync) else { continue };
                    if socket.send(Message::Text(resync.into())).await.is_err() {
                        return;
                    }
                }
                Err(RecvError::Closed) => return,
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(_)) => {}
                _ => return,
            },
        }
    }
}

#[derive(Deserialize)]
struct UsageQuery {
    #[serde(default)]
    refresh: bool,
    /// Ask the engine again even if the limits are recent, e.g. from a refresh button.
    #[serde(default)]
    force: bool,
}

#[derive(Deserialize)]
struct ModelsQuery {
    #[serde(default)]
    refresh: bool,
}

#[derive(Deserialize)]
struct DoctorQuery {
    engine: String,
}

#[cfg(test)]
mod error_status_tests {
    use super::*;

    #[test]
    fn database_errors_are_500_and_refusals_stay_400() {
        let db = anyhow::Error::new(rusqlite::Error::QueryReturnedNoRows).context("reading thread");
        assert_eq!(ApiError::from(db).status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(ApiError::from(anyhow::anyhow!("pick a folder")).status, StatusCode::BAD_REQUEST);
        // The body gains a kind next to the text and the status stays what it was.
        let gone = ApiError::from(anyhow::Error::new(crate::store::ThreadGone("t".into())));
        assert_eq!((gone.status, gone.code), (StatusCode::BAD_REQUEST, ErrorCode::NotFound));
        assert_eq!(ApiError::new(StatusCode::NOT_FOUND, "no such thread".into()).code, ErrorCode::NotFound);
        // A phone told it may only make Safety stricter gets a 403 with the refused kind.
        let phone = ApiError::new(StatusCode::FORBIDDEN, "a phone can only make Safety stricter".into());
        assert_eq!((phone.status, phone.code), (StatusCode::FORBIDDEN, ErrorCode::Refused));
    }
}
