//! A look at the iPhone simulator of a Mac, so you can see what a thread builds without leaving
//! Hub. It runs where the simulator is (the MacBook's hubd); every other surface asks that machine
//! with `?host=`, which the forwarding middleware already does.
//!
//! - A **picture** (`frame`) always works on a Mac with Xcode: `xcrun simctl io <udid> screenshot`.
//! - The **live** picture streams the simulator through `serve-sim` (Apache-2.0, pinned below,
//!   installed once into the data folder with its scripts off). Hub runs it on loopback only, reads
//!   its motion-JPEG stream and keeps the last picture; `frame` hands that out, so a surface that
//!   asks a few times a second sees the screen move without the middleman having to relay a
//!   stream. Nothing asks for a minute and the stream stops by itself.
//!
//! **Touch** (`input`) goes the other way: the tool's control socket takes a touch (begin, move,
//! end, as 0..1 of the screen) or a button (home, swipe home, app switcher) as one binary message
//! each, a code byte and JSON. Hub keeps one socket open while the live picture runs and sends
//! what a surface posts.

use crate::locked::ResultExt;
use crate::locked::LockExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio_tungstenite::tungstenite::Message;

use crate::config::Config;
use crate::model::{SimDevice, SimLive, SimView, now_ms};
use crate::proc::tokio_command;

/// The streaming tool and the version Hub installs. A newer one is a decision, not an update.
const TOOL: &str = "serve-sim";
const TOOL_VERSION: &str = "0.1.48";
/// Nobody asked for a picture for this long: the stream stops.
const IDLE_MS: i64 = 60_000;
/// The pause after each touch point sent to the simulator, so a batch still reads as a swipe.
const TOUCH_GAP_MS: u64 = 16;
/// Booting a simulator on an old Intel Mac takes most of a minute.
const BOOT_WAIT: Duration = Duration::from_secs(240);

#[derive(Default)]
pub struct Simview {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    live: Mutex<SimLive>,
    frame: Mutex<Option<Arc<Vec<u8>>>>,
    /// The last time a surface asked for a picture, for the idle stop.
    asked: AtomicI64,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Where the streaming tool is installed, once a live view was asked for.
    dir: Mutex<Option<PathBuf>>,
    /// What to send the tool's control socket (a code byte and JSON), while the live picture runs.
    input: Mutex<Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>,
}

impl Inner {
    fn set(&self, phase: &str, udid: Option<&str>, detail: Option<String>) {
        *self.live.locked() =
            SimLive { phase: phase.into(), udid: udid.map(str::to_owned), detail, fps: None };
    }
}

impl Simview {
    /// What this machine can show and what is being watched.
    pub async fn state(&self) -> SimView {
        let mut view = SimView { live: self.inner.live.locked().clone(), ..Default::default() };
        if let Some(why) = unavailable() {
            view.reason = Some(why);
            return view;
        }
        view.available = true;
        view.live_possible = tool_programs().is_some();
        match devices().await {
            Ok(list) => view.devices = list,
            Err(e) => view.reason = Some(e.to_string()),
        }
        view
    }

    /// Starts the live picture of `udid` (booting it first if needed). Returns at once; the phase
    /// in `state` says how far it got.
    pub fn start(&self, cfg: &Config, udid: &str) -> Result<()> {
        if let Some(why) = unavailable() {
            bail!(why);
        }
        if tool_programs().is_none() {
            bail!("live view needs Node and npm on this Mac");
        }
        {
            let live = self.inner.live.locked();
            if live.udid.as_deref() == Some(udid) && matches!(live.phase.as_str(), "installing" | "booting" | "starting" | "live") {
                return Ok(());
            }
        }
        self.stop();
        self.inner.asked.store(now_ms(), Ordering::Relaxed);
        self.inner.set("installing", Some(udid), None);
        let inner = self.inner.clone();
        let dir = tool_dir(cfg);
        *inner.dir.locked() = Some(dir.clone());
        let udid = udid.to_owned();
        let handle = tokio::spawn(async move {
            let result = run(&inner, &dir, &udid).await;
            *inner.frame.locked() = None;
            *inner.input.locked() = None;
            match result {
                Ok(()) => inner.set("off", None, None),
                Err(e) => {
                    tracing::warn!("simulator view: {e:#}");
                    inner.set("error", Some(&udid), Some(format!("{e:#}")));
                }
            }
        });
        *self.inner.task.locked() = Some(handle);
        Ok(())
    }

