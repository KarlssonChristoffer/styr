//! The bench vocabulary (`crates/hub-core/src/bench/`): the built-in suite reads and keeps its
//! version, mistakes in a suite are refused, a case's folder is laid down as one commit, every
//! check word passes and fails on a synthetic log built from the typed payloads, the features suite
//! sits beside smoke with its own version, the checks and
//! the measurements read the real adapter recordings (`replay.rs`), and the summary arithmetic.

use std::path::Path;

use hub_core::bench::{self, Case, CheckResult, Measured};
use hub_core::model::{
    AskData, ErrorData, Event, EventData, PageData, PermissionAutoData, PermissionRequestData, PermissionResolvedData, ProofData, TurnEndData, TurnStartData, TurnUsage,
    UpdateData, UserMessageData,
};
use serde_json::{Value, json};

use crate::replay::play;

/// The smoke suite's version. Changing `bench/smoke.toml` changes it on purpose: a new series.
const SMOKE_HASH: &str = "b816ec531e97";
/// The features suite's version, the same way.
const FEATURES_HASH: &str = "b53d13be1808";

fn typed<T: EventData>(seq: i64, ts: i64, data: &T) -> Event {
    Event { thread_id: "bench".into(), seq, ts, kind: T::KIND, data: data.to_json() }
}

fn update(seq: i64, ts: i64, update: Value) -> Event {
    typed(seq, ts, &UpdateData(update.as_object().cloned().unwrap_or_default()))
}

fn said(seq: i64, ts: i64, text: &str) -> Event {
    update(seq, ts, json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": text } }))
}

fn message(seq: i64, ts: i64, text: &str) -> Event {
    typed(seq, ts, &UserMessageData { text: text.into(), ..Default::default() })
}

fn end(seq: i64, ts: i64, prompt: i64, stop: &str) -> Event {
    typed(seq, ts, &TurnEndData { stop_reason: stop.into(), prompt: Some(prompt), ..Default::default() })
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@localhost")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@localhost")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A case with every check word, each set so that the good run passes it.
const EVERY_WORD: &str = r#"
suite = "every-word"
engine = "e"
model = "fast"
repeats = 1
timeout_s = 60
cost_cap_usd = 1.0

[[case]]
id = "all"
prompts = ["do it"]
[case.files]
"keep.txt" = "keep\n"
"edit.txt" = "old\n"
[[case.check]]
ended = "end_turn"
[[case.check]]
no_error = true
[[case.check]]
settled = true
[[case.check]]
refused_max = 0
[[case.check]]
file = "edit.txt"
matches = 'new'
lines = 1
[[case.check]]
unchanged = ["keep.txt"]
[[case.check]]
reply = '(?m)^done$'
[[case.check]]
names = ["out.csv"]
[[case.check]]
hub_tool = "page_check"
min = 1
[[case.check]]
proof = "shown"
[[case.check]]
page = true
[[case.check]]
steps = 2
[[case.check]]
commits = 1
[[case.check]]
event = "page"
matches = '"title":"Page"'
"#;

fn every_word() -> Case {
    bench::parse(EVERY_WORD).unwrap().cases.remove(0)
}

fn plan(done: usize) -> Value {
    let entries: Vec<Value> = (0..2).map(|i| json!({ "content": format!("step {i}"), "priority": "medium", "status": if i < done { "completed" } else { "pending" } })).collect();
    json!({ "sessionUpdate": "plan", "entries": entries })
}

fn failed(results: &[CheckResult]) -> Vec<&str> {
    results.iter().filter(|r| !r.ok).map(|r| r.check.as_str()).collect()
}

#[test]
fn the_smoke_suite_is_built_in_and_keeps_its_version() {
    let suite = bench::builtin("smoke").unwrap();
    let ids: Vec<&str> = suite.cases.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["read-answer", "edit-check", "csv-svg", "html-page", "steps-plan"]);
    assert_eq!((suite.model.as_str(), suite.repeats, suite.timeout_s), (bench::suite::FAST, 3, 600));
    assert!(!suite.engine.is_empty() && suite.cost_cap_usd > 0.0);
    assert!(bench::builtin_names().any(|n| n == "smoke"));
    let text = include_str!("../../bench/smoke.toml");
    assert_eq!(suite.hash, bench::suite_hash(text));
    assert_eq!(suite.hash, SMOKE_HASH, "the smoke suite changed: that is a new result series; update SMOKE_HASH if you meant it");
    assert!(bench::builtin("nope").is_err());
}

