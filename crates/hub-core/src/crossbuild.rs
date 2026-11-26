//! Builds hubd on this machine for the Linux machines it reaches, so a Raspberry Pi gets a
//! version in seconds instead of compiling it for an hour, and needs neither Rust nor a GitHub
//! login. `cargo zigbuild` cross-compiles with zig as the C compiler and linker, against glibc
//! 2.31, so the result runs on Debian 11, Raspberry Pi OS and Ubuntu 20.04 and newer.
//!
//! The tools are installed once, after the user said yes: the Rust target (`rustup target add`),
//! `cargo-zigbuild` (`cargo install`), and zig itself in `<data>/zig`, downloaded from
//! ziglang.org and checked against the checksum its index gives. Builds run in a worktree of the
//! updater's clone at the version wanted, in `cross` inside the updater's build folder (so it
//! follows `update.target_dir` and counts toward `max_build_gb`), one at a time, with their output
//! in `cross.log`.

use crate::locked::ResultExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use tokio::process::Command;

/// The zig cargo-zigbuild is known to work with.
const ZIG_VERSION: &str = "0.13.0";
/// The oldest glibc the builds run on.
const GLIBC: &str = "2.31";

/// The build target for what `uname -m` (or `uname -sm`) says, or None where there is no build.
/// Only Linux machines get one: a Mac builds hubd for itself.
pub fn target(uname: &str) -> Option<&'static str> {
    let mut words = uname.split_whitespace();
    let mut arch = words.next().unwrap_or_default();
    if let Some(machine) = words.next() {
        if arch != "Linux" {
            return None;
        }
        arch = machine;
    }
    match arch {
        "x86_64" | "amd64" => Some("x86_64-unknown-linux-gnu"),
        "aarch64" | "arm64" => Some("aarch64-unknown-linux-gnu"),
        "armv7l" | "armv7" => Some("armv7-unknown-linux-gnueabihf"),
        _ => None,
    }
}

/// The folder in the updater's build folder that builds for other machines use.
pub const DIR: &str = "cross";

static BUILDING: AtomicBool = AtomicBool::new(false);

/// Whether a build for another machine is running, so its folder is not trimmed away under it.
pub fn building() -> bool {
    BUILDING.load(Ordering::SeqCst)
}

struct Building;

impl Building {
    fn start() -> Self {
        BUILDING.store(true, Ordering::SeqCst);
        Building
    }
}

impl Drop for Building {
    fn drop(&mut self) {
        BUILDING.store(false, Ordering::SeqCst);
    }
}

/// One build at a time: they share the build folder, and each takes all cores.
#[derive(Default)]
pub struct Builder {
    lock: tokio::sync::Mutex<()>,
}

impl Builder {
    /// What is missing to build for `target`, in words; empty when all is there.
    pub async fn missing(&self, data: &Path, target: &str) -> Vec<String> {
        let mut missing = Vec::new();
        let installed = run(crate::proc::tokio_command("rustup").args(["target", "list", "--installed"])).await.unwrap_or_default();
        if !installed.lines().any(|l| l.trim() == target) {
            missing.push(format!("the Rust target {target}"));
        }
        if run(crate::proc::tokio_command("cargo-zigbuild").arg("--version")).await.is_err() {
            missing.push("cargo-zigbuild".to_owned());
        }
        if zig(data).is_none() {
            missing.push(format!("zig {ZIG_VERSION}"));
        }
        missing
    }

    /// Installs what `missing` names.
    pub async fn install_tools(&self, data: &Path, target: &str) -> Result<()> {
        let _one = self.lock.lock().await;
        let log = data.join("cross.log");
        let installed = run(crate::proc::tokio_command("rustup").args(["target", "list", "--installed"])).await.unwrap_or_default();
        if !installed.lines().any(|l| l.trim() == target) {
            logged(crate::proc::tokio_command("rustup").args(["target", "add", target]), &log).await.context("rustup target add")?;
        }
        if run(crate::proc::tokio_command("cargo-zigbuild").arg("--version")).await.is_err() {
            logged(crate::proc::tokio_command("cargo").args(["install", "cargo-zigbuild", "--locked"]), &log)
                .await
                .context("installing cargo-zigbuild")?;
        }
        if zig(data).is_none() {
            install_zig(data).await.context("installing zig")?;
        }
        Ok(())
    }

