//! Model types: usage.

use super::*;

/// How much happened on one day: what the activity map lights up.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityDay {
    /// `YYYY-MM-DD` in the asker's time zone.
    pub date: String,
    /// Threads with a message or a finished turn that day.
    pub threads: u32,
    /// Messages you sent.
    pub messages: u32,
    /// Turns the agents finished.
    pub turns: u32,
    /// Your commits in the repositories threads and projects live in.
    pub commits: u32,
    /// Prompts you wrote in Claude Code outside Hub.
    pub prompts: u32,
    /// Claude Code sessions outside Hub with a prompt that day.
    pub sessions: u32,
    /// Claude Code tokens, in and out of Hub: fresh input plus output, without cache reads.
    #[ts(type = "number")]
    pub tokens: i64,
    /// GitHub contributions: commits, pull requests, issues and reviews in any repository.
    pub github: u32,
    /// Of those, commits (GitHub counts at most the first 100 commit days per repository).
    #[serde(default)]
    pub github_commits: u32,
}

/// The map's days, and why a source is missing if one is.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityReport {
    pub days: Vec<ActivityDay>,
    pub notes: Vec<String>,
}

/// Models, tokens and cost over the last days, for the overview.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Stats {
    pub days: u32,
    /// Answers from a known model.
    pub answers: u32,
    /// Fresh input plus output, without cache reads.
    #[ts(type = "number")]
    pub tokens: i64,
    /// Dollars the same tokens would cost through the API.
    pub cost: f64,
    /// Most tokens first.
    pub models: Vec<ModelStat>,
    /// Most expensive first.
    pub projects: Vec<ProjectStat>,
    /// Hub threads, most expensive first.
    pub threads: Vec<ThreadStat>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelStat {
    /// "Opus 4.8".
    pub model: String,
    pub answers: u32,
    #[ts(type = "number")]
    pub tokens: i64,
    pub cost: f64,
    /// False when Hub knows no price for the model, so its cost is left out.
    pub priced: bool,
    /// Changes it made in Hub threads that Hub checked for proof, and how many showed it (a test
    /// run or screenshot that passed). A change goes to the model that spent most on it.
    pub results: u32,
    pub proven: u32,
    /// What those changes cost, proven or not, so `result_cost / proven` is the price of one
    /// proven change.
    pub result_cost: f64,
}

/// All AI use of the last days, in Hub and outside it (`GET /api/usage/spend`, `spend.rs`):
/// one lane per place it happened, each split per day, Hub's lane also per engine and by who
/// asked. Plan use is priced as the same tokens would cost through the API; DeepSeek is what its
/// balance really fell by.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Spend {
    pub days: u32,
    /// The days covered, oldest first, as YYYY-MM-DD in the asked time zone. Every `daily` list
    /// below has one number per date, in this order.
    pub dates: Vec<String>,
    /// "hub" (everything run through Hub, its helper calls included), "claude_code" (Claude Code
    /// and the Claude app on their own) and "codex" (Codex on its own). A client draws a key it
    /// does not know plainly.
    pub lanes: Vec<SpendLane>,
    /// Hub's lane by who asked. Keys: "you", "chat", "side_job", an `OwnerKind` ("vakten",
    /// "fixer", "merge", "routine", "widget", "customs", "agent"), "helper" (Hub's one-shot
    /// calls: titles, patch notes, summaries) and "deleted" (a thread no longer in Hub).
    pub sources: Vec<SpendSource>,
    /// Asked for every machine (`everywhere=true`, feature `usage.spend.machines`): what each
    /// one counted, this machine first. The lanes and sources above are then all of them added
    /// together. Absent when only this machine was asked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub machines: Option<Vec<SpendMachine>>,
    /// Why part of the use could not be read, in words for the page (such as the cloud sessions
    /// while Claude Code's sign-in is out of date).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notes: Option<Vec<String>>,
    /// What a provider itself says its whole account used, wherever it ran, per day in the order
    /// of `dates` (feature `usage.spend.accounts`). Tokens only, as the provider counts them: no
    /// price, no model. Today only Codex (`account/usage/read`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub accounts: Option<Vec<SpendAccount>>,
    /// Asked with `hours=true` (feature `usage.spend.hours`): the same use hour by hour over the
    /// last 31 days of the range at most, for a page that zooms in on a day. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hourly: Option<SpendHourly>,
    /// "tokens" when asked with `unit=tokens` (feature `usage.spend.tokens`): every `cost` and
    /// `daily` here then counts tokens (fresh input plus output, without cache reads) in place of
    /// dollars, and nothing is estimated or paid by the balance. Absent means dollars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unit: Option<String>,
}

