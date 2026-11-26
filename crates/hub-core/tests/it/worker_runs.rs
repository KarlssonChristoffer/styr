//! A Worker's runs as threads: a fake Worker (a shell script printing runs and an event journal)
//! is followed the way `worker_runs::follower` follows Knegarn.
#![cfg(unix)]

use std::sync::Arc;

use hub_core::model::{EventKind, NewThread, Project, RunWorkerControl};
use hub_core::{Config, Hub, Store};

const SCRIPT: &str = r#"#!/bin/sh
# runs: run 7 was handed over from the thread in work.txt; run 8 came from elsewhere.
if [ "$1" = runs ]; then
  ended=$(cat ended.txt 2>/dev/null)
  printf '{"runs":[{"run_id":7,"title":"Add retries","work_ref":"%s","terminal_status":%s,"escalation":{"reason":null},"delivery":{"pr_url":"https://example.test/pr/1"}},{"run_id":8,"title":"From Forman","work_ref":"forman:1","terminal_status":null}]}\n' "$(cat work.txt)" "${ended:-null}"
elif [ "$1" = events ]; then
  if [ "$2" = 7 ] && [ "$3" -lt 3 ]; then
    echo '{"events":[{"seq":1,"kind":"phase_entered","phase":"implementing","round":1,"detail":""},{"seq":2,"kind":"agent_call","phase":"implementing","round":1,"detail":"implementer attempt 1/2: ok","data":{"role":"implementer","model":"sonnet"}},{"seq":3,"kind":"phase_entered","phase":"reviewing","round":1,"detail":""}]}'
  elif [ "$2" = 7 ] && [ "$3" -lt 4 ] && [ -f paused.txt ]; then
    echo '{"events":[{"seq":4,"kind":"control","phase":"reviewing","round":1,"detail":"paused","data":{"action":"pause"}}]}'
  else
    echo '{"events":[]}'
  fi
elif [ "$1" = pause ]; then
  touch paused.txt
  echo '{"paused":true}'
fi
"#;

const MANIFEST: &str = r#"name = "Fake Knegarn"
[worker]
label = "Fake run"
runs = ["sh", "tool.sh", "runs"]
id = "/run_id"
title = "/title"
work = "/work_ref"
ended = "/terminal_status"
said = "/escalation/reason"
link = "/delivery/pr_url"
events = ["sh", "tool.sh", "events", "{run}", "{seq}"]
agent = "/data/role"
model = "/data/model"
pause = "/data/action"
[[worker.step]]
id = "build"
label = "Build"
phases = ["implementing"]
[[worker.step]]
id = "review"
label = "Review"
phases = ["reviewing"]
[[worker.control]]
id = "pause"
label = "Pause"
run = ["sh", "tool.sh", "pause", "{run}"]
when = "running"
[[worker.control]]
id = "resume"
label = "Resume"
run = ["sh", "tool.sh", "resume", "{run}"]
when = "paused"
"#;

fn new_thread(cwd: &str, title: &str) -> NewThread {
    serde_json::from_value(serde_json::json!({ "cwd": cwd, "title": title })).unwrap()
}

