//! Side jobs: a thread's agent hands one piece of work to a child thread, often on another engine
//! (a picture from Codex while Claude keeps coding), and carries on. The child is a thread like any
//! other, linked to its parent by `Thread::parent`. The parent logs a `child` event as the job
//! starts and again when it is done or failed, and its agent gets the result as a message marked
//! `child: <child id>`.

use crate::locked::ResultExt;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::hub::{Hub, conversation_log};
use crate::model::{ChildData, Event, EventKind, NewThread, Outcome, Thread, outcome};

/// What a side job may be, so the right engine picks it up (`Config::side_jobs`).
pub const KINDS: [&str; 6] = ["image", "research", "review", "subtask", "quick", "other"];

/// How much of the parent a child starts with.
pub const CONTEXTS: [&str; 3] = ["brief", "handover", "full"];

/// A side job as the parent's agent asks for it.
#[derive(Debug, Clone)]
pub struct SideJob {
    pub title: String,
    /// What to do, written by the parent's agent with the details the child needs.
    pub task: String,
    pub kind: String,
    pub context: String,
    /// An engine by name, instead of the one configured for the kind.
    pub engine: Option<String>,
    /// A model the engine offers ("haiku"), found the way `start_thread` finds one.
    pub model: Option<String>,
}

impl Hub {
    /// Starts `job` as a child of thread `parent_id` and returns the child at once. The child gets
    /// its first message when the context it needs is ready (a handover takes a while).
    pub async fn start_side_job(self: &Arc<Self>, parent_id: &str, job: SideJob) -> Result<Thread> {
        let parent = self.store.thread(parent_id).await?.context("no such thread")?;
        if parent.parent.is_some() {
            bail!("a side job cannot start side jobs of its own; do it yourself or say what is missing in your reply");
        }
        if !KINDS.contains(&job.kind.as_str()) {
            bail!("unknown kind {:?}; one of {}", job.kind, KINDS.join(", "));
        }
        if !CONTEXTS.contains(&job.context.as_str()) {
            bail!("unknown context {:?}; one of {}", job.context, CONTEXTS.join(", "));
        }
        if job.task.trim().is_empty() {
            bail!("say what the side job should do");
        }
        // Who the user chose for this kind of task in this kind of thread (`handoff.rs`) decides the
        // engine and, unless the agent names a model, the model.
        let chosen = match crate::handoff::task_of_kind(&job.kind) {
            Some(task) => crate::handoff::pick(&self.ctx(), task, Some(&parent.cwd)).await.map(|p| (task, p)),
            None => None,
        };
        if let Some((task, p)) = &chosen
            && let Some(name) = job.engine.as_deref().map(str::trim).filter(|n| !n.is_empty() && *n != p.engine)
        {
            bail!("the user chose {} for {}; do not name {name:?}, leave the engine out", p.engine, crate::handoff::label(task));
        }
        let engine = match &chosen {
            Some((_, p)) => p.engine.clone(),
            None => self.side_job_engine(&parent, &job)?,
        };
        let model = job.model.clone().or_else(|| chosen.as_ref().and_then(|(_, p)| p.model.clone()));
        // Only the model is the agent's to pick, so a side job is never given more rights than a
        // thread of the engine's defaults.
        let mut settings = std::collections::BTreeMap::new();
        if let Some(model) = model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
            let (id, value) = crate::engine_models::pick(&self.store, &engine, model).await?;
            settings.insert(id, json!(value));
        }
        let title = match job.title.trim() {
            "" => crate::hub::title_from_message(&job.task),
            t => t.chars().take(80).collect(),
        };
        let new = NewThread {
            cwd: parent.cwd.clone(),
            title: Some(title.clone()),
            engine: Some(engine.clone()),
            message: None,
            attachments: Vec::new(),
            settings,
            original: None,
            client_id: None,
            attempts: Vec::new(),
            warm: None,
            design: false,
            // A bench case's side job stays under the bench's answer, inside the case's folder, and
            // never waits on the person (`permit::bench`); any other side job asks as a thread does.
            unattended: self.store.unattended(&parent.id).await?.filter(|u| u.by == crate::permit::BENCH),
            reach: None,
            cloud: false,
        };
        let child = self.create_thread_as(new, Some(parent.id.clone()), crate::system::agent(Some(&parent))).await?;
        let started = ChildData {
            id: child.id.clone(),
            title: title.clone(),
            kind: job.kind.clone(),
            context: job.context.clone(),
            engine: engine.clone(),
            model: Some(model.clone()),
            state: "working".into(),
            by: "agent".into(),
            ..Default::default()
        };
        self.emit_typed(&parent.id, &started).await?;

