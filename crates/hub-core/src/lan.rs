//! Machines on the home network that SSH answers on, for setting up one that is not on the
//! tailnet yet, such as a Raspberry Pi straight out of the box. Every address on this machine's
//! own network (its /24) is tried on port 22 at once; what answers says what it runs in its
//! SSH greeting (`SSH-2.0-OpenSSH_9.2p1 Raspbian-2+deb12u3`).

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

use tokio::io::AsyncReadExt;

use crate::model::LanMachine;

/// The machines on this network that answer SSH, each matched to the machine in Hub with that
/// address. `hosts` are the machines in Hub with the names and addresses they go by.
pub async fn scan(hosts: &[(String, Vec<String>)]) -> anyhow::Result<Vec<LanMachine>> {
    let me = local_ipv4().ok_or_else(|| anyhow::anyhow!("this machine has no address on a home network"))?;
    let [a, b, c, _] = me.octets();
    let mut tries = tokio::task::JoinSet::new();
    for d in 1..=254u8 {
        let ip = Ipv4Addr::new(a, b, c, d);
        if ip == me {
            continue;
        }
        tries.spawn(greeting(ip));
    }
    let mut found = Vec::new();
    while let Some(done) = tries.join_next().await {
        if let Ok(Some((ip, banner))) = done {
            let ip = ip.to_string();
            let machine = hosts.iter().find(|(_, ids)| ids.contains(&ip)).map(|(name, _)| name.clone());
            found.push(LanMachine { system: system(&banner), ip, banner, machine });
        }
    }
    found.sort_by_key(|m| m.ip.parse::<Ipv4Addr>().map(u32::from).unwrap_or(0));
    Ok(found)
}

/// What SSH on `ip` says first, when it answers.
async fn greeting(ip: Ipv4Addr) -> Option<(Ipv4Addr, String)> {
    let addr = SocketAddr::new(IpAddr::V4(ip), 22);
    let mut stream = tokio::time::timeout(Duration::from_millis(700), tokio::net::TcpStream::connect(addr)).await.ok()?.ok()?;
    let mut buf = [0u8; 256];
    let n = tokio::time::timeout(Duration::from_millis(1500), stream.read(&mut buf)).await.ok().and_then(|r| r.ok()).unwrap_or(0);
    let banner = String::from_utf8_lossy(&buf[..n]).lines().next().unwrap_or_default().trim().to_owned();
    Some((ip, banner))
}

/// This machine's address on its network: the one the system would send from. Nothing is sent.
fn local_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    match socket.local_addr().ok()?.ip() {
        // Not the tailnet's own range: that is not a home network.
        IpAddr::V4(ip) if !ip.is_loopback() && !(ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1])) => Some(ip),
        _ => None,
    }
}

/// A word on what runs there, from the SSH greeting.
pub fn system(banner: &str) -> String {
    let b = banner.to_lowercase();
    let known = [
        ("raspbian", "Raspberry Pi OS"),
        ("ubuntu", "Ubuntu"),
        ("debian", "Debian"),
        ("fedora", "Fedora"),
        ("dropbear", "a small device (Dropbear)"),
        ("windows", "Windows"),
    ];
    match known.iter().find(|(k, _)| b.contains(k)) {
        Some((_, name)) => (*name).to_owned(),
        None if b.starts_with("ssh-") => "SSH".to_owned(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_a_pi_from_its_ssh_greeting() {
        assert_eq!(system("SSH-2.0-OpenSSH_9.2p1 Raspbian-2+deb12u3"), "Raspberry Pi OS");
        assert_eq!(system("SSH-2.0-OpenSSH_8.9p1 Ubuntu-3ubuntu0.10"), "Ubuntu");
        assert_eq!(system("SSH-2.0-OpenSSH_9.6"), "SSH");
    }

}
