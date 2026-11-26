//! Payloads of what shows in a thread beside the conversation itself: `page`, `replies`, `ask`,
//! `plugin_suggested`, `reaction`, `gist`, `output`, `desk_note` and `reported`. The rules for a
//! payload struct are in `event_data.rs`.

use super::*;

/// `page`: the agent showed a page it wrote (`show_page`). `id` names the saved file, `height`
/// is the height it asked for (`null`: the app's own), `bytes` its size; `replaces` is the id of
/// the earlier version this one takes the place of.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PageData {
    #[serde(default, deserialize_with = "lenient")]
    pub id: String,
    #[serde(default, deserialize_with = "lenient")]
    pub title: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub height: Option<Option<i64>>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub bytes: u64,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub replaces: Option<String>,
}

/// One answer the agent offers: the button's `label` and the `message` it sends.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReplyOption {
    #[serde(default, deserialize_with = "lenient")]
    pub label: String,
    #[serde(default, deserialize_with = "lenient")]
    pub message: String,
}

/// `replies`: buttons over the message box that answer the agent's `question` (`offer_replies`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RepliesData {
    #[serde(default, deserialize_with = "lenient")]
    pub question: String,
    #[serde(default, deserialize_with = "lenient_items")]
    pub options: Vec<ReplyOption>,
}

/// `ask`: a question the agent asked without waiting (`ask_user`); it carried on with `assumed`.
/// `id` is what an answer names.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AskData {
    #[serde(default, deserialize_with = "lenient")]
    pub question: String,
    #[serde(default, deserialize_with = "lenient_items")]
    pub options: Vec<ReplyOption>,
    #[serde(default, deserialize_with = "lenient")]
    pub id: String,
    #[serde(default, deserialize_with = "lenient")]
    pub assumed: String,
}

/// `plugin_suggested`: a card for the GitHub repository `repo` with the agent's reason `why`.
/// `repo` is the `PluginRepo` of the day it was suggested, kept as the JSON it was written as:
/// an older card lacks fields the type has since got, and still shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginSuggestedData {
    #[serde(default)]
    #[ts(as = "PluginRepo")]
    pub repo: serde_json::Value,
    #[serde(default, deserialize_with = "lenient")]
    pub why: String,
}

/// `reaction`: `by` ("you" or "agent") put `emoji` on the message at `seq`, or took it away
/// (`on` false; an event that does not say is on).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReactionData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub seq: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub emoji: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub by: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub on: Option<bool>,
}

/// `gist`: a TL;DR of the thread up to seq `upto` for its hover card: `points`, and what it
/// needs from the person (`ask`, or `null`). `by` is "agent" since the agent writes it itself;
/// the older ones, written by a small model, have none.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GistData {
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub upto: i64,
    #[serde(default, deserialize_with = "lenient_items")]
    pub points: Vec<String>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub ask: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub by: Option<String>,
}

/// `output`: a piece (`data`) of what the command of tool call `tool_call_id` prints while it
/// runs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputData {
    #[serde(default, deserialize_with = "lenient")]
    pub tool_call_id: String,
    #[serde(default, deserialize_with = "lenient")]
    pub data: String,
}

/// `desk_note`, in a project's desk: `thread` (titled `title`) finished a turn (`state` "done"),
/// needs you ("asks") or failed ("failed"). `text` is the end of its reply or what it asks;
/// `permission` says it waits on a permission question.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeskNoteData {
    #[serde(default, deserialize_with = "lenient")]
    pub thread: String,
    #[serde(default, deserialize_with = "lenient")]
    pub title: String,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient")]
    pub text: String,
    #[serde(default, deserialize_with = "lenient")]
    pub permission: bool,
}

/// The pull request a fix thread opened, as far as Hub knew when the fix ended.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReportedPr {
    #[serde(default, deserialize_with = "lenient")]
    pub number: u32,
    #[serde(default, deserialize_with = "lenient")]
    pub url: String,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
}

/// `reported`: the failure at seq `about` is problem `problem` in Problems. `state` "sent", then
/// "fixing" once thread `fix` (titled `fix_title`) works on it, then how that ended: "fixed",
/// "stopped" or "failed", with the end of its reply (`text`) and its pull request (`pr`, `null`
/// when it opened none). The latest event for one `about` is the card.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReportedData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub about: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub problem: Option<i64>,
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fix: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fix_title: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable, as = "Option<ReportedPr>")]
    pub pr: Option<Option<ReportedPr>>,
}
