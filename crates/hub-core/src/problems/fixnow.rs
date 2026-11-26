//! A thread's failure as a card in the thread: the `error` went to Problems ([`reported`]), and
//! "Fix now" ([`fix_now`]) starts a fix thread on it in the background, through the same path as
//! Vakten's fixes (`start_thread`), with what the failed thread knows. When the fix thread's first
//! turn ends ([`fix_finished`]), the card in the failed thread says so, with its last words and
//! its pull request, so the user can try again. All of it is `reported` events in the failed
//! thread, logged again under the same `about` as it moves.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Problem;
use crate::hub::Hub;
use crate::model::{Event, EventKind, ReportedData, ReportedPr, Thread};

/// Which failure a fix thread is working on, kept under `problems.fix.<fix thread>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Fixing {
    thread: String,
    about: i64,
    problem: i64,
}

fn key(fix: &str) -> String {
    format!("problems.fix.{fix}")
}

/// Notes in the failed thread that its error is problem `problem` now.
pub(crate) async fn reported(hub: &Hub, error: &Event, problem: &Problem) -> Result<()> {
    hub.emit_typed(&error.thread_id, &ReportedData { about: Some(error.seq), problem: Some(problem.id), state: "sent".into(), ..Default::default() }).await?;
    Ok(())
}

/// The latest card for error `about` in a thread's log.
fn card(events: &[Event], about: i64) -> Option<ReportedData> {
    events.iter().rev().filter_map(Event::data_as::<ReportedData>).find(|r| r.about == Some(about))
}

/// Starts a fix thread on the failure `about` (an `error` event's seq) in `thread`, or hands back
/// the one already on it.
pub async fn fix_now(hub: &Arc<Hub>, thread_id: &str, about: i64) -> Result<Thread> {
    let thread = hub
        .store
        .thread(thread_id)
        .await?
        .with_context(|| format!("no thread {thread_id}"))?;
    let events = hub.store.events_of(thread_id, 0, &[EventKind::Error, EventKind::Reported]).await?;
    let error = events
        .iter()
        .find(|e| e.seq == about && e.kind == EventKind::Error)
        .context("that is not a failure in this thread")?;
    let Some(last) = card(&events, about) else {
        bail!("this failure was not sent to Problems")
    };
    if last.state == "fixing"
        && let Some(fix) = last.fix.as_deref()
        && let Some(t) = hub.store.thread(fix).await?
    {
        return Ok(t);
    }
    let problem = last.problem.context("the card names no problem")?;
    let plan = plan(&thread, error, hub.remotes.local_name());
    let started = super::start_thread(hub, problem, "you", Some(&plan), None).await?;
    let fix = match started.thread {
        Some(t) => t,
        // Hub does not know the problem's folder: the failed thread's will do.
        None => {
            let unattended = super::get(hub, problem).await?.map(|p| super::fix_unattended(&p, "you", None));
            let t = hub.create_thread_as(
                crate::model::NewThread {
                    cwd: thread.cwd.clone(),
                    title: Some(
                        format!(
                            "Fix #{problem}: {}",
                            super::normalize(error.failure().unwrap_or("failure"))
                        )
                        .chars()
                        .take(80)
                        .collect(),
                    ),
                    engine: None,
                    message: Some(started.prompt.clone()),
                    attachments: Vec::new(),
                    settings: Default::default(),
                    original: None,
                    client_id: None,
                    attempts: Vec::new(),
                    warm: None,
                    design: false,
                    reach: None,
                    cloud: false,
                    unattended,
                },
                None,
                Some(crate::system::fixer("you", problem)),
            )
            .await?;
            super::set_thread(hub, problem, &t.id, "you").await?;
            t
        }
    };
    let fixing = Fixing {
        thread: thread.id.clone(),
        about,
        problem,
    };
    hub.store
        .set_value(&key(&fix.id), &serde_json::to_string(&fixing)?)
        .await?;
    let fixing = ReportedData { about: Some(about), problem: Some(problem), state: "fixing".into(), fix: Some(fix.id.clone()), fix_title: Some(fix.title.clone()), ..Default::default() };
    hub.emit_typed(&thread.id, &fixing).await?;
    Ok(fix)
}

