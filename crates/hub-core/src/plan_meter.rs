//! What Claude's plan spent where Hub cannot see (claude.ai's chat and anything else on the same
//! plan), estimated from the plan's own weekly meter.
//!
//! Each time Claude's limits are read, the weekly meter for all models is kept as a sample
//! (`plan_samples`). A day's climb of the meter is set against the Claude use Hub counted that
//! day, in dollars: on days where nearly everything ran where Hub sees it, that gives what one
//! percent of the week is worth. The rate is taken from the upper part of those days (a day with
//! chat on top climbs more per counted dollar, so it reads low) and kept, so a short range still
//! has one. What the meter climbed beyond what Hub counted, at that rate, is the estimate: the
//! lane `claude_elsewhere`, drawn as approximate (`priced: false`).
//!
//! The other subscriptions' plans for the usage report (`other_plans`) are read here too.

use crate::locked::ResultExt;

use crate::store::Store;
use crate::model::{Spend, SpendLane, now_ms};
pub use crate::model::rows::{Sample};

pub const LANE: &str = "claude_elsewhere";
/// A sample at most this often, unless the meter moved.
const EVERY_MS: i64 = 10 * 60_000;
const RATE_KEY: &str = "plan.rate";
/// A day must climb at least this many points to say anything about the rate.
const MIN_CLIMB: f64 = 2.0;
/// Days it takes before a rate is trusted.
const MIN_DAYS: usize = 3;
/// Claude Code outside Hub (a spend lane key).
pub const CLAUDE_CODE: &str = "claude_code";
/// Claude Code sessions in Anthropic's cloud (`cloud_usage.rs`): one login's, so another
/// machine's count of them is the same sessions again.
pub const CLAUDE_CLOUD: &str = "claude_cloud";
/// The lanes whose dollars come out of Claude's plan.
const CLAUDE_LANES: &[&str] = &[CLAUDE_CODE, CLAUDE_CLOUD];

/// The plans of subscriptions besides the default engine's, each read its own way; one never set
/// up is left out.
pub async fn other_plans() -> Vec<crate::usage::AccountUsage> {
    let Some(dir) = crate::engine_home::engine_home("codex").filter(|d| d.exists()) else { return Vec::new() };
    let logs = tokio::task::spawn_blocking({
        let dir = dir.clone();
        move || crate::codex_live::usage(&dir)
    })
    .await
    .ok();
    // Codex's own answer, right now; the session logs when it cannot give one.
    match (crate::codex_live::read(&dir).await, logs) {
        (Some(plan), Some(logs)) => vec![crate::usage::AccountUsage { plan: Some(plan), note: None, ..logs }],
        (None, Some(logs)) => vec![logs],
        _ => Vec::new(),
    }
}

/// The weekly limit for all models out of Claude's limits, as a sample.
pub fn weekly(plan: &crate::usage::PlanUsage, now: i64) -> Option<Sample> {
    let weekly: Vec<&crate::usage::PlanLimit> = plan.limits.iter().filter(|l| l.label.to_lowercase().contains("week")).collect();
    let l = weekly.iter().find(|l| l.label.to_lowercase().contains("all")).or_else(|| weekly.first())?;
    let window = l.resets_at.map(|r| r.to_string()).or_else(|| l.resets.clone()).unwrap_or_default();
    Some(Sample { ts: now, percent: l.percent as f64, window })
}

/// Keeps the meter as it stands now, at most every ten minutes unless it moved.
pub async fn record(store: &Store, plan: &crate::usage::PlanUsage) {
    let now = now_ms();
    let Some(sample) = weekly(plan, now) else { return };
    let last = store.plan_samples(now - 8 * 86_400_000).await.unwrap_or_default().pop();
    if last.as_ref().is_some_and(|l| l.percent == sample.percent && l.window == sample.window && now - l.ts < EVERY_MS) {
        return;
    }
    if let Err(e) = store.add_plan_sample(&sample).await {
        tracing::debug!("could not keep the plan meter: {e:#}");
    }
}

/// How many points the meter climbed on each of `dates` (in time zone `tz`). A new week starts
/// again from nothing, so its first reading is all climb, but only a week whose meter fell is new:
/// the reset's wording or the limit read can change inside a week (a reset time worded another way,
/// the weekly limit of one model in place of all), and taking each such change for a new week
/// counted the whole meter again every time. A day climbs at most the whole meter.
pub fn climbs(samples: &[Sample], dates: &[String], tz: i32) -> Vec<f64> {
    let mut out = vec![0.0; dates.len()];
    for pair in samples.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let new_week = a.window != b.window && b.percent < a.percent;
        let climb = if new_week { b.percent } else { (b.percent - a.percent).max(0.0) };
        let date = crate::activity::date_of(b.ts, tz);
        if let Some(i) = dates.iter().position(|d| *d == date) {
            out[i] = (out[i] + climb).min(100.0);
        }
    }
    out
}

/// Dollars per point of the week, from days that climbed enough: the upper quarter, since a day
/// with unseen use on top reads low. `None` with too few days.
pub fn rate(climbs: &[f64], counted: &[f64]) -> Option<f64> {
    let mut ratios: Vec<f64> = climbs.iter().zip(counted).filter(|(c, d)| **c >= MIN_CLIMB && **d > 0.0).map(|(c, d)| d / c).collect();
    if ratios.len() < MIN_DAYS {
        return None;
    }
    ratios.sort_by(f64::total_cmp);
    Some(ratios[(ratios.len() * 3 / 4).min(ratios.len() - 1)])
}

