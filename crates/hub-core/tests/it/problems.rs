//! Problems: a Sentry SDK's envelope comes in with its DSN key, the same error groups into one
//! problem, a resolved one that happens again opens again, and Hub's own reports land too.

use std::io::Write;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use hub_core::problems::{self, Archive, Lane, Level, NewSource, Prio, ProblemFilter, ProblemPatch, ProblemStatus, Resolve, View};
use hub_core::{Config, Hub, Store};
use tower::ServiceExt;

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}

fn event(line: u32, value: &str) -> String {
    serde_json::json!({
        "level": "error",
        "release": "aftermath@0.9.2",
        "exception": { "values": [{
            "type": "KeyError",
            "value": value,
            "stacktrace": { "frames": [
                { "filename": "aftermath/cli.py", "function": "run", "lineno": 41, "in_app": true },
                { "filename": "aftermath/adapters/claude_code.py", "function": "parse_mark", "lineno": line, "in_app": true,
                  "context_line": "    sid = row[\"session_id\"]" }
            ]}
        }]},
        "breadcrumbs": { "values": [{ "timestamp": 1761925262.0, "level": "warning", "message": "unknown line type summary_v2" }] }
    })
    .to_string()
}

fn envelope(event: &str) -> Vec<u8> {
    let body = format!("{{\"event_id\":\"abc\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{event}\n", event.len());
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(body.as_bytes()).unwrap();
    gz.finish().unwrap()
}

async fn post(hub: &Arc<Hub>, uri: &str, auth: Option<&str>, body: Vec<u8>, gzip: bool) -> StatusCode {
    let mut req = Request::post(uri).header("content-type", "application/x-sentry-envelope");
    if let Some(key) = auth {
        req = req.header("x-sentry-auth", format!("Sentry sentry_version=7, sentry_key={key}, sentry_client=sentry.python/2.18.0"));
    }
    if gzip {
        req = req.header("content-encoding", "gzip");
    }
    hub_core::api::router(hub.clone()).oneshot(req.body(Body::from(body)).unwrap()).await.unwrap().status()
}

