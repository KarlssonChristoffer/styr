//! The thread tools for talking with the user and handing work on: a second opinion and a design
//! review, plugin and handoff cards, reactions, the steps card, the TL;DR, one-click replies and
//! `ask_user`. Their declarations (`talk_tools`) and what each call does.

use std::sync::Arc;

use serde_json::{Value, json};

use super::on_thread;
use crate::hub::Hub;
use crate::hub_tools::tool::{Tool, text};
use crate::model::ReviewRequest;
use crate::run;

const SECOND_OPINION: &str = "Ask an independent reviewer (another model) for a second opinion on the \
changes in this repository since the last review. Use it when it matters: risky or tricky changes \
(security, data loss, concurrency, migrations, money, auth), a large change, or before calling a \
sizeable task done. Not for small or obvious edits. It runs in the background; its findings come back \
to you as a message after your turn, so carry on or end the turn.";

const OFFER_REPLIES: &str = "Offers the user one-click answers to the question you end your turn on: buttons \
docked over the message box, also answered with the keys 1, 2, 3. Use it whenever you end with a yes/no question or a \
proposal (\"Should I start a new thread on those, with the review attached?\"), as the last thing before your turn ends, and \
still ask the question in your reply. Give 2 to 4 short options the user would really pick, the one you recommend first; \
for work that finishes this thread, offer one that also closes it, e.g. [{\"label\": \"Go ahead\"}, {\"label\": \"Go ahead and close \
this one\", \"message\": \"Go ahead, then close this thread.\"}, {\"label\": \"No\"}]. A click sends `message` (or the label) \
as the user's next message. Writing anything else puts the buttons away.";

const ASK_USER: &str = "Asks the user a question without waiting for the answer. The call returns at once with \
`{\"answered\": false, \"assuming\": ...}`: carry on with your assumption (say in your reply what you assumed), or do other work that \
does not depend on it. The question stays in the chat as buttons; if the user answers later, it reaches you as a normal message (\"You \
asked X and assumed Y; the person answered Z\"), and if Z differs from Y you change course and redo only what depended on it. The thread \
does not show \"needs you\" because of it. Use it for forks that are cheap to undo. For anything hard to undo (deleting, pushing, \
spending) do not use it: do the step and let the permission prompt ask, which does wait. Give 2 to 4 short options and the one you \
assume, which should be one of them.";

const UPDATE_STEPS: &str = "Shows the user a small plan card above your work: the steps you are taking, ticked off as you go. \
Use it when a job has three or more steps (not for a quick answer or one edit): call it first with the steps as pending and the \
first one in_progress, then again each time a step starts or finishes. Every call carries the whole list, so it replaces the \
earlier one. Keep it to 3 to 8 short steps in the user's language, with one in_progress at a time. It changes nothing else: it is \
only what the user sees. If you have a to-do tool of your own, use that instead.";

const WRITE_TLDR: &str = "Writes the TL;DR the user sees when they rest the pointer on this thread in their list. Call it as \
the last thing before your turn ends when the work was more than a small change: what the goal or problem was, what you \
did, where it stands now (tests, pull request, what is left). One to three short points in the language the user writes in, \
files and commands in backticks, pull requests as #123. `needs` is one short question or request if you need something from \
the user, otherwise leave it out. It replaces your earlier TL;DR and is shown with the time you wrote it. Skip it for a \
quick answer. When the user asks you for a TL;DR, always write one.";

