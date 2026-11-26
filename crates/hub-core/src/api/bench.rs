//! Routes for the bench (`bench/run.rs`, `bench/results.rs`, `bench/catalog.rs`): list the
//! suites, start a run, list the runs, read one with its report and summary, stop the one that is
//! going. Features `bench.runs`, `bench.summary` and `bench.suites` in `compat::FEATURES`.

use super::*;

pub(super) fn routes() -> Router<Arc<Hub>> {
    Router::new()
        .route("/api/bench/suites", get(suites))
        .route("/api/bench/runs", get(runs).post(start))
        .route("/api/bench/runs/{id}", get(run))
        .route("/api/bench/runs/{id}/stop", post(stop))
}

/// The built-in suites with their defaults and estimates from earlier runs.
async fn suites(State(hub): State<Arc<Hub>>) -> Json<Vec<crate::model::BenchSuiteInfo>> {
    Json(crate::bench::catalog::suites(&hub.ctx()))
}

async fn start(State(hub): State<Arc<Hub>>, Json(ask): Json<crate::model::BenchAsk>) -> ApiResult<Json<crate::model::BenchRun>> {
    Ok(Json(crate::bench::run::start(&hub, &ask).await?))
}

/// Every run, newest first, without their repeats' rows.
async fn runs(State(hub): State<Arc<Hub>>) -> Json<Vec<crate::model::BenchRun>> {
    let mut all = crate::bench::results::list(&hub.ctx());
    for run in &mut all {
        run.rows.clear();
    }
    Json(all)
}

/// One run and its report, `{ run, report }`; `run.summary` is the report's table as data.
async fn run(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    let ctx = hub.ctx();
    let run = crate::bench::results::load(&ctx, &id).ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, format!("no bench run {id}")))?;
    let report = crate::bench::results::load_report(&ctx, &id);
    Ok(Json(serde_json::json!({ "run": run, "report": report })))
}

async fn stop(State(hub): State<Arc<Hub>>, Path(id): Path<String>) -> ApiResult<Json<serde_json::Value>> {
    if crate::bench::run::stop(&hub, &id).await {
        Ok(Json(serde_json::json!({ "stopping": true, "note": "it stops once the repeat it is in has ended" })))
    } else {
        Err(ApiError::new(StatusCode::CONFLICT, format!("bench run {id} is not going")))
    }
}
