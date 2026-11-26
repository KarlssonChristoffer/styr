//! Routes for the threads area: the threads themselves, their messages, queue, undo, handoff, attempts, the bag and the emoji.

use super::*;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
        .route("/api/waits", get(|State(hub): State<Arc<Hub>>| async move { Json(hub.waits.until()) }))
        .route("/api/threads", get(list_threads).post(create_thread))
        .route("/api/threads/move", post(move_thread))
        .route("/api/threads/leaving", post(leaving))
        .route("/api/threads/export", post(export_thread))
        .route("/api/threads/import", post(import_thread))
        .route("/api/threads/stepped-aside", post(stepped_aside))
        .route("/api/threads/warm", post(warm_new))
        .route("/api/branches", get(branches))
        .route("/api/threads/{id}", get(get_thread).patch(patch_thread).delete(delete_thread))
        .route("/api/threads/{id}/events", get(list_events))
        .route("/api/threads/{id}/tidy", post(tidy_thread))
        .route("/api/notices", get(|| async { Json(crate::notice::recent()) }).post(post_notice))
        .route("/api/threads/{id}/pages/{page}", get(page))
        .route("/api/threads/{id}/git", get(thread_git))
        .route("/api/threads/{id}/changes", get(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
            hub.thread_changes(&id).await.map(Json).map_err(ApiError::from)
        }))
        .route("/api/threads/{id}/messages", post(send_message))
        .route("/api/threads/{id}/react", post(react))
        .route("/api/emoji", get(|State(hub): State<Arc<Hub>>| async move { ApiResult::Ok(Json(crate::reactions::state(&hub.ctx()).await?)) }))
        .route("/api/emoji/{name}", axum::routing::put(put_emoji).delete(delete_emoji))
        .route("/api/threads/{id}/messages/{seq}/edit", post(|State(hub): State<Arc<Hub>>, Path((id, seq)): Path<(String, i64)>, Json(edit): Json<crate::model::EditFrom>| async move {
            hub.edit_from(&id, seq, edit.text, edit.attachments, edit.files).await?;
            ApiResult::Ok(StatusCode::ACCEPTED)
        }))
        .route("/api/threads/{id}/settings", post(set_setting))
        .route("/api/threads/{id}/files", get(find_files))
        .route("/api/threads/{id}/reach", post(reach))
        .route("/api/threads/{id}/rules", post(save_rule))
        .route("/api/threads/{id}/handoff", post(handoff))
        .route("/api/threads/{id}/retitle", post(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
            hub.regenerate_title(&id).await.map(Json).map_err(ApiError::from)
        }))
        .route("/api/threads/{id}/peek", get(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
            crate::gist::peek(&hub.ctx(), &id).await.map(Json).map_err(ApiError::from)
        }))
        .route("/api/threads/{id}/gist", post(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
            // Older apps ask for a TL;DR when the card opens; Hub no longer writes one itself.
            crate::gist::peek(&hub.ctx(), &id).await.map(Json).map_err(ApiError::from)
        }))
        .route("/api/threads/{id}/undo/preview", post(undo_preview))
        .route("/api/threads/{id}/undo", post(undo))
        .route("/api/threads/{id}/review", post(review))
        .route("/api/threads/{id}/design", post(design_switch))
        .route("/api/threads/{id}/design/review", post(design_review))
        .route("/api/threads/{id}/quota-wait", post(quota_wait))
        .route("/api/threads/{id}/attempts", get(attempts))
        .route("/api/threads/{id}/attempts/pick", post(pick_attempt))
        .route("/api/files", get(find_files_in))
        .route("/api/diff", get(uncommitted))
        .route("/api/threads/{id}/context", get(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
                hub.context_detail(&id).await.map(Json).map_err(ApiError::from)
            }))
        .route("/api/threads/{id}/reach", get(thread_reach).put(set_thread_reach))
        .route("/api/tidy", post(tidy))
        .route("/api/bag", get(|State(hub): State<Arc<Hub>>| async move { crate::bag::list(&hub).await.map(Json).map_err(ApiError::from) })
            .post(|State(hub): State<Arc<Hub>>, Json(add): Json<crate::model::BagAdd>| async move {
                crate::bag::add(&hub, add).await.map(Json).map_err(ApiError::from)
            })
            .delete(|State(hub): State<Arc<Hub>>| async move {
                crate::bag::clear(&hub).await.map(|n| Json(serde_json::json!({ "removed": n }))).map_err(ApiError::from)
            }))
        .route("/api/bag/picture", post(|Json(body): Json<serde_json::Value>| async move {
                crate::bag::picture(body["url"].as_str().unwrap_or_default()).await.map(Json).map_err(ApiError::from)
            }))
        .route("/api/bag/{id}", get(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
                match crate::bag::get(&hub, &id).await {
                    Ok(Some(item)) => Json(item).into_response(),
                    Ok(None) => (StatusCode::NOT_FOUND, "no such item").into_response(),
                    Err(err) => ApiError::from(err).into_response(),
                }
            })
            .delete(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
                crate::bag::remove(&hub, &id).await.map(Json).map_err(ApiError::from)
            }))
        .route("/api/history", get(history))
        .route("/api/threads/{id}/cancel", post(cancel))
        .route("/api/threads/{id}/warm", post(warm))
        .route("/api/threads/{id}/permissions/{request_id}", post(answer_permission))
        .route("/api/threads/{id}/queue/{seq}", axum::routing::patch(edit_queued).delete(remove_queued))
        .route("/api/threads/{id}/aside", get(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move { Json(hub.last_aside(&id)) }))
        .route("/api/threads/{id}/queue/{seq}/steer", post(|State(hub): State<Arc<Hub>>, Path((id, seq)): Path<(String, i64)>| async move {
            hub.steer(&id, seq).await?;
            ApiResult::Ok(StatusCode::NO_CONTENT)
        }))
        .route("/api/threads/{id}/handoff/draft", post(handoff_draft))
        .route("/api/threads/{id}/handoff/tool", post(handoff_tool))
        .route("/api/threads/{id}/handoff/now", post(handoff_now))
        .route("/api/threads/{id}/shutdown", post(shutdown))
        .route("/api/threads/{id}/takeover", post(take_over))
        .route("/api/threads/{id}/handback", post(hand_back))
        .route("/api/stream", get(stream))
}