    /// Stops the live picture. The simulator itself keeps running.
    pub fn stop(&self) {
        let udid = self.inner.live.locked().udid.clone();
        let dir = self.inner.dir.locked().clone();
        if let Some(task) = self.inner.task.locked().take() {
            // The task owns the tool's process, which is killed when the task is dropped.
            task.abort();
        }
        *self.inner.frame.locked() = None;
        *self.inner.input.locked() = None;
        self.inner.set("off", None, None);
        // A tool killed from outside can leave its helper behind; ask it to clean up.
        if let (Some(udid), Some(dir), Some((node, _))) = (udid, dir, tool_programs()) {
            let entry = entry_in(&dir);
            if entry.is_file() && check_udid(&udid).is_ok() {
                tokio::spawn(async move {
                    tokio_command(node).arg(entry).args(["--kill", &udid]).output().await.warn_on_err("kill the simulator view");
                });
            }
        }
    }

    /// The newest picture of `udid`: the live one while it is being watched, else a screenshot.
    /// `None` when the stream is starting and has no picture yet.
    pub async fn frame(&self, udid: &str) -> Result<Option<Vec<u8>>> {
        self.inner.asked.store(now_ms(), Ordering::Relaxed);
        let watching = {
            let live = self.inner.live.locked();
            live.udid.as_deref() == Some(udid) && matches!(live.phase.as_str(), "installing" | "booting" | "starting" | "live")
        };
        if watching {
            return Ok(self.inner.frame.locked().as_ref().map(|f| f.as_ref().clone()));
        }
        screenshot(udid).await.map(Some)
    }

    /// Sends touches and a button press to the simulator that is watched live. Each touch is
    /// `begin`, `move` or `end` at a place given as 0..1 of the screen from its top left corner.
    pub fn input(&self, udid: &str, touches: &[SimTouch], button: Option<&str>) -> Result<()> {
        check_udid(udid)?;
        self.inner.asked.store(now_ms(), Ordering::Relaxed);
        {
            let live = self.inner.live.locked();
            if live.udid.as_deref() != Some(udid) || live.phase != "live" {
                bail!("the simulator is not live");
            }
        }
        let Some(tx) = self.inner.input.locked().clone() else { bail!("the simulator does not take touches yet") };
        let mut messages = Vec::new();
        for t in touches {
            messages.push(control_message(3, &serde_json::json!({ "type": t.phase_name()?, "x": t.x.clamp(0.0, 1.0), "y": t.y.clamp(0.0, 1.0) })));
        }
        if let Some(button) = button {
            if !matches!(button, "home" | "swipe_home" | "app_switcher") {
                bail!("unknown button");
            }
            messages.push(control_message(4, &serde_json::json!({ "button": button })));
        }
        for m in messages {
            tx.send(m).map_err(|_| anyhow::anyhow!("the simulator does not take touches any more"))?;
        }
        Ok(())
    }
}

/// One finger on the screen.
#[derive(Debug, Clone, Deserialize)]
pub struct SimTouch {
    /// `begin`, `move` or `end`.
    pub phase: String,
    pub x: f64,
    pub y: f64,
}

impl SimTouch {
    fn phase_name(&self) -> Result<&str> {
        match self.phase.as_str() {
            "begin" | "move" | "end" => Ok(self.phase.as_str()),
            _ => bail!("a touch is begin, move or end"),
        }
    }
}