impl Spend {
    /// The lane (`SpendLane::key`) of Hub's own threads; the engines' own lanes are named in
    /// `spend.rs` and `plan_meter.rs`.
    pub const HUB: &'static str = "hub";
    /// The longest range asked: ten years.
    pub const MAX_DAYS: u32 = 3650;
}

/// Use hour by hour. Each lane's (and source's) `daily` here holds one number per hour from `start`, and its
/// `cost` is their sum; Hub's lane carries its engines the same way. What is only known per day
/// (DeepSeek's balance, the estimate from Claude's plan meter) is left out, and cloud sessions
/// fall on the hours their totals were read, so their hours are approximate.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpendHourly {
    /// The first hour's start, ms since the epoch: a midnight in the asked time zone, the first
    /// date's or 31 days before the end of today's.
    #[ts(type = "number")]
    pub start: i64,
    pub lanes: Vec<SpendLane>,
    /// Hub's lane by who asked, each source's `daily` per hour.
    #[serde(default)]
    pub sources: Vec<SpendSource>,
}

/// A provider's own count of its account's tokens.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpendAccount {
    /// "codex".
    pub key: String,
    #[ts(type = "number")]
    pub tokens: i64,
    #[ts(type = "number[]")]
    pub daily: Vec<i64>,
    /// Since the account began, when the provider says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub lifetime: Option<i64>,
}

/// One machine's part of the spend, or why it could not be asked.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpendMachine {
    pub name: String,
    pub cost: f64,
    pub calls: u32,
    #[ts(type = "number")]
    pub tokens: i64,
    /// Why it is not in the numbers: offline, too old to count, or no answer in time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpendLane {
    pub key: String,
    /// Dollars.
    pub cost: f64,
    /// Answers counted.
    pub calls: u32,
    /// Fresh input plus output, without cache reads.
    #[ts(type = "number")]
    pub tokens: i64,
    /// Dollars per day, in the order of `Spend.dates`.
    pub daily: Vec<f64>,
    /// The lane per engine maker: "claude", "codex", "deepseek". Only Hub's lane has more than one.
    pub engines: Vec<SpendEngine>,
    /// False when a model had no known price and an estimate stands in for it.
    pub priced: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpendEngine {
    pub key: String,
    pub cost: f64,
    pub daily: Vec<f64>,
    /// Real money per token, outside any plan (DeepSeek).
    pub paid: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpendSource {
    pub key: String,
    pub calls: u32,
    /// Threads behind it; 0 for "helper".
    pub threads: u32,
    #[ts(type = "number")]
    pub tokens: i64,
    pub cost: f64,
    /// Its dollars per date of the page (or per hour, in `hourly`), so a bar can be split by who
    /// asked (feature `usage.spend.hours`). Absent from an older hubd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub daily: Option<Vec<f64>>,
}

/// One Hub thread's Claude Code tokens and cost, and its checked changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadStat {
    pub id: ThreadId,
    pub title: String,
    #[ts(type = "number")]
    pub tokens: i64,
    pub cost: f64,
    pub results: u32,
    pub proven: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectStat {
    pub name: String,
    /// The project's folder, or the session's when it belongs to no project.
    pub path: Option<String>,
    #[ts(type = "number")]
    pub tokens: i64,
    pub cost: f64,
}

/// How a project or one of its checks looks, worst last.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Health {
    /// Nothing to say, e.g. GitHub could not be asked.
    #[default]
    Unknown,
    Ok,
    /// Worth a look: uncommitted work, stale branches, a quiet project.
    Warn,
    /// Broken: CI failing, the folder gone.
    Bad,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HealthCheck {
    /// "CI", "Uncommitted", "Branches"...
    pub label: String,
    pub health: Health,
    pub detail: String,
    /// Where to read more, such as the CI run.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectHealth {
    pub name: String,
    pub path: String,
    /// The worst of its checks.
    pub health: Health,
    pub checks: Vec<HealthCheck>,
}

/// The weekly health check of every project.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HealthReport {
    /// Unix milliseconds; 0 when no check has run yet.
    #[ts(type = "number")]
    pub checked_at: i64,
    /// Worst first.
    pub projects: Vec<ProjectHealth>,
}

/// A day of the devlog: what Hub wrote about it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevlogDay {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub summary: String,
}

/// One day of one project's devlog, read straight from git and Hub's threads.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectDay {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub commits: u32,
    /// The day's commit subjects, newest first, at most a handful.
    pub subjects: Vec<String>,
    /// Threads in the project that you or their agent wrote in that day.
    pub threads: Vec<DayRef>,
}

/// A thread named in a summary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DayRef {
    pub id: String,
    pub title: String,
}

