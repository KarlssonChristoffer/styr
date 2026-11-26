//! The `Hub` half of `desk.rs`: making a project's desk and leaving a note in it as the project's
//! threads finish, ask or fail. Both make or change threads, so they live here; `desk.rs` keeps
//! which thread is a folder's desk and names nothing above it.

use std::sync::Arc;

use anyhow::Result;

use super::Hub;
use crate::locked::ResultExt;
use crate::desk;
use crate::model::{EventKind, NewThread, Thread, ThreadStatus};

/// How much of a reply or question a note carries.
const NOTE_CHARS: usize = 280;

impl Hub {
    /// The desk of `folder`, made the first time. An archived desk comes back.
    pub async fn open_desk(self: &Arc<Self>, folder: &str) -> Result<Thread> {
        if let Some(desk) = desk::of(&self.ctx(), folder).await? {
            if desk.archived {
                return self.update_thread(&desk.id, |t| t.archived = false).await;
            }
            return Ok(desk);
        }
        // Named like the project: the name it was added with, else its folder's.
        let added = self.store.projects().await?.into_iter().find(|p| desk::norm(&p.path) == desk::norm(folder)).map(|p| p.name);
        let name = added.unwrap_or_else(|| {
            std::path::Path::new(folder.trim().trim_end_matches(['/', '\\']))
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Project".into())
        });
        let thread = self
            .create_thread(NewThread {
                cwd: folder.trim().to_owned(),
                title: Some(format!("{name} desk")),
                engine: None,
                message: None,
                attachments: vec![],
                settings: Default::default(),
                original: None,
                client_id: None,
                attempts: vec![],
                warm: None,
                design: false,
                unattended: None,
                reach: None,
                cloud: false,
            })
            .await?;
        self.store.set_value(&desk::key(folder), &thread.id).await?;
        // Named by Hub for good, and never settled away: it is where you come back to.
        self.update_thread(&thread.id, |t| {
            t.title_locked = true;
            t.auto_settle = false;
        })
        .await
    }

    /// Called as a thread's status changes from `before` to the thread's status now: a thread that
    /// finished a turn, now needs you or failed leaves a note in its project's desk.
    pub(crate) async fn note_desk(&self, before: ThreadStatus, thread: &Thread) {
        let state = match (before, thread.status) {
            (b, ThreadStatus::WaitingOnYou) if b != ThreadStatus::WaitingOnYou => "asks",
            (b, ThreadStatus::Error) if b != ThreadStatus::Error => "failed",
            (ThreadStatus::Running, ThreadStatus::Idle) => "done",
            _ => return,
        };
        // Side jobs report to their parent, and remote threads are noted by their own machine.
        if thread.parent.is_some() || thread.host.is_some() {
            return;
        }
        let Ok(Some(desk)) = desk::of(&self.ctx(), &thread.cwd).await else { return };
        if desk.id == thread.id {
            return;
        }
        let (text, permission) = match self.desk_note_text(state, &thread.id).await {
            Ok(found) => found,
            Err(err) => {
                tracing::debug!(thread = %thread.id, "no text for the desk note: {err:#}");
                (String::new(), false)
            }
        };
        let data = crate::model::DeskNoteData { thread: thread.id.clone(), title: thread.title.clone(), state: state.to_owned(), text, permission };
        if let Err(err) = self.emit_typed(&desk.id, &data).await {
            tracing::warn!(desk = %desk.id, "could not leave a note in the desk: {err:#}");
            return;
        }
        // Something new in the desk: it reads as unread until you look.
        let now = crate::model::now_ms();
        self.update_thread(&desk.id, move |t| t.returned_at = Some(now)).await.warn_on_err("mark the desk unread");
    }

    /// What a note says: the question, the error, or the end of the reply; and whether the question
    /// is a permission to answer in the thread rather than with a message.
    async fn desk_note_text(&self, state: &str, thread_id: &str) -> Result<(String, bool)> {
        let last = |kind| async move { Ok::<_, anyhow::Error>(self.store.events_of(thread_id, 0, &[kind]).await?.pop()) };
        let reply = || async { Ok::<_, anyhow::Error>(crate::replies::reply(&self.store.last_reply_chunks(thread_id).await?, i64::MAX)) };
        let mut permission = false;
        let text = match state {
            "failed" => last(EventKind::Error).await?.and_then(|e| e.failure().map(str::to_owned)).unwrap_or_default(),
            "asks" => match last(EventKind::PermissionRequest).await? {
                Some(e) => {
                    permission = true;
                    format!("May I {}?", e.tool_call()["title"].as_str().unwrap_or("use a tool"))
                }
                None => reply().await?,
            },
            _ => reply().await?,
        };
        Ok((crate::text::tail(text.trim(), NOTE_CHARS), permission))
    }
}
