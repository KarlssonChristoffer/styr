//! What is running on this machine: listening ports, the processes behind them and what started
//! them: a thread (by the id its processes inherit), Hub itself, or something else.

use std::path::Path;
use std::sync::{LazyLock, Mutex};

use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::model::{DiskUse, HostSnapshot, Listener, ListenerOrigin, MachineAbout, MachineKind, Thread};

// Kept between snapshots: CPU usage is the difference between two refreshes.
static SYSTEM: LazyLock<Mutex<System>> = LazyLock::new(|| Mutex::new(System::new()));

/// `dev_ports` are the ports Hub's own dev mode serves on (`dev::PORT`, `dev::CORE_PORT`), named
/// by the caller so this module stays below `dev`.
pub fn snapshot(threads: &[Thread], dev_ports: &[u16]) -> HostSnapshot {
    let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
    sys.refresh_memory();
    sys.refresh_cpu_usage();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::OnlyIfNotSet)
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_environ(UpdateKind::OnlyIfNotSet),
    );

    let mut listeners: Vec<Listener> = listening_sockets()
        .into_iter()
        .map(|(port, address, protocol, pid)| {
            let process = pid.and_then(|pid| sys.process(Pid::from_u32(pid)));
            let cwd = process.and_then(|p| p.cwd()).map(|c| c.to_string_lossy().into_owned());
            let exe = process.and_then(|p| p.exe()).map(|e| e.to_string_lossy().into_owned());
            let name = process.map(|p| p.name().to_string_lossy().into_owned());
            let system = crate::proc::is_system(pid, name.as_deref(), exe.as_deref());
            let carried = process.and_then(|p| crate::workers::thread_of(p.environ()));
            // System processes run in system folders; matching them to a thread is noise.
            let in_folder =
                if system { None } else { cwd.as_deref().and_then(|cwd| owning_thread(threads, cwd)) };
            let engine = process.is_some_and(|p| pid.is_some_and(|pid| crate::workers::is_engine(pid, p.start_time())));
            let (origin, thread_id) = if engine {
                (ListenerOrigin::Engine, carried)
            } else if let Some(id) = carried {
                (ListenerOrigin::Thread, Some(id))
            } else if dev_ports.contains(&port) {
                (ListenerOrigin::DevMode, None)
            } else if pid.is_some_and(|pid| from_hub(&sys, pid)) {
                (ListenerOrigin::Hub, None)
            } else if in_folder.is_some() {
                (ListenerOrigin::Folder, in_folder)
            } else {
                (ListenerOrigin::Other, None)
            };
            let command = process
                .map(|p| p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" "))
                .filter(|c| !c.is_empty())
                .map(|c| crate::text::clip(&c, 400));
            let parent = process.and_then(|p| p.parent()).map(|ppid| match sys.process(ppid) {
                Some(pp) if pp.start_time() <= process.map_or(0, |p| p.start_time()) => {
                    format!("{} · pid {ppid}", pp.name().to_string_lossy())
                }
                _ => format!("pid {ppid}, exited"),
            });
            Listener {
                port,
                address,
                protocol,
                pid,
                process: name,
                is_hub: pid == Some(std::process::id()),
                system,
                exe,
                cwd,
                thread_id,
                origin,
                command,
                started_at: process.map(|p| p.start_time() as i64 * 1000),
                parent,
            }
        })
        .collect();
    listeners.sort_by_key(|l| (l.port, l.address.clone()));
    listeners.dedup_by(|a, b| a.port == b.port && a.pid == b.pid && a.protocol == b.protocol);

    HostSnapshot {
        hostname: System::host_name().unwrap_or_else(|| "this computer".into()),
        os: System::long_os_version().unwrap_or_default(),
        cpu_percent: sys.global_cpu_usage(),
        mem_used: sys.used_memory(),
        mem_total: sys.total_memory(),
        listeners,
    }
}

