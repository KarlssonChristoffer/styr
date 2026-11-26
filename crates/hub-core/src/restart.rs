//! Turns that a restart of the core cut off carry on in the next core.
//!
//! An engine is a child of the core, so an update, new settings or a crash ends whatever turn it
//! was in. Each turn is logged as a `turn_start` before the prompt goes out and a `turn_end` (or
//! `error`) after, so the next core can tell from the log alone which turn was cut off and which
//! messages were sent meanwhile without reaching the engine. It resumes the engine's session and
//! asks the agent to carry on, then sends the waiting messages in order.
//!
//! Nothing is sent twice: the message asking to carry on is marked `restart: <prompt>` and is
//! itself the receipt, so a core that dies again before the engine got it does not ask again but
//! sends the one already logged. Messages from the app carry a client id whose receipt is logged
//! with the message (`Store::append_event_once`). A turn that keeps getting cut off is let go
//! after a few tries. The idea and the shape follow T3 Code's `RestartContinuation` and command
//! receipts (MIT).

use crate::locked::send_or_log;
use std::sync::Arc;

use anyhow::Result;
use serde_json::json;

use crate::engine::{Command, Prompt};
use crate::hub::Hub;
use crate::model::{Event, EventKind, PermissionResolvedData, RestartData, ThreadStatus};

/// Carrying on is given up after this many restarts in a row cut the same work off before a
/// turn could finish.
const MAX_IN_A_ROW: usize = 3;

/// What the agent is told; the user sees it folded into the restart line.
pub const CARRY_ON: &str = "Hub restarted while you were working on this, so your last turn was cut off. Carry on where you left off. Commands you started may or may not have finished: check what is already done before you run anything again that must not run twice (pushes, migrations, messages sent).";

/// The same after the panic button (`crate::halt`), which also cuts turns off.
pub const CARRY_ON_PAUSED: &str = "Hub paused every agent while you were working on this, so your last turn was cut off. You are resumed now: carry on where you left off. Commands you started may or may not have finished: check what is already done before you run anything again that must not run twice (pushes, migrations, messages sent).";

/// Finds every thread whose turn a restart cut off, or whose messages never reached the engine,
/// and carries it on. Run once when the core starts, after the previous core is gone. While the
/// panic button holds the agents, nothing is carried on until they are resumed.
pub async fn resume(hub: &Arc<Hub>) -> Result<usize> {
    let by = hub.store.take_value(crate::hub::RESTART_PLANNED).await?.unwrap_or_else(|| "crash".into());
    if hub.halted() {
        return Ok(0);
    }
    // Questions the last core asked can no longer be answered here: its answers went to a
    // connection that is gone. Closed first, so the ones an engine asks again are the only open ones.
    if let Err(err) = close_orphaned_questions(hub).await {
        tracing::warn!("could not close the questions the last core left open: {err:#}");
    }
    // Engines a keeper held through the restart go on where they are; nothing was cut off there.
    let taken = take_over(hub).await;
    Ok(taken + carry_on(hub, &by).await?)
}

/// Logs an answer-less end for every permission question the previous core left open. Without it
/// the cards stay in the thread, and answering one says "no open permission request": the pending
/// question lived in that core's memory. Only threads whose latest question came after their latest
/// turn end are looked at, since a turn that ends closes its questions.
async fn close_orphaned_questions(hub: &Arc<Hub>) -> Result<usize> {
    let mut ended = std::collections::HashMap::new();
    for kind in [EventKind::TurnEnd, EventKind::Error] {
        for e in hub.store.latest_of_kind(kind).await? {
            let seq = ended.entry(e.thread_id).or_insert(0);
            *seq = (*seq).max(e.seq);
        }
    }
    let mut closed = 0;
    for asked in hub.store.latest_of_kind(EventKind::PermissionRequest).await? {
        if ended.get(&asked.thread_id).is_some_and(|end| *end > asked.seq) {
            continue;
        }
        let kinds = [EventKind::PermissionRequest, EventKind::PermissionResolved];
        let events = hub.store.events_of(&asked.thread_id, 0, &kinds).await?;
        for id in crate::model::open_questions(&events) {
            let by_restart = PermissionResolvedData { restart: Some(true), ..PermissionResolvedData::answer(id, None) };
            hub.emit_typed(&asked.thread_id, &by_restart).await?;
            closed += 1;
        }
    }
    if closed > 0 {
        tracing::info!("closed {closed} permission questions the core before left open");
    }
    Ok(closed)
}

