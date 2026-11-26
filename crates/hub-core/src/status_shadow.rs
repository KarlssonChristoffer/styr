//! Shadow mode for the thread status (D3 step 3): `status::Tracker` follows every event the core
//! logs and works out what the status should be; a moment later it is compared with what
//! `Hub::set_status` really set. Nothing here changes behaviour. A difference is counted and kept
//! (the last few, in `kv`) for the card in Problems, so a wrong or missing rule shows before the
//! log becomes the truth.
//!
//! The comparison waits a few seconds after the last event of a thread, because the log and the
//! field are written in two calls and in no fixed order.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::locked::LockExt;
use crate::store::Store;
use crate::model::{Event, ShadowMiss, StatusShadow, ThreadStatus, now_ms};
use crate::status::Tracker;

const KEY: &str = "status.shadow";
const KEPT: usize = 20;
/// How long a thread must be quiet before the two answers are compared.
const SETTLE: Duration = Duration::from_secs(4);

struct Seen {
    tracker: Tracker,
    /// The last event fed in, so a later one cancels an older comparison.
    seq: i64,
    /// False until the thread's own status is read; events before that are not followed.
    ready: bool,
}

#[derive(Default)]
struct Inner {
    /// One count at a time, so two comparisons never overwrite each other.
    counting: tokio::sync::Mutex<()>,
    /// Replaces [`SETTLE`] (tests wait less).
    settle: std::sync::Mutex<Option<Duration>>,
    threads: std::sync::Mutex<std::collections::HashMap<String, Seen>>,
}

#[derive(Default, Clone)]
pub struct Shadow(Arc<Inner>);

impl Shadow {
    /// Feeds a logged event and, if it moved the status, schedules the comparison.
    pub(crate) fn note(&self, store: &Store, event: &Event) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        let first = {
            let mut threads = self.0.threads.locked();
            match threads.get_mut(&event.thread_id) {
                Some(seen) if seen.ready => false,
                Some(_) => return,
                None => {
                    threads.insert(event.thread_id.clone(), Seen { tracker: Tracker::new(ThreadStatus::Idle), seq: event.seq, ready: false });
                    true
                }
            }
        };
        if !first {
            self.feed(store, event);
            return;
        }
        // First sight of this thread since the core started: begin at its real status and
        // compare from the next event on.
        let (shadow, store, id) = (self.clone(), store.clone(), event.thread_id.clone());
        runtime.spawn(async move {
            match store.thread(&id).await {
                Ok(Some(thread)) => {
                    if let Some(seen) = shadow.0.threads.locked().get_mut(&id) {
                        seen.tracker = Tracker::new(thread.status);
                        seen.ready = true;
                    }
                }
                _ => {
                    shadow.0.threads.locked().remove(&id);
                }
            }
        });
    }

    fn feed(&self, store: &Store, event: &Event) {
        let (changed, shadow) = {
            let mut threads = self.0.threads.locked();
            let Some(seen) = threads.get_mut(&event.thread_id) else { return };
            seen.seq = event.seq;
            (seen.tracker.feed(event), seen.tracker.status)
        };
        if !changed {
            return;
        }
        let (me, store, thread_id, seq, kind) = (self.clone(), store.clone(), event.thread_id.clone(), event.seq, event.kind);
        tokio::spawn(async move {
            let settle = me.0.settle.locked().unwrap_or(SETTLE);
            tokio::time::sleep(settle).await;
            if !me.0.threads.locked().get(&thread_id).is_some_and(|s| s.seq == seq) {
                return;
            }
            let Ok(Some(thread)) = store.thread(&thread_id).await else { return };
            let miss = (thread.status != shadow).then(|| ShadowMiss {
                thread_id: thread_id.clone(),
                title: thread.title.clone(),
                seq,
                kind: serde_json::to_value(kind).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default(),
                shadow,
                real: thread.status,
                at: now_ms(),
            });
            if miss.is_some() {
                tracing::warn!(thread = %thread_id, seq, ?shadow, real = ?thread.status, "status shadow disagreed");
                // The field is the truth for now: follow it, so one miss is not counted again.
                if let Some(seen) = me.0.threads.locked().get_mut(&thread_id) {
                    seen.tracker.status = thread.status;
                }
            }
            if let Err(err) = count(&me, &store, miss).await {
                tracing::warn!("could not keep the status shadow: {err:#}");
            }
        });
    }
}

