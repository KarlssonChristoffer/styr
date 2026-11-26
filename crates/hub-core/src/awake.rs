//! Keeps the computer awake while a thread works, after T3 Code's `RunningThreadKeepAlive`. A long
//! turn or a night of queued work should not stop because the PC went to sleep. Only sleep is
//! held off: the screen still turns off and locks as usual. Let go once nothing runs.

use std::sync::Arc;
use std::time::Duration;

use crate::hub::Hub;
use crate::model::{Thread, ThreadStatus};

/// Looks every 20 seconds whether a thread here is running, and holds the machine awake while one is.
pub async fn hold(hub: Arc<Hub>) {
    if !hub.config.keep_awake {
        return;
    }
    let mut lock = Lock::default();
    loop {
        let running = match hub.store.threads().await {
            Ok(threads) => any_running_here(&threads),
            Err(_) => lock.held(),
        };
        lock.set(running);
        tokio::time::sleep(Duration::from_secs(20)).await;
    }
}

/// Whether a thread that runs on this machine (not one that runs in the cloud) is working.
fn any_running_here(threads: &[Thread]) -> bool {
    threads.iter().any(|t| t.cloud.is_none() && t.status == ThreadStatus::Running)
}

#[cfg(windows)]
mod os {
    use std::sync::mpsc::{Sender, channel};

    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
    }

    /// Windows keeps the request for as long as the thread that made it says so, so one thread of
    /// its own makes every request.
    pub struct Lock {
        tx: Sender<bool>,
        held: bool,
    }

    impl Default for Lock {
        fn default() -> Self {
            let (tx, rx) = channel::<bool>();
            std::thread::Builder::new()
                .name("hub-awake".into())
                .spawn(move || {
                    for on in rx {
                        let flags = if on { ES_CONTINUOUS | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
                        // SAFETY: a plain Win32 call with flag bits; it touches no memory of ours.
                        unsafe { SetThreadExecutionState(flags) };
                    }
                })
                .ok();
            Self { tx, held: false }
        }
    }

    impl Lock {
        pub fn held(&self) -> bool {
            self.held
        }
        pub fn set(&mut self, on: bool) {
            if on != self.held && self.tx.send(on).is_ok() {
                self.held = on;
            }
        }
    }
}

#[cfg(not(windows))]
mod os {
    use tokio::process::Child;

    /// Elsewhere a helper holds it while it runs: `caffeinate` on macOS, `systemd-inhibit` on Linux.
    #[derive(Default)]
    pub struct Lock {
        child: Option<Child>,
    }

    impl Lock {
        pub fn held(&self) -> bool {
            self.child.is_some()
        }
        pub fn set(&mut self, on: bool) {
            if on == self.held() {
                return;
            }
            if !on {
                self.child = None;
                return;
            }
            let mut cmd = if cfg!(target_os = "macos") {
                let mut c = crate::proc::tokio_command("caffeinate");
                c.arg("-i");
                c
            } else {
                let mut c = crate::proc::tokio_command("systemd-inhibit");
                c.args(["--what=sleep", "--who=Hub", "--why=A thread is working", "sleep", "infinity"]);
                c
            };
            cmd.kill_on_drop(true).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            // Without the helper the machine sleeps as it would anyway.
            self.child = cmd.spawn().ok();
        }
    }
}

use os::Lock;

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(status: ThreadStatus, cloud: bool) -> Thread {
        Thread {
            id: "t".into(), title: "t".into(), cwd: "/tmp".into(), engine: "claude".into(), status,
            engine_session_id: None, title_locked: false, archived: false, returned_at: None, seen_at: None,
            settled_at: None, sent_at: None, pinned_at: None, snoozed_until: None, auto_settle: true,
            parent: None, reading: None, owner: None, cloud: cloud.then(crate::model::CloudRun::default),
            host: None, project: None, display: None, created_at: 1, updated_at: 1,
        }
    }

    #[test]
    fn only_a_running_thread_here_holds_the_machine_awake() {
        assert!(!any_running_here(&[]));
        assert!(!any_running_here(&[thread(ThreadStatus::Idle, false)]));
        assert!(any_running_here(&[thread(ThreadStatus::Idle, false), thread(ThreadStatus::Running, false)]));
        assert!(!any_running_here(&[thread(ThreadStatus::Running, true)]), "a cloud thread does not run here");
    }

    #[cfg(not(windows))]
    #[test]
    fn the_lock_follows_what_it_is_told() {
        // No helper to start on a machine without `systemd-inhibit` or `caffeinate`; the lock then
        // reports not held, which is "the machine sleeps as it would anyway".
        let mut lock = Lock::default();
        assert!(!lock.held());
        lock.set(false);
        assert!(!lock.held());
    }
}
