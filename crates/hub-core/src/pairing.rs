//! Letting other devices in without copying the connection token around. `hubd pair` (or
//! Settings > Machines) makes a one-time code; the device that wants in shows it once and gets a
//! token of its own, which can be taken back on its own. The idea is T3 Code's pairing
//! (github.com/pingdotgg/t3code, MIT): the route a device comes in on (Tailscale, an SSH tunnel)
//! says nothing about who it is, only its token does.

use crate::locked::LockExt;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

use crate::model::{Device, PairCode, now_ms};
use crate::store::Store;

/// Devices let in, as JSON in the store. Only a hash of each token is kept.
const DEVICES_KEY: &str = "devices";
/// How long a code works.
const CODE_LIFE: Duration = Duration::from_secs(10 * 60);
/// Codes waiting at once; making another drops the oldest.
const MAX_CODES: usize = 5;
/// Wrong codes one sender may try before it is refused for `CODE_LIFE`, right code or not. Counted
/// per sender, so someone guessing on the tailnet no longer drops the codes you are about to use.
const MAX_MISSES: u32 = 10;
/// Wrong codes from everyone together before every waiting code is dropped, for guessing spread
/// over many addresses. A code is 40 bits, so this is far from a fair chance either way.
const MAX_ALL_MISSES: u32 = 100;
/// No 0/O or 1/I, so a code read aloud or off a phone is typed right.
const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Record {
    id: String,
    name: String,
    hash: String,
    created_at: i64,
    /// A phone let in by the QR code: it may only follow threads and answer them (`phone.rs`).
    #[serde(default)]
    phone: bool,
}

/// What a token lets a device do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// Everything, like this machine's own token: another machine's Hub.
    Full,
    /// The phone page's few calls, from the tailnet only.
    Phone,
}

#[derive(Default)]
pub struct Pairing {
    /// Waiting codes, until when they work, and whether they let a phone in.
    codes: Mutex<Vec<(String, Instant, bool)>>,
    /// Wrong codes by sender, since when it started missing; `None` keys the sum of all.
    misses: Mutex<HashMap<Option<IpAddr>, (u32, Instant)>>,
    /// The stored devices, read on first use.
    devices: tokio::sync::Mutex<Option<Vec<Record>>>,
    seen: Mutex<std::collections::HashMap<String, i64>>,
}

impl Pairing {
    /// A fresh code, shown as `ABCD-EFGH`.
    pub fn code(&self, url: Option<String>) -> PairCode {
        self.make(url, false)
    }

    /// A code for the phone page, which the QR code carries. The token it gets is a phone's.
    pub fn phone_code(&self) -> PairCode {
        self.make(None, true)
    }

