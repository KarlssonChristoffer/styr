//! Runs of your Workers as threads. A Worker is one of your own tools whose `hub.toml` has a
//! `[worker]` section (Knegarn): it does whole jobs by itself, with several agents, and Hub shows
//! each job, a run, in a thread. A run handed over from a thread (its work reference is
//! `hub:thread:<id>`) shows in that thread; a run started elsewhere gets a thread of its own in
//! the tool's folder.
//!
//! Hub knows nothing about any one Worker. Every few seconds it asks each one for its runs, and
//! for each run still going it replays the run's events after the last one it read, with its own
//! cursor, folds them into where the run stands (the steps of the chain, the round, the agents,
//! paused or not) and writes a `worker_run` event when anything moved: the state, plus only the
//! lines that are new. The inbox reads the newest of each thread's runs from memory
//! (`GET /api/worker-runs`); the controls are command lines from the file, run in the tool's
//! folder like every other command of your tools.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::locked::LockExt;
use crate::model::{
    EventKind, OwnToolRan, RunWorkerControl, ThreadId, WorkerAgent, WorkerControl, WorkerLine, WorkerRun, WorkerRunData, WorkerStep, now_ms,
};
use crate::own_tools::{self, Worker};

/// How often the Workers are asked while a run is going.
const EVERY: Duration = Duration::from_secs(15);
/// The longest wait between asks while nothing runs (or the tool fails), reached by doubling.
const IDLE_MAX: Duration = Duration::from_secs(120);
/// A run started elsewhere that ended before Hub saw it gets a thread only when it is this new.
const NEW_ENOUGH_MS: i64 = 6 * 3_600_000;
/// Lines one `worker_run` carries at most; a run that wrote more in one go keeps its newest.
const LINES: usize = 200;
/// How long listing a Worker's runs may take. Its command is Python, and a machine busy with an
/// update build can take well over the usual 30 s; a late answer is normal then, so this allows
/// twice that. Hub keeps the list it had and the next refresh reads again.
const LISTING: Duration = Duration::from_secs(60);
/// Refreshes in a row that miss `LISTING` before Hub warns about it.
const MISSES_TO_WARN: u32 = 3;

/// Where each run stands, by the tool's folder and the run's id, with the thread it shows in.
static RUNS: LazyLock<Mutex<HashMap<(String, String), (ThreadId, WorkerRun)>>> = LazyLock::new(Mutex::default);
static SEEDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Refreshes in a row that missed `LISTING`, by the tool's folder.
static MISSES: LazyLock<Mutex<HashMap<String, u32>>> = LazyLock::new(Mutex::default);

/// The newest run of each thread that shows one, for the inbox.
pub fn by_thread() -> BTreeMap<ThreadId, WorkerRun> {
    let runs = RUNS.lock().unwrap_or_else(|e| e.into_inner());
    let mut out: BTreeMap<ThreadId, WorkerRun> = BTreeMap::new();
    for (thread, run) in runs.values() {
        // A running run wins over one that ended; between equals the later id.
        let better = out.get(thread).is_none_or(|have| (run.state == "running", &run.run) > (have.state == "running", &have.run));
        if better {
            out.insert(thread.clone(), run.clone());
        }
    }
    out
}

pub async fn follower(hub: Arc<crate::Hub>) {
    tokio::time::sleep(Duration::from_secs(20)).await;
    let mut wait = EVERY;
    loop {
        if let Err(err) = tick(&hub).await {
            tracing::warn!("worker runs: {err:#}");
        }
        wait = next_wait(wait, any_running());
        tokio::time::sleep(wait).await;
    }
}

fn any_running() -> bool {
    RUNS.lock().unwrap_or_else(|e| e.into_inner()).values().any(|(_, run)| run.state == "running")
}

/// Fast while a run goes, doubling up to `IDLE_MAX` while none does.
fn next_wait(last: Duration, running: bool) -> Duration {
    if running { EVERY } else { (last * 2).min(IDLE_MAX) }
}

/// What the threads already say, after a restart: the newest `worker_run` of each.
async fn seed(hub: &crate::Hub) -> Result<()> {
    if SEEDED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    let mut runs = RUNS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for e in hub.store.latest_of_kind(EventKind::WorkerRun).await? {
        if let Some(WorkerRunData { run, .. }) = e.data_as::<WorkerRunData>() {
            runs.entry((run.folder.clone(), run.run.clone())).or_insert((e.thread_id, run));
        }
    }
    *RUNS.lock().unwrap_or_else(|e| e.into_inner()) = runs;
    Ok(())
}

