//! Work that goes on beside the thread: a side job (its own child thread, offered to top-level
//! threads only) and watching a pull request until it merges or closes. Their declarations
//! (`side_tools`) and what each call does.

use std::sync::Arc;

use serde_json::{Value, json};

use super::{on_thread, thread_of};
use crate::hub::Hub;
use crate::hub_tools::Caller;
use crate::hub_tools::tool::{Tool, text};
use crate::run;

const WATCH_PR: &str = "Watch a pull request on GitHub for this thread: Hub checks it every two minutes and \
sends you a message when a check fails, all checks pass, someone comments or reviews, or the branch starts to conflict \
with its base, so you can fix it. Use it when the user asks you to watch, monitor or babysit a pull request. Comments \
carrying a Claude Code footer do not wake you. Watching ends when the pull request merges or closes.";

const SIDE_JOB: &str = "Hand one piece of work to a side job: its own thread, often on another model, that runs \
in the background while you carry on. Its result comes back to you as a message when it is done, and the user sees it \
in this thread as it happens. Use it for every picture the user asks you to make or edit (kind \"image\": it runs on \
the engine the user picked for pictures), also when you have an image tool of your own, and for a separate piece that would sidetrack this thread: a research \
question, a review, a self-contained sub-task. Not for a diagram, flowchart or chart (draw those yourself as SVG or a page), and not \
for the main work the user asked you for. Write the task so it \
stands on its own: what to make, every detail that matters (style, sizes, names, where to save files), and what to \
answer with. With context \"brief\" the side job sees only your task; \"handover\" adds a summary of this thread; \
\"full\" the whole conversation. Pick the least that works.";

/// The side job, which a side job does not get itself, and watching a pull request.
pub(super) fn side_tools(tools: &mut Vec<Tool>, caller: &Caller) {
    if thread_of(caller).is_some_and(|t| t.parent.is_none()) {
        tools.push(Tool::of(json!({
            "name": "start_side_job",
            "description": SIDE_JOB,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "A short name the user sees, a few words, e.g. \"Hero image\"." },
                    "task": { "type": "string", "description": "What the side job should do, standing on its own." },
                    "kind": { "type": "string", "enum": crate::children::KINDS },
                    "context": { "type": "string", "enum": crate::children::CONTEXTS, "description": "How much of this thread it gets. Defaults to brief." },
                    "engine": { "type": "string", "description": "An engine from hub_status, to run it on instead of the one Hub picks for this kind (pictures go to the engine the user chose for them). Leave out unless the job suits another engine, e.g. a quick lookup on a cheaper one." },
                    "model": { "type": "string", "description": "A model that engine offers (see hub_status), e.g. \"haiku\" for a quick lookup." },
                },
                "required": ["title", "task", "kind"],
            },
        }), run!(|c| thread!(c, |t| text(start_side_job(c.hub, t, c.args).await)))));
    }
    tools.push(Tool::of(json!({
        "name": "watch_pull_request",
        "description": WATCH_PR,
        "inputSchema": {
            "type": "object",
            "properties": {
                "number": { "type": "integer", "description": "The pull request's number. Leave it out for the pull request of this thread's branch." },
            },
        },
    }), run!(|c| thread!(c, |t| text(watch_pull_request(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "stop_watching_pull_request",
        "description": "Stops watching the pull request this thread watches. Call it when you hand the work back to the user.",
        "inputSchema": { "type": "object", "properties": {} },
    }), run!(|c| thread!(c, |t| text(stop_watching(c.hub, t).await)))));
}

async fn start_side_job(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let text = |key: &str| args[key].as_str().unwrap_or_default().to_owned();
    let job = crate::children::SideJob {
        title: text("title"),
        task: text("task"),
        kind: Some(text("kind")).filter(|k| !k.is_empty()).unwrap_or_else(|| "other".into()),
        context: Some(text("context")).filter(|c| !c.is_empty()).unwrap_or_else(|| "brief".into()),
        engine: Some(text("engine")).filter(|e| !e.is_empty()),
        model: Some(text("model")).filter(|m| !m.is_empty()),
    };
    // A picture for an everyday chat does not come back as a message (`children::end_side_job`).
    let shown_only = job.kind == "image" && hub.projects_root().await.is_ok_and(|root| crate::projects_root::is_chat_folder(&root, &thread.cwd));
    let after = if shown_only {
        "You are not called back when it is done: the picture shows in the chat by itself. Do anything else the user asked now, then end your turn."
    } else {
        "Its result comes back to you as a message when it is done; carry on with your work meanwhile, or end your turn if there is nothing else to do."
    };
    match hub.start_side_job(&thread.id, job).await {
        Ok(child) => Ok(format!(
            "Started the side job \"{}\" on {} as its own thread. The user sees it working in this thread and sees its \
             result there the moment it is done, so don't say it is still being made or promise to show it; your words \
             stay in the thread after it is done. {after}",
            child.title, child.engine
        )),
        Err(e) => Err(format!("{e:#}")),
    }
}

async fn watch_pull_request(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    match hub.watch_pr(&thread.id, args["number"].as_u64()).await {
        Ok(n) => Ok(format!(
            "Watching pull request #{n}. Hub checks it every two minutes and messages you when a check fails, all checks pass, \
             someone comments or reviews, or the branch starts to conflict. End this turn now; the message wakes you."
        )),
        Err(e) => Err(format!("{e:#}")),
    }
}

async fn stop_watching(hub: &Arc<Hub>, thread: &crate::model::Thread) -> Result<String, String> {
    match hub.unwatch_pr(&thread.id).await {
        Ok(true) => Ok("Stopped watching.".into()),
        Ok(false) => Ok("This thread was not watching a pull request.".into()),
        Err(e) => Err(format!("{e:#}")),
    }
}
