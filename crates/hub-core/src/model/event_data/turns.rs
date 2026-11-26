//! Payloads of the kinds every turn writes: `user_message`, `turn_start`, `turn_end`, `error` and
//! `permission_request`. The rules for a payload struct are in `event_data.rs`.

use super::*;

/// `user_message`: what the user (or Hub, on their behalf) said. Everything beside the text is in
/// `marks`, in the order it was written: `attachments`, what Hub marks a message with (`review`,
/// `leak`, `proof`, `docs`, `child`, `restart`, ...) and `original`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserMessageData {
    #[serde(default, deserialize_with = "lenient")]
    pub text: String,
    #[serde(default, flatten)]
    #[ts(skip)]
    pub marks: serde_json::Map<String, serde_json::Value>,
}

impl UserMessageData {
    /// What the user wrote before Hub changed it (a rule applied, a template). Not a field of its
    /// own: it is written after the attachments and the switch mark, and a field would move it.
    pub fn original(&self) -> Option<&str> {
        self.marks.get("original").and_then(|o| o.as_str())
    }
}

/// `turn_start`: a prompt goes to the engine. `prompt` is the user message's seq; missing for a
/// turn the engine started by itself (`unprompted`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TurnStartData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null", optional)]
    pub prompt: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unprompted: Option<bool>,
}

/// `turn_end`: how a turn ended. `stop_reason` is the engine's word ("end_turn", "cancelled",
/// "max_tokens", ...); an empty one is an old or empty event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TurnEndData {
    #[serde(default, deserialize_with = "lenient")]
    pub stop_reason: String,
    /// The turn was cut off by Hub (a stop that the engine did not answer).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub forced: Option<bool>,
    /// The user message the turn answered; missing in logs from before it was added.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null", optional)]
    pub prompt: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unprompted: Option<bool>,
    /// The turn went to a terminal; the thread is not idle (`status.rs`).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handed_over: Option<bool>,
    /// The tokens the engine said the turn used, when its answer said (`engine/acp/turns.rs`).
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage: Option<TurnUsage>,
}

/// The tokens of one turn as the engine counted them. A count the engine did not give is left out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TurnUsage {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub input: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub output: Option<i64>,
    /// Read from the model's prompt cache.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub cache_read: Option<i64>,
    /// Written to the model's prompt cache.
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(type = "number", optional)]
    pub cache_write: Option<i64>,
}

impl TurnEndData {
    pub fn ended_well(&self) -> bool {
        self.stop_reason == "end_turn"
    }

    pub fn cancelled(&self) -> bool {
        self.stop_reason == "cancelled"
    }
}

/// What a thread says when its engine's connection closed in the middle of a turn
/// ("Incoming transport closed" in ACP's words). The status rules read it too (`status.rs`).
pub const ENGINE_GONE: &str =
    "The engine stopped in the middle of this turn, so its work was cut off. Send a message to start it again; it picks up the same conversation.";

/// `error`: the engine (or Hub) failed. `leaves` says what state the thread is left in ("idle" or
/// "error"); without it the old rule applies (see `status.rs`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ErrorData {
    #[serde(default, deserialize_with = "lenient")]
    pub message: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(type = "number | null", optional)]
    pub prompt: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub leaves: Option<String>,
}

impl ErrorData {
    /// An error with only a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), ..Default::default() }
    }
}

/// `permission_request`: the engine waits for an answer. `tool_call` and `options` are the
/// engine protocol's own JSON (ACP), kept as it came.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PermissionRequestData {
    #[serde(default)]
    #[ts(type = "unknown")]
    pub tool_call: serde_json::Value,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub options: serde_json::Value,
    /// Hub's id for the question; the answer (`permission_resolved`) names it. Written last, as
    /// it always was (the engine's request first, then the id Hub gave it).
    #[serde(default, deserialize_with = "lenient")]
    pub id: String,
}

/// Reads of the five kinds above straight from the stored JSON, for code that looks at one key of
/// many events and should not copy each payload into its struct to do it. They read the key and
/// nothing else: on an event of another kind, or one written without the key, they say `None`,
/// `false` or JSON `null`. Code with an `Event` in hand reads these keys here; what is left
/// outside is mostly SQL (`json_extract`, rows read as kind and JSON in `problems/tools.rs` and
/// `store/events.rs`).
impl Event {
    /// `user_message`: the text.
    pub fn said(&self) -> Option<&str> {
        self.data["text"].as_str()
    }

    /// `user_message`: one of its marks (`restart`, `review`, `proof`, `attachments`, ...), `null`
    /// when it has none.
    pub fn mark(&self, key: &str) -> &serde_json::Value {
        &self.data[key]
    }

    /// Any of the five: the key was written, whatever its value.
    pub fn has(&self, key: &str) -> bool {
        self.data.get(key).is_some()
    }

    /// `turn_start`, `turn_end`, `error`: the user message it belongs to.
    pub fn prompt(&self) -> Option<i64> {
        self.data["prompt"].as_i64()
    }

    /// `turn_end`: the engine's word for how the turn ended.
    pub fn stop_reason(&self) -> Option<&str> {
        self.data["stop_reason"].as_str()
    }

    /// `turn_end`: the turn ran to its end.
    pub fn ended_well(&self) -> bool {
        self.stop_reason() == Some("end_turn")
    }

