//! Tests of Hub's MCP tools.

use serde_json::{Value, json};

use super::talk::{replies, steps_plan};
use crate::auth::with_token;
use crate::tool_instructions::{INSTRUCTIONS, PICTURES, WRITING_MARK, chat_instructions};

#[test]
fn a_chat_is_told_how_to_talk_not_how_a_coding_thread_writes() {
    let chat = chat_instructions(true);
    assert!(chat.contains("This is an everyday chat"), "the chat style is there");
    assert!(chat.contains("hub_brief"), "the tools are still described");
    assert!(chat.contains(PICTURES), "pictures still go to a side job");
    assert!(!chat.contains("Writing for Hub"), "no status headings, notes or checklists");
    assert!(!chat.contains("**Needs you**"));
    assert!(INSTRUCTIONS.contains(WRITING_MARK), "the split point exists, or a chat would get the rules too");
    assert!(!chat_instructions(false).contains(PICTURES));
}

#[test]
fn the_token_can_ride_in_the_address() {
    assert_eq!(with_token("http://127.0.0.1:1/mcp/t1", "abc"), "http://127.0.0.1:1/mcp/t1?token=abc");
    assert_eq!(with_token("http://h/mcp/t1?x=1", "abc"), "http://h/mcp/t1?x=1&token=abc");
}

#[test]
fn steps_become_a_plan_update_the_card_reads() {
    let plan = steps_plan(&json!({ "steps": [
        { "content": "  Read   the code ", "status": "completed" },
        { "content": "Write it", "status": "in_progress" },
        { "content": "Test", "status": "whatever" },
        { "content": "   ", "status": "pending" },
    ] }))
    .unwrap();
    assert_eq!(plan["sessionUpdate"], "plan");
    let entries = plan["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "a blank step is dropped");
    assert_eq!(entries[0]["content"], "Read the code");
    assert_eq!(entries[1]["status"], "in_progress");
    assert_eq!(entries[2]["status"], "pending", "an unknown status reads as not started");
    assert!(steps_plan(&json!({ "steps": [] })).is_err());
    assert!(steps_plan(&json!({})).is_err());
}

#[test]
fn offered_replies_are_cleaned_and_bounded() {
    let offer = replies(&json!({
        "question": "Should I start a new thread?",
        "options": [{ "label": "Go ahead" }, { "label": "Go ahead and close this one", "message": "Go ahead, then close this thread." }, "No"],
    }))
    .unwrap();
    let options = &offer.options;
    assert_eq!(options.len(), 3);
    assert_eq!(options[0].message, "Go ahead", "a label alone is what a click sends");
    assert_eq!(options[1].message, "Go ahead, then close this thread.");
    assert_eq!(options[2].label, "No");
    assert!(replies(&json!({ "question": "Ok?", "options": [{ "label": "Yes" }] })).is_err());
    assert!(replies(&json!({ "options": [{ "label": "Yes" }, { "label": "No" }] })).is_err());
    let five: Vec<Value> = (0..5).map(|n| json!({ "label": n.to_string() })).collect();
    assert!(replies(&json!({ "question": "Which?", "options": five })).is_err());
}
