//! Starting and stopping everything a project runs, from its page.
//!
//! What a project starts is its `Procfile.dev` (or `Procfile`): one `name: command` per line, the
//! format foreman, honcho and overmind read, so agents know it and it works without Hub. Without
//! one, Hub guesses from the manifests (`npm run dev`, `cargo run`, ...) and can save the guess as
//! `Procfile.dev`. A docker compose file is brought up too.
//!
//! Services run outside Hub's own process tree and outlive the core's restarts for updates. Each
//! carries `HUB_PROJECT` and `HUB_SERVICE` in its environment, which is how Hub finds them again,
//! as `runtime.rs` finds what threads started. Output goes to `<data dir>/services/`, one log per
//! service, emptied at each start and again when it passes [`LOG_CAP`].
//!
//! Stop all also shuts down the engines of the project's threads and whatever those started,
//! except threads open in a terminal (that half is `hub/services.rs`). Processes started by hand in
//! the folder are left alone.

use crate::locked::ResultExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::model::{ProjectServices, ServiceInfo, ServicesDone};
use crate::process_marks::{PROJECT_VAR, SERVICE_VAR, SERVICE_VAR_EQ, THREAD_VAR};
use crate::runtime::same_or_under;

/// A service log is emptied when it grows past this.
const LOG_CAP: u64 = 20 * 1024 * 1024;

const PROCFILES: [&str; 2] = ["Procfile.dev", "Procfile"];
const COMPOSE: [&str; 4] = ["compose.yaml", "compose.yml", "docker-compose.yml", "docker-compose.yaml"];

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceDef {
    pub name: String,
    pub command: String,
}

/// `name: command` lines; blank lines and `#` comments are skipped.
pub fn parse_procfile(text: &str) -> Vec<ServiceDef> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (name, command) = line.split_once(':')?;
            let name = name.trim();
            let ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            (ok && !command.trim().is_empty()).then(|| ServiceDef { name: name.to_owned(), command: command.trim().to_owned() })
        })
        .collect()
}

/// What a folder probably runs, from its manifests: the folder itself and the folders right under it.
pub fn guess(root: &Path) -> Vec<ServiceDef> {
    let mut out = Vec::new();
    let mut dirs = vec![(root.to_path_buf(), None)];
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut subs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| !n.starts_with('.') && !["node_modules", "target", "dist", "build", "vendor"].contains(&n)))
            .collect();
        subs.sort();
        dirs.extend(subs.into_iter().filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            Some((p, Some(name)))
        }));
    }
    for (dir, sub) in dirs {
        let name = |default: &str| sub.clone().unwrap_or_else(|| default.to_owned());
        if let Ok(text) = std::fs::read_to_string(dir.join("package.json"))
            && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&text)
        {
            let script = ["dev", "start"].into_iter().find(|s| pkg["scripts"][s].is_string());
            if let Some(script) = script {
                let command = match &sub {
                    Some(sub) => format!("npm --prefix {sub} run {script}"),
                    None => format!("npm run {script}"),
                };
                out.push(ServiceDef { name: name(if script == "dev" { "web" } else { "app" }), command });
            }
        }
        if sub.is_none() && dir.join("Cargo.toml").is_file() && dir.join("src/main.rs").is_file() {
            out.push(ServiceDef { name: "server".into(), command: "cargo run".into() });
        }
        if sub.is_none() && dir.join("manage.py").is_file() {
            out.push(ServiceDef { name: "web".into(), command: "python manage.py runserver".into() });
        }
    }
    // Names are the keys; the first of each wins.
    let mut seen = std::collections::HashSet::new();
    out.retain(|s| seen.insert(s.name.clone()));
    out
}

/// The services of the project at `root`, and the Procfile they come from (None for guesses).
pub fn defined(root: &Path) -> (Vec<ServiceDef>, Option<String>) {
    for file in PROCFILES {
        if let Ok(text) = std::fs::read_to_string(root.join(file)) {
            return (parse_procfile(&text), Some(file.to_owned()));
        }
    }
    (guess(root), None)
}

pub fn compose_file(root: &Path) -> Option<String> {
    COMPOSE.iter().find(|f| root.join(f).is_file()).map(|f| f.to_string())
}

