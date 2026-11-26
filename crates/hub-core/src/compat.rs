//! What keeps Hub's versions talking to each other: a phone, a window on another machine's Hub
//! and a core relaying another host's threads each run a different build of Hub from the hubd
//! they reach. The rules are in ARCHITECTURE.md ("Versions that talk to each other"); this file
//! holds the parts code can check.
//!
//! - `CORE_CALLS` is the contract every client may rely on forever. A call is added here only
//!   when it can never need to change, and never removed.
//! - `FEATURES` names what a hubd has beyond that core. `/api/health` lists them, so a client
//!   asks once what is there and hides or disables only the rest, instead of calling and
//!   reading a 403 or 404 as "needs updating". Names are never reused for something else.
//! - `thread_from_wire` and `stream_from_wire` read what a newer hubd sent: a value this build
//!   does not know turns into the nearest thing it does, and one unreadable thread never costs
//!   the whole list.

use serde_json::Value;

use crate::model::{StreamMsg, Thread};

/// The core contract's version. It does not move: a change that would break a client is a new
/// feature next to the old call, never an edit of it.
pub const CORE: u32 = 1;

/// Method and path of a call every client may rely on. `{id}` stands for a thread id.
pub type Call = (&'static str, &'static str);

/// Threads, live updates and answering: what a client needs to follow and steer its threads.
/// Append-only (`tests/it/compat.rs` keeps a copy and fails when one goes missing).
pub const CORE_CALLS: &[Call] = &[
    ("GET", "/api/health"),
    ("GET", "/api/threads"),
    ("GET", "/api/threads/{id}"),
    ("GET", "/api/threads/{id}/events"),
    ("GET", "/api/stream"),
    ("POST", "/api/threads/{id}/messages"),
    ("POST", "/api/threads/{id}/cancel"),
    ("POST", "/api/threads/{id}/permissions/{request}"),
];

/// A thing a hubd can do beyond the core, under a name clients ask for.
pub struct Feature {
    pub name: &'static str,
    /// What a client gets from it, for the docs and for whoever reads `/api/health`.
    pub what: &'static str,
    /// The calls it adds. Those of a `phone.` feature must be in `phone::may`.
    pub calls: &'static [Call],
}