/// Asking, reacting and handing over, for a thread's own agent.
pub(super) fn talk_tools(tools: &mut Vec<Tool>, hub: &Arc<Hub>) {
    tools.push(Tool::of(json!({
        "name": "second_opinion",
        "description": SECOND_OPINION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "reason": { "type": "string", "description": "Why this needs another look, in a sentence the user sees." },
            },
            "required": ["reason"],
        },
    }), run!(|c| thread!(c, |t| text(second_opinion(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "review_design",
        "description": "Has fresh reviewers read your design document under docs/design/ without this conversation: one explains it back, one looks for gaps, one checks it against the code and one says whether it is ready to build from. Use it in design mode when the document is written or updated. Their findings come back to you as a message after this turn.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "doc": { "type": "string", "description": "The document, relative to the folder. Leave out for the one you last wrote under docs/design/." },
            },
        },
    }), run!(|c| thread!(c, |t| text(review_design(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "suggest_plugin",
        "description": "Shows the user a plugin card in this thread: a GitHub repository made for agents (an MCP server, a skill or a tool) with its stars, how fast it is rising and what Customs says, and buttons to open it in Hub. Use it when the user asks for a tool for something or you find a repository that would help, after plugins_search. Nothing is installed or run.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "repo": { "type": "string", "description": "owner/name, or the GitHub link." },
                "why": { "type": "string", "description": "One or two sentences the user reads on the card: what it does and why it fits here." },
            },
            "required": ["repo", "why"],
        },
    }), run!(|c| thread!(c, |t| text(suggest_plugin(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "offer_handoff",
        "description": "Offers to hand this thread's work to one of the user's own tools that takes handoffs, such as Knegarn (the ones your_tools lists with a handoff). Use it when the user asks for it in words (\"run Knegarn on this\", \"let Knegarn build it\"), never on your own. The user sees a card with one button; pressing it is the approval, and Hub then drafts the brief from this conversation and hands it over. Nothing reaches the tool before that press. Never start such a tool by running its scripts yourself.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "folder": { "type": "string", "description": "The tool's folder, as your_tools gives it." },
                "why": { "type": "string", "description": "One sentence the user reads on the card: the work you would hand over." },
            },
            "required": ["folder", "why"],
        },
    }), run!(|c| thread!(c, |t| text(offer_handoff(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "react",
        "description": crate::reactions::tool_description(&hub.config.data_dir),
        "inputSchema": {
            "type": "object",
            "properties": {
                "emoji": { "type": "string", "description": "One emoji, for example 👍, or the :name: of one of the user's own." },
                "remove": { "type": "boolean", "description": "Take your reaction away again instead of putting it on." },
            },
            "required": ["emoji"],
        },
    }), run!(|c| thread!(c, |t| text(react(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "update_steps",
        "description": UPDATE_STEPS,
        "inputSchema": {
            "type": "object",
            "properties": {
                "steps": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 12,
                    "description": "The whole plan, in order.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "The step, a few words." },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] },
                        },
                        "required": ["content", "status"],
                    },
                },
            },
            "required": ["steps"],
        },
    }), run!(|c| thread!(c, |t| text(update_steps(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "write_tldr",
        "description": WRITE_TLDR,
        "inputSchema": {
            "type": "object",
            "properties": {
                "points": { "type": "array", "minItems": 1, "maxItems": 3, "items": { "type": "string" }, "description": "One to three short points." },
                "needs": { "type": "string", "description": "What you need from the user, one short question or request. Leave out when nothing." },
            },
            "required": ["points"],
        },
    }), run!(|c| thread!(c, |t| text(write_tldr(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "offer_replies",
        "description": OFFER_REPLIES,
        "inputSchema": {
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The question, short, as the user reads it over the buttons." },
                "options": {
                    "type": "array",
                    "minItems": 2,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string", "description": "A few words on the button, e.g. \"Go ahead\"." },
                            "message": { "type": "string", "description": "What the click sends, when the label alone is not clear enough." },
                        },
                        "required": ["label"],
                    },
                },
            },
            "required": ["question", "options"],
        },
    }), run!(|c| thread!(c, |t| text(offer_replies(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "ask_user",
        "description": ASK_USER,
        "inputSchema": {
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "The question, short, as the user reads it over the buttons." },
                "options": {
                    "type": "array",
                    "minItems": 2,
                    "maxItems": 4,
                    "items": {
                        "type": "object",
                        "properties": {
                            "label": { "type": "string", "description": "A few words on the button." },
                            "message": { "type": "string", "description": "What the answer says, when the label alone is not clear enough." },
                        },
                        "required": ["label"],
                    },
                },
                "assumed": { "type": "string", "description": "What you carry on with until the user answers; one of the options." },
            },
            "required": ["question", "options", "assumed"],
        },
    }), run!(|c| thread!(c, |t| text(ask_user(c.hub, t, c.args).await)))));
}

