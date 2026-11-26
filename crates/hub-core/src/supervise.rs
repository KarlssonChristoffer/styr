//! Keeps each engine's whole process tree together, so stopping a thread stops everything it
//! started. Engines are usually behind launchers (`npx` → `node`), and killing only the launcher
//! leaves the agent running.
//!
//! On Unix the ACP crate already starts the engine as the leader of its own process group and
//! kills the group. Windows has no groups, so `hubd engine <command>` sits in between: it puts
//! itself in a Job Object that kills every process in it when the last handle closes, then runs
//! the engine with the same stdio. When the core stops the engine it kills this wrapper, the
//! handle closes, and Windows ends the rest of the tree.

use anyhow::{Context, Result};

/// The `hubd` subcommand that runs an engine inside a job.
pub const ENGINE_ARG: &str = "engine";

/// The engine's command, wrapped in `hubd engine` when running as `hubd` on Windows.
pub(crate) fn contained(config: agent_client_protocol::AcpAgentConfig) -> agent_client_protocol::AcpAgentConfig {
    if !cfg!(windows) {
        return config;
    }
    // Only `hubd` has the subcommand; tests and other hosts run the engine directly.
    let Some(exe) = std::env::current_exe().ok().filter(|p| p.file_stem().is_some_and(|s| s == "hubd")) else {
        return config;
    };
    agent_client_protocol::AcpAgentConfig::new(exe)
        .arg(ENGINE_ARG)
        .arg(config.command().display().to_string())
        .args(config.arguments().iter().cloned())
        .envs(config.environment().clone())
}

/// Keeps Windows from showing error boxes for this process and everything it starts. A child
/// that fails to start (a `git.exe` that hits 0xc0000142 when memory runs short) otherwise waits
/// on an "Application Error" box nobody sees coming, and whatever in the core waits on it hangs.
/// Children inherit the mode, so this is called first thing in `hubd`.
pub fn quiet_errors() {
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn SetErrorMode(mode: u32) -> u32;
        }
        // SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX
        unsafe { SetErrorMode(0x0001 | 0x0002 | 0x8000) };
    }
}

/// Ties a helper `child` (a server Hub keeps running beside it) to this process, so Windows ends
/// it when this process ends, also by a crash or an update. Dropping the child kills it only when
/// the core shuts down cleanly; orphaned whisper-servers held 8 GB on the PC before this.
pub fn tie(child: &tokio::process::Child) {
    #[cfg(windows)]
    {
        static JOB: std::sync::OnceLock<Option<win32job::Job>> = std::sync::OnceLock::new();
        let job = JOB.get_or_init(|| {
            let mut limits = win32job::ExtendedLimitInfo::new();
            limits.limit_kill_on_job_close();
            win32job::Job::create_with_limit_info(&limits).ok()
        });
        if let (Some(job), Some(handle)) = (job, child.raw_handle()) {
            crate::locked::ResultExt::warn_on_err(job.assign_process(handle as isize), "put the child in the job");
        }
    }
    #[cfg(not(windows))]
    let _unused_off_windows = child;
}

/// Ends leftover copies of `exe`, from a core that ended without stopping them.
pub fn end_leftovers(exe: &std::path::Path) {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
    for process in sys.processes().values() {
        if process.exe().is_some_and(|p| p == exe) {
            process.kill();
        }
    }
}

/// Whether `cmd` is the command line of one of Hub's own SSH tunnels (`remote::open_tunnel`).
/// Nothing else starts `ssh -N -L 127.0.0.1…` with these options, so a person's own tunnels stay.
pub fn is_hub_tunnel(cmd: &[String]) -> bool {
    let has = |s: &str| cmd.iter().any(|a| a == s);
    let program = cmd.first().and_then(|c| std::path::Path::new(c).file_stem()).is_some_and(|s| s == "ssh");
    program
        && has("-N")
        && has("-L")
        && has("ExitOnForwardFailure=yes")
        && has("BatchMode=yes")
        && has("ServerAliveCountMax=3")
        && cmd.iter().any(|a| a.contains(":127.0.0.1:"))
}

/// Ends Hub's SSH tunnels that no core owns any more. `kill_on_drop` does not run when the core
/// exits (an update, a crash), so the `ssh` stays, holding a login open on the other machine; a
/// few dozen of those and its sshd starts refusing new connections. Only tunnels whose parent is
/// gone are ended, so a core handing over to a new one keeps the tunnels it still uses.
pub fn end_stale_tunnels() -> usize {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    let mut ended = 0;
    for process in sys.processes().values() {
        let orphan = process.parent().is_none_or(|p| p.as_u32() <= 1 || sys.process(p).is_none());
        let cmd: Vec<String> = process.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect();
        if orphan && is_hub_tunnel(&cmd) && process.kill() {
            ended += 1;
        }
    }
    ended
}

