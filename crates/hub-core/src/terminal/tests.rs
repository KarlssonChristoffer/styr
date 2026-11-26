//! Guards taking a thread over in a terminal and handing it back: a stopped turn carries on unless
//! the terminal finished it, only a thread back from a terminal is imported, the tail after the
//! known prompts is what is new, and the terminal view draws the conversation.

use super::*;

fn ev(seq: i64, kind: EventKind, data: Value) -> Event {
    Event { thread_id: "t".into(), seq, ts: 0, kind, data }
}

fn chunk(kind: &str, text: &str) -> Value {
    json!({ "sessionUpdate": kind, "content": { "type": "text", "text": text } })
}

#[test]
fn short_ids_use_the_random_end() {
    assert_eq!(short_id("0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b"), "2e3f4a5b");
    assert_eq!(tmux_session("0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b"), "hub-2e3f4a5b");
}

#[test]
fn a_stopped_turn_carries_on_unless_the_terminal_finished_it() {
    let mut log = vec![
        ev(1, EventKind::UserMessage, json!({ "text": "fix it" })),
        ev(2, EventKind::TurnStart, json!({ "prompt": 1 })),
        ev(3, EventKind::TurnEnd, json!({ "stop_reason": "cancelled", "prompt": 1 })),
        ev(4, EventKind::Terminal, json!({ "state": "taken", "cut": 1 })),
        ev(5, EventKind::Terminal, json!({ "state": "back" })),
    ];
    // Nothing said in the terminal, or only the CLI's note that it was interrupted.
    assert_eq!(carry_on_after_terminal(&log, &[]), Some(4));
    let interrupted = [chunk("user_message_chunk", "[Request interrupted by user for tool use]")];
    assert_eq!(carry_on_after_terminal(&log, &interrupted), Some(4));
    let stopped_later = [
        chunk("user_message_chunk", "run the tests"),
        chunk("agent_message_chunk", "Running"),
        chunk("user_message_chunk", "[Request interrupted by user]"),
    ];
    assert_eq!(carry_on_after_terminal(&log, &stopped_later), Some(4));
    // A request the terminal saw through: the person steered it, leave it.
    let steered = [chunk("user_message_chunk", "stop, do it this way"), chunk("agent_message_chunk", "Done.")];
    assert_eq!(carry_on_after_terminal(&log, &steered), None);
    // Asked once per takeover.
    log.push(ev(6, EventKind::UserMessage, json!({ "text": CARRY_ON_TERMINAL, "terminal": 4 })));
    assert_eq!(carry_on_after_terminal(&log, &[]), None);
    // Taken while idle: nothing was stopped.
    let idle = vec![
        ev(1, EventKind::UserMessage, json!({ "text": "a" })),
        ev(2, EventKind::TurnEnd, json!({})),
        ev(3, EventKind::Terminal, json!({ "state": "taken" })),
        ev(4, EventKind::Terminal, json!({ "state": "back" })),
    ];
    assert_eq!(carry_on_after_terminal(&idle, &[]), None);
}

#[test]
fn only_a_thread_back_from_a_terminal_needs_importing() {
    let mut log = vec![
        ev(1, EventKind::UserMessage, json!({ "text": "a" })),
        ev(2, EventKind::SessionStarted, json!({ "resumed": false })),
        ev(3, EventKind::TurnEnd, json!({})),
        ev(4, EventKind::UserMessage, json!({ "text": "b" })),
        ev(5, EventKind::TurnEnd, json!({})),
    ];
    assert_eq!(pending_import(&log), None);
    log.push(ev(6, EventKind::Terminal, json!({ "state": "taken" })));
    log.push(ev(7, EventKind::Terminal, json!({ "state": "back" })));
    assert_eq!(pending_import(&log), Some(2));
    // Three prompts in the session once the terminal's were copied in; one more from Hub.
    log.push(ev(8, EventKind::Terminal, json!({ "state": "imported", "turns": 1, "replay_turns": 3 })));
    assert_eq!(pending_import(&log), None);
    log.push(ev(9, EventKind::UserMessage, json!({ "text": "c" })));
    log.push(ev(10, EventKind::Terminal, json!({ "state": "back" })));
    assert_eq!(pending_import(&log), Some(4));
    // A session that could not be loaded starts over.
    log.push(ev(11, EventKind::SessionStarted, json!({ "resumed": false })));
    log.push(ev(12, EventKind::Terminal, json!({ "state": "imported", "turns": 0, "replay_turns": 0 })));
    log.push(ev(13, EventKind::Terminal, json!({ "state": "back" })));
    assert_eq!(pending_import(&log), Some(0));
}

