//! Guards Where it goes: each answer lands in its lane and with who asked for it, unknown models
//! and missing prices say so, a Codex log is read once per answer and is Hub's only when Hub held
//! it, and hours, days, machines and all-time totals add up without counting a balance twice.

use super::*;
use crate::activity::claude_code::{CcMark, CcSession};
use crate::model::{Owner, OwnerKind, SpendAccount, ThreadStatus};

fn thread(id: &str, cwd: &str) -> Thread {
    Thread {
        id: id.into(),
        title: id.into(),
        cwd: cwd.into(),
        engine: "claude".into(),
        status: ThreadStatus::Idle,
        engine_session_id: Some(format!("s-{id}")),
        title_locked: false,
        archived: false,
        host: None, project: None, display: None,
        created_at: 0,
        updated_at: 0,
        returned_at: None,
        seen_at: None,
        settled_at: None,
        sent_at: None,
        pinned_at: None,
        snoozed_until: None,
        auto_settle: true,
        parent: None,
        reading: None,
        owner: None,
        cloud: None,
    }
}

fn mark(session: &str, ts: i64, cache_write: i64) -> CcMark {
    CcMark {
        key: format!("{session}-{ts}"),
        session_id: session.into(),
        ts,
        prompt: None,
        tokens: 100,
        model: Some("claude-opus-5-5".into()),
        input: 50,
        output: 50,
        cache_write,
        cache_read: 1000,
    }
}

fn session(id: &str, cwd: &str) -> CcSession {
    CcSession { id: id.into(), cwd: Some(cwd.into()), title: None }
}

const DAY: i64 = 86_400_000;

#[test]
fn each_claude_answer_goes_to_its_lane_and_who_asked_for_it() {
    let mine = thread("a", "/p/hub");
    let chat = thread("b", "/p/chats/2026-11-03");
    let mut side = thread("c", "/p/hub");
    side.parent = Some("a".into());
    let mut guard = thread("d", "/p/hub");
    guard.owner = Some(Owner::new(OwnerKind::Vakten, "Vakten", None));
    let mut on_key = thread("e", "/p/hub");
    on_key.engine = "claude-deepseek".into();
    let threads = vec![mine, chat, side, guard, on_key];
    // An older session of thread a, known only from its log, and one of a thread since removed.
    let sessions = vec![("a".to_owned(), "old-a".to_owned()), ("gone".to_owned(), "s-gone".to_owned())];
    let marks = vec![
        mark("s-a", 10, 0),
        mark("old-a", DAY + 11, 10),
        mark("s-b", 12, 0),
        mark("s-c", 13, 0),
        mark("s-d", 14, 0),
        mark("s-e", 15, 0),
        mark("s-gone", 16, 0),
        mark("helper", 17, 30_000),
        mark("mine", DAY + 18, 0),
        mark("slipped", DAY + 19, 0),
    ];
    // "slipped" ran outside the temp folder, but a packing slip says a one-shot went into it.
    let cc = vec![session("helper", "/tmp/"), session("mine", "/home/me/code"), session("slipped", "/home/me/code")];
    let paid: HashSet<String> = ["claude-deepseek".to_owned()].into();
    let helpers: HashSet<String> = ["slipped".to_owned()].into();
    let known = Known { threads: &threads, sessions: &sessions, chats: "/p", paid_engines: &paid, helpers: &helpers };
    let mut tally = Tally::new(vec!["1970-01-01".into(), "1970-01-02".into()], 0);
    claude(&mut tally, &marks, &cc, &known, &crate::config::claude_prices(), "/tmp");
    let spend = tally.finish(2);
    let lane = |k: &str| spend.lanes.iter().find(|l| l.key == k).unwrap();
    let source = |k: &str| spend.sources.iter().find(|s| s.key == k).unwrap_or_else(|| panic!("no {k}"));
    // Answers on DeepSeek's key are not Claude's.
    assert_eq!(lane(HUB).calls, 8);
    assert_eq!(lane(CLAUDE_CODE).calls, 1);
    assert_eq!((source("you").calls, source("you").threads), (2, 1));
    for key in ["chat", "side_job", "vakten", "deleted"] {
        assert_eq!(source(key).calls, 1, "{key}");
    }
    assert_eq!((source("helper").calls, source("helper").threads), (2, 0));
    // The helper wrote a whole context, so it costs most.
    assert_eq!(spend.sources[0].key, "helper");
    assert_eq!(lane(HUB).engines.len(), 1);
    assert!((lane(HUB).daily.iter().sum::<f64>() - lane(HUB).cost).abs() < 1e-9);
    assert!(lane(CLAUDE_CODE).daily[0] == 0.0 && lane(CLAUDE_CODE).daily[1] > 0.0);
    assert!(spend.lanes.iter().all(|l| l.priced));
}

