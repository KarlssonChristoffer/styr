//! Where problems wait to be taken in (`problems::start`): [`capture`] hands one over from
//! anywhere, including the log layer and code that knows nothing of the core, and never blocks or
//! fails. Kept apart from `problems/` so a reporter does not reach the module that records them.

use std::sync::{Mutex, OnceLock};

use tokio::sync::mpsc;

use crate::locked::LockExt;

use crate::model::problems::{Hit, Level, Report};

/// A problem on its way in, before it is grouped.
#[derive(Debug, Clone)]
pub struct Incoming {
    pub project: String,
    pub source: String,
    pub level: Level,
    pub folder: Option<String>,
    pub fingerprint: Option<Vec<String>>,
    pub hit: Hit,
}

impl Incoming {
    pub fn from_report(report: Report, default_source: &str) -> Incoming {
        Incoming {
            project: report.project.filter(|p| !p.trim().is_empty()).unwrap_or_else(|| "Hub".into()),
            source: report.source.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| default_source.into()),
            level: report.level.unwrap_or(Level::Error),
            folder: report.folder,
            fingerprint: report.fingerprint,
            hit: report.hit,
        }
    }
}

static INBOX: OnceLock<mpsc::UnboundedSender<Incoming>> = OnceLock::new();
/// Problems reported before the core was up (a warning while reading the settings).
static EARLY: Mutex<Vec<Incoming>> = Mutex::new(Vec::new());

/// Hands a problem to the core from anywhere, including the log layer; never blocks or fails.
pub fn capture(incoming: Incoming) {
    match INBOX.get() {
        // A closed inbox is a core that is gone: nothing is left to tell.
        Some(tx) => drop(tx.send(incoming)),
        None => {
            let mut early = EARLY.locked();
            if early.len() < 200 {
                early.push(incoming);
            }
        }
    }
}

/// Opens the inbox for the core that takes problems in, with what [`capture`] got before it was
/// open. None when a core already took it (a second core in the same process, in tests).
pub(crate) fn open() -> Option<mpsc::UnboundedReceiver<Incoming>> {
    let (tx, rx) = mpsc::unbounded_channel();
    if INBOX.set(tx.clone()).is_err() {
        return None;
    }
    for early in std::mem::take(&mut *EARLY.locked()) {
        drop(tx.send(early));
    }
    Some(rx)
}
