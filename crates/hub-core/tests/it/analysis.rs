//! Code analysis is a snapshot, not a runtime event. Absence is evidence only in a comparable,
//! successful snapshot; out-of-order completions must never undo a newer baseline.
use hub_core::problems::analysis::{self, AnalysisFinish, AnalysisInput, AnalysisState, CodeFinding};
use hub_core::problems::{self, Level, ProblemFilter, ProblemStatus, View};
use hub_core::{Config, Hub, Store};
use std::sync::Arc;

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}
fn input() -> AnalysisInput {
    AnalysisInput {
        project: "Hub".into(),
        folder: "D:/Dev/hub".into(),
        branch: "main".into(),
        commit: "a".repeat(40),
        tool: "example".into(),
        tool_version: "1".into(),
        rules_digest: "rules-and-config-v1".into(),
        scope: "tracked-source".into(),
    }
}
fn finding(line: u32) -> CodeFinding {
    CodeFinding {
        rule: "unsafe-call".into(),
        path: "src/main.rs".into(),
        key: "main:unsafe-call:context".into(),
        line,
        column: Some(1),
        level: Level::Warning,
        message: "Unsafe call needs checking".into(),
    }
}
fn result(state: AnalysisState, findings: Vec<CodeFinding>) -> AnalysisFinish {
    AnalysisFinish {
        state,
        findings,
        diagnostic: if state == AnalysisState::Succeeded {
            None
        } else {
            Some("scanner stopped early".into())
        },
    }
}
async fn scan(hub: &Hub, input: AnalysisInput, result: AnalysisFinish) -> i64 {
    let id = analysis::start(hub, input).await.unwrap().id;
    analysis::finish(hub, id, result).await.unwrap();
    id
}
async fn all(hub: &Hub) -> Vec<problems::Problem> {
    problems::list(
        hub,
        ProblemFilter {
            view: Some(View::All),
            source: Some(analysis::SOURCE.into()),
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn stable_identity_moves_resolves_and_regresses_with_commit_evidence() {
    let hub = hub();
    scan(&hub, input(), result(AnalysisState::Succeeded, vec![finding(12)])).await;
    let first = all(&hub).await.remove(0);
    let mut next = input();
    next.commit = "b".repeat(40);
    let id = scan(&hub, next.clone(), result(AnalysisState::Succeeded, vec![finding(21)])).await;
    let ps = all(&hub).await;
    assert_eq!(ps.len(), 1);
    assert_eq!(ps[0].id, first.id);
    assert_eq!(ps[0].count, 2);
    assert_eq!(ps[0].culprit.as_deref(), Some("src/main.rs:21"));
    let detail = problems::detail(&hub, first.id, 30).await.unwrap().unwrap();
    assert!(
        detail
            .hits
            .iter()
            .any(|h| h.extra["analysis_run"] == id && h.release.as_deref() == Some(next.commit.as_str()))
    );
    scan(&hub, next, result(AnalysisState::Succeeded, vec![])).await;
    assert_eq!(all(&hub).await[0].status, ProblemStatus::Resolved);
    scan(&hub, input(), result(AnalysisState::Succeeded, vec![finding(30)])).await;
    let ps = all(&hub).await;
    assert_eq!(ps[0].id, first.id);
    assert_eq!(ps[0].status, ProblemStatus::Open);
    assert!(ps[0].regressed);
    assert_eq!(all(&hub).await.len(), 1);
}

#[tokio::test]
async fn incomplete_failed_and_different_profiles_never_close_findings() {
    let hub = hub();
    scan(&hub, input(), result(AnalysisState::Succeeded, vec![finding(1)])).await;
    for state in [AnalysisState::Failed, AnalysisState::Partial] {
        let id = scan(&hub, input(), result(state, vec![])).await;
        let run = analysis::get_run(&hub, id).await.unwrap().unwrap();
        assert_eq!(run.state, state);
        assert!(run.finished_at.is_some());
        assert!(run.diagnostic.is_some());
        assert_eq!(all(&hub).await[0].status, ProblemStatus::Open);
    }
    for field in ["tool_version", "rules_digest", "scope", "branch", "folder", "project", "tool"] {
        let mut value = serde_json::to_value(input()).unwrap();
        value[field] = "different".into();
        scan(&hub, serde_json::from_value(value).unwrap(), result(AnalysisState::Succeeded, vec![])).await;
        assert_eq!(all(&hub).await[0].status, ProblemStatus::Open, "{field}");
    }
    scan(&hub, input(), result(AnalysisState::Succeeded, vec![])).await;
    assert_eq!(all(&hub).await[0].status, ProblemStatus::Resolved);
}

#[tokio::test]
async fn newer_success_cannot_be_overwritten_and_terminal_runs_are_immutable() {
    let hub = hub();
    let old = analysis::start(&hub, input()).await.unwrap();
    let new = scan(&hub, input(), result(AnalysisState::Succeeded, vec![finding(1)])).await;
    assert!(analysis::finish(&hub, old.id, result(AnalysisState::Succeeded, vec![])).await.is_err());
    assert_eq!(analysis::get_run(&hub, old.id).await.unwrap().unwrap().state, AnalysisState::Running);
    assert!(analysis::finish(&hub, new, result(AnalysisState::Succeeded, vec![])).await.is_err());
    analysis::finish(&hub, old.id, result(AnalysisState::Partial, vec![])).await.unwrap();
    assert_eq!(all(&hub).await[0].status, ProblemStatus::Open);
}

#[tokio::test]
async fn invalid_snapshot_is_atomic_and_distinct_occurrences_are_preserved() {
    let hub = hub();
    let run = analysis::start(&hub, input()).await.unwrap();
    assert!(
        analysis::finish(&hub, run.id, result(AnalysisState::Succeeded, vec![finding(1), finding(2)]))
            .await
            .is_err()
    );
    assert!(all(&hub).await.is_empty());
    assert!(analysis::get_run(&hub, run.id).await.unwrap().unwrap().findings.is_empty());
    let mut other = finding(2);
    other.key = "other-symbol".into();
    analysis::finish(&hub, run.id, result(AnalysisState::Succeeded, vec![finding(1), other]))
        .await
        .unwrap();
    assert_eq!(all(&hub).await.len(), 2);
    let invalid = analysis::start(&hub, input()).await.unwrap();
    let mut bad = finding(1);
    bad.path = "../outside.rs".into();
    assert!(analysis::finish(&hub, invalid.id, result(AnalysisState::Succeeded, vec![bad])).await.is_err());
}

#[tokio::test]
async fn runtime_status_and_merge_cannot_verify_a_code_fix() {
    let hub = hub();
    scan(&hub, input(), result(AnalysisState::Succeeded, vec![finding(1)])).await;
    let p = all(&hub).await.remove(0);
    assert!(problems::set_status(&hub, p.id, ProblemStatus::Resolved).await.is_err());
    problems::set_status(&hub, p.id, ProblemStatus::Archived).await.unwrap();
    scan(&hub, input(), result(AnalysisState::Succeeded, vec![])).await;
    assert_eq!(all(&hub).await[0].status, ProblemStatus::Archived);
    problems::set_status(&hub, p.id, ProblemStatus::Open).await.unwrap();
    scan(&hub, input(), result(AnalysisState::Succeeded, vec![])).await;
    assert_eq!(all(&hub).await[0].status, ProblemStatus::Resolved);
}

#[tokio::test]
async fn analysis_api_registers_runs_and_keeps_partial_output_visible() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let hub = hub();
    let app = hub_core::api::router(hub.clone());
    let token = format!("Bearer {}", hub_core::auth::token(&hub.config.data_dir).unwrap());
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/problems/analysis/runs")
                .header("authorization", &token)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&input()).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let run: analysis::AnalysisRun = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 100000).await.unwrap()).unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("/api/problems/analysis/runs/{}/finish", run.id))
                .header("authorization", &token)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&result(AnalysisState::Partial, vec![finding(9)])).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let run = analysis::get_run(&hub, run.id).await.unwrap().unwrap();
    assert_eq!(run.findings.len(), 1);
    assert_eq!(run.state, AnalysisState::Partial);
    assert!(all(&hub).await.is_empty());
    let response = app
        .oneshot(
            Request::get("/api/problems/analysis/runs?project=Hub")
                .header("authorization", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
