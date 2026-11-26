//! Where Hub's own source is, for the tests that read it: the rules and layers guards, the map,
//! the docs' `covers:`, fixtures. Found at run time, not with `env!("CARGO_MANIFEST_DIR")`, which
//! is baked in when the test is built: where checkouts share one build folder
//! (`CARGO_TARGET_DIR`), cargo reuses a test built in one worktree for another of the same commit
//! (its fingerprint holds no absolute path), and that test then read, and blessed, the first
//! worktree's files. cargo runs a test with its package folder as the working folder, so the
//! checkout is the nearest folder at or above it with a workspace `Cargo.toml` and Hub's core in it.
//! No test changes its working folder (`set_current_dir`); one that did would move this too.
//! `prefs::source_dir` keeps `env!`: a dev build's own checkout is where it was compiled.

use std::path::{Path, PathBuf};

/// The checkout the running test belongs to (the workspace root).
pub fn root() -> PathBuf {
    let here = std::env::current_dir().expect("a test has a working folder");
    find(&here).unwrap_or_else(|| panic!("{} is not inside a Hub checkout; run the tests with cargo", here.display()))
}

/// `crates/hub-core` of that checkout.
pub fn hub_core() -> PathBuf {
    root().join("crates").join("hub-core")
}

/// The nearest folder at or above `start` whose `Cargo.toml` is a workspace with Hub's core in it.
fn find(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|dir| {
        let workspace = std::fs::read_to_string(dir.join("Cargo.toml")).is_ok_and(|t| t.lines().any(|l| l.trim() == "[workspace]"));
        workspace && dir.join("crates").join("hub-core").join("Cargo.toml").is_file()
    }).map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checkout_is_found_from_where_the_test_runs_not_where_it_was_built() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("checkout");
        let core = repo.join("crates").join("hub-core");
        std::fs::create_dir_all(core.join("src")).unwrap();
        std::fs::write(repo.join("Cargo.toml"), "[workspace]\nmembers = [\"crates/hub-core\"]\n").unwrap();
        // A member says `version.workspace = true`; that is not a workspace.
        std::fs::write(core.join("Cargo.toml"), "[package]\nversion.workspace = true\n").unwrap();
        assert_eq!(find(&core.join("src")), Some(repo.clone()));
        assert_eq!(find(&repo), Some(repo.clone()));
        // A workspace without Hub's core is some other project.
        std::fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        assert_eq!(find(dir.path()), None);
        // This test itself runs in crates/hub-core of the checkout it was started in.
        let root = root();
        assert!(root.join("crates/hub-core/src/source_tree.rs").is_file(), "{}", root.display());
        assert_eq!(hub_core(), root.join("crates").join("hub-core"));
        assert_eq!(std::env::current_dir().unwrap(), hub_core());
    }
}