        let hub = self.clone();
        let child_id = child.id.clone();
        tokio::spawn(async move {
            let message = match hub.side_job_message(&parent, &job).await {
                Ok(m) => m,
                Err(e) => {
                    hub.end_side_job(&parent.id, started.clone(), Err(format!("{e:#}"))).await.warn_on_err("end the side job");
                    return;
                }
            };
            if let Err(e) = hub.send(&child_id, message, Vec::new(), None).await {
                hub.end_side_job(&parent.id, started.clone(), Err(format!("{e:#}"))).await.warn_on_err("end the side job");
            }
        });
        Ok(child)
    }

    fn side_job_engine(&self, parent: &Thread, job: &SideJob) -> Result<String> {
        let name = match &job.engine {
            Some(name) if !name.trim().is_empty() => name.trim().to_owned(),
            _ => match self.config.side_jobs.get(&job.kind) {
                Some(name) => name.clone(),
                None => return Ok(parent.engine.clone()),
            },
        };
        if !self.config.engines.contains_key(&name) {
            bail!("no engine {name:?} here for a {} side job; engines: {}", job.kind, self.config.engines.keys().cloned().collect::<Vec<_>>().join(", "));
        }
        Ok(name)
    }

    /// The child's first message: the task, a note on what it is, and the parent's context at the
    /// level asked for.
    async fn side_job_message(&self, parent: &Thread, job: &SideJob) -> Result<String> {
        let note = side_job_note(&parent.title, &job.kind);
        let task = job.task.trim();
        Ok(match job.context.as_str() {
            "handover" => {
                let handover = self.handoff(&parent.id).await.context("writing the handover")?;
                format!("{note}\n\n{task}\n\nWhere the other thread stands:\n\n{handover}")
            }
            "full" => {
                let log = conversation_log(&crate::replies::conversation(&self.store, &parent.id, &[]).await?);
                format!("{note}\n\n{task}\n\nThe other thread's conversation, for context only:\n\n<conversation>{log}\n</conversation>")
            }
            _ => format!("{note}\n\n{task}"),
        })
    }

    /// After a turn in a side job ends or fails: the first result, or failure, goes back to the
    /// parent. Anything after that (the user carried on in the child) stays in the child.
    pub(crate) async fn child_finished(self: &Arc<Self>, end: &Event) -> Result<()> {
        let Some(child) = self.store.thread(&end.thread_id).await? else { return Ok(()) };
        let Some(parent_id) = child.parent.clone() else { return Ok(()) };
        let parent_events = self.store.events_of(&parent_id, 0, &[EventKind::Child]).await?;
        let Some(last) = parent_events.iter().rev().filter_map(Event::data_as::<ChildData>).find(|c| c.id == child.id) else {
            return Ok(());
        };
        if last.state != "working" {
            return Ok(());
        }
        let result = match end.kind {
            EventKind::TurnEnd if end.ended_well() => {
                let events = self.store.events_after_last(&child.id, EventKind::UserMessage, &[]).await?;
                Ok(outcome(&events))
            }
            EventKind::TurnEnd => Err("it was stopped".to_owned()),
            _ => Err(end.failure().unwrap_or("the engine failed").to_owned()),
        };
        self.end_side_job(&parent_id, last, result).await
    }

    /// Logs how side job `started` ended in its parent and tells the parent's agent: now, or for a
    /// picture in an everyday chat with the next message.
    async fn end_side_job(self: &Arc<Self>, parent_id: &str, started: ChildData, result: Result<Outcome, String>) -> Result<()> {
        let (id, title, engine) = (started.id.clone(), started.title.clone(), started.engine.clone());
        let (title, engine) = (title.as_str(), engine.as_str());
        let mut data = started;
        let message = match &result {
            Ok(done) => {
                data.state = "done".into();
                data.reply = Some(crate::text::clip_note(&done.reply, 8_000, CUT_NOTE));
                data.images = Some(done.images.clone());
                // An everyday chat shows the picture itself, so it is not told where it is saved: told so,
                // the agent answered "the picture is saved as C:\\Users\\...\\Temp\\...png" (2026-11-03).
                let chat = match (self.store.thread(parent_id).await?, self.projects_root().await) {
                    (Some(parent), Ok(root)) => crate::connections::is_chat_folder(&root, &parent.cwd),
                    _ => false,
                };
                // There the picture shows itself and the agent has nothing to add: it is not woken to
                // say "the picture is done" (a whole turn) and hears of it with the next message
                // (`tool_instructions::quiet_side_jobs`).
                if chat && !done.images.is_empty() {
                    data.quiet = Some(true);
                    self.emit_typed(parent_id, &data).await?;
                    // The turn that used to follow marked the chat as having something new.
                    if self.store.thread(parent_id).await?.is_some_and(|t| t.status == crate::model::ThreadStatus::Idle) {
                        self.update_thread(parent_id, |t| t.returned_at = Some(crate::model::now_ms())).await?;
                    }
                    return Ok(());
                }
                done_message(title, engine, done, chat)
            }
            Err(error) => {
                data.state = "failed".into();
                data.error = Some(error.clone());
                format!(
                    "(Note from Hub: the side job \"{title}\" you started on {engine} failed: {error}. Carry on without \
it, or do it another way.)"
                )
            }
        };
        self.emit_typed(parent_id, &data).await?;
        self.send_from(parent_id, message, Vec::new(), None, Some(("child", json!(id))), None).await
    }
}

