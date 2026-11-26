//! Readings: a decision model (an API connection that answers questions, `apis.rs`, Jev) reads
//! something for one of Hub's own decisions, and Hub goes by it only when the model is sure.
//! When it is unsure, slow or not connected, Hub does what it did without it. Every answer is
//! kept with `Hub::choose`, so what it said can be looked back at.
//!
//! Four live here. Where a finished turn left the work: done, waiting on your answer, stuck, or
//! stopped half way, so the lists can tell a thread that needs you from one that finished. And
//! which line of the project's stage a new thread works towards, which makes the focus check
//! (`focus.rs`) answer in a moment instead of asking a model on your subscription.

use std::sync::Arc;
use std::time::Duration;

use crate::focus::Stage;
use crate::hub::Hub;
use crate::model::{Event, EventKind, ReadingData, ThreadStatus};
use crate::text::cut;

/// Where a turn can leave the work, with what each means to the model.
pub const TURN_STATES: [(&str, &str); 4] = [
    ("done", "Says the requested work is finished, or fully answers the question, with nothing left for the person but to look"),
    ("asks", "Asks the person a question, or waits for them to decide, approve, test or provide something before it can go on"),
    ("stuck", "Could not do the work: an error, a blocker or missing access it could not get around"),
    ("partway", "Stopped with the work unfinished without asking anything, for example saying what is left or what it will do next"),
];

/// How sure the model must be for Hub to go by a reading.
const SURE: f64 = 0.6;
/// A reading that interrupts the agent or the person needs more.
const VERY_SURE: f64 = 0.75;
/// Nothing waits on these, so they can take a moment.
const WAIT: Duration = Duration::from_secs(8);

/// The message a turn answered and the agent's reply to it, from a thread's log.
pub fn turn_text(events: &[Event], end: i64) -> Option<(String, String)> {
    let asked = events.iter().rev().find(|e| e.seq < end && e.kind == EventKind::UserMessage)?;
    let reply: String = events
        .iter()
        .filter(|e| e.seq > asked.seq && e.seq < end)
        .filter(|e| e.kind == EventKind::Update && crate::model::update_kind(&e.data) == Some("agent_message_chunk"))
        .filter_map(|e| e.data["content"]["text"].as_str())
        .collect();
    Some((asked.said().unwrap_or_default().to_owned(), reply))
}

/// The agent's last sentences, for the decision's quote: what the reading mostly went by.
pub(crate) fn last_words(reply: &str) -> String {
    let flat = reply.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = flat.chars().count();
    if count <= 150 {
        return flat;
    }
    let tail: String = flat.chars().skip(count - 150).collect();
    // Start at a sentence or a word, not in the middle of one.
    let start = tail.find(". ").map(|i| i + 2).filter(|&i| i < tail.len() - 20).or_else(|| tail.find(' ').map(|i| i + 1)).unwrap_or(0);
    format!("…{}", &tail[start..])
}

/// What the model is shown of a turn: the ask, then the end of the reply, where it sums up.
pub fn turn_state(asked: &str, reply: &str) -> String {
    format!("Request:\n{}\n\nReply:\n{}", cut(asked, 1_500, false), cut(reply, 3_500, true))
}

/// The choices for a stage: its open "Delivered when" lines and its ideas, then "none". The
/// value next to each name is what it means to the model; the line is where it is in the file.
pub fn stage_options(stage: &Stage) -> Vec<(String, String, Option<u32>)> {
    let mut options: Vec<(String, String, Option<u32>)> = stage
        .done
        .iter()
        .filter(|c| !c.done)
        .enumerate()
        .map(|(i, c)| (format!("goal{}", i + 1), c.text.clone(), Some(c.line)))
        .collect();
    if options.is_empty() {
        return options;
    }
    options.extend(stage.ideas.iter().enumerate().map(|(i, idea)| (format!("idea{}", i + 1), idea.text.clone(), None)));
    options.push((
        "none".into(),
        "Works towards none of these: a different goal, a new idea, or something outside this stage".into(),
        None,
    ));
    options
}

