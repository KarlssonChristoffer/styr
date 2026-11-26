//! How soon a thread's engine is started again after it failed, so a failure that repeats cannot
//! feed itself: each new try waits longer, and after a few in a row what was queued is dropped.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// Failures in a row after which the messages queued behind them wait for the user's next one.
pub(super) const GIVE_UP: u32 = 3;

/// Failed starts in a row per thread. Counted here, not from the log, since other events (queue
/// notes, status) land between two failures and would break a count over the log's last events.
static STREAKS: LazyLock<Mutex<HashMap<String, u32>>> = LazyLock::new(Default::default);

/// Notes one more failure; how many in a row there are now.
pub(super) fn failed(thread_id: &str) -> u32 {
    let mut streaks = STREAKS.lock().unwrap_or_else(|p| p.into_inner());
    let n = streaks.entry(thread_id.to_owned()).or_insert(0);
    *n += 1;
    *n
}

/// The engine got through: the next failure starts the count over.
pub(super) fn ended_well(thread_id: &str) {
    STREAKS.lock().unwrap_or_else(|p| p.into_inner()).remove(thread_id);
}

/// How long to wait before trying again after `streak` failures in a row: 2 s, 4 s, 8 s, up to a minute.
pub(super) fn wait(streak: u32) -> Duration {
    Duration::from_secs((1u64 << streak.clamp(1, 6)).min(60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_failure_in_a_row_waits_longer_and_a_good_run_resets() {
        assert_eq!(failed("t-retry"), 1);
        assert_eq!(failed("t-retry"), 2);
        assert_eq!(failed("t-retry"), 3);
        assert!(wait(1) < wait(2) && wait(2) < wait(3));
        assert_eq!(wait(99), Duration::from_secs(60));
        ended_well("t-retry");
        assert_eq!(failed("t-retry"), 1);
        ended_well("t-retry");
    }
}
