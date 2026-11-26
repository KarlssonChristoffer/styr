//! Keeping the activity list short by itself: the settings for it (when a thread settles, how much
//! a chat shows), the pull request each thread's branch has, and reading GitHub's times. The loop
//! that looks the pull requests up and settles threads is on `Hub`, in `hub/triage.rs`; this file
//! names nothing above it.

use std::collections::BTreeMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::ThreadId;

/// When Hub settles threads by itself. Set in Settings, kept in the store.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ThreadPrefs {
    /// Settle a thread that sat idle this many days. Off at 0.
    pub settle_after_days: u32,
    /// Settle a thread when the pull request of its branch merges.
    pub settle_on_merge: bool,
    /// Watch the pull request a thread opens, so a failed check, a review comment or a conflict
    /// sends the thread back to work (`prwatch.rs`). Stopping a watch in a thread keeps it stopped.
    pub watch_prs: bool,
}

impl Default for ThreadPrefs {
    fn default() -> Self {
        Self { settle_after_days: 3, settle_on_merge: true, watch_prs: true }
    }
}

/// How much of the agent's work a thread shows on the phone: "minimal", "quiet", "normal" or "all".
/// Kept in the store; a level a client does not know reads as Quiet.
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(default)]
#[ts(export)]
pub struct ChatDetail {
    /// The level threads follow that have none of their own. Empty reads as "quiet".
    pub default: String,
    /// A level a thread was given, by thread id.
    pub threads: BTreeMap<String, String>,
}

/// The pull request a thread's branch has on GitHub.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadPr {
    pub number: u32,
    pub title: String,
    pub url: String,
    /// "open", "draft", "merged" or "closed".
    pub state: String,
}

/// Pull requests by thread, as last looked up.
#[derive(Default)]
pub struct Prs(pub(crate) Mutex<BTreeMap<ThreadId, ThreadPr>>);

/// Unix milliseconds of a GitHub time like `2026-10-30T16:57:41Z`.
pub(crate) fn parse_time(s: &str) -> Option<i64> {
    let (date, time) = s.strip_suffix('Z')?.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let mut t = time.split(':').map(|p| p.split('.').next().unwrap_or("0").parse::<i64>());
    let (h, min, sec) = (t.next()?.ok()?, t.next()?.ok()?, t.next()?.ok()?);
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + h) * 60 + min) * 60_000 + sec * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_times() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_time("2026-10-30T16:57:41Z"), Some(1_793_379_461_000));
        assert_eq!(parse_time("2024-02-29T12:00:00Z"), Some(1_709_208_000_000));
        assert_eq!(parse_time("nope"), None);
    }
}
