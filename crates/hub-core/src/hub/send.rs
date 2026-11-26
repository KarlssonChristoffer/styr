//! Sending a message: the turn itself, undo and edit-from-here, tidy and dictation clean-up, hand-off and titles,
//! and the log helpers that build a hand-over.

use crate::locked::ResultExt;
use super::*;

mod notes;
use notes::{Notes, add_notes, switch_since};

impl Hub {
    pub async fn send(
        self: &Arc<Self>,
        thread_id: &str,
        text: String,
        attachments: Vec<Attachment>,
        original: Option<String>,
    ) -> Result<()> {
        self.send_from(thread_id, text, attachments, original, None, None).await
    }

    /// Sends a message once per `client_id`: a retry of the same request, also one after the
    /// core restarted in between, does nothing. The message and its slip are logged together.
    pub async fn send_once(
        self: &Arc<Self>,
        thread_id: &str,
        text: String,
        attachments: Vec<Attachment>,
        original: Option<String>,
        client_id: Option<String>,
        steer: bool,
    ) -> Result<()> {
        let typed = text.clone();
        let plain = attachments.is_empty();
        let said = self.send_inner(thread_id, text, attachments, original, None, client_id).await?;
        // A message that waits behind a turn: the decision model reads it when there is one
        // (`judge.rs`), otherwise steering is the person's own setting. One that goes straight in
        // needs neither.
        if let Some(seq) = said && self.queued_behind(thread_id, seq).await? {
            if plain && self.helper("judge").await.is_some() {
                tokio::spawn(self.clone().judge(thread_id.to_owned(), seq, typed, steer));
            } else if steer {
                self.steer(thread_id, seq).await?;
            }
        }
        Ok(())
    }

    /// Sends a message; `mark` marks one Hub sent itself, as `review: id` with a second opinion's
    /// findings, `leak: seq` with the leak guard's, `proof: seq` asking for proof, `quota: seq`
    /// carrying on after a plan limit reset, or one an agent sent through Hub's tools, as
    /// `from: { thread, title } | { agent }`. Surfaces show
    /// those as part of what sent them rather than as something the user wrote.
    pub(crate) async fn send_from(
        self: &Arc<Self>,
        thread_id: &str,
        text: String,
        attachments: Vec<Attachment>,
        original: Option<String>,
        mark: Option<(&str, serde_json::Value)>,
        client_id: Option<String>,
    ) -> Result<()> {
        self.send_inner(thread_id, text, attachments, original, mark, client_id).await.map(|_| ())
    }

    /// [`Hub::send_from`], with the seq of the message it logged (`None` for a repeated request).
    /// What a message does to its thread besides going to the engine: the topic check, a title when
    /// the messages so far said little, and back from the archive or the settled shelf.
    async fn note_message(
        self: &Arc<Self>,
        thread: Thread,
        said_seq: i64,
        text: &str,
        (from_hub, said_little, had_events, has_turn_end): (bool, bool, bool, bool),
    ) -> Result<Thread> {
        // A message you wrote to a thread that has finished a turn: is it the same work?
        if !from_hub && !text.trim().is_empty() && has_turn_end {
            tokio::spawn(self.clone().check_topic(thread.id.clone(), said_seq, text.to_owned()));
        }
        // Greetings make poor titles. Until a message says something, the latest one names it.
        if !thread.title_locked && said_little && had_events && !text.trim().is_empty() {
            let title = title_from_message(text);
            self.update_thread(&thread.id, move |t| t.title = title).await?;
        }
        // Writing to a thread brings it back from the archive or the settled shelf.
        let now = now_ms();
        self.update_thread(&thread.id, move |t| {
                t.archived = false;
                t.settled_at = None;
                t.snoozed_until = None;
                t.sent_at = Some(now);
                t.reading = None;
            })
            .await
    }

