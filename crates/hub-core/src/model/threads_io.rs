//! Model types: threads_io.

use super::*;

/// How far an update this machine sent to another one has got. Steps only go forward; `Failed`
/// can come from any of them. A client that reads a step it does not know shows it as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UpdateStep {
    /// This machine builds or sends it.
    Sending,
    /// Delivered to the other machine's folder; its updater has not said it knows yet.
    Sent,
    /// Its updater said it knows the version (answered the ask, or reports it as its target).
    Received,
    /// Its updater builds it, or waits for a quiet branch first (`detail` says which).
    Building,
    /// Built there; it switches when no thread works there.
    Ready,
    Restarting,
    /// The machine runs the version.
    Done,
    Failed,
}

/// The update this machine sent to one other machine, step by step, for Settings > Updates
/// (`MachineUpdates::hubd_track`). It only moves forward, so a row never falls back to "sent".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateTrack {
    /// The version sent (short commit).
    pub version: String,
    pub step: UpdateStep,
    /// One line on where it stands: the build step, what it waits for, or why it failed.
    pub detail: Option<String>,
    /// When it was delivered (ms), once it was.
    #[ts(type = "number | null")]
    pub sent_at: Option<i64>,
    /// When the other machine's updater said it knew (ms), once it did.
    #[ts(type = "number | null")]
    pub received_at: Option<i64>,
    /// When the step or the detail last changed (ms).
    #[ts(type = "number")]
    pub changed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewThread {
    pub cwd: String,
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub engine: Option<String>,
    /// First message, sent right away when present.
    #[ts(optional)]
    pub message: Option<String>,
    #[serde(default)]
    #[ts(as = "Option<Vec<Attachment>>", optional)]
    pub attachments: Vec<Attachment>,
    /// Session settings (config id to value) applied before the first message.
    #[serde(default)]
    #[ts(as = "Option<BTreeMap<String, SettingValue>>", optional)]
    pub settings: BTreeMap<String, serde_json::Value>,
    /// What the user first wrote, when `message` is a tidied version of it.
    #[serde(default)]
    #[ts(optional)]
    pub original: Option<String>,
    /// Any id the client picks for this request. Sending it again (a retry after a dropped
    /// connection) does nothing the second time.
    #[serde(default)]
    #[ts(optional)]
    pub client_id: Option<String>,
    /// Two to four attempts at the same task, each in its own git worktree and thread, each with
    /// these settings on top of `settings` (a different model, say). The first thread is returned.
    #[serde(default)]
    #[ts(as = "Option<Vec<BTreeMap<String, SettingValue>>>", optional)]
    pub attempts: Vec<BTreeMap<String, serde_json::Value>>,
    /// The id `POST /api/threads/warm` gave while the message was typed: the engine already
    /// started for it becomes this thread's (`warm.rs`). Unknown or used ids are ignored.
    #[serde(default)]
    #[ts(optional)]
    pub warm: Option<String>,
    /// Starts the thread in design mode (`design.rs`): it writes a design document before any code.
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub design: bool,
    /// The thread runs without the user, as a fix thread from Problems does: Hub answers its
    /// permission questions itself and picks its settings by words (`Unattended`).
    #[serde(default)]
    #[ts(optional)]
    pub unattended: Option<Unattended>,
    /// What the thread can reach beyond its own folder from the start (`reach.rs`). A thread with
    /// extra folders or connections off does not take a warm engine, which started without them.
    #[serde(default)]
    #[ts(optional)]
    pub reach: Option<ThreadReach>,
    /// Starts the thread on Anthropic's or OpenAI's servers (`cloud_threads.rs`), by the
    /// harness's own subscription. It needs a first message, and the core to have `threads.cloud`.
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub cloud: bool,
}

/// A folder a thread may also read, besides the one it works in (`reach.rs`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReachFolder {
    /// Absolute, on the machine the thread runs on.
    pub path: String,
    /// How the list shows it; the folder's name when empty.
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
}

