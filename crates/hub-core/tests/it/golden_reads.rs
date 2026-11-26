//! What the payload structs read from payloads that are not what today's code writes: a field
//! missing, `null` or of the wrong type, an older shape, another kind. Each case is what the code
//! read from the JSON before the kind had a struct (`e.data["x"].as_str()` and the like), so a
//! reader could move to the struct without deciding differently. The text the structs write is in
//! `golden_log.rs`.

use hub_core::model::*;
use serde_json::{Value, json};

use super::golden_log::event;

#[test]
fn a_payload_of_another_kind_or_with_a_new_field_still_reads() {
    // Kind mismatch is "not this one", not a panic.
    assert!(event(EventKind::Update, json!({ "message": "x" })).data_as::<ErrorData>().is_none());
    // A field a newer Hub added is ignored.
    let newer = event(EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "added_later": [1, 2] }));
    assert_eq!(newer.data_as::<TurnEndData>().unwrap().stop_reason, "end_turn");
    // A payload that is not an object has nothing to read.
    assert!(event(EventKind::TurnEnd, Value::Null).data_as::<TurnEndData>().is_none());
}

#[test]
fn the_session_payloads_read_what_the_code_read_from_the_json() {
    // The model in use: the option in category "model", or a setting that chose one.
    let state = event(EventKind::SessionState, json!({ "config_options": [{ "id": "m", "category": "model", "currentValue": "opus" }], "modes": null }));
    assert_eq!(state.data_as::<SessionStateData>().unwrap().model(), Some("opus"));
    assert_eq!(event(EventKind::SessionState, json!({ "config_options": null })).data_as::<SessionStateData>().unwrap().model(), None);
    let chose = |d: Value| event(EventKind::Setting, d).data_as::<SettingData>().unwrap();
    assert_eq!(chose(json!({ "config_id": "model", "value": "opus" })).model(), Some("opus"));
    assert_eq!(chose(json!({ "config_id": "effort", "value": "high" })).model(), None);
    assert_eq!(chose(json!({ "config_id": "model", "value": true, "by": 7 })).by, None);
    // Missing `resumed` is not a fresh session (`data["resumed"] == false` was false for it).
    let started = |d: Value| event(EventKind::SessionStarted, d).data_as::<SessionStartedData>().unwrap();
    assert_eq!(started(json!({})).resumed, None);
    assert_eq!(started(json!({ "resumed": false })).resumed, Some(false));
    assert!(started(json!({ "start_ms": { "initialize": "soon" } })).start_ms.is_none());
    // A written `null` stays a written `null`.
    assert_eq!(started(json!({ "agent": null })).agent, Some(Value::Null));
    assert_eq!(started(json!({})).agent, None);
    let wait = event(EventKind::QuotaWait, json!({ "state": "waiting", "seq": null, "at": "later" })).data_as::<QuotaWaitData>().unwrap();
    assert_eq!((wait.state.as_str(), wait.seq, wait.at), ("waiting", Some(None), None));
}

#[test]
fn the_code_history_payloads_read_what_the_code_read_from_the_json() {
    // A checkpoint without a `seq` is not the checkpoint of message 0.
    let checkpoint = |d: Value| event(EventKind::Checkpoint, d).data_as::<CheckpointData>().unwrap();
    assert_eq!(checkpoint(json!({ "commit": "c" })).seq, None);
    assert_eq!(checkpoint(json!({ "seq": 3, "head": null })).head, Some(None));
    // An attempt is the start when it has `n`, and decided when it has a `state`.
    let attempt = |d: Value| event(EventKind::Attempt, d).data_as::<AttemptData>().unwrap();
    let start = attempt(json!({ "group": "g", "n": 1, "threads": ["a", "b"] }));
    assert_eq!((start.n, start.state, start.threads), (Some(1), None, Some(vec!["a".to_owned(), "b".to_owned()])));
    assert_eq!(attempt(json!({ "group": "g", "state": "dropped", "by": "a" })).n, None);
    assert_eq!(attempt(json!({ "state": 7 })).state, None);
    // The thread ids that read, as `filter_map(|t| t.as_str())` gave.
    assert_eq!(attempt(json!({ "threads": ["a", 1, "b"] })).threads, Some(vec!["a".to_owned(), "b".to_owned()]));
    let terminal = |d: Value| event(EventKind::Terminal, d).data_as::<TerminalData>().unwrap();
    assert_eq!(terminal(json!({ "state": "taken", "pid": 7 })).pid, Some(Some(7)));
    assert_eq!(terminal(json!({ "state": "taken", "pid": null })).pid, Some(None));
    assert_eq!(terminal(json!({ "state": "imported", "replay_turns": -1 })).replay_turns, None);
    let rewind = event(EventKind::Rewind, json!({ "files": "yes" })).data_as::<RewindData>().unwrap();
    assert_eq!((rewind.from, rewind.files), (None, false));
}

