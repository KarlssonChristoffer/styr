//! The bench's read routes as the Labs card calls them: `GET /api/bench/runs/{id}` carries the
//! report's table as data (`run.summary`, also for a run stored before the field) and
//! `GET /api/bench/suites` lists the built-in suites with their defaults and, only from earlier
//! finished runs, an estimate. A run of the scripted mock agent (helpers in `bench_run.rs`) and
//! run files laid down by hand; nothing real is spent.

use std::sync::Arc;

use hub_core::Hub;
use hub_core::model::{BenchRun, BenchSummary, BenchSuiteInfo};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::bench_run::{ask, ended, suite, tidy, warmed};

async fn get(hub: &Arc<Hub>, path: &str) -> Value {
    let token = hub_core::auth::token(&hub.config.data_dir).unwrap();
    let req = axum::http::Request::builder().uri(path).header("authorization", format!("Bearer {token}")).body(axum::body::Body::empty()).unwrap();
    let res = hub_core::api::router(hub.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), 200, "GET {path}");
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn a_run_answers_with_its_summary_and_the_report_says_the_same() {
    let hub = warmed().await;
    let text = suite(60, &[("write", "write it", &["ended = \"end_turn\""]), ("plan", "plan it", &["steps = 3"])]);
    let started = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap();
    let run = ended(&hub, &started.id).await;

    let answer = get(&hub, &format!("/api/bench/runs/{}", run.id)).await;
    let read: BenchRun = serde_json::from_value(answer["run"].clone()).unwrap();
    let summary = read.summary.clone().expect("a run saved now carries its summary");
    assert_eq!(summary.before, None);
    let cases: Vec<(&str, u32, u32, bool)> = summary.cases.iter().map(|c| (c.case.as_str(), c.passed, c.runs, c.flag.is_none())).collect();
    assert_eq!(cases, vec![("write", 1, 1, true), ("plan", 0, 1, true)], "{summary:#?}");
    assert!(summary.cases[0].wall_ms.is_some(), "{summary:#?}");
    let report = answer["report"].as_str().unwrap();
    assert!(report.contains("| write | 1/1 | new |") && report.contains("| plan | 0/1 | new |"), "{report}");

    // The list leaves the rows out but keeps the summary.
    let listed = get(&hub, "/api/bench/runs").await;
    assert_eq!(listed[0]["rows"], json!([]));
    assert_eq!(serde_json::from_value::<BenchSummary>(listed[0]["summary"].clone()).unwrap(), summary);
    tidy(&run);
}

/// The card reads the run and offers Stop only while it is running; a new run may start once it
/// has ended. So the moment the stored run says ended (read in a tight loop, no sleep that could
/// hide a gap), it is no longer going: not in progress, Stop answers no, and a start is not refused.
#[tokio::test]
async fn a_run_that_reads_as_ended_is_no_longer_going() {
    let hub = warmed().await;
    let text = suite(60, &[("write", "write it", &["ended = \"end_turn\""])]);
    let first = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap();
    let run = loop {
        match hub_core::bench::results::load(&hub.ctx(), &first.id) {
            Some(run) if run.state != "running" => break run,
            _ => tokio::task::yield_now().await,
        }
    };
    assert!(hub_core::bench::run::progress(&hub).is_none(), "the file says {} but the run is still going", run.state);
    assert!(!hub_core::bench::run::stop(&hub, &run.id).await, "a run that reads as ended answered a stop");
    let second = hub_core::bench::run::start_text(&hub, &text, &ask()).await;
    let second = second.unwrap_or_else(|e| panic!("a run that reads as ended still held off a new one: {e:#}"));
    tidy(&run);
    tidy(&ended(&hub, &second.id).await);
}

/// A finished smoke run as an older Hub stored it: no `summary` key.
fn stored_run(id: &str, started: i64, state: &str, tokens: i64) -> Value {
    let smoke = hub_core::bench::builtin("smoke").unwrap();
    let rows: Vec<Value> = smoke
        .cases
        .iter()
        .map(|c| json!({ "case": c.id, "repeat": 1, "state": "done", "pass": true, "measured": { "wall_ms": 1000, "tokens": { "input": tokens }, "cost_usd": 0.01 } }))
        .collect();
    json!({
        "id": id, "suite": "smoke", "suite_hash": smoke.hash, "engine": "e", "model_asked": "small", "machine": "pc",
        "hub_version": "1", "started": started, "state": state, "repeats": 1,
        "cases": smoke.cases.iter().map(|c| c.id.clone()).collect::<Vec<_>>(), "rows": rows,
    })
}

#[tokio::test]
async fn suites_list_their_defaults_and_estimate_only_from_earlier_runs() {
    let config = hub_core::Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = Hub::new(hub_core::Store::in_memory().unwrap(), config);
    let smoke = hub_core::bench::builtin("smoke").unwrap();
    let find = |all: Value| -> BenchSuiteInfo {
        let all: Vec<BenchSuiteInfo> = serde_json::from_value(all).unwrap();
        assert_eq!(all.len(), hub_core::bench::builtin_names().count(), "every built-in suite is listed");
        for name in ["smoke", "features"] {
            assert!(all.iter().any(|s| s.name == name), "suite {name} is not listed");
        }
        all.into_iter().find(|s| s.name == "smoke").unwrap()
    };

    let listed = find(get(&hub, "/api/bench/suites").await);
    assert_eq!((listed.hash.as_str(), listed.engine.as_str(), listed.model.as_str()), (smoke.hash.as_str(), smoke.engine.as_str(), smoke.model.as_str()));
    assert_eq!((listed.repeats, listed.cost_cap_usd, listed.cases.len()), (smoke.repeats, smoke.cost_cap_usd, smoke.cases.len()));
    assert!(listed.estimates.is_empty(), "no earlier run, no estimate: {listed:#?}");

    let dir = hub_core::bench::results::runs_dir(&hub.ctx());
    std::fs::create_dir_all(&dir).unwrap();
    for (id, started, state, tokens) in [("1-smoke", 1, "done", 10), ("2-smoke", 2, "done", 30), ("3-smoke", 3, "stopped", 1_000)] {
        std::fs::write(dir.join(format!("{id}.json")), stored_run(id, started, state, tokens).to_string()).unwrap();
    }
    let listed = find(get(&hub, "/api/bench/suites").await);
    let n = smoke.cases.len() as i64;
    let [only] = &listed.estimates[..] else { panic!("{listed:#?}") };
    // Two finished runs (the stopped one is left out): the median is the upper of the two.
    assert_eq!((only.engine.as_str(), only.model.as_str(), only.repeats, only.runs, only.tokens), ("e", "small", 1, 2, Some(30 * n)));
    assert!(only.cost_usd.is_some_and(|c| (c - 0.01 * n as f64).abs() < 1e-6), "{only:#?}");

    // A run stored without the summary gets it on read.
    let answer = get(&hub, "/api/bench/runs/2-smoke").await;
    let read: BenchRun = serde_json::from_value(answer["run"].clone()).unwrap();
    let summary = read.summary.expect("worked out on read");
    assert_eq!(summary.before.as_ref().map(|b| b.id.as_str()), Some("1-smoke"));
    assert!(summary.cases.iter().all(|c| c.passed == 1 && c.flag.is_some()), "{summary:#?}");
    std::fs::remove_dir_all(&hub.config.data_dir).ok();
}
