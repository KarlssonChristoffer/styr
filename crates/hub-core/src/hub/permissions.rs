//! Permission questions: answering them, putting them to sleep, and the queue behind a turn.

use crate::locked::ResultExt;
use super::*;
use crate::model::{PermissionResolvedData, QueueData, SleptData};

impl Hub {
    pub async fn answer_permission(
        self: &Arc<Self>,
        thread_id: &str,
        request_id: &str,
        option_id: Option<String>,
    ) -> Result<()> {
        let pending = self.permissions.locked().remove(request_id);
        match pending {
            Some(p) if p.thread_id == thread_id => {
                self.resolve(p, request_id, option_id).await;
                Ok(())
            }
            Some(p) => {
                self.permissions.locked().insert(request_id.to_owned(), p);
                bail!("permission request belongs to another thread")
            }
            None => {
                let kinds = [EventKind::PermissionRequest, EventKind::PermissionResolved, EventKind::Slept, EventKind::TurnStart];
                let events = self.store.events_of(thread_id, 0, &kinds).await?;
                // A question the session went to sleep on is answered by starting it again, also
                // after a restart: nothing of it lived in memory.
                if sleeping_question(&events, request_id) {
                    return self.wake_from_sleep(thread_id, request_id, option_id, &events).await;
                }
                // A card from before a restart: the question lived in the old core's memory. Close
                // it in the log, so it stops asking, and say why the answer did not count.
                if crate::model::open_questions(&events).iter().any(|id| id == request_id) {
                    let closed = PermissionResolvedData { stale: Some(true), ..PermissionResolvedData::answer(request_id, None) };
                    self.emit_typed(thread_id, &closed).await?;
                    bail!("that question is from before Hub restarted, so the agent no longer waits for it; it asks again if it still needs the answer");
                }
                bail!("no open permission request {request_id}")
            }
        }
    }

    /// Stops the engine of every thread whose permission questions have all waited for the user
    /// longer than `after`, and keeps the questions. The answer starts the engine again on the
    /// same session (`wake_from_sleep`), so a session that waits a night takes nothing meanwhile.
    /// Returns the threads put to sleep.
    pub async fn sleep_waiting(self: &Arc<Self>, after: std::time::Duration) -> Vec<ThreadId> {
        if self.halt.locked().is_some() {
            return Vec::new();
        }
        let asleep: Vec<(ThreadId, Vec<String>)> = {
            let mut map = self.permissions.locked();
            let mut by_thread: HashMap<ThreadId, Vec<(String, std::time::Instant)>> = HashMap::new();
            for (id, p) in map.iter() {
                by_thread.entry(p.thread_id.clone()).or_default().push((id.clone(), p.asked));
            }
            let runtimes = self.runtimes.locked();
            let due: Vec<(ThreadId, Vec<String>)> = by_thread
                .into_iter()
                .filter(|(thread, asked)| runtimes.contains_key(thread) && asked.iter().all(|(_, at)| at.elapsed() >= after))
                .map(|(thread, asked)| (thread, asked.into_iter().map(|(id, _)| id).collect()))
                .collect();
            // Taken out of the open questions without an answer: the engine reads that as cancelled.
            for (_, ids) in &due {
                for id in ids {
                    map.remove(id);
                }
            }
            due
        };
        let mut out = Vec::new();
        for (thread_id, ids) in asleep {
            if self.emit_typed(&thread_id, &SleptData { ids }).await.is_err() {
                continue;
            }
            if let Err(err) = self.shutdown(&thread_id, false).await {
                tracing::warn!(thread = %thread_id, "could not put it to sleep: {err:#}");
                continue;
            }
            // The engine leaves it idle when it goes; it is still waiting for the user.
            self.set_status(&thread_id, ThreadStatus::WaitingOnYou).await.warn_on_err("set status WaitingOnYou");
            tracing::info!(thread = %thread_id, "asleep until the user answers");
            out.push(thread_id);
        }
        out
    }

