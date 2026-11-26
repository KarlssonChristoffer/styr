//! Running a bench suite: one run at a time, its cases in order and each case's repeats in order,
//! one thread at a time so the timings compare. A repeat lays the case down in a fresh folder
//! under the system temp folder (`hub-bench-<run>/<case>-<n>-<word>`, swept by the janitor after a day),
//! starts a Bench-owned thread there that runs unattended under the bench's own permission answer
//! (`permit::bench`: nothing outside that folder), sends the prompts one by one, waits until the
//! thread has settled (Hub's own follow-ups, such as a request for proof, included), then checks
//! and measures it from the log and the folder, archives the thread, and removes the folder when
//! the repeat passed. A side job the case's agent starts runs under the same answer
//! (`children.rs`), is waited for like a follow-up, and is stopped and archived with the repeat.
//! Results go to files (`bench/results.rs`), never to events.
//!
//! Guards: another run going, every agent paused, a plan limit close to its line, a suite that
//! asks for an engine mode (a mode can stop the engine asking before it acts, and then Hub's
//! answers never fence it). Between repeats a run stops when asked to, when the plan passes 80 %,
//! when its cost passes the suite's cap, or when Hub's version is no longer the one it began on.
//! Its place is kept in kv (`bench.current`), so after a restart it carries on with the repeat it
//! was in, or ends as "Hub updated mid-run" when the restart was an update.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::suite::{Case, Suite};
use super::suite::FAST;
use super::{check, lay_down, measure, results};
use crate::hub::Hub;
use crate::locked::{Ignore, LockExt, ResultExt};
use crate::model::{BenchAsk, BenchCheck, BenchRow, BenchRun, EventKind, NewThread, Owner, OwnerKind, ThreadPatch, ThreadStatus, now_ms};

/// Where a run's place is kept while it goes.
const CURRENT: &str = "bench.current";

/// A run does not start with a plan limit at this percent or more, and stops between repeats at
/// [`PLAN_STOP`].
const PLAN_START: f32 = 60.0;
const PLAN_STOP: f32 = 80.0;
/// Nor within this many points of the quota guard's own line (`GuardPrefs.switch_at`).
const SWITCH_MARGIN: f32 = 10.0;

/// How long a thread must log nothing, after its turn, before it counts as settled: Hub's checks
/// after a turn (`hub/after_turn.rs`) may still log a proof request and send a follow-up then.
const QUIET: Duration = Duration::from_secs(5);

/// The run going in each Hub (by data folder, so tests with their own Hub do not meet).
static GOING: LazyLock<Mutex<HashMap<PathBuf, Arc<Going>>>> = LazyLock::new(Default::default);

struct Going {
    id: String,
    stop: AtomicBool,
    /// "case 2/5 edit-check, repeat 1/3", for the report while it runs.
    at: Mutex<String>,
}

/// The place of the run that is going, kept so a restart can carry on.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Current {
    run: String,
    suite_text: String,
    suite_hash: String,
    case: String,
    repeat: u32,
    version: String,
    #[serde(default)]
    thread: Option<String>,
}

/// What a run needs that the record does not hold.
struct Plan {
    suite: Suite,
    text: String,
    /// The model setting's id and value.
    setting: (String, String),
}

/// Starts a run of a built-in suite. Returns at once with the run's record; the run goes on in
/// the background. Every refusal says why in words the person can act on.
pub async fn start(hub: &Arc<Hub>, ask: &BenchAsk) -> Result<BenchRun> {
    let text = super::suite::builtin_text(&ask.suite).with_context(|| {
        format!("there is no suite called {:?}; the built-in ones are: {}", ask.suite, super::builtin_names().collect::<Vec<_>>().join(", "))
    })?;
    start_text(hub, text, ask).await
}

