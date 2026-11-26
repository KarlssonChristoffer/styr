//! Model types: timing.

use super::*;

/// A program serving models on a machine itself, such as Ollama or LM Studio.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LocalServer {
    pub id: String,
    pub label: String,
    /// Where it answers on that machine.
    pub url: String,
    pub running: bool,
    pub models: Vec<String>,
    /// Where to get it, when it is not running.
    pub get_at: String,
}

/// The local model servers found on one machine (`GET /api/local-models`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LocalModels {
    pub servers: Vec<LocalServer>,
}

/// How long an engine took to come up, measured where it happens: from the core starting it to
/// the process answering `initialize`, then to `session/new` (or the resumed session) being
/// open, and in all. Milliseconds, in `SessionStarted.start_ms` (`timings.rs`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StartTiming {
    #[ts(type = "number")]
    pub initialize: i64,
    #[ts(type = "number")]
    pub session: i64,
    #[ts(type = "number")]
    pub total: i64,
}

/// One tool call of a turn and how long it took, when it finished.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolTiming {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub ms: Option<i64>,
    pub failed: bool,
}

/// Where the time of one turn went, read from the event log (`timings.rs`). All in milliseconds;
/// the parts after `begin_ms` count from the turn starting, and a part the turn never reached is
/// left out.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TurnTiming {
    /// The seq of the user message the turn answers.
    #[ts(type = "number")]
    pub prompt: i64,
    /// When the message was sent.
    #[ts(type = "number")]
    pub at: i64,
    /// From the message to the engine's session being open; only when an engine came up for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub engine_ms: Option<i64>,
    /// What the engine start was made of, when it was measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start: Option<StartTiming>,
    /// From the message to the turn starting (the engine start, and any queue before it).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub begin_ms: Option<i64>,
    /// To the first model call answering.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub model_ms: Option<i64>,
    /// To the first tool call.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub tool_ms: Option<i64>,
    /// To the first words of the answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub text_ms: Option<i64>,
    /// To the turn ending.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub done_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stop: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    /// Tokens of context at the first model call, before the answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub context: Option<i64>,
    /// Slash commands the engine listed before the turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub commands: Option<i64>,
    pub tools: Vec<ToolTiming>,
    /// The tokens the engine said the turn used (`TurnEndData.usage`); left out when it said none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage: Option<TurnUsage>,
}

/// The median and the slow end (90th percentile) of a part, in milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Spread {
    #[ts(type = "number")]
    pub p50: i64,
    #[ts(type = "number")]
    pub p90: i64,
}

/// The first answers of one kind of thread on one engine, summed up.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimingGroup {
    pub engine: String,
    /// "chat" (a folder under `chats/`) or "dev".
    pub kind: String,
    #[ts(type = "number")]
    pub n: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub engine_ms: Option<Spread>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_ms: Option<Spread>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text_ms: Option<Spread>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub done_ms: Option<Spread>,
}

/// One thread's first answer, for the list under the summary.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadTiming {
    pub thread: ThreadId,
    pub title: String,
    pub engine: String,
    pub kind: String,
    pub turn: TurnTiming,
}

/// `GET /api/timings`: how fast the first answers of the latest threads came (`timings.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimingSummary {
    pub groups: Vec<TimingGroup>,
    pub recent: Vec<ThreadTiming>,
}

/// One part of what was sent to an AI, as `package` built it: the person's words, the Hub notes
/// before them, the conversation handed over, and so on. Sizes only: the text itself is rebuilt
/// from its sources (the thread's log, the folder), never kept a second time.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SlipSection {
    /// What the part is: `you`, `handover`, `rolled_back`, `clash`, `design`, `lost_session`,
    /// `prompt` for a one-shot that is not split yet.
    pub id: String,
    pub tokens: u32,
    pub chars: u32,
    /// The part was cut to fit its budget. Nothing cuts yet; recipes will.
    #[serde(default)]
    pub cut: bool,
}

/// The slip for one package sent to an AI (`package/`): who got it, what it was for, how big
/// each part was. Written when it goes, so it is there when something fails afterwards.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Slip {
    pub id: String,
    #[ts(type = "number")]
    pub at: i64,
    /// What it was for: `message` (you wrote it), `switch`, `rewind`, `moved` (a conversation
    /// handed to a fresh session), `hub:<mark>` (a message Hub wrote, marked as `review` or
    /// `quota`), or a one-shot's name (`gist`, `tidy`, `judge_answer`).
    pub purpose: String,
    /// The thread it went to, when it went to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    /// The user message it answers in that thread's log: the pointer to rebuild the text from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub seq: Option<i64>,
    /// The engine that got it, by the name Hub shows (a one-shot has the harness, not the name).
    pub engine: String,
    /// The engine's own session it went into: the key that joins this slip to what the engine
    /// counted (its token log) and to Aftermath's capture. Empty when it was not known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// All of it, counted as one token for about four characters until an engine reports the real
    /// figure (`estimated` says which).
    pub tokens: u32,
    pub estimated: bool,
    pub sections: Vec<SlipSection>,
    /// Files and pictures that went with it, not counted in `tokens`.
    pub attachments: u32,
    /// The recipe that built it, with its version. `legacy` is how it was built before recipes.
    pub recipe: String,
    /// A recipe failed and the old way was used instead.
    #[serde(default)]
    pub fallback: bool,
    /// The first words, for a one-shot that has no thread to rebuild it from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub head: String,
}

/// What one purpose took on one day.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageTotal {
    pub purpose: String,
    pub calls: u32,
    #[ts(type = "number")]
    pub tokens: u64,
}

/// One day of `PackageSummary`, newest first in the list.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageDay {
    /// `YYYY-MM-DD`, in Hub's time zone.
    pub day: String,
    pub purposes: Vec<PackageTotal>,
}

/// `GET /api/package/summary`: what Hub sent to AI, per day and purpose.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PackageSummary {
    pub days: Vec<PackageDay>,
}

/// One use of a part of Hub, as the page reports it. `feature` is a short name such as
/// `thread.composer`, `area` the page it sits on. Never anything typed or opened: the name is
/// checked on arrival (`uselog::clean`) and a line that does not look like one is dropped.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UseHit {
    #[ts(type = "number")]
    pub ts: i64,
    pub feature: String,
    pub area: String,
    /// The feature used just before this one, for the flows.
    #[serde(default)]
    pub prev: String,
    /// How long it took, for things that wait.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub took_ms: Option<i64>,
}

/// How much one feature was used in the range.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UseCount {
    pub feature: String,
    pub area: String,
    #[ts(type = "number")]
    pub count: i64,
    #[ts(type = "number")]
    pub last_ts: i64,
    /// Median of the times reported, when any were.
    #[ts(type = "number | null")]
    pub median_ms: Option<i64>,
}

/// How often one feature came right after another.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UseFlow {
    pub from: String,
    pub to: String,
    #[ts(type = "number")]
    pub count: i64,
}

/// What `/api/uselog` answers: counts per feature, the flows between them, and whether the log
/// is switched off.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UseReport {
    pub off: bool,
    pub features: Vec<UseCount>,
    pub flows: Vec<UseFlow>,
    /// Rows in the log in all, and the moment of the oldest.
    #[ts(type = "number")]
    pub total: i64,
    #[ts(type = "number | null")]
    pub since: Option<i64>,
}