/// Takes over every engine a keeper held for this core (`crate::keeper`), and sends each the
/// messages that were written while no core had it. Returns how many.
async fn take_over(hub: &Arc<Hub>) -> usize {
    let mut taken = 0;
    for (id, link) in crate::keeper::take_over(&hub.config.data_dir).await {
        let thread = match hub.store.thread(&id).await {
            Ok(Some(t)) if t.status != ThreadStatus::InTerminal && link.attached.init.is_some() && link.attached.session.is_some() => t,
            // Nothing to carry on with: let the engine go (dropping the link ends it).
            _ => continue,
        };
        let queued = match hub.store.last_turn(&id).await {
            Ok(turn) => turn.queued,
            Err(_) => Vec::new(),
        };
        match hub.adopt_engine(thread, link) {
            Ok(commands) => {
                for e in queued.iter().filter(|e| e.mark("restart").is_null()) {
                    send_or_log(&commands, Command::Prompt(Prompt::resend(e)), "restart prompt");
                }
                taken += 1;
            }
            Err(err) => tracing::warn!(thread = %id, "could not take the engine over: {err:#}"),
        }
    }
    if taken > 0 {
        tracing::info!("took over {taken} engines from the core before");
    }
    taken
}

/// Carries on every cut-off turn and sends the messages that waited. `by` says what cut them
/// off: "update", "restart", "stop", "crash", or "pause" for the panic button.
pub(crate) async fn carry_on(hub: &Arc<Hub>, by: &str) -> Result<usize> {
    let paused = by == "pause";
    let running = hub.engines();
    let mut carried = 0;
    for thread in hub.store.threads().await? {
        if thread.status == ThreadStatus::InTerminal || running.contains(&thread.id) {
            continue;
        }
        // A core whose handover failed parks threads with their status as it was.
        if matches!(thread.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou) {
            hub.set_status_because(&thread.id, ThreadStatus::Idle, "restart").await?;
        }
        let turn = hub.store.last_turn(&thread.id).await?;
        let mut queued = turn.queued;
        if let Some(cut_off) = turn.cut_off
            && !queued.iter().any(|e| e.mark("restart").as_i64() == Some(cut_off))
        {
            let events = hub.store.events_after_last(&thread.id, EventKind::TurnEnd, &[EventKind::Update]).await?;
            // The user paused it on purpose: always carried on, and never counted as a loop.
            let why = if paused {
                None
            } else if !hub.config.resume_turns {
                Some("off")
            } else if in_a_row(&events) >= MAX_IN_A_ROW {
                Some("loop")
            } else {
                None
            };
            let seqs: Vec<i64> = queued.iter().map(|e| e.seq).collect();
            let state = if why.is_some() { "stopped" } else { "continued" };
            let data = RestartData { by: by.to_owned(), prompt: cut_off, queued: seqs, state: state.to_owned(), why: why.map(str::to_owned) };
            hub.emit_typed(&thread.id, &data).await?;
            if why.is_none() {
                let text = if paused { CARRY_ON_PAUSED } else { CARRY_ON };
                let ask = json!({ "text": text, "attachments": [], "restart": cut_off });
                queued.push(hub.emit(&thread.id, EventKind::UserMessage, ask).await?);
            }
        }
        if queued.is_empty() {
            continue;
        }
        // The request to carry on goes first: the messages after it were sent while it worked.
        queued.sort_by_key(|e| (e.mark("restart").is_null(), e.seq));
        let commands = match hub.runtime(thread.clone()) {
            Ok(commands) => commands,
            Err(err) => {
                tracing::warn!(thread = %thread.id, "could not carry on after a restart: {err:#}");
                continue;
            }
        };
        for e in queued {
            send_or_log(&commands, Command::Prompt(Prompt::resend(&e)), "restart prompt");
        }
        carried += 1;
    }
    if carried > 0 {
        tracing::info!("carried on {carried} threads ({by})");
    }
    Ok(carried)
}

/// How many times Hub asked to carry on since the last turn that finished.
fn in_a_row(events: &[Event]) -> usize {
    events
        .iter()
        .rev()
        .take_while(|e| e.kind != EventKind::TurnEnd)
        .filter_map(Event::data_as::<RestartData>)
        .filter(|r| r.state == "continued" && r.by != "pause")
        .count()
}