    async fn send_inner(
        self: &Arc<Self>,
        thread_id: &str,
        text: String,
        attachments: Vec<Attachment>,
        original: Option<String>,
        mark: Option<(&str, serde_json::Value)>,
        client_id: Option<String>,
    ) -> Result<Option<i64>> {
        if text.trim().is_empty() && attachments.is_empty() {
            bail!("empty message");
        }
        let mut thread = self.store.thread(thread_id).await?.context("no such thread")?;
        if thread.status == ThreadStatus::InTerminal {
            bail!("this thread is open in a terminal; take it back first");
        }
        // The clean-up may have removed the thread's worktree once its work was pushed.
        self.bring_back_folder(&thread).await;
        // What you write instead takes the place of carrying on after a limit.
        if mark.is_none() {
            self.stop_waiting(thread_id, "message").await?;
        }
        // The log is mostly `update` payloads, so only its markers are read: from the last turn end
        // or the last message on (whichever is earlier), without the updates.
        let last_end = self.store.last_seq(thread_id, Some(EventKind::TurnEnd)).await?;
        let last_said = self.store.last_seq(thread_id, Some(EventKind::UserMessage)).await?;
        let after = match (last_end, last_said) {
            (Some(end), Some(said)) => end.min(said),
            _ => 0,
        };
        let recent = self.store.events_except(thread_id, after, &[EventKind::Update]).await?;
        let had_events = self.store.last_seq(thread_id, None).await?.is_some();
        let has_turn_end = last_end.is_some();
        // Another engine took over, or the person went back and edited an earlier message, and
        // no turn has finished since: this message carries the conversation to the fresh
        // session, again if the first one after it failed.
        let since_turn: Vec<&Event> = recent.iter().rev().take_while(|e| e.kind != EventKind::TurnEnd).collect();
        let switched = switch_since(&since_turn);
        // A handover carries the whole conversation; only then is the whole log read.
        let previous = match &switched {
            Some(_) => self.store.events(thread_id, 0, 100_000).await?,
            None => Vec::new(),
        };
        // Whether every message so far said little (the greeting that makes a poor title).
        let said_little = self
            .store
            .events_of(thread_id, 0, &[EventKind::UserMessage])
            .await?
            .iter()
            .all(|e| word_count(e.said().unwrap_or_default()) < 4);
        let mut data = json!({ "text": text, "attachments": attachments });
        if let Some(switch) = &switched {
            let key = if switch.kind == EventKind::Rewind { "rewind" } else { "handoff" };
            data[key] = json!(switch.seq);
        }
        if let Some(original) = original.filter(|o| o != &text) {
            data["original"] = json!(original);
        }
        let from_hub = mark.is_some();
        let from_mark: Option<String> = mark.as_ref().map(|(key, _)| (*key).to_owned());
        if let Some((key, value)) = mark {
            data[key] = value;
        }
        let said = match client_id {
            Some(client_id) => match self.emit_once(thread_id, EventKind::UserMessage, data, &client_id).await? {
                Some(said) => said,
                None => return Ok(None),
            },
            None => self.emit(thread_id, EventKind::UserMessage, data).await?,
        };
        thread = self.note_message(thread, said.seq, &text, (from_hub, said_little, had_events, has_turn_end)).await?;
        // The engine still remembers the work that was rolled back; tell it once.
        let said_text = text.clone();
        let since: Vec<&Event> = recent.iter().rev().take_while(|e| e.kind != EventKind::UserMessage).collect();
        let Notes { text, parts, purpose, recipe } = add_notes(text, &since, switched.as_ref(), &previous, thread_id);
        let purpose = purpose.map(str::to_owned).unwrap_or_else(|| from_mark.map_or_else(|| "message".to_owned(), |m| format!("hub:{m}")));
        // Paused from the panic button: the message is logged and goes out on resume.
        if self.halted() {
            return Ok(Some(said.seq));
        }
        // A cloud thread has no engine here: the message goes to its session.
        if thread.cloud.is_some() {
            crate::cloud_threads::prompt(self, &thread, text).await?;
            return Ok(Some(said.seq));
        }
        let idle = matches!(thread.status, ThreadStatus::Idle | ThreadStatus::Error);
        let commands = self.runtime(thread)?;
        commands
            .send(Command::Prompt(Prompt { text, attachments, seq: said.seq, said: said_text, purpose, parts, recipe }))
            .context("engine stopped")?;
        // Working from the moment the message is in, not once the engine is up: starting one
        // can take seconds, and a thread that looks idle meanwhile looks like it dropped it.
        if idle {
            self.set_status(thread_id, ThreadStatus::Running).await?;
        }
        Ok(Some(said.seq))
    }

