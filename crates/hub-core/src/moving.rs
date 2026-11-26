//! Moving a thread to another machine: the same thread carries on over there, with its code.
//!
//! The machine the app talks to orchestrates (it need not be either end). It asks the machine the
//! thread runs on to **export** it: stop a running turn if the person said so, pack the project
//! with its uncommitted changes (`carry::pack_for`) and hand over the whole event log. The other
//! machine **imports** it: finds the project's folder there (or clones it), puts the commit,
//! branch and uncommitted changes in place, adds the thread under the same id and logs a `moved`
//! event. The first machine then **steps aside**: a `moved_away` event, the thread archived and
//! no longer listed (`Store::threads`), so lists show it once, where it is.
//!
//! The engine's own session cannot travel (the core stays engine-agnostic), so the next message
//! to the moved thread carries the conversation so far to a fresh session, the way switching
//! engine does. A turn that was running is carried on over there like one a restart cut off.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::engine::{Command, Prompt};
use crate::hub::Hub;
use crate::model::{
    Attachment, Event, EventKind, Left, LeftBehind, MoveThread, MovedAwayData, MovedData, MovedThread, PermissionResolvedData, RestartData, StreamMsg, Thread,
    ThreadExport, ThreadStatus,
    UnpackProject, open_questions,
};

/// What the agent is told when the turn it was in the middle of was cut off by the move.
const CARRY_ON: &str = "This thread was moved to another machine while you were working on it, so your last turn was cut off. Carry on where you left off. The folder here has the same commit, branch and uncommitted changes. Commands you started may or may not have finished: check what is already done before you run anything again that must not run twice (pushes, migrations, messages sent).";

/// How long a pack or an import may take; a big history over a slow link is slow.
const WAIT: Duration = Duration::from_secs(600);

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportAsk {
    pub thread: String,
    #[serde(default)]
    pub stop: bool,
}

/// What the machine that took the thread did with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Imported {
    pub folder: String,
    pub via: String,
    pub dirty: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SteppedAside {
    pub thread: String,
    pub to: String,
    pub folder: String,
}

fn is_here(hub: &Hub, name: &str) -> bool {
    name.eq_ignore_ascii_case(hub.remotes.local_name())
}

/// Moves the thread from the machine it runs on to `req.to`. Either may be this one.
pub async fn move_thread(hub: &Arc<Hub>, req: MoveThread) -> Result<MovedThread> {
    let from = req.from.clone().filter(|f| !is_here(hub, f));
    let to_here = is_here(hub, &req.to);
    if from.is_none() && to_here {
        bail!("the thread is already on this machine");
    }
    if from.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(&req.to)) {
        bail!("the thread is already on {}", req.to);
    }
    let from_name = from.clone().unwrap_or_else(|| hub.remotes.local_name().to_owned());

    let ask = ExportAsk { thread: req.thread.clone(), stop: req.stop };
    let mut export: ThreadExport = match &from {
        None => export(hub, ask).await?,
        Some(f) => hub.remotes.post_to(f, "/api/threads/export", &ask, WAIT).await?,
    };
    export.from = from_name;
    let stopped = export.stopped;

    let imported: Imported = if to_here {
        import(hub, export).await?
    } else {
        hub.remotes
            .post_to(&req.to, "/api/threads/import", &export, WAIT)
            .await
            .with_context(|| if stopped { "the thread was stopped but did not move; write to it to carry on".to_owned() } else { "it did not move".to_owned() })?
    };

    let aside = SteppedAside { thread: req.thread.clone(), to: req.to.clone(), folder: imported.folder.clone() };
    match &from {
        None => step_aside(hub, aside).await?,
        Some(f) => {
            let _: serde_json::Value = hub.remotes.post_to(f, "/api/threads/stepped-aside", &aside, Duration::from_secs(60)).await?;
        }
    }
    // Surfaces learn the new place from the machines' own lists.
    if let Some(f) = &from {
        hub.remotes.refresh_threads(f).await;
    }
    if !to_here {
        hub.remotes.refresh_threads(&req.to).await;
    }
    hub.publish(StreamMsg::Resync);
    Ok(MovedThread { thread: req.thread, to: req.to, folder: imported.folder, via: imported.via, dirty: imported.dirty, stopped })
}

