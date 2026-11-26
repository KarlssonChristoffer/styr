//! Typed payloads of events. The log stores `data` as JSON and stays append-only, so a payload struct
//! describes what is already in it, it does not change it. One file per group of kinds in
//! `event_data/`; the registry at the end of this file names the struct of each kind. The rules:
//! - every field has `#[serde(default)]` (or is an `Option`), so the oldest events still read
//!   (`tests/it/golden_log.rs` reads a frozen old log into the structs);
//! - every field that can fail to read is read with [`lenient`]: a value of the wrong type becomes the
//!   field's default and the rest of the payload still reads, as `data["x"].as_str()` did (a list
//!   of plain values uses [`lenient_items`], which leaves out only the item that does not read);
//! - an unknown field is ignored on reading, and a field that is `None` is not written;
//! - fields are declared in the order the old `json!` wrote its keys (the JSON keeps its key order),
//!   so a struct writes the text the code wrote before it. The workspace turns `preserve_order`
//!   on itself (the ACP crate needs it too), and the golden log checks that it is on;
//! - a new field is added as `Option` with `skip_serializing_if`, never renamed or retyped.
//!
//! Writers build the struct and call `Hub::emit_typed`; readers call `Event::data_as`. Every
//! kind has a struct and a golden line (`golden_log::every_kind_has_a_struct_and_a_golden_line`).
//! A field the old code wrote as an explicit `null` is an `Option<Option<T>>` read with [`nullable`],
//! so missing, `null` and a value are each written back as they were.

use serde::Deserializer;
use serde::de::DeserializeOwned;

use super::*;

mod asks;
mod checks;
mod delegated;
mod history;
mod judged;
mod session;
mod shown;
mod turns;
pub use asks::*;
pub use checks::*;
pub use delegated::*;
pub use history::*;
pub use judged::*;
pub use session::*;
pub use shown::*;
pub use turns::*;

/// A kind whose `data` has a struct. `KIND` is the event it belongs to (set by the registry below).
pub trait EventData: Serialize + DeserializeOwned {
    const KIND: EventKind;

    fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Reads a field, or gives its default when the value has another type than the field (also for
/// `null`). Use with `#[serde(default, deserialize_with = "lenient")]`.
pub(crate) fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// Reads a field that holds another protocol's JSON and may be `null`: missing is `None` (with
/// `#[serde(default)]`), anything written, `null` too, is `Some`. Use with `#[serde(default,
/// deserialize_with = "present", skip_serializing_if = "Option::is_none")]`.
pub(crate) fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<serde_json::Value>, D::Error> {
    serde_json::Value::deserialize(d).map(Some)
}

/// A string field as the old readers saw it: `None` when nothing was written (the default, "").
pub(crate) fn written(field: &str) -> Option<&str> {
    (!field.is_empty()).then_some(field)
}

/// Reads a list item by item: an item of the wrong type is left out and the rest still read, as
/// `filter_map(|i| i.as_str())` did. Use with `#[serde(default, deserialize_with = "lenient_items")]`.
pub(crate) fn lenient_items<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    let items = match serde_json::Value::deserialize(d)? {
        serde_json::Value::Array(items) => items,
        _ => Vec::new(),
    };
    Ok(items.into_iter().filter_map(|item| serde_json::from_value(item).ok()).collect())
}

/// A list that may be missing, read item by item when it is there (`lenient_items`).
pub(crate) fn some_items<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<Vec<T>>, D::Error> {
    lenient_items(d).map(Some)
}

/// Reads a field the old code wrote as `null` when it had nothing: missing is `None` (with
/// `#[serde(default)]`), `null` is `Some(None)`, a value is `Some(Some(value))`. A value of the
/// wrong type reads as `null`. Use with `#[serde(default, deserialize_with = "nullable",
/// skip_serializing_if = "Option::is_none")]` and `#[ts(optional, type = "T | null")]`.
#[allow(clippy::option_option)] // three states on purpose: missing, null, value
pub(crate) fn nullable<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<Option<T>>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(Some(serde_json::from_value(value).ok()))
}

