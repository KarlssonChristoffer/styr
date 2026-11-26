//! Who in Hub started a thread on its own (`Thread.owner`): the types and what makes an owner a
//! system one. Split out of `model.rs`, which re-exports them, so API types keep their path.
//! Knows nothing else; `system.rs` is what starts such threads.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Who in Hub started a thread on its own (`system.rs`). Lists mark such a thread with a robot
/// and this name, and a system owner's threads live under System instead of in the inbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Owner {
    pub kind: OwnerKind,
    /// What the user reads: "Vakten", the routine's name, "Merge queue", the starting thread's title.
    pub name: String,
    /// Which one of its kind: the routine's id, the problem's number, the starting thread's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
}

/// What kind of Hub agent started a thread. The first four, and Bench, are system threads: they work for
/// Hub across projects and machines, sit folded under "From Hub" in the inbox, and come to its
/// top only when they need the user. The others are errands done inside a project, listed there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum OwnerKind {
    /// Vakten's own chat, the guard of the Problems page.
    Vakten,
    /// A run of a routine (Daily review, Weakness hunt, ...).
    Routine,
    /// The merge queue resolving a conflict in its build folder.
    Merge,
    /// A thread building an Overview widget.
    Widget,
    /// A repeat of a bench case (`bench/run.rs`), archived once it is checked.
    Bench,
    /// A fix thread for a problem, from Vakten or Fix now.
    Fixer,
    /// Customs' monthly dependency update in a project.
    Customs,
    /// A thread another thread started through Hub's tools, or a side job.
    Agent,
}

impl OwnerKind {
    /// A system thread: Hub's own, not one project's.
    pub fn system(self) -> bool {
        matches!(self, Self::Vakten | Self::Routine | Self::Merge | Self::Widget | Self::Bench)
    }
}

impl Owner {
    pub fn new(kind: OwnerKind, name: impl Into<String>, id: Option<String>) -> Self {
        Self { kind, name: name.into(), id }
    }

    pub fn system(&self) -> bool {
        self.kind.system()
    }
}