    /// Snapshots the thread's repository before message `seq` reaches the engine. A failure is
    /// logged and never holds the message up.
    pub(crate) async fn checkpoint(&self, thread: &Thread, seq: i64) {
        if !self.config.checkpoints {
            return;
        }
        let cwd = PathBuf::from(&thread.cwd);
        let name = format!("{}/{seq}", thread.id);
        let message = format!("Hub: before message {seq} of thread {}", thread.id);
        let taken = tokio::task::spawn_blocking(move || crate::checkpoint::take(&cwd, &name, &message)).await;
        match taken {
            Ok(Ok(Some(s))) => {
                let data = CheckpointData { seq: Some(seq), commit: s.commit, root: s.root.display().to_string(), head: Some(s.head), branch: Some(s.branch) };
                self.emit_typed(&thread.id, &data).await.warn_on_err("emit Checkpoint");
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => tracing::warn!(thread = %thread.id, "no checkpoint: {e:#}"),
            Err(e) => tracing::warn!(thread = %thread.id, "no checkpoint: {e}"),
        }
    }

    /// A snapshot of this thread's to roll back to, found in its log: a checkpoint, or the state
    /// an earlier rollback replaced.
    async fn undo_target(&self, thread_id: &str, to: &str) -> Result<(Thread, PathBuf, Option<i64>, crate::checkpoint::Target)> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let events = self.store.events_of(thread_id, 0, &[EventKind::Checkpoint, EventKind::Restored, EventKind::Attempt]).await?;
        let target = |head: Option<Option<String>>, branch: Option<Option<String>>| crate::checkpoint::Target { commit: to.into(), head: head.flatten(), branch: branch.flatten() };
        for e in events.iter().rev() {
            // The snapshot, the message it was taken before, and the repository it is of.
            let found = match e.kind {
                EventKind::Checkpoint => e.data_as::<CheckpointData>().filter(|c| written(&c.commit) == Some(to)).map(|c| (c.seq, target(c.head, c.branch), c.root)),
                EventKind::Restored => {
                    e.data_as::<RestoredData>().filter(|r| written(&r.safety) == Some(to)).map(|r| (None, target(r.safety_head, r.safety_branch), r.root))
                }
                EventKind::Attempt => e
                    .data_as::<AttemptData>()
                    .filter(|a| a.safety.as_deref() == Some(to))
                    .map(|a| (None, target(a.safety_head, a.safety_branch), a.root.unwrap_or_default())),
                _ => None,
            };
            if let Some((seq, target, root)) = found {
                let root = PathBuf::from(written(&root).unwrap_or(&thread.cwd));
                return Ok((thread, root, seq, target));
            }
        }
        bail!("this thread has no checkpoint {to}")
    }

    /// Every file that differs from the thread's first snapshot, so a file changed by a command
    /// counts as much as one the agent edited. None when the thread has no snapshot (no git).
    pub async fn thread_changes(&self, thread_id: &str) -> Result<Option<Vec<ThreadChange>>> {
        let events = self.store.events_of(thread_id, 0, &[EventKind::Checkpoint]).await?;
        let Some(first) = events.iter().find(|e| e.kind == EventKind::Checkpoint) else {
            return Ok(None);
        };
        let first = first.data_as::<CheckpointData>().unwrap_or_default();
        let root = PathBuf::from(written(&first.root).context("bad checkpoint")?);
        let base = written(&first.commit).context("bad checkpoint")?.to_owned();
        let files = tokio::task::spawn_blocking(move || -> Result<_> {
            let files = crate::checkpoint::diff_now(&root, &base)?;
            Ok(files
                .into_iter()
                .map(|f| ThreadChange { path: root.join(&f.path).display().to_string(), diff: f.diff, added: f.added, removed: f.removed })
                .collect())
        })
        .await??;
        Ok(Some(files))
    }