/// A control message of the tool: a code byte, then JSON.
fn control_message(code: u8, body: &serde_json::Value) -> Vec<u8> {
    let mut m = vec![code];
    m.extend_from_slice(body.to_string().as_bytes());
    m
}

fn unavailable() -> Option<String> {
    if !cfg!(target_os = "macos") {
        return Some("simulators run on a Mac with Xcode".into());
    }
    if crate::proc::find_program("xcrun").is_none() {
        return Some("Xcode's command line tools were not found on this Mac".into());
    }
    None
}

/// Node and npm, which run and install the streaming tool.
fn tool_programs() -> Option<(PathBuf, PathBuf)> {
    Some((crate::proc::find_program("node")?, crate::proc::find_program("npm")?))
}

fn tool_dir(cfg: &Config) -> PathBuf {
    cfg.data_dir.join("tools").join(TOOL)
}

fn entry_in(dir: &Path) -> PathBuf {
    dir.join("node_modules").join(TOOL).join("dist").join("serve-sim.js")
}

#[derive(Deserialize)]
struct Listing {
    devices: std::collections::BTreeMap<String, Vec<Listed>>,
}

#[derive(Deserialize)]
struct Listed {
    name: String,
    udid: String,
    state: String,
    #[serde(default, rename = "isAvailable")]
    available: bool,
}

/// The iPhones and iPads this Mac can run, booted ones first.
pub async fn devices() -> Result<Vec<SimDevice>> {
    let out = tokio_command("xcrun").args(["simctl", "list", "devices", "available", "-j"]).output().await.context("xcrun simctl")?;
    if !out.status.success() {
        bail!("xcrun simctl: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    parse_devices(&out.stdout)
}

fn parse_devices(json: &[u8]) -> Result<Vec<SimDevice>> {
    let listing: Listing = serde_json::from_slice(json).context("reading the simulator list")?;
    let mut devices = Vec::new();
    for (runtime, list) in listing.devices {
        let Some(version) = runtime.rsplit('.').next().and_then(|r| r.strip_prefix("iOS-")) else { continue };
        let runtime = format!("iOS {}", version.replace('-', "."));
        for d in list.into_iter().filter(|d| d.available) {
            let kind = if d.name.starts_with("iPad") {
                "ipad"
            } else if d.name.starts_with("iPhone") {
                "iphone"
            } else {
                continue;
            };
            devices.push(SimDevice { udid: d.udid, name: d.name, runtime: runtime.clone(), booted: d.state == "Booted", kind: kind.into() });
        }
    }
    devices.sort_by(|a, b| b.booted.cmp(&a.booted).then((a.kind == "ipad").cmp(&(b.kind == "ipad"))).then(b.runtime.cmp(&a.runtime)).then(b.name.cmp(&a.name)));
    Ok(devices)
}

/// Boots a simulator and waits until it is ready (it is fine if it already is).
pub async fn boot(udid: &str) -> Result<()> {
    check_udid(udid)?;
    let out = tokio_command("xcrun").args(["simctl", "boot", udid]).output().await.context("xcrun simctl boot")?;
    let said = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() && !said.contains("current state: Booted") {
        bail!("could not boot the simulator: {}", said.trim());
    }
    let wait = tokio_command("xcrun").args(["simctl", "bootstatus", udid]).output();
    match tokio::time::timeout(BOOT_WAIT, wait).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => bail!("xcrun simctl bootstatus: {e}"),
        Err(_) => bail!("the simulator did not finish booting in {} seconds", BOOT_WAIT.as_secs()),
    }
}

/// Hub's own iPhone app, as the simulator knows it.
const APP_ID: &str = "se.karlsson.hub";

/// What to do with Hub's app in a simulator (the "Hub app" menu of the view).
#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
    /// Start it paired with this machine, so no one types a code: the simulator shares the Mac's
    /// network, so the pairing link points at this machine's own loopback address.
    Start,
    /// Open one thread (`hub://thread/<id>`).
    Thread(String),
    /// Start it on one of its four tabs (0 Inbox, 1 Chats, 2 Machines, 3 Settings).
    Tab(u8),
}

