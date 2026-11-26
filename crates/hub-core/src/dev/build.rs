//! The clone's packages and its build cache: `npm install` with a stamp of what it installed (so
//! setup and a preview refresh know when to install again), and keeping the build cache under its
//! cap (`trim_cache`, for the dev core and the worktrees).

use std::path::Path;

use anyhow::{Context, Result};

use super::run;

/// Written into `node_modules` after an install: what `package.json` and `package-lock.json` said
/// then. Comparing the files with it, not with the commit the last refresh started from, catches a
/// package added by a merge whose refresh never installed (the install failed, was cut off, or
/// the clone was moved by something else).
pub(super) const PACKAGES_STAMP: &str = "node_modules/.hub-packages";

pub(super) fn packages_stamp(app: &Path) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for name in ["package.json", "package-lock.json"] {
        hasher.update(std::fs::read(app.join(name)).unwrap_or_default());
        hasher.update([0]);
    }
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The clone has no packages, or not the ones its `package.json` and lock ask for now.
pub(super) fn packages_stale(app: &Path) -> bool {
    !app.join("node_modules").is_dir() || std::fs::read_to_string(app.join(PACKAGES_STAMP)).map_or(true, |s| s.trim() != packages_stamp(app))
}

/// `npm install` in the clone's `app`, then the stamp of what it installed.
pub(super) fn install_packages(app: &Path, log: &Path) -> Result<()> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    run(app, npm, &["install", "--no-fund", "--no-audit"], log)?;
    std::fs::write(app.join(PACKAGES_STAMP), packages_stamp(app)).context("writing the packages stamp")
}

/// Bytes in a folder, links not followed.
fn size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

/// Keeps the build cache under `cap`: past it, the incremental cache goes first (it grows with
/// every build and is quick to make again), then the whole debug build. Answers the size after.
pub(crate) fn trim_cache(target: &Path, cap: u64) -> u64 {
    let mut now = size(target);
    for part in ["debug/incremental", "debug"] {
        if now <= cap {
            break;
        }
        let path = target.join(part);
        tracing::info!("dev: the build cache is {} MB, over its cap; removing {}", now >> 20, path.display());
        // A build running meanwhile keeps its files; the rest goes.
        crate::locked::clean_dir(&path);
        now = size(target);
    }
    now
}
