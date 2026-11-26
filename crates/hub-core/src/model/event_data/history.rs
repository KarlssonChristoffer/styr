//! Payloads of the kinds about a thread's code and where it runs: `checkpoint`, `restored`,
//! `rewind`, `rule_saved`, `attempt`, `terminal`, `moved` and `moved_away`. The rules for a
//! payload struct are in `event_data.rs`.

use serde::Serializer;
use serde::ser::SerializeMap;

use super::*;

/// `checkpoint`: a snapshot of the repository `root` taken just before the user message `seq`
/// reached the engine, kept under `refs/hub/<thread>/<seq>` as `commit`. `head` and `branch` say
/// where the repository stood (`null` on a detached head or an unborn branch).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CheckpointData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub seq: Option<i64>,
    #[serde(default, deserialize_with = "lenient")]
    pub commit: String,
    #[serde(default, deserialize_with = "lenient")]
    pub root: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub head: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub branch: Option<Option<String>>,
}

/// `restored`: the repository `root` was rolled back to snapshot `to` (the checkpoint before
/// message `seq`, `null` when it was the state an earlier rollback replaced). `files` and
/// `commits` count what changed; `kept_commits` says why commits stayed when they did. How it
/// was before is kept as snapshot `safety` (with `safety_head` and `safety_branch`), so the
/// rollback can be undone.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RestoredData {
    #[serde(default, deserialize_with = "lenient")]
    pub to: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub seq: Option<Option<i64>>,
    #[serde(default, deserialize_with = "lenient")]
    pub root: String,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub files: u64,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub commits: u64,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub kept_commits: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient")]
    pub safety: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub safety_head: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string | null")]
    pub safety_branch: Option<Option<String>>,
}

/// `rewind`: "Edit from here". The conversation from user message `from` up to this event is left
/// behind (`cut_seqs`); `files` says the code was rolled back to before that message too. `by` is
/// "you".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RewindData {
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub from: Option<i64>,
    #[serde(default, deserialize_with = "lenient")]
    pub files: bool,
    #[serde(default, deserialize_with = "lenient")]
    pub by: String,
}

/// `rule_saved`: the user saved `text` as a rule in the engine's rules file `path`, from the
/// message `seq`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RuleSavedData {
    #[serde(default, deserialize_with = "lenient")]
    pub text: String,
    #[serde(default, deserialize_with = "lenient")]
    pub path: String,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(type = "number")]
    pub seq: i64,
}

/// `attempt`: the thread is one of several attempts at the same message (`group`). Three shapes:
/// - the start, first in each attempt: `n` of `of`, the group's `threads`, the commit they all
///   start from (`base`) in repository `root`, the folder the message was sent in (`cwd`) and
///   this attempt's own `worktree` and `branch`;
/// - `state` "picked": its work landed in `cwd` as `files` changed files; how the folder was
///   before is snapshot `safety` (as in `restored`);
/// - `state` "dropped": another attempt (`by`, a thread id) was picked.
///
/// The picked shape wrote `cwd` before `root` and the start the other way round.
// So `Serialize` is written by hand below instead of derived.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[ts(export)]
pub struct AttemptData {
    #[serde(default, deserialize_with = "lenient")]
    pub group: String,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional, type = "number")]
    pub n: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional, type = "number")]
    pub of: Option<u64>,
    #[serde(default, deserialize_with = "some_items")]
    #[ts(optional)]
    pub threads: Option<Vec<String>>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub base: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub state: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional, type = "number")]
    pub files: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub root: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub cwd: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub worktree: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub branch: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub safety: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub safety_head: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    #[ts(optional, type = "string | null")]
    pub safety_branch: Option<Option<String>>,
    #[serde(default, deserialize_with = "lenient")]
    #[ts(optional)]
    pub by: Option<String>,
}

impl Serialize for AttemptData {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        fn put<M: SerializeMap, T: Serialize>(map: &mut M, key: &str, value: &Option<T>) -> Result<(), M::Error> {
            value.as_ref().map_or(Ok(()), |v| map.serialize_entry(key, v))
        }
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("group", &self.group)?;
        put(&mut map, "n", &self.n)?;
        put(&mut map, "of", &self.of)?;
        put(&mut map, "threads", &self.threads)?;
        put(&mut map, "base", &self.base)?;
        put(&mut map, "state", &self.state)?;
        put(&mut map, "files", &self.files)?;
        // Picked: `cwd`, then `root`. The start: `root`, then `cwd`.
        if self.state.is_some() {
            put(&mut map, "cwd", &self.cwd)?;
            put(&mut map, "root", &self.root)?;
        } else {
            put(&mut map, "root", &self.root)?;
            put(&mut map, "cwd", &self.cwd)?;
        }
        put(&mut map, "worktree", &self.worktree)?;
        put(&mut map, "branch", &self.branch)?;
        put(&mut map, "safety", &self.safety)?;
        put(&mut map, "safety_head", &self.safety_head)?;
        put(&mut map, "safety_branch", &self.safety_branch)?;
        put(&mut map, "by", &self.by)?;
        map.end()
    }
}

/// `terminal`: the thread's session went to a terminal or came back. `state` "taken": process
/// `pid` (`null` when not known) runs `command` on it, and `cut` is the user message whose turn
/// that stopped. "back": Hub has it again. "imported": what was said there is copied into the
/// log, `turns` prompts of it, and the session now holds `replay_turns` prompts in all;
/// `carry_on` says the stopped turn is carried on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TerminalData {
    #[serde(default, deserialize_with = "lenient")]
    pub state: String,
    #[serde(default, deserialize_with = "nullable", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number | null")]
    pub pid: Option<Option<u64>>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub command: Option<String>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub cut: Option<i64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub turns: Option<u64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub replay_turns: Option<u64>,
    #[serde(default, deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub carry_on: Option<bool>,
}

impl TerminalData {
    pub fn state(state: &str) -> Self {
        Self { state: state.to_owned(), ..Default::default() }
    }
}

/// `moved`: the thread came here from machine `from`, into `folder`. `via` says how its code
/// came ("bundle", "clone", "existing", or "files" when every file was copied), `files` and
/// `dirty` are what `Brought` says of the files written and the uncommitted ones, `stopped` that
/// a turn was cut off by the move and is carried on here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MovedData {
    #[serde(default, deserialize_with = "lenient")]
    pub from: String,
    #[serde(default, deserialize_with = "lenient")]
    pub folder: String,
    #[serde(default, deserialize_with = "lenient")]
    pub via: String,
    #[serde(default, deserialize_with = "lenient_items")]
    pub files: Vec<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub dirty: u32,
    #[serde(default, deserialize_with = "lenient")]
    pub stopped: bool,
}

/// `moved_away`: the thread left for machine `to`, where it lives in `folder`. Here it is
/// archived and no longer listed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MovedAwayData {
    #[serde(default, deserialize_with = "lenient")]
    pub to: String,
    #[serde(default, deserialize_with = "lenient")]
    pub folder: String,
}
