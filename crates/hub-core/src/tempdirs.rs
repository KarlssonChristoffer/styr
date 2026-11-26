//! Where tests make folders that must sit outside any git repository. A leaf module so the
//! low-level files (`git`) can use it without depending on `janitor`.

use std::path::{Path, PathBuf};

/// Where a test makes a folder that has to be outside any git repository: the system's temp
/// folder, or beside the outermost repository holding it. It can be inside one: an older
/// updater pointed it into the source clone's `target`.
pub fn temp_outside_git() -> PathBuf {
    let temp = std::env::temp_dir();
    let outermost = temp.ancestors().filter(|d| d.join(".git").exists()).last();
    outermost.and_then(Path::parent).map(Path::to_path_buf).unwrap_or(temp)
}

#[cfg(test)]
pub(crate) fn tempdir_outside_git() -> tempfile::TempDir {
    tempfile::tempdir_in(temp_outside_git()).expect("making a test folder")
}
