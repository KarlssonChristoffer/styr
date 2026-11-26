//! Model types: customs.

use super::*;

/// Something from outside Hub that agents build with, as Customs knows it: a package (npm,
/// crates, PyPI), a CSS library from a CDN, a CLI tool, an MCP server, a Docker image, a web
/// service. Agents use only what is cleared; anything unknown is reviewed first, and what cannot
/// be verified is held until the user clears it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomsItem {
    /// Where it comes from: "npm", "crates", "pypi", "cdn", "cli", "mcp", "docker", "service"...
    /// An open list: a new kind needs no change in the core.
    pub kind: String,
    pub name: String,
    pub status: CustomsStatus,
    /// What shelf it is on, e.g. "UI", "Testing", "HTTP". Empty until someone sorts it.
    #[serde(default)]
    pub category: String,
    /// One line on what it does.
    #[serde(default)]
    pub summary: String,
    /// The versions that are cleared, as the ecosystem writes them ("^5", ">=1.4"); empty means any.
    #[serde(default)]
    pub versions: String,
    /// Why it has its status: the review's verdict, or why it waits.
    #[serde(default)]
    pub reason: String,
    /// Who decided: "existing" (already in use when it was added), "you", "review", "declared"
    /// (an agent declared it and it is held) or "found" (a turn brought it in without clearing).
    pub decided_by: String,
    /// When Customs' review last looked at it.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub reviewed_at: Option<i64>,
    /// Known vulnerabilities in its newest version, by advisory id, from the daily check.
    #[serde(default)]
    pub advisories: Vec<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CustomsStatus {
    /// Stamped: agents may use it.
    Cleared,
    /// In quarantine: not used until the user clears it.
    Held,
    /// Not used; `reason` says why.
    Refused,
}

/// The user's decision on something at Customs, or something they add or sort.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomsEdit {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    #[ts(optional)]
    pub status: Option<CustomsStatus>,
    #[serde(default)]
    #[ts(optional)]
    pub category: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub summary: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub versions: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub reason: Option<String>,
}

/// How Customs runs by itself. Kept in the store, set in Settings > Customs.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct CustomsPrefs {
    /// Review what is declared or found as soon as it is: green clears by itself, anything
    /// that cannot be verified stays held.
    pub auto_review: bool,
    /// Once a day, look up everything on the list in OSV: known malware refuses it, known
    /// vulnerabilities in its newest version are noted.
    pub daily_check: bool,
    /// Once a month, review again what was reviewed more than a month ago, and start a thread
    /// in each project to update its cleared dependencies.
    pub monthly_update: bool,
    /// Agents' package managers fetch through Hub's registry: what has cleared is served, what
    /// is new is reviewed first and served once it clears, and no version younger than a week.
    pub registry: bool,
}

/// One thing that happened at Customs, as `customs_log` keeps it: what came through, what the
/// review or the user decided, and the daily and monthly rounds. Append-only; each machine keeps
/// its own.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomsLogEntry {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub ts: i64,
    /// What happened: "declared" (an agent asked), "found" (a turn brought it in), "asked" (a
    /// package manager asked through Hub's registry), "cleared", "held", "refused", "reviewed"
    /// (the review ran but a person's decision stands), "advisory", "added", "removed",
    /// "imported", "daily_check" or "monthly".
    pub what: String,
    /// The item it is about; empty for the rounds and an import.
    pub kind: String,
    pub name: String,
    /// Who: "you", "review", "agent", "check" (the daily check) or "registry".
    pub by: String,
    /// In words: the review's reason, what the round found, where a turn brought it in.
    pub detail: String,
    /// The thread it came from, with its title and folder, when one did.
    #[ts(optional)]
    pub thread_id: Option<String>,
    #[ts(optional)]
    pub thread_title: Option<String>,
    #[ts(optional)]
    pub cwd: Option<String>,
    /// How long it took, for a review.
    #[ts(type = "number | null")]
    pub took_ms: Option<i64>,
}

/// A review running now.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomsReviewing {
    pub kind: String,
    pub name: String,
    #[ts(type = "number")]
    pub since: i64,
}

/// What the Customs page shows of what happens: the log, newest first, and what is being
/// reviewed right now.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomsActivity {
    pub entries: Vec<CustomsLogEntry>,
    pub reviewing: Vec<CustomsReviewing>,
}

impl Default for CustomsPrefs {
    fn default() -> Self {
        Self { auto_review: true, daily_check: true, monthly_update: true, registry: false }
    }
}

/// Where Customs' registry answers, for pointing a package manager at it by hand.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomsRegistry {
    pub base: String,
    /// The variables engines get, name to value.
    pub env: Vec<(String, String)>,
    /// What goes in `~/.cargo/config.toml`, since cargo cannot be pointed by a variable.
    pub cargo_config: String,
}

/// What reading a project's manifests added.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomsImport {
    /// New items, cleared as already in use.
    pub added: u32,
    /// Ones that were known already, whatever their status.
    pub known: u32,
}
