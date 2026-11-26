//! Where it goes (`GET /api/usage/spend`, Settings > Where it goes): all AI use of the last days,
//! in Hub and outside it, per day.
//!
//! - **Claude** comes from Claude Code's own transcripts (`cc_marks`). An answer in a Hub thread's
//!   session is Hub's, put to who asked for it; one in a session a packing slip names for a
//!   one-shot, or (before the slips) started in the system temp folder, is a Hub helper call
//!   (`engine::complete`); the rest is Claude Code and the Claude app on their own.
//! - **Codex** comes from its session logs (`sessions/YYYY/MM/DD/rollout-*.jsonl`, the
//!   `token_count` events). A session a Hub thread holds, one a packing slip names for a one-shot,
//!   or one started through the ACP adapter Hub runs Codex with, is Hub's; the rest is Codex on
//!   its own.
//! - **DeepSeek** is what its balance fell by each day (`balance::spent_by_day`): real money, and
//!   only Hub uses it. Claude Code answers on DeepSeek's key are left out of the Claude count so
//!   they are not counted twice.
//!
//! Plan use is priced as the same tokens would cost through the API.
//!
//! Asked for every machine (`everywhere`), each online host is asked for its own count and the
//! counts are added up; DeepSeek is one account whichever machine asks, so only this machine's
//! counts.
//!
//! This file is engine-neutral: the lanes and totals. Reading each engine's own records (the
//! lanes it starts with, its first day, its answers, its account's count) is in `spend/engines.rs`.

use std::collections::{BTreeMap, HashMap, HashSet};

use std::sync::Arc;

use anyhow::{Context, Result};

use crate::hub::Hub;
use crate::model::{Spend, SpendEngine, SpendHourly, SpendLane, SpendMachine, SpendSource, Thread, now_ms};

pub const HUB: &str = Spend::HUB;

/// Who a thread's answers count for. `chats` is the projects root, whose `chats/` holds the
/// everyday chats.
pub fn source_of(thread: Option<&Thread>, chats: &str) -> String {
    let Some(t) = thread else { return "deleted".into() };
    if let Some(owner) = &t.owner {
        return serde_json::to_value(owner.kind).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_else(|| "agent".into());
    }
    if t.parent.is_some() {
        "side_job".into()
    } else if crate::connections::is_chat_folder(chats, &t.cwd) {
        "chat".into()
    } else {
        "you".into()
    }
}

#[derive(Default)]
struct Sum {
    cost: f64,
    calls: u32,
    tokens: i64,
    daily: Vec<f64>,
    priced: bool,
}

impl Sum {
    fn new(days: usize) -> Self {
        Sum { daily: vec![0.0; days], priced: true, ..Default::default() }
    }
    fn add(&mut self, day: Option<usize>, cost: f64, tokens: i64, priced: bool) {
        self.cost += cost;
        self.calls += 1;
        self.tokens += tokens;
        self.priced &= priced;
        if let Some(slot) = day.and_then(|d| self.daily.get_mut(d)) {
            *slot += cost;
        }
    }
}

/// Adds answers up as they are found: per lane, Hub's lane per engine and per source.
pub struct Tally {
    dates: Vec<String>,
    tz: i32,
    lanes: BTreeMap<String, Sum>,
    engines: BTreeMap<String, Sum>,
    sources: BTreeMap<String, (SpendSource, HashSet<String>)>,
    hours: Option<Hours>,
    /// Counts tokens in place of dollars.
    tokens: bool,
}

/// The same answers hour by hour, when the page asked for them.
struct Hours {
    start: i64,
    count: usize,
    lanes: BTreeMap<String, Vec<f64>>,
    engines: BTreeMap<String, Vec<f64>>,
    sources: BTreeMap<String, Vec<f64>>,
}

const HOUR: i64 = 3_600_000;

impl Hours {
    fn add(&mut self, lane: &str, engine: Option<(&str, &str)>, ts: i64, cost: f64) {
        if ts < self.start {
            return;
        }
        let at = ((ts - self.start) / HOUR) as usize;
        if at >= self.count {
            return;
        }
        let n = self.count;
        self.lanes.entry(lane.to_owned()).or_insert_with(|| vec![0.0; n])[at] += cost;
        if let Some((e, source)) = engine {
            self.engines.entry(e.to_owned()).or_insert_with(|| vec![0.0; n])[at] += cost;
            self.sources.entry(source.to_owned()).or_insert_with(|| vec![0.0; n])[at] += cost;
        }
    }
}