    /// The user answers a question that was asked before the engine went to sleep: logs the answer
    /// and starts the engine again with it, as a message on the same session.
    async fn wake_from_sleep(self: &Arc<Self>, thread_id: &str, request_id: &str, option_id: Option<String>, events: &[Event]) -> Result<()> {
        let Some(asked) = events.iter().rev().find(|e| e.kind == EventKind::PermissionRequest && *e.request_id() == request_id) else {
            bail!("no open permission request {request_id}")
        };
        let call = asked.tool_call()["title"].as_str().unwrap_or("a tool");
        let choice = asked.options()
            .as_array()
            .and_then(|o| o.iter().find(|o| option_id.is_some() && o["optionId"].as_str() == option_id.as_deref()))
            .and_then(|o| o["name"].as_str())
            .unwrap_or("cancelled");
        let woke = PermissionResolvedData { woke: Some(true), ..PermissionResolvedData::answer(request_id, option_id) };
        self.emit_typed(thread_id, &woke).await?;
        let prompt = events.iter().rev().find(|e| e.kind == EventKind::TurnStart).and_then(|e| e.prompt()).unwrap_or(0);
        let text = format!("You asked the user about \"{call}\" and your session was put to sleep while they were away. They answered: {choice}. Carry on from there.");
        let ask = json!({ "text": text, "attachments": [], "restart": prompt });
        let message = self.emit(thread_id, EventKind::UserMessage, ask).await?;
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let commands = self.runtime(thread)?;
        self.set_status(thread_id, ThreadStatus::Running).await?;
        send_or_log(&commands, Command::Prompt(Prompt::resend(&message)), "restart prompt");
        Ok(())
    }

    /// The engine takes user message `prompt` from its queue: as it was last edited, or `None`
    /// when it was removed or let go while it waited. From here on it can no longer be changed.
    pub(crate) async fn take_prompt(&self, thread_id: &str, mut prompt: Prompt) -> Result<Option<Prompt>> {
        let mut taken = self.taken.lock().await;
        let events = self.store.events_of(thread_id, prompt.seq, &[EventKind::Queue]).await?;
        let mut gone = false;
        for change in events.iter().filter_map(Event::data_as::<QueueData>).filter(|q| q.seq == prompt.seq) {
            if change.gone() {
                gone = true;
            } else if change.action == "edited" {
                let new = change.text.unwrap_or_default();
                let notes = prompt.text.strip_suffix(prompt.said.as_str()).unwrap_or_default();
                prompt.text = format!("{notes}{new}");
                prompt.said = new;
            }
        }
        taken.insert((thread_id.to_owned(), prompt.seq));
        Ok((!gone).then_some(prompt))
    }

    /// Edits user message `seq` while it waits in the queue behind a running turn, or removes it
    /// when `text` is `None`. Refused once the engine has taken it.
    pub async fn change_queued(&self, thread_id: &str, seq: i64, text: Option<String>) -> Result<()> {
        if text.as_deref().is_some_and(|t| t.trim().is_empty()) {
            bail!("an edited message cannot be empty; remove it instead");
        }
        let taken = self.taken.lock().await;
        let events = self.store.events_of(thread_id, seq - 1, &[EventKind::UserMessage, EventKind::TurnStart, EventKind::Queue]).await?;
        if !events.first().is_some_and(|e| e.seq == seq && e.kind == EventKind::UserMessage) {
            bail!("no message {seq} in this thread");
        }
        let started = taken.contains(&(thread_id.to_owned(), seq))
            || events.iter().any(|e| e.kind == EventKind::TurnStart && e.prompt() == Some(seq));
        if started {
            bail!("the agent already has this message");
        }
        if events.iter().filter_map(Event::data_as::<QueueData>).any(|q| q.seq == seq && q.gone()) {
            bail!("this message is no longer in the queue");
        }
        let data = match text {
            Some(text) => QueueData { text: Some(text), ..QueueData::new(seq, "edited") },
            None => QueueData::new(seq, "removed"),
        };
        self.emit_typed(thread_id, &data).await?;
        drop(taken);
        Ok(())
    }

    /// Steers with queued user message `seq`: the running turn stops and it goes to the engine
    /// next, ahead of anything queued before it. Logged as a `queue` event, `steered`.
    pub async fn steer(&self, thread_id: &str, seq: i64) -> Result<()> {
        let taken = self.taken.lock().await;
        if taken.contains(&(thread_id.to_owned(), seq)) {
            bail!("the agent already has this message");
        }
        let events = self.store.events_of(thread_id, seq - 1, &[EventKind::UserMessage, EventKind::TurnStart, EventKind::Queue]).await?;
        if !events.first().is_some_and(|e| e.seq == seq && e.kind == EventKind::UserMessage) {
            bail!("no message {seq} in this thread");
        }
        let gone = events.iter().any(|e| {
            (e.kind == EventKind::TurnStart && e.prompt() == Some(seq))
                || e.data_as::<QueueData>().is_some_and(|q| q.seq == seq && q.gone())
        });
        if gone {
            bail!("this message is no longer in the queue");
        }
        self.emit_typed(thread_id, &QueueData::new(seq, "steered")).await?;
        let commands = self.runtimes.locked().get(thread_id).map(|r| r.commands.clone());
        if let Some(commands) = commands {
            send_or_log(&commands, Command::Steer(seq), "steer");
        }
        drop(taken);
        Ok(())
    }

