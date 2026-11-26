//! Model types: misc.

use super::*;

/// The newest commit of a branch, for the phone's Git part.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CommitLine {
    pub sha: String,
    pub subject: String,
    /// When it was made, ms since the epoch.
    #[ts(type = "number")]
    pub at: i64,
}

/// A project's repository at a glance (feature `phone.projectgit`): the default branch and its
/// CI, the open pull requests and the recent branches. Read only.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GitCard {
    /// The default branch, or what is checked out when it cannot be told.
    pub branch: String,
    pub head: Option<CommitLine>,
    /// CI of the default branch: "pass", "fail", "pending" or "none".
    pub ci: String,
    /// Why the GitHub parts are empty (gh missing, not signed in), when they are.
    pub message: Option<String>,
    pub pulls: Vec<PullRow>,
    pub branches: Vec<BranchRow>,
}

/// A markdown file of the repo's docs, for the phone's Docs part (feature `phone.projectdocs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DocRow {
    /// Relative to the project; open it with the files calls.
    pub rel: String,
    pub title: String,
    /// One short line: the stage of FOCUS.md, the folder of another file.
    pub sub: String,
    /// When it last changed, ms since the epoch.
    #[ts(type = "number")]
    pub at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DocsCard {
    pub pinned: Vec<DocRow>,
    pub recent: Vec<DocRow>,
    /// How many markdown files the docs hold in all.
    #[ts(type = "number")]
    pub count: usize,
}

/// A line of `docs/BACKLOG.md`: an idea parked for later (feature `phone.projectbacklog`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BacklogItem {
    /// Its line in the file, from 1. Changes send it back with the title, which must still match.
    #[ts(type = "number")]
    pub line: u32,
    pub title: String,
    pub tag: String,
}

/// A card of `docs/BOARD.md` (feature `phone.projectboard`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BoardCard {
    #[ts(type = "number")]
    pub line: u32,
    pub title: String,
    /// The thread working on it, when the line names one (`thread:<id>`).
    pub thread: String,
    /// The pull request, when the line names one (`#724`).
    #[ts(type = "number | null")]
    pub pr: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BoardColumn {
    pub name: String,
    pub cards: Vec<BoardCard>,
}

/// A change to the backlog or the board. `op` is `add` (`title`, `tag`; on the board to the
/// column `to`), `board` (move a backlog item onto the board's To do), `move` (a card to column
/// `to`) or `drop`. `line` and `title` name the item.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct PartEdit {
    pub path: String,
    pub op: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
}

/// Shadow mode of the thread status (D3): how often the status the event log implies was
/// compared with the one the core set, and the latest differences.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatusShadow {
    #[ts(type = "number")]
    pub checked: u64,
    #[ts(type = "number")]
    pub disagreed: u64,
    /// Newest first, a few.
    pub recent: Vec<ShadowMiss>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ShadowMiss {
    pub thread_id: ThreadId,
    pub title: String,
    /// The event after which the two answers differed.
    #[ts(type = "number")]
    pub seq: i64,
    pub kind: String,
    /// What the events say.
    pub shadow: ThreadStatus,
    /// What the thread had.
    pub real: ThreadStatus,
    #[ts(type = "number")]
    pub at: i64,
}
