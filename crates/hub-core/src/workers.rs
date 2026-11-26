//! Everything Hub has running in the background, in one place: each thread's engine and the
//! processes it started (found by the `HUB_THREAD_ID` they inherit, see `runtime.rs`), Hub's own
//! helpers, and containers. Processes can be stopped, paused and resumed as a tree.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, bail};
#[cfg(unix)]
use sysinfo::Signal;
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind};

use crate::model::{ThreadId, ThreadWork, WorkerProcess, Workers};
use crate::process_marks::THREAD_VAR;

// Kept between calls: CPU use is the difference between two refreshes.
static SYSTEM: LazyLock<Mutex<System>> = LazyLock::new(|| Mutex::new(System::new()));

/// Processes paused from Hub. Windows does not report a suspended process as such.
static PAUSED: LazyLock<Mutex<HashSet<u32>>> = LazyLock::new(Mutex::default);

struct Proc {
    parent: Option<u32>,
    thread: Option<ThreadId>,
    start: u64,
}

/// One look at the machine: every process with its parent and the thread it carries.
struct Scan {
    procs: HashMap<u32, Proc>,
    children: HashMap<u32, Vec<u32>>,
}

impl Scan {
    fn take(sys: &System) -> Self {
        let mut procs = HashMap::new();
        for (pid, p) in sys.processes() {
            // On Linux the threads of a process are listed as processes too, and a process that
            // has exited lingers until its parent collects it.
            if p.thread_kind().is_some() || matches!(p.status(), ProcessStatus::Zombie | ProcessStatus::Dead) {
                continue;
            }
            procs.insert(pid.as_u32(), Proc { parent: p.parent().map(|p| p.as_u32()), thread: thread_of(p.environ()), start: p.start_time() });
        }
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (pid, p) in &procs {
            // A parent that started after its child is a reused pid, not the parent.
            if let Some(parent) = p.parent
                && procs.get(&parent).is_some_and(|q| q.start <= p.start)
            {
                children.entry(parent).or_default().push(*pid);
            }
        }
        for list in children.values_mut() {
            list.sort_by_key(|pid| (procs[pid].start, *pid));
        }
        Scan { procs, children }
    }

    /// `pid` and everything under it, parents first, with their depth below `pid`.
    fn tree(&self, pid: u32, keep: &dyn Fn(u32) -> bool) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let mut stack = vec![(pid, 0)];
        while let Some((pid, depth)) = stack.pop() {
            out.push((pid, depth));
            for child in self.children.get(&pid).into_iter().flatten().rev() {
                if keep(*child) {
                    stack.push((*child, depth + 1));
                }
            }
        }
        out
    }

    fn ancestors(&self, pid: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut at = pid;
        while let Some(parent) = self.procs.get(&at).and_then(|p| p.parent) {
            if out.contains(&parent) || out.len() > 64 {
                break;
            }
            out.push(parent);
            at = parent;
        }
        out
    }
}

pub(crate) fn thread_of(environ: &[std::ffi::OsString]) -> Option<ThreadId> {
    environ.iter().find_map(|var| {
        let var = var.to_string_lossy();
        var.strip_prefix(THREAD_VAR).and_then(|r| r.strip_prefix('=')).map(str::to_owned)
    })
}

fn refresh(sys: &mut System) {
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_cwd(UpdateKind::OnlyIfNotSet)
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_environ(UpdateKind::OnlyIfNotSet),
    );
}

