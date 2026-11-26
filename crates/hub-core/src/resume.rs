//! Carrying threads on after a plan limit. When a turn stops because the 5-hour or a weekly limit
//! ran out, Hub works out when that limit resets (from the engine's message, or from the plan
//! limits the quota guard reads with the engine's usage probe), logs a `quota_wait`, and at that time sends the
//! agent a message to carry on. A message you write first, or the button in the thread, cancels it.
//! Reading a limit and its reset is here; the `impl Hub` that waits and carries on is `hub/resume.rs`.

use crate::locked::LockExt;
use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::Result;

use crate::ctx::Ctx;
use crate::model::{Event, EventKind, QuotaWaitData, ThreadId};
use crate::usage::{PlanLimit, PlanUsage};

/// What Hub tells the agent when the limit has reset.
pub const CARRY_ON: &str = "Hub: the plan limit that stopped you has reset. Carry on where you left off.";

/// The threads waiting for a limit to reset: the seq of the `waiting` event and when to go on.
#[derive(Default)]
pub struct Waits(pub(crate) Mutex<HashMap<ThreadId, (i64, i64)>>, Mutex<HashMap<String, i64>>);

impl Waits {
    /// When each waiting thread carries on, Unix milliseconds.
    pub fn until(&self) -> std::collections::BTreeMap<ThreadId, i64> {
        self.0.locked().iter().map(|(t, (_, at))| (t.clone(), *at)).collect()
    }

    pub(crate) fn take(&self, thread_id: &str) -> Option<(i64, i64)> {
        self.0.locked().remove(thread_id)
    }

    pub fn is_waiting(&self, thread_id: &str) -> bool {
        self.0.locked().contains_key(thread_id)
    }

    /// Notes that `engine`'s plan limit has run out until `at`, so no thread is moved onto it.
    pub(crate) fn mark_limited(&self, engine: &str, at: i64) {
        self.1.locked().insert(engine.to_owned(), at);
    }

    pub(crate) fn is_limited(&self, engine: &str, now: i64) -> bool {
        self.1.locked().get(engine).is_some_and(|at| *at > now)
    }
}

/// How many times the thread was carried on since its last turn that ended well, and the user
/// message before `error_seq` (what the stopped turn answered).
pub async fn tries(ctx: &Ctx<'_>, thread_id: &str, error_seq: i64) -> Result<(usize, Option<i64>)> {
    let events = ctx.store.events_of(thread_id, 0, &[EventKind::TurnEnd, EventKind::QuotaWait, EventKind::UserMessage]).await?;
    let since_done = events.iter().rev().take_while(|e| !(e.kind == EventKind::TurnEnd && e.ended_well()));
    let tries = since_done.filter_map(Event::data_as::<QuotaWaitData>).filter(|w| w.state == "resumed").count();
    let seq = events.iter().rev().find(|e| e.kind == EventKind::UserMessage && e.seq < error_seq).map(|e| e.seq);
    Ok((tries, seq))
}

/// A turn that stopped on a plan limit, as the engine said it.
#[derive(Debug, PartialEq)]
pub struct LimitHit {
    pub text: String,
    /// When it resets, when the message says so exactly.
    pub at: Option<i64>,
}

/// Recognises the engine's message for a plan limit that ran out by the engine's
/// `limit_phrases`, like Claude Code's "You've hit your limit · resets 3am (Europe/Stockholm)" or
/// the older "Claude AI usage limit reached|<unix time>". Out of credits or blocked by an admin is
/// not a limit that resets, so it is not one.
pub fn limit_hit(message: &str, phrases: &[String]) -> Option<LimitHit> {
    let text = message.trim().trim_start_matches("Internal error:").trim();
    let phrases: Vec<String> = phrases.iter().map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()).collect();
    if let Some((head, time)) = text.split_once('|') {
        let lower = head.to_lowercase();
        if lower.contains("limit") && phrases.iter().any(|p| lower.contains(p.as_str())) {
            let at = time.trim().parse::<i64>().ok().map(|s| s * 1000);
            return Some(LimitHit { text: head.trim().to_owned(), at });
        }
    }
    let lower = text.to_lowercase();
    let at = phrases.iter().filter_map(|s| lower.find(s.as_str())).min()?;
    let text = &text[at..];
    if !text.to_lowercase().contains("limit") {
        return None;
    }
    Some(LimitHit { text: text.lines().next().unwrap_or(text).trim().to_owned(), at: None })
}

