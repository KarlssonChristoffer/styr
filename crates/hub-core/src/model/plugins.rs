//! Model types: plugins.

use super::*;

/// One GitHub repository on the Plugins page: something made for agents (an MCP server, a skill
/// or a tool) that could be brought into Hub.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginRepo {
    /// `owner/name`, as GitHub writes it.
    pub full_name: String,
    pub owner: String,
    pub name: String,
    pub description: String,
    pub url: String,
    #[ts(type = "number")]
    pub stars: u64,
    /// Stars gained in the last 7 days, when Hub could tell.
    #[ts(type = "number | null")]
    pub week: Option<u64>,
    /// `week` is projected from the newest stars rather than counted one by one.
    pub week_estimated: bool,
    /// Running total of new stars in equal steps up to now, oldest first, for a sparkline.
    pub spark: Vec<u32>,
    /// The time `spark` spans in milliseconds: 14 days, or less for a busy repository whose
    /// newest stars Hub read do not reach back that far. 0 when Hub could not look.
    #[ts(type = "number")]
    pub spark_span: i64,
    /// What it is, guessed from its topics, name and description: "mcp", "skill", "tool", or
    /// "worker" (a `hub-worker` topic, or a `hub.toml` with a `[worker]` once its page is read).
    pub kind: String,
    #[ts(optional)]
    pub language: Option<String>,
    pub topics: Vec<String>,
    #[ts(optional)]
    pub license: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub pushed_at: i64,
    pub archived: bool,
    /// The owner's picture as a data URL (the app only shows pictures it has in hand).
    #[ts(optional)]
    pub avatar: Option<String>,
    /// What Customs says about it, when it is on Customs' list (kind `github-repo`).
    #[ts(optional)]
    pub customs: Option<CustomsStatus>,
    #[serde(default)]
    pub customs_reason: String,
}

/// The Discover tab: what is rising, what is new and what most people use.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginDiscover {
    /// Most stars gained in the last 7 days.
    pub rising: Vec<PluginRepo>,
    /// Made in the last 30 days, most stars first.
    pub fresh: Vec<PluginRepo>,
    /// Most stars of all, among those worked on in the last month.
    pub top: Vec<PluginRepo>,
    /// Why the lists are short or empty (gh missing, GitHub's rate limit).
    #[ts(optional)]
    pub message: Option<String>,
    #[ts(type = "number")]
    pub fetched_at: i64,
    /// Hub is counting the lists again in the background (or still counting Rising); ask again
    /// in a moment for the new ones.
    #[serde(default)]
    pub updating: bool,
}

/// What a search on the Plugins page found.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginSearch {
    pub repos: Vec<PluginRepo>,
    #[ts(optional)]
    pub message: Option<String>,
}

/// One thing Hub checked about a repository before Customs has looked at it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginCheck {
    /// "ok" or "warn".
    pub level: String,
    pub text: String,
}

/// One plugin's own page.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginDetail {
    pub repo: PluginRepo,
    /// The README as Markdown, pictures taken out (the app cannot show outside pictures).
    pub readme: String,
    /// The repository's own social picture as a data URL, when it has one of its own.
    #[ts(optional)]
    pub banner: Option<String>,
    #[ts(optional)]
    pub homepage: Option<String>,
    /// Commits in the last 30 days (at most 100 counted).
    pub commits_month: u32,
    /// People who committed (at most 100 counted).
    pub contributors: u32,
    #[ts(type = "number")]
    pub forks: u64,
    /// Running total of new stars in 30 equal steps up to now, oldest first.
    pub curve: Vec<u32>,
    /// The time `curve` spans in milliseconds: 30 days, or less when the newest stars Hub read
    /// do not reach back that far.
    #[ts(type = "number")]
    pub curve_span: i64,
    /// What Hub found in the repository that says what it is ("package.json uses the MCP SDK").
    pub inside: Vec<String>,
    /// Hub's own first look; Customs' review comes with installing.
    pub checks: Vec<PluginCheck>,
    pub similar: Vec<PluginRepo>,
    /// It is a Worker (its `hub.toml` has a `[worker]` section) and can be installed.
    #[serde(default)]
    #[ts(optional)]
    pub worker: Option<PluginWorker>,
}

