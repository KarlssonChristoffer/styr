//! Hub's tools: Hub's threads as tools for agents. Any agent can see what is going on, list and
//! read threads, start one, message one, wait for it and stop it. The model is T3 Code's
//! orchestrator MCP server (github.com/pingdotgg/t3code, docs/orchestration-v2/
//! orchestrator-mcp-server.md and docs/user/outside-agents.md, MIT), rebuilt on Hub's own log.
//!
//! An engine Hub runs reaches the tools as its own thread (`/mcp/<thread id>`, see `mcp.rs`) with
//! `supervised` access: a thread it starts gets its own thread's settings, so it never runs with
//! a broader permission mode than the caller. An agent outside Hub (Claude Code in a terminal, a
//! bot) shows an agent key at `/mcp`, and the user picked what that key may do when they made it.
//! Messages an agent sends are marked `from` in the log, so they never pass for the user's.
//!
//! `hub_brief` answers "what's going on?" in one call, and `delegate` hands work to a new thread
//! whose answer comes back to the caller's thread as a message by itself.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::children::CONTEXTS;
use crate::folders::same_folder;
use crate::hub::Hub;
use crate::run;
use tool::{Out, Tool, json_out};
use crate::model::{AgentAccess, AgentKey, Attachment, CreatedAgentKey, Event, EventKind, NewAgentKey, NewThread, Thread, ThreadPatch, ThreadStatus, now_ms};

const KEYS: &str = "agent_keys";
/// How long a key lasts unless the user says otherwise, as with T3's sign-ins.
const DAYS: u32 = 30;
/// What one item of a thread and one read of it may hold, so a read never floods the reader.
const ITEM_CHARS: usize = 4_000;
use crate::replies::{READ_CHARS, reply};
use crate::text::tail;
use crate::thread_wait::wait;
/// How long a wait lasts unless asked otherwise, and at most. A wait that runs out changes
/// nothing; the agent can wait again.
const WAIT: u64 = 120;
const WAIT_MAX: u64 = 600;

/// Who is calling the tools.
pub enum Caller {
    /// The engine of a thread Hub runs.
    Thread(Thread),
    /// An agent outside Hub, by the key it showed.
    Agent { name: String, access: AgentAccess },
}

impl Caller {
    /// The Hub thread whose engine is calling, or None for an agent outside Hub.
    pub fn thread(&self) -> Option<&Thread> {
        match self {
            Caller::Thread(t) => Some(t),
            Caller::Agent { .. } => None,
        }
    }

    pub fn access(&self) -> AgentAccess {
        match self {
            Caller::Thread(_) => AgentAccess::Supervised,
            Caller::Agent { access, .. } => *access,
        }
    }

    /// Marks a message this caller sends, so surfaces show who sent it.
    fn mark(&self) -> Value {
        match self {
            Caller::Thread(t) => json!({ "thread": t.id, "title": t.title }),
            Caller::Agent { name, .. } => json!({ "agent": name }),
        }
    }
}

/// How a thread or project is linked in a reply, so Hub shows it as a chip.
fn link(kind: &str, id: &str) -> String {
    let mut out = format!("hub://{kind}/");
    for b in id.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Runs one of Hub's tools, or None when `name` is not one of them.
pub async fn call(hub: &Arc<Hub>, caller: &Caller, name: &str, args: &Value) -> Option<Result<String, String>> {
    let answer = tool::call(hub, caller, &registry(AgentAccess::Full), name, args).await?;
    Some(answer.and_then(|out| match out {
        Out::Text(text) => Ok(text),
        Out::Content(_) => Err(format!("{name} answers with more than text")),
    }))
}

/// What one brief item says of a thread's latest reply.
const BRIEF_REPLY: usize = 600;
/// How many finished threads a brief lists.
const BRIEF_FINISHED: usize = 15;

/// The folder threads asked about all of Hub from Home work in. Its CLAUDE.md and AGENTS.md tell
/// the engine what it is there for.
pub fn folder(data_dir: &std::path::Path) -> Result<PathBuf> {
    let dir = data_dir.join("desk");
    write_brief(&dir, true)?;
    // Threads from before the desk had a name started in `platschef`: they get the same brief.
    write_brief(&data_dir.join("platschef"), false)?;
    Ok(dir)
}

/// Whether `cwd` is the desk folder (or the older `platschef` one): a place to look across the
/// projects from, not one to work in, so a thread that has to write code is never started there.
pub fn is_desk_folder(data_dir: &std::path::Path, cwd: &str) -> bool {
    let cwd = crate::desk::norm(cwd);
    ["desk", "platschef"].iter().any(|name| crate::desk::norm(&data_dir.join(name).display().to_string()) == cwd)
}

fn write_brief(dir: &std::path::Path, make: bool) -> Result<()> {
    if !make && !dir.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for name in ["CLAUDE.md", "AGENTS.md"] {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(BRIEF) {
            std::fs::write(&path, BRIEF).with_context(|| format!("writing {}", path.display()))?;
        }
    }
    Ok(())
}

const BRIEF: &str = "# Hub

Hub runs the user's coding agents in threads, one folder (project) each. This thread was started
from Home to look across all of them, so this folder is only a desk: the work happens in the
projects.

Your tools are Hub itself (the MCP server `hub`): `hub_brief`, `hub_status`, `list_threads`,
`read_thread`, `delegate`, `start_thread`, `send_to_thread`, `wait_for_thread` and `stop_thread`.

- \"What's going on?\": call `hub_brief` once and answer in a few plain sentences: what needs
  the user first, then what is working, then what finished. Read a thread only when the brief is
  not enough.
- \"Fix X in Atlas\": `delegate` it to that project with a complete brief, since the new thread
  sees nothing else, tell the user which thread is on it, and end your turn. Its reply comes back
  here as a message; then tell the user how it went.
- Look before you act: read a thread before you message or stop it.
- Never stop or redirect a thread the user is working in unless they asked for it.
- Answer in the language the user writes in.
- Link every thread and project you mention, using the `link` and `project_link` the tools
  give, e.g. [Fix login](hub://thread/abc123) in [Atlas](hub://project/...). Hub shows them as
  names with live status. Group a status answer under **Needs you**, **Working**, **Done** and
  **Other**, leaving out empty groups.
";

/// One tool entry and running it (listing and calling read the same list).
pub mod tool;

// Agent keys -------------------------------------------------------------------------------

mod registry;
pub use registry::*;

mod context;
use context::*;

mod problems;
use problems::*;

mod customs;
use customs::*;

mod threads;
mod bench;
use threads::*;

mod delegate;
pub use delegate::*;

mod manage;
use manage::*;

mod keys;
pub use keys::*;

#[cfg(test)]
mod tests;
