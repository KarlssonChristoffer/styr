//! Keeps Guard's own failures from feeding themselves: after failed turns of its thread it is left
//! alone for longer each time, and failures that repeat do not each become a problem.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::hub::Hub;

/// How the turns of Guard's thread ended, kept with its state (`vakten.rs`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Streak {
    /// A message was sent to its thread and the turn has not been counted yet.
    pub sent: bool,
    /// Failed turns in a row.
    fails: u32,
    /// When the latest was noticed.
    failed_at: i64,
}

impl Streak {
    /// After two failed turns in a row (the engine or the network is down) Guard is left alone for
    /// 5, then 15, then 60 minutes, so a broken engine is not woken every few minutes by its own
    /// failure. A turn that gets through starts the count over. True while it must wait.
    pub fn hold(&mut self, failed: bool, now: i64) -> bool {
        if !failed {
            *self = Streak::default();
            return false;
        }
        if self.sent {
            self.fails += 1;
            self.failed_at = now;
            self.sent = false;
        }
        let wait_min = match self.fails {
            0 | 1 => return false,
            2 => 5,
            3 => 15,
            _ => 60,
        };
        now - self.failed_at < wait_min * 60_000
    }
}

/// Failures noted by thread and message, and when.
static FAILED: LazyLock<Mutex<HashMap<(String, String), i64>>> = LazyLock::new(Default::default);

/// Whether a failed turn of `thread_id` is left out of Problems: Guard's own thread never makes
/// problems for Guard to look at (a down engine would wake it with its own failure), and the same
/// failure of any thread comes in at most once a minute (a retry loop would write one per try).
pub(crate) async fn skip_failure(hub: &Hub, thread_id: &str, ts: i64, message: &str) -> bool {
    if super::is_vakten(hub, thread_id).await {
        return true;
    }
    let mut failed = FAILED.lock().unwrap_or_else(|p| p.into_inner());
    if failed.len() > 500 {
        failed.retain(|_, at| ts - *at < 60_000);
    }
    let last = failed.insert((thread_id.to_owned(), crate::problems::normalize(message)), ts);
    last.is_some_and(|at| (0..60_000).contains(&(ts - at)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_waits_longer_after_each_failed_turn_and_a_good_turn_resets() {
        let mut s = Streak::default();
        let at = |min: i64| min * 60_000;
        // One failed turn: tried again right away.
        s.sent = true;
        assert!(!s.hold(true, at(0)));
        // The second in a row: 5 minutes' peace, then 15, then 60.
        s.sent = true;
        assert!(s.hold(true, at(10)));
        assert!(s.hold(true, at(14)));
        assert!(!s.hold(true, at(15)));
        s.sent = true;
        assert!(s.hold(true, at(40)));
        assert!(!s.hold(true, at(55)));
        s.sent = true;
        assert!(s.hold(true, at(100)));
        assert!(!s.hold(true, at(160)));
        // A turn that got through starts over.
        assert!(!s.hold(false, at(170)));
        assert_eq!(s.fails, 0);
    }
}