/// Everything beyond the core. Add one line with the code that adds the calls; never rename or
/// drop one, since an old client may still ask for it by name.
pub const FEATURES: &[Feature] = &[
    Feature {
        name: "problems.analysis",
        what: "record code-analysis runs and findings; verify disappearance with a comparable complete scan",
        calls: &[("GET", "/api/problems/analysis/runs"), ("POST", "/api/problems/analysis/runs"),
            ("GET", "/api/problems/analysis/runs/{id}"), ("POST", "/api/problems/analysis/runs/{id}/finish")],
    },
    Feature {
        name: "status.shadow",
        what: "how often the status the event log implies differed from the thread's own, and the latest differences",
        calls: &[("GET", "/api/status-shadow"), ("DELETE", "/api/status-shadow")],
    },
    Feature {
        name: "usage.log",
        what: "count which parts of Hub are used (names and times only), show them as a map and clear or switch the log off",
        calls: &[("GET", "/api/uselog"), ("POST", "/api/uselog"), ("DELETE", "/api/uselog"), ("PUT", "/api/uselog/off")],
    },
    Feature {
        name: "update.alive",
        what: "the pages of a new version tell the core they rendered, which ends its probation; without it a version is not rolled back for a blank window",
        calls: &[("POST", "/api/update/alive")],
    },
    Feature {
        name: "update.rollback",
        what: "a version that fails its probation is replaced by the one before it; the update status says so (`rolled_back`) until dismissed",
        calls: &[("DELETE", "/api/update/rolled-back")],
    },
    Feature {
        name: "update.checks",
        what: "run another machine's tests for an update, so it builds without testing in the same memory",
        calls: &[("GET", "/api/checks"), ("POST", "/api/checks/prepare"), ("POST", "/api/checks/source"), ("POST", "/api/checks/run")],
    },
    Feature {
        name: "sim.view",
        what: "see a Mac's iPhone simulator: a picture of it, or its screen live",
        calls: &[("GET", "/api/simview"), ("GET", "/api/simview/frame"), ("POST", "/api/simview/live"), ("POST", "/api/simview/boot")],
    },
    Feature {
        name: "sim.touch",
        what: "touch a Mac's iPhone simulator that is watched live: taps, swipes and the home button",
        calls: &[("POST", "/api/simview/input")],
    },
    Feature {
        name: "sim.app",
        what: "start Hub's own iPhone app in a Mac's simulator already paired, open a thread in it or a tab",
        calls: &[("POST", "/api/simview/app")],
    },
    Feature {
        name: "sim.shutdown",
        what: "shut a Mac's iPhone simulator down from Hub (starting one is `sim.view`'s boot)",
        calls: &[("POST", "/api/simview/shutdown")],
    },
    Feature {
        name: "phone.threadpicture",
        what: "show a picture an agent named by path in its answer, from the thread's own folder",
        calls: &[("GET", "/api/phone/threads/{id}/picture")],
    },
    Feature {
        name: "phone.threadpage",
        what: "run an HTML file an agent named by path in its answer as a live page, from the thread's own folder",
        calls: &[("POST", "/api/phone/threads/{id}/page")],
    },
    Feature { name: "view.sheet", what: "read a spreadsheet (.xlsx, .xlsm, .xls, .ods) as one table per sheet", calls: &[("GET", "/api/project/sheet")] },
    Feature { name: "view.database", what: "read a SQLite file (.db, .sqlite, .sqlite3) as its tables, columns, row counts and first rows", calls: &[("GET", "/api/project/database")] },
    Feature { name: "pages.file", what: "run an HTML file of a project as a live page, so its scripts draw", calls: &[("POST", "/api/project/page")] },
    Feature { name: "phone.halt", what: "see and press the panic button", calls: &[("GET", "/api/halt"), ("POST", "/api/halt/pause"), ("POST", "/api/halt/resume")] },
    Feature { name: "phone.ideas", what: "keep an idea", calls: &[("POST", "/api/ideas")] },
    Feature { name: "phone.machines", what: "follow the other machines directly", calls: &[("POST", "/api/phone/machines")] },
    Feature { name: "speed.timings", what: "where the time of an answer goes", calls: &[("GET", "/api/timings"), ("GET", "/api/threads/{id}/timings")] },
    Feature {
        name: "tools.facts",
        what: "take the facts Hub writes about a finished stretch of a thread into a tool of yours that asks for them (Aftermath)",
        calls: &[("GET", "/api/tools/facts"), ("POST", "/api/tools/facts")],
    },
    Feature {
        name: "tools.allow",
        what: "approve or take back the folders whose hub.toml may run, and see the ones waiting for approval",
        calls: &[("GET", "/api/tools/unapproved"), ("POST", "/api/tools/allow"), ("DELETE", "/api/tools/allow")],
    },
    Feature { name: "package.ledger", what: "what Hub sent to an AI: a slip per package, and the totals per day and purpose", calls: &[("GET", "/api/package/slips"), ("GET", "/api/package/summary")] },
    Feature {
        name: "chat.reactions",
        what: "an emoji on a message, from you or the agent, your own emojis, and the ones you use most",
        calls: &[("POST", "/api/threads/{id}/react"), ("GET", "/api/emoji"), ("PUT", "/api/emoji/{name}"), ("DELETE", "/api/emoji/{name}")],
    },
    Feature {
        name: "handoff.tasks",
        what: "who does which kind of task (pictures, plugin vetting, research...), per chats and dev threads, and the note in the chat when Hub ran one",
        calls: &[("GET", "/api/handoff-prefs"), ("PUT", "/api/handoff-prefs")],
    },
    Feature {
        name: "customs.github",
        what: "put a GitHub repository through Customs (age, stars, owner, licence, install scripts, README and code read by a model) and get it back Cleared, Held or Refused",
        calls: &[("POST", "/api/customs/repo")],
    },
    Feature { name: "chat.aside", what: "what Hub did with a message written to a working thread, or its answer to a question", calls: &[("GET", "/api/threads/{id}/aside")] },
    Feature {
        name: "apis.agent",
        what: "agents asking the decision model (`ask_jev`), with a switch for all agents (the `agent` feature) and one per thread",
        calls: &[("GET", "/api/threads/{id}/jev"), ("PUT", "/api/threads/{id}/jev")],
    },
    Feature { name: "apis.tune", what: "a brief of the decision model's latest decisions, to tune it from", calls: &[("POST", "/api/apis/tune")] },
    Feature {
        name: "apis.tune.run",
        what: "a tune of the decision model in the background, its suggestions, and using or skipping one",
        calls: &[("POST", "/api/apis/tune/run"), ("GET", "/api/apis/tune/state"), ("POST", "/api/apis/tune/answer")],
    },
    Feature { name: "phone.peek", what: "a thread's card on an inbox row", calls: &[("GET", "/api/threads/{id}/peek")] },
    Feature { name: "phone.tidy", what: "settle, snooze, pin, archive or mark seen", calls: &[("POST", "/api/threads/{id}/tidy")] },
    Feature { name: "phone.threads", what: "start an ordinary thread", calls: &[("POST", "/api/phone/threads")] },
    Feature {
        name: "notices.post",
        what: "notices from outside Hub (a widget, a script, a server): post one, read the latest, and follow them on the stream",
        calls: &[("GET", "/api/notices"), ("POST", "/api/notices")],
    },
    Feature {
        name: "phone.notify",
        what: "notices through ntfy",
        calls: &[("GET", "/api/phone/notify"), ("PUT", "/api/phone/notify"), ("POST", "/api/phone/notify/test")],
    },
    Feature { name: "phone.changes", what: "review what a thread changed", calls: &[("GET", "/api/threads/{id}/changes")] },
    Feature {
        name: "phone.git",
        what: "a thread's git line: state, branches, drafted message, commit and push, a pull request's diff",
        calls: &[
            ("GET", "/api/threads/{id}/gitstate"),
            ("GET", "/api/threads/{id}/branches"),
            ("POST", "/api/threads/{id}/gitdraft"),
            ("POST", "/api/threads/{id}/ship"),
            ("GET", "/api/github/diff"),
        ],
    },
    Feature {
        name: "phone.engines",
        what: "pick engine, provider, model and effort before a thread starts",
        calls: &[("GET", "/api/phone/engines"), ("POST", "/api/phone/warm")],
    },
    Feature {
        name: "phone.bring",
        what: "carry a project to a machine that lacks it, from the new-thread page",
        calls: &[("POST", "/api/phone/bring")],
    },
    Feature {
        name: "phone.files",
        what: "browse and read a project's files, pictures and PDFs, and search them",
        calls: &[
            ("GET", "/api/phone/files/tree"),
            ("GET", "/api/phone/files/file"),
            ("GET", "/api/phone/files/raw"),
            ("GET", "/api/phone/files/search"),
        ],
    },
    Feature {
        name: "phone.projectgit",
        what: "a project's repo at a glance: its default branch with CI, open pull requests and recent branches",
        calls: &[("GET", "/api/phone/project/git")],
    },
    Feature {
        name: "phone.projectdocs",
        what: "a project's markdown docs: the pinned ones and the ones changed last",
        calls: &[("GET", "/api/phone/project/docs")],
    },
    Feature {
        name: "phone.projectbacklog",
        what: "a project's backlog, ideas parked in docs/BACKLOG.md: read, add, drop, or put one on the board",
        calls: &[("GET", "/api/phone/project/backlog"), ("POST", "/api/phone/project/backlog")],
    },
    Feature {
        name: "phone.projectboard",
        what: "a project's board in docs/BOARD.md: To do, Doing and Done, move and add cards",
        calls: &[("GET", "/api/phone/project/board"), ("POST", "/api/phone/project/board")],
    },
    Feature {
        name: "phone.watch",
        what: "the problems Hub found (with a fix thread) and the routines (with a run now)",
        calls: &[
            ("GET", "/api/problems"),
            ("GET", "/api/problems/{problem}"),
            ("POST", "/api/problems/{problem}/thread"),
            ("GET", "/api/routines"),
            ("POST", "/api/routines/{id}/run"),
            ("POST", "/api/routines/{id}/enabled"),
            ("PATCH", "/api/problems/{problem}"),
        ],
    },
    Feature { name: "phone.stats", what: "usage over time", calls: &[("GET", "/api/stats")] },
    Feature { name: "phone.devices", what: "the devices let in, read only", calls: &[("GET", "/api/devices")] },
    Feature {
        name: "phone.hosts",
        what: "machines, their rings and checks, updating hubd",
        calls: &[
            ("GET", "/api/hosts"),
            ("GET", "/api/host"),
            ("GET", "/api/host/about"),
            ("GET", "/api/machine/ready"),
            ("GET", "/api/updates"),
            ("GET", "/api/usage"),
            ("POST", "/api/hosts/{name}/update"),
            ("POST", "/api/hosts/{name}/reconnect"),
        ],
    },
    Feature {
        name: "phone.projects",
        what: "projects and their focus, adding one",
        calls: &[("GET", "/api/projects"), ("GET", "/api/project"), ("POST", "/api/project/focus/summaries"), ("POST", "/api/projects")],
    },
    Feature { name: "project.rules", what: "the rules a project's agents are given, with when each last changed", calls: &[("GET", "/api/project/rules")] },
    Feature { name: "phone.now", what: "what works and what waits on you, with the plan limits, in one small answer", calls: &[("GET", "/api/phone/now")] },
    Feature { name: "phone.scratch", what: "start a thread with no project, in a fresh scratch folder", calls: &[("POST", "/api/phone/threads")] },
    Feature { name: "phone.chat", what: "start an everyday chat: no project, in its own chats folder", calls: &[("POST", "/api/phone/threads")] },
    Feature { name: "phone.pickbyname", what: "a model or thinking level picked by its name (`~terra`) before the thread has run", calls: &[("POST", "/api/phone/threads")] },
    Feature { name: "phone.focus", what: "tick a box in FOCUS.md", calls: &[("POST", "/api/phone/focus")] },
    Feature {
        name: "phone.library",
        what: "what a project's threads made: pictures and docs",
        calls: &[("GET", "/api/project/outputs"), ("GET", "/api/project/outputs/raw"), ("GET", "/api/project/outputs/text")],
    },
    Feature {
        name: "phone.settings",
        what: "Safety and when threads settle",
        calls: &[("GET", "/api/prefs/permissions"), ("PUT", "/api/prefs/permissions"), ("GET", "/api/prefs/threads"), ("PUT", "/api/prefs/threads")],
    },
    Feature { name: "phone.chatdetail", what: "how much of the agent's work a thread shows", calls: &[("GET", "/api/prefs/chatdetail"), ("PUT", "/api/prefs/chatdetail")] },
    Feature {
        name: "phone.github",
        what: "a thread's pull request: read, approve, merge",
        calls: &[("GET", "/api/github/watches"), ("GET", "/api/github/pull"), ("GET", "/api/github/pulls"), ("POST", "/api/github/action")],
    },
    Feature {
        name: "github.card",
        what: "the small card that shows a pull request when its number is hovered in a chat",
        calls: &[("GET", "/api/github/card")],
    },
    Feature {
        name: "github.repos",
        what: "every repository of the GitHub account this machine is signed in as, and signing in from Hub (gh's device login)",
        calls: &[("GET", "/api/github/repos"), ("GET", "/api/github/connect"), ("POST", "/api/github/connect"), ("DELETE", "/api/github/connect")],
    },
    Feature {
        name: "github.account",
        what: "who the GitHub sign-in on a machine is (name, picture, scopes), a sign-in GitHub refuses (`lost`), and signing out",
        calls: &[("POST", "/api/github/disconnect")],
    },
    Feature { name: "sign_ins.pending", what: "sign-ins waiting for a code on the other machines", calls: &[("GET", "/api/sign-ins/pending")] },
    Feature {
        name: "phone.move",
        what: "the thread menu: rename, engine, its settings and queue, move to another machine",
        calls: &[
            ("PATCH", "/api/threads/{id}"),
            ("POST", "/api/threads/{id}/settings"),
            ("PATCH", "/api/threads/{id}/queue/{seq}"),
            ("DELETE", "/api/threads/{id}/queue/{seq}"),
            ("POST", "/api/threads/{id}/queue/{seq}/steer"),
            ("POST", "/api/threads/move"),
        ],
    },
    Feature {
        name: "dev.live",
        what: "Live mode in the desktop window: the open pull requests that change only the pages, served with hot reload",
        calls: &[("POST", "/api/dev/preview"), ("GET", "/api/dev")],
    },
    Feature {
        name: "project.worktrees",
        what: "give each new thread in a project a git worktree of its own (a switch on the project page)",
        calls: &[("GET", "/api/projects/trees"), ("PUT", "/api/projects/trees")],
    },
    Feature {
        name: "project.templates",
        what: "start a new project from a template the core writes (`template` on the new project), a public repository (`public`) and a build-and-test workflow (`ci`), and which of the tools the stacks need this machine has",
        calls: &[("GET", "/api/projects/templates"), ("POST", "/api/projects")],
    },
    Feature {
        name: "limit.switch",
        what: "move a thread that waits for a plan limit to another engine and carry on there (action `switch`), and the engine the wait offers",
        calls: &[("POST", "/api/threads/{id}/quota-wait")],
    },
    Feature {
        name: "engines.pairs",
        what: "a harness paired with a provider's key (Claude Code on DeepSeek): the catalog's `pairs`, added from Settings > Engines and picked in the model picker beside the same model in another harness",
        calls: &[("GET", "/api/engines"), ("POST", "/api/engines")],
    },
    Feature {
        name: "update.quiet",
        what: "a version that changes Rust waits for a quiet branch, and Build now starts it",
        calls: &[("POST", "/api/update/build-now")],
    },
    Feature {
        name: "update.modes",
        what: "Settings > Updates chooses when a version that changes Rust builds (right away, after N merges, at set times, only on Build now); `UpdateStatus.waiting` counts what is held and `GET /api/update/waiting` lists it",
        calls: &[("GET", "/api/update/waiting")],
    },
    Feature {
        name: "update.look",
        what: "another machine that sent this one a version asks its updater to look now and gets the updater's state back as the receipt (Settings > Updates follows a sent version step by step)",
        calls: &[("POST", "/api/update/look")],
    },
    Feature {
        name: "threads.cloud",
        what: "a new thread can run on Anthropic's servers (Claude Code cloud) on the Claude subscription: `NewThread.cloud`, and `Thread.cloud` for where it stands (Run on > Cloud)",
        calls: &[],
    },
    Feature {
        name: "phone.cloud",
        what: "the phone can start a thread in the cloud: `cloud: true` in the new-thread call, and `Thread.cloud` to show where it stands",
        calls: &[("POST", "/api/phone/threads")],
    },
    Feature {
        name: "thread.reach",
        what: "extra folders a thread may read and connections switched off for one thread (the composer's +)",
        calls: &[("GET", "/api/threads/{id}/reach"), ("PUT", "/api/threads/{id}/reach")],
    },
    Feature {
        name: "guard.reports",
        what: "Guard's fix threads report back to Guard instead of the user (ask_guard, report, answer_worker), and Guard may start a problem over",
        calls: &[("POST", "/api/problems/guard/answer")],
    },
    Feature {
        name: "usage.spend",
        what: "all AI use per day, in Hub (per engine and by who asked) and in Claude Code and Codex on their own, DeepSeek's spend included (Settings > Where it goes)",
        calls: &[("GET", "/api/usage/spend")],
    },
    Feature {
        name: "usage.spend.machines",
        what: "Where it goes adds every machine's use together (`everywhere=true`) and says what each one counted",
        calls: &[],
    },
    Feature {
        name: "usage.spend.accounts",
        what: "Where it goes carries a provider's own count of the whole account's tokens per day (`accounts`; Codex)",
        calls: &[],
    },
    Feature {
        name: "usage.spend.all",
        what: "Where it goes answers `all=true`: every day from the first one anything was counted on, on every machine",
        calls: &[],
    },
    Feature {
        name: "usage.spend.tokens",
        what: "Where it goes answers `unit=tokens` with the same lanes, days, hours and sources counted in tokens in place of dollars (`unit` says so)",
        calls: &[],
    },
    Feature {
        name: "usage.spend.hours",
        what: "Where it goes answers `hours=true` with the same use hour by hour (`hourly`) over the last 31 days, and splits who asked per day and hour (each source's `daily`)",
        calls: &[],
    },
    Feature {
        name: "usage.spend.estimate",
        what: "Where it goes estimates plan use Hub cannot see (the Claude app's chat) from the weekly plan meter, as an approximate lane of its own",
        calls: &[],
    },
    Feature {
        name: "usage.spend.cloud",
        what: "Where it goes counts Claude Code sessions in Anthropic's cloud (a lane of its own) and says in `notes` when it could not",
        calls: &[],
    },
    Feature {
        name: "phone.looking",
        what: "the phone says which thread it shows while it is in front, and no notice comes about it",
        calls: &[("POST", "/api/phone/looking")],
    },
    Feature {
        name: "presence.desktop",
        what: "the desktop says which thread it shows while you are at it, and the phone gets no notice about it (a question or approval is checked again after 45 seconds)",
        calls: &[("POST", "/api/presence")],
    },
    Feature {
        name: "machine.admin",
        what: "`POST /api/hosts/{name}/open` with `admin` opens a Windows Terminal window as administrator on this computer (Windows asks once), outside Hub",
        calls: &[],
    },
    Feature {
        name: "machine.admin.terminal",
        what: "`POST /api/hosts/{name}/terminals` with `admin` opens a terminal inside Hub that runs as administrator on this computer (Windows asks first; `hubd admin-host` runs it elevated and connects back); the terminal lists `admin`",
        calls: &[],
    },
    Feature {
        name: "machine.dist",
        what: "`GET /api/dist?platform=<os>-<arch>` sends this machine's installed Hub as a .tar.gz to a new machine of the same kind (`install.ps1 -From`), so it installs without compiling",
        calls: &[("GET", "/api/dist")],
    },
    Feature {
        name: "machine.credentials",
        what: "`POST /api/machine/credentials/check` tries the engines' sign-ins and keys and the API connections here for real",
        calls: &[("POST", "/api/machine/credentials/check")],
    },
    Feature {
        name: "threads.display",
        what: "a thread carries `project` (the folder of the project it belongs to) and `display` (waiting, error, asks, running, unread or idle), worked out once by the core",
        calls: &[],
    },
    Feature {
        name: "hosts.settings",
        what: "`PATCH /api/hosts/{name}` changes a machine added from the app (optional fields, now `push_quiet_mins`: minutes between two versions sent to it on its own, 0 = none); a host's info carries `push_quiet_mins`",
        calls: &[("PATCH", "/api/hosts/{name}")],
    },
    Feature {
        name: "bench.runs",
        what: "run a built-in bench suite as real threads, one at a time, and read each run's results and report (`bench/run.rs`)",
        calls: &[("GET", "/api/bench/runs"), ("POST", "/api/bench/runs"), ("GET", "/api/bench/runs/{id}"), ("POST", "/api/bench/runs/{id}/stop")],
    },
    Feature {
        name: "bench.summary",
        what: "a bench run carries `summary` (`BenchSummary`): per case the numbers and flag its report's table prints, from the same code (`bench/results.rs`)",
        calls: &[],
    },
    Feature {
        name: "bench.suites",
        what: "`GET /api/bench/suites` lists the built-in suites with their cases, defaults and cost cap, and estimates from earlier finished runs (`bench/catalog.rs`)",
        calls: &[("GET", "/api/bench/suites")],
    },
];

