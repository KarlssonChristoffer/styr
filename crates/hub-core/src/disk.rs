//! How much room Hub takes and how much is left. The updater builds Hub from source, and a Rust
//! build folder grows by gigabytes with every version unless it is kept in check, so it looks
//! here before it builds and cleans up after: where the build folder is (`target_dir`), whether
//! there is room to build (`check_space`) and trimming it (`trim`), kept here so `remote` can ask
//! without naming `update`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const GB: u64 = 1 << 30;

/// The size of everything under `path`, not following links.
pub fn size(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return 0 };
    if !meta.is_dir() {
        return meta.len();
    }
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    entries.flatten().map(|e| size(&e.path())).sum()
}

/// Free space on the drive `path` is on, or would be on once created.
pub fn free(path: &Path) -> Option<u64> {
    let mut existing = path.to_path_buf();
    while !existing.exists() {
        existing = existing.parent()?.to_path_buf();
    }
    let path = existing.canonicalize().unwrap_or(existing);
    // Windows canonical paths start with `\\?\`, which no mount point does.
    let path = match path.to_str().and_then(|t| t.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    };
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// Removes what a build folder can do without: incremental caches always, and while the folder
/// is over `max` bytes, the debug build (only the tests use it) and then the builds for other
/// machines (`crossbuild`), unless one is running. Returns the bytes freed.
pub fn trim_build(target: &Path, max: u64) -> u64 {
    let before = size(target);
    for profile in ["debug", "release"] {
        crate::locked::clean_dir(target.join(profile).join("incremental"));
    }
    if size(target) > max {
        crate::locked::clean_dir(target.join("debug"));
    }
    if size(target) > max && !crate::crossbuild::building() {
        crate::locked::clean_dir(target.join(crate::crossbuild::DIR));
    }
    before.saturating_sub(size(target))
}

/// One thing Hub keeps on disk, for the settings page.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiskItem {
    pub label: String,
    pub path: String,
    #[ts(type = "number")]
    pub bytes: u64,
    /// Whether Hub rebuilds or refetches it by itself when it is deleted.
    pub disposable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiskUsage {
    pub items: Vec<DiskItem>,
    /// Free space on the drive of the data folder and of the build folder.
    #[ts(type = "number | null")]
    pub data_free: Option<u64>,
    #[ts(type = "number | null")]
    pub build_free: Option<u64>,
}

pub fn usage(data: &Path, src: &Path, build: &Path) -> DiskUsage {
    let item = |label: &str, path: PathBuf, disposable: bool| DiskItem {
        label: label.into(),
        bytes: size(&path),
        path: path.display().to_string(),
        disposable,
    };
    let cross = item("Builds for other machines", build.join(crate::crossbuild::DIR), true);
    // The builds for other machines are in the build folder, but listed on their own.
    let mut folder = item("Build folder", build.to_path_buf(), true);
    folder.bytes = folder.bytes.saturating_sub(cross.bytes);
    let items = vec![
        folder,
        cross,
        item("Installed versions", hub_install::bin_dir(data), false),
        item("Database", data.join("hub.db"), false),
        item("Database copies", crate::backup::dir(data), true),
        item("Speech to text", data.join("voice"), true),
        item("Logs", data.join("logs"), true),
        item("Engine logs", data.join("engines"), true),
        item("Source", src.to_path_buf(), true),
    ];
    DiskUsage { items, data_free: free(data), build_free: free(build) }
}

/// Where Rust builds go: the configured folder, or `target` in the source folder.
pub fn target_dir(cfg: &crate::config::UpdateConfig, src: &Path) -> PathBuf {
    cfg.target_dir.clone().unwrap_or_else(|| src.join("target"))
}

/// Keeps the build folder, builds for other machines included, under `max_build_gb`; also run
/// when the core starts.
pub fn trim(cfg: &crate::config::UpdateConfig, src: &Path) {
    let target = target_dir(cfg, src);
    let freed = trim_build(&target, cfg.max_build_gb * GB);
    if freed > 0 {
        tracing::info!("freed {:.1} GB in {}", freed as f64 / GB as f64, target.display());
    }
}

/// Too little room on the drive to build. Not the commit's fault, so it is tried again later.
#[derive(Debug, thiserror::Error)]
#[error("only {free:.1} GB free on the drive of {path}, and an update needs {needed} GB. Free some space or move the build folder to another drive in Settings")]
pub struct LowDisk {
    pub free: f64,
    pub needed: u64,
    pub path: String,
}

pub fn check_space(cfg: &crate::config::UpdateConfig, target: &Path) -> anyhow::Result<()> {
    let Some(free) = free(target) else { return Ok(()) };
    if free < cfg.min_free_gb * GB {
        return Err(LowDisk { free: free as f64 / GB as f64, needed: cfg.min_free_gb, path: target.display().to_string() }.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trimming_drops_caches_and_an_oversized_debug_build() {
        let target = tempfile::tempdir().unwrap();
        let t = target.path();
        for dir in ["debug/incremental", "debug/deps", "release/incremental", "release/deps"] {
            std::fs::create_dir_all(t.join(dir)).unwrap();
            std::fs::write(t.join(dir).join("f"), vec![0u8; 1000]).unwrap();
        }
        assert_eq!(trim_build(t, u64::MAX), 2000);
        assert!(t.join("debug/deps/f").exists());
        assert_eq!(trim_build(t, 1500), 1000);
        assert!(!t.join("debug").exists() && t.join("release/deps/f").exists());
        std::fs::create_dir_all(t.join("cross/target")).unwrap();
        std::fs::write(t.join("cross/target/f"), vec![0u8; 1000]).unwrap();
        assert_eq!(trim_build(t, 1500), 1000);
        assert!(!t.join("cross").exists() && t.join("release/deps/f").exists());
        assert!(free(t).is_some_and(|f| f > 0));
    }
}
