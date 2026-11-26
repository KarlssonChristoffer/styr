//! The event log is append-only, so every shape an old Hub wrote must still read. `frozen/golden_events.jsonl`
//! holds payloads as older versions wrote them (one `{ kind, data }` per line); each must read into the
//! payload struct of its kind and write back the same text, key order included, and the structs must write
//! the shapes the code wrote before they existed. Add lines when a new shape appears; never edit or remove a
//! line a released Hub wrote. (Builds between #973 and this file's text comparison wrote `turn_end` with
//! `prompt` before `forced` and `permission_request` with `id` first; those read, but are not lines here. Nor
//! are the `review` events from before 2026-10-30, which had no `head`, `seq`, `by` or `chunks` and said
//! `state` after `base` when they ended; they read too, and are never a place to start the next review from.)
//! Every kind has a struct and at least one line; what the structs read from odd payloads is in `golden_reads.rs`.


use hub_core::model::*;
use serde_json::{Value, json};

fn file(path: &str) -> String {
    let path = hub_core::source_tree::hub_core().join(path);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn kind(name: &str) -> EventKind {
    serde_json::from_value(json!(name)).unwrap_or(EventKind::Unknown)
}

fn golden() -> Vec<(EventKind, Value)> {
    file("tests/it/frozen/golden_events.jsonl")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let line: Value = serde_json::from_str(l).unwrap_or_else(|e| panic!("{l}: {e}"));
            (kind(line["kind"].as_str().unwrap_or_default()), line["data"].clone())
        })
        .collect()
}

pub(super) fn event(kind: EventKind, data: Value) -> Event {
    Event { thread_id: "t".into(), seq: 1, ts: 1, kind, data }
}

