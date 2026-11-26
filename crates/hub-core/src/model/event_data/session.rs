//! Payloads of the kinds about the engine's session and what it is set to: `session_started`,
//! `session_state`, `setting`, `quota_switch`, `quota_wait`, `engine_switch` and `sign_in_lost`
//! (`unattended` is `Unattended` in `threads_io.rs`, which a new thread is also asked with; it
//! reads strictly, the one exception to the rule that every field is lenient).
//! The rules for a payload struct are in `event_data.rs`.

use super::*;

/// `session_started`: an engine started or resumed the thread's session, so the `update`s after it
/// can be read knowing which protocol and agent wrote them. `protocol_version` and `agent` are the
/// engine protocol's own JSON (`agent` is `null` when the engine did not say who it is).
/// `start_ms` is how long the engine took to come up (`timings.rs`); older logs lack it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionStartedData {
    #[serde(default, deserialize_with = "lenient")]
    pub driver: String,
    #[serde(default, deserialize_with = "lenient")]
    pub command: String,
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub protocol_version: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub agent: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "lenient")]
    pub session_id: String,
    /// False for a fresh session: it has seen only the messages that waited for it. Missing in a
    /// payload reads as `None`, which is not "fresh".
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resumed: Option<bool>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_ms: Option<StartTiming>,
}

/// `session_state`: what can be set on the session (model, effort, permission mode, ...) and what
/// a prompt may contain, as the engine protocol's own JSON. Sent when a session starts (with
/// `prompt_capabilities`) and after a setting changes (without); surfaces keep the latest of each
/// field. `config_options` and `modes` are `null` when the engine has none.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionStateData {
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub config_options: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub modes: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub prompt_capabilities: Option<serde_json::Value>,
}

impl SessionStateData {
    /// The value of the option in category "model", when the session has one.
    pub fn model(&self) -> Option<&str> {
        let options = self.config_options.as_ref()?.as_array()?;
        options.iter().find(|o| o["category"] == "model")?["currentValue"].as_str()
    }
}

/// `setting`: `config_id` was set to `value` (a string, or a boolean for an on/off setting). It
/// is applied to the running session, or when the engine next starts. `by` says who chose when it
/// was not the user: "name" (a pick by family name, looked up), "default" (a new thread's mode),
/// "prefer" (an unattended thread's words) or "auto" (the old Auto).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SettingData {
    #[serde(default, deserialize_with = "lenient")]
    pub config_id: String,
    #[serde(default)]
    #[ts(type = "string | boolean")]
    pub value: serde_json::Value,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub by: Option<String>,
}

impl SettingData {
    pub fn new(config_id: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        Self { config_id: config_id.into(), value: value.into(), by: None }
    }

    pub fn by(mut self, by: &str) -> Self {
        self.by = Some(by.to_owned());
        self
    }

    /// The model it chose, when it is a choice of model.
    pub fn model(&self) -> Option<&str> {
        (self.config_id == "model").then(|| self.value.as_str()).flatten()
    }
}

/// `quota_switch`: the quota guard moved the thread to a cheaper model because plan limit `limit`
/// was at `percent`. `from` is the session's value before (`null` when it had none), `to` the
/// model it moved to. A `setting` event with the model comes first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QuotaSwitchData {
    #[serde(default, deserialize_with = "lenient")]
    pub limit: String,
    #[serde(default)]
    #[ts(type = "number")]
    pub percent: serde_json::Value,
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub from: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "lenient")]
    pub to: String,
}

/// `quota_wait`: a turn stopped on a plan limit and Hub carries the thread on when it resets.
/// `state` "waiting": `seq` is the user message whose turn stopped (`null` when none was found),
/// `at` when Hub will continue (ms), `known` false when that is only a guess, `limit` the limit's
/// label and `resets` the engine's words for when (both `null` when the plan did not say),
/// `message` what the engine said, `tries` how many times it was resumed already, `offer` another
/// subscription the thread can go on with meanwhile. Later events name the wait's seq as `wait`:
/// "resumed" and "cancelled" with `by` ("hub", "you" or "message"), "switched" with `to`, `from`
/// and `by`. "gave_up" (stopped again after several tries) has `seq`, `message` and `tries`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QuotaWaitData {
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub seq: Option<Option<i64>>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub at: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub known: Option<bool>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub limit: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub resets: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub tries: Option<u64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub offer: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub wait: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub by: Option<String>,
}

impl QuotaWaitData {
    /// What ends or moves wait `wait`: `state` is "resumed", "cancelled" or "switched".
    pub fn after(state: &str, wait: i64, by: &str) -> Self {
        Self { state: state.to_owned(), wait: Some(wait), by: Some(by.to_owned()), ..Default::default() }
    }
}

/// `engine_switch`: the thread moved from engine `from` to engine `to`. The old session is left
/// behind; the next message carries the conversation so far to the new engine, marked
/// `handoff: seq` with this event's seq. Settings logged before it belonged to `from`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineSwitchData {
    #[serde(default, deserialize_with = "lenient")]
    pub from: String,
    #[serde(default, deserialize_with = "lenient")]
    pub to: String,
}

/// `sign_in_lost`: the turn failed because the engine's sign-in on `machine` is gone (its error,
/// `message`, matched the engine's `auth_phrases`). `kept` says it was the long-lived sign-in Hub
/// keeps (`signin.rs`), `can_keep` that Hub can keep one for this engine, `keychain` that the
/// engine's own login sits in the Mac's Keychain where hubd cannot read it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SignInLostData {
    #[serde(default, deserialize_with = "lenient")]
    pub engine: String,
    #[serde(default, deserialize_with = "lenient")]
    pub label: String,
    #[serde(default, deserialize_with = "lenient")]
    pub machine: String,
    #[serde(default, deserialize_with = "lenient")]
    pub message: String,
    #[serde(default, deserialize_with = "lenient")]
    pub kept: bool,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub can_keep: Option<bool>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub keychain: Option<bool>,
}