/// What one thread can reach beyond its folder and the engine's own settings (`reach.rs`): more
/// folders for the agent to read, and connections switched off for this thread only. Applies when
/// the thread's engine next starts; one that is idle is stopped so that is the next message.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadReach {
    #[serde(default)]
    #[ts(as = "Option<Vec<ReachFolder>>", optional)]
    pub folders: Vec<ReachFolder>,
    /// `Connection.name`s, off for this thread on top of what `ConnectionPrefs` switches off.
    #[serde(default)]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub connections_off: Vec<String>,
}

/// A thread that runs in the background and never waits for the user. It is also the payload of
/// the `unattended` event, written as it is asked: keep the field order, and add a field only as
/// an `Option` that is not written when `None` (the rules in `event_data.rs`). Unlike the other
/// payloads it reads strictly (a payload without `by` does not read), as its reader always did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Unattended {
    /// Who started it, as the user reads it: "Problems" or "Vakten".
    pub by: String,
    /// Settings by category ("model", "mode"), each a list of words in order of preference. When
    /// the engine first starts, the first choice it offers whose id or name is (else contains)
    /// one of the words is picked, unless the thread already chose that setting.
    #[serde(default)]
    #[ts(as = "Option<BTreeMap<String, Vec<String>>>", optional)]
    pub prefer: BTreeMap<String, Vec<String>>,
}

/// Asks for an engine to start for a new thread ahead of its first message (`warm.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WarmAsk {
    pub cwd: String,
    #[ts(optional)]
    pub engine: Option<String>,
    /// The settings the composer would start the thread with.
    #[serde(default)]
    #[ts(as = "Option<BTreeMap<String, SettingValue>>", optional)]
    pub settings: BTreeMap<String, serde_json::Value>,
}

/// The id the thread should be created with (`NewThread.warm`), or null when nothing started.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Warmed {
    pub id: Option<String>,
}

/// What each model the engines offer can do (`capabilities.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelTable {
    /// Where the list of models comes from.
    pub source: String,
    /// When it was last fetched, Unix milliseconds; null when it never was.
    #[ts(type = "number | null")]
    pub fetched_at: Option<i64>,
    /// How many models the list has from the providers engines use.
    pub known: u32,
    /// Why the last fetch failed; the list from before is kept.
    pub error: Option<String>,
    /// The file that corrects a match or what a model can do.
    pub overrides: String,
    pub engines: Vec<EngineModels>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineModels {
    pub engine: String,
    /// Side jobs that make pictures go to this engine (`side_jobs.image`).
    pub draws: bool,
    /// The models it offered in its latest session; empty until it has run here.
    pub models: Vec<ModelCaps>,
    /// The fast model: the cheapest one, unless the overrides file
    /// marks another `"fast": true`.
    pub fast: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModelCaps {
    pub engine: String,
    /// The value the engine takes for it.
    pub value: String,
    pub name: String,
    /// The model in the list it was matched to.
    pub catalog: Option<String>,
    /// The list has that model.
    pub found: bool,
    /// The overrides file says something about it.
    pub corrected: bool,
    /// Tokens it reads at most.
    #[ts(type = "number | null")]
    pub context: Option<u64>,
    pub image_in: bool,
    pub pdf_in: bool,
    pub tools: bool,
    pub reasoning: bool,
    pub web_search: bool,
    /// What it costs per million tokens it writes, in US dollars, as the list says, a sign of
    /// how heavy the model is; on a subscription it is not what you pay.
    pub cost: Option<f64>,
}

/// Where a connection comes from, so you know where to switch it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ConnectionSource {
    /// A connector on your claude.ai account, which Claude Code picks up by itself.
    ClaudeAi,
    /// Your own settings for the engine (`~/.claude.json`, `~/.codex/config.toml`).
    User,
    /// The project's shared file (`.mcp.json`).
    Project,
    /// Your own settings for one folder only.
    Local,
    /// Comes with a plugin.
    Plugin,
    /// Hub's own tools.
    Hub,
    /// Only seen in threads; the engine no longer lists it here.
    Seen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ConnectionState {
    Connected,
    NeedsLogin,
    Failed,
    Off,
    /// The engine was not asked, or does not say.
    Unknown,
}

/// One tool of a connection that a thread called.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolUse {
    pub name: String,
    pub count: u32,
    #[ts(type = "number")]
    pub last_used: i64,
}

