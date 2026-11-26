//! The update's status types (`UpdateStatus` and what hangs off it), split from `model.rs`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UpdateState {
    /// Development build or switched off in config.
    Disabled,
    Idle,
    Checking,
    Building,
    /// Built and installed; restarts once no thread is working.
    Ready,
    Restarting,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateStatus {
    /// The running version (short commit), or "dev".
    pub version: String,
    pub state: UpdateState,
    /// The version being built or waiting to start.
    pub target: Option<String>,
    pub message: Option<String>,
    /// While a version is ready (`target`), a newer one being built behind it. The ready one
    /// stays on offer until the newer one is done.
    #[serde(default)]
    pub building: Option<String>,
    /// The build step running now, as "Step 3 of 5: cargo test …".
    #[serde(default)]
    pub step: Option<String>,
    #[ts(type = "number | null")]
    pub checked_at: Option<i64>,
    /// While the version being built (`target`, or `building` behind a ready one) changes the Rust
    /// code: when it starts, after the branch has been quiet `update.rust_quiet_mins`. POST
    /// `/api/update/build-now` starts it now (feature `update.quiet`).
    #[serde(default)]
    #[ts(optional, type = "number | null")]
    pub waiting_until: Option<i64>,
    /// While a version that changes Rust is held back by the build mode: what waits and when it
    /// goes (feature `update.modes`). None when nothing is held.
    #[serde(default)]
    #[ts(optional)]
    pub waiting: Option<UpdateWait>,
    /// The last time a version was rolled back (feature `update.rollback`), until dismissed with
    /// DELETE `/api/update/rolled-back`.
    #[serde(default)]
    #[ts(optional)]
    pub rolled_back: Option<RolledBackInfo>,
    /// While the running version is on probation (feature `update.rollback`): what Hub goes back
    /// to if it fails, and about when it counts as healthy. The window confirms its pages with
    /// POST `/api/update/alive` (feature `update.alive`).
    #[serde(default)]
    #[ts(optional)]
    pub probation: Option<ProbationInfo>,
}

/// The running version has not yet shown it is healthy (`UpdateStatus::probation`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProbationInfo {
    /// What Hub goes back to; None when there is nothing to go back to.
    #[serde(default)]
    #[ts(optional)]
    pub previous: Option<String>,
    /// When the version counts as healthy at the earliest, ms since the epoch.
    #[ts(type = "number | null")]
    pub ends_at: Option<i64>,
}

/// A version that failed its probation and was replaced by the one before it (`rollback.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RolledBackInfo {
    pub from: String,
    pub to: String,
    /// Why, in words for the person.
    pub reason: String,
    #[ts(type = "number")]
    pub at_ms: i64,
    /// File name in `backups/` of the database as it was when the version was rolled back, kept
    /// because the version before could not open it; the last minutes of changes are in it.
    #[serde(default)]
    #[ts(optional)]
    pub db_set_aside: Option<String>,
}

/// What a held-back version waits for (`UpdateStatus::waiting`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UpdateWait {
    /// Merged pull requests that change the Rust code since the running version.
    pub rust: u32,
    /// All merged pull requests since the running version.
    pub total: u32,
    /// The count that starts the build; None in the modes that do not count.
    #[serde(default)]
    pub goal: Option<u32>,
    /// The counts at which the window tells how many wait.
    #[serde(default)]
    pub notice_at: Vec<u32>,
    /// `right_away`, `merges`, `times` or `manual`.
    pub mode: String,
}

/// One merged change that waits for a build (`GET /api/update/waiting`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WaitingChange {
    pub sha: String,
    /// The pull request's title, or the commit's subject.
    pub title: String,
    /// The pull request's number, when the commit says it.
    #[serde(default)]
    pub pr: Option<u32>,
    #[ts(type = "number")]
    pub at: i64,
    /// Whether it changes the Rust code (a full build), not only the pages.
    pub rust: bool,
}