impl Tally {
    pub fn new(dates: Vec<String>, tz: i32) -> Self {
        let n = dates.len();
        let lanes = std::iter::once(HUB).chain(OWN_LANES.iter().copied()).map(|k| (k.to_owned(), Sum::new(n))).collect();
        Tally { dates, tz, lanes, engines: BTreeMap::new(), sources: BTreeMap::new(), hours: None, tokens: false }
    }

    /// Counts every answer's tokens where it would count its dollars.
    pub fn in_tokens(mut self) -> Self {
        self.tokens = true;
        self
    }

    /// What an answer adds: its dollars, or its tokens when counting those.
    fn amount(&self, cost: f64, tokens: i64) -> f64 {
        if self.tokens { tokens as f64 } else { cost }
    }

    /// Also counts `count` hours from `start` (ms).
    pub fn hourly(mut self, start: i64, count: usize) -> Self {
        self.hours = Some(Hours { start, count, lanes: BTreeMap::new(), engines: BTreeMap::new(), sources: BTreeMap::new() });
        self
    }

    fn day(&self, ts: i64) -> Option<usize> {
        let date = crate::activity::date_of(ts, self.tz);
        self.dates.iter().position(|d| *d == date)
    }

    /// An answer Hub asked for, on `engine`, for `source`, in `thread` when there is one.
    pub fn hub(&mut self, engine: &str, source: &str, thread: Option<&str>, ts: i64, cost: f64, tokens: i64, priced: bool) {
        let cost = self.amount(cost, tokens);
        let (day, n) = (self.day(ts), self.dates.len());
        self.lanes.get_mut(HUB).expect("hub lane").add(day, cost, tokens, priced);
        self.engines.entry(engine.to_owned()).or_insert_with(|| Sum::new(n)).add(day, cost, tokens, priced);
        if let Some(h) = &mut self.hours {
            h.add(HUB, Some((engine, source)), ts, cost);
        }
        let (s, seen) = self
            .sources
            .entry(source.to_owned())
            .or_insert_with(|| (SpendSource { key: source.to_owned(), daily: Some(vec![0.0; n]), ..Default::default() }, HashSet::new()));
        if let Some(slot) = day.and_then(|d| s.daily.as_mut()?.get_mut(d)) {
            *slot += cost;
        }
        s.calls += 1;
        s.tokens += tokens;
        s.cost += cost;
        if let Some(id) = thread {
            seen.insert(id.to_owned());
        }
    }

    /// An answer that ran outside Hub, in `lane`.
    pub fn alone(&mut self, lane: &str, ts: i64, cost: f64, tokens: i64, priced: bool) {
        let cost = self.amount(cost, tokens);
        let (day, n) = (self.day(ts), self.dates.len());
        self.lanes.entry(lane.to_owned()).or_insert_with(|| Sum::new(n)).add(day, cost, tokens, priced);
        if let Some(h) = &mut self.hours {
            h.add(lane, None, ts, cost);
        }
    }

    /// Use counted as a running total rather than per answer, so it adds no answers.
    pub fn lump(&mut self, lane: &str, ts: i64, cost: f64, tokens: i64) {
        let cost = self.amount(cost, tokens);
        let (day, n) = (self.day(ts), self.dates.len());
        let sum = self.lanes.entry(lane.to_owned()).or_insert_with(|| Sum::new(n));
        sum.add(day, cost, tokens, true);
        sum.calls -= 1;
        if let Some(h) = &mut self.hours {
            h.add(lane, None, ts, cost);
        }
    }

    /// Money a paid engine spent on `date`, inside Hub but not put to who asked: its provider
    /// only says what the balance fell by.
    pub fn paid(&mut self, engine: &str, date: &str, cost: f64) {
        // The balance says dollars only.
        if self.tokens {
            return;
        }
        let Some(day) = self.dates.iter().position(|d| d == date) else { return };
        let n = self.dates.len();
        for sum in [self.lanes.get_mut(HUB).expect("hub lane"), self.engines.entry(engine.to_owned()).or_insert_with(|| Sum::new(n))] {
            sum.cost += cost;
            sum.daily[day] += cost;
        }
    }