/// An MCP server or connector an engine's agents can reach (`connections.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Connection {
    pub engine: String,
    /// As the engine names it, e.g. `claude.ai ElevenLabs`.
    pub name: String,
    /// For people, e.g. `ElevenLabs`.
    pub label: String,
    pub source: ConnectionSource,
    pub state: ConnectionState,
    /// `http`, `sse` or `stdio`.
    pub transport: Option<String>,
    /// Where it answers: a URL without its query, or the program it starts. Never a secret.
    pub target: Option<String>,
    /// Why it failed, or which plugin it comes with.
    pub detail: Option<String>,
    /// Where it is switched on and off, when that is a web page.
    pub manage_url: Option<String>,
    /// The tools threads called, most used first. A connection can have more.
    pub tools: Vec<ToolUse>,
    pub uses: u32,
    /// How many threads used it.
    pub threads: u32,
    #[ts(type = "number | null")]
    pub last_used: Option<i64>,
    /// Switched off for Hub's threads (`ConnectionPrefs`); a terminal still has it.
    pub off: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineConnections {
    pub engine: String,
    /// What it asked, e.g. `claude mcp list`.
    pub source: String,
    #[ts(type = "number")]
    pub checked_at: i64,
    /// Why the engine could not be asked; the list then comes from its files and the threads.
    pub error: Option<String>,
    pub connections: Vec<Connection>,
    /// The connectors that come with the engine's account, e.g. "claude.ai connectors" or
    /// "ChatGPT apps"; null when Hub cannot switch them.
    pub account: Option<String>,
    /// They are switched off for Hub's threads.
    pub account_off: bool,
    /// Installed plugins; empty when the engine does not say.
    pub plugins: Vec<EnginePlugin>,
    /// A command that checks the engine's installation, e.g. `claude doctor`.
    pub doctor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EnginePlugin {
    /// e.g. `engineering@synced`.
    pub id: String,
    pub version: Option<String>,
    /// Where it is installed: user, project, local, synced.
    pub scope: Option<String>,
    /// The engine has it switched on.
    pub enabled: bool,
    /// Switched off for Hub's threads.
    pub off: bool,
    /// The skills it brings, as the engine lists them to the model (`figma:figma-use`), when
    /// Hub can tell. Codex lists every one within a small budget, so a big plugin crowds others out.
    #[serde(default)]
    pub skills: Vec<String>,
}

/// What Hub switches off in the engines it starts, by engine name. Kept by Hub, so a terminal
/// keeps everything; applied when an engine starts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ConnectionPrefs {
    /// Connections by the name the engine lists, e.g. `claude.ai ElevenLabs`.
    pub off: BTreeMap<String, Vec<String>>,
    /// Engines whose account connectors (claude.ai connectors, ChatGPT apps) are off.
    pub account_off: Vec<String>,
    /// Plugins by id.
    pub plugins_off: BTreeMap<String, Vec<String>>,
}

/// What a doctor command said.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DoctorReport {
    pub command: String,
    pub ok: bool,
    pub output: String,
}

/// Every connection of every engine, for Settings.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Connections {
    pub engines: Vec<EngineConnections>,
}

/// What one thread's agent can reach, and what it used.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadConnections {
    pub engine: String,
    /// Connections this thread called, with only its own calls counted.
    pub used: Vec<Connection>,
    /// Everything else it can reach from its folder.
    pub available: Vec<Connection>,
    pub error: Option<String>,
}

