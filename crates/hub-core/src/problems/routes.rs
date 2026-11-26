//! Problems over HTTP: the `/api/problems/...` router and its handlers in the router's order, the
//! request bodies they take, and the intake a program reports through without a Hub token (Sentry's
//! `/api/<n>/envelope/` and `/store/`, OTLP's `/api/<n>/otlp/v1/...`; see [`is_ingest`]). The
//! handlers only translate: what they do lives in `mod.rs` and the other files of `problems/`.

use std::sync::Arc;

use anyhow::Result;
use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{self, delete, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::{
    IngestError, Incoming, NewSource, ProblemFilter, ProblemPatch, Report, View, add_source, analysis, change, decode, detail,
    dossier, fix_cards, fix_here, fixnow, ingest, list, metrics, note_activity, otlp, quiet, record, remove_source, report, rules,
    set_thread, shown_key, source_for, sources, start_thread, tools, vakten, watch,
};
use crate::hub::Hub;
use crate::locked::ResultExt;

pub fn router() -> Router<Arc<Hub>> {
    Router::new()
        .merge(analysis::router())
        .route("/api/problems", routing::get(list_route))
        .route("/api/problems/report", post(report_route))
        .route("/api/problems/sources", routing::get(sources_route).post(add_source_route))
        .route("/api/problems/sources/{id}", delete(remove_source_route))
        .route("/api/problems/metric", post(metric_route))
        .route("/api/problems/metrics", routing::get(metrics_route))
        .route("/api/problems/vakten", routing::get(vakten_route).put(vakten_prefs_route))
        .route("/api/problems/vakten/look", post(vakten_look_route))
        .route("/api/problems/guard/queue", routing::get(guard_queue_route))
        .route("/api/problems/guard/answer", post(guard_answer_route))
        .route("/api/problems/guard/{id}/change", post(guard_change_route))
        .route("/api/problems/guard/{id}/fix", post(guard_fix_route))
        .route("/api/problems/guard/{id}/tell", post(guard_tell_route))
        .route("/api/problems/guard/{id}/linked", post(guard_linked_route))
        .route("/api/problems/fix_here", post(fix_here_route))
        .route("/api/problems/rules", routing::get(rules_route).put(rules_put_route))
        .route("/api/problems/rules/add", post(rules_add_route))
        .route("/api/problems/views", routing::get(views_route).put(views_put_route))
        .route("/api/problems/monitors", routing::get(monitors_route))
        .route("/api/problems/tools", routing::get(tools_route))
        .route("/api/problems/tools/review", routing::get(tools_review_route))
        .route("/api/problems/fix", post(fix_route))
        .route("/api/problems/fixes", routing::get(fixes_route))
        .route("/api/problems/{id}", routing::get(detail_route).patch(patch_route))
        .route("/api/problems/{id}/thread", post(thread_route))
        .route("/api/problems/{id}/dossier", routing::get(dossier_route))
        .route("/api/{id}/otlp/v1/logs", post(otlp_logs_route))
        .route("/api/{id}/otlp/v1/traces", post(otlp_traces_route))
        .route("/api/{id}/envelope/", post(envelope_route))
        .route("/api/{id}/envelope", post(envelope_route))
        .route("/api/{id}/store/", post(store_route))
        .route("/api/{id}/store", post(store_route))
}

/// Paths a Sentry SDK posts to. They carry no Hub token; the source's key lets them in.
pub fn is_ingest(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/") else { return false };
    let parts: Vec<&str> = rest.trim_end_matches('/').split('/').collect();
    let numeric = |id: &str| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
    match parts.as_slice() {
        [id, "envelope" | "store"] => numeric(id),
        [id, "otlp", "v1", "logs" | "traces"] => numeric(id),
        _ => false,
    }
}

fn failed(status: StatusCode, err: impl std::fmt::Display) -> Response {
    (status, Json(json!({ "error": err.to_string() }))).into_response()
}

pub(super) fn answer<T: Serialize>(result: Result<T>) -> Response {
    match result {
        Ok(v) => Json(v).into_response(),
        Err(e) => failed(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

async fn list_route(State(hub): State<Arc<Hub>>, Query(filter): Query<ProblemFilter>) -> Response {
    answer(list(&hub, filter).await)
}

async fn report_route(State(hub): State<Arc<Hub>>, Json(report): Json<Report>) -> Response {
    answer(record(&hub, Incoming::from_report(report, "app")).await)
}

async fn sources_route(State(hub): State<Arc<Hub>>) -> Response {
    Json(sources(&hub).await).into_response()
}

async fn add_source_route(State(hub): State<Arc<Hub>>, Json(new): Json<NewSource>) -> Response {
    answer(add_source(&hub, new).await)
}

async fn remove_source_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<u32>) -> Response {
    answer(remove_source(&hub, id).await)
}

/// A timing from the app or a script: `{ name, value, release?, project? }`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct MetricReport {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    #[ts(optional)]
    pub release: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub project: Option<String>,
}

async fn metric_route(Json(m): Json<MetricReport>) -> Response {
    metrics::sample(&m.name, m.value, m.release.as_deref(), m.project.as_deref().unwrap_or("Hub"));
    Json(json!({})).into_response()
}

async fn metrics_route(State(hub): State<Arc<Hub>>) -> Response {
    answer(metrics::summaries(&hub).await)
}

async fn vakten_route(State(hub): State<Arc<Hub>>) -> Response {
    answer(vakten::status(&hub).await)
}

async fn vakten_prefs_route(State(hub): State<Arc<Hub>>, Json(prefs): Json<vakten::VaktenPrefs>) -> Response {
    match vakten::set_prefs(&hub, prefs).await {
        Ok(_) => answer(vakten::status(&hub).await),
        Err(e) => failed(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

/// "Look now": Vakten takes what is new without waiting for the minute to pass.
async fn vakten_look_route(State(hub): State<Arc<Hub>>) -> Response {
    watch::sweep(&hub).await.warn_on_err("sweep the watch");
    match vakten::look(&hub).await {
        Ok(_) => answer(vakten::status(&hub).await),
        Err(e) => failed(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

/// Another machine's Vakten asking for this one's queue (see [`vakten::guard_queue`]).
#[derive(Debug, Deserialize)]
struct GuardAsk {
    by: String,
    #[serde(default)]
    since: i64,
    #[serde(default)]
    level: i64,
    #[serde(default)]
    limit: Option<usize>,
    /// The asking Guard takes its fix threads' reports too (`report.rs`).
    #[serde(default)]
    reports: bool,
}

async fn guard_queue_route(State(hub): State<Arc<Hub>>, Query(ask): Query<GuardAsk>) -> Response {
    answer(vakten::guard_queue(&hub, &ask.by, ask.since, ask.level, ask.limit.unwrap_or(15).min(50), ask.reports).await)
}

/// Guard's answer to one of its fix threads running here (`answer_worker`).
#[derive(Debug, Deserialize)]
struct GuardAnswer {
    thread: String,
    text: String,
}

async fn guard_answer_route(State(hub): State<Arc<Hub>>, Json(ask): Json<GuardAnswer>) -> Response {
    answer(report::answer(&hub, &ask.thread, &ask.text, serde_json::json!({ "agent": "Guard" })).await.map(|_| serde_json::json!({ "sent": true })))
}

/// A change another machine's Vakten (or a thread there) makes to a problem here, under its name.
#[derive(Debug, Deserialize)]
struct GuardChange {
    by: String,
    patch: ProblemPatch,
}

async fn guard_change_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<i64>, Json(ask): Json<GuardChange>) -> Response {
    answer(change(&hub, id, ask.patch, &ask.by).await)
}

/// A fix thread started here for another machine's Vakten, held to the same lanes.
#[derive(Debug, Deserialize)]
struct GuardFix {
    by: String,
    #[serde(default)]
    plan: Option<String>,
    /// "small" or "big", as Vakten judged the fix (`big_fix`).
    #[serde(default)]
    size: Option<String>,
    /// Guard starts the problem over after its thread reported back.
    #[serde(default)]
    again: bool,
}

async fn guard_fix_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<i64>, Json(ask): Json<GuardFix>) -> Response {
    if ask.by == "Vakten"
        && let Err(e) = vakten::may_fix(&hub, id, ask.again).await
    {
        return failed(StatusCode::CONFLICT, format!("{e:#}"));
    }
    answer(start_thread(&hub, id, &ask.by, ask.plan.as_deref(), ask.size.as_deref()).await)
}

#[derive(Debug, Deserialize)]
struct GuardTell {
    text: String,
}

async fn guard_tell_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<i64>, Json(ask): Json<GuardTell>) -> Response {
    answer(note_activity(&hub, id, "Vakten", "told", &ask.text).await)
}

#[derive(Debug, Deserialize)]
struct GuardLinked {
    thread: String,
    by: String,
}

/// Another machine started the fix thread for this problem (`fix_here`).
async fn guard_linked_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<i64>, Json(ask): Json<GuardLinked>) -> Response {
    answer(set_thread(&hub, id, &ask.thread, &ask.by).await)
}

#[derive(Debug, Deserialize)]
struct FixHere {
    host: String,
    id: i64,
    #[serde(default)]
    plan: Option<String>,
}

async fn fix_here_route(State(hub): State<Arc<Hub>>, Json(ask): Json<FixHere>) -> Response {
    answer(fix_here(&hub, &ask.host, ask.id, "you", ask.plan.as_deref(), None).await)
}

async fn rules_route(State(hub): State<Arc<Hub>>) -> Response {
    answer(rules::page(&hub).await)
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct GlobalRules {
    pub global: String,
}

async fn rules_put_route(State(hub): State<Arc<Hub>>, Json(body): Json<GlobalRules>) -> Response {
    match rules::set_global(&hub, &body.global).await {
        Ok(()) => answer(rules::page(&hub).await),
        Err(e) => failed(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

async fn rules_add_route(State(hub): State<Arc<Hub>>, Json(rule): Json<rules::Rule>) -> Response {
    match rules::add_global(&hub, rule).await {
        Ok(()) => answer(rules::page(&hub).await),
        Err(e) => failed(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

/// A search the user keeps under a name, as a tab on the Problems page.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SavedView {
    pub name: String,
    pub query: String,
    #[serde(default)]
    pub view: Option<View>,
}

const VIEWS: &str = "problems.views";

pub async fn saved_views(hub: &Hub) -> Vec<SavedView> {
    hub.store.get_value(VIEWS).await.ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

async fn views_route(State(hub): State<Arc<Hub>>) -> Response {
    Json(saved_views(&hub).await).into_response()
}

async fn views_put_route(State(hub): State<Arc<Hub>>, Json(views): Json<Vec<SavedView>>) -> Response {
    let views: Vec<SavedView> = views.into_iter().filter(|v| !v.name.trim().is_empty()).take(30).collect();
    match serde_json::to_string(&views) {
        Ok(text) => match hub.store.set_value(VIEWS, &text).await {
            Ok(()) => Json(views).into_response(),
            Err(e) => failed(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
        },
        Err(e) => failed(StatusCode::BAD_REQUEST, e),
    }
}

async fn monitors_route(State(hub): State<Arc<Hub>>) -> Response {
    Json(quiet::monitors(&hub).await.into_values().collect::<Vec<_>>()).into_response()
}

#[derive(Debug, Deserialize)]
struct ToolsAsk {
    #[serde(default)]
    days: Option<u32>,
}

async fn tools_route(State(hub): State<Arc<Hub>>, Query(ask): Query<ToolsAsk>) -> Response {
    answer(tools::stats(&hub, ask.days.unwrap_or(7)).await)
}

async fn tools_review_route(State(hub): State<Arc<Hub>>) -> Response {
    answer(tools::review(&hub).await)
}

#[derive(Debug, Deserialize)]
struct FixAsk {
    thread: String,
    /// The `error` event's seq.
    about: i64,
}

/// "Fix now" on a failure's card in a thread.
async fn fix_route(State(hub): State<Arc<Hub>>, Json(ask): Json<FixAsk>) -> Response {
    answer(fixnow::fix_now(&hub, &ask.thread, ask.about).await)
}

async fn fixes_route(State(hub): State<Arc<Hub>>) -> Response {
    answer(fix_cards(&hub).await)
}

async fn detail_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<i64>) -> Response {
    match detail(&hub, id, 10).await {
        Ok(Some(d)) => Json(d).into_response(),
        Ok(None) => failed(StatusCode::NOT_FOUND, format!("no problem {id}")),
        Err(e) => failed(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}")),
    }
}

async fn patch_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<i64>, Json(patch): Json<ProblemPatch>) -> Response {
    answer(change(&hub, id, patch, "you").await)
}

#[derive(Debug, Default, Deserialize)]
struct ThreadAsk {
    /// "Let Vakten do it": Vakten's plan goes along, and the thread ends at a pull request.
    #[serde(default)]
    plan: Option<String>,
}

async fn thread_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<i64>, ask: Option<Json<ThreadAsk>>) -> Response {
    let plan = ask.and_then(|Json(a)| a.plan);
    answer(start_thread(&hub, id, "you", plan.as_deref(), None).await)
}

async fn dossier_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<i64>) -> Response {
    answer(dossier::dossier(&hub, id).await)
}

// ---------------------------------------------------------------------------------------------
// Intake: programs reporting with an OTLP exporter or a Sentry SDK, let in by the source's key

async fn otlp_logs_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<u32>, uri: Uri, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    otlp_route(hub, id, uri, headers, body, false).await
}

async fn otlp_traces_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<u32>, uri: Uri, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    otlp_route(hub, id, uri, headers, body, true).await
}

/// The key an OTLP exporter shows: `Authorization: Bearer <key>`, `x-hub-key`, or `?key=`.
fn otlp_key(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).map(str::to_owned);
    header("authorization")
        .and_then(|v| v.strip_prefix("Bearer ").map(str::trim).map(str::to_owned))
        .or_else(|| header("x-hub-key"))
        .or_else(|| query?.split('&').find_map(|p| p.strip_prefix("key=").map(str::to_owned)))
}

async fn otlp_route(hub: Arc<Hub>, id: u32, uri: Uri, headers: HeaderMap, body: axum::body::Bytes, traces: bool) -> Response {
    let key = otlp_key(&headers, uri.query());
    let source = match source_for(&hub, id, key.as_deref()).await {
        Ok(s) => s,
        Err(IngestError::Unknown) => return failed(StatusCode::NOT_FOUND, format!("no source {id}")),
        Err(IngestError::Key) => return failed(StatusCode::UNAUTHORIZED, "the key does not match this source"),
        Err(IngestError::Bad(e)) => return failed(StatusCode::BAD_REQUEST, e),
    };
    if headers.get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|t| t.contains("protobuf")) {
        return failed(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Hub takes OTLP as JSON: set OTEL_EXPORTER_OTLP_PROTOCOL=http/json");
    }
    let encoding = headers.get("content-encoding").and_then(|v| v.to_str().ok());
    let body = match decode(encoding, &body).map_err(|e| format!("{e:#}")).and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|e| format!("not JSON: {e}"))) {
        Ok(v) => v,
        Err(e) => return failed(StatusCode::BAD_REQUEST, e),
    };
    let done = if traces { otlp::traces(&hub, &source, &body).await } else { otlp::logs(&hub, &source, &body).await };
    match done {
        // OTLP's answer: an empty partial success.
        Ok(_) => Json(json!({})).into_response(),
        Err(e) => failed(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

async fn envelope_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<u32>, uri: Uri, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    sentry_route(hub, id, uri, headers, body, true).await
}

async fn store_route(State(hub): State<Arc<Hub>>, UrlPath(id): UrlPath<u32>, uri: Uri, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    sentry_route(hub, id, uri, headers, body, false).await
}

async fn sentry_route(hub: Arc<Hub>, id: u32, uri: Uri, headers: HeaderMap, body: axum::body::Bytes, envelope: bool) -> Response {
    let key = shown_key(&headers, uri.query(), &body);
    let encoding = headers.get("content-encoding").and_then(|v| v.to_str().ok());
    match ingest(&hub, id, key.as_deref(), encoding, &body, envelope).await {
        // Sentry's answer: the SDK only looks at the status.
        Ok(_) => Json(json!({})).into_response(),
        Err(IngestError::Unknown) => failed(StatusCode::NOT_FOUND, format!("no source {id}: add it under Problems > Connect a program")),
        Err(IngestError::Key) => failed(StatusCode::UNAUTHORIZED, "the DSN's key does not match this source"),
        Err(IngestError::Bad(e)) => failed(StatusCode::BAD_REQUEST, e),
    }
}
