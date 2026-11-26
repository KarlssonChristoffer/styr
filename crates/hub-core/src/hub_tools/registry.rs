//! Every tool Hub offers agents, in the order clients see them: each one's name, description,
//! input schema and handler, grouped by what it is for and gated by the caller's access.

use super::*;

/// The tools every caller may get, as far as its access reaches.
pub fn registry(access: AgentAccess) -> Vec<Tool> {
    let mut tools = reading_tools();
    everyday_tools(&mut tools);
    if access >= AgentAccess::Supervised {
        thread_start_tools(&mut tools, access);
        thread_work_tools(&mut tools);
        problem_tools(&mut tools);
    }
    if access == AgentAccess::Full {
        bench_tools(&mut tools);
    }
    tools
}

/// The bench (`bench/run.rs`): run a suite as real threads and read its report. Full access only,
/// since a run starts many threads and spends the plan.
fn bench_tools(tools: &mut Vec<Tool>) {
    tools.push(Tool::new(
        "bench_run",
        "Starts a run of a built-in bench suite: each case's prompts as real threads in fresh folders, one at a time, \
         checked, timed and kept for comparison with Hub's earlier versions. Returns at once with the run's id; read it with \
         bench_report. Only when the user asks for a bench. Example: {\"suite\": \"smoke\"}.",
        json!({
            "suite": { "type": "string", "description": "A built-in suite: \"smoke\" (small coding tasks) or \"features\" (Hub's own features: files in the chat, a side job, a handoff, a commit on a branch)." },
            "engine": { "type": "string", "description": "An engine from hub_status. Defaults to the suite's." },
            "model": { "type": "string", "description": "A model the engine offers, or \"fast\" for its cheapest (the suite's default)." },
            "repeats": { "type": "integer", "description": "Times each case runs, 1 to 10. Defaults to the suite's." },
            "cases": { "type": "array", "items": { "type": "string" }, "description": "Only these cases, by id." },
        }),
        &["suite"],
        false,
        run!(|c| json_out(super::bench::run_tool(c.hub, c.args).await)),
    ));
    tools.push(Tool::new(
        "bench_report",
        "The report of a bench run as markdown: per case how many repeats passed, now and in the run before, times, tokens, \
         cost and a flag, with the threads of failed repeats. The latest run unless you give its id; says how far a run \
         that is still going has got. Example: {} or {\"id\": \"1794327811326-smoke\"}.",
        json!({ "id": { "type": "string", "description": "A run's id from bench_run. Defaults to the latest." } }),
        &[],
        true,
        run!(|c| tool::text(super::bench::report_tool(c.hub, c.args).await)),
    ));
}