/// The names `/api/health` lists.
pub fn feature_names() -> Vec<&'static str> {
    FEATURES.iter().map(|f| f.name).collect()
}

/// Reads a thread another hubd sent. A hubd newer than this build may have a status or an owner
/// this one does not know: the thread is shown as idle (or without an owner) rather than lost.
/// None only when it is not a thread at all.
pub fn thread_from_wire(mut value: Value) -> Option<Thread> {
    if let Ok(thread) = serde_json::from_value::<Thread>(value.clone()) {
        return Some(thread);
    }
    let object = value.as_object_mut()?;
    let status_known = object.get("status").is_some_and(|s| serde_json::from_value::<crate::model::ThreadStatus>(s.clone()).is_ok());
    if !status_known {
        object.insert("status".into(), Value::from("idle"));
    }
    let owner_known = object.get("owner").is_none_or(|o| o.is_null() || serde_json::from_value::<crate::model::Owner>(o.clone()).is_ok());
    if !owner_known {
        object.insert("owner".into(), Value::Null);
    }
    serde_json::from_value(value).ok()
}

/// Reads a list of threads one by one, so one this build cannot read costs that thread only.
pub fn threads_from_wire(list: Vec<Value>) -> Vec<Thread> {
    list.into_iter().filter_map(thread_from_wire).collect()
}

