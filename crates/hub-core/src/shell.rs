//! What the app's native shell was built from, so an update can tell whether the open window
//! must restart or only reload its pages.
//!
//! The app is two things: a small native shell (`app/src-tauri`: the window, its title bar,
//! folder picker, notifications) and the pages it shows (`app/src`). Nearly every version only
//! changes the pages. The updater copies the built pages to `bin/<version>/ui` and writes a stamp
//! of the shell's own code and dependencies to `bin/<version>/SHELL`; a running window whose shell
//! has the same stamp loads the new pages in place (`relaunch` in `app/src-tauri/src/main.rs`)
//! and keeps its size, place, page, drafts and other windows. Only a new stamp restarts it.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Folders whose files make up the shell, relative to the repository.
const SOURCES: &[&str] = &["app/src-tauri", "crates/hub-install"];

/// Folders under them that builds write and that say nothing about the shell.
const SKIP: &[&str] = &["target", "gen", "node_modules"];

/// The stamp of the shell in the checkout at `src`: its files, the Rust crates it is built from
/// (as `Cargo.lock` pins them, following `hub-app`'s dependencies only) and the version of the
/// Tauri script packages the pages talk to it with.
pub fn stamp(src: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    for root in SOURCES {
        let mut files = Vec::new();
        walk(&src.join(root), &mut files)?;
        files.sort();
        for file in files {
            let rel = file.strip_prefix(src).unwrap_or(&file).to_string_lossy().replace('\\', "/");
            let bytes = std::fs::read(&file).with_context(|| format!("reading {}", file.display()))?;
            hash.update(rel.as_bytes());
            hash.update([0]);
            // A checkout on Windows may have CRLF where the same commit elsewhere has LF.
            hash.update(strip_cr(&bytes));
            hash.update([0]);
        }
    }
    let lock = std::fs::read_to_string(src.join("Cargo.lock")).context("reading Cargo.lock")?;
    for line in crates_of(&lock, "hub-app")? {
        hash.update(line.as_bytes());
        hash.update(b"\n");
    }
    if let Ok(npm) = std::fs::read_to_string(src.join("app/package-lock.json")) {
        for line in tauri_packages(&npm) {
            hash.update(line.as_bytes());
            hash.update(b"\n");
        }
    }
    Ok(hash.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect())
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if !SKIP.iter().any(|s| entry.file_name() == *s) {
                walk(&path, out)?;
            }
        } else if kind.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

fn strip_cr(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().copied().filter(|b| *b != b'\r').collect()
}

/// `name version source` of every package `root` is built from, sorted.
fn crates_of(lock: &str, root: &str) -> Result<BTreeSet<String>> {
    #[derive(serde::Deserialize)]
    struct Lock {
        #[serde(default)]
        package: Vec<Package>,
    }
    #[derive(serde::Deserialize)]
    struct Package {
        name: String,
        version: String,
        #[serde(default)]
        source: Option<String>,
        #[serde(default)]
        dependencies: Vec<String>,
    }
    let lock: Lock = toml::from_str(lock).context("reading Cargo.lock")?;
    let mut by_name: HashMap<&str, Vec<&Package>> = HashMap::new();
    for p in &lock.package {
        by_name.entry(p.name.as_str()).or_default().push(p);
    }
    // A dependency is `name`, or `name version` (plus ` (source)`) when several versions exist.
    let find = |dep: &str| -> Option<&Package> {
        let mut parts = dep.split(' ');
        let name = parts.next()?;
        let version = parts.next();
        let all = by_name.get(name)?;
        match version {
            Some(v) => all.iter().copied().find(|p| p.version == v),
            None => all.first().copied(),
        }
    };
    let mut seen = BTreeSet::new();
    let mut todo: Vec<&Package> = by_name.get(root).map(|v| v.to_vec()).unwrap_or_default();
    anyhow::ensure!(!todo.is_empty(), "{root} is not in Cargo.lock");
    while let Some(p) = todo.pop() {
        let line = format!("{} {} {}", p.name, p.version, p.source.as_deref().unwrap_or(""));
        if seen.insert(line) {
            todo.extend(p.dependencies.iter().filter_map(|d| find(d)));
        }
    }
    Ok(seen)
}