/// What this machine is, for its page. Tailscale is asked separately, it can take a moment.
pub async fn about() -> MachineAbout {
    let mut about = tokio::task::spawn_blocking(about_now).await.unwrap_or_else(|_| about_now());
    about.tailscale = tailscale_line().await;
    about
}

fn about_now() -> MachineAbout {
    let (cpu, cores) = {
        let mut sys = SYSTEM.lock().unwrap_or_else(|e| e.into_inner());
        sys.refresh_cpu_usage();
        let brand = sys.cpus().first().map(|c| c.brand().trim().to_owned()).filter(|b| !b.is_empty());
        (brand, sys.cpus().len() as u32)
    };
    let home = crate::folders::home();
    let model = board_model();
    MachineAbout {
        hostname: System::host_name().unwrap_or_else(|| "this computer".into()),
        os: System::long_os_version().unwrap_or_default(),
        kernel: System::kernel_version(),
        arch: System::cpu_arch(),
        kind: Some(machine_kind(model.as_deref())),
        model,
        cpu,
        cores,
        uptime_secs: System::uptime(),
        disk: disk_of(&home),
        temp_c: temperature(),
        home: home.display().to_string(),
        tailscale: String::new(),
    }
}

/// The board a single-board computer says it is (Raspberry Pi and the like), or the vendor's
/// product name on a PC running Linux.
fn board_model() -> Option<String> {
    ["/proc/device-tree/model", "/sys/firmware/devicetree/base/model", "/sys/class/dmi/id/product_name"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .map(|m| m.trim_matches(|c: char| c == '\0' || c.is_whitespace()).to_owned())
        .filter(|m| !m.is_empty() && m != "To Be Filled By O.E.M." && m != "System Product Name")
}

/// What kind of computer this is, for its icon. The hardware does not change while hubd runs, so
/// it is worked out once (on Windows and macOS that asks a slow command).
pub fn machine_kind(model: Option<&str>) -> MachineKind {
    static KIND: std::sync::OnceLock<MachineKind> = std::sync::OnceLock::new();
    *KIND.get_or_init(|| {
        // A Mac says what it is through sysctl: "MacBookPro18,1", "Macmini9,1".
        let mac = cfg!(target_os = "macos").then(|| quiet("sysctl", &["-n", "hw.model"])).flatten();
        let facts = Facts {
            model: model.map(str::to_owned).or(mac),
            dmi: ["sys_vendor", "product_name", "board_vendor"]
                .iter()
                .filter_map(|f| std::fs::read_to_string(format!("/sys/class/dmi/id/{f}")).ok())
                .collect::<Vec<_>>()
                .join(" "),
            chassis: chassis_type(),
            hypervisor: hypervisor(),
            battery: has_battery(),
            device_tree: Path::new("/proc/device-tree/model").exists(),
            desktop_session: desktop_session(),
            os: std::env::consts::OS,
        };
        kind_of(&facts)
    })
}

/// What `kind_of` decides from, read from the machine apart so the decision can be tested.
#[derive(Default)]
struct Facts {
    model: Option<String>,
    /// The vendor and product strings the firmware gives (Linux).
    dmi: String,
    /// The SMBIOS chassis type: 9 and 10 are laptops, 3 a desktop, 17 a rack server, …
    chassis: Option<u32>,
    /// Runs under a hypervisor (the CPU says so).
    hypervisor: bool,
    battery: bool,
    /// Linux on a board that describes itself in a device tree, as single-board computers do.
    device_tree: bool,
    /// A graphical desktop is installed (Linux).
    desktop_session: bool,
    os: &'static str,
}

fn kind_of(f: &Facts) -> MachineKind {
    let model = f.model.as_deref().unwrap_or_default().to_ascii_lowercase();
    if ["raspberry pi", "orange pi", "rock pi", "odroid", "jetson", "beaglebone", "pine64", "rockpro64", "radxa"]
        .iter()
        .any(|b| model.contains(b))
    {
        return MachineKind::Board;
    }
    let dmi = format!("{} {model}", f.dmi.to_ascii_lowercase());
    let cloud = [
        "qemu", "kvm", "vmware", "virtualbox", "xen", "bochs", "hetzner", "digitalocean", "droplet", "amazon ec2",
        "google compute", "openstack", "linode", "akamai", "vultr", "upcloud", "scaleway", "oraclecloud", "virtual machine",
        "standard pc (",
    ];
    if f.hypervisor || cloud.iter().any(|c| dmi.contains(c)) {
        return MachineKind::Vps;
    }
    if f.os == "linux" && f.device_tree {
        return MachineKind::Board;
    }
    if model.contains("macbook") {
        return MachineKind::Laptop;
    }
    match f.chassis {
        Some(8 | 9 | 10 | 11 | 14 | 30 | 31 | 32) => return MachineKind::Laptop,
        Some(17 | 23 | 25 | 28 | 29) => return MachineKind::Server,
        _ => {}
    }
    if f.battery {
        return MachineKind::Laptop;
    }
    if f.os == "linux" && !f.desktop_session { MachineKind::Server } else { MachineKind::Desktop }
}

/// The SMBIOS chassis type, from the firmware on Linux and from WMI on Windows.
fn chassis_type() -> Option<u32> {
    if cfg!(windows) {
        let out = quiet("powershell", &["-NoProfile", "-NonInteractive", "-Command", "(Get-CimInstance Win32_SystemEnclosure).ChassisTypes[0]"])?;
        return out.trim().parse().ok();
    }
    std::fs::read_to_string("/sys/class/dmi/id/chassis_type").ok()?.trim().parse().ok()
}

/// The CPU says it runs under a hypervisor. Windows itself sets this bit when it runs with
/// virtualisation-based security, so it is only asked on Linux.
fn hypervisor() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    let flags = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let wsl = std::fs::read_to_string("/proc/sys/kernel/osrelease").is_ok_and(|r| r.to_ascii_lowercase().contains("microsoft"));
    !wsl && (flags.lines().any(|l| l.starts_with("flags") && l.split_whitespace().any(|w| w == "hypervisor"))
        || Path::new("/sys/hypervisor/type").exists())
}

