//! Routes for the phone area: the phone page and its pairing, the phone's own narrow calls
//! (threads, files, notices) and the devices that are paired.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};

use super::*;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
.route("/api/phone/threads", post(phone_thread))
.route("/api/phone/engines", get(phone_engines))
.route("/api/phone/warm", post(phone_warm))
.route("/api/phone/bring", post(phone_bring))
.route("/api/phone/now", get(|State(hub): State<Arc<Hub>>| async move { crate::now::read(&hub).await.map(Json).map_err(ApiError::from) }))
.route("/api/phone/focus", post(phone_focus))
.route("/api/phone/notify", get(phone_notify).put(set_phone_notify))
.route("/api/phone/notify/test", post(test_phone_notify))
.route("/api/phone/looking", post(|Json(l): Json<crate::ntfy::Looking>| async move {
            crate::ntfy::look(l);
            StatusCode::NO_CONTENT
        }))
.route("/api/presence", post(|Json(p): Json<crate::ntfy::Presence>| async move {
            crate::ntfy::present(p);
            StatusCode::NO_CONTENT
        }))
.route("/api/phone/threads/{id}/picture", get(thread_picture))
.route("/api/phone/threads/{id}/page", post(thread_page))
.route("/api/phone/files/tree", get(phone_tree))
.route("/api/phone/files/file", get(phone_file))
.route("/api/phone/files/raw", get(phone_raw))
.route("/api/phone/files/search", get(phone_search))
.route("/api/phone/project/git", get(part_git))
.route("/api/phone/project/docs", get(part_docs))
.route("/api/phone/project/backlog", get(part_backlog).post(part_backlog_edit))
.route("/api/phone/project/board", get(part_board).post(part_board_edit))
.route("/api/pair", post(pair))
.route("/api/pair/redeem", post(redeem))
.route("/api/phone", post(phone_link))
.route("/api/phone/machines", post(phone_machines))
.route("/api/pair/phone-code", post(phone_code))
.route("/phone", get(crate::phone::serve))
.route("/phone/{file}", get(crate::phone::serve))
.route("/api/devices", get(devices))
.route("/api/devices/{id}", axum::routing::delete(revoke_device))
}

/// A thread started from the phone: a folder, a first message, and at most an engine, a title and
/// the choice of model and thinking effort (`phone_settings`). Nothing else of `NewThread`
/// (permission mode, attempts, unattended runs), so the phone's narrow token starts an ordinary
/// thread that asks before it acts, as the project's safety says.
#[derive(serde::Deserialize)]
struct PhoneThread {
    #[serde(default)]
    cwd: String,
    /// No project: the machine makes a fresh scratch folder (`<projects root>/scratch/<time>`), as
    /// the desktop's "No project" does, and the thread runs there.
    #[serde(default)]
    scratch: bool,
    /// An everyday chat, not a dev thread: like `scratch` it has no project, but its folder is
    /// `<projects root>/chats/<time>`, so every surface can tell the two apart by the path alone.
    #[serde(default)]
    chat: bool,
    message: String,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    /// Pictures and files sent with the first message, as a message from the phone can carry.
    #[serde(default)]
    attachments: Vec<crate::model::Attachment>,
    /// Model and effort picked before the first message (`phone_settings` lets only those through).
    #[serde(default)]
    settings: std::collections::BTreeMap<String, serde_json::Value>,
    /// The id `POST /api/phone/warm` gave, so the engine that was started for the pick is used.
    #[serde(default)]
    warm: Option<String>,
    /// Run it on the harness's own servers (`cloud_threads.rs`) instead of this machine.
    #[serde(default)]
    cloud: bool,
}

/// The settings a phone may start a thread with: which model, which provider and how hard it
/// thinks. The permission mode and everything else stay as the project's safety sets them.
fn phone_settings(mut settings: std::collections::BTreeMap<String, serde_json::Value>) -> std::collections::BTreeMap<String, serde_json::Value> {
    const OK: [&str; 5] = ["hub_route", "model", "thought_level", "reasoning_effort", "effort"];
    settings.retain(|k, v| OK.contains(&k.as_str()) && (v.is_string() || v.is_boolean()));
    settings
}