/// [`start`] for a suite given as its TOML text (tests use their own).
pub async fn start_text(hub: &Arc<Hub>, text: &str, ask: &BenchAsk) -> Result<BenchRun> {
    if hub.halted() {
        bail!("every agent is paused; resume them first, then start the bench again");
    }
    if let Some(going) = going(hub) {
        bail!("a bench run is going already ({}, {}); wait for it or stop it first", going.id, going.at.locked());
    }
    if let Some(stored) = current(hub).await {
        bail!("bench run {} is still marked as going; it carries on when Hub starts, or stop it first", stored.run);
    }
    guard_is_on()?;
    let mut suite = super::parse(text)?;
    if let Some(engine) = ask.engine.as_ref().filter(|e| !e.trim().is_empty()) {
        suite.engine = engine.trim().to_owned();
    }
    if let Some(model) = ask.model.as_ref().filter(|m| !m.trim().is_empty()) {
        suite.model = model.trim().to_owned();
    }
    if let Some(n) = ask.repeats {
        if n == 0 || n > 10 {
            bail!("repeats must be 1 to 10");
        }
        suite.repeats = n;
    }
    if let Some(only) = ask.cases.as_ref().filter(|c| !c.is_empty()) {
        if let Some(unknown) = only.iter().find(|id| !suite.cases.iter().any(|c| &c.id == *id)) {
            bail!("the suite has no case {unknown:?}; it has {}", suite.cases.iter().map(|c| c.id.as_str()).collect::<Vec<_>>().join(", "));
        }
        suite.cases.retain(|c| only.contains(&c.id));
    }
    if let Some(case) = suite.cases.iter().find(|c| !c.mode.trim().is_empty()) {
        bail!("case {:?} asks for the engine mode {:?}; the bench runs every case in the engine's default mode, where it asks before it acts and Hub keeps it inside its folder", case.id, case.mode);
    }
    if !hub.config.engines.contains_key(&suite.engine) {
        bail!("there is no engine called {:?} in Settings > Engines", suite.engine);
    }
    if let Some(why) = plan_too_full(hub, &suite.engine, PLAN_START).await {
        bail!("{why}; start the bench once the plan has room");
    }
    let setting = model_setting(hub, &suite.engine, &suite.model).await?;
    let started = now_ms();
    let run = BenchRun {
        id: format!("{started}-{}", suite.name),
        suite: suite.name.clone(),
        suite_hash: suite.hash.clone(),
        engine: suite.engine.clone(),
        model_asked: setting.1.clone(),
        model_answered: None,
        machine: sysinfo::System::host_name().unwrap_or_else(|| "this computer".into()),
        hub_version: hub.update_status().version,
        started,
        ended: None,
        state: "running".into(),
        why: None,
        repeats: suite.repeats,
        cases: suite.cases.iter().map(|c| c.id.clone()).collect(),
        rows: Vec::new(),
        summary: None,
    };
    results::save(&hub.ctx(), &run)?;
    let plan = Plan { suite, text: text.to_owned(), setting };
    let first = plan.suite.cases.first().map(|c| c.id.clone()).unwrap_or_default();
    let place = Current {
        run: run.id.clone(),
        suite_text: plan.text.clone(),
        suite_hash: run.suite_hash.clone(),
        case: first,
        repeat: 1,
        version: run.hub_version.clone(),
        thread: None,
    };
    remember(hub, &place).await?;
    let going = register(hub, &run.id)?;
    tokio::spawn(go(hub.clone(), run.clone(), plan, going, None));
    Ok(run)
}

/// Asks the run going to stop once its current repeat is done, or ends one that is only marked as
/// going in kv (Hub could not carry it on after a restart). `false` when neither is run `id`.
pub async fn stop(hub: &Arc<Hub>, id: &str) -> bool {
    if let Some(g) = going(hub) {
        if g.id != id {
            return false;
        }
        g.stop.store(true, Ordering::SeqCst);
        return true;
    }
    match current(hub).await {
        Some(place) if place.run == id => {
            end_marked(hub, "stopped", "stopped while it was not running").await;
            true
        }
        _ => false,
    }
}

/// Ends the run kept in kv that is not going: its record says why, and kv forgets it.
async fn end_marked(hub: &Arc<Hub>, state: &str, why: &str) {
    if let Some(place) = current(hub).await
        && let Some(mut run) = results::load(&hub.ctx(), &place.run)
        && run.state == "running"
    {
        run.state = state.into();
        run.why = Some(why.into());
        run.ended = Some(now_ms());
        results::save(&hub.ctx(), &run).warn_on_err("bench: writing the run's results");
    }
    forget(hub).await;
}

/// Where the run going is, as "case edit-check (2 of 5), repeat 1/3"; `None` when none is.
pub fn progress(hub: &Arc<Hub>) -> Option<(String, String)> {
    going(hub).map(|g| (g.id.clone(), g.at.locked().clone()))
}

