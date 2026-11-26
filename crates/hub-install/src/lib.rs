//! Layout of an installed Hub, shared by the core, the app and the launcher:
//!
//! ```text
//! <data dir>/            %LOCALAPPDATA%\hub on Windows, ~/.local/share/hub elsewhere
//!   hub.db, hub.json
//!   src/                 the updater's own clone of the repository
//!   bin/<version>/       hubd, hub-app, hub-launcher (a core-only machine: hubd; the PROGRAMS file lists them) and a VERSION file, one folder per build;
//!                        also the app's pages (`ui/`) and its shell's stamp (`SHELL`)
//!   current              the version the launcher starts
//!   last-good, probation.json, rolled-back.json   see [`probation`]
//!   Hub(.exe)            the launcher, which the Start menu shortcut points at
//! ```

use std::path::{Path, PathBuf};

pub mod ignore;
pub mod probation;
pub mod proc;

pub fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join("hub")
}

/// `name` with `.exe` on Windows.
pub fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() }
}

pub fn bin_dir(data: &Path) -> PathBuf {
    data.join("bin")
}

pub fn current_version(data: &Path) -> Option<String> {
    let v = std::fs::read_to_string(data.join("current")).ok()?;
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_owned())
}

/// The folder of the version the launcher would start.
pub fn current_dir(data: &Path) -> Option<PathBuf> {
    let dir = bin_dir(data).join(current_version(data)?);
    dir.is_dir().then_some(dir)
}

/// Points the launcher at `version`. Written to a temporary file first so it is never half done.
pub fn set_current(data: &Path, version: &str) -> std::io::Result<()> {
    let tmp = data.join("current.tmp");
    std::fs::write(&tmp, version)?;
    std::fs::rename(tmp, data.join("current"))
}

/// The folder in a version's `bin/<version>/` with the app's built pages, which a running window
/// whose shell has the same [`SHELL`] stamp loads in place of its own (no restart of the app).
pub const UI_DIR: &str = "ui";

/// The file in `bin/<version>/` naming what the app's native shell was built from (its own code
/// and dependencies, not the pages). Two versions with the same stamp have the same shell.
pub const SHELL: &str = "SHELL";

/// The file in a version's folder with the stamp of the Rust code its programs were built from
/// (`update::native_stamp`), so a later version with the same code can keep them.
pub const NATIVE: &str = "NATIVE";

/// The shell stamp of the version in `dir`, when it has one.
pub fn shell_stamp(dir: &Path) -> Option<String> {
    let s = std::fs::read_to_string(dir.join(SHELL)).ok()?;
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// The version of the running program: the VERSION file next to it, or "dev" for a build that
/// was not installed by the updater.
pub fn running_version() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::read_to_string(p.parent()?.join("VERSION")).ok())
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "dev".into())
}