    /// What rolling the thread's repository back to snapshot `to` would change.
    pub async fn undo_preview(&self, thread_id: &str, to: &str) -> Result<UndoPreview> {
        let (_, root, _, target) = self.undo_target(thread_id, to).await?;
        tokio::task::spawn_blocking(move || crate::checkpoint::preview(&root, &target)).await?
    }

    /// Rolls the thread's repository back to snapshot `to`: its files, and its commits unless
    /// they were pushed or the branch changed. How it was is kept, so this can be undone too.
    pub async fn undo(&self, thread_id: &str, to: &str) -> Result<UndoPreview> {
        let (thread, root, seq, target) = self.undo_target(thread_id, to).await?;
        if matches!(thread.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou | ThreadStatus::InTerminal) {
            bail!("stop the thread before rolling back");
        }
        let safety = format!("{thread_id}/undo-{}", now_ms());
        let at = root.clone();
        let (before, done) =
            tokio::task::spawn_blocking(move || crate::checkpoint::restore(&at, &target, &safety)).await??;
        let restored = RestoredData {
            to: to.to_owned(),
            seq: Some(seq),
            root: root.display().to_string(),
            files: done.files.len() as u64,
            commits: if done.kept_commits.is_some() { 0 } else { done.commits.len() as u64 },
            kept_commits: Some(done.kept_commits.clone()),
            safety: before.commit,
            safety_head: Some(before.head),
            safety_branch: Some(before.branch),
        };
        self.emit_typed(thread_id, &restored).await?;
        Ok(done)
    }

    /// "Edit from here": goes back to user message `seq` and sends `text` in its place. A turn in
    /// progress is stopped and messages still waiting in the queue are removed. With `files`, the
    /// repository goes back to the checkpoint taken before `seq` too (an ordinary rollback, so it
    /// can be undone). ACP has no way to cut a session short, so the engine's session is left
    /// behind and the next message hands what was said before `seq` to a fresh one, as an engine
    /// switch does. What came after stays in the log, marked by a `rewind` event.
    pub async fn edit_from(
        self: &Arc<Self>,
        thread_id: &str,
        seq: i64,
        text: String,
        attachments: Vec<Attachment>,
        files: bool,
    ) -> Result<()> {
        if text.trim().is_empty() && attachments.is_empty() {
            bail!("empty message");
        }
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        if thread.status == ThreadStatus::InTerminal {
            bail!("this thread is open in a terminal; take it back first");
        }
        if thread.host.is_some() {
            bail!("this thread runs on another machine");
        }
        let events = self.store.events_of(thread_id, 0, &[EventKind::UserMessage, EventKind::Checkpoint, EventKind::Rewind]).await?;
        let said = events
            .iter()
            .find(|e| e.seq == seq && e.kind == EventKind::UserMessage)
            .with_context(|| format!("no message {seq} in this thread"))?;
        if cut_seqs(&events).iter().any(|(from, to)| (*from..*to).contains(&seq)) {
            bail!("this message was already edited away");
        }
        if said.said().is_none() || HUB_MARKS.iter().any(|k| !said.mark(k).is_null()) {
            bail!("Hub sent this message itself; edit one of yours");
        }
        let checkpoint = events
            .iter()
            .filter_map(Event::data_as::<CheckpointData>)
            .find(|c| c.seq == Some(seq))
            .and_then(|c| written(&c.commit).map(str::to_owned));
        if files && checkpoint.is_none() {
            bail!("there is no snapshot of the files from before this message");
        }
        // Later messages still waiting would otherwise run after the edited one.
        for later in events.iter().filter(|e| e.kind == EventKind::UserMessage && e.seq > seq) {
            self.change_queued(thread_id, later.seq, None).await.warn_on_err("unqueue a later message");
        }
        self.shutdown(thread_id, false).await?;
        self.cancel_permissions(thread_id).await;
        if matches!(self.store.thread(thread_id).await?.map(|t| t.status), Some(ThreadStatus::Running | ThreadStatus::WaitingOnYou)) {
            self.update_thread(thread_id, |t| t.status = ThreadStatus::Idle).await?;
        }
        if let Some(commit) = checkpoint.filter(|_| files) {
            self.undo(thread_id, &commit).await?;
        }
        self.emit_typed(thread_id, &RewindData { from: Some(seq), files, by: "you".into() }).await?;
        self.update_thread(thread_id, |t| t.engine_session_id = None).await?;
        // A message let go as the engine stopped may have started it again on the old session.
        self.shutdown(thread_id, false).await?;
        self.send_inner(thread_id, text, attachments, None, None, None).await?;
        Ok(())
    }