/// Moves every thread here that is working or waiting on you to `to`, stopping their turns.
pub async fn leaving(hub: &Arc<Hub>, to: &str) -> Result<Left> {
    if is_here(hub, to) {
        bail!("pick another machine than this one");
    }
    let mut left = Left { moved: Vec::new(), failed: Vec::new() };
    let busy: Vec<Thread> = hub
        .store
        .threads()
        .await?
        .into_iter()
        .filter(|t| !t.archived && t.parent.is_none() && matches!(t.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou))
        .collect();
    for t in busy {
        let req = MoveThread { thread: t.id.clone(), from: None, to: to.to_owned(), stop: true };
        match move_thread(hub, req).await {
            Ok(moved) => left.moved.push(moved),
            Err(e) => left.failed.push(LeftBehind { thread: t.id, title: t.title, why: format!("{e:#}") }),
        }
    }
    Ok(left)
}

/// Packs a thread of this machine for another, stopping its turn when `ask.stop`.
pub async fn export(hub: &Arc<Hub>, ask: ExportAsk) -> Result<ThreadExport> {
    let thread = hub.store.thread(&ask.thread).await?.context("no such thread")?;
    if thread.host.is_some() {
        bail!("this thread runs on another machine");
    }
    if hub.store.moved_to(&thread.id).await?.is_some() {
        bail!("this thread was moved away already");
    }
    if thread.status == ThreadStatus::InTerminal {
        bail!("this thread is open in a terminal; take it back first");
    }
    let cwd = thread.cwd.clone();
    let busy = matches!(thread.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou);
    if busy && !ask.stop {
        bail!("the thread is working; wait for it to finish, or stop it to move");
    }
    // Stopped first, so the files do not change while they are packed. The turn's `turn_start`
    // has no end in the log, which is how the other machine knows to carry it on.
    if busy {
        hub.shutdown(&thread.id, false).await?;
        hub.set_status_because(&thread.id, ThreadStatus::Idle, "move_stopped").await?;
    }
    let pack = tokio::task::spawn_blocking(move || crate::carry::pack_folder(&cwd)).await??;
    let events: Vec<Event> = hub.store.events(&thread.id, 0, 10_000_000).await?.into_iter().filter(|e| e.kind != EventKind::Unknown).collect();
    let mut thread = hub.store.thread(&thread.id).await?.context("no such thread")?;
    thread.status = ThreadStatus::Idle;
    Ok(ThreadExport { thread, events, from: hub.remotes.local_name().to_owned(), pack, stopped: busy })
}

