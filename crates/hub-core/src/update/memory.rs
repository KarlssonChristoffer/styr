//! Whether the machine has room for an update build: wait out dictation and low memory before a
//! step starts, stop a build once memory stays short (`memory_short`) and name who held it, tell
//! an out-of-memory failure from broken code, and how many build jobs to run (half the cores).

use super::*;

/// While someone dictates, a build is stopped when less than this is left: whisper's biggest
/// model needs about 2 GB to load next to the rest of Hub.
pub(super) const DICTATION_MEMORY: u64 = 3 << 30;

/// Waits until nobody has dictated for a while, so a build step does not start mid-sentence.
pub(crate) fn wait_for_dictation(say: &dyn Fn(), stop: &AtomicBool) -> Result<()> {
    let mut told = false;
    while crate::voice::dictating() {
        if stop.load(Ordering::Relaxed) {
            bail!("stopped to install the version that was ready");
        }
        if !told {
            say();
            told = true;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    Ok(())
}

/// A build needs this much free memory to start, and is stopped below `LOW_MEMORY` once that has
/// lasted `LOW_MEMORY_GRACE`.
pub(super) const BUILD_MEMORY: u64 = 4 << 30;
pub(super) const LOW_MEMORY: u64 = 1 << 30;
pub(super) const LOW_MEMORY_GRACE: Duration = Duration::from_secs(20);

/// True when `left` has been below `LOW_MEMORY` for `LOW_MEMORY_GRACE` without a break. `low_since`
/// is when the current dip started; memory that comes back resets it.
pub(super) fn memory_short(low_since: &mut Option<std::time::Instant>, left: Option<u64>, now: std::time::Instant) -> bool {
    if left.is_some_and(|left| left < LOW_MEMORY) {
        let since = *low_since.get_or_insert(now);
        now.duration_since(since) >= LOW_MEMORY_GRACE
    } else {
        *low_since = None;
        false
    }
}

/// The five processes holding the most memory, for the log line of a build that was stopped for
/// memory: the stop only says that it ran short, this says who had it.
pub(super) fn biggest_processes() -> String {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let mut all: Vec<(String, u64)> = sys.processes().values().map(|p| (p.name().to_string_lossy().into_owned(), p.memory())).collect();
    // The same program often runs many times (a browser, the compiler): add them up by name.
    all.sort();
    let mut sums: Vec<(String, u64, usize)> = Vec::new();
    for (name, memory) in all {
        match sums.last_mut() {
            Some(last) if last.0 == name => {
                last.1 += memory;
                last.2 += 1;
            }
            _ => sums.push((name, memory, 1)),
        }
    }
    sums.sort_by_key(|x| std::cmp::Reverse(x.1));
    sums.iter().take(5).map(|(name, memory, count)| format!("{name} x{count} {} MB", memory >> 20)).collect::<Vec<_>>().join(", ")
}

/// What can still be handed out: free RAM plus free page file or swap. On Windows this is what
/// runs out first (os error 1455) when many threads and a build share the machine.
pub(super) fn headroom() -> Option<u64> {
    // macOS keeps its memory full of cache that it gives back on demand and grows its swap on
    // the disk as needed, so "available" stays low on a Mac that has plenty: waiting for it
    // never ended. A Mac builds whenever it is asked to.
    if cfg!(target_os = "macos") {
        return None;
    }
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total = sys.total_memory();
    (total > 0).then(|| sys.available_memory() + sys.free_swap())
}

/// Waits until there is room for a build, so an update never starts while your work has the
/// memory. `say` is told how many GB it waits for, once.
pub(crate) fn wait_for_memory(say: &dyn Fn(u64), stop: &AtomicBool) -> Result<()> {
    let mut told = false;
    while headroom().is_some_and(|left| left < BUILD_MEMORY) {
        if stop.load(Ordering::Relaxed) {
            bail!("stopped to install the version that was ready");
        }
        if !told {
            say(BUILD_MEMORY >> 30);
            told = true;
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    Ok(())
}

/// Parallel build jobs for an update: half the cores, at least one.
pub(crate) fn build_jobs() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).div_ceil(2)
}

/// A build that failed because the machine ran out of memory or page file, not because of the code.
pub(crate) fn out_of_memory(output: &str) -> bool {
    let text = output.to_lowercase();
    ["os error 1455", "os error 1453", "os error 8)", "paging file", "sidväxlingsfilen", "memory allocation of", "out of memory", "signal: 9, sigkill"]
        .iter()
        .any(|m| text.contains(m))
}