    pub fn finish(self, days: u32) -> Spend {
        let mut engines: Vec<SpendEngine> = self
            .engines
            .into_iter()
            .map(|(key, s)| SpendEngine { paid: key == "deepseek", key, cost: s.cost, daily: s.daily })
            .collect();
        engines.sort_by(|a, b| b.cost.total_cmp(&a.cost));
        let mut lanes: Vec<SpendLane> = self
            .lanes
            .into_iter()
            .map(|(key, s)| SpendLane {
                engines: if key == HUB { engines.clone() } else { Vec::new() },
                key,
                cost: s.cost,
                calls: s.calls,
                tokens: s.tokens,
                daily: s.daily,
                priced: s.priced,
            })
            .collect();
        lanes.sort_by(|a, b| b.cost.total_cmp(&a.cost));
        let mut sources: Vec<SpendSource> = self
            .sources
            .into_values()
            .map(|(mut s, seen)| {
                s.threads = seen.len() as u32;
                s
            })
            .collect();
        sources.sort_by(|a, b| b.cost.total_cmp(&a.cost));
        let hourly = self.hours.map(|h| {
            let engines: Vec<SpendEngine> = h
                .engines
                .into_iter()
                .map(|(key, daily)| SpendEngine { paid: false, cost: daily.iter().sum(), key, daily })
                .collect();
            let lanes = h
                .lanes
                .into_iter()
                .map(|(key, daily)| SpendLane {
                    engines: if key == HUB { engines.clone() } else { Vec::new() },
                    cost: daily.iter().sum(),
                    key,
                    daily,
                    priced: true,
                    ..Default::default()
                })
                .collect();
            let mut sources: Vec<SpendSource> = h
                .sources
                .into_iter()
                .map(|(key, daily)| SpendSource { cost: daily.iter().sum(), key, daily: Some(daily), ..Default::default() })
                .collect();
            sources.sort_by(|a, b| b.cost.total_cmp(&a.cost));
            SpendHourly { start: h.start, lanes, sources }
        });
        Spend { days, dates: self.dates, lanes, sources, machines: None, notes: None, accounts: None, hourly, unit: self.tokens.then(|| "tokens".into()) }
    }
}

/// What Hub knows about its threads, for telling its answers from the rest.
pub struct Known<'a> {
    pub threads: &'a [Thread],
    /// (thread, engine session) from `session_started` events, older sessions included.
    pub sessions: &'a [(String, String)],
    /// The projects root, whose `chats/` holds the everyday chats.
    pub chats: &'a str,
    /// Engines on a pay-per-token key: their answers are counted from the balance instead.
    pub paid_engines: &'a HashSet<String>,
    /// Engine sessions Hub's one-shots went into, from the packing slips (`package/ledger.rs`).
    pub helpers: &'a HashSet<String>,
}

impl Known<'_> {
    pub(crate) fn owners(&self) -> HashMap<&str, &str> {
        self.sessions
            .iter()
            .map(|(t, s)| (s.as_str(), t.as_str()))
            .chain(self.threads.iter().filter_map(|t| Some((t.engine_session_id.as_deref()?, t.id.as_str()))))
            .collect()
    }
}

mod engines;
pub use engines::*;

/// The longest range asked: ten years.
pub const MAX_DAYS: u32 = Spend::MAX_DAYS;

/// How many days "all time" takes on this machine: from the first day anything was counted
/// (Claude Code, the cloud, a Codex session log) up to today in time zone `tz`.
pub async fn all_days(hub: &Arc<Hub>, tz: i32) -> u32 {
    let now = now_ms();
    let stored = hub.store.first_use().await.ok().flatten();
    let first = [stored, first_use(tz).await].into_iter().flatten().min().unwrap_or(now);
    days_between(&crate::activity::date_of(first, tz), &crate::activity::date_of(now, tz), tz)
}

