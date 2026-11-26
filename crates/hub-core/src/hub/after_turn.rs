//! What Hub looks at once a turn ends: edits, leaks, proof, docs.

use crate::locked::ResultExt;
use super::*;

const LEAKED: &str = "Hub's leak guard found what looks like a secret in lines you added:";

const LEAKED_TAIL: &str = "Keep secrets out of the code: read them from an environment variable or from a file \
git ignores (such as a .env listed in .gitignore). If one was already committed or pushed, say so plainly, since \
that key must then be replaced. If one is a harmless test value, say so in one sentence.";

const STOPPED: &str = "Hub's Customs looked at what your last turn brought in. These were refused, so they may not be used:";

const STOPPED_TAIL: &str = "A refused one must come out again; customs_search shows why, and something cleared may do the job.";

const DOCS_UNCHECKED: &str = "Hub: your last reply changed code that these doc sections describe, without opening them:";

const DOCS_UNCHECKED_TAIL: &str = "Read each one and update what your change made untrue or incomplete, in the same words \
and style. If it all still holds, say so in one sentence.";

const PROOF_FAILED: &str = "Hub: the last test run in your reply failed (`{command}`). Fix it, or explain why the \
failure is unrelated, then run it again and say what passed.";

const PROOF_HIDDEN: &str = "Hub: the last test run in your reply (`{command}`) exited cleanly, but its output reads \
as if something failed. Look at the whole output, not only its exit code (a pipe like `| tail` hides it). Fix \
what failed, or say what the output shows and why it is fine, then run it again and say what passed.";

/// What a feature above Hub does once an event is logged: gets the hub and the event, and runs in
/// the same task as the rest of [`Hub::look_at`].
pub type LookHook = fn(Arc<Hub>, Event) -> futures::future::BoxFuture<'static, ()>;

impl Hub {
    /// Adds what runs on each event that `look_at` sees (edits, tool calls, errors and turn ends).
    /// Problems sits above Hub, so hubd installs it (`problems::install`); Hub does not name it.
    pub fn on_look(&self, hook: LookHook) {
        self.look_hooks.locked().push(hook);
    }

    async fn run_look_hooks(self: &Arc<Self>, event: &Event) {
        let hooks: Vec<LookHook> = self.look_hooks.locked().clone();
        for hook in hooks {
            hook(self.clone(), event.clone()).await;
        }
    }

    /// A second look at an event once it is logged: edits for clashes, turn ends for leaks and
    /// proof, errors for plan limits. Runs in its own task, so the engine never waits for it.
    pub(super) async fn look_at(self: Arc<Self>, event: Event) {
        match event.kind {
            EventKind::Error => {
                if let Err(e) = crate::routines::finished(&self.ctx(), &event, |thread| self.publish(StreamMsg::Thread { thread })).await {
                    tracing::warn!(thread = %event.thread_id, "ending a routine's run: {e:#}");
                }
                if let Err(e) = crate::widgets::finished(&self.ctx(), &event).await {
                    tracing::warn!(thread = %event.thread_id, "ending a widget's build: {e:#}");
                }
                if let Err(e) = self.child_finished(&event).await {
                    tracing::warn!(thread = %event.thread_id, "sending a side job back: {e:#}");
                }
                if let Err(e) = self.after_error(&event).await {
                    tracing::warn!(thread = %event.thread_id, "after a plan limit: {e:#}");
                }
                if let Err(e) = self.after_sign_in_error(&event).await {
                    tracing::warn!(thread = %event.thread_id, "after a lost sign-in: {e:#}");
                }
                self.run_look_hooks(&event).await;
            }
            EventKind::Update => {
                self.run_look_hooks(&event).await;
                self.note_edits(&event).await
            }
            EventKind::TurnEnd => {
                if let Err(e) = crate::routines::finished(&self.ctx(), &event, |thread| self.publish(StreamMsg::Thread { thread })).await {
                    tracing::warn!(thread = %event.thread_id, "ending a routine's run: {e:#}");
                }
                if let Err(e) = crate::widgets::finished(&self.ctx(), &event).await {
                    tracing::warn!(thread = %event.thread_id, "ending a widget's build: {e:#}");
                }
                if let Err(e) = self.child_finished(&event).await {
                    tracing::warn!(thread = %event.thread_id, "sending a side job back: {e:#}");
                }
                if let Err(e) = self.after_turn(&event).await {
                    tracing::warn!(thread = %event.thread_id, "after the turn: {e:#}");
                }
                if let Err(e) = self.design_after_turn(&event).await {
                    tracing::warn!(thread = %event.thread_id, "checking a design turn: {e:#}");
                }
                self.run_look_hooks(&event).await;
                self.signed_in_again(&event.thread_id).await;
                tokio::spawn(self.clone().read_turn(event.clone()));
            }
            _ => {}
        }
    }

