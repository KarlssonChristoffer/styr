//! Routines: work Hub starts by itself on a schedule, such as a daily look through the code for
//! bugs and weak spots. A run is an ordinary thread on the subscription engine, marked
//! `routine: { id, name }`, that only reports: while its first turn runs, Hub refuses edits and
//! anything that commits, pushes, opens or merges a pull request, allows reading and commands that
//! only look, and refuses the rest instead of asking, so a run never waits (`permit::routine`). The
//! report comes back like any finished thread; answering in it ("fix 2") makes it a normal thread.
//!
//! Routines are kept as one list under `routines`. A run that was missed while the core was down
//! happens once when it is back, not once per missed slot. A run that failed on its own is tried
//! again twice, after 10 and then 30 minutes, and then waits for its next slot.
//!
//! This file keeps the list and how a run ends (`&Ctx`); the scheduler and starting a run need the
//! running Hub and are `routine_runs.rs`.

use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use tokio::sync::Mutex;

use crate::ctx::Ctx;
use crate::model::{
    Event, EventKind, Every, Routine, RoutineDraft, RoutineRun, RoutineTemplate, Routines, RunState, Schedule, Thread, now_ms,
};

const KEY: &str = "routines";
const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
/// Runs kept per routine.
pub(crate) const KEEP_RUNS: usize = 20;
/// Pauses before the first and the second retry of a failed run. That is also the cap: after the
/// last one a failure waits for the next slot like before.
const RETRY_PAUSES: [i64; 2] = [10 * MINUTE_MS, 30 * MINUTE_MS];
/// The report of a run the user (or a pause) stopped, which is never run again by itself.
const STOPPED: &str = "Stopped before it was done.";

/// One writer at a time for the list, since a run ending and the user editing can meet.
static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Whether the latest run failed on its own (not stopped by the user) and is due another try: at
/// most two retries after a scheduled run, each after a pause. A run the user started ("you") is
/// never retried, and a run that worked ends the streak.
pub(crate) fn retry_due(r: &Routine, now: i64) -> bool {
    let streak: Vec<&RoutineRun> = r.runs.iter().take_while(|run| run.state == RunState::Failed).collect();
    let (Some(last), Some(first)) = (streak.first(), streak.last()) else { return false };
    let retries = streak.iter().filter(|run| run.by == "retry").count();
    first.by == "schedule"
        && streak.iter().all(|run| run.by != "you" && run.report != STOPPED)
        && RETRY_PAUSES.get(retries).is_some_and(|pause| now >= last.ended_at.unwrap_or(last.at) + pause)
}

pub(crate) async fn tz(ctx: &Ctx<'_>) -> i32 {
    ctx.store.get_value(crate::devlog::TZ).await.ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0)
}

pub(crate) async fn load(ctx: &Ctx<'_>) -> Result<Vec<Routine>> {
    Ok(match ctx.store.get_value(KEY).await? {
        Some(json) => serde_json::from_str(&json).context("reading the routines")?,
        None => Vec::new(),
    })
}

async fn save(ctx: &Ctx<'_>, routines: &[Routine]) -> Result<()> {
    ctx.store.set_value(KEY, &serde_json::to_string(routines)?).await
}

/// Changes the routine `id` under the lock and returns it.
pub(crate) async fn change(ctx: &Ctx<'_>, id: &str, f: impl FnOnce(&mut Routine) -> Result<()>) -> Result<Routine> {
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    let routine = all.iter_mut().find(|r| r.id == id).context("no such routine")?;
    f(routine)?;
    let out = routine.clone();
    save(ctx, &all).await?;
    Ok(out)
}

/// Every routine with when it runs next, and the starter routines.
pub async fn list(ctx: &Ctx<'_>) -> Result<Routines> {
    let tz = tz(ctx).await;
    let mut routines = load(ctx).await?;
    for r in &mut routines {
        r.next_at = next_run(r, tz);
    }
    Ok(Routines { routines, templates: templates() })
}

