//! Payloads of work the thread gave to someone else: `child` (a side job), `handoff_offer`,
//! `handed_off` and `handoff_result` (one of the user's own tools), `worker_run` (a run such a
//! tool does) and `auto` (the old Auto, read only). The rules for a payload struct are in
//! `event_data.rs`.

use super::*;

/// `child`, in the parent: the side job `id` (a thread, titled `title`) of `kind` with `context`
/// runs on `engine` with `model` (`null`: the engine's own; the oldest have no `model`). `by` is
/// "agent". Logged as `state` "working", then again with the same fields as "done" (with the
/// `reply` and the `images` it drew) or "failed" (with `error`). `quiet` on a "done" one: the
/// parent's agent was not woken for it and hears of it with the next message.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChildData {
    #[serde(default, deserialize_with = "lenient")]
    pub id: String,
    #[serde(default, deserialize_with = "lenient")]
    pub title: String,
    #[serde(default, deserialize_with = "lenient")]
    pub kind: String,
    #[serde(default, deserialize_with = "lenient")]
    pub context: String,
    #[serde(default, deserialize_with = "lenient")]
    pub engine: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub model: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient")]
    pub by: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reply: Option<String>,
    #[serde(default, deserialize_with = "some_items", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub images: Option<Vec<String>>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub quiet: Option<bool>,
}

/// `auto`: the old Auto, which chose who answered each message. No Hub writes it any more; old
/// logs hold three shapes: `on` with `by` (turned on or off), `ask`, `engine` and `why` (a
/// message went to a side job), and those with `from`, `to`, `from_name` and `to_name` (the
/// thread moved to another model).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AutoData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub on: Option<bool>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub by: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ask: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub engine: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub why: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_name: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_name: Option<String>,
}

/// `handoff_offer`: a card offering to hand the work to the user's own tool `tool` in `folder`,
/// with the tool's `label` and what it says `about` itself (`null` when nothing), whether it can
/// build (`can_build`) and the agent's reason `why`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HandoffOfferData {
    #[serde(default, deserialize_with = "lenient")]
    pub tool: String,
    #[serde(default, deserialize_with = "lenient")]
    pub folder: String,
    #[serde(default, deserialize_with = "lenient")]
    pub label: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub about: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient")]
    pub can_build: bool,
    #[serde(default, deserialize_with = "lenient")]
    pub why: String,
}

/// `handed_off`: the work titled `title` went to `tool` in `folder` (`ok`), or did not (`ok`
/// false); `output` is what the tool or Hub said. `work` is the tool's own name for it, which
/// following it needs; a handoff that never reached the tool has none.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HandedOffData {
    #[serde(default, deserialize_with = "lenient")]
    pub tool: String,
    #[serde(default, deserialize_with = "lenient")]
    pub title: String,
    #[serde(default, deserialize_with = "lenient")]
    pub ok: bool,
    #[serde(default, deserialize_with = "lenient")]
    pub output: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub folder: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub work: Option<String>,
}

/// `handoff_result`: the work titled `title` that went to `tool` ended as `state` ("done" or
/// "stopped"). The rest is what the tool said of it, each `null` when it did not: whether its
/// tests proved it (`proven`), the `model`, the `cost`, its own words (`said`) and a `link`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HandoffResultData {
    #[serde(default, deserialize_with = "lenient")]
    pub tool: String,
    #[serde(default, deserialize_with = "lenient")]
    pub title: String,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "boolean | null")]
    pub proven: Option<Option<bool>>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub model: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub cost: Option<Option<f64>>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub said: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub link: Option<Option<String>>,
}

/// `worker_run`: how a run of one of the user's tools stands, the whole `WorkerRun` each time,
/// and the `lines` that are new since the event before. Read strictly, as `WorkerRun` always
/// was: a payload that is not a whole run is no run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkerRunData {
    #[serde(flatten)]
    pub run: WorkerRun,
    #[serde(default, deserialize_with = "lenient_items")]
    pub lines: Vec<WorkerLine>,
}
