//! Quiet sources: things that should be heard from and are not. A cron job that reports with a
//! Sentry SDK's check-ins (`monitor_slug`) and missed its time or failed, a program that has not
//! sent anything for longer than the user said it should, a machine that has been offline for
//! half an hour, a routine whose run failed. Each becomes a problem from the source "monitor",
//! and the same one counts up while it lasts.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::{Hit, Incoming, Level, Source};
use crate::hub::Hub;
use crate::model::{HostState, now_ms};

const MONITORS: &str = "problems.monitors";
const HEARD: &str = "problems.heard";
const MINUTE_MS: i64 = 60_000;
/// A machine counts as gone after this long offline.
const HOST_GONE_MS: i64 = 30 * MINUTE_MS;

/// When each source was last heard from, by name. Kept in memory and written down on each sweep.
static LAST_HEARD: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);
/// Machines seen offline, since when, and whether that was reported.
static OFFLINE: Mutex<Option<HashMap<String, (i64, bool)>>> = Mutex::new(None);
/// Sources and routine runs already reported as quiet or failed, so a sweep says it once.
static SAID: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// Routine runs that ended before the core started are old news.
static STARTED: Mutex<i64> = Mutex::new(0);

/// A cron job's check-ins.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Monitor {
    pub source: String,
    pub slug: String,
    #[ts(type = "number | null")]
    pub last_ok: Option<i64>,
    #[ts(type = "number | null")]
    pub last_seen: Option<i64>,
    /// "ok", "error", "in_progress" or "missed".
    pub status: String,
    /// How often it runs, from its schedule or from the gap between its check-ins.
    #[ts(type = "number | null")]
    pub every_ms: Option<i64>,
    #[ts(type = "number")]
    pub margin_ms: i64,
}

pub fn heard(source: &str) {
    LAST_HEARD.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).insert(source.to_owned(), now_ms());
}

/// When each source was last heard from.
pub async fn last_heard(hub: &Hub) -> HashMap<String, i64> {
    let mut stored: HashMap<String, i64> = hub.store.get_value(HEARD).await.ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    if let Some(live) = LAST_HEARD.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        for (k, v) in live {
            let e = stored.entry(k.clone()).or_default();
            *e = (*e).max(*v);
        }
    }
    stored
}

pub async fn monitors(hub: &Hub) -> BTreeMap<String, Monitor> {
    hub.store.get_value(MONITORS).await.ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

async fn save_monitors(hub: &Hub, all: &BTreeMap<String, Monitor>) -> Result<()> {
    hub.store.set_value(MONITORS, &serde_json::to_string(all)?).await
}

static MONITOR_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A Sentry check-in: `{"monitor_slug", "status": "in_progress" | "ok" | "error", "monitor_config"}`.
pub async fn check_in(hub: &Arc<Hub>, source: &Source, item: &Value) -> Result<()> {
    let Some(slug) = item["monitor_slug"].as_str().filter(|s| !s.is_empty()) else { return Ok(()) };
    let status = item["status"].as_str().unwrap_or("ok").to_owned();
    let now = now_ms();
    let _held = MONITOR_LOCK.lock().await;
    let mut all = monitors(hub).await;
    let key = format!("{}/{slug}", source.name);
    let m = all.entry(key).or_insert_with(|| Monitor { source: source.name.clone(), slug: slug.to_owned(), margin_ms: 5 * MINUTE_MS, ..Default::default() });
    if let Some(every) = schedule_ms(&item["monitor_config"]["schedule"]) {
        m.every_ms = Some(every);
    }
    if let Some(margin) = item["monitor_config"]["checkin_margin"].as_i64() {
        m.margin_ms = margin.max(1) * MINUTE_MS;
    }
    m.last_seen = Some(now);
    if status == "ok" {
        // Without a schedule, the gap between two good runs is the guess.
        if item["monitor_config"]["schedule"].is_null()
            && let Some(prev) = m.last_ok
            && now - prev > MINUTE_MS
        {
            m.every_ms = Some(now - prev);
        }
        m.last_ok = Some(now);
        SAID.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashSet::new).remove(&format!("missed:{}/{slug}", source.name));
    }
    m.status = status.clone();
    save_monitors(hub, &all).await?;
    drop(_held);
    if status == "error" {
        let message = format!("Cron job {slug} failed");
        super::record(hub, quiet_problem(&source.name, source.folder.clone(), &["monitor", slug, "failed"], &message, Level::Error, json!({ "monitor": slug, "check_in": item }))).await?;
    }
    Ok(())
}

