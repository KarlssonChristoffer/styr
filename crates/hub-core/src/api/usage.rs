//! Routes for the usage area: spend and stats, activity and pulse, the use log, dev log, timings, voice,
//! widgets, own tools, workers and halt, plus the cleanup, away and checkup calls.

use super::*;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
.route("/api/version", get(|State(hub): State<Arc<Hub>>| async move { Json(hub.update_status()) }))
.route("/api/cleanup", get(last_cleanup).post(clean_up))
.route("/api/threads/{id}/timings", get(thread_timings))
.route("/api/timings", get(timings_summary))
.route("/api/package/slips", get(package_slips))
.route("/api/package/summary", get(package_summary))
.route("/api/usage", get(usage))
.route("/api/voice", get(voice_status))
.route("/api/voice/prepare", post(voice_prepare))
.route("/api/voice/transcribe", post(voice_transcribe))
.route("/api/voice/polish", post(voice_polish))
.route("/api/voice/mute", post(voice_mute))
.route("/api/voice/mute/teach", post(voice_mute_teach))
.route("/api/activity", get(activity))
.route("/api/activity/day", get(activity_day).post(summarize_day))
.route("/api/uselog", get(uselog_report).post(uselog_record).delete(uselog_clear))
.route("/api/uselog/off", axum::routing::put(uselog_off))
.route("/api/status-shadow", get(status_shadow).delete(status_shadow_clear))
.route("/api/pulse", get(pulse))
.route("/api/pulse/lines", get(pulse_lines))
.route("/api/stats", get(stats))
.route("/api/usage/spend", get(spend))
.route("/api/devlog", get(devlog))
.route("/api/devlog/project", get(project_devlog))
.route("/api/devlog/day", get(|State(hub): State<Arc<Hub>>, Query(q): Query<DayQuery>| async move {
                crate::devlog::day(&hub.ctx(), &q.date).await.map(Json).map_err(ApiError::from)
            }))
.route("/api/away", get(away))
.route("/api/checkup", get(checkup).post(run_checkup))
.route("/api/widgets", get(|State(hub): State<Arc<Hub>>| async move { crate::widgets::list(&hub.ctx()).await.map(Json).map_err(ApiError::from) })
            .post(|State(hub): State<Arc<Hub>>, Json(ask): Json<crate::model::WidgetAsk>| async move {
                crate::widget_builds::create(&hub, &ask.text).await.map(Json).map_err(ApiError::from)
            }))
.route("/api/widgets/{id}", axum::routing::delete(|State(hub): State<Arc<Hub>>, Path(id): Path<String>| async move {
                crate::widgets::delete(&hub.ctx(), &id).await.map(Json).map_err(ApiError::from)
            }))
.route("/api/widgets/{id}/change", post(|State(hub): State<Arc<Hub>>, Path(id): Path<String>, Json(ask): Json<crate::model::WidgetAsk>| async move {
                crate::widget_builds::ask_change(&hub, &id, &ask.text).await.map(Json).map_err(ApiError::from)
            }))
.route("/api/workers", get(workers))
.route("/api/halt", get(|State(hub): State<Arc<Hub>>| async move { Json(crate::halt::current(&hub)) }))
.route("/api/halt/pause", post(pause_all))
.route("/api/halt/resume", post(resume_all))
.route("/api/tools", get(own_tools))
.route("/api/tools/action", post(own_tool_action))
.route("/api/tools/unapproved", get(own_tools_unapproved))
.route("/api/tools/allow", post(own_tools_allow).delete(own_tools_forget))
.route("/api/tools/facts", get(tool_facts).post(tool_facts_receive))
.route("/api/worker-runs", get(|| async { Json(crate::worker_runs::by_thread()) }))
.route("/api/worker-runs/control", post(worker_control))
}

/// Where the time of each turn of a thread went (`timings.rs`).
async fn thread_timings(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::timings::turns(&hub.store.events(&id, 0, 50_000).await?)))
}

/// How fast the first answers of the latest threads came, by engine and kind of thread.
async fn timings_summary(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::timings::summary(&hub).await?))
}

#[derive(Deserialize)]
struct SlipsQuery {
    thread: Option<String>,
    purpose: Option<String>,
    session: Option<String>,
    before: Option<i64>,
    limit: Option<usize>,
}