/// A new folder under `<root>/<kind>` (`scratch` or `chats`) named by the time, with `-2`, `-3`
/// when one is taken.
fn scratch_folder(root: &std::path::Path, now_ms: i64, kind: &str) -> std::io::Result<std::path::PathBuf> {
    let secs = now_ms / 1000;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970 (Howard Hinnant's algorithm), in UTC.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let stamp = format!("{y:04}-{m:02}-{d:02}-{:02}{:02}{:02}", rest / 3600, rest % 3600 / 60, rest % 60);
    let base = root.join(kind);
    std::fs::create_dir_all(&base)?;
    for n in 1.. {
        let dir = if n == 1 { base.join(&stamp) } else { base.join(format!("{stamp}-{n}")) };
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

fn phone_new_thread(p: PhoneThread) -> Result<NewThread, serde_json::Error> {
    serde_json::from_value(serde_json::json!({
        "cwd": p.cwd, "message": p.message, "engine": p.engine, "title": p.title, "client_id": p.client_id,
        "attachments": p.attachments, "settings": phone_settings(p.settings), "warm": p.warm, "cloud": p.cloud,
    }))
}

/// Starts the engine for a new thread while the phone's page is open, with the model and effort
/// picked so far, so the engine can say what it offers (its models, its efforts) before the
/// first message. The thread it makes is claimed by `POST /api/phone/threads` with `warm`.
/// The warm ask of the phone: a folder, or `chat` for an everyday chat, whose folder does not
/// exist yet (`<projects root>/chats/<time>`) and is made here, so the engine has somewhere to start.
#[derive(serde::Deserialize)]
struct PhoneWarm {
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    settings: std::collections::BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    chat: bool,
}

/// The chats folder a warm engine is waiting in, so a pick that changes (model, effort) reuses the
/// engine and the folder instead of leaving a new empty folder each time. Cleared when a thread
/// takes it.
static WAITING_CHAT: std::sync::Mutex<Option<(std::path::PathBuf, std::time::Instant)>> = std::sync::Mutex::new(None);

/// How long a chats folder waits for its thread; the engine behind it waits three minutes.
const CHAT_WAIT: std::time::Duration = std::time::Duration::from_secs(170);

fn waiting_chat_folder(root: &std::path::Path, now_ms: i64) -> std::io::Result<std::path::PathBuf> {
    let mut waiting = WAITING_CHAT.locked();
    if let Some((dir, at)) = waiting.as_ref()
        && at.elapsed() < CHAT_WAIT
        && dir.starts_with(root)
        && std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none())
    {
        return Ok(dir.clone());
    }
    let dir = scratch_folder(root, now_ms, "chats")?;
    *waiting = Some((dir.clone(), std::time::Instant::now()));
    Ok(dir)
}

async fn phone_warm(State(hub): State<Arc<Hub>>, Json(ask): Json<PhoneWarm>) -> ApiResult<impl IntoResponse> {
    let mut cwd = ask.cwd;
    if ask.chat {
        let root = crate::folders::expand(&hub.projects_root().await?);
        cwd = blocking(move || Ok(waiting_chat_folder(&root, crate::model::now_ms())?)).await?.display().to_string();
    }
    let ask = crate::model::WarmAsk { cwd, engine: ask.engine, settings: phone_settings(ask.settings) };
    warm_new(State(hub), Json(ask)).await
}

/// One engine as the phone's New thread page picks from it.
#[derive(serde::Serialize)]
struct PhoneEngine {
    name: String,
    label: String,
    /// New threads start on it.
    default: bool,
    /// Where its models come from, when it has one fixed provider: `deepseek`.
    provider: Option<String>,
    provider_label: Option<String>,
    /// The model it starts on.
    model: Option<String>,
    /// A harness that runs on several providers (Hermes, OpenCode): the provider and model to
    /// pick, as an ACP select option grouped by provider, with the one it starts on.
    route: Option<serde_json::Value>,
    /// The model and effort settings the engine offered in its latest thread, as it sent them
    /// (ACP config options): known before a new thread has run, so the phone can show them at once.
    options: Vec<serde_json::Value>,
    /// The thinking levels of each provider it can use, by provider id, when Hub knows them.
    efforts: std::collections::BTreeMap<String, Vec<crate::engines::Effort>>,
}

/// The engines this machine can start, with what Hub itself knows of their providers and models.
/// The agent's own options (Claude's model list, Codex's) come from `POST /api/phone/warm`.
async fn phone_engines(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let vault = hub.secrets.list().await?;
    let providers = crate::engines::providers();
    let mut out = Vec::new();
    for (name, e) in &hub.config.engines {
        if name == crate::sim::ENGINE {
            continue;
        }
        let route_value = crate::routes::default_route(e);
        let live = crate::routes::live_models(&hub.secrets, name, e, &vault).await;
        let route = crate::routes::option(name, e, &route_value, &vault, &live);
        let mut ids: Vec<String> = e.provider.iter().cloned().collect();
        if let Some(groups) = route.as_ref().and_then(|r| r["options"].as_array()) {
            ids.extend(groups.iter().filter_map(|g| g["group"].as_str().map(str::to_owned)));
        }
        let efforts = ids
            .into_iter()
            .filter_map(|id| providers.iter().find(|p| p.id == id && !p.efforts.is_empty()).map(|p| (id, p.efforts.clone())))
            .collect();
        let provider = e.provider.as_ref().and_then(|id| providers.iter().find(|p| &p.id == id));
        out.push(PhoneEngine {
            name: name.clone(),
            label: crate::engines::label(name, e).unwrap_or_else(|| name.clone()),
            default: hub.config.default_engine == *name,
            provider: e.provider.clone(),
            provider_label: provider.map(|p| p.label.clone()),
            model: e.model.clone(),
            route,
            options: hub.known_options(name).await,
            efforts,
        });
    }
    Ok(Json(out))
}

/// Validate on the destination machine, before starting an engine. History from Home is not
/// a coding project; canonical paths also catch a symlink or `..` spelling of that workspace.
/// `home` is the home folder (`folders::home_dir`), given so a test can lay out its own.
fn phone_project(cwd: &str, data_dir: &std::path::Path, home: Option<&std::path::Path>) -> ApiResult<()> {
    if cwd.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "pick a project, or no project".into()));
    }
    let folder = std::path::Path::new(cwd).canonicalize()
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, format!("project folder {cwd:?} does not exist; pick an existing project")))?;
    if !folder.is_dir() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "pick a project folder".into()));
    }
    // Not the home folder or a folder of logins: the thread's pictures and pages are read from it.
    if crate::phone_files::too_wide_under(&folder, data_dir, home) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "this folder holds the whole home folder, Hub's own data or logins; pick a project".into()));
    }
    for name in ["desk", "platschef"] {
        let home = data_dir.join(name);
        if home.canonicalize().ok().as_ref() == Some(&folder) {
            return Err(ApiError::new(StatusCode::BAD_REQUEST, "this is Hub's Home workspace; pick a coding project or no project".into()));
        }
    }
    // A moved Home folder (older installs put it under Dev) retains its generated brief.
    for ancestor in folder.ancestors() {
        for name in ["AGENTS.md", "CLAUDE.md"] {
            if std::fs::read_to_string(ancestor.join(name)).ok().is_some_and(|text| {
                text.contains("from Home to look across all of them, so this folder is only a desk:")
            }) {
                return Err(ApiError::new(StatusCode::BAD_REQUEST, "this folder has Hub's Home instructions; pick a coding project or no project".into()));
            }
        }
    }
    Ok(())
}

