//! Payloads of what Hub checks after a turn and tells the agent about: `clash`, `leak`, `customs`,
//! `proof` and `docs`. The rules for a payload struct are in `event_data.rs`.

use super::*;

/// `clash`: thread `other` (titled `other_title`) changed `path` (absolute) too, within the last
/// few hours. Logged in both threads, once per file and pair.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ClashData {
    #[serde(default, deserialize_with = "lenient")]
    pub path: String,
    #[serde(default, deserialize_with = "lenient")]
    pub other: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub other_title: Option<String>,
}

/// One secret a turn wrote: where, what kind, and only a hint of the value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LeakFinding {
    #[serde(default, deserialize_with = "lenient")]
    pub path: String,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub line: u64,
    #[serde(default, deserialize_with = "lenient")]
    pub kind: String,
    #[serde(default, deserialize_with = "lenient")]
    pub hint: String,
}

/// `leak`: lines the turn that ended at `seq` added hold what looks like a secret. `sent` says
/// the findings went to the agent as a user message marked `leak: seq`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LeakData {
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub seq: i64,
    #[serde(default, deserialize_with = "lenient_items")]
    pub findings: Vec<LeakFinding>,
    #[serde(default, deserialize_with = "lenient")]
    pub sent: bool,
}

/// One thing at Customs: a dependency (`kind`, `name`) found in `path`, with its `status` on
/// Customs' list ("cleared", "held" or "refused"). A declared one has an empty `path` and the
/// agent's `why`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomsFound {
    #[serde(default, deserialize_with = "lenient")]
    pub kind: String,
    #[serde(default, deserialize_with = "lenient")]
    pub name: String,
    #[serde(default, deserialize_with = "lenient")]
    pub path: String,
    #[serde(default, deserialize_with = "lenient")]
    pub status: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub why: Option<String>,
}

/// `customs`: the turn that ended at `seq` brought in something that had not cleared Customs
/// (`found`); `sent` says a refusal went to the agent as a user message marked `customs: seq`
/// (or that nothing needed to). Or, with `declared`, the agent declared something mid-turn with
/// `customs_declare` and it is held.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomsData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub seq: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub declared: Option<bool>,
    #[serde(default, deserialize_with = "lenient_items")]
    pub found: Vec<CustomsFound>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sent: Option<bool>,
}

/// `proof`: whether the turn whose `turn_end` is `reply` showed its code working, after it
/// changed `files` files that need it. `kind` ("tests" or "screenshot") and `command` are what
/// showed it, `null` when nothing did; `hidden` says a run that exited cleanly printed failures.
/// `state` is "shown", "failed", "asked" (Hub asked for proof with a user message marked
/// `proof: reply`), "missing" (asked and still none) or "skipped"; `why` says what led to it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProofData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub reply: Option<i64>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub files: Option<Option<i64>>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub kind: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub command: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hidden: Option<bool>,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub why: Option<String>,
}

/// A doc section that covers code a turn changed: lines `first` to `last` of `doc` under
/// `heading` (`null` for a whole doc), and the changed `files` it covers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DocsSection {
    #[serde(default, deserialize_with = "lenient")]
    pub doc: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub heading: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub first: u64,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub last: u64,
    #[serde(default, deserialize_with = "lenient_items")]
    pub files: Vec<String>,
}

/// `docs`: after the turn whose `turn_end` is `reply` changed code that doc `sections` cover
/// (`covers.rs`). `state` is "updated" (it changed the doc), "checked" (it opened the doc or asked
/// `docs_for`), "asked" (it did neither, and Hub asked it to in a user message marked
/// `docs: reply` or, with a proof request, `proof: reply`), "skipped" (`why` "reverted") or "ignored" (the answer to a request that did not look, or with
/// `why` "asked_before" a section it was asked about before).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DocsData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub reply: Option<i64>,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub why: Option<String>,
    #[serde(default, deserialize_with = "lenient_items")]
    pub sections: Vec<DocsSection>,
}

impl DocsData {
    pub fn new(reply: i64, state: &str, sections: Vec<DocsSection>) -> Self {
        Self { reply: Some(reply), state: state.to_owned(), why: None, sections }
    }

    pub fn why(mut self, why: &str) -> Self {
        self.why = Some(why.to_owned());
        self
    }
}
