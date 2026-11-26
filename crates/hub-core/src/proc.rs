//! Starting programs without a console window flashing on Windows. See `hub_install::proc`.
//! Which processes belong to the operating system (`is_system`), so nothing stops one.
//! Also the one runner for the GitHub CLI that waits with a limit (`gh_within`), kept this low so any
//! module can ask GitHub without naming a module above it.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub use hub_install::proc::{CREATE_NO_WINDOW, command_with_console, quiet};

/// A windowless `std::process::Command`. Use this, never `Command::new`.
pub fn command(program: impl AsRef<OsStr>) -> std::process::Command {
    hub_install::proc::command(program)
}

/// A windowless `tokio::process::Command`. Use this, never `tokio::process::Command::new`.
pub fn tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    tokio::process::Command::from(command(program))
}

/// `command_with_console` for tokio: for a program run in the person's own terminal.
pub fn tokio_command_with_console(program: impl AsRef<OsStr>) -> tokio::process::Command {
    tokio::process::Command::from(command_with_console(program))
}

/// Why the GitHub CLI gave no answer: it is not installed, or it failed (its first line of error,
/// or that it took too long).
pub(crate) enum GhError {
    Missing,
    Failed(String),
}

/// Runs the GitHub CLI (`gh`) in `root` and gives what it printed, waiting at most `limit`. It lives
/// down here because everything from the activity map to the project page asks GitHub this way.
pub(crate) async fn gh_within(root: &std::path::Path, args: &[&str], limit: std::time::Duration) -> std::result::Result<String, GhError> {
    let mut cmd = tokio_command("gh");
    cmd.args(args).current_dir(root).kill_on_drop(true).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = match tokio::time::timeout(limit, cmd.output()).await {
        Err(_) => return Err(GhError::Failed("GitHub did not answer in time.".into())),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => return Err(GhError::Missing),
        Ok(Err(e)) => return Err(GhError::Failed(e.to_string())),
        Ok(Ok(out)) => out,
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("gh failed").trim();
        return Err(GhError::Failed(first.to_owned()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Where a program would be found: the PATH, and where Hub's installer puts Node and the user's
/// own tools, which a service's PATH often lacks.
pub fn find_program(program: &str) -> Option<PathBuf> {
    if program.is_empty() {
        return None;
    }
    let given = Path::new(program);
    if given.components().count() > 1 {
        return given.is_file().then(|| given.to_path_buf());
    }
    let home = crate::home_files::home_dir().unwrap_or_else(|| "/".into());
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share"));
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend([home.join(".local/bin"), data.join("hub/tools/bin"), data.join("hub/node/bin"), home.join(".npm-global/bin")]);
    let names: Vec<String> = if cfg!(windows) && given.extension().is_none() {
        ["exe", "cmd", "bat"].iter().map(|e| format!("{program}.{e}")).collect()
    } else {
        vec![program.to_owned()]
    };
    dirs.iter().flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file())
}

/// Processes that belong to the operating system. Stopping one can take the machine down.
pub(crate) fn is_system(pid: Option<u32>, name: Option<&str>, exe: Option<&str>) -> bool {
    if matches!(pid, Some(0..=4)) {
        return true;
    }
    if cfg!(windows) {
        const NAMES: &[&str] = &[
            "system", "svchost.exe", "lsass.exe", "services.exe", "wininit.exe", "csrss.exe",
            "smss.exe", "spoolsv.exe", "winlogon.exe", "registry", "idle",
        ];
        if name.is_some_and(|n| NAMES.contains(&n.to_ascii_lowercase().as_str())) {
            return true;
        }
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        return exe.is_some_and(|e| e.to_ascii_lowercase().starts_with(&root.to_ascii_lowercase()));
    }
    // Unix: daemons under the system binary folders, and anything we cannot even name.
    name.is_none()
        || exe.is_some_and(|e| ["/sbin/", "/usr/sbin/", "/usr/lib/systemd/"].iter().any(|p| e.starts_with(p)))
}
