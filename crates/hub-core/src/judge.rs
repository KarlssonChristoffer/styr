//! What you write to a thread that is working. The message is logged and queued as always; the
//! decision model (`apis.rs`, feature `judge`) then reads it with where the work stands and picks
//! one of five: a question for the side (answered by the small tidying model from the thread's
//! log, without the agent hearing of it), a stop, a correction (steers), something for later
//! (stays in the queue) or a task of its own (offers a thread, as `check_topic` does). A model that
//! is missing, slow or unsure leaves it to the person's own setting, queue or steer, so nothing
//! is lost. The result goes to the windows as one `aside` stream message, not into the log.

use crate::locked::ResultExt;
use crate::locked::LockExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::hub::{Hub, conversation_log};
use crate::model::{Aside, Event, EventKind, StreamMsg, Thread, ThreadStatus, now_ms};
use crate::text::cut;

/// How sure the model must be, per pick. Stopping needs the most: it ends work in flight.
const SURE: f64 = 0.6;
const SURE_STOP: f64 = 0.8;
const SURE_NEW: f64 = 0.75;
/// A decision that takes longer leaves the message to the person's setting.
const WAIT: Duration = Duration::from_secs(4);
const ANSWER_WAIT: Duration = Duration::from_secs(45);

const ANSWER: &str = "Below is a conversation between a person and a coding agent that is still working. \
The person asks you a short question on the side, or asks for a short piece of text. Do not continue the \
conversation, do not use tools, and do not claim to be the agent. Answer from the log when the question is \
about the work and say plainly when the log does not show it; answer from general knowledge when it is not \
about the work. At most three short sentences, or the text asked for if it is short. Use the language the \
person writes in.";

/// The last note per thread, so a window that opens or reloads still shows it.
fn notes() -> &'static Mutex<HashMap<String, Aside>> {
    static NOTES: OnceLock<Mutex<HashMap<String, Aside>>> = OnceLock::new();
    NOTES.get_or_init(Default::default)
}

/// The options the decision model picks from.
pub(crate) fn menu() -> Vec<(String, String)> {
    [
        ("ask", "A question or a request for a short piece of text, meant for the person's side: how it is going, what it is doing now, why, how much is left, a fact, a quick draft. It needs an answer but must not change or interrupt the running work"),
        ("stop", "Tells the assistant to stop, wait or cancel what it is doing now, with nothing new to do instead"),
        ("steer", "Changes the work in progress: a correction, a different approach or an instruction the running work should follow now rather than after it finishes"),
        ("later", "Something to do after the current work is done: a follow-up, an extra step, or an answer to what the assistant just asked"),
        ("new", "A different task that has little to do with the running work and would fit a thread of its own"),
    ]
    .map(|(n, m)| (n.to_owned(), m.to_owned()))
    .to_vec()
}

/// What the decision model reads: what the thread is for, what the agent is doing and said last,
/// whether it waits for the person, and the new message.
pub(crate) fn state(thread: &Thread, events: &[Event], text: &str) -> String {
    let began = events
        .iter()
        .find(|e| e.kind == EventKind::UserMessage)
        .and_then(|e| e.said())
        .unwrap_or_default();
    let since_turn: Vec<&Event> = events.iter().rev().take_while(|e| e.kind != EventKind::TurnStart).collect();
    let doing: Vec<&str> = since_turn
        .iter()
        .filter(|e| e.kind == EventKind::Update && crate::model::update_kind(&e.data) == Some("tool_call"))
        .filter_map(|e| e.data["title"].as_str())
        .take(4)
        .collect();
    let mut said: Vec<&str> = since_turn
        .iter()
        .filter(|e| e.kind == EventKind::Update && crate::model::update_kind(&e.data) == Some("agent_message_chunk"))
        .filter_map(|e| e.data["content"]["text"].as_str())
        .collect();
    said.reverse();
    let said: String = said.concat();
    let waiting = if thread.status == ThreadStatus::WaitingOnYou {
        "\nIt is waiting for the person to answer a permission question."
    } else {
        ""
    };
    format!(
        "The thread began with:\n{}\n\nThe assistant is working right now. Its latest steps: {}\nIts latest words: {}{waiting}\n\nThe person just wrote:\n{}",
        cut(began, 600, false),
        if doing.is_empty() { "none yet".to_owned() } else { doing.join("; ") },
        if said.trim().is_empty() { "nothing yet".to_owned() } else { cut(&said, 600, true) },
        cut(text, 900, false),
    )
}

/// What the answering model reads: the thread's conversation, the latest steps and the question.
fn answer_prompt(thread: &Thread, events: &[Event], question: &str) -> String {
    let log = conversation_log(events);
    let chars: Vec<char> = log.chars().collect();
    let log = if chars.len() > 16_000 {
        format!("{}\n…\n{}", chars[..4_000].iter().collect::<String>(), chars[chars.len() - 12_000..].iter().collect::<String>())
    } else {
        log
    };
    let running = events.iter().rev().find(|e| e.kind == EventKind::TurnStart).map(|e| (now_ms() - e.ts) / 1000);
    let clock = running.map(|s| format!("\nThe current turn has run for {s} seconds.")).unwrap_or_default();
    format!("{ANSWER}\n\nThread: {}{clock}\n\n<conversation>{log}\n</conversation>\n\nThe question: {question}", thread.title)
}

impl Hub {
    /// The latest note of a thread's judge, if any.
    pub fn last_aside(&self, thread_id: &str) -> Option<Aside> {
        notes().locked().get(thread_id).cloned()
    }