async fn list_threads(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.all_threads().await?))
}

pub(super) async fn create_thread(
    State(hub): State<Arc<Hub>>,
    Json(new): Json<NewThread>,
) -> ApiResult<impl IntoResponse> {
    // Threads started here, not side jobs or routines, are checked against the project's stage
    // now. A group of attempts is one task, checked once on the thread that comes back.
    let ask = new.message.clone();
    let thread = hub.create_thread(new).await?;
    if let Some(ask) = ask {
        tokio::spawn(hub.clone().focus_check(thread.id.clone(), thread.cwd.clone(), ask));
    }
    Ok((StatusCode::CREATED, Json(thread)))
}

async fn get_thread(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    match hub.store.thread(&id).await? {
        Some(thread) => Ok(Json(thread)),
        None => Err(ApiError::new(StatusCode::NOT_FOUND, format!("no thread {id}"))),
    }
}

async fn patch_thread(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(patch): Json<ThreadPatch>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.patch_thread(&id, patch).await?))
}

/// A notice from outside Hub (`notice.rs`): a widget, a script or a server.
async fn post_notice(State(hub): State<Arc<Hub>>, Json(new): Json<crate::model::NewNotice>) -> ApiResult<impl IntoResponse> {
    use crate::notice::Refused;
    match hub.notice(new).await {
        Ok(notice) => Ok(Json(notice)),
        Err(Refused::Invalid(why)) => Err(ApiError::new(StatusCode::BAD_REQUEST, why)),
        Err(Refused::TooMany) => Err(ApiError::new(StatusCode::TOO_MANY_REQUESTS, "that source sent too many a minute ago; try again in a minute".into())),
    }
}

/// The inbox's own changes to a thread, which a phone may make (`phone::may`): settle, snooze, pin,
/// archive, mark seen. The rest of a patch (title, engine, auto settle) is left out, so the
/// phone's narrow token cannot reach them through it.
async fn tidy_thread(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(patch): Json<ThreadPatch>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.patch_thread(&id, tidy_only(patch)).await?))
}

fn tidy_only(patch: ThreadPatch) -> ThreadPatch {
    ThreadPatch { title: None, engine: None, auto_settle: None, ..patch }
}