    fn make(&self, url: Option<String>, phone: bool) -> PairCode {
        let raw = uuid::Uuid::new_v4();
        let code: String = raw.as_bytes()[..8].iter().map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char).collect();
        let mut codes = self.codes.locked();
        codes.retain(|(_, until, _)| *until > Instant::now());
        if codes.len() >= MAX_CODES {
            codes.remove(0);
        }
        codes.push((code.clone(), Instant::now() + CODE_LIFE, phone));
        PairCode {
            code: format!("{}-{}", &code[..4], &code[4..]),
            expires_at: now_ms() + CODE_LIFE.as_millis() as i64,
            url,
        }
    }

    /// Takes a code, once, and gives the device a token of its own.
    pub async fn redeem(&self, store: &Store, code: &str, name: &str) -> Result<String> {
        self.redeem_from(store, code, name, true, None).await
    }

    /// Like `redeem`, saying whether the device asks from the tailnet (or this machine). A phone's
    /// code only works from there, so a code that leaks elsewhere lets nobody in.
    /// `sender` is where it comes from, to count its wrong codes (`None`: unknown, this process).
    pub async fn redeem_from(&self, store: &Store, code: &str, name: &str, tailnet: bool, sender: Option<IpAddr>) -> Result<String> {
        let code: String = code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect();
        if sender.is_some() && self.misses(sender) >= MAX_MISSES {
            bail!("too many wrong codes from here; try again in a few minutes");
        }
        let found = {
            let mut codes = self.codes.locked();
            codes.retain(|(_, until, _)| *until > Instant::now());
            let at = codes.iter().position(|(c, _, phone)| *c == code && (tailnet || !phone));
            at.map(|at| codes.remove(at))
        };
        let Some((_, _, phone)) = found else {
            self.miss(sender);
            bail!("that code is wrong, used or expired; make a new one with `hubd pair`");
        };
        let name = match name.trim() {
            "" => "a device".to_owned(),
            name => name.chars().take(60).collect(),
        };
        let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        let record = Record { id: uuid::Uuid::new_v4().simple().to_string()[..12].to_owned(), name, hash: hash(&token), created_at: now_ms(), phone };
        let mut devices = self.devices.lock().await;
        let list = load(store, &mut devices).await?;
        list.push(record.clone());
        store.set_value(DEVICES_KEY, &serde_json::to_string(list)?).await?;
        tracing::info!("paired {} ({})", record.name, record.id);
        Ok(token)
    }

    /// Wrong codes `sender` (or, for `None`, everyone) tried in the last `CODE_LIFE`.
    fn misses(&self, sender: Option<IpAddr>) -> u32 {
        let mut misses = self.misses.locked();
        misses.retain(|_, (_, since)| since.elapsed() < CODE_LIFE);
        misses.get(&sender).map_or(0, |(n, _)| *n)
    }

    fn miss(&self, sender: Option<IpAddr>) {
        let mut misses = self.misses.locked();
        let now = Instant::now();
        if sender.is_some() {
            misses.entry(sender).or_insert((0, now)).0 += 1;
        }
        let all = misses.entry(None).or_insert((0, now));
        all.0 += 1;
        if all.0 >= MAX_ALL_MISSES {
            misses.clear();
            self.codes.locked().clear();
            tracing::warn!("{MAX_ALL_MISSES} wrong pairing codes; every waiting code was dropped");
        }
    }

    /// Whether `token` belongs to a device let in, noting that it called.
    pub async fn knows(&self, store: &Store, token: &str) -> bool {
        self.grant(store, token).await.is_some()
    }

    /// What `token` lets its device do, if it belongs to one let in, noting that it called.
    pub async fn grant(&self, store: &Store, token: &str) -> Option<Grant> {
        let hash = hash(token);
        let mut devices = self.devices.lock().await;
        let list = load(store, &mut devices).await.ok()?;
        // Every hash is compared in full, so timing says nothing about which one was close.
        let found = list.iter().fold(None, |found, r| if same(&r.hash, &hash) { Some(r) } else { found })?;
        self.seen.locked().insert(found.id.clone(), now_ms());
        Some(if found.phone { Grant::Phone } else { Grant::Full })
    }

    pub async fn devices(&self, store: &Store) -> Result<Vec<Device>> {
        let mut devices = self.devices.lock().await;
        let list = load(store, &mut devices).await?;
        let seen = self.seen.locked();
        Ok(list
            .iter()
            .map(|r| Device {
                id: r.id.clone(),
                name: r.name.clone(),
                created_at: r.created_at,
                last_seen: seen.get(&r.id).copied(),
                phone: r.phone,
            })
            .collect())
    }

    /// Takes a device's access back; its token stops working at once.
    pub async fn revoke(&self, store: &Store, id: &str) -> Result<()> {
        let mut devices = self.devices.lock().await;
        let list = load(store, &mut devices).await?;
        let before = list.len();
        list.retain(|r| r.id != id);
        if list.len() == before {
            bail!("no device {id}");
        }
        store.set_value(DEVICES_KEY, &serde_json::to_string(list)?).await?;
        Ok(())
    }
}

async fn load<'a>(store: &Store, devices: &'a mut Option<Vec<Record>>) -> Result<&'a mut Vec<Record>> {
    if devices.is_none() {
        let list = match store.get_value(DEVICES_KEY).await? {
            Some(json) => serde_json::from_str(&json)?,
            None => Vec::new(),
        };
        *devices = Some(list);
    }
    Ok(devices.get_or_insert_with(Vec::new))
}

fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Where Tailscale serves hubd on `port` over HTTPS (`tailscale serve`), e.g.
/// `https://tincan.tail1234.ts.net`. None without Tailscale or without such a mapping.
pub async fn tailscale_url(port: u16) -> Option<String> {
    let out = tailscale(&["serve", "status", "--json"]).await.ok()?;
    serve_url(&out, port)
}

/// Lets Tailscale serve hubd at `https://<machine>.<tailnet>.ts.net`, with a real certificate, to
/// every device on the tailnet. It stays across restarts. Requests arrive on loopback and still
/// need a token.
pub async fn tailscale_serve(port: u16) -> Result<String> {
    if let Some(url) = tailscale_url(port).await {
        return Ok(url);
    }
    tailscale(&["serve", "--bg", "--https=443", &format!("http://127.0.0.1:{port}")]).await?;
    match tailscale_url(port).await {
        Some(url) => Ok(url),
        None => bail!("tailscale serve did not take; see `tailscale serve status`"),
    }
}

/// Where the Tailscale CLI is when it is not on the PATH: the Windows and macOS apps put it here.
///
/// A service started by launchd or systemd has a short PATH (on a Mac `/usr/bin:/bin:/usr/sbin:/sbin`),
/// so Homebrew's and the open-source daemon's places are tried by name too.
const TAILSCALE_ELSEWHERE: &[&str] = &[
    r"C:\Program Files\Tailscale\tailscale.exe",
    "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
    "/usr/local/bin/tailscale",
    "/opt/homebrew/bin/tailscale",
];

/// What `tailscale` fails with when no Tailscale command is found in any of those places.
pub(crate) const TAILSCALE_MISSING: &str = "Tailscale is not installed on this machine";