impl Hub {
    /// Reads where the turn that ended at `end` left the work, and marks the thread when the
    /// model is sure. A thread that asks you or is stuck comes back from the settled shelf and
    /// from a snooze, like one that needs a permission answer.
    pub(crate) async fn read_turn(self: Arc<Self>, end: Event) {
        if !end.ended_well() {
            return;
        }
        let Ok(Some(thread)) = self.store.thread(&end.thread_id).await else { return };
        // A side job's result goes back to its thread, which reads it there.
        if thread.parent.is_some() {
            return;
        }
        // The message that began the turn and what came after it, not the whole log.
        let Ok(said) = self.store.events_of(&thread.id, 0, &[EventKind::UserMessage]).await else { return };
        let Some(asked) = said.iter().rev().find(|e| e.seq < end.seq) else { return };
        // `turn_text` reads that message and the reply chunks after it.
        let Ok(chunks) = self.store.reply_chunks(&thread.id, asked.seq, end.seq).await else { return };
        let events: Vec<Event> = std::iter::once(asked.clone()).chain(chunks).collect();
        let Some((asked, reply)) = turn_text(&events, end.seq) else { return };
        if reply.trim().is_empty() {
            return;
        }
        let options: Vec<(String, String)> = TURN_STATES.iter().map(|(n, m)| (n.to_string(), m.to_string())).collect();
        let instructions = "Where does the coding assistant's work stand at the end of its reply";
        let state = turn_state(&asked, &reply);
        let Some(c) = self.choose("turns", Some(&thread.id), instructions, &state, &last_words(&reply), &options, "", SURE, WAIT).await else {
            return;
        };
        let data = ReadingData { seq: Some(end.seq), state: Some(c.pick.clone()), ..read_as("turn", &c) };
        if let Err(e) = self.emit_typed(&thread.id, &data).await {
            tracing::warn!(thread = %thread.id, "logging a turn reading: {e:#}");
        }
        if !c.acted {
            return;
        }
        let (pick, at) = (c.pick, end.ts);
        let needs_you = pick == "asks" || pick == "stuck";
        let marked = self
            .update_thread(&thread.id, move |t| {
                // Only while that turn is still the last word: work sent since starts afresh.
                if t.status != ThreadStatus::Idle || t.sent_at.is_some_and(|sent| sent > at) {
                    return;
                }
                t.reading = Some(pick);
                if needs_you {
                    t.settled_at = None;
                    t.snoozed_until = None;
                }
            })
            .await;
        if let Err(e) = marked {
            tracing::warn!(thread = %thread.id, "marking a turn reading: {e:#}");
        }
    }

    /// Whether a test run that exited cleanly printed a failure anyway, as when its output went
    /// through `| tail` and the pipe's exit code won. Only runs whose output holds a word that
    /// may mean failure are asked about (`proof::may_hide_failure`).
    pub(crate) async fn hides_failure(&self, thread_id: &str, command: &str, output: &str) -> bool {
        if !crate::proof::may_hide_failure(output) {
            return false;
        }
        let options = [
            ("passed", "Everything that ran passed. Any mention of failures is a count of zero, a name or a description"),
            ("failed", "At least one test or check failed, or the run hit an error such as a compile error"),
        ]
        .map(|(n, m)| (n.to_owned(), m.to_owned()));
        let instructions = "Does the end of this command's output show that the tests or checks failed";
        let state = format!("Command: {command}\n\nEnd of its output:\n{output}");
        let c = self.choose("proof", Some(thread_id), instructions, &state, command, &options, "passed", VERY_SURE, WAIT).await;
        c.is_some_and(|c| c.acted && c.pick == "failed")
    }