async fn delete_thread(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    hub.delete_thread(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// An agent's page, in its sandbox (`pages.rs`).
async fn page(State(hub): State<Arc<Hub>>, Path((id, page)): Path<(String, String)>) -> Response {
    let data_dir = hub.config.data_dir.clone();
    match blocking(move || crate::pages::load(&data_dir, &id, &page)).await {
        Ok(html) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CONTENT_SECURITY_POLICY, crate::pages::CSP),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (header::REFERRER_POLICY, "no-referrer"),
                (header::CACHE_CONTROL, "private, max-age=31536000, immutable"),
            ],
            html,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, [(header::CONTENT_SECURITY_POLICY, "default-src 'none'; sandbox")], "no such page").into_response(),
    }
}

/// The branch of every thread on this machine that is not archived, for the activity list.
async fn branches(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    let threads = hub.store.threads().await?;
    let found = tokio::task::spawn_blocking(move || {
        threads
            .into_iter()
            .filter(|t| !t.archived)
            .filter_map(|t| Some((t.id, crate::git::branch_of(std::path::Path::new(&t.cwd))?)))
            .collect::<std::collections::BTreeMap<_, _>>()
    })
    .await
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(found))
}

async fn thread_git(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let thread = hub
        .store
        .thread(&id)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("no thread {id}")))?;
    let info = tokio::task::spawn_blocking(move || crate::git::info(std::path::Path::new(&thread.cwd)))
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(info))
}

#[derive(Deserialize)]
struct EventsQuery {
    #[serde(default)]
    after: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    5000
}

async fn list_events(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Query(q): Query<EventsQuery>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.store.events(&id, q.after, q.limit.clamp(1, 50_000)).await?))
}

async fn send_message(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(msg): Json<SendMessage>,
) -> ApiResult<impl IntoResponse> {
    hub.send_once(&id, msg.text, msg.attachments, msg.original, msg.client_id, msg.steer).await?;
    Ok(StatusCode::ACCEPTED)
}

/// A reaction from the user: on or off an emoji on the message at `seq`.
async fn react(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(req): Json<crate::reactions::React>,
) -> ApiResult<impl IntoResponse> {
    hub.react(&id, req.seq, &req.emoji, req.on.unwrap_or(true), "you")
        .await
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct EmojiPicture {
    mime: String,
    data: String,
}

async fn put_emoji(State(hub): State<Arc<Hub>>, Path(name): Path<String>, Json(pic): Json<EmojiPicture>) -> ApiResult<impl IntoResponse> {
    let data_dir = hub.config.data_dir.clone();
    blocking(move || crate::reactions::save_custom(&data_dir, &name, &pic.mime, &pic.data))
        .await
        .map_err(|e| ApiError::new(StatusCode::BAD_REQUEST, e.message))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_emoji(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> ApiResult<impl IntoResponse> {
    let data_dir = hub.config.data_dir.clone();
    blocking(move || crate::reactions::delete_custom(&data_dir, &name)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn tidy(
    State(hub): State<Arc<Hub>>,
    Json(req): Json<TidyRequest>,
) -> ApiResult<impl IntoResponse> {
    let text = if req.context == Some(true) {
        hub.autoprompt(&req.text, req.cwd.as_deref(), req.thread_id.as_deref()).await?
    } else {
        hub.tidy(&req.text, req.cwd.as_deref()).await?
    };
    Ok(Json(Tidied { text }))
}

#[derive(Deserialize)]
struct HistoryQuery {
    q: String,
    #[serde(default)]
    tz: i32,
}

async fn history(State(hub): State<Arc<Hub>>, Query(q): Query<HistoryQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.history_search(&q.q, q.tz).await?))
}

async fn save_rule(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(rule): Json<SaveRule>,
) -> ApiResult<impl IntoResponse> {
    hub.save_rule(&id, &rule.text, rule.seq).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn handoff(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(Tidied { text: hub.handoff(&id).await? }))
}

async fn undo_preview(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(t): Json<UndoTarget>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.undo_preview(&id, &t.to).await?))
}

async fn undo(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(t): Json<UndoTarget>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.undo(&id, &t.to).await?))
}

async fn review(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(r): Json<ReviewRequest>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.review(&id, r).await?))
}

