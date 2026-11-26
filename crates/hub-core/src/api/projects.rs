//! Routes for the projects area: projects, folders, the library, ideas, routines and recipes, and
//! a project's files, vault, focus and services.

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};

use super::*;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
        .route("/api/routines", get(|State(hub): State<Arc<Hub>>| async move { crate::routines::list(&hub.ctx()).await.map(Json).map_err(ApiError::from) })
            .post(|State(hub): State<Arc<Hub>>, Json(draft): Json<crate::model::RoutineDraft>| async move {
                crate::routines::create(&hub.ctx(), draft).await.map(Json).map_err(ApiError::from)
            }))
        .route("/api/routines/{id}", axum::routing::put(|State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(draft): Json<crate::model::RoutineDraft>| async move {
                crate::routines::update(&hub.ctx(), &id, draft).await.map(Json).map_err(ApiError::from)
            })
            .delete(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
                crate::routines::delete(&hub.ctx(), &id).await.map(Json).map_err(ApiError::from)
            }))
        .route("/api/routines/{id}/enabled", post(|State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(ask): Json<serde_json::Value>| async move {
            let enabled = ask.get("enabled").and_then(|v| v.as_bool()).ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "enabled: true or false".into()))?;
            crate::routines::set_enabled(&hub.ctx(), &id, enabled).await.map(Json).map_err(ApiError::from)
        }))
        .route("/api/routines/{id}/run", post(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
            crate::routine_runs::start(&hub, &id, "you").await.map(|thread| Json(serde_json::json!({ "thread": thread }))).map_err(ApiError::from)
        }))
        .route("/api/recipes", get(|State(hub): State<Arc<Hub>>| async move { crate::recipes::list(&hub.ctx()).await.map(Json).map_err(ApiError::from) })
            .post(|State(hub): State<Arc<Hub>>, Json(draft): Json<crate::model::RecipeDraft>| async move {
                crate::recipes::create(&hub.ctx(), draft).await.map(Json).map_err(ApiError::from)
            }))
        .route("/api/recipes/{id}", axum::routing::put(|State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(draft): Json<crate::model::RecipeDraft>| async move {
                crate::recipes::update(&hub.ctx(), &id, draft).await.map(Json).map_err(ApiError::from)
            })
            .delete(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
                crate::recipes::delete(&hub.ctx(), &id).await.map(Json).map_err(ApiError::from)
            }))
        .route("/api/recipes/{id}/keep", post(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
            crate::recipes::keep(&hub.ctx(), &id).await.map(Json).map_err(ApiError::from)
        }))
        .route("/api/recipes/{id}/run", post(|State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(run): Json<crate::model::RunRecipe>| async move {
            let how = crate::recipes::Start { note: run.note, ..Default::default() };
            hub.start_recipe(&id, run.folder.as_deref(), how).await.map(|thread| Json(serde_json::json!({ "thread": thread }))).map_err(ApiError::from)
        }))
        .route("/api/projects/trees", get(project_trees).put(set_project_trees))
        .route("/api/projects", get(list_projects).post(create_project).delete(remove_project))
        .route("/api/projects/defaults", get(project_defaults).put(set_project_defaults))
        .route("/api/projects/templates", get(project_templates))
        .route("/api/folders", get(list_folders).post(create_folder))
        .route("/api/folders/repo", get(repo_places))
        .route("/api/folders/projects", get(project_folders))
        .route("/api/folders/clone", post(clone_repo))
        .route("/api/folders/pack", post(pack_project))
        .route("/api/folders/unpack", post(unpack_project))
        .route("/api/folders/bring", post(bring_project))
        .route("/api/project", get(project_info))
        .route("/api/project/runtime", get(project_runtime))
        .route("/api/project/outputs", get(project_outputs))
        .route("/api/project/outputs/text", get(output_text))
        .route("/api/project/outputs/raw", get(output_raw))
        .route("/api/project/tree", get(project_tree))
        .route("/api/project/file", get(project_file))
        .route("/api/project/sheet", get(project_sheet))
        .route("/api/project/database", get(project_database))
        .route("/api/project/raw", get(project_raw))
        .route("/api/project/page", post(project_page))
        .route("/api/project/vault", get(project_vault))
        .route("/api/project/vault/doc", get(vault_doc).put(save_doc))
        .route("/api/library", get(library))
        .route("/api/library/prompt", get(library_prompt))
        .route("/api/project/vault/search", get(vault_search))
        .route("/api/project/plan", post(project_plan))
        .route("/api/project/rules", get(project_rules))
        .route("/api/project/focus", post(project_focus))
        .route("/api/project/focus/summaries", post(focus_summaries))
        .route("/api/ideas", get(list_ideas).post(capture_idea))
        .route("/api/ideas/places", get(idea_places))
        .route("/api/ideas/{id}", axum::routing::delete(drop_idea))
        .route("/api/ideas/{id}/move", post(move_idea))
        .route("/api/project/focus/draft", get(|| async { Json(serde_json::json!({ "message": crate::focus::draft_prompt() })) }))
        .route("/api/project/docs", post(project_docs))
        .route("/api/open", post(open_url))
        .route("/api/project/open", post(open_folder))
        .route("/api/reveal", post(reveal))
        .route("/api/downloads", post(download))
        .route("/api/project/images", post(project_image))
        .route("/api/project/services", get(project_services))
        .route("/api/project/services/start", post(start_services))
        .route("/api/project/services/stop", post(stop_services))
        .route("/api/project/services/log", get(service_log))
        .route("/api/project/services/save", post(save_services))
}