pub async fn tick(hub: &Arc<crate::Hub>) -> Result<()> {
    seed(hub).await?;
    for folder in own_tools::folders(&hub.ctx()).await {
        let Ok(text) = std::fs::read_to_string(folder.join(own_tools::FILE)) else { continue };
        let Ok(manifest) = own_tools::parse(&text) else { continue };
        let Some(worker) = &manifest.worker else { continue };
        let key = folder.display().to_string();
        match follow_tool(hub, &folder, &manifest.name, &manifest.env, worker).await {
            Ok(()) => refreshed(&mut MISSES.locked(), &key),
            Err(err) if err.downcast_ref::<tokio::time::error::Elapsed>().is_some() => {
                // A miss keeps the runs Hub already shows; only a streak of them is news.
                if missed(&mut MISSES.locked(), &key) {
                    tracing::warn!("worker runs of {}: {err:#} ({MISSES_TO_WARN} refreshes in a row)", manifest.name);
                } else {
                    tracing::debug!("worker runs of {}: {err:#}", manifest.name);
                }
            }
            Err(err) => tracing::warn!("worker runs of {}: {err:#}", manifest.name),
        }
    }
    Ok(())
}

/// A refresh that got the list: the streak of misses ends.
fn refreshed(misses: &mut HashMap<String, u32>, folder: &str) {
    misses.remove(folder);
}

/// A refresh that missed `LISTING`. True on the miss that makes the streak reach `MISSES_TO_WARN`,
/// so a streak warns once.
fn missed(misses: &mut HashMap<String, u32>, folder: &str) -> bool {
    let n = misses.entry(folder.to_owned()).or_insert(0);
    *n += 1;
    *n == MISSES_TO_WARN
}

async fn follow_tool(hub: &Arc<crate::Hub>, folder: &Path, tool: &str, env: &BTreeMap<String, String>, worker: &Worker) -> Result<()> {
    let doc = own_tools::document_for(folder, env, &worker.runs, LISTING).await?;
    let Some(items) = doc.pointer(&worker.items).and_then(Value::as_array) else { return Ok(()) };
    let folder_s = folder.display().to_string();
    for item in items {
        let run_id = item.pointer(&worker.id).map(own_tools::plain).unwrap_or_default();
        if run_id.is_empty() {
            continue;
        }
        let ended = item.pointer(&worker.ended).map(own_tools::plain).filter(|s| !s.is_empty());
        let key = (folder_s.clone(), run_id.clone());
        let known = RUNS.lock().unwrap_or_else(|e| e.into_inner()).get(&key).cloned();
        if known.as_ref().is_some_and(|(_, run)| run.state != "running") {
            continue; // already told in its thread how it ended
        }
        let thread = match &known {
            Some((thread, _)) => thread.clone(),
            None => match thread_for(hub, &folder_s, worker, item, &run_id, ended.is_some()).await? {
                Some(thread) => thread,
                None => continue,
            },
        };
        let before = known.map(|(_, run)| run);
        let cursor = before.as_ref().map_or(0, |r| r.cursor);
        let inputs = HashMap::from([("run".to_owned(), run_id.clone()), ("seq".to_owned(), cursor.to_string())]);
        let events = match read_events(folder, env, &worker.events, &inputs).await {
            Ok(doc) => doc.pointer(&worker.event_items).and_then(Value::as_array).cloned().unwrap_or_default(),
            Err(err) => {
                tracing::warn!("events of {tool} run {run_id}: {err:#}");
                Vec::new()
            }
        };
        let base = before.clone().unwrap_or_else(|| fresh(tool, &folder_s, &run_id, worker));
        let (mut run, lines) = fold(base, worker, &events);
        run.title = item.pointer(&worker.title).map(own_tools::plain).filter(|s| !s.is_empty()).unwrap_or(run.title);
        run.link = pick(item, worker.link.as_deref()).or(run.link);
        run.cost = worker.cost.as_deref().and_then(|p| item.pointer(p)).and_then(Value::as_f64).or(run.cost);
        if let Some(ended) = &ended {
            end(&mut run, ended, pick(item, worker.state.as_deref()), pick(item, worker.said.as_deref()));
        }
        run.controls = controls(worker, &run);
        if before.as_ref() == Some(&run) && lines.is_empty() {
            continue;
        }
        let data = WorkerRunData { run, lines };
        hub.emit_typed(&thread, &data).await?;
        let run = data.run;
        let ended_now = run.state != "running";
        RUNS.lock().unwrap_or_else(|e| e.into_inner()).insert(key, (thread.clone(), run));
        if ended_now {
            // It handed the ball back: the thread comes up in the inbox, done or not.
            let now = now_ms();
            hub.update_thread(&thread, move |t| {
                t.returned_at = Some(now);
                t.settled_at = None;
                t.snoozed_until = None;
            })
            .await?;
        }
    }
    Ok(())
}