async fn second_opinion(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let reason = args["reason"].as_str().unwrap_or("The agent asked for one").trim().to_owned();
    let ask = ReviewRequest { reason: Some(reason), ..Default::default() };
    match hub.review(&thread.id, ask).await {
        Ok(started) => Ok(format!(
            "{} is reviewing the changes since the last review. Its findings come back to you as a message after this turn.",
            started.model
        )),
        Err(e) => Err(format!("{e:#}")),
    }
}

async fn review_design(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let doc = args["doc"].as_str().map(str::to_owned);
    match hub.design_review(&thread.id, crate::model::DesignReviewAsk { doc }, "agent").await {
        Ok(started) => Ok(format!(
            "Four fresh reviewers are reading {}. Their findings come back to you as a message after this turn; end the turn now.",
            started.doc
        )),
        Err(e) => Err(format!("{e:#}")),
    }
}

/// Puts a plugin card in the thread: the repository as GitHub has it now, and the agent's reason.
async fn suggest_plugin(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let raw = args["repo"].as_str().unwrap_or_default();
    let name = crate::plugins::repo_from_link(raw).ok_or_else(|| format!("Not a GitHub repository: {raw}. Pass owner/name."))?;
    let why: String = args["why"].as_str().unwrap_or_default().trim().chars().take(400).collect();
    let repo = crate::plugins::card(&hub.ctx(), &name).await.map_err(|e| format!("{e:#}"))?;
    let shown = repo.full_name.clone();
    hub.emit_typed(&thread.id, &crate::model::PluginSuggestedData { repo: json!(repo), why })
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(format!(
        "The card for {shown} is showing in the thread with your reason, and the user can open it from there. \
         Nothing is installed; don't repeat the card's facts in your reply."
    ))
}

/// Puts a handoff card in the thread: the tool, and the work the agent would hand it.
async fn offer_handoff(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let folder = args["folder"].as_str().map(str::trim).filter(|f| !f.is_empty()).ok_or("folder is missing: pass the tool's folder from your_tools")?;
    let why: String = args["why"].as_str().unwrap_or_default().trim().chars().take(400).collect();
    let label = hub.offer_handoff(&thread.id, folder, &why).await.map_err(|e| format!("{e:#}"))?;
    Ok(format!(
        "A \"{label}\" card is showing in the thread. Nothing is handed over until the user presses it; \
         tell them in one line, and do not run the tool yourself."
    ))
}

async fn react(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let emoji = args["emoji"].as_str().unwrap_or_default().trim();
    let on = !args["remove"].as_bool().unwrap_or(false);
    let events = hub.store.events_of(&thread.id, 0, &[crate::model::EventKind::UserMessage]).await.map_err(|e| format!("{e:#}"))?;
    let Some(seq) = crate::reactions::latest_user_message(&events) else {
        return Err("The user has not written anything here to react to.".into());
    };
    match hub.react(&thread.id, seq, emoji, on, "agent").await.map_err(|e| format!("{e:#}"))? {
        Some(_) if on => Ok(format!("{emoji} is on the user's message. Go on with the work; add words only if they help.")),
        Some(_) => Ok("Taken away again.".into()),
        None if on => Ok(format!("{emoji} was on the message already.")),
        None => Ok("It was not there.".into()),
    }
}