pub async fn create(ctx: &Ctx<'_>, draft: RoutineDraft) -> Result<Routine> {
    let draft = checked(ctx, draft)?;
    let routine = Routine {
        id: uuid::Uuid::now_v7().to_string(),
        name: draft.name,
        folder: draft.folder,
        engine: draft.engine,
        prompt: draft.prompt,
        schedule: draft.schedule,
        enabled: draft.enabled,
        template: draft.template,
        since: now_ms(),
        runs: Vec::new(),
        next_at: None,
    };
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    all.push(routine.clone());
    save(ctx, &all).await?;
    Ok(routine)
}

pub async fn update(ctx: &Ctx<'_>, id: &str, draft: RoutineDraft) -> Result<Routine> {
    let draft = checked(ctx, draft)?;
    change(ctx, id, |r| {
        // A new schedule, or turning it back on, counts from now: no run for slots it was not set for.
        if r.schedule != draft.schedule || (!r.enabled && draft.enabled) {
            r.since = now_ms();
        }
        r.name = draft.name;
        r.folder = draft.folder;
        r.engine = draft.engine;
        r.prompt = draft.prompt;
        r.schedule = draft.schedule;
        r.enabled = draft.enabled;
        Ok(())
    })
    .await
}

/// Switch a routine on or off and nothing else: what the phone may do to one (`phone.watch`).
pub async fn set_enabled(ctx: &Ctx<'_>, id: &str, enabled: bool) -> Result<Routine> {
    let r = load(ctx).await?.into_iter().find(|r| r.id == id).with_context(|| format!("no routine {id}"))?;
    update(
        ctx,
        id,
        RoutineDraft { name: r.name, folder: r.folder, engine: r.engine, prompt: r.prompt, schedule: r.schedule, enabled, template: r.template },
    )
    .await
}

pub async fn delete(ctx: &Ctx<'_>, id: &str) -> Result<()> {
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    all.retain(|r| r.id != id);
    save(ctx, &all).await
}

fn checked(ctx: &Ctx<'_>, mut draft: RoutineDraft) -> Result<RoutineDraft> {
    draft.name = draft.name.trim().chars().take(80).collect();
    if draft.name.is_empty() {
        bail!("give the routine a name");
    }
    if draft.prompt.trim().is_empty() {
        bail!("say what the routine should do");
    }
    if !std::path::Path::new(&draft.folder).is_dir() {
        bail!("folder {:?} does not exist on this machine", draft.folder);
    }
    draft.engine = draft.engine.filter(|e| !e.trim().is_empty());
    if let Some(engine) = &draft.engine
        && !ctx.config.engines.contains_key(engine)
    {
        bail!("no engine {engine:?} here");
    }
    let s = &mut draft.schedule;
    s.at = s.at.min(24 * 60 - 1);
    s.weekday = s.weekday.min(6);
    if s.every == Every::Hours {
        s.hours = s.hours.clamp(1, 24 * 7);
    }
    Ok(draft)
}

/// After a turn ends or fails: a routine's run that was working is done, with the reply as its
/// report. Turns after that, when the user answers in the thread, are the user's own.
pub(crate) async fn finished(ctx: &Ctx<'_>, event: &Event, publish: impl Fn(Thread)) -> Result<()> {
    if working_run(ctx, &event.thread_id).await?.is_none() {
        return Ok(());
    }
    let (state, report) = match event.kind {
        EventKind::TurnEnd if event.ended_well() => {
            let events = ctx.store.events_after_last(&event.thread_id, EventKind::UserMessage, &[]).await?;
            (RunState::Done, crate::model::outcome(&events).reply)
        }
        EventKind::TurnEnd => (RunState::Failed, STOPPED.to_owned()),
        _ => (RunState::Failed, event.failure().unwrap_or("The engine failed.").to_owned()),
    };
    end(ctx, &event.thread_id, state, report, publish).await
}