/// Where a project's service logs go: the folder's name and a hash of its path, so two folders
/// with the same name do not share logs.
fn log_dir(data_dir: &Path, root: &Path) -> PathBuf {
    let path = root.display().to_string();
    let path = if cfg!(windows) { path.to_lowercase() } else { path };
    let hash = path.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
    let name: String = root
        .file_name()
        .map(|n| n.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect())
        .unwrap_or_default();
    data_dir.join("services").join(format!("{name}-{:08x}", hash as u32))
}

fn log_path(data_dir: &Path, root: &Path, name: &str) -> PathBuf {
    log_dir(data_dir, root).join(format!("{name}.log"))
}

/// Running copies of each service: the top pid of each and the ports any of their processes
/// listen on.
fn running(root: &Path) -> std::collections::HashMap<String, (Vec<u32>, Vec<u16>)> {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_environ(UpdateKind::Always),
    );
    let mut service_of = std::collections::HashMap::new();
    for (pid, p) in sys.processes() {
        if p.thread_kind().is_some() || matches!(p.status(), sysinfo::ProcessStatus::Zombie | sysinfo::ProcessStatus::Dead) {
            continue;
        }
        let mut service = None;
        let mut project = None;
        for var in p.environ() {
            let var = var.to_string_lossy();
            if let Some(v) = var.strip_prefix(SERVICE_VAR_EQ) {
                service = Some(v.to_owned());
            } else if let Some(v) = var.strip_prefix(PROJECT_VAR).and_then(|r| r.strip_prefix('=')) {
                project = Some(v.to_owned());
            }
        }
        if let (Some(service), Some(project)) = (service, project)
            && same_or_under(Path::new(&project), root)
            && same_or_under(root, Path::new(&project))
        {
            service_of.insert(pid.as_u32(), (service, p.parent().map(|p| p.as_u32())));
        }
    }
    let ports = crate::workers::listening();
    let mut out: std::collections::HashMap<String, (Vec<u32>, Vec<u16>)> = Default::default();
    for (pid, (service, parent)) in &service_of {
        let entry = out.entry(service.clone()).or_default();
        let top = !parent.is_some_and(|p| service_of.get(&p).is_some_and(|(s, _)| s == service));
        if top {
            entry.0.push(*pid);
        }
        entry.1.extend(ports.get(pid).into_iter().flatten());
    }
    for (pids, ports) in out.values_mut() {
        pids.sort_unstable();
        ports.sort_unstable();
        ports.dedup();
    }
    out
}

/// The project's services with what of them is running.
pub fn list(root: &Path, data_dir: &Path) -> ProjectServices {
    let (defs, procfile) = defined(root);
    let mut live = running(root);
    let mut services: Vec<ServiceInfo> = defs
        .into_iter()
        .map(|d| {
            let (pids, ports) = live.remove(&d.name).unwrap_or_default();
            ServiceInfo { log: log_path(data_dir, root, &d.name).display().to_string(), name: d.name, command: d.command, pids, ports }
        })
        .collect();
    // Still running after its line was removed or renamed.
    for (name, (pids, ports)) in live {
        services.push(ServiceInfo { log: log_path(data_dir, root, &name).display().to_string(), command: String::new(), name, pids, ports });
    }
    for s in &services {
        let log = Path::new(&s.log);
        if std::fs::metadata(log).is_ok_and(|m| m.len() > LOG_CAP)
            && let Ok(f) = std::fs::OpenOptions::new().write(true).open(log)
        {
            f.set_len(0).warn_on_err("truncate the service log");
        }
    }
    ProjectServices { services, procfile, compose: compose_file(root) }
}

/// A command run through the shell, so `npm`, `.cmd` scripts and pipes work as typed.
fn shell(command: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = crate::proc::command("cmd");
        c.arg("/C").raw_arg(command);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = crate::proc::command("sh");
        c.arg("-c").arg(command);
        c
    }
}

/// `program args` in a systemd scope of its own when the core runs as a systemd service, so a
/// restart of the service does not take it along.
pub(crate) fn outside_service(program: &str, args: &[String]) -> std::process::Command {
    let systemd = cfg!(target_os = "linux")
        && std::env::var_os("INVOCATION_ID").is_some()
        && crate::proc::command("systemd-run")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
    if systemd {
        let mut c = crate::proc::command("systemd-run");
        c.args(["--user", "--scope", "--quiet", "--collect", "--"]).arg(program).args(args);
        c
    } else {
        let mut c = crate::proc::command(program);
        c.args(args);
        c
    }
}