async fn list_projects(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.store.projects().await?))
}

async fn create_project(
    State(hub): State<Arc<Hub>>,
    Json(new): Json<crate::model::NewProject>,
) -> ApiResult<impl IntoResponse> {
    Ok((StatusCode::CREATED, Json(hub.create_project(new).await?)))
}

#[derive(Deserialize)]
struct FoldersQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    hidden: bool,
}

async fn list_folders(Query(q): Query<FoldersQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::folders::list(&q.path, q.hidden)).await?))
}

async fn create_folder(Json(new): Json<crate::model::NewFolder>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::folders::create(&new.path, new.git)).await?))
}

#[derive(Deserialize)]
struct RepoQuery {
    /// A folder on this machine: its repository's key and remote.
    path: Option<String>,
    /// A repository key: where it is on this machine.
    key: Option<String>,
}

async fn repo_places(State(hub): State<Arc<Hub>>, Query(q): Query<RepoQuery>) -> ApiResult<impl IntoResponse> {
    let root = hub.projects_root().await?;
    let mut known: Vec<String> = hub.store.threads().await?.into_iter().filter(|t| t.host.is_none()).map(|t| t.cwd).collect();
    known.extend(hub.store.projects().await?.into_iter().map(|p| p.path));
    known.sort();
    known.dedup();
    let places = blocking(move || {
        let (key, url) = match (&q.path, &q.key) {
            (Some(path), _) => (crate::folders::repo_key(path), crate::folders::clone_url(path)),
            (None, Some(key)) => (Some(key.trim().to_lowercase()), None),
            (None, None) => anyhow::bail!("give a path or a key"),
        };
        let found = match (&key, &q.key) {
            (Some(key), Some(_)) => crate::folders::find_repo(key, &known, std::slice::from_ref(&root)),
            _ => Vec::new(),
        };
        Ok(crate::model::RepoPlaces { key, url, found, root, home: crate::folders::home().display().to_string() })
    })
    .await?;
    Ok(Json(places))
}

/// The projects on this machine, for the folder picker to start from.
async fn project_folders(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let root = hub.projects_root().await?;
    let mut known: Vec<String> = hub.store.threads().await?.into_iter().filter(|t| t.host.is_none()).map(|t| t.cwd).collect();
    known.extend(hub.store.projects().await?.into_iter().map(|p| p.path));
    known.sort();
    known.dedup();
    Ok(Json(blocking(move || Ok(crate::folders::projects(&root, &known))).await?))
}

async fn clone_repo(Json(body): Json<crate::model::CloneRepo>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::folders::clone(&body.url, &body.path)).await?))
}

#[derive(Deserialize)]
struct PackRequest {
    path: String,
}

async fn pack_project(Json(body): Json<PackRequest>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::carry::pack(&body.path)).await?))
}

async fn unpack_project(Json(body): Json<crate::model::UnpackProject>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::carry::unpack(body)).await?))
}

