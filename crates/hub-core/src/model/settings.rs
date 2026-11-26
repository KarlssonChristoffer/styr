//! Model types: settings.

use super::*;

/// The core settings the settings page can change. They live in `hub.json` and apply when the
/// core restarts.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CoreSettings {
    pub path: String,
    /// What the running core uses.
    pub running: CoreValues,
    /// What `hub.json` says now. It differs from `running` until the core restarts.
    pub saved: CoreValues,
    /// The engine commands Hub ships with, to compare and reset to.
    pub default_engines: BTreeMap<String, String>,
    /// A thread is working or waiting on you, so a restart would interrupt it.
    pub busy: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CoreValues {
    pub default_engine: String,
    /// Engine name to the command that starts its ACP agent.
    pub engines: BTreeMap<String, String>,
    /// Engine name to what Hub calls it, for engines added from a harness and a provider, e.g.
    /// "Hermes · DeepSeek".
    pub engine_labels: BTreeMap<String, String>,
    pub checkpoints: bool,
    pub tidy_model: Option<String>,
    pub voice_language: String,
    /// Held while dictating as a voice chat's "Push to Mute" key; null is off.
    pub voice_mute_key: Option<String>,
    /// Dictated text is tidied by the small model when dictation stops.
    pub voice_polish: bool,
    pub update_enabled: bool,
    pub update_branch: String,
    /// Ask before installing a version that is built and tested.
    pub update_ask: bool,
    /// Where Rust builds go; null is `target` in the source folder.
    pub update_target_dir: Option<String>,
    #[ts(type = "number")]
    pub update_interval_secs: u64,
    /// `right_away`, `merges`, `times` or `manual` (`update.build_mode`).
    #[serde(default)]
    pub update_build_mode: String,
    #[serde(default)]
    pub update_build_merges: u32,
    #[serde(default)]
    pub update_build_notice_at: Vec<u32>,
    #[serde(default)]
    #[ts(type = "number")]
    pub update_build_max_wait_mins: u64,
    #[serde(default)]
    pub update_build_times: Vec<String>,
    pub editor: Option<String>,
    pub terminal: Option<String>,
}

/// One change to `hub.json`. `key` is one of the names the settings page edits, e.g.
/// `checkpoints` or `engines.claude`; a null value puts the default back.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigEdit {
    pub key: String,
    #[ts(type = "string | number | boolean | null")]
    pub value: serde_json::Value,
}

/// A program on this computer that Hub or its engines use.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolInfo {
    pub name: String,
    pub label: String,
    /// What Hub uses it for.
    pub purpose: String,
    pub found: bool,
    pub version: Option<String>,
    /// e.g. who git commits as, or which GitHub account gh is logged in to.
    pub detail: Option<String>,
}

/// What changed in one version of Hub, written when the updater builds it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReleaseNotes {
    pub version: String,
    /// The version it replaced, when known.
    pub from: Option<String>,
    /// Markdown: what is new, grouped by feature, and what to try. Empty when no model could
    /// be asked; the commits are still listed.
    pub text: String,
    /// Every commit in the range, newest first.
    pub commits: Vec<Change>,
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub created_at: i64,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// A secret Hub keeps, without its value.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SecretInfo {
    /// Also the environment variable an engine gets it as.
    pub name: String,
    /// The last four characters, or "set" for short values.
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SetSecret {
    pub value: String,
}

/// `POST /api/keys`: a new key.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewSecret {
    pub name: String,
    pub value: String,
}

/// `GET /api/keys`: Settings > Key vault.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KeyVault {
    /// Where the values are, e.g. "Windows Credential Manager".
    pub kept_in: String,
    pub keys: Vec<VaultKey>,
    /// Well-known keys in the core's environment that are not in the vault.
    pub elsewhere: Vec<KeyStatus>,
}

/// One key in the vault, never with its value.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct VaultKey {
    pub name: String,
    /// The last four characters, or "set" for short values; None when the name is listed but
    /// the credential store no longer has it.
    pub hint: Option<String>,
    #[ts(type = "number | null")]
    pub created: Option<i64>,
    /// Last added or replaced.
    #[ts(type = "number | null")]
    pub changed: Option<i64>,
    /// Last handed to an engine, a project's threads or one of Hub's own calls.
    #[ts(type = "number | null")]
    pub used: Option<i64>,
    pub uses: Vec<KeyUse>,
    /// A model provider's own key, which would move an engine off its subscription.
    pub engine_key: bool,
    /// The core's environment has a variable of the same name too.
    pub in_env: bool,
}

/// Something that uses a key.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KeyUse {
    /// "engine", "api" (an API connection), "project" or "hub" (one of Hub's own features).
    pub kind: String,
    pub label: String,
    /// What to open: the engine, the connection's name or the project folder.
    pub target: String,
    /// The connection it comes through, for projects and Hub's features.
    pub via: Option<String>,
    /// For a connection: whether its last check worked.
    pub ok: Option<bool>,
}

/// What an agent outside Hub may do with Hub's tools (`hub_tools.rs`). An engine Hub runs
/// is always `supervised`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AgentAccess {
    /// List and read threads and see the engines. Changes nothing.
    Read,
    /// Also start, message and stop threads. A thread it starts runs with the engine's default
    /// permission mode, or its own thread's settings; it may only pick the model and effort.
    Supervised,
    /// Also start threads with any setting, the permission mode included.
    Full,
}

/// A key an outside agent (Claude Code in a terminal, a bot) shows to reach Hub's tools at
/// `/mcp`. Only its hash is kept.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentKey {
    pub id: String,
    pub name: String,
    pub access: AgentAccess,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub expires_at: i64,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub last_used_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewAgentKey {
    pub name: String,
    pub access: AgentAccess,
    /// How long it lasts; 30 days when left out.
    #[serde(default)]
    #[ts(optional)]
    pub days: Option<u32>,
}

/// A new key, shown once with the command that adds Hub to Claude Code.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreatedAgentKey {
    pub key: AgentKey,
    pub token: String,
    pub url: String,
    pub command: String,
}