/// Days from `first` to `last`, both counted, within 1..=MAX_DAYS.
fn days_between(first: &str, last: &str, tz: i32) -> u32 {
    match (crate::activity::start_of(first, tz), crate::activity::start_of(last, tz)) {
        (Some(a), Some(b)) => ((b - a).div_euclid(86_400_000) + 1).clamp(1, MAX_DAYS as i64) as u32,
        _ => 1,
    }
}

/// The last `days` days up to today in time zone `tz` (minutes east of UTC).
pub async fn spend(hub: &Arc<Hub>, days: u32, tz: i32) -> Result<Spend> {
    spend_hours(hub, days, tz, false).await
}

/// How many days back the answer goes hour by hour.
pub const MAX_HOUR_DAYS: u32 = 31;

/// As `spend`, and with `hours` the same use hour by hour over its last `MAX_HOUR_DAYS` days.
pub async fn spend_hours(hub: &Arc<Hub>, days: u32, tz: i32, hours: bool) -> Result<Spend> {
    spend_in(hub, days, tz, hours, false).await
}

/// As `spend_hours`, and with `tokens` counted in tokens in place of dollars.
pub async fn spend_in(hub: &Arc<Hub>, days: u32, tz: i32, hours: bool, tokens: bool) -> Result<Spend> {
    let days = days.clamp(1, MAX_DAYS);
    let today = crate::activity::date_of(now_ms(), tz);
    let to = crate::activity::start_of(&today, tz).context("bad date")? + 86_400_000;
    let from = to - days as i64 * 86_400_000;
    let dates: Vec<String> = (0..days as i64).map(|i| crate::activity::date_of(from + i * 86_400_000 + 1, tz)).collect();
    let threads = hub.store.threads().await?;
    let sessions = hub.store.thread_sessions().await?;
    let chats = hub.projects_root().await.unwrap_or_default();
    let paid_engines: HashSet<String> =
        hub.config.engines.iter().filter(|(_, e)| e.provider.as_deref() == Some("deepseek")).map(|(n, _)| n.clone()).collect();
    let helpers: HashSet<String> =
        crate::package::ledger::helper_sessions(&hub.store, from - 86_400_000).await.unwrap_or_default().into_iter().collect();
    let known = Known { threads: &threads, sessions: &sessions, chats: &chats, paid_engines: &paid_engines, helpers: &helpers };
    let mut tally = Tally::new(dates.clone(), tz);
    if tokens {
        tally = tally.in_tokens();
    }
    if hours {
        let start = from.max(to - MAX_HOUR_DAYS as i64 * 86_400_000);
        tally = tally.hourly(start, ((to - start) / HOUR) as usize);
    }
    count(hub, &mut tally, &known, from, to).await?;
    if !paid_engines.is_empty() {
        for (date, spent) in crate::balance::spent_by_day(hub).await {
            tally.paid("deepseek", &date, spent);
        }
    }
    let mut spend = tally.finish(days);
    finish(hub, &mut spend).await;
    Ok(spend)
}

/// How long one machine may take to answer before the page goes on without it.
const MACHINE_WAIT: std::time::Duration = std::time::Duration::from_secs(8);

