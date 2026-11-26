//! Routes for the github area: pull requests and issues, the updater, the dev setup and the
//! (retired) merge queue. Handlers moved here from `api.rs` unchanged.

use super::*;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
        .route("/api/update/install", post(|State(hub): State<Arc<Hub>>| async move {
            hub.approve_install()?;
            ApiResult::Ok(StatusCode::ACCEPTED)
        }))
        .route("/api/update/retry", post(|State(hub): State<Arc<Hub>>| async move {
            hub.retry_update()?;
            ApiResult::Ok(StatusCode::ACCEPTED)
        }))
        .route("/api/update/look", post(update_look))
        .route("/api/update/build-now", post(|State(hub): State<Arc<Hub>>| async move {
            hub.build_update_now()?;
            ApiResult::Ok(StatusCode::ACCEPTED)
        }))
        .route("/api/update/waiting", get(update_waiting))
        .route("/api/update/alive", post(update_alive))
        .route("/api/update/rolled-back", axum::routing::delete(|State(hub): State<Arc<Hub>>| async move {
            crate::rollback::dismiss(&hub);
            StatusCode::NO_CONTENT
        }))
        .route("/api/prs", get(|State(hub): State<Arc<Hub>>| async move { Json(hub.thread_prs()) }))
        .route("/api/updates", get(updates_fleet))
        .route("/api/updates/run", post(update_everywhere))
        .route("/api/updates/follow", axum::routing::put(set_follow))
        .route("/api/updates/news", get(cli_news))
        .route("/api/mergeq", get(merge_queue).put(ask_merge_queue))
        .route("/api/dev", get(dev_status))
        .route("/api/dev/setup", post(dev_setup))
        .route("/api/dev/start", post(|State(hub): State<Arc<Hub>>| async move { hub.dev_start().await.map(Json).map_err(ApiError::from) }))
        .route("/api/dev/stop", post(|State(hub): State<Arc<Hub>>| async move { hub.dev_stop().await.map(Json).map_err(ApiError::from) }))
        .route("/api/dev/ship", post(dev_ship))
        .route("/api/dev/core", post(dev_core))
        .route("/api/dev/preview", post(dev_preview))
        .route("/api/dev/show", post(dev_show))
        .route("/api/dev/worktrees", post(dev_worktrees))
        .route("/api/dev/worktrees/remove", post(dev_remove_worktree))
        .route("/api/project/github", get(project_github))
        .route("/api/github/pulls", get(github_pulls))
        .route("/api/github/pull", get(github_pull))
        .route("/api/github/card", get(github_card))
        .route("/api/github/diff", get(github_diff))
        .route("/api/github/action", post(github_action))
        .route("/api/github/watches", get(github_watches))
        .route("/api/github/repos", get(github_repos))
        .route("/api/github/connect", get(github_connect).post(github_connect_start).delete(github_connect_cancel))
        .route("/api/github/disconnect", post(github_disconnect))
        .route("/api/github/issues", get(github_issues))
        .route("/api/github/issue", get(github_issue).post(github_issue_action))
        .route("/api/github/branches", get(github_branches).delete(github_delete_branches))
}

#[derive(Deserialize, Default)]
struct LookBody {
    /// The version the other machine says it sent.
    #[serde(default)]
    version: Option<String>,
}

/// Another machine's "I sent you this version, look now": answers with the updater's state.
async fn update_look(State(hub): State<Arc<Hub>>, body: Option<Json<LookBody>>) -> impl IntoResponse {
    let body = body.map(|b| b.0).unwrap_or_default();
    Json(hub.update_look(body.version.as_deref()))
}

#[derive(serde::Deserialize)]
struct AliveBody {
    /// The version stamp of the pages that rendered.
    version: String,
}

/// A window says the pages of its version rendered, which ends the probation of a new version that
/// needs one (`rollback.rs`). `counted` is false for pages of another version than the one running.
async fn update_alive(State(hub): State<Arc<Hub>>, Json(body): Json<AliveBody>) -> impl IntoResponse {
    let running = hub.update_status().version;
    let counted = crate::rollback::alive(&hub.config.data_dir, &running, &body.version);
    Json(serde_json::json!({ "counted": counted }))
}

async fn update_waiting(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let running = hub.update_status().version;
    Ok(Json(blocking(move || Ok(crate::update::waiting_changes(&hub.config.update, &hub.config.data_dir, &running))).await?))
}

#[derive(serde::Deserialize)]
struct FleetQuery {
    #[serde(default)]
    fresh: bool,
}

/// Every machine's hubd and CLI versions; the last read unless `fresh`.
async fn updates_fleet(State(hub): State<Arc<Hub>>, Query(q): Query<FleetQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(if q.fresh { crate::updates::fleet(&hub).await } else { crate::updates::cached(&hub).await }))
}

/// Updates one CLI, or hubd, on every machine online, or on the one named.
async fn update_everywhere(State(hub): State<Arc<Hub>>, Json(ask): Json<ToolAsk>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::updates::everywhere(&hub, &ask.tool, ask.machine.as_deref()).await?))
}

#[derive(serde::Deserialize)]
struct NewsQuery {
    tool: String,
    from: Option<String>,
    to: Option<String>,
}

/// What changed in an agent CLI between two versions, from its own release notes.
async fn cli_news(Query(q): Query<NewsQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::updates::news(&q.tool, q.from.as_deref(), q.to.as_deref()).await))
}

#[derive(serde::Deserialize)]
struct FollowAsk {
    on: bool,
}

async fn set_follow(State(hub): State<Arc<Hub>>, Json(ask): Json<FollowAsk>) -> ApiResult<impl IntoResponse> {
    crate::updates::set_follow(&hub, ask.on).await?;
    Ok(Json(crate::updates::cached(&hub).await))
}