/// A group of attempts at one task, from any of its threads.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AttemptGroup {
    pub group: String,
    /// The folder the attempts started from and the picked one lands in.
    pub cwd: String,
    /// One of them was picked; the group is over.
    pub done: bool,
    pub attempts: Vec<AttemptInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AttemptInfo {
    pub n: u32,
    pub thread_id: ThreadId,
    pub title: String,
    pub status: ThreadStatus,
    /// The model it runs on, as last set.
    #[ts(optional)]
    pub model: Option<String>,
    pub files: u32,
    pub added: u32,
    pub removed: u32,
    /// "picked" or "dropped" once the group is done.
    #[ts(optional)]
    pub state: Option<String>,
}

/// Fields to change on a thread; missing fields stay as they are.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadPatch {
    #[ts(optional)]
    pub title: Option<String>,
    /// Moves the thread to another engine, handing the conversation over with the next message.
    #[ts(optional)]
    pub engine: Option<String>,
    #[ts(optional)]
    pub archived: Option<bool>,
    /// Marks the thread done (true) or back in play (false).
    #[ts(optional)]
    pub settled: Option<bool>,
    /// The user has seen the thread as it is now (true), or wants it unread again (false).
    #[ts(optional)]
    pub seen: Option<bool>,
    /// Pins the thread on top of the activity list, or unpins it.
    #[ts(optional)]
    pub pinned: Option<bool>,
    /// Pins the thread at this place among the pins: a value between those of its neighbours.
    #[ts(optional, type = "number")]
    pub pin_order: Option<i64>,
    /// Hides the thread until this time (Unix milliseconds); 0 wakes it.
    #[ts(optional, type = "number")]
    pub snooze_until: Option<i64>,
    #[ts(optional)]
    pub auto_settle: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SendMessage {
    pub text: String,
    #[serde(default)]
    #[ts(as = "Option<Vec<Attachment>>", optional)]
    pub attachments: Vec<Attachment>,
    /// What the user first wrote, when `text` is a tidied version of it. Kept in the log only.
    #[serde(default)]
    #[ts(optional)]
    pub original: Option<String>,
    /// Any id the client picks for this request. Sending it again (a retry after a dropped
    /// connection) does nothing the second time.
    #[serde(default)]
    #[ts(optional)]
    pub client_id: Option<String>,
    /// Steer rather than queue: when a turn is running it stops, and this message goes next.
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub steer: bool,
}

/// "Edit from here": `text` goes in place of an earlier user message, and what followed it is
/// left out of the conversation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EditFrom {
    pub text: String,
    #[serde(default)]
    #[ts(as = "Option<Vec<Attachment>>", optional)]
    pub attachments: Vec<Attachment>,
    /// Roll the files back to the snapshot taken before that message too.
    #[serde(default)]
    #[ts(as = "Option<bool>", optional)]
    pub files: bool,
}

/// A rule to keep, said in a message.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SaveRule {
    pub text: String,
    /// The user message it came from.
    #[ts(type = "number")]
    pub seq: i64,
}

/// A long, messy message to turn into a clear prompt.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TidyRequest {
    pub text: String,
    /// Folder the engine runs in; the thread's or the one picked on Home.
    #[ts(optional)]
    pub cwd: Option<String>,
    /// Write the prompt knowing the project, folder, rules and thread (autoprompt) instead of
    /// only putting the text in order. A hubd from before this field ignores it.
    #[serde(default)]
    #[ts(optional)]
    pub context: Option<bool>,
    /// The thread the message is for, when there is one; its conversation goes into the context.
    #[serde(default)]
    #[ts(optional)]
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Tidied {
    pub text: String,
}

/// Something sent along with a message. Exactly one of `data`, `text`, `path` or `url` is set.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Attachment {
    pub name: String,
    pub mime: String,
    /// Base64 bytes, for images and other binary files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub data: Option<String>,
    /// Contents of a text file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text: Option<String>,
    /// A file on the host, sent as a link the engine reads itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
    /// A web address, sent as a link the engine opens itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
}