#[test]
fn the_features_suite_is_built_in_beside_smoke() {
    let suite = bench::builtin("features").unwrap();
    let ids: Vec<&str> = suite.cases.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["files-in-chat", "side-job", "handoff", "branch-commit"]);
    assert_eq!(bench::builtin_names().collect::<Vec<_>>(), ["smoke", "features"], "smoke stays first, the app's default");
    // The same settings as smoke: data, no engine named beyond the suite's own key.
    let smoke = bench::builtin("smoke").unwrap();
    assert_eq!((suite.engine.as_str(), suite.model.as_str(), suite.repeats, suite.timeout_s), (smoke.engine.as_str(), smoke.model.as_str(), 3, 600));
    let text = include_str!("../../bench/features.toml");
    assert_eq!(suite.hash, bench::suite_hash(text));
    assert_eq!(suite.hash, FEATURES_HASH, "the features suite changed: that is a new result series; update FEATURES_HASH if you meant it");
    // Parsing is strict, so every check is a known word; each case is judged on more than its end.
    for case in &suite.cases {
        assert!(case.checks.len() >= 4, "{}", case.id);
        assert!(case.mode.is_empty(), "{}: the bench refuses a mode", case.id);
    }
    // Only the file case sends a file in its message, and that file is not in its folder.
    let files = &suite.cases[0];
    assert_eq!(files.attach.keys().collect::<Vec<_>>(), ["receipt.txt"]);
    assert!(!files.files.contains_key("receipt.txt"));
    assert!(suite.cases[1..].iter().all(|c| c.attach.is_empty()));
    // Only the handoff case needs something of the machine: a tool of the person's that takes handoffs.
    let needs: Vec<(&str, &[String])> = suite.cases.iter().filter(|c| !c.needs.is_empty()).map(|c| (c.id.as_str(), c.needs.as_slice())).collect();
    assert_eq!(needs, [("handoff", &["handoff_tool".to_owned()][..])]);
}

#[test]
fn a_typo_in_a_suite_is_refused_with_the_case_named() {
    let word = format!("{:#}", bench::parse(&EVERY_WORD.replace("page = true", "pages = true")).unwrap_err());
    assert!(word.contains("case \"all\"") && word.contains("pages"), "{word}");
    let key = format!("{:#}", bench::parse(&EVERY_WORD.replace("min = 1", "minimum = 1")).unwrap_err());
    assert!(key.contains("case \"all\"") && key.contains("`minimum`"), "{key}");
}

#[test]
fn a_case_is_laid_down_as_one_commit() {
    let dir = hub_core::janitor::test_dir();
    let case = every_word();
    let fixture = bench::lay_down(&case, &dir).unwrap();
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]), "1");
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), fixture);
    assert_eq!(git(&dir, &["log", "-1", "--format=%an"]), "Hub");
    assert_eq!(std::fs::read_to_string(dir.join("edit.txt")).unwrap(), "old\n");
    // A folder with something in it is not a fresh one.
    assert!(bench::lay_down(&case, &dir).is_err());

    let mut climbing = case.clone();
    climbing.files.insert("../x".into(), "out".into());
    let other = hub_core::janitor::test_dir();
    assert!(bench::lay_down(&climbing, &other.join("case")).is_err());
    assert!(!other.join("x").exists(), "a fixture climbed out of its folder");
    hub_core::locked::clean_dir(dir);
    hub_core::locked::clean_dir(other);
}