    /// Turns a long, messy message into a clear prompt with one answer from a small model.
    pub async fn tidy(&self, text: &str, cwd: Option<&str>) -> Result<String> {
        if text.trim().is_empty() {
            bail!("nothing to tidy");
        }
        let (name, tidy_model) = self.tidy_target(cwd).await;
        let engine = self.config.engines.get(&name).with_context(|| format!("unknown engine {name:?}"))?;
        let cwd = cwd
            .filter(|c| Path::new(c).is_dir())
            .map(str::to_owned)
            .unwrap_or_else(|| std::env::temp_dir().display().to_string());
        let prompt = format!("{TIDY}\n\n<dump>\n{text}\n</dump>");
        let answer = engine::complete("tidy", engine, &cwd, tidy_model.as_deref(), prompt);
        tokio::time::timeout(std::time::Duration::from_secs(120), answer)
            .await
            .context("tidying took too long")?
    }

    /// Tidies dictated text lightly with the small model tidying uses: punctuation, spelling of
    /// names and tech words, filler words out. Unlike `tidy` it keeps the person's own wording.
    /// An answer that is far off in length (the model answered instead of correcting) is dropped
    /// and the text comes back as it was.
    pub async fn polish_dictation(&self, text: &str, cwd: Option<&str>) -> Result<String> {
        if text.trim().is_empty() {
            return Ok(text.to_owned());
        }
        let (name, tidy_model) = self.tidy_target(cwd).await;
        let engine = self.config.engines.get(&name).with_context(|| format!("unknown engine {name:?}"))?;
        let cwd = cwd
            .filter(|c| Path::new(c).is_dir())
            .map(str::to_owned)
            .unwrap_or_else(|| std::env::temp_dir().display().to_string());
        let prompt = format!("{POLISH}\n\n<dictated>\n{}\n</dictated>", text.trim());
        let answer = engine::complete("polish", engine, &cwd, tidy_model.as_deref(), prompt);
        let answer = tokio::time::timeout(std::time::Duration::from_secs(30), answer)
            .await
            .context("tidying the dictation took too long")??;
        Ok(polished(text, &answer))
    }

    /// Adds a rule to the rules file of the thread's engine (`EngineTraits::rules_file`) in its
    /// folder, under a heading of its own. The engine reads it when its next session starts.
    pub async fn save_rule(&self, thread_id: &str, text: &str, seq: i64) -> Result<()> {
        let rule = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if rule.is_empty() {
            bail!("an empty rule");
        }
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let path = Path::new(&thread.cwd).join(self.config.traits(&thread.engine).rules_file());
        let file = path.clone();
        let text = rule.clone();
        tokio::task::spawn_blocking(move || add_rule(&file, &text)).await??;
        self.emit_typed(thread_id, &RuleSavedData { text: rule, path: path.display().to_string(), seq }).await?;
        Ok(())
    }