/// What was sent to an AI, newest first (`package/ledger.rs`).
async fn package_slips(State(hub): State<Arc<Hub>>, Query(q): Query<SlipsQuery>) -> ApiResult<impl IntoResponse> {
    let filter = crate::package::ledger::Filter { thread: q.thread, purpose: q.purpose, session: q.session, before: q.before, limit: q.limit };
    Ok(Json(crate::package::ledger::list(&hub.store, filter).await?))
}

#[derive(Deserialize)]
struct PackageSummaryQuery {
    days: Option<u32>,
}

/// What was sent to an AI per day and purpose, in the time zone Hub keeps for its day views.
async fn package_summary(State(hub): State<Arc<Hub>>, Query(q): Query<PackageSummaryQuery>) -> ApiResult<impl IntoResponse> {
    let tz: i32 = hub.store.get_value(crate::devlog::TZ).await.ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0);
    Ok(Json(crate::package::ledger::summary(&hub.store, q.days.unwrap_or(7), tz).await?))
}

#[derive(Deserialize)]
struct ActivityQuery {
    #[serde(default = "default_days")]
    days: u32,
    /// Minutes east of UTC.
    #[serde(default)]
    tz: i32,
}

async fn activity(State(hub): State<Arc<Hub>>, Query(q): Query<ActivityQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.activity(q.days, q.tz).await?))
}

#[derive(serde::Deserialize)]
struct UselogQuery {
    #[serde(default = "default_days")]
    days: u32,
}

async fn uselog_report(State(hub): State<Arc<Hub>>, Query(q): Query<UselogQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::uselog::report(&hub, q.days).await?))
}

async fn uselog_record(State(hub): State<Arc<Hub>>, Json(hits): Json<Vec<crate::model::UseHit>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::uselog::record(&hub, hits).await?))
}

async fn uselog_clear(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::uselog::clear(&hub).await?))
}

#[derive(serde::Deserialize)]
struct UselogOff {
    off: bool,
}

async fn uselog_off(State(hub): State<Arc<Hub>>, Json(b): Json<UselogOff>) -> ApiResult<impl IntoResponse> {
    crate::uselog::set_off(&hub, b.off).await?;
    Ok(Json(serde_json::json!({ "off": b.off })))
}

async fn pulse(State(hub): State<Arc<Hub>>, Query(q): Query<ActivityQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.pulse(q.days, q.tz).await?))
}

async fn pulse_lines(State(hub): State<Arc<Hub>>, Query(q): Query<ActivityQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.lines(q.days, q.tz).await?))
}

#[derive(Deserialize)]
struct StatsQuery {
    #[serde(default = "thirty")]
    days: u32,
    #[serde(default)]
    tz: i32,
    /// Only this project's folder.
    project: Option<String>,
}

async fn stats(State(hub): State<Arc<Hub>>, Query(q): Query<StatsQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.stats(q.days, q.tz, q.project.as_deref()).await?))
}

#[derive(Deserialize)]
struct SpendQuery {
    #[serde(default = "seven")]
    days: u32,
    #[serde(default)]
    tz: i32,
    /// Every machine added together (`usage.spend.machines`), not only this one.
    #[serde(default)]
    everywhere: bool,
    /// From the first day anything was counted (`usage.spend.all`); `days` is then ignored.
    #[serde(default)]
    all: bool,
    /// Also hour by hour (`usage.spend.hours`), over the range's last 31 days.
    #[serde(default)]
    hours: bool,
    /// "tokens" counts tokens in place of dollars (`usage.spend.tokens`).
    #[serde(default)]
    unit: Option<String>,
}

/// All AI use per day, in Hub and outside it (`spend.rs`).
async fn spend(State(hub): State<Arc<Hub>>, Query(q): Query<SpendQuery>) -> ApiResult<impl IntoResponse> {
    let days = if q.all { crate::spend::all_days(&hub, q.tz).await } else { q.days };
    let tokens = q.unit.as_deref() == Some("tokens");
    let mut spend = if q.everywhere {
        crate::spend::everywhere(&hub, days, q.tz, q.all, q.hours, tokens).await?
    } else {
        crate::spend::spend_in(&hub, days, q.tz, q.hours, tokens).await?
    };
    // After every machine is in, so their use counts as seen. The estimate is in dollars only.
    if !tokens {
        crate::plan_meter::add_estimate(&hub.store, &mut spend, q.tz).await;
    }
    Ok(Json(spend))
}