/// The steps an agent keeps, as the `plan` update the app's steps card folds (`transcript.ts`):
/// the same shape an ACP agent's own to-do list arrives in, so a thread whose engine has no such
/// tool shows the same card. The whole list every time, as ACP does.
pub(super) fn steps_plan(args: &Value) -> Result<Value, String> {
    let entries: Vec<Value> = args["steps"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|s| {
                    let content: String = s["content"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect();
                    let status = match s["status"].as_str().unwrap_or("pending") {
                        done @ ("completed" | "in_progress") => done,
                        _ => "pending",
                    };
                    (!content.is_empty()).then(|| json!({ "content": content, "priority": "medium", "status": status }))
                })
                .take(12)
                .collect()
        })
        .unwrap_or_default();
    if entries.is_empty() {
        return Err("Give the steps, each with a content and a status.".into());
    }
    Ok(json!({ "sessionUpdate": "plan", "entries": entries }))
}

async fn update_steps(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let plan = steps_plan(args)?;
    let n = plan["entries"].as_array().map_or(0, Vec::len);
    hub.emit(&thread.id, crate::model::EventKind::Update, plan).await.map_err(|e| format!("{e:#}"))?;
    Ok(format!("The user sees your {n} steps. Go on with the work; call update_steps again when a step starts or finishes."))
}

async fn write_tldr(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let (points, ask) = crate::gist::from_args(args)?;
    let gist = crate::gist::note(&hub.ctx(), &thread.id, points, ask).await.map_err(|e| format!("{e:#}"))?;
    hub.emit_typed(&thread.id, &gist).await.map_err(|e| format!("{e:#}"))?;
    Ok("The TL;DR is on the thread's hover card. End your turn.".into())
}

/// The replies an agent offers, cleaned up: 2 to 4 options with a short label each.
pub(super) fn replies(args: &Value) -> Result<crate::model::RepliesData, String> {
    let line = |v: &Value, max: usize| -> String {
        let text = v.as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
        text.chars().take(max).collect()
    };
    let question = line(&args["question"], 300);
    let options: Vec<crate::model::ReplyOption> = args["options"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|o| {
                    // A bare string is a label too.
                    let label = line(if o.is_string() { o } else { &o["label"] }, 60);
                    let message: String = o["message"].as_str().unwrap_or_default().trim().chars().take(2_000).collect();
                    (!label.is_empty()).then(|| crate::model::ReplyOption { message: if message.is_empty() { label.clone() } else { message }, label })
                })
                .collect()
        })
        .unwrap_or_default();
    if question.is_empty() {
        return Err("Give the question the buttons answer.".into());
    }
    if !(2..=4).contains(&options.len()) {
        return Err(format!("Give 2 to 4 options, each with a label; got {}.", options.len()));
    }
    Ok(crate::model::RepliesData { question, options })
}

async fn offer_replies(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let offer = replies(args)?;
    let n = offer.options.len();
    hub.emit_typed(&thread.id, &offer).await.map_err(|e| format!("{e:#}"))?;
    Ok(format!(
        "The {n} buttons show over the message box when your turn ends. Ask the question in your reply as well, and end your turn."
    ))
}

/// `ask_user`: logs the question as an `ask` event and answers the agent at once with its own
/// assumption. Nothing waits and the thread's status does not change.
async fn ask_user(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let asked = replies(args)?;
    let assumed: String = args["assumed"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect();
    if assumed.is_empty() {
        return Err("Say what you assume until the user answers.".into());
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let ask = crate::model::AskData { question: asked.question, options: asked.options, id: id.clone(), assumed: assumed.clone() };
    hub.emit_typed(&thread.id, &ask).await.map_err(|e| format!("{e:#}"))?;
    Ok(json!({
        "answered": false,
        "assuming": assumed,
        "note": "The question is in the chat. Carry on with your assumption and say what you assumed; a later answer reaches you as a message.",
    })
    .to_string())
}