/// At start-up: a run kept in kv carries on with the repeat it was in (the same Hub version), or
/// ends as "Hub updated mid-run" with the repeats it finished.
pub async fn resume(hub: Arc<Hub>) {
    if let Err(e) = resume_inner(&hub).await {
        tracing::warn!("bench: carrying on after a restart: {e:#}");
        if going(&hub).is_none() {
            end_marked(&hub, "failed", &format!("it could not carry on after Hub restarted: {e:#}")).await;
        }
    }
}

async fn resume_inner(hub: &Arc<Hub>) -> Result<()> {
    let Some(place) = current(hub).await else { return Ok(()) };
    let ctx = hub.ctx();
    let Some(mut run) = results::load(&ctx, &place.run) else {
        forget(hub).await;
        bail!("run {} is gone from disk", place.run);
    };
    // The repeat the restart cut short.
    let mut cut = BenchRow { case: place.case.clone(), repeat: place.repeat, thread: place.thread.clone(), state: "interrupted".into(), ..Default::default() };
    cut.note = Some("Hub restarted during this repeat; it does not count".into());
    run.rows.push(cut);
    if let Some(thread) = &place.thread {
        archive(hub, thread).await;
    }
    let version = hub.update_status().version;
    if version != place.version {
        run.state = "hub_updated".into();
        run.why = Some(format!("Hub updated mid-run ({} to {version}); the repeats before it are kept", place.version));
        run.ended = Some(now_ms());
        results::save(&ctx, &run)?;
        forget(hub).await;
        return Ok(());
    }
    let suite = super::parse(&place.suite_text)?;
    let setting = model_setting(hub, &run.engine, &run.model_asked).await?;
    let mut suite = suite;
    suite.engine = run.engine.clone();
    suite.repeats = run.repeats;
    suite.cases.retain(|c| run.cases.contains(&c.id));
    results::save(&ctx, &run)?;
    let going = register(hub, &run.id)?;
    let plan = Plan { suite, text: place.suite_text.clone(), setting };
    let from = Some((place.case.clone(), place.repeat));
    tokio::spawn(go(hub.clone(), run, plan, going, from));
    Ok(())
}

/// The run itself: every case's repeats in order, from `from` (case, repeat) when it resumes.
async fn go(hub: Arc<Hub>, mut run: BenchRun, plan: Plan, going: Arc<Going>, from: Option<(String, u32)>) {
    let ctx = hub.ctx();
    let total = plan.suite.cases.len();
    let mut skipping = from.is_some();
    'cases: for (i, case) in plan.suite.cases.iter().enumerate() {
        for n in 1..=plan.suite.repeats {
            if skipping {
                if from.as_ref().is_some_and(|(c, r)| *c == case.id && *r == n) {
                    skipping = false;
                } else {
                    continue;
                }
            }
            if let Some((state, why)) = stop_now(&hub, &run, &plan.suite, &going).await {
                run.state = state.into();
                run.why = Some(why);
                break 'cases;
            }
            *going.at.locked() = format!("case {} ({} of {total}), repeat {n}/{}", case.id, i + 1, plan.suite.repeats);
            let place = Current {
                run: run.id.clone(),
                suite_text: plan.text.clone(),
                suite_hash: run.suite_hash.clone(),
                case: case.id.clone(),
                repeat: n,
                version: run.hub_version.clone(),
                thread: None,
            };
            remember(&hub, &place).await.warn_on_err("bench: keeping the run's place");
            let row = repeat(&hub, &run, &plan, case, n, place).await;
            if row.invalid.is_none()
                && let Some(model) = row.measured["model"].as_str()
            {
                run.model_answered = Some(model.to_owned());
            }
            let spent = row.invalid.as_deref() == Some(super::measure::PLAN_LIMIT);
            let at = format!("{} {n}/{}", case.id, plan.suite.repeats);
            run.rows.push(row);
            results::save(&ctx, &run).warn_on_err("bench: writing the run's results");
            // Going on would only spend retries against a plan that is out.
            if spent {
                run.state = "plan_limit".into();
                run.why = Some(format!("{} in {at}; that repeat is not counted and the run stopped there", super::measure::PLAN_LIMIT));
                break 'cases;
            }
        }
    }
    if run.state == "running" {
        run.state = "done".into();
    }
    run.ended = Some(now_ms());
    // Ended and no longer going are one fact. kv forgets the run first (still registered, so no
    // start gets past `going`), then the ended file is written and the run unregistered under the
    // one lock `going` takes: whoever reads the ended file finds it neither going nor stoppable,
    // and a new run may start.
    forget(&hub).await;
    {
        let mut all = GOING.locked();
        results::save(&ctx, &run).warn_on_err("bench: writing the run's results");
        all.remove(&hub.config.data_dir);
    }
    // The run's own folder goes when every repeat passed; a failed repeat's folder keeps it.
    std::fs::remove_dir(std::env::temp_dir().join(format!("hub-bench-{}", run.id))).ignore("a kept failed repeat's folder is still in it");
}