/// Something kept in the context bag: a file, picture, link or snippet that goes along into any
/// thread, in any project (`/api/bag`). In the list, `item` has no `data` or `text`; one item
/// (`GET /api/bag/{id}`) has everything.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BagItem {
    pub id: String,
    pub item: Attachment,
    /// A small picture for the list, as a data URL, for pictures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thumb: Option<String>,
    /// How big the content is, in bytes; 0 for links and files on the host.
    #[ts(type = "number")]
    pub size: i64,
    /// Where it was put in the bag from, such as a thread's title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from: Option<String>,
    #[ts(type = "number")]
    pub added_at: i64,
}

/// Something to put in the bag (`POST /api/bag`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BagAdd {
    pub item: Attachment,
    #[serde(default)]
    #[ts(optional)]
    pub thumb: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub from: Option<String>,
}

/// A setting is a string (a select option) or a boolean.
#[derive(TS)]
#[ts(export, type = "string | boolean")]
pub struct SettingValue;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SetSetting {
    pub config_id: String,
    #[ts(as = "SettingValue")]
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PermissionAnswer {
    /// The chosen option, or null to reject.
    pub option_id: Option<String>,
}

/// How much threads may do without asking. Hub answers the permission questions this covers;
/// the rest still come to the user. Hub's own read-only tools never ask, whatever the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PermissionLevel {
    /// Every question the engine asks comes to the user.
    Ask,
    /// Reading, searching and fetching.
    #[default]
    Read,
    /// Reading and changing files, but not deleting them.
    Edit,
    /// Everything but deletes, commands that destroy work, and leaving plan mode.
    Everything,
}

/// The user's permission settings, from Settings > Safety. Rules are globs (`*` is anything)
/// matched against a tool call's command, title and tool name; `Bash(npm test:*)` works as in
/// Claude Code. Protected files are path globs, gitignore style.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct PermissionPrefs {
    pub level: PermissionLevel,
    /// Always allowed, whatever the level.
    pub allow: Vec<String>,
    /// Always refused. Wins over `allow`.
    pub deny: Vec<String>,
    /// Paths no agent may edit, move or delete.
    pub protected: Vec<String>,
}

/// A snapshot of the thread's repository to roll back to: a checkpoint's or a rollback's commit.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UndoTarget {
    pub to: String,
}

/// Ask another model to review what the thread changed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewRequest {
    /// Picks the first model whose id or name contains this. Without it the core picks one
    /// other than the thread's.
    #[serde(default)]
    #[ts(optional)]
    pub model: Option<String>,
    /// A checkpoint commit of the thread: review only what changed since it, e.g. one reply.
    /// Without it, everything since the thread started.
    #[serde(default)]
    #[ts(optional)]
    pub from: Option<String>,
    /// Send the findings back to the thread's agent (the default), unless the review finds
    /// nothing worth fixing.
    #[serde(default)]
    #[ts(optional)]
    pub send_back: Option<bool>,
    /// Set when the thread's agent asked for the review itself, in its words.
    #[serde(default)]
    #[ts(optional)]
    pub reason: Option<String>,
}

/// Turns design mode on or off for a thread (`design.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesignSwitch {
    pub on: bool,
}

/// Asks fresh sessions to review a thread's design document (`design.rs`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesignReviewAsk {
    /// The document, relative to the thread's folder. Without it, the one the thread last wrote
    /// under `docs/design/`, or else the newest there.
    #[serde(default)]
    #[ts(optional)]
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DesignReviewStarted {
    pub id: String,
    pub doc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReviewStarted {
    pub id: String,
    pub model: String,
}

/// What rolling back to a snapshot would change.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UndoPreview {
    /// The repository's folder; paths are relative to it.
    pub root: String,
    pub files: Vec<UndoFile>,
    /// Commits made since, newest first, as `<short hash> <subject>`.
    pub commits: Vec<String>,
    /// Why those commits stay, when they do: the files are rolled back but the branch is not.
    pub kept_commits: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UndoFile {
    pub path: String,
    /// "restore" a changed file, "remove" a new one, "recreate" a deleted one.
    pub action: String,
}