async fn phone_thread(State(hub): State<Arc<Hub>>, Json(mut p): Json<PhoneThread>) -> ApiResult<impl IntoResponse> {
    if p.message.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "write what the thread should do first".into()));
    }
    if p.scratch || p.chat {
        let root = crate::folders::expand(&hub.projects_root().await?);
        // A chat whose engine was warmed while the page was open runs in the folder it was warmed in.
        let warmed = p.warm.as_deref().and_then(crate::warm::pending).map(|w| std::path::PathBuf::from(w.cwd)).filter(|d| p.chat && d.starts_with(root.join("chats")));
        if let Some(dir) = warmed {
            *WAITING_CHAT.locked() = None;
            p.cwd = dir.display().to_string();
        } else {
            let kind = if p.chat { "chats" } else { "scratch" };
            let folder = blocking(move || Ok(scratch_folder(&root, crate::model::now_ms(), kind)?)).await?;
            p.cwd = folder.display().to_string();
        }
    }
    if p.cwd.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "pick a project, or no project".into()));
    }
    phone_project(&p.cwd, &hub.config.data_dir, crate::folders::home_dir().as_deref())?;
    refuse_desk(&hub, &p.cwd)?;
    let new = phone_new_thread(p).map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.to_string()))?;
    create_thread(State(hub), Json(new)).await
}

