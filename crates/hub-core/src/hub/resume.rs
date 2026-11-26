//! The `Hub` half of `resume.rs`: waiting for a plan limit to reset and carrying the thread on,
//! or moving it to another engine. Reading a limit and its reset time stays in `resume.rs`,
//! which then names nothing above `usage`.

use crate::locked::LockExt;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde_json::json;

use super::Hub;
use crate::model::{Event, EventKind, QuotaWaitData, ThreadId, ThreadStatus, now_ms};
use crate::resume::{CARRY_ON, exhausted, limit_hit};
use crate::usage::PlanUsage;

/// Hub carries on this long after the reset, so the limit has surely gone.
const MARGIN_MS: i64 = 2 * 60 * 1000;
/// When the reset time is not known, Hub tries again after this long.
const GUESS_MS: i64 = 30 * 60 * 1000;
/// Hub gives up after carrying a thread on this many times without a finished turn.
const MAX_TRIES: usize = 3;
/// How often due waits are looked for. Wall clock, so a PC that slept catches up when it wakes.
const TICK: Duration = Duration::from_secs(20);

impl Hub {
    /// Picks up waits logged before the core last stopped, then carries threads on as their
    /// limits reset.
    pub(crate) async fn watch_waits(self: Arc<Self>) {
        match self.store.latest_of_kind(EventKind::QuotaWait).await {
            Ok(latest) => {
                let mut waits = self.waits.0.locked();
                for e in latest {
                    if let Some(wait) = e.data_as::<QuotaWaitData>().filter(|w| w.state == "waiting") {
                        waits.insert(e.thread_id, (e.seq, wait.at.unwrap_or_default()));
                    }
                }
            }
            Err(err) => tracing::warn!("could not read waiting threads: {err:#}"),
        }
        loop {
            let now = now_ms();
            let due: Vec<ThreadId> =
                self.waits.0.locked().iter().filter(|(_, (_, at))| *at <= now).map(|(t, _)| t.clone()).collect();
            for thread_id in due {
                if let Err(err) = self.carry_on(&thread_id, "hub").await {
                    tracing::warn!(thread = %thread_id, "could not carry on after the limit: {err:#}");
                }
            }
            tokio::time::sleep(TICK).await;
        }
    }