/// Looking: status, problems, waiting.
fn reading_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "hub_brief",
            "The answer to \"what's going on?\" in one call: threads that need the user and what they ask, \
             threads working now and what they are doing, threads that finished lately with the end of their \
             reply (unread ones marked), and failed ones with their error. Use it instead of hub_status plus \
             a read of every thread. Example: {\"hours\": 24} for since yesterday.",
            json!({
                "hours": { "type": "integer", "description": "How far back finished threads count. Default 12." },
            }),
            &[],
            true,
            run!(|c| json_out(brief_tool(c.hub, c.args).await)),
        ),
        Tool::new(
            "problems",
            "Errors and warnings Hub caught, grouped (the same error is one problem with a count): from Hub's core \
             on this machine, the app window, failed threads, quiet sources, slower releases, and the user's own programs \
             that report with a Sentry SDK or OpenTelemetry, on this machine and every other one online. Newest first, with their stamps (prio, lane, category). \
             Example: {\"query\": \"is:unstamped level:error\"}. Read one with problem.",
            json!({
                "view": { "type": "string", "enum": ["needs_you", "vakten", "open", "fixed", "back", "archived", "resolved", "quiet", "slower", "all"], "description": "Default open." },
                "level": { "type": "string", "enum": ["info", "warning", "error"], "description": "The lowest level to list. Default warning." },
                "project": { "type": "string", "description": "Only this project (\"Hub\" for Hub's own)." },
                "source": { "type": "string", "enum": ["hubd", "app", "thread", "sentry", "otlp", "monitor", "metrics"] },
                "query": { "type": "string", "description": "Words and terms: is:new|unstamped|regressed|escalating|needs_you, level:, project:, source:, machine:, release:, prio:<=p1, lane:ask, category:, thread:no, times_seen:>10, lastSeen:-24h, firstSeen:+7d, -term to leave out." },
                "limit": { "type": "integer", "description": "Default 50." },
                "machine": { "type": "string", "description": "Only this machine. Default every machine online; each problem says its machine." },
            }),
            &[],
            true,
            run!(|c| json_out(problems_tool(c.hub, c.args).await)),
        ),
        Tool::new(
            "problem",
            "One problem from the problems list with its latest hits: message, stack, the log lines or breadcrumbs \
             before it, release and anything else the sender said. Example: {\"id\": 12}.",
            json!({
                "id": { "type": "integer" }, "machine": { "type": "string", "description": "The other machine the problem is on, as the list says; leave out for this one." },
                "hits": { "type": "integer", "description": "How many of the latest hits. Default 3." },
            }),
            &["id"],
            true,
            run!(|c| json_out(problem_tool(c.hub, c.args).await)),
        ),
        Tool::new(
            "problem_dossier",
            "Everything around one problem beyond its stack: the commits that last touched the files in it (suspects), \
             problems like it, what else went wrong anywhere in the minutes around it, and earlier attempts (threads, pull \
             requests, comebacks). Example: {\"id\": 12}.",
            json!({ "id": { "type": "integer" }, "machine": { "type": "string", "description": "The other machine the problem is on, as the list says; leave out for this one." } }),
            &["id"],
            true,
            run!(|c| json_out(dossier_tool(c.hub, c.args).await)),
        ),
        Tool::new(
            "where_am_i",
            "The machine, project and programs around you in one call: this computer's name, OS, CPU, home folder \
             and free disk, the programs on PATH (git, node, cargo, python, docker, ...), the other machines Hub \
             knows, and your own thread, folder and project. Use it before assuming a path style, a shell or a \
             tool exists. From outside Hub pass thread_id to get that thread's folder and project.",
            json!({
                "thread_id": { "type": "string", "description": "Only for agents outside Hub: the thread to describe." },
            }),
            &[],
            true,
            run!(|c| json_out(context_tool(c.hub, c.caller, c.args).await)),
        ),
        Tool::new(
            "hub_status",
            "Hub's inventory: thread ids by state, the projects with their folders, and the engines with the \
             models each offers. Use it to find a project or model name for start_thread or delegate; for \
             \"what's going on?\" use hub_brief.",
            json!({}),
            &[],
            true,
            run!(|c| json_out(status(c.hub).await)),
        ),
        Tool::new(
            "list_threads",
            "Finds threads, newest activity first, with id, title, folder, engine, status and host. \
             Example: {\"project\": \"atlas\", \"status\": \"running\"}.",
            json!({
                "query": { "type": "string", "description": "Only threads whose title or folder contains this." },
                "project": { "type": "string", "description": "Only threads in this project (its name or folder)." },
                "status": { "type": "string", "enum": ["idle", "running", "waiting_on_you", "error", "in_terminal"] },
                "archived": { "type": "boolean", "description": "Include archived threads. Default false." },
                "limit": { "type": "integer", "description": "At most this many. Default 20." },
            }),
            &[],
            true,
            run!(|c| json_out(list(c.hub, c.args).await)),
        ),
        Tool::new(
            "read_thread",
            "Reads a thread's conversation: what was asked, what the agent answered and, with view \
             \"activity\", its tool calls and turn ends. Without after_seq you get the latest part, which \
             is usually what you need; pass next_seq back as after_seq to read on from there.",
            json!({
                "thread_id": { "type": "string" },
                "view": { "type": "string", "enum": ["messages", "activity"], "description": "Default messages." },
                "after_seq": { "type": "integer", "description": "Read from just after this point in the log." },
            }),
            &["thread_id"],
            true,
            run!(|c| json_out(read(c.hub, c.args).await)),
        ),
        Tool::new(
            "wait_for_thread",
            "Waits until a thread's latest message has been answered, or it needs the user, then \
             returns how it ended and the reply. A wait that times out changes nothing; wait again. \
             To not wait at all, delegate instead.",
            json!({
                "thread_id": { "type": "string" },
                "timeout_s": { "type": "integer", "description": "Default 120, at most 600." },
            }),
            &["thread_id"],
            true,
            run!(|c| json_out(wait_tool(c.hub, c.args).await)),
        ),
    ]
}