/// A phone thread that has code to write never starts in Hub's desk folder: its brief says the work
/// happens in the projects, so the agent there only hands the task on. Better a clear refusal than a
/// thread that does the wrong thing quietly.
fn refuse_desk(hub: &Hub, cwd: &str) -> Result<(), ApiError> {
    if crate::hub_tools::is_desk_folder(&hub.config.data_dir, cwd) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "that folder is Hub's own desk, not a project: pick the project the work belongs to".into(),
        ));
    }
    Ok(())
}

/// The phone's notices from this machine (`ntfy.rs`).
async fn phone_notify(State(hub): State<Arc<Hub>>) -> Json<crate::ntfy::Notify> {
    Json(crate::ntfy::load(&hub.ctx()).await)
}

async fn set_phone_notify(State(hub): State<Arc<Hub>>, Json(n): Json<crate::ntfy::Notify>) -> ApiResult<impl IntoResponse> {
    crate::ntfy::save(&hub.ctx(), &n).await?;
    Ok(Json(n))
}

async fn test_phone_notify(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let n = crate::ntfy::load(&hub.ctx()).await;
    if n.topic.trim().is_empty() {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "pick a topic first".into()));
    }
    let name = hub.config.name.clone().unwrap_or_else(|| "Hub".into());
    crate::ntfy::send(&n, &format!("{name} · Test"), "Notices from Hub reach this phone.", Some("hub://inbox"), false)
        .await
        .map_err(|e| ApiError::new(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// A box in FOCUS.md ticked or unticked from the phone: a `tick` focus edit and nothing else.
#[derive(serde::Deserialize)]
struct PhoneTick {
    path: String,
    line: u32,
    done: bool,
}

fn phone_tick(t: PhoneTick) -> crate::focus::FocusEdit {
    crate::focus::FocusEdit { path: t.path, op: "tick".into(), line: Some(t.line), done: Some(t.done), list: None, text: None }
}

async fn phone_focus(Json(t): Json<PhoneTick>) -> ApiResult<impl IntoResponse> {
    let edit = phone_tick(t);
    Ok(Json(blocking(move || crate::focus::edit(std::path::Path::new(&edit.path), &edit)).await?))
}

/// `POST /api/phone/bring`: the phone picks a machine that lacks a project and the project is
/// carried there (history, env files; no GitHub login needed there), as the desktop folder picker
/// does. Narrower than `/api/folders/bring`: the phone names the project and the machines, never
/// the target path. The project must be one the source machine lists, and it goes to
/// `<that machine's projects folder>/<the folder's name>`.
#[derive(Debug, serde::Deserialize)]
struct PhoneBring {
    /// The machine the project is on. None is this machine.
    from: Option<String>,
    path: String,
    /// The machine that should have it. None is this machine.
    to: Option<String>,
}

/// The listed project a path from the phone means: the folder itself, or the project it lies in
/// (a thread's working folder inside the repository, or a worktree below it), found as text so
/// `/` or `\\` and case do not matter. Never a folder the machine does not list as a project.
fn pick_project<'a>(projects: &'a [crate::model::ProjectFolder], path: &str) -> Option<&'a crate::model::ProjectFolder> {
    let norm = |s: &str| s.trim().replace('\\', "/").trim_end_matches('/').to_lowercase();
    let want = norm(path);
    projects
        .iter()
        .find(|p| norm(&p.path) == want)
        .or_else(|| projects.iter().filter(|p| !norm(&p.path).is_empty() && want.starts_with(&format!("{}/", norm(&p.path)))).max_by_key(|p| p.path.len()))
}