/// Starts one service unless it runs already.
pub fn start(root: &Path, data_dir: &Path, service: &ServiceDef) -> Result<bool> {
    if running(root).get(&service.name).is_some_and(|(pids, _)| !pids.is_empty()) {
        return Ok(false);
    }
    let log = log_path(data_dir, root, &service.name);
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::fs::File::create(&log).with_context(|| format!("creating {}", log.display()))?;
    let err = out.try_clone()?;
    let mut cmd = if cfg!(windows) { shell(&service.command) } else { outside_service("sh", &["-c".into(), service.command.clone()]) };
    // A group of its own: signals for the core's group do not reach it.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    cmd.current_dir(root)
        .env(PROJECT_VAR, root)
        .env(SERVICE_VAR, &service.name)
        .env_remove(THREAD_VAR)
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err);
    #[cfg(windows)]
    let child = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        // Out of any job the core runs in, when that job allows it.
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        match cmd.spawn() {
            Ok(child) => Ok(child),
            Err(_) => cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP).spawn(),
        }
        .with_context(|| format!("starting {}", service.name))?
    };
    #[cfg(not(windows))]
    let child = cmd.spawn().with_context(|| format!("starting {}", service.name))?;
    // Collected when it exits, so it does not linger as a zombie.
    let mut child = child;
    std::thread::spawn(move || child.wait());
    Ok(true)
}

/// Stops every running copy of a service, with everything it started.
pub fn stop(root: &Path, name: &str) -> Result<bool> {
    let Some((pids, _)) = running(root).remove(name) else { return Ok(false) };
    for pid in &pids {
        if let Err(e) = crate::workers::act(*pid, crate::workers::Action::Stop) {
            // A copy that exited between the scan and the stop is already stopped.
            if running(root).get(name).is_some_and(|(left, _)| left.contains(pid)) {
                return Err(e);
            }
        }
    }
    Ok(!pids.is_empty())
}

/// The end of a service's log.
pub fn log_tail(root: &Path, data_dir: &Path, name: &str) -> Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL: u64 = 64 * 1024;
    let path = log_path(data_dir, root, name);
    let Ok(mut file) = std::fs::File::open(&path) else { return Ok(String::new()) };
    let len = file.metadata()?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(TAIL)))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    // Cut at a line when the start was cut.
    Ok(if len > TAIL { text.split_once('\n').map(|(_, rest)| rest.to_owned()).unwrap_or(text) } else { text })
}

/// Writes the guessed services as `Procfile.dev`, so they can be edited.
pub fn save_guess(root: &Path) -> Result<String> {
    if PROCFILES.iter().any(|f| root.join(f).exists()) {
        bail!("the project has a Procfile already");
    }
    let guessed = guess(root);
    if guessed.is_empty() {
        bail!("nothing to guess from; write a Procfile.dev with `name: command` lines");
    }
    let mut text = String::from("# What Hub's Start all runs: one `name: command` per line.\n");
    for s in guessed {
        text.push_str(&format!("{}: {}\n", s.name, s.command));
    }
    std::fs::write(root.join("Procfile.dev"), text)?;
    Ok("Procfile.dev".into())
}

/// `docker compose <args>` in the folder, given a few minutes.
async fn compose(root: &Path, args: &[&str]) -> Result<()> {
    let mut cmd = crate::proc::tokio_command("docker");
    cmd.arg("compose").args(args).current_dir(root).kill_on_drop(true).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = tokio::time::timeout(Duration::from_secs(300), cmd.output())
        .await
        .context("docker compose took too long")?
        .context("docker is not installed")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("docker compose {}: {}", args[0], err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed"));
    }
    Ok(())
}

/// What the project runs: its services and their state, and whether compose is up.
pub async fn listed(root: &str, data_dir: &Path) -> Result<ProjectServices> {
    let (root, data) = (PathBuf::from(root), data_dir.to_path_buf());
    Ok(tokio::task::spawn_blocking(move || list(&root, &data)).await?)
}