#[test]
fn a_question_or_an_unknown_model_is_no_answer_and_a_missing_price_says_so() {
    let mut asked = mark("x", 1, 0);
    asked.prompt = Some("hi".into());
    let mut odd = mark("x", 2, 0);
    odd.model = Some("mystery-1".into());
    let mut blank = mark("x", 3, 0);
    blank.model = None;
    let known = Known { threads: &[], sessions: &[], chats: "/p", paid_engines: &HashSet::new(), helpers: &HashSet::new() };
    let mut tally = Tally::new(vec!["1970-01-01".into()], 0);
    claude(&mut tally, &[asked, odd, blank], &[], &known, &crate::config::claude_prices(), "/tmp");
    let spend = tally.finish(1);
    let cc = spend.lanes.iter().find(|l| l.key == CLAUDE_CODE).unwrap();
    assert_eq!(cc.calls, 1);
    assert!(!cc.priced);
}

#[test]
fn reads_a_codex_log_once_per_answer() {
    let log = r#"{"timestamp":"2026-11-03T10:00:00.000Z","type":"session_meta","payload":{"id":"cx-1","cwd":"/p","originator":"codex_cli_rs"}}
{"timestamp":"2026-11-03T10:00:01.000Z","type":"turn_context","payload":{"cwd":"/p","model":"gpt-5-codex"}}
{"timestamp":"2026-11-03T10:00:05.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":1200},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":800,"output_tokens":200,"total_tokens":1200}}}}
{"timestamp":"2026-11-03T10:00:06.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":1200},"last_token_usage":{"input_tokens":1000,"cached_input_tokens":800,"output_tokens":200,"total_tokens":1200}}}}
{"timestamp":"2026-11-03T10:00:07.000Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{}}}
{"timestamp":"2026-11-03T10:01:00.000Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":2500},"last_token_usage":{"input_tokens":1100,"cached_input_tokens":1000,"output_tokens":200,"total_tokens":1300}}}}"#;
    let s = read_rollout(log);
    assert_eq!((s.id.as_str(), s.originator.as_str(), s.uses.len()), ("cx-1", "codex_cli_rs", 2));
    assert_eq!(s.uses[0].model, "gpt-5-codex");
    let (cost, known) = codex_cost(&s.uses[0]);
    assert!(known);
    // 200 fresh at 1.25, 800 cached at 0.125, 200 out at 10.
    assert!((cost - (200.0 * 1.25 + 800.0 * 0.125 + 200.0 * 10.0) / 1e6).abs() < 1e-12);
}

#[test]
fn a_codex_session_is_hubs_when_a_thread_holds_it_or_hubs_adapter_started_it() {
    let mut t = thread("a", "/p/hub");
    t.engine = "codex".into();
    t.engine_session_id = Some("cx-hub".into());
    let threads = vec![t];
    let at = crate::usage::iso_ms("2026-11-03T10:00:00Z").unwrap();
    let u = CodexUse { ts: at, model: "mystery".into(), input: 1000, cached: 0, output: 100 };
    let log = |id: &str, by: &str| Arc::new(CodexSession { id: id.into(), originator: by.into(), uses: vec![u.clone()] });
    let logs = vec![log("cx-hub", "codex_cli_rs"), log("cx-other", "codex-acp"), log("cx-mine", "codex_cli_rs"), log("cx-vs", "codex_vscode")];
    let known = Known { threads: &threads, sessions: &[], chats: "/p", paid_engines: &HashSet::new(), helpers: &HashSet::new() };
    let mut tally = Tally::new(vec!["2026-11-03".into()], 0);
    codex(&mut tally, &logs, &known, at - 1, at + DAY);
    tally.paid("deepseek", "2026-11-03", 0.5);
    tally.paid("deepseek", "2026-10-01", 9.0);
    let spend = tally.finish(1);
    let lane = |k: &str| spend.lanes.iter().find(|l| l.key == k).unwrap();
    assert_eq!((lane(HUB).calls, lane(CODEX).calls), (2, 2));
    // An unknown model is priced as GPT-5 and marked as an estimate.
    assert!(!lane(CODEX).priced && lane(CODEX).cost > 0.0);
    let engines: Vec<(&str, bool)> = lane(HUB).engines.iter().map(|e| (e.key.as_str(), e.paid)).collect();
    assert!(engines.contains(&("deepseek", true)) && engines.contains(&("codex", false)));
    // A day outside the range is left out.
    assert!((lane(HUB).engines.iter().find(|e| e.key == "deepseek").unwrap().cost - 0.5).abs() < 1e-9);
    let mut sources: Vec<&str> = spend.sources.iter().map(|s| s.key.as_str()).collect();
    sources.sort();
    assert_eq!(sources, ["deleted", "you"]);
}

