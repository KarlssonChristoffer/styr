//! The pages-only lane of an update: the stamp of what the programs are built from
//! (`native_stamp`), an installed version whose programs a new commit may keep, and a version
//! made of those programs plus freshly built pages (`build_pages_only`), checked by the docs'
//! `covers:` test; `place` moves any finished version into `bin/<version>`.

use super::*;

/// What the programs (`hubd`, the launcher, the app's shell) are built from: git's ids of the Rust
/// code, the lock files, the shell's folder and the toolchain settings at `rev`, hashed. Two
/// commits with the same stamp build the same programs, whatever else differs: the pages, the
/// docs, the design files. None when git cannot say.
pub(crate) fn native_stamp_at(src: &Path, rev: &str) -> Option<String> {
    let listing = git(src, &["ls-tree", rev, "--", "crates", "Cargo.toml", "Cargo.lock", "app/src-tauri", ".cargo", "rust-toolchain.toml", "rust-toolchain"]).ok()?;
    if !listing.contains("crates") {
        return None;
    }
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(listing.replace('\r', "").as_bytes());
    Some(hash.iter().take(16).map(|b| format!("{b:02x}")).collect())
}

pub(crate) fn native_stamp(src: &Path) -> Option<String> {
    native_stamp_at(src, "HEAD")
}

/// The pages-only lane could not put its version together (a program missing from the version
/// before, no pages built): the full build takes over.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(crate) struct PagesUnavailable(String);

/// An installed version whose programs a build of `src` may keep: the same Rust code and the same
/// app shell, and all its programs there. The newest such one. None when a full build is due,
/// the fast lane is off, the build has its own steps, or no version has a stamp yet (the first
/// build after this arrived).
pub(crate) fn pages_only_donor(cfg: &UpdateConfig, data: &Path, src: &Path, version: &str) -> Option<PathBuf> {
    if !cfg.fast_pages || cfg.steps.is_some() || cfg.artifacts.is_some() {
        return None;
    }
    let native = native_stamp(src)?;
    let shell = crate::shell::stamp(src).ok()?;
    let read = |dir: &Path, name: &str| std::fs::read_to_string(dir.join(name)).ok().map(|t| t.trim().to_owned());
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(hub_install::bin_dir(data)).ok()?.flatten() {
        let dir = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == version || name.ends_with(".partial") || !dir.join("VERSION").is_file() {
            continue;
        }
        let whole = ["hubd", "hub-app", "hub-launcher"].iter().all(|n| dir.join(hub_install::exe(n)).is_file());
        if !whole || read(&dir, hub_install::NATIVE).as_deref() != Some(native.as_str()) || read(&dir, hub_install::SHELL).as_deref() != Some(shell.as_str()) {
            continue;
        }
        let when = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(t, _)| when > *t) {
            best = Some((when, dir));
        }
    }
    best.map(|(_, dir)| dir)
}

/// The pages' own checks (the docs' `covers:` lines are the one Rust test a change to pages can
/// break) and their build, then a version made of `donor`'s programs and the new pages.
pub(super) fn build_pages_only(cfg: &UpdateConfig, data: &Path, src: &Path, version: &str, donor: &Path, progress: &dyn Fn(String), stop: &AtomicBool) -> Result<()> {
    if version == hub_install::running_version() {
        bail!("{version} is the running version; it cannot be replaced while it runs");
    }
    run_steps(cfg, data, src, &target_dir(cfg, src), &UpdateConfig::pages_steps(), progress, stop)?;
    covers_hold(src)?;
    let partial = hub_install::bin_dir(data).join(format!("{version}.partial"));
    assemble_pages(donor, src, &partial, version).map_err(|err| PagesUnavailable(format!("{err:#}")))?;
    place(data, &partial, version)
}

/// The test that Hub's docs still point at real files, run here because no Rust test runs on
/// the pages-only lane.
pub(crate) fn covers_hold(src: &Path) -> Result<()> {
    let stale = crate::covers::stale(src);
    if stale.is_empty() {
        return Ok(());
    }
    bail!("these covers: match no file any more, fix the doc:\n{}", stale.join("\n"))
}

/// A version's folder made of `donor`'s programs and the pages built in `src`.
pub(super) fn assemble_pages(donor: &Path, src: &Path, dest: &Path, version: &str) -> Result<()> {
    crate::locked::clean_dir(dest);
    std::fs::create_dir_all(dest)?;
    for name in ["hubd", "hub-app", "hub-launcher"] {
        let file = hub_install::exe(name);
        std::fs::copy(donor.join(&file), dest.join(&file)).with_context(|| format!("copying {file} from the version before"))?;
    }
    let pages = src.join("app/dist");
    if !pages.join("index.html").is_file() {
        bail!("the pages did not build: no app/dist/index.html");
    }
    copy_dir(&pages, &dest.join(hub_install::UI_DIR)).context("copying the app's pages")?;
    std::fs::write(dest.join(hub_install::SHELL), crate::shell::stamp(src)?)?;
    std::fs::write(dest.join(hub_install::NATIVE), native_stamp(src).context("no stamp for the programs")?)?;
    hub_install::probation::stamp_version(dest, version)?;
    Ok(())
}

/// Moves the assembled folder `partial` into place as `bin/<version>` and tidies up.
pub(super) fn place(data: &Path, partial: &Path, version: &str) -> Result<()> {
    let target = hub_install::bin_dir(data).join(version);
    crate::locked::clean_dir(&target);
    std::fs::rename(partial, &target)?;

    // The launcher exits right after starting the app, so it is almost never locked.
    let launcher = target.join(hub_install::exe("hub-launcher"));
    if launcher.exists() {
        let name = if cfg!(windows) { "Hub.exe" } else { "hub" };
        if let Err(err) = std::fs::copy(&launcher, data.join(name)) {
            tracing::warn!("could not update the launcher: {err}");
        }
    }
    prune(data, version);
    Ok(())
}
