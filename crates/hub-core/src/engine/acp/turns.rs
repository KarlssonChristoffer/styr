//! The turn loop of a running ACP session: one prompt at a time with Hub's context around it,
//! commands that arrive mid-turn (stop, steer, settings), a stop the engine ignores ended from
//! Hub's side, how each turn ended, and letting an idle engine go. Also the check that keeps an
//! engine from naming a thread after Hub's own rules (`names_hubs_rules`).

use crate::locked::ResultExt;
use crate::locked::Ignore;
use super::*;
use crate::model::{ErrorData, TurnEndData, TurnStartData, TurnUsage};

/// Takes prompts one at a time until the commands close or the engine is let go. `resumed` is
/// the prompt of a turn the core before this one started, still going in a taken-over engine.
#[allow(clippy::too_many_arguments)]
pub(super) async fn turns(
    hub: &Arc<Hub>,
    thread: &Thread,
    session_id: &SessionId,
    state: &mut SessionState,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    queue: &Parked,
    conn: &ConnectionTo<Agent>,
    unprompted: &Arc<crate::engine::unprompted::Unprompted>,
    mut lost: bool,
    mut resumed: Option<i64>,
) -> Result<()> {
    let mut open = true;
    // Whether this session has been given Hub's instructions in a message (see below).
    let mut told = false;
    loop {
        // A turn the core before this one started goes on: wait for its end first.
        let (asked, mut turn): (i64, TurnFuture) = if let Some(asked) = resumed.take() {
            unprompted.hand_over(hub, &thread.id).await;
            unprompted.prompting(true);
            hub.set_status(&thread.id, ThreadStatus::Running).await?;
            let ask = agent_client_protocol::UntypedMessage::new(crate::keeper::WAIT_TURN, json!({})).map_err(acp_err)?;
            let end = conn.send_request(ask).block_task();
            (asked, Box::pin(async move { end.await.and_then(|v| serde_json::from_value(v).map_err(|_| agent_client_protocol::Error::internal_error())) }))
        } else {
            // Popped before the match: a guard held over the wait below would pin the task to one
            // thread.
            let queued = queue.locked().pop_front();
            let next = match queued {
                Some(prompt) => prompt,
                None if !open => return Ok(()),
                // An idle engine is stopped to free memory; the next message resumes the session.
                None => match idle_wait(hub, thread, conn, session_id, state, commands, unprompted).await? {
                    Idle::Prompt(prompt) => prompt,
                    Idle::Again => continue,
                    Idle::End => return Ok(()),
                },
            };
            // Edited or removed while it waited behind the turn before.
            let Some(mut next) = hub.take_prompt(&thread.id, next).await? else {
                // Hub marks a thread working as a message comes in; one taken back leaves it idle.
                if queue.locked().is_empty() {
                    hub.set_status_because(&thread.id, ThreadStatus::Idle, "message_taken_back").await?;
                }
                continue;
            };
            add_context(hub, thread, &mut next, std::mem::take(&mut lost), &mut told).await?;
            unprompted.hand_over(hub, &thread.id).await;
            unprompted.prompting(true);
            hub.set_status(&thread.id, ThreadStatus::Running).await?;
            hub.checkpoint(thread, next.seq).await;
            guard(hub, &thread.id, &thread.engine, conn, session_id, state).await?;
            // A start without an end after it tells the next core this turn was cut off.
            let asked = next.seq;
            hub.emit_typed(&thread.id, &TurnStartData { prompt: Some(asked), ..Default::default() }).await?;
            // The slip goes before the package does, so it is there if the turn fails.
            crate::package::send(hub.slips.as_ref(), next.slip(&thread.id, &thread.engine, state.model_now().as_deref(), &session_id.0));
            let prompt = PromptRequest::new(session_id.clone(), content(next));
            (asked, Box::pin(conn.send_request(prompt).block_task()))
        };
        // Set by Stop or Steer: an engine that has not ended the turn by then is stopped by Hub.
        let mut stop_by: Option<tokio::time::Instant> = None;
        let outcome = loop {
            let grace = stop_by.unwrap_or_else(|| tokio::time::Instant::now() + std::time::Duration::from_secs(3600));
            tokio::select! {
                result = &mut turn => break Turn::Done(Box::new(result)),
                _ = tokio::time::sleep_until(grace), if stop_by.is_some() => break Turn::Forced,
                cmd = commands.recv(), if open => mid_turn(hub, thread, conn, session_id, state, queue, cmd, (&mut open, &mut stop_by)).await?,
            }
        };
        // An update after this starts a turn of the engine's own, logged after this one's end.
        let ending = unprompted.prompt_over().await;
        hub.after_turn_usage(&thread.engine).await;
        let result = match outcome {
            Turn::Done(result) => *result,
            Turn::Forced => {
                force_stop(hub, thread, asked).await?;
                return Ok(());
            }
        };
        if !end_turn(hub, thread, asked, result).await? {
            return Ok(());
        }
        drop(ending);
        // A question the agent left open belongs to the turn that just ended.
        hub.cancel_permissions(&thread.id).await;
        if state.apply_deferred(conn, session_id).await {
            state.publish(hub, &thread.id).await?;
        }
        // Not over a turn of the engine's own that began once this one's end was logged.
        unprompted.idle_unless_own_turn(hub, &thread.id).await?;
    }
}

