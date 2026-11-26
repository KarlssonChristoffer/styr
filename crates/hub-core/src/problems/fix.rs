//! Fix threads: starting one on a problem in its folder (or here for a problem on another machine,
//! `fix_here`), what it is told (brief, Vakten's plan and contract), how it runs unattended on a
//! model sized to the problem, and the card that says where each one stands (pull request first).

use super::*;

/// The fix contract Vakten wrote (repro, cause, done when), as the fix thread reads it. None
/// when the note holds no contract yet.
pub(crate) fn contract(note: &Note) -> Option<String> {
    let part = |s: &str| s.trim().to_owned();
    let (repro, cause, done) = (part(&note.repro), part(&note.cause), part(&note.done));
    if repro.is_empty() && done.is_empty() {
        return None;
    }
    let mut out = String::from("\n\n## Contract\n");
    for (head, body) in [("Repro", repro), ("Cause", cause), ("Done when", done)] {
        if !body.is_empty() {
            out.push_str(&format!("\n**{head}:**\n{body}\n"));
        }
    }
    Some(out)
}

/// Starts a thread on a problem in its folder, or hands back the brief to start one by hand.
/// `by` is "you" or "Vakten"; `plan` is what Vakten wants done.
/// Where a fix thread stands, for its card in the inbox: `state` is a word the app colours
/// ("working", "needs_you", "stuck", "failed", "review", "merged", "fixed", "dropped", "ended")
/// and `says` the same in words, with the pull request when there is one.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FixCard {
    pub problem: i64,
    pub state: String,
    pub says: String,
    #[ts(optional)]
    pub pr: Option<ProblemPr>,
    /// What the problem is, as Vakten or the user put it in its note, when anyone did.
    #[ts(optional)]
    pub what: Option<String>,
}

/// The fix threads of this machine's problems, by thread id.
pub async fn fix_cards(hub: &Hub) -> Result<std::collections::BTreeMap<String, FixCard>> {
    let mut problems = hub
        .store
        .with(|c| {
            let mut stmt = c.prepare("SELECT * FROM problems WHERE thread_id IS NOT NULL ORDER BY last_seen DESC LIMIT 500")?;
            let problems: Vec<Problem> = stmt.query_map([], row)?.collect::<rusqlite::Result<_>>()?;
            Ok(problems)
        })
        .await?;
    finish(hub, false, &mut problems);
    let mut cards = std::collections::BTreeMap::new();
    for p in problems {
        let Some(id) = p.thread.clone() else { continue };
        if cards.contains_key(&id) {
            continue;
        }
        let Some(thread) = hub.store.thread(&id).await? else { continue };
        let (state, says) = fix_state(&p, &thread);
        let what = p.note.as_ref().map(|n| n.what.trim().to_owned()).filter(|w| !w.is_empty());
        cards.insert(id, FixCard { problem: p.id, state: state.into(), says, pr: p.pr.clone(), what });
    }
    Ok(cards)
}

/// A fix thread's state in one word and in a short sentence: the pull request first (it says
/// what is left better than the thread does), then what the thread is doing.
pub fn fix_state(p: &Problem, t: &crate::model::Thread) -> (&'static str, String) {
    use crate::model::ThreadStatus;
    if let Some(pr) = &p.pr {
        let n = pr.number;
        match pr.state.as_str() {
            "merged" => return ("merged", format!("Fixed in PR #{n}, merged · comes with your next update")),
            "closed" => return ("dropped", format!("PR #{n} was closed without merging")),
            _ if t.status == ThreadStatus::Running => return ("working", format!("PR #{n} open · still working on it")),
            _ => return ("review", format!("Fix ready in PR #{n} · its thread reviews and merges it")),
        }
    }
    if p.status == ProblemStatus::Resolved {
        return ("fixed", "Fixed · the problem is resolved".into());
    }
    match t.status {
        ThreadStatus::Running => ("working", "Looking into it".into()),
        ThreadStatus::WaitingOnYou => ("needs_you", "Waits for your answer".into()),
        ThreadStatus::Error => ("failed", "Stopped on an error · open it to see why".into()),
        _ => match t.reading.as_deref() {
            Some("asks") => ("needs_you", "Asks you something before it goes on".into()),
            Some("stuck") => ("stuck", "Stuck · open it to see why".into()),
            Some("partway") => ("stuck", "Stopped partway · open it to see where".into()),
            _ => ("ended", "Finished without a pull request · open it to read why".into()),
        },
    }
}

