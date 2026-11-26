//! Vakten ("the guard"): an agent that reads new problems, stamps them (prio, lane, category),
//! writes a contract for each it can fix (repro, cause, done when), and hands that to a fix
//! thread of its own. Hub sorts first without AI ([`super::rules::built_in`] and the user's
//! rules); what is still without a lane is Vakten's queue.
//!
//! Vakten works in one thread of its own, in Hub's data folder, woken a minute after something
//! new comes in (so a burst is one turn), up to [`BATCH`] problems a turn; the rest wait in the
//! queue for the next turn, which starts as soon as this one ends. That thread may read and use
//! Hub's problem tools but changes no files. Fixes happen in ordinary threads it starts in the
//! problem's project, one per problem, all at once; each ends at an open pull request. Vakten does
//! not wait for them, so it keeps sorting while they work.
//!
//! There is one Vakten for every machine: the one on the machine that has the others (the PC).
//! It reads each online machine's queue too (`/api/problems/guard/queue`), and stamps, fixes and
//! tells there through the `guard` routes, so a fix thread for a problem on tincan starts on
//! tincan, where the code is. A machine asked for its queue knows it is guarded ([`guarded`]):
//! its own Vakten stays still, and its unsorted errors do not count as needing the user.
//!
//! The lanes: quiet (fix, say nothing beyond the weekly summary), tell (fix, then tell the user),
//! ask (write a plan and wait for the user's go). Hub holds Vakten to them: `problem_thread`
//! refuses an ask problem, one that came back after a fix, and one without a contract. There is no
//! cap on fix threads. Vakten waits while the panic button holds and while a plan limit is spent.

use crate::locked::{LockExt, ResultExt};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Notify;
use ts_rs::TS;

pub(super) mod backoff;

use super::{Lane, ProblemStatus};
use crate::hub::Hub;
use crate::model::{NewThread, Owner, OwnerKind, StreamMsg, ThreadStatus, now_ms};

const PREFS: &str = "problems.vakten";
use crate::vakten_thread::STATE;
/// A burst of problems is one turn: Vakten waits this long after the first.
const SETTLE: Duration = Duration::from_secs(60);
/// It looks anyway this often, for what the clock changed (archives that ran out, quiet sources).
const TICK: Duration = Duration::from_secs(300);
/// While Vakten's thread is busy and the queue is not empty, it checks back this often.
const BUSY: Duration = Duration::from_secs(20);
const DAY_MS: i64 = 86_400_000;
/// Problems handed over in one turn; the rest wait for the next.
const BATCH: usize = 15;
/// How many it reads from each machine to pick the turn's problems from.
const PEEK: usize = 50;
/// A problem handed over and left as it was comes back after this long, if nothing changed.
const AGAIN: i64 = 6 * 3_600_000;
/// What its thread started with: a hash of the brief and Vakten's tools, so any change to either
/// starts the thread over (an engine keeps the tools it was given when its session began) without
/// anyone having to remember to raise a number.
fn brief_version() -> u32 {
    version_of(BRIEF, &crate::permit::VAKTEN_TOOLS.join(","))
}

/// FNV-1a over the brief, a separator and the tool names.
fn version_of(brief: &str, tools: &str) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in brief.bytes().chain(std::iter::once(0)).chain(tools.bytes()) {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h
}
/// More waited than one turn takes: look again as soon as Vakten is free.
static LEFTOVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Fix threads one problem may have had, Guard's tries again included; then Guard leaves it.
pub const TRIES: usize = 3;

static WAKE: LazyLock<Notify> = LazyLock::new(Notify::new);
/// A guard that has not asked for this long is gone.
const GUARD_MS: i64 = 15 * 60_000;

/// The machine whose Vakten looks after this one, while it keeps asking.
pub fn guarded(hub: &Hub) -> Option<String> {
    let g = hub.remotes.guarding.by.locked();
    g.as_ref().filter(|(_, until)| *until > now_ms()).map(|(by, _)| by.clone())
}

fn mark_guarded(hub: &Hub, by: &str) {
    *hub.remotes.guarding.by.locked() = Some((by.to_owned(), now_ms() + GUARD_MS));
}

/// Whether some Vakten reads this machine's problems: its own, or another machine's.
pub async fn watched(hub: &Hub) -> bool {
    prefs(hub).await.on || guarded(hub).is_some()
}