#[derive(Deserialize)]
struct DevlogQuery {
    #[serde(default = "thirty")]
    days: u32,
}

async fn devlog(State(hub): State<Arc<Hub>>, Query(q): Query<DevlogQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::devlog::days(&hub.ctx(), q.days as usize).await?))
}

#[derive(Deserialize)]
struct ProjectDevlogQuery {
    path: String,
    #[serde(default = "fourteen")]
    days: u32,
    #[serde(default)]
    tz: i32,
}

async fn project_devlog(State(hub): State<Arc<Hub>>, Query(q): Query<ProjectDevlogQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::devlog::project_days(&hub.ctx(), &q.path, q.days, q.tz).await?))
}

#[derive(Deserialize)]
struct AwayQuery {
    since: i64,
    #[serde(default)]
    tz: i32,
}

async fn away(State(hub): State<Arc<Hub>>, Query(q): Query<AwayQuery>) -> ApiResult<impl IntoResponse> {
    let mut report = hub.away(q.since, q.tz).await?;
    report.health = crate::health::troubled_since(&hub.ctx(), q.since).await?;
    Ok(Json(report))
}

async fn checkup(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::health::last(&hub.ctx()).await?))
}

async fn run_checkup(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::health::check_all(&hub.ctx()).await?))
}

#[derive(Deserialize)]
struct DayQuery {
    date: String,
    #[serde(default)]
    tz: i32,
}

async fn activity_day(State(hub): State<Arc<Hub>>, Query(q): Query<DayQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.activity_day(&q.date, q.tz).await?))
}

async fn summarize_day(State(hub): State<Arc<Hub>>, Query(q): Query<DayQuery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(Tidied { text: hub.summarize_day(&q.date, q.tz).await? }))
}

/// What the last clean-up after agent work removed and left (`null` before the first).
async fn last_cleanup(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.last_cleanup().await))
}

/// Cleans up after agent work now.
async fn clean_up(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.clean_up().await?))
}

#[derive(Deserialize)]
struct ToolsQuery {
    #[serde(default)]
    fresh: bool,
}

async fn own_tools(State(hub): State<Arc<Hub>>, Query(q): Query<ToolsQuery>) -> Json<Vec<crate::model::OwnTool>> {
    Json(crate::own_tools::list(&hub.ctx(), q.fresh).await)
}

/// Which tools here take the files Hub writes about its threads (`facts.rs`).
async fn tool_facts(State(hub): State<Arc<Hub>>) -> Json<Vec<crate::facts::Accepts>> {
    Json(crate::facts::accepts_here(&hub).await)
}

async fn tool_facts_receive(State(hub): State<Arc<Hub>>, Json(delivery): Json<crate::facts::Delivery>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::facts::receive(&hub, &delivery).await?))
}

/// Folders with a `hub.toml` that are not approved, so nothing in them runs yet.
async fn own_tools_unapproved(State(hub): State<Arc<Hub>>) -> Json<Vec<crate::model::OwnToolFound>> {
    Json(crate::own_tools::unapproved(&hub.ctx()).await)
}

#[derive(Deserialize)]
struct AllowAsk {
    folder: String,
}

async fn own_tools_allow(State(hub): State<Arc<Hub>>, Json(ask): Json<AllowAsk>) -> ApiResult<impl IntoResponse> {
    crate::own_tools::allow(&hub.ctx(), &ask.folder).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn own_tools_forget(State(hub): State<Arc<Hub>>, Json(ask): Json<AllowAsk>) -> ApiResult<impl IntoResponse> {
    crate::own_tools::forget(&hub.ctx(), &ask.folder).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn own_tool_action(State(hub): State<Arc<Hub>>, Json(ask): Json<crate::model::RunOwnToolAction>) -> ApiResult<impl IntoResponse> {
    // The app asks twice before a dangerous one, so the person has already said yes.
    Ok(Json(crate::own_tools::run(&hub.ctx(), &ask, true).await?))
}

async fn worker_control(State(hub): State<Arc<Hub>>, Json(ask): Json<crate::model::RunWorkerControl>) -> ApiResult<impl IntoResponse> {
    // The app asks twice before a dangerous one and says when pressing authorizes something.
    Ok(Json(crate::worker_runs::control(&hub, &ask).await?))
}

async fn workers(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    // Containers first, so the `docker ps` that lists them is not listed as one of Hub's helpers.
    let containers = crate::runtime::all_containers().await;
    let engines = hub.engines();
    let mut workers = blocking(move || Ok(crate::workers::snapshot(&engines))).await?;
    workers.containers = containers;
    Ok(Json(workers))
}

/// The panic button. An empty body means everywhere.
async fn pause_all(State(hub): State<Arc<Hub>>, body: Option<Json<crate::model::HaltRequest>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::halt::pause(&hub, body.map(|b| b.0).unwrap_or_default()).await?))
}

