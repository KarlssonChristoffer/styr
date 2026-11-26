//! The tools between Guard and its fix threads: Guard answers a fix thread that reported back
//! (`answer_worker`), and a fix thread asks Guard (`ask_guard`) and says how it ended (`report`).
//! Their declarations (`guard_tools`) and what each call does; the rest is `problems/report.rs`.

use std::sync::Arc;

use serde_json::{Value, json};

use super::{on_thread, thread_of};
use crate::hub::Hub;
use crate::hub_tools::Caller;
use crate::hub_tools::tool::{Tool, text};
use crate::run;

/// The tools between Guard and its fix threads: a fix thread asks Guard and says how it ended,
/// and Guard answers it (`problems/report.rs`).
pub(super) async fn guard_tools(hub: &Arc<Hub>, caller: &Caller) -> Vec<Tool> {
    let Some(thread) = thread_of(caller) else { return Vec::new() };
    if crate::problems::vakten::is_vakten(hub, &thread.id).await {
        return vec![Tool::of(json!({
            "name": "answer_worker",
            "description": "Answers one of your fix threads that reported back: your decision, the answer to its question or what to do next. It carries on with it. Example: {\"thread_id\": \"t_8f2\", \"machine\": \"tincan\", \"text\": \"Take the second option: keep the old field and add the new one.\"}.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "The thread_id from the report." },
                    "machine": { "type": "string", "description": "The machine the report names for the thread; leave out for this one." },
                    "text": { "type": "string" },
                },
                "required": ["thread_id", "text"],
            },
        }), run!(|c| thread!(c, |t| text(answer_worker(c.hub, t, c.args).await))))];
    }
    if crate::problems::report::fix_of(hub, thread).await.is_none() {
        return Vec::new();
    }
    vec![
        Tool::of(json!({
            "name": "ask_guard",
            "description": "Asks Guard, who started this fix thread, instead of the user: a choice between ways to fix it, something you cannot find out, or whether to go on. Then end your turn; Guard's answer comes back here as a message.",
            "inputSchema": {
                "type": "object",
                "properties": { "question": { "type": "string", "description": "The question, with the options and what you would pick." } },
                "required": ["question"],
            },
        }), run!(|c| thread!(c, |t| text(ask_guard(c.hub, t, c.args).await)))),
        Tool::of(json!({
            "name": "report",
            "description": "Tells Guard how this fix ended, just before you end your last turn: \"fixed\" (with the pull request) or \"blocked\" (what stops you and what would unblock it).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "state": { "type": "string", "enum": ["fixed", "blocked"] },
                    "text": { "type": "string", "description": "A few sentences for Guard." },
                },
                "required": ["state", "text"],
            },
        }), run!(|c| thread!(c, |t| text(report(c.hub, t, c.args).await)))),
    ]
}

/// Guard's answer to a fix thread, here or on the machine it runs on.
async fn answer_worker(hub: &Arc<Hub>, guard: &crate::model::Thread, args: &Value) -> Result<String, String> {
    if !crate::problems::vakten::is_vakten(hub, &guard.id).await {
        return Err("answer_worker is Guard's".into());
    }
    let thread = args["thread_id"].as_str().map(str::trim).unwrap_or_default();
    let text = args["text"].as_str().map(str::trim).unwrap_or_default();
    if thread.is_empty() || text.is_empty() {
        return Err("thread_id and text are needed".into());
    }
    let host = args["machine"].as_str().map(str::trim).filter(|m| !m.is_empty() && !m.eq_ignore_ascii_case(hub.remotes.local_name()));
    let sent = match host {
        Some(host) => hub
            .remotes
            .post_to::<Value>(host, "/api/problems/guard/answer", &json!({ "thread": thread, "text": text }), std::time::Duration::from_secs(30))
            .await
            .map(|_| ()),
        None => crate::problems::report::answer(hub, thread, text, json!({ "thread": guard.id, "title": "Guard" })).await,
    };
    match sent {
        Ok(()) => Ok("Sent; the thread carries on with it. Its next report comes to you when its turn ends.".into()),
        Err(e) => Err(format!("{e:#}")),
    }
}

async fn ask_guard(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    match crate::problems::report::ask(hub, thread, args["question"].as_str().unwrap_or_default()).await {
        Ok(()) => Ok("Guard has your question. End your turn now; its answer comes back to this thread as a message.".into()),
        Err(e) => Err(format!("{e:#}")),
    }
}

async fn report(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    match crate::problems::report::said(hub, thread, args["state"].as_str().unwrap_or_default(), args["text"].as_str().unwrap_or_default()).await {
        Ok(()) => Ok("Noted. Guard reads it when this turn ends; end your turn now.".into()),
        Err(e) => Err(format!("{e:#}")),
    }
}