/// What runs on this machine on behalf of Hub. `engines` are the threads whose engine Hub
/// considers running.
pub fn snapshot(engines: &[ThreadId]) -> Workers {
    let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    refresh(&mut sys);
    let scan = Scan::take(&sys);
    let hub = std::process::id();
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32;
    let ports = listening();

    let mut paused = PAUSED.lock().unwrap_or_else(|e| e.into_inner());
    paused.retain(|pid| scan.procs.contains_key(pid));

    let describe = |pid: u32, depth: u32| -> Option<WorkerProcess> {
        let p = sys.process(Pid::from_u32(pid))?;
        let command = p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
        let mut list = ports.get(&pid).cloned().unwrap_or_default();
        list.sort_unstable();
        list.dedup();
        Some(WorkerProcess {
            pid,
            name: p.name().to_string_lossy().into_owned(),
            command: crate::text::clip(&command, 200),
            ports: list,
            cpu_percent: p.cpu_usage() / cores,
            mem: p.memory(),
            started_at: p.start_time() as i64 * 1000,
            depth,
            paused: paused.contains(&pid) || p.status() == ProcessStatus::Stop,
            cwd: p.cwd().map(|c| c.display().to_string()),
        })
    };

    // Each thread's processes as trees: roots are those whose parent is not the thread's.
    let mut by_thread: BTreeMap<ThreadId, Vec<u32>> = BTreeMap::new();
    for (pid, p) in &scan.procs {
        if *pid == hub {
            continue;
        }
        if let Some(t) = &p.thread {
            by_thread.entry(t.clone()).or_default().push(*pid);
        }
    }
    for id in engines {
        by_thread.entry(id.clone()).or_default();
    }
    let mut threads = Vec::new();
    for (thread_id, pids) in by_thread {
        let mine = |pid: u32| scan.procs.get(&pid).is_some_and(|p| p.thread.as_deref() == Some(thread_id.as_str()));
        let engine = pids.iter().copied().find(|pid| scan.procs[pid].parent == Some(hub));
        let mut roots: Vec<u32> =
            pids.iter().copied().filter(|pid| !scan.procs[pid].parent.is_some_and(&mine)).collect();
        roots.sort_by_key(|pid| (Some(*pid) != engine, scan.procs[pid].start, *pid));
        let processes: Vec<WorkerProcess> = roots
            .iter()
            .flat_map(|root| scan.tree(*root, &mine))
            .filter_map(|(pid, depth)| describe(pid, depth))
            .collect();
        threads.push(ThreadWork {
            engine_running: engines.contains(&thread_id),
            engine,
            cpu_percent: processes.iter().map(|p| p.cpu_percent).sum(),
            mem: processes.iter().map(|p| p.mem).sum(),
            thread_id,
            processes,
        });
    }

    // Hub and the helpers it runs without a thread: tidy, handoff, usage, voice, update builds.
    let own: Vec<WorkerProcess> = scan
        .tree(hub, &|pid| scan.procs.get(&pid).is_some_and(|p| p.thread.is_none()))
        .into_iter()
        .filter_map(|(pid, depth)| describe(pid, depth))
        .collect();

    Workers {
        host: System::host_name().unwrap_or_else(|| "this computer".into()),
        threads,
        hub: own,
        containers: Vec::new(),
    }
}

/// The processes each thread's engine is made of (the command Hub started and what it started
/// to get going), as pid and start time, noted once its session is up and before any prompt, so
/// nothing a tool started is among them. A port one of them listens on is the engine's own.
static ENGINES: LazyLock<Mutex<HashMap<ThreadId, HashSet<(u32, u64)>>>> = LazyLock::new(Mutex::default);

/// Notes what `thread_id`'s engine is made of right now. Called when its session has started.
pub fn note_engine(thread_id: &str) {
    let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    refresh(&mut sys);
    let scan = Scan::take(&sys);
    let hub = std::process::id();
    let mine = |pid: u32| scan.procs.get(&pid).is_some_and(|p| p.thread.as_deref() == Some(thread_id));
    let root = scan.procs.iter().find(|(pid, p)| p.parent == Some(hub) && mine(**pid)).map(|(pid, _)| *pid);
    let parts: HashSet<(u32, u64)> =
        root.map(|root| scan.tree(root, &mine).into_iter().map(|(pid, _)| (pid, scan.procs[&pid].start)).collect()).unwrap_or_default();
    ENGINES.lock().unwrap_or_else(|e| e.into_inner()).insert(thread_id.to_owned(), parts);
}

/// Whether the process `pid`, started at `start`, is part of a thread's engine.
pub fn is_engine(pid: u32, start: u64) -> bool {
    ENGINES.lock().unwrap_or_else(|e| e.into_inner()).values().any(|parts| parts.contains(&(pid, start)))
}

/// Every pid that carries `thread_id`, children before parents so nothing restarts them.
pub fn thread_processes(thread_id: &str) -> Vec<u32> {
    let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    refresh(&mut sys);
    let scan = Scan::take(&sys);
    let hub = std::process::id();
    let mut pids: Vec<u32> = scan
        .procs
        .iter()
        .filter(|(pid, p)| **pid != hub && p.thread.as_deref() == Some(thread_id))
        .map(|(pid, _)| *pid)
        .collect();
    pids.sort_by_key(|pid| std::cmp::Reverse(scan.procs[pid].start));
    pids
}