/// What happened while you were away besides the threads (the app has those): commits per
/// repository, pull requests opened, what Hub did on its own, and trouble the health check found.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AwayReport {
    /// Most commits first.
    pub repos: Vec<AwayRepo>,
    /// Opened on GitHub since that day (GitHub only says the day).
    pub pulls: Vec<DayPull>,
    /// Permission questions Hub answered by your settings.
    pub permissions: u32,
    /// Threads Hub carried on after a plan limit or a restart.
    pub resumed: u32,
    /// Second opinions that finished.
    pub reviews: u32,
    /// Side jobs that finished.
    pub side_jobs: u32,
    /// Projects the health check found broken or worth a look, when it ran meanwhile.
    pub health: Vec<ProjectHealth>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AwayRepo {
    /// The repository's folder.
    pub path: String,
    pub name: String,
    pub commits: u32,
    /// The newest subjects, at most three.
    pub subjects: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HistoryKind {
    /// Something you wrote to a Hub thread.
    Message,
    /// A prompt in Claude Code outside Hub.
    Prompt,
    /// A commit of yours.
    Commit,
    /// A devlog day.
    Day,
}

/// One match when searching what you did.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HistoryHit {
    pub kind: HistoryKind,
    /// Unix milliseconds; 0 for a devlog day, which has `date` instead.
    #[ts(type = "number")]
    pub ts: i64,
    pub date: Option<String>,
    pub text: String,
    /// The Hub thread, for a message.
    pub thread_id: Option<String>,
    /// Thread, session or repository name.
    pub title: Option<String>,
    pub cwd: Option<String>,
}

/// One day in detail, for clicking a dot on the map.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActivityDetail {
    pub date: String,
    pub threads: Vec<DayThread>,
    pub commits: Vec<DayCommit>,
    /// Claude Code sessions outside Hub.
    pub sessions: Vec<DaySession>,
    /// GitHub: commits per repository, and pull requests opened.
    pub github: Vec<DayRepo>,
    pub pulls: Vec<DayPull>,
    /// The summary written for this day earlier, if any.
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DayThread {
    pub id: ThreadId,
    pub title: String,
    pub cwd: String,
    /// Your messages that day, first lines only.
    pub messages: Vec<String>,
    pub turns: u32,
    /// Claude Code tokens the thread used that day.
    #[ts(type = "number")]
    pub tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DaySession {
    pub id: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    /// First lines of the prompts written that day.
    pub prompts: Vec<String>,
    #[ts(type = "number")]
    pub tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DayRepo {
    /// `owner/name`.
    pub repo: String,
    pub commits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DayPull {
    pub repo: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DayCommit {
    /// The repository's folder.
    pub repo: String,
    pub hash: String,
    pub subject: String,
    #[ts(type = "number")]
    pub ts: i64,
    /// Lines added, lock files and binaries left out.
    #[serde(default)]
    pub added: u32,
    /// Lines removed, lock files and binaries left out.
    #[serde(default)]
    pub removed: u32,
}

/// The playful side of the overview: when you write and the raw material for records and
/// badges, all from Hub's database so it answers at once. Lines in and out are a separate,
/// slower answer (`LinesReport`). The page counts it up in the asker's own clock.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Pulse {
    /// When you wrote, in ms: your messages in Hub and your prompts in Claude Code outside it.
    #[ts(type = "number[]")]
    pub moments: Vec<i64>,
    /// Pull requests you opened on GitHub, per day that had any (from the cached GitHub answer).
    pub pulls: Vec<DayCount>,
    /// The thread with the most finished turns in the range.
    pub longest_thread: Option<LongThread>,
    pub notes: Vec<String>,
}

/// Lines your commits added and removed per day, and how many commits had to be diffed for it
/// this time (the rest came from the cache).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LinesReport {
    pub days: Vec<LinesDay>,
    pub diffed: u32,
    pub commits: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LinesDay {
    pub date: String,
    pub added: u32,
    pub removed: u32,
    pub commits: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DayCount {
    pub date: String,
    pub count: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LongThread {
    pub id: String,
    pub title: String,
    pub turns: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlanLimit {
    /// "5-hour limit", "Weekly · all models", "Weekly · Opus", ...
    pub label: String,
    pub percent: f32,
    /// As the engine words it, e.g. "Oct 30, 1:50 AM GMT+1".
    pub resets: Option<String>,
    /// When it resets, for engines that give a time rather than words.
    #[ts(type = "number | null")]
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlanUsage {
    /// The subscription, e.g. "max".
    pub plan: Option<String>,
    pub limits: Vec<PlanLimit>,
    #[ts(type = "number")]
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadUsage {
    pub thread_id: ThreadId,
    /// Tokens in the context window now, and its size.
    #[ts(type = "number")]
    pub used: u64,
    #[ts(type = "number")]
    pub size: u64,
    /// What the engine's session has cost so far, when it says.
    pub cost: Option<f64>,
    pub currency: Option<String>,
}
