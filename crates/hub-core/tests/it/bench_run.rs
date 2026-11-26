//! The bench runner (`crates/hub-core/src/bench/run.rs`) against the scripted mock agent only: no
//! real engine starts and nothing is spent. A suite built from the mock's keywords (`write`,
//! `plan`, `homepage`, `permission`, `quota`, `slow`) checks pass and fail per check, Hub's own
//! proof round, a permission answered without waiting, a plan limit, a timeout, the guards, a
//! restart in the middle of a run, what happens to threads and folders, and the files a run leaves.

use std::sync::Arc;
use std::time::Duration;

use hub_core::model::{BenchAsk, BenchRow, BenchRun, NewThread, OwnerKind, PlanLimit, PlanUsage, ThreadStatus, now_ms};
use hub_core::{Config, Hub, Store};
use crate::support::{mock_agent, wait_status};

fn hub() -> Arc<Hub> {
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.engines.clear();
    config.engines.insert(
        "mock".into(),
        hub_core::config::EngineConfig {
            command: mock_agent().display().to_string(),
            traits: hub_core::config::EngineTraits::preset("claude"),
            ..Default::default()
        },
    );
    config.default_engine = "mock".into();
    Hub::new(Store::in_memory().unwrap(), config)
}

/// One ordinary thread first, so Hub knows which models the mock offers (as a real engine
/// must have run once before the bench can pick its model).
pub(crate) async fn warmed() -> Arc<Hub> {
    let hub = hub();
    let new: NewThread = serde_json::from_value(serde_json::json!({ "cwd": hub_core::janitor::test_dir().display().to_string(), "message": "hello" })).unwrap();
    let thread = hub.create_thread(new).await.unwrap();
    crate::support::turn_ended(&hub, &thread.id).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    hub
}

pub(crate) fn suite(timeout_s: u64, cases: &[(&str, &str, &[&str])]) -> String {
    let mut text = format!("suite = \"mock\"\nengine = \"mock\"\nmodel = \"haiku\"\nrepeats = 1\ntimeout_s = {timeout_s}\ncost_cap_usd = 5.0\n");
    for (id, prompt, checks) in cases {
        text.push_str(&format!("\n[[case]]\nid = \"{id}\"\nprompts = [\"{prompt}\"]\n[case.files]\n\"README.md\" = \"# {id}\\n\"\n"));
        for check in *checks {
            text.push_str(&format!("[[case.check]]\n{check}\n"));
        }
    }
    text
}

pub(crate) fn ask() -> BenchAsk {
    BenchAsk { suite: "mock".into(), ..Default::default() }
}

/// Waits until run `id` has ended and returns it.
pub(crate) async fn ended(hub: &Arc<Hub>, id: &str) -> BenchRun {
    for _ in 0..1_200 {
        if let Some(run) = hub_core::bench::results::load(&hub.ctx(), id)
            && run.state != "running"
        {
            return run;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("bench run {id} did not end");
}

/// Removes the folders a run's failed repeats kept (only those: tests running beside this one may
/// have runs whose ids share the millisecond, and so the parent folder).
pub(crate) fn tidy(run: &BenchRun) {
    for folder in run.rows.iter().filter_map(|r| r.folder.as_deref()) {
        hub_core::locked::clean_dir(folder);
    }
    std::fs::remove_dir(std::env::temp_dir().join(format!("hub-bench-{}", run.id))).ok();
}

fn row<'a>(run: &'a BenchRun, case: &str) -> &'a BenchRow {
    run.rows.iter().find(|r| r.case == case && r.state != "interrupted").unwrap_or_else(|| panic!("no row for {case}: {run:#?}"))
}

fn check<'a>(row: &'a BenchRow, starts: &str) -> &'a hub_core::model::BenchCheck {
    row.checks.iter().find(|c| c.check.starts_with(starts)).unwrap_or_else(|| panic!("no check {starts} in {row:#?}"))
}