#[test]
fn every_check_word_passes_on_a_good_run_and_fails_on_a_bad_one() {
    let case = every_word();

    // The good run: the agent edited a file, wrote one, committed, used Hub's tools and said so.
    let good = hub_core::janitor::test_dir();
    bench::lay_down(&case, &good).unwrap();
    std::fs::write(good.join("edit.txt"), "new\n").unwrap();
    std::fs::create_dir_all(good.join("out")).unwrap();
    std::fs::write(good.join("out/out.csv"), "a\n").unwrap();
    git(&good, &["add", "-A"]);
    git(&good, &["commit", "-q", "-m", "work"]);
    let t = 1_800_000_000_000;
    let events = vec![
        message(1, t, "do it"),
        typed(2, t + 10, &TurnStartData { prompt: Some(1), ..Default::default() }),
        update(3, t + 20, json!({ "sessionUpdate": "tool_call", "toolCallId": "a", "title": "mcp__hub__page_check" })),
        update(4, t + 30, json!({ "sessionUpdate": "tool_call_update", "toolCallId": "a", "status": "completed" })),
        update(5, t + 40, plan(2)),
        typed(6, t + 50, &PageData { id: "p".into(), title: "Page".into(), ..Default::default() }),
        typed(7, t + 60, &PermissionRequestData { id: "q".into(), tool_call: json!({ "title": "Write" }), ..Default::default() }),
        typed(8, t + 70, &PermissionResolvedData { id: "q".into(), ..Default::default() }),
        typed(9, t + 75, &PermissionAutoData { allowed: true, why: "allowed".into(), ..Default::default() }),
        said(10, t + 80, "I wrote `out/out.csv`.\ndone"),
        end(11, t + 90, 1, "end_turn"),
        typed(12, t + 95, &ProofData { reply: Some(11), state: "shown".into(), ..Default::default() }),
    ];
    let results = bench::check(&case, &events, &good, &[1]);
    assert_eq!(results.len(), 14);
    assert_eq!(failed(&results), Vec::<&str>::new(), "{results:#?}");

    // The bad run: nothing done, everything that can go wrong did.
    let bad = hub_core::janitor::test_dir();
    bench::lay_down(&case, &bad).unwrap();
    std::fs::write(bad.join("keep.txt"), "changed\n").unwrap();
    let events = vec![
        message(1, t, "do it"),
        typed(2, t + 10, &TurnStartData { prompt: Some(1), ..Default::default() }),
        update(3, t + 20, json!({ "sessionUpdate": "tool_call", "toolCallId": "b", "title": "mcp__other__page_check" })),
        update(4, t + 40, plan(1)),
        typed(5, t + 50, &PermissionRequestData { id: "q".into(), tool_call: json!({ "title": "Write" }), ..Default::default() }),
        typed(6, t + 55, &PermissionAutoData { allowed: false, why: "a protected file".into(), ..Default::default() }),
        typed(7, t + 60, &ErrorData::new("the engine fell over")),
        said(8, t + 80, "not finished"),
        end(9, t + 90, 1, "max_tokens"),
        typed(10, t + 95, &ProofData { reply: Some(9), state: "asked".into(), ..Default::default() }),
    ];
    let results = bench::check(&case, &events, &bad, &[1]);
    let still_ok: Vec<&CheckResult> = results.iter().filter(|r| r.ok).collect();
    assert!(still_ok.is_empty(), "{still_ok:#?}");
    assert!(results.iter().all(|r| r.why.as_deref().is_some_and(|w| !w.is_empty())), "{results:#?}");

    hub_core::locked::clean_dir(good);
    hub_core::locked::clean_dir(bad);
}

#[test]
fn the_smaller_readings_of_the_words() {
    let only = |check: &str| bench::parse(&format!("{}\n[[case.check]]\n{check}\n", EVERY_WORD.split("[[case.check]]").next().unwrap())).unwrap().cases.remove(0);
    let dir = hub_core::janitor::test_dir();
    let t = 1_800_000_000_000;
    let ok = |case: &Case, events: &[Event], sent: &[i64]| bench::check(case, events, &dir, sent).iter().all(|r| r.ok);

    // `ended` looks at the turn of every sent prompt, and needs one sent.
    let ended = only("ended = \"end_turn\"");
    let two = [message(1, t, "a"), end(2, t, 1, "end_turn"), message(3, t, "b"), end(4, t, 3, "cancelled")];
    assert!(ok(&ended, &two, &[1]));
    assert!(!ok(&ended, &two, &[1, 3]));
    assert!(!ok(&ended, &two, &[]));

    // An `ask_user` question waits until a later message quotes it, as the app's answer does.
    let settled = only("settled = true");
    let ask = typed(2, t, &AskData { question: "Blue or red?".into(), assumed: "blue".into(), id: "k".into(), ..Default::default() });
    assert!(!ok(&settled, &[message(1, t, "go"), ask.clone()], &[1]));
    assert!(ok(&settled, &[message(1, t, "go"), ask, message(3, t, "You asked \u{201c}Blue or red?\u{201d} and assumed \u{201c}blue\u{201d}; the person answered: red")], &[1]));

    // No proof event, or a skipped one, is "none".
    let none = only("proof = \"none\"");
    assert!(ok(&none, &[], &[1]));
    assert!(ok(&none, &[typed(1, t, &ProofData { state: "skipped".into(), ..Default::default() })], &[1]));
    assert!(!ok(&none, &[typed(1, t, &ProofData { state: "shown".into(), ..Default::default() })], &[1]));
    let failed_proof = only("proof = \"failed\"");
    assert!(ok(&failed_proof, &[typed(1, t, &ProofData { state: "missing".into(), ..Default::default() })], &[1]));

    // A Hub tool is counted once per call, however many updates the call has.
    let twice = only("hub_tool = \"update_steps\"\nmin = 2");
    let calls = |ids: &[&str]| -> Vec<Event> {
        ids.iter().enumerate().map(|(n, id)| update(n as i64 + 1, t, json!({ "sessionUpdate": "tool_call_update", "toolCallId": id, "title": "hub.update_steps" }))).collect()
    };
    assert!(!ok(&twice, &calls(&["a", "a", "a"]), &[1]));
    assert!(ok(&twice, &calls(&["a", "b"]), &[1]));
    hub_core::locked::clean_dir(dir);
}

