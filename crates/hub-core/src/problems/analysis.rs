//! Tool-neutral code analysis. Finishing a run is one transaction: observations, Problems and
//! verified disappearances are committed together. No scanner or agent is started here.
use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::Response,
    routing::{get, post},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use super::{Frame, Hit, Level};
use crate::{
    hub::Hub,
    model::{StreamMsg, now_ms},
};

pub const SOURCE: &str = "code_analysis";

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AnalysisInput {
    pub project: String,
    /// Canonical project folder, supplied by the adapter (not a thread's worktree folder).
    pub folder: String,
    pub branch: String,
    pub commit: String,
    pub tool: String,
    pub tool_version: String,
    /// Digest of effective rules AND configuration, including exclusions.
    pub rules_digest: String,
    /// Stable identifier of the exact analysis targets; partial scans use another scope.
    pub scope: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AnalysisState {
    Running,
    Succeeded,
    Failed,
    Partial,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CodeFinding {
    pub rule: String,
    /// Project-relative path with forward slashes.
    pub path: String,
    /// Adapter's stable occurrence key (e.g. enclosing symbol + normalized code context).
    /// Never use a line number alone: positions move, and a rule can occur twice in a file.
    pub key: String,
    pub line: u32,
    #[serde(default)]
    pub column: Option<u32>,
    pub level: Level,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AnalysisFinish {
    pub state: AnalysisState,
    pub findings: Vec<CodeFinding>,
    #[serde(default)]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AnalysisRun {
    #[ts(type = "number")]
    pub id: i64,
    pub input: AnalysisInput,
    pub state: AnalysisState,
    #[ts(type = "number")]
    pub started_at: i64,
    #[ts(type = "number | null")]
    pub finished_at: Option<i64>,
    pub diagnostic: Option<String>,
    pub findings: Vec<CodeFinding>,
}

fn digest(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    format!("{:x}", hash.finalize())
}

fn required(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 4096 || value.contains('\0') {
        bail!("analysis identifiers must be nonempty, at most 4096 bytes and contain no NUL");
    }
    Ok(())
}

pub async fn start(hub: &Hub, input: AnalysisInput) -> Result<AnalysisRun> {
    for value in [
        &input.project,
        &input.folder,
        &input.branch,
        &input.commit,
        &input.tool,
        &input.tool_version,
        &input.rules_digest,
        &input.scope,
    ] {
        required(value)?;
    }
    let mut value = serde_json::to_value(input)?;
    hub.secrets.redact(&mut value);
    let input: AnalysisInput = serde_json::from_value(value)?;
    // Versions are comparison boundaries, but not finding identities: upgrading a tool should
    // not duplicate every finding. A run from the old profile cannot close newer findings.
    let stream = digest(&[&input.project, &input.folder, &input.branch, &input.tool, &input.scope]);
    let profile = digest(&[&input.tool_version, &input.rules_digest]);
    let data = serde_json::to_string(&input)?;
    let id = hub
        .store
        .with(move |c| {
            c.execute(
                "INSERT INTO analysis_runs(stream, profile, data, started_at) VALUES (?1, ?2, ?3, ?4)",
                params![stream, profile, data, now_ms()],
            )?;
            Ok(c.last_insert_rowid())
        })
        .await?;
    get_run(hub, id).await?.context("analysis run just created")
}

fn read_run(c: &rusqlite::Connection, id: i64) -> Result<Option<AnalysisRun>> {
    let row = c
        .query_row(
            "SELECT data, state, started_at, finished_at, diagnostic FROM analysis_runs WHERE id = ?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((data, state, started_at, finished_at, diagnostic)) = row else {
        return Ok(None);
    };
    let findings = c
        .prepare("SELECT data FROM analysis_observations WHERE run_id = ?1 ORDER BY identity")?
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|s| serde_json::from_str(&s))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(Some(AnalysisRun {
        id,
        input: serde_json::from_str(&data)?,
        state: serde_json::from_value(state.into())?,
        started_at,
        finished_at,
        diagnostic,
        findings,
    }))
}

pub async fn get_run(hub: &Hub, id: i64) -> Result<Option<AnalysisRun>> {
    hub.store.with(move |c| read_run(c, id)).await
}

#[derive(Default, Deserialize)]
struct RunFilter {
    project: Option<String>,
}

async fn runs(hub: &Hub, project: Option<String>) -> Result<Vec<AnalysisRun>> {
    hub.store
        .with(move |c| {
            let ids = c
                .prepare("SELECT id FROM analysis_runs WHERE (?1 IS NULL OR json_extract(data, '$.project') = ?1) ORDER BY id DESC LIMIT 100")?
                .query_map(params![project], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids.into_iter().map(|id| read_run(c, id)?.context("listed run missing")).collect()
        })
        .await
}

/// The checks on what a finishing analysis sends before anything is kept.
fn check_finish(result: &AnalysisFinish) -> Result<()> {
    if !matches!(result.state, AnalysisState::Succeeded | AnalysisState::Failed | AnalysisState::Partial) {
        bail!("finish state must be succeeded, failed or partial");
    }
    if result.findings.len() > 10000 {
        bail!("at most 10000 findings per run");
    }
    if result.diagnostic.as_ref().is_some_and(|s| s.len() > 16000) {
        bail!("diagnostic is too long");
    }
    if result.state != AnalysisState::Succeeded && result.diagnostic.as_deref().is_none_or(|s| s.trim().is_empty()) {
        bail!("failed and partial runs require a diagnostic");
    }
    let mut identities = HashSet::new();
    for f in &result.findings {
        for s in [&f.rule, &f.path, &f.key, &f.message] {
            required(s)?;
        }
        if f.line == 0
            || f.column == Some(0)
            || f.path.contains(['\\', ':'])
            || f.path.starts_with('/')
            || f.path.split('/').any(|p| p == ".." || p == "." || p.is_empty())
        {
            bail!("finding needs a relative normalized path and positive positions");
        }
        if !identities.insert(digest(&[&f.rule, &f.path, &f.key])) {
            bail!("duplicate finding identity");
        }
    }
    Ok(())
}

/// Keeps one finding of a run as an observation, and when the run succeeded as the current
/// finding (a new problem, or a problem seen again). Returns the problem it touched.
#[allow(clippy::too_many_arguments)]
fn record_finding(
    tx: &rusqlite::Transaction,
    id: i64,
    run: &AnalysisRun,
    (stream, profile): (&str, &str),
    f: &CodeFinding,
    machine: &str,
    ts: i64,
    succeeded: bool,
) -> Result<Option<i64>> {
    let identity = digest(&[&f.rule, &f.path, &f.key]);
    let data = serde_json::to_string(f)?;
    tx.execute("INSERT INTO analysis_observations(run_id, identity, data) VALUES (?1, ?2, ?3)", params![id, identity, data])?;
    // Failed/partial output is evidence, not a new current baseline.
    if !succeeded { return Ok(None); }
    let existing: Option<i64> = tx.query_row("SELECT problem_id FROM analysis_findings WHERE stream = ?1 AND identity = ?2",
        params![stream, identity], |r| r.get(0)).optional()?;
    let culprit = format!("{}:{}", f.path, f.line);
    let problem = if let Some(problem) = existing {
        let was: String = tx.query_row("SELECT status FROM problems WHERE id = ?1", [problem], |r| r.get(0))?;
        tx.execute("UPDATE problems SET title = ?2, culprit = ?3, level = ?4, level_rank = ?5,
            last_seen = ?6, count = count + 1, status = CASE WHEN status = 'resolved' THEN 'open' ELSE status END,
            regressed = CASE WHEN status = 'resolved' THEN 1 ELSE regressed END,
            resolved_at = NULL, resolved_by = NULL, resolved_releases = NULL WHERE id = ?1",
            params![problem, f.message, culprit, f.level.as_str(), f.level as i64, ts])?;
        if was == "resolved" { super::log(tx, problem, "Analysis", "regressed", "Found again by a successful code analysis")?; }
        tx.execute("UPDATE analysis_findings SET profile = ?2, last_run = ?3, data = ?4 WHERE problem_id = ?1",
            params![problem, profile, id, data])?;
        problem
    } else {
        let fingerprint = format!("analysis:{}", digest(&[stream, &identity]));
        tx.execute("INSERT INTO problems(fingerprint, project, source, level, level_rank, title, culprit, folder, machine,
            first_seen, last_seen, count, status, regressed, lane, stamped_by)
            VALUES (?1, ?2, 'code_analysis', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 1, 'open', 0, 'ask', 'Analysis')",
            params![fingerprint, run.input.project, f.level.as_str(), f.level as i64, f.message, culprit, run.input.folder, machine, ts])?;
        let problem = tx.last_insert_rowid();
        tx.execute("INSERT INTO analysis_findings(problem_id, stream, identity, profile, first_run, last_run, data) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
            params![problem, stream, identity, profile, id, data])?;
        super::log(tx, problem, "Analysis", "seen", &format!("Code finding in run {id}, commit {}", run.input.commit))?;
        problem
    };
    let hit = Hit { ts, message: f.message.clone(), kind: Some(f.rule.clone()),
        frames: vec![Frame { file: Some(f.path.clone()), line: Some(f.line), own: true, ..Default::default() }],
        release: Some(run.input.commit.clone()),
        extra: serde_json::json!({"analysis_run": id, "analysis": run.input, "finding": f}), ..Default::default() };
    tx.execute("INSERT INTO problem_hits(problem_id, ts, data) VALUES (?1, ?2, ?3)", params![problem, ts, serde_json::to_string(&hit)?])?;
    tx.execute("DELETE FROM problem_hits WHERE problem_id = ?1 AND rowid NOT IN (SELECT rowid FROM problem_hits WHERE problem_id = ?1 ORDER BY rowid DESC LIMIT 30)", [problem])?;
    tx.execute("INSERT INTO problem_days(problem_id, day, count) VALUES (?1, ?2, 1) ON CONFLICT(problem_id, day) DO UPDATE SET count = count + 1", params![problem, ts / super::DAY_MS])?;
    tx.execute("DELETE FROM problem_days WHERE problem_id = ?1 AND day < ?2", params![problem, ts / super::DAY_MS - 90])?;
    Ok(Some(problem))
}

/// Problems a complete successful run no longer finds are resolved.
fn resolve_absent(tx: &rusqlite::Transaction, id: i64, run: &AnalysisRun, (stream, profile): (&str, &str), ts: i64) -> Result<Vec<i64>> {
    let mut changed = Vec::new();
        let missing = tx.prepare("SELECT f.problem_id FROM analysis_findings f JOIN problems p ON p.id = f.problem_id
            WHERE f.stream = ?1 AND f.profile = ?2 AND p.status = 'open'
            AND NOT EXISTS(SELECT 1 FROM analysis_observations o WHERE o.run_id = ?3 AND o.identity = f.identity)")?
            .query_map(params![stream, profile, id], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for problem in missing {
            tx.execute("UPDATE problems SET status = 'resolved', resolved_at = ?2, resolved_by = ?3, regressed = 0,
                resolved_releases = NULL WHERE id = ?1", params![problem, ts, format!("Analysis run {id}")])?;
            super::log(tx, problem, "Analysis", "status", &format!("Absent in complete run {id}, commit {}", run.input.commit))?;
            changed.push(problem);
        }
    Ok(changed)
}

pub async fn finish(hub: &Hub, id: i64, result: AnalysisFinish) -> Result<AnalysisRun> {
    check_finish(&result)?;
    // Redact before observations, current finding metadata or Problems reach SQLite.
    let mut value = serde_json::to_value(result)?;
    hub.secrets.redact(&mut value);
    let result: AnalysisFinish = serde_json::from_value(value)?;
    let machine = hub.remotes.local_name().to_owned();
    let changed = hub.store.with(move |c| {
        let tx = c.transaction()?;
        let run = read_run(&tx, id)?.context("no such analysis run")?;
        if run.state != AnalysisState::Running { bail!("analysis run is already finished"); }
        let (stream, profile): (String, String) = tx.query_row("SELECT stream, profile FROM analysis_runs WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if result.state == AnalysisState::Succeeded {
            let newer: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM analysis_runs WHERE stream = ?1 AND id > ?2 AND state = 'succeeded')",
                params![stream, id], |r| r.get(0))?;
            if newer { bail!("a newer successful run already superseded this run; finish it as partial"); }
        }
        let ts = now_ms();
        let mut changed = Vec::new();
        for f in &result.findings {
            if let Some(problem) = record_finding(&tx, id, &run, (&stream, &profile), f, &machine, ts, result.state == AnalysisState::Succeeded)? {
                changed.push(problem);
            }
        }
        if result.state == AnalysisState::Succeeded {
            changed.extend(resolve_absent(&tx, id, &run, (&stream, &profile), ts)?);
        }
        tx.execute("UPDATE analysis_runs SET state = ?2, finished_at = ?3, diagnostic = ?4 WHERE id = ?1",
            params![id, serde_json::to_value(result.state)?.as_str().context("state")?, ts, result.diagnostic])?;
        tx.commit()?;
        Ok(changed)
    }).await?;
    for id in changed {
        if let Some(problem) = super::get(hub, id).await? {
            hub.publish(StreamMsg::Problem { problem });
        }
    }
    get_run(hub, id).await?.context("finished analysis run missing")
}

pub fn router() -> Router<Arc<Hub>> {
    Router::new()
        .route("/api/problems/analysis/runs", get(list_route).post(start_route))
        .route("/api/problems/analysis/runs/{id}", get(detail_route))
        .route("/api/problems/analysis/runs/{id}/finish", post(finish_route))
}
async fn list_route(State(hub): State<Arc<Hub>>, Query(filter): Query<RunFilter>) -> Response {
    super::answer(runs(&hub, filter.project).await)
}
async fn start_route(State(hub): State<Arc<Hub>>, Json(input): Json<AnalysisInput>) -> Response {
    super::answer(start(&hub, input).await)
}
async fn detail_route(State(hub): State<Arc<Hub>>, Path(id): Path<i64>) -> Response {
    super::answer(get_run(&hub, id).await)
}
async fn finish_route(State(hub): State<Arc<Hub>>, Path(id): Path<i64>, Json(result): Json<AnalysisFinish>) -> Response {
    super::answer(finish(&hub, id, result).await)
}