/// Why the run should stop before its next repeat, if it should: asked to, the cost cap, the
/// plan nearly spent, or another Hub version than it began on.
async fn stop_now(hub: &Arc<Hub>, run: &BenchRun, suite: &Suite, going: &Going) -> Option<(&'static str, String)> {
    if going.stop.load(Ordering::SeqCst) {
        return Some(("stopped", "stopped on request; the repeats before are kept".into()));
    }
    if hub.halted() {
        return Some(("stopped", "every agent was paused".into()));
    }
    if let Some(why) = over_cap(run, suite.cost_cap_usd) {
        return Some(("cost_cap", why));
    }
    if let Some(why) = plan_too_full(hub, &run.engine, PLAN_STOP).await {
        return Some(("plan_limit", why));
    }
    let version = hub.update_status().version;
    if version != run.hub_version {
        return Some(("hub_updated", format!("Hub updated mid-run ({} to {version}); the repeats before it are kept", run.hub_version)));
    }
    None
}

/// Why what the run has spent so far ends it, when it does: the engine's own figures, summed over
/// the repeats, at or past the suite's cap.
fn over_cap(run: &BenchRun, cap: f64) -> Option<String> {
    let spent = results::cost(run)?;
    (spent >= cap).then(|| format!("it has cost ${spent:.2} and the suite's cap is ${cap:.2}; the repeats before are kept"))
}

/// One repeat of one case, start to end. Never fails: what went wrong is in the row.
async fn repeat(hub: &Arc<Hub>, run: &BenchRun, plan: &Plan, case: &Case, n: u32, mut place: Current) -> BenchRow {
    let mut row = BenchRow { case: case.id.clone(), repeat: n, state: "not_started".into(), ..Default::default() };
    // A case this machine cannot run says nothing about Hub: invalid, not failed, and no tokens.
    if let Some(why) = unmet(hub, case).await {
        row.note = Some(format!("not run: {why}"));
        row.invalid = Some(why);
        return row;
    }
    // A word of its own, so a repeat run again after a restart never meets the folder it left.
    let own = uuid::Uuid::new_v4().simple().to_string();
    let folder = std::env::temp_dir().join(format!("hub-bench-{}", run.id)).join(format!("{}-{n}-{}", case.id, &own[..6]));
    row.folder = Some(folder.display().to_string());
    if let Err(e) = lay_down(case, &folder) {
        row.note = Some(format!("could not lay the case down: {e:#}"));
        return row;
    }
    let thread = match new_thread(hub, run, plan, case, n, &folder).await {
        Ok(t) => t,
        Err(e) => {
            row.note = Some(format!("could not start its thread: {e:#}"));
            return row;
        }
    };
    row.thread = Some(thread.clone());
    place.thread = Some(thread.clone());
    remember(hub, &place).await.warn_on_err("bench: keeping the run's place");

    let mut sent = Vec::new();
    let mut state = "done";
    for (i, prompt) in case.prompts.iter().enumerate() {
        let attached = if i == 0 { super::suite::attachments(case) } else { Vec::new() };
        if let Err(e) = hub.send(&thread, prompt.clone(), attached, None).await {
            row.note = Some(format!("could not send a prompt: {e:#}"));
            state = "not_started";
            break;
        }
        let Ok(Some(asked)) = hub.store.last_event(&thread, EventKind::UserMessage).await.map(|e| e.map(|e| e.seq)) else { break };
        sent.push(asked);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(plan.suite.timeout_s);
        match settle(hub, &thread, asked, deadline).await {
            Settled::Done => {}
            Settled::NeedsUser => {
                state = "needs_user";
                break;
            }
            Settled::TimedOut => {
                state = "timed_out";
                hub.cancel(&thread).await.warn_on_err("bench: stopping a repeat that ran out of time");
                break;
            }
        }
    }
    row.state = state.into();
    let events = all_events(hub, &thread).await;
    let waiting = hub.store.thread(&thread).await.ok().flatten().is_some_and(|t| t.status == ThreadStatus::WaitingOnYou);
    let mut checks: Vec<BenchCheck> = check(case, &events, &folder, &sent).into_iter().map(|c| BenchCheck { check: c.check, ok: c.ok, why: c.why }).collect();
    // The log cannot say the thread waits on the person; its status can (`check.rs`).
    if waiting {
        for c in checks.iter_mut().filter(|c| c.check == "settled") {
            c.ok = false;
            c.why = Some("the thread waits on you".into());
        }
    }
    let phrases = hub.config.traits(&run.engine).limit_phrases.unwrap_or_default();
    // What the side jobs spent is the repeat's too, so the cost cap counts it.
    let mut jobs = Vec::new();
    for child in side_jobs(&events, false) {
        jobs.push(super::measure::spent(&all_events(hub, &child).await));
    }
    let measured = measure(&events, &sent, Some(&run.model_asked), &phrases).with_side_jobs(&jobs);
    // A bench thread is not carried on when the plan resets: nobody is waiting for it.
    if measured.invalid.as_deref() == Some(super::measure::PLAN_LIMIT) {
        hub.stop_waiting(&thread, "hub").await.map(drop).warn_on_err("bench: ending the wait for the plan to reset");
    }
    row.turn_usage = crate::timings::turns(&events).into_iter().map(|t| t.usage).collect();
    row.invalid = measured.invalid.clone();
    let version = hub.update_status().version;
    if row.invalid.is_none() && version != run.hub_version {
        row.invalid = Some(format!("Hub's version changed during the repeat ({} to {version})", run.hub_version));
    }
    row.pass = state == "done" && checks.iter().all(|c| c.ok);
    row.checks = checks;
    row.measured = serde_json::to_value(&measured).unwrap_or_default();
    // A side job is a thread of its own: it ends and is put away with the repeat that started it.
    for child in side_jobs(&events, true) {
        hub.cancel(&child).await.warn_on_err("bench: stopping a side job of a finished repeat");
    }
    for child in side_jobs(&events, false) {
        archive(hub, &child).await;
    }
    archive(hub, &thread).await;
    if row.pass {
        remove_folder(&folder).await;
    }
    row
}