    fn put_aside(&self, thread_id: &str, kind: &str, question: Option<String>, text: String) {
        let aside = Aside { kind: kind.into(), question, text, at: now_ms() };
        notes().locked().insert(thread_id.to_owned(), aside.clone());
        self.publish(StreamMsg::Aside { thread_id: thread_id.to_owned(), aside });
    }

    /// Decides what message `seq`, which waits behind a running turn, is, and does it. `steer`
    /// is the person's own setting, which applies whenever the model does not decide.
    pub(crate) async fn judge(self: Arc<Self>, thread_id: String, seq: i64, text: String, steer: bool) {
        match self.judged(&thread_id, seq, &text).await {
            Ok(true) => {}
            Ok(false) => {
                if steer && let Err(e) = self.steer(&thread_id, seq).await {
                    tracing::info!(thread = %thread_id, "steering after no decision: {e:#}");
                }
            }
            Err(e) => {
                tracing::warn!(thread = %thread_id, "judging a message: {e:#}");
                if steer {
                    self.steer(&thread_id, seq).await.warn_on_err("steer the thread");
                }
            }
        }
    }

    /// Ok(true) when the message was dealt with, Ok(false) when the person's setting decides.
    async fn judged(self: &Arc<Self>, thread_id: &str, seq: i64, text: &str) -> Result<bool> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let events = crate::replies::conversation(&self.store, thread_id, &[EventKind::TurnStart]).await?;
        let state = state(&thread, &events, text);
        let instructions = "What is the person doing by writing this to the assistant while it works";
        let Some(c) = self.choose("judge", Some(thread_id), instructions, &state, text, &menu(), "", SURE, WAIT).await else {
            return Ok(false);
        };
        if !c.acted {
            return Ok(false);
        }
        match c.pick.as_str() {
            "ask" => self.answer_aside(thread_id, seq, text, &thread, &events).await,
            "stop" if c.confidence >= SURE_STOP => {
                // Nothing to carry out afterwards: the message is not for the agent.
                self.change_queued(thread_id, seq, None).await?;
                self.cancel(thread_id).await?;
                self.put_aside(thread_id, "stopped", None, String::new());
                Ok(true)
            }
            "steer" => {
                self.steer(thread_id, seq).await?;
                self.put_aside(thread_id, "steered", None, String::new());
                Ok(true)
            }
            "later" => {
                self.put_aside(thread_id, "queued", None, String::new());
                Ok(true)
            }
            "new" if c.confidence >= SURE_NEW => {
                let data = crate::model::ReadingData { seq: Some(seq), state: Some("new".into()), acted: true, ..crate::reading::read_as("topic", &c) };
                self.emit_typed(thread_id, &data).await?;
                self.put_aside(thread_id, "new", None, String::new());
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Answers a question for the side with the small model tidying uses, then takes the question
    /// out of the queue so the agent never sees it. If the agent took it first (the turn ended
    /// meanwhile) it is an ordinary message and the answer is dropped.
    async fn answer_aside(&self, thread_id: &str, seq: i64, text: &str, thread: &Thread, events: &[Event]) -> Result<bool> {
        let name = self.config.tidy.engine.as_ref().unwrap_or(&self.config.default_engine);
        let engine = self.config.engines.get(name).with_context(|| format!("unknown engine {name:?}"))?;
        let prompt = answer_prompt(thread, events, text);
        let answer = crate::engine::complete("judge_answer", engine, &thread.cwd, self.config.tidy.model.as_deref(), prompt);
        let answer = tokio::time::timeout(ANSWER_WAIT, answer).await.context("the answer took too long")??;
        let answer = answer.trim().to_owned();
        if answer.is_empty() {
            bail!("the model gave no answer");
        }
        if self.change_queued(thread_id, seq, None).await.is_err() {
            return Ok(true);
        }
        self.put_aside(thread_id, "answer", Some(cut(text, 400, false)), answer);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(seq: i64, kind: EventKind, data: serde_json::Value) -> Event {
        Event { thread_id: "t".into(), seq, ts: seq, kind, data }
    }

    fn thread(status: ThreadStatus) -> Thread {
        Thread {
            id: "t".into(), title: "Fix the login form".into(), cwd: "/tmp".into(), engine: "claude".into(), status,
            engine_session_id: None, title_locked: false, archived: false, returned_at: None, seen_at: None,
            settled_at: None, sent_at: None, pinned_at: None, snoozed_until: None, auto_settle: true,
            parent: None, reading: None, owner: None, cloud: None, host: None, project: None, display: None, created_at: 1, updated_at: 1,
        }
    }

    #[test]
    fn the_judge_reads_what_the_agent_is_doing_and_said() {
        let events = vec![
            ev(1, EventKind::UserMessage, json!({ "text": "Fix the login form" })),
            ev(2, EventKind::TurnStart, json!({ "prompt": 1 })),
            ev(3, EventKind::Update, json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": "Looking at the form. " } })),
            ev(4, EventKind::Update, json!({ "sessionUpdate": "tool_call", "title": "Editing Login.svelte" })),
            ev(5, EventKind::Update, json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": "Now the tests." } })),
        ];
        let s = state(&thread(ThreadStatus::Running), &events, "how is it going?");
        assert!(s.contains("Fix the login form"));
        assert!(s.contains("Editing Login.svelte"));
        assert!(s.contains("Looking at the form. Now the tests."));
        assert!(s.ends_with("how is it going?"));
        assert!(!s.contains("permission"));
        assert!(state(&thread(ThreadStatus::WaitingOnYou), &events, "yes").contains("permission"));
    }

    #[test]
    fn the_menu_has_the_five_picks() {
        let names: Vec<String> = menu().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["ask", "stop", "steer", "later", "new"]);
    }
}