/// Vakten's queue, as SQL on `problems p`: open, at or above its level (the parameter), and
/// without a lane, whether nothing sorted it yet or Hub's first pass only gave it a head start.
fn queued(level: &str) -> String {
    // Vakten's own word (or the user's) stands until something happens to the problem; a rule's
    // stamp without a lane is only a head start.
    format!("p.status = 'open' AND p.merged_into IS NULL AND p.level_rank >= {level} AND p.lane IS NULL AND COALESCE(p.stamped_by, 'rule') = 'rule'")
}

/// Vakten's settings, under Problems.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct VaktenPrefs {
    pub on: bool,
    /// The engine its thread runs on; Hub's default when empty.
    #[ts(optional)]
    pub engine: Option<String>,
    /// The lowest level it looks at by itself: "warning" or "error".
    pub level: super::Level,
}

impl Default for VaktenPrefs {
    fn default() -> Self {
        Self { on: false, engine: None, level: super::Level::Warning }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct VaktenState {
    thread: Option<String>,
    /// Up to when Vakten has been told what happened.
    looked: i64,
    /// The same for each other machine, by name.
    looked_on: HashMap<String, i64>,
    weekly: i64,
    /// What it was handed and when, by "machine:id" ("" for this machine), so a problem it left
    /// as it was is not handed again until it changes or [`AGAIN`] passes.
    handed: HashMap<String, Handed>,
    /// The [`brief_version`] its thread started with.
    brief: u32,
    /// When it last checked the tools that fail most (`tools.rs`), reviewed or not.
    tools: i64,
    /// How its own thread's turns have ended lately (`backoff.rs`).
    streak: backoff::Streak,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Handed {
    count: i64,
    at: i64,
}

/// What the Problems page shows about Vakten.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VaktenStatus {
    pub prefs: VaktenPrefs,
    pub thread: Option<String>,
    /// "off", "paused" (panic button or plan limit), "working", "waiting" (on the user) or "idle".
    pub state: String,
    #[ts(type = "number")]
    pub fix_threads_today: i64,
    /// Open problems without a lane yet: Vakten's queue.
    #[ts(type = "number")]
    pub unstamped: i64,
    /// Its fix threads still working.
    #[ts(type = "number")]
    pub fixing: i64,
    /// The machine whose Vakten looks after this one's problems instead.
    #[ts(optional)]
    pub guarded_by: Option<String>,
}

pub async fn prefs(hub: &Hub) -> VaktenPrefs {
    hub.store.get_value(PREFS).await.ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

pub async fn set_prefs(hub: &Hub, prefs: VaktenPrefs) -> Result<VaktenPrefs> {
    hub.store.set_value(PREFS, &serde_json::to_string(&prefs)?).await?;
    if prefs.on {
        nudge();
    }
    Ok(prefs)
}

async fn state(hub: &Hub) -> VaktenState {
    hub.store.get_value(STATE).await.ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

async fn set_state(hub: &Hub, state: &VaktenState) -> Result<()> {
    hub.store.set_value(STATE, &serde_json::to_string(state)?).await
}

/// Something new happened: Vakten looks within a minute, if it is on.
pub fn nudge() {
    WAKE.notify_one();
}

/// Whether this thread is Vakten's own (it gets [`crate::permit::vakten`]).
pub async fn is_vakten(hub: &Hub, thread_id: &str) -> bool {
    crate::vakten_thread::is_vakten(&hub.store, thread_id).await
}

pub async fn status(hub: &Hub) -> Result<VaktenStatus> {
    let prefs = prefs(hub).await;
    let st = state(hub).await;
    let thread = match &st.thread {
        Some(id) => hub.store.thread(id).await?.filter(|t| !t.archived),
        None => None,
    };
    let guarded_by = guarded(hub);
    let state = if !prefs.on && guarded_by.is_some() {
        "guarded"
    } else if !prefs.on {
        "off"
    } else if hub.halted() || limited(hub) {
        "paused"
    } else {
        match thread.as_ref().map(|t| t.status) {
            Some(ThreadStatus::Running) => "working",
            Some(ThreadStatus::WaitingOnYou) => "waiting",
            _ => "idle",
        }
    };
    let level = prefs.level as i64;
    let (today, unstamped, threads) = hub
        .store
        .with(move |c| {
            let today: i64 = c.query_row(
                "SELECT COUNT(*) FROM problem_activity WHERE by = 'Vakten' AND kind = 'thread' AND ts > ?1",
                [now_ms() - DAY_MS],
                |r| r.get(0),
            )?;
            let unstamped: i64 = c.query_row(&format!("SELECT COUNT(*) FROM problems p WHERE {}", queued("?1")), [level], |r| r.get(0))?;
            let threads: Vec<String> = c
                .prepare("SELECT thread_id FROM problems WHERE status = 'open' AND merged_into IS NULL AND thread_id IS NOT NULL")?
                .query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok((today, unstamped, threads))
        })
        .await?;
    let mut fixing = 0;
    for id in threads {
        if hub.store.thread(&id).await?.is_some_and(|t| !t.archived && t.status == ThreadStatus::Running) {
            fixing += 1;
        }
    }
    let unstamped = unstamped + if prefs.on { remote_waiting(hub) } else { 0 };
    Ok(VaktenStatus { prefs, thread: thread.map(|t| t.id), state: state.into(), fix_threads_today: today, unstamped, fixing, guarded_by })
}

/// A plan limit is spent: some thread waits for it to reset.
fn limited(hub: &Hub) -> bool {
    !hub.waits.until().is_empty()
}

/// Runs for as long as the core: wakes on [`nudge`] or every few minutes, does the clock's work
/// ([`super::watch::sweep`]), then hands Vakten what is new.
pub async fn run(hub: Arc<Hub>) {
    tokio::time::sleep(Duration::from_secs(30)).await;
    let mut wait = TICK;
    loop {
        // Something new: wait a minute so a burst is one turn. The queue draining: go right on.
        if tokio::time::timeout(wait, WAKE.notified()).await.is_ok() {
            tokio::time::sleep(SETTLE).await;
        }
        if let Err(e) = super::watch::sweep(&hub).await {
            tracing::debug!(target: "hub_core::problems", "problems sweep: {e:#}");
        }
        if let Err(e) = look(&hub).await {
            tracing::debug!(target: "hub_core::problems", "Vakten: {e:#}");
        }
        wait = if LEFTOVER.load(std::sync::atomic::Ordering::Relaxed) { BUSY } else { TICK };
    }
}

/// How many problems wait in Vakten's queue on every machine, 0 when it is off.
pub async fn queue(hub: &Hub) -> Result<i64> {
    let prefs = prefs(hub).await;
    if !prefs.on || guarded(hub).is_some() {
        return Ok(0);
    }
    Ok(queued_here(hub, prefs.level as i64).await? + remote_waiting(hub))
}

async fn queued_here(hub: &Hub, level: i64) -> Result<i64> {
    hub.store.with(move |c| Ok(c.query_row(&format!("SELECT COUNT(*) FROM problems p WHERE {}", queued("?1")), [level], |r| r.get(0))?)).await
}

fn remote_waiting(hub: &Hub) -> i64 {
    hub.remotes.guarding.waiting.locked().values().sum()
}

/// One machine's queue, as another machine's Vakten asks for it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GuardQueue {
    pub fresh: Vec<Fresh>,
    /// All that waits there, the handed ones included.
    pub waiting: i64,
    /// What Guard's fix threads there reported (`report.rs`), handed over once; only to a Guard
    /// that asks for them, so an older one never makes them vanish.
    #[serde(default)]
    pub reports: Vec<super::report::Report>,
}

/// What the Vakten of `by` gets from this machine: its queue and what changed since `since`.
/// From now on this machine is guarded, and its own Vakten stays still.
pub async fn guard_queue(hub: &Hub, by: &str, since: i64, level: i64, limit: usize, reports: bool) -> Result<GuardQueue> {
    mark_guarded(hub, by);
    let (fresh, _) = fresh_here(hub, since, level, limit).await?;
    let waiting = queued_here(hub, level).await?;
    let reports = if reports { super::report::take(hub).await } else { Vec::new() };
    Ok(GuardQueue { fresh, waiting, reports })
}

/// Problems on this machine that Vakten should read: its queue, and those that something not
/// Vakten's own did to since `since`. Also the time it looked.
async fn fresh_here(hub: &Hub, since: i64, level: i64, limit: usize) -> Result<(Vec<Fresh>, i64)> {
    hub.store
        .with(move |c| {
            let now = now_ms();
            let mut q = c.prepare(&format!(
                "SELECT p.id, p.title, p.project, p.level, p.count, p.lane,
                    (SELECT GROUP_CONCAT(a.text, '; ') FROM problem_activity a
                       WHERE a.problem_id = p.id AND a.ts > ?1 AND a.by != 'Vakten' AND a.kind IN ('regressed', 'status', 'escalating', 'pr', 'thread', 'stamped')),
                    p.category, p.prio
                 FROM problems p
                 WHERE (({}) OR (p.status = 'open' AND p.merged_into IS NULL AND p.level_rank >= ?2
                   AND EXISTS (SELECT 1 FROM problem_activity a WHERE a.problem_id = p.id AND a.ts > ?1
                          AND a.by != 'Vakten' AND a.kind IN ('regressed', 'status', 'escalating'))))
                   AND p.source != 'code_analysis'
                 ORDER BY p.level_rank DESC, p.last_seen DESC LIMIT ?3",
                queued("?2")
            ))?;
            let rows = q
                .query_map(params![since, level, limit as i64], |r| {
                    Ok(Fresh {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        project: r.get(2)?,
                        level: r.get(3)?,
                        count: r.get(4)?,
                        lane: r.get(5)?,
                        what: r.get(6)?,
                        category: r.get(7)?,
                        prio: r.get(8)?,
                        machine: None,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((rows, now))
        })
        .await
}

/// Every other machine online: its queue too, and from now on it leaves the sorting to us.
async fn read_other_machines(
    hub: &Arc<Hub>,
    st: &mut VaktenState,
    level: i64,
    now: i64,
    (fresh, waiting, reports): (&mut Vec<Fresh>, &mut i64, &mut Vec<super::report::Report>),
) {
    let me = hub.remotes.local_name().to_owned();
    let mut counts = HashMap::new();
    for host in hub.remotes.online() {
        let since = st.looked_on.get(&host).copied().unwrap_or(0);
        let path = format!("/api/problems/guard/queue?by={}&since={since}&level={level}&limit={PEEK}&reports=true", encode(&me));
        match hub.remotes.get_from::<GuardQueue>(&host, &path).await {
            Ok(q) => {
                *waiting += q.waiting;
                counts.insert(host.clone(), q.waiting);
                fresh.extend(q.fresh.into_iter().map(|f| Fresh { machine: Some(host.clone()), ..f }));
                reports.extend(q.reports.into_iter().map(|r| r.from_host(&host)));
                st.looked_on.insert(host, now);
            }
            Err(e) => tracing::debug!(target: "hub_core::problems", "Vakten could not read {host}'s problems: {e:#}"),
        }
    }
    *hub.remotes.guarding.waiting.locked() = counts;
}

/// What Vakten is told: the fix threads' reports, what is new, the weekly summary when it is
/// due, and the tools that fail most.
fn brief_text(reports: &[super::report::Report], fresh: &[Fresh], waiting: i64, weekly_due: bool, tools: &[super::tools::Candidate]) -> String {
    let mut text = String::new();
    if !reports.is_empty() {
        text.push_str(&report_text(reports));
    }
    if !fresh.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str("New or changed since you last looked:\n");
        for f in fresh {
            let on = f.machine.as_deref().map(|m| format!(" on {m}")).unwrap_or_default();
            text.push_str(&format!("- #{}{on} [{}] {}: {} ({}×)", f.id, f.level, f.project, f.title, f.count));
            if let Some(lane) = &f.lane {
                text.push_str(&format!(", lane {lane}"));
            }
            // What Hub's first pass already made of it.
            let head: Vec<&str> = [f.prio.as_deref(), f.category.as_deref()].into_iter().flatten().collect();
            if f.lane.is_none() && !head.is_empty() {
                text.push_str(&format!(", Hub's guess {}", head.join(" ")));
            }
            if let Some(what) = f.what.as_deref().filter(|w| !w.is_empty()) {
                text.push_str(&format!(" · {what}"));
            }
            text.push('\n');
        }
        if LEFTOVER.load(std::sync::atomic::Ordering::Relaxed) && waiting > 0 {
            text.push_str(&format!("\n{waiting} more wait in the queue; you get them next turn, so keep this one short.\n"));
        }
    }
    if weekly_due {
        text.push_str(
            "\nIt is a week since your last summary: also write the weekly summary for the user (what you fixed, what \
             waits for them, what keeps coming back, and the rules you suggest), and say it with tell_user.\n",
        );
    }
    if !tools.is_empty() {
        text.push_str(&super::tools::review_brief(tools));
    }
    text
}

/// Starts Vakten's own thread. Its work is not the user's to sort: as a system thread it is
/// folded under From Hub in the inbox (`system.rs`).
async fn start_thread(hub: &Arc<Hub>, prefs: &VaktenPrefs) -> Result<String> {
    let made = hub.create_thread_as(
        NewThread {
            cwd: hub.config.data_dir.to_string_lossy().into_owned(),
            title: Some("Vakten".into()),
            engine: prefs.engine.clone().filter(|e| !e.trim().is_empty()),
            message: None,
            attachments: Vec::new(),
            settings: Default::default(),
            original: None,
            client_id: None,
            attempts: Vec::new(),
            warm: None,
            design: false,
            unattended: None,
            reach: None,
            cloud: false,
        },
        None,
        Some(Owner::new(OwnerKind::Vakten, "Vakten", None)),
    )
    .await
    .context("could not start Vakten's thread")?;
    Ok(made.id)
}

/// Hands Vakten what came in or changed since it last looked, when it is on and free.
pub async fn look(hub: &Arc<Hub>) -> Result<bool> {
    let prefs = prefs(hub).await;
    // Another machine's Vakten reads this one: one Vakten for all of them.
    if !prefs.on || guarded(hub).is_some() || hub.halted() || limited(hub) {
        return Ok(false);
    }
    let mut st = state(hub).await;
    let mut thread = match &st.thread {
        Some(id) => hub.store.thread(id).await?.filter(|t| !t.archived),
        None => None,
    };
    if thread.as_ref().is_some_and(|t| matches!(t.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou | ThreadStatus::InTerminal)) {
        // Its fix threads reported while it works: look again as soon as it is free.
        if super::report::waiting(hub).await {
            LEFTOVER.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        return Ok(false);
    }
    // Its engine keeps failing: leave it be for a while rather than wake it with the next failure.
    if let Some(t) = &thread
        && st.streak.hold(t.status == ThreadStatus::Error, now_ms())
    {
        set_state(hub, &st).await?;
        return Ok(false);
    }
    // A thread from before the brief or the tools changed keeps the old ones: start over.
    if st.brief != brief_version()
        && let Some(old) = thread.take()
    {
        hub.store.mark_thread(&old.id, |t| t.archived = true).await.warn_on_err("archive the old Vakten thread");
    }
    LEFTOVER.store(false, std::sync::atomic::Ordering::Relaxed);
    let level = prefs.level as i64;
    let (mut fresh, now) = fresh_here(hub, st.looked, level, PEEK).await?;
    let mut waiting = queued_here(hub, level).await?;
    let mut reports = super::report::take(hub).await;
    read_other_machines(hub, &mut st, level, now, (&mut fresh, &mut waiting, &mut reports)).await;
    let (fresh, leftover) = pick(&mut st.handed, fresh, now);
    LEFTOVER.store(leftover, std::sync::atomic::Ordering::Relaxed);
    let weekly_due = now - st.weekly > 7 * DAY_MS && st.weekly > 0;
    // Once a day, the tools that fail most, in one go rather than failure by failure.
    let tools = if now - st.tools > DAY_MS {
        st.tools = now;
        super::tools::review_candidates(hub).await
    } else {
        Vec::new()
    };
    if fresh.is_empty() && reports.is_empty() && !weekly_due && tools.is_empty() {
        set_state(hub, &st).await?;
        return Ok(false);
    }
    let waiting = waiting - fresh.iter().filter(|f| f.lane.is_none()).count() as i64;
    let text = brief_text(&reports, &fresh, waiting, weekly_due, &tools);
    let first = thread.is_none();
    st.brief = brief_version();
    let message = if first { format!("{}\n\n{text}", BRIEF.trim()) } else { text };
    let thread_id = match thread {
        Some(t) => t.id,
        None => start_thread(hub, &prefs).await?,
    };
    st.thread = Some(thread_id.clone());
    st.streak.sent = true;
    st.looked = now;
    if st.weekly == 0 || weekly_due {
        st.weekly = now;
    }
    set_state(hub, &st).await?;
    hub.send_from(&thread_id, message, Vec::new(), None, Some(("vakten", json!({ "problems": fresh.len() }))), None).await?;
    Ok(true)
}

/// What Guard's fix threads reported, and what Guard does about it, without the user.
fn report_text(reports: &[super::report::Report]) -> String {
    let mut text = String::from("Your fix threads reported back:\n");
    for r in reports {
        text.push_str(&r.line());
        text.push('\n');
    }
    text.push_str(&format!(
        "Decide each yourself; the user is not asked. A question or a choice: answer it with answer_worker (pass its \
         thread_id, and its machine when one is given). Blocked, failed, stopped or ended without a pull request: read the \
         thread (read_thread, or the problem's activity), then answer_worker with what to do, or problem_thread with \
         again: true and a new plan when the approach was wrong (at most {TRIES} threads per problem), or leave the problem \
         with stamp_problem or change_problem and say why. Never do the same thing twice.\n"
    ));
    text
}

/// The turn's problems: the most severe first, at most [`BATCH`], leaving out what Vakten was
/// handed before and left as it was (else a problem it cannot settle comes back every look),
/// until it changes or [`AGAIN`] passes. Notes what it hands; true when more waited.
fn pick(handed: &mut HashMap<String, Handed>, mut fresh: Vec<Fresh>, now: i64) -> (Vec<Fresh>, bool) {
    let key = |f: &Fresh| format!("{}:{}", f.machine.as_deref().unwrap_or_default(), f.id);
    handed.retain(|_, h| now - h.at < 7 * DAY_MS);
    fresh.retain(|f| {
        let changed = f.what.as_deref().is_some_and(|w| !w.is_empty());
        handed.get(&key(f)).is_none_or(|h| changed || h.count != f.count || now - h.at > AGAIN)
    });
    fresh.sort_by_key(|f| std::cmp::Reverse(super::Level::parse(&f.level) as i64));
    let leftover = fresh.len() > BATCH;
    fresh.truncate(BATCH);
    for f in &fresh {
        handed.insert(key(f), Handed { count: f.count, at: now });
    }
    (fresh, leftover)
}

/// One problem as Vakten is handed it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fresh {
    id: i64,
    title: String,
    project: String,
    level: String,
    count: i64,
    lane: Option<String>,
    what: Option<String>,
    category: Option<String>,
    prio: Option<String>,
    /// The other machine it is on; None for this one.
    #[serde(default)]
    machine: Option<String>,
}

/// A value for a URL's query.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// What Vakten may fix, as far as the problem itself tells (the rest is in its activity, which
/// [`may_fix`] reads on the machine the problem is on).
pub fn may_fix_problem(p: &super::Problem, again: bool) -> Result<()> {
    let id = p.id;
    if p.status != ProblemStatus::Open {
        bail!("#{id} is not open");
    }
    if p.lane == Some(Lane::Ask) {
        bail!("#{id} is in the ask-first lane: write your plan with stamp_problem (note) and leave it to the user");
    }
    if p.lane.is_none() {
        bail!("stamp #{id} with a lane first");
    }
    if !again && p.thread.is_some() && p.pr.as_ref().is_none_or(|pr| pr.state != "closed") {
        bail!("#{id} already has a thread on it; when it reported back and you want a new attempt, pass again: true");
    }
    Ok(())
}

/// Whether Guard may start problem `id` over (`again`): not while its thread works, and not past
/// [`TRIES`] threads, counted from the problem's activity (newest first, as a detail gives it).
pub fn may_try_again(id: i64, activity: &[super::Activity], running: bool) -> Result<()> {
    if running {
        bail!("#{id}'s fix thread is still working; answer it with answer_worker instead");
    }
    let tries = activity.iter().filter(|a| a.kind == "thread").count();
    if tries >= TRIES {
        bail!("#{id} has had {tries} fix threads; leave it with stamp_problem or change_problem and say why");
    }
    Ok(())
}

/// Whether Vakten may start a fix thread on this problem now, and why not.
pub async fn may_fix(hub: &Hub, id: i64, again: bool) -> Result<()> {
    let detail = super::detail(hub, id, 1).await?.with_context(|| format!("no problem {id}"))?;
    let p = &detail.problem;
    may_fix_problem(p, again)?;
    if again {
        let running = match &p.thread {
            Some(t) => hub.store.thread(t).await?.is_some_and(|t| !t.archived && t.status == ThreadStatus::Running),
            None => false,
        };
        may_try_again(id, &detail.activity, running)?;
    }
    let came_back = hub
        .store
        .with(move |c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM problem_activity WHERE problem_id = ?1 AND kind = 'regressed'
                   AND ts > (SELECT COALESCE(MIN(ts), 0) FROM problem_activity WHERE problem_id = ?1 AND kind = 'thread' AND by = 'Vakten')
                   AND EXISTS (SELECT 1 FROM problem_activity WHERE problem_id = ?1 AND kind = 'thread' AND by = 'Vakten')",
                [id],
                |r| r.get::<_, i64>(0),
            )?)
        })
        .await?;
    if came_back > 0 {
        bail!("#{id} came back after an earlier fix; that is the user's call now");
    }
    Ok(())
}

/// Tells the user something Vakten did ("tell" lane, or the weekly summary): a line on the
/// Problems page and in the problem's activity. It is a desktop notification only when `critical`.
pub async fn tell(hub: &Hub, id: Option<i64>, text: &str, critical: bool) -> Result<()> {
    if let Some(id) = id {
        super::note_activity(hub, id, "Vakten", "told", text).await?;
    }
    hub.publish(StreamMsg::ProblemNews { id, text: text.to_owned(), critical: critical.then_some(true) });
    Ok(())
}

/// What Vakten's thread is told the first time. Later turns get only what is new.
const BRIEF: &str = r#"
(Note from Hub: you are Vakten, the guard of the user's Problems page. Hub collects errors and warnings from Hub
itself, from every machine and from the user's own programs, and sorts what the words alone tell before you see it
("Hub's guess" in the list). You read the rest, sort them, and hand each fixable one to a fix thread of its own with
a contract. Fix threads run side by side and you never wait for them: hand over and move on to the next problem.
You are the one Vakten for all the user's machines: problems from another machine come as #12 on tincan. Pass
that machine as "machine" to every problem tool for it (ids are per machine); its fix thread then starts on that
machine, where its code is. You work in this one thread; you change no files here. Use Hub's tools:

- problems (takes a search: "is:new", "is:unstamped", "project:Hub level:error", "prio:<=p1 thread:no") and problem
  to read; problem_dossier for everything around one (suspect commits, similar problems, other logs at the same
  time, earlier attempts). Read a problem's activity before acting so you never do the same thing twice.
- stamp_problem: prio (P0 down or data lost, P1 security or hits the user, P2 a real fault that can wait, P3
  warnings and clutter), lane, category (bug, config, environment, external, flaky, performance, security, noise,
  expected), confidence (low, medium, high), fixable (yes, no, unknown), and note: what it is, the likely cause,
  your plan, and what you do next. Stamp every problem you are handed.
- change_problem: merge one into another that is the same, archive clutter ("worse" for noise that may grow,
  "forever" for expected), resolve what is no longer true. Always give why.
- problem_thread: hand a problem to a fix thread in its project, with the contract: repro (how to make it happen
  again), cause (what you believe is wrong), done (checkable points that must hold when it is fixed, always
  including the project's tests) and plan. One problem per thread; start as many as there are fixable problems,
  there is no cap. Each ends at an open pull request; the merge queue is off, so the thread has it reviewed, merges it itself when the checks are green and Hub builds the update from the merge. Hub refuses ask-first problems,
  ones that came back after a fix, and a missing contract. Give size: "small" for most (a small model does it),
  "big" when the fix spans several files or the cause is subtle (a mid-sized model). Fix threads run in the
  background on Auto and never ask the user; they never run on the largest model.
- Your fix threads report back to you, never to the user: a question, a choice, being blocked, failing or ending without
  a pull request comes to you as "Your fix threads reported back". You have the user's mandate to decide those
  yourself together with the thread: answer_worker sends the thread your answer and it carries on; problem_thread with
  again: true starts the problem over with a new plan (at most 3 threads per problem); or leave the problem and say why.
  The user is needed only when Hub's auto mode refuses something, and Hub asks them itself.
- tell_user: one line on the Problems page, for the tell lane and the weekly summary. It sends no notification;
  Christoffer wants none from Guard. Pass critical: true only when something is down, data is at risk or a security
  hole is open and he must know now. Otherwise stay quiet; never use it to ask for a click, a go, or a choice, or to correct your own earlier remarks.
- tool_review: once a day Hub may add the agent tools that fail most; review them together and save what you found
  with this tool (it shows on the Problems page's Tool calls tab).

Lanes, what you may do without the user:
- quiet: fix it and say nothing (small, safe, inside one project: a typo, a missing null check, log noise).
- tell: fix it, then tell_user what you did (still no notification).
- ask: write the plan in the note and stop. Use it for changes in behaviour, deletes, the database schema, big
  rewrites, anything on another machine, security holes where the fix is not obvious, and when unsure.
A problem with category missing-viewer ("No viewer for .xlsx files", from the app window) means an agent made a file that
the chat cannot show as what it is. It is a request to build the viewer: stamp it fixable, lane tell, and hand it to
a fix thread whose contract says: repro (open a file of that kind in a thread's reply), done (the kind is in
`app/src/lib/filePreview.ts`, drawn by `FilePreview.svelte` in Hub's own look with no browser controls, a test covers
its parsing, `npm run check` and `cargo test --workspace` pass, the matching section of docs/ARCHITECTURE.md is
updated), plan (read the file in the browser from `/api/project/raw` or `/api/project/file`, no new dependency
unless nothing small exists; for a format only a program can read, convert it in the core and keep the call
additive). One thread per kind; the same kind coming back means the fix is not merged yet. Binary kinds that no
viewer can sensibly show (an executable, a disk image) are noise: archive them "forever".
Never ask the user to click, press, say go, run a command or change a setting, and never suggest turning the merge queue on:
it is deleted, and nothing lands through it. Hub's own steps (updating a frozen list with HUB_BLESS=1, a stale generated
file, a failing check, a setting Hub's tools can reach) are work for you or a fix thread: hand it to problem_thread with a
contract that includes the step, or do it through your tools. If something truly cannot be done by an agent, park the problem
with the reason in its note and carry on; Hub's auto mode asks the user itself when it refuses something.
A P0 or P1 is never quiet. You have lots of freedom inside these lanes; when a choice is close, pick the safer lane.
A problem you leave as it is is not handed to you again until it changes, so stamp each one you can.
End each turn with one short line on what you did; the user reads the Problems page, not this thread.)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_brief_or_tool_list_starts_the_thread_over() {
        assert_eq!(brief_version(), brief_version());
        let base = version_of("brief", "a,b");
        assert_ne!(base, version_of("brief.", "a,b"));
        assert_ne!(base, version_of("brief", "a,b,c"));
        assert_ne!(version_of("ab", "c"), version_of("a", "bc"));
    }

    #[test]
    fn the_brief_never_sends_the_user_to_click() {
        assert!(BRIEF.contains("Never ask the user to click"));
        assert!(BRIEF.contains("never suggest turning the merge queue on"));
    }

    fn fresh(id: i64, machine: Option<&str>, count: i64) -> Fresh {
        Fresh {
            id,
            title: format!("problem {id}"),
            project: "Hub".into(),
            level: "error".into(),
            count,
            lane: None,
            what: None,
            category: None,
            prio: None,
            machine: machine.map(Into::into),
        }
    }

    #[test]
    fn a_problem_starts_over_only_when_its_thread_stopped_and_not_past_three() {
        let thread = |ts| super::super::Activity { ts, by: "Vakten".into(), kind: "thread".into(), text: String::new() };
        let note = super::super::Activity { ts: 0, by: "Vakten".into(), kind: "note".into(), text: String::new() };
        assert!(may_try_again(1, &[thread(1), note.clone()], false).is_ok());
        assert!(may_try_again(1, &[thread(1)], true).is_err());
        assert!(may_try_again(1, &[thread(1), thread(2), thread(3), note], false).is_err());
    }

    #[test]
    fn what_was_left_as_it_was_is_not_handed_again() {
        let mut handed = HashMap::new();
        let now = 1_000_000_000;
        let (first, _) = pick(&mut handed, vec![fresh(1, None, 2), fresh(1, Some("hermes-vps"), 3)], now);
        assert_eq!(first.len(), 2, "the same id on two machines is two problems");
        // Twenty seconds later, nothing changed: nothing to hand.
        let (again, more) = pick(&mut handed, vec![fresh(1, None, 2), fresh(1, Some("hermes-vps"), 3)], now + 20_000);
        assert!(again.is_empty() && !more);
        // It happened again on hermes-vps: that one comes back.
        let (back, _) = pick(&mut handed, vec![fresh(1, None, 2), fresh(1, Some("hermes-vps"), 4)], now + 40_000);
        assert_eq!(back.iter().map(|f| f.machine.as_deref()).collect::<Vec<_>>(), vec![Some("hermes-vps")]);
        // Something was done to it by someone else: it comes back too.
        let mut changed = fresh(1, None, 2);
        changed.what = Some("Reopened".into());
        assert_eq!(pick(&mut handed, vec![changed], now + 60_000).0.len(), 1);
        // And after a while, even unchanged.
        assert_eq!(pick(&mut handed, vec![fresh(1, Some("hermes-vps"), 4)], now + AGAIN + 50_000).0.len(), 1);
        // More than a turn takes: the rest waits, and Vakten is told to look again soon.
        let many: Vec<Fresh> = (10..40).map(|i| fresh(i, None, 1)).collect();
        let (turn, more) = pick(&mut handed, many, now);
        assert_eq!((turn.len(), more), (BATCH, true));
    }
}