async fn phone_bring(State(hub): State<Arc<Hub>>, Json(ask): Json<PhoneBring>) -> ApiResult<impl IntoResponse> {
    let here = |h: &Option<String>| h.as_deref().is_none_or(|h| h.eq_ignore_ascii_case(hub.remotes.local_name()));
    let (from_here, to_here) = (here(&ask.from), here(&ask.to));
    if from_here && to_here {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "the project is on that machine already".into()));
    }
    let local = |hub: Arc<Hub>| async move {
        let root = hub.projects_root().await?;
        let mut known: Vec<String> = hub.store.threads().await?.into_iter().filter(|t| t.host.is_none()).map(|t| t.cwd).collect();
        known.extend(hub.store.projects().await?.into_iter().map(|p| p.path));
        known.sort();
        known.dedup();
        blocking(move || Ok(crate::folders::projects(&root, &known))).await
    };
    let source: crate::model::ProjectFolders = if from_here {
        local(hub.clone()).await?
    } else {
        hub.remotes.get_from(ask.from.as_deref().unwrap_or_default(), "/api/folders/projects").await?
    };
    let Some(project) = pick_project(&source.projects, &ask.path) else {
        let machine = ask.from.as_deref().unwrap_or_else(|| hub.remotes.local_name());
        return Err(ApiError::new(StatusCode::FORBIDDEN, format!("{} is not a project {machine} lists", ask.path)));
    };
    let target: crate::model::ProjectFolders = if to_here {
        local(hub.clone()).await?
    } else {
        hub.remotes.get_from(ask.to.as_deref().unwrap_or_default(), "/api/folders/projects").await?
    };
    let sep = if target.root.contains('\\') { '\\' } else { '/' };
    let name = project.name.trim();
    if name.is_empty() || name.contains(['/', '\\']) || name == ".." || name == "." {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "the project has no usable folder name".into()));
    }
    let target_path = format!("{}{sep}{name}", target.root.trim_end_matches(['/', '\\']));
    let brought = bring(&hub, crate::model::BringProject { from: ask.from, path: project.path.clone(), to: ask.to, target: target_path }).await?;
    Ok(Json(brought))
}

/// The folder of a project Hub knows, for the phone's file browser (`phone_files.rs`): any other
/// folder on the machine is refused, whatever the phone asks for.
async fn phone_root(hub: &Hub, path: &str) -> ApiResult<std::path::PathBuf> {
    let same = |a: &str, b: &str| {
        let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
        norm(a) == norm(b)
    };
    if !hub.store.projects().await?.iter().any(|p| same(&p.path, path)) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "not a project of this machine".into()));
    }
    let root = std::path::PathBuf::from(path);
    if crate::phone_files::too_wide(&root, &hub.config.data_dir) {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "this folder holds the whole home folder or Hub's own data; open it on the machine".into()));
    }
    Ok(root)
}

/// The parts of a project's page on the phone (`project_parts.rs`): only folders that are Hub's projects.
async fn part_git(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let root = phone_root(&hub, &q.path).await?;
    Ok(Json(crate::project_parts::git_card(&root).await?))
}

async fn part_docs(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let root = phone_root(&hub, &q.path).await?;
    Ok(Json(blocking(move || Ok(crate::project_parts::docs_card(&root))).await?))
}

async fn part_backlog(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let root = phone_root(&hub, &q.path).await?;
    Ok(Json(blocking(move || Ok(crate::project_parts::backlog(&root))).await?))
}

async fn part_backlog_edit(State(hub): State<Arc<Hub>>, Json(e): Json<crate::model::PartEdit>) -> ApiResult<impl IntoResponse> {
    let root = phone_root(&hub, &e.path).await?;
    Ok(Json(blocking(move || crate::project_parts::edit_backlog(&root, &e)).await?))
}

async fn part_board(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let root = phone_root(&hub, &q.path).await?;
    Ok(Json(blocking(move || Ok(crate::project_parts::board(&root))).await?))
}

async fn part_board_edit(State(hub): State<Arc<Hub>>, Json(e): Json<crate::model::PartEdit>) -> ApiResult<impl IntoResponse> {
    let root = phone_root(&hub, &e.path).await?;
    Ok(Json(blocking(move || crate::project_parts::edit_board(&root, &e)).await?))
}

async fn phone_tree(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let root = phone_root(&hub, &q.path).await?;
    Ok(Json(blocking(move || crate::phone_files::tree(&root, &q.rel)).await?))
}

