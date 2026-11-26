//! The bench's API types: what a run is asked for and the record of a run (`bench/run.rs` writes
//! it to `<data>/bench/runs/<id>.json`, `bench/results.rs` reads and reports it). Split from
//! `model.rs`, which re-exports them.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::TurnUsage;

/// `POST /api/bench/runs` and the `bench_run` tool: which suite, and what to change of it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchAsk {
    /// A built-in suite's name, such as "smoke".
    pub suite: String,
    /// An engine from the settings; the suite's when left out.
    #[serde(default)]
    #[ts(optional)]
    pub engine: Option<String>,
    /// A model the engine offers, or "fast" for its cheapest; the suite's when left out.
    #[serde(default)]
    #[ts(optional)]
    pub model: Option<String>,
    /// How many times each case runs; the suite's when left out.
    #[serde(default)]
    #[ts(optional)]
    pub repeats: Option<u32>,
    /// Only these cases (by id), in the suite's order; all when left out.
    #[serde(default)]
    #[ts(optional)]
    pub cases: Option<Vec<String>>,
}

/// One run of a suite, as kept on disk and answered by the API.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchRun {
    /// `<started>-<suite>`, also the file's name.
    pub id: String,
    pub suite: String,
    /// The suite's version (`bench::suite_hash`): runs compare only within one.
    pub suite_hash: String,
    pub engine: String,
    /// The model the run asked for, resolved ("fast" is the engine's cheapest).
    pub model_asked: String,
    /// The model the engine said answered, when it said.
    #[serde(default)]
    #[ts(optional)]
    pub model_answered: Option<String>,
    pub machine: String,
    pub hub_version: String,
    #[ts(type = "number")]
    pub started: i64,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub ended: Option<i64>,
    /// "running", "done", "stopped", "cost_cap", "plan_limit", "hub_updated" or "failed".
    pub state: String,
    /// What ended it early, in words.
    #[serde(default)]
    #[ts(optional)]
    pub why: Option<String>,
    pub repeats: u32,
    pub cases: Vec<String>,
    #[serde(default)]
    pub rows: Vec<BenchRow>,
    /// Per case, what the report's table says (feature `bench.summary`). Absent in the answer to
    /// a start and in a run stored before the field without rows; one stored before with rows gets
    /// it worked out on read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub summary: Option<BenchSummary>,
}

/// One repeat of one case.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchRow {
    pub case: String,
    /// 1-based.
    pub repeat: u32,
    #[serde(default)]
    #[ts(optional)]
    pub thread: Option<String>,
    /// "done", "timed_out", "needs_user", "interrupted" or "not_started".
    pub state: String,
    /// Every check passed and the repeat ran to its end.
    pub pass: bool,
    #[serde(default)]
    pub checks: Vec<BenchCheck>,
    /// `bench::Measured`, as JSON.
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub measured: serde_json::Value,
    /// Each turn's tokens as the engine reported them, in order (to see whether an engine counts
    /// per turn or per session).
    #[serde(default)]
    pub turn_usage: Vec<Option<TurnUsage>>,
    /// Why this repeat cannot be compared, when it cannot; never counted as failed.
    #[serde(default)]
    #[ts(optional)]
    pub invalid: Option<String>,
    /// The repeat's folder: removed when it passed, kept for a look when it did not.
    #[serde(default)]
    #[ts(optional)]
    pub folder: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub note: Option<String>,
}

/// One check's verdict (`bench::CheckResult`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchCheck {
    pub check: String,
    pub ok: bool,
    #[serde(default)]
    #[ts(optional)]
    pub why: Option<String>,
}

/// A run summed up per case: the numbers and flags its report's case table prints, worked out by
/// `bench::results::summary`; the report renders from it, so the two always agree.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchSummary {
    /// The run compared with: the newest earlier finished run with the same suite hash, engine
    /// and answering model; null for a new series.
    pub before: Option<BenchRunRef>,
    /// One per case of the run, in the suite's order.
    pub cases: Vec<BenchCaseSummary>,
}

/// Another run, named.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchRunRef {
    pub id: String,
    pub hub_version: String,
}

/// One case over its repeats (interrupted ones left out). Not run when `runs + invalid` is 0.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchCaseSummary {
    pub case: String,
    /// Valid repeats whose checks all passed: `passed` of `runs`.
    pub passed: u32,
    /// Valid repeats.
    pub runs: u32,
    /// Repeats that cannot be compared, counted apart (never failed).
    pub invalid: u32,
    /// Over the valid repeats; null when none measured it.
    pub wall_ms: Option<BenchRange>,
    pub first_word_ms: Option<BenchRange>,
    /// Total tokens (all four counts).
    pub tokens: Option<BenchRange>,
    /// Summed over every repeat of the case that said one; null when none did.
    pub cost_usd: Option<f64>,
    /// The same case in the run compared with; null when that run has none or there is none.
    pub before: Option<BenchCount>,
    /// Against the run before (amber there too turns amber red); null when not run or new.
    pub flag: Option<BenchFlag>,
}

/// k of n.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchCount {
    pub passed: u32,
    pub runs: u32,
}

/// The median (`timings::spread`'s: the middle value, the upper one of two) and the extremes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchRange {
    #[ts(type = "number")]
    pub median: i64,
    #[ts(type = "number")]
    pub min: i64,
    #[ts(type = "number")]
    pub max: i64,
}

/// How a case moved against the version before (`bench::flag`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum BenchLevel {
    Green,
    Amber,
    Red,
}

/// The level and what raised it, one line per reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchFlag {
    pub level: BenchLevel,
    pub why: Vec<String>,
}

/// `GET /api/bench/suites`: a built-in suite with its defaults (what a run uses for what the ask
/// leaves out) and what earlier runs of it took.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchSuiteInfo {
    pub name: String,
    /// `bench::suite_hash`: the version.
    pub hash: String,
    /// Case ids, in order.
    pub cases: Vec<String>,
    pub engine: String,
    /// "fast" (the engine's cheapest) or a model name.
    pub model: String,
    pub repeats: u32,
    #[ts(type = "number")]
    pub timeout_s: u64,
    /// For the whole run.
    pub cost_cap_usd: f64,
    /// From finished runs of this suite version over all its cases, one per engine, resolved model
    /// and repeats, newest first; empty when there is none (no history, no estimate).
    pub estimates: Vec<BenchEstimate>,
}

/// What finished runs of a suite took, as medians over those runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BenchEstimate {
    pub engine: String,
    /// The model those runs asked for, resolved ("fast" was already the engine's cheapest).
    pub model: String,
    pub repeats: u32,
    /// How many runs the medians are over.
    pub runs: u32,
    /// Median of each run's total tokens (all four counts); null when no run said.
    #[ts(type = "number | null")]
    pub tokens: Option<i64>,
    /// Median of each run's cost; null when no run said.
    pub cost_usd: Option<f64>,
}