/// Takes a thread from another machine: its project, its log, and the turn it was in.
pub async fn import(hub: &Arc<Hub>, export: ThreadExport) -> Result<Imported> {
    let ThreadExport { mut thread, events, from, pack, stopped } = export;
    if !hub.config.engines.contains_key(&thread.engine) {
        bail!("this machine has no engine called {:?}; add it under Settings > Engines first", thread.engine);
    }
    let came_back = hub.store.moved_to(&thread.id).await?.is_some();
    if hub.store.thread(&thread.id).await?.is_some() && !came_back {
        bail!("this thread is here already");
    }

    // The project: this machine's clone of the same repository, or a new one under the projects
    // folder.
    let root = hub.projects_root().await?;
    let mut known: Vec<String> = hub.store.threads().await?.into_iter().map(|t| t.cwd).collect();
    known.extend(hub.store.projects().await?.into_iter().map(|p| p.path));
    let name = thread.cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("project").to_owned();
    let url = pack.url.clone();
    let plain = pack.plain;
    // Back on a machine it left: the folder it had here, as long as it is still there.
    let before = match hub.store.thread(&thread.id).await? {
        Some(old) if came_back && crate::folders::expand(&old.cwd).is_dir() => Some(old.cwd),
        _ => None,
    };
    let resume = before.is_some();
    let target = {
        let root = root.clone();
        let cwd = thread.cwd.clone();
        tokio::task::spawn_blocking(move || {
            if let Some(old) = before {
                return old;
            }
            if plain {
                return plain_target(&root, &cwd);
            }
            let key = url.as_deref().and_then(crate::folders::key_of);
            let found = key.map(|k| crate::folders::find_repo(&k, &known, std::slice::from_ref(&root))).unwrap_or_default();
            found.into_iter().next().unwrap_or_else(|| std::path::Path::new(&root).join(&name).display().to_string())
        })
        .await?
    };
    let brought = tokio::task::spawn_blocking(move || crate::carry::unpack(UnpackProject { path: target, pack, sync: true, resume })).await??;

    if came_back {
        hub.store.delete_thread(&thread.id).await?;
        hub.store.clear_moved(&thread.id).await?;
    }
    thread.cwd = brought.folder.clone();
    thread.host = None;
    thread.engine_session_id = None;
    thread.status = ThreadStatus::Idle;
    hub.store.import_thread(thread.clone(), events.clone()).await?;

    // Questions that were open when it left cannot be answered any more.
    for open in open_questions(&events) {
        let closed = PermissionResolvedData { moved: Some(true), ..PermissionResolvedData::answer(open, None) };
        hub.emit_typed(&thread.id, &closed).await?;
    }
    let data = MovedData { from: from.clone(), folder: brought.folder.clone(), via: brought.via.clone(), files: brought.files.clone(), dirty: brought.dirty, stopped };
    let moved = hub.emit_typed(&thread.id, &data).await?;
    hub.publish(StreamMsg::Thread { thread: hub.store.thread(&thread.id).await?.context("no such thread")? });

    if stopped {
        carry_on(hub, &thread.id, &from, &events, moved.seq).await?;
    } else {
        hub.warm(&thread.id).await?;
    }
    Ok(Imported { folder: brought.folder, via: brought.via, dirty: brought.dirty })
}

/// Where a folder that is not a repository goes on this machine: a Scratch thread's folder stays
/// in `scratch` (under the projects folder), the rest keep their name. A folder already there
/// holds something else (or the thread's own earlier copy), so it gets the next free name.
fn plain_target(root: &str, cwd: &str) -> String {
    let mut parts = cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']);
    let name = parts.next().filter(|n| !n.is_empty()).unwrap_or("project");
    let scratch = parts.next().is_some_and(|p| p.eq_ignore_ascii_case("scratch"));
    let base = if scratch { std::path::Path::new(root).join("scratch") } else { std::path::Path::new(root).to_path_buf() };
    let empty = |p: &std::path::Path| p.read_dir().map(|mut d| d.next().is_none()).unwrap_or(true);
    let first = base.join(name);
    if empty(&first) {
        return first.display().to_string();
    }
    (2..)
        .map(|n| base.join(format!("{name}-{n}")))
        .find(|p| empty(p))
        .unwrap_or(first)
        .display()
        .to_string()
}

