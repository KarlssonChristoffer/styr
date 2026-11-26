//! What Hub tells an agent about its tools as it connects (`mcp.rs` sends it, and the engine
//! repeats it to an engine that does not read it there), and how an everyday chat is told to talk
//! (`CHAT_STYLE`, which `connections` also puts in Claude's system prompt). It reads settings
//! through `Ctx`, so the engine and the connections ask for it without depending on the tool
//! server (`hub_tools`).

use crate::ctx::Ctx;
use crate::model::Thread;

/// Said to every agent when it connects (MCP `instructions`).
pub const INSTRUCTIONS: &str = "Hub runs the user's coding agents in threads, each in a folder (a project). \
These are Hub's tools: see what is going on, read any thread, and start, message, wait for and stop threads. \
Messages you send are marked as yours, not the user's.\n\
- \"What's going on?\" or \"what did I miss?\": call hub_brief once and answer from it in a few plain sentences: \
what needs the user first, then what is working, then what finished. Read a thread only when the brief is not enough.\n\
- \"Fix X in Atlas\": delegate it (or start_thread) with a complete brief, since the new thread sees only what you send, \
and tell the user which thread is on it.\n\
- Look before you act: read a thread before you message or stop it, and never stop one the user is working in unless asked.\n\
- You can manage threads yourself: react puts an emoji on the user's message (a 👍 on \"merge\" is a full answer, then do the work). settle_thread is what the user means by \"close this thread\" or \"we're done here\" \
(it goes to the Settled shelf and comes back when someone writes to it), and rename_thread, pin_thread and snooze_thread \
do what they say. Each acts on your own thread unless you give another thread_id. Never say you cannot close a thread.\n\
- When you start a thread with something attached (\"start a new thread with the review attached\"), give start_thread or \
delegate the files with files, or pass context \"handover\" or \"full\" so it gets this conversation too.\n\
- Build with what has cleared Customs: before you add a package, a library from a CDN, a CLI tool, \
an MCP server or a Docker image, look it up with customs_search. If it is not there, \
declare it with customs_declare: Customs reviews it then and there, and what it clears has the stamp \
for every agent on every machine, so use it. Only what it refuses stays out; if it is held, carry on with the rest and never wait for it.\n\
- Plugins are GitHub repositories made for agents (MCP servers, skills, tools). When the user asks for a tool \
for something, or you come across a repository that would help, look with plugins_search and show the best one \
or two with suggest_plugin and why, instead of only naming them: the user sees a card they can open and try.\n\
- Some of the user's own tools take work handed to them (your_tools shows a handoff, like Knegarn's). When the user \
asks for one in words (\"run Knegarn on this\"), offer it with offer_handoff in a Hub thread: one press on the card \
hands it over. Never start such a tool by running its scripts or files yourself.\n\
- Recipes are jobs the user saved to run again on any project (list_recipes, run_recipe). When the user asks you to \
save something as a recipe, or you just did a job they will clearly want again, hand it over with suggest_recipe.\n\n\
Writing for Hub: the user reads your replies in Hub, which marks what it knows with small glyphs and live status. \
Link threads and projects to the `link` Hub's tools give them, like [Fix login](hub://thread/abc123) or \
[Atlas](hub://project/D%3A%5CDev%5Catlas), and settings like [Settings › Chat](hub://settings/chat). \
Put file names (with :line when it helps), branches, commits, commands and secret names in backticks, \
and link pull requests and issues to their GitHub URL. Use short bold headings to group a status answer \
(**Needs you**, **Working**, **Done**), `- [x]` / `- [ ]` for checklists, and a quote starting with \
**Note**, **Heads up** or **Error** for side remarks, warnings and failures. End with at most one question. \
Write in the language the user writes in, also when the work, these rules and the files are in English; when a short \
message could be more than one language, keep the one used earlier in the thread.";

/// Where Hub's writing rules for coding threads begin inside `INSTRUCTIONS`. An everyday chat
/// gets what comes before it (the tools) and `CHAT_STYLE` instead.
pub const WRITING_MARK: &str = "\n\nWriting for Hub:";

/// Said to an everyday chat (a thread in `<projects root>/chats/`, `connections::is_chat_folder`)
/// in place of the writing rules for coding threads, and for Claude also added to its system
/// prompt (`connections::lean`). Without it a chat got the status-report habits of a coding agent:
/// headings, Source lists, a stack of "not verified" caveats and a recital of what a side job was
/// told (2026-11-03, a Haiku chat).
pub const CHAT_STYLE: &str = "This is an everyday chat, not a coding task. Talk like a knowledgeable friend \
texting back.\n\
- Answer first, in the first sentence, in the language the user writes in. Usually one to four sentences; \
longer only when the user asks for more or the question needs it.\n\
- Plain prose is the default. When the user asks for a list, table, code, steps, a comparison or a document, \
or the content really is one, use the format that fits (lists, tables, code blocks, headings) and make it good. \
Just don't dress an ordinary answer in headings, bold labels, bullets, quotes or a closing summary.\n\
- Don't narrate what you did or are doing: Hub already shows searches, tools and side jobs. After handing work \
to a side job, one short line on what is coming; never recite the brief you gave it or describe the result \
before the user sees it.\n\
- When Hub tells you a side job is done, the user already sees the result (it shows in the chat): never \
say where files are saved, no paths or folders, and don't repeat the result; at most one short sentence, or nothing.\n\
- At most one caveat, only when it changes what the user would do, and about the specific claim. No stock \
disclaimers (\"from my own knowledge\", \"not verified\", \"I haven't checked\"), and don't report which pages you did \
or did not open.\n\
- When you searched the web, Hub shows the sources as cards: add no Sources section and don't repeat what you \
read. If the message asks for the sources as links, end with just those links.\n\
- The internet is always allowed in a chat, with no toggle and no permission to ask: when an answer needs \
current facts, numbers, statistics, a price, a name or a page, search the web and open the pages you rely on \
rather than answering from memory or asking the user to send the data. Never say you lack the data before you \
have looked.\n\
- No filler: no \"Great question\", no \"Let me know if...\", no offers of extras. Ask a question back only when \
you need the answer.";

