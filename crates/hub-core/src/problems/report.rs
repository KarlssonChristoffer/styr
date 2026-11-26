//! Guard's fix threads report back to Guard, not to the user (Guard 2, step 1).
//!
//! When a fix thread Guard started ends a turn without a pull request, fails, or asks something,
//! a [`Report`] waits for Guard's next turn: what happened and the thread's last words. A turn that
//! ends at a pull request is only noted in the problem's activity; the merge queue takes it from
//! there. Guard
//! then decides by itself: it answers the thread (`answer_worker`), starts the fix again with a new
//! plan (`problem_thread` with `again`), or leaves the problem and says why. The thread itself can
//! say how it ended (`report`) and ask Guard instead of the user (`ask_guard`).
//!
//! Reports wait on the machine where the thread runs. Guard's own machine reads its own; another
//! machine hands its reports over with its queue (`vakten::guard_queue`), as it does its problems.

use std::sync::Arc;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::hub::Hub;
use crate::model::{Event, EventKind, OwnerKind, Thread, now_ms};

/// Reports waiting for Guard on this machine, oldest first.
const PENDING: &str = "problems.vakten.reports";
/// The most kept while Guard is off or away: the oldest go first.
const MAX: usize = 50;
/// A report's words, at most.
const WORDS: usize = 600;
/// One writer at a time on the pending list.
static LIST: Mutex<()> = Mutex::const_new(());

fn fix_key(thread: &str) -> String {
    format!("problems.vakten.fix:{thread}")
}
fn said_key(thread: &str) -> String {
    format!("problems.vakten.said:{thread}")
}
fn asked_key(thread: &str) -> String {
    format!("problems.vakten.asked:{thread}")
}

/// Which problem a fix thread of Guard's works on, and the machine the problem is on (None: the
/// machine the thread runs on).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fix {
    pub problem: i64,
    #[serde(default)]
    pub machine: Option<String>,
}

/// What a fix thread of Guard's did, for Guard's next turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Report {
    pub problem: i64,
    /// The machine the problem is on, as the sender sees it; None for the sender itself.
    #[serde(default)]
    pub machine: Option<String>,
    pub thread: String,
    /// The machine the thread runs on; None for the sender itself.
    #[serde(default)]
    pub thread_machine: Option<String>,
    pub title: String,
    /// "pr" (a pull request is open), "ended" (finished its turn without one), "blocked" (says it
    /// cannot go on), "question" (asks Guard), "stopped" (cut off) or "failed" (its engine failed).
    pub outcome: String,
    pub text: String,
    #[serde(default)]
    pub pr: Option<u32>,
    pub ts: i64,
}

impl Report {
    /// The report as seen from Guard's machine, when `host` handed it over.
    pub fn from_host(mut self, host: &str) -> Self {
        self.machine = Some(self.machine.unwrap_or_else(|| host.to_owned()));
        self.thread_machine = Some(self.thread_machine.unwrap_or_else(|| host.to_owned()));
        self
    }

    /// One line in Guard's turn.
    pub fn line(&self) -> String {
        let on = self.machine.as_deref().map(|m| format!(" on {m}")).unwrap_or_default();
        let runs = self.thread_machine.as_deref().map(|m| format!(", machine \"{m}\"")).unwrap_or_default();
        let what = match self.outcome.as_str() {
            "pr" => format!("pull request #{} is open", self.pr.unwrap_or_default()),
            "ended" => "ended its turn without a pull request".into(),
            "blocked" => "says it is blocked".into(),
            "question" => "asks you".into(),
            "stopped" => "was stopped before it finished".into(),
            "failed" => "failed".into(),
            other => other.to_owned(),
        };
        let text = self.text.trim();
        let says = if text.is_empty() { String::new() } else { format!(": {}", text.replace('\n', " ")) };
        format!("- #{}{on} · thread \"{}\" (thread_id {}{runs}) {what}{says}", self.problem, self.title, self.thread)
    }
}

/// Notes that `thread` is Guard's fix thread for `problem` (on `machine`, None for here).
pub async fn remember(hub: &Hub, thread: &str, problem: i64, machine: Option<&str>) -> Result<()> {
    let fix = Fix { problem, machine: machine.map(str::to_owned) };
    hub.store.set_value(&fix_key(thread), &serde_json::to_string(&fix)?).await
}

