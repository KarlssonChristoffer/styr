//! Model types: system.

use super::*;

/// `POST /api/threads/{id}/shutdown`: stop the engine and, with `everything`, what it started.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Shutdown {
    #[serde(default)]
    pub everything: bool,
}

/// The panic button pressed: no agent runs on this machine until it is resumed (`crate::halt`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Halt {
    #[ts(type = "number")]
    pub since: i64,
    /// The machine it was pressed on.
    pub by: String,
    /// Threads whose turn it cut off; they carry on when resumed.
    pub threads: Vec<ThreadId>,
    /// The other machines, and whether each one paused too.
    pub hosts: Vec<HaltReach>,
    /// Your tools with a pause action (`panic = "pause"` in their `hub.toml`), and how it went.
    pub tools: Vec<HaltReach>,
}

/// One machine or tool the panic button reached for.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HaltReach {
    pub name: String,
    pub ok: bool,
    /// What went wrong, or how many turns it stopped there.
    pub detail: Option<String>,
}

/// `POST /api/halt/pause` and `/api/halt/resume`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HaltRequest {
    /// Only this machine: what one machine asks of the others.
    #[serde(default)]
    pub here: bool,
    /// The machine that asked, when it is another one.
    #[serde(default)]
    pub by: Option<String>,
}

/// What resuming did.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Resumed {
    /// Threads carried on, here and on the other machines.
    pub threads: u32,
    pub hosts: Vec<HaltReach>,
    pub tools: Vec<HaltReach>,
}

/// The settings page's view of this Hub: version, where its files live, what changed lately.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct About {
    pub version: String,
    pub update_enabled: bool,
    pub remote: String,
    pub branch: String,
    #[ts(type = "number")]
    pub interval_secs: u64,
    /// The repository on GitHub, when the remote points there.
    pub repo_url: Option<String>,
    pub data_dir: String,
    pub config_path: String,
    /// Hub runs on defaults until `hub.json` exists.
    pub config_exists: bool,
    /// Why Hub runs on defaults although the file exists: it could not be read at start and was
    /// moved aside (`config::load`).
    pub config_problem: Option<String>,
    pub os: String,
    pub user: Option<String>,
    /// Newest first, from the clone the updater builds (or the source of a dev build).
    pub changelog: Vec<Change>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Change {
    pub hash: String,
    pub subject: String,
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub ts: i64,
}

/// Logins and keys the engines can use. Only whether they exist and whose they are; secrets
/// never leave the core.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Accounts {
    pub engines: Vec<EngineAccount>,
    pub keys: Vec<KeyStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineAccount {
    /// Key into the configured engines, e.g. "claude".
    pub engine: String,
    pub label: String,
    /// The command that starts its ACP agent, when the engine is configured.
    pub command: Option<String>,
    pub logged_in: bool,
    /// "subscription" or "api_key".
    pub method: Option<String>,
    /// The plan of a subscription login, e.g. "max".
    pub plan: Option<String>,
    pub email: Option<String>,
    pub organization: Option<String>,
    /// The file the login was read from.
    pub source: Option<String>,
}

/// An API key the core can see, by name.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KeyStatus {
    /// Environment variable name, e.g. ANTHROPIC_API_KEY.
    pub name: String,
    pub service: String,
    /// Where it was found: "environment" for now; Hub's own vault later.
    pub source: String,
    /// The last four characters, so you can tell which key it is.
    pub hint: String,
}

/// What `~/.ssh` holds: keys and the hosts named in its config.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SshInfo {
    pub dir: String,
    pub keys: Vec<SshKey>,
    pub hosts: Vec<SshHost>,
    #[ts(type = "number")]
    pub known_hosts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SshKey {
    /// File name of the public key.
    pub file: String,
    /// e.g. ssh-ed25519.
    pub kind: String,
    pub comment: String,
    /// The private half sits next to it.
    pub has_private: bool,
    #[ts(type = "number | null")]
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SshHost {
    pub alias: String,
    /// Other names on the same `Host` line: the same machine.
    pub aliases: Vec<String>,
    pub hostname: Option<String>,
    pub user: Option<String>,
    #[ts(type = "number | null")]
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
}
