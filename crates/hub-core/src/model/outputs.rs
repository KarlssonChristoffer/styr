//! Model types: outputs.

use super::*;

/// What `hubd attach` runs once the core has let go of a thread.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Takeover {
    pub thread_id: ThreadId,
    pub title: String,
    pub cwd: String,
    /// The engine's own CLI resuming the thread's session, e.g. `claude --resume <id>`.
    pub command: String,
    /// Taking over stopped a turn Hub was in the middle of. Hub carries it on when the thread
    /// comes back, unless the terminal finished the work.
    #[serde(default)]
    pub stopped_turn: bool,
    /// The session had no conversation yet, so `command` starts a new one under its id instead
    /// of resuming.
    #[serde(default)]
    pub fresh: bool,
    /// What the engine's configuration sets in its environment (provider and model, never keys),
    /// so the CLI runs on what the thread runs on.
    #[serde(default)]
    #[ts(type = "Record<string, string>")]
    pub env: BTreeMap<String, String>,
}

/// A terminal for a thread, opened or ready to attach to.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TerminalOpened {
    /// A window was opened on the machine the thread runs on.
    pub opened: bool,
    /// The tmux session it runs in, when there is one.
    pub session: Option<String>,
    /// What to type in a terminal on that machine to get to it.
    pub attach: String,
}

/// Something a project starts: a line of its `Procfile.dev` or `Procfile`, or a guess from its
/// manifests when it has neither.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ServiceInfo {
    pub name: String,
    pub command: String,
    /// The pids at the top of each running copy; empty when stopped.
    #[ts(type = "Array<number>")]
    pub pids: Vec<u32>,
    #[ts(type = "Array<number>")]
    pub ports: Vec<u16>,
    /// Where its output goes.
    pub log: String,
}

/// Something a project's threads made, for the project's Library tab (`outputs.rs`): a page an
/// agent showed, a picture, a doc or file it wrote, or a server it started.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Output {
    /// Stable while the thing lives: the kind and where it is.
    pub id: String,
    /// "page", "picture", "doc", "file" or "server".
    pub kind: String,
    pub title: String,
    /// The thread that made it, and that thread's title.
    pub thread: Option<ThreadId>,
    pub thread_title: Option<String>,
    #[ts(type = "number")]
    pub made_at: i64,
    #[ts(type = "number")]
    pub changed_at: i64,
    /// Pages: how many versions the agent showed. Files: how many turns changed it.
    pub versions: u32,
    /// Running now and reachable: a server on a port.
    pub live: bool,
    /// Every place it is, the main one first.
    pub places: Vec<OutputPlace>,
    /// What happened to it, oldest first.
    pub life: Vec<OutputStep>,
    /// Other threads that changed it after it was made.
    pub used_in: Vec<OutputUse>,
    /// Where the app opens it: a page's thread and id, a file's path, or a server's address.
    pub open: OutputOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputPlace {
    /// "hub" (kept by Hub), "folder" (a file on a machine), "repo" (in git) or "server".
    pub kind: String,
    /// In words: "Kept in Hub", "PC", "hub · main", "PC :5173".
    pub label: String,
    /// The path, URL or commit.
    pub detail: String,
    /// "live", "kept", "committed", "uncommitted", "untracked" or "gone".
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputStep {
    #[ts(type = "number")]
    pub at: i64,
    pub what: String,
    pub thread: Option<ThreadId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputUse {
    pub thread: ThreadId,
    pub title: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OutputOpen {
    pub thread: Option<ThreadId>,
    pub page: Option<String>,
    pub path: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectServices {
    pub services: Vec<ServiceInfo>,
    /// `Procfile.dev` or `Procfile` when the services come from one; null when they are guesses.
    pub procfile: Option<String>,
    /// The docker compose file, which Start all brings up too.
    pub compose: Option<String>,
}

/// What Start all or Stop all did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ServicesDone {
    /// Services started or stopped.
    pub services: Vec<String>,
    /// Compose was brought up or stopped.
    pub compose: bool,
    /// Thread engines shut down (Stop all).
    #[ts(type = "number")]
    pub engines: u32,
    /// Other processes the project's threads had started, stopped (Stop all).
    #[ts(type = "number")]
    pub processes: u32,
    /// Threads left alone because they are open in a terminal.
    #[ts(type = "number")]
    pub in_terminal: u32,
    pub errors: Vec<String>,
}