    /// After an engine error: when it was a plan limit, wait for it to reset.
    pub(crate) async fn after_error(self: &Arc<Self>, error: &Event) -> Result<()> {
        let thread_id = error.thread_id.as_str();
        let Some(thread) = self.store.thread(thread_id).await? else { return Ok(()) };
        let phrases = self.config.traits(&thread.engine).limit_phrases.unwrap_or_default();
        let Some(hit) = limit_hit(error.failure().unwrap_or_default(), &phrases) else { return Ok(()) };
        if !self.guard_prefs().await.unwrap_or_default().resume {
            return Ok(());
        }
        let (tries, seq) = crate::resume::tries(&self.ctx(), thread_id, error.seq).await?;
        if tries >= MAX_TRIES {
            let data = QuotaWaitData { state: "gave_up".into(), seq: Some(seq), message: Some(hit.text), tries: Some(tries as u64), ..Default::default() };
            self.emit_typed(thread_id, &data).await?;
            return Ok(());
        }

        let now = now_ms();
        let (limit, at) = match hit.at {
            Some(at) => (None, Some(at)),
            None => match self.fresh_plan(&thread.engine, error.ts).await.and_then(|p| exhausted(&p, now)) {
                Some((limit, at)) => (Some(limit), at),
                None => (None, None),
            },
        };
        let known = at.is_some();
        let at = at.map_or(now + GUESS_MS, |t| (t + MARGIN_MS).max(now + 60_000));
        // A message written while the plan was being read wins.
        let latest = self.store.events(thread_id, error.seq, 1_000).await?;
        if latest.iter().any(|e| e.kind == EventKind::UserMessage) {
            return Ok(());
        }
        let mut data = QuotaWaitData {
            state: "waiting".into(),
            seq: Some(seq),
            at: Some(at),
            known: Some(known),
            limit: Some(limit.as_ref().map(|l| l.label.clone())),
            resets: Some(limit.and_then(|l| l.resets)),
            message: Some(hit.text),
            tries: Some(tries as u64),
            ..Default::default()
        };
        // The engine the thread can go on with meanwhile: another subscription whose own limit
        // has not run out. With both out, or no other, it waits for the reset.
        self.waits.mark_limited(&thread.engine, at);
        let guard = self.guard_prefs().await.unwrap_or_default();
        let elsewhere = match guard.limit_switch.as_str() {
            "auto" | "ask" if thread.host.is_none() => self.partner_engine(&thread.engine),
            _ => None,
        };
        data.offer = elsewhere.clone();
        let wait = self.emit_typed(thread_id, &data).await?;
        self.waits.0.locked().insert(thread_id.to_owned(), (wait.seq, at));
        if let Some(engine) = elsewhere
            && guard.limit_switch == "auto"
        {
            // The turn that failed may still be closing, and a switch waits for the thread to be idle.
            for attempt in 0..20 {
                match self.continue_elsewhere(thread_id, Some(&engine), "hub").await {
                    Ok(()) => break,
                    Err(err) if attempt < 19 && format!("{err:#}").contains("busy") => tokio::time::sleep(Duration::from_millis(250)).await,
                    Err(err) => {
                        tracing::warn!(thread = %thread_id, "could not go on with {engine} after the limit: {err:#}");
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Another subscription engine a thread on `current` can go on with: configured, with plan
    /// limits of its own, not out of them. The default engine first.
    fn partner_engine(&self, current: &str) -> Option<String> {
        let now = now_ms();
        std::iter::once(&self.config.default_engine)
            .chain(self.config.engines.keys())
            .filter(|e| e.as_str() != current && e.as_str() != crate::sim::ENGINE)
            .find(|e| self.config.is_subscription(e) && !self.waits.is_limited(e, now))
            .cloned()
    }

    /// Moves a thread that waits for its plan limit to another subscription engine and goes on
    /// there now: the next message hands the conversation over (see `switch_engine`). `engine`
    /// is the one in the wait's offer, else Hub picks one. `by` is "you" from the thread or "hub".
    pub async fn continue_elsewhere(self: &Arc<Self>, thread_id: &str, engine: Option<&str>, by: &str) -> Result<()> {
        if !self.waits.is_waiting(thread_id) {
            anyhow::bail!("this thread is not waiting for a limit");
        }
        let thread = self.store.thread(thread_id).await?.ok_or_else(|| anyhow::anyhow!("no such thread"))?;
        let engine = match engine {
            Some(e) => e.to_owned(),
            None => self.partner_engine(&thread.engine).ok_or_else(|| anyhow::anyhow!("no other subscription is free to go on with"))?,
        };
        if !self.config.is_subscription(&engine) {
            anyhow::bail!("{engine} bills per use, so Hub does not move a thread there when a limit runs out");
        }
        let from = thread.engine.clone();
        // Fails, and leaves the wait as it is, when the engine is unknown or the thread is busy.
        self.switch_engine(thread_id, engine.clone()).await?;
        let Some((wait, _)) = self.waits.take(thread_id) else { return Ok(()) };
        let switched = QuotaWaitData { to: Some(engine.clone()), from: Some(from.clone()), ..QuotaWaitData::after("switched", wait, by) };
        self.emit_typed(thread_id, &switched).await?;
        let text = format!("Hub: {from} ran out of its plan limit, so this thread moved to {engine}. Carry on where you left off.");
        self.send_from(thread_id, text, Vec::new(), None, Some(("quota", json!(wait))), None).await
    }

    /// `engine`'s plan limits read after `since`, fetched again if need be. None when Hub does
    /// not read that engine's limits, or the fetch failed or is backing off.
    async fn fresh_plan(self: &Arc<Self>, engine: &str, since: i64) -> Option<PlanUsage> {
        if !self.reads_plan_of(engine) {
            return None;
        }
        self.usage.expire();
        self.usage_report(true).await.ok()?;
        for _ in 0..100 {
            if let Some((plan, _)) = self.usage.plan()
                && plan.checked_at >= since
            {
                return Some(plan);
            }
            // Asked once above; when that is over without newer limits, there are none.
            if !self.usage.snapshot().2 {
                return None;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        None
    }

    /// Sends the waiting thread on now, by Hub when the limit reset or by `you` from the thread.
    pub async fn carry_on(self: &Arc<Self>, thread_id: &str, by: &str) -> Result<()> {
        let Some((wait, _)) = self.waits.take(thread_id) else { anyhow::bail!("this thread is not waiting for a limit") };
        let Some(thread) = self.store.thread(thread_id).await? else { return Ok(()) };
        if thread.status == ThreadStatus::InTerminal || thread.status == ThreadStatus::Running {
            // Someone carried on another way; nothing to do.
            self.emit_typed(thread_id, &QuotaWaitData::after("cancelled", wait, "message")).await?;
            return Ok(());
        }
        self.emit_typed(thread_id, &QuotaWaitData::after("resumed", wait, by)).await?;
        self.send_from(thread_id, CARRY_ON.into(), Vec::new(), None, Some(("quota", json!(wait))), None).await
    }

    /// Stops waiting: `by` is "you" from the thread, or "message" when you wrote to it first.
    pub async fn stop_waiting(&self, thread_id: &str, by: &str) -> Result<bool> {
        let Some((wait, _)) = self.waits.take(thread_id) else { return Ok(false) };
        self.emit_typed(thread_id, &QuotaWaitData::after("cancelled", wait, by)).await?;
        Ok(true)
    }
}