/// Reads a stream message this build's `StreamMsg` refused because of a thread inside it. A
/// message type this build has never heard of stays unread (None), which callers skip.
pub fn stream_from_wire(text: &str) -> Option<StreamMsg> {
    let mut value: Value = serde_json::from_str(text).ok()?;
    if value["type"] == "thread" {
        let thread = thread_from_wire(value["thread"].take())?;
        return Some(StreamMsg::Thread { thread });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_names_are_unique_and_dotted() {
        let mut names = feature_names();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "a feature name is used twice");
        assert!(names.iter().all(|n| n.contains('.') && n.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c == '_')));
    }

    #[test]
    fn an_unknown_status_or_owner_still_gives_a_thread() {
        let thread = serde_json::json!({
            "id": "a", "title": "t", "cwd": "/x", "engine": "claude", "status": "napping",
            "engine_session_id": null, "title_locked": false, "archived": false,
            "created_at": 1, "updated_at": 2, "owner": { "kind": "from_the_future" }
        });
        let read = thread_from_wire(thread).unwrap();
        assert_eq!(read.status, crate::model::ThreadStatus::Idle);
        assert!(read.owner.is_none());
    }

    #[test]
    fn something_that_is_no_thread_is_skipped_not_fatal() {
        let list = vec![serde_json::json!({ "nonsense": true }), serde_json::json!(7)];
        assert!(threads_from_wire(list).is_empty());
    }
}