fn side_job_note(parent_title: &str, kind: &str) -> String {
    let files = match kind {
        "subtask" => "Change files as the task needs.",
        // The engine picked for pictures may draw with a tool of its own or a connector, like ElevenLabs.
        "image" => "Make the picture with the image tool you have, and save it where the task says. End with the full path of the picture and nothing else: no description, no copies, no style notes, no \"verified\".",
        _ => "Don't change files in the folder unless the task asks for it.",
    };
    format!(
        "(Note from Hub: you are a side job. Another agent, working in this folder in the thread \"{parent_title}\", \
handed you the task below and carries on meanwhile. Do this task and nothing else. {files} Your last reply goes back \
to that agent as it is, so end with the result itself: the answer, or where the files you made are.)"
    )
}

fn done_message(title: &str, engine: &str, done: &Outcome, chat: bool) -> String {
    if chat {
        return chat_done_message(title, engine, done);
    }
    let mut text = format!(
        "(Note from Hub: the side job \"{title}\" you started on {engine} is done. The user sees its result already, \
and the card says which machine it is saved on, with a button to the folder. Don't repeat the result, don't \
describe it, don't check or verify the file, and don't list paths, copies or its style. Use it for the work in \
hand; if nothing more is asked of you, reply in at most two short lines in the user's language, or not at all.)"
    );
    if !done.images.is_empty() {
        text.push_str("\n\nPictures it made, saved at (for your work; say a path only if the user needs it, in `backticks` so a click shows the picture):\n");
        for path in &done.images {
            text.push_str(&format!("- `{path}`\n"));
        }
    } else if !done.reply.trim().is_empty() {
        text.push_str(&format!("\n\nIts answer:\n\n{}", crate::text::clip_note(done.reply.trim(), 8_000, CUT_NOTE)));
    }
    text
}

