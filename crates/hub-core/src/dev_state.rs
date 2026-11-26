//! Dev mode's state (`dev.rs`): what is being set up, the servers this core started and the
//! preview's last refresh. Kept apart so `Hub` holds it without reaching the dev mode code, which
//! sits above `hub`.

use std::process::Child;
use std::sync::Mutex;

use crate::locked::LockExt;

use crate::model::{DevPreviewCheck, DevPreviewPr, DevState};

#[derive(Default)]
pub struct Dev {
    pub(crate) inner: Mutex<Inner>,
    /// Wakes the preview loop for a refresh now.
    pub(crate) kick: tokio::sync::Notify,
    /// When a surface last asked for the status (`GET /api/dev`), in ms.
    pub(crate) looked: std::sync::atomic::AtomicI64,
    /// Held while the preview moves the clone between branches, so two moves never overlap.
    pub(crate) moving: tokio::sync::Mutex<()>,
}

#[derive(Default)]
pub(crate) struct Inner {
    /// Setting up or starting, which the folder and the port do not show yet.
    pub(crate) busy: Option<DevState>,
    pub(crate) failed: bool,
    pub(crate) message: Option<String>,
    /// The live UI server, when this core started it.
    pub(crate) server: Option<Child>,
    /// The dev core, when this core started it.
    pub(crate) core: Option<Child>,
    pub(crate) core_building: bool,
    pub(crate) core_failed: bool,
    pub(crate) core_message: Option<String>,
    /// Whether the dev window should talk to the dev core. Ends the loop that rebuilds it.
    pub(crate) core_wanted: bool,
    /// The shared build cache's size when last measured.
    pub(crate) cache_bytes: Option<u64>,
    pub(crate) preview: Preview,
}

/// The preview's last refresh, and the loop that keeps it current.
#[derive(Default)]
pub(crate) struct Preview {
    pub(crate) following: bool,
    /// The next refresh parks changed files instead of waiting for them to go.
    pub(crate) force: bool,
    pub(crate) refreshing: bool,
    /// The base and pull request heads it was built from; a refresh happens when they move.
    pub(crate) built_from: Option<String>,
    pub(crate) base: Option<String>,
    pub(crate) included: Vec<DevPreviewPr>,
    pub(crate) skipped: Vec<DevPreviewPr>,
    pub(crate) open: Vec<DevPreviewPr>,
    pub(crate) core_changed: bool,
    pub(crate) refreshed_at: Option<i64>,
    pub(crate) message: Option<String>,
    /// Merged lately and not in the installed version yet: the pages already show them (they are
    /// in the base), and the update is on its way.
    pub(crate) merged: Vec<DevPreviewPr>,
    /// What the last look for pushes found.
    pub(crate) check: Option<DevPreviewCheck>,
    /// The merged list gh last answered, with the base head and open pull requests it was asked
    /// under and when: asked again only when those move or the answer is old (`MERGED_FRESH_MS`).
    pub(crate) merged_asked: Option<(String, i64, Vec<MergedPull>)>,
    /// What the pages that changed in the last rebuild say, to read back from Vite.
    pub(crate) probe: Vec<Probe>,
}

/// A page the preview changed and a few phrases from the change, to ask Vite for it and see that
/// it serves what is on disk.
#[derive(Clone)]
pub(crate) struct Probe {
    /// As Vite serves it, `/src/lib/components/X.svelte`.
    pub(crate) url_path: String,
    pub(crate) needles: Vec<String>,
}

impl Dev {
    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.locked()
    }

    pub(crate) fn fail(&self, message: String) {
        let mut inner = self.lock();
        inner.busy = None;
        inner.failed = true;
        inner.message = Some(message);
    }
}

#[derive(serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MergedPull {
    pub(crate) number: u32,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) merge_commit: Option<MergeCommit>,
}

#[derive(serde::Deserialize, Clone)]
pub(crate) struct MergeCommit {
    pub(crate) oid: String,
}