/// This machine's spend plus every online host's own, added together, with each machine's part.
/// With `all`, each host counts from its own first day, and the range reaches back to the
/// earliest of them.
pub async fn everywhere(hub: &Arc<Hub>, days: u32, tz: i32, all: bool, hours: bool, tokens: bool) -> Result<Spend> {
    let mut total = spend_in(hub, days, tz, hours, tokens).await?;
    let hosts = hub.remotes.list();
    let mut machines = Vec::new();
    if let Some(here) = hosts.iter().find(|h| h.local) {
        machines.push(part(&here.name, &total, None));
    }
    // Without `everywhere`, so a host counts only itself and never asks the others again. An
    // older host does not know `all` and gives `days`.
    // An older host does not know `hours` and sends days only; its hours are then missing.
    // An older host does not know `unit` and answers in dollars; it is then left out by name.
    let path = format!(
        "/api/usage/spend?days={days}&tz={tz}{}{}{}",
        if all { "&all=true" } else { "" },
        if hours { "&hours=true" } else { "" },
        if tokens { "&unit=tokens" } else { "" }
    );
    let asks = hosts.iter().filter(|h| !h.local).map(|h| {
        let (name, path) = (h.name.clone(), path.clone());
        async move {
            let answer = tokio::time::timeout(MACHINE_WAIT, hub.remotes.get_from::<Spend>(&name, &path)).await;
            (name, answer)
        }
    });
    for (name, answer) in futures::future::join_all(asks).await {
        match answer {
            Ok(Ok(theirs)) if theirs.unit != total.unit => machines.push(part(&name, &Spend::default(), Some("too old to count tokens".into()))),
            Ok(Ok(theirs)) => {
                let theirs = without_paid(theirs);
                machines.push(part(&name, &theirs, None));
                if all {
                    widen(&mut total, &theirs, tz);
                }
                add(&mut total, &theirs);
            }
            Ok(Err(e)) => machines.push(part(&name, &Spend::default(), Some(why_not(&format!("{e:#}"))))),
            Err(_) => machines.push(part(&name, &Spend::default(), Some("no answer in time".into()))),
        }
    }
    total.machines = Some(machines);
    Ok(total)
}

fn why_not(error: &str) -> String {
    if error.contains("offline") {
        "offline".into()
    } else if error.contains("404") {
        "too old to count".into()
    } else {
        error.chars().take(120).collect()
    }
}

fn part(name: &str, spend: &Spend, error: Option<String>) -> SpendMachine {
    SpendMachine {
        name: name.to_owned(),
        cost: spend.lanes.iter().map(|l| l.cost).sum(),
        calls: spend.lanes.iter().map(|l| l.calls).sum(),
        tokens: spend.lanes.iter().map(|l| l.tokens).sum(),
        error,
    }
}

/// Another machine's spend without what is one account's whichever machine asks: the money paid
/// by the token (one balance), the cloud sessions (one login) and the estimate from the plan
/// meter (one plan). This machine counts those.
fn without_paid(mut spend: Spend) -> Spend {
    spend.lanes.retain(|l| !ONE_ACCOUNT_LANES.contains(&l.key.as_str()));
    if let Some(h) = &mut spend.hourly {
        h.lanes.retain(|l| !ONE_ACCOUNT_LANES.contains(&l.key.as_str()));
    }
    for lane in &mut spend.lanes {
        for engine in lane.engines.iter().filter(|e| e.paid) {
            lane.cost -= engine.cost;
            for (day, paid) in lane.daily.iter_mut().zip(&engine.daily) {
                *day -= paid;
            }
        }
        lane.engines.retain(|e| !e.paid);
    }
    spend
}

/// Reaches `total` back to the first date of `more` when that is earlier, with nothing on the
/// days it gains.
fn widen(total: &mut Spend, more: &Spend, tz: i32) {
    let (Some(first), Some(ours)) = (more.dates.first(), total.dates.first()) else { return };
    if first >= ours {
        return;
    }
    let gained = days_between(first, ours, tz).saturating_sub(1) as usize;
    let Some(start) = crate::activity::start_of(first, tz) else { return };
    let dates: Vec<String> = (0..gained as i64).map(|i| crate::activity::date_of(start + i * 86_400_000 + 1, tz)).collect();
    let front = |daily: &mut Vec<f64>| {
        daily.splice(0..0, std::iter::repeat_n(0.0, gained));
    };
    for lane in &mut total.lanes {
        front(&mut lane.daily);
        for engine in &mut lane.engines {
            front(&mut engine.daily);
        }
    }
    for account in total.accounts.iter_mut().flatten() {
        account.daily.splice(0..0, std::iter::repeat_n(0, gained));
    }
    for source in &mut total.sources {
        if let Some(daily) = &mut source.daily {
            front(daily);
        }
    }
    total.dates.splice(0..0, dates);
    total.days = total.dates.len() as u32;
}