#[tokio::test]
async fn a_sentry_sdk_reports_and_the_same_error_groups() {
    let hub = hub();
    let source = problems::add_source(&hub, NewSource { name: "Aftermath".into(), folder: None, quiet_after_hours: None }).await.unwrap();
    assert!(source.dsn.starts_with(&format!("http://{}@127.0.0.1:", source.key)));
    assert!(source.dsn.ends_with(&format!("/{}", source.id)));
    // The same name gives the same source back.
    assert_eq!(problems::add_source(&hub, NewSource { name: "aftermath".into(), folder: None, quiet_after_hours: None }).await.unwrap().id, source.id);

    let uri = format!("/api/{}/envelope/", source.id);
    assert_eq!(post(&hub, &uri, Some("wrong"), envelope(&event(88, "'x'")), true).await, StatusCode::UNAUTHORIZED);
    assert_eq!(post(&hub, "/api/99/envelope/", Some(&source.key), envelope(&event(88, "'x'")), true).await, StatusCode::NOT_FOUND);
    assert_eq!(post(&hub, &uri, Some(&source.key), envelope(&event(88, "'session_id'")), true).await, StatusCode::OK);
    // Another line and another key in the message: the same problem.
    assert_eq!(post(&hub, &uri, Some(&source.key), envelope(&event(91, "'model'")), true).await, StatusCode::OK);

    let all = problems::list(&hub, ProblemFilter::default()).await.unwrap();
    assert_eq!(all.len(), 1);
    let p = &all[0];
    assert_eq!((p.project.as_str(), p.source.as_str(), p.level, p.count), ("Aftermath", "sentry", Level::Error, 2));
    assert_eq!(p.culprit.as_deref(), Some("aftermath/adapters/claude_code.py in parse_mark, line 91"));
    assert_eq!(p.days.len(), 7);
    let detail = problems::detail(&hub, p.id, 10).await.unwrap().unwrap();
    assert_eq!(detail.hits.len(), 2);
    assert_eq!(detail.hits[0].crumbs[0].message, "unknown line type summary_v2");
    assert_eq!(detail.hits[0].release.as_deref(), Some("aftermath@0.9.2"));

    // Resolved, then it happens again: open again, marked as come back.
    problems::set_status(&hub, p.id, ProblemStatus::Resolved).await.unwrap();
    assert!(problems::list(&hub, ProblemFilter::default()).await.unwrap().is_empty());
    assert_eq!(post(&hub, &uri, Some(&source.key), envelope(&event(91, "'model'")), true).await, StatusCode::OK);
    let back = problems::get(&hub, p.id).await.unwrap().unwrap();
    assert_eq!((back.status, back.regressed, back.count), (ProblemStatus::Open, true, 3));

    // The key may also come in the query, and the body need not be packed.
    let plain = format!("{{}}\n{{\"type\":\"event\"}}\n{}\n", r#"{"level":"warning","message":"disk almost full"}"#).into_bytes();
    assert_eq!(post(&hub, &format!("{uri}?sentry_key={}", source.key), None, plain, false).await, StatusCode::OK);
    let warnings = problems::list(&hub, ProblemFilter { query: Some("disk".into()), ..Default::default() }).await.unwrap();
    assert_eq!(warnings[0].level, Level::Warning);
}

#[tokio::test]
async fn hubs_own_reports_and_a_brief_to_work_from() {
    let hub = hub();
    let report = serde_json::json!({
        "source": "app",
        "level": "error",
        "message": "Cannot read properties of undefined (reading 'model')",
        "kind": "TypeError",
        "trace": "TypeError: Cannot read properties of undefined\n    at Composer.svelte:120:14",
    });
    let req = Request::post("/api/problems/report")
        .header("origin", "tauri://localhost")
        .header("content-type", "application/json")
        .body(Body::from(report.to_string()))
        .unwrap();
    let res = hub_core::api::router(hub.clone()).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    // Without a token or the app's origin, nobody reports.
    let req = Request::post("/api/problems/report").header("content-type", "application/json").body(Body::from(report.to_string())).unwrap();
    assert_eq!(hub_core::api::router(hub.clone()).oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);

    let p = problems::list(&hub, ProblemFilter::default()).await.unwrap().remove(0);
    assert_eq!((p.project.as_str(), p.source.as_str()), ("Hub", "app"));
    assert_eq!(p.title, "TypeError: Cannot read properties of undefined (reading 'model')");
    // With no folder known for "Hub" here, starting a thread hands back the brief instead.
    let started = problems::start_thread(&hub, p.id, "you", None, None).await.unwrap();
    assert!(started.thread.is_none());
    assert!(started.prompt.contains("Composer.svelte:120"));
    assert!(started.prompt.contains(&format!("`problem` (id {})", p.id)));
    // Archived problems leave the open list but keep counting.
    problems::set_status(&hub, p.id, ProblemStatus::Archived).await.unwrap();
    assert!(problems::list(&hub, ProblemFilter::default()).await.unwrap().is_empty());
}

fn report(message: &str, release: &str) -> problems::Incoming {
    problems::Incoming::from_report(
        serde_json::from_value(serde_json::json!({ "project": "Knegarn", "level": "error", "message": message, "release": release })).unwrap(),
        "app",
    )
}

fn view(v: View) -> ProblemFilter {
    ProblemFilter { view: Some(v), ..Default::default() }
}

#[tokio::test]
async fn stamps_lanes_archives_releases_and_merges() {
    let hub = hub();
    let a = problems::record(&hub, report("could not reach tincan over ssh", "1.0")).await.unwrap();
    // Unsorted errors need the user while Vakten is off.
    assert!(a.needs_you && a.new);
    assert_eq!(problems::list(&hub, view(View::NeedsYou)).await.unwrap().len(), 1);

    // Stamped into the quiet lane at P3, it no longer needs the user; the activity says who did it.
    let patch = ProblemPatch { prio: Some(Prio::P3), lane: Some(Lane::Quiet), category: Some("Environment".into()), why: Some("flaky VPN".into()), ..Default::default() };
    let a = problems::change(&hub, a.id, patch, "Vakten").await.unwrap();
    assert_eq!((a.prio, a.lane, a.category.as_deref(), a.stamped_by.as_deref()), (Some(Prio::P3), Some(Lane::Quiet), Some("environment"), Some("Vakten")));
    assert!(!a.needs_you);
    let patch = ProblemPatch { lane: Some(Lane::Ask), ..Default::default() };
    problems::change(&hub, a.id, patch, "you").await.unwrap();
    let d = problems::detail(&hub, a.id, 5).await.unwrap().unwrap();
    assert!(d.activity.iter().any(|x| x.by == "Vakten" && x.text.contains("environment") && x.text.contains("flaky VPN")));
    assert!(d.activity.iter().any(|x| x.by == "you" && x.text.starts_with("Changed Vakten's stamps")));
    assert!(d.problem.needs_you, "ask first needs the user");

    // The search language.
    let found = |q: &str| {
        let hub = hub.clone();
        let q = q.to_owned();
        async move { problems::list(&hub, ProblemFilter { query: Some(q), ..Default::default() }).await.unwrap().len() }
    };
    assert_eq!(found("lane:ask stamp:environment prio:p3 \"over ssh\"").await, 1);
    assert_eq!(found("release:1.0 project:Knegarn").await, 1);
    assert_eq!(found("release:2.0").await, 0);
    assert_eq!(found("-project:Knegarn").await, 0);

    // Archived until it has happened 3 times: back on the third.
    let b = problems::record(&hub, report("disk almost full", "1.0")).await.unwrap();
    problems::change(&hub, b.id, ProblemPatch { archive: Some(Archive::Count { count: 3 }), ..Default::default() }, "you").await.unwrap();
    assert_eq!(problems::record(&hub, report("disk almost full", "1.0")).await.unwrap().status, ProblemStatus::Archived);
    assert_eq!(problems::record(&hub, report("disk almost full", "1.0")).await.unwrap().status, ProblemStatus::Open);

    // Resolved in the next release: the old release does not bring it back, a new one does.
    problems::change(&hub, b.id, ProblemPatch { resolve: Some(Resolve::NextRelease), ..Default::default() }, "you").await.unwrap();
    assert_eq!(problems::record(&hub, report("disk almost full", "1.0")).await.unwrap().status, ProblemStatus::Resolved);
    assert_eq!(problems::list(&hub, view(View::Fixed)).await.unwrap().len(), 1);
    let back = problems::record(&hub, report("disk almost full", "1.1")).await.unwrap();
    assert_eq!((back.status, back.regressed), (ProblemStatus::Open, true));
    assert_eq!(problems::list(&hub, view(View::Back)).await.unwrap().len(), 1);

    // Merged: its hits count in the other, and so do later ones.
    let a_count = problems::get(&hub, a.id).await.unwrap().unwrap().count;
    problems::change(&hub, b.id, ProblemPatch { merge_into: Some(a.id), why: Some("same root".into()), ..Default::default() }, "you").await.unwrap();
    problems::record(&hub, report("disk almost full", "1.1")).await.unwrap();
    let a2 = problems::get(&hub, a.id).await.unwrap().unwrap();
    assert_eq!(a2.count, a_count + 6);
    assert!(problems::list(&hub, view(View::All)).await.unwrap().iter().all(|p| p.id != b.id));
    // And split off again: new hits count on their own.
    problems::change(&hub, b.id, ProblemPatch { unmerge: Some(true), ..Default::default() }, "you").await.unwrap();
    problems::record(&hub, report("disk almost full", "1.1")).await.unwrap();
    assert_eq!(problems::get(&hub, a.id).await.unwrap().unwrap().count, a_count + 6);
}

#[tokio::test]
async fn rules_group_stamp_and_archive_what_comes_in() {
    let hub = hub();
    problems::rules::set_global(
        &hub,
        r#"
        [[rule]]
        name = "ssh"
        project = "Knegarn"
        match = "could not reach * over ssh"
        group = "ssh"
        prio = "P3"
        lane = "quiet"

        [[rule]]
        match = "disk almost full"
        archive = "forever"
        "#,
    )
    .await
    .unwrap();
    let a = problems::record(&hub, report("could not reach tincan over ssh", "1.0")).await.unwrap();
    let b = problems::record(&hub, report("could not reach clanker over ssh", "1.0")).await.unwrap();
    assert_eq!(a.id, b.id, "grouped by the rule");
    assert_eq!((b.prio, b.lane, b.stamped_by.as_deref()), (Some(Prio::P3), Some(Lane::Quiet), Some("rule")));
    let c = problems::record(&hub, report("disk almost full", "1.0")).await.unwrap();
    assert_eq!(c.status, ProblemStatus::Archived);
    assert!(problems::rules::set_global(&hub, "[[rule]]\narchive = 3").await.is_err());
}

#[tokio::test]
async fn check_ins_and_open_telemetry_come_in() {
    let hub = hub();
    let source = problems::add_source(&hub, NewSource { name: "Forman".into(), folder: None, quiet_after_hours: None }).await.unwrap();
    // A failed cron job, as a Sentry SDK reports it.
    let check_in = r#"{"check_in_id":"c1","monitor_slug":"nightly-backup","status":"error","monitor_config":{"schedule":{"type":"crontab","value":"0 3 * * *"}}}"#;
    let body = format!("{{}}\n{{\"type\":\"check_in\"}}\n{check_in}\n").into_bytes();
    assert_eq!(post(&hub, &format!("/api/{}/envelope/", source.id), Some(&source.key), body, false).await, StatusCode::OK);
    let monitors = problems::quiet::monitors(&hub).await;
    assert_eq!(monitors.values().next().unwrap().every_ms, Some(86_400_000));
    let failed = problems::list(&hub, view(View::Quiet)).await.unwrap();
    assert_eq!(failed[0].title, "Cron job nightly-backup failed");

    // OpenTelemetry logs as JSON, with the key as a bearer token.
    let logs = serde_json::json!({ "resourceLogs": [{
        "resource": { "attributes": [{ "key": "service.version", "value": { "stringValue": "2.3" } }] },
        "scopeLogs": [{ "scope": { "name": "forman.worker" }, "logRecords": [
            { "timeUnixNano": "1761925262500000000", "severityNumber": 17, "body": { "stringValue": "job 41 crashed" },
              "attributes": [{ "key": "exception.type", "value": { "stringValue": "ValueError" } }] },
            { "severityNumber": 9, "body": { "stringValue": "all good" } }
        ]}]
    }]});
    let req = Request::post(format!("/api/{}/otlp/v1/logs", source.id))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", source.key))
        .body(Body::from(logs.to_string()))
        .unwrap();
    assert_eq!(hub_core::api::router(hub.clone()).oneshot(req).await.unwrap().status(), StatusCode::OK);
    let otlp = problems::list(&hub, ProblemFilter { source: Some("otlp".into()), ..Default::default() }).await.unwrap();
    assert_eq!(otlp.len(), 1);
    assert_eq!((otlp[0].title.as_str(), otlp[0].project.as_str()), ("ValueError: job 41 crashed", "Forman"));
    let detail = problems::detail(&hub, otlp[0].id, 1).await.unwrap().unwrap();
    assert_eq!(detail.hits[0].release.as_deref(), Some("2.3"));
}

#[tokio::test]
async fn hub_sorts_first_and_vakten_gets_the_rest_with_a_contract() {
    let hub = hub();
    problems::vakten::set_prefs(&hub, problems::vakten::VaktenPrefs { on: true, ..Default::default() }).await.unwrap();
    let thread_failed = |message: &str| {
        problems::Incoming::from_report(
            serde_json::from_value(serde_json::json!({ "project": "Hub", "level": "error", "message": message, "kind": "claude failed" })).unwrap(),
            "thread",
        )
    };
    // Sign-in lost: only the user can fix it, so Hub's first pass decides it all.
    let signin = problems::record(&hub, thread_failed("Authentication required")).await.unwrap();
    let signin = problems::get(&hub, signin.id).await.unwrap().unwrap();
    assert_eq!((signin.prio, signin.lane, signin.category.as_deref()), (Some(Prio::P1), Some(Lane::Ask), Some("config")));
    // An engine's internal error gets Hub's guess but stays in Vakten's queue, and so does a plain bug.
    let internal = problems::record(&hub, thread_failed("Internal error")).await.unwrap();
    let internal = problems::get(&hub, internal.id).await.unwrap().unwrap();
    assert_eq!((internal.prio, internal.lane, internal.stamped_by.as_deref()), (Some(Prio::P2), None, Some("rule")));
    problems::record(&hub, report("index out of range in parse_rows", "1.0")).await.unwrap();
    assert_eq!(problems::vakten::queue(&hub).await.unwrap(), 2);
    // Once Vakten stamps a lane, it leaves the queue.
    let patch = ProblemPatch { lane: Some(Lane::Tell), why: Some("engine side".into()), ..Default::default() };
    problems::change(&hub, internal.id, patch, "Vakten").await.unwrap();
    assert_eq!(problems::vakten::queue(&hub).await.unwrap(), 1);
    assert_eq!(problems::vakten::status(&hub).await.unwrap().unstamped, 1);

    // The contract in the note goes along to the fix thread.
    let note = problems::Note {
        repro: "Open an empty sheet.".into(),
        cause: "parse_rows reads row 0 of an empty list.".into(),
        done: "An empty sheet opens; a test covers it.".into(),
        ..Default::default()
    };
    problems::change(&hub, internal.id, ProblemPatch { note: Some(note), ..Default::default() }, "Vakten").await.unwrap();
    let started = problems::start_thread(&hub, internal.id, "Vakten", Some("Guard the list."), None).await.unwrap();
    for part in ["## Contract", "**Repro:**", "Open an empty sheet.", "**Done when:**", "a test covers it"] {
        assert!(started.prompt.contains(part), "{part} missing from {}", started.prompt);
    }
}

#[tokio::test]
async fn a_note_adds_to_the_one_there() {
    let hub = hub();
    let p = problems::record(&hub, report("index out of range in parse_rows", "1.0")).await.unwrap();
    let first = problems::Note { what: "Reads row 0 of an empty list.".into(), plan: "Guard it.".into(), ..Default::default() };
    problems::change(&hub, p.id, ProblemPatch { note: Some(first), ..Default::default() }, "Vakten").await.unwrap();
    let contract = problems::Note { repro: "Open an empty sheet.".into(), done: "It opens; a test covers it.".into(), ..Default::default() };
    problems::change(&hub, p.id, ProblemPatch { note: Some(contract), ..Default::default() }, "Vakten").await.unwrap();
    let note = problems::get(&hub, p.id).await.unwrap().unwrap().note.unwrap();
    assert_eq!(note.what, "Reads row 0 of an empty list.");
    assert_eq!(note.plan, "Guard it.");
    assert_eq!(note.repro, "Open an empty sheet.");
    assert_eq!(note.done, "It opens; a test covers it.");
}

#[tokio::test]
async fn a_steady_slow_metric_is_one_hit_until_it_grows() {
    let hub = hub();
    let metric = |median: f64| problems::metrics::MetricSummary {
        name: "hubd_memory_mb".into(),
        label: "Memory hubd holds".into(),
        unit: "MB".into(),
        release: "c85bb69a8903".into(),
        median,
        samples: 50,
        previous_release: Some("f719c23417a9".into()),
        previous_median: Some(76.6),
        trend: vec![],
    };
    // Sweep after sweep at the same median: the problem keeps one hit and does not escalate.
    for _ in 0..20 {
        problems::metrics::report(&hub, &metric(148.2), hub_core::model::now_ms()).await.unwrap();
    }
    let p = problems::list(&hub, ProblemFilter::default()).await.unwrap().remove(0);
    assert_eq!((p.count, p.escalating, p.prio), (1, false, None));

    // Grown by a tenth or more over the recorded median: a second hit, with the sweeps it stayed slow.
    problems::metrics::report(&hub, &metric(165.0), hub_core::model::now_ms()).await.unwrap();
    let d = problems::detail(&hub, p.id, 5).await.unwrap().unwrap();
    assert_eq!(d.problem.count, 2);
    assert!(d.hits[0].extra["sweeps"].is_i64());
}

#[tokio::test]
async fn a_metric_problem_merged_into_another_still_counts_once_per_release() {
    let hub = hub();
    let metric = |median: f64| problems::metrics::MetricSummary {
        name: "hubd_memory_mb".into(),
        label: "Memory hubd holds".into(),
        unit: "MB".into(),
        release: "afb0f201ae13".into(),
        median,
        samples: 50,
        previous_release: Some("9f65263509c0".into()),
        previous_median: Some(78.0),
        trend: vec![],
    };
    let sweep = |median: f64| {
        let m = metric(median);
        let hub = hub.clone();
        async move { problems::metrics::report(&hub, &m, hub_core::model::now_ms()).await }
    };
    sweep(313.0).await.unwrap();
    let mine = problems::list(&hub, ProblemFilter::default()).await.unwrap().remove(0);
    let other = problems::record(&hub, report("something else", "1.0")).await.unwrap();
    // The earlier problems were merged into one: this release's fingerprint now lives as an alias.
    problems::change(&hub, mine.id, ProblemPatch { merge_into: Some(other.id), why: Some("same metric".into()), ..Default::default() }, "you")
        .await
        .unwrap();
    let before = problems::get(&hub, other.id).await.unwrap().unwrap().count;
    for median in [313.0, 315.0, 316.0] {
        sweep(median).await.unwrap();
    }
    assert_eq!(problems::get(&hub, other.id).await.unwrap().unwrap().count, before, "1% growth is no new hit");
    sweep(380.0).await.unwrap();
    assert_eq!(problems::get(&hub, other.id).await.unwrap().unwrap().count, before + 1, "a rise of 20% is");
}