/// Runs `command` in a job that dies with this process and returns its exit code.
pub fn run_contained(command: &[String]) -> Result<i32> {
    let (program, args) = command.split_first().context("no engine command")?;
    #[cfg(windows)]
    {
        let mut limits = win32job::ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let job = win32job::Job::create_with_limit_info(&limits).context("creating a job object")?;
        job.assign_current_process().context("joining the job object")?;
        // Kept open until this process exits; then Windows ends every process in the job. Closed
        // any earlier, it would end this process too, before the engine's exit code is passed on
        // (and in a test, the whole test run with success).
        std::mem::forget(job);
    }
    let mut cmd = crate::proc::command(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let status = cmd.status().with_context(|| format!("starting {program}"))?;
    Ok(status.code().unwrap_or(1))
}

/// First wait before a loop that panicked starts again; doubles up to [`RESTART_MAX`].
const RESTART_FIRST: std::time::Duration = std::time::Duration::from_secs(1);
const RESTART_MAX: std::time::Duration = std::time::Duration::from_secs(300);
/// A run that lasted this long was healthy, so the next panic starts the backoff over.
const RESTART_HEALTHY: std::time::Duration = std::time::Duration::from_secs(60);

/// Runs a background loop of the core (`make` builds a fresh run each time) and starts it again
/// after a panic, which a bare `tokio::spawn` would swallow, ending the loop for good. The panic
/// is logged as an error (so it reaches the log and Problems), then the restart waits 1 s, 2 s,
/// 4 s … up to 5 min; a run that lasted a minute resets the wait. A loop that returns normally
/// has ended on purpose and is not started again.
pub fn forever<F, Fut>(name: &'static str, make: F) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(forever_with(name, make, RESTART_FIRST, RESTART_MAX, RESTART_HEALTHY))
}

async fn forever_with<F, Fut>(
    name: &'static str,
    make: F,
    first: std::time::Duration,
    max: std::time::Duration,
    healthy: std::time::Duration,
) where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let mut wait = first;
    loop {
        let started = std::time::Instant::now();
        match tokio::spawn(make()).await {
            Ok(()) => return,
            // Aborted with the runtime shutting down: nothing to restart.
            Err(e) if !e.is_panic() => return,
            Err(e) => {
                let panic = e.into_panic();
                let what = panic
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "a panic without a message".into());
                if started.elapsed() >= healthy {
                    wait = first;
                }
                tracing::error!("background loop {name} panicked: {what}; starting it again in {wait:?}");
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(max);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn a_loop_that_panics_is_started_again() {
        use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
        let runs = Arc::new(AtomicU32::new(0));
        let seen = runs.clone();
        let ms = std::time::Duration::from_millis;
        super::forever_with(
            "test",
            move || {
                let seen = seen.clone();
                async move {
                    // Panics twice, then ends normally.
                    if seen.fetch_add(1, Ordering::SeqCst) < 2 {
                        panic!("boom");
                    }
                }
            },
            ms(1),
            ms(4),
            ms(1000),
        )
        .await;
        assert_eq!(runs.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn a_loop_that_returns_is_not_started_again() {
        use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
        let runs = Arc::new(AtomicU32::new(0));
        let seen = runs.clone();
        let ms = std::time::Duration::from_millis;
        super::forever_with("test", move || { let seen = seen.clone(); async move { seen.fetch_add(1, Ordering::SeqCst); } }, ms(1), ms(4), ms(1000)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[cfg(unix)]
    #[test]
    fn leftover_copies_of_a_helper_are_ended() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("whisper-server");
        std::fs::copy("/bin/sleep", &exe).unwrap();
        // A test running beside this one may fork while the copy is still open for writing, and
        // the exec then fails with "Text file busy" for a moment; try again until it lets go.
        let spawn = || std::process::Command::new(&exe).arg("60").spawn();
        let mut left = (0..50)
            .find_map(|_| match spawn() {
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    None
                }
                other => Some(other),
            })
            .expect("the copy stayed busy")
            .unwrap();
        let mut other = std::process::Command::new("sleep").arg("60").spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        super::end_leftovers(&exe);
        let start = std::time::Instant::now();
        while left.try_wait().unwrap().is_none() {
            assert!(start.elapsed().as_secs() < 5, "the leftover still runs");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(other.try_wait().unwrap().is_none(), "other programs are left alone");
        let _ = other.kill();
    }

    #[test]
    fn only_hubs_own_tunnels_are_recognised() {
        let c = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
        let hub = "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o StrictHostKeyChecking=accept-new mac -N -o ExitOnForwardFailure=yes -L 5123:127.0.0.1:8787";
        assert!(super::is_hub_tunnel(&c(hub)));
        assert!(super::is_hub_tunnel(&c(&hub.replacen("ssh", "/usr/bin/ssh", 1))));
        assert!(!super::is_hub_tunnel(&c("ssh -N -L 5123:127.0.0.1:8787 mac")), "a person's own tunnel stays");
        assert!(!super::is_hub_tunnel(&c("ssh mac")));
        assert!(!super::is_hub_tunnel(&c(&hub.replacen("ssh", "sleep", 1))));
    }

    #[test]
    fn the_engine_exit_code_comes_through() {
        let command = if cfg!(windows) { ["cmd", "/C", "exit 4"] } else { ["sh", "-c", "exit 4"] };
        let command: Vec<String> = command.iter().map(|s| s.to_string()).collect();
        assert_eq!(super::run_contained(&command).unwrap(), 4);
    }
}