/// The problem a thread fixes for Guard, if it is one of Guard's fix threads.
pub async fn fix_of(hub: &Hub, thread: &Thread) -> Option<Fix> {
    // "null": started over, so it reports no more.
    if let Some(kept) = hub.store.get_value(&fix_key(&thread.id)).await.ok().flatten() {
        return serde_json::from_str::<Option<Fix>>(&kept).ok().flatten();
    }
    // Started before Hub kept this: Guard's errands carry the problem in their owner.
    let owner = thread.owner.as_ref().filter(|o| o.kind == OwnerKind::Fixer && o.name == "Vakten")?;
    Some(Fix { problem: owner.id.as_deref()?.parse().ok()?, machine: None })
}

/// The thread says how it ended, before it ends its turn: "fixed" or "blocked", and why.
pub async fn said(hub: &Hub, thread: &Thread, state: &str, text: &str) -> Result<()> {
    if fix_of(hub, thread).await.is_none() {
        bail!("report is for Guard's fix threads; this thread is not one");
    }
    if !matches!(state, "fixed" | "blocked") {
        bail!("state is \"fixed\" or \"blocked\"");
    }
    let value = serde_json::json!({ "state": state, "text": crate::text::cut(text, WORDS, false) });
    hub.store.set_value(&said_key(&thread.id), &value.to_string()).await
}

/// The thread asks Guard instead of the user. Guard gets it on its next turn and answers in
/// the thread; the turn that asked is not reported again.
pub async fn ask(hub: &Hub, thread: &Thread, question: &str) -> Result<()> {
    let Some(fix) = fix_of(hub, thread).await else { bail!("ask_guard is for Guard's fix threads; this thread is not one") };
    let question = question.trim();
    if question.is_empty() {
        bail!("question is missing");
    }
    hub.store.set_value(&asked_key(&thread.id), &now_ms().to_string()).await?;
    let report = Report {
        problem: fix.problem,
        machine: fix.machine,
        thread: thread.id.clone(),
        thread_machine: None,
        title: thread.title.clone(),
        outcome: "question".into(),
        text: crate::text::cut(question, WORDS, false),
        pr: None,
        ts: now_ms(),
    };
    push(hub, report).await
}

/// How a turn ended, from what the thread said and its pull request: the outcome and its words.
pub fn outcome(failed: bool, stop_reason: &str, said: Option<(&str, &str)>, pr: Option<u32>, reply: &str, error: &str) -> (&'static str, String) {
    if failed {
        let error = if error.trim().is_empty() { "The engine failed" } else { error };
        return ("failed", error.to_owned());
    }
    if stop_reason != "end_turn" {
        return ("stopped", "The turn was stopped before it finished.".into());
    }
    let words = |own: Option<&str>| own.filter(|t| !t.trim().is_empty()).unwrap_or(reply).to_owned();
    match (said, pr) {
        (Some(("blocked", text)), _) => ("blocked", words(Some(text))),
        (Some((_, text)), Some(_)) => ("pr", words(Some(text))),
        (None, Some(_)) => ("pr", words(None)),
        (Some((_, text)), None) => ("ended", words(Some(text))),
        (None, None) => ("ended", words(None)),
    }
}

/// A turn of one of Guard's fix threads ended or failed: Guard hears what happened.
pub async fn turn_ended(hub: &Arc<Hub>, end: &Event) -> Result<()> {
    if !matches!(end.kind, EventKind::TurnEnd | EventKind::Error) {
        return Ok(());
    }
    let Some(thread) = hub.store.thread(&end.thread_id).await? else { return Ok(()) };
    if thread.archived {
        return Ok(());
    }
    let Some(fix) = fix_of(hub, &thread).await else { return Ok(()) };
    let said: Option<(String, String)> = hub
        .store
        .take_value(&said_key(&thread.id))
        .await?
        .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok())
        .map(|v| (v["state"].as_str().unwrap_or_default().to_owned(), v["text"].as_str().unwrap_or_default().to_owned()));
    // The turn asked Guard: that question is already on its way.
    if hub.store.take_value(&asked_key(&thread.id)).await?.is_some() && end.kind == EventKind::TurnEnd && said.is_none() {
        return Ok(());
    }
    let chunks = hub.store.last_reply_chunks(&thread.id).await?;
    let reply = crate::text::tail(crate::replies::reply(&chunks, i64::MAX).trim(), WORDS);
    let pr = hub.thread_prs().get(&thread.id).map(|p| p.number).or_else(|| super::fixnow::pr_link(&reply).map(|(_, n)| n));
    let (outcome, text) = outcome(
        end.kind == EventKind::Error,
        end.stop_reason().unwrap_or_default(),
        said.as_ref().map(|(s, t)| (s.as_str(), t.as_str())),
        pr,
        &reply,
        end.failure().unwrap_or_default(),
    );
    let text = crate::text::cut(&text, WORDS, true);
    if fix.machine.is_none() && outcome != "pr" {
        let line = format!("{}: {outcome}", thread.title);
        if let Err(e) = super::note_activity(hub, fix.problem, "Hub", "report", &line).await {
            tracing::debug!(target: "hub_core::problems", "noting a fix thread's report on #{}: {e:#}", fix.problem);
        }
    }
    // At a pull request the fix is done as far as Guard goes; Hub follows the pull request.
    if outcome == "pr" {
        return Ok(());
    }
    let report = Report {
        problem: fix.problem,
        machine: fix.machine,
        thread: thread.id.clone(),
        thread_machine: None,
        title: thread.title.clone(),
        outcome: outcome.into(),
        text,
        pr,
        ts: now_ms(),
    };
    push(hub, report).await
}