/// The thread a run shows in: the one that handed the work over, else a thread of its own in the
/// tool's folder, made once (by a client id, so a restart finds the same one). None for a run
/// from elsewhere that ended too long ago to be news.
async fn thread_for(hub: &Arc<crate::Hub>, folder: &str, worker: &Worker, item: &Value, run_id: &str, ended: bool) -> Result<Option<ThreadId>> {
    if let Some(work) = pick(item, worker.work.as_deref())
        && let Some(id) = work.strip_prefix("hub:thread:")
        && hub.store.thread(id).await?.is_some()
    {
        return Ok(Some(id.to_owned()));
    }
    if ended {
        let started = pick(item, worker.started.as_deref()).and_then(|s| crate::customs_review::iso_ms(&s));
        if started.is_none_or(|at| now_ms() - at > NEW_ENOUGH_MS) {
            return Ok(None);
        }
    }
    let title = pick(item, Some(&worker.title)).unwrap_or_else(|| format!("{} {run_id}", worker.label));
    let new: crate::model::NewThread = serde_json::from_value(serde_json::json!({
        "cwd": folder,
        "title": title,
        "client_id": format!("worker:{folder}:{run_id}"),
    }))?;
    Ok(Some(hub.create_thread(new).await?.id))
}

async fn read_events(folder: &Path, env: &BTreeMap<String, String>, run: &[String], inputs: &HashMap<String, String>) -> Result<Value> {
    let out = own_tools::command(folder, env, run, inputs)?.output();
    let out = tokio::time::timeout(own_tools::TIMEOUT, out).await.context("took too long")??;
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim()).with_context(|| format!("did not print JSON ({})", out.status))
}