#[test]
fn a_session_nobody_has_prompted_has_no_conversation_to_resume() {
    // Started ahead of the first message, and again after a move or a switch of engine: what
    // the thread said before went to other sessions.
    let mut log = vec![ev(1, EventKind::SessionStarted, json!({ "resumed": false }))];
    assert_eq!(session_prompts(&log), 0);
    log.push(ev(2, EventKind::UserMessage, json!({ "text": "a" })));
    assert_eq!(session_prompts(&log), 1);
    log.push(ev(3, EventKind::TurnEnd, json!({})));
    log.push(ev(4, EventKind::Moved, json!({})));
    log.push(ev(5, EventKind::SessionStarted, json!({ "resumed": false })));
    assert_eq!(session_prompts(&log), 0);
    // A message waiting for the new session reaches it.
    log.push(ev(6, EventKind::UserMessage, json!({ "text": "b" })));
    assert_eq!(session_prompts(&log), 1);
}

#[test]
fn an_engine_saved_without_a_terminal_command_gets_its_harness() {
    use crate::config::EngineConfig;
    let saved = EngineConfig { harness: Some("hermes".into()), terminal: None, ..Default::default() };
    assert_eq!(terminal_command("hermes-deepseek", &saved).as_deref(), Some("hermes --resume {session}"));
    let own = EngineConfig { terminal: Some("my-cli {session}".into()), ..saved.clone() };
    assert_eq!(terminal_command("hermes-deepseek", &own).as_deref(), Some("my-cli {session}"));
    assert_eq!(terminal_command("gemini", &EngineConfig::default()), None);
}

#[test]
fn the_tail_is_what_came_after_the_known_prompts() {
    let replayed = vec![
        chunk("user_message_chunk", "a"),
        chunk("agent_message_chunk", "A"),
        chunk("user_message_chunk", "b"),
        chunk("user_message_chunk", " more of b"),
        json!({ "sessionUpdate": "tool_call", "toolCallId": "1", "title": "Read" }),
        json!({ "sessionUpdate": "usage_update", "used": 1 }),
        chunk("user_message_chunk", "in the terminal"),
        chunk("agent_message_chunk", "answer"),
    ];
    let (tail, total) = terminal_tail(&replayed, 2);
    assert_eq!(total, 3);
    assert_eq!(tail, vec![chunk("user_message_chunk", "in the terminal"), chunk("agent_message_chunk", "answer")]);
    assert_eq!(turns_in(&tail), 1);
    let (tail, _) = terminal_tail(&replayed, 1);
    assert_eq!(turns_in(&tail), 2);
    assert_eq!(tail.len(), 5, "usage updates are left out");
    assert!(terminal_tail(&replayed, 3).0.is_empty());
}

#[test]
fn the_terminal_view_draws_the_conversation() {
    let mut v = View::default();
    let out: String = [
        ev(1, EventKind::UserMessage, json!({ "text": "fix it" })),
        ev(2, EventKind::Update, chunk("agent_message_chunk", "On it")),
        ev(3, EventKind::Update, json!({ "sessionUpdate": "tool_call", "toolCallId": "x", "title": "Edit a.rs" })),
        ev(4, EventKind::Update, json!({ "sessionUpdate": "tool_call", "toolCallId": "x", "title": "Edit a.rs" })),
        ev(5, EventKind::TurnEnd, json!({})),
    ]
    .iter()
    .map(|e| v.render(e))
    .collect();
    assert_eq!(out, "\n› fix it\nOn it\n  · Edit a.rs\n  — done\n");
}