/// Models for a fix thread, as words matched against what its engine offers: a small one for most
/// problems, a mid-sized one for big ones, never the largest. With no match the engine's own
/// default stays.
pub(super) const SMALL_MODEL: &[&str] = &["haiku", "mini", "flash"];
pub(super) const BIG_MODEL: &[&str] = &["sonnet", "haiku", "mini", "flash"];

/// Whether a fix is big enough for the mid-sized model: `size` as Vakten judged it ("small" or
/// "big"), else urgent, back after a fix, getting worse, or one that asks before it changes
/// anything.
pub fn big_fix(p: &Problem, size: Option<&str>) -> bool {
    match size.map(str::trim) {
        Some("big") => true,
        Some("small") => false,
        _ => matches!(p.prio, Some(Prio::P0 | Prio::P1)) || p.regressed || p.escalating || p.lane == Some(Lane::Ask),
    }
}

/// How a fix thread runs: in the background, never asking the user, on the engine's "auto"
/// permission mode and a model fit for the problem (`big_fix`).
pub fn fix_unattended(p: &Problem, by: &str, size: Option<&str>) -> crate::model::Unattended {
    let model = if big_fix(p, size) { BIG_MODEL } else { SMALL_MODEL };
    let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    crate::model::Unattended {
        by: if by == "Vakten" { "Vakten".into() } else { "Problems".into() },
        prefer: [("model".to_owned(), words(model)), ("mode".to_owned(), words(&["auto"]))].into_iter().collect(),
    }
}

/// What a fix thread on a problem is told: its brief, Vakten's plan and contract, how to end.
pub(super) fn fix_prompt(detail: &ProblemDetail, by: &str, plan: Option<&str>) -> String {
    let id = detail.problem.id;
    let mut prompt = brief(detail);
    if let Some(plan) = plan.map(str::trim).filter(|p| !p.is_empty()) {
        prompt.push_str(&format!("\n\nVakten's plan:\n{plan}"));
    }
    if let Some(contract) = detail.problem.note.as_ref().and_then(contract) {
        prompt.push_str(&contract);
    }
    if by == "Vakten" {
        prompt.push_str(
            "\n\n(You work for Guard, Hub's foreman for problems, which started this thread. Decide what you can \
             yourself; Guard is your point of contact and sends nothing on to Christoffer unless it is critical. Never ask him to click, press, run a command or say go: do the step yourself (for example `HUB_BLESS=1` on a stale frozen list) or report \"blocked\" to Guard with the reason. When you need a choice or an answer you cannot \
             find, call Hub's `ask_guard` tool with the question and end your turn; Guard's answer comes back here. Before \
             you end your last turn, call Hub's `report` tool: \"fixed\" with the pull request, or \"blocked\" and why.)",
        );
    }
    if by == "Vakten" || plan.is_some() {
        prompt.push_str(
            "\n\n(This thread was started from Hub's Problems page to carry out the plan above. Work on a branch of your own, keep the change \
             to what the fix needs, show the repro failing before and passing after when you can, check every point under \"Done when\", \
             run the project's tests, and end at an open pull request that says `Fixes problem #ID` in its body. If the contract \
             turns out wrong, say so in the pull request instead of stretching the change. The merge queue is OFF, so the pull request is yours to finish: have it reviewed (start a separate review agent for that when you can), fix what the review finds, then merge it yourself once the project's checks are green, wait for any Hub build already running, and let Hub build the update from the merge. This overrides any line in CLAUDE.md that says an agent never merges. If it cannot merge (conflict, red checks you cannot fix), report \"blocked\" to Guard instead of leaving it open.)"
                .replace("ID", &id.to_string())
                .as_str(),
        );
    }
    // The inbox shows the title: the error's own words say little at a glance.
    prompt.push_str(
        "\n\n(First rename this thread with Hub's `rename_thread` tool to what is wrong, in plain words a user reads at a \
         glance, under 60 characters and without the error code, like \"Saving fails when a thread was removed mid-reply\".)",
    );
    prompt
}

