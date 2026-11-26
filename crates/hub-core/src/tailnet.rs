//! New machines are found on the tailnet: whatever joins it shows up in Settings > Machines, and
//! a Linux machine can be set up from there with one click. T3 Code lists the machines on its
//! account (T3 Connect); for Hub the tailnet is that list, read from `tailscale status`.

use std::collections::HashSet;

use anyhow::Result;

use crate::model::{SshHost, Tailnet, TailnetPeer};
use crate::store::Store;

/// Machines hidden from the nudge, by name, as JSON in the store.
const DISMISSED_KEY: &str = "tailnet_dismissed";

/// The tailnet as this machine sees it, each machine on it matched to the one it is in Hub by the
/// names in `hosts` (`Remotes::identities`) and `ssh` (`~/.ssh/config`). `Remotes::tailnet` reads
/// both, so this module names neither `remote` nor `prefs`.
pub async fn scan(store: &Store, ssh: &[SshHost], hosts: &[(String, Vec<String>)]) -> Tailnet {
    let json = match crate::pairing::tailscale(&["status", "--json"]).await {
        Ok(json) => json,
        Err(e) => return Tailnet { error: Some(format!("{e:#}")), ..Default::default() },
    };
    let dismissed = dismissed(store).await.unwrap_or_default();
    read_status(&json, hosts, ssh, &dismissed)
}

/// Hides a machine from the nudge; it stays in the list in Settings.
pub async fn dismiss(store: &Store, name: &str) -> Result<()> {
    let mut names = dismissed(store).await?;
    if !names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
        names.push(name.to_owned());
        store.set_value(DISMISSED_KEY, &serde_json::to_string(&names)?).await?;
    }
    Ok(())
}

async fn dismissed(store: &Store) -> Result<Vec<String>> {
    Ok(match store.get_value(DISMISSED_KEY).await? {
        Some(json) => serde_json::from_str(&json)?,
        None => Vec::new(),
    })
}

/// Reads `tailscale status --json`: `{"BackendState": "Running", "Self": {...}, "Peer":
/// {"nodekey:…": {"HostName", "DNSName", "TailscaleIPs", "OS", "Online"}}}`. Machines shared in
/// from other tailnets and Mullvad exit nodes are left out; they are not this person's to set up.
pub fn read_status(json: &str, hosts: &[(String, Vec<String>)], ssh: &[SshHost], dismissed: &[String]) -> Tailnet {
    let Ok(status) = serde_json::from_str::<serde_json::Value>(json) else {
        return Tailnet { error: Some("tailscale status gave something unreadable".to_owned()), ..Default::default() };
    };
    let state = status["BackendState"].as_str().unwrap_or_default();
    if state != "Running" {
        let why = match state {
            "NeedsLogin" => "Tailscale here is logged out".to_owned(),
            "Stopped" => "Tailscale here is switched off".to_owned(),
            "" => "Tailscale here did not say its state".to_owned(),
            other => format!("Tailscale here is {other}"),
        };
        return Tailnet { error: Some(why), ..Default::default() };
    }
    let mut peers: Vec<TailnetPeer> = status["Peer"]
        .as_object()
        .into_iter()
        .flat_map(|peers| peers.values())
        .filter(|p| !p["ShareeNode"].as_bool().unwrap_or(false))
        .filter_map(|p| {
            let dns = p["DNSName"].as_str().unwrap_or_default().trim_end_matches('.').to_owned();
            if dns.ends_with(".mullvad.ts.net") {
                return None;
            }
            let name = match p["HostName"].as_str().filter(|n| !n.is_empty()) {
                Some(n) => n.to_owned(),
                None => dns.split('.').next().filter(|n| !n.is_empty())?.to_owned(),
            };
            let ips: Vec<String> =
                p["TailscaleIPs"].as_array().into_iter().flatten().filter_map(|ip| ip.as_str().map(str::to_owned)).collect();
            let ip = ips.iter().find(|ip| !ip.contains(':')).cloned();
            let os = p["OS"].as_str().unwrap_or_default().to_owned();
            let mut names: HashSet<String> = ips.iter().map(|ip| ip.to_lowercase()).collect();
            names.insert(name.to_lowercase());
            if !dns.is_empty() {
                names.insert(dns.to_lowercase());
                names.extend(dns.split('.').next().map(str::to_lowercase));
            }
            let machine = hosts
                .iter()
                .find(|(_, ids)| ids.iter().any(|id| names.contains(&id.to_lowercase())))
                .map(|(host, _)| host.clone());
            let alias = ssh.iter().find(|h| {
                std::iter::once(&h.alias).chain(&h.aliases).chain(&h.hostname).any(|n| names.contains(&n.to_lowercase()))
            });
            let ssh = match (alias, &ip) {
                (Some(h), _) => h.alias.clone(),
                (None, _) if !dns.is_empty() => dns.clone(),
                (None, Some(ip)) => ip.clone(),
                (None, None) => name.clone(),
            };
            Some(TailnetPeer {
                dismissed: dismissed.iter().any(|d| d.eq_ignore_ascii_case(&name)),
                supported: supported_os(&os),
                online: p["Online"].as_bool().unwrap_or(false),
                name,
                dns,
                ip,
                os,
                machine,
                ssh,
            })
        })
        .collect();
    // Machines to set up first, then by name.
    peers.sort_by(|a, b| {
        (a.machine.is_some(), !a.supported, !a.online, a.name.to_lowercase()).cmp(&(
            b.machine.is_some(),
            !b.supported,
            !b.online,
            b.name.to_lowercase(),
        ))
    });
    let me = status["Self"]["HostName"].as_str().map(str::to_owned);
    Tailnet { running: true, error: None, me, peers }
}