/// Guard answers one of its fix threads here: the thread carries on with it, marked as Guard's.
pub async fn answer(hub: &Arc<Hub>, thread: &str, text: &str, from: serde_json::Value) -> Result<()> {
    let text = text.trim();
    if text.is_empty() {
        bail!("text is missing");
    }
    let Some(t) = hub.store.thread(thread).await? else { bail!("no thread {thread} here") };
    if t.archived || fix_of(hub, &t).await.is_none() {
        bail!("thread {thread} is not one of Guard's fix threads");
    }
    hub.send_from(thread, text.to_owned(), Vec::new(), None, Some(("from", from)), None).await?;
    Ok(())
}

/// A fix thread was started over (`again`): it no longer reports, and it is put away.
pub async fn retire(hub: &Hub, thread: &str) -> Result<()> {
    hub.store.set_value(&fix_key(thread), "null").await?;
    let Some(t) = hub.store.thread(thread).await? else { return Ok(()) };
    if t.settled_at.is_none() && !t.archived {
        let now = now_ms();
        let thread = hub.store.mark_thread(thread, move |t| t.settled_at = Some(now)).await?;
        hub.publish(crate::model::StreamMsg::Thread { thread });
    }
    Ok(())
}

/// Puts a report on the list for Guard and wakes it.
pub async fn push(hub: &Hub, report: Report) -> Result<()> {
    {
        let _one = LIST.lock().await;
        let mut list = read(hub).await;
        list.push(report);
        if list.len() > MAX {
            list.drain(..list.len() - MAX);
        }
        hub.store.set_value(PENDING, &serde_json::to_string(&list)?).await?;
    }
    super::vakten::nudge();
    Ok(())
}

/// Whether reports wait for Guard here.
pub async fn waiting(hub: &Hub) -> bool {
    !read(hub).await.is_empty()
}

/// Every report waiting here, taken off the list.
pub async fn take(hub: &Hub) -> Vec<Report> {
    let _one = LIST.lock().await;
    hub.store.take_value(PENDING).await.ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

async fn read(hub: &Hub) -> Vec<Report> {
    hub.store.get_value(PENDING).await.ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_ends_as_what_it_said_and_its_pull_request() {
        assert_eq!(outcome(true, "", None, None, "", "boom").0, "failed");
        assert_eq!(outcome(false, "cancelled", None, None, "half", "").0, "stopped");
        assert_eq!(outcome(false, "end_turn", None, Some(31), "Opened #31", ""), ("pr", "Opened #31".into()));
        assert_eq!(outcome(false, "end_turn", Some(("blocked", "No access to tincan")), Some(31), "x", ""), ("blocked", "No access to tincan".into()));
        assert_eq!(outcome(false, "end_turn", Some(("fixed", "")), None, "Done, see above", ""), ("ended", "Done, see above".into()));
        assert_eq!(outcome(false, "end_turn", None, None, "Which file?", "").0, "ended");
    }

    #[test]
    fn a_report_from_another_machine_names_it() {
        let r = Report {
            problem: 12,
            machine: None,
            thread: "t1".into(),
            thread_machine: None,
            title: "Fix #12: x".into(),
            outcome: "question".into(),
            text: "Which file?".into(),
            pr: None,
            ts: 0,
        }
        .from_host("tincan");
        assert_eq!(r.machine.as_deref(), Some("tincan"));
        assert!(r.line().contains("#12 on tincan"));
        assert!(r.line().contains("machine \"tincan\""));
        assert!(r.line().contains("asks you: Which file?"));
    }
}