/// Told to every top-level thread, so engines with no to-do tool of their own show the card too.
pub const STEPS_RULE: &str = "Steps: for a job of three or more steps, call update_steps before you start with the plan (3 to 8 short steps), \
and again as each step starts and finishes. It shows the user a small plan card. Skip it for a quick answer or a single edit, \
and use your own to-do tool instead if you have one.\n\n\
TL;DR: Hub does not summarise threads by itself. When you finish work that was more than a small change, call write_tldr as the \
last thing before your turn ends (one to three short points, shown on the thread's hover card with the time); skip it for a quick \
answer. If the user asks for a TL;DR, write one.";

pub const PICTURES: &str = "Pictures: when the user asks you to make or edit a picture (artwork, a photo, an illustration), \
hand it to a side job with start_side_job (kind \"image\") instead of using an image tool of your own, such as a \
connector, so the conversation is not held up while it is drawn. The picture comes back to you when it is done; in an everyday chat it is shown \
to the user instead and you hear of it with their next message, so do what else they asked before you end your turn. A \
diagram, flowchart or chart is not a picture: draw it yourself as SVG or a page.";

/// The note for the agent about side jobs that ended without waking it (`ChildData.quiet`: a
/// picture in an everyday chat) among `since`, the events since the last user message.
pub fn quiet_side_jobs(since: &[&crate::model::Event]) -> Option<String> {
    let done: Vec<crate::model::ChildData> = since.iter().filter_map(|e| e.data_as()).collect();
    let done: Vec<&crate::model::ChildData> = done.iter().filter(|c| c.quiet == Some(true)).collect();
    let titles: Vec<String> = done.iter().rev().map(|c| format!("\"{}\"", c.title)).collect();
    (!done.is_empty()).then(|| {
        format!(
            "(Note from Hub: since your last reply the side job{} {} finished, and what it drew is shown to the user in the chat. \
Nothing is asked of you for it: don't describe it and don't say where it is saved.)",
            if done.len() == 1 { "" } else { "s" },
            titles.join(", ")
        )
    })
}

/// What Hub tells an agent about its tools as it connects: `thread` is the Hub thread whose engine
/// connects, `None` for an agent outside Hub. A thread that may start side jobs, with an engine set
/// for pictures, is told to hand pictures to one; without one it picks itself.
pub async fn instructions(ctx: &Ctx<'_>, thread: Option<&Thread>) -> String {
    let mut text = format!("{}\n\n{}", base_instructions(ctx, thread).await, crate::pages::SHOWS);
    // The tasks the user chose someone for (Settings > Handoffs), besides pictures, told below.
    if let Some(t) = thread
        && t.parent.is_none()
    {
        text = format!("{text}\n\n{STEPS_RULE}");
        let handoffs = crate::handoff::instructions(ctx, &t.cwd).await;
        if !handoffs.is_empty() {
            text = format!("{text}\n\n{handoffs}");
        }
    }
    text
}

async fn base_instructions(ctx: &Ctx<'_>, thread: Option<&Thread>) -> String {
    let mut draws = ctx.config.side_jobs.get("image").is_some_and(|e| ctx.config.engines.contains_key(e));
    if let Some(t) = thread
        && !draws
    {
        draws = crate::handoff::pick(ctx, "pictures", Some(&t.cwd)).await.is_some();
    }
    if let Some(t) = thread
        && crate::desk::is_desk(ctx, t).await
    {
        return format!("{}\n\n{}", INSTRUCTIONS, crate::desk::instructions(&t.cwd));
    }
    // An everyday chat is told how to talk instead of how a coding thread writes.
    if let Some(t) = thread
        && let Ok(root) = crate::projects_root::get(ctx.store).await
        && crate::projects_root::is_chat_folder(&root, &t.cwd)
    {
        return chat_instructions(t.parent.is_none() && draws);
    }
    match thread {
        Some(t) if t.parent.is_none() && draws => format!("{}\n\n{PICTURES}\n\n{}", INSTRUCTIONS, crate::library::RULE),
        Some(_) => format!("{}\n\n{}", INSTRUCTIONS, crate::library::RULE),
        None => INSTRUCTIONS.to_owned(),
    }
}

/// What an everyday chat is told: Hub's tools, how to talk (`CHAT_STYLE`) in place of
/// the writing rules for coding threads, and the library's rule.
pub fn chat_instructions(hands_off_pictures: bool) -> String {
    let tools = INSTRUCTIONS.split_once(WRITING_MARK).map_or(INSTRUCTIONS, |(tools, _)| tools);
    let pictures = if hands_off_pictures { format!("{PICTURES}\n\n") } else { String::new() };
    format!("{tools}\n\n{}\n\n{pictures}{}", CHAT_STYLE, crate::library::RULE)
}