#[tokio::test]
async fn runs_show_in_their_threads_and_obey_controls() {
    hub_core::worker_runs::forget_all();
    let tool = tempfile::tempdir().unwrap();
    std::fs::write(tool.path().join("tool.sh"), SCRIPT).unwrap();
    std::fs::write(tool.path().join(hub_core::own_tools::FILE), MANIFEST).unwrap();
    let mut config = Config::default();
    config.data_dir = tempfile::tempdir().unwrap().keep();
    let hub = Hub::new(Store::in_memory().unwrap(), config);
    let folder = tool.path().display().to_string();
    hub.store.insert_project(Project { path: folder.clone(), name: "Fake".into(), created_at: 0 }).await.unwrap();
    let origin = hub.create_thread(new_thread(&std::env::temp_dir().display().to_string(), "origin")).await.unwrap();
    std::fs::write(tool.path().join("work.txt"), format!("hub:thread:{}", origin.id)).unwrap();

    hub_core::worker_runs::tick(&hub).await.unwrap();

    // Run 7 shows in the thread that handed it over, in the review step after one build.
    let runs = hub_core::worker_runs::by_thread();
    let run = runs.get(&origin.id).expect("run 7 in its thread");
    assert_eq!((run.run.as_str(), run.state.as_str(), run.cursor), ("7", "running", 3));
    let steps: Vec<(&str, &str)> = run.steps.iter().map(|s| (s.id.as_str(), s.state.as_str())).collect();
    assert_eq!(steps, [("build", "done"), ("review", "now")]);
    assert_eq!(run.agents[0].role, "implementer");
    assert_eq!(run.agents[0].model.as_deref(), Some("sonnet"));
    assert_eq!(run.controls.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["pause"]);
    let events = hub.store.events(&origin.id, 0, 1000).await.unwrap();
    let written: Vec<_> = events.iter().filter(|e| e.kind == EventKind::WorkerRun).collect();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].data["lines"].as_array().unwrap().len(), 3);

    // Run 8 came from elsewhere: a thread of its own in the tool's folder, made once.
    let own: Vec<_> = runs.iter().filter(|(_, r)| r.run == "8").collect();
    assert_eq!(own.len(), 1);
    let own_thread = hub.store.thread(own[0].0).await.unwrap().unwrap();
    assert_eq!((own_thread.title.as_str(), own_thread.cwd.as_str()), ("From Forman", folder.as_str()));

    // Nothing moved: nothing is written again.
    hub_core::worker_runs::tick(&hub).await.unwrap();
    let again = hub.store.events(&origin.id, 0, 1000).await.unwrap();
    assert_eq!(again.iter().filter(|e| e.kind == EventKind::WorkerRun).count(), 1);
    assert_eq!(hub_core::worker_runs::by_thread().values().filter(|r| r.run == "8").count(), 1);

    // Pause: the tool's command runs, the run reads paused and offers Resume instead.
    let ran = hub_core::worker_runs::control(
        &hub,
        &RunWorkerControl { thread_id: origin.id.clone(), control: "pause".into(), inputs: Default::default() },
    )
    .await
    .unwrap();
    assert!(ran.ok, "{}", ran.output);
    let run = hub_core::worker_runs::by_thread()[&origin.id].clone();
    assert!(run.paused);
    assert_eq!(run.controls.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["resume"]);
    // A control the run does not offer now is refused.
    let refused = hub_core::worker_runs::control(
        &hub,
        &RunWorkerControl { thread_id: origin.id.clone(), control: "pause".into(), inputs: Default::default() },
    )
    .await;
    assert!(refused.is_err());

    // It ends: done, no controls, and the thread comes back to the inbox.
    std::fs::write(tool.path().join("ended.txt"), "\"completed\"").unwrap();
    hub_core::worker_runs::tick(&hub).await.unwrap();
    let run = hub_core::worker_runs::by_thread()[&origin.id].clone();
    assert_eq!(run.state, "done");
    assert!(run.controls.is_empty() && !run.paused);
    assert!(run.steps.iter().all(|s| s.state != "now"));
    assert_eq!(run.link.as_deref(), Some("https://example.test/pr/1"));
    assert!(hub.store.thread(&origin.id).await.unwrap().unwrap().returned_at.is_some());

    // After a restart the threads say where every run stood; an ended run is not read again.
    hub_core::worker_runs::forget_all();
    hub_core::worker_runs::tick(&hub).await.unwrap();
    let after = hub.store.events(&origin.id, 0, 1000).await.unwrap();
    assert_eq!(after.iter().filter(|e| e.kind == EventKind::WorkerRun).count(), 3);
    let _: Arc<Hub> = hub;
}