/// Starts one service, or with no name everything: compose and every service.
pub async fn start_named(root: &str, data_dir: &Path, name: Option<&str>) -> Result<ServicesDone> {
    let root = PathBuf::from(root);
    if !root.is_dir() {
        bail!("{} is not a folder", root.display());
    }
    let mut done = ServicesDone::default();
    if name.is_none() && compose_file(&root).is_some() {
        match compose(&root, &["up", "-d"]).await {
            Ok(()) => done.compose = true,
            Err(err) => done.errors.push(format!("{err:#}")),
        }
    }
    let (defs, _) = defined(&root);
    let picked: Vec<ServiceDef> = defs.into_iter().filter(|d| name.is_none_or(|n| n == d.name)).collect();
    if let Some(n) = name
        && picked.is_empty()
    {
        bail!("the project has no service called {n}");
    }
    let data = data_dir.to_path_buf();
    let started = tokio::task::spawn_blocking(move || {
        picked.into_iter().map(|d| (d.name.clone(), start(&root, &data, &d))).collect::<Vec<_>>()
    })
    .await?;
    for (name, result) in started {
        match result {
            Ok(true) => done.services.push(name),
            Ok(false) => {}
            Err(err) => done.errors.push(format!("{name}: {err:#}")),
        }
    }
    Ok(done)
}

/// Stops one service, or with no name every service and compose. The engines of the project's
/// threads are Hub's to stop (`Hub::stop_services` in `hub/services.rs` calls this first).
pub async fn stop_named(root: &Path, name: Option<&str>) -> Result<ServicesDone> {
    let mut done = ServicesDone::default();
    let names: Vec<String> = match name {
        Some(n) => vec![n.to_owned()],
        None => {
            let at = root.to_path_buf();
            tokio::task::spawn_blocking(move || running(&at).into_keys().collect()).await?
        }
    };
    let at = root.to_path_buf();
    let stopped = tokio::task::spawn_blocking(move || names.into_iter().map(|n| (n.clone(), stop(&at, &n))).collect::<Vec<_>>()).await?;
    for (name, result) in stopped {
        match result {
            Ok(true) => done.services.push(name),
            Ok(false) => {}
            Err(err) => done.errors.push(format!("{name}: {err:#}")),
        }
    }
    if name.is_none() && compose_file(root).is_some() && !crate::runtime::containers(root).await.is_empty() {
        match compose(root, &["stop"]).await {
            Ok(()) => done.compose = true,
            Err(err) => done.errors.push(format!("{err:#}")),
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procfiles_are_name_colon_command() {
        let text = "# dev\nweb: npm run dev -- --port 3000\n\nworker:cargo run -p worker\nbad line\n: nothing\n";
        assert_eq!(
            parse_procfile(text),
            vec![
                ServiceDef { name: "web".into(), command: "npm run dev -- --port 3000".into() },
                ServiceDef { name: "worker".into(), command: "cargo run -p worker".into() },
            ]
        );
    }

    #[test]
    fn guesses_come_from_manifests_and_can_be_saved() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::write(root.join("app/package.json"), r#"{"scripts":{"dev":"vite"}}"#).unwrap();
        std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
        std::fs::write(root.join("node_modules/x/package.json"), r#"{"scripts":{"dev":"no"}}"#).unwrap();

        let (found, procfile) = defined(root);
        assert_eq!(procfile, None);
        assert_eq!(
            found,
            vec![
                ServiceDef { name: "server".into(), command: "cargo run".into() },
                ServiceDef { name: "app".into(), command: "npm --prefix app run dev".into() },
            ]
        );
        assert_eq!(save_guess(root).unwrap(), "Procfile.dev");
        let (saved, procfile) = defined(root);
        assert_eq!(procfile.as_deref(), Some("Procfile.dev"));
        assert_eq!(saved, found);
        assert!(save_guess(root).is_err(), "an existing Procfile is never replaced");
    }

    /// A service runs outside the core, is found again by its environment, logs to its file and
    /// stops with what it started.
    #[cfg(unix)]
    #[test]
    fn a_service_starts_logs_and_stops() {
        let until = |what: &str, ok: &dyn Fn() -> bool| {
            for _ in 0..100 {
                if ok() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            panic!("never {what}");
        };
        let dir = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let (root, data) = (dir.path(), data.path());
        std::fs::write(root.join("Procfile.dev"), "ticker: echo started; sleep 30 & wait\n").unwrap();
        let (defs, _) = defined(root);
        assert!(start(root, data, &defs[0]).unwrap());
        until("started", &|| log_tail(root, data, "ticker").unwrap().trim() == "started");
        let listed = list(root, data);
        assert_eq!(listed.services.len(), 1);
        assert_eq!(listed.services[0].pids.len(), 1, "one copy, its children under it");
        assert!(!start(root, data, &defs[0]).unwrap(), "a running service is not started twice");

        assert!(stop(root, "ticker").unwrap());
        until("stopped", &|| list(root, data).services[0].pids.is_empty());
    }
}
