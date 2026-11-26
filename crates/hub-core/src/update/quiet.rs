//! When a version that changes Rust may build, per the build mode (`hold`): after a quiet branch
//! (capped at `MAX_WAIT_QUIET_TIMES`), after enough merges, at a set clock time, or on Build now;
//! a version whose Rust is already installed never waits. Also the merged changes the running
//! version lacks (`GET /api/update/waiting`), and the build settings re-read from `hub.json`.

use super::*;

/// How many times the quiet time a Rust change may wait in all, however often the branch moves:
/// merges that keep arriving every few minutes would otherwise push the build off for ever (an
/// hour and a half, once, with a fix waiting in it).
pub(super) const MAX_WAIT_QUIET_TIMES: i64 = 3;

/// When the last build ended on this machine (ms), 0 before the first. A commit that was merged
/// while a build ran has had its wait already: the build was the window in which more Rust
/// changes joined, so the next build starts as soon as the first is done.
pub(super) static LAST_BUILD_END: AtomicI64 = AtomicI64::new(0);

/// When a build of `rev` may start: `quiet_mins` after the commit, when `rev` changes the programs
/// (no installed version has its Rust) and the branch has not been quiet that long yet, but never
/// later than `MAX_WAIT_QUIET_TIMES` times that after the oldest Rust change `running` lacks, and
/// not at all when the newest commit arrived while the last build (`built_at`) was running. None
/// when it may start now.
pub(super) fn quiet_until(data: &Path, src: &Path, rev: &str, running: &str, quiet_mins: u64, built_at: i64, now: i64) -> Option<i64> {
    if quiet_mins == 0 {
        return None;
    }
    let native = native_stamp_at(src, rev)?;
    let kept = std::fs::read_dir(hub_install::bin_dir(data))
        .map(|dirs| dirs.flatten().any(|d| std::fs::read_to_string(d.path().join(hub_install::NATIVE)).is_ok_and(|t| t.trim() == native)))
        .unwrap_or(false);
    if kept {
        return None;
    }
    let at: i64 = git(src, &["log", "-1", "--format=%ct", rev]).ok()?.trim().parse().ok()?;
    if at * 1000 <= built_at {
        return None;
    }
    // The oldest commit since the running version that touches the programs, when the running
    // version is one this checkout knows.
    let oldest: Option<i64> = git(
        src,
        &["log", "--reverse", "--format=%ct", &format!("{running}..{rev}"), "--", "crates", "Cargo.toml", "Cargo.lock", "app/src-tauri", ".cargo", "rust-toolchain.toml", "rust-toolchain"],
    )
    .ok()
    .and_then(|log| log.lines().next().and_then(|l| l.trim().parse().ok()));
    let until = wait_until(at, oldest, quiet_mins);
    (until > now).then_some(until)
}

/// Quiet time after the newest commit (`at`, seconds), cut off `MAX_WAIT_QUIET_TIMES` quiet times
/// after the oldest unbuilt Rust change (`oldest`, seconds) when there is one.
pub(super) fn wait_until(at: i64, oldest: Option<i64>, quiet_mins: u64) -> i64 {
    let quiet = quiet_mins as i64 * 60_000;
    let until = at * 1000 + quiet;
    oldest.map_or(until, |oldest| until.min(oldest * 1000 + quiet * MAX_WAIT_QUIET_TIMES))
}

/// The paths whose changes need a full build: the programs.
pub(super) const NATIVE_PATHS: [&str; 7] = ["crates", "Cargo.toml", "Cargo.lock", "app/src-tauri", ".cargo", "rust-toolchain.toml", "rust-toolchain"];

/// What a version that has not been built yet is held back for, and until when.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Held {
    /// The time it goes at the latest, ms. None when only a click starts it.
    pub(super) until: Option<i64>,
    pub(super) wait: UpdateWait,
    pub(super) why: String,
}

/// The merged changes (first-parent commits, one per merged pull request) of `rev` that the
/// running version lacks, newest first, each with whether it changes the programs. Empty when the
/// running version is not one this checkout knows.
pub(super) fn changes_since(src: &Path, running: &str, rev: &str) -> Vec<WaitingChange> {
    let range = format!("{running}..{rev}");
    let Ok(log) = git(src, &["log", "--first-parent", "--format=%H\t%ct\t%s", &range]) else {
        return Vec::new();
    };
    let mut args = vec!["log", "--first-parent", "--format=%H", range.as_str(), "--"];
    args.extend(NATIVE_PATHS);
    let rust: std::collections::HashSet<String> = git(src, &args).map(|l| l.lines().map(|h| h.trim().to_owned()).collect()).unwrap_or_default();
    log.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (sha, at, subject) = (parts.next()?.trim(), parts.next()?.trim().parse::<i64>().ok()?, parts.next()?.trim());
            // "Merge pull request #12 from owner/branch" has the title on the line after; the
            // number is what the list shows when the subject has no better words.
            let pr = subject.strip_prefix("Merge pull request #").and_then(|r| r.split_whitespace().next()).and_then(|n| n.parse().ok());
            let title = match pr {
                Some(_) => git(src, &["log", "-1", "--format=%b", sha]).ok().and_then(|b| b.lines().find(|l| !l.trim().is_empty()).map(|l| l.trim().to_owned())),
                None => None,
            }
            .unwrap_or_else(|| subject.to_owned());
            Some(WaitingChange { sha: sha.chars().take(12).collect(), title, pr, at: at * 1000, rust: rust.contains(sha) })
        })
        .collect()
}