/// Carries a project between two machines (either may be this one). The pack goes through this
/// core, so the two machines need not reach each other, only this one.
async fn bring_project(State(hub): State<Arc<Hub>>, Json(body): Json<crate::model::BringProject>) -> ApiResult<impl IntoResponse> {
    Ok(Json(bring(&hub, body).await?))
}

#[derive(Deserialize)]
struct TreesQuery {
    path: String,
}

async fn project_trees(State(hub): State<Arc<Hub>>, Query(q): Query<TreesQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.project_trees(&q.path).await?))
}

async fn set_project_trees(State(hub): State<Arc<Hub>>, Json(set): Json<crate::model::ProjectTreesSet>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.set_project_trees(set).await?))
}

async fn project_defaults(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let (root, github) = tokio::join!(hub.projects_root(), crate::project::gh_ready());
    Ok(Json(crate::model::ProjectDefaults { root: root?, github }))
}

async fn project_templates() -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::templates::available().await))
}

async fn set_project_defaults(
    State(hub): State<Arc<Hub>>,
    Json(body): Json<crate::model::SetProjectRoot>,
) -> ApiResult<impl IntoResponse> {
    hub.set_projects_root(&body.root).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_project(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    hub.remove_project(&q.path).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn project_info(Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::project::info(std::path::Path::new(&q.path))).await?))
}

async fn project_runtime(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let threads = hub.store.threads().await?;
    let root = std::path::PathBuf::from(&q.path);
    let blocking_root = root.clone();
    let processes = blocking(move || Ok(crate::runtime::processes(&blocking_root, &threads)));
    let (processes, containers) = tokio::join!(processes, crate::runtime::containers(&root));
    let mut runtime = processes?;
    runtime.containers = containers;
    Ok(Json(runtime))
}

/// What the project's threads made, where each thing is now and what happened to it (`outputs.rs`).
async fn project_outputs(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::outputs::list(&hub, &q.path).await?))
}

/// The text of one thing in the project's library (`rel` is its id), to read it inside Hub.
async fn output_text(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    match crate::outputs::content(&hub, &q.path, &q.rel).await? {
        crate::outputs::Content::File(path) => {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            Ok(Json(blocking(move || crate::project::text_of(&path, &name)).await?))
        }
        crate::outputs::Content::Picture { .. } => Err(ApiError::new(StatusCode::BAD_REQUEST, "a picture has no text".into())),
    }
}

/// One picture of the project's library as it is (`rel` is its id), for an `<img>` in Hub.
async fn output_raw(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<axum::response::Response> {
    let (mime, bytes) = match crate::outputs::content(&hub, &q.path, &q.rel).await? {
        crate::outputs::Content::Picture { mime, bytes } => (mime, bytes),
        crate::outputs::Content::File(path) => {
            let mime = crate::outputs::picture_mime(&path).ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "only pictures are shown raw".into()))?;
            let bytes = blocking(move || Ok(std::fs::read(&path)?)).await?;
            (mime.to_owned(), bytes)
        }
    };
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'; sandbox".to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        bytes,
    )
        .into_response())
}

async fn project_tree(Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::project::tree(std::path::Path::new(&q.path), &q.rel)).await?))
}

async fn project_file(Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::project::read(std::path::Path::new(&q.path), &q.rel)).await?))
}

/// A spreadsheet (`.xlsx`, `.xlsm`, `.xls`, `.ods`) of a project as rows of text, one table per sheet,
/// for the file readers (`sheets.rs`).
async fn project_sheet(Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::sheets::read(std::path::Path::new(&q.path), &q.rel)).await?))
}

/// A SQLite file (`.db`, `.sqlite`, `.sqlite3`) of a project as its tables with columns, row counts and
/// first rows, for the file readers (`dbview.rs`).
async fn project_database(Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::dbview::read(std::path::Path::new(&q.path), &q.rel)).await?))
}