fn has_battery() -> bool {
    if cfg!(target_os = "macos") {
        return quiet("pmset", &["-g", "batt"]).is_some_and(|o| o.contains("InternalBattery"));
    }
    if cfg!(windows) {
        return quiet("powershell", &["-NoProfile", "-NonInteractive", "-Command", "@(Get-CimInstance Win32_Battery).Count"])
            .is_some_and(|o| o.trim().parse::<u32>().is_ok_and(|n| n > 0));
    }
    std::fs::read_dir("/sys/class/power_supply").is_ok_and(|d| {
        d.flatten().any(|e| {
            std::fs::read_to_string(e.path().join("type")).is_ok_and(|t| t.trim() == "Battery")
                && std::fs::read_to_string(e.path().join("scope")).map_or(true, |s| s.trim() != "Device")
        })
    })
}

/// A graphical desktop is installed: some session to log in to.
fn desktop_session() -> bool {
    ["/usr/share/xsessions", "/usr/share/wayland-sessions"]
        .iter()
        .any(|d| std::fs::read_dir(d).is_ok_and(|mut e| e.next().is_some()))
}

/// A command's output, without a window flashing up on Windows; None when it fails.
pub(crate) fn quiet(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = crate::proc::command(program);
    cmd.args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The disk a folder is on: the one with the longest mount point that holds it.
pub(crate) fn disk_of(folder: &Path) -> Option<DiskUse> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| folder.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| DiskUse {
            mount: d.mount_point().display().to_string(),
            total: d.total_space(),
            free: d.available_space(),
        })
}