#[test]
fn the_check_payloads_read_what_the_code_read_from_the_json() {
    // A section that was asked about is written back as it was read.
    let asked = json!({ "reply": 4, "state": "asked", "sections": [{ "doc": "d.md", "heading": null, "first": 1, "last": 2, "files": ["a.rs", 7] }] });
    let docs = event(EventKind::Docs, asked).data_as::<DocsData>().unwrap();
    assert_eq!((docs.reply, docs.sections[0].files.as_slice()), (Some(4), ["a.rs".to_owned()].as_slice()));
    assert_eq!(event(EventKind::Docs, json!({ "state": "asked" })).data_as::<DocsData>().unwrap().reply, None);
    // Only a customs event that says `sent: true` counts as told.
    let customs = |d: Value| event(EventKind::Customs, d).data_as::<CustomsData>().unwrap();
    assert_eq!(customs(json!({ "declared": true, "found": [] })).sent, None);
    let odd = customs(json!({ "sent": "yes", "found": [{ "kind": "npm", "name": "x" }, 3] }));
    assert_eq!((odd.sent, odd.found.len()), (None, 1));
    // A clash without a title is not a clash with an empty one.
    let clash = |d: Value| event(EventKind::Clash, d).data_as::<ClashData>().unwrap().other_title;
    assert_eq!((clash(json!({ "path": "p" })), clash(json!({ "other_title": "" }))), (None, Some(String::new())));
    let proof = |d: Value| event(EventKind::Proof, d).data_as::<ProofData>().unwrap();
    assert_eq!(proof(json!({ "state": "shown" })).files, None);
    assert_eq!(proof(json!({ "state": "shown", "files": null, "kind": null })).files, Some(None));
    assert_eq!(proof(json!({ "kind": "tests", "state": 7 })).state, "");
    let leak = event(EventKind::Leak, json!({ "findings": [{ "path": "a", "hint": "h", "line": "x" }] })).data_as::<LeakData>().unwrap();
    assert_eq!((leak.findings[0].path.as_str(), leak.findings[0].line), ("a", 0));
}

#[test]
fn the_judged_payloads_read_what_the_code_read_from_the_json() {
    // A review to start the next one from is one that said where it ended.
    let review = |d: Value| event(EventKind::Review, d).data_as::<ReviewData>().unwrap();
    assert_eq!(review(json!({ "state": "done" })).head, None);
    assert_eq!(review(json!({ "state": "done", "head": 7 })).head, None);
    let done = review(json!({ "id": "r", "state": "done", "head": "h", "seq": "x", "chunks": [{ "title": "A", "files": ["a", 1] }, 2] }));
    assert_eq!((done.head.as_deref(), done.seq, done.reason.clone(), done.chunks.len()), (Some("h"), 0, None, 1));
    assert_eq!(done.chunks[0].files, ["a"]);
    // A review from before reviews said where they ended: it reads, with nothing to start from.
    let early = review(json!({ "id": "r", "model": "opus", "files": ["a.rs"], "base": "b", "state": "done", "sent": true, "text": "Needs changes" }));
    assert_eq!((early.state.as_str(), early.head, early.sent, early.chunks.len()), ("done", None, Some(true), 0));
    // Design mode is what the last event with an `on` says; a review or stray files say nothing of it.
    let design = |d: Value| event(EventKind::Design, d).data_as::<DesignData>().unwrap();
    assert_eq!(design(json!({ "on": true })), DesignData { on: Some(true), ..Default::default() });
    assert_eq!(design(json!({ "review": "r", "state": "done" })).on, None);
    assert_eq!(design(json!({ "on": "yes", "review": 3, "strayed": ["a", 1] })), DesignData { strayed: Some(vec!["a".into()]), ..Default::default() });
    let task = event(EventKind::Task, json!({ "id": "v", "model": null, "ms": "x" })).data_as::<TaskData>().unwrap();
    assert_eq!((task.id.as_str(), task.model, task.ms), ("v", Some(None), None));
    let reading = event(EventKind::Reading, json!({ "feature": "turn", "acted": false, "confidence": "high" })).data_as::<ReadingData>().unwrap();
    assert_eq!((reading.acted, reading.confidence, reading.line), (false, 0.0, None));
    assert!(event(EventKind::Focus, json!({ "fits": false, "line": 5 })).data_as::<FocusData>().is_some_and(|f| !f.fits && f.line == 5));
}

