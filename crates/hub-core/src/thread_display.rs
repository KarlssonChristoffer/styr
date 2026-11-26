//! What every client shows of a thread, worked out once on the server: the project folder the
//! thread belongs to (`project_root`) and the word for its state (`display_status`). The app,
//! the iPhone app and the phone page read `Thread.project` and `Thread.display` instead of each
//! deriving them from `cwd` and `status`. Knows nothing but the model; the store fills the
//! fields when it reads a thread. Does not look at the registered projects: that is
//! `stats.rs`'s and `project.rs`'s job, they start from the folder this gives.

use crate::model::{Thread, ThreadStatus};

/// The folder of the project `cwd` belongs to, spelled as `cwd` spells it (either slash).
/// - a worktree Hub made (`<repo>/.hub-threads/<id>`, `<repo>/.git/hub-attempts/<n>`) is its repository
/// - a quick thread's folder (`<root>/scratch/<date-time>`) and a chat's (`<root>/chats/<date-time>`)
///   belong to the `scratch` and `chats` folders
/// - an Overview widget's folder (`<data>/widgets/w<id>`) is the `widgets` folder
/// - anything else is the folder itself
pub fn project_root(cwd: &str) -> String {
    let comps = components(cwd);
    let name = |i: usize| &cwd[comps[i].0..comps[i].1];
    let n = comps.len();
    // A date-time folder under `scratch` or `chats`.
    for i in (1..n.saturating_sub(1)).rev() {
        let kind = name(i);
        let next = name(i + 1);
        if matches!(kind, "scratch" | "chats") && date_time(next) {
            return cwd[..comps[i].1].to_owned();
        }
    }
    if n >= 2 && name(n - 2) == "widgets" && is_widget_id(name(n - 1)) {
        return cwd[..comps[n - 2].1].to_owned();
    }
    for i in 0..n {
        let hub_tree = name(i) == ".hub-threads" || (name(i) == ".git" && i + 1 < n && name(i + 1) == "hub-attempts");
        let id = if name(i) == ".git" { i + 2 } else { i + 1 };
        if hub_tree && id < n {
            let (head, rest) = (cwd[..comps[i].0].trim_end_matches(['/', '\\']), &cwd[comps[id].1..]);
            return project_root(format!("{head}{rest}").trim_end_matches(['/', '\\']));
        }
    }
    cwd.to_owned()
}

/// The non-empty parts of a path as (start, end) byte ranges, split on either slash.
fn components(path: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (at, c) in path.char_indices() {
        match (c == '/' || c == '\\', start) {
            (true, Some(s)) => {
                out.push((s, at));
                start = None;
            }
            (false, None) => start = Some(at),
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, path.len()));
    }
    out
}

/// `2026-11-06-130344` or `2026-11-06-130344-2`.
fn date_time(s: &str) -> bool {
    let mut parts = s.splitn(5, '-');
    let digits = |p: Option<&str>, n: usize| p.is_some_and(|p| p.len() == n && p.bytes().all(|b| b.is_ascii_digit()));
    let ok = digits(parts.next(), 4) && digits(parts.next(), 2) && digits(parts.next(), 2) && digits(parts.next(), 6);
    ok && parts.next().is_none_or(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `w` and ten hex digits (`widgets::is_id`).
fn is_widget_id(s: &str) -> bool {
    s.len() == 11 && s.starts_with('w') && s[1..].bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// The one word a thread's state shows as, for a dot or a badge. A client reading a word it does
/// not know shows the thread as idle.
/// - `waiting`: a permission question waits on you
/// - `error`: the last turn failed
/// - `asks`: the last reply asks you something or got stuck (`reading.rs`)
/// - `running`: a turn is working, or the thread is in a terminal
/// - `unread`: it came back after you last opened it
/// - `idle`: none of those
pub fn display_status(t: &Thread) -> &'static str {
    match t.status {
        ThreadStatus::WaitingOnYou => "waiting",
        ThreadStatus::Error => "error",
        ThreadStatus::Running | ThreadStatus::InTerminal => "running",
        ThreadStatus::Idle if matches!(t.reading.as_deref(), Some("asks" | "stuck")) => "asks",
        ThreadStatus::Idle if t.returned_at.is_some_and(|r| t.seen_at.is_none_or(|s| s < r)) => "unread",
        ThreadStatus::Idle => "idle",
    }
}

/// Fills `project` and `display` on a thread that lacks them. A thread another hubd sent keeps
/// what that one worked out.
pub fn fill(t: &mut Thread) {
    if t.project.is_none() {
        t.project = Some(project_root(&t.cwd));
    }
    if t.display.is_none() {
        t.display = Some(display_status(t).to_owned());
    }
}

/// Works both out again, after something changed the thread (its status, reading or folder).
pub fn refill(t: &mut Thread) {
    (t.project, t.display) = (None, None);
    fill(t);
}

/// `t` with `project` and `display` filled in (`fill`), for a struct literal's end.
pub fn filled(mut t: Thread) -> Thread {
    fill(&mut t);
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hub_worktree_is_its_repository() {
        assert_eq!(project_root("D:\\Dev\\hub\\.hub-threads\\abc"), "D:\\Dev\\hub");
        assert_eq!(project_root("/home/a/hub/.hub-threads/fix-1/sub/dir"), "/home/a/hub/sub/dir");
        assert_eq!(project_root("/home/a/hub/.git/hub-attempts/3"), "/home/a/hub");
        assert_eq!(project_root("/home/a/hub/.git/hub-attempts/3/crates"), "/home/a/hub/crates");
        assert_eq!(project_root("/home/a/hub/.hub-threads"), "/home/a/hub/.hub-threads");
        assert_eq!(project_root("/home/a/hub"), "/home/a/hub");
    }

    #[test]
    fn quick_threads_and_chats_belong_to_their_folder() {
        assert_eq!(project_root("/d/Dev/scratch/2026-11-01-130344"), "/d/Dev/scratch");
        assert_eq!(project_root("D:\\Dev\\scratch\\2026-11-01-130344-2\\x"), "D:\\Dev\\scratch");
        assert_eq!(project_root("/d/chats/2026-11-01-130344"), "/d/chats");
        assert_eq!(project_root("/d/scratch/notes"), "/d/scratch/notes");
    }

    #[test]
    fn widgets_have_one_folder() {
        assert_eq!(project_root("/data/widgets/w0123456789"), "/data/widgets");
        assert_eq!(project_root("/data/widgets/notes"), "/data/widgets/notes");
    }

    #[test]
    fn status_words() {
        let mut t: Thread = serde_json::from_value(serde_json::json!({
            "id": "t", "title": "t", "cwd": "/tmp", "engine": "e1", "status": "idle",
            "title_locked": false, "archived": false, "created_at": 1, "updated_at": 1
        }))
        .unwrap();
        assert_eq!(display_status(&t), "idle");
        t.returned_at = Some(5);
        assert_eq!(display_status(&t), "unread");
        t.seen_at = Some(6);
        assert_eq!(display_status(&t), "idle");
        t.reading = Some("stuck".into());
        assert_eq!(display_status(&t), "asks");
        t.status = ThreadStatus::Error;
        assert_eq!(display_status(&t), "error");
        t.status = ThreadStatus::InTerminal;
        assert_eq!(display_status(&t), "running");
        t.status = ThreadStatus::WaitingOnYou;
        assert_eq!(display_status(&t), "waiting");
        refill(&mut t);
        assert_eq!(t.display.as_deref(), Some("waiting"), "refill follows a change");
    }
}