/// How warm the chip is: the average of its CPU sensors, or the kernel's first thermal zone (where
/// a Raspberry Pi reports its SoC).
fn temperature() -> Option<f32> {
    let components = sysinfo::Components::new_with_refreshed_list();
    let readings: Vec<(String, f32)> = components
        .list()
        .iter()
        .filter_map(|c| Some((c.label().to_owned(), c.temperature()?)))
        .collect();
    chip_temperature(&readings).or_else(|| {
        let milli = std::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").ok()?;
        milli.trim().parse::<f32>().ok().map(|m| m / 1000.0)
    })
}

/// One number from a machine's sensors. The hottest of them is the wrong one to show: an Apple
/// Silicon Mac has a sensor per core that reads 90 °C or more under a short load while the case
/// is only warm, and that scared. So the CPU sensors (by their label: `tdie`, `cpu`, `package`,
/// `core`, `soc`) are averaged; a machine that labels none gets the median of all it has.
fn chip_temperature(readings: &[(String, f32)]) -> Option<f32> {
    let mut valid: Vec<(String, f32)> = readings.iter().filter(|(_, t)| t.is_finite() && *t > 0.0 && *t < 150.0).cloned().collect();
    if valid.is_empty() {
        return None;
    }
    let cpu: Vec<f32> = valid
        .iter()
        .filter(|(l, _)| {
            let l = l.to_lowercase();
            ["tdie", "cpu", "package", "tctl", "core", "soc"].iter().any(|k| l.contains(k))
        })
        .map(|(_, t)| *t)
        .collect();
    if !cpu.is_empty() {
        return Some(cpu.iter().sum::<f32>() / cpu.len() as f32);
    }
    valid.sort_by(|a, b| a.1.total_cmp(&b.1));
    Some(valid[valid.len() / 2].1)
}

/// Tailscale here in a few words: "Running · 100.64.0.7", "Logged out", or "Not installed".
async fn tailscale_line() -> String {
    let status = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        crate::pairing::tailscale(&["status", "--json", "--peers=false"]),
    )
    .await;
    let line = match status {
        Ok(Ok(json)) => return tailscale_summary(&json),
        // The command was there but failed: say how, rather than calling it missing.
        Ok(Err(e)) if !format!("{e:#}").contains(crate::pairing::TAILSCALE_MISSING) => format!("{e:#}"),
        Ok(Err(_)) => "Not installed".to_owned(),
        Err(_) => "Did not answer".to_owned(),
    };
    // A tailnet address on one of this machine's own interfaces says it runs, whatever the
    // command could tell: the Mac app's command line cannot reach the app from hubd's launchd
    // daemon (error -600), and a service's short PATH may not have it.
    match tailnet_address() {
        Some(ip) => format!("Running · {ip}"),
        None => line,
    }
}

/// An address of this machine in Tailscale's range, 100.64.0.0/10.
fn tailnet_address() -> Option<std::net::Ipv4Addr> {
    let networks = sysinfo::Networks::new_with_refreshed_list();
    networks.values().flat_map(|n| n.ip_networks()).find_map(|net| match net.addr {
        std::net::IpAddr::V4(ip) if is_tailnet(ip) => Some(ip),
        _ => None,
    })
}

fn is_tailnet(ip: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && (64..128).contains(&b)
}

fn tailscale_summary(json: &str) -> String {
    let Ok(status) = serde_json::from_str::<serde_json::Value>(json) else {
        return "Unreadable status".to_owned();
    };
    match status["BackendState"].as_str().unwrap_or_default() {
        "Running" => {
            let ip = status["Self"]["TailscaleIPs"].as_array().and_then(|ips| ips.first()).and_then(|ip| ip.as_str());
            ip.map_or_else(|| "Running".to_owned(), |ip| format!("Running · {ip}"))
        }
        "NeedsLogin" => "Logged out".to_owned(),
        "Stopped" => "Switched off".to_owned(),
        "NoState" => "Not connected (NoState): the Tailscale service is up but not signed in or started".to_owned(),
        "" => "Did not say its state".to_owned(),
        other => other.to_owned(),
    }
}

