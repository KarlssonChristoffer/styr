//! What a project has running and what it uses, worked out without anyone tagging anything.
//!
//! Owned: every engine is started with `HUB_THREAD_ID` and `HUB_PROJECT` in its environment, and
//! every process it starts (dev servers, builds, shells) inherits them. A process that carries
//! them, or that runs inside the project folder, belongs to the project. Docker containers belong
//! to it when compose started them from the project folder.
//!
//! Used: a server the project's processes are connected to but did not start, e.g. an Aftermath
//! instance. It is shown, never stopped.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use ts_rs::TS;

use crate::model::Thread;
pub use crate::model::{Container};

use crate::process_marks::{PROJECT_VAR, THREAD_VAR};
/// The name Aftermath reads from the engine's environment to know which piece of work a session
/// belongs to. Set for a development thread only (see `acp.rs`); Aftermath never guesses one.
pub const WORK_REF_VAR: &str = "AFTERMATH_WORK_REF";

/// The work name of a thread for Aftermath: `work://hub/<thread id>`, so the thread is found from
/// it and nothing about the work is stored twice.
pub fn work_ref(thread_id: &str) -> String {
    format!("work://hub/{thread_id}")
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ProjectRuntime {
    /// Processes of the project that listen on a port.
    pub servers: Vec<OwnedProcess>,
    /// Other processes of the project (shells, builds, watchers), only counted.
    #[ts(type = "number")]
    pub other_processes: u32,
    pub containers: Vec<Container>,
    /// Servers the project talks to but did not start.
    pub uses: Vec<UsedService>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct OwnedProcess {
    #[ts(type = "number")]
    pub pid: u32,
    pub name: String,
    /// The command line, shortened.
    pub command: String,
    #[ts(type = "Array<number>")]
    pub ports: Vec<u16>,
    /// The thread that started it, when it carries one.
    pub thread_id: Option<String>,
    /// The project service it is, when Start all started it (see `services.rs`).
    pub service: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct UsedService {
    #[ts(type = "number")]
    pub port: u16,
    #[ts(type = "number | null")]
    pub pid: Option<u32>,
    pub process: Option<String>,
    /// Where the server runs, so the page can name the project it belongs to.
    pub cwd: Option<String>,
}


/// Why a process belongs to the project, if it does.
fn owner(
    environ: &[std::ffi::OsString],
    cwd: Option<&Path>,
    root: &Path,
    threads: &HashSet<&str>,
) -> Option<Option<String>> {
    let mut thread = None;
    let mut project = None;
    for var in environ {
        let var = var.to_string_lossy();
        if let Some(v) = var.strip_prefix(THREAD_VAR).and_then(|r| r.strip_prefix('=')) {
            thread = Some(v.to_owned());
        } else if let Some(v) = var.strip_prefix(PROJECT_VAR).and_then(|r| r.strip_prefix('=')) {
            project = Some(v.to_owned());
        }
    }
    if project.as_deref().is_some_and(|p| same_or_under(Path::new(p), root))
        || thread.as_deref().is_some_and(|t| threads.contains(t))
    {
        return Some(thread);
    }
    // Started some other way, e.g. by hand in a terminal, but inside the project folder.
    if project.is_none() && cwd.is_some_and(|c| same_or_under(c, root)) {
        return Some(None);
    }
    None
}

fn service_of(environ: &[std::ffi::OsString]) -> Option<String> {
    environ.iter().find_map(|v| v.to_string_lossy().strip_prefix(crate::process_marks::SERVICE_VAR_EQ).map(str::to_owned))
}

/// `path` is `root` or inside it. Windows paths compare without case.
pub(crate) fn same_or_under(path: &Path, root: &Path) -> bool {
    if cfg!(windows) {
        let lower = |p: &Path| std::path::PathBuf::from(p.to_string_lossy().to_lowercase().replace('/', "\\"));
        lower(path).starts_with(lower(root))
    } else {
        path.starts_with(root)
    }
}

struct Sockets {
    /// Listening port to the process behind it.
    listening: HashMap<u16, Option<u32>>,
    /// (pid, remote port) of connections to this machine.
    connections: Vec<(u32, u16)>,
}

fn sockets() -> Sockets {
    let mut out = Sockets { listening: HashMap::new(), connections: Vec::new() };
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let Ok(all) = netstat2::get_sockets_info(af, ProtocolFlags::TCP) else { return out };
    for s in all {
        let ProtocolSocketInfo::Tcp(tcp) = s.protocol_socket_info else { continue };
        let pid = s.associated_pids.first().copied();
        match tcp.state {
            TcpState::Listen => {
                out.listening.entry(tcp.local_port).or_insert(pid);
            }
            TcpState::Established if tcp.remote_addr.is_loopback() || tcp.remote_addr == tcp.local_addr => {
                if let Some(pid) = pid {
                    out.connections.push((pid, tcp.remote_port));
                }
            }
            _ => {}
        }
    }
    out
}

/// The processes of the project at `root`, and the servers they use.
pub fn processes(root: &Path, threads: &[Thread]) -> ProjectRuntime {
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .with_exe(UpdateKind::Always)
            .with_cmd(UpdateKind::Always)
            .with_environ(UpdateKind::Always),
    );
    let ids: HashSet<&str> = threads.iter().filter(|t| same_or_under(Path::new(&t.cwd), root)).map(|t| t.id.as_str()).collect();
    let hub = Pid::from_u32(std::process::id());

    let mut owned: BTreeMap<u32, Option<String>> = BTreeMap::new();
    for (pid, p) in sys.processes() {
        // Hub and the engines it runs are not the project's own processes (the services it starts
        // are), and on Linux the threads of a process are listed as processes too.
        let service = service_of(p.environ());
        if *pid == hub || (p.parent() == Some(hub) && service.is_none()) || p.thread_kind().is_some() {
            continue;
        }
        let name = p.name().to_string_lossy();
        let exe = p.exe().map(|e| e.to_string_lossy().into_owned());
        if crate::proc::is_system(Some(pid.as_u32()), Some(&name), exe.as_deref()) {
            continue;
        }
        if let Some(thread) = owner(p.environ(), p.cwd(), root, &ids) {
            owned.insert(pid.as_u32(), thread);
        }
    }

    let net = sockets();
    let mut ports: HashMap<u32, Vec<u16>> = HashMap::new();
    for (port, pid) in &net.listening {
        if let Some(pid) = pid {
            ports.entry(*pid).or_default().push(*port);
        }
    }

    let mut servers = Vec::new();
    let mut other = 0;
    for (pid, thread) in &owned {
        let Some(mut list) = ports.get(pid).cloned() else {
            other += 1;
            continue;
        };
        list.sort_unstable();
        let p = sys.process(Pid::from_u32(*pid));
        let command = p
            .map(|p| p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        servers.push(OwnedProcess {
            pid: *pid,
            name: p.map(|p| p.name().to_string_lossy().into_owned()).unwrap_or_default(),
            command: crate::text::clip(&command, 160),
            ports: list,
            thread_id: thread.clone(),
            service: p.and_then(|p| service_of(p.environ())),
            cwd: p.and_then(|p| p.cwd()).map(|c| c.display().to_string()),
        });
    }
    servers.sort_by_key(|s| s.ports.first().copied().unwrap_or(u16::MAX));

    let mut seen = HashSet::new();
    let mut uses = Vec::new();
    for (pid, remote_port) in &net.connections {
        if !owned.contains_key(pid) || !seen.insert(*remote_port) {
            continue;
        }
        let Some(server) = net.listening.get(remote_port) else { continue };
        if server.is_some_and(|s| owned.contains_key(&s) || s == hub.as_u32()) {
            continue;
        }
        let p = server.and_then(|s| sys.process(Pid::from_u32(s)));
        uses.push(UsedService {
            port: *remote_port,
            pid: *server,
            process: p.map(|p| p.name().to_string_lossy().into_owned()),
            cwd: p.and_then(|p| p.cwd()).map(|c| c.display().to_string()),
        });
    }
    uses.sort_by_key(|u| u.port);

    ProjectRuntime { servers, other_processes: other, containers: Vec::new(), uses }
}

/// Running containers that docker compose started from the project folder. Empty when docker is
/// missing, stopped or slow.
pub async fn containers(root: &Path) -> Vec<Container> {
    let mut all = all_containers().await;
    all.retain(|c| c.project.as_deref().is_some_and(|dir| same_or_under(Path::new(dir), root)));
    all
}

/// Every running container. Empty when docker is missing, stopped or slow.
pub async fn all_containers() -> Vec<Container> {
    let mut cmd = crate::proc::tokio_command("docker");
    cmd.args(["ps", "--format", "{{json .}}"]).kill_on_drop(true).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(4), cmd.output()).await else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_containers(&String::from_utf8_lossy(&out.stdout))
}

fn parse_containers(text: &str) -> Vec<Container> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .map(|c| {
            let s = |k: &str| c[k].as_str().unwrap_or_default().to_owned();
            let project = c["Labels"]
                .as_str()
                .unwrap_or_default()
                .split(',')
                .find_map(|l| l.strip_prefix("com.docker.compose.project.working_dir="))
                .map(str::to_owned);
            Container { id: s("ID"), name: s("Names"), image: s("Image"), status: s("Status"), ports: s("Ports"), project }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn the_work_name_for_aftermath_follows_from_the_thread() {
        assert_eq!(work_ref("01a1-b2"), "work://hub/01a1-b2");
        assert_eq!(WORK_REF_VAR, "AFTERMATH_WORK_REF");
    }

    #[test]
    fn owners_come_from_the_environment_or_the_folder() {
        let root = Path::new("/dev/game");
        let ids: HashSet<&str> = ["t1"].into_iter().collect();
        let env = |vars: &[&str]| vars.iter().map(OsString::from).collect::<Vec<_>>();

        // Started by a thread of the project, wherever it runs.
        let started = owner(&env(&["HUB_THREAD_ID=t1", "HUB_PROJECT=/dev/game"]), Some(Path::new("/tmp")), root, &ids);
        assert_eq!(started, Some(Some("t1".into())));
        // Started by a thread of another project, even inside this folder: not ours.
        let other = owner(&env(&["HUB_THREAD_ID=t9", "HUB_PROJECT=/dev/aftermath"]), Some(Path::new("/dev/game/sub")), root, &ids);
        assert_eq!(other, None);
        // Started by hand inside the folder.
        assert_eq!(owner(&env(&["PATH=/bin"]), Some(Path::new("/dev/game/server")), root, &ids), Some(None));
        // Unrelated.
        assert_eq!(owner(&env(&[]), Some(Path::new("/dev/gamer")), root, &ids), None);
    }

    #[test]
    fn compose_containers_from_the_folder() {
        let out = concat!(
            r#"{"ID":"a1","Names":"game-db-1","Image":"postgres:16","Status":"Up 2 hours","Ports":"0.0.0.0:5432->5432/tcp","Labels":"com.docker.compose.project=game,com.docker.compose.project.working_dir=/dev/game"}"#,
            "\n",
            r#"{"ID":"b2","Names":"aftermath","Image":"aftermath","Status":"Up","Ports":"","Labels":"com.docker.compose.project.working_dir=/dev/aftermath"}"#,
        );
        let mut found = parse_containers(out);
        assert_eq!(found.len(), 2);
        found.retain(|c| c.project.as_deref().is_some_and(|dir| same_or_under(Path::new(dir), Path::new("/dev/game"))));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "game-db-1");
    }
}