async fn phone_file(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let root = phone_root(&hub, &q.path).await?;
    Ok(Json(blocking(move || crate::phone_files::file(&root, &q.rel)).await?))
}

/// A picture a thread's agent named by its path in an answer, for the phone (`phone_files::thread_picture`):
/// only inside the thread's own folder, only a picture, headers like `phone_raw`.
async fn thread_picture(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Query(q): Query<PictureQuery>) -> ApiResult<axum::response::Response> {
    let thread = hub.store.thread(&id).await?.ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such thread".into()))?;
    let (mime, bytes) = blocking(move || crate::phone_files::thread_picture(std::path::Path::new(&thread.cwd), &q.path))
        .await
        .map_err(|e| ApiError::new(StatusCode::NOT_FOUND, e.message))?;
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_owned()),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'; sandbox".to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize)]
struct PictureQuery {
    /// The path as the agent wrote it: absolute, or relative to the thread's folder.
    path: String,
}

/// An HTML file a thread's agent named by its path in an answer, run as a live page for the phone: the
/// same page route `show_page` uses, only for a file inside the thread's own folder
/// (`phone_files::thread_html`). Says where to open it, like `project_page`.
async fn thread_page(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(q): Json<PictureQuery>) -> ApiResult<impl IntoResponse> {
    let thread = hub.store.thread(&id).await?.ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "no such thread".into()))?;
    let data_dir = hub.config.data_dir.clone();
    let page = blocking(move || {
        let cwd = std::path::PathBuf::from(&thread.cwd);
        let rel = crate::phone_files::thread_html(&cwd, &q.path)?;
        crate::pages::save_file(&data_dir, &cwd, &rel)
    })
    .await
    .map_err(|e| ApiError::new(StatusCode::NOT_FOUND, e.message))?;
    Ok(Json(serde_json::json!({ "thread": crate::pages::FILES, "page": page })))
}

/// A picture or PDF as it is, sandboxed like `project_raw`.
async fn phone_raw(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<axum::response::Response> {
    let root = phone_root(&hub, &q.path).await?;
    let (mime, bytes) = blocking(move || crate::phone_files::raw(&root, &q.rel)).await?;
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_owned()),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'; sandbox".to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        bytes,
    )
        .into_response())
}

/// File names holding `rel`, in the whole project.
async fn phone_search(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let root = phone_root(&hub, &q.path).await?;
    Ok(Json(blocking(move || crate::phone_files::search(&root, &q.rel)).await?))
}

/// A one-time code for another device, with the address Tailscale serves this machine at.
async fn pair(State(hub): State<Arc<Hub>>) -> Json<crate::model::PairCode> {
    // A code is for a device somewhere else, which reaches this machine through Tailscale serve:
    // turn it on when it is not yet, as the phone's code does.
    let url = crate::pairing::tailscale_serve(hub.config.bind.port()).await.ok();
    Json(hub.pairing.code(url))
}

async fn redeem(
    State(hub): State<Arc<Hub>>,
    tailnet: Option<axum::Extension<crate::phone::Tailnet>>,
    sender: Option<axum::Extension<crate::phone::Sender>>,
    Json(req): Json<crate::model::PairRequest>,
) -> ApiResult<impl IntoResponse> {
    let tailnet = tailnet.is_some_and(|t| t.0.0);
    let sender = sender.and_then(|s| s.0.0);
    let token = hub.pairing.redeem_from(&hub.store, &req.code, &req.name, tailnet, sender).await.map_err(|e| ApiError::new(StatusCode::FORBIDDEN, format!("{e:#}")))?;
    Ok(Json(crate::model::Paired { token, machine: hub.remotes.local_name().to_owned() }))
}

/// A QR code for the phone: lets Tailscale serve this machine on the tailnet when it does not yet,
/// and makes a phone's one-time code.
async fn phone_link(State(hub): State<Arc<Hub>>) -> Json<crate::model::PhoneLink> {
    let code = hub.pairing.phone_code();
    let (url, problem) = match crate::pairing::tailscale_serve(hub.config.bind.port()).await {
        Ok(url) => (Some(url), None),
        Err(e) => (None, Some(format!("{e:#}"))),
    };
    let link = url.map(|url| format!("{}{}#pair={}", url.trim_end_matches('/'), crate::phone::PATH, code.code));
    let qr = link.as_deref().and_then(crate::phone::qr_svg);
    Json(crate::model::PhoneLink { code: code.code, expires_at: code.expires_at, link, qr, problem })
}