/// A picture, PDF, sound or video file as it is, for the file readers. Sandboxed like the
/// library's pictures, so an SVG or PDF opened on its own cannot run anything as Hub.
async fn project_raw(Query(q): Query<ProjectQuery>) -> ApiResult<axum::response::Response> {
    let (mime, bytes) = blocking(move || crate::project::raw(std::path::Path::new(&q.path), &q.rel)).await?;
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

/// Runs an HTML file of a project as a live page (see `pages::save_file`) and says where to open it:
/// the same page route `show_page` uses, so the viewer can frame it with scripts allowed and no
/// network, instead of showing a still picture.
async fn project_page(State(hub): State<Arc<Hub>>, Json(q): Json<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let data_dir = hub.config.data_dir.clone();
    let page = blocking(move || crate::pages::save_file(&data_dir, std::path::Path::new(&q.path), &q.rel)).await?;
    Ok(Json(serde_json::json!({ "thread": crate::pages::FILES, "page": page })))
}

/// The project's markdown as linked docs: titles, tags and how they link.
async fn project_vault(Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || Ok(crate::vault::map(std::path::Path::new(&q.path)))).await?))
}

/// One doc with where its links lead and what links to it. `rel` is a path or a doc name.
async fn vault_doc(Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::vault::doc(std::path::Path::new(&q.path), &q.rel)).await?))
}

#[derive(Deserialize)]
struct SaveDoc {
    path: String,
    rel: String,
    body: String,
}

/// Writes a doc the user edited in the docs tab.
async fn save_doc(Json(b): Json<SaveDoc>) -> ApiResult<impl IntoResponse> {
    blocking(move || crate::vault::save(std::path::Path::new(&b.path), &b.rel, &b.body)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Where the library is, after Hub brought its facts up to date. The app reads it like a project's docs.
async fn library(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let root = hub.library().await?;
    Ok(Json(serde_json::json!({ "root": root.to_string_lossy() })))
}

#[derive(Deserialize)]
struct LibraryPrompt {
    #[serde(default)]
    ask: Option<String>,
    #[serde(default)]
    learn: Option<String>,
}

/// The first message of a thread that answers a question from the library, or teaches one note and checks it stuck.
async fn library_prompt(Query(q): Query<LibraryPrompt>) -> ApiResult<impl IntoResponse> {
    let message = match (q.learn, q.ask) {
        (Some(note), _) => crate::library::learn_prompt(note.trim()),
        (None, Some(question)) => crate::library::ask_prompt(&question),
        (None, None) => return Err(ApiError::new(StatusCode::BAD_REQUEST, "ask or learn what?".into())),
    };
    Ok(Json(serde_json::json!({ "message": message })))
}

#[derive(Deserialize)]
struct VaultSearch {
    path: String,
    q: String,
}

async fn vault_search(Query(q): Query<VaultSearch>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || Ok(crate::vault::search(std::path::Path::new(&q.path), &q.q, 30))).await?))
}

async fn project_plan(Json(edit): Json<crate::project::PlanEdit>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::project::edit_plan(std::path::Path::new(&edit.path), &edit)).await?))
}

#[derive(Deserialize)]
struct PathsBody {
    paths: Vec<String>,
}

/// Each folder's focus in a line, by path; folders without a focus file are left out.
async fn focus_summaries(Json(body): Json<PathsBody>) -> ApiResult<impl IntoResponse> {
    Ok(Json(
        blocking(move || {
            Ok(body
                .paths
                .into_iter()
                .take(200)
                .filter_map(|p| crate::focus::summary(std::path::Path::new(&p)).map(|s| (p, s)))
                .collect::<std::collections::BTreeMap<_, _>>())
        })
        .await?,
    ))
}

async fn list_ideas(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.ideas().await?))
}

/// Keeps the idea and answers at once; the stream says where it went.
async fn capture_idea(State(hub): State<Arc<Hub>>, Json(new): Json<crate::model::NewIdea>) -> ApiResult<impl IntoResponse> {
    Ok((StatusCode::CREATED, Json(hub.capture_idea(new, false).await?)))
}

async fn idea_places(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let places: Vec<crate::model::IdeaPlace> = hub
        .idea_candidates()
        .await?
        .into_iter()
        .map(|c| crate::model::IdeaPlace {
            stages: c.focus.stages.iter().map(|s| s.title.clone()).collect(),
            active: c.focus.active,
            path: c.path,
            name: c.name,
        })
        .collect();
    Ok(Json(places))
}

async fn move_idea(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(to): Json<crate::model::MoveIdea>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.move_idea(&id, to).await?))
}

async fn drop_idea(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    hub.drop_idea(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The rules the project's rules files give its agents, each with when it last changed.
async fn project_rules(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    let mut st = blocking(move || Ok(crate::standing::read(std::path::Path::new(&q.path)))).await?;
    crate::standing::with_threads(&mut st, &hub.store.rules_said().await?);
    Ok(Json(st))
}

async fn project_focus(Json(edit): Json<crate::focus::FocusEdit>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::focus::edit(std::path::Path::new(&edit.path), &edit)).await?))
}

