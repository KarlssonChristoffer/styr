//! Two small helpers for failures that used to vanish or cascade.
//!
//! A `std` mutex is poisoned when a thread panics while holding it; `lock().unwrap()` then panics
//! in every later caller until hubd restarts. `locked()` takes the guard anyway: the data is
//! whatever the panicking thread left, which is what the rest of the code (store, dev, notice)
//! already chose to live with.
//!
//! `send_or_log` replaces `let _ = tx.send(..)` where the message matters: a closed channel is
//! logged with what was lost instead of disappearing.
//!
//! `warn_on_err` does the same for any `Result` that used to be `let _ = ..`: the failure is
//! logged with what was being done. Use it for writes and events that matter; plain best-effort
//! clean-up (`remove_file` of a temp file) uses `clean_file` / `clean_dir` (and the `_async` ones):
//! already gone is fine, any other failure is logged. A failure that is expected is
//! `.ignore("why")` (`Ignore`, re-exported from `hub_install::ignore`).

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use tokio::sync::mpsc::UnboundedSender;

pub trait LockExt<T> {
    /// `lock()` that survives a poisoned mutex.
    fn locked(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn locked(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub trait RwLockExt<T> {
    /// `read()` that survives a poisoned lock.
    fn read_locked(&self) -> RwLockReadGuard<'_, T>;
    /// `write()` that survives a poisoned lock.
    fn write_locked(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_locked(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    fn write_locked(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub trait ResultExt {
    /// Log `what` and the error if this is an `Err`; the result is dropped either way.
    fn warn_on_err(self, what: &str);
}

impl<T, E: std::fmt::Display> ResultExt for Result<T, E> {
    fn warn_on_err(self, what: &str) {
        if let Err(err) = self {
            tracing::warn!("{what} failed: {err:#}");
        }
    }
}

/// `x.ignore("why")`: a discard on purpose, with the reason at the call site. It lives in
/// `hub_install::ignore` so the crates that do not link hub-core share the one copy.
pub use hub_install::ignore::Ignore;

/// Send on an unbounded channel; if the receiver is gone, log `what` and return false.
pub fn send_or_log<T>(tx: &UnboundedSender<T>, msg: T, what: &str) -> bool {
    match tx.send(msg) {
        Ok(()) => true,
        Err(_) => {
            tracing::warn!("{what}: the channel is closed, the message was dropped");
            false
        }
    }
}

/// Best-effort clean-up of a file: already gone is fine, any other failure is logged.
pub fn clean_file(path: impl AsRef<std::path::Path>) {
    let path = path.as_ref();
    clean_result(std::fs::remove_file(path), path);
}

/// Best-effort clean-up of a folder and what is in it, see `clean_file`.
pub fn clean_dir(path: impl AsRef<std::path::Path>) {
    let path = path.as_ref();
    clean_result(std::fs::remove_dir_all(path), path);
}

/// `clean_file` for async code.
pub async fn clean_file_async(path: impl AsRef<std::path::Path>) {
    let path = path.as_ref();
    clean_result(tokio::fs::remove_file(path).await, path);
}

/// `clean_dir` for async code.
pub async fn clean_dir_async(path: impl AsRef<std::path::Path>) {
    let path = path.as_ref();
    clean_result(tokio::fs::remove_dir_all(path).await, path);
}

fn clean_result(result: std::io::Result<()>, path: &std::path::Path) {
    if let Err(err) = result
        && err.kind() != std::io::ErrorKind::NotFound
    {
        if is_locked(&err) {
            // A viewer, a build or a running exe still has the file open (pages::prune tries again later).
            tracing::debug!("clean-up of {} skipped, in use or access denied: {err}", path.display());
        } else {
            tracing::warn!("clean-up of {} failed: {err}", path.display());
        }
    }
}

/// Windows says a file is still open (access denied, os error 5, or a sharing violation, 32) when
/// another process holds it. Elsewhere it is never expected, so it stays a warning.
fn is_locked(err: &std::io::Error) -> bool {
    cfg!(windows) && matches!(err.raw_os_error(), Some(5 | 32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn warn_on_err_swallows_both_outcomes() {
        Ok::<(), String>(()).warn_on_err("fine");
        Err::<(), String>("boom".into()).warn_on_err("the write");
    }

    #[test]
    fn ignore_takes_any_error_and_an_option() {
        // No `Display` on the error: `GhError` has none.
        struct Opaque;
        Err::<(), Opaque>(Opaque).ignore("an error without Display");
        Ok::<u8, Opaque>(1).ignore("an Ok is dropped too");
        None::<String>.ignore("git::run gives an Option");
        Some(1).ignore("so is a Some");
        let (tx, rx) = tokio::sync::oneshot::channel::<u8>();
        drop(rx);
        let sent = tx.send(1);
        assert!(sent.is_err(), "the receiver is gone, so the send fails");
        sent.ignore("the receiver is gone in this test");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "ignore() needs a reason")]
    fn ignore_refuses_an_empty_reason() {
        Err::<(), ()>(()).ignore(" ");
    }

    #[test]
    fn locked_survives_poison() {
        let m = Arc::new(Mutex::new(1));
        let m2 = m.clone();
        let _ = std::thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("poison it");
        })
        .join();
        assert!(m.lock().is_err());
        *m.locked() += 1;
        assert_eq!(*m.locked(), 2);
    }

    #[test]
    fn in_use_files_are_told_apart_from_real_failures() {
        let io = std::io::Error::from_raw_os_error;
        // Access denied and sharing violation: expected on Windows while the file is open.
        assert_eq!(is_locked(&io(5)), cfg!(windows));
        assert_eq!(is_locked(&io(32)), cfg!(windows));
        // Any other error stays a warning.
        assert!(!is_locked(&io(13)));
        assert!(!is_locked(&std::io::Error::other("boom")));
    }

    #[test]
    fn send_or_log_reports_closed() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        assert!(send_or_log(&tx, 1, "test"));
        drop(rx);
        assert!(!send_or_log(&tx, 2, "test"));
    }
}