/// Sends the turn the move cut off, and the messages written meanwhile, to the engine here. The
/// first carries the conversation so far, since the session did not come along.
async fn carry_on(hub: &Arc<Hub>, id: &str, from: &str, events: &[Event], moved: i64) -> Result<()> {
    // Where the turns stood: the last turn started and not ended is cut off; user messages after
    // it (or after the last turn that ended, when none started) never reached the engine.
    let start = events.iter().rev().find(|e| e.kind == EventKind::TurnStart);
    let (cut_off, after) = match start {
        Some(start) => {
            let prompt = start.prompt().unwrap_or(0);
            let ended = events.iter().any(|e| e.seq > start.seq && matches!(e.kind, EventKind::TurnEnd | EventKind::Error));
            ((!ended && prompt > 0).then_some(prompt), prompt)
        }
        None => (None, events.iter().rev().find(|e| matches!(e.kind, EventKind::TurnEnd | EventKind::Error)).map(|e| e.seq).unwrap_or(0)),
    };
    let mut queued: Vec<Event> = events.iter().filter(|e| e.kind == EventKind::UserMessage && e.seq > after && e.mark("restart").is_null()).cloned().collect();
    if cut_off.is_none() && queued.is_empty() {
        return hub.warm(id).await;
    }
    let mut prompts: Vec<Event> = Vec::new();
    if let Some(prompt) = cut_off {
        let seqs: Vec<i64> = queued.iter().map(|e| e.seq).collect();
        let carried = RestartData { by: "move".into(), prompt, queued: seqs, state: "continued".into(), why: None };
        hub.emit_typed(id, &carried).await?;
        let ask = json!({ "text": CARRY_ON, "attachments": [], "restart": prompt, "moved": moved });
        prompts.push(hub.emit(id, EventKind::UserMessage, ask).await?);
    }
    prompts.append(&mut queued);
    if hub.halted() {
        return Ok(());
    }
    let thread = hub.store.thread(id).await?.context("no such thread")?;
    let commands = hub.runtime(thread)?;
    let (log, recipe) = crate::hub::handover_log(id, events);
    let context = format!("{}\n\n<conversation>{}\n</conversation>", crate::hub::moved_note(from), log);
    for (n, e) in prompts.iter().enumerate() {
        let attachments: Vec<Attachment> = serde_json::from_value(e.mark("attachments").clone()).unwrap_or_default();
        let said = e.said().unwrap_or_default().to_owned();
        let text = if n == 0 { format!("{context}\n\n{said}") } else { said.clone() };
        let parts = if n == 0 { vec![("handover".to_owned(), context.chars().count())] } else { Vec::new() };
        commands.send(Command::Prompt(Prompt { text, attachments, seq: e.seq, said, purpose: "moved".into(), parts, recipe })).context("engine stopped")?;
    }
    hub.set_status_because(id, ThreadStatus::Running, "move_carried_on").await?;
    Ok(())
}

/// The machine the thread left steps aside: it is archived, noted as moved and no longer listed.
pub async fn step_aside(hub: &Arc<Hub>, aside: SteppedAside) -> Result<()> {
    let id = aside.thread;
    hub.shutdown(&id, false).await?;
    hub.emit_typed(&id, &MovedAwayData { to: aside.to.clone(), folder: aside.folder }).await?;
    hub.store.set_moved_to(&id, &aside.to).await?;
    hub.unwatch_pr(&id).await.ok();
    hub.update_thread(&id, |t| t.archived = true).await?;
    hub.publish(StreamMsg::ThreadDeleted { id });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scratch_folder_stays_in_scratch_and_never_lands_on_another() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().display().to_string();
        let made = |p: String| std::path::PathBuf::from(p);
        let first = made(plain_target(&root, "/home/me/dev/scratch/2026-11-01-130000"));
        assert_eq!(first, tmp.path().join("scratch").join("2026-11-01-130000"));
        std::fs::create_dir_all(&first).unwrap();
        assert_eq!(made(plain_target(&root, "/home/me/dev/scratch/2026-11-01-130000")), first, "an empty folder is free");
        std::fs::write(first.join("a.txt"), "x").unwrap();
        let second = made(plain_target(&root, r"C:\Dev\scratch\2026-11-01-130000"));
        assert_eq!(second, tmp.path().join("scratch").join("2026-11-01-130000-2"));
        assert_eq!(made(plain_target(&root, "/srv/notes")), tmp.path().join("notes"));
    }

    fn ev(seq: i64, kind: EventKind, data: serde_json::Value) -> Event {
        Event { thread_id: "t".into(), seq, ts: seq, kind, data }
    }

    #[test]
    fn questions_open_when_it_left() {
        let events = vec![
            ev(1, EventKind::PermissionRequest, json!({ "id": "a" })),
            ev(2, EventKind::PermissionRequest, json!({ "id": "b" })),
            ev(3, EventKind::PermissionResolved, json!({ "id": "a" })),
        ];
        assert_eq!(open_questions(&events), ["b"]);
    }
}