fn lane_of(key: &str, cost: f64, daily: Vec<f64>, engines: Vec<SpendEngine>) -> SpendLane {
    SpendLane { key: key.into(), cost, calls: 1, tokens: 10, daily, engines, priced: true }
}

#[test]
fn hours_fall_on_their_hour_and_machines_add_up_by_the_hour() {
    let start = crate::activity::start_of("2026-11-03", 0).unwrap();
    let mut tally = Tally::new(vec!["2026-11-03".into(), "2026-11-04".into()], 0).hourly(start, 48);
    tally.hub("claude", "you", None, start + 30 * 60_000, 2.0, 10, true);
    tally.hub("codex", "you", None, start + 25 * HOUR + 1, 1.0, 10, true);
    tally.alone(CLAUDE_CODE, start + 47 * HOUR + 5, 3.0, 10, true);
    tally.alone(CLAUDE_CODE, start - 1, 9.0, 10, true);
    tally.alone(CLAUDE_CODE, start + 48 * HOUR, 9.0, 10, true);
    let spend = tally.finish(2);
    let hours = spend.hourly.expect("asked for hours");
    assert_eq!(hours.start, start);
    let hub = hours.lanes.iter().find(|l| l.key == HUB).unwrap();
    assert_eq!(hub.daily.len(), 48);
    assert_eq!((hub.daily[0], hub.daily[25], hub.cost), (2.0, 1.0, 3.0));
    assert_eq!(hub.engines.iter().find(|e| e.key == "codex").unwrap().daily[25], 1.0);
    assert_eq!(hours.sources[0].daily.as_ref().unwrap()[25], 1.0);
    assert_eq!(spend.sources[0].daily, Some(vec![2.0, 1.0]), "who asked also per date");
    let cc = hours.lanes.iter().find(|l| l.key == CLAUDE_CODE).unwrap();
    assert_eq!((cc.daily[47], cc.cost), (3.0, 3.0), "answers outside the hours are left out");

    // Another machine an hour ahead in its start: its first hour is this one's second.
    let mut here = SpendHourly { start, lanes: vec![lane_of(HUB, 1.0, vec![1.0, 0.0, 0.0], vec![])], sources: vec![] };
    let there = SpendHourly {
        start: start + HOUR,
        lanes: vec![lane_of(HUB, 2.0, vec![2.0, 0.0, 5.0], vec![]), lane_of(CODEX, 1.0, vec![0.0, 1.0, 0.0], vec![])],
        sources: vec![SpendSource { key: "you".into(), daily: Some(vec![2.0, 0.0, 5.0]), ..Default::default() }],
    };
    add_hours(&mut here, &there);
    assert_eq!(here.lanes[0].daily, vec![1.0, 2.0, 0.0]);
    assert_eq!(here.lanes[0].cost, 3.0);
    assert_eq!(here.lanes.iter().find(|l| l.key == CODEX).unwrap().daily, vec![0.0, 0.0, 1.0]);
    assert_eq!(here.sources[0].daily, Some(vec![0.0, 2.0, 0.0]));
    // Asked without hours, none are sent.
    assert!(Tally::new(vec!["2026-11-03".into()], 0).finish(1).hourly.is_none());
}