/// Removes a passed repeat's folder. The engine just stopped may still hold it as its working
/// folder for a moment (Windows refuses to remove it then), so it tries again for a few seconds.
async fn remove_folder(folder: &std::path::Path) {
    for _ in 0..50 {
        crate::locked::clean_dir(folder);
        if !folder.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn new_thread(hub: &Arc<Hub>, run: &BenchRun, plan: &Plan, case: &Case, n: u32, folder: &std::path::Path) -> Result<String> {
    let (id, value) = &plan.setting;
    let new = NewThread {
        cwd: folder.display().to_string(),
        title: Some(format!("Bench · {}/{} · {n}/{}", run.suite, case.id, plan.suite.repeats)),
        engine: Some(run.engine.clone()),
        message: None,
        attachments: Vec::new(),
        settings: [(id.clone(), json!(value))].into_iter().collect(),
        original: None,
        client_id: None,
        attempts: Vec::new(),
        warm: None,
        design: false,
        // Hub answers every question itself, by the bench's stricter rule, and picks no mode.
        unattended: Some(crate::model::Unattended { by: crate::permit::BENCH.into(), prefer: Default::default() }),
        reach: None,
        cloud: false,
    };
    let owner = Owner::new(OwnerKind::Bench, "Bench", Some(run.id.clone()));
    Ok(hub.create_thread_as(new, None, Some(owner)).await?.id)
}

enum Settled {
    Done,
    NeedsUser,
    TimedOut,
}

/// Said when the case's `needs = ["handoff_tool"]` is not met.
pub const NO_HANDOFF_TOOL: &str = "needs one of your tools that takes handoffs (a hub.toml with [handoff]); this machine has none";

/// Why this machine cannot run `case` (its `needs`), if it cannot. Reads files only.
async fn unmet(hub: &Arc<Hub>, case: &Case) -> Option<String> {
    for need in &case.needs {
        if need == "handoff_tool" && !crate::own_tools::takes_handoffs(&hub.ctx()).await {
            return Some(NO_HANDOFF_TOOL.into());
        }
    }
    None
}

/// The side jobs a thread started, by the `child` events in its log: every one, or with
/// `working` only those whose last word is still "working" (its result has not come back).
fn side_jobs(events: &[crate::model::Event], working: bool) -> Vec<String> {
    let mut last: Vec<(String, String)> = Vec::new();
    for child in events.iter().filter_map(crate::model::Event::data_as::<crate::model::ChildData>) {
        match last.iter_mut().find(|(id, _)| *id == child.id) {
            Some(known) => known.1 = child.state,
            None => last.push((child.id, child.state)),
        }
    }
    last.into_iter().filter(|(_, state)| !working || state == "working").map(|(id, _)| id).collect()
}

/// Waits until message `asked` is answered and the thread has gone quiet, following Hub's own
/// follow-up messages (proof, docs, a side job's result) to their answers too, until `deadline`.
/// A side job still working counts as working: its result comes back as a message.
async fn settle(hub: &Arc<Hub>, thread: &str, mut asked: i64, deadline: tokio::time::Instant) -> Settled {
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Settled::TimedOut;
        }
        let answer = crate::thread_wait::wait(hub, thread, asked, left).await;
        match answer.as_ref().ok().and_then(|a| a["state"].as_str()) {
            Some("still_working") => return Settled::TimedOut,
            Some("needs_user") => return Settled::NeedsUser,
            _ => {}
        }
        // Then quiet: nothing logged for a while, not working, and no newer message from Hub.
        loop {
            let newer = hub.store.last_event(thread, EventKind::UserMessage).await.ok().flatten().map(|e| e.seq).filter(|s| *s > asked);
            if let Some(seq) = newer {
                asked = seq;
                break;
            }
            let status = hub.store.thread(thread).await.ok().flatten().map(|t| t.status);
            if status == Some(ThreadStatus::WaitingOnYou) {
                return Settled::NeedsUser;
            }
            let last = hub.store.last_events(thread, 1, &[]).await.ok().and_then(|e| e.last().map(|e| e.ts)).unwrap_or(0);
            let quiet = now_ms() - last >= QUIET.as_millis() as i64;
            let children = hub.store.events_of(thread, 0, &[EventKind::Child]).await.unwrap_or_default();
            let working = status == Some(ThreadStatus::Running) || !side_jobs(&children, true).is_empty();
            if quiet && !working {
                return Settled::Done;
            }
            if tokio::time::Instant::now() >= deadline {
                return if working { Settled::TimedOut } else { Settled::Done };
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

/// A thread's whole log, read in pieces.
async fn all_events(hub: &Arc<Hub>, thread: &str) -> Vec<crate::model::Event> {
    const PIECE: i64 = 5_000;
    let mut out: Vec<crate::model::Event> = Vec::new();
    loop {
        let after = out.last().map_or(0, |e| e.seq);
        let Ok(piece) = hub.store.events(thread, after, PIECE).await else { break };
        let full = piece.len() as i64 == PIECE;
        out.extend(piece);
        if !full {
            break;
        }
    }
    out
}

async fn archive(hub: &Arc<Hub>, thread: &str) {
    let patch = ThreadPatch { archived: Some(true), ..Default::default() };
    hub.patch_thread(thread, patch).await.map(drop).warn_on_err("bench: archiving a finished repeat's thread");
}

/// The model setting for `model` on `engine`: "fast" is the engine's cheapest (`EngineModels.fast`).
async fn model_setting(hub: &Arc<Hub>, engine: &str, model: &str) -> Result<(String, String)> {
    let name = if model == FAST {
        let table = hub.model_table(false).await?;
        table.engines.into_iter().find(|e| e.engine == engine).and_then(|e| e.fast).with_context(|| {
            format!("Hub does not know {engine}'s cheapest model yet: run one ordinary thread on {engine} first, or name a model")
        })?
    } else {
        model.to_owned()
    };
    crate::engine_models::pick(&hub.store, engine, &name)
        .await
        .with_context(|| format!("the bench cannot pick the model {name:?} on {engine}; run one ordinary thread on it first so Hub learns its models"))
}

/// Why the plan has no room for a bench, if it has none: its fullest limit (Hub reads only the
/// plan of one engine, `Hub::reads_plan_of`) at `limit` percent or more, or within
/// [`SWITCH_MARGIN`] of the quota guard's own line, the same figures `quota_switch` reads.
async fn plan_too_full(hub: &Arc<Hub>, engine: &str, limit: f32) -> Option<String> {
    if !hub.reads_plan_of(engine) {
        return None;
    }
    let (plan, _) = hub.usage.plan()?;
    let fullest = plan.highest()?.clone();
    let line = hub.guard_prefs().await.ok().and_then(|p| p.switch_at).map(|s| s - SWITCH_MARGIN);
    let at = line.map_or(limit, |l| l.min(limit));
    (fullest.percent >= at).then(|| format!("the plan limit {:?} is at {:.0} % and a bench run needs it under {at:.0} %", fullest.label, fullest.percent))
}

/// The bench's own permission answer must be in force: it refuses a write outside the run folder.
fn guard_is_on() -> Result<()> {
    let folder = std::env::temp_dir().join("hub-bench-guard");
    let outside = std::env::temp_dir().join("hub-bench-guard-outside.txt");
    let call = json!({ "kind": "edit", "title": "Write", "rawInput": { "file_path": outside.display().to_string() } });
    match crate::permit::unattended_by(crate::permit::BENCH, &Default::default(), &call, &folder) {
        crate::permit::Verdict::Refuse(_) => Ok(()),
        crate::permit::Verdict::Allow(_) => bail!("the bench's permission guard is not in force (a write outside its folder would be allowed); the bench does not start"),
    }
}

fn going(hub: &Arc<Hub>) -> Option<Arc<Going>> {
    GOING.locked().get(&hub.config.data_dir).cloned()
}

fn register(hub: &Arc<Hub>, id: &str) -> Result<Arc<Going>> {
    let mut all = GOING.locked();
    if let Some(g) = all.get(&hub.config.data_dir) {
        bail!("a bench run is going already ({})", g.id);
    }
    let going = Arc::new(Going { id: id.to_owned(), stop: AtomicBool::new(false), at: Mutex::new("starting".into()) });
    all.insert(hub.config.data_dir.clone(), going.clone());
    Ok(going)
}

async fn current(hub: &Arc<Hub>) -> Option<Current> {
    serde_json::from_str(&hub.store.get_value(CURRENT).await.ok()??).ok()
}

async fn remember(hub: &Arc<Hub>, place: &Current) -> Result<()> {
    hub.store.set_value(CURRENT, &serde_json::to_string(place)?).await
}

async fn forget(hub: &Arc<Hub>) {
    hub.store.take_value(CURRENT).await.map(drop).warn_on_err("bench: clearing the run's place");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cost_cap_ends_a_run_once_the_repeats_reach_it() {
        let row = |cost: Option<f64>| BenchRow { measured: json!({ "cost_usd": cost }), ..Default::default() };
        let mut run = BenchRun { rows: vec![row(Some(2.0)), row(None)], ..Default::default() };
        assert_eq!(over_cap(&run, 5.0), None);
        run.rows.push(row(Some(3.0)));
        assert!(over_cap(&run, 5.0).is_some_and(|why| why.contains("$5.00") && why.contains("$5.00")));
        // A side job's spend is in its repeat's cost, so the cap sees it.
        let parent = crate::bench::Measured { cost_usd: Some(1.0), ..Default::default() };
        let job = crate::bench::Measured { cost_usd: Some(4.5), ..Default::default() };
        let with_job = BenchRow { measured: serde_json::to_value(parent.with_side_jobs(&[job])).unwrap_or_default(), ..Default::default() };
        assert!(over_cap(&BenchRun { rows: vec![with_job], ..Default::default() }, 5.0).is_some());
        // An engine that never says what it cost never trips the cap.
        assert_eq!(over_cap(&BenchRun { rows: vec![row(None)], ..Default::default() }, 0.01), None);
    }

    #[test]
    fn a_side_job_is_working_until_its_last_word_says_otherwise() {
        let child = |seq, id: &str, state: &str| crate::model::Event {
            thread_id: "t".into(),
            seq,
            ts: seq,
            kind: EventKind::Child,
            data: json!({ "id": id, "title": id, "state": state }),
        };
        let log = [child(1, "a", "working"), child(2, "b", "working"), child(3, "a", "done")];
        assert_eq!(side_jobs(&log, true), ["b"]);
        assert_eq!(side_jobs(&log, false), ["a", "b"]);
        assert!(side_jobs(&log[..0], false).is_empty());
    }

    #[test]
    fn the_guard_is_in_force() {
        assert!(guard_is_on().is_ok());
    }
}