#[test]
fn the_shown_payloads_read_what_the_code_read_from_the_json() {
    // A reaction that does not say is on; one that names no message, emoji or author matches nothing.
    let reaction = |d: Value| event(EventKind::Reaction, d).data_as::<ReactionData>().unwrap();
    assert_eq!(reaction(json!({ "seq": 1, "emoji": "👍", "by": "you" })).on, None);
    assert_eq!(reaction(json!({ "seq": "1", "on": false })), ReactionData { on: Some(false), ..Default::default() });
    // A card is found by the failure it is about, and names its problem or nothing.
    let card = |d: Value| event(EventKind::Reported, d).data_as::<ReportedData>().unwrap();
    assert_eq!((card(json!({ "state": "sent" })).about, card(json!({ "about": 4, "problem": "7" })).problem), (None, None));
    assert_eq!(card(json!({ "about": 4, "pr": null })).pr, Some(None));
    assert_eq!(card(json!({ "about": 4, "pr": { "number": 13 } })).pr.flatten().map(|p| p.number), Some(13));
    // A gist by the small model has no `by`; its points read one by one.
    let gist = event(EventKind::Gist, json!({ "upto": 2, "points": ["a", 1], "ask": null })).data_as::<GistData>().unwrap();
    assert_eq!((gist.upto, gist.points.len(), gist.ask, gist.by), (2, 1, Some(None), None));
    // Offered replies: a bare string was never an option in the log, and a label alone reads with no message.
    let replies = event(EventKind::Replies, json!({ "question": "Q?", "options": [{ "label": "Yes" }, "No", { "message": "m" }] })).data_as::<RepliesData>().unwrap();
    assert_eq!(replies.options, [ReplyOption { label: "Yes".into(), message: String::new() }, ReplyOption { label: String::new(), message: "m".into() }]);
    assert!(event(EventKind::Replies, json!({ "question": "Q?" })).data_as::<RepliesData>().unwrap().options.is_empty());
    // A page with no id is no page to list; an older plugin card keeps the fields it has.
    let page = event(EventKind::Page, json!({ "title": "T", "height": null })).data_as::<PageData>().unwrap();
    assert_eq!((page.id.as_str(), page.height, page.replaces), ("", Some(None), None));
    let old_card = json!({ "repo": { "full_name": "a/b", "stars": 3 }, "why": "w" });
    assert_eq!(event(EventKind::PluginSuggested, old_card.clone()).data_as::<PluginSuggestedData>().unwrap().to_json().to_string(), old_card.to_string());
}