async fn devices(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.pairing.devices(&hub.store).await?))
}

async fn revoke_device(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    hub.pairing.revoke(&hub.store, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A one-time phone code on this machine, asked for by the machine the phone paired with, so the
/// phone can follow this one directly.
async fn phone_code(State(hub): State<Arc<Hub>>) -> Json<crate::model::PairCode> {
    let mut code = hub.pairing.phone_code();
    code.url = crate::pairing::tailscale_url(hub.config.bind.port()).await;
    Json(code)
}

/// The other machines a phone can reach by itself, with a phone code on each it has no token
/// for. The phone redeems the codes at the machines themselves.
async fn phone_machines(
    State(hub): State<Arc<Hub>>,
    Json(req): Json<crate::model::PhoneMachinesRequest>,
) -> Json<Vec<crate::model::PhoneMachine>> {
    Json(hub.remotes.phone_machines(&req.have).await)
}

#[cfg(test)]
mod phone_tests {
    use super::*;

    fn folder(path: &str) -> crate::model::ProjectFolder {
        crate::model::ProjectFolder {
            name: path.rsplit(['/', '\\']).next().unwrap_or_default().into(),
            path: path.into(),
            branch: None,
            key: None,
            touched: 0,
            main: None,
            behind: None,
            worktree_of: None,
            worktrees: 0,
        }
    }

    #[test]
    fn a_path_from_the_phone_finds_its_listed_project() {
        let list = [folder("C:\\Users\\me\\Desk"), folder("/home/me/code/hub"), folder("/home/me/code/hub/sub")];
        let name = |p: Option<&crate::model::ProjectFolder>| p.map(|p| p.path.clone());
        // The folder itself, in either slash style, case and a trailing slash aside.
        assert_eq!(name(pick_project(&list, "c:/users/me/desk/")), Some("C:\\Users\\me\\Desk".into()));
        // A working folder inside the project means the project, the deepest one listed.
        assert_eq!(name(pick_project(&list, "/home/me/code/hub/crates/x")), Some("/home/me/code/hub".into()));
        assert_eq!(name(pick_project(&list, "/home/me/code/hub/sub/y")), Some("/home/me/code/hub/sub".into()));
        // A folder that is only a name prefix, or not listed at all, is refused.
        assert!(pick_project(&list, "/home/me/code/hubris").is_none());
        assert!(pick_project(&list, "/etc").is_none());
    }

    #[test]
    fn a_scratch_folder_is_named_by_the_time_and_never_reused() {
        let root = crate::janitor::test_dir();
        // 2026-11-01 13:03:44 UTC.
        let at = 1_793_538_224_000;
        let first = scratch_folder(&root, at, "scratch").unwrap();
        assert_eq!(first, root.join("scratch").join("2026-11-01-130344"));
        assert_eq!(scratch_folder(&root, at, "scratch").unwrap(), root.join("scratch").join("2026-11-01-130344-2"));
        assert!(first.is_dir());
        // An everyday chat from the phone gets its own folder beside scratch, so paths tell them apart.
        let chat = scratch_folder(&root, at, "chats").unwrap();
        assert_eq!(chat, root.join("chats").join("2026-11-01-130344"));
    }

    #[test]
    fn a_chat_that_is_warmed_again_keeps_its_folder() {
        let root = crate::janitor::test_dir();
        let at = 1_793_538_300_000;
        let first = waiting_chat_folder(&root, at).unwrap();
        // A changed pick asks again: the same empty folder, not a new one.
        assert_eq!(waiting_chat_folder(&root, at + 5_000).unwrap(), first);
        // Once something is in it, or a thread took it, the next chat gets its own.
        std::fs::write(first.join("note"), "x").unwrap();
        let second = waiting_chat_folder(&root, at + 9_000).unwrap();
        assert_ne!(second, first);
        *WAITING_CHAT.locked() = None;
        assert_ne!(waiting_chat_folder(&root, at + 20_000).unwrap(), second);
    }

    #[test]
    fn a_phone_tick_is_only_a_tick() {
        let edit = phone_tick(PhoneTick { path: "/dev/hub".into(), line: 12, done: true });
        assert_eq!((edit.op.as_str(), edit.line, edit.done), ("tick", Some(12), Some(true)));
        assert!(edit.text.is_none() && edit.list.is_none());
    }

    #[test]
    fn phone_coding_requires_an_existing_non_home_folder() {
        // `root` is Hub's data folder; projects live elsewhere, since one inside it is refused. Both
        // are in a home of the test's own, laid out as on Windows: the machine's temp folder is in
        // its AppData there, a folder of logins no phone may code in.
        let home = crate::janitor::test_dir();
        let (root, away) = (home.join("AppData").join("Local").join("hub"), home.join("code"));
        let phone_project = |path: &str, root: &std::path::Path| phone_project(path, root, Some(&home));
        for name in ["desk", "platschef", "project"] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        for name in ["project", "other/desk"] {
            std::fs::create_dir_all(away.join(name)).unwrap();
        }
        for path in [String::new(), " ".into(), root.join("missing").display().to_string(),
            root.join("desk").display().to_string(), root.join("project/../desk").display().to_string(),
            root.join("platschef").display().to_string(), root.join("project").display().to_string()] {
            let error = phone_project(&path, &root).expect_err("invalid coding destination");
            assert_eq!(error.status, StatusCode::BAD_REQUEST);
        }
        // A real project named desk elsewhere is not the Home workspace.
        for name in ["project", "other/desk"] {
            assert!(phone_project(&away.join(name).display().to_string(), &root).is_ok());
        }
        let moved = crate::hub_tools::folder(&root.join("old-install")).unwrap();
        let moved_to = away.join("moved-desk");
        std::fs::rename(moved, &moved_to).unwrap();
        assert!(phone_project(&moved_to.display().to_string(), &root).is_err());
        std::fs::create_dir(moved_to.join("child")).unwrap();
        assert!(phone_project(&moved_to.join("child").display().to_string(), &root).is_err());
        let scratch = scratch_folder(&away, 1_793_538_224_000, "scratch").unwrap();
        assert!(phone_project(&scratch.display().to_string(), &root).is_ok());
        #[cfg(unix)] {
            std::os::unix::fs::symlink(root.join("desk"), root.join("alias")).unwrap();
            assert!(phone_project(&root.join("alias").display().to_string(), &root).is_err());
        }
    }

    #[test]
    fn a_phone_thread_is_an_ordinary_one() {
        let p = PhoneThread { cwd: "/dev/hub".into(), scratch: false, chat: false, message: "go".into(), engine: Some("claude".into()), title: None, client_id: Some("c1".into()), attachments: vec![], settings: Default::default(), warm: None, cloud: false };
        let new = phone_new_thread(p).unwrap();
        assert_eq!((new.cwd.as_str(), new.message.as_deref(), new.engine.as_deref()), ("/dev/hub", Some("go"), Some("claude")));
        assert!(new.settings.is_empty() && new.attempts.is_empty() && !new.design && !new.cloud);
    }

    #[test]
    fn a_phone_can_ask_for_the_cloud() {
        let p: PhoneThread = serde_json::from_value(serde_json::json!({ "cwd": "/p", "message": "hi", "cloud": true })).unwrap();
        assert!(phone_new_thread(p).unwrap().cloud);
    }

    #[test]
    fn a_phone_picks_model_and_effort_but_not_the_permission_mode() {
        let p: PhoneThread = serde_json::from_value(serde_json::json!({
            "cwd": "/p", "message": "hi", "warm": "w1",
            "settings": { "model": "opus", "thought_level": "high", "hub_route": "deepseek:deepseek-v4-pro", "mode": "bypassPermissions", "env": "x", "effort": 3 },
        }))
        .unwrap();
        let new = phone_new_thread(p).unwrap();
        let keys: Vec<&str> = new.settings.keys().map(String::as_str).collect();
        assert_eq!(keys, ["hub_route", "model", "thought_level"]);
        assert_eq!(new.warm.as_deref(), Some("w1"));
        assert!(new.attempts.is_empty() && new.unattended.is_none());
    }
}
