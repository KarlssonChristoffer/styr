//! Payloads of the kinds about questions, the queue and what stops or restarts a turn:
//! `permission_resolved`, `permission_auto`, `slept`, `status`, `queue`, `paused` and `restart`.
//! The rules for a payload struct are in `event_data.rs`.

use super::*;

/// `permission_resolved`: a permission question was answered or closed. `option_id` is always
/// written; `null` means cancelled or closed without an answer. One flag may say how it closed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PermissionResolvedData {
    /// The `permission_request`'s id.
    #[serde(default, deserialize_with = "lenient")]
    pub id: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub option_id: Option<Option<String>>,
    /// The card was from before a restart; the agent no longer waited for it (`Hub::answer`).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stale: Option<bool>,
    /// The answer woke a session that slept on the question (`Hub::wake_from_sleep`): the thread
    /// works again, however many other questions are open.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub woke: Option<bool>,
    /// Closed when the core started and found it open (`restart.rs`); the status is set there.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub restart: Option<bool>,
    /// Closed because the thread moved to another machine (`moving.rs`).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub moved: Option<bool>,
}

impl PermissionResolvedData {
    /// An answer to question `id`.
    pub fn answer(id: impl Into<String>, option_id: Option<String>) -> Self {
        Self { id: id.into(), option_id: Some(option_id), ..Default::default() }
    }

    /// Closed where the status is set by whoever closed it, so the answer itself changes nothing.
    pub fn closed_elsewhere(&self) -> bool {
        self.restart == Some(true) || self.moved == Some(true)
    }
}

/// `permission_auto`: Hub answered a permission question itself, by the user's permission settings
/// (`permit.rs`), so it never reached them. `tool_call` is the engine protocol's own JSON, `why`
/// says which setting in words the user sees, `option_id` is `null` when there was no option to pick.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PermissionAutoData {
    #[serde(default)]
    #[ts(type = "unknown")]
    pub tool_call: serde_json::Value,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub option_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient")]
    pub allowed: bool,
    #[serde(default, deserialize_with = "lenient")]
    pub why: String,
}

/// `slept`: the engine was stopped while these permission questions waited for the user, so it
/// takes no memory meanwhile. The answer wakes it (`Hub::sleep_waiting`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SleptData {
    #[serde(default, deserialize_with = "lenient_items")]
    pub ids: Vec<String>,
}

/// `status`: the thread's status changed for a reason no engine event carries. `status` is a
/// `ThreadStatus` word, kept as written (a newer Hub may know more words); `why` says what
/// happened in a word ("core_started", "engine_stopped", "cloud_prompt", "restart", ...).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatusData {
    #[serde(default, deserialize_with = "lenient")]
    pub status: String,
    #[serde(default, deserialize_with = "lenient")]
    pub why: String,
}

impl StatusData {
    pub fn new(status: ThreadStatus, why: &str) -> Self {
        let word = serde_json::to_value(status).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
        Self { status: word, why: why.to_owned() }
    }

    /// The status it names, when this version knows the word.
    pub fn to(&self) -> Option<ThreadStatus> {
        serde_json::from_value(serde_json::Value::String(self.status.clone())).ok()
    }
}

/// `queue`: user message `seq` waited behind a running turn and was changed before the engine
/// took it. `action` is "edited" (with the new `text`), "removed", "dropped" (the engine kept
/// failing, so it was let go) or "steered" (the running turn was stopped so it goes next).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QueueData {
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub seq: i64,
    #[serde(default, deserialize_with = "lenient")]
    pub action: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
}

impl QueueData {
    pub fn new(seq: i64, action: &str) -> Self {
        Self { seq, action: action.to_owned(), text: None }
    }

    /// The message never goes to the engine.
    pub fn gone(&self) -> bool {
        matches!(self.action.as_str(), "removed" | "dropped")
    }
}

/// `paused`: the panic button stopped this thread's turn (`halt.rs`), pressed on machine `by`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PausedData {
    #[serde(default, deserialize_with = "lenient")]
    pub by: String,
}

/// `restart`: the turn for user message `prompt` was cut off and Hub found it when it started
/// (or, `by: "move"`, when the thread arrived on this machine). `by` is "update", "restart",
/// "crash", "pause" or "move"; `queued` are the messages sent meanwhile; `state` is "continued"
/// or "stopped", and then `why` is "off" (`resume_turns` is off) or "loop".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RestartData {
    #[serde(default, deserialize_with = "lenient")]
    pub by: String,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub prompt: i64,
    #[serde(default, deserialize_with = "lenient_items")]
    #[ts(type = "Array<number>")]
    pub queued: Vec<i64>,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub why: Option<String>,
}