#[test]
fn in_tokens_every_answer_adds_its_tokens_and_the_balance_adds_nothing() {
    let start = crate::activity::start_of("2026-11-03", 0).unwrap();
    let mut tally = Tally::new(vec!["2026-11-03".into()], 0).in_tokens().hourly(start, 24);
    tally.hub("claude", "you", Some("t1"), start + 3_600_000, 2.5, 1200, true);
    tally.alone(CLAUDE_CODE, start + 7_200_000, 9.0, 300, true);
    tally.paid("deepseek", "2026-11-03", 4.0);
    let spend = tally.finish(1);
    assert_eq!(spend.unit.as_deref(), Some("tokens"));
    let lane = |k: &str| spend.lanes.iter().find(|l| l.key == k).unwrap().clone();
    assert_eq!((lane(HUB).cost, lane(HUB).daily.clone()), (1200.0, vec![1200.0]));
    assert_eq!(lane(CLAUDE_CODE).cost, 300.0);
    assert_eq!(spend.sources[0].daily, Some(vec![1200.0]));
    assert_eq!(spend.hourly.unwrap().lanes.iter().find(|l| l.key == HUB).unwrap().daily[1], 1200.0);
    assert!(Tally::new(vec!["2026-11-03".into()], 0).finish(1).unit.is_none());
}

#[test]
fn machines_add_up_by_date_and_the_paid_balance_counts_once() {
    let mut here = Spend {
        days: 2,
        dates: vec!["2026-11-02".into(), "2026-11-03".into()],
        lanes: vec![lane_of(HUB, 3.0, vec![1.0, 2.0], vec![SpendEngine { key: "deepseek".into(), cost: 1.0, daily: vec![0.0, 1.0], paid: true }])],
        sources: vec![SpendSource { key: "you".into(), calls: 1, threads: 1, tokens: 10, cost: 2.0, daily: Some(vec![0.0, 2.0]) }],
        machines: None,
        notes: None,
        accounts: None,
        hourly: None,
        unit: None,
    };
    // A day behind: its "2026-11-01" is outside this page and its "2026-11-02" is this one's first day.
    let there = Spend {
        days: 2,
        dates: vec!["2026-11-01".into(), "2026-11-02".into()],
        lanes: vec![
            lane_of(HUB, 6.0, vec![1.0, 5.0], vec![
                SpendEngine { key: "claude".into(), cost: 2.0, daily: vec![1.0, 1.0], paid: false },
                SpendEngine { key: "deepseek".into(), cost: 4.0, daily: vec![0.0, 4.0], paid: true },
            ]),
            lane_of(CODEX, 0.5, vec![0.0, 0.5], vec![]),
            lane_of(CLAUDE_CLOUD, 9.0, vec![4.0, 5.0], vec![]),
        ],
        sources: vec![SpendSource { key: "you".into(), calls: 2, threads: 1, tokens: 5, cost: 1.0, daily: Some(vec![0.5, 0.5]) }],
        machines: None,
        notes: None,
        accounts: None,
        hourly: None,
        unit: None,
    };
    let there = without_paid(there);
    assert!((part("mac", &there, None).cost - 2.5).abs() < 1e-9, "the other machine's DeepSeek and cloud sessions are not counted again");
    add(&mut here, &there);
    let hub = here.lanes.iter().find(|l| l.key == HUB).unwrap();
    assert!((hub.cost - 5.0).abs() < 1e-9);
    assert_eq!(hub.daily, vec![2.0, 2.0]);
    assert_eq!(hub.engines.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), vec!["deepseek", "claude"]);
    assert_eq!(here.lanes.iter().find(|l| l.key == CODEX).unwrap().daily, vec![0.5, 0.0]);
    assert_eq!((here.sources[0].calls, here.sources[0].threads), (3, 2));
    assert_eq!(here.sources[0].daily, Some(vec![0.5, 2.0]), "who asked is split by the same dates");
}