    /// hubd `version` for `target`, built in `dir` from a worktree of the clone at `src`.
    pub async fn build(&self, data: &Path, dir: &Path, src: &Path, version: &str, target: &str) -> Result<Vec<u8>> {
        let _one = self.lock.lock().await;
        let _building = Building::start();
        let zig = zig(data).context("zig is not installed")?;
        let tree = dir.join("src");
        let log = data.join("cross.log");
        if !tree.join(".git").exists() {
            crate::locked::clean_dir_async(&tree).await;
            tokio::fs::create_dir_all(dir).await?;
            let pruned = crate::git::output_async(src, &["worktree", "prune"], crate::git::TIMEOUT).await.map_err(anyhow::Error::from);
            pruned.and_then(|o| if o.status.success() { Ok(()) } else { bail!("{}", String::from_utf8_lossy(&o.stderr).trim()) }).warn_on_err("git worktree prune");
            git_logged(src, &["worktree", "add", "--detach", "--force", &tree.display().to_string(), version], &log)
                .await
                .context("making a worktree to build in")?;
        } else {
            git_logged(&tree, &["checkout", "--quiet", "--detach", "--force", version], &log)
                .await
                .with_context(|| format!("checking out {version}"))?;
        }
        let mut path = std::ffi::OsString::from(zig.parent().context("zig's folder")?);
        path.push(if cfg!(windows) { ";" } else { ":" });
        path.push(std::env::var_os("PATH").unwrap_or_default());
        let target_dir = dir.join("target");
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        let cores_to_use = jobs(free_memory(), cores);
        let mut cmd = crate::proc::tokio_command("cargo");
        cmd.args(["zigbuild", "--locked", "--release", "-p", "hubd", "--target", &format!("{target}.{GLIBC}")])
            .arg("--target-dir")
            .arg(&target_dir)
            .current_dir(&tree)
            .env("PATH", path)
            .env("CARGO_ZIGBUILD_ZIG_PATH", &zig)
            .env("CARGO_INCREMENTAL", "0")
            .env("CARGO_BUILD_JOBS", cores_to_use.to_string());
        if let Err(e) = logged(&mut cmd, &log).await {
            let why = failure_in(&log).map(|w| format!(": {w}")).unwrap_or_default();
            return Err(e).with_context(|| format!("building hubd {version} for {target}{why}; see cross.log"));
        }
        let built = target_dir.join(target).join("release").join("hubd");
        tokio::fs::read(&built).await.with_context(|| format!("reading {}", built.display()))
    }
}

/// Memory one parallel rustc job of hubd needs at most (hub-core is the big one).
const GB_PER_JOB: u64 = 3;
/// Most parallel jobs a build for another machine takes: it runs beside whatever else the machine does.
const MAX_JOBS: usize = 4;

/// How many parallel jobs a cross build gets: what `free` bytes of memory feed at `GB_PER_JOB`
/// each, at least one, at most `MAX_JOBS` and the `cores`. The machine's own update build is not
/// held back by this: it waits for its own build folder, and a cross build is only started while
/// it is not building (`ship.rs`, `push_updates`), so the two seldom meet; if they do, this cap is
/// what keeps the memory from running out.
fn jobs(free: u64, cores: usize) -> usize {
    ((free >> 30) / GB_PER_JOB).clamp(1, MAX_JOBS as u64).min(cores.max(1) as u64) as usize
}

fn free_memory() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.available_memory()
}

/// Whether `why` (a build's failure text) says the machine ran out of memory, which passes by
/// itself and so is waited out instead of counted as a failed send.
pub fn out_of_memory(why: &str) -> bool {
    ["out of memory", "memory allocation of", "Allocation failed"].iter().any(|m| why.contains(m))
}

/// What the end of `log` says went wrong: the linker's own error line when there is one ("LLVM
/// ERROR: out of memory" is what a machine short of memory gives), else the last `error` line.
fn failure_in(log: &Path) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(log).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(16 * 1024))).ok()?;
    let mut tail = String::new();
    file.read_to_string(&mut tail).ok()?;
    failure_line(&tail)
}

fn failure_line(tail: &str) -> Option<String> {
    let line = |needle: &str| tail.lines().rev().map(str::trim).find(|l| l.contains(needle) && l.len() < 300);
    let found = line("LLVM ERROR").or_else(|| line("memory allocation of")).or_else(|| line("error[")).or_else(|| line("error:"))?;
    Some(found.trim_start_matches("= note: ").to_owned())
}

/// zig in `<data>/zig`, as `install_zig` unpacks it.
fn zig(data: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "zig.exe" } else { "zig" };
    std::fs::read_dir(data.join("zig")).ok()?.flatten().map(|e| e.path().join(exe)).find(|p| p.is_file())
}

/// zig's own name for this machine in its download index.
fn zig_host() -> Option<&'static str> {
    Some(match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => "x86_64-windows",
        ("aarch64", "windows") => "aarch64-windows",
        ("x86_64", "linux") => "x86_64-linux",
        ("aarch64", "linux") => "aarch64-linux",
        ("x86_64", "macos") => "x86_64-macos",
        ("aarch64", "macos") => "aarch64-macos",
        _ => return None,
    })
}