    /// Message `seq` waits behind a turn that is running or about to: one started for an earlier
    /// message and not ended.
    pub(super) async fn queued_behind(&self, thread_id: &str, seq: i64) -> Result<bool> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        Ok(matches!(thread.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou)
            && !self.taken.lock().await.contains(&(thread_id.to_owned(), seq)))
    }

    /// Rejects every open permission request of a thread, as ACP requires on cancel.
    pub(crate) async fn cancel_permissions(&self, thread_id: &str) {
        let open: Vec<(String, PendingPermission)> = {
            let mut map = self.permissions.locked();
            let ids: Vec<String> =
                map.iter().filter(|(_, p)| p.thread_id == thread_id).map(|(id, _)| id.clone()).collect();
            ids.into_iter().filter_map(|id| map.remove(&id).map(|p| (id, p))).collect()
        };
        for (id, p) in open {
            self.resolve(p, &id, None).await;
        }
    }

    async fn resolve(&self, p: PendingPermission, request_id: &str, option_id: Option<String>) {
        let thread_id = p.thread_id.clone();
        p.answer.send(option_id.clone()).ignore("the turn that asked may have ended");
        self.emit_typed(&thread_id, &PermissionResolvedData::answer(request_id, option_id.clone())).await.warn_on_err("emit PermissionResolved");
        let still_waiting =
            self.permissions.locked().values().any(|p| p.thread_id == thread_id);
        // Back to work only from waiting on you: an answer that comes after the turn ended (a
        // stale card) must not mark an idle thread as running again.
        if !still_waiting {
            self
                .update_thread(&thread_id, |t| {
                    if t.status == ThreadStatus::WaitingOnYou {
                        t.status = ThreadStatus::Running;
                    }
                })
                .await.warn_on_err("set status Running");
        }
    }

    /// Hub's own answer to a permission question, by the user's permission settings.
    pub(super) async fn auto_permission(&self, thread_id: &str, data: &serde_json::Value) -> Option<crate::permit::Verdict> {
        let thread = self.store.thread(thread_id).await.ok()??;
        let prefs = self.permission_prefs().await.unwrap_or_default();
        // A routine's run only reports and never waits for the user, whatever the level.
        if let Ok(Some(_)) = crate::routines::working_run(&self.ctx(), thread_id).await {
            return Some(crate::permit::routine(&prefs, &data["tool_call"], Path::new(&thread.cwd)));
        }
        // Design mode writes the design document and nothing else.
        if crate::design::is_on(&self.ctx(), thread_id).await
            && let Some(verdict) = crate::permit::hub_files(&data["tool_call"])
                .or_else(|| crate::permit::design(&data["tool_call"], Path::new(&thread.cwd)))
        {
            return Some(verdict);
        }
        // Vakten sorts problems with Hub's tools and changes no files itself.
        if crate::vakten_thread::is_vakten(&self.store, thread_id).await {
            return Some(crate::permit::vakten(&prefs, &data["tool_call"], Path::new(&thread.cwd)));
        }
        // A fix thread from Problems or a bench case runs in the background and never waits for the user.
        if let Ok(Some(by)) = self.store.unattended(thread_id).await {
            return Some(crate::permit::unattended_by(&by.by, &prefs, &data["tool_call"], Path::new(&thread.cwd)));
        }
        crate::permit::decide(&prefs, &data["tool_call"], Path::new(&thread.cwd))
    }
}

/// Whether `request_id` was asked, put to sleep on (`Hub::sleep_waiting`) and not answered since.
fn sleeping_question(events: &[Event], request_id: &str) -> bool {
    let slept = events.iter().filter_map(Event::data_as::<SleptData>).any(|s| s.ids.iter().any(|i| i == request_id));
    slept && crate::model::open_questions(events).iter().any(|id| id == request_id)
}

#[cfg(test)]
mod sleep_tests {
    use crate::model::{Event, EventKind};
    use serde_json::json;

    fn ev(seq: i64, kind: EventKind, data: serde_json::Value) -> Event {
        Event { thread_id: "t".into(), seq, ts: seq, kind, data }
    }

    #[test]
    fn only_a_question_that_slept_and_is_still_open_wakes_the_session() {
        let mut events = vec![ev(1, EventKind::PermissionRequest, json!({ "id": "a" })), ev(2, EventKind::PermissionRequest, json!({ "id": "b" }))];
        assert!(!super::sleeping_question(&events, "a"), "asked, but not asleep: that is a card from before a restart");
        events.push(ev(3, EventKind::Slept, json!({ "ids": ["a"] })));
        assert!(super::sleeping_question(&events, "a"));
        assert!(!super::sleeping_question(&events, "b"));
        events.push(ev(4, EventKind::PermissionResolved, json!({ "id": "a", "woke": true })));
        assert!(!super::sleeping_question(&events, "a"), "answered once");
    }
}