fn pick(doc: &Value, pointer: Option<&str>) -> Option<String> {
    pointer.and_then(|p| own_tools::render(p, doc)).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// A run Hub has not seen before: every step still to come.
pub fn fresh(tool: &str, folder: &str, run_id: &str, worker: &Worker) -> WorkerRun {
    WorkerRun {
        tool: tool.to_owned(),
        folder: folder.to_owned(),
        run: run_id.to_owned(),
        title: format!("{} {run_id}", worker.label),
        state: "running".into(),
        paused: false,
        said: None,
        steps: worker.steps.iter().map(|s| WorkerStep { id: s.id.clone(), label: s.label.clone(), state: "todo".into(), visits: 0 }).collect(),
        round: None,
        agents: Vec::new(),
        link: None,
        cost: None,
        controls: Vec::new(),
        cursor: 0,
    }
}

/// Where the run stands after `events`, and the lines they make.
pub fn fold(mut run: WorkerRun, worker: &Worker, events: &[Value]) -> (WorkerRun, Vec<WorkerLine>) {
    let mut lines = Vec::new();
    for e in events {
        let Some(seq) = e.pointer(&worker.seq).and_then(Value::as_i64) else { continue };
        if seq <= run.cursor {
            continue;
        }
        run.cursor = seq;
        let phase = e.pointer(&worker.phase).map(own_tools::plain).unwrap_or_default();
        let step = worker.steps.iter().find(|s| s.phases.iter().any(|p| p == &phase)).map(|s| s.id.clone());
        if let Some(step) = &step {
            enter(&mut run, step);
        }
        let round = e.pointer(&worker.round).and_then(Value::as_i64).filter(|r| *r > 0);
        if round.is_some() {
            run.round = round;
        }
        let text = e.pointer(&worker.text).map(own_tools::plain).unwrap_or_default();
        let agent = pick(e, worker.agent.as_deref());
        if let Some(role) = &agent {
            let model = pick(e, worker.model.as_deref());
            match run.agents.iter_mut().find(|a| &a.role == role) {
                Some(a) => {
                    a.calls += 1;
                    a.model = model.or(a.model.take());
                    a.last = Some(text.clone()).filter(|t| !t.is_empty());
                }
                None => run.agents.push(WorkerAgent { role: role.clone(), calls: 1, model, last: Some(text.clone()).filter(|t| !t.is_empty()) }),
            }
        }
        match pick(e, worker.pause.as_deref()).as_deref() {
            Some("pause") => run.paused = true,
            Some("resume") => run.paused = false,
            _ => {}
        }
        lines.push(WorkerLine {
            seq,
            kind: e.pointer(&worker.kind).map(own_tools::plain).unwrap_or_default(),
            step,
            round,
            agent,
            text: text.chars().take(1_000).collect(),
        });
    }
    if lines.len() > LINES {
        lines.drain(..lines.len() - LINES);
    }
    (run, lines)
}

/// The run moved into `step`: the one it was in is done. A step it skipped (no repair was needed)
/// stays to do, so the chain shows only what really happened.
fn enter(run: &mut WorkerRun, step: &str) {
    let Some(at) = run.steps.iter().position(|s| s.id == step) else { return };
    if run.steps[at].state == "now" {
        return;
    }
    for (i, s) in run.steps.iter_mut().enumerate() {
        if i == at {
            s.state = "now".into();
            s.visits += 1;
        } else if s.state == "now" {
            s.state = "done".into();
        }
    }
}

/// Words that mean a run finished what it set out to do.
const DONE: &[&str] = &["completed", "complete", "done", "success", "succeeded", "delivered", "deliverable"];

fn end(run: &mut WorkerRun, ended: &str, state: Option<String>, said: Option<String>) {
    let done = DONE.contains(&ended.to_lowercase().as_str());
    run.state = if done { "done" } else { "stopped" }.into();
    run.paused = false;
    if done {
        for s in &mut run.steps {
            if s.state == "now" {
                s.state = "done".into();
            }
        }
    }
    run.said = said.or(state).or_else(|| Some(ended.to_owned())).filter(|s| !s.is_empty());
}

/// What the user may do to the run now.
fn controls(worker: &Worker, run: &WorkerRun) -> Vec<WorkerControl> {
    if run.state != "running" {
        return Vec::new();
    }
    let now = if run.paused { "paused" } else { "running" };
    worker
        .controls
        .iter()
        .filter(|c| c.when.as_deref().is_none_or(|w| w == now))
        .map(|c| WorkerControl {
            id: c.id.clone(),
            label: c.label.clone(),
            about: c.about.clone(),
            ask: c.ask.clone(),
            danger: c.danger,
            authorizes: c.run.iter().any(|a| a.contains("{authorization}")),
        })
        .collect()
}

/// Runs one of a run's controls, for the thread it shows in, and reads the run again at once.
pub async fn control(hub: &Arc<crate::Hub>, ask: &RunWorkerControl) -> Result<OwnToolRan> {
    let found = {
        let runs = RUNS.lock().unwrap_or_else(|e| e.into_inner());
        let mut mine: Vec<&WorkerRun> = runs.values().filter(|(t, _)| t == &ask.thread_id).map(|(_, r)| r).collect();
        mine.sort_by_key(|r| (r.state == "running", r.run.clone()));
        mine.last().map(|r| (r.folder.clone(), r.run.clone(), r.controls.clone()))
    };
    let Some((folder, run_id, offered)) = found else { bail!("this thread shows no run") };
    if !offered.iter().any(|c| c.id == ask.control) {
        bail!("{} cannot be done to the run now", ask.control);
    }
    let (dir, manifest) = own_tools::found(&hub.ctx(), &folder).await?;
    let worker = manifest.worker.as_ref().context("no [worker] in its hub.toml any more")?;
    let def = worker.controls.iter().find(|c| c.id == ask.control).context("no such control")?;
    let mut inputs: HashMap<String, String> = HashMap::new();
    for name in &def.ask {
        let value = ask.inputs.get(name).map(|v| v.trim()).unwrap_or_default();
        if value.is_empty() {
            bail!("{name} is needed");
        }
        inputs.insert(name.clone(), value.to_owned());
    }
    inputs.insert("run".into(), run_id);
    inputs.insert("authorization".into(), format!("hub:approved:{}:thread:{}", now_ms(), ask.thread_id));
    let out = own_tools::command(&dir, &manifest.env, &def.run, &inputs)?.output();
    let out = tokio::time::timeout(own_tools::TIMEOUT, out).await.context("took too long")??;
    let mut said = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if !out.status.success() {
        said = format!("{said}\n{}", String::from_utf8_lossy(&out.stderr).trim()).trim().to_owned();
    }
    if let Err(err) = tick(hub).await {
        tracing::warn!("worker runs after {}: {err:#}", ask.control);
    }
    Ok(OwnToolRan { ok: out.status.success(), output: said.chars().take(4_000).collect(), asks: Vec::new() })
}

/// Forgets every run, for tests.
#[doc(hidden)]
pub fn forget_all() {
    RUNS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    SEEDED.store(false, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_ask_slows_down_while_nothing_runs_and_speeds_up_with_a_run() {
        let mut wait = EVERY;
        let mut seen = Vec::new();
        for _ in 0..5 {
            wait = next_wait(wait, false);
            seen.push(wait.as_secs());
        }
        assert_eq!(seen, [30, 60, 120, 120, 120]);
        assert_eq!(next_wait(wait, true), EVERY);
    }

    #[test]
    fn one_miss_is_silent_a_streak_warns_once_and_a_success_resets() {
        let mut misses = HashMap::new();
        // Misses 1 to 5 make a streak that warns at 3; the success at 6 clears it; 7 to 9 warn at 9.
        let outcomes = [false, true, true, true, true, true, false, true, true, true];
        let mut warned = Vec::new();
        for (i, miss) in outcomes.into_iter().enumerate() {
            if miss {
                if missed(&mut misses, "/k") {
                    warned.push(i);
                }
            } else {
                refreshed(&mut misses, "/k");
            }
        }
        assert_eq!(warned, [3, 9]);
        assert_eq!(misses.get("/k"), Some(&3));
        refreshed(&mut misses, "/k");
        assert!(misses.is_empty());
    }

    fn knegarn() -> Worker {
        own_tools::parse(include_str!("../../../docs/examples/knegarn-hub.toml")).unwrap().worker.unwrap()
    }

    fn states(run: &WorkerRun) -> Vec<(&str, &str, u32)> {
        run.steps.iter().map(|s| (s.id.as_str(), s.state.as_str(), s.visits)).collect()
    }

    #[test]
    fn a_repair_round_goes_back_and_the_chain_keeps_what_was_done() {
        let w = knegarn();
        let ev = |seq: i64, phase: &str, round: i64| json!({ "seq": seq, "kind": "phase_entered", "phase": phase, "round": round, "detail": "" });
        let events = [ev(1, "implementing", 1), ev(2, "verifying", 1), ev(3, "reviewing", 1), ev(4, "repairing", 2), ev(5, "verifying", 2)];
        let (run, lines) = fold(fresh("Knegarn", "/k", "7", &w), &w, &events);
        assert_eq!(lines.len(), 5);
        assert_eq!(run.round, Some(2));
        assert_eq!(
            states(&run),
            [("build", "done", 1), ("test", "now", 2), ("review", "done", 1), ("repair", "done", 1), ("final", "todo", 0), ("deliver", "todo", 0)]
        );
        // Read again from the cursor: nothing is counted twice.
        let (again, more) = fold(run.clone(), &w, &events);
        assert!(more.is_empty());
        assert_eq!(again, run);
    }

    #[test]
    fn agents_pauses_and_the_end() {
        let w = knegarn();
        let events = [
            json!({ "seq": 1, "kind": "agent_call", "phase": "implementing", "round": 1, "detail": "implementer attempt 1/2: ok", "data": { "role": "implementer", "model": "sonnet" } }),
            json!({ "seq": 2, "kind": "agent_call", "phase": "reviewing", "round": 1, "detail": "reviewer attempt 1/2: ok", "data": { "role": "reviewer", "model": "opus" } }),
            json!({ "seq": 3, "kind": "control", "phase": "reviewing", "round": 1, "detail": "paused", "data": { "action": "pause" } }),
        ];
        let (mut run, _) = fold(fresh("Knegarn", "/k", "7", &w), &w, &events);
        assert!(run.paused);
        assert_eq!(run.agents.iter().map(|a| (a.role.as_str(), a.model.as_deref())).collect::<Vec<_>>(), [("implementer", Some("sonnet")), ("reviewer", Some("opus"))]);
        assert_eq!(controls(&w, &run).iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["resume"]);
        run.paused = false;
        let offered = controls(&w, &run);
        assert_eq!(offered.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["pause", "more", "stop"]);
        assert!(offered.iter().find(|c| c.id == "more").unwrap().authorizes);
        assert!(offered.iter().find(|c| c.id == "stop").unwrap().danger);
        end(&mut run, "escalated", Some("needs_human".into()), Some("stopped_by_person".into()));
        assert_eq!((run.state.as_str(), run.said.as_deref()), ("stopped", Some("stopped_by_person")));
        assert!(controls(&w, &run).is_empty());
        let mut done = fresh("Knegarn", "/k", "8", &w);
        enter(&mut done, "deliver");
        end(&mut done, "completed", None, None);
        assert_eq!(done.state, "done");
        // Only what it went through is done; the steps it skipped stay to do.
        assert_eq!(done.steps.iter().filter(|s| s.state == "done").map(|s| s.id.as_str()).collect::<Vec<_>>(), ["deliver"]);
        assert!(done.steps.iter().all(|s| s.state != "now"));
    }
}