    /// Notes the files an edit touched, and warns both threads when another one changed them too.
    async fn note_edits(&self, event: &Event) {
        let Ok(Some(thread)) = self.store.thread(&event.thread_id).await else { return };
        if !self.guard_prefs().await.unwrap_or_default().clash_warn {
            return;
        }
        let files = crate::clash::edited(&event.data, Path::new(&thread.cwd));
        let found: Vec<(String, ThreadId)> = {
            let mut clashes = self.clashes.locked();
            files.iter().flat_map(|f| clashes.touch(&thread.id, f, event.ts).into_iter().map(move |o| (f.clone(), o))).collect()
        };
        for (path, other) in found {
            let Ok(Some(other)) = self.store.thread(&other).await else { continue };
            if other.archived {
                continue;
            }
            for (here, there) in [(&thread, &other), (&other, &thread)] {
                let data = ClashData { path: path.clone(), other: there.id.clone(), other_title: Some(there.title.clone()) };
                self.emit_typed(&here.id, &data).await.warn_on_err("emit Clash");
            }
        }
    }

    /// After a turn that changed the repository: the leak guard looks at the lines it added,
    /// Customs at what it brought in, and proof before done at what it ran. Each can send the
    /// agent one message about it.
    async fn after_turn(self: &Arc<Self>, end: &Event) -> Result<()> {
        if !end.ended_well() {
            return Ok(());
        }
        let prefs = self.guard_prefs().await.unwrap_or_default();
        if !prefs.proof && !prefs.leak_guard && !prefs.customs && !prefs.docs_check {
            return Ok(());
        }
        let thread_id = end.thread_id.as_str();
        // Before the turn only the kinds the checks read, since the log's updates are most of its
        // size; from the turn on everything, since the checks read what the turn did.
        let kinds = [EventKind::UserMessage, EventKind::Checkpoint, EventKind::Leak, EventKind::Customs, EventKind::Proof, EventKind::Docs];
        let mut events = self.store.events_of(thread_id, 0, &kinds).await?;
        let Some(asked_seq) = events.iter().rev().find(|e| e.seq < end.seq && e.kind == EventKind::UserMessage).map(|e| e.seq) else {
            return Ok(());
        };
        events.retain(|e| e.seq <= asked_seq);
        events.extend(self.store.events_except(thread_id, asked_seq, &[]).await?);
        let asked = events.iter().find(|e| e.seq == asked_seq).expect("the message was just read");
        let Some(checkpoint) = events.iter().filter_map(Event::data_as::<CheckpointData>).find(|c| c.seq == Some(asked.seq)) else {
            return Ok(());
        };
        let root = PathBuf::from(written(&checkpoint.root).context("bad checkpoint")?);
        let base = written(&checkpoint.commit).context("bad checkpoint")?.to_owned();
        let (files, brought) = tokio::task::spawn_blocking(move || -> Result<_> {
            // A folder that was a repository at the checkpoint can stop being one (its worktree
            // removed under the thread). Say that, not a bare git error.
            if !crate::checkpoint::is_repo(&root) {
                bail!("{} is no longer a git repository, so the checks after this turn did not run", root.display());
            }
            let files = crate::checkpoint::diff_now(&root, &base)?;
            let brought = crate::customs::in_turn(&root, &base, &files);
            Ok((files, brought))
        })
        .await??;
        let mut sent = false;

        if prefs.leak_guard {
            sent = self.leak_check(end, &events, &files).await?;
        }

        if prefs.customs && !brought.is_empty() {
            sent = self.customs_check(end, &events, &brought, sent).await?;
        }

        // A reply sent back over a leak is judged when the agent has answered it.
        // What Hub asks after this turn, in one message: proof first, then the docs.
        let mut ask: Vec<String> = Vec::new();
        let mut mark = "docs";
        if prefs.proof && !sent
            && let Some(message) = self.proof_check(&events, asked, &checkpoint, end, &files).await? {
                ask.push(message);
                mark = "proof";
            }

        if prefs.docs_check && !sent
            && let Some(message) = self.docs_check(&events, asked, &checkpoint, end, &files).await? {
                ask.push(message);
            }
        if !ask.is_empty() {
            self.send_from(thread_id, ask.join("\n\n"), Vec::new(), None, Some((mark, json!(end.seq))), None).await?;
        }
        Ok(())
    }