/// `@tauri-apps/<name> <version>` for each Tauri package in the app's npm lockfile.
fn tauri_packages(lock: &str) -> BTreeSet<String> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(lock) else { return BTreeSet::new() };
    let Some(packages) = json["packages"].as_object() else { return BTreeSet::new() };
    packages
        .iter()
        .filter_map(|(path, p)| {
            let name = path.strip_prefix("node_modules/")?;
            name.starts_with("@tauri-apps/").then(|| format!("{name} {}", p["version"].as_str().unwrap_or("")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        crate::source_tree::root()
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn fake_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "app/src-tauri/src/main.rs", "fn main() {}\n");
        write(dir.path(), "app/src-tauri/tauri.conf.json", "{}\n");
        write(dir.path(), "crates/hub-install/src/lib.rs", "\n");
        write(
            dir.path(),
            "Cargo.lock",
            r#"version = 4
[[package]]
name = "hub-app"
version = "0.1.0"
dependencies = ["tauri", "hub-install"]
[[package]]
name = "hub-install"
version = "0.1.0"
[[package]]
name = "tauri"
version = "2.0.0"
source = "registry"
[[package]]
name = "axum"
version = "0.8.0"
source = "registry"
"#,
        );
        write(dir.path(), "app/package-lock.json", r#"{"packages":{"node_modules/@tauri-apps/api":{"version":"2.1.0"},"node_modules/svelte":{"version":"5.0.0"}}}"#);
        write(dir.path(), "app/src/App.svelte", "<p>hi</p>\n");
        dir
    }

    #[test]
    fn the_real_checkout_has_a_stamp() {
        assert_eq!(stamp(&repo()).unwrap().len(), 32);
    }

    #[test]
    fn pages_and_the_core_do_not_change_the_stamp_but_the_shell_does() {
        let dir = fake_repo();
        let first = stamp(dir.path()).unwrap();
        // The pages, a core-only crate and npm packages other than Tauri's are not the shell.
        write(dir.path(), "app/src/App.svelte", "<p>changed</p>\n");
        let lock = std::fs::read_to_string(dir.path().join("Cargo.lock")).unwrap().replace("0.8.0", "0.8.1");
        write(dir.path(), "Cargo.lock", &lock);
        write(dir.path(), "app/package-lock.json", r#"{"packages":{"node_modules/@tauri-apps/api":{"version":"2.1.0"},"node_modules/svelte":{"version":"5.1.0"}}}"#);
        // Build output under the shell is not either.
        write(dir.path(), "app/src-tauri/gen/schemas/x.json", "{}");
        write(dir.path(), "app/src-tauri/target/debug/x", "bin");
        // Nor line endings.
        write(dir.path(), "app/src-tauri/src/main.rs", "fn main() {}\r\n");
        assert_eq!(stamp(dir.path()).unwrap(), first);

        write(dir.path(), "app/src-tauri/src/main.rs", "fn main() { println!(); }\n");
        let changed = stamp(dir.path()).unwrap();
        assert_ne!(changed, first);

        let lock = std::fs::read_to_string(dir.path().join("Cargo.lock")).unwrap().replace("2.0.0", "2.0.1");
        write(dir.path(), "Cargo.lock", &lock);
        let tauri = stamp(dir.path()).unwrap();
        assert_ne!(tauri, changed);

        write(dir.path(), "app/package-lock.json", r#"{"packages":{"node_modules/@tauri-apps/api":{"version":"2.2.0"}}}"#);
        assert_ne!(stamp(dir.path()).unwrap(), tauri);
    }
}