/// Adds `more` into `total`, day by day on the dates they share.
fn add(total: &mut Spend, more: &Spend) {
    let at: Vec<Option<usize>> = more.dates.iter().map(|d| total.dates.iter().position(|t| t == d)).collect();
    let days = total.dates.len();
    let add_daily = |into: &mut Vec<f64>, from: &[f64]| {
        into.resize(days, 0.0);
        for (i, v) in from.iter().enumerate() {
            if let Some(Some(j)) = at.get(i) {
                into[*j] += v;
            }
        }
    };
    for lane in &more.lanes {
        let i = match total.lanes.iter().position(|l| l.key == lane.key) {
            Some(i) => i,
            None => {
                total.lanes.push(SpendLane { key: lane.key.clone(), priced: true, daily: vec![0.0; days], ..Default::default() });
                total.lanes.len() - 1
            }
        };
        let into = &mut total.lanes[i];
        into.cost += lane.cost;
        into.calls += lane.calls;
        into.tokens += lane.tokens;
        into.priced &= lane.priced;
        add_daily(&mut into.daily, &lane.daily);
        for engine in &lane.engines {
            let j = match into.engines.iter().position(|e| e.key == engine.key) {
                Some(j) => j,
                None => {
                    into.engines.push(SpendEngine { key: engine.key.clone(), paid: engine.paid, daily: vec![0.0; days], cost: 0.0 });
                    into.engines.len() - 1
                }
            };
            into.engines[j].cost += engine.cost;
            add_daily(&mut into.engines[j].daily, &engine.daily);
        }
    }
    for source in &more.sources {
        let i = match total.sources.iter().position(|s| s.key == source.key) {
            Some(i) => i,
            None => {
                total.sources.push(SpendSource { key: source.key.clone(), daily: Some(vec![0.0; days]), ..Default::default() });
                total.sources.len() - 1
            }
        };
        let s = &mut total.sources[i];
        s.calls += source.calls;
        s.threads += source.threads;
        s.tokens += source.tokens;
        s.cost += source.cost;
        if let (Some(into), Some(from)) = (s.daily.as_mut(), source.daily.as_deref()) {
            add_daily(into, from);
        }
    }
    total.sources.sort_by(|a, b| b.cost.total_cmp(&a.cost));
    if let (Some(into), Some(from)) = (&mut total.hourly, &more.hourly) {
        add_hours(into, from);
    }
}

/// Adds another machine's hours into ours, hour by hour on the hours they share.
fn add_hours(total: &mut SpendHourly, more: &SpendHourly) {
    let shift = (more.start - total.start) / HOUR;
    let add_into = |into: &mut Vec<f64>, from: &[f64]| {
        for (i, v) in from.iter().enumerate() {
            let at = i as i64 + shift;
            if at >= 0 && (at as usize) < into.len() {
                into[at as usize] += v;
            }
        }
    };
    let n = total.lanes.iter().map(|l| l.daily.len()).max().unwrap_or(0);
    for source in &more.sources {
        let Some(from) = source.daily.as_deref() else { continue };
        let i = match total.sources.iter().position(|s| s.key == source.key) {
            Some(i) => i,
            None => {
                total.sources.push(SpendSource { key: source.key.clone(), daily: Some(vec![0.0; n]), ..Default::default() });
                total.sources.len() - 1
            }
        };
        let into = &mut total.sources[i];
        let daily = into.daily.get_or_insert_with(|| vec![0.0; n]);
        add_into(daily, from);
        into.cost = daily.iter().sum();
    }
    for lane in &more.lanes {
        let i = match total.lanes.iter().position(|l| l.key == lane.key) {
            Some(i) => i,
            None => {
                total.lanes.push(SpendLane { key: lane.key.clone(), priced: true, daily: vec![0.0; n], ..Default::default() });
                total.lanes.len() - 1
            }
        };
        let into = &mut total.lanes[i];
        add_into(&mut into.daily, &lane.daily);
        into.cost = into.daily.iter().sum();
        for engine in &lane.engines {
            let j = match into.engines.iter().position(|e| e.key == engine.key) {
                Some(j) => j,
                None => {
                    into.engines.push(SpendEngine { key: engine.key.clone(), daily: vec![0.0; n], ..Default::default() });
                    into.engines.len() - 1
                }
            };
            add_into(&mut into.engines[j].daily, &engine.daily);
            into.engines[j].cost = into.engines[j].daily.iter().sum();
        }
    }
}

#[cfg(test)]
mod tests;