/// The rest every caller gets.
fn everyday_tools(tools: &mut Vec<Tool>) {
    tools.push(Tool::new(
        "your_tools",
        "The user's own tools on this machine (Forman, Knegarn, Aftermath and anything else with a hub.toml): \
         what each says it is doing, its lists (questions waiting on the user, runs, checks) and the actions \
         run_tool_action can run on it. Use it for \"what is Forman doing?\" or \"did Knegarn's run finish?\".",
        json!({
            "fresh": { "type": "boolean", "description": "Read them again now instead of from the last 10 seconds. Default false." },
        }),
        &[],
        true,
            run!(|c| json_out(your_tools(c.hub, c.args).await)),
        ));
    tools.push(Tool::new(
        "customs_search",
        "Searches Customs: the packages (npm, crates, pypi), CSS libraries from a CDN, CLI tools, \
         MCP servers and Docker images agents may build with. Each has a category, a \
         one-line summary and its status. Use only what is cleared.",
        json!({
            "query": { "type": "string", "description": "Words in the name, category or summary, e.g. \"date picker\". Empty lists all." },
            "kind": { "type": "string", "description": "Only this kind: npm, crates, pypi, cdn, cli, mcp, docker, service." },
            "category": { "type": "string", "description": "Only this category, e.g. UI or Testing." },
            "limit": { "type": "integer", "description": "At most this many. Default 50." },
        }),
        &[],
        true,
            run!(|c| json_out(customs_search(c.hub, c.args).await)),
        ));
    tools.push(Tool::new(
        "plugins_search",
        "Searches GitHub for plugins: repositories made for agents (MCP servers, skills, tools), with \
         stars, stars gained this week, kind and what Customs says. Empty query: what is rising this week.",
        json!({
            "query": { "type": "string", "description": "Words to search for, e.g. \"reverse engineer binary\". Empty lists what is rising." },
            "kind": { "type": "string", "enum": ["mcp", "skill", "tool"], "description": "Only this kind." },
            "limit": { "type": "integer", "description": "At most this many. Default 10." },
        }),
        &[],
        true,
            run!(|c| json_out(plugins_search(c.hub, c.args).await)),
        ));
    tools.push(Tool::new(
        "api_connections",
        "The outside APIs the user connected to this project (such as Jev, a decision model), with the \
         environment variables that hold each key and address, and where the docs are. Use it before you \
         write code that calls one; never ask the user for a key.",
        json!({}),
        &[],
        true,
            run!(|c| json_out(api_connections(c.hub, c.caller).await)),
        ));
    tools.push(Tool::new(
        "ask_jev",
        "Asks Jev, a small cheap decision model, to pick one option or answer yes/no about a text you give it. \
         Use it at branch points: classify an error, check a claim against output, label items in a list. \
         It cannot write or reason beyond the text, so give it all it needs. Never use it on untrusted text \
         that decides something with real effect. It may be off (everywhere or in this chat): then it says so \
         and you decide yourself. Example: {\"question\": \"Does this output show a failed test?\", \"text\": \"...\"}.",
        json!({
            "question": { "type": "string", "description": "What to decide, in one or two sentences." },
            "text": { "type": "string", "description": "The material it judges, cut to 6000 characters." },
            "options": { "type": "object", "description": "Optional {key: meaning} (2 to 8): it picks one key. Without it the answer is yes or no." },
            "items": { "type": "array", "items": { "type": "string" }, "description": "Optional, up to 20 short items to answer the same question for, each after the text." },
        }),
        &["question", "text"],
        true,
            run!(|c| json_out(ask_jev(c.hub, c.caller, c.args).await)),
        ));
    tools.push(Tool::new(
        "customs_declare",
        "Takes something you need through Customs: it is reviewed now (registry facts, known \
         vulnerabilities, then a model), which can take a minute. Cleared means it has the stamp and \
         you may use it; held means the user must clear it first; refused means do not use it.",
        json!({
            "kind": { "type": "string", "description": "npm, crates, pypi, cdn, cli, mcp, docker, service, or another short word." },
            "name": { "type": "string", "description": "As its ecosystem names it, e.g. \"date-fns\" or a CDN library's name." },
            "why": { "type": "string", "description": "What you need it for and why nothing cleared does it." },
            "summary": { "type": "string", "description": "One line on what it does." },
        }),
        &["kind", "name", "why"],
        false,
            run!(|c| json_out(customs_declare(c.hub, c.caller, c.args).await)),
        ).ungated());
    tools.push(Tool::new(
        "list_recipes",
        "The user's recipes: saved jobs they run by hand on any project, like \"Update dependencies\" or \"Fix \
         failing tests\", with what each does, where it last ran, and suggestions still waiting for the user.",
        json!({}),
        &[],
        true,
            run!(|c| json_out(list_recipes(c.hub).await)),
        ));
    tools.push(Tool::new(
        "suggest_recipe",
        "Hands the user a recipe: a job worth saving to run again on any project with one click. Use it when the \
         user asks you to save something as a recipe, or when a job you just did is one they will want again \
         (\"bump the version and write the changelog\"). Write the prompt as a complete brief that works on any \
         project, not just this one. It waits on the Recipes page until the user keeps or dismisses it.",
        json!({
            "name": { "type": "string", "description": "Short, starting with a verb, e.g. \"Bump version and changelog\"." },
            "about": { "type": "string", "description": "One line on what it does." },
            "prompt": { "type": "string", "description": "The whole brief a fresh thread gets. Hub adds its own note (new branch, checks, stop at a pull request)." },
            "why": { "type": "string", "description": "Why it is worth keeping, for the user." },
        }),
        &["name", "prompt"],
        false,
            run!(|c| json_out(suggest_recipe(c.hub, c.caller, c.args).await)),
        ).ungated());
}

