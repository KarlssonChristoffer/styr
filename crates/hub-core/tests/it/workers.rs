//! `workers` through its public face: what a machine snapshot shows for a thread, how a thread's
//! processes are found and what its tools are. The unit tests inside `workers.rs` cover pause,
//! stop and the engine marks; these cover the rest of the snapshot.
#![cfg(unix)]

use std::process::Child;
use std::time::{Duration, Instant};

use hub_core::process_marks::THREAD_VAR;
use hub_core::workers::{self, Action};

/// A name no other test uses: the process list is the whole machine's.
fn tag(name: &str) -> String {
    format!("w-{name}-{}", std::process::id())
}

fn tagged(thread: &str, script: &str) -> Child {
    hub_core::proc::command("sh").args(["-c", script]).env(THREAD_VAR, thread).spawn().unwrap()
}

/// Waits until `ready` holds; the processes a script starts take a moment to show.
fn until(what: &str, mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn stop(mut child: Child, thread: &str) {
    for pid in workers::thread_processes(thread) {
        let _ = workers::act(pid, Action::Stop);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn a_thread_whose_engine_hub_holds_is_listed_even_before_it_started_anything() {
    let thread = tag("empty");
    let snap = workers::snapshot(std::slice::from_ref(&thread));
    let work = snap.threads.iter().find(|t| t.thread_id == thread).expect("listed");
    assert!(work.engine_running);
    assert_eq!(work.engine, None);
    assert!(work.processes.is_empty());
    assert_eq!((work.cpu_percent, work.mem), (0.0, 0));
    // Without being held it is not there at all.
    assert!(!workers::snapshot(&[]).threads.iter().any(|t| t.thread_id == thread));
}

#[test]
fn a_process_left_behind_is_listed_as_not_held() {
    let thread = tag("left");
    let child = tagged(&thread, "sleep 30");
    until("the process to be listed", || workers::snapshot(&[]).threads.iter().any(|t| t.thread_id == thread));
    let snap = workers::snapshot(&[]);
    let work = snap.threads.iter().find(|t| t.thread_id == thread).unwrap();
    assert!(!work.engine_running, "Hub holds no engine for it");
    assert!(!work.processes.is_empty());
    stop(child, &thread);
}

#[test]
fn a_process_is_described_with_its_command_start_and_memory() {
    let thread = tag("describe");
    let child = tagged(&thread, "sleep 31 & wait");
    until("both processes", || workers::thread_processes(&thread).len() == 2);
    let snap = workers::snapshot(std::slice::from_ref(&thread));
    let work = snap.threads.iter().find(|t| t.thread_id == thread).unwrap();
    assert_eq!(work.processes.len(), 2);
    let sleep = work.processes.iter().find(|p| p.command.contains("sleep 31") && p.depth == 1).expect("the child, one level down");
    assert_eq!(sleep.name, "sleep");
    assert!(sleep.started_at > 1_600_000_000_000, "milliseconds since 1970: {}", sleep.started_at);
    assert!(sleep.mem > 0);
    assert!(!sleep.paused);
    assert!(sleep.ports.is_empty());
    // The thread's totals are the sums of its processes.
    let mem: u64 = work.processes.iter().map(|p| p.mem).sum();
    assert_eq!(work.mem, mem);
    let cpu: f32 = work.processes.iter().map(|p| p.cpu_percent).sum();
    assert!((work.cpu_percent - cpu).abs() < 0.001);
    // The shell Hub started is the engine and comes first.
    assert_eq!(work.engine, Some(child.id()));
    assert_eq!(work.processes[0].pid, child.id());
    stop(child, &thread);
}

#[test]
fn a_long_command_line_is_shortened() {
    let thread = tag("long");
    let script = format!("sleep 32 # {}", "x".repeat(600));
    let child = tagged(&thread, &script);
    until("the process", || !workers::thread_processes(&thread).is_empty());
    let snap = workers::snapshot(&[]);
    let work = snap.threads.iter().find(|t| t.thread_id == thread).unwrap();
    assert!(work.processes[0].command.chars().count() <= 201, "{}", work.processes[0].command.len());
    stop(child, &thread);
}

#[test]
fn two_threads_are_kept_apart() {
    let (a, b) = (tag("two-a"), tag("two-b"));
    let (ca, cb) = (tagged(&a, "sleep 33 & wait"), tagged(&b, "sleep 34"));
    until("both threads", || workers::thread_processes(&a).len() == 2 && !workers::thread_processes(&b).is_empty());
    let snap = workers::snapshot(&[]);
    let pids = |id: &str| snap.threads.iter().find(|t| t.thread_id == id).unwrap().processes.iter().map(|p| p.pid).collect::<Vec<_>>();
    let (pa, pb) = (pids(&a), pids(&b));
    assert_eq!(pa.len(), 2);
    assert!(!pb.is_empty());
    assert!(pa.iter().all(|p| !pb.contains(p)));
    stop(ca, &a);
    stop(cb, &b);
}

#[test]
fn a_threads_processes_are_all_found_and_no_other_threads_are() {
    let thread = tag("order");
    let child = tagged(&thread, "sleep 35 & wait");
    until("both processes", || workers::thread_processes(&thread).len() == 2);
    let pids = workers::thread_processes(&thread);
    // Start times are whole seconds, so a parent and child started together may come either way
    // round; only the pair is certain.
    assert!(pids.contains(&child.id()));
    assert!(workers::thread_processes(&tag("nobody")).is_empty());
    stop(child, &thread);
}

#[test]
fn the_tools_an_engine_started_are_found_through_hub_or_through_its_keeper() {
    let thread = tag("keeper");
    // keeper (no thread id) → engine (carries the id) → tool. Hub is not the engine's parent.
    let keeper = hub_core::proc::command("sh")
        .args(["-c", &format!("{THREAD_VAR}={thread} sh -c 'sleep 36 & wait' & wait")])
        .spawn()
        .unwrap();
    until("the engine and its tool", || workers::thread_processes(&thread).len() == 2);
    assert!(workers::engine_tools(&thread, None).is_empty(), "with no keeper named the engine is not found");
    let tools = workers::engine_tools(&thread, Some(keeper.id()));
    assert_eq!(tools.len(), 1);
    assert!(!workers::thread_processes(&thread).iter().all(|p| tools.contains(p)), "the engine itself is not a tool");
    assert!(!tools.contains(&keeper.id()));
    stop(keeper, &thread);
}

#[test]
fn hub_lists_itself_first_and_never_as_a_thread() {
    let snap = workers::snapshot(&[]);
    assert_eq!(snap.hub.first().map(|p| p.pid), Some(std::process::id()));
    assert_eq!(snap.hub[0].depth, 0);
    assert!(snap.hub[0].mem > 0);
    assert!(!snap.host.is_empty());
    assert!(snap.threads.iter().all(|t| t.processes.iter().all(|p| p.pid != std::process::id())));
}

#[test]
fn a_helper_hub_started_without_a_thread_is_listed_under_hub() {
    let child = hub_core::proc::command("sleep").arg("37").spawn().unwrap();
    let pid = child.id();
    until("the helper under Hub", || workers::snapshot(&[]).hub.iter().any(|p| p.pid == pid));
    let snap = workers::snapshot(&[]);
    let helper = snap.hub.iter().find(|p| p.pid == pid).unwrap();
    assert_eq!(helper.depth, 1);
    assert!(snap.threads.iter().all(|t| t.processes.iter().all(|p| p.pid != pid)));
    let mut child = child;
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn a_process_that_listens_shows_its_port() {
    let Ok(py) = which_python() else { return };
    let thread = tag("port");
    let child = hub_core::proc::command(py)
        .args(["-c", "import socket,time\ns=socket.socket()\ns.bind(('127.0.0.1',0))\ns.listen()\nprint(s.getsockname()[1],flush=True)\ntime.sleep(30)"])
        .env(THREAD_VAR, &thread)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut child = child;
    let port: u16 = {
        use std::io::BufRead;
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        line.trim().parse().unwrap()
    };
    let pid = child.id();
    let mut listed = Vec::new();
    until("the port to show", || {
        let snap = workers::snapshot(&[]);
        listed = snap.threads.iter().find(|t| t.thread_id == thread).and_then(|t| t.processes.iter().find(|p| p.pid == pid)).map(|p| p.ports.clone()).unwrap_or_default();
        !listed.is_empty()
    });
    assert_eq!(listed, vec![port]);
    stop(child, &thread);
}

fn which_python() -> Result<&'static str, ()> {
    ["python3", "python"].into_iter().find(|p| hub_core::proc::command(p).arg("--version").output().is_ok_and(|o| o.status.success())).ok_or(())
}

#[test]
fn a_process_that_is_gone_cannot_be_paused_or_resumed() {
    for action in [Action::Pause, Action::Resume, Action::Stop] {
        let err = workers::act(4_000_000_000, action).unwrap_err();
        assert!(err.to_string().contains("no process"), "{err}");
    }
}

#[test]
fn a_paused_process_is_marked_until_it_is_resumed() {
    let thread = tag("pause");
    let child = tagged(&thread, "sleep 38 & wait");
    until("both processes", || workers::thread_processes(&thread).len() == 2);
    let tool = workers::engine_tools(&thread, None);
    assert_eq!(tool.len(), 1);
    let pid = tool[0];
    let paused = |pid: u32| {
        let snap = workers::snapshot(&[]);
        snap.threads.iter().find(|t| t.thread_id == thread).and_then(|t| t.processes.iter().find(|p| p.pid == pid)).map(|p| p.paused)
    };
    workers::act(pid, Action::Pause).unwrap();
    until("the pause to show", || paused(pid) == Some(true));
    workers::act(pid, Action::Resume).unwrap();
    until("the resume to show", || paused(pid) == Some(false));
    stop(child, &thread);
}
