//! Guards Hub's tools for agents: what a key's access may call (read-only gets nothing that changes
//! anything), that managing tools patch like the user's buttons, how a read folds a log and a wait
//! sees its own turn's end, that a carried-on delegation is followed, and attached files and links.

use super::*;
use crate::thread_wait::answered;

fn ev(seq: i64, kind: EventKind, data: Value) -> Event {
    Event { thread_id: "t".into(), seq, ts: 0, kind, data }
}

#[test]
fn the_desk_folder_is_known_by_its_path() {
    let data = std::path::Path::new("/home/c/.local/share/hub");
    assert!(is_desk_folder(data, "/home/c/.local/share/hub/desk"));
    assert!(is_desk_folder(data, "/home/c/.local/share/hub/platschef/"));
    assert!(!is_desk_folder(data, "/home/c/Dev/desk"), "a project that happens to be called desk is a project");
    assert!(!is_desk_folder(data, "/home/c/.local/share/hub"));
}

#[test]
fn a_job_carried_on_by_hub_is_followed() {
    let events = vec![
        ev(5, EventKind::UserMessage, json!({ "text": "the brief" })),
        ev(7, EventKind::UserMessage, json!({ "text": "carry on", "quota": 6 })),
        ev(9, EventKind::UserMessage, json!({ "text": "you wrote this" })),
        ev(11, EventKind::UserMessage, json!({ "text": "carry on", "restart": "x" })),
    ];
    assert_eq!(carried_on(&events, 5), 11);
    assert_eq!(carried_on(&events, 11), 11);
    assert_eq!(carried_on(&events[..1], 5), 5);
}

fn chunk(text: &str) -> Value {
    json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": text } })
}

#[test]
fn a_queued_message_waits_for_its_own_turn() {
    // Message 2 was queued while 1 was running; 1's turn ends first.
    let events = [
        ev(3, EventKind::Update, chunk("first ")),
        ev(4, EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 1 })),
        ev(5, EventKind::Update, chunk("second")),
        ev(6, EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 2 })),
    ];
    assert_eq!(answered(&events, 2), Some(("done", 6)));
    assert_eq!(reply(&events, 6), "second");
    assert_eq!(answered(&events[..2], 2), None);
}

#[test]
fn a_failed_or_stopped_turn_says_so() {
    let failed = [ev(2, EventKind::Error, json!({ "message": "boom", "prompt": 1 }))];
    assert_eq!(answered(&failed, 1), Some(("failed", 2)));
    // An engine that fails to start is not the answer to anything.
    let crashed = [ev(2, EventKind::Error, json!({ "message": "no engine" }))];
    assert_eq!(answered(&crashed, 1), None);
    let stopped = [ev(2, EventKind::TurnEnd, json!({ "stop_reason": "cancelled", "prompt": 1 }))];
    assert_eq!(answered(&stopped, 1), Some(("stopped", 2)));
}

#[test]
fn reading_joins_chunks_and_marks_who_spoke() {
    let events = [
        ev(1, EventKind::UserMessage, json!({ "text": "hi" })),
        ev(2, EventKind::Update, chunk("Hel")),
        ev(3, EventKind::Update, chunk("lo")),
        ev(4, EventKind::Update, json!({ "sessionUpdate": "tool_call", "title": "Read file" })),
        ev(5, EventKind::TurnEnd, json!({ "stop_reason": "end_turn" })),
        ev(6, EventKind::UserMessage, json!({ "text": "do it", "from": { "thread": "x", "title": "Boss" } })),
    ];
    let messages = fold(&events, false);
    let who: Vec<_> = messages.iter().map(|i| (i.who, i.text.as_str())).collect();
    assert_eq!(who, [("user", "hi"), ("reply", "Hello"), ("agent", "do it")]);
    assert_eq!(messages[2].from.as_deref(), Some("Boss"));
    assert_eq!(fold(&events, true).len(), 5);
}

#[test]
fn read_only_agents_get_no_tools_that_change_anything() {
    let names = |a| tools(a).iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    assert!(!names(AgentAccess::Read).contains(&"start_thread".to_owned()));
    assert!(names(AgentAccess::Supervised).contains(&"start_thread".to_owned()));
    let full = tools(AgentAccess::Full);
    let start = full.iter().find(|t| t["name"] == "start_thread").unwrap();
    assert!(start["inputSchema"]["properties"]["settings"].is_object());
    let supervised = tools(AgentAccess::Supervised);
    let start = supervised.iter().find(|t| t["name"] == "start_thread").unwrap();
    assert!(start["inputSchema"]["properties"].get("settings").is_none());
}

#[test]
fn agents_can_manage_threads_but_read_keys_cannot() {
    let names = |a| tools(a).iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
    for tool in ["settle_thread", "rename_thread", "pin_thread", "snooze_thread"] {
        assert!(names(AgentAccess::Supervised).contains(&tool.to_owned()), "{tool}");
        assert!(!names(AgentAccess::Read).contains(&tool.to_owned()), "{tool}");
        assert!(crate::permit::QUIET_TOOLS.contains(&tool), "{tool} never asks");
    }
    let start = tools(AgentAccess::Supervised).into_iter().find(|t| t["name"] == "start_thread").unwrap();
    assert!(start["inputSchema"]["properties"]["context"].is_object());
    assert!(start["inputSchema"]["properties"]["files"].is_object());
}

#[test]
fn managing_tools_patch_like_the_users_buttons() {
    let settle = patch_for("settle_thread", &json!({})).unwrap();
    assert_eq!(settle.settled, Some(true));
    assert_eq!(patch_for("settle_thread", &json!({ "back": true })).unwrap().settled, Some(false));
    assert_eq!(patch_for("rename_thread", &json!({ "title": "  Fix the export  " })).unwrap().title.as_deref(), Some("Fix the export"));
    assert!(patch_for("rename_thread", &json!({ "title": " " })).is_err());
    assert_eq!(patch_for("pin_thread", &json!({})).unwrap().pinned, Some(true));
    assert_eq!(patch_for("pin_thread", &json!({ "pinned": false })).unwrap().pinned, Some(false));
    assert_eq!(patch_for("snooze_thread", &json!({ "hours": 0 })).unwrap().snooze_until, Some(0));
    assert!(patch_for("snooze_thread", &json!({ "hours": 2 })).unwrap().snooze_until.unwrap() > now_ms());
    assert!(patch_for("snooze_thread", &json!({ "hours": -1 })).is_err());
}

#[test]
fn attached_files_must_exist_and_are_sent_as_links() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("review.md"), "# Review").unwrap();
    let folder = dir.path().to_string_lossy().into_owned();
    let files = attach(&json!({ "files": ["review.md"] }), &folder).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].mime, "text/markdown");
    assert!(files[0].path.as_deref().unwrap().ends_with("review.md"));
    assert!(attach(&json!({ "files": ["missing.md"] }), &folder).is_err());
    assert!(attach(&json!({}), &folder).unwrap().is_empty());
}

#[test]
fn folders_compare_across_slashes_and_case() {
    assert!(same_folder("D:\\Dev\\Atlas\\", "d:/dev/atlas"));
    assert!(!same_folder("D:\\Dev\\Atlas", "D:\\Dev\\Atlas2"));
}

#[test]
fn links_escape_folders() {
    assert_eq!(link("thread", "abc-123"), "hub://thread/abc-123");
    assert_eq!(link("project", r"D:\Dev\hub dev"), "hub://project/D%3A%5CDev%5Chub%20dev");
}