/// Starting threads and keeping ideas (supervised and up).
fn thread_start_tools(tools: &mut Vec<Tool>, access: AgentAccess) {
    tools.push(Tool::new(
        "park_idea",
        "Keeps an idea the user has, so it does not get lost: Hub puts it in the focus \
         (docs/FOCUS.md) of the project it is about, under the stage it fits or under Parked, and \
         says where. Use it when the user says they have an idea, or asks to note, park or save one \
         for later, instead of building it now. Hub guesses the project unless you give it. \
         Example: {\"idea\": \"Recipes: saved recurring jobs per project\"}.",
        json!({
            "idea": { "type": "string", "description": "The idea in the user's words, one or two lines." },
            "project": { "type": "string", "description": "A project's name or folder, when you know it. Defaults to a guess." },
        }),
        &["idea"],
        false,
        run!(|c| json_out(park_idea(c.hub, c.caller, c.args).await)),
    ));
    let settings = if access == AgentAccess::Full {
        json!({ "type": "object", "description": "Any session settings, config id to value (full access only)." })
    } else {
        Value::Null
    };
    let mut start = json!({
        "message": { "type": "string", "description": "The whole brief. The new thread sees nothing else." },
        "project": { "type": "string", "description": "A project's name or folder. Or give folder." },
        "folder": { "type": "string", "description": "An absolute folder. Defaults to your own thread's." },
        "title": { "type": "string" },
        "engine": { "type": "string", "description": "An engine from hub_status. Defaults to yours, or Hub's default." },
        "model": { "type": "string", "description": "A model the engine offers (see hub_status), e.g. \"sonnet\"." },
        "context": { "type": "string", "enum": CONTEXTS, "description": "How much of your own thread it gets besides the brief: \"brief\" (nothing, the default), \"handover\" (a summary of where your thread stands) or \"full\" (your whole conversation). Only from a Hub thread." },
        "files": { "type": "array", "items": { "type": "string" }, "description": "Files to attach, as absolute paths or paths in the folder, e.g. a review you wrote. The thread gets them as links it reads itself." },
        "wait": { "type": "boolean", "description": "Wait for the first answer. Default false." },
        "timeout_s": { "type": "integer", "description": "How long to wait. Default 120, at most 600." },
    });
    if !settings.is_null() {
        start["settings"] = settings;
    }
    let mut delegate = start.clone();
    if let Some(props) = delegate.as_object_mut() {
        props.remove("wait");
        props.remove("timeout_s");
    }
    tools.push(Tool::new(
        "start_thread",
        "Starts a new thread in a project and sends it the brief. It runs on its own like any \
         thread, and the user sees it. Returns its id; pass wait to get the first answer, or read \
         it later. Example: {\"project\": \"atlas\", \"message\": \"Fix the login redirect: ...\", \
         \"model\": \"sonnet\"}.",
        start,
        &["message"],
        false,
        run!(|c| json_out(self::start(c.hub, c.caller, c.args).await)),
    ));
    tools.push(Tool::new(
        "delegate",
        "Hands a task to a new thread and returns at once. When that thread's turn ends (done, \
         failed or stopped), its reply arrives in your thread as a message marked as from it, and \
         you get a new turn to act on it. So end your turn after delegating unless you have other \
         work. The new thread shows in the user's list like any other. Only for agents running in \
         a Hub thread. Example: {\"project\": \"atlas\", \"message\": \"Run the tests and fix \
         what fails. Report what you changed.\"}.",
        delegate,
        &["message"],
        false,
        run!(|c| json_out(self::delegate(c.hub, c.caller, c.args).await)),
    ));
    tools.push(Tool::new(
        "send_to_thread",
        "Sends a message to another thread, marked as yours. A busy thread gets it as its next \
         turn. Read the thread first so the message fits where it is.",
        json!({
            "thread_id": { "type": "string" },
            "message": { "type": "string" },
            "wait": { "type": "boolean", "description": "Wait for the answer. Default false." },
            "timeout_s": { "type": "integer", "description": "How long to wait. Default 120, at most 600." },
        }),
        &["thread_id", "message"],
        false,
        run!(|c| json_out(send(c.hub, c.caller, c.args).await)),
    ));
}