/// What goes around a message: the conversation in front of it when the agent lost the session,
/// and Hub's instructions after the first message of a session.
async fn add_context(hub: &Arc<Hub>, thread: &Thread, next: &mut Prompt, lost: bool, told: &mut bool) -> Result<()> {
    if lost
        && !next.text.contains("<conversation>")
        && let Some((context, recipe)) = hub.lost_session_context(&thread.id, next.seq).await?
    {
        if recipe != "legacy" {
            next.recipe = recipe;
        }
        let before = next.text.chars().count();
        next.text = format!("{context}\n\n{}", next.text);
        next.parts.push(("lost_session".into(), next.text.chars().count() - before));
    }
    // An engine that does not put MCP instructions in front of the model gets them after its
    // first message of the session (the person's words first, so a title is about them): what the chat shows, how to show a diagram, what the
    // tools are for. The message in the log stays the user's own words.
    if !std::mem::replace(told, true)
        && hub.config.agent_tools
        && !hub.config.traits(&thread.engine).mcp_instructions.unwrap_or(false)
    {
        let rules = crate::tool_instructions::instructions(&hub.ctx(), Some(thread)).await;
        let note = format!("<hub_instructions>\n{rules}\n</hub_instructions>\n\n");
        next.parts.push(("hub_instructions".into(), note.chars().count()));
        // The person's words go first: an engine names the session after how its first message
        // begins, and with the rules in front every thread was named after Hub's own rules.
        next.text = format!("{}\n\n{}", next.text, note.trim_end());
    }
    Ok(())
}

/// Whether a title an engine suggests is about Hub's own rules and not about what the person
/// asked: it names Hub and its agents or threads, as the rules do, and nothing the person wrote
/// names Hub. An engine told the rules in its first message (`add_context`) can name the session
/// after them ("Hub coding agent workflow"). A title that only names Hub ("Fix Hub updater") is
/// taken: work on Hub itself is named so without the person writing "Hub".
pub(super) fn names_hubs_rules(title: &str, said: &[&str]) -> bool {
    let has = |text: &str, words: &[&str]| text.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|w| words.contains(&w));
    let hub = ["hub", "hubs", "hubben", "huben"];
    has(title, &hub) && has(title, &["agent", "agents", "thread", "threads"]) && !said.iter().any(|s| has(s, &hub))
}

/// The engine ignored a stop: ends the turn from Hub's side.
async fn force_stop(hub: &Arc<Hub>, thread: &Thread, asked: i64) -> Result<()> {
    // The engine ignored session/cancel. Ending here drops the connection, which
    // ends the engine and everything it started; the prompts it held go to a fresh
    // engine (`run`), the next message starts one.
    tracing::warn!(thread = %thread.id, "the engine did not stop within {} s of Stop; stopped it", CANCEL_GRACE.as_secs());
    // What it was running (a command that never ends) may sit in a process group of its
    // own, which the keeper's end of the engine does not reach.
    let (dir, id) = (hub.config.data_dir.clone(), thread.id.clone());
    tokio::task::spawn_blocking(move || {
        for pid in crate::workers::engine_tools(&id, crate::keeper::pid(&dir, &id)) {
            crate::workers::act(pid, crate::workers::Action::Stop).ignore("a tool may end on its own before it is stopped");
        }
    })
    .await.warn_on_err("stop the engine's tools");
    hub.emit_typed(&thread.id, &TurnEndData { stop_reason: "cancelled".into(), forced: Some(true), prompt: Some(asked), ..Default::default() }).await?;
    hub.set_status(&thread.id, ThreadStatus::Idle).await?;
    Ok(())
}

/// Logs how a turn ended. `false` when the engine is gone and the turns end with it.
async fn end_turn(hub: &Arc<Hub>, thread: &Thread, asked: i64, result: std::result::Result<agent_client_protocol::schema::v1::PromptResponse, agent_client_protocol::Error>) -> Result<bool> {
    match result {
        Ok(done) => {
            // The engine protocol's own word for it ("end_turn", "cancelled", ...), as it was logged before.
            let stop_reason = serde_json::to_value(done.stop_reason).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default();
            let usage = done.usage.as_ref().map(turn_usage);
            hub.emit_typed(&thread.id, &TurnEndData { stop_reason, prompt: Some(asked), usage, ..Default::default() }).await?;
        }
        // The engine is gone: its connection closed mid-turn. Nothing more goes over it, so
        // the engine ends here and the next message starts a fresh one on the same session;
        // carrying on with it would fail every later message at once.
        Err(e) if agent_client_protocol::is_incoming_transport_closed(&e) => {
            let data = ErrorData { message: ENGINE_GONE.into(), prompt: Some(asked), detail: Some(closed_detail(hub, &thread.id, &e)), ..Default::default() };
            hub.emit_typed(&thread.id, &data).await?;
            hub.cancel_permissions(&thread.id).await;
            // A turn cut off needs you, as a crash does; the caller leaves it so.
            hub.set_status(&thread.id, ThreadStatus::Error).await?;
            return Ok(false);
        }
        Err(e) => {
            let data = ErrorData { message: e.message.clone(), prompt: Some(asked), detail: error_detail(&e).map(plain_detail), ..Default::default() };
            hub.emit_typed(&thread.id, &data).await?;
        }
    }
    Ok(true)
}