/// Linux machines and Macs run hubd; phones and Windows do not (yet).
fn supported_os(os: &str) -> bool {
    os.eq_ignore_ascii_case("linux") || os.eq_ignore_ascii_case("macos")
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = r#"{
        "BackendState": "Running",
        "MagicDNSSuffix": "tail1234.ts.net",
        "Self": {"HostName": "PC", "DNSName": "pc.tail1234.ts.net.", "OS": "windows", "Online": true},
        "Peer": {
            "nodekey:a": {"HostName": "clanker", "DNSName": "clanker.tail1234.ts.net.", "OS": "linux",
                          "TailscaleIPs": ["100.101.1.2", "fd7a:115c:a1e0::1"], "Online": true},
            "nodekey:b": {"HostName": "tincan", "DNSName": "tincan.tail1234.ts.net.", "OS": "linux",
                          "TailscaleIPs": ["100.71.243.18"], "Online": false},
            "nodekey:c": {"HostName": "Pixel 8", "DNSName": "pixel-8.tail1234.ts.net.", "OS": "android",
                          "TailscaleIPs": ["100.80.0.3"], "Online": true},
            "nodekey:d": {"HostName": "box", "DNSName": "box.friend.ts.net.", "OS": "linux", "ShareeNode": true,
                          "TailscaleIPs": ["100.90.0.4"], "Online": true},
            "nodekey:e": {"HostName": "se-sto-wg-001", "DNSName": "se-sto-wg-001.mullvad.ts.net.", "OS": "linux",
                          "TailscaleIPs": ["100.64.0.9"], "Online": true},
            "nodekey:f": {"HostName": "fresh-pi", "DNSName": "fresh-pi.tail1234.ts.net.", "OS": "linux",
                          "TailscaleIPs": ["100.100.0.7"], "Online": true}
        }
    }"#;

    fn ssh_host(alias: &str, hostname: &str) -> SshHost {
        SshHost {
            alias: alias.to_owned(),
            aliases: Vec::new(),
            hostname: Some(hostname.to_owned()),
            user: Some("christoffer".to_owned()),
            port: None,
            identity_file: None,
            proxy_jump: None,
        }
    }

    #[test]
    fn finds_new_machines_and_knows_the_added_ones() {
        // tincan is in Hub by its address; clanker is only in the SSH config, under another name.
        let hosts = vec![("tincan".to_owned(), vec!["tincan".to_owned(), "100.71.243.18".to_owned()])];
        let ssh = vec![ssh_host("pi", "100.101.1.2")];
        let net = read_status(STATUS, &hosts, &ssh, &["Pixel 8".to_owned()]);
        assert!(net.running);
        assert_eq!(net.me.as_deref(), Some("PC"));
        let names: Vec<&str> = net.peers.iter().map(|p| p.name.as_str()).collect();
        // Not shared-in machines or exit nodes; ones to set up first.
        assert_eq!(names, ["clanker", "fresh-pi", "Pixel 8", "tincan"]);

        let clanker = &net.peers[0];
        assert_eq!(clanker.ssh, "pi", "the SSH alias that points at it");
        assert_eq!(clanker.ip.as_deref(), Some("100.101.1.2"));
        assert!(clanker.supported && clanker.online && clanker.machine.is_none());

        let fresh = &net.peers[1];
        assert_eq!(fresh.ssh, "fresh-pi.tail1234.ts.net", "its MagicDNS name");

        let phone = &net.peers[2];
        assert!(!phone.supported && phone.dismissed);
        assert!(supported_os("macOS") && supported_os("linux") && !supported_os("iOS") && !supported_os("windows"));

        assert_eq!(net.peers[3].machine.as_deref(), Some("tincan"));
    }

    #[test]
    fn says_why_there_is_no_list() {
        let net = read_status(r#"{"BackendState": "NeedsLogin", "Peer": null}"#, &[], &[], &[]);
        assert!(!net.running && net.peers.is_empty());
        assert_eq!(net.error.as_deref(), Some("Tailscale here is logged out"));
    }
}