/// The moment a limit's `resets` text names, like "Oct 30, 1:50 AM GMT+1" or "4pm UTC", as ms
/// since the epoch. The year is the one that puts it nearest after `now`. Without a UTC offset
/// the moment is unknown.
pub fn reset_at(text: &str, now: i64) -> Option<i64> {
    let re = regex::Regex::new(
        r"(?i)(?:\b(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?\s+(\d{1,2}),?\s+(?:at\s+)?)?\b(\d{1,2})(?::(\d{2}))?\s*(am|pm)?\s*\(?(?:gmt|utc)\s*(?:([+-]\d{1,2})(?::?(\d{2}))?)?\)?",
    )
    .ok()?;
    let caps = re.captures(text)?;
    let num = |i: usize| caps.get(i).and_then(|m| m.as_str().trim_start_matches('+').parse::<i64>().ok());
    let mut hour = num(3)?;
    let minute = num(4).unwrap_or(0);
    if let Some(ampm) = caps.get(5).map(|m| m.as_str().to_lowercase()) {
        if hour == 0 || hour > 12 {
            return None;
        }
        hour = match (ampm.as_str(), hour) {
            ("am", 12) => 0,
            ("pm", h) if h != 12 => h + 12,
            (_, h) => h,
        };
    }
    if hour > 23 || minute > 59 {
        return None;
    }
    // Minutes east of UTC.
    let offset = match caps.get(6) {
        Some(h) => {
            let mins = num(6)?.abs() * 60 + num(7).unwrap_or(0);
            if h.as_str().starts_with('-') { -mins } else { mins }
        }
        None => 0,
    } * 60_000;
    let day_ms = |y: i64, m: i64, d: i64| days_from_civil(y, m, d) * 86_400_000 + (hour * 60 + minute) * 60_000 - offset;
    let (year, _, _) = civil_from_days((now + offset).div_euclid(86_400_000));
    match (caps.get(1), num(2)) {
        (Some(month), Some(day)) => {
            let month = MONTHS.iter().position(|m| month.as_str().to_lowercase().starts_with(m))? as i64 + 1;
            if !(1..=31).contains(&day) {
                return None;
            }
            // "Jan 2" read on Dec 30 is next year.
            [year - 1, year, year + 1].into_iter().map(|y| day_ms(y, month, day)).min_by_key(|t| (t - now).abs())
        }
        _ => {
            let today = (now + offset).div_euclid(86_400_000);
            let (y, m, d) = civil_from_days(today);
            let at = day_ms(y, m, d);
            Some(if at < now - 60 * 60 * 1000 { at + 86_400_000 } else { at })
        }
    }
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// Days since 1970-01-01 of a date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The limit that ran out and when it resets: the full one that resets last (a weekly limit
/// outlasts the 5-hour one), or the fullest when none reads full.
pub fn exhausted(plan: &PlanUsage, now: i64) -> Option<(PlanLimit, Option<i64>)> {
    let full: Vec<&PlanLimit> = plan.limits.iter().filter(|l| l.percent >= 99.5).collect();
    let pick = if full.is_empty() { plan.highest().into_iter().collect() } else { full };
    pick.into_iter()
        .map(|l| (l.clone(), l.resets.as_deref().and_then(|r| reset_at(r, now))))
        .max_by_key(|(_, at)| at.unwrap_or(i64::MIN))
}


#[cfg(test)]
mod tests {
    use super::*;

    // 2026-10-30 10:00 UTC.
    const NOW: i64 = 1_793_354_400_000;

    fn utc(y: i64, m: i64, d: i64, h: i64, min: i64) -> i64 {
        days_from_civil(y, m, d) * 86_400_000 + (h * 60 + min) * 60_000
    }

    #[test]
    fn knows_the_engine_s_limit_messages() {
        let claude = crate::config::EngineTraits::preset("claude").limit_phrases.unwrap();
        let hit = limit_hit("Internal error: You've hit your limit · resets 3am (Europe/Stockholm)", &claude).unwrap();
        assert_eq!(hit.text, "You've hit your limit · resets 3am (Europe/Stockholm)");
        assert_eq!(hit.at, None);
        assert!(limit_hit("You've reached your weekly limit · resets Nov 3, 9am", &claude).is_some());
        let old = limit_hit("Claude AI usage limit reached|1793358000", &claude).unwrap();
        assert_eq!(old.at, Some(1_793_358_000_000));
        assert_eq!(limit_hit("You're out of usage credits", &claude), None);
        assert_eq!(limit_hit("Internal error: overloaded", &claude), None);
        assert_eq!(limit_hit("You've hit your stride", &claude), None);
        // An engine that names no phrases has no limit messages.
        assert_eq!(limit_hit("You've hit your limit", &[]), None);
    }

    #[test]
    fn reads_reset_times_as_usage_words_them() {
        assert_eq!(utc(2026, 10, 30, 10, 0), NOW);
        assert_eq!(reset_at("Oct 30, 1:50 PM GMT+1", NOW), Some(utc(2026, 10, 30, 12, 50)));
        assert_eq!(reset_at("Nov 1, 4:00 PM GMT+1", NOW), Some(utc(2026, 11, 1, 15, 0)));
        assert_eq!(reset_at("Oct 31, 12:30 AM GMT-4", NOW), Some(utc(2026, 10, 31, 4, 30)));
        assert_eq!(reset_at("Oct 30, 3pm (UTC)", NOW), Some(utc(2026, 10, 30, 15, 0)));
        assert_eq!(reset_at("Oct 30, 3pm GMT+5:30", NOW), Some(utc(2026, 10, 30, 9, 30)));
        // Only a time: today, or tomorrow once it has passed.
        assert_eq!(reset_at("4pm UTC", NOW), Some(utc(2026, 10, 30, 16, 0)));
        assert_eq!(reset_at("3am UTC", NOW), Some(utc(2026, 10, 31, 3, 0)));
        // Early January read at the end of December is next year.
        let dec = utc(2026, 12, 30, 12, 0);
        assert_eq!(reset_at("Jan 2, 9:00 AM GMT", dec), Some(utc(2027, 1, 2, 9, 0)));
        // Without an offset the moment is unknown.
        assert_eq!(reset_at("3am (Europe/Stockholm)", NOW), None);
        assert_eq!(reset_at("soon", NOW), None);
    }

    #[test]
    fn the_full_limit_that_resets_last_is_the_one_to_wait_for() {
        let limit = |label: &str, percent: f32, resets: Option<&str>| PlanLimit {
            label: label.into(),
            percent,
            resets: resets.map(str::to_owned),
            resets_at: None,
        };
        let plan = PlanUsage {
            plan: None,
            limits: vec![
                limit("5-hour limit", 100.0, Some("Oct 30, 1:50 PM GMT+1")),
                limit("Weekly · all models", 100.0, Some("Nov 1, 4:00 PM GMT+1")),
                limit("Weekly · Opus", 40.0, Some("Nov 2, 4:00 PM GMT+1")),
            ],
            checked_at: NOW,
        };
        let (l, at) = exhausted(&plan, NOW).unwrap();
        assert_eq!(l.label, "Weekly · all models");
        assert_eq!(at, Some(utc(2026, 11, 1, 15, 0)));

        let plan = PlanUsage { limits: vec![limit("5-hour limit", 97.0, Some("Oct 30, 1:50 PM GMT+1"))], ..plan };
        assert_eq!(exhausted(&plan, NOW).unwrap().0.label, "5-hour limit");
    }
}