/// What each day climbed beyond what was counted, in dollars at `rate`.
pub fn beyond(climbs: &[f64], counted: &[f64], rate: f64) -> Vec<f64> {
    climbs.iter().zip(counted).map(|(c, d)| (c * rate - d).max(0.0)).collect()
}

/// Claude's counted dollars per day: on its own, in the cloud and Hub's Claude engine.
fn counted(spend: &Spend) -> Vec<f64> {
    let mut out = vec![0.0; spend.dates.len()];
    let lanes = spend.lanes.iter().filter(|l| CLAUDE_LANES.contains(&l.key.as_str())).map(|l| &l.daily);
    let hub = spend.lanes.iter().filter(|l| l.key == Spend::HUB).flat_map(|l| l.engines.iter().filter(|e| e.key == "claude")).map(|e| &e.daily);
    for daily in lanes.chain(hub) {
        for (o, v) in out.iter_mut().zip(daily) {
            *o += v;
        }
    }
    out
}

/// Adds the estimate as its own lane, or a note on why there is none yet.
pub async fn add_estimate(store: &Store, spend: &mut Spend, tz: i32) {
    let Some(first) = spend.dates.first().and_then(|d| crate::activity::start_of(d, tz)) else { return };
    // One sample from before the range, so its first day has something to climb from.
    let samples = store.plan_samples(first - 8 * 86_400_000).await.unwrap_or_default();
    let climbs = climbs(&samples, &spend.dates, tz);
    let counted = counted(spend);
    let rate = match rate(&climbs, &counted) {
        Some(r) => {
            store.set_value(RATE_KEY, &r.to_string()).await.warn_on_err("save RATE_KEY");
            Some(r)
        }
        None => store.get_value(RATE_KEY).await.ok().flatten().and_then(|v| v.parse::<f64>().ok()),
    };
    let Some(rate) = rate else {
        if samples.len() > 1 || climbs.iter().any(|c| *c > 0.0) {
            note(spend, "Claude outside Hub's sight is estimated once the plan meter has a few days of readings");
        }
        return;
    };
    let daily = beyond(&climbs, &counted, rate);
    let cost: f64 = daily.iter().sum();
    if cost <= 0.0 {
        return;
    }
    spend.lanes.push(SpendLane { key: LANE.into(), cost, calls: 0, tokens: 0, daily, engines: Vec::new(), priced: false });
    spend.lanes.sort_by(|a, b| b.cost.total_cmp(&a.cost));
}

fn note(spend: &mut Spend, text: &str) {
    spend.notes.get_or_insert_with(Vec::new).push(text.to_owned());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage::{PlanLimit, PlanUsage};

    fn s(ts: i64, percent: f64, window: &str) -> Sample {
        Sample { ts, percent, window: window.into() }
    }

    #[test]
    fn the_weekly_meter_for_all_models_is_the_one_kept() {
        let limit = |label: &str, percent: f32| PlanLimit { label: label.into(), percent, resets: Some("Nov 7".into()), resets_at: None };
        let plan = PlanUsage { plan: Some("max".into()), limits: vec![limit("5-hour limit", 9.0), limit("Weekly · Fable", 2.0), limit("Weekly · all models", 61.0)], checked_at: 0 };
        assert_eq!(weekly(&plan, 5), Some(s(5, 61.0, "Nov 7")));
        assert!(weekly(&PlanUsage { plan: None, limits: vec![limit("5-hour limit", 1.0)], checked_at: 0 }, 5).is_none());
    }

    #[test]
    fn a_day_climbs_what_its_readings_rose_and_a_new_week_starts_from_nothing() {
        let day = 86_400_000;
        let dates = vec![crate::activity::date_of(0, 0), crate::activity::date_of(day, 0)];
        let samples = [s(1000, 10.0, "w1"), s(5000, 18.0, "w1"), s(9000, 17.0, "w1"), s(day + 10, 4.0, "w2"), s(day + 20, 9.0, "w2")];
        assert_eq!(climbs(&samples, &dates, 0), vec![8.0, 9.0]);
    }

    #[test]
    fn a_reset_worded_another_way_is_not_a_new_week() {
        // The same week read with its reset in two wordings, back and forth: 60, 61, 62 is a climb
        // of two points, not 60 + 61 + 62 again and again.
        let dates = vec![crate::activity::date_of(0, 0)];
        let samples = [s(10, 60.0, "Nov 7"), s(20, 61.0, "Nov 7, 9:00 AM"), s(30, 61.0, "Nov 7"), s(40, 62.0, "Nov 7, 9:00 AM")];
        assert_eq!(climbs(&samples, &dates, 0), vec![2.0]);
        // A day never climbs more than the whole meter.
        let wild: Vec<Sample> = (0..10).map(|i| s(10 + i, if i % 2 == 0 { 5.0 } else { 90.0 }, if i % 2 == 0 { "a" } else { "b" })).collect();
        assert!(climbs(&wild, &dates, 0)[0] <= 100.0);
    }

    #[test]
    fn the_rate_comes_from_days_that_were_nearly_all_counted() {
        // Five days: $4 a point when everything was counted, less on days with chat on top.
        let climbs = [10.0, 10.0, 10.0, 10.0, 1.0];
        let counted = [40.0, 38.0, 20.0, 25.0, 50.0];
        assert_eq!(rate(&climbs, &counted), Some(4.0));
        assert_eq!(rate(&climbs[..2], &counted[..2]), None, "two days are too few");
        assert_eq!(beyond(&[10.0, 5.0], &[20.0, 30.0], 4.0), vec![20.0, 0.0]);
    }
}