    /// A prompt that lets a fresh thread carry on where this one is, written from its log.
    pub async fn handoff(&self, thread_id: &str) -> Result<String> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let log = conversation_log(&self.store.events(thread_id, 0, 100_000).await?);
        let name = &thread.engine;
        let engine = self.config.engines.get(name).with_context(|| format!("unknown engine {name:?}"))?;
        let prompt = format!(
            "{HANDOFF}\n\nThread: {}\nFolder: {}\n\n<conversation>{log}\n</conversation>",
            thread.title, thread.cwd
        );
        let answer = engine::complete("handoff", engine, &thread.cwd, None, prompt);
        tokio::time::timeout(std::time::Duration::from_secs(240), answer)
            .await
            .context("writing the handoff took too long")?
    }

    /// Names the thread again from what it turned into, with the small model tidying uses. The
    /// name is kept like one the user gave, so automatic titles leave it alone.
    pub async fn regenerate_title(&self, thread_id: &str) -> Result<Thread> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let log = conversation_log(&self.store.events(thread_id, 0, 100_000).await?);
        // The start says what it was for, the end what it became.
        let chars: Vec<char> = log.chars().collect();
        let log: String = if chars.len() > 12_000 {
            chars[..6_000].iter().chain(['\n', '…', '\n'].iter()).chain(chars[chars.len() - 6_000..].iter()).collect()
        } else {
            log
        };
        let (name, tidy_model) = self.tidy_target(Some(&thread.cwd)).await;
        let engine = self.config.engines.get(&name).with_context(|| format!("unknown engine {name:?}"))?;
        let prompt = format!("{RETITLE}\n\n<conversation>{log}\n</conversation>");
        let answer = engine::complete("title", engine, &thread.cwd, tidy_model.as_deref(), prompt);
        let answer = tokio::time::timeout(std::time::Duration::from_secs(90), answer).await.context("naming took too long")??;
        let title: String = answer.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().trim_matches(['"', '*', '#', ' ']).chars().take(80).collect();
        if title.is_empty() {
            bail!("the model gave no title");
        }
        self.update_thread(thread_id, move |t| {
            t.title = title;
            t.title_locked = true;
        })
        .await
    }
}

const ROLLED_BACK: &str = "(Note from Hub: the user rolled the repository back with Hub's undo since \
your last turn, so files you changed may be as they were before. Check before relying on earlier work.)";

/// Marks on a user message that say Hub sent it itself, not the person.
const HUB_MARKS: &[&str] = &["review", "leak", "customs", "proof", "docs", "quota", "restart", "terminal", "watch", "child", "routine", "from", "vakten"];

/// The parts of a log that "Edit from here" left behind, as `[from, to)` seq ranges: from the
/// edited message up to the `rewind` event.
pub(crate) fn cut_seqs(events: &[Event]) -> Vec<(i64, i64)> {
    events
        .iter()
        .filter_map(|e| Some((e.data_as::<RewindData>()?.from?, e.seq)))
        .collect()
}

/// A thread's log as turns: `(seq of its first event, "\n\nUser: …")`. Consecutive chunks of one
/// speaker are one turn; every tool call is its own. What an edit left behind is not part of it.
fn log_turns(events: &[Event]) -> Vec<(i64, String)> {
    let cut = cut_seqs(events);
    let mut turns: Vec<(i64, String)> = Vec::new();
    let mut speaker = "";
    for e in events.iter().filter(|e| !cut.iter().any(|(from, to)| (*from..*to).contains(&e.seq))) {
        let (who, text) = match e.kind {
            EventKind::UserMessage => ("User", e.said().unwrap_or_default()),
            EventKind::Update if crate::model::update_kind(&e.data) == Some("agent_message_chunk") => {
                ("Agent", e.data["content"]["text"].as_str().unwrap_or_default())
            }
            EventKind::Update if crate::model::update_kind(&e.data) == Some("tool_call") => {
                ("Tool", e.data["title"].as_str().unwrap_or_default())
            }
            _ => continue,
        };
        if who != speaker || who == "Tool" {
            turns.push((e.seq, format!("\n\n{who}: ")));
            speaker = who;
        }
        if let Some((_, last)) = turns.last_mut() {
            last.push_str(text);
        }
    }
    turns
}

/// What was said in a thread, as `User:`, `Agent:` and `Tool:` lines, cut to fit a prompt.
/// What an edit left behind is not part of it.
pub(crate) fn conversation_log(events: &[Event]) -> String {
    let log: String = log_turns(events).into_iter().map(|(_, t)| t).collect();
    // The start says what the work is, the end where it stands; the middle can go.
    const KEEP: usize = 60_000;
    let chars: Vec<char> = log.chars().collect();
    if chars.len() > KEEP {
        let head: String = chars[..KEEP / 4].iter().collect();
        let tail: String = chars[chars.len() - KEEP * 3 / 4..].iter().collect();
        return format!("{head}\n\n[… middle of the conversation left out …]\n\n{tail}");
    }
    log
}