/// What the fix thread is asked to do, on top of the problem's brief.
fn plan(thread: &Thread, error: &Event, machine: &str) -> String {
    let message = error.failure().unwrap_or("the engine failed");
    let detail = match error.detail() {
        Value::Null => String::new(),
        Value::String(s) => format!("\n{s}"),
        other => format!("\n{other}"),
    };
    format!(
        "\"Fix now\" was pressed in the thread \"{title}\" (id {id}, folder {cwd}) when its engine ({engine} on {machine}) failed with:\n\n\
{message}{detail}\n\n\
What happened before it is in the hits above; read that thread with Hub's read_thread tool when you need more, and look in Hub's own \
log on {machine} if the engine said nothing useful. Find why it failed and fix the cause. When the cause is outside the code (a \
sign-in, a setting, the network, a program missing on the machine), fix it there if that is safe and say exactly what you did; \
otherwise say what the user must do. End your reply with one short line saying what was wrong and whether trying again should \
work now or only after your pull request is released.",
        title = thread.title,
        id = thread.id,
        cwd = thread.cwd,
        engine = thread.engine,
    )
}

/// The last pull request link in a reply: its address and number.
pub(crate) fn pr_link(text: &str) -> Option<(String, u32)> {
    static PR: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"https://github\.com/[\w.-]+/[\w.-]+/pull/(\d+)").expect("pr pattern")
    });
    let found = PR.captures_iter(text).last()?;
    Some((found[0].to_owned(), found[1].parse().ok()?))
}

/// A fix thread's first turn ended: tells the thread it was started from.
pub(crate) async fn fix_finished(hub: &Arc<Hub>, end: &Event) -> Result<()> {
    let Some(saved) = hub.store.get_value(&key(&end.thread_id)).await? else {
        return Ok(());
    };
    let fixing: Fixing = serde_json::from_str(&saved)?;
    let (state, text) = match end.kind {
        EventKind::TurnEnd if end.ended_well() => {
            let chunks = hub.store.last_reply_chunks(&end.thread_id).await?;
            (
                "fixed",
                crate::text::tail(crate::replies::reply(&chunks, i64::MAX).trim(), 600),
            )
        }
        EventKind::TurnEnd => (
            "stopped",
            "The fix thread was stopped before it finished.".to_owned(),
        ),
        _ => (
            "failed",
            end.failure().unwrap_or("The fix thread's engine failed").to_owned(),
        ),
    };
    hub.store.take_value(&key(&end.thread_id)).await?;
    // Hub looks pull requests up every few minutes; the reply's own link counts until then.
    let pr = hub
        .thread_prs()
        .get(&end.thread_id)
        .map(|p| ReportedPr { number: p.number, url: p.url.clone(), state: p.state.clone() })
        .or_else(|| pr_link(&text).map(|(url, number)| ReportedPr { number, url, state: "open".into() }));
    let title = hub
        .store
        .thread(&end.thread_id)
        .await?
        .map(|t| t.title)
        .unwrap_or_default();
    let ended = ReportedData {
        about: Some(fixing.about),
        problem: Some(fixing.problem),
        state: state.to_owned(),
        fix: Some(end.thread_id.clone()),
        fix_title: Some(title),
        text: Some(text),
        pr: Some(pr),
    };
    hub.emit_typed(&fixing.thread, &ended).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_pull_request_in_a_reply() {
        let text =
            "Opened https://github.com/a/hub/pull/12 and then https://github.com/a/hub/pull/13).";
        assert_eq!(
            pr_link(text),
            Some(("https://github.com/a/hub/pull/13".to_owned(), 13))
        );
        assert_eq!(pr_link("no link"), None);
    }
}