/// The sent prompts of a replayed thread: its user messages.
fn sent(events: &[Event]) -> Vec<i64> {
    events.iter().filter(|e| e.kind == hub_core::model::EventKind::UserMessage).map(|e| e.seq).collect()
}

fn recording_case() -> Case {
    let head = EVERY_WORD.split("[[case.check]]").next().unwrap();
    let checks = [
        "ended = \"end_turn\"",
        "no_error = true",
        "hub_tool = \"page_check\"\nmin = 0",
        "hub_tool = \"page_check\"\nmin = 1",
        "proof = \"none\"",
        "proof = \"shown\"",
        "reply = '(?s)\\S'",
        "reply = '^this is not the answer$'",
        "settled = true",
    ];
    let text = checks.iter().fold(head.to_owned(), |t, c| format!("{t}\n[[case.check]]\n{c}\n"));
    bench::parse(&text).unwrap().cases.remove(0)
}

fn verdicts(results: &[CheckResult]) -> Vec<bool> {
    results.iter().map(|r| r.ok).collect()
}

fn usage(input: i64, output: i64, cache_read: i64, cache_write: i64) -> Option<TurnUsage> {
    Some(TurnUsage { input: Some(input), output: Some(output), cache_read: Some(cache_read), cache_write: Some(cache_write) })
}

#[tokio::test]
async fn the_checks_and_measures_read_a_real_claude_adapter_turn() {
    let events = play("acp-claude-tool-turn.trace.jsonl", "claude").await;
    let sent = sent(&events);
    let dir = hub_core::janitor::test_dir();
    let results = bench::check(&recording_case(), &events, &dir, &sent);
    // ended, no_error, page_check >= 0, >= 1, proof none, shown, a reply, the wrong reply, settled.
    assert_eq!(verdicts(&results), [true, true, true, false, true, false, true, false, true], "{results:#?}");

    let m: Measured = bench::measure(&events, &sent, None, &[]);
    assert_eq!(m.tokens, usage(4, 105, 58_290, 58_397));
    assert_eq!(m.total_tokens(), Some(4 + 105 + 58_290 + 58_397));
    assert_eq!(m.cost_usd, Some(0.240601));
    assert_eq!((m.turns, m.proof_rounds, m.perm_refused), (1, 0, 0));
    assert!(m.tool_calls >= 1 && m.wall_ms.is_some() && m.first_word_ms.is_some(), "{m:?}");
    let model = m.model.clone().expect("the adapter names its model");
    assert_eq!(m.invalid, None);
    assert_eq!(bench::measure(&events, &sent, Some(&model), &[]).invalid, None);
    assert!(bench::measure(&events, &sent, Some("another-model"), &[]).invalid.is_some());
    hub_core::locked::clean_dir(dir);
}

#[tokio::test]
async fn the_checks_and_measures_read_a_real_codex_adapter_turn() {
    let events = play("acp-codex-tool-turn.trace.jsonl", "codex").await;
    let sent = sent(&events);
    let dir = hub_core::janitor::test_dir();
    let results = bench::check(&recording_case(), &events, &dir, &sent);
    assert_eq!(verdicts(&results), [true, true, true, false, true, false, true, false, true], "{results:#?}");

    let m = bench::measure(&events, &sent, None, &[]);
    assert_eq!(m.tokens, usage(7_401, 67, 34_304, 0));
    // This adapter sends no cost.
    assert_eq!(m.cost_usd, None);
    assert_eq!(m.turns, 1);
    hub_core::locked::clean_dir(dir);
}

#[tokio::test]
async fn a_permission_question_is_settled_only_once_it_is_closed() {
    let events = play("acp-claude-permission.trace.jsonl", "claude").await;
    let sent = sent(&events);
    let dir = hub_core::janitor::test_dir();
    let settled = |events: &[Event]| bench::check(&recording_case(), events, &dir, &sent).into_iter().find(|r| r.check == "settled").unwrap();
    // The turn's end closed the open question (`permission_resolved`), so the whole log is settled.
    assert!(settled(&events).ok, "{:?}", settled(&events));
    // Cut just before it closed, the question is left waiting.
    let closed = events.iter().position(|e| e.kind == hub_core::model::EventKind::PermissionResolved).expect("the question is closed in the log");
    let open = settled(&events[..closed]);
    assert!(!open.ok && open.why.as_deref().is_some_and(|w| w.contains("permission")), "{open:?}");
    assert_eq!(bench::measure(&events, &sent, None, &[]).tokens, usage(4, 110, 89_071, 27_688));
    hub_core::locked::clean_dir(dir);
}