async fn read(store: &Store) -> Result<StatusShadow> {
    Ok(store.get_value(KEY).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

async fn count(me: &Shadow, store: &Store, miss: Option<ShadowMiss>) -> Result<()> {
    let _one = me.0.counting.lock().await;
    let mut report = read(store).await?;
    report.checked += 1;
    if let Some(miss) = miss {
        report.disagreed += 1;
        report.recent.insert(0, miss);
        report.recent.truncate(KEPT);
    }
    store.set_value(KEY, &serde_json::to_string(&report)?).await
}

impl Shadow {
    /// What the shadow has found so far.
    pub async fn report(&self, store: &Store) -> Result<StatusShadow> {
        read(store).await
    }

    /// Forgets what the shadow found (the card's "Clear").
    pub async fn clear(&self, store: &Store) -> Result<()> {
        let _one = self.0.counting.lock().await;
        store.set_value(KEY, &serde_json::to_string(&StatusShadow::default())?).await
    }

    #[doc(hidden)]
    pub fn settle_after(&self, after: Duration) {
        *self.0.settle.locked() = Some(after);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::Hub;
    use crate::model::{EventKind, Thread};
    use crate::{Config, Store};
    use serde_json::json;

    async fn hub_with_thread() -> (Arc<Hub>, String) {
        let config = Config { data_dir: crate::janitor::test_dir(), ..Default::default() };
        let hub = Hub::new(Store::in_memory().unwrap(), config);
        hub.shadow.settle_after(Duration::from_millis(50));
        let t = Thread {
            id: "t".into(), title: "t".into(), cwd: "/tmp".into(), engine: "claude".into(), status: ThreadStatus::Idle,
            engine_session_id: None, title_locked: false, archived: false, returned_at: None, seen_at: None,
            settled_at: None, sent_at: None, pinned_at: None, snoozed_until: None, auto_settle: true,
            parent: None, reading: None, owner: None, cloud: None, host: None, project: None, display: None, created_at: 1, updated_at: 1,
        };
        hub.store.insert_thread(t).await.unwrap();
        (hub, "t".into())
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    #[tokio::test]
    async fn agreeing_statuses_are_counted_and_a_difference_is_kept() {
        let (hub, id) = hub_with_thread().await;
        // First sight only learns the thread's own status.
        hub.emit(&id, EventKind::UserMessage, json!({"text": "hi"})).await.unwrap();
        settle().await;
        // The usual order: the core sets the status, the events follow it.
        hub.set_status(&id, ThreadStatus::Running).await.unwrap();
        hub.emit(&id, EventKind::TurnStart, json!({})).await.unwrap();
        settle().await;
        hub.emit(&id, EventKind::TurnEnd, json!({})).await.unwrap();
        hub.set_status(&id, ThreadStatus::Idle).await.unwrap();
        settle().await;
        let report = hub.shadow.report(&hub.store).await.unwrap();
        assert_eq!((report.checked, report.disagreed), (2, 0), "{report:?}");

        // The core forgets to set the status after a turn start.
        hub.emit(&id, EventKind::TurnStart, json!({})).await.unwrap();
        settle().await;
        let report = hub.shadow.report(&hub.store).await.unwrap();
        assert_eq!((report.checked, report.disagreed), (3, 1));
        assert_eq!(report.recent[0].shadow, ThreadStatus::Running);
        assert_eq!(report.recent[0].real, ThreadStatus::Idle);
        assert_eq!(report.recent[0].kind, "turn_start");

        hub.shadow.clear(&hub.store).await.unwrap();
        assert_eq!(hub.shadow.report(&hub.store).await.unwrap().checked, 0);
    }
}