#[test]
fn the_delegated_payloads_read_what_the_code_read_from_the_json() {
    // The oldest side jobs said no model; that is not the same as saying `null`.
    let child = |d: Value| event(EventKind::Child, d).data_as::<ChildData>().unwrap();
    assert_eq!((child(json!({ "id": "c", "state": "working" })).model, child(json!({ "id": "c", "model": null })).model), (None, Some(None)));
    let done = child(json!({ "id": "c", "state": "done", "images": ["a.png", 3], "reply": 7 }));
    assert_eq!((done.images, done.reply, done.title.as_str()), (Some(vec!["a.png".to_owned()]), None, ""));
    assert_eq!(child(json!({ "id": "c", "state": "failed" })).images, None);
    // Only a handoff that names its folder and its work is followed.
    let handed = |d: Value| event(EventKind::HandedOff, d).data_as::<HandedOffData>().unwrap();
    assert_eq!(handed(json!({ "tool": "K", "ok": true, "folder": "/k" })).work, None);
    let odd = handed(json!({ "ok": "yes", "folder": 3, "work": "w" }));
    assert_eq!((odd.ok, odd.folder, odd.work.as_deref()), (false, None, Some("w")));
    // A worker run reads only when the whole run is there, with or without lines.
    let run = json!({ "tool": "K", "folder": "/k", "run": "r", "title": "T", "state": "done", "paused": false, "said": null, "steps": [], "round": null, "agents": [], "link": null, "cost": null, "controls": [], "cursor": 3 });
    let read = event(EventKind::WorkerRun, run.clone()).data_as::<WorkerRunData>().expect("a run without lines");
    assert_eq!((read.run.run.as_str(), read.run.cursor, read.lines.len()), ("r", 3, 0));
    let mut short = run.clone();
    short.as_object_mut().unwrap().remove("cursor");
    assert!(event(EventKind::WorkerRun, short).data_as::<WorkerRunData>().is_none());
    let mut lined = run;
    lined["lines"] = json!([{ "seq": 3, "kind": "k", "step": null, "round": null, "agent": null, "text": "x" }, "odd"]);
    assert_eq!(event(EventKind::WorkerRun, lined).data_as::<WorkerRunData>().unwrap().lines.len(), 1);
    // An old Auto line: on or off, or a pick.
    let auto = |d: Value| event(EventKind::Auto, d).data_as::<AutoData>().unwrap();
    assert_eq!((auto(json!({ "on": false, "by": "pick" })).on, auto(json!({ "ask": "image", "engine": "codex", "why": "w" })).on), (Some(false), None));
}

#[test]
fn null_missing_and_a_value_are_three_things_and_a_list_reads_item_by_item() {
    let read = |d: Value| event(EventKind::PermissionResolved, d).data_as::<PermissionResolvedData>().unwrap().option_id;
    assert_eq!(read(json!({ "id": "a" })), None);
    assert_eq!(read(json!({ "id": "a", "option_id": null })), Some(None));
    assert_eq!(read(json!({ "id": "a", "option_id": "o" })), Some(Some("o".into())));
    assert_eq!(read(json!({ "id": "a", "option_id": 7 })), Some(None), "a value of the wrong type reads as null");
    // What `filter_map(|i| i.as_str())` gave: the items that read.
    let slept = event(EventKind::Slept, json!({ "ids": ["a", 1, "b"] })).data_as::<SleptData>().unwrap();
    assert_eq!(slept.ids, ["a", "b"]);
    assert!(event(EventKind::Slept, json!({ "ids": "a" })).data_as::<SleptData>().unwrap().ids.is_empty());
}

#[test]
fn a_mistyped_field_is_its_default_and_the_rest_still_reads() {
    // What `data["handed_over"] == true` beside `data["stop_reason"].as_str()` gave: each field on its own.
    let odd = event(EventKind::TurnEnd, json!({ "handed_over": true, "stop_reason": null, "prompt": "three" }));
    let read = odd.data_as::<TurnEndData>().unwrap();
    assert_eq!((read.handed_over, read.stop_reason.as_str(), read.prompt), (Some(true), "", None));
    let odd = event(EventKind::Error, json!({ "message": 7, "prompt": 5, "leaves": ["idle"] }));
    let read = odd.data_as::<ErrorData>().unwrap();
    assert_eq!((read.message.as_str(), read.prompt, read.leaves), ("", Some(5), None));
    // A message keeps its marks whatever its text is.
    let odd = event(EventKind::UserMessage, json!({ "text": null, "review": "r1" }));
    let read = odd.data_as::<UserMessageData>().unwrap();
    assert_eq!((read.text.as_str(), read.marks.get("review")), ("", Some(&json!("r1"))));
    // An array is not a payload, whatever is in it.
    assert!(event(EventKind::TurnEnd, json!(["end_turn"])).data_as::<TurnEndData>().is_none());
    // A turn end from before the tokens were kept has none; mistyped tokens are none, or the count alone is.
    let old = event(EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 3 })).data_as::<TurnEndData>().unwrap();
    assert_eq!((old.stop_reason.as_str(), old.prompt, old.usage), ("end_turn", Some(3), None));
    let odd = event(EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "usage": 12 })).data_as::<TurnEndData>().unwrap();
    assert_eq!((odd.stop_reason.as_str(), odd.usage), ("end_turn", None));
    let odd = event(EventKind::TurnEnd, json!({ "usage": { "input": "4", "output": 5 } })).data_as::<TurnEndData>().unwrap();
    assert_eq!(odd.usage, Some(TurnUsage { output: Some(5), ..Default::default() }));
    let said = event(EventKind::UserMessage, json!({ "text": "a", "attachments": [], "original": "b" }));
    assert_eq!(said.data_as::<UserMessageData>().unwrap().original(), Some("b"));
    let odd = event(EventKind::PermissionRequest, json!({ "id": 7, "tool_call": "x" }));
    assert_eq!(odd.data_as::<PermissionRequestData>().unwrap().tool_call, json!("x"));
}

