//! Finding and fetching whisper.cpp: where its binaries sit in the release zips under
//! `<data_dir>/voice`, downloads with curl, and running its tools without a console window, with
//! the last line of error as the error.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result, bail};

/// whisper-cli's location in the release zip has moved between versions, so search for it.
/// Older releases call it `main`; newer ones keep a `main` that only prints a deprecation.
pub(super) fn find_cli(dir: &Path) -> Option<PathBuf> {
    let names = if cfg!(windows) { ["whisper-cli.exe", "main.exe"] } else { ["whisper-cli", "main"] };
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for path in entries.flatten().map(|e| e.path()) {
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    names.iter().find_map(|name| files.iter().find(|p| p.file_name().is_some_and(|n| n == *name)).cloned())
}

/// whisper-server ships next to whisper-cli in the release zip.
pub(super) fn server_binary(cli: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "whisper-server.exe" } else { "whisper-server" };
    Some(cli.parent()?.join(name)).filter(|p| p.is_file())
}

/// Files called `name` under `dir`, a few levels deep (the release zips nest their binaries).
pub(super) fn walk(dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&d) else { continue };
        for e in read.flatten() {
            let p = e.path();
            if p.is_dir() && depth < 4 {
                stack.push((p, depth + 1));
            } else if e.file_name() == name {
                found.push(p);
            }
        }
    }
    found
}

pub(super) async fn fetch(url: &str, to: &Path) -> Result<()> {
    // curl ships with Windows 10 and later, so no HTTP client is needed in the core.
    let mut cmd = crate::proc::tokio_command("curl");
    cmd.args(["-L", "--fail", "--silent", "--show-error", "-o"]).arg(to).arg(url);
    run(&mut cmd).await.with_context(|| format!("downloading {url}"))?;
    Ok(())
}

pub(super) async fn run(cmd: &mut tokio::process::Command) -> Result<String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = cmd.output().await.context("starting a voice tool")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