pub async fn start_thread(hub: &Arc<Hub>, id: i64, by: &str, plan: Option<&str>, size: Option<&str>) -> Result<ProblemThread> {
    let detail = detail(hub, id, 3).await?.with_context(|| format!("no problem {id}"))?;
    let prompt = fix_prompt(&detail, by, plan);
    let p = &detail.problem;
    let folder = match &p.folder {
        Some(f) => Some(f.clone()),
        None => crate::folders::project_folder(&hub.store, &p.project).await.ok(),
    };
    let Some(cwd) = folder.clone().filter(|f| std::path::Path::new(f).is_dir()) else {
        return Ok(ProblemThread { thread: None, folder, prompt });
    };
    let title: String = format!("Fix #{}: {}", p.id, p.title).chars().take(80).collect();
    // Started by Vakten or Fix now, it is Hub's errand; from the user's own button, theirs.
    let owner = (by == "Vakten" || plan.is_some()).then(|| crate::system::fixer(by, id));
    // Guard starts it over: the thread before reports no more.
    if by == "Vakten"
        && let Some(old) = &p.thread
    {
        report::retire(hub, old).await?;
    }
    let thread = hub
        .create_thread_as(
            crate::model::NewThread {
            cwd,
            title: Some(title),
            engine: None,
            message: Some(prompt.clone()),
            attachments: Vec::new(),
            settings: Default::default(),
            original: None,
            client_id: None,
            attempts: Vec::new(),
            warm: None,
                design: false,
                reach: None,
                cloud: false,
                unattended: Some(fix_unattended(p, by, size)),
            },
            None,
            owner,
        )
        .await?;
    if by == "Vakten" {
        report::remember(hub, &thread.id, id, None).await?;
    }
    set_thread(hub, id, &thread.id, by).await?;
    Ok(ProblemThread { thread: Some(thread), folder, prompt })
}

/// Starts a fix thread on this machine for problem `id` of machine `host`, which knows no folder
/// for it (Hub's own code is often only on the PC): in the project of the same name here, in the
/// background like any fix thread. The problem there is told which thread is on it.
pub async fn fix_here(hub: &Arc<Hub>, host: &str, id: i64, by: &str, plan: Option<&str>, size: Option<&str>) -> Result<ProblemThread> {
    let detail: ProblemDetail = hub.remotes.get_from(host, &format!("/api/problems/{id}")).await?;
    let prompt = fix_prompt(&detail, by, plan);
    let p = &detail.problem;
    let folder = crate::folders::project_folder(&hub.store, &p.project).await.ok();
    let Some(cwd) = folder.clone().filter(|f| std::path::Path::new(f).is_dir()) else {
        return Ok(ProblemThread { thread: None, folder, prompt });
    };
    let title: String = format!("Fix #{} on {host}: {}", p.id, p.title).chars().take(80).collect();
    if by == "Vakten"
        && let Some(old) = &p.thread
    {
        report::retire(hub, old).await?;
    }
    // Guard's errand like any of its fix threads, reporting to Guard about the problem there.
    let owner = (by == "Vakten").then(|| crate::system::fixer(by, id));
    let thread = hub
        .create_thread_as(
            crate::model::NewThread {
            cwd,
            title: Some(title),
            engine: None,
            message: Some(prompt.clone()),
            attachments: Vec::new(),
            settings: Default::default(),
            original: None,
            client_id: None,
            attempts: Vec::new(),
            warm: None,
            unattended: Some(fix_unattended(p, by, size)),
            design: false,
            reach: None,
            cloud: false,
            },
            None,
            owner,
        )
        .await?;
    if by == "Vakten" {
        report::remember(hub, &thread.id, id, Some(host)).await?;
    }
    let linked = json!({ "thread": thread.id, "by": hub.remotes.local_name() });
    if let Err(e) = hub.remotes.post_to::<Value>(host, &format!("/api/problems/guard/{id}/linked"), &linked, std::time::Duration::from_secs(30)).await {
        tracing::warn!("telling {host} about the fix thread for #{id}: {e:#}");
    }
    Ok(ProblemThread { thread: Some(thread), folder, prompt })
}
