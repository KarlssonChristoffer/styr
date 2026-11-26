//! Usage: plan limits (5-hour and weekly windows) and what each thread uses. ACP has no call for
//! plan limits, so they come from the default engine's usage probe (`EngineTraits::usage_probe`)
//! run in a throwaway session: Claude's adapter answers `/usage` locally, without a model call,
//! as Markdown. An engine without a probe is never asked.

use crate::locked::LockExt;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::now_ms;
pub use crate::model::{PlanLimit, PlanUsage, ThreadUsage};




#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UsageReport {
    pub plan: Option<PlanUsage>,
    pub plan_error: Option<String>,
    /// Plan limits are being fetched; ask again in a moment.
    pub refreshing: bool,
    pub threads: Vec<ThreadUsage>,
    /// Limits of the other subscriptions the engines are logged in with, e.g. Codex's.
    pub others: Vec<AccountUsage>,
}

/// What a thread's tokens went to, from the engine's session log (`usage_detail.rs`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ContextDetail {
    /// Whether the engine's log was found; without it the rest is empty and `note` says why.
    pub found: bool,
    pub note: Option<String>,
    /// Answers the engine gave, and how often the conversation was compacted.
    pub answers: u32,
    pub compactions: u32,
    /// Tokens over the whole thread, as the engine counted them: fresh input, output, written
    /// to the cache and read back from it.
    #[ts(type = "number")]
    pub input: u64,
    #[ts(type = "number")]
    pub output: u64,
    #[ts(type = "number")]
    pub cache_write: u64,
    #[ts(type = "number")]
    pub cache_read: u64,
    /// The context of the first answer: the system prompt, tool definitions and rules files
    /// before any work.
    #[ts(type = "number")]
    pub start: u64,
    /// The context of the latest answer.
    #[ts(type = "number")]
    pub now: u64,
    /// Estimated tokens of what was said (your messages and the replies) and of thinking.
    #[ts(type = "number")]
    pub conversation: u64,
    #[ts(type = "number")]
    pub thinking: u64,
    /// Per tool, heaviest first: calls and estimated tokens of their input and output.
    pub tools: Vec<ToolBurn>,
    /// The same per MCP server, with "Built-in tools" for the engine's own.
    pub servers: Vec<ToolBurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolBurn {
    pub name: String,
    /// The MCP server, for `mcp__<server>__<tool>`.
    pub server: Option<String>,
    pub calls: u32,
    #[ts(type = "number")]
    pub tokens: u64,
}

/// One subscription's limits next to Claude's.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AccountUsage {
    /// Key into the configured engines, e.g. "codex".
    pub engine: String,
    pub label: String,
    /// The last limits the engine reported, with `checked_at` when that was.
    pub plan: Option<PlanUsage>,
    /// Why there are none, e.g. not used yet.
    pub note: Option<String>,
    /// What is left on a pay-per-token account instead of limits, e.g. "$4.20" (`balance.rs`).
    #[serde(default)]
    pub balance: Option<String>,
    /// What that balance has fallen by today, in the same currency, e.g. "$0.42" (`balance.rs`).
    #[serde(default)]
    pub spent_today: Option<String>,
}

/// The quota guard: when to warn about plan limits, when to move threads to a cheaper model, and
/// which model gives second opinions. Set in Settings, kept in the store.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct GuardPrefs {
    /// Warn when a plan limit reaches this percent.
    pub warn_at: f32,
    /// Move a thread to `fallback_model` before its next message once a limit reaches this
    /// percent. Off when null.
    pub switch_at: Option<f32>,
    /// Matched against the engine's model ids and names, like "sonnet".
    pub fallback_model: String,
    /// The model for second opinions, Sonnet unless changed. Null picks one other than the
    /// thread's, and a cheap one when a limit is past `warn_at`.
    pub review_model: Option<String>,
    /// Ask the agent once for proof (tests or a screenshot) when a reply changed code without it.
    pub proof: bool,
    /// Look for keys and tokens in what each turn added, and send findings to the agent.
    pub leak_guard: bool,
    /// Warn when two threads change the same file.
    pub clash_warn: bool,
    /// Look for dependencies and CDN links each turn added that have not cleared Customs, hold
    /// them, and tell the agent.
    pub customs: bool,
    /// Carry a thread on by itself when the plan limit that stopped it resets.
    pub resume: bool,
    /// What to do when a thread's plan limit runs out and another subscription engine is free:
    /// "auto" moves it there and carries on, "ask" offers that in the thread, "off" only waits
    /// for the reset (`resume`). Never to an engine that bills per use.
    pub limit_switch: String,
    /// Say when a new thread does not belong to its project's stage now (`docs/FOCUS.md`).
    pub focus_check: bool,
    /// Ask the agent once to check the docs that cover code a reply changed (`covers:`), when it
    /// neither opened nor changed them.
    pub docs_check: bool,
}

