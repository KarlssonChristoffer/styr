//! Model types: machines.

use super::*;

pub use crate::git::GitInfo;

/// A file that differs from how it was when the thread started, however it was changed: by an
/// edit the agent reported, a command it ran, or a commit.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadChange {
    /// Absolute path.
    pub path: String,
    /// The file's unified diff.
    pub diff: String,
    #[ts(type = "number")]
    pub added: usize,
    #[ts(type = "number")]
    pub removed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Listener {
    #[ts(type = "number")]
    pub port: u16,
    pub address: String,
    pub protocol: String,
    #[ts(type = "number | null")]
    pub pid: Option<u32>,
    pub process: Option<String>,
    /// The hub core itself, which cannot be stopped from here.
    pub is_hub: bool,
    /// Part of the operating system. Shown, never stoppable.
    pub system: bool,
    /// Full path of the executable, when the OS reveals it.
    pub exe: Option<String>,
    pub cwd: Option<String>,
    /// The thread behind the process: the one that started it, or with `origin` `folder` the
    /// one whose folder it runs in.
    pub thread_id: Option<ThreadId>,
    /// How the process came to run, which says how sure `thread_id` is.
    pub origin: ListenerOrigin,
    /// The command line, shortened.
    pub command: Option<String>,
    /// Unix milliseconds.
    #[ts(type = "number | null")]
    pub started_at: Option<i64>,
    /// The process that started it, as "name · pid", or just the pid when it has exited.
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ListenerOrigin {
    /// Started by a thread: it carries the thread's id, inherited from the engine.
    Thread,
    /// A thread's engine itself (an agent's own local server), not something it was asked to run.
    Engine,
    /// Hub did not start it, but it runs in a thread's folder. A guess.
    Folder,
    /// Hub's own live UI for working on Hub (Labs, Work on Hub).
    DevMode,
    /// Started by the Hub core outside any thread.
    Hub,
    /// None of the above: started by you or another program.
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HostSnapshot {
    pub hostname: String,
    pub os: String,
    pub cpu_percent: f32,
    #[ts(type = "number")]
    pub mem_used: u64,
    #[ts(type = "number")]
    pub mem_total: u64,
    pub listeners: Vec<Listener>,
}

/// What a machine is, for its page (`GET /api/host/about`, `?host=` for another one): the hardware,
/// how long it has been up and how full its disk is. What it cannot tell is left out.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MachineAbout {
    pub hostname: String,
    pub os: String,
    pub kernel: Option<String>,
    /// `x86_64`, `aarch64`, …
    pub arch: String,
    /// The board or computer, e.g. "Raspberry Pi 4 Model B Rev 1.5", when the machine says.
    pub model: Option<String>,
    /// What kind of computer it is, for its icon. None from a hubd older than this field.
    #[serde(default)]
    pub kind: Option<MachineKind>,
    pub cpu: Option<String>,
    pub cores: u32,
    #[ts(type = "number")]
    pub uptime_secs: u64,
    /// The disk the home folder is on.
    pub disk: Option<DiskUse>,
    /// The chip's temperature in °C: the average of its CPU sensors, or the board's own (`host.rs`).
    pub temp_c: Option<f32>,
    pub home: String,
    /// Tailscale on that machine: its state and address, or why there is none.
    pub tailscale: String,
}

/// What kind of computer a machine is, told from its hardware (`host.rs`), so it gets the right
/// icon. The person can pick another one in the app (`Looks::machines`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MachineKind {
    /// A desktop computer, with a screen.
    Desktop,
    /// A laptop: it has a battery or says so.
    Laptop,
    /// A single-board computer, such as a Raspberry Pi.
    Board,
    /// A virtual machine, most often rented in a data centre.
    Vps,
    /// A box without a desktop, such as an old PC running Ubuntu Server.
    Server,
}

/// How machines and projects look in the app: icons the person picked over the ones Hub gives
/// them (`GET`/`PUT /api/looks`). Kept on this machine's Hub.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct Looks {
    /// A machine's kind by its name, over the kind it reports.
    pub machines: BTreeMap<String, MachineKind>,
    /// A project's icon by `host|path` (an empty host for this machine).
    pub projects: BTreeMap<String, ProjectLook>,
    /// Projects pinned to the top of the rail, by the same `host|path` key, in the order shown.
    pub pinned: Vec<String>,
}

/// A project's own icon. Each part left out falls back to the one Hub picks from the name.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ProjectLook {
    /// One of the app's project icons, by name.
    pub icon: Option<String>,
    /// One of the app's project colours, by name.
    pub color: Option<String>,
    /// A picture instead of an icon: a small `data:image/…` URL, at most `LOOK_IMAGE_MAX` bytes.
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiskUse {
    pub mount: String,
    #[ts(type = "number")]
    pub total: u64,
    #[ts(type = "number")]
    pub free: u64,
}

/// What to open on a machine from its page: a terminal window on this computer, logged in there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MachineOpen {
    /// A shell.
    Shell,
    /// A root shell, through sudo.
    Root,
    /// An engine's own CLI (`claude`, `codex`), in the home folder.
    Engine,
    /// `hubd sign-in <engine>`: signs the engine in there in a way that lasts (`signin.rs`).
    SignIn,
    /// `hubd attach <thread> --if-idle` there: follows the thread live and runs its engine's own
    /// CLI on its session, in the thread's folder, once it is idle or Enter is pressed
    /// (`terminal.rs`). The thread's Take over button.
    Attach,
}

/// An engine's own CLI, e.g. `claude` for the engine `claude` (`GET /api/engines/clis`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineCli {
    pub engine: String,
    pub cli: String,
}

/// `POST /api/hosts/{name}/open`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OpenOnMachine {
    pub what: MachineOpen,
    /// The engine whose CLI to open, for `engine`.
    #[serde(default)]
    pub engine: Option<String>,
    /// As administrator: on this computer when it is Windows, a window outside Hub that Windows
    /// asks the person to allow (UAC). Only `shell` and `engine`.
    #[serde(default)]
    #[ts(optional)]
    pub admin: Option<bool>,
}

/// `POST /api/hosts/{name}/terminals`: a terminal inside Hub, logged in on that machine.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OpenTerminal {
    pub what: MachineOpen,
    /// The engine whose CLI to run, for `engine` and `sign_in`.
    #[serde(default)]
    pub engine: Option<String>,
    /// The thread to take over, for `attach`.
    #[serde(default)]
    pub thread: Option<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    /// As administrator: on this computer when it is Windows, after Windows asked the person to
    /// allow it (UAC). Only `shell` and `engine`.
    #[serde(default)]
    #[ts(optional)]
    pub admin: Option<bool>,
}

fn default_cols() -> u16 {
    100
}

fn default_rows() -> u16 {
    30
}

/// A terminal running inside Hub (`pty.rs`). It keeps running while no page shows it, until what
/// runs in it exits or it is closed; `GET /api/terminals/{id}/ws` shows it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TerminalSession {
    pub id: String,
    /// The machine it is logged in on.
    pub machine: String,
    pub what: MachineOpen,
    pub title: String,
    /// What runs in it, as it would be typed.
    pub command: String,
    #[ts(type = "number")]
    pub started_at: i64,
    /// Set once what ran in it has exited: its exit code, or -1 when there was none.
    pub exit: Option<i32>,
    /// The thread it took over, for `attach`: the thread's side column shows it.
    #[serde(default)]
    pub thread: Option<String>,
    /// Runs as administrator (Windows, elevated; `pty.rs`).
    #[serde(default)]
    #[ts(optional)]
    pub admin: Option<bool>,
}