pub(crate) async fn tailscale(args: &[&str]) -> Result<String> {
    let mut found = None;
    for program in std::iter::once("tailscale").chain(TAILSCALE_ELSEWHERE.iter().copied()) {
        let mut cmd = crate::proc::tokio_command(program);
        cmd.args(args).stdin(std::process::Stdio::null()).kill_on_drop(true);
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        match tokio::time::timeout(Duration::from_secs(15), cmd.output()).await {
            Err(_) => bail!("tailscale did not answer"),
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Ok(Err(e)) => bail!("running tailscale ({e})"),
            Ok(Ok(out)) => {
                found = Some(out);
                break;
            }
        }
    }
    let out = found.ok_or_else(|| anyhow::anyhow!(TAILSCALE_MISSING))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("failed");
        if line.contains("Access denied") || line.contains("operator") {
            bail!("tailscale: {line}. On Linux, once: sudo tailscale set --operator=$USER");
        }
        bail!("tailscale: {line}");
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Reads `tailscale serve status --json`: `{"Web": {"host.ts.net:443": {"Handlers": {"/":
/// {"Proxy": "http://127.0.0.1:7878"}}}}}`.
fn serve_url(json: &str, port: u16) -> Option<String> {
    let status: serde_json::Value = serde_json::from_str(json).ok()?;
    let ours = [format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}"), port.to_string()];
    status["Web"].as_object()?.iter().find_map(|(host, web)| {
        let proxy = web["Handlers"]["/"]["Proxy"].as_str()?.trim_end_matches('/');
        if !ours.iter().any(|o| o == proxy) {
            return None;
        }
        let host = host.strip_suffix(":443").unwrap_or(host);
        Some(format!("https://{host}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_code_lets_one_device_in_once() {
        let store = Store::in_memory().unwrap();
        let pairing = Pairing::default();
        let code = pairing.code(None).code;
        assert_eq!(code.len(), 9);
        let token = pairing.redeem(&store, &code.to_lowercase(), "phone").await.unwrap();
        assert!(pairing.knows(&store, &token).await);
        assert!(!pairing.knows(&store, "something else").await);
        assert!(pairing.redeem(&store, &code, "again").await.is_err());
        let devices = pairing.devices(&store).await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "phone");
        assert!(devices[0].last_seen.is_some());

        // Kept across starts, as a hash only.
        let stored = store.get_value(DEVICES_KEY).await.unwrap().unwrap();
        assert!(!stored.contains(&token));
        assert!(Pairing::default().knows(&store, &token).await);

        pairing.revoke(&store, &devices[0].id).await.unwrap();
        assert!(!pairing.knows(&store, &token).await);
    }

    #[tokio::test]
    async fn a_phone_code_gives_a_phone_token_and_only_on_the_tailnet() {
        let store = Store::in_memory().unwrap();
        let pairing = Pairing::default();
        let code = pairing.phone_code().code;
        assert!(pairing.redeem_from(&store, &code, "phone", false, None).await.is_err());
        let token = pairing.redeem_from(&store, &code, "phone", true, None).await.unwrap();
        assert_eq!(pairing.grant(&store, &token).await, Some(Grant::Phone));
        assert!(pairing.devices(&store).await.unwrap()[0].phone);

        let machine = pairing.code(None).code;
        let token = pairing.redeem_from(&store, &machine, "tincan", false, None).await.unwrap();
        assert_eq!(pairing.grant(&store, &token).await, Some(Grant::Full));
    }

    #[tokio::test]
    async fn one_guesser_is_shut_out_and_the_codes_stay() {
        let store = Store::in_memory().unwrap();
        let pairing = Pairing::default();
        let code = pairing.code(None).code;
        let guesser = Some("100.64.0.9".parse().unwrap());
        for _ in 0..MAX_MISSES {
            assert!(pairing.redeem_from(&store, "AAAA-AAAA", "guess", true, guesser).await.is_err());
        }
        // The guesser is refused even with the right code, and the code still works for you.
        assert!(pairing.redeem_from(&store, &code, "guess", true, guesser).await.is_err());
        let you = Some("100.64.0.2".parse().unwrap());
        assert!(pairing.redeem_from(&store, &code, "phone", true, you).await.is_ok());
    }

    #[tokio::test]
    async fn guessing_from_everywhere_drops_every_waiting_code() {
        let store = Store::in_memory().unwrap();
        let pairing = Pairing::default();
        let code = pairing.code(None).code;
        for i in 0..MAX_ALL_MISSES {
            let sender = Some(IpAddr::from([100, 64, (i / 200) as u8, (i % 200) as u8]));
            assert!(pairing.redeem_from(&store, "AAAA-AAAA", "guess", true, sender).await.is_err());
        }
        assert!(pairing.redeem(&store, &code, "late").await.is_err());
    }

    #[test]
    fn the_serve_mapping_to_hubd_gives_the_url() {
        let json = r#"{"TCP":{"443":{"HTTPS":true}},"Web":{
            "other.tail1.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:3000"}}},
            "tincan.tail1.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:7878"}}}}}"#;
        assert_eq!(serve_url(json, 7878).as_deref(), Some("https://tincan.tail1.ts.net"));
        assert_eq!(serve_url(json, 7979), None);
        assert_eq!(serve_url("{}", 7878), None);
    }
}