pub(crate) async fn end(ctx: &Ctx<'_>, thread: &str, state: RunState, report: String, publish: impl Fn(Thread)) -> Result<()> {
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    for r in &mut all {
        for run in &mut r.runs {
            if run.thread.as_deref() == Some(thread) && run.state == RunState::Working {
                run.state = state;
                run.report = crate::text::clip_note(report.trim(), 20_000, CUT_NOTE);
                run.ended_at = Some(now_ms());
            }
        }
    }
    save(ctx, &all).await?;
    // The report is on the Routines page now, so the run need not wait in the inbox.
    if state == RunState::Done {
        settle_run(ctx, thread, publish).await?;
    }
    Ok(())
}

/// Puts a routine's finished run away: its report is on the Routines page, so the thread need not
/// wait in the inbox. One that needs the user stays.
async fn settle_run(ctx: &Ctx<'_>, thread_id: &str, publish: impl Fn(Thread)) -> Result<()> {
    let now = crate::model::now_ms();
    let Some(thread) = ctx.store.thread(thread_id).await? else {
        return Ok(());
    };
    if thread.settled_at.is_some()
        || matches!(
            thread.status,
            crate::model::ThreadStatus::WaitingOnYou | crate::model::ThreadStatus::Error
        )
    {
        return Ok(());
    }
    let thread = ctx
        .store
        .mark_thread(thread_id, move |t| t.settled_at = Some(now))
        .await?;
    publish(thread);
    Ok(())
}

/// The routine whose run in `thread` is still on its first turn, which is when it only reports.
pub(crate) async fn working_run(ctx: &Ctx<'_>, thread: &str) -> Result<Option<String>> {
    Ok(load(ctx)
        .await?
        .into_iter()
        .find(|r| r.runs.iter().any(|run| run.thread.as_deref() == Some(thread) && run.state == RunState::Working))
        .map(|r| r.id))
}

/// When routine `r` runs next, or None while it is off.
pub fn next_run(r: &Routine, tz: i32) -> Option<i64> {
    if !r.enabled {
        return None;
    }
    let after = r.runs.first().map_or(r.since, |run| run.at.max(r.since));
    Some(next_after(&r.schedule, after, tz))
}

/// The first slot of `s` strictly after `after`, in a time zone `tz` minutes east of UTC.
pub fn next_after(s: &Schedule, after: i64, tz: i32) -> i64 {
    if s.every == Every::Hours {
        return after + s.hours.max(1) as i64 * HOUR_MS;
    }
    let shift = tz as i64 * MINUTE_MS;
    let today = (after + shift).div_euclid(DAY_MS);
    (today..today + 9)
        .filter(|day| match s.every {
            // 1970-01-01 was a Thursday; Monday is 0.
            Every::Weekdays => (day + 3).rem_euclid(7) < 5,
            Every::Week => (day + 3).rem_euclid(7) == s.weekday as i64,
            _ => true,
        })
        .map(|day| day * DAY_MS + s.at as i64 * MINUTE_MS - shift)
        .find(|&at| at > after)
        .unwrap_or(after + DAY_MS)
}

pub(crate) const CUT_NOTE: &str = "[… cut, the whole report is in the run's thread …]";

fn at(hour: u32, minute: u32) -> u32 {
    hour * 60 + minute
}