impl AppAction {
    /// What a request names: `start`, `thread` (with the thread id) or `tab` (with its number).
    pub fn parse(action: &str, thread: Option<&str>, tab: Option<u8>) -> Result<Self> {
        match action {
            "start" => Ok(Self::Start),
            "thread" => match thread {
                Some(id) if !id.is_empty() && id.len() <= 80 && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) => Ok(Self::Thread(id.to_owned())),
                _ => bail!("not a thread id"),
            },
            "tab" => match tab {
                Some(t) if t <= 3 => Ok(Self::Tab(t)),
                _ => bail!("the app has tabs 0 to 3"),
            },
            _ => bail!("unknown app action"),
        }
    }

    /// The arguments `simctl launch` gets after the app id, or `None` for an action that opens a
    /// link instead. `code` is a fresh one-time pairing code and `port` where this Hub listens.
    fn launch_args(&self, code: &str, port: u16) -> Option<Vec<String>> {
        let pair = format!("http://127.0.0.1:{port}/#pair={code}");
        match self {
            Self::Start => Some(vec!["-pair".into(), pair]),
            Self::Tab(t) => Some(vec!["-pair".into(), pair, "-tab".into(), t.to_string()]),
            Self::Thread(_) => None,
        }
    }
}

/// Runs an action on Hub's app in a simulator. `code` is a fresh one-time pairing code.
pub async fn app(udid: &str, action: &AppAction, code: &str, port: u16) -> Result<()> {
    check_udid(udid)?;
    let (verb, args): (&str, Vec<String>) = match (action.launch_args(code, port), action) {
        (Some(args), _) => ("launch", [vec!["--terminate-running-process".into(), udid.into(), APP_ID.into()], args].concat()),
        (None, AppAction::Thread(id)) => ("openurl", vec![udid.into(), format!("hub://thread/{id}")]),
        (None, _) => bail!("nothing to run"),
    };
    let out = tokio_command("xcrun").arg("simctl").arg(verb).args(&args).output().await.context("xcrun simctl")?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr);
        if said.contains("not installed") || said.contains("unknown to FrontBoard") || said.contains("No such file") {
            bail!("Hub's app is not installed in this simulator yet");
        }
        bail!("could not start Hub's app: {}", said.trim());
    }
    Ok(())
}

/// Shuts a simulator down (it is fine if it already is). The streaming tool must have let go of it
/// first, which `Simview::stop` does.
pub async fn shutdown(udid: &str) -> Result<()> {
    check_udid(udid)?;
    let out = tokio_command("xcrun").args(["simctl", "shutdown", udid]).output().await.context("xcrun simctl shutdown")?;
    let said = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() && !said.contains("current state: Shutdown") {
        bail!("could not shut the simulator down: {}", said.trim());
    }
    Ok(())
}

/// A UDID is hex and dashes; anything else never reaches a command line.
fn check_udid(udid: &str) -> Result<()> {
    if udid.len() < 8 || !udid.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        bail!("not a simulator id");
    }
    Ok(())
}

/// One screenshot of a booted simulator, as a JPEG.
pub async fn screenshot(udid: &str) -> Result<Vec<u8>> {
    check_udid(udid)?;
    let file = std::env::temp_dir().join(format!("hub-simview-{}-{}.jpg", std::process::id(), now_ms()));
    let out = tokio_command("xcrun")
        .args(["simctl", "io", udid, "screenshot", "--type=jpeg"])
        .arg(&file)
        .output()
        .await
        .context("xcrun simctl io")?;
    let bytes = std::fs::read(&file);
    crate::locked::clean_file(&file);
    match bytes {
        Ok(bytes) if !bytes.is_empty() => Ok(bytes),
        _ => bail!("no screenshot: {}", String::from_utf8_lossy(&out.stderr).trim()),
    }
}

