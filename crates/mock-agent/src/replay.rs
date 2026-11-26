//! Replays a recorded ACP session (`trace_engines` in hub.json writes one per thread as
//! `<data>/engines/<thread>.trace.jsonl`, lines of `{ from: "hub" | "agent", ts, msg }`).
//!
//! The agent side of the recording is played back in order: each message Hub sends (a request or
//! a notification) is answered with the agent lines the recording holds between that message and
//! the next one Hub sent. Responses get the id of the request being answered, so Hub's own
//! numbering may differ from the recording's; requests the agent made (permission) keep theirs.
//! It does not look at what Hub sent, so a replay checks that Hub still understands what a real
//! adapter said, not that it still says what it said.

use std::io::Write;
use std::path::Path;

use serde_json::Value;

/// The agent lines to send for each message Hub sends, in the order Hub sent them.
fn segments(trace: &str) -> Vec<Vec<Value>> {
    let mut out: Vec<Vec<Value>> = Vec::new();
    for line in trace.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(entry) = serde_json::from_str::<Value>(line) else { continue };
        let msg = entry["msg"].clone();
        match entry["from"].as_str() {
            // Only what Hub asked or told starts a segment; its answers to the agent's requests do not.
            Some("hub") if msg["method"].is_string() => out.push(Vec::new()),
            Some("agent") => {
                if let Some(last) = out.last_mut() {
                    last.push(msg);
                }
            }
            _ => {}
        }
    }
    out
}

/// What goes out when Hub sent `incoming`: the segment's lines, responses re-addressed.
fn answer(segment: &[Value], incoming: &Value) -> Vec<Value> {
    segment
        .iter()
        .cloned()
        .map(|mut msg| {
            let response = msg.get("method").is_none() && msg.get("id").is_some();
            if response && let Some(id) = incoming.get("id") {
                msg["id"] = id.clone();
            }
            msg
        })
        .collect()
}

/// Plays `trace` on stdio until Hub closes it.
pub async fn serve(trace: &Path) -> std::io::Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let text = std::fs::read_to_string(trace)?;
    let mut segments = segments(&text).into_iter();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = std::io::stdout();
    while let Some(line) = lines.next_line().await? {
        let Ok(incoming) = serde_json::from_str::<Value>(&line) else { continue };
        if incoming.get("method").is_none() {
            continue; // Hub answering a request the agent made
        }
        let Some(segment) = segments.next() else { continue };
        for msg in answer(&segment, &incoming) {
            writeln!(stdout, "{msg}")?;
        }
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(from: &str, msg: Value) -> String {
        json!({ "from": from, "ts": 1, "msg": msg }).to_string()
    }

    #[test]
    fn agent_lines_follow_the_hub_message_they_came_after() {
        let trace = [
            line("agent", json!({ "method": "ignored: before anything" })),
            line("hub", json!({ "id": 0, "method": "initialize" })),
            line("agent", json!({ "id": 0, "result": { "ok": 1 } })),
            line("hub", json!({ "id": 1, "method": "session/prompt" })),
            line("agent", json!({ "method": "session/update", "params": {} })),
            line("agent", json!({ "method": "session/request_permission", "id": 9 })),
            line("hub", json!({ "id": 9, "result": { "outcome": "allow" } })),
            line("agent", json!({ "id": 1, "result": { "stopReason": "end_turn" } })),
        ]
        .join("\n");
        let segments = segments(&trace);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].len(), 1);
        assert_eq!(segments[1].len(), 3, "Hub's answer to the agent does not split the turn");
    }

    #[test]
    fn responses_take_the_id_asked_and_requests_keep_theirs() {
        let segment = vec![json!({ "method": "session/request_permission", "id": 9 }), json!({ "id": 1, "result": {} })];
        let sent = answer(&segment, &json!({ "id": 42, "method": "session/prompt" }));
        assert_eq!(sent[0]["id"], 9);
        assert_eq!(sent[1]["id"], 42);
    }
}