/// `{"type": "interval", "value": 10, "unit": "minute"}` or a crontab with a fixed step.
fn schedule_ms(s: &Value) -> Option<i64> {
    match s["type"].as_str()? {
        "interval" => {
            let n = s["value"].as_i64()?;
            let unit = match s["unit"].as_str()?.trim_end_matches('s') {
                "minute" => MINUTE_MS,
                "hour" => 60 * MINUTE_MS,
                "day" => 24 * 60 * MINUTE_MS,
                "week" => 7 * 24 * 60 * MINUTE_MS,
                "month" => 30 * 24 * 60 * MINUTE_MS,
                "year" => 365 * 24 * 60 * MINUTE_MS,
                _ => return None,
            };
            Some(n * unit)
        }
        "crontab" => cron_ms(s["value"].as_str()?),
        _ => None,
    }
}

/// How often a crontab line runs, for the common shapes: `*/5 * * * *`, `0 * * * *`,
/// `0 3 * * *`, `0 3 * * 1`. Anything else is learned from the check-ins.
fn cron_ms(line: &str) -> Option<i64> {
    let f: Vec<&str> = line.split_whitespace().collect();
    let [min, hour, dom, mon, dow] = f.as_slice() else { return None };
    let step = |s: &str| s.strip_prefix("*/").and_then(|n| n.parse::<i64>().ok());
    let fixed = |s: &str| s.parse::<i64>().is_ok();
    if *dom != "*" || *mon != "*" {
        return None;
    }
    match (step(min), *hour, *dow) {
        (Some(n), "*", "*") => Some(n * MINUTE_MS),
        _ if *min == "*" && *hour == "*" && *dow == "*" => Some(MINUTE_MS),
        _ if fixed(min) && *hour == "*" && *dow == "*" => Some(60 * MINUTE_MS),
        _ if fixed(min) && step(hour).is_some() && *dow == "*" => Some(step(hour)? * 60 * MINUTE_MS),
        _ if fixed(min) && fixed(hour) && *dow == "*" => Some(24 * 60 * MINUTE_MS),
        _ if fixed(min) && fixed(hour) && fixed(dow) => Some(7 * 24 * 60 * MINUTE_MS),
        _ => None,
    }
}

fn quiet_problem(project: &str, folder: Option<String>, words: &[&str], message: &str, level: Level, extra: Value) -> Incoming {
    Incoming {
        project: project.to_owned(),
        source: "monitor".into(),
        level,
        folder,
        fingerprint: Some(words.iter().map(|w| w.to_string()).collect()),
        hit: Hit { ts: now_ms(), message: message.to_owned(), kind: None, extra, ..Default::default() },
    }
}

fn say_once(key: String) -> bool {
    SAID.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashSet::new).insert(key)
}

fn unsay(key: &str) {
    SAID.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashSet::new).remove(key);
}

/// Cron jobs that missed their time.
async fn missed_cron_jobs(hub: &Arc<Hub>, now: i64) -> Result<Vec<Incoming>> {
    let mut found: Vec<Incoming> = Vec::new();
    let mut all = monitors(hub).await;
    let mut changed = false;
    let sources = super::sources(hub).await;
    for m in all.values_mut() {
        let (Some(every), Some(last)) = (m.every_ms, m.last_ok.or(m.last_seen)) else { continue };
        let late = now - last - every - m.margin_ms;
        if late > 0 && say_once(format!("missed:{}/{}", m.source, m.slug)) {
            m.status = "missed".into();
            changed = true;
            let folder = sources.iter().find(|s| s.name == m.source).and_then(|s| s.folder.clone());
            let ago = (now - last) / MINUTE_MS;
            found.push(quiet_problem(
                &m.source,
                folder,
                &["monitor", &m.slug, "missed"],
                &format!("Cron job {} missed its check-in: last heard {ago} minutes ago, it runs every {} minutes", m.slug, every / MINUTE_MS),
                Level::Error,
                json!({ "monitor": m.slug }),
            ));
        }
    }
    if changed {
        save_monitors(hub, &all).await?;
    }
    Ok(found)
}

