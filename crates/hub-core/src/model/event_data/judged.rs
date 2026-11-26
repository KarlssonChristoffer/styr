//! Payloads of what another model was asked to judge beside the thread's own work: `review`,
//! `design`, `task`, `focus` and `reading`. The rules for a payload struct are in `event_data.rs`.

use serde::Serializer;
use serde::ser::SerializeMap;

use super::*;

/// One part of a review: the `files` in it, whether it needs a look (`review`, and `why` not) and
/// how far it is: `state` "waiting", "skipped", "running", "done" (with `verdict` and `text`) or
/// "failed" (with `error`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewChunk {
    #[serde(default, deserialize_with = "lenient")]
    pub title: String,
    #[serde(default, deserialize_with = "lenient_items")]
    pub files: Vec<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub review: bool,
    #[serde(default, deserialize_with = "lenient")]
    pub why: String,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub verdict: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// `review`: a second opinion by `model` on what the thread changed between commits `base` and
/// `head` (`files`), asked by `by` ("you", or "agent" with its `reason`) when the log stood at
/// `seq`. Every step is logged again under the same `id`: `state` "running" while the `chunks`
/// are read, then "done" (with the whole `text`, and `sent` when it went back to the agent) or
/// "failed" (with `error`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewData {
    #[serde(default, deserialize_with = "lenient")]
    pub id: String,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient")]
    pub model: String,
    #[serde(default, deserialize_with = "lenient_items")]
    pub files: Vec<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub base: String,
    /// Missing in a review that never got as far as a diff; such a one is no place to start the
    /// next review from.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub head: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub seq: i64,
    #[serde(default, deserialize_with = "lenient")]
    pub by: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub reason: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient_items")]
    pub chunks: Vec<ReviewChunk>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sent: Option<bool>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
}

/// One check of a design review, run by `model` (`null`: the engine's own): `state` "running",
/// "done" (with `verdict` and `text`) or "failed" (with `error`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesignCheck {
    #[serde(default, deserialize_with = "lenient")]
    pub name: String,
    #[serde(default, deserialize_with = "lenient")]
    pub title: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub model: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub verdict: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// `design`: design mode. Three shapes:
/// - `on` with `by` ("you", "start"): the mode was turned on or off;
/// - `strayed` with `seq`: the turn that ended at `seq` changed these files outside the design
///   folder;
/// - `review` (an id) with `state`, `doc`, `checks` and `by`: a review of the design document,
///   logged again at every step. It ends "done" (with `ready`, `verdict` and `sent`) or "failed"
///   (with `error`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesignData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub on: Option<bool>,
    #[serde(default, deserialize_with = "some_items", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub strayed: Option<Vec<String>>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub seq: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub review: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub doc: Option<String>,
    #[serde(default, deserialize_with = "some_items", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub checks: Option<Vec<DesignCheck>>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub by: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ready: Option<bool>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub verdict: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sent: Option<bool>,
}

impl DesignData {
    /// The mode turned on or off by `by`.
    pub fn switched(on: bool, by: &str) -> Self {
        Self { on: Some(on), by: Some(by.to_owned()), ..Default::default() }
    }
}

/// `task`: a side job Hub (`by`) gave another model while the thread waited, shown as a note.
/// Today only `task` "vetting": `engine` and `model` look at the dependency `name` of `kind`.
/// Logged twice under one `id`: `state` "working", then "done" (with the `verdict` "cleared",
/// "held" or "refused" and its `reason`) or "failed" (`reason` is the error), with the `ms` it
/// took. When the checks alone settled it no model ran: `engine` is "" and `model` `null`.
///
/// The start wrote `state` before `by`, the end `by` and `ms` before `state`.
// So `Serialize` is written by hand below instead of derived.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct TaskData {
    #[serde(default, deserialize_with = "lenient")]
    pub id: String,
    #[serde(default, deserialize_with = "lenient")]
    pub task: String,
    #[serde(default, deserialize_with = "lenient")]
    pub name: String,
    #[serde(default, deserialize_with = "lenient")]
    pub kind: String,
    #[serde(default, deserialize_with = "lenient")]
    pub engine: String,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub model: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient")]
    pub by: String,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional, type = "number")]
    pub ms: Option<i64>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub verdict: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub reason: Option<String>,
}

impl Serialize for TaskData {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        fn put<M: SerializeMap, T: Serialize>(map: &mut M, key: &str, value: &Option<T>) -> Result<(), M::Error> {
            value.as_ref().map_or(Ok(()), |v| map.serialize_entry(key, v))
        }
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("task", &self.task)?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("kind", &self.kind)?;
        map.serialize_entry("engine", &self.engine)?;
        put(&mut map, "model", &self.model)?;
        // The end: `by`, `ms`, then `state`. The start: `state`, then `by`.
        if self.ms.is_some() {
            map.serialize_entry("by", &self.by)?;
            put(&mut map, "ms", &self.ms)?;
            map.serialize_entry("state", &self.state)?;
        } else {
            map.serialize_entry("state", &self.state)?;
            map.serialize_entry("by", &self.by)?;
        }
        put(&mut map, "verdict", &self.verdict)?;
        put(&mut map, "reason", &self.reason)?;
        map.end()
    }
}

/// `focus`: the thread's first message does not fit (`fits` is false; a thread that fits logs
/// nothing) the active stage `stage` of the folder's focus file, whose heading is on `line`; `why`
/// is the model's reason.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FocusData {
    #[serde(default, deserialize_with = "lenient")]
    pub fits: bool,
    #[serde(default, deserialize_with = "lenient")]
    pub stage: String,
    #[serde(default, deserialize_with = "lenient")]
    pub line: u32,
    #[serde(default, deserialize_with = "lenient")]
    pub why: String,
}

/// `reading`: a decision model (`connection`) read something for the `feature` and answered in
/// `ms` with this `confidence` (0 to 1); `acted` says Hub went by it. "turn": where the turn that
/// ended at `seq` left the work (`state`). "topic": the message `seq` starts something new
/// (`state` "new"). "focus": the thread works towards the goal or idea `text` of `stage` (`line`
/// is where a goal stands in the focus file, `null` for an idea).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReadingData {
    #[serde(default, deserialize_with = "lenient")]
    pub feature: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub seq: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stage: Option<String>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub line: Option<Option<u32>>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub confidence: f64,
    #[serde(default, deserialize_with = "lenient")]
    pub connection: String,
    #[serde(default, deserialize_with = "lenient")]
    pub ms: u32,
    #[serde(default, deserialize_with = "lenient")]
    pub acted: bool,
}