/// The characters of conversation a handover carries (about 6 000 tokens): the recipe
/// `handover-v1` of `package/`.
pub(crate) const HANDOVER_CHARS: usize = 24_000;

/// The conversation a fresh session is handed (model switch, rewind, move, lost session): the
/// start, which says what the work is, and the latest part, which says where it stands, within
/// [`HANDOVER_CHARS`]. What goes between is named by where it is in the log, so the new agent
/// can read exactly that with `read_thread` when the message needs it, instead of getting
/// everything up front. Returns the text and the recipe that made it; a thread that fits is
/// handed whole by the old recipe's rules (`legacy`).
pub(crate) fn handover_log(thread_id: &str, events: &[Event]) -> (String, &'static str) {
    let turns = log_turns(events);
    let size = |t: &(i64, String)| t.1.chars().count();
    let total: usize = turns.iter().map(size).sum();
    if total <= HANDOVER_CHARS {
        return (conversation_log(events), "legacy");
    }
    // The start, up to a quarter of the budget; the end, up to the rest. A turn longer than its
    // share is cut at its own start or end, so the first message and the last answer survive.
    let (mut head, mut head_chars) = (0, 0);
    while head < turns.len() && (head == 0 || head_chars + size(&turns[head]) <= HANDOVER_CHARS / 4) {
        head_chars += size(&turns[head]);
        head += 1;
    }
    let head_chars = head_chars.min(HANDOVER_CHARS / 4);
    let (mut tail, mut tail_chars) = (turns.len(), 0);
    while tail > head && (tail == turns.len() || tail_chars + size(&turns[tail - 1]) <= HANDOVER_CHARS - head_chars) {
        tail_chars += size(&turns[tail - 1]);
        tail -= 1;
    }
    let share = HANDOVER_CHARS / 4;
    let clip = |text: &str, from_end: bool, to: usize| -> String {
        let chars: Vec<char> = text.chars().collect();
        if chars.len() <= to {
            return text.to_owned();
        }
        if from_end { chars[chars.len() - to..].iter().collect() } else { chars[..to].iter().collect() }
    };
    let mut out = String::new();
    for (i, (_, t)) in turns[..head].iter().enumerate() {
        out.push_str(&if i == 0 { clip(t, false, share) } else { t.clone() });
    }
    let left_out = tail - head;
    if left_out > 0 {
        let after = turns[head - 1].0;
        out.push_str(&format!(
            "\n\n[… {left_out} turns in between are left out. They are in this thread's log: read_thread with \
thread_id {thread_id} and after_seq {after} reads them, only if the message needs them …]"
        ));
        out.push_str(&left_out_asks(&turns[head..tail]));
    }
    let last = turns.len() - 1;
    for (i, (_, t)) in turns[tail..].iter().enumerate() {
        out.push_str(&if tail + i == last { clip(t, true, HANDOVER_CHARS / 2) } else { t.clone() });
    }
    (out, "handover-v1")
}

/// What the person asked in the turns a handover leaves out, one short line each, so the new agent
/// knows the direction of the gap without reading it. Stops at [`ASKS_CHARS`] and says how many it
/// did not list.
fn left_out_asks(left_out: &[(i64, String)]) -> String {
    let asks: Vec<String> = left_out
        .iter()
        .filter_map(|(_, t)| t.strip_prefix("\n\nUser: "))
        .map(|t| {
            let one: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
            if one.chars().count() > ASK_CHARS { format!("{}…", one.chars().take(ASK_CHARS).collect::<String>()) } else { one }
        })
        .collect();
    if asks.is_empty() {
        return String::new();
    }
    let (mut out, mut listed) = (String::from("\n[What the person asked in between:"), 0);
    for ask in &asks {
        if out.chars().count() + ask.chars().count() > ASKS_CHARS {
            break;
        }
        out.push_str(&format!("\n- {ask}"));
        listed += 1;
    }
    if listed < asks.len() {
        out.push_str(&format!("\n- … and {} more", asks.len() - listed));
    }
    out.push_str("\n]");
    out
}

/// One of the person's messages in the left-out part of a handover, cut to this many characters.
const ASK_CHARS: usize = 160;
/// All of those lines together.
const ASKS_CHARS: usize = 2_400;