/// Installs the streaming tool (once), boots the simulator, starts the tool and keeps the newest
/// picture until nobody asks for a minute. Returns when the stream ends.
async fn run(inner: &Arc<Inner>, dir: &Path, udid: &str) -> Result<()> {
    check_udid(udid)?;
    let (node, npm) = tool_programs().context("Node and npm are needed for the live view")?;
    let entry = entry_in(dir);
    if !entry.is_file() {
        std::fs::create_dir_all(dir)?;
        let out = tokio_command(&npm)
            .arg("install")
            .arg("--prefix")
            .arg(dir)
            .arg(format!("{TOOL}@{TOOL_VERSION}"))
            .args(["--ignore-scripts", "--no-fund", "--no-audit"])
            .output()
            .await
            .context("npm install")?;
        if !out.status.success() || !entry.is_file() {
            bail!("could not install {TOOL}: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("npm failed"));
        }
    }
    inner.set("booting", Some(udid), None);
    boot(udid).await?;

    inner.set("starting", Some(udid), None);
    let port = free_port()?;
    let mut child = tokio_command(&node)
        .arg(&entry)
        .arg(udid)
        .args(["--port", &port.to_string(), "--host", "127.0.0.1", "--exit-on-simulator-shutdown"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("starting serve-sim")?;
    let client = reqwest::Client::new();
    let stream_url = wait_for_stream(&client, port, &mut child).await?;

    let mut response = client.get(&stream_url).send().await?.error_for_status().context("opening the stream")?;
    // The control socket sits beside the stream. If it does not open, the picture still runs.
    let ws_url = stream_url.replacen("http://", "ws://", 1).replace("stream.mjpeg", "ws");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    match tokio_tungstenite::connect_async(ws_url.as_str()).await {
        Ok((ws, _)) => {
            let (mut sink, mut incoming) = ws.split();
            let writer = tokio::spawn(async move {
                while let Some(m) = rx.recv().await {
                    let touch = m.first() == Some(&3);
                    if sink.send(Message::Binary(m.into())).await.is_err() {
                        break;
                    }
                    // The simulator reads a swipe from the time between its points: touches that
                    // arrive together (a slow link sends several moves in one request) are one jump
                    // and do not scroll. Space them like a finger would.
                    if touch {
                        tokio::time::sleep(Duration::from_millis(TOUCH_GAP_MS)).await;
                    }
                }
            });
            // The tool sends its own state down the socket; read it so the socket stays healthy.
            let reader = tokio::spawn(async move { while incoming.next().await.is_some() {} });
            *inner.input.locked() = Some(tx);
            let _guard = AbortOnDrop(vec![writer, reader]);
            return stream_loop(inner, &mut response, &mut child, udid).await;
        }
        Err(e) => tracing::warn!("simulator view: no control socket: {e}"),
    }
    stream_loop(inner, &mut response, &mut child, udid).await
}

struct AbortOnDrop(Vec<tokio::task::JoinHandle<()>>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        for t in &self.0 {
            t.abort();
        }
    }
}

/// Reads the picture stream until nobody watches, keeping the newest picture.
async fn stream_loop(inner: &Arc<Inner>, response: &mut reqwest::Response, child: &mut tokio::process::Child, udid: &str) -> Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut window: std::collections::VecDeque<i64> = Default::default();
    loop {
        if now_ms() - inner.asked.load(Ordering::Relaxed) > IDLE_MS {
            return Ok(());
        }
        let chunk = tokio::select! {
            chunk = response.chunk() => chunk.context("reading the stream")?,
            status = child.wait() => bail!("serve-sim ended ({})", status?),
            _ = tokio::time::sleep(Duration::from_secs(5)) => continue,
        };
        let Some(chunk) = chunk else { bail!("the stream ended") };
        buf.extend_from_slice(&chunk);
        while let Some(frame) = take_part(&mut buf) {
            let now = now_ms();
            window.push_back(now);
            while window.front().is_some_and(|t| now - t > 5_000) {
                window.pop_front();
            }
            *inner.frame.locked() = Some(Arc::new(frame));
            let fps = (window.len() as f64) / 5.0;
            *inner.live.locked() = SimLive { phase: "live".into(), udid: Some(udid.to_owned()), detail: None, fps: Some(fps) };
        }
        if buf.len() > 32 * 1024 * 1024 {
            buf.clear();
        }
    }
}

/// A port nothing listens on yet, on loopback.
fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

/// Waits until the tool's preview server answers and tells where its picture stream is.
async fn wait_for_stream(client: &reqwest::Client, port: u16, child: &mut tokio::process::Child) -> Result<String> {
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            bail!("serve-sim ended before it was ready ({status})");
        }
        if let Ok(answer) = client.get(format!("http://127.0.0.1:{port}/api")).timeout(Duration::from_secs(3)).send().await
            && answer.status().is_success()
            && let Ok(json) = answer.json::<serde_json::Value>().await
            && let Some(url) = json.get("streamUrl").and_then(|u| u.as_str())
        {
            // Whatever host it names, it is this one.
            let path = url.split_once("//").and_then(|(_, rest)| rest.find('/').map(|i| &rest[i..])).unwrap_or("/stream.mjpeg");
            return Ok(format!("http://127.0.0.1:{port}{path}"));
        }
        if started.elapsed() > Duration::from_secs(60) {
            bail!("serve-sim did not start within a minute");
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// Takes the first whole picture out of a motion-JPEG stream. Each part has a header block with
/// `Content-Length`; a stream without it is cut at the JPEG start and end markers instead.
fn take_part(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    let head_end = find(buf, b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&buf[..head_end]).to_ascii_lowercase();
    if let Some(len) = head.lines().find_map(|l| l.trim().strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()) {
        let start = head_end + 4;
        if buf.len() < start + len {
            return None;
        }
        let frame = buf[start..start + len].to_vec();
        buf.drain(..start + len);
        return Some(frame);
    }
    let start = find(buf, &[0xFF, 0xD8])?;
    let end = start + find(&buf[start..], &[0xFF, 0xD9])? + 2;
    let frame = buf[start..end].to_vec();
    buf.drain(..end);
    Some(frame)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(body: &[u8]) -> Vec<u8> {
        let mut v = format!("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
        v.extend_from_slice(body);
        v.extend_from_slice(b"\r\n");
        v
    }

    #[test]
    fn parts_are_cut_by_their_length_even_when_split_in_pieces() {
        let first = [0xFF, 0xD8, 1, 2, 3, 0xFF, 0xD9];
        let second = [0xFF, 0xD8, 9, 0xFF, 0xD9];
        let mut stream = part(&first);
        stream.extend(part(&second));
        let mut buf = Vec::new();
        let mut got = Vec::new();
        for chunk in stream.chunks(7) {
            buf.extend_from_slice(chunk);
            while let Some(frame) = take_part(&mut buf) {
                got.push(frame);
            }
        }
        assert_eq!(got, vec![first.to_vec(), second.to_vec()]);
    }

    #[test]
    fn a_part_without_length_is_cut_at_the_jpeg_markers() {
        let mut buf = b"--frame\r\nContent-Type: image/jpeg\r\n\r\n".to_vec();
        buf.extend([0xFF, 0xD8, 5, 6, 0xFF, 0xD9, b'\r', b'\n']);
        assert_eq!(take_part(&mut buf), Some(vec![0xFF, 0xD8, 5, 6, 0xFF, 0xD9]));
    }

    #[test]
    fn an_unfinished_part_waits() {
        let mut buf = b"--frame\r\nContent-Length: 10\r\n\r\nabc".to_vec();
        assert_eq!(take_part(&mut buf), None);
    }

    #[test]
    fn devices_are_read_newest_first_with_booted_on_top() {
        let json = br#"{"devices":{
          "com.apple.CoreSimulator.SimRuntime.iOS-26-3":[
            {"name":"iPhone 17 Pro","udid":"AAAAAAAA-1","state":"Shutdown","isAvailable":true},
            {"name":"iPad Pro","udid":"BBBBBBBB-2","state":"Shutdown","isAvailable":true},
            {"name":"iPhone 16e","udid":"CCCCCCCC-3","state":"Booted","isAvailable":true},
            {"name":"iPhone Gone","udid":"DDDDDDDD-4","state":"Shutdown","isAvailable":false}],
          "com.apple.CoreSimulator.SimRuntime.watchOS-10-0":[
            {"name":"Apple Watch","udid":"EEEEEEEE-5","state":"Shutdown","isAvailable":true}]}}"#;
        let list = parse_devices(json).unwrap();
        let names: Vec<_> = list.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["iPhone 16e", "iPhone 17 Pro", "iPad Pro"]);
        assert_eq!(list[0].runtime, "iOS 26.3");
        assert!(list[0].booted && !list[1].booted);
    }

    #[test]
    fn a_control_message_is_a_code_byte_and_json() {
        let m = control_message(3, &serde_json::json!({ "type": "begin", "x": 0.5, "y": 0.25 }));
        assert_eq!(m[0], 3);
        let body: serde_json::Value = serde_json::from_slice(&m[1..]).unwrap();
        assert_eq!(body["type"], "begin");
        assert_eq!(body["x"], 0.5);
    }

    #[test]
    fn input_needs_a_live_simulator_and_known_things() {
        let sim = Simview::default();
        let touch = [SimTouch { phase: "begin".into(), x: 0.5, y: 0.5 }];
        // Nothing is live.
        assert!(sim.input("AAAA-1111", &touch, None).is_err());
        // Nothing but a simulator id reaches a command.
        assert!(sim.input("AAAA; rm -rf /", &touch, None).is_err());
        assert!(SimTouch { phase: "tap".into(), x: 0.0, y: 0.0 }.phase_name().is_err());
        assert!(SimTouch { phase: "move".into(), x: 0.0, y: 0.0 }.phase_name().is_ok());
    }

    #[test]
    fn only_a_simulator_id_reaches_a_command() {
        assert!(check_udid("3F2A9C1E-0B4D-4E57-9A10-6C8D2B7E1F00").is_ok());
        assert!(check_udid("x; rm -rf /").is_err());
        assert!(check_udid("").is_err());
    }
    #[test]
    fn the_app_actions_are_known_and_start_paired_on_loopback() {
        assert_eq!(AppAction::parse("start", None, None).unwrap(), AppAction::Start);
        assert_eq!(AppAction::parse("tab", None, Some(2)).unwrap(), AppAction::Tab(2));
        assert!(AppAction::parse("tab", None, Some(9)).is_err());
        assert!(AppAction::parse("tab", None, None).is_err());
        assert!(AppAction::parse("thread", Some("cmsg_01Ab-x"), None).is_ok());
        assert!(AppAction::parse("thread", Some("a b; rm"), None).is_err());
        assert!(AppAction::parse("thread", Some(""), None).is_err());
        assert!(AppAction::parse("erase", None, None).is_err());
        let args = AppAction::Tab(1).launch_args("ABCD1234", 7878).unwrap();
        assert_eq!(args, ["-pair", "http://127.0.0.1:7878/#pair=ABCD1234", "-tab", "1"]);
        assert!(AppAction::Thread("x".into()).launch_args("ABCD1234", 7878).is_none());
    }
}
