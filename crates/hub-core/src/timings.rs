//! Where the time of an answer goes, read from the event log.
//!
//! Every message, engine start, model call, tool call and turn end is already logged with its
//! time, so how long a turn took in each part needs no new instrument: this module reads the
//! log. Only the engine's own start is measured where it happens (`SessionStarted.start_ms`).
//! Nothing here changes what old events mean.
//!
//! A turn is cut like this, all in milliseconds:
//!
//! - `engine_ms`: from the message to the session being open. Only the first turn of an engine
//!   has it (a warm engine, or one still running from the last turn, has none).
//! - `model_ms`: from the turn starting to the first model call answering (the engine's first
//!   `usage_update`). This is where Claude Code's own start-up, its MCP servers and the model's
//!   first token wait.
//! - `tool_ms` and `text_ms`: until the first tool call, and until the first words of the answer.
//! - `done_ms`: until the turn ended.
//!
//! `usage` is the tokens the engine said the turn used, from the turn's end (`TurnEndData.usage`).
//!
//! <!-- see docs/ARCHITECTURE.md "Where the time goes" -->

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;

use crate::hub::Hub;
use crate::model::{Event, EventKind, Spread, ThreadTiming, TimingGroup, TimingSummary, ToolTiming, TurnTiming};

/// The turns in a thread's log, oldest first.
pub fn turns(events: &[Event]) -> Vec<TurnTiming> {
    let mut out: Vec<TurnTiming> = Vec::new();
    let mut sent: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    // The last engine start and how many commands the engine listed, before a turn.
    let mut session: Option<&Event> = None;
    let mut commands: Option<i64> = None;
    let mut open: Option<(usize, i64)> = None;
    let mut tools: std::collections::HashMap<String, (usize, i64)> = std::collections::HashMap::new();

    for e in events {
        match e.kind {
            EventKind::UserMessage => {
                sent.insert(e.seq, e.ts);
            }
            EventKind::SessionStarted => {
                session = Some(e);
            }
            EventKind::TurnStart => {
                let prompt = e.prompt().unwrap_or(0);
                let sent_at = sent.get(&prompt).copied();
                // An engine that came up for this message: its start is part of the wait.
                let engine_ms = match (session, sent_at) {
                    (Some(s), Some(at)) if s.seq > prompt => Some((s.ts - at).max(0)),
                    _ => None,
                };
                let start_ms = session.filter(|_| engine_ms.is_some()).and_then(|s| s.data_as::<crate::model::SessionStartedData>()?.start_ms);
                out.push(TurnTiming {
                    prompt,
                    at: sent_at.unwrap_or(e.ts),
                    engine_ms,
                    start: start_ms,
                    begin_ms: sent_at.map(|at| (e.ts - at).max(0)),
                    model_ms: None,
                    tool_ms: None,
                    text_ms: None,
                    done_ms: None,
                    stop: None,
                    model: None,
                    context: None,
                    commands,
                    tools: Vec::new(),
                    usage: None,
                });
                open = Some((out.len() - 1, e.ts));
                tools.clear();
            }
            EventKind::TurnEnd => {
                if let Some((i, t0)) = open.take() {
                    out[i].done_ms = Some((e.ts - t0).max(0));
                    out[i].stop = e.stop_reason().map(str::to_owned);
                    out[i].usage = e.data_as::<crate::model::TurnEndData>().and_then(|end| end.usage);
                }
            }
            EventKind::Update => {
                let kind = crate::model::update_kind(&e.data).unwrap_or("");
                let Some((i, t0)) = open else {
                    if kind == "available_commands_update" {
                        commands = e.data["availableCommands"].as_array().map(|a| a.len() as i64);
                    }
                    continue;
                };
                let rel = (e.ts - t0).max(0);
                let turn = &mut out[i];
                match kind {
                    "usage_update" if turn.model_ms.is_none() => {
                        turn.model_ms = Some(rel);
                        turn.context = e.data["used"].as_i64();
                        turn.model = crate::connections::update_model(&e.data);
                    }
                    "agent_message_chunk" if turn.text_ms.is_none() => turn.text_ms = Some(rel),
                    "tool_call" => {
                        if turn.tool_ms.is_none() {
                            turn.tool_ms = Some(rel);
                        }
                        if let Some(id) = e.data["toolCallId"].as_str() {
                            let name = e.data["name"].as_str().or_else(|| e.data["title"].as_str()).unwrap_or("tool").to_owned();
                            turn.tools.push(ToolTiming { name, ms: None, failed: false });
                            tools.insert(id.to_owned(), (turn.tools.len() - 1, e.ts));
                        }
                    }
                    "tool_call_update" => {
                        let status = e.data["status"].as_str().unwrap_or("");
                        if matches!(status, "completed" | "failed")
                            && let Some(id) = e.data["toolCallId"].as_str()
                            && let Some((n, at)) = tools.remove(id)
                        {
                            turn.tools[n].ms = Some((e.ts - at).max(0));
                            turn.tools[n].failed = status == "failed";
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// The middle and the slow end of a list: the median and the 90th percentile. None when empty.
pub fn spread(mut values: Vec<i64>) -> Option<(i64, i64)> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let at = |p: f64| values[((values.len() - 1) as f64 * p).round() as usize];
    Some((at(0.5), at(0.9)))
}

/// How many of the latest threads the summary reads, and how much of each one's log: the first
/// answer is in the first events, and a log can run to many thousands.
const THREADS: usize = 40;
const EVENTS: i64 = 1_500;

/// The first answers of the latest threads, summed up by engine and kind of thread.
pub async fn summary(hub: &Arc<Hub>) -> Result<TimingSummary> {
    let root = hub.projects_root().await.unwrap_or_default();
    let mut recent = Vec::new();
    for thread in hub.store.threads().await?.into_iter().take(THREADS) {
        let events = hub.store.events(&thread.id, 0, EVENTS).await?;
        let Some(turn) = turns(&events).into_iter().find(|t| t.done_ms.is_some() || t.model_ms.is_some()) else { continue };
        let kind = if crate::connections::is_chat_folder(&root, &thread.cwd) { "chat" } else { "dev" };
        recent.push(ThreadTiming { thread: thread.id, title: thread.title, engine: thread.engine, kind: kind.to_owned(), turn });
    }
    let mut by: BTreeMap<(String, String), Vec<&TurnTiming>> = BTreeMap::new();
    for r in &recent {
        by.entry((r.engine.clone(), r.kind.clone())).or_default().push(&r.turn);
    }
    let part = |turns: &[&TurnTiming], f: fn(&TurnTiming) -> Option<i64>| {
        spread(turns.iter().filter_map(|t| f(t)).collect()).map(|(p50, p90)| Spread { p50, p90 })
    };
    let groups = by
        .iter()
        .map(|((engine, kind), t)| TimingGroup {
            engine: engine.clone(),
            kind: kind.clone(),
            n: t.len() as i64,
            engine_ms: part(t, |t| t.engine_ms),
            model_ms: part(t, |t| t.model_ms),
            text_ms: part(t, |t| t.text_ms),
            done_ms: part(t, |t| t.done_ms),
        })
        .collect();
    Ok(TimingSummary { groups, recent })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn ev(seq: i64, ts: i64, kind: EventKind, data: Value) -> Event {
        Event { thread_id: "t".into(), seq, ts, kind, data }
    }

    #[test]
    fn cuts_a_cold_first_turn_into_its_parts() {
        let log = vec![
            ev(1, 0, EventKind::UserMessage, json!({ "text": "hej" })),
            ev(2, 7_450, EventKind::Update, json!({ "sessionUpdate": "available_commands_update", "availableCommands": [{}, {}, {}] })),
            ev(3, 7_600, EventKind::SessionStarted, json!({ "start_ms": { "initialize": 4000, "session": 3000, "total": 7000 } })),
            ev(4, 7_700, EventKind::TurnStart, json!({ "prompt": 1 })),
            ev(5, 15_500, EventKind::Update, json!({ "sessionUpdate": "usage_update", "used": 44_199, "_meta": { "_claude/model": "claude-haiku-5-5" } })),
            ev(6, 17_790, EventKind::Update, json!({ "sessionUpdate": "tool_call", "toolCallId": "a", "name": "WebSearch" })),
            ev(7, 28_070, EventKind::Update, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "a", "status": "completed" })),
            ev(8, 32_980, EventKind::Update, json!({ "sessionUpdate": "agent_message_chunk" })),
            ev(9, 37_000, EventKind::TurnEnd, json!({ "prompt": 1, "stop_reason": "end_turn" })),
        ];
        let t = turns(&log);
        assert_eq!(t.len(), 1);
        let t = &t[0];
        assert_eq!(t.engine_ms, Some(7_600));
        assert_eq!(t.begin_ms, Some(7_700));
        assert_eq!(t.model_ms, Some(7_800));
        assert_eq!(t.tool_ms, Some(10_090));
        assert_eq!(t.text_ms, Some(25_280));
        assert_eq!(t.done_ms, Some(29_300));
        assert_eq!((t.context, t.commands), (Some(44_199), Some(3)));
        assert_eq!(t.model.as_deref(), Some("claude-haiku-5-5"));
        assert_eq!((t.tools[0].name.as_str(), t.tools[0].ms), ("WebSearch", Some(10_280)));
        assert_eq!(t.start.as_ref().unwrap().total, 7000);
    }

    #[test]
    fn a_second_turn_on_a_running_engine_has_no_engine_wait() {
        let log = vec![
            ev(1, 0, EventKind::UserMessage, json!({})),
            ev(2, 100, EventKind::SessionStarted, json!({})),
            ev(3, 150, EventKind::TurnStart, json!({ "prompt": 1 })),
            ev(4, 900, EventKind::TurnEnd, json!({ "prompt": 1, "stop_reason": "end_turn" })),
            ev(5, 5_000, EventKind::UserMessage, json!({})),
            ev(6, 5_020, EventKind::TurnStart, json!({ "prompt": 5 })),
            ev(7, 6_000, EventKind::TurnEnd, json!({ "prompt": 5, "stop_reason": "end_turn" })),
        ];
        let t = turns(&log);
        assert_eq!((t[0].engine_ms, t[1].engine_ms), (Some(100), None));
        assert_eq!((t[1].begin_ms, t[1].done_ms), (Some(20), Some(980)));
    }

    #[test]
    fn a_turn_shows_the_tokens_its_end_carried_and_one_without_shows_none() {
        let log = vec![
            ev(1, 0, EventKind::UserMessage, json!({})),
            ev(2, 10, EventKind::TurnStart, json!({ "prompt": 1 })),
            ev(3, 900, EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 1, "usage": { "input": 4, "output": 105, "cache_read": 58_290, "cache_write": 58_397 } })),
            ev(4, 1_000, EventKind::UserMessage, json!({})),
            ev(5, 1_010, EventKind::TurnStart, json!({ "prompt": 4 })),
            ev(6, 2_000, EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 4 })),
        ];
        let t = turns(&log);
        let usage = t[0].usage.as_ref().expect("the first turn's tokens");
        assert_eq!((usage.input, usage.output, usage.cache_read, usage.cache_write), (Some(4), Some(105), Some(58_290), Some(58_397)));
        assert_eq!(t[1].usage, None);
        // An old reader of the API never sees the key for a turn without it.
        assert!(serde_json::to_value(&t[1]).unwrap().get("usage").is_none());
    }

    #[test]
    fn spread_gives_median_and_slow_end() {
        assert_eq!(spread(vec![]), None);
        assert_eq!(spread(vec![5]), Some((5, 5)));
        let (p50, p90) = spread((1..=10).collect()).unwrap();
        assert_eq!((p50, p90), (6, 9));
    }
}