/// Design mode on or off (`design.rs`).
async fn design_switch(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(s): Json<crate::model::DesignSwitch>,
) -> ApiResult<impl IntoResponse> {
    hub.switch_design(&id, s.on).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Fresh sessions review the thread's design document (`design.rs`).
async fn design_review(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(ask): Json<crate::model::DesignReviewAsk>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.design_review(&id, ask, "you").await?))
}

async fn attempts(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.attempts(&id).await?))
}

async fn pick_attempt(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.pick_attempt(&id).await?))
}

async fn set_setting(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    narrow: Option<axum::Extension<crate::auth::Narrow>>,
    Json(setting): Json<SetSetting>,
) -> ApiResult<impl IntoResponse> {
    // A phone may switch a thread's mode, but not to one where the agent stops asking.
    let loosening = setting.value.as_str().is_some_and(crate::engines::asks_nothing)
        || (setting.value.as_bool() == Some(true) && crate::engines::asks_nothing(&setting.config_id));
    if narrow.is_some() && loosening {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "a phone cannot switch a thread to a mode that asks nothing; do that on the computer".into()));
    }
    hub.set_setting(&id, setting.config_id, setting.value).await?;
    Ok(StatusCode::ACCEPTED)
}

#[derive(Deserialize)]
struct FilesQuery {
    #[serde(default)]
    q: String,
    /// Folder to search when there is no thread yet.
    #[serde(default)]
    cwd: String,
}

/// Files in a thread's folder matching `q`, for @-mentions in the composer.
async fn find_files(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Query(q): Query<FilesQuery>,
) -> ApiResult<impl IntoResponse> {
    let thread = hub
        .store
        .thread(&id)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("no thread {id}")))?;
    search(thread.cwd, q.q).await
}

#[derive(Deserialize)]
struct ReachQuery {
    targets: Vec<String>,
}

/// What the paths a destructive command names hold, so the user sees it before answering.
async fn reach(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(q): Json<ReachQuery>,
) -> ApiResult<impl IntoResponse> {
    let thread = hub
        .store
        .thread(&id)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("no thread {id}")))?;
    let targets: Vec<String> = q.targets.into_iter().take(20).collect();
    let found = tokio::task::spawn_blocking(move || {
        crate::files::reach(std::path::Path::new(&thread.cwd), &targets)
    })
    .await
    .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(found))
}

async fn find_files_in(Query(q): Query<FilesQuery>) -> ApiResult<impl IntoResponse> {
    search(q.cwd, q.q).await
}

#[derive(Deserialize)]
struct QuotaWaitBody {
    /// "now" carries on now, "cancel" stops waiting, "switch" moves the thread to `engine` (else
    /// the one the wait offers) and carries on there.
    action: String,
    #[serde(default)]
    engine: Option<String>,
}

/// Carries a thread waiting for a plan limit on now, or stops it waiting.
async fn quota_wait(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(body): Json<QuotaWaitBody>,
) -> ApiResult<impl IntoResponse> {
    match body.action.as_str() {
        "now" => hub.carry_on(&id, "you").await?,
        "cancel" => {
            hub.stop_waiting(&id, "you").await?;
        }
        "switch" => hub.continue_elsewhere(&id, body.engine.as_deref(), "you").await?,
        other => return Err(anyhow::anyhow!("unknown action {other:?}").into()),
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn cancel(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    hub.cancel(&id).await?;
    Ok(StatusCode::ACCEPTED)
}

pub(super) async fn warm_new(State(hub): State<Arc<Hub>>, Json(ask): Json<crate::model::WarmAsk>) -> ApiResult<impl IntoResponse> {
    let id = hub.warm_new(ask).await?;
    Ok(Json(crate::model::Warmed { id }))
}

async fn warm(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    hub.warm(&id).await?;
    Ok(StatusCode::ACCEPTED)
}

async fn answer_permission(
    State(hub): State<Arc<Hub>>,
    Path((id, request_id)): Path<(String, String)>,
    Json(answer): Json<PermissionAnswer>,
) -> ApiResult<impl IntoResponse> {
    hub.answer_permission(&id, &request_id, answer.option_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct QueuedEdit {
    text: String,
}

/// A message waiting behind a running turn, changed before the engine takes it.
async fn edit_queued(
    State(hub): State<Arc<Hub>>,
    Path((id, seq)): Path<(String, i64)>,
    Json(edit): Json<QueuedEdit>,
) -> ApiResult<impl IntoResponse> {
    hub.change_queued(&id, seq, Some(edit.text)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_queued(State(hub): State<Arc<Hub>>, Path((id, seq)): Path<(String, i64)>) -> ApiResult<impl IntoResponse> {
    hub.change_queued(&id, seq, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Moves a thread to another machine (`moving.rs`).
async fn move_thread(State(hub): State<Arc<Hub>>, Json(body): Json<crate::model::MoveThread>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::moving::move_thread(&hub, body).await?))
}

/// "Leaving": moves what is working here to a machine that stays on.
async fn leaving(State(hub): State<Arc<Hub>>, Json(body): Json<crate::model::Leaving>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::moving::leaving(&hub, &body.to).await?))
}

async fn export_thread(State(hub): State<Arc<Hub>>, Json(body): Json<crate::moving::ExportAsk>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::moving::export(&hub, body).await?))
}

async fn import_thread(State(hub): State<Arc<Hub>>, Json(body): Json<crate::model::ThreadExport>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::moving::import(&hub, body).await?))
}