#[tokio::test]
async fn a_mock_suite_runs_checks_and_measures_each_case() {
    let hub = warmed().await;
    let text = suite(60, &[
        ("write", "write it", &["ended = \"end_turn\"", "file = \"mock.txt\"\nmatches = 'write it'"]),
        ("plan", "plan it", &["ended = \"end_turn\"", "steps = 3"]),
        ("homepage", "make a homepage please", &["ended = \"end_turn\"", "hub_tool = \"page_check\"", "proof = \"shown\""]),
        ("permission", "permission please", &["settled = true", "refused_max = 0", "ended = \"end_turn\""]),
        ("quota", "quota now", &["no_error = true", "ended = \"end_turn\""]),
    ]);
    let started = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap();
    assert_eq!((started.state.as_str(), started.model_asked.as_str()), ("running", "haiku"));
    let run = ended(&hub, &started.id).await;
    // The `quota` case comes last and ends the run on the spent plan (below).
    assert_eq!(run.rows.len(), 5, "{run:#?}");

    let write = row(&run, "write");
    assert!(write.pass, "{write:#?}");
    let plan = row(&run, "plan");
    assert!(!plan.pass && check(plan, "ended").ok && !check(plan, "steps").ok, "{plan:#?}");
    assert!(check(plan, "steps").why.as_deref().is_some_and(|w| w.contains("3 steps, 1 done")), "{plan:#?}");

    // Hub asked the homepage for proof once, and the page_check it got showed it.
    let homepage = row(&run, "homepage");
    assert!(homepage.pass, "{homepage:#?}");
    assert_eq!(homepage.measured["proof_rounds"], 1, "{homepage:#?}");
    assert!(homepage.measured["turns"].as_u64().is_some_and(|t| t >= 2), "{homepage:#?}");

    // The permission question was answered by Hub at once (the bench's own answer), so nothing waited.
    let permission = row(&run, "permission");
    assert!(permission.pass, "{permission:#?}");
    assert_eq!(permission.measured["perm_allowed"], 1, "{permission:#?}");

    // A plan limit hit in the middle of a turn is not Hub failing the case: the repeat is invalid
    // (not counted, not failed) and the run stops there, saying so.
    let quota = row(&run, "quota");
    assert_eq!(quota.invalid.as_deref(), Some(hub_core::bench::measure::PLAN_LIMIT), "{quota:#?}");
    assert!(!quota.pass, "{quota:#?}");
    assert_eq!(run.state, "plan_limit", "{run:#?}");
    assert!(run.why.as_deref().is_some_and(|w| w.contains("quota 1/1")), "{run:#?}");

    for r in &run.rows {
        // Every repeat's thread is the bench's, a system thread, and archived once checked.
        let thread = hub.store.thread(r.thread.as_deref().unwrap()).await.unwrap().unwrap();
        assert!(thread.archived, "{} not archived", r.case);
        assert_eq!(thread.owner.as_ref().map(|o| o.kind), Some(OwnerKind::Bench));
        assert!(thread.owner.as_ref().is_some_and(|o| o.system()));
        assert!(thread.title.starts_with(&format!("Bench · mock/{} · 1/1", r.case)), "{}", thread.title);
        // A passed repeat's folder is gone; a failed one's is kept for a look.
        let folder = std::path::Path::new(r.folder.as_deref().unwrap());
        assert_eq!(folder.join("README.md").exists(), !r.pass, "{r:#?}");
    }

    // The run's files: its JSON and its report.
    let report = hub_core::bench::results::load_report(&hub.ctx(), &run.id).unwrap();
    assert!(report.contains("| write | 1/1 | new |") && report.contains("| plan | 0/1 | new |"), "{report}");
    assert!(report.contains("## Failed repeats") && report.contains(plan.thread.as_deref().unwrap()), "{report}");
    assert!(report.contains(hub_core::bench::results::HOW_TO_READ));
    tidy(&run);
}

#[tokio::test]
async fn a_second_run_is_compared_with_the_first() {
    let hub = warmed().await;
    let text = suite(60, &[("write", "write it", &["ended = \"end_turn\""])]);
    let first = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap();
    let first = ended(&hub, &first.id).await;
    // Run ids are milliseconds; two runs never share one.
    tokio::time::sleep(Duration::from_millis(5)).await;
    let second = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap();
    let second = ended(&hub, &second.id).await;
    let report = hub_core::bench::results::load_report(&hub.ctx(), &second.id).unwrap();
    assert!(report.contains(&format!("Compared with run `{}`", first.id)) && report.contains("| write | 1/1 | 1/1 |"), "{report}");
    assert_eq!(hub_core::bench::results::list(&hub.ctx()).len(), 2);
    tidy(&first);
    tidy(&second);
}