    /// Looks for secrets in what the turn wrote and tells the agent to take them out. `true` when
    /// it sent a message.
    async fn leak_check(self: &Arc<Self>, end: &Event, events: &[Event], files: &[crate::checkpoint::FileDiff]) -> Result<bool> {
        let thread_id = end.thread_id.as_str();
        let vault = self.secrets.cached();
        let known: std::collections::HashSet<(String, String)> = events
            .iter()
            .filter_map(Event::data_as::<LeakData>)
            .flat_map(|leak| leak.findings)
            .map(|f| (f.path, f.hint))
            .collect();
        let found: Vec<crate::leak::InFile> = files
            .iter()
            .flat_map(|f| crate::leak::in_diff(&f.path, &f.diff, &vault))
            .filter(|f| !known.contains(&(f.path.clone(), f.hint.clone())))
            .collect();
        if !found.is_empty() {
            let findings: Vec<LeakFinding> =
                found.iter().map(|f| LeakFinding { path: f.path.clone(), line: f.line as u64, kind: f.kind.clone(), hint: f.hint.clone() }).collect();
            let list: String = found.iter().map(|f| format!("- {}:{}: {} ({})\n", f.path, f.line, f.kind, f.hint)).collect();
            self.emit_typed(thread_id, &LeakData { seq: end.seq, findings, sent: true }).await?;
            let message = format!("{LEAKED}\n\n{list}\n{LEAKED_TAIL}");
            self.send_from(thread_id, message, Vec::new(), None, Some(("leak", json!(end.seq))), None).await?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Sends what the turn brought into Customs through it. A refusal goes back to the agent unless
    /// a message already went this turn (`sent`). Returns whether one has.
    async fn customs_check(self: &Arc<Self>, end: &Event, events: &[Event], brought: &[crate::customs::Found], mut sent: bool) -> Result<bool> {
        let thread_id = end.thread_id.as_str();
        let told: std::collections::HashSet<(String, String)> = events
            .iter()
            .filter_map(Event::data_as::<CustomsData>)
            .filter(|c| c.sent == Some(true))
            .flat_map(|c| c.found)
            .map(|f| (f.kind, f.name))
            .collect();
        let title = self.store.thread(thread_id).await?.map(|t| t.title).unwrap_or_default();
        let new: Vec<_> = crate::customs::hold(&self.ctx(), brought, &title, Some(thread_id))
            .await?
            .into_iter()
            .filter(|(f, _)| !told.contains(&(f.kind.clone(), f.name.clone())))
            .collect();
        // What is new goes through Customs now: what clears keeps its place in the code with the
        // stamp, and only what stays held or is refused goes back to the agent.
        let mut looked = Vec::new();
        for (f, s) in new {
            let status = match s {
                // Looked at once: a held item is not reviewed again every turn.
                crate::model::CustomsStatus::Held if !crate::customs::get(&self.ctx(), &f.kind, &f.name).await?.is_some_and(|m| m.reviewed_at.is_some()) => match crate::customs_review::settle(self, &f.kind, &f.name).await {
                    Ok(Some(item)) => item.status,
                    Ok(None) => s,
                    Err(e) => {
                        tracing::warn!(kind = %f.kind, name = %f.name, "customs review failed: {e:#}");
                        s
                    }
                },
                s => s,
            };
            looked.push((f, status));
        }
        if !looked.is_empty() {
            let found: Vec<CustomsFound> = looked
                .iter()
                .map(|(f, s)| CustomsFound { kind: f.kind.clone(), name: f.name.clone(), path: f.path.clone(), status: json!(s).as_str().unwrap_or_default().to_owned(), why: None })
                .collect();
            let list: String = looked
                .iter()
                // Only a refusal goes back to the agent: it must come out. A held one waits at
                // Customs and shows as Hub's own row; it never wakes the agent.
                .filter(|(_, s)| *s == crate::model::CustomsStatus::Refused)
                .map(|(f, _)| format!("- {} {} in {} (refused)\n", f.kind, f.name, f.path))
                .collect();
            let tell = !list.is_empty() && !sent;
            self.emit_typed(thread_id, &CustomsData { seq: Some(end.seq), declared: None, found, sent: Some(tell || list.is_empty()) }).await?;
            // One message from Hub a turn: after a leak, the agent hears about these next time.
            if tell {
                let message = format!("{STOPPED}\n\n{list}\n{STOPPED_TAIL}");
                self.send_from(thread_id, message, Vec::new(), None, Some(("customs", json!(end.seq))), None).await?;
                sent = true;
            }
        }
        Ok(sent)
    }

    /// Judges whether the turn showed its code working. Logs a `proof` event and returns what to
    /// ask the agent when it did not.
    async fn proof_check(
        self: &Arc<Self>,
        events: &[Event],
        asked: &Event,
        checkpoint: &CheckpointData,
        end: &Event,
        files: &[crate::checkpoint::FileDiff],
    ) -> Result<Option<String>> {
        let thread_id = end.thread_id.as_str();
        let turn: Vec<Event> = events.iter().filter(|e| e.seq > asked.seq && e.seq < end.seq).cloned().collect();
        let mut found = crate::proof::evidence(&turn);
        // A run can exit cleanly and still print failures, as through `| tail`. A decision
        // model reads the end of the output when it holds a word that may mean it.
        let mut hidden = false;
        if let Some(run) = found.iter_mut().rev().find(|e| e.kind == "tests")
            && run.ok && self.hides_failure(thread_id, &run.command, &run.output).await {
                run.ok = false;
                hidden = true;
            }
        let verdict = crate::proof::verdict(&found);
        let changed = files.iter().filter(|f| crate::proof::needs_proof(&f.path)).count();
        let mut data = ProofData {
            reply: Some(end.seq),
            files: Some(Some(changed as i64)),
            kind: Some(verdict.map(|v| v.kind.to_owned())),
            command: Some(verdict.map(|v| v.command.clone())),
            hidden: Some(hidden),
            ..Default::default()
        };
        let state = match verdict {
            Some(v) if v.ok => "shown",
            Some(_) => "failed",
            None => "missing",
        };
        if let Some(reply) = asked.mark("proof").as_i64() {
            // The answer to Hub's request: whatever it showed is the verdict on that reply.
            let earlier = events.iter().rev().filter_map(Event::data_as::<ProofData>).find(|p| p.reply == Some(reply));
            data.reply = Some(reply);
            // The files of the turn that was asked: `null` when that event did not say.
            data.files = Some(earlier.map_or(Some(changed as i64), |p| p.files.flatten()));
            data.state = state.to_owned();
            self.emit_typed(thread_id, &data).await?;
        } else if changed > 0 {
            let skip = if state == "missing" { self.proof_not_needed(events, asked, checkpoint, files).await } else { None };
            if let Some(why) = skip {
                // Nothing new to show working: the turn took code out, put earlier work back,
                // or the person said not to test. Noted, never asked.
                data.state = "skipped".to_owned();
                data.why = Some(why.to_owned());
                self.emit_typed(thread_id, &data).await?;
            } else {
                let asking = state != "shown";
                data.state = if asking { "asked" } else { state }.to_owned();
                data.why = Some(state.to_owned());
                self.emit_typed(thread_id, &data).await?;
                if asking {
                    let cwd = self.store.thread(thread_id).await?.context("no such thread")?.cwd;
                    return Ok(Some(match verdict {
                        Some(v) if hidden => PROOF_HIDDEN.replace("{command}", &v.command),
                        Some(v) => PROOF_FAILED.replace("{command}", &v.command),
                        None => crate::proof::ask_missing(&files.iter().map(|f| f.path.as_str()).filter(|p| crate::proof::needs_proof(p)).collect::<Vec<_>>(), Path::new(&checkpoint.root), Path::new(&cwd)),
                    }));
                }
            }
        }
        Ok(None)
    }

    /// Docs in step with code: after a turn that changed code some doc section covers (`covers:`),
    /// whether the agent changed, opened or asked `docs_for` about that section. Logs a `docs`
    /// event, and returns what to ask the agent when it did none of those. A turn answering Hub
    /// settles the earlier request instead of asking again; a revert of earlier work asks nothing.
    async fn docs_check(
        &self,
        events: &[Event],
        asked: &Event,
        checkpoint: &CheckpointData,
        end: &Event,
        files: &[crate::checkpoint::FileDiff],
    ) -> Result<Option<String>> {
        let thread_id = end.thread_id.as_str();
        let turn: Vec<Event> = events.iter().filter(|e| e.seq > asked.seq && e.seq < end.seq).cloned().collect();
        let looked = crate::covers::looked(&turn);
        let changed: Vec<String> = files.iter().filter(|f| f.lines > 0).map(|f| f.path.clone()).collect();
        let root = PathBuf::from(written(&checkpoint.root).context("bad checkpoint")?);

        // The answer to Hub's request: did it look this time?
        let from_hub = asked.mark("docs").as_i64().or(asked.mark("proof").as_i64());
        if let Some(reply) = from_hub {
            let open = events.iter().rev().filter_map(Event::data_as::<DocsData>).find(|d| d.reply == Some(reply));
            if let Some(open) = open.filter(|d| d.state == "asked") {
                let sections = open.sections;
                let in_mind = |s: &DocsSection| crate::covers::kept_in_mind(&s.doc, &s.files, &changed, &looked);
                let updated = sections.iter().any(|s| changed.contains(&s.doc));
                let state = if updated { "updated" } else if sections.iter().all(in_mind) { "checked" } else { "ignored" };
                self.emit_typed(thread_id, &DocsData::new(reply, state, sections)).await?;
            }
            return Ok(None);
        }

        let code: Vec<String> = files.iter().filter(|f| f.lines > 0 && crate::proof::needs_proof(&f.path)).map(|f| f.path.clone()).collect();
        if code.is_empty() {
            return Ok(None);
        }
        let (at, code2, changed2, looked2) = (root.clone(), code.clone(), changed.clone(), looked.clone());
        let touched = tokio::task::spawn_blocking(move || crate::covers::touched(&at, &code2, &changed2, &looked2)).await?;
        if touched.is_empty() {
            return Ok(None);
        }
        let section = |t: &crate::covers::Touched| {
            let c = &t.covering;
            DocsSection { doc: c.rel.clone(), heading: Some(c.heading.as_ref().map(|h| h.text.clone())), first: c.first as u64, last: c.last as u64, files: t.files.clone() }
        };
        let all: Vec<DocsSection> = touched.iter().map(|(t, _)| section(t)).collect();
        let unseen: Vec<&crate::covers::Touched> = touched.iter().filter(|(_, seen)| !seen).map(|(t, _)| t).collect();
        if unseen.is_empty() {
            let updated = touched.iter().any(|(t, _)| changed.contains(&t.covering.rel));
            self.emit_typed(thread_id, &DocsData::new(end.seq, if updated { "updated" } else { "checked" }, all)).await?;
            return Ok(None);
        }
        // Putting earlier work back leaves the docs as they were for it.
        let earlier: Vec<String> = events
            .iter()
            .filter_map(Event::data_as::<CheckpointData>)
            .filter(|c| c.seq.is_some_and(|s| s < asked.seq) && c.root == checkpoint.root)
            .filter_map(|c| written(&c.commit).map(str::to_owned))
            .collect();
        let paths: Vec<String> = unseen.iter().flat_map(|t| t.files.clone()).collect();
        let reverted = !earlier.is_empty()
            && tokio::task::spawn_blocking(move || crate::checkpoint::put_back(&root, &earlier, &paths)).await?.ok().flatten().is_some();
        if reverted {
            self.emit_typed(thread_id, &DocsData::new(end.seq, "skipped", all).why("reverted")).await?;
            return Ok(None);
        }
        // A section the agent was asked about before for the same files is not asked again.
        let told: std::collections::HashSet<(String, String)> = events
            .iter()
            .filter_map(Event::data_as::<DocsData>)
            .filter(|d| d.state == "asked")
            .flat_map(|d| d.sections)
            .flat_map(|s| s.files.into_iter().map(move |f| (s.doc.clone(), f)))
            .collect();
        let missed: Vec<&crate::covers::Touched> =
            unseen.into_iter().filter(|t| t.files.iter().any(|f| !told.contains(&(t.covering.rel.clone(), f.clone())))).collect();
        if missed.is_empty() {
            self.emit_typed(thread_id, &DocsData::new(end.seq, "ignored", all).why("asked_before")).await?;
            return Ok(None);
        }
        let sections: Vec<DocsSection> = missed.iter().map(|t| section(t)).collect();
        let list: String = missed
            .iter()
            .map(|t| format!("- {} (lines {}-{}), for {}\n", crate::covers::place(&t.covering), t.covering.first, t.covering.last, t.files.join(", ")))
            .collect();
        self.emit_typed(thread_id, &DocsData::new(end.seq, "asked", sections)).await?;
        Ok(Some(format!("{DOCS_UNCHECKED}\n\n{list}\n{DOCS_UNCHECKED_TAIL}")))
    }

    /// Why a turn that changed code and ran nothing needs no proof, if it does not: the person
    /// said not to test, it only took lines out, it worked in a scratch folder, or it put the files back as an earlier turn found
    /// them (a revert of the agent's own work).
    async fn proof_not_needed(
        &self,
        events: &[Event],
        asked: &Event,
        checkpoint: &CheckpointData,
        files: &[crate::checkpoint::FileDiff],
    ) -> Option<&'static str> {
        if crate::proof::waived(asked.said().unwrap_or_default()) {
            return Some("waived");
        }
        let code: Vec<&crate::checkpoint::FileDiff> = files.iter().filter(|f| crate::proof::needs_proof(&f.path)).collect();
        if crate::proof::only_removes(&code) {
            return Some("removed");
        }
        let root = PathBuf::from(written(&checkpoint.root)?);
        // A scratch thread makes things for the person, not code that ships.
        if let Ok(projects) = self.projects_root().await
            && crate::projects_root::is_made_for_the_person(&projects, &root.to_string_lossy())
        {
            return Some("scratch");
        }
        let earlier: Vec<String> = events
            .iter()
            .filter_map(Event::data_as::<CheckpointData>)
            .filter(|c| c.seq.is_some_and(|s| s < asked.seq) && c.root == checkpoint.root)
            .filter_map(|c| written(&c.commit).map(str::to_owned))
            .collect();
        let paths: Vec<String> = code.iter().map(|f| f.path.clone()).collect();
        match tokio::task::spawn_blocking(move || crate::checkpoint::put_back(&root, &earlier, &paths)).await {
            Ok(Ok(Some(_))) => Some("reverted"),
            Ok(Ok(None)) => None,
            Ok(Err(e)) => {
                tracing::warn!("proof revert check failed: {e:#}");
                None
            }
            Err(e) => {
                tracing::warn!("proof revert check failed: {e}");
                None
            }
        }
    }
}