/// The same for an everyday chat: the user sees the result and has no use for a file path or a
/// folder, so the agent gets none and is asked for no more than a few words. Not for pictures:
/// those show in the chat and do not wake the agent (`end_side_job`).
fn chat_done_message(title: &str, engine: &str, done: &Outcome) -> String {
    let mut text = format!(
        "(Note from Hub: the side job \"{title}\" you started on {engine} is done. The user sees its result already, \
in the chat. Say nothing about where anything is saved, give no file path or folder, and don't repeat the \
result. If a word is called for, make it one short sentence in the user's language, or no reply at all.)"
    );
    if !done.reply.trim().is_empty() {
        text.push_str(&format!("\n\nIts answer:\n\n{}", crate::text::clip_note(done.reply.trim(), 8_000, CUT_NOTE)));
    }
    text
}

const CUT_NOTE: &str = "[… cut, the whole answer is in the side job's thread …]";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn ev(seq: i64, kind: EventKind, data: Value) -> Event {
        Event { thread_id: "c".into(), seq, ts: seq, kind, data }
    }

    #[test]
    fn the_outcome_is_the_last_reply_and_its_pictures() {
        let events = vec![
            ev(1, EventKind::UserMessage, json!({ "text": "draw" })),
            ev(2, EventKind::Update, json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "old" } })),
            ev(3, EventKind::UserMessage, json!({ "text": "draw again" })),
            ev(4, EventKind::Update, json!({ "sessionUpdate": "tool_call", "toolCallId": "i", "title": "Image generation" })),
            ev(5, EventKind::Update, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "i", "content": [
                { "type": "content", "content": { "type": "image", "data": "x", "mimeType": "image/png", "uri": "/tmp/a.png" } },
            ] })),
            ev(6, EventKind::Update, json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "Here " } })),
            ev(7, EventKind::Update, json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "it is." } })),
        ];
        assert_eq!(outcome(&events), Outcome { reply: "Here it is.".into(), images: vec!["/tmp/a.png".into()] });
    }

    #[test]
    fn a_chat_is_not_told_where_anything_is_saved() {
        let done = Outcome { reply: "It is 42.".into(), images: Vec::new() };
        let chat = done_message("Sum", "codex", &done, true);
        assert!(chat.contains("give no file path") && chat.contains("It is 42."), "{chat}");
        let drawn = Outcome { reply: "Done. Image saved: C:\\x\\a.png. Verified. Style: ink".into(), images: vec!["C:\\x\\a.png".into()] };
        let dev = done_message("Ozzy", "codex", &drawn, false);
        assert!(dev.contains("a.png"), "a coding thread gets the path");
        assert!(dev.contains("at most two short lines"), "and is asked for a short word: {dev}");
        assert!(!dev.contains("Verified"), "the job's recital is not handed on");
    }

    #[test]
    fn a_picture_that_did_not_wake_the_agent_is_told_with_the_next_message() {
        let child = |seq, title: &str, quiet: bool| {
            let mut data = json!({ "id": title, "title": title, "state": "done", "images": ["/tmp/a.png"] });
            if quiet {
                data["quiet"] = json!(true);
            }
            ev(seq, EventKind::Child, data)
        };
        let (a, b, c) = (child(1, "Teapot", true), child(2, "Sunset", true), child(3, "Loud", false));
        let note = crate::tool_instructions::quiet_side_jobs(&[&c, &b, &a]).unwrap_or_default();
        assert!(note.contains("side jobs \"Teapot\", \"Sunset\" finished"), "{note}");
        assert!(!note.contains("Loud") && !note.contains("a.png"), "{note}");
        assert_eq!(crate::tool_instructions::quiet_side_jobs(&[&c]), None);
    }
}