#[tokio::test]
async fn a_repeat_that_runs_out_of_time_is_timed_out_and_stop_ends_the_run() {
    let hub = warmed().await;
    let mut text = suite(1, &[("slow", "slow slow slow slow slow slow slow slow slow slow", &["ended = \"end_turn\""])]);
    text = text.replace("repeats = 1", "repeats = 3");
    let run = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap();
    // Stopped once its first repeat is under way, it ends after that repeat.
    for _ in 0..600 {
        if hub_core::bench::run::progress(&hub).is_some_and(|(_, at)| at.contains("repeat 1/3")) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // One run at a time.
    let again = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap_err();
    assert!(format!("{again:#}").contains("going already"), "{again:#}");
    assert!(hub_core::bench::run::stop(&hub, &run.id).await);
    let run = ended(&hub, &run.id).await;
    assert_eq!(run.state, "stopped", "{run:#?}");
    assert_eq!(run.rows.len(), 1, "it stops after the repeat it is in: {run:#?}");
    assert_eq!(run.rows[0].state, "timed_out");
    assert!(!run.rows[0].pass);
    assert!(!hub_core::bench::run::stop(&hub, &run.id).await, "a run that ended cannot be stopped");
    tidy(&run);
}

#[tokio::test]
async fn a_plan_nearly_spent_refuses_a_run() {
    // Not warmed: a thread's turn would fetch the mock's own plan figures over the seeded ones.
    let hub = hub();
    hub.usage.seed(PlanUsage { plan: None, limits: vec![PlanLimit { label: "Weekly".into(), percent: 65.0, resets: None, resets_at: None }], checked_at: now_ms() });
    let text = suite(60, &[("write", "write it", &["ended = \"end_turn\""])]);
    let refused = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap_err();
    let why = format!("{refused:#}");
    assert!(why.contains("Weekly") && why.contains("65 %") && why.contains("under 60 %"), "{why}");
    // A case that asks for an engine mode is refused too: the mode could stop the engine asking.
    let moded = text.replace("prompts = [", "mode = \"auto\"\nprompts = [");
    let refused = hub_core::bench::run::start_text(&hub, &moded, &ask()).await.unwrap_err();
    assert!(format!("{refused:#}").contains("engine mode"), "{refused:#}");
}

/// The place a run keeps in kv, as `run.rs` writes it.
fn place(run: &str, text: &str, version: &str) -> String {
    serde_json::json!({ "run": run, "suite_text": text, "suite_hash": hub_core::bench::suite_hash(text), "case": "write", "repeat": 1, "version": version }).to_string()
}

#[tokio::test]
async fn a_run_cut_short_by_a_restart_carries_on_with_its_repeat() {
    let hub = warmed().await;
    let text = suite(60, &[("write", "write it", &["ended = \"end_turn\""])]);
    let suite = hub_core::bench::parse(&text).unwrap();
    let version = hub.update_status().version;
    let run = BenchRun {
        id: "1-mock".into(),
        suite: "mock".into(),
        suite_hash: suite.hash.clone(),
        engine: "mock".into(),
        model_asked: "haiku".into(),
        hub_version: version.clone(),
        started: 1,
        state: "running".into(),
        repeats: 1,
        cases: vec!["write".into()],
        ..Default::default()
    };
    hub_core::bench::results::save(&hub.ctx(), &run).unwrap();
    hub.store.set_value("bench.current", &place("1-mock", &text, &version)).await.unwrap();
    hub_core::bench::run::resume(hub.clone()).await;
    let resumed = ended(&hub, "1-mock").await;
    assert_eq!(resumed.state, "done", "{resumed:#?}");
    let states: Vec<&str> = resumed.rows.iter().map(|r| r.state.as_str()).collect();
    assert_eq!(states, ["interrupted", "done"]);
    assert!(resumed.rows[1].pass);
    assert_eq!(hub.store.get_value("bench.current").await.unwrap(), None);
    tidy(&resumed);

    // After an update the run ends, keeping what it finished.
    let mut older = run.clone();
    older.id = "2-mock".into();
    older.hub_version = "0.0.0-old".into();
    hub_core::bench::results::save(&hub.ctx(), &older).unwrap();
    hub.store.set_value("bench.current", &place("2-mock", &text, "0.0.0-old")).await.unwrap();
    hub_core::bench::run::resume(hub.clone()).await;
    let updated = ended(&hub, "2-mock").await;
    assert_eq!(updated.state, "hub_updated", "{updated:#?}");
    assert!(updated.why.as_deref().is_some_and(|w| w.contains("Hub updated mid-run")));
    assert_eq!(updated.rows.iter().map(|r| r.state.as_str()).collect::<Vec<_>>(), ["interrupted"]);
    // A run still marked as going holds a new one back until it is dealt with.
    hub.store.set_value("bench.current", &place("3-mock", &text, &version)).await.unwrap();
    let held = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap_err();
    assert!(format!("{held:#}").contains("still marked as going"), "{held:#}");
    // Stop ends a run that is only marked, and frees the bench.
    assert!(hub_core::bench::run::stop(&hub, "3-mock").await);
    assert_eq!(hub.store.get_value("bench.current").await.unwrap(), None);
}

/// The features suite by its name, on the mock: the attachment reaches the engine with the first
/// prompt, and the mock, which only echoes, does not say what the taxi cost, so the case fails on
/// its answer and on nothing else.
#[tokio::test]
async fn the_features_suite_runs_by_name_and_a_mock_fails_the_attached_file_case() {
    let hub = warmed().await;
    let ask = BenchAsk { suite: "features".into(), engine: Some("mock".into()), model: Some("haiku".into()), repeats: Some(1), cases: Some(vec!["files-in-chat".into()]) };
    let started = hub_core::bench::run::start(&hub, &ask).await.unwrap();
    assert_eq!(started.suite, "features");
    let run = ended(&hub, &started.id).await;
    let case = row(&run, "files-in-chat");
    assert!(!case.pass, "{case:#?}");
    let failed: Vec<&str> = case.checks.iter().filter(|c| !c.ok).map(|c| c.check.as_str()).collect();
    assert_eq!(failed.len(), 1, "{case:#?}");
    assert!(failed[0].starts_with("reply"), "{case:#?}");
    // The receipt went with the message and was never laid down in the folder.
    let folder = std::path::Path::new(case.folder.as_deref().unwrap());
    assert!(folder.join("README.md").exists() && !folder.join("receipt.txt").exists());
    let thread = case.thread.as_deref().unwrap();
    let asked = hub.store.last_event(thread, hub_core::model::EventKind::UserMessage).await.unwrap().unwrap();
    assert_eq!(asked.data["attachments"][0]["name"], "receipt.txt", "{:#}", asked.data);
    assert!(asked.data["attachments"][0]["text"].as_str().is_some_and(|t| t.contains("412.50")));
    tidy(&run);
}

/// A case with an attachment passes when the reply shows the engine got it (the mock echoes it).
#[tokio::test]
async fn an_attached_file_reaches_the_engine_and_the_case_passes() {
    let hub = warmed().await;
    let mut text = suite(60, &[("attached", "read the attachment", &["ended = \"end_turn\"", "reply = 'file:///receipt\\.txt'"])]);
    text = text.replace("[case.files]", "[case.attach]\n\"receipt.txt\" = \"Taxi 412.50\\n\"\n[case.files]");
    let started = hub_core::bench::run::start_text(&hub, &text, &ask()).await.unwrap();
    let run = ended(&hub, &started.id).await;
    let case = row(&run, "attached");
    assert!(case.pass, "{case:#?}");
    tidy(&run);
}

/// The handoff case needs one of the person's tools that takes handoffs. Without one its repeats
/// are invalid, never sent (no thread, no tokens) and said so in the report; with one (a fixture
/// `hub.toml` under the test Hub's own projects root) the case runs.
#[tokio::test]
async fn the_handoff_case_is_invalid_without_a_tool_that_takes_handoffs() {
    let ask = BenchAsk { suite: "features".into(), engine: Some("mock".into()), model: Some("haiku".into()), repeats: Some(1), cases: Some(vec!["handoff".into()]) };
    let takes = |dir: &std::path::Path| {
        std::fs::read_to_string(dir.join("hub.toml")).ok().and_then(|t| hub_core::own_tools::parse(&t).ok()).is_some_and(|m| m.handoff.is_some())
    };
    // The home folder is looked in too: a machine with such a tool right in it cannot show the half without.
    let home_has_one = hub_core::own_tools::candidates(&[], &[hub_core::folders::home()]).iter().any(|d| takes(d));

    if !home_has_one {
        let hub = warmed().await;
        let empty = hub_core::janitor::test_dir();
        hub.store.set_value("projects.root", &empty.display().to_string()).await.unwrap();
        let started = hub_core::bench::run::start(&hub, &ask).await.unwrap();
        let run = ended(&hub, &started.id).await;
        let case = row(&run, "handoff");
        assert_eq!(case.invalid.as_deref(), Some(hub_core::bench::run::NO_HANDOFF_TOOL), "{case:#?}");
        assert!(!case.pass && case.thread.is_none() && case.folder.is_none(), "nothing was started: {case:#?}");
        assert!(case.measured["tokens"].is_null() && case.measured["cost_usd"].is_null(), "{case:#?}");
        let report = hub_core::bench::results::load_report(&hub.ctx(), &run.id).unwrap();
        assert!(report.contains(hub_core::bench::run::NO_HANDOFF_TOOL) && !report.contains("## Failed"), "{report}");
        tidy(&run);
        hub_core::locked::clean_dir(empty);
    }

    let hub = warmed().await;
    let root = hub_core::janitor::test_dir();
    let tool = root.join("helper");
    std::fs::create_dir_all(&tool).unwrap();
    std::fs::write(
        tool.join("hub.toml"),
        "name = \"Bench helper\"\n\n[handoff]\nlabel = \"Hand to Bench helper\"\nguide = \"Say what to build.\"\nrun = [\"take\", \"{brief_file}\"]\n",
    )
    .unwrap();
    assert!(takes(&tool), "the fixture is a tool that takes handoffs");
    // A project's folder is always one Hub may run (`own_tools::folders`), whatever was approved before.
    hub.store.set_value("projects.root", &root.display().to_string()).await.unwrap();
    let project = hub_core::model::Project { path: tool.display().to_string(), name: "helper".into(), created_at: now_ms() };
    hub.store.insert_project(project).await.unwrap();
    let started = hub_core::bench::run::start(&hub, &ask).await.unwrap();
    let run = ended(&hub, &started.id).await;
    let case = row(&run, "handoff");
    assert!(case.invalid.is_none() && case.thread.is_some(), "the case ran: {case:#?}");
    // The mock calls no Hub tool, so it fails on the handoff itself.
    assert!(!case.pass && !check(case, "hub_tool").ok, "{case:#?}");
    tidy(&run);
    hub_core::locked::clean_dir(root);
}

/// A side job of a bench thread stays under the bench's answer (inside the case's folder, never
/// waiting on the person); a side job of any other thread asks as before.
#[tokio::test]
async fn a_bench_threads_side_job_runs_under_the_bench_guard() {
    let hub = warmed().await;
    let dir = hub_core::janitor::test_dir();
    let new = |unattended: serde_json::Value| -> NewThread {
        serde_json::from_value(serde_json::json!({ "cwd": dir.display().to_string(), "unattended": unattended })).unwrap()
    };
    let job = || hub_core::children::SideJob {
        title: "Sum".into(),
        task: "add the numbers".into(),
        kind: "other".into(),
        context: "brief".into(),
        engine: None,
        model: None,
    };
    let bench = hub.create_thread(new(serde_json::json!({ "by": hub_core::permit::BENCH }))).await.unwrap();
    let child = hub.start_side_job(&bench.id, job()).await.unwrap();
    assert_eq!(hub.store.unattended(&child.id).await.unwrap().map(|u| u.by).as_deref(), Some(hub_core::permit::BENCH));
    assert_eq!(child.cwd, bench.cwd);

    let fixer = hub.create_thread(new(serde_json::json!({ "by": "Problems" }))).await.unwrap();
    let child = hub.start_side_job(&fixer.id, job()).await.unwrap();
    assert_eq!(hub.store.unattended(&child.id).await.unwrap(), None);
    let plain = hub.create_thread(new(serde_json::Value::Null)).await.unwrap();
    let child = hub.start_side_job(&plain.id, job()).await.unwrap();
    assert_eq!(hub.store.unattended(&child.id).await.unwrap(), None);
}

#[test]
fn the_bench_tools_are_for_full_access_only() {
    use hub_core::model::AgentAccess;
    let names = |access| hub_core::hub_tools::tools(access).into_iter().filter_map(|t| t["name"].as_str().map(str::to_owned)).collect::<Vec<_>>();
    assert!(names(AgentAccess::Full).iter().any(|n| n == "bench_run") && names(AgentAccess::Full).iter().any(|n| n == "bench_report"));
    assert!(!names(AgentAccess::Supervised).iter().any(|n| n.starts_with("bench_")));
    assert!(!names(AgentAccess::Read).iter().any(|n| n.starts_with("bench_")));
}