fn listening_sockets() -> Vec<(u16, String, String, Option<u32>)> {
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto = ProtocolFlags::TCP;
    let Ok(sockets) = netstat2::get_sockets_info(af, proto) else {
        return Vec::new();
    };
    sockets
        .into_iter()
        .filter_map(|s| match s.protocol_socket_info {
            ProtocolSocketInfo::Tcp(tcp) if tcp.state == TcpState::Listen => Some((
                tcp.local_port,
                tcp.local_addr.to_string(),
                "tcp".to_owned(),
                s.associated_pids.first().copied(),
            )),
            _ => None,
        })
        .collect()
}

/// The thread with the deepest folder that contains `cwd`.
fn owning_thread(threads: &[Thread], cwd: &str) -> Option<String> {
    let cwd = Path::new(cwd);
    threads
        .iter()
        .filter(|t| cwd.starts_with(&t.cwd))
        .max_by_key(|t| (t.cwd.len(), t.updated_at))
        .map(|t| t.id.clone())
}

/// Whether the Hub core is among `pid`'s ancestors. A parent that started after its child is a
/// reused pid, so the chain stops there.
fn from_hub(sys: &System, pid: u32) -> bool {
    let hub = std::process::id();
    let mut at = sys.process(Pid::from_u32(pid));
    for _ in 0..64 {
        let Some(p) = at else { return false };
        let Some(parent) = p.parent() else { return false };
        if parent.as_u32() == hub {
            return true;
        }
        match sys.process(parent) {
            Some(q) if q.start_time() <= p.start_time() => at = Some(q),
            _ => return false,
        }
    }
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::process_marks::THREAD_VAR;

    #[test]
    fn the_temperature_is_the_chip_average_not_the_hottest_sensor() {
        let r = |l: &str, t: f32| (l.to_owned(), t);
        // A Mac: per-core die sensors that spike, a battery and a flash chip that are cool.
        let mac = [r("PMU tdie1", 95.0), r("PMU tdie2", 71.0), r("PMU tdie3", 69.0), r("PMU tdie4", 73.0), r("gas gauge battery", 31.0), r("NAND CH0 temp", 38.0)];
        assert_eq!(chip_temperature(&mac).map(|t| t.round()), Some(77.0));
        // A Pi: the one sensor it has.
        assert_eq!(chip_temperature(&[r("cpu_thermal", 52.0)]), Some(52.0));
        // Unlabelled sensors: the median, so one outlier does not decide.
        assert_eq!(chip_temperature(&[r("a", 30.0), r("b", 40.0), r("c", 99.0)]), Some(40.0));
        assert_eq!(chip_temperature(&[r("x", 0.0), r("y", f32::NAN)]), None);
    }

    /// Starts a process listening on a free port and returns it with the port.
    fn listen(thread: Option<&str>) -> (std::process::Child, u16) {
        use std::io::BufRead;
        let script = "import socket,time\ns=socket.socket()\ns.bind(('127.0.0.1',0))\ns.listen()\nprint(s.getsockname()[1],flush=True)\ntime.sleep(30)";
        let mut cmd = crate::proc::command("python3");
        cmd.args(["-c", script]).stdout(std::process::Stdio::piped());
        if let Some(t) = thread {
            cmd.env(THREAD_VAR, t);
        }
        let mut child = cmd.spawn().unwrap();
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        (child, line.trim().parse().unwrap())
    }

    /// A server is tied to the thread that started it by the id it inherits, not by its folder,
    /// and one Hub started outside a thread says so.
    #[test]
    fn a_server_knows_what_started_it() {
        let thread = format!("test-{}", std::process::id());
        let (mut by_thread, thread_port) = listen(Some(&thread));
        let (mut by_hub, hub_port) = listen(None);

        let snap = snapshot(&[], &[]);
        let find = |port| snap.listeners.iter().find(|l| l.port == port).expect("listed");
        let l = find(thread_port);
        assert_eq!(l.origin, ListenerOrigin::Thread);
        assert_eq!(l.thread_id.as_deref(), Some(thread.as_str()));
        assert!(l.command.as_deref().is_some_and(|c| c.starts_with("python3")));
        assert!(l.started_at.is_some_and(|t| t > 0));
        assert!(l.parent.as_deref().is_some_and(|p| p.ends_with(&format!("pid {}", std::process::id()))));
        // This test stands in for the core, so its own child counts as started by Hub.
        let l = find(hub_port);
        assert_eq!(l.origin, ListenerOrigin::Hub);
        assert_eq!(l.thread_id, None);

        for child in [&mut by_thread, &mut by_hub] {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod about_tests {
    use super::*;

    fn linux(f: Facts) -> MachineKind {
        kind_of(&Facts { os: "linux", ..f })
    }

    #[test]
    fn tells_the_kind_of_machine() {
        // clanker: a Raspberry Pi.
        let pi = Facts { model: Some("Raspberry Pi 4 Model B Rev 1.5".into()), device_tree: true, ..Default::default() };
        assert_eq!(linux(pi), MachineKind::Board);
        // hermes-vps: KVM in a data centre.
        let vps = Facts { dmi: "QEMU Standard PC (Q35 + ICH9, 2009)".into(), chassis: Some(1), hypervisor: true, ..Default::default() };
        assert_eq!(linux(vps), MachineKind::Vps);
        let hetzner = Facts { dmi: "Hetzner vServer".into(), ..Default::default() };
        assert_eq!(linux(hetzner), MachineKind::Vps);
        // tincan: an old PC with Ubuntu Server, no desktop.
        let tincan = Facts { dmi: "Dell Inc. OptiPlex 7010".into(), chassis: Some(3), ..Default::default() };
        assert_eq!(linux(tincan), MachineKind::Server);
        let workstation = Facts { chassis: Some(3), desktop_session: true, ..Default::default() };
        assert_eq!(linux(workstation), MachineKind::Desktop);
        let thinkpad = Facts { dmi: "LENOVO 20XW".into(), chassis: Some(10), ..Default::default() };
        assert_eq!(linux(thinkpad), MachineKind::Laptop);
        // The MacBook Pro, and the PC.
        let mac = Facts { model: Some("Mac14,7".into()), battery: true, os: "macos", ..Default::default() };
        assert_eq!(kind_of(&mac), MachineKind::Laptop);
        let mini = Facts { model: Some("Macmini9,1".into()), os: "macos", ..Default::default() };
        assert_eq!(kind_of(&mini), MachineKind::Desktop);
        let pc = Facts { chassis: Some(3), os: "windows", ..Default::default() };
        assert_eq!(kind_of(&pc), MachineKind::Desktop);
        let unknown_pc = Facts { os: "windows", ..Default::default() };
        assert_eq!(kind_of(&unknown_pc), MachineKind::Desktop);
    }

    #[test]
    fn tailscale_reads_as_a_few_words() {
        let running = r#"{"BackendState": "Running", "Self": {"TailscaleIPs": ["100.64.0.7", "fd7a::7"]}}"#;
        assert_eq!(tailscale_summary(running), "Running · 100.64.0.7");
        assert_eq!(tailscale_summary(r#"{"BackendState": "NeedsLogin"}"#), "Logged out");
        assert!(tailscale_summary(r#"{"BackendState": "NoState"}"#).starts_with("Not connected"));
        assert!(is_tailnet("100.82.234.33".parse().unwrap()));
        assert!(!is_tailnet("100.128.0.1".parse().unwrap()));
        assert!(!is_tailnet("192.168.1.2".parse().unwrap()));
    }

    #[test]
    fn this_machine_can_say_what_it_is() {
        let about = about_now();
        assert!(!about.arch.is_empty());
        assert!(about.cores > 0);
    }
}
