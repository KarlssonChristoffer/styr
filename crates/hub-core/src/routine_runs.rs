//! Running routines (`routines.rs`): the scheduler that starts the ones that are due, and starting
//! one run, which makes the run's thread and sends it the brief. Those need the running Hub; the
//! list and how a run ends are `routines.rs`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::activity::date_of;
use crate::hub::Hub;
use crate::locked::ResultExt;
use crate::model::{NewThread, Routine, RoutineRun, RunState, StreamMsg, ThreadStatus, now_ms};
use crate::routines;

/// How much of the last report the next run gets.
const LAST_REPORT: usize = 4_000;

/// Starts the routines that are due, looking every minute.
pub async fn scheduler(hub: Arc<Hub>) {
    // Let the core settle after a start before using an engine.
    tokio::time::sleep(Duration::from_secs(90)).await;
    loop {
        if let Err(err) = run_due(&hub).await {
            tracing::warn!("routines: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn run_due(hub: &Arc<Hub>) -> Result<()> {
    // Paused from the panic button: due routines run once agents are resumed.
    if hub.halted() {
        return Ok(());
    }
    let tz = routines::tz(&hub.ctx()).await;
    let now = now_ms();
    for routine in routines::load(&hub.ctx()).await? {
        if !routine.enabled {
            continue;
        }
        let retry = routines::retry_due(&routine, now);
        if !retry && routines::next_run(&routine, tz).is_none_or(|at| at > now) {
            continue;
        }
        // One run at a time: a run still busy (or asking something) holds the next one back.
        if let Some(thread) = routine.runs.first().filter(|r| r.state == RunState::Working).and_then(|r| r.thread.clone())
            && let Some(t) = hub.store.thread(&thread).await?
            && matches!(t.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou)
        {
            continue;
        }
        match start(hub, &routine.id, if retry { "retry" } else { "schedule" }).await {
            Ok(_) => tracing::info!("routine {:?} started", routine.name),
            Err(err) => tracing::warn!("routine {:?}: {err:#}", routine.name),
        }
    }
    Ok(())
}

/// Starts a run of routine `id` now and returns its thread. `by` is "schedule" or "you".
pub async fn start(hub: &Arc<Hub>, id: &str, by: &str) -> Result<String> {
    if hub.halted() {
        bail!("every agent is paused; resume them first");
    }
    let routine = routines::load(&hub.ctx()).await?.into_iter().find(|r| r.id == id).context("no such routine")?;
    let tz = routines::tz(&hub.ctx()).await;
    let now = now_ms();
    let title = format!("{} · {}", routine.name, date_of(now, tz));
    let new = NewThread {
        cwd: routine.folder.clone(),
        title: Some(title),
        engine: routine.engine.clone(),
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
    };
    let owner = crate::model::Owner::new(crate::model::OwnerKind::Routine, routine.name.clone(), Some(routine.id.clone()));
    let made = hub.create_thread_as(new, None, Some(owner)).await;
    let run = RoutineRun {
        thread: made.as_ref().ok().map(|t| t.id.clone()),
        at: now,
        by: by.to_owned(),
        state: if made.is_ok() { RunState::Working } else { RunState::Failed },
        report: made.as_ref().err().map(|e| format!("Could not start: {e:#}")).unwrap_or_default(),
        ended_at: made.is_err().then_some(now),
    };
    let last = routine.runs.iter().find(|r| r.state == RunState::Done && !r.report.trim().is_empty()).map(|r| r.report.clone());
    routines::change(&hub.ctx(), id, |r| {
        r.runs.insert(0, run);
        r.runs.truncate(routines::KEEP_RUNS);
        Ok(())
    })
    .await?;
    let thread = made?;
    let message = brief(&routine, last.as_deref(), by);
    let mark = json!({ "id": routine.id, "name": routine.name });
    if let Err(e) = hub.send_from(&thread.id, message, Vec::new(), None, Some(("routine", mark)), None).await {
        let error = format!("{e:#}");
        routines::end(&hub.ctx(), &thread.id, RunState::Failed, format!("Could not start: {error}"), |t| hub.publish(StreamMsg::Thread { thread: t }))
            .await
            .warn_on_err("marking a routine run that could not start");
        bail!(error);
    }
    Ok(thread.id)
}

/// What the run's agent is asked: the note on how a routine works, the routine's own prompt, and
/// what it reported last time.
fn brief(routine: &Routine, last: Option<&str>, by: &str) -> String {
    let when = if by == "you" { "started by the user" } else { "run on its schedule" };
    let mut text = format!(
        "(Note from Hub: this is the routine \"{name}\", {when}. A routine only reports. Don't change, \
commit, push or merge anything and don't open pull requests; Hub refuses those while this runs. Reading, searching \
and commands that only look (tests, linters, git log) are fine and never ask; anything else is refused at once, so \
put what you wanted to run and why in the report instead. End with a short report: the findings that matter most \
first, numbered, each with where it is (file:line), why it matters and the fix you suggest, so the user can answer \
\"fix 2\". Leave out style nits. If nothing is worth the user's time, say so in one line. The user reads the report \
later, so don't ask questions; note what you could not check instead.)\n\n{prompt}",
        name = routine.name,
        prompt = routine.prompt.trim(),
    );
    if let Some(last) = last {
        text.push_str(&format!(
            "\n\nWhat this routine reported last time, so you don't repeat it (mention an item again only if it is \
still there and serious, and look somewhere new where the task allows):\n\n<last-report>\n{}\n</last-report>",
            crate::text::clip_note(last, LAST_REPORT, routines::CUT_NOTE)
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routines::templates;

    #[test]
    fn the_brief_carries_the_last_report() {
        let t = &templates()[0];
        let r = Routine {
            id: "r".into(),
            name: t.name.clone(),
            folder: ".".into(),
            engine: None,
            prompt: t.prompt.clone(),
            schedule: t.schedule.clone(),
            enabled: true,
            template: Some(t.id.clone()),
            since: 0,
            runs: Vec::new(),
            next_at: None,
        };
        let text = brief(&r, Some("1. src/a.rs:3 unwrap on input"), "schedule");
        assert!(text.contains("only reports"));
        assert!(text.contains("<last-report>\n1. src/a.rs:3"));
        assert!(!brief(&r, None, "you").contains("last-report"));
    }
}
