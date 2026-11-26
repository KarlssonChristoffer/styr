//! What the updater and the rest of the core say to each other without naming `update`: whether
//! this machine's own update is building or testing a version right now, and a nudge to look for
//! a new version now instead of at the next round (`hub` sends it when the user asks).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set while `update::build_and_install` runs, so nothing else empties the build folder under it.
static BUILDING: AtomicBool = AtomicBool::new(false);

/// Whether this machine's own update is building or testing a version right now.
pub fn building() -> bool {
    BUILDING.load(Ordering::Relaxed)
}

pub(crate) fn set_building(on: bool) {
    BUILDING.store(on, Ordering::Relaxed);
}

/// What the updater waits on between rounds.
pub(crate) fn look() -> &'static tokio::sync::Notify {
    static LOOK: OnceLock<tokio::sync::Notify> = OnceLock::new();
    LOOK.get_or_init(tokio::sync::Notify::new)
}

/// Asks the updater to fetch and look now instead of at its next round.
pub(crate) fn look_now() {
    look().notify_one();
}