    /// Reads whether the message `seq`, which you wrote to a thread that has finished a turn,
    /// starts a different task than the thread has been on. When the model is sure it did, a
    /// `reading` event offers a thread of its own for it.
    pub(crate) async fn check_topic(self: Arc<Self>, thread_id: String, seq: i64, text: String) {
        if text.split_whitespace().count() < 6 {
            return;
        }
        // Only the user's messages and the turn ends are read whole; of the reply, just its text.
        let Ok(marks) = self.store.events_of(&thread_id, 0, &[EventKind::UserMessage, EventKind::TurnEnd]).await else { return };
        let Some(first) = marks.iter().find(|e| e.kind == EventKind::UserMessage && e.seq < seq) else { return };
        let Some(last_end) = marks.iter().rev().find(|e| e.kind == EventKind::TurnEnd && e.seq < seq) else { return };
        let Some(asked) = marks.iter().rev().find(|e| e.kind == EventKind::UserMessage && e.seq < last_end.seq) else { return };
        let Ok(chunks) = self.store.reply_chunks(&thread_id, asked.seq, last_end.seq).await else { return };
        let mut events: Vec<Event> = marks.iter().filter(|e| e.seq <= last_end.seq).cloned().chain(chunks).collect();
        events.sort_by_key(|e| e.seq);
        let Some((_, reply)) = turn_text(&events, last_end.seq) else { return };
        let started = cut(first.said().unwrap_or_default(), 700, false);
        let state = format!(
            "The thread began with:\n{started}\n\nIts latest reply ended:\n{}\n\nNew message:\n{}",
            cut(&reply, 900, true),
            cut(&text, 900, false)
        );
        let options = [
            ("same", "Carries on the work in this thread: a follow-up, a fix or adjustment, a question about it, the next step"),
            ("new", "A different task that has little to do with the work so far, and would fit a thread of its own"),
        ]
        .map(|(n, m)| (n.to_owned(), m.to_owned()));
        let instructions = "Is the new message about the same work as the thread or a different task";
        let Some(c) = self.choose("topic", Some(&thread_id), instructions, &state, &text, &options, "same", VERY_SURE, WAIT).await else {
            return;
        };
        if !(c.acted && c.pick == "new") {
            return;
        }
        let data = ReadingData { seq: Some(seq), state: Some("new".into()), acted: true, ..read_as("topic", &c) };
        if let Err(e) = self.emit_typed(&thread_id, &data).await {
            tracing::warn!(thread = %thread_id, "logging a topic reading: {e:#}");
        }
    }

    /// Asks the decision model which line of `stage` a new thread's first message works towards.
    /// Some(false, why) when it is sure the thread is off the stage, Some(true, _) when it is sure
    /// it is on it (the line is logged as a reading), None when it is unsure or not there.
    pub(crate) async fn focus_reading(&self, thread_id: &str, stage: &Stage, ask: &str) -> Option<(bool, String)> {
        let options = stage_options(stage);
        if options.is_empty() {
            return None;
        }
        let pairs: Vec<(String, String)> = options.iter().map(|(n, m, _)| (n.clone(), m.clone())).collect();
        let instructions = format!("Which goal of the project's current stage, \"{}\", does this new task work towards", stage.title);
        let c = self.choose("focus", Some(thread_id), &instructions, ask, ask, &pairs, "", SURE, WAIT).await?;
        if !c.acted {
            return None;
        }
        if c.pick == "none" {
            return Some((false, format!("not one of the stage's goals or ideas ({} is {:.0}% sure)", c.connection, c.confidence * 100.0)));
        }
        let (_, text, line) = options.iter().find(|(n, _, _)| n == &c.pick)?;
        let data = ReadingData { stage: Some(stage.title.clone()), line: Some(*line), text: Some(text.clone()), acted: true, ..read_as("focus", &c) };
        if let Err(e) = self.emit_typed(thread_id, &data).await {
            tracing::warn!(thread = %thread_id, "logging a focus reading: {e:#}");
        }
        Some((true, String::new()))
    }
}

