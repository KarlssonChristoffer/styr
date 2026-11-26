//! Which version a failed probation goes back to, split from `probation.rs`.

use std::path::Path;

use super::*;

/// The version to go back to for the one on probation: what it replaced, else the last good one,
/// else the newest other version that has all the programs. Never `failing` itself.
pub fn roll_back_target(data: &Path, failing: &str) -> Option<String> {
    roll_back_candidates(data, failing).into_iter().next()
}

/// Every version worth going back to, best first (see [`roll_back_target`]), for a rollback that
/// has to try the next one when the first does not start.
pub fn roll_back_candidates(data: &Path, failing: &str) -> Vec<String> {
    // A version that was just rolled back, or marked failed, is not a place to go back to: two bad
    // versions would send each other back and forth.
    let bad: Vec<String> = [std::fs::read_to_string(data.join("update-failed")).ok().map(|v| v.trim().to_owned()), read_rolled_back(data).map(|r| r.from)]
        .into_iter()
        .flatten()
        .collect();
    // A machine that runs the app never goes back to a core-only folder, which could not start it.
    let has_app = |v: &str| bin_dir(data).join(v).join(exe("hub-app")).is_file();
    let needs_app = has_app(failing);
    let usable = |v: &String| v != failing && !bad.contains(v) && complete(data, v) && (!needs_app || has_app(v));
    let mut out: Vec<String> = Vec::new();
    let mut add = |v: Option<String>| {
        if let Some(v) = v.filter(usable).filter(|v| !out.contains(v)) {
            out.push(v);
        }
    };
    add(read(data).filter(|p| p.version == failing).and_then(|p| p.previous));
    add(last_good(data));
    let mut others: Vec<(std::time::SystemTime, String)> = std::fs::read_dir(bin_dir(data))
        .map(|dir| {
            dir.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let at = e.metadata().ok()?.modified().ok()?;
                    Some((at, name))
                })
                .collect()
        })
        .unwrap_or_default();
    others.sort();
    for (_, name) in others.into_iter().rev() {
        add(Some(name));
    }
    out
}
