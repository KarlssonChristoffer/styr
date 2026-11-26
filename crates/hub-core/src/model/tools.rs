//! Model types: tools.

use super::*;

/// Everything Hub has running in the background on one host, for the Workers view.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Workers {
    /// The host it all runs on. Only this computer for now; remote hosts will add their own.
    pub host: String,
    /// Threads with an engine or processes running, including archived and deleted threads
    /// that left processes behind.
    pub threads: Vec<ThreadWork>,
    /// Hub itself first, then the helpers it runs without a thread (tidy, usage, update builds).
    pub hub: Vec<WorkerProcess>,
    /// Running containers, with the folder compose started them from when known.
    pub containers: Vec<Container>,
}

/// One of your own tools, found by the `hub.toml` in its folder (see `own_tools.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OwnTool {
    pub name: String,
    pub about: Option<String>,
    /// Where it is on this machine; the commands run there.
    pub folder: String,
    /// What it says it is doing, in its own words.
    pub headline: Option<String>,
    /// Its own one-word state, such as `working` or `HEALTHY`.
    pub state: Option<String>,
    pub detail: Option<String>,
    pub lists: Vec<OwnToolList>,
    pub actions: Vec<OwnToolAction>,
    /// Items in lists marked as waiting on the user.
    pub needs_you: u32,
    /// A thread's work can be handed to it.
    pub handoff: Option<OwnToolHandoff>,
    /// It is a Worker: its runs show as threads (`worker_runs.rs`). What a run of it is called.
    #[serde(default)]
    pub worker: Option<String>,
    /// Why the file or the status command could not be read. Lists keep their own.
    pub error: Option<String>,
    #[ts(type = "number")]
    pub checked_at: i64,
}

/// A folder with a `hub.toml` that nobody approved yet, so nothing in it runs.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OwnToolFound {
    pub folder: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OwnToolList {
    pub title: String,
    pub needs_you: bool,
    /// The first few.
    pub items: Vec<OwnToolItem>,
    /// How many there are in all.
    pub total: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OwnToolItem {
    pub id: Option<String>,
    pub text: String,
    pub state: Option<String>,
    pub detail: Option<String>,
    pub link: Option<String>,
}

/// What a tool calls taking a thread's work, such as "Hand to Knegarn".
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OwnToolHandoff {
    pub label: String,
    pub about: Option<String>,
    /// The tool can be told to really build ("Let it build"), not only to try.
    #[serde(default)]
    pub can_build: bool,
}

/// The brief for handing a thread's work to a tool, as drafted or as the user approved it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HandoffBrief {
    /// The tool's folder.
    pub folder: String,
    pub title: String,
    pub brief: String,
    /// The user ticked "Let it build": the tool's `build` arguments go along.
    #[serde(default)]
    pub build: bool,
    /// Answers to what the tool asked the last time (`ToolAsk`), each passed as `--name value`.
    #[serde(default)]
    pub answers: BTreeMap<String, String>,
}

/// Something a tool needs answered before it takes the work, such as how a repository runs its
/// tests. The handoff is tried again with the answer.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolAsk {
    pub name: String,
    pub label: String,
    #[serde(default)]
    pub hint: Option<String>,
}

/// A command Hub may run on the tool.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OwnToolAction {
    pub id: String,
    pub label: String,
    /// The list whose items it acts on, one at a time.
    pub on: Option<String>,
    /// What has to be typed before it runs.
    pub ask: Vec<String>,
    /// It stops or undoes something.
    pub danger: bool,
    pub about: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunOwnToolAction {
    pub folder: String,
    pub action: String,
    /// The id of the item it acts on, for an action `on` a list.
    #[serde(default)]
    pub item: Option<String>,
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
}

/// What an action printed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OwnToolRan {
    pub ok: bool,
    pub output: String,
    /// The tool needs these answered first; nothing was handed over.
    #[serde(default)]
    pub asks: Vec<ToolAsk>,
}

/// Where a run of one of your Workers stands: the chain in the inbox and the run card in the
/// thread (`worker_runs.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkerRun {
    pub tool: String,
    /// The tool's folder, where its commands run.
    pub folder: String,
    /// The tool's own id for the run.
    pub run: String,
    pub title: String,
    /// "running", "done" or "stopped".
    pub state: String,
    pub paused: bool,
    /// The tool's own word for how it went, such as an escalation reason.
    pub said: Option<String>,
    pub steps: Vec<WorkerStep>,
    #[ts(type = "number | null")]
    pub round: Option<i64>,
    pub agents: Vec<WorkerAgent>,
    pub link: Option<String>,
    pub cost: Option<f64>,
    /// What the user may do to it now.
    pub controls: Vec<WorkerControl>,
    /// The last event of the run that Hub has read.
    #[ts(type = "number")]
    pub cursor: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkerStep {
    pub id: String,
    pub label: String,
    /// "done", "now" or "todo".
    pub state: String,
    /// How many times the run has been in it.
    pub visits: u32,
}

/// One kind of agent working in the run, such as the reviewer.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkerAgent {
    pub role: String,
    pub calls: u32,
    pub model: Option<String>,
    /// What its latest call said.
    pub last: Option<String>,
}

/// One event of a run, as the run card lists it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkerLine {
    #[ts(type = "number")]
    pub seq: i64,
    pub kind: String,
    /// The step it belongs to, by the tool's steps.
    pub step: Option<String>,
    #[ts(type = "number | null")]
    pub round: Option<i64>,
    pub agent: Option<String>,
    pub text: String,
}

/// Something the user may do to a running run: a command the tool's own CLI has.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct WorkerControl {
    pub id: String,
    pub label: String,
    pub about: Option<String>,
    /// What is typed first, each filling `{name}`.
    pub ask: Vec<String>,
    /// Asked twice in the app.
    pub danger: bool,
    /// Pressing it authorizes something (it widens a budget): the app says so.
    pub authorizes: bool,
}

/// The user pressed one of a run's controls.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunWorkerControl {
    pub thread_id: ThreadId,
    pub control: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
}

/// One thread's engine and what it started.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadWork {
    pub thread_id: ThreadId,
    /// Hub holds a running engine for the thread.
    pub engine_running: bool,
    /// The engine's process, when it is found.
    #[ts(type = "number | null")]
    pub engine: Option<u32>,
    /// The engine and every process carrying the thread's id as trees, parents first.
    pub processes: Vec<WorkerProcess>,
    /// Share of the whole machine, summed over the processes.
    pub cpu_percent: f32,
    #[ts(type = "number")]
    pub mem: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkerProcess {
    #[ts(type = "number")]
    pub pid: u32,
    pub name: String,
    /// The command line, shortened.
    pub command: String,
    #[ts(type = "Array<number>")]
    pub ports: Vec<u16>,
    /// Share of the whole machine.
    pub cpu_percent: f32,
    #[ts(type = "number")]
    pub mem: u64,
    #[ts(type = "number")]
    pub started_at: i64,
    /// Levels below the process at the top of its tree.
    pub depth: u32,
    pub paused: bool,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: String,
    /// The folder docker compose started it from.
    pub project: Option<String>,
}
