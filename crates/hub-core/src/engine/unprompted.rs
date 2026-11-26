//! Turns an engine starts on its own.
//!
//! ACP knows one kind of turn: the answer to a `session/prompt` Hub sent. Some engines go on after
//! it. Hermes runs `delegate_task` in the background, so the turn that started it ends with "the
//! review is running"; when the helper is done Hermes starts a new turn in the same session and
//! streams `session/update` for it with no prompt behind it. Hub logged those updates but marked
//! no turn, so the thread read as idle while the agent worked (no "working", no end, no
//! notification), and a message sent meanwhile waited behind it unseen.
//!
//! An update that arrives while no prompt is in flight starts an unprompted turn here: the thread
//! shows as working, `turn_start` is logged with `unprompted: true` and no `prompt`, and it ends
//! (`turn_end`, same flag) once no tool call is open and the engine has been quiet for a while. A
//! prompt of Hub's own ends it at once, since the engine answers that one after its own turn.

use crate::locked::ResultExt;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::Mutex;

use crate::hub::Hub;
use crate::model::{ThreadStatus, TurnEndData, TurnStartData};

/// How long the engine must say nothing, with no tool call open, for its turn to count as over
/// (milliseconds; tests shorten it). Per Hub, not per process: all of `tests/it` is one program,
/// and one test's short quiet ended the engines' own turns in every other test's Hub.
pub(crate) struct Quiet(AtomicU64);

impl Default for Quiet {
    fn default() -> Self {
        Self(AtomicU64::new(30_000))
    }
}

#[doc(hidden)]
pub fn set_quiet(hub: &Hub, quiet: Duration) {
    hub.unprompted_quiet.0.store(quiet.as_millis() as u64, Ordering::Relaxed);
}

fn quiet_for(hub: &Hub) -> Duration {
    Duration::from_millis(hub.unprompted_quiet.0.load(Ordering::Relaxed))
}
/// A turn with a tool call that never reported back ends after this much silence anyway.
const STUCK: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
struct Inner {
    active: bool,
    last: Option<Instant>,
    /// Tool calls started and not yet reported finished.
    open: HashSet<String>,
}

#[derive(Default)]
pub(super) struct Unprompted {
    /// Set while a prompt of Hub's is in flight: its updates belong to that turn.
    in_prompt: AtomicBool,
    inner: Mutex<Inner>,
}

impl Unprompted {
    pub(super) fn new() -> Arc<Self> {
        Arc::default()
    }

    /// Hub sent a prompt (or stopped waiting for one).
    pub(super) fn prompting(&self, on: bool) {
        self.in_prompt.store(on, Ordering::Release);
    }

    /// Hub's prompt came back. Updates from here on start a turn of the engine's own, but not
    /// before the prompt's end is logged: hold the returned guard until it is. Without it an
    /// update just after the answer could log its turn's start, and with a short quiet its end,
    /// ahead of the prompt's own end.
    pub(super) async fn prompt_over(&self) -> tokio::sync::MutexGuard<'_, impl Sized> {
        let held = self.inner.lock().await;
        self.prompting(false);
        held
    }

    /// The end of Hub's turn: the thread goes idle, unless a turn of the engine's own has begun
    /// meanwhile and shows it working. Under the turn lock, so the two never cross.
    pub(super) async fn idle_unless_own_turn(&self, hub: &Hub, thread_id: &str) -> anyhow::Result<()> {
        let inner = self.inner.lock().await;
        if !inner.active {
            hub.set_status(thread_id, ThreadStatus::Idle).await?;
        }
        Ok(())
    }

    pub(super) async fn active(&self) -> bool {
        self.inner.lock().await.active
    }

    /// Looks at an update the engine sent. Call it before the update is logged, so the turn's
    /// start comes first.
    pub(super) async fn saw(self: &Arc<Self>, hub: &Arc<Hub>, thread_id: &str, update: &Value) {
        if self.in_prompt.load(Ordering::Acquire) {
            return;
        }
        let kind = crate::model::update_kind(update).unwrap_or("");
        if !matches!(kind, "agent_message_chunk" | "agent_thought_chunk" | "tool_call" | "tool_call_update") {
            return;
        }
        let mut inner = self.inner.lock().await;
        inner.last = Some(Instant::now());
        if let Some(id) = update["toolCallId"].as_str() {
            let done = matches!(update["status"].as_str(), Some("completed" | "failed"));
            if done {
                inner.open.remove(id);
            } else if kind == "tool_call" || kind == "tool_call_update" {
                inner.open.insert(id.to_owned());
            }
        }
        if inner.active {
            return;
        }
        inner.active = true;
        if let Err(err) = start(hub, thread_id).await {
            tracing::warn!(thread = %thread_id, "could not log the engine's own turn: {err:#}");
        }
        tokio::spawn(self.clone().watch(hub.clone(), thread_id.to_owned()));
    }

    /// Ends an unprompted turn that is still going, because Hub is about to send a prompt. The
    /// status is left to that prompt.
    pub(super) async fn hand_over(&self, hub: &Hub, thread_id: &str) {
        let mut inner = self.inner.lock().await;
        if !std::mem::take(&mut inner.active) {
            return;
        }
        inner.open.clear();
        hub.emit_typed(thread_id, &TurnEndData { stop_reason: "end_turn".into(), unprompted: Some(true), handed_over: Some(true), ..Default::default() }).await.warn_on_err("emit TurnEnd");
    }

    /// Stop: ends the unprompted turn in the log once the engine was told to cancel.
    pub(super) async fn cancelled(&self, hub: &Hub, thread_id: &str) {
        let mut inner = self.inner.lock().await;
        if !std::mem::take(&mut inner.active) {
            return;
        }
        inner.open.clear();
        hub.emit_typed(thread_id, &TurnEndData { stop_reason: "cancelled".into(), unprompted: Some(true), ..Default::default() }).await.warn_on_err("emit TurnEnd");
        hub.set_status(thread_id, ThreadStatus::Idle).await.warn_on_err("set status Idle");
    }

    async fn watch(self: Arc<Self>, hub: Arc<Hub>, thread_id: String) {
        loop {
            let quiet_for = quiet_for(&hub);
            tokio::time::sleep((quiet_for / 10).clamp(Duration::from_millis(20), Duration::from_secs(2))).await;
            let mut inner = self.inner.lock().await;
            if !inner.active || self.in_prompt.load(Ordering::Acquire) {
                return;
            }
            let quiet = inner.last.map(|t| t.elapsed()).unwrap_or(quiet_for);
            if (inner.open.is_empty() && quiet >= quiet_for) || quiet >= STUCK {
                inner.active = false;
                inner.open.clear();
                // Held through the log write: a prompt arriving now waits for it.
                hub.emit_typed(&thread_id, &TurnEndData { stop_reason: "end_turn".into(), unprompted: Some(true), ..Default::default() }).await.warn_on_err("emit TurnEnd");
                hub.set_status(&thread_id, ThreadStatus::Idle).await.warn_on_err("set status Idle");
                return;
            }
        }
    }
}

async fn start(hub: &Arc<Hub>, thread_id: &str) -> anyhow::Result<()> {
    hub.set_status(thread_id, ThreadStatus::Running).await?;
    hub.emit_typed(thread_id, &TurnStartData { unprompted: Some(true), ..Default::default() }).await?;
    Ok(())
}