/// What `choice` said for `feature`, as the reading to log; the caller fills in what it was about.
pub(crate) fn read_as(feature: &str, choice: &crate::apis::Choice) -> ReadingData {
    ReadingData {
        feature: feature.to_owned(),
        confidence: choice.confidence,
        connection: choice.connection.clone(),
        ms: choice.ms,
        acted: choice.acted,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus::{Check, Idea};
    use serde_json::json;

    fn ev(seq: i64, kind: EventKind, data: serde_json::Value) -> Event {
        Event { thread_id: "t".into(), seq, ts: seq, kind, data }
    }

    fn chunk(text: &str) -> serde_json::Value {
        json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": text } })
    }

    #[test]
    fn a_turn_is_the_last_ask_and_the_reply_to_it() {
        let events = vec![
            ev(1, EventKind::UserMessage, json!({ "text": "first" })),
            ev(2, EventKind::Update, chunk("old reply")),
            ev(3, EventKind::TurnEnd, json!({ "stop_reason": "end_turn" })),
            ev(4, EventKind::UserMessage, json!({ "text": "fix the build" })),
            ev(5, EventKind::Update, chunk("Fixed it. ")),
            ev(6, EventKind::Update, json!({ "sessionUpdate": "tool_call", "title": "cargo test" })),
            ev(7, EventKind::Update, chunk("Should I push?")),
            ev(8, EventKind::TurnEnd, json!({ "stop_reason": "end_turn" })),
        ];
        let (asked, reply) = turn_text(&events, 8).unwrap();
        assert_eq!(asked, "fix the build");
        assert_eq!(reply, "Fixed it. Should I push?");
        assert!(turn_text(&events[1..3], 3).is_none());
    }

    #[test]
    fn long_replies_keep_their_end() {
        let reply = format!("{}All done, tests pass.", "x".repeat(5_000));
        let state = turn_state("do it", &reply);
        assert!(state.ends_with("All done, tests pass."));
        assert!(state.chars().count() < 3_700);
    }

    #[test]
    fn a_stage_offers_its_open_goals_its_ideas_and_none() {
        let check = |text: &str, done: bool, line: u32| Check { text: text.into(), done, line };
        let mut stage = Stage {
            title: "Stage 2".into(),
            done: vec![check("Undo per thread", true, 4), check("Chat queue", false, 5)],
            ideas: vec![Idea { text: "Phone page".into(), line: 9 }],
            ..Default::default()
        };
        let names: Vec<String> = stage_options(&stage).into_iter().map(|o| o.0).collect();
        assert_eq!(names, ["goal1", "idea1", "none"]);
        assert_eq!(stage_options(&stage)[0].2, Some(5));
        stage.done[1].done = true;
        assert!(stage_options(&stage).is_empty());
    }

    /// A stand-in for Jev that answers by what the text says.
    async fn fake_jev() -> String {
        async fn answer(axum::Json(body): axum::Json<serde_json::Value>) -> axum::Json<serde_json::Value> {
            let state = body["state"].as_str().unwrap_or_default();
            let (choice, confidence) = if state.contains("Should I push?") {
                ("asks", 0.91)
            } else if state.contains("FAILED") {
                ("failed", 0.9)
            } else if state.contains("different job") {
                ("new", 0.85)
            } else if state.contains("same job") {
                ("same", 0.9)
            } else if state.contains("maybe") {
                ("done", 0.4)
            } else if state.contains("chat queue") {
                ("goal1", 0.8)
            } else if state.contains("weather") {
                ("none", 0.85)
            } else {
                ("work", 0.9)
            };
            axum::Json(json!({
                "model": "jev-test",
                "answers": { "pick": { "choice": choice, "confidence": confidence } },
                "usage": { "input_tokens": 120, "output_tokens": 0 },
            }))
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().route("/v1/systemone", axum::routing::post(answer));
        tokio::spawn(async move { axum::serve(listener, app).await });
        url
    }

    #[test]
    fn last_words_keep_the_end_of_a_reply_from_a_sentence_on() {
        assert_eq!(last_words("Done.\n\nAll  green."), "Done. All green.");
        let long = format!("{} The test now waits for the redirect itself. Ran it 50 times, all green.", "word ".repeat(60));
        let end = last_words(&long);
        assert!(end.starts_with('…') && end.ends_with("all green."), "{end}");
        assert!(end.chars().count() <= 151, "{end}");
        assert!(!end.contains("wor "), "starts at a word: {end}");
    }

    #[tokio::test]
    async fn a_sure_reading_marks_the_thread_and_an_unsure_one_does_not() -> anyhow::Result<()> {
        let data = tempfile::tempdir()?;
        let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
        let hub = Hub::new(crate::Store::in_memory()?, config);
        let k = &crate::apis::catalog()[0];
        let jev = crate::apis::ApiConnection {
            name: "jev".into(),
            kind: k.kind.clone(),
            label: k.label.clone(),
            url: fake_jev().await,
            // Not the catalog's name: on Windows the vault is the system's, shared by tests that
            // run side by side, and another test checks the end of TYPESAFE_API_KEY.
            secret: "JEV_READING_TEST_KEY".into(),
            model: k.model.clone(),
            test_path: k.test_path.clone(),
            docs: None,
            daily_tokens: 0,
            projects: Vec::new(),
            auth: Default::default(),
            client_id: None,
            credit: None,
        };
        hub.set_api_prefs(crate::apis::ApiPrefs { connections: vec![jev], helper: None, off: Vec::new(), ..Default::default() }).await?;

        // No key yet: nothing is asked.
        assert!(hub.api_decisions().await?.is_empty());
        hub.secrets.set("JEV_READING_TEST_KEY", "ts-test-key-1234567890").await?;

        // Turns: a sure "asks" marks the thread; an unsure answer leaves it as it was.
        hub.store.insert_thread(crate::model::Thread {
            id: "t1".into(), title: "t".into(), cwd: "/tmp".into(), engine: "claude".into(), status: ThreadStatus::Idle,
            engine_session_id: None, title_locked: false, archived: false, returned_at: None, seen_at: None,
            settled_at: Some(5), sent_at: Some(1), pinned_at: None, snoozed_until: None, auto_settle: true,
            parent: None, reading: None, owner: None, cloud: None, host: None, project: None, display: None, created_at: 1, updated_at: 1,
        }).await?;
        hub.store.append_event("t1", EventKind::UserMessage, json!({ "text": "fix the build" })).await?;
        hub.store.append_event("t1", EventKind::Update, chunk("Fixed it, tests pass. Should I push?")).await?;
        let end = hub.store.append_event("t1", EventKind::TurnEnd, json!({ "stop_reason": "end_turn" })).await?;
        hub.clone().read_turn(end).await;
        let thread = hub.store.thread("t1").await?.unwrap();
        assert_eq!(thread.reading.as_deref(), Some("asks"));
        assert_eq!(thread.settled_at, None, "a thread that asks you comes back from the shelf");
        let kept = hub.api_decisions().await?.remove(0);
        assert_eq!(kept.quote.as_deref(), Some("Fixed it, tests pass. Should I push?"), "the decision keeps what the agent said");
        assert_eq!(kept.tokens, Some(120));

        hub.store.update_thread("t1", |t| t.reading = None).await?;
        hub.store.append_event("t1", EventKind::UserMessage, json!({ "text": "push it" })).await?;
        hub.store.append_event("t1", EventKind::Update, chunk("Pushed, maybe.")).await?;
        let end = hub.store.append_event("t1", EventKind::TurnEnd, json!({ "stop_reason": "end_turn" })).await?;
        hub.clone().read_turn(end).await;
        assert_eq!(hub.store.thread("t1").await?.unwrap().reading, None);
        let events = hub.store.events("t1", 0, 100).await?;
        let readings: Vec<&Event> = events.iter().filter(|e| e.kind == EventKind::Reading).collect();
        assert_eq!(readings.len(), 2);
        assert_eq!(readings[1].data["acted"], false);

        // Focus: a sure goal is logged as a reading, a sure "none" is off the stage.
        let stage = Stage {
            title: "Stage 2".into(),
            done: vec![Check { text: "Chat queue".into(), done: false, line: 5 }],
            ..Default::default()
        };
        assert_eq!(hub.focus_reading("t1", &stage, "build the chat queue steer button").await, Some((true, String::new())));
        let last = hub.store.events("t1", 0, 100).await?.pop().unwrap();
        assert_eq!((last.data["feature"].as_str(), last.data["line"].as_u64()), (Some("focus"), Some(5)));
        let (fits, why) = hub.focus_reading("t1", &stage, "what is the weather in Lund").await.unwrap();
        assert!(!fits && why.contains("85%"), "{why}");

        // Test output: only a run whose output holds a failure word is asked about, and only a
        // sure "failed" counts.
        let before = hub.api_decisions().await?.len();
        assert!(!hub.hides_failure("t1", "cargo test | tail", "test result: ok. 3 passed; 0 failed").await);
        assert_eq!(hub.api_decisions().await?.len(), before, "a clean run costs no question");
        assert!(hub.hides_failure("t1", "cargo test | tail", "test b ... FAILED\ntest result: FAILED. 1 passed; 1 failed").await);
        assert!(!hub.hides_failure("t1", "npm test", "3 passing, Error handling suite").await, "the stand-in is not sure of this one");

        // A new task: asked only after a finished turn, and a sure "new" is logged under that message.
        hub.store.append_event("t1", EventKind::UserMessage, json!({ "text": "now something else" })).await?;
        let said = hub.store.append_event("t1", EventKind::UserMessage, json!({ "text": "a different job: write the release notes for the phone page please" })).await?;
        hub.clone().check_topic("t1".into(), said.seq, "a different job: write the release notes for the phone page please".into()).await;
        let last = hub.store.events("t1", 0, 100).await?.pop().unwrap();
        assert_eq!((last.data["feature"].as_str(), last.data["seq"].as_i64()), (Some("topic"), Some(said.seq)));
        let n = hub.store.events("t1", 0, 100).await?.len();
        hub.clone().check_topic("t1".into(), said.seq + 1, "the same job, please also fix the second button".into()).await;
        hub.clone().check_topic("t1".into(), said.seq + 1, "too short".into()).await;
        assert_eq!(hub.store.events("t1", 0, 100).await?.len(), n, "same task and short messages log nothing");

        // A threshold you set wins over the feature's own: at 95% the same sure "asks" is not gone by.
        hub.set_api_prefs(crate::apis::ApiPrefs { sure: [("turns".to_owned(), 0.95), ("nope".to_owned(), 0.7), ("judge".to_owned(), 0.1)].into(), ..hub.api_prefs().await? }).await?;
        let prefs = hub.api_prefs().await?;
        assert_eq!(prefs.sure.get("judge"), Some(&0.5), "a threshold is kept between 50% and 99%");
        assert!(!prefs.sure.contains_key("nope"));
        hub.store.update_thread("t1", |t| t.reading = None).await?;
        hub.store.append_event("t1", EventKind::UserMessage, json!({ "text": "and the docs" })).await?;
        hub.store.append_event("t1", EventKind::Update, chunk("Docs done. Should I push?")).await?;
        let end = hub.store.append_event("t1", EventKind::TurnEnd, json!({ "stop_reason": "end_turn" })).await?;
        hub.clone().read_turn(end).await;
        assert_eq!(hub.store.thread("t1").await?.unwrap().reading, None);
        assert!(!hub.api_decisions().await?[0].acted);

        // Turned off, a feature asks nothing.
        hub.set_api_prefs(crate::apis::ApiPrefs { off: vec!["focus".into()], ..hub.api_prefs().await? }).await?;
        assert_eq!(hub.focus_reading("t1", &stage, "what is the weather in Lund").await, None);
        let _ = hub.secrets.delete("JEV_READING_TEST_KEY").await;
        Ok(())
    }
}