/// The merge queue is gone. The route stays, answering "off", so an older app or phone that
/// still asks keeps working.
// compat-until: 2026-12-09
async fn merge_queue() -> impl IntoResponse {
    Json(crate::model::MergeQueue { mode: "off".into(), ..Default::default() })
}

async fn ask_merge_queue(Json(_): Json<crate::model::QueueAsk>) -> impl IntoResponse {
    merge_queue().await
}

async fn dev_status(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    hub.dev_looked();
    Ok(Json(hub.dev_status().await?))
}

#[derive(Deserialize, Default)]
struct DevSetup {
    /// Where to clone Hub, when not `<projects root>/hub`.
    dir: Option<String>,
}

async fn dev_setup(State(hub): State<Arc<Hub>>, body: Option<Json<DevSetup>>) -> ApiResult<impl IntoResponse> {
    let body = body.map(|Json(b)| b).unwrap_or_default();
    Ok(Json(hub.dev_setup(body.dir).await?))
}

async fn dev_ship(State(hub): State<Arc<Hub>>, Json(ship): Json<crate::model::DevShip>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.dev_ship(ship).await?))
}

#[derive(Deserialize)]
struct DevCore {
    on: bool,
}

async fn dev_core(State(hub): State<Arc<Hub>>, Json(body): Json<DevCore>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.dev_core(body.on).await?))
}

async fn dev_preview(State(hub): State<Arc<Hub>>, Json(set): Json<crate::model::DevPreviewSet>) -> ApiResult<impl IntoResponse> {
    hub.dev_looked();
    Ok(Json(hub.dev_preview(set).await?))
}

async fn dev_show(State(hub): State<Arc<Hub>>, Json(show): Json<crate::model::DevShow>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.dev_show(show).await?))
}

async fn dev_worktrees(State(hub): State<Arc<Hub>>, Json(body): Json<DevCore>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.dev_set_worktrees(body.on).await?))
}

#[derive(Deserialize)]
struct DevWorktree {
    thread: String,
}

async fn dev_remove_worktree(State(hub): State<Arc<Hub>>, Json(body): Json<DevWorktree>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.dev_remove_worktree(&body.thread).await?))
}

async fn project_github(Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::project::github(std::path::Path::new(&q.path)).await))
}

/// What the project has running and what it uses, from processes, connections and docker.
#[derive(Deserialize)]
struct PullQuery {
    path: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    number: u64,
}

async fn github_pulls(Query(q): Query<PullQuery>) -> Json<crate::model::PullList> {
    Json(crate::github::pulls(std::path::Path::new(&q.path), &q.state).await)
}

async fn github_pull(Query(q): Query<PullQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::github::pull(std::path::Path::new(&q.path), q.number).await?))
}

#[derive(Deserialize)]
struct CardQuery {
    path: String,
    #[serde(default)]
    repo: String,
    number: u64,
}

/// The hover card of a pull request named in a chat.
async fn github_card(Query(q): Query<CardQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::github::card(std::path::Path::new(&q.path), &q.repo, q.number).await?))
}

async fn github_diff(Query(q): Query<PullQuery>) -> ApiResult<Json<crate::voice::Transcript>> {
    let text = crate::github::diff(std::path::Path::new(&q.path), q.number).await?;
    Ok(Json(crate::voice::Transcript { text }))
}

async fn github_action(Json(a): Json<crate::model::PullAction>) -> ApiResult<impl IntoResponse> {
    crate::github::act(&a).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ReposQuery {
    #[serde(default)]
    fresh: bool,
}

/// Every repository of the account `gh` is signed in as on this machine.
async fn github_repos(Query(q): Query<ReposQuery>) -> Json<crate::model::GithubRepos> {
    Json(crate::github_repos::repos(q.fresh).await)
}

async fn github_connect() -> Json<crate::model::GithubConnect> {
    Json(crate::github_repos::status().await)
}

/// Starts `gh auth login` on this machine and answers with the code to type on GitHub.
async fn github_connect_start() -> Json<crate::model::GithubConnect> {
    Json(crate::github_repos::start().await)
}

async fn github_disconnect() -> Json<crate::model::GithubConnect> {
    Json(crate::github_repos::disconnect().await)
}

async fn github_connect_cancel() -> Json<crate::model::GithubConnect> {
    Json(crate::github_repos::cancel().await)
}

async fn github_issues(Query(q): Query<PullQuery>) -> Json<crate::model::IssueList> {
    Json(crate::github::issues(std::path::Path::new(&q.path), &q.state).await)
}

async fn github_issue(Query(q): Query<PullQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::github::issue(std::path::Path::new(&q.path), q.number).await?))
}

async fn github_issue_action(Json(a): Json<crate::model::IssueAction>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::github::act_on_issue(&a).await?))
}

async fn github_branches(Query(q): Query<ProjectQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::github::branches(std::path::Path::new(&q.path))).await?))
}

#[derive(Deserialize)]
struct DeleteBranches {
    path: String,
    names: Vec<String>,
    #[serde(default)]
    local: bool,
    #[serde(default)]
    remote: bool,
}

/// Deletes branches; answers with the ones that could not go, and why.
async fn github_delete_branches(Json(d): Json<DeleteBranches>) -> ApiResult<impl IntoResponse> {
    Ok(Json(blocking(move || crate::github::delete_branches(std::path::Path::new(&d.path), &d.names, d.local, d.remote)).await?))
}

/// The pull request each thread watches, by thread.
async fn github_watches(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let watches = hub.watches().await?;
    Ok(Json(watches.into_iter().map(|(id, w)| (id, w.number)).collect::<std::collections::BTreeMap<_, _>>()))
}