/// Programs that went silent for longer than their owner said they may.
async fn silent_sources(hub: &Arc<Hub>, now: i64, sources: &[super::Source]) -> Result<Vec<Incoming>> {
    let mut found: Vec<Incoming> = Vec::new();
    let heard = last_heard(hub).await;
    hub.store.set_value(HEARD, &serde_json::to_string(&heard)?).await?;
    for s in sources {
        let Some(hours) = s.quiet_after_hours.filter(|h| *h > 0) else { continue };
        let last = heard.get(&s.name).copied();
        let key = format!("quiet:{}", s.name);
        match last {
            Some(at) if now - at > hours as i64 * 60 * MINUTE_MS => {
                if say_once(key) {
                    found.push(quiet_problem(
                        &s.name,
                        s.folder.clone(),
                        &["source quiet", &s.name],
                        &format!("{} has sent nothing for {} hours (it should within {hours})", s.name, (now - at) / (60 * MINUTE_MS)),
                        Level::Warning,
                        json!({ "source": s.name }),
                    ));
                }
            }
            _ => unsay(&key),
        }
    }
    Ok(found)
}

/// Machines offline for half an hour.
fn offline_machines(hub: &Arc<Hub>, now: i64) -> Vec<Incoming> {
    let mut found: Vec<Incoming> = Vec::new();
    let hosts = hub.remotes.list();
    {
        let mut offline = OFFLINE.lock().unwrap_or_else(|p| p.into_inner());
        let offline = offline.get_or_insert_with(HashMap::new);
        for h in hosts.iter().filter(|h| !h.local && h.setup.is_none()) {
            if h.state == HostState::Offline {
                let entry = offline.entry(h.name.clone()).or_insert((now, false));
                if !entry.1 && now - entry.0 > HOST_GONE_MS {
                    entry.1 = true;
                    found.push(quiet_problem(
                        "Hub",
                        None,
                        &["host offline", &h.name],
                        &format!("{} has been offline for {} minutes: {}", h.name, (now - entry.0) / MINUTE_MS, h.error.as_deref().unwrap_or("no answer")),
                        Level::Warning,
                        json!({ "host": h.name }),
                    ));
                }
            } else {
                offline.remove(&h.name);
            }
        }
    }
    found
}

/// Routine runs that failed since the core started.
async fn failed_routines(hub: &Arc<Hub>) -> Vec<Incoming> {
    let mut found: Vec<Incoming> = Vec::new();
    let since = *STARTED.lock().unwrap_or_else(|p| p.into_inner());
    for r in crate::routines::load(&hub.ctx()).await.unwrap_or_default() {
        for run in r.runs.iter().filter(|run| run.state == crate::model::RunState::Failed && run.at >= since) {
            if say_once(format!("routine:{}:{}", r.id, run.at)) {
                found.push(quiet_problem(
                    "Hub",
                    None,
                    &["routine failed", &r.id],
                    &format!("The routine {} failed: {}", r.name, run.report.lines().next().unwrap_or("no reason given")),
                    Level::Warning,
                    json!({ "routine": r.id, "thread": run.thread }),
                ));
            }
        }
    }
    found
}

/// Looks for what went quiet: missed check-ins, silent sources, machines gone, failed routine runs.
pub async fn sweep(hub: &Arc<Hub>) -> Result<()> {
    let now = now_ms();
    {
        let mut started = STARTED.lock().unwrap_or_else(|p| p.into_inner());
        if *started == 0 {
            *started = now;
        }
    }
    let mut found: Vec<Incoming> = Vec::new();
    found.extend(missed_cron_jobs(hub, now).await?);
    let sources = super::sources(hub).await;
    found.extend(silent_sources(hub, now, &sources).await?);
    found.extend(offline_machines(hub, now));
    found.extend(failed_routines(hub).await);
    for incoming in found {
        super::record(hub, incoming).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_in_milliseconds() {
        assert_eq!(schedule_ms(&json!({ "type": "interval", "value": 10, "unit": "minute" })), Some(10 * MINUTE_MS));
        assert_eq!(cron_ms("*/5 * * * *"), Some(5 * MINUTE_MS));
        assert_eq!(cron_ms("0 * * * *"), Some(60 * MINUTE_MS));
        assert_eq!(cron_ms("0 3 * * *"), Some(24 * 60 * MINUTE_MS));
        assert_eq!(cron_ms("0 3 * * 1"), Some(7 * 24 * 60 * MINUTE_MS));
        assert_eq!(cron_ms("0 3 1 * *"), None);
    }
}