/// Working on threads (supervised and up).
fn thread_work_tools(tools: &mut Vec<Tool>) {
    tools.push(Tool::new(
        "stop_thread",
        "Stops the turn a thread is working on, like the user's Stop. Queued messages still run.",
        json!({ "thread_id": { "type": "string" } }),
        &["thread_id"],
        false,
        run!(|c| json_out(stop(c.hub, c.caller, c.args).await)),
    ));
    let own = "Your own thread unless you give thread_id.";
    tools.push(Tool::new(
        "settle_thread",
        "Settles a thread: marks it done and puts it on the Settled shelf, out of the user's list. This is what \
         the user means by \"close this thread\" or \"close this one\". It comes back by itself when someone writes \
         to it, it asks something or it fails, and the user can put it back. Settle your own thread as the last \
         thing in your turn, after you have said what you did. back: true puts a settled thread back in the list. \
         Example: {} for your own thread.",
        json!({
            "thread_id": { "type": "string", "description": own },
            "back": { "type": "boolean", "description": "Put it back in the list instead. Default false." },
        }),
        &[],
        false,
        run!(|c| json_out(manage(c.hub, c.caller, c.name, c.args).await)),
    ));
    tools.push(Tool::new(
        "rename_thread",
        "Renames a thread. The name is what the user finds it by in the list: a few words in sentence case, \
         starting with the verb of the goal, e.g. \"Fix Safari export button\". Hub stops naming it by itself.",
        json!({
            "thread_id": { "type": "string", "description": own },
            "title": { "type": "string" },
        }),
        &["title"],
        false,
        run!(|c| json_out(manage(c.hub, c.caller, c.name, c.args).await)),
    ));
    tools.push(Tool::new(
        "pin_thread",
        "Pins a thread on top of the user's list, or unpins it with pinned: false.",
        json!({
            "thread_id": { "type": "string", "description": own },
            "pinned": { "type": "boolean", "description": "Default true." },
        }),
        &[],
        false,
        run!(|c| json_out(manage(c.hub, c.caller, c.name, c.args).await)),
    ));
    tools.push(Tool::new(
        "snooze_thread",
        "Hides a thread from the user's list for some hours; it comes back then, or sooner when it needs the \
         user or fails. hours: 0 wakes it now. Example: {\"hours\": 24}.",
        json!({
            "thread_id": { "type": "string", "description": own },
            "hours": { "type": "number", "description": "How long, e.g. 3 or 24. 0 wakes it." },
        }),
        &["hours"],
        false,
        run!(|c| json_out(manage(c.hub, c.caller, c.name, c.args).await)),
    ));
    tools.push(Tool::new(
        "run_tool_action",
        "Runs one of the actions a tool from your_tools offers, such as answering one of Forman's questions or \
         nudging it. Actions marked danger (halting, stopping) need full access. Example: {\"folder\": \"/home/c/Forman\", \
         \"action\": \"answer\", \"item\": \"q-20260909-001\", \"inputs\": {\"text\": \"Use python:3.12-slim.\"}}.",
        json!({
            "folder": { "type": "string", "description": "The tool's folder, as your_tools gives it." },
            "action": { "type": "string", "description": "The action's id." },
            "item": { "type": "string", "description": "For an action on a list: the id of the item it acts on." },
            "inputs": { "type": "object", "description": "What the action asks for, name to text." },
        }),
        &["folder", "action"],
        false,
        run!(|c| json_out(run_tool_action(c.hub, c.caller, c.args).await)),
    ));
    tools.push(Tool::new(
        "run_recipe",
        "Runs one of the user's recipes (see list_recipes) on a project: a new thread does the job and ends at \
         an open pull request. Returns the thread. Example: {\"recipe\": \"Update dependencies\", \"project\": \"atlas\"}.",
        json!({
            "recipe": { "type": "string", "description": "The recipe's name or id." },
            "project": { "type": "string", "description": "A project's name or folder. Or give folder." },
            "folder": { "type": "string", "description": "An absolute folder. Defaults to your own thread's." },
            "note": { "type": "string", "description": "Anything to add this time, e.g. \"only the api crate\"." },
        }),
        &["recipe"],
        false,
        run!(|c| json_out(run_recipe(c.hub, c.caller, c.args).await)),
    ));
}