impl Default for GuardPrefs {
    fn default() -> Self {
        Self {
            warn_at: 80.0,
            switch_at: None,
            fallback_model: "sonnet".into(),
            review_model: Some("sonnet".into()),
            proof: true,
            leak_guard: true,
            clash_warn: true,
            customs: true,
            resume: true,
            limit_switch: "auto".into(),
            focus_check: true,
            docs_check: true,
        }
    }
}

impl PlanUsage {
    /// The limit closest to running out.
    pub fn highest(&self) -> Option<&PlanLimit> {
        self.limits.iter().max_by(|a, b| a.percent.total_cmp(&b.percent))
    }
}

/// Plan limits are fetched at most this often.
const PLAN_MAX_AGE_MS: i64 = 2 * 60 * 1000;
/// After a failed fetch Hub waits twice as long each time before the next, up to this.
const MAX_BACKOFF_MS: i64 = 60 * 60 * 1000;

#[derive(Default)]
pub struct PlanCache {
    usage: Option<PlanUsage>,
    error: Option<String>,
    refreshing: bool,
    /// The next refresh may not wait for the cached limits to age.
    expired: bool,
    /// Fetches that failed in a row, and when the last one did.
    failures: u32,
    failed_at: i64,
}

impl PlanCache {
    /// Whether a failed fetch was too recent to try again yet.
    fn backing_off(&self, now: i64) -> bool {
        self.failures > 0 && now - self.failed_at < backoff(self.failures)
    }
}

/// How long to wait after `failures` failed fetches in a row.
fn backoff(failures: u32) -> i64 {
    PLAN_MAX_AGE_MS.saturating_mul(1 << failures.saturating_sub(1).min(10)).min(MAX_BACKOFF_MS)
}

#[derive(Default)]
pub struct Usage {
    cache: Mutex<PlanCache>,
}

impl Usage {
    /// The plan limits, if fetched, and whether they are older than the refresh interval.
    pub fn plan(&self) -> Option<(PlanUsage, bool)> {
        let c = self.cache.locked();
        c.usage.clone().map(|u| {
            let stale = now_ms() - u.checked_at >= PLAN_MAX_AGE_MS;
            (u, stale)
        })
    }

    pub fn snapshot(&self) -> (Option<PlanUsage>, Option<String>, bool) {
        let c = self.cache.locked();
        (c.usage.clone(), c.error.clone(), c.refreshing)
    }

    /// Starts from limits kept from an earlier run, unless newer ones are already in.
    pub fn seed(&self, usage: PlanUsage) {
        let mut c = self.cache.locked();
        if c.usage.is_none() {
            c.usage = Some(usage);
        }
    }

    /// Claims the refresh when the cached limits are stale, nobody is fetching them and the last
    /// fetch did not fail too recently.
    pub fn start_refresh(&self) -> bool {
        let mut c = self.cache.locked();
        let now = now_ms();
        let fresh = c.usage.as_ref().is_some_and(|u| now - u.checked_at < PLAN_MAX_AGE_MS);
        if c.refreshing || (fresh && !c.expired) || c.backing_off(now) {
            return false;
        }
        c.refreshing = true;
        c.expired = false;
        true
    }

    /// Lets the next refresh fetch the limits however recent they are, e.g. after one ran out.
    pub fn expire(&self) {
        self.cache.locked().expired = true;
    }

    pub fn finish_refresh(&self, result: anyhow::Result<String>) {
        let mut c = self.cache.locked();
        c.refreshing = false;
        match result.map(|markdown| parse(&markdown)) {
            Ok(usage) if !usage.limits.is_empty() => {
                c.usage = Some(usage);
                c.error = None;
                c.failures = 0;
                return;
            }
            Ok(_) => c.error = Some("The engine reported no plan limits".into()),
            Err(e) => c.error = Some(format!("{e:#}")),
        }
        c.failures += 1;
        c.failed_at = now_ms();
    }

    /// Says there are no plan limits to read, without asking any engine.
    pub fn none(&self, why: &str) {
        let mut c = self.cache.locked();
        if c.usage.is_none() {
            c.error = Some(why.to_owned());
        }
    }
}