impl Event {
    /// The payload as its struct, when the event is of that kind and the payload is an object. A
    /// field of the wrong type is its default (see [`lenient`]); the rest still reads.
    pub fn data_as<T: EventData>(&self) -> Option<T> {
        if self.kind != T::KIND || !self.data.is_object() {
            return None;
        }
        T::deserialize(&self.data).ok()
    }
}

/// The registry: one row per kind that has a struct, `Variant, json_name => Struct`. It writes the
/// `EventData` impl, the TypeScript map `EventPayloads` (payload type by kind, for the app) and
/// [`rewritten`].
macro_rules! event_kinds {
    ($($kind:ident, $key:ident => $data:ident;)*) => {
        $(impl EventData for $data {
            const KIND: EventKind = EventKind::$kind;
        })*

        /// The payload type of each kind, by the kind's name on the wire. Only its TypeScript
        /// export is used (`app/src/lib/bindings/EventPayloads.ts`); Rust never builds one.
        #[derive(TS)]
        #[ts(export)]
        pub struct EventPayloads {
            $(pub $key: $data,)*
        }

        /// `data` read as the struct of `kind` and written back as JSON text. `None` when the kind
        /// has no struct (only `unknown`), `Some(None)` when the payload does not read. For the golden log.
        pub fn rewritten(kind: EventKind, data: &serde_json::Value) -> Option<Option<String>> {
            match kind {
                $(EventKind::$kind => Some(data.is_object().then(|| $data::deserialize(data).ok()).flatten().map(|d| d.to_json().to_string())),)*
                _ => None,
            }
        }
    };
}

event_kinds! {
    UserMessage, user_message => UserMessageData;
    TurnStart, turn_start => TurnStartData;
    TurnEnd, turn_end => TurnEndData;
    Error, error => ErrorData;
    PermissionRequest, permission_request => PermissionRequestData;
    PermissionResolved, permission_resolved => PermissionResolvedData;
    PermissionAuto, permission_auto => PermissionAutoData;
    Slept, slept => SleptData;
    Status, status => StatusData;
    Queue, queue => QueueData;
    Paused, paused => PausedData;
    Restart, restart => RestartData;
    SessionStarted, session_started => SessionStartedData;
    SessionState, session_state => SessionStateData;
    Setting, setting => SettingData;
    QuotaSwitch, quota_switch => QuotaSwitchData;
    QuotaWait, quota_wait => QuotaWaitData;
    EngineSwitch, engine_switch => EngineSwitchData;
    SignInLost, sign_in_lost => SignInLostData;
    Unattended, unattended => Unattended;
    Checkpoint, checkpoint => CheckpointData;
    Restored, restored => RestoredData;
    Rewind, rewind => RewindData;
    RuleSaved, rule_saved => RuleSavedData;
    Attempt, attempt => AttemptData;
    Terminal, terminal => TerminalData;
    Moved, moved => MovedData;
    MovedAway, moved_away => MovedAwayData;
    Clash, clash => ClashData;
    Leak, leak => LeakData;
    Customs, customs => CustomsData;
    Proof, proof => ProofData;
    Docs, docs => DocsData;
    Review, review => ReviewData;
    Design, design => DesignData;
    Task, task => TaskData;
    Focus, focus => FocusData;
    Reading, reading => ReadingData;
    Page, page => PageData;
    Replies, replies => RepliesData;
    Ask, ask => AskData;
    PluginSuggested, plugin_suggested => PluginSuggestedData;
    Reaction, reaction => ReactionData;
    Gist, gist => GistData;
    Output, output => OutputData;
    DeskNote, desk_note => DeskNoteData;
    Reported, reported => ReportedData;
    Child, child => ChildData;
    Auto, auto => AutoData;
    HandoffOffer, handoff_offer => HandoffOfferData;
    HandedOff, handed_off => HandedOffData;
    HandoffResult, handoff_result => HandoffResultData;
    WorkerRun, worker_run => WorkerRunData;
    Update, update => UpdateData;
}