/// The starter routines.
pub fn templates() -> Vec<RoutineTemplate> {
    let t = |id: &str, name: &str, about: &str, schedule: Schedule, prompt: &str| RoutineTemplate {
        id: id.into(),
        name: name.into(),
        about: about.into(),
        prompt: prompt.into(),
        schedule,
    };
    vec![
        t(
            "daily-review",
            "Daily review",
            "Reads what was committed in the last day for bugs, risks and missing tests.",
            Schedule { every: Every::Day, at: at(7, 0), ..Default::default() },
            "Review what changed in this repository in the last 24 hours: `git log --since=\"24 hours ago\" -p` on every \
branch that moved (`git branch -a --sort=-committerdate`). Look for bugs, behaviour that changed by accident, error \
cases that are not handled, risky changes without tests, and anything that contradicts the project's own rules \
(CLAUDE.md, AGENTS.md, docs). If nothing was committed, say so in one line and stop.",
        ),
        t(
            "weakness-hunt",
            "Weakness hunt",
            "Picks one part of the code each night and looks hard for security holes, crashes and edge cases.",
            Schedule { every: Every::Day, at: at(3, 0), ..Default::default() },
            "Pick one part of this codebase (a module, a service, an API surface) that is risky and that the last report \
did not cover, say which you picked and why, and go through it carefully for weaknesses: input that is trusted \
without being checked, injection and path traversal, secrets in code or logs, panics and unhandled errors, races, \
resource leaks, timeouts that are missing, and edge cases (empty, huge, unicode, concurrent, offline). Run the tests \
that cover it if they are quick. Rank by how likely and how bad.",
        ),
        t(
            "dependencies",
            "Dependencies",
            "Mondays: outdated or vulnerable packages, and anything new that Customs has not cleared.",
            Schedule { every: Every::Week, at: at(8, 0), weekday: 0, ..Default::default() },
            "Check this project's dependencies. List the manifests (Cargo.toml, package.json, pyproject.toml, \
requirements*.txt, go.mod and so on). Use the audit tools that are installed and work offline or read-only \
(`cargo audit`, `npm audit --omit=dev`, `pip-audit`, `npm outdated`, `cargo outdated`), and skip any that are \
missing rather than installing them. For each direct dependency that is new since last time or has a known \
vulnerability, look it up with Hub's `customs_search` tool and say whether Customs has cleared, held or refused it; \
declare the ones it does not know with `customs_declare`. Report vulnerabilities first, then major versions behind, \
then unused dependencies.",
        ),
        t(
            "dead-code",
            "Dead code and TODOs",
            "Fridays: code nothing uses, stale TODOs and FIXMEs, leftovers worth deleting.",
            Schedule { every: Every::Week, at: at(8, 0), weekday: 4, ..Default::default() },
            "Look for what can go: functions, types, files, endpoints, flags and settings that nothing uses (check with \
search before claiming it), commented-out code, and TODO, FIXME, HACK and XXX notes. For each note, say how old it is \
(`git blame`) and whether it is still true, done already, or worth a thread. Group the report as: safe to delete, \
needs a decision, real work left.",
        ),
        t(
            "test-gaps",
            "Test gaps",
            "Wednesdays: important code with no tests, and tests that would not catch a break.",
            Schedule { every: Every::Week, at: at(8, 0), weekday: 2, ..Default::default() },
            "Find the places where a bug would hurt most and nothing would catch it: important logic with no tests, \
error paths never exercised, tests that only check the happy path or assert nothing meaningful, and flaky or skipped \
tests. Run the test suite if it is quick and report failures first. For each gap, describe the test you would write \
in a sentence or two.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::date_of;

    const MON: i64 = 1_792_972_800_000; // 2026-10-26 00:00 UTC, a Monday

    fn day(at: u32) -> Schedule {
        Schedule { every: Every::Day, at, ..Default::default() }
    }

    #[test]
    fn monday_is_monday() {
        assert_eq!(date_of(MON, 0), "2026-10-26");
        let week = Schedule { every: Every::Week, at: 0, weekday: 0, hours: 0 };
        assert_eq!(next_after(&week, MON - 1, 0), MON);
    }

    #[test]
    fn a_daily_routine_runs_at_its_time_in_the_users_zone() {
        // 06:00 UTC on Monday, in UTC+1: 07:00 local is 06:00 UTC, which is not after 06:00.
        let six = MON + 6 * HOUR_MS;
        assert_eq!(next_after(&day(at(7, 0)), six - 1, 60), six);
        assert_eq!(next_after(&day(at(7, 0)), six, 60), six + DAY_MS);
        // Just before local midnight it is still the same local day.
        assert_eq!(next_after(&day(at(23, 30)), MON + 22 * HOUR_MS, 60), MON + 22 * HOUR_MS + 30 * MINUTE_MS);
    }

    #[test]
    fn weekdays_skip_the_weekend() {
        let friday_noon = MON + 4 * DAY_MS + 12 * HOUR_MS;
        let s = Schedule { every: Every::Weekdays, at: at(8, 0), ..Default::default() };
        assert_eq!(next_after(&s, friday_noon, 0), MON + 7 * DAY_MS + 8 * HOUR_MS);
    }

    #[test]
    fn weekly_and_hourly() {
        let wed = Schedule { every: Every::Week, at: at(8, 0), weekday: 2, hours: 0 };
        assert_eq!(next_after(&wed, MON, 0), MON + 2 * DAY_MS + 8 * HOUR_MS);
        assert_eq!(next_after(&wed, MON + 2 * DAY_MS + 9 * HOUR_MS, 0), MON + 9 * DAY_MS + 8 * HOUR_MS);
        let hours = Schedule { every: Every::Hours, hours: 6, ..Default::default() };
        assert_eq!(next_after(&hours, MON, 0), MON + 6 * HOUR_MS);
    }

    fn failed(by: &str, at: i64) -> RoutineRun {
        RoutineRun {
            thread: None,
            at,
            by: by.to_owned(),
            state: RunState::Failed,
            report: "The engine failed.".to_owned(),
            ended_at: Some(at + 1_000),
        }
    }

    fn with_runs(runs: Vec<RoutineRun>) -> Routine {
        Routine {
            id: "r".into(),
            name: "n".into(),
            folder: ".".into(),
            engine: None,
            prompt: "p".into(),
            schedule: day(at(7, 0)),
            enabled: true,
            template: None,
            since: 0,
            runs,
            next_at: None,
        }
    }

    #[test]
    fn a_failed_run_is_retried_twice_after_a_pause_and_then_left_alone() {
        let t0 = 1_000_000;
        let one = with_runs(vec![failed("schedule", t0)]);
        assert!(!retry_due(&one, t0 + 5 * MINUTE_MS), "too soon");
        assert!(retry_due(&one, t0 + 11 * MINUTE_MS));
        let two = with_runs(vec![failed("retry", t0 + 11 * MINUTE_MS), failed("schedule", t0)]);
        assert!(!retry_due(&two, t0 + 20 * MINUTE_MS));
        assert!(retry_due(&two, t0 + 45 * MINUTE_MS));
        let three = with_runs(vec![failed("retry", t0 + 45 * MINUTE_MS), failed("retry", t0 + 11 * MINUTE_MS), failed("schedule", t0)]);
        assert!(!retry_due(&three, t0 + 10 * 60 * MINUTE_MS), "the cap holds");
    }

    #[test]
    fn stopped_manual_and_working_runs_are_not_retried() {
        let t0 = 1_000_000;
        let later = t0 + 3 * 60 * MINUTE_MS;
        assert!(!retry_due(&with_runs(vec![]), later));
        assert!(!retry_due(&with_runs(vec![failed("you", t0)]), later));
        let mut stopped = failed("schedule", t0);
        stopped.report = STOPPED.to_owned();
        assert!(!retry_due(&with_runs(vec![stopped]), later));
        let mut done = failed("schedule", t0);
        done.state = RunState::Done;
        assert!(!retry_due(&with_runs(vec![done, failed("schedule", t0 - 1)]), later));
    }

    #[test]
    fn a_missed_run_happens_once() {
        let mut r = Routine {
            id: "r".into(),
            name: "n".into(),
            folder: ".".into(),
            engine: None,
            prompt: "p".into(),
            schedule: day(at(7, 0)),
            enabled: true,
            template: None,
            since: MON,
            runs: Vec::new(),
            next_at: None,
        };
        // Off for three days: due since Monday 07:00, so it runs when the core is back.
        assert_eq!(next_run(&r, 0), Some(MON + 7 * HOUR_MS));
        let back = MON + 3 * DAY_MS + 12 * HOUR_MS;
        r.runs.push(RoutineRun { thread: None, at: back, by: "schedule".into(), state: RunState::Done, report: String::new(), ended_at: None });
        assert_eq!(next_run(&r, 0), Some(MON + 4 * DAY_MS + 7 * HOUR_MS));
        r.enabled = false;
        assert_eq!(next_run(&r, 0), None);
    }
}