fn switched_note(from: &str) -> String {
    format!(
        "(Note from Hub: you are taking over this thread from another agent, {from}. Below is the \
conversation so far, for context only; the work it describes is in the folder, and where the two \
differ the folder is right. Open a file when the new message needs it rather than reading everything \
first, and answer the message after it.)"
    )
}

/// For the first message after a thread moved here from machine `from`.
pub(crate) fn moved_note(from: &str) -> String {
    format!(
        "(Note from Hub: this thread was moved here from another machine, {from}, and carries on in a \
fresh session. Below is the conversation so far, for context only; the folder here has the same commit, \
branch and uncommitted changes, and where the two differ the folder is right. Open a file when the new \
message needs it rather than reading everything first, and answer the message after it.)"
    )
}

fn rewound_note(files: bool) -> String {
    let folder = if files {
        "The files were rolled back to how they were at that point too."
    } else {
        "The files were not rolled back, so the folder may still hold work from the part that was left \
out; check before relying on it."
    };
    format!(
        "(Note from Hub: the person went back to an earlier point of this conversation and wrote the \
message below in place of what followed, so this is a fresh session. Below is the conversation up \
to that point, for context only. {folder} Answer the message after it.)"
    )
}

const HANDOFF: &str = "Below is a conversation between a person and a coding agent. Do not continue \
it and do not use tools. Write the first message for a fresh agent that takes over in the same \
folder: what the goal is, what has been done and decided, what is in progress, what is left, \
and anything to watch out for. Name files, commands and branches exactly. Write it as the person \
speaking to the new agent, in the person's language, and output only that message.";

const RETITLE: &str = "Name this coding conversation for a list of threads. Answer with the name \
only: three to six words, sentence case, no quotes and no full stop. Lead with the verb of the goal, \
like \"Fix Safari export button\", and name the specific feature or bug in the user's words. Do not \
use tools.";

/// The model's tidy version of dictated `text`, or `text` itself when the answer is empty or
/// far off in length.
fn polished(text: &str, answer: &str) -> String {
    let answer = answer.trim().trim_start_matches("<dictated>").trim_end_matches("</dictated>").trim();
    let (was, now) = (text.trim().chars().count() as f64, answer.chars().count() as f64);
    if answer.is_empty() || now > was * 1.5 + 20.0 || now < was * 0.6 {
        return text.trim().to_owned();
    }
    answer.to_owned()
}

const POLISH: &str = "You correct text a person dictated with speech recognition, to be sent \
as a message to a coding agent. Do not answer it, act on it or use tools: only correct it. Fix \
punctuation and capitals, misheard words (especially names, code terms, file names and English \
tech words inside Swedish), and remove filler words, false starts and repeated words. Keep the \
person's own wording, order and language (Swedish stays Swedish); do not shorten, summarise, \
restructure or add anything. Output only the corrected text.";

const TIDY: &str = "You turn a person's messy, often dictated brain dump into a clear prompt for \
a coding agent. Do not act on it, do not use tools and do not answer it: only rewrite it. Fix \
obvious transcription errors and put the thoughts in order under these headings: Task, Why, \
Guardrails, What done looks like, Open questions. Leave out a heading that would be empty. Keep \
the person's meaning, decisions and language (Swedish stays Swedish); add nothing they did not \
say. Copy code, paths, commands, quotes and creative writing verbatim. Output only the prompt.";

#[cfg(test)]
mod polish_tests {
    #[test]
    fn a_tidy_dictation_keeps_the_wording_or_is_dropped() {
        let said = "eh kan du titta på voice rs och fixa testet";
        assert_eq!(super::polished(said, " Kan du titta på voice.rs och fixa testet? \n"), "Kan du titta på voice.rs och fixa testet?");
        // An answer to the message instead of a correction of it.
        let answer = "Visst! Jag har tittat på voice.rs och här är vad jag hittade: ".repeat(4);
        assert_eq!(super::polished(said, &answer), said);
        assert_eq!(super::polished(said, ""), said);
    }
}

#[cfg(test)]
mod handover_tests;