/// Sorting and fixing problems (supervised and up).
fn problem_tools(tools: &mut Vec<Tool>) {
    tools.push(Tool::new(
        "stamp_problem",
        "Sorts a problem: prio (P0 down or data lost, P1 security or hits the user, P2 can wait, P3 clutter), lane (quiet: \
         fix silently; tell: fix and tell the user; ask: plan and wait for the user), category (bug, config, environment, \
         external, flaky, performance, security, noise, expected), confidence, fixable, and a note the user reads first \
         (what, cause, plan, next, and the fix contract's repro and done; a field you leave out keeps what it said). \
         Give why. Example: {\"id\": 12, \"prio\": \"P2\", \"lane\": \"tell\", \"category\": \"bug\", \"confidence\": \"high\", \
         \"fixable\": \"yes\", \"note\": {\"what\": \"...\", \"cause\": \"...\", \"plan\": \"...\", \"next\": \"...\"}, \"why\": \"...\"}.",
        json!({
            "id": { "type": "integer" }, "machine": { "type": "string", "description": "The other machine the problem is on, as the list says; leave out for this one." },
            "prio": { "type": "string", "enum": ["P0", "P1", "P2", "P3"] },
            "lane": { "type": "string", "enum": ["quiet", "tell", "ask"] },
            "category": { "type": "string" },
            "confidence": { "type": "string", "enum": ["low", "medium", "high"] },
            "fixable": { "type": "string", "enum": ["yes", "no", "unknown"] },
            "note": { "type": "object", "properties": { "what": { "type": "string" }, "cause": { "type": "string" }, "plan": { "type": "string" }, "next": { "type": "string" }, "repro": { "type": "string" }, "done": { "type": "string" } } },
            "why": { "type": "string" },
        }),
        &["id"],
        false,
        run!(|c| json_out(change_problem(c.hub, c.caller, c.args, true).await)),
    ));
    tools.push(Tool::new(
        "change_problem",
        "Resolves, archives, opens or merges a problem. resolve: \"now\" or \"next_release\" (hits from the releases it was \
         seen in do not bring it back). archive: {\"until\": \"worse\"} (back when it happens clearly more), \
         {\"until\": \"count\", \"count\": 100}, {\"until\": \"time\", \"at\": <unix ms>} or {\"until\": \"forever\"}. \
         merge_into: another problem's id that is the same error. Always give why. Example: {\"id\": 12, \"merge_into\": 9, \"why\": \"same ssh timeout\"}.",
        json!({
            "id": { "type": "integer" }, "machine": { "type": "string", "description": "The other machine the problem is on, as the list says; leave out for this one." },
            "resolve": { "type": "string", "enum": ["now", "next_release"] },
            "archive": { "type": "object" },
            "status": { "type": "string", "enum": ["open"] },
            "merge_into": { "type": "integer" },
            "unmerge": { "type": "boolean" },
            "why": { "type": "string" },
        }),
        &["id", "why"],
        false,
        run!(|c| json_out(change_problem(c.hub, c.caller, c.args, false).await)),
    ));
    tools.push(Tool::new(
        "problem_thread",
        "Hands a problem to a fix thread of its own, in the problem's project, with its brief and your contract: repro (how \
         to make it happen), cause (what you believe is wrong), done (what must be true when it is fixed, checkable) and \
         plan. It runs alongside the others and ends at an open pull request; you do not wait for it. Refused for an \
         ask-first problem, one that came back after a fix, and one with a thread on it unless you start it over with \
         again: true after its thread reported back (at most 3 threads per problem). There is no daily cap. Example: \
         {\"id\": 12, \"repro\": \"Open Aftermath with an empty sheet.\", \"cause\": \"parse_rows indexes row 0 of an empty list.\", \
         \"done\": \"An empty sheet opens with no KeyError; a test covers it; the project's tests pass.\", \"plan\": \"Guard the empty list in parse_rows and add a test.\"}.",
        json!({
            "id": { "type": "integer" }, "machine": { "type": "string", "description": "The other machine the problem is on, as the list says; leave out for this one." },
            "repro": { "type": "string" },
            "cause": { "type": "string" },
            "done": { "type": "string" },
            "plan": { "type": "string" },
            "size": { "type": "string", "enum": ["small", "big"], "description": "How big the fix is: \"small\" (the default for most) runs on a small model, \"big\" (several files, a subtle cause, a regression) on a mid-sized one. Fix threads never run on the largest model." },
            "again": { "type": "boolean", "description": "Start the problem over with a new thread and your new plan, after its fix thread reported back blocked, failed or without a pull request. The thread before is put away." },
        }),
        &["id", "repro", "cause", "done", "plan"],
        false,
        run!(|c| json_out(problem_thread(c.hub, c.caller, c.args).await)),
    ));
    tools.push(Tool::new(
        "tell_user",
        "One line about what you did (the tell lane, or the weekly summary). It shows on the Problems page and in the \
         problem's activity, and is NOT a notification: the user does not want notices from Guard. Set critical only when \
         something is down, data is at risk or a security hole is open and the user must know now. \
         Example: {\"id\": 12, \"text\": \"Fixed the KeyError in Aftermath's parser, PR #31.\"}.",
        json!({ "id": { "type": "integer" }, "machine": { "type": "string", "description": "The other machine the problem is on, as the list says; leave out for this one." }, "text": { "type": "string" }, "critical": { "type": "boolean", "description": "Also a desktop notification. Only for what is absolutely critical; default false." } }),
        &["text"],
        false,
        run!(|c| json_out(tell_tool(c.hub, c.args).await)),
    ));
    tools.push(Tool::new(
        "tool_review",
        "Saves your review of the agent tools that fail most, which Hub hands you once a day: a short text for the user              (what fails most, why, what you did, what waits for them) and one finding per tool, with its cause (agent,              environment, hub, external, noise), what you did or suggest, and whether it is worth fixing. It replaces the              last review on the Problems page. Example: {\"text\": \"Bash fails most on tincan: cargo is missing there...\",              \"findings\": [{\"engine\": \"<engine id>\", \"tool\": \"Bash\", \"machine\": \"tincan\", \"cause\": \"environment\",              \"action\": \"Install Rust on tincan (asked the user)\", \"verdict\": \"waits\", \"problem\": 41}]}.",
        json!({
            "text": { "type": "string" },
            "findings": { "type": "array", "items": { "type": "object", "properties": {
                "engine": { "type": "string" }, "tool": { "type": "string" }, "machine": { "type": "string" },
                "cause": { "type": "string", "enum": ["agent", "environment", "hub", "external", "noise"] },
                "action": { "type": "string" },
                "verdict": { "type": "string", "enum": ["worth", "skip", "waits"] },
                "problem": { "type": "integer" },
            }, "required": ["engine", "tool", "cause", "action"] } },
        }),
        &["text", "findings"],
        false,
        run!(|c| json_out(review_tool(c.hub, c.args).await)),
    ));
}

/// The same tools as MCP definitions.
pub fn tools(access: AgentAccess) -> Vec<Value> {
    registry(access).into_iter().map(|t| t.def).collect()
}