async fn install_zig(data: &Path) -> Result<()> {
    let host = zig_host().context("no zig for this machine")?;
    let client = reqwest::Client::builder().user_agent("hubd").timeout(Duration::from_secs(600)).build()?;
    let index: serde_json::Value =
        client.get("https://ziglang.org/download/index.json").send().await?.error_for_status()?.json().await?;
    let entry = &index[ZIG_VERSION][host];
    let (Some(url), Some(sum)) = (entry["tarball"].as_str(), entry["shasum"].as_str()) else {
        bail!("ziglang.org has no zig {ZIG_VERSION} for {host}");
    };
    let bytes = client.get(url).send().await?.error_for_status()?.bytes().await?;
    if hex(&Sha256::digest(&bytes)) != sum.to_ascii_lowercase() {
        bail!("the zig download does not match its checksum");
    }
    let dir = data.join("zig");
    crate::locked::clean_dir_async(&dir).await;
    tokio::fs::create_dir_all(&dir).await?;
    let archive = dir.join(url.rsplit('/').next().unwrap_or("zig.archive"));
    tokio::fs::write(&archive, &bytes).await?;
    // tar reads the .tar.xz on Linux and macOS, and (bsdtar) the .zip on Windows 10 and newer.
    let out = crate::proc::tokio_command("tar").arg("-xf").arg(&archive).current_dir(&dir).output().await.context("running tar")?;
    if !out.status.success() {
        bail!("unpacking zig: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    crate::locked::clean_file_async(&archive).await;
    zig(data).map(drop).context("zig is not where it was unpacked")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Runs a short command and gives back what it printed, or an error when it failed.
async fn run(cmd: &mut Command) -> Result<String> {
    quiet(cmd);
    let out = tokio::time::timeout(Duration::from_secs(60), cmd.output()).await.context("timed out")??;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs a long command with its output appended to `log`.
async fn logged(cmd: &mut Command, log: &Path) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(file, "\n$ {:?}", cmd.as_std())?;
    quiet(cmd);
    cmd.stdout(file.try_clone()?).stderr(file);
    let status = cmd.status().await?;
    if !status.success() {
        bail!("{status}");
    }
    Ok(())
}

/// [`logged`] for git, run by git.rs under its long limit (a checkout of a whole tree), with what
/// git printed appended to `log` once it is done.
async fn git_logged(dir: &Path, args: &[&str], log: &Path) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
    writeln!(file, "\n$ git -C {} {}", dir.display(), args.join(" "))?;
    let out = crate::git::output_async(dir, args, crate::git::LONG).await?;
    file.write_all(&out.stdout)?;
    file.write_all(&out.stderr)?;
    if !out.status.success() {
        bail!("{}", out.status);
    }
    Ok(())
}

fn quiet(cmd: &mut Command) {
    cmd.stdin(std::process::Stdio::null()).kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_machine_gets_its_own_build() {
        assert_eq!(target("aarch64\n"), Some("aarch64-unknown-linux-gnu"));
        assert_eq!(target("armv7l"), Some("armv7-unknown-linux-gnueabihf"));
        assert_eq!(target("x86_64"), Some("x86_64-unknown-linux-gnu"));
        assert_eq!(target("armv6l"), None);
        assert_eq!(target("Linux aarch64\n"), Some("aarch64-unknown-linux-gnu"));
        // A Mac builds for itself, whether Intel or Apple silicon.
        assert_eq!(target("Darwin x86_64\n"), None);
        assert_eq!(target("Darwin arm64"), None);
    }

    #[test]
    fn the_failure_names_what_the_linker_said() {
        let tail = "   Compiling x\nerror: linking with `zigcc` failed: exit code: 9\n  = note: LLVM ERROR: out of memory\n\nerror: could not compile `hubd`\n";
        assert_eq!(failure_line(tail).as_deref(), Some("LLVM ERROR: out of memory"));
        assert_eq!(failure_line("error[E0425]: cannot find function\n").as_deref(), Some("error[E0425]: cannot find function"));
        assert_eq!(failure_line("all fine"), None);
        let abort = "memory allocation of 2097152 bytes failed\nerror: could not compile `hub-core`\n";
        assert_eq!(failure_line(abort).as_deref(), Some("memory allocation of 2097152 bytes failed"));
    }

    #[test]
    fn memory_failures_are_told_apart_and_jobs_follow_memory() {
        assert!(out_of_memory("building hubd x for y: rustc-LLVM ERROR: out of memory; see cross.log"));
        assert!(out_of_memory("memory allocation of 2097152 bytes failed"));
        assert!(out_of_memory("Allocation failed"));
        assert!(!out_of_memory("error[E0425]: cannot find function"));
        let gb = 1u64 << 30;
        assert_eq!(jobs(2 * gb, 16), 1);
        assert_eq!(jobs(7 * gb, 16), 2);
        assert_eq!(jobs(64 * gb, 16), MAX_JOBS);
        assert_eq!(jobs(64 * gb, 2), 2);
        assert_eq!(jobs(0, 0), 1);
    }

    #[test]
    fn zig_is_found_where_it_was_unpacked() {
        let data = tempfile::tempdir().unwrap();
        assert!(zig(data.path()).is_none());
        let dir = data.path().join("zig").join(format!("zig-linux-x86_64-{ZIG_VERSION}"));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(if cfg!(windows) { "zig.exe" } else { "zig" });
        std::fs::write(&exe, "").unwrap();
        assert_eq!(zig(data.path()), Some(exe));
    }
}