async fn stepped_aside(State(hub): State<Arc<Hub>>, Json(body): Json<crate::moving::SteppedAside>) -> ApiResult<impl IntoResponse> {
    crate::moving::step_aside(&hub, body).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct DraftAsk {
    folder: String,
}

async fn handoff_draft(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(ask): Json<DraftAsk>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.draft_handoff(&id, &ask.folder).await?))
}

/// Pressing the agent's handoff card is the approval: the rest happens in the background and shows
/// in the thread as `handed_off`.
async fn handoff_now(State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(ask): Json<DraftAsk>) -> ApiResult<impl IntoResponse> {
    crate::own_tools::found(&hub.ctx(), &ask.folder).await?;
    tokio::spawn(async move { hub.hand_off_now(&id, &ask.folder).await });
    Ok(Json(serde_json::json!({ "started": true })))
}

/// Pressing the button on the brief the user read is the approval.
async fn handoff_tool(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(brief): Json<crate::model::HandoffBrief>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.hand_off(&id, &brief).await?))
}

async fn thread_reach(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::reach::get(&hub.ctx(), &id).await?))
}

async fn set_thread_reach(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(body): Json<crate::model::ThreadReach>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.set_reach(&id, body).await?))
}

async fn shutdown(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(body): Json<Shutdown>,
) -> ApiResult<impl IntoResponse> {
    hub.shutdown(&id, body.everything).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct TakeoverBody {
    /// The `hubd attach` process, so a second one is refused while it lives.
    #[serde(default)]
    pid: Option<u32>,
}

async fn take_over(
    State(hub): State<Arc<Hub>>,
    Path(id): Path<String>,
    Json(body): Json<TakeoverBody>,
) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.take_over(&id, body.pid).await?))
}

async fn hand_back(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.hand_back(&id).await?))
}

#[derive(Deserialize)]
struct DiffQuery {
    cwd: String,
}

/// The folder's uncommitted changes, for attaching to a message. Empty when there are none.
async fn uncommitted(Query(q): Query<DiffQuery>) -> ApiResult<Json<crate::voice::Transcript>> {
    let text = tokio::task::spawn_blocking(move || crate::files::uncommitted(std::path::Path::new(&q.cwd)))
        .await
        .map_err(|e| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .unwrap_or_default();
    Ok(Json(crate::voice::Transcript { text }))
}

#[cfg(test)]
mod tidy_tests {
    use super::*;

    #[test]
    fn a_tidy_keeps_the_inbox_changes_and_drops_the_rest() {
        let patch = ThreadPatch {
            title: Some("renamed".into()),
            engine: Some("codex".into()),
            auto_settle: Some(true),
            settled: Some(true),
            snooze_until: Some(5),
            pinned: Some(true),
            archived: Some(false),
            seen: Some(true),
            ..Default::default()
        };
        let tidy = tidy_only(patch);
        assert!(tidy.title.is_none() && tidy.engine.is_none() && tidy.auto_settle.is_none());
        assert_eq!((tidy.settled, tidy.snooze_until, tidy.pinned, tidy.archived, tidy.seen), (Some(true), Some(5), Some(true), Some(false), Some(true)));
    }
}
