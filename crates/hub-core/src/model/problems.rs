//! Model types: problems (Guard's list, moved here so the model knows no function module).

use super::*;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Level {
    Info,
    Warning,
    Error,
}

impl Level {
    pub(crate) fn parse(s: &str) -> Level {
        match s.to_ascii_lowercase().as_str() {
            "fatal" | "critical" | "error" => Level::Error,
            "warning" | "warn" => Level::Warning,
            _ => Level::Info,
        }
    }
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warning => "warning",
            Level::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ProblemStatus {
    Open,
    Resolved,
    /// Out of the list until the archive's condition says it is back (`Archive`). "muted" was
    /// its name before archives had conditions.
    #[serde(alias = "muted")]
    Archived,
}

impl ProblemStatus {
    pub(crate) fn parse(s: &str) -> ProblemStatus {
        match s {
            "resolved" => ProblemStatus::Resolved,
            "archived" | "muted" => ProblemStatus::Archived,
            _ => ProblemStatus::Open,
        }
    }
}

/// How urgent, highest first: P0 something is down or data is lost, P1 a security hole or an
/// error that hits the user, P2 a real fault that can wait, P3 warnings and clutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Prio {
    P0,
    P1,
    P2,
    P3,
}

/// What Vakten may do about a problem without the user: fix it and say nothing beyond the weekly
/// report, fix it and tell, or make a plan and ask first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Lane {
    Quiet,
    Tell,
    Ask,
}

/// When an archived problem comes back by itself.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "until", rename_all = "lowercase")]
#[ts(export)]
pub enum Archive {
    /// When it happens clearly more than before: over twice the daily rate it had, at least 10
    /// times in a day.
    Worse {
        #[serde(default)]
        rate: f64,
    },
    /// After this time.
    Time {
        #[ts(type = "number")]
        at: i64,
    },
    /// When the count reaches this.
    Count {
        #[ts(type = "number")]
        count: i64,
    },
    Forever,
}

/// Vakten's reading of a problem: the box on top of its page.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Note {
    #[serde(default)]
    pub what: String,
    #[serde(default)]
    pub cause: String,
    #[serde(default)]
    pub plan: String,
    /// What it does next, or what it waits for.
    #[serde(default)]
    pub next: String,
    /// The fix contract's first part: how to make it happen again.
    #[serde(default)]
    pub repro: String,
    /// The fix contract's last part: what has to be true for the fix to count as done.
    #[serde(default)]
    pub done: String,
}

/// The pull request of the thread working on a problem.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProblemPr {
    pub number: u32,
    pub url: String,
    /// "open", "draft", "merged" or "closed".
    pub state: String,
}

/// Something that happened to a problem, and who did it: "you", "Vakten", a thread's title, "Hub".
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Activity {
    #[ts(type = "number")]
    pub ts: i64,
    pub by: String,
    /// seen, stamped, status, thread, pr, merged, regressed, escalating, note, rule.
    pub kind: String,
    pub text: String,
}

/// One error or warning, grouped: every hit with the same fingerprint counts here.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Problem {
    #[ts(type = "number")]
    pub id: i64,
    /// The project it belongs to: "Hub" for Hub's own, else the source's name.
    pub project: String,
    /// What sent it: "hubd", "app", "thread", or "sentry" for a project's SDK.
    pub source: String,
    pub level: Level,
    pub title: String,
    /// Where in the code, when known: "file in function, line N".
    pub culprit: Option<String>,
    /// The folder to work on it in, when known.
    pub folder: Option<String>,
    /// The machine it happened on.
    pub machine: String,
    #[ts(type = "number")]
    pub first_seen: i64,
    #[ts(type = "number")]
    pub last_seen: i64,
    #[ts(type = "number")]
    pub count: i64,
    pub status: ProblemStatus,
    /// It came back after being resolved.
    pub regressed: bool,
    /// The thread started on it.
    pub thread: Option<String>,
    /// Hits on each of the last 7 days, oldest first.
    #[ts(type = "number[]")]
    pub days: Vec<i64>,
    pub prio: Option<Prio>,
    pub lane: Option<Lane>,
    /// bug, config, environment, external, flaky, performance, security, noise, expected.
    pub category: Option<String>,
    /// How sure Vakten is of its reading: low, medium, high.
    pub confidence: Option<String>,
    /// Whether a change in code can fix it: yes, no, unknown.
    pub fixable: Option<String>,
    pub note: Option<Note>,
    /// Who set the stamps last: "you" or "Vakten".
    pub stamped_by: Option<String>,
    pub archive: Option<Archive>,
    #[ts(type = "number | null")]
    pub resolved_at: Option<i64>,
    /// "you", "Vakten", or the pull request that fixed it ("PR #241").
    pub resolved_by: Option<String>,
    /// Resolved for the releases it was seen in: it only comes back from another release.
    pub next_release: bool,
    /// The pull request of the thread on it.
    pub pr: Option<ProblemPr>,
    /// Merged into this problem; its hits count there now.
    #[ts(type = "number | null")]
    pub merged_into: Option<i64>,
    /// Happening clearly more than it used to.
    pub escalating: bool,
    /// First seen in the last 7 days and nobody looked yet.
    pub new: bool,
    /// The user's to decide or look at: asks first, came back, urgent with nobody on it, or
    /// unsorted while Vakten is off.
    pub needs_you: bool,
}

