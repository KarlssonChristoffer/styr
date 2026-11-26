//! Clash warnings: two threads changing the same file at the same time overwrite each other's
//! work. The core notes every file an agent edits (from its `tool_call` updates) and warns both
//! threads, once per file, when another thread changed it within the last few hours. Threads in
//! separate worktrees (attempts) never clash, since their files are different paths.
//!
//! What was touched is kept in memory, so it covers work since the core started.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use crate::model::ThreadId;

/// Edits older than this no longer count as working on the file at the same time.
pub const WINDOW_MS: i64 = 3 * 60 * 60 * 1000;

#[derive(Default)]
pub struct Clashes {
    /// Each file, with the threads that changed it and when they last did.
    touched: HashMap<String, HashMap<ThreadId, i64>>,
    /// Pairs of threads already warned about a file: (thread, other, file).
    warned: HashSet<(ThreadId, ThreadId, String)>,
}

impl Clashes {
    /// Notes that `thread` changed `file` at `now`; returns the other threads that changed it
    /// recently and have not been warned about it with this one yet.
    pub fn touch(&mut self, thread: &str, file: &str, now: i64) -> Vec<ThreadId> {
        let by = self.touched.entry(file.to_owned()).or_default();
        by.retain(|_, at| now - *at < WINDOW_MS);
        by.insert(thread.to_owned(), now);
        let others: Vec<ThreadId> = by.keys().filter(|t| *t != thread).cloned().collect();
        others
            .into_iter()
            .filter(|other| {
                let pair = if thread < other.as_str() { (thread.to_owned(), other.clone()) } else { (other.clone(), thread.to_owned()) };
                self.warned.insert((pair.0, pair.1, file.to_owned()))
            })
            .collect()
    }

    /// Forgets a thread, when it is archived or deleted.
    pub fn forget(&mut self, thread: &str) {
        for by in self.touched.values_mut() {
            by.remove(thread);
        }
    }
}

/// The files an ACP update says the agent edited, made absolute against the thread's folder.
pub fn edited(update: &serde_json::Value, cwd: &Path) -> Vec<String> {
    if !matches!(crate::model::update_kind(update), Some("tool_call" | "tool_call_update")) || update["status"] == "failed" {
        return Vec::new();
    }
    let edits = matches!(update["kind"].as_str(), Some("edit" | "delete" | "move"));
    let mut paths: Vec<&str> = update["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["type"] == "diff")
        .filter_map(|c| c["path"].as_str())
        .collect();
    if edits {
        paths.extend(update["locations"].as_array().into_iter().flatten().filter_map(|l| l["path"].as_str()));
    }
    let mut out: Vec<String> = paths.into_iter().map(|p| key(&cwd.join(p))).collect();
    out.sort();
    out.dedup();
    out
}

/// One spelling per file: `.` and `..` resolved, `/` separators, and no case on Windows.
fn key(path: &Path) -> String {
    let mut clean = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                clean.pop();
            }
            other => clean.push(other),
        }
    }
    let text = clean.display().to_string().replace('\\', "/");
    if cfg!(windows) { text.to_lowercase() } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn warns_each_pair_once_per_file_while_recent() {
        let mut c = Clashes::default();
        assert!(c.touch("a", "/r/x.rs", 0).is_empty());
        assert_eq!(c.touch("b", "/r/x.rs", 10), ["a"]);
        assert!(c.touch("a", "/r/x.rs", 20).is_empty(), "already warned");
        assert!(c.touch("c", "/r/x.rs", WINDOW_MS + 30).is_empty(), "a and b are long done");
        c.forget("c");
        assert!(c.touch("d", "/r/x.rs", WINDOW_MS + 40).is_empty());
    }

    #[test]
    fn edits_come_from_diffs_and_edit_locations() {
        let cwd = Path::new("/repo/app");
        let edit = json!({ "sessionUpdate": "tool_call", "kind": "edit", "status": "completed",
            "content": [{ "type": "diff", "path": "src/a.ts", "newText": "" }], "locations": [{ "path": "/repo/app/src/../src/a.ts" }] });
        assert_eq!(edited(&edit, cwd), ["/repo/app/src/a.ts"]);
        let read = json!({ "sessionUpdate": "tool_call", "kind": "read", "locations": [{ "path": "/repo/app/b.ts" }] });
        assert!(edited(&read, cwd).is_empty());
    }
}
