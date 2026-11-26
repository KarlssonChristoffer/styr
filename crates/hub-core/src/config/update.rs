//! The update settings' own defaults: which steps build a version and what it installs, whether
//! updating is on, and where the updater's clone lives. Read by the updater and by the pages that
//! show it, so none of them needs `update.rs` for it.

use std::path::{Path, PathBuf};

use super::{BuildStep, UpdateConfig};

impl UpdateConfig {
    /// Off for development builds unless set; on for installed builds.
    pub fn is_enabled(&self, running_version: &str) -> bool {
        self.enabled.unwrap_or(running_version != "dev")
    }

    /// The updater's own clone: `source_dir`, else `<data dir>/src`.
    pub fn source_in(&self, data: &Path) -> PathBuf {
        self.source_dir.clone().unwrap_or_else(|| data.join("src"))
    }

    /// What a version needs when only its pages changed: the packages, the pages' own checks and
    /// their build. The Rust tests and the builds of the programs are left out.
    pub fn pages_steps() -> Vec<BuildStep> {
        let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
        let step = |cmd: &[&str]| BuildStep { dir: "app".into(), command: cmd.iter().map(|s| s.to_string()).collect() };
        vec![
            step(&[npm, "install", "--no-fund", "--no-audit"]),
            step(&[npm, "run", "check"]),
            step(&[npm, "test"]),
            step(&[npm, "run", "build"]),
        ]
    }

    pub fn default_steps() -> Vec<BuildStep> {
        let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
        let step = |dir: &str, cmd: &[&str]| BuildStep {
            dir: dir.into(),
            command: cmd.iter().map(|s| s.to_string()).collect(),
        };
        vec![
            step("app", &[npm, "install", "--no-fund", "--no-audit"]),
            // Tests first: a commit that fails them is never built, let alone installed. This is
            // the CI, run where Hub runs.
            step("app", &[npm, "run", "check"]),
            // The pages' behaviour tests (vitest, about a second); the type check alone passes
            // while these fail.
            step("app", &[npm, "test"]),
            // Four tests at a time per binary: the engine tests each start an agent process, and
            // on a machine still hot from the build, more than that made them time out.
            step("", &["cargo", "test", "--workspace", "--exclude", "hub-app", "--", "--test-threads=4"]),
            // Builds the frontend and the release app with it embedded, without an installer.
            step("app", &[npm, "run", "tauri", "--", "build", "--no-bundle"]),
            step("", &["cargo", "build", "--release", "-p", "hubd", "-p", "hub-launcher"]),
        ]
    }

    pub fn default_artifacts() -> Vec<PathBuf> {
        ["hubd", "hub-app", "hub-launcher"]
            .iter()
            .map(|n| PathBuf::from("target/release").join(hub_install::exe(n)))
            .collect()
    }
}