/// The first clock time of `times` (`HH:MM` on the clock `offset_min` from UTC) at or after
/// `from` (ms), as a time in ms. None when none of them reads as a time.
pub(super) fn next_slot(times: &[String], offset_min: i32, from: i64) -> Option<i64> {
    let minute = 60_000_i64;
    let day = 1_440 * minute;
    let local = from + i64::from(offset_min) * minute;
    let start = local - local.rem_euclid(day);
    times
        .iter()
        .filter_map(|t| {
            let (h, m) = t.trim().split_once(':')?;
            let (h, m): (i64, i64) = (h.parse().ok()?, m.parse().ok()?);
            (h < 24 && m < 60).then_some((h * 60 + m) * minute)
        })
        .flat_map(|at| [start + at, start + day + at])
        .filter(|t| *t >= local)
        .min()
        .map(|t| t - i64::from(offset_min) * minute)
}

/// Decides, from what is merged, whether a full build of `rev` may start. None when it may.
/// A version that only changes the pages (an installed one has its Rust) never waits.
#[allow(clippy::too_many_arguments)]
pub(super) fn hold(data: &Path, src: &Path, rev: &str, running: &str, cfg: &UpdateConfig, built_at: i64, now: i64) -> Option<Held> {
    let native = native_stamp_at(src, rev)?;
    let kept = std::fs::read_dir(hub_install::bin_dir(data))
        .map(|dirs| dirs.flatten().any(|d| std::fs::read_to_string(d.path().join(hub_install::NATIVE)).is_ok_and(|t| t.trim() == native)))
        .unwrap_or(false);
    if kept {
        return None;
    }
    let changes = changes_since(src, running, rev);
    let rust: Vec<&WaitingChange> = changes.iter().filter(|c| c.rust).collect();
    let mode = match cfg.build_mode {
        BuildMode::RightAway => "right_away",
        BuildMode::Merges => "merges",
        BuildMode::Times => "times",
        BuildMode::Manual => "manual",
    };
    let wait = |goal: Option<u32>| UpdateWait { rust: rust.len() as u32, total: changes.len() as u32, goal, notice_at: cfg.build_notice_at.clone(), mode: mode.into() };
    // The oldest Rust change the running version lacks, in ms; unknown when this checkout does not
    // know the running version, and then the newest commit's time stands in for it.
    let oldest = rust.iter().map(|c| c.at).min();
    match cfg.build_mode {
        BuildMode::RightAway => {
            let until = quiet_until(data, src, rev, running, cfg.rust_quiet_mins, built_at, now)?;
            Some(Held { until: Some(until), wait: wait(None), why: format!("Rust changed: waits until the branch has been quiet {} minutes, so more Rust changes build with it", cfg.rust_quiet_mins) })
        }
        BuildMode::Merges => {
            let goal = cfg.build_merges.max(1);
            // A merge that landed while the last build ran has waited long enough already.
            let cap = oldest.map(|o| o + cfg.build_max_wait_mins as i64 * 60_000);
            if rust.len() as u32 >= goal && !rust.is_empty() || cap.is_some_and(|c| c <= now) {
                return None;
            }
            let why = format!("{} of {goal} Rust changes wait; it builds at {goal}, or {} minutes after the first", rust.len(), cfg.build_max_wait_mins);
            Some(Held { until: cap, wait: wait(Some(goal)), why })
        }
        BuildMode::Times => {
            let from = oldest.unwrap_or(now);
            // What was merged before a slot builds at it; what is merged after waits for the next.
            let until = next_slot(&cfg.build_times, cfg.build_utc_offset_min, from)?;
            if until <= now {
                return None;
            }
            Some(Held { until: Some(until), wait: wait(None), why: "Rust changed: builds at the next set time".into() })
        }
        BuildMode::Manual => Some(Held { until: None, wait: wait(None), why: "Rust changed: builds when you press Build now".into() }),
    }
}

/// The settings that say when to build, read again from `hub.json` on every look so a change
/// under Settings > Updates counts at once, not after a restart.
pub(super) fn live(cfg: &UpdateConfig) -> UpdateConfig {
    let mut cfg = cfg.clone();
    let saved = crate::config::read_file().ok().and_then(|f| f.get("update").cloned()).and_then(|u| serde_json::from_value::<UpdateConfig>(u).ok());
    if let Some(saved) = saved {
        cfg.build_mode = saved.build_mode;
        cfg.build_merges = saved.build_merges;
        cfg.build_notice_at = saved.build_notice_at;
        cfg.build_max_wait_mins = saved.build_max_wait_mins;
        cfg.build_times = saved.build_times;
        cfg.build_utc_offset_min = saved.build_utc_offset_min;
        cfg.rust_quiet_mins = saved.rust_quiet_mins;
    }
    cfg
}

/// What is merged and not yet built into the running version, newest first (`GET
/// /api/update/waiting`). Empty before the first fetch or when the running version is unknown here.
pub fn waiting_changes(cfg: &UpdateConfig, data: &Path, running: &str) -> Vec<WaitingChange> {
    let src = cfg.source_in(data);
    let rev = format!("{}/{}", cfg.remote, cfg.branch);
    changes_since(&src, running, &rev)
}
