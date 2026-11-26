//! Who does which kind of task (Settings > Handoffs). Each task type has a pick for everyday chats
//! and one for dev threads: an engine and a model. A task left out is done by the thread itself,
//! as before.
//!
//! Two roads reach the same table. Hub's own one-shot questions (`engine::complete`, each with a
//! `purpose`) ask `tidy_target`, and Customs asks for `vetting`: those are hard, since Hub is the
//! one that runs them. A side job the agent starts (`children.rs`) takes its engine from the
//! table by its `kind` and refuses another one: soft, since the agent still has to choose to hand
//! the job over (Hub's tool instructions say so, `instructions`). Subscriptions and the keys the
//! user added by hand only: a pick is never swapped for another engine when it fails.
//!
//! Kept in the store (kv `handoff_prefs`) and sent along with the engines to every machine, the
//! way `agent_prefs.rs` is, so a thread run on another machine hands off the same way.

use anyhow::Result;

use crate::ctx::Ctx;
use crate::model::{HandoffPick, HandoffPrefs};

const PREFS: &str = "handoff_prefs";
const PREFS_AT: &str = "handoff_prefs_at";

/// The task types, as (id, what the user calls it). The ids are the keys of `HandoffPrefs::tasks`.
pub const TASKS: [(&str, &str); 6] = [
    ("pictures", "Pictures"),
    ("vetting", "Plugin vetting"),
    ("research", "Research"),
    ("code_review", "Code review"),
    ("quick", "Quick questions"),
    ("housekeeping", "Titles and summaries"),
];

/// The task a side job of `kind` belongs to (`children::KINDS`); `subtask` and `other` are the
/// thread's own business.
pub fn task_of_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "image" => Some("pictures"),
        "research" => Some("research"),
        "review" => Some("code_review"),
        "quick" => Some("quick"),
        _ => None,
    }
}

/// What the user calls task `id`.
pub fn label(id: &str) -> &'static str {
    TASKS.iter().find(|(t, _)| *t == id).map_or("a task", |(_, l)| l)
}

pub async fn prefs(ctx: &Ctx<'_>) -> Result<HandoffPrefs> {
    Ok(ctx.store.get_value(PREFS).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

pub async fn set_prefs(ctx: &Ctx<'_>, prefs: HandoffPrefs) -> Result<HandoffPrefs> {
    save(ctx, prefs, crate::model::now_ms()).await
}

/// Keeps what can be used: known task ids, picks that name an engine. An engine this machine does
/// not have stays (the pick may be for a thread on another machine), so the table travels whole.
async fn save(ctx: &Ctx<'_>, mut prefs: HandoffPrefs, at: i64) -> Result<HandoffPrefs> {
    prefs.tasks.retain(|id, _| TASKS.iter().any(|(t, _)| t == id));
    for task in prefs.tasks.values_mut() {
        for side in [&mut task.chat, &mut task.dev] {
            *side = side.take().and_then(clean);
        }
    }
    prefs.tasks.retain(|_, t| t.chat.is_some() || t.dev.is_some());
    ctx.store.set_value(PREFS, &serde_json::to_string(&prefs)?).await?;
    ctx.store.set_value(PREFS_AT, &at.to_string()).await?;
    Ok(prefs)
}

fn clean(pick: HandoffPick) -> Option<HandoffPick> {
    let engine = pick.engine.trim().to_owned();
    let model = pick.model.map(|m| m.trim().to_owned()).filter(|m| !m.is_empty());
    (!engine.is_empty()).then_some(HandoffPick { engine, model })
}

async fn set_at(ctx: &Ctx<'_>) -> i64 {
    ctx.store.get_value(PREFS_AT).await.ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// What travels with the engines to every other machine (`engine_share::Shipment`). None when the table
/// was never set here.
pub async fn outgoing(ctx: &Ctx<'_>) -> Option<Shared> {
    let at = set_at(ctx).await;
    (at != 0).then_some(())?;
    Some(Shared { prefs: prefs(ctx).await.ok()?, at })
}

/// Takes in what another machine sent when it was set later than what is here; the newest wins.
pub async fn take_in(ctx: &Ctx<'_>, shared: Shared) -> Result<bool> {
    if shared.at <= set_at(ctx).await {
        return Ok(false);
    }
    save(ctx, shared.prefs, shared.at).await?;
    Ok(true)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Shared {
    pub prefs: HandoffPrefs,
    /// When it was set, in ms.
    pub at: i64,
}

/// The pick for `task` in a thread whose folder is `cwd` (a chat or a dev thread), when there is
/// one and this machine has the engine. `None` cwd is a dev thread.
pub async fn pick(ctx: &Ctx<'_>, task: &str, cwd: Option<&str>) -> Option<HandoffPick> {
    let chat = match cwd {
        Some(cwd) => crate::projects_root::get(ctx.store).await.is_ok_and(|root| crate::projects_root::is_chat_folder(&root, cwd)),
        None => false,
    };
    let all = prefs(ctx).await.ok()?;
    let task = all.tasks.get(task)?;
    let pick = if chat { task.chat.clone() } else { task.dev.clone() }?;
    ctx.config.engines.contains_key(&pick.engine).then_some(pick)
}

/// The engine and model for Hub's own small helper questions (titles, summaries, tidying): the
/// pick for "Titles and summaries" in the thread's folder, else the tidy engine and model from
/// `hub.json` as before. `Hub::tidy_target` asks this.
pub async fn tidy_target(ctx: &Ctx<'_>, cwd: Option<&str>) -> (String, Option<String>) {
    if let Some(p) = pick(ctx, "housekeeping", cwd).await {
        return (p.engine, p.model);
    }
    let name = ctx.config.tidy.engine.clone().unwrap_or_else(|| ctx.config.default_engine.clone());
    (name, ctx.config.tidy.model.clone())
}

/// What Hub tells an agent about the side jobs it may hand over, for the tasks that have a pick in
/// its kind of thread (pictures are told apart, in `mcp.rs`). Empty when there are none.
pub async fn instructions(ctx: &Ctx<'_>, cwd: &str) -> String {
    let mut lines = Vec::new();
    for (kind, text) in [
        ("research", "looking things up and reading sources"),
        ("review", "reviewing code or a change"),
        ("quick", "a quick question that needs no context"),
    ] {
        let Some(task) = task_of_kind(kind) else { continue };
        if let Some(p) = pick(ctx, task, Some(cwd)).await {
            let model = p.model.map(|m| format!(", {m}")).unwrap_or_default();
            lines.push(format!("- {text}: start_side_job with kind \"{kind}\" (it runs on {}{model}; do not name another engine)", p.engine));
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    format!("The user has chosen who does these tasks. Hand them to a side job instead of doing them yourself:\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_map_to_tasks_and_subtasks_stay_with_the_thread() {
        assert_eq!(task_of_kind("image"), Some("pictures"));
        assert_eq!(task_of_kind("review"), Some("code_review"));
        assert_eq!(task_of_kind("subtask"), None);
        assert_eq!(task_of_kind("other"), None);
        for (id, _) in TASKS {
            assert!(!label(id).is_empty());
        }
    }

    #[test]
    fn a_pick_without_an_engine_is_dropped_and_a_blank_model_is_none() {
        assert!(clean(HandoffPick { engine: "  ".into(), model: None }).is_none());
        let p = clean(HandoffPick { engine: " hermes ".into(), model: Some("  ".into()) }).unwrap();
        assert_eq!((p.engine.as_str(), p.model), ("hermes", None));
    }
}