    /// `turn_end`: the turn was stopped.
    pub fn cancelled(&self) -> bool {
        self.stop_reason() == Some("cancelled")
    }

    /// `turn_end`: the turn went to a terminal.
    pub fn handed_over(&self) -> bool {
        self.data["handed_over"] == true
    }

    /// `error`: the message.
    pub fn failure(&self) -> Option<&str> {
        self.data["message"].as_str()
    }

    /// `error`: the state the failure leaves the thread in ("idle" or "error"), when it says.
    pub fn leaves(&self) -> Option<&str> {
        self.data["leaves"].as_str()
    }

    /// `error`: the detail, a string in every writer of today.
    pub fn detail(&self) -> &serde_json::Value {
        &self.data["detail"]
    }

    /// `permission_request`: Hub's id for the question.
    pub fn request_id(&self) -> &serde_json::Value {
        &self.data["id"]
    }

    /// `permission_request`: the engine's tool call, as it came.
    pub fn tool_call(&self) -> &serde_json::Value {
        &self.data["tool_call"]
    }

    /// `permission_request`: the engine's options, as they came.
    pub fn options(&self) -> &serde_json::Value {
        &self.data["options"]
    }
}

/// `update`: one session update, the engine protocol's own JSON (ACP's `SessionUpdate`) kept as
/// it came, or one Hub wrote in the same shape (a `plan` from `update_steps`, a `notice`). Nine
/// events in ten are of this kind, so nothing is read into fields: the struct is the object as
/// it is, and [`update_kind`] reads which update it is without copying anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export)]
pub struct UpdateData(#[ts(type = "{ sessionUpdate?: string } & Record<string, unknown>")] pub serde_json::Map<String, serde_json::Value>);

/// Which session update `data` is: its `sessionUpdate` ("agent_message_chunk", "tool_call",
/// "tool_call_update", "plan", ...), `None` when it does not say. For an `update` event's payload
/// and for an update still on its way from the engine.
pub fn update_kind(data: &serde_json::Value) -> Option<&str> {
    data["sessionUpdate"].as_str()
}

/// Whether an update is about a tool call Hub counts (`problems/tools.rs`): one starting, or one that failed.
pub fn worth_a_look(update: &serde_json::Value) -> bool {
    match update_kind(update) {
        Some("tool_call") => true,
        Some("tool_call_update") => update["status"] == "failed",
        _ => false,
    }
}

/// What a tool call's terminal has printed once the update `data` is counted in, `None` when the
/// update carries none: `_meta.terminal_output` is the whole output, `terminal_output_delta` the
/// next chunk after `so_far`.
pub fn terminal_output(data: &serde_json::Value, so_far: &str) -> Option<String> {
    let meta = &data["_meta"];
    let whole = meta["terminal_output"]["data"].as_str();
    let chunk = meta["terminal_output_delta"]["data"].as_str();
    (whole.is_some() || chunk.is_some()).then(|| format!("{}{}", whole.unwrap_or(so_far), chunk.unwrap_or("")))
}

/// Permission questions asked and not answered, by Hub's id, in the order they were asked.
pub fn open_questions(events: &[Event]) -> Vec<String> {
    let mut open: Vec<String> = Vec::new();
    for e in events {
        match e.kind {
            EventKind::PermissionRequest => {
                if let Some(id) = e.request_id().as_str() {
                    open.push(id.to_owned());
                }
            }
            EventKind::PermissionResolved => {
                if let Some(answer) = e.data_as::<PermissionResolvedData>() {
                    open.retain(|o| *o != answer.id);
                }
            }
            _ => {}
        }
    }
    open
}

/// What a thread's answer was (a side job's for its parent, a widget build's, a routine run's): its last reply and the pictures it made since the last message.
#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub reply: String,
    pub images: Vec<String>,
}

/// A thread's last reply and the pictures it made since the last message it got.
pub fn outcome(events: &[Event]) -> Outcome {
    let start = events.iter().rposition(|e| e.kind == EventKind::UserMessage).map_or(0, |i| i + 1);
    let mut out = Outcome::default();
    // All it said since, with a break where it used a tool.
    let mut reply = String::new();
    for e in &events[start..] {
        if e.kind != EventKind::Update {
            continue;
        }
        match update_kind(&e.data) {
            Some("agent_message_chunk") => {
                let content = &e.data["content"];
                if content["type"] == "image" {
                    push_image(&mut out.images, content);
                } else if let Some(t) = content["text"].as_str() {
                    reply.push_str(t);
                }
            }
            Some("tool_call") | Some("tool_call_update") => {
                for item in e.data["content"].as_array().into_iter().flatten() {
                    if item["content"]["type"] == "image" {
                        push_image(&mut out.images, &item["content"]);
                    }
                }
                if !reply.trim().is_empty() {
                    reply.push_str("\n\n");
                }
            }
            _ => {}
        }
    }
    out.reply = reply.trim().to_owned();
    out
}

fn push_image(images: &mut Vec<String>, content: &serde_json::Value) {
    if let Some(uri) = content["uri"].as_str() {
        let path = uri.strip_prefix("file://").unwrap_or(uri).to_owned();
        if !images.contains(&path) {
            images.push(path);
        }
    }
}