#[derive(Deserialize)]
struct ProjectBody {
    path: String,
}

async fn project_docs(Json(body): Json<ProjectBody>) -> ApiResult<impl IntoResponse> {
    blocking(move || crate::project::set_up_docs(std::path::Path::new(&body.path))).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct OpenBody {
    url: String,
}

async fn open_url(Json(body): Json<OpenBody>) -> ApiResult<impl IntoResponse> {
    blocking(move || crate::project::open_url(&body.url)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct OpenFolderBody {
    path: String,
    /// editor, terminal or files.
    with: String,
}

async fn open_folder(State(hub): State<Arc<Hub>>, Json(body): Json<OpenFolderBody>) -> ApiResult<impl IntoResponse> {
    blocking(move || {
        let open = crate::prefs::open_config(&hub.config);
        crate::project::open_folder(std::path::Path::new(&body.path), &body.with, &open)
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct DownloadQuery {
    name: String,
}

/// The body is the file's bytes; it lands in Downloads, and the answer says where.
async fn download(Query(q): Query<DownloadQuery>, body: axum::body::Bytes) -> ApiResult<impl IntoResponse> {
    let path = blocking(move || crate::project::save_download(&q.name, &body)).await?;
    Ok(Json(serde_json::json!({ "path": path })))
}

#[derive(Deserialize)]
struct ProjectImageQuery {
    path: String,
    name: String,
}

/// The body is the picture's bytes; it lands in the project's `docs/images`, and the answer says where.
async fn project_image(Query(q): Query<ProjectImageQuery>, body: axum::body::Bytes) -> ApiResult<impl IntoResponse> {
    let path = blocking(move || crate::project::add_image(std::path::Path::new(&q.path), &q.name, &body)).await?;
    Ok(Json(serde_json::json!({ "path": path })))
}

async fn reveal(Json(body): Json<ProjectBody>) -> ApiResult<impl IntoResponse> {
    blocking(move || crate::project::reveal(std::path::Path::new(&body.path))).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn project_services(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::services::listed(&q.path, &hub.config.data_dir).await?))
}

#[derive(Deserialize)]
struct ServicesBody {
    path: String,
    /// One service; without it, everything.
    #[serde(default)]
    name: Option<String>,
}

async fn start_services(State(hub): State<Arc<Hub>>, Json(body): Json<ServicesBody>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::services::start_named(&body.path, &hub.config.data_dir, body.name.as_deref()).await?))
}

async fn stop_services(State(hub): State<Arc<Hub>>, Json(body): Json<ServicesBody>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.stop_services(&body.path, body.name.as_deref()).await?))
}

#[derive(Deserialize)]
struct ServiceLogQuery {
    path: String,
    name: String,
}

async fn service_log(State(hub): State<Arc<Hub>>, Query(q): Query<ServiceLogQuery>) -> ApiResult<impl IntoResponse> {
    let text = blocking(move || crate::services::log_tail(std::path::Path::new(&q.path), &hub.config.data_dir, &q.name)).await?;
    Ok(Json(serde_json::json!({ "text": text })))
}

async fn save_services(Json(body): Json<ServicesBody>) -> ApiResult<impl IntoResponse> {
    let file = blocking(move || crate::services::save_guess(std::path::Path::new(&body.path))).await?;
    Ok(Json(serde_json::json!({ "file": file })))
}

/// `GET /new?prompt=…`: a prompt link (`promptlink.rs`). Puts the draft in the open window's
/// message box and answers with a page that says whether a window was there to take it.
pub(super) async fn prompt_link(State(hub): State<Arc<Hub>>, headers: HeaderMap, Query(q): Query<crate::promptlink::LinkQuery>) -> Response {
    let draft = match crate::promptlink::draft(&headers, q) {
        Ok(draft) => draft,
        Err(why) => return (StatusCode::FORBIDDEN, why).into_response(),
    };
    let seen = hub.subscribers() > 0;
    hub.publish(crate::model::StreamMsg::Draft { draft });
    axum::response::Html(crate::promptlink::page(seen)).into_response()
}