#[test]
fn all_time_reaches_back_to_the_machine_that_began_first() {
    let mut here = Spend {
        days: 2,
        dates: vec!["2026-11-02".into(), "2026-11-03".into()],
        lanes: vec![lane_of(HUB, 3.0, vec![1.0, 2.0], vec![SpendEngine { key: "claude".into(), cost: 3.0, daily: vec![1.0, 2.0], paid: false }])],
        sources: vec![],
        machines: None,
        notes: None,
        accounts: Some(vec![SpendAccount { key: "codex".into(), tokens: 5, daily: vec![2, 3], lifetime: None }]),
        hourly: None,
        unit: None,
    };
    let there = Spend {
        days: 4,
        dates: vec!["2026-10-31".into(), "2026-11-01".into(), "2026-11-02".into(), "2026-11-03".into()],
        lanes: vec![lane_of(CLAUDE_CODE, 7.0, vec![4.0, 0.0, 1.0, 2.0], vec![])],
        sources: vec![],
        machines: None,
        notes: None,
        accounts: None,
        hourly: None,
        unit: None,
    };
    widen(&mut here, &there, 60);
    add(&mut here, &there);
    assert_eq!(here.dates, vec!["2026-10-31", "2026-11-01", "2026-11-02", "2026-11-03"]);
    assert_eq!(here.days, 4);
    let hub = here.lanes.iter().find(|l| l.key == HUB).unwrap();
    assert_eq!((hub.daily.clone(), hub.engines[0].daily.clone()), (vec![0.0, 0.0, 1.0, 2.0], vec![0.0, 0.0, 1.0, 2.0]));
    assert_eq!(here.lanes.iter().find(|l| l.key == CLAUDE_CODE).unwrap().daily, vec![4.0, 0.0, 1.0, 2.0]);
    assert_eq!(here.accounts.unwrap()[0].daily, vec![0, 0, 2, 3]);
    // A machine that began later changes nothing.
    let mut later = there.clone();
    widen(&mut later, &Spend { dates: vec!["2026-11-03".into()], ..Spend::default() }, 60);
    assert_eq!(later.dates.len(), 4);
}

#[test]
fn all_time_counts_both_ends_and_codex_begins_at_its_oldest_log() {
    assert_eq!(days_between("2026-10-30", "2026-11-03", 60), 5);
    assert_eq!(days_between("2026-11-03", "2026-11-03", 60), 1);
    assert_eq!(days_between("1990-01-01", "2026-11-03", 60), MAX_DAYS);
    let dir = tempfile::TempDir::new().unwrap();
    for d in ["sessions/2026/10/14", "sessions/2026/10/02", "sessions/2026/11/01", "archived_sessions"] {
        std::fs::create_dir_all(dir.path().join(d)).unwrap();
    }
    std::fs::write(dir.path().join("archived_sessions/rollout-2026-09-20T10-00-00-abc.jsonl"), "").unwrap();
    assert_eq!(codex_first(dir.path()).as_deref(), Some("2026-09-20"));
    std::fs::remove_file(dir.path().join("archived_sessions/rollout-2026-09-20T10-00-00-abc.jsonl")).unwrap();
    assert_eq!(codex_first(dir.path()).as_deref(), Some("2026-10-02"));
}

#[test]
fn the_lane_names_the_page_and_other_hubs_key_on_stay_the_same() {
    // `WhereItGoes.svelte` and other machines' hubds read these keys off the wire.
    let tally = Tally::new(vec!["2026-11-03".into()], 60);
    assert_eq!(tally.lanes.keys().map(String::as_str).collect::<Vec<_>>(), vec!["claude_code", "codex", "hub"]);
    let lanes = |keys: &[&str]| keys.iter().map(|k| lane_of(k, 1.0, vec![1.0], vec![])).collect::<Vec<_>>();
    let keys = |l: &[SpendLane]| l.iter().map(|l| l.key.clone()).collect::<Vec<_>>();
    let theirs = Spend {
        days: 1,
        dates: vec!["2026-11-03".into()],
        lanes: lanes(&["hub", "claude_code", "codex", "claude_cloud", "claude_elsewhere"]),
        // The plan-meter estimate is per day only, so it never comes by the hour.
        hourly: Some(SpendHourly { start: 0, lanes: lanes(&["hub", "claude_code", "codex", "claude_cloud"]), sources: vec![] }),
        ..Spend::default()
    };
    let kept = without_paid(theirs);
    assert_eq!(keys(&kept.lanes), vec!["hub", "claude_code", "codex"]);
    assert_eq!(keys(&kept.hourly.expect("hourly").lanes), vec!["hub", "claude_code", "codex"]);
}
