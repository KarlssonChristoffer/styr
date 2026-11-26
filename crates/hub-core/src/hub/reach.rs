//! The `Hub` half of `reach.rs`: a thread's reach changed, so its engine has to start again on it.
//! Saving and reading the reach stay in `reach.rs`, which then names nothing above the database.

use crate::locked::LockExt;

use anyhow::Result;

use super::Hub;
use crate::model::{ThreadReach, ThreadStatus};

impl Hub {
    /// Keeps `reach` for the thread and sees to it that its engine starts on it.
    pub async fn set_reach(&self, thread_id: &str, reach: ThreadReach) -> Result<ThreadReach> {
        let (reach, changed) = crate::reach::save(&self.ctx(), thread_id, reach).await?;
        if changed {
            self.reach_changed(thread_id).await;
        }
        Ok(reach)
    }

    /// A thread's reach changed: its engine, if running, has to start again to use it.
    async fn reach_changed(&self, thread_id: &str) {
        if self.engines().iter().any(|e| e == thread_id) {
            self.reach_stale.locked().insert(thread_id.to_owned());
            self.renew_reach().await;
        }
    }

    /// Stops the idle engines whose reach changed since they started; one in a turn is looked at
    /// again (`engine_renew::renew_once` calls this every few seconds). The threads it stopped.
    pub async fn renew_reach(&self) -> Vec<String> {
        let stale: Vec<String> = self.reach_stale.locked().iter().cloned().collect();
        let mut renewed = Vec::new();
        for id in stale {
            let busy = matches!(
                self.store.thread(&id).await.ok().flatten().map(|t| t.status),
                Some(ThreadStatus::Running | ThreadStatus::WaitingOnYou)
            );
            if busy {
                continue;
            }
            self.reach_stale.locked().remove(&id);
            if !self.engines().contains(&id) {
                continue;
            }
            tracing::info!(thread = %id, "its reach changed; starting its engine again on the new one");
            if let Err(e) = self.shutdown(&id, false).await {
                tracing::warn!(thread = %id, "could not stop the engine for its new reach: {e:#}");
            }
            renewed.push(id);
        }
        renewed
    }
}
