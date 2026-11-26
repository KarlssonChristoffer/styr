//! `delegate`: hand a brief to a new thread and send its answer back to the caller's thread as a
//! message from it. Watches the delegated thread up to a day, waits out a plan limit that cut its
//! turn off, and keeps the pending report-backs in the store so a restart of the core resumes them.

use crate::locked::ResultExt;
use super::*;

/// How long a delegated thread is watched before Hub stops waiting to report back.
pub(super) const DELEGATE_HOURS: u64 = 24;
/// Delegations still to report back, by the delegated thread, kept in the store so that a restart
/// of the core does not drop the report.
pub(super) const DELEGATIONS: &str = "delegations";
/// A plan limit that stopped the delegated thread is waited out; the wait lasts at least this long.
pub(super) const LIMIT_GRACE_MS: i64 = 2 * 3_600_000;
/// How long after a limit error Hub is given to decide what to do about it before the failure is
/// taken as final.
pub(super) const LIMIT_DECIDES_MS: i64 = 5 * 60_000;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct Delegation {
    /// The thread the report goes to.
    to: String,
    /// The message the delegated thread was asked.
    asked: i64,
    /// When Hub stops waiting, Unix milliseconds.
    until: i64,
}

pub(super) static DELEGATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) async fn delegations(hub: &Hub) -> BTreeMap<String, Delegation> {
    match hub.store.get_value(DELEGATIONS).await {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        _ => BTreeMap::new(),
    }
}

/// Keeps `d` for delegated thread `from`, or forgets it.
pub(super) async fn keep_delegation(hub: &Hub, from: &str, d: Option<Delegation>) {
    let _one_at_a_time = DELEGATION_LOCK.lock().await;
    let mut all = delegations(hub).await;
    match d {
        Some(d) => all.insert(from.to_owned(), d),
        None => all.remove(from),
    };
    if let Ok(json) = serde_json::to_string(&all) {
        hub.store.set_value(DELEGATIONS, &json).await.warn_on_err("save DELEGATIONS");
    }
}

/// Picks up the delegations the core was waiting on when it last stopped.
pub async fn resume_delegations(hub: Arc<Hub>) {
    for (from, d) in delegations(&hub).await {
        tokio::spawn(report_back(hub.clone(), d.to, from, d.asked, d.until));
    }
}

pub(super) async fn delegate(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let Caller::Thread(own) = caller else {
        bail!("delegate reports back to a Hub thread; from outside Hub use start_thread, then wait_for_thread")
    };
    let (thread, asked) = open(hub, caller, args).await?;
    let until = now_ms() + DELEGATE_HOURS as i64 * 3_600_000;
    keep_delegation(hub, &thread.id, Some(Delegation { to: own.id.clone(), asked, until })).await;
    tokio::spawn(report_back(hub.clone(), own.id.clone(), thread.id.clone(), asked, until));
    Ok(json!({
        "thread": brief(&thread),
        "note": "It works on its own. Its reply will arrive here as a message when its turn ends; \
                 end your turn now unless you have other work.",
    }))
}

/// The message that is the delegated thread's job now: the one Hub sent to carry it on after a
/// plan limit or a restart cut the turn off, else the one it was asked.
pub(super) fn carried_on(events: &[Event], asked: i64) -> i64 {
    events
        .iter()
        .filter(|e| e.seq > asked && e.kind == EventKind::UserMessage && (e.has("quota") || e.has("restart")))
        .map(|e| e.seq)
        .max()
        .unwrap_or(asked)
}

/// Whether the turn that failed on a plan limit is being waited out, so that it is not over yet.
pub(super) async fn limit_pending(hub: &Arc<Hub>, id: &str, asked: i64) -> bool {
    let Ok(Some(thread)) = hub.store.thread(id).await else { return false };
    if !hub.guard_prefs().await.map(|g| g.resume).unwrap_or(false) {
        return false;
    }
    let Ok(events) = hub.store.events_of(id, asked, &[EventKind::Error, EventKind::QuotaWait]).await else { return false };
    let Some(error) = events.iter().find(|e| e.kind == EventKind::Error && e.prompt() == Some(asked)) else { return false };
    let phrases = hub.config.traits(&thread.engine).limit_phrases.unwrap_or_default();
    if crate::resume::limit_hit(error.failure().unwrap_or_default(), &phrases).is_none() {
        return false;
    }
    match events.iter().rev().find(|e| e.seq > error.seq && e.kind == EventKind::QuotaWait) {
        Some(w) => w.data_as::<crate::model::QuotaWaitData>().is_some_and(|w| matches!(w.state.as_str(), "waiting" | "resumed" | "switched")),
        // Hub is still reading the plan to see when the limit resets.
        None => now_ms() - error.ts < LIMIT_DECIDES_MS,
    }
}

/// Watches a delegated thread until the brief is answered, then sends the reply to the thread
/// that delegated it, marked as from the delegated thread. A turn that a plan limit cut off is
/// waited out with it, since Hub carries it on when the limit resets.
pub(super) async fn report_back(hub: Arc<Hub>, to: String, from: String, asked: i64, mut until: i64) {
    let result = loop {
        let left = until - now_ms();
        if left <= 0 {
            tracing::warn!(thread = %from, "stopped waiting for a delegated thread to report back");
            keep_delegation(&hub, &from, None).await;
            return;
        }
        let current = match hub.store.events_of(&from, asked, &[EventKind::UserMessage]).await {
            Ok(events) => carried_on(&events, asked),
            Err(_) => asked,
        };
        let slice = Duration::from_secs(WAIT_MAX.min((left / 1000).max(1) as u64));
        match wait(&hub, &from, current, slice).await {
            // Asking the user something: the user answers in Hub, and the turn goes on.
            Ok(r) if r["state"] == "still_working" || r["state"] == "needs_user" => {
                if r["state"] == "needs_user" {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
            Ok(r) if r["state"] == "failed" && limit_pending(&hub, &from, current).await => {
                let longer = now_ms() + LIMIT_GRACE_MS;
                if until < longer {
                    until = longer;
                    keep_delegation(&hub, &from, Some(Delegation { to: to.clone(), asked, until })).await;
                }
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
            Ok(r) => break r,
            Err(err) => {
                tracing::warn!(thread = %from, "a delegated thread cannot report back: {err:#}");
                keep_delegation(&hub, &from, None).await;
                return;
            }
        }
    };
    keep_delegation(&hub, &from, None).await;
    let Ok(Some(thread)) = hub.store.thread(&from).await else { return };
    let state = result["state"].as_str().unwrap_or("done");
    let reply = result["reply"].as_str().unwrap_or_default();
    let reply = if reply.is_empty() { "(no reply)" } else { reply };
    let message = format!(
        "Delegated thread \"{}\" ({}) is {state}. Its reply:\n\n{reply}\n\nread_thread {} for more, \
         send_to_thread to follow up.",
        thread.title, thread.id, thread.id
    );
    if let Err(err) = deliver(&hub, &Caller::Thread(thread), &to, &message, vec![]).await {
        tracing::warn!(thread = %to, "a delegated thread's reply could not be delivered: {err:#}");
    }
}