#[test]
fn an_update_is_kept_as_it_came_and_says_which_it_is() {
    let chunk = json!({ "content": { "type": "text", "text": "Hi" }, "sessionUpdate": "agent_message_chunk", "_meta": { "x": 1 } });
    let update = event(EventKind::Update, chunk.clone());
    assert_eq!(update_kind(&update.data), Some("agent_message_chunk"));
    // Whatever order the engine wrote its keys in is the order they are written back in.
    assert_eq!(update.data_as::<UpdateData>().unwrap().to_json().to_string(), chunk.to_string());
    assert_eq!((update_kind(&json!({ "sessionUpdate": 3 })), update_kind(&json!("plan")), update_kind(&json!({}))), (None, None, None));
}

/// The borrowing reads say what the struct says, and nothing on an event without the key.
#[test]
fn the_borrowing_reads_agree_with_the_structs() {
    let said = event(EventKind::UserMessage, json!({ "text": "hi", "restart": 4, "quota": null }));
    assert_eq!(said.said(), Some(said.data_as::<UserMessageData>().unwrap().text.as_str()));
    assert_eq!((said.mark("restart").as_i64(), said.mark("review").is_null(), said.mark("quota").is_null()), (Some(4), true, true));
    assert!(said.has("quota") && !said.has("review"));

    let end = event(EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 7 }));
    let typed = end.data_as::<TurnEndData>().unwrap();
    assert_eq!((end.ended_well(), end.cancelled(), end.prompt(), end.handed_over()), (typed.ended_well(), typed.cancelled(), typed.prompt, false));
    assert_eq!(end.stop_reason(), Some("end_turn"));
    let stopped = event(EventKind::TurnEnd, json!({ "stop_reason": "cancelled", "handed_over": true }));
    assert!(stopped.cancelled() && !stopped.ended_well() && stopped.handed_over());

    let error = event(EventKind::Error, json!({ "message": "no", "detail": "why", "prompt": 3 }));
    assert_eq!((error.failure(), error.detail().as_str(), error.prompt()), (Some("no"), Some("why"), Some(3)));

    let asked = event(EventKind::PermissionRequest, json!({ "tool_call": { "title": "Run" }, "options": [{ "optionId": "a" }], "id": "p1" }));
    assert_eq!(*asked.request_id(), "p1");
    assert_eq!((asked.tool_call()["title"].as_str(), asked.options()[0]["optionId"].as_str()), (Some("Run"), Some("a")));

    assert!(error.has("prompt") && error.leaves().is_none());
    assert_eq!(event(EventKind::Error, json!({ "message": "no", "leaves": "idle" })).leaves(), Some("idle"));

    // A payload that is not an object has no keys.
    let bare = event(EventKind::UserMessage, Value::Null);
    assert!(bare.mark("restart").is_null() && !bare.has("restart") && bare.said().is_none() && bare.request_id().is_null());

    // An event without the key, or with it mistyped, says nothing.
    let empty = event(EventKind::TurnEnd, json!({ "stop_reason": 5 }));
    assert_eq!((empty.said(), empty.prompt(), empty.stop_reason(), empty.failure()), (None, None, None, None));
    assert!(!empty.ended_well() && !empty.cancelled() && !empty.handed_over() && empty.request_id().is_null());
}