/// What a thread's engine has started and not finished: the first processes under it (a shell, a
/// build, a `find` that never ends), found through their parents, so one that left the engine's
/// process group is found too. Not the engine itself, and not what was handed over to the system
/// (a dev server the agent left on). The engine is the process carrying the thread's id whose
/// parent is Hub, or its keeper (`keeper`, the pid on its card).
pub fn engine_tools(thread_id: &str, keeper: Option<u32>) -> Vec<u32> {
    let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    refresh(&mut sys);
    let scan = Scan::take(&sys);
    let hub = std::process::id();
    let engines: Vec<u32> = scan
        .procs
        .iter()
        .filter(|(pid, p)| {
            **pid != hub && p.thread.as_deref() == Some(thread_id) && p.parent.is_some_and(|parent| parent == hub || Some(parent) == keeper)
        })
        .map(|(pid, _)| *pid)
        .collect();
    let mut tools: Vec<u32> =
        engines.iter().flat_map(|engine| scan.children.get(engine).into_iter().flatten().copied()).filter(|pid| *pid != hub).collect();
    tools.sort_unstable();
    tools.dedup();
    tools
}

/// Stops every process carrying `thread_id` and waits up to `limit` for them to have ended, so a
/// folder they worked in can go next: Windows does not delete a folder a process still works in,
/// and a stopped process takes a moment to end. Answers whether they all ended.
pub fn stop_thread(thread_id: &str, limit: std::time::Duration) -> bool {
    let until = std::time::Instant::now() + limit;
    for pid in thread_processes(thread_id) {
        if let Err(e) = act(pid, Action::Stop) {
            tracing::debug!(thread = %thread_id, "stop {pid}: {e:#}");
        }
    }
    loop {
        let left = thread_processes(thread_id).into_iter().filter(|pid| running(*pid)).count();
        if left == 0 {
            return true;
        }
        if std::time::Instant::now() >= until {
            tracing::warn!(thread = %thread_id, "{left} of its processes still running {limit:?} after being stopped");
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Whether `pid` has not ended yet. The scan already leaves out a process that ended and waits
/// for its parent; Windows lists one until the last handle to it closes, so it is asked.
#[cfg(unix)]
fn running(_pid: u32) -> bool {
    true
}

#[cfg(windows)]
fn running(pid: u32) -> bool {
    win::running(pid)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Stop,
    Pause,
    Resume,
}

/// Stops, pauses or resumes `pid` and everything it started. Hub itself, the processes Hub runs
/// under, its engines (for pausing) and the operating system are refused.
pub fn act(pid: u32, action: Action) -> Result<()> {
    let hub = std::process::id();
    if pid == hub {
        bail!("Hub does not stop or pause itself");
    }
    let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    refresh(&mut sys);
    let scan = Scan::take(&sys);
    let Some(target) = scan.procs.get(&pid) else { bail!("no process {pid}") };
    if scan.ancestors(hub).contains(&pid) {
        bail!("Hub runs under this process, so it is not stopped from Hub");
    }
    if action == Action::Pause && target.parent == Some(hub) {
        bail!("this is a thread's engine: stop its turn instead of pausing it");
    }
    let mut pids: Vec<u32> = scan.tree(pid, &|p| p != hub).into_iter().map(|(p, _)| p).collect();
    // Children first, so a parent cannot start a replacement for a child that was just stopped.
    pids.reverse();
    for p in pids {
        let Some(process) = sys.process(Pid::from_u32(p)) else { continue };
        let name = process.name().to_string_lossy();
        let exe = process.exe().map(|e| e.to_string_lossy().into_owned());
        if crate::proc::is_system(Some(p), Some(&name), exe.as_deref()) {
            if p == pid {
                bail!("{name} is part of the operating system and is not touched from Hub");
            }
            continue;
        }
        let done = match action {
            Action::Stop => process.kill(),
            Action::Pause => suspend(process, true),
            Action::Resume => suspend(process, false),
        };
        if done {
            let mut paused = PAUSED.lock().unwrap_or_else(|e| e.into_inner());
            match action {
                Action::Pause => paused.insert(p),
                Action::Resume | Action::Stop => paused.remove(&p),
            };
        } else if p == pid {
            bail!("could not {} process {pid}", format!("{action:?}").to_lowercase());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn suspend(process: &sysinfo::Process, pause: bool) -> bool {
    process.kill_with(if pause { Signal::Stop } else { Signal::Continue }).unwrap_or(false)
}

#[cfg(windows)]
fn suspend(process: &sysinfo::Process, pause: bool) -> bool {
    win::suspend(process.pid().as_u32(), pause)
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    const PROCESS_SUSPEND_RESUME: u32 = 0x0800;

    #[link(name = "kernel32", kind = "raw-dylib")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn WaitForSingleObject(handle: *mut c_void, ms: u32) -> u32;
    }

    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_TIMEOUT: u32 = 0x102;

    /// Whether the process is still running: not signalled yet. One that cannot be opened is gone.
    pub fn running(pid: u32) -> bool {
        // SAFETY: the handle is checked before use and closed once.
        unsafe {
            let handle = OpenProcess(SYNCHRONIZE, 0, pid);
            if handle.is_null() {
                return false;
            }
            let state = WaitForSingleObject(handle, 0);
            CloseHandle(handle);
            state == WAIT_TIMEOUT
        }
    }

    // Not documented, but the call Process Explorer and Resource Monitor use for this.
    #[link(name = "ntdll", kind = "raw-dylib")]
    unsafe extern "system" {
        fn NtSuspendProcess(handle: *mut c_void) -> i32;
        fn NtResumeProcess(handle: *mut c_void) -> i32;
    }

    pub fn suspend(pid: u32, pause: bool) -> bool {
        // SAFETY: the handle is checked before use and closed once.
        unsafe {
            let handle = OpenProcess(PROCESS_SUSPEND_RESUME, 0, pid);
            if handle.is_null() {
                return false;
            }
            let status = if pause { NtSuspendProcess(handle) } else { NtResumeProcess(handle) };
            CloseHandle(handle);
            status >= 0
        }
    }
}

/// Listening TCP ports by process.
pub(crate) fn listening() -> HashMap<u32, Vec<u16>> {
    use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
    let mut out: HashMap<u32, Vec<u16>> = HashMap::new();
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let Ok(all) = netstat2::get_sockets_info(af, ProtocolFlags::TCP) else { return out };
    for s in all {
        if let ProtocolSocketInfo::Tcp(tcp) = s.protocol_socket_info
            && tcp.state == TcpState::Listen
            && let Some(pid) = s.associated_pids.first()
        {
            out.entry(*pid).or_default().push(tcp.local_port);
        }
    }
    out
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A child started with a thread's id shows up under that thread, can be paused, resumed and
    /// stopped together with what it started.
    #[test]
    fn a_threads_processes_are_found_paused_and_stopped() {
        let thread = format!("test-{}", std::process::id());
        let mut child = crate::proc::command("sh")
            .args(["-c", "sleep 30 & wait"])
            .env(THREAD_VAR, &thread)
            .spawn()
            .unwrap();
        let pid = child.id();
        std::thread::sleep(std::time::Duration::from_millis(300));

        let workers = snapshot(&[]);
        let work = workers.threads.iter().find(|t| t.thread_id == thread).expect("thread listed");
        // Started by this process with a thread's id, the way Hub starts engines.
        assert_eq!(work.engine, Some(pid));
        assert_eq!(work.processes[0].pid, pid);
        assert_eq!(work.processes[0].depth, 0);
        let sleep = work.processes.iter().find(|p| p.depth == 1 && p.name.starts_with("sleep")).expect("child listed").pid;
        assert!(workers.hub.first().is_some_and(|p| p.pid == std::process::id()));

        assert!(act(pid, Action::Pause).is_err(), "engines are not paused");
        act(sleep, Action::Pause).unwrap();
        let paused = snapshot(&[]);
        let work = paused.threads.iter().find(|t| t.thread_id == thread).unwrap();
        assert!(work.processes.iter().find(|p| p.pid == sleep).unwrap().paused);
        assert!(!work.processes[0].paused);
        act(sleep, Action::Resume).unwrap();

        assert_eq!(thread_processes(&thread).len(), 2);
        act(pid, Action::Stop).unwrap();
        let _ = child.wait();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(thread_processes(&thread).is_empty());
    }

    /// Stopping one thread's process leaves a neighbour with another thread's id, and one with
    /// no id at all, running.
    #[test]
    fn act_stops_only_the_process_asked_for_and_what_it_started() {
        let tag = std::process::id();
        let spawn = |thread: Option<String>, script: &str| {
            let mut cmd = crate::proc::command("sh");
            cmd.args(["-c", script]);
            if let Some(t) = thread {
                cmd.env(THREAD_VAR, t);
            }
            cmd.spawn().unwrap()
        };
        let mut target = spawn(Some(format!("target-{tag}")), "sleep 30 & wait");
        let mut other = spawn(Some(format!("other-{tag}")), "sleep 30 & wait");
        let mut plain = spawn(None, "sleep 30");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let alive = |pid: u32| thread_processes_all().contains(&pid);
        let target_pids = thread_processes(&format!("target-{tag}"));
        assert_eq!(target_pids.len(), 2);

        act(target.id(), Action::Stop).unwrap();
        let _ = target.wait();
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(thread_processes(&format!("target-{tag}")).is_empty(), "the tree is gone");
        assert_eq!(thread_processes(&format!("other-{tag}")).len(), 2, "another thread's processes stay");
        assert!(alive(plain.id()), "a process with no thread stays");

        for c in [&mut other, &mut plain] {
            let _ = c.kill();
            let _ = c.wait();
        }
        for pid in thread_processes(&format!("other-{tag}")) {
            let _ = act(pid, Action::Stop);
        }
    }

    fn thread_processes_all() -> Vec<u32> {
        let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
        refresh(&mut sys);
        Scan::take(&sys).procs.keys().copied().collect()
    }

    #[test]
    fn a_missing_process_and_the_process_hub_runs_under_are_refused() {
        // A pid far above any real one.
        assert!(act(4_000_000_000, Action::Stop).unwrap_err().to_string().contains("no process"));
        let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
        refresh(&mut sys);
        let parent = Scan::take(&sys).procs.get(&std::process::id()).and_then(|p| p.parent);
        drop(sys);
        if let Some(parent) = parent {
            assert!(act(parent, Action::Stop).is_err(), "the parent of Hub is not stopped from Hub");
        }
    }

    #[test]
    fn the_thread_of_a_process_is_read_from_its_environment() {
        let env = |s: &str| std::ffi::OsString::from(s);
        assert_eq!(thread_of(&[env("PATH=/bin"), env(&format!("{THREAD_VAR}=abc"))]), Some("abc".into()));
        assert_eq!(thread_of(&[env("PATH=/bin")]), None);
        // A variable that only starts with the same letters is not it.
        assert_eq!(thread_of(&[env(&format!("{THREAD_VAR}_X=abc"))]), None);
    }

    /// What an engine started is found through its parent; the engine itself is not.
    #[test]
    fn engine_tools_are_the_first_processes_under_the_engine() {
        let thread = format!("tools-{}", std::process::id());
        let mut engine = crate::proc::command("sh").args(["-c", "sleep 30 & wait"]).env(THREAD_VAR, &thread).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let tools = engine_tools(&thread, None);
        assert_eq!(tools.len(), 1);
        assert_ne!(tools[0], engine.id());
        assert!(engine_tools("no-such-thread", None).is_empty());
        let _ = act(engine.id(), Action::Stop);
        let _ = engine.wait();
    }

    #[test]
    fn a_noted_engine_is_recognised_by_pid_and_start_time() {
        let thread = format!("note-{}", std::process::id());
        let mut engine = crate::proc::command("sh").args(["-c", "sleep 30"]).env(THREAD_VAR, &thread).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        note_engine(&thread);
        let start = {
            let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
            refresh(&mut sys);
            Scan::take(&sys).procs[&engine.id()].start
        };
        assert!(is_engine(engine.id(), start));
        assert!(!is_engine(engine.id(), start + 1000), "a reused pid is not the engine");
        let _ = act(engine.id(), Action::Stop);
        let _ = engine.wait();
    }

    #[test]
    fn hub_is_never_stopped_from_here() {
        assert!(act(std::process::id(), Action::Stop).is_err());
    }
}