/// A Worker on the Plugins page: what a run of it is called and where it is installed here.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PluginWorker {
    pub label: String,
    /// The folder it is installed in on this machine.
    pub installed: Option<String>,
    /// Why it cannot be installed yet (Customs has not cleared it), when it cannot.
    pub why_not: Option<String>,
}

/// A Worker installed, or checked again: where it is and what its ready check said.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkerReady {
    pub folder: String,
    /// None when the Worker has no ready check.
    pub ok: Option<bool>,
    pub checks: Vec<WorkerCheck>,
    /// Why the check could not be read.
    pub error: Option<String>,
}

/// One line of a Worker's ready check: "pass", "warn" or "fail".
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkerCheck {
    #[serde(default)]
    pub section: String,
    pub status: String,
    pub label: String,
    #[serde(default)]
    pub detail: String,
}

/// What the clean-up after agent work did (`GET /api/cleanup`, `POST /api/cleanup` runs it now).
/// Every half hour Hub removes the worktrees, branches and temporary files agent work left, once
/// nothing in them is only on this machine; what it had to leave is listed with why.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Cleanup {
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub at: i64,
    pub removed: Vec<Cleaned>,
    /// Left alone, with why: work that is only here, a thread still on it, or not Hub's.
    pub kept: Vec<Cleaned>,
    /// Bytes the removed files took, where they were counted (temporary files and logs).
    #[ts(type = "number")]
    pub freed: u64,
}

/// One thing the clean-up removed or left.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Cleaned {
    /// `worktree`, `branch`, `temp` or `log`.
    pub kind: String,
    /// A folder, or `branch` in `repo` for a branch.
    pub path: String,
    /// What it was or why it stayed, in a few words.
    pub why: String,
    /// How many, for temporary files and logs counted together.
    #[ts(type = "number")]
    pub count: u32,
}

/// How Hub starts every agent, from Settings > Engines (`agent_prefs.rs`). Only what the engine
/// can be told at start; one that cannot keeps its own default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct AgentPrefs {
    /// Never compact a long conversation by itself; Compact by Send still does.
    pub auto_compact_off: bool,
    /// Compact once this much of the context is used, in percent. Unset: the engine decides.
    pub compact_at: Option<u8>,
    /// How the agents write: `Explanatory` or `Learning` (Claude Code's output styles). Unset:
    /// their own default.
    pub output_style: Option<String>,
    /// Projects that differ, by folder: a thread in that folder (or under it) starts with these.
    pub projects: std::collections::BTreeMap<String, ProjectPrefs>,
}

/// Where one project differs from `AgentPrefs`. Unset: as everywhere else.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ProjectPrefs {
    /// `engine` (the agent decides), `never`, or a percent such as `70`.
    pub compact: Option<String>,
    /// Empty for the agent's own default, else as `AgentPrefs::output_style`.
    pub output_style: Option<String>,
}

/// Who does which kind of task, from Settings > Handoffs (`handoff.rs`): per task type, the
/// engine and model that may do it in everyday chats and in dev threads. A task left out, or a
/// side unset, is done by the thread itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct HandoffPrefs {
    /// By task id (`handoff::TASKS`).
    pub tasks: std::collections::BTreeMap<String, HandoffTask>,
}

/// One task type's picks, one per kind of thread.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct HandoffTask {
    /// In everyday chats.
    pub chat: Option<HandoffPick>,
    /// In dev threads.
    pub dev: Option<HandoffPick>,
}

/// An engine, and a model it offers, picked for a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HandoffPick {
    pub engine: String,
    /// Found the way a side job finds one ("haiku", "deepseek-v4-pro"). Unset: the engine's own.
    #[serde(default)]
    pub model: Option<String>,
}