#[test]
fn every_old_payload_reads_and_writes_back_the_same_text() {
    // Without `preserve_order` both sides would sort their keys and the comparison would see nothing.
    assert_eq!(json!({ "b": 1, "a": 2 }).to_string(), r#"{"b":1,"a":2}"#, "serde_json must keep key order (Cargo.toml)");
    let lines = golden();
    assert!(lines.len() >= 172, "the golden log is shorter than it was");
    for (kind, data) in lines {
        let back = rewritten(kind, &data)
            .unwrap_or_else(|| panic!("no struct for {kind:?}: add it to the registry in model/event_data.rs"))
            .unwrap_or_else(|| panic!("{kind:?} {data} no longer reads"));
        // Text, not value: comparing values would not see keys change places.
        assert_eq!(back, data.to_string(), "{kind:?} changed when read and written back");
    }
}

#[test]
fn every_kind_has_a_struct_and_a_golden_line() {
    // The kinds as the app knows them: the generated union, which follows the enum. It is the
    // committed file, written by the lib's tests (`cargo test -p hub-core`): after adding a kind,
    // run those first, as the rule for `model.rs` says anyway.
    let union = file("../../app/src/lib/bindings/EventKind.ts");
    let names: Vec<&str> = union.split_once('=').map(|(_, kinds)| kinds).unwrap_or_default().split('"').skip(1).step_by(2).collect();
    assert!(names.len() >= 55, "could not read the kinds from the binding");
    let in_golden: Vec<EventKind> = golden().into_iter().map(|(k, _)| k).collect();
    for name in names.iter().copied().filter(|n| *n != "unknown") {
        assert!(rewritten(kind(name), &json!({})).is_some(), "{name} has no payload struct: write one (model/event_data/) and add it to the registry");
        assert!(in_golden.contains(&kind(name)), "{name} has a struct but no line in frozen/golden_events.jsonl");
    }
}

#[test]
fn the_structs_write_the_text_the_code_wrote_before_them() {
    let text = |d: Value| d.to_string();
    assert_eq!(TurnStartData { prompt: Some(3), ..Default::default() }.to_json().to_string(), text(json!({ "prompt": 3 })));
    assert_eq!(TurnStartData { unprompted: Some(true), ..Default::default() }.to_json().to_string(), text(json!({ "unprompted": true })));
    assert_eq!(
        TurnEndData { stop_reason: "cancelled".into(), forced: Some(true), prompt: Some(3), ..Default::default() }.to_json().to_string(),
        text(json!({ "stop_reason": "cancelled", "forced": true, "prompt": 3 }))
    );
    assert_eq!(
        TurnEndData { stop_reason: "end_turn".into(), unprompted: Some(true), handed_over: Some(true), ..Default::default() }.to_json().to_string(),
        text(json!({ "stop_reason": "end_turn", "unprompted": true, "handed_over": true }))
    );
    // The tokens come last, and only the counts the engine gave.
    let usage = TurnUsage { input: Some(4), output: Some(105), ..Default::default() };
    assert_eq!(
        TurnEndData { stop_reason: "end_turn".into(), prompt: Some(3), usage: Some(usage), ..Default::default() }.to_json().to_string(),
        text(json!({ "stop_reason": "end_turn", "prompt": 3, "usage": { "input": 4, "output": 105 } }))
    );
    assert_eq!(ErrorData::new("boom").to_json().to_string(), text(json!({ "message": "boom" })));
    assert_eq!(
        ErrorData { message: "m".into(), prompt: Some(5), detail: Some("d".into()), ..Default::default() }.to_json().to_string(),
        text(json!({ "message": "m", "prompt": 5, "detail": "d" }))
    );
    // The engine's request first, then the id Hub gave it: `data["id"] = id` came last.
    assert_eq!(
        PermissionRequestData { id: "7".into(), tool_call: json!({}), options: json!([]) }.to_json().to_string(),
        text(json!({ "tool_call": {}, "options": [], "id": "7" }))
    );
}

#[test]
fn the_structs_about_questions_and_status_write_the_text_the_code_wrote_before_them() {
    let wrote = |d: &dyn Fn() -> Value, old: Value| assert_eq!(d().to_string(), old.to_string());
    // An answer always says `option_id`, `null` when there was none; a flag comes after it.
    let answer = PermissionResolvedData::answer;
    wrote(&|| answer("a", Some("allow".into())).to_json(), json!({ "id": "a", "option_id": "allow" }));
    wrote(&|| answer("a", None).to_json(), json!({ "id": "a", "option_id": null }));
    wrote(&|| PermissionResolvedData { stale: Some(true), ..answer("a", None) }.to_json(), json!({ "id": "a", "option_id": null, "stale": true }));
    wrote(&|| PermissionResolvedData { woke: Some(true), ..answer("a", Some("o".into())) }.to_json(), json!({ "id": "a", "option_id": "o", "woke": true }));
    wrote(&|| PermissionResolvedData { restart: Some(true), ..answer("a", None) }.to_json(), json!({ "id": "a", "option_id": null, "restart": true }));
    wrote(&|| PermissionResolvedData { moved: Some(true), ..answer("a", None) }.to_json(), json!({ "id": "a", "option_id": null, "moved": true }));
    wrote(
        &|| PermissionAutoData { tool_call: json!({ "title": "Delete" }), option_id: Some(None), allowed: false, why: "locked".into() }.to_json(),
        json!({ "tool_call": { "title": "Delete" }, "option_id": null, "allowed": false, "why": "locked" }),
    );
    wrote(&|| SleptData { ids: vec!["a".into()] }.to_json(), json!({ "ids": ["a"] }));
    use ThreadStatus::*;
    for status in [Idle, Running, WaitingOnYou, Error, InTerminal] {
        wrote(&|| StatusData::new(status, "why").to_json(), json!({ "status": status, "why": "why" }));
        assert_eq!(StatusData::new(status, "why").to(), Some(status));
    }
    wrote(&|| QueueData::new(7, "steered").to_json(), json!({ "seq": 7, "action": "steered" }));
    wrote(&|| QueueData { text: Some("two".into()), ..QueueData::new(7, "edited") }.to_json(), json!({ "seq": 7, "action": "edited", "text": "two" }));
    wrote(&|| PausedData { by: "pc".into() }.to_json(), json!({ "by": "pc" }));
    let restart = |why: Option<&str>| RestartData { by: "crash".into(), prompt: 5, queued: vec![7], state: "stopped".into(), why: why.map(str::to_owned) };
    wrote(&|| restart(None).to_json(), json!({ "by": "crash", "prompt": 5, "queued": [7], "state": "stopped" }));
    wrote(&|| restart(Some("loop")).to_json(), json!({ "by": "crash", "prompt": 5, "queued": [7], "state": "stopped", "why": "loop" }));
}

#[test]
fn the_structs_about_the_session_write_the_text_the_code_wrote_before_them() {
    let wrote = |d: Value, old: Value| assert_eq!(d.to_string(), old.to_string());
    // The engine's own JSON is written as it came, `null` too; `start_ms` came last.
    let started = |agent: Value, start_ms| SessionStartedData {
        driver: "acp".into(),
        command: "agent".into(),
        protocol_version: Some(json!(1)),
        agent: Some(agent),
        session_id: "s".into(),
        resumed: Some(false),
        start_ms,
    };
    let base = json!({ "driver": "acp", "command": "agent", "protocol_version": 1, "agent": null, "session_id": "s", "resumed": false });
    wrote(started(Value::Null, None).to_json(), base.clone());
    let mut timed = base;
    timed["start_ms"] = json!({ "initialize": 1, "session": 2, "total": 3 });
    wrote(started(Value::Null, Some(StartTiming { initialize: 1, session: 2, total: 3 })).to_json(), timed);
    // After a setting changed there is no `prompt_capabilities`; an engine without options says `null`.
    let state = |caps: Option<Value>| SessionStateData { config_options: Some(Value::Null), modes: Some(json!({ "currentModeId": "plan" })), prompt_capabilities: caps };
    wrote(state(None).to_json(), json!({ "config_options": null, "modes": { "currentModeId": "plan" } }));
    wrote(
        state(Some(json!({ "image": true }))).to_json(),
        json!({ "config_options": null, "modes": { "currentModeId": "plan" }, "prompt_capabilities": { "image": true } }),
    );
    wrote(SettingData::new("model", "opus").to_json(), json!({ "config_id": "model", "value": "opus" }));
    wrote(SettingData::new("fast", true).to_json(), json!({ "config_id": "fast", "value": true }));
    wrote(SettingData::new("mode", "plan").by("default").to_json(), json!({ "config_id": "mode", "value": "plan", "by": "default" }));
    let switch = |from: Value| QuotaSwitchData { limit: "5-hour".into(), percent: json!(91.5_f32), from: Some(from), to: "sonnet".into() };
    wrote(switch(json!("opus")).to_json(), json!({ "limit": "5-hour", "percent": 91.5_f32, "from": "opus", "to": "sonnet" }));
    wrote(switch(Value::Null).to_json(), json!({ "limit": "5-hour", "percent": 91.5_f32, "from": null, "to": "sonnet" }));
    // A wait always says `seq`, `limit` and `resets`, `null` when it has none; an offer comes last.
    let waiting = |offer: Option<&str>| QuotaWaitData {
        state: "waiting".into(),
        seq: Some(None),
        at: Some(9),
        known: Some(false),
        limit: Some(None),
        resets: Some(Some("3pm".into())),
        message: Some("m".into()),
        tries: Some(0),
        offer: offer.map(str::to_owned),
        ..Default::default()
    };
    let old = json!({ "state": "waiting", "seq": null, "at": 9, "known": false, "limit": null, "resets": "3pm", "message": "m", "tries": 0 });
    wrote(waiting(None).to_json(), old.clone());
    let mut offered = old;
    offered["offer"] = json!("codex");
    wrote(waiting(Some("codex")).to_json(), offered);
    wrote(QuotaWaitData::after("resumed", 4, "hub").to_json(), json!({ "state": "resumed", "wait": 4, "by": "hub" }));
    wrote(QuotaWaitData::after("cancelled", 4, "message").to_json(), json!({ "state": "cancelled", "wait": 4, "by": "message" }));
    wrote(
        QuotaWaitData { to: Some("b".into()), from: Some("a".into()), ..QuotaWaitData::after("switched", 4, "you") }.to_json(),
        json!({ "state": "switched", "wait": 4, "to": "b", "from": "a", "by": "you" }),
    );
    wrote(
        QuotaWaitData { state: "gave_up".into(), seq: Some(Some(4)), message: Some("m".into()), tries: Some(3), ..Default::default() }.to_json(),
        json!({ "state": "gave_up", "seq": 4, "message": "m", "tries": 3 }),
    );
    wrote(EngineSwitchData { from: "a".into(), to: "b".into() }.to_json(), json!({ "from": "a", "to": "b" }));
    let lost = SignInLostData { engine: "e".into(), label: "E".into(), machine: "pc".into(), message: "m".into(), kept: false, can_keep: Some(true), keychain: Some(false) };
    wrote(lost.to_json(), json!({ "engine": "e", "label": "E", "machine": "pc", "message": "m", "kept": false, "can_keep": true, "keychain": false }));
    let alone = Unattended { by: "Vakten".into(), prefer: [("model".to_owned(), vec!["sonnet".to_owned()])].into() };
    wrote(alone.to_json(), json!({ "by": "Vakten", "prefer": { "model": ["sonnet"] } }));
}

#[test]
fn the_structs_about_code_history_write_the_text_the_code_wrote_before_them() {
    let wrote = |d: Value, old: Value| assert_eq!(d.to_string(), old.to_string());
    let none: Option<String> = None;
    // Where the repository stood is always said, `null` when it has no head or branch.
    let checkpoint = CheckpointData { seq: Some(3), commit: "c".into(), root: "/r".into(), head: Some(None), branch: Some(Some("main".into())) };
    wrote(checkpoint.to_json(), json!({ "seq": 3, "commit": "c", "root": "/r", "head": none, "branch": "main" }));
    let restored = |seq: Option<i64>, kept: Option<&str>| RestoredData {
        to: "c".into(),
        seq: Some(seq),
        root: "/r".into(),
        files: 2,
        commits: 1,
        kept_commits: Some(kept.map(str::to_owned)),
        safety: "s".into(),
        safety_head: Some(Some("h".into())),
        safety_branch: Some(None),
    };
    let old = |seq: Option<i64>, kept: Option<&str>| {
        json!({ "to": "c", "seq": seq, "root": "/r", "files": 2_usize, "commits": 1_usize, "kept_commits": kept, "safety": "s", "safety_head": "h", "safety_branch": none })
    };
    wrote(restored(Some(3), None).to_json(), old(Some(3), None));
    wrote(restored(None, Some("pushed")).to_json(), old(None, Some("pushed")));
    wrote(RewindData { from: Some(3), files: true, by: "you".into() }.to_json(), json!({ "from": 3, "files": true, "by": "you" }));
    wrote(RuleSavedData { text: "t".into(), path: "/p".into(), seq: 3 }.to_json(), json!({ "text": "t", "path": "/p", "seq": 3 }));
    // The start says `root` then `cwd`; a picked attempt said `cwd` then `root`.
    let start = AttemptData {
        group: "g".into(),
        n: Some(1),
        of: Some(2),
        threads: Some(vec!["a".into(), "b".into()]),
        base: Some("c".into()),
        root: Some("/r".into()),
        cwd: Some("/r/app".into()),
        worktree: Some("/w".into()),
        branch: Some("b".into()),
        ..Default::default()
    };
    wrote(
        start.to_json(),
        json!({ "group": "g", "n": 1_usize, "of": 2_usize, "threads": ["a", "b"], "base": "c", "root": "/r", "cwd": "/r/app", "worktree": "/w", "branch": "b" }),
    );
    let picked = AttemptData {
        group: "g".into(),
        state: Some("picked".into()),
        files: Some(4),
        cwd: Some("/r/app".into()),
        root: Some("/r".into()),
        safety: Some("s".into()),
        safety_head: Some(None),
        safety_branch: Some(Some("main".into())),
        ..Default::default()
    };
    wrote(
        picked.to_json(),
        json!({ "group": "g", "state": "picked", "files": 4_usize, "cwd": "/r/app", "root": "/r", "safety": "s", "safety_head": none, "safety_branch": "main" }),
    );
    let dropped = AttemptData { group: "g".into(), state: Some("dropped".into()), by: Some("a".into()), ..Default::default() };
    wrote(dropped.to_json(), json!({ "group": "g", "state": "dropped", "by": "a" }));
    let pid: Option<u32> = None;
    let taken = |cut| TerminalData { pid: Some(pid.map(u64::from)), command: Some("cli".into()), cut, ..TerminalData::state("taken") };
    wrote(taken(None).to_json(), json!({ "state": "taken", "pid": pid, "command": "cli" }));
    wrote(taken(Some(41)).to_json(), json!({ "state": "taken", "pid": pid, "command": "cli", "cut": 41 }));
    wrote(TerminalData { pid: Some(Some(7)), ..TerminalData::state("taken") }.to_json(), json!({ "state": "taken", "pid": Some(7_u32) }));
    wrote(TerminalData::state("back").to_json(), json!({ "state": "back" }));
    let imported = |carry_on| TerminalData { turns: Some(2), replay_turns: Some(9), carry_on, ..TerminalData::state("imported") };
    wrote(imported(None).to_json(), json!({ "state": "imported", "turns": 2_usize, "replay_turns": 9_usize }));
    wrote(imported(Some(true)).to_json(), json!({ "state": "imported", "turns": 2_usize, "replay_turns": 9_usize, "carry_on": true }));
    let moved = MovedData { from: "pc".into(), folder: "/r".into(), via: "files".into(), files: vec![".env".into()], dirty: 2, stopped: true };
    wrote(moved.to_json(), json!({ "from": "pc", "folder": "/r", "via": "files", "files": [".env"], "dirty": 2_u32, "stopped": true }));
    wrote(MovedAwayData { to: "mac".into(), folder: "/r".into() }.to_json(), json!({ "to": "mac", "folder": "/r" }));
}

#[test]
fn the_structs_about_checks_after_a_turn_write_the_text_the_code_wrote_before_them() {
    let wrote = |d: Value, old: Value| assert_eq!(d.to_string(), old.to_string());
    let none: Option<String> = None;
    wrote(
        ClashData { path: "/p".into(), other: "t".into(), other_title: Some("T".into()) }.to_json(),
        json!({ "path": "/p", "other": "t", "other_title": "T" }),
    );
    let finding = LeakFinding { path: "a.rs".into(), line: 3, kind: "api key".into(), hint: "sk-…".into() };
    wrote(
        LeakData { seq: 4, findings: vec![finding], sent: true }.to_json(),
        json!({ "seq": 4, "findings": [{ "path": "a.rs", "line": 3_usize, "kind": "api key", "hint": "sk-…" }], "sent": true }),
    );
    // A status is written as Customs' own word for it.
    let status = |s: CustomsStatus| json!(s).as_str().unwrap().to_owned();
    let found = |s: CustomsStatus| CustomsFound { kind: "npm".into(), name: "x".into(), path: "package.json".into(), status: status(s), why: None };
    for s in [CustomsStatus::Cleared, CustomsStatus::Held, CustomsStatus::Refused] {
        assert!(!status(s).is_empty());
        wrote(
            CustomsData { seq: Some(4), declared: None, found: vec![found(s)], sent: Some(false) }.to_json(),
            json!({ "seq": 4, "found": [{ "kind": "npm", "name": "x", "path": "package.json", "status": s }], "sent": false }),
        );
    }
    let declared = CustomsFound { kind: "cli".into(), name: "rg".into(), path: String::new(), status: "held".into(), why: Some("search".into()) };
    wrote(
        CustomsData { declared: Some(true), found: vec![declared], ..Default::default() }.to_json(),
        json!({ "declared": true, "found": [{ "kind": "cli", "name": "rg", "path": "", "status": "held", "why": "search" }] }),
    );
    // `kind` and `command` are always said, `null` when nothing showed the code working; the
    // state and its reason came last.
    let proof = |shown: Option<&str>, files: Option<i64>| ProofData {
        reply: Some(4),
        files: Some(files),
        kind: Some(shown.map(|_| "tests".to_owned())),
        command: Some(shown.map(str::to_owned)),
        hidden: Some(false),
        ..Default::default()
    };
    wrote(
        ProofData { state: "shown".into(), ..proof(Some("cargo test"), Some(2)) }.to_json(),
        json!({ "reply": 4, "files": 2_usize, "kind": "tests", "command": "cargo test", "hidden": false, "state": "shown" }),
    );
    wrote(
        ProofData { state: "asked".into(), why: Some("missing".into()), ..proof(None, Some(2)) }.to_json(),
        json!({ "reply": 4, "files": 2_usize, "kind": none, "command": none, "hidden": false, "state": "asked", "why": "missing" }),
    );
    // An answer to Hub's request copies `files` from the event that asked, `null` when it had none.
    wrote(
        ProofData { state: "missing".into(), ..proof(None, None) }.to_json(),
        json!({ "reply": 4, "files": none, "kind": none, "command": none, "hidden": false, "state": "missing" }),
    );
    let section = |heading: Option<&str>| DocsSection { doc: "docs/a.md".into(), heading: Some(heading.map(str::to_owned)), first: 3, last: 9, files: vec!["src/a.rs".into()] };
    let old = |heading: Option<&str>| json!({ "doc": "docs/a.md", "heading": heading, "first": 3_usize, "last": 9_usize, "files": ["src/a.rs"] });
    wrote(DocsData::new(4, "updated", vec![section(Some("A"))]).to_json(), json!({ "reply": 4, "state": "updated", "sections": [old(Some("A"))] }));
    wrote(DocsData::new(4, "asked", vec![section(None)]).to_json(), json!({ "reply": 4, "state": "asked", "sections": [old(None)] }));
    wrote(
        DocsData::new(4, "skipped", vec![section(None)]).why("reverted").to_json(),
        json!({ "reply": 4, "state": "skipped", "why": "reverted", "sections": [old(None)] }),
    );
}

#[test]
fn the_structs_about_what_another_model_judged_write_the_text_the_code_wrote_before_them() {
    let wrote = |d: Value, old: Value| assert_eq!(d.to_string(), old.to_string());
    let none: Option<String> = None;
    // A review is one value logged again and again: what is added later comes last.
    let part = ReviewChunk { title: "All changes".into(), files: vec!["a.rs".into()], review: true, state: "waiting".into(), ..Default::default() };
    let mut review = ReviewData {
        id: "r".into(),
        state: "running".into(),
        model: "opus".into(),
        files: vec!["a.rs".into()],
        base: "b".into(),
        head: Some("h".into()),
        seq: 9,
        by: "you".into(),
        reason: Some(None),
        ..Default::default()
    };
    let start = |chunks: Value| json!({ "id": "r", "state": "running", "model": "opus", "files": ["a.rs"], "base": "b", "head": "h", "seq": 9, "by": "you", "reason": none, "chunks": chunks });
    wrote(review.to_json(), start(json!([])));
    review.chunks = vec![part.clone()];
    wrote(review.to_json(), start(json!([{ "title": "All changes", "files": ["a.rs"], "review": true, "why": "", "state": "waiting" }])));
    let read = ReviewChunk { state: "done".into(), verdict: Some("Looks good".into()), text: Some("Looks good.".into()), ..part.clone() };
    let mut old = start(json!([{ "title": "All changes", "files": ["a.rs"], "review": true, "why": "", "state": "done", "verdict": "Looks good", "text": "Looks good." }]));
    old["state"] = json!("done");
    old["sent"] = json!(false);
    old["text"] = json!("Looks good.");
    wrote(ReviewData { state: "done".into(), chunks: vec![read], sent: Some(false), text: Some("Looks good.".into()), ..review.clone() }.to_json(), old);
    let lost = ReviewChunk { state: "failed".into(), error: Some("no".into()), ..part };
    let mut old = start(json!([{ "title": "All changes", "files": ["a.rs"], "review": true, "why": "", "state": "failed", "error": "no" }]));
    old["state"] = json!("failed");
    old["error"] = json!("no");
    wrote(ReviewData { state: "failed".into(), chunks: vec![lost], error: Some("no".into()), by: "you".into(), ..review }.to_json(), old);

    wrote(DesignData::switched(true, "start").to_json(), json!({ "on": true, "by": "start" }));
    wrote(DesignData { strayed: Some(vec!["a.rs".into()]), seq: Some(9), ..Default::default() }.to_json(), json!({ "strayed": ["a.rs"], "seq": 9 }));
    let check = DesignCheck { name: "ready".into(), title: "Ready".into(), model: Some(None), state: "running".into(), ..Default::default() };
    let design = DesignData { review: Some("d".into()), state: Some("running".into()), doc: Some("docs/design/a.md".into()), checks: Some(vec![check.clone()]), by: Some("you".into()), ..Default::default() };
    let mut old = json!({ "review": "d", "state": "running", "doc": "docs/design/a.md", "checks": [{ "name": "ready", "title": "Ready", "model": none, "state": "running" }], "by": "you" });
    wrote(design.to_json(), old.clone());
    old["checks"][0]["state"] = json!("done");
    old["checks"][0]["verdict"] = json!("Ready to build");
    old["checks"][0]["text"] = json!("Ready to build.");
    old["state"] = json!("done");
    old["ready"] = json!(true);
    old["verdict"] = json!("Ready to build");
    old["sent"] = json!(false);
    let ended = DesignCheck { state: "done".into(), verdict: Some("Ready to build".into()), text: Some("Ready to build.".into()), ..check };
    wrote(
        DesignData { state: Some("done".into()), checks: Some(vec![ended]), ready: Some(true), verdict: Some("Ready to build".into()), sent: Some(false), ..design }.to_json(),
        old,
    );

    // A vetting note: the start says `state` before `by`, the end `by` and `ms` before `state`.
    let task = TaskData { id: "v".into(), task: "vetting".into(), name: "x".into(), kind: "npm".into(), engine: "claude".into(), model: Some(Some("haiku".into())), state: "working".into(), by: "hub".into(), ..Default::default() };
    wrote(task.to_json(), json!({ "id": "v", "task": "vetting", "name": "x", "kind": "npm", "engine": "claude", "model": "haiku", "state": "working", "by": "hub" }));
    wrote(
        TaskData { model: Some(None), ..task.clone() }.to_json(),
        json!({ "id": "v", "task": "vetting", "name": "x", "kind": "npm", "engine": "claude", "model": none, "state": "working", "by": "hub" }),
    );
    wrote(
        TaskData { engine: String::new(), model: Some(None), ms: Some(7), state: "done".into(), verdict: Some("cleared".into()), reason: Some(String::new()), ..task.clone() }.to_json(),
        json!({ "id": "v", "task": "vetting", "name": "x", "kind": "npm", "engine": "", "model": none, "by": "hub", "ms": 7_i64, "state": "done", "verdict": "cleared", "reason": "" }),
    );
    wrote(
        TaskData { ms: Some(7), state: "failed".into(), reason: Some("no".into()), ..task }.to_json(),
        json!({ "id": "v", "task": "vetting", "name": "x", "kind": "npm", "engine": "claude", "model": "haiku", "by": "hub", "ms": 7_i64, "state": "failed", "reason": "no" }),
    );

    wrote(
        FocusData { fits: false, stage: "Stage 2".into(), line: 5, why: "w".into() }.to_json(),
        json!({ "fits": false, "stage": "Stage 2", "line": 5_u32, "why": "w" }),
    );
    let reading = |feature: &str| ReadingData { feature: feature.into(), confidence: 0.9, connection: "jev".into(), ms: 40, acted: true, ..Default::default() };
    wrote(
        ReadingData { seq: Some(8), state: Some("asks".into()), ..reading("turn") }.to_json(),
        json!({ "feature": "turn", "seq": 8, "state": "asks", "confidence": 0.9_f64, "connection": "jev", "ms": 40_u32, "acted": true }),
    );
    for line in [Some(5_u32), None] {
        wrote(
            ReadingData { stage: Some("Stage 2".into()), line: Some(line), text: Some("Chat queue".into()), ..reading("focus") }.to_json(),
            json!({ "feature": "focus", "stage": "Stage 2", "line": line, "text": "Chat queue", "confidence": 0.9_f64, "connection": "jev", "ms": 40_u32, "acted": true }),
        );
    }
}

#[test]
fn the_structs_about_what_shows_beside_the_conversation_write_the_text_the_code_wrote_before_them() {
    let wrote = |d: Value, old: Value| assert_eq!(d.to_string(), old.to_string());
    let none: Option<String> = None;
    // A page says its height even when it has none; `replaces` came last and only when there was one.
    let page = |height: Option<i64>, replaces: Option<&str>| PageData { id: "p".into(), title: "T".into(), height: Some(height), bytes: 9, replaces: replaces.map(str::to_owned) };
    wrote(page(Some(300), None).to_json(), json!({ "id": "p", "title": "T", "height": 300, "bytes": 9_usize }));
    wrote(page(None, Some("o")).to_json(), json!({ "id": "p", "title": "T", "height": none, "bytes": 9_usize, "replaces": "o" }));
    let options = vec![ReplyOption { label: "Yes".into(), message: "Yes".into() }, ReplyOption { label: "No".into(), message: "No, wait".into() }];
    let old = json!([{ "label": "Yes", "message": "Yes" }, { "label": "No", "message": "No, wait" }]);
    wrote(RepliesData { question: "Q?".into(), options: options.clone() }.to_json(), json!({ "question": "Q?", "options": old }));
    // An `ask` was the replies with `id` and `assumed` added after.
    wrote(
        AskData { question: "Q?".into(), options, id: "i".into(), assumed: "a".into() }.to_json(),
        json!({ "question": "Q?", "options": old, "id": "i", "assumed": "a" }),
    );
    let repo = PluginRepo { full_name: "a/b".into(), stars: 3, spark: vec![1, 2], ..Default::default() };
    wrote(PluginSuggestedData { repo: json!(repo), why: "w".into() }.to_json(), json!({ "repo": repo, "why": "w" }));
    wrote(
        ReactionData { seq: Some(4), emoji: Some("👍".into()), by: Some("you".into()), on: Some(false) }.to_json(),
        json!({ "seq": 4, "emoji": "👍", "by": "you", "on": false }),
    );
    for ask in [None, Some("Merge?".to_owned())] {
        wrote(
            GistData { upto: 4, points: vec!["Done.".into()], ask: Some(ask.clone()), by: Some("agent".into()) }.to_json(),
            json!({ "upto": 4, "points": ["Done."], "ask": ask, "by": "agent" }),
        );
    }
    wrote(OutputData { tool_call_id: "c".into(), data: "out\n".into() }.to_json(), json!({ "tool_call_id": "c", "data": "out\n" }));
    wrote(
        DeskNoteData { thread: "t".into(), title: "T".into(), state: "asks".into(), text: "x".into(), permission: true }.to_json(),
        json!({ "thread": "t", "title": "T", "state": "asks", "text": "x", "permission": true }),
    );
    let card = |state: &str| ReportedData { about: Some(4), problem: Some(7), state: state.into(), ..Default::default() };
    wrote(card("sent").to_json(), json!({ "about": 4, "problem": 7, "state": "sent" }));
    let fixing = ReportedData { fix: Some("f".into()), fix_title: Some("F".into()), ..card("fixing") };
    wrote(fixing.to_json(), json!({ "about": 4, "problem": 7, "state": "fixing", "fix": "f", "fix_title": "F" }));
    let pr = ReportedPr { number: 13, url: "u".into(), state: "open".into() };
    for pr in [None, Some(pr)] {
        let old = pr.as_ref().map(|p| json!({ "number": p.number, "url": p.url, "state": p.state }));
        wrote(
            ReportedData { state: "fixed".into(), text: Some("x".into()), pr: Some(pr), ..fixing.clone() }.to_json(),
            json!({ "about": 4, "problem": 7, "state": "fixed", "fix": "f", "fix_title": "F", "text": "x", "pr": old }),
        );
    }
}

#[test]
fn the_structs_about_work_given_to_someone_else_write_the_text_the_code_wrote_before_them() {
    let wrote = |d: Value, old: Value| assert_eq!(d.to_string(), old.to_string());
    let none: Option<String> = None;
    // A side job's end is its start written again, with how it went added last.
    let child = |model: Option<&str>| ChildData {
        id: "c".into(),
        title: "T".into(),
        kind: "image".into(),
        context: "brief".into(),
        engine: "codex".into(),
        model: Some(model.map(str::to_owned)),
        state: "working".into(),
        by: "agent".into(),
        ..Default::default()
    };
    let old = |model: Option<&str>| json!({ "id": "c", "title": "T", "kind": "image", "context": "brief", "engine": "codex", "model": model, "state": "working", "by": "agent" });
    wrote(child(Some("m")).to_json(), old(Some("m")));
    wrote(child(None).to_json(), old(None));
    let mut done = old(None);
    done["state"] = json!("done");
    done["reply"] = json!("Drew it.");
    done["images"] = json!(["a.png"]);
    wrote(ChildData { state: "done".into(), reply: Some("Drew it.".into()), images: Some(vec!["a.png".into()]), ..child(None) }.to_json(), done);
    let mut failed = old(Some("m"));
    failed["state"] = json!("failed");
    failed["error"] = json!("it was stopped");
    wrote(ChildData { state: "failed".into(), error: Some("it was stopped".into()), ..child(Some("m")) }.to_json(), failed);

    wrote(
        HandoffOfferData { tool: "K".into(), folder: "/k".into(), label: "Hand to K".into(), about: Some(None), can_build: true, why: "w".into() }.to_json(),
        json!({ "tool": "K", "folder": "/k", "label": "Hand to K", "about": none, "can_build": true, "why": "w" }),
    );
    // A handoff that never reached the tool names no work.
    let handed = |ok: bool, work: Option<&str>| HandedOffData { tool: "K".into(), title: "T".into(), ok, output: "o".into(), folder: Some("/k".into()), work: work.map(str::to_owned) };
    wrote(handed(true, Some("w-1")).to_json(), json!({ "tool": "K", "title": "T", "ok": true, "output": "o", "folder": "/k", "work": "w-1" }));
    wrote(handed(false, None).to_json(), json!({ "tool": "K", "title": "T", "ok": false, "output": "o", "folder": "/k" }));
    // A result says every field, `null` for what the tool did not say.
    let said = |text: Option<&str>| Some(text.map(str::to_owned));
    wrote(
        HandoffResultData { tool: "K".into(), title: "T".into(), state: "done".into(), proven: Some(Some(true)), model: said(Some("m")), cost: Some(Some(1.5)), said: said(None), link: said(None) }.to_json(),
        json!({ "tool": "K", "title": "T", "state": "done", "proven": true, "model": "m", "cost": 1.5_f64, "said": none, "link": none }),
    );
    wrote(
        HandoffResultData { tool: "K".into(), title: "T".into(), state: "stopped".into(), proven: Some(None), model: said(None), cost: Some(None), said: said(None), link: said(None) }.to_json(),
        json!({ "tool": "K", "title": "T", "state": "stopped", "proven": none, "model": none, "cost": none, "said": none, "link": none }),
    );
    // A worker run was the run as JSON with its new lines set after.
    let run = WorkerRun {
        tool: "K".into(),
        folder: "/k".into(),
        run: "r".into(),
        title: "T".into(),
        state: "running".into(),
        paused: false,
        said: None,
        steps: Vec::new(),
        round: Some(2),
        agents: Vec::new(),
        link: None,
        cost: Some(0.5),
        controls: Vec::new(),
        cursor: 7,
    };
    let lines = vec![WorkerLine { seq: 7, kind: "review".into(), step: None, round: Some(2), agent: None, text: "x".into() }];
    let mut old = json!(run);
    old["lines"] = json!(lines);
    wrote(WorkerRunData { run, lines }.to_json(), old);
}