/// Reads the limits out of the adapter's `/usage` Markdown, lines like
/// `**5-hour limit** — **9%** · Resets Oct 30, 1:50 AM GMT+1`.
pub fn parse(markdown: &str) -> PlanUsage {
    let mut plan = None;
    let mut limits = Vec::new();
    for line in markdown.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("> Claude ")
            && let Some(name) = rest.strip_suffix(" subscription usage")
        {
            plan = Some(name.replace('\\', ""));
            continue;
        }
        let Some(rest) = line.strip_prefix("**") else { continue };
        let Some((label, rest)) = rest.split_once("** — **") else { continue };
        let Some((percent, rest)) = rest.split_once("%**") else { continue };
        let Ok(percent) = percent.trim().parse::<f32>() else { continue };
        let resets = rest.trim().strip_prefix("· Resets ").map(|r| r.trim().replace('\\', ""));
        limits.push(PlanLimit { label: label.replace('\\', ""), percent, resets, resets_at: None });
    }
    PlanUsage { plan, limits, checked_at: now_ms() }
}

/// What a Codex limit window is called, from its length in minutes.
pub(crate) fn window_label(minutes: Option<i64>, fallback: &str) -> String {
    match minutes {
        Some(300) => "5-hour limit".to_owned(),
        Some(10080) => "Weekly limit".to_owned(),
        Some(m) if m > 0 && m % 1440 == 0 => format!("{}-day limit", m / 1440),
        Some(m) if m > 0 && m % 60 == 0 => format!("{}-hour limit", m / 60),
        _ => fallback.to_owned(),
    }
}

/// Milliseconds since 1970 for a UTC time like `2026-10-30T17:01:02.123Z`.
pub(crate) fn iso_ms(s: &str) -> Option<i64> {
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, sec) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    let ms = s.get(19..).and_then(|f| f.strip_prefix('.')).map_or(0, |f| {
        let digits: String = f.chars().take_while(char::is_ascii_digit).take(3).collect();
        format!("{digits:0<3}").parse().unwrap_or(0)
    });
    // Days from the civil date (Howard Hinnant's algorithm).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (mo + if mo > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + h) * 60 + mi) * 60_000 + sec * 1000 + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backs_off_after_failed_fetches() {
        let usage = Usage::default();
        assert!(usage.start_refresh());
        usage.finish_refresh(Err(anyhow::anyhow!("no engine")));
        // Neither a stale cache nor an expired one asks again straight after a failure.
        usage.expire();
        assert!(!usage.start_refresh());
        assert_eq!(usage.snapshot().1.as_deref(), Some("no engine"));
        assert_eq!([1, 2, 3, 6, 40].map(backoff), [120_000, 240_000, 480_000, 3_600_000, 3_600_000]);
        // Once the wait is over it tries again, and a success starts the count afresh.
        usage.cache.locked().failed_at -= backoff(1);
        assert!(usage.start_refresh());
        usage.finish_refresh(Ok("**5-hour limit** — **9%**".into()));
        assert_eq!(usage.cache.locked().failures, 0);
        // No limits in the answer counts as a failure too.
        usage.expire();
        assert!(usage.start_refresh());
        usage.finish_refresh(Ok("nothing here".into()));
        usage.expire();
        assert!(!usage.start_refresh());
    }

    #[test]
    fn reads_utc_times() {
        assert_eq!(iso_ms("1970-01-01T00:00:01.5Z"), Some(1500));
        assert_eq!(iso_ms("2000-03-01T00:00:00Z"), Some(951_868_800_000));
    }

    #[test]
    fn reads_limits_from_usage_markdown() {
        let md = "## Usage\n\n> Claude max subscription usage\n\n### Limits\n\n\
            **5-hour limit** — **9%** · Resets Oct 30, 1:50 AM GMT+1\n\n`██░░`\n\n\
            **Weekly · all models** — **10%** · Resets Nov 1, 4:00 PM GMT+1\n\n\
            **Weekly · Fable** — **2.5%**\n\n---\n\n### This session\n\n| Cost |\n";
        let usage = parse(md);
        assert_eq!(usage.plan.as_deref(), Some("max"));
        let labels: Vec<_> = usage.limits.iter().map(|l| (l.label.as_str(), l.percent)).collect();
        assert_eq!(labels, [("5-hour limit", 9.0), ("Weekly · all models", 10.0), ("Weekly · Fable", 2.5)]);
        assert_eq!(usage.limits[0].resets.as_deref(), Some("Oct 30, 1:50 AM GMT+1"));
        assert_eq!(usage.limits[2].resets, None);
    }
}