/// The tokens of a turn in Hub's words, from the prompt response's `usage` (ACP's `inputTokens`,
/// `outputTokens`, `cachedReadTokens`, `cachedWriteTokens`; its totals are left out, they add up).
fn turn_usage(usage: &agent_client_protocol::schema::v1::Usage) -> TurnUsage {
    let count = |n: u64| i64::try_from(n).ok();
    TurnUsage {
        input: count(usage.input_tokens),
        output: count(usage.output_tokens),
        cache_read: usage.cached_read_tokens.and_then(count),
        cache_write: usage.cached_write_tokens.and_then(count),
    }
}

/// What an idle engine does next.
enum Idle {
    Prompt(Prompt),
    /// A command that is not a prompt was handled: look again.
    Again,
    /// The commands closed, or the engine sat idle: let it go.
    End,
}

/// Waits for the next command while no turn runs. An idle engine is stopped to free memory; the
/// next message resumes the session.
async fn idle_wait(
    hub: &Arc<Hub>,
    thread: &Thread,
    conn: &ConnectionTo<Agent>,
    session_id: &SessionId,
    state: &mut SessionState,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    unprompted: &Arc<crate::engine::unprompted::Unprompted>,
) -> Result<Idle> {
    Ok(match tokio::time::timeout(IDLE_TIMEOUT, commands.recv()).await {
        Ok(Some(Command::Prompt(prompt))) => Idle::Prompt(prompt),
        Ok(Some(Command::SetConfig { config_id, .. })) if config_id == crate::routes::ROUTE => Idle::Again,
        Ok(Some(Command::SetConfig { config_id, value })) => {
            state.set(conn, session_id, &config_id, value).await;
            state.publish(hub, &thread.id).await?;
            Idle::Again
        }
        // Stop on a turn the engine started itself.
        Ok(Some(Command::Cancel)) => {
            if unprompted.active().await {
                conn.send_notification(CancelNotification::new(session_id.clone())).map_err(acp_err)?;
                hub.cancel_permissions(&thread.id).await;
                unprompted.cancelled(hub, &thread.id).await;
            }
            Idle::Again
        }
        Ok(Some(Command::Steer(_))) => Idle::Again,
        Ok(None) => Idle::End,
        // Not idle while the engine works on a turn of its own.
        Err(_) if unprompted.active().await => Idle::Again,
        Err(_) => Idle::End,
    })
}

/// A command that comes in while a turn runs. `open` goes false when the commands close;
/// `stop_by` is set by Stop or Steer: an engine that has not ended the turn by then is stopped by Hub.
#[allow(clippy::too_many_arguments)]
async fn mid_turn(
    hub: &Arc<Hub>,
    thread: &Thread,
    conn: &ConnectionTo<Agent>,
    session_id: &SessionId,
    state: &mut SessionState,
    queue: &Parked,
    cmd: Option<Command>,
    (open, stop_by): (&mut bool, &mut Option<tokio::time::Instant>),
) -> Result<()> {
    match cmd {
        Some(Command::Prompt(prompt)) => queue.locked().push_back(prompt),
        Some(Command::SetConfig { config_id, .. }) if config_id == crate::routes::ROUTE => {}
        Some(Command::SetConfig { config_id, value }) => {
            // A refusal is not published: the composer keeps showing the pick, and it
            // is tried again when the turn ends.
            if state.set_during_turn(conn, session_id, config_id, value).await {
                state.publish(hub, &thread.id).await?;
            }
        }
        Some(Command::Cancel) => {
            conn.send_notification(CancelNotification::new(session_id.clone()))
                .map_err(acp_err)?;
            hub.cancel_permissions(&thread.id).await;
            stop_by.get_or_insert(tokio::time::Instant::now() + CANCEL_GRACE);
        }
        // ACP has no way into a running turn, so steering stops it and the message
        // goes first.
        Some(Command::Steer(seq)) => {
            {
                let mut queue = queue.locked();
                if let Some(i) = queue.iter().position(|p| p.seq == seq)
                    && let Some(prompt) = queue.remove(i)
                {
                    queue.push_front(prompt);
                }
            }
            conn.send_notification(CancelNotification::new(session_id.clone()))
                .map_err(acp_err)?;
            hub.cancel_permissions(&thread.id).await;
            stop_by.get_or_insert(tokio::time::Instant::now() + CANCEL_GRACE);
        }
        None => *open = false,
    }
    Ok(())
}