/// A line in a stack, innermost (where it broke) last, as Sentry orders them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Frame {
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub function: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
    /// The line of code itself.
    #[serde(default)]
    pub code: Option<String>,
    /// The project's own code, not a library's.
    #[serde(default)]
    pub own: bool,
}

/// Something that happened before the problem: a log line, a click, a request.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Crumb {
    #[serde(default)]
    #[ts(type = "number | null")]
    pub ts: Option<i64>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    pub message: String,
}

/// One time a problem happened.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Hit {
    #[serde(default)]
    #[ts(type = "number")]
    pub ts: i64,
    pub message: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub frames: Vec<Frame>,
    /// A stack as plain text, when it did not come as frames (a JavaScript `Error.stack`).
    #[serde(default)]
    pub trace: Option<String>,
    #[serde(default)]
    pub crumbs: Vec<Crumb>,
    #[serde(default)]
    pub release: Option<String>,
    #[serde(default)]
    pub environment: Option<String>,
    /// Anything else the sender said (tags, the thread, the engine).
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub extra: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProblemDetail {
    pub problem: Problem,
    /// The latest hits, newest first.
    pub hits: Vec<Hit>,
    /// Newest first.
    pub activity: Vec<Activity>,
    /// Hits per release and per machine, most first.
    pub tags: Vec<ProblemTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProblemTag {
    pub key: String,
    pub value: String,
    #[ts(type = "number")]
    pub count: i64,
}

/// A report in Hub's own format (`POST /api/problems/report`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Report {
    #[serde(default)]
    #[ts(optional)]
    pub project: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub source: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub level: Option<Level>,
    #[serde(default)]
    #[ts(optional)]
    pub folder: Option<String>,
    /// Groups hits by these words instead of by stack or message.
    #[serde(default)]
    #[ts(optional)]
    pub fingerprint: Option<Vec<String>>,
    #[serde(flatten)]
    pub hit: Hit,
}

/// A program that reports with a Sentry SDK, and the DSN it uses.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Source {
    /// Sentry's project id, a number in the DSN's path.
    pub id: u32,
    pub name: String,
    pub key: String,
    #[serde(default)]
    pub folder: Option<String>,
    /// To put in the program's settings: `SENTRY_DSN` or `sentry_sdk.init(dsn=…)`.
    #[serde(default)]
    pub dsn: String,
    /// It should send something at least this often; when it does not, that is a problem.
    #[serde(default)]
    pub quiet_after_hours: Option<u32>,
    /// When it last sent anything.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub last_heard: Option<i64>,
    /// Where to send OpenTelemetry (OTLP over HTTP, JSON) instead: logs at `<otlp>/v1/logs`,
    /// with the key as `Authorization: Bearer <key>`.
    #[serde(default)]
    pub otlp: String,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct NewSource {
    pub name: String,
    #[serde(default)]
    #[ts(optional)]
    pub folder: Option<String>,
    /// For an existing source: change when it counts as quiet (0 for never).
    #[serde(default)]
    #[ts(optional)]
    pub quiet_after_hours: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProblemFilter {
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<ProblemStatus>,
    /// The lowest level to show.
    #[serde(default)]
    #[ts(optional)]
    pub level: Option<Level>,
    #[serde(default)]
    #[ts(optional)]
    pub project: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub source: Option<String>,
    /// Words in the title or where it happened.
    #[serde(default)]
    #[ts(optional)]
    pub query: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// Which list: needs_you, vakten, open (default), fixed, back, archived, resolved, all.
    #[serde(default)]
    #[ts(optional)]
    pub view: Option<View>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum View {
    /// Asks first, came back, urgent with nobody on it, or unsorted while Vakten is off.
    NeedsYou,
    /// A thread is on it.
    Vakten,
    Open,
    /// Resolved in the last 7 days.
    Fixed,
    /// Came back after being resolved.
    Back,
    Archived,
    Resolved,
    /// Quiet sources: missed check-ins, silent programs, machines gone, failed routine runs.
    Quiet,
    /// Slower releases.
    Slower,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Resolve {
    /// Any later hit brings it back.
    Now,
    /// Hits from the releases it was seen in do not bring it back; another release does.
    NextRelease,
}

/// A change to a problem. Every field is optional; only what is given changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProblemPatch {
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<ProblemStatus>,
    #[serde(default)]
    #[ts(optional)]
    pub resolve: Option<Resolve>,
    #[serde(default)]
    #[ts(optional)]
    pub archive: Option<Archive>,
    #[serde(default)]
    #[ts(optional)]
    pub prio: Option<Prio>,
    #[serde(default)]
    #[ts(optional)]
    pub lane: Option<Lane>,
    #[serde(default)]
    #[ts(optional)]
    pub category: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub confidence: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub fixable: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub note: Option<Note>,
    /// Looked at: no longer "new".
    #[serde(default)]
    #[ts(optional)]
    pub reviewed: Option<bool>,
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub merge_into: Option<i64>,
    #[serde(default)]
    #[ts(optional)]
    pub unmerge: Option<bool>,
    /// Why, for the activity log.
    #[serde(default)]
    #[ts(optional)]
    pub why: Option<String>,
}