async fn resume_all(State(hub): State<Arc<Hub>>, body: Option<Json<crate::model::HaltRequest>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(crate::halt::resume(&hub, body.map(|b| b.0).unwrap_or_default()).await?))
}

async fn voice_status(State(hub): State<Arc<Hub>>) -> Json<crate::voice::VoiceStatus> {
    Json(hub.voice.status())
}

/// Called as a recording starts. Starts the first-time download in the background, or loads the
/// model into whisper-server so the first words are not held up by a cold start; then the
/// graphics card's build and bigger model when there is a card for them. Poll `/api/voice` for
/// progress.
async fn voice_prepare(State(hub): State<Arc<Hub>>) -> StatusCode {
    tokio::spawn(async move {
        if let Err(e) = hub.voice.warm().await {
            tracing::warn!("voice setup failed: {e:#}");
            return;
        }
        if let Err(e) = hub.voice.upgrade().await {
            tracing::warn!("voice on the graphics card failed: {e:#}");
        }
    });
    StatusCode::ACCEPTED
}

#[derive(Deserialize)]
struct MuteQuery {
    on: bool,
}

/// Holds the voice chat's mute key while dictating (`on=true`, repeated to keep it), or lets go.
async fn voice_mute(State(hub): State<Arc<Hub>>, Query(q): Query<MuteQuery>) -> ApiResult<StatusCode> {
    hub.voice.mute(q.on)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct TeachQuery {
    key: String,
}

/// Presses `key` once after three seconds, time to switch to the chat app's keybind recorder.
async fn voice_mute_teach(Query(q): Query<TeachQuery>) -> ApiResult<StatusCode> {
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    tokio::task::spawn_blocking(move || crate::mute::Mute::tap(&q.key)).await.map_err(anyhow::Error::from)??;
    Ok(StatusCode::NO_CONTENT)
}

/// Body: a 16 kHz mono WAV recording. Query: `prompt`, the text just before it, and `final`.
async fn voice_transcribe(
    State(hub): State<Arc<Hub>>,
    Query(hints): Query<crate::voice::Hints>,
    body: axum::body::Bytes,
) -> ApiResult<Json<crate::voice::Transcript>> {
    let text = hub.voice.transcribe(&body, &hints).await?;
    Ok(Json(crate::voice::Transcript { text }))
}

/// Body: `text` just dictated and the thread's `cwd`. Tidies it with the small model when
/// Settings > Dictation asks for it (`voice.polish`).
async fn voice_polish(State(hub): State<Arc<Hub>>, Json(req): Json<TidyRequest>) -> ApiResult<Json<Tidied>> {
    Ok(Json(Tidied { text: hub.polish_dictation(&req.text, req.cwd.as_deref()).await? }))
}

async fn usage(
    State(hub): State<Arc<Hub>>,
    Query(q): Query<UsageQuery>,
) -> ApiResult<Json<crate::usage::UsageReport>> {
    if q.force {
        hub.usage.expire();
    }
    Ok(Json(hub.usage_report(q.refresh || q.force).await?))
}

async fn status_shadow(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    Ok(Json(hub.shadow.report(&hub.store).await?))
}

async fn status_shadow_clear(State(hub): State<Arc<Hub>>) -> ApiResult<impl IntoResponse> {
    hub.shadow.clear(&hub.store).await?;
    Ok(Json(hub.shadow.report(&hub.store).await?))
}
