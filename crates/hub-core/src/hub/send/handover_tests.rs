//! Guards the handover a fresh session gets (`handover_log` in `send.rs`): a short thread goes
//! whole by the old rules, a long one keeps its start and end within the budget and names the gap
//! and the user's messages in it for `read_thread`, and those asks are cut and the rest counted.

use crate::model::{Event, EventKind};
use serde_json::json;

fn ev(seq: i64, kind: EventKind, data: serde_json::Value) -> Event {
    Event { thread_id: "t".into(), seq, ts: seq, kind, data }
}
fn said(seq: i64, text: &str) -> Event {
    ev(seq, EventKind::UserMessage, json!({ "text": text }))
}
fn answered(seq: i64, text: &str) -> Event {
    ev(seq, EventKind::Update, json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": text } }))
}

#[test]
fn a_short_thread_is_handed_over_whole_by_the_old_rules() {
    let events = vec![said(1, "fix the bug"), answered(2, "done"), ev(3, EventKind::TurnEnd, json!({}))];
    let (log, recipe) = super::handover_log("t", &events);
    assert_eq!(recipe, "legacy");
    assert_eq!(log, super::conversation_log(&events));
}

#[test]
fn a_long_thread_keeps_its_start_and_end_and_names_what_it_left_out() {
    let mut events = vec![said(1, "the goal: port the parser")];
    for n in 0..60 {
        events.push(answered(2 + n * 2, &format!("middle turn {n} {}", "x".repeat(900))));
        events.push(said(3 + n * 2, &format!("go on {n}")));
    }
    events.push(answered(500, "the latest state: all done but the docs"));
    let (log, recipe) = super::handover_log("t-1", &events);
    assert_eq!(recipe, "handover-v1");
    assert!(log.chars().count() <= super::HANDOVER_CHARS + super::ASKS_CHARS + 600, "kept within its budget: {}", log.chars().count());
    assert!(log.contains("the goal: port the parser"), "the start says what the work is");
    assert!(log.contains("the latest state: all done but the docs"), "the end says where it stands");
    assert!(log.contains("read_thread with thread_id t-1 and after_seq"), "the gap says where to read it");
    assert!(!log.contains("middle turn 30 "), "the middle is not carried");
    assert!(log.contains("What the person asked in between:"), "what was asked in the gap is listed");
    assert!(log.contains("- go on 20"), "a left-out message of the person is named");
}

#[test]
fn the_asks_in_the_gap_are_cut_and_the_rest_counted() {
    let turns: Vec<(i64, String)> = (0..40).map(|n| (n, format!("\n\nUser: {n} {}", "word ".repeat(80)))).collect();
    let out = super::left_out_asks(&turns);
    assert!(out.chars().count() <= super::ASKS_CHARS + 200);
    assert!(out.contains("…"), "a long message is cut");
    assert!(out.contains("more"), "the ones that did not fit are counted");
    assert_eq!(super::left_out_asks(&[(1, "\n\nAgent: hi".into())]), "");
}
