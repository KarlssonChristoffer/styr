//! Lets an agent see a live page before (or after) it shows it: `page_check` opens the page in a
//! headless Chrome, Edge or Chromium already on the machine, and answers with a screenshot, the
//! page's height and what went wrong in it: script errors, `console.error`, and what the page's
//! sandbox blocked. The browser is driven over the DevTools protocol, the same way on every
//! system, with a profile of its own that is thrown away after, so it never touches the user's.
//!
//! The page is loaded from this core's own page route (`pages.rs`), so it runs under exactly the
//! sandbox and CSP it gets in the thread.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use futures::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

use crate::locked::Ignore;

/// The chat column a page sits in.
pub const DEFAULT_WIDTH: u32 = 720;
/// The tallest screenshot; a taller page is cut there and says so.
const MAX_SHOT: u32 = 2400;
/// The most problems told back; the rest are counted.
const MAX_PROBLEMS: usize = 20;

/// How the words beside the picture start, once the page was looked at.
pub const RENDERED: &str = "Rendered at ";
/// What they say when nothing went wrong in the page. Proof before done reads both (`proof.rs`).
pub const CLEAN: &str = "No script errors, console errors or blocked loads.";

/// What a check found.
pub struct Checked {
    pub width: u32,
    pub height: u32,
    /// A PNG, base64.
    pub png: String,
    pub problems: Vec<String>,
}

impl Checked {
    /// The words the agent reads beside the picture.
    pub fn summary(&self) -> String {
        let cut = if self.height > MAX_SHOT { format!(" (the picture shows the top {MAX_SHOT} px)") } else { String::new() };
        let mut out = format!("{RENDERED}{}×{} px{cut}.", self.width, self.height);
        if self.problems.is_empty() {
            out.push(' ');
            out.push_str(CLEAN);
        } else {
            out.push_str(&format!(" {} problem{}:", self.problems.len(), if self.problems.len() == 1 { "" } else { "s" }));
            for p in self.problems.iter().take(MAX_PROBLEMS) {
                out.push_str("\n- ");
                out.push_str(p);
            }
            if self.problems.len() > MAX_PROBLEMS {
                out.push_str(&format!("\n- … and {} more", self.problems.len() - MAX_PROBLEMS));
            }
        }
        out
    }
}

/// A Chromium-family browser on this machine: `HUB_BROWSER` if set, else Edge (on every Windows),
/// Chrome or Chromium where they usually live.
pub fn find_browser() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("HUB_BROWSER").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let mut spots: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var).map(PathBuf::from) {
                spots.push(base.join(r"Microsoft\Edge\Application\msedge.exe"));
                spots.push(base.join(r"Google\Chrome\Application\chrome.exe"));
                spots.push(base.join(r"Chromium\Application\chrome.exe"));
            }
        }
    } else if cfg!(target_os = "macos") {
        for app in ["Google Chrome.app/Contents/MacOS/Google Chrome", "Microsoft Edge.app/Contents/MacOS/Microsoft Edge", "Chromium.app/Contents/MacOS/Chromium"] {
            spots.push(Path::new("/Applications").join(app));
        }
    }
    if let Some(found) = spots.into_iter().find(|p| p.is_file()) {
        return Some(found);
    }
    let names = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "microsoft-edge", "microsoft-edge-stable", "chrome"];
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).flat_map(|dir| names.iter().map(move |n| dir.join(n))).find(|p| p.is_file())
}

/// Opens `url` at `width` CSS pixels and looks at it.
pub async fn check(url: &str, width: u32) -> Result<Checked> {
    let browser = find_browser().ok_or_else(|| {
        anyhow!("page_check needs Chrome, Edge or Chromium on the machine the thread runs on, and found none (set HUB_BROWSER to its path)")
    })?;
    let profile = std::env::temp_dir().join(format!("hub-page-check-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&profile).context("making a profile for the check")?;
    let result = tokio::time::timeout(Duration::from_secs(45), run(&browser, &profile, url, width))
        .await
        .unwrap_or_else(|_| Err(anyhow!("the page did not finish loading in 45 seconds")));
    remove_profile(&profile).await;
    result
}

/// Deletes the browser's profile. The browser lets go of it a moment after it ends, so this waits
/// for that here: left to a task of its own it was lost whenever the runtime ended first, as
/// every test run's does, and each left half a gigabyte of Edge's profile behind. Only what
/// still holds on after that is retried in the background.
async fn remove_profile(dir: &Path) {
    for _ in 0..20 {
        let d = dir.to_owned();
        if tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&d).is_ok() || !d.exists()).await.unwrap_or(false) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let dir = dir.to_owned();
    tokio::spawn(async move {
        for _ in 0..60 {
            if std::fs::remove_dir_all(&dir).is_ok() || !dir.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

/// Starts the headless browser on a profile of its own.
fn start_browser(browser: &Path, profile: &Path, width: u32) -> Result<tokio::process::Child> {
    let mut cmd = crate::proc::tokio_command(browser);
    cmd.arg("--headless")
        .arg("--remote-debugging-port=0")
        .arg(format!("--user-data-dir={}", profile.display()))
        .args(["--no-first-run", "--no-default-browser-check", "--disable-gpu", "--hide-scrollbars", "--mute-audio"])
        .args(["--disable-extensions", "--disable-sync", "--disable-background-networking"])
        .arg(format!("--window-size={width},800"))
        .arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Chrome refuses to run as root with its own sandbox, as on a server's hubd. The page keeps
    // the sandbox and CSP it is served with.
    if as_root() {
        cmd.arg("--no-sandbox");
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd.spawn().with_context(|| format!("starting {}", browser.display()))
}

/// Waits for the browser to say where its debugging socket is.
async fn debug_endpoint(child: &mut tokio::process::Child, profile: &Path) -> Result<String> {
    // The browser writes the port it took, and the path to it, into its profile.
    let active = profile.join("DevToolsActivePort");
    let mut endpoint = None;
    let mut handed_on = false;
    for _ in 0..150 {
        if let Ok(text) = std::fs::read_to_string(&active) {
            let mut lines = text.lines();
            if let (Some(port), Some(path)) = (lines.next(), lines.next()) {
                endpoint = Some(format!("ws://127.0.0.1:{}{}", port.trim(), path.trim()));
                break;
            }
        }
        // Edge and Chrome can start a fresh copy of themselves and end with success, as when an
        // update is waiting; the copy keeps the arguments, so it still writes into the profile.
        if !handed_on && let Some(status) = child.try_wait()? {
            if !status.success() {
                bail!("the browser ended before it was ready ({status})");
            }
            handed_on = true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    endpoint.ok_or_else(|| match handed_on {
        true => anyhow!("the browser ended before it was ready (exit code: 0) and nothing took over"),
        false => anyhow!("the browser did not open its debugging port"),
    })
}

async fn run(browser: &Path, profile: &Path, url: &str, width: u32) -> Result<Checked> {
    let mut child = start_browser(browser, profile, width)?;
    let endpoint = debug_endpoint(&mut child, profile).await?;
    let (ws, _) = tokio_tungstenite::connect_async(endpoint.as_str()).await.context("connecting to the browser")?;
    let mut cdp = Cdp { ws, next: 0, problems: Vec::new(), loaded: false };

    let target = cdp.call(None, "Target.createTarget", json!({ "url": "about:blank" })).await?;
    let target = target["targetId"].as_str().context("no target")?.to_owned();
    let attached = cdp.call(None, "Target.attachToTarget", json!({ "targetId": target, "flatten": true })).await?;
    let session = attached["sessionId"].as_str().context("no session")?.to_owned();
    let s = Some(session.as_str());
    for method in ["Page.enable", "Runtime.enable", "Log.enable"] {
        cdp.call(s, method, json!({})).await?;
    }
    let metrics = |height: u32| json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false });
    cdp.call(s, "Emulation.setDeviceMetricsOverride", metrics(800)).await?;
    // Where the page paints nothing, the picture shows Hub's own background, not the browser's white.
    cdp.call(s, "Emulation.setDefaultBackgroundColorOverride", json!({ "color": { "r": 26, "g": 25, "b": 24, "a": 1 } })).await?;
    let nav = cdp.call(s, "Page.navigate", json!({ "url": url })).await?;
    if let Some(err) = nav["errorText"].as_str() {
        bail!("the page did not load: {err}");
    }
    // Wait for the load, then a little for scripts and animations to settle.
    let waited = tokio::time::Instant::now();
    while !cdp.loaded && waited.elapsed() < Duration::from_secs(20) {
        cdp.pump(Duration::from_millis(200)).await?;
    }
    if !cdp.loaded {
        cdp.problems.push("The page's load event never fired within 20 seconds.".into());
    }
    cdp.pump(Duration::from_millis(1200)).await?;

    // Measured in a short window, so the height is the page's own and not the window's.
    cdp.call(s, "Emulation.setDeviceMetricsOverride", metrics(60)).await?;
    let measured = cdp
        .call(s, "Runtime.evaluate", json!({
            "expression": "Math.ceil(Math.max(document.documentElement.scrollHeight, document.body ? document.body.scrollHeight : 0))",
            "returnByValue": true,
        }))
        .await?;
    let mut height = measured["result"]["value"].as_f64().unwrap_or(800.0).max(1.0) as u32;
    cdp.call(s, "Emulation.setDeviceMetricsOverride", metrics(height.clamp(60, MAX_SHOT))).await?;
    cdp.pump(Duration::from_millis(300)).await?;
    // A page sized by its window (vh units, a chart that fills it) can come out shorter once the
    // window has grown to the measured height: measure again so the picture has no empty strip.
    let again = cdp
        .call(s, "Runtime.evaluate", json!({
            "expression": "Math.ceil(Math.max(document.documentElement.getBoundingClientRect().height, document.body ? document.body.getBoundingClientRect().height : 0))",
            "returnByValue": true,
        }))
        .await?;
    if let Some(h) = again["result"]["value"].as_f64().map(|h| h as u32)
        && h >= 60
        && h + 4 < height
    {
        height = h;
        cdp.call(s, "Emulation.setDeviceMetricsOverride", metrics(height.clamp(60, MAX_SHOT))).await?;
        cdp.pump(Duration::from_millis(200)).await?;
    }
    let shot = cdp.call(s, "Page.captureScreenshot", json!({ "format": "png" })).await?;
    let png = shot["data"].as_str().context("no screenshot")?.to_owned();
    cdp.call(None, "Browser.close", json!({})).await.ignore("the browser may close before it answers");
    tokio::time::timeout(Duration::from_secs(3), child.wait()).await.ignore("a browser still open after 3 s is killed below");
    child.kill().await.ignore("the browser has usually ended by now");
    Ok(Checked { width, height, png, problems: dedup(cdp.problems) })
}

#[cfg(unix)]
fn as_root() -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").is_ok_and(|m| m.uid() == 0)
}

#[cfg(not(unix))]
fn as_root() -> bool {
    false
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A DevTools connection that notes what goes wrong in the page while it waits for answers.
struct Cdp {
    ws: Socket,
    next: u64,
    problems: Vec<String>,
    loaded: bool,
}

impl Cdp {
    async fn call(&mut self, session: Option<&str>, method: &str, params: Value) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        self.ws.send(Message::text(msg.to_string())).await?;
        loop {
            let m = self.read().await?.ok_or_else(|| anyhow!("the browser closed the connection"))?;
            if m["id"].as_u64() == Some(id) {
                if let Some(e) = m.get("error") {
                    bail!("{method}: {}", e["message"].as_str().unwrap_or("failed"));
                }
                return Ok(m["result"].clone());
            }
            self.note(&m);
        }
    }

    /// Takes in events for `wait`.
    async fn pump(&mut self, wait: Duration) -> Result<()> {
        let until = tokio::time::Instant::now() + wait;
        while let Ok(m) = tokio::time::timeout_at(until, self.read()).await {
            match m? {
                Some(m) => self.note(&m),
                None => break,
            }
        }
        Ok(())
    }

    async fn read(&mut self) -> Result<Option<Value>> {
        while let Some(m) = self.ws.next().await {
            if let Message::Text(t) = m? {
                return Ok(Some(serde_json::from_str(&t)?));
            }
        }
        Ok(None)
    }

    fn note(&mut self, m: &Value) {
        let p = &m["params"];
        match m["method"].as_str().unwrap_or_default() {
            "Page.loadEventFired" => self.loaded = true,
            "Runtime.exceptionThrown" => self.problems.push(exception(&p["exceptionDetails"])),
            "Runtime.consoleAPICalled" if p["type"] == "error" || p["type"] == "assert" => {
                let args: Vec<String> = p["args"].as_array().into_iter().flatten().map(remote_text).collect();
                self.problems.push(format!("console.error: {}", crate::text::clip(args.join(" ").trim(), CLIP)));
            }
            "Log.entryAdded" if p["entry"]["level"] == "error" => {
                let e = &p["entry"];
                let text = e["text"].as_str().unwrap_or_default();
                let url = e["url"].as_str().filter(|u| !u.is_empty() && !u.starts_with("http://127.0.0.1") && !u.starts_with("http://["));
                self.problems.push(match url {
                    Some(u) => format!("{} ({u})", crate::text::clip(text.trim(), CLIP)),
                    None => crate::text::clip(text.trim(), CLIP),
                });
            }
            _ => {}
        }
    }
}

/// A thrown error as the agent can find it: message and line in its own document.
fn exception(d: &Value) -> String {
    let what = d["exception"]["description"].as_str().or(d["text"].as_str()).unwrap_or("An error");
    let first = what.lines().next().unwrap_or(what);
    // Hub's bridge sits on the document's first line, so lines count as in the agent's page.
    match d["lineNumber"].as_u64() {
        Some(line) => format!("{} (line {})", crate::text::clip(first.trim(), CLIP), line + 1),
        None => crate::text::clip(first.trim(), CLIP),
    }
}

fn remote_text(v: &Value) -> String {
    match &v["value"] {
        Value::String(s) => s.clone(),
        Value::Null => v["description"].as_str().unwrap_or("").to_owned(),
        other => other.to_string(),
    }
}

const CLIP: usize = 300;

fn dedup(problems: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    problems.into_iter().filter(|p| seen.insert(p.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_says_size_and_problems() {
        let ok = Checked { width: 720, height: 300, png: String::new(), problems: vec![] };
        assert_eq!(ok.summary(), "Rendered at 720×300 px. No script errors, console errors or blocked loads.");
        let bad = Checked { width: 720, height: 3000, png: String::new(), problems: vec!["ReferenceError: x is not defined (line 4)".into()] };
        let s = bad.summary();
        assert!(s.contains("top 2400 px") && s.contains("1 problem:\n- ReferenceError"), "{s}");
    }

    #[test]
    fn exceptions_name_the_line_in_the_agents_page() {
        let d = json!({ "lineNumber": 3, "exception": { "description": "ReferenceError: x is not defined\n    at <anonymous>:4:1" } });
        assert_eq!(exception(&d), "ReferenceError: x is not defined (line 4)");
    }

    #[tokio::test]
    async fn the_profile_is_gone_when_the_check_returns() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("profile");
        std::fs::create_dir_all(dir.join("Default/Cache")).unwrap();
        std::fs::write(dir.join("Default/Cache/x"), "abc").unwrap();
        remove_profile(&dir).await;
        assert!(!dir.exists());
    }

    /// Runs where a browser is found (or `HUB_BROWSER` names one); passes quietly elsewhere.
    #[tokio::test]
    async fn a_broken_page_is_seen_with_its_error() {
        if find_browser().is_none() {
            return;
        }
        let page = "data:text/html,<body style='height:500px'><p>hi</p><script>console.error('oops');nope()</script></body>";
        let seen = check(page, 400).await.unwrap();
        assert_eq!(seen.width, 400);
        assert!(seen.height >= 500, "{}", seen.height);
        assert!(seen.png.starts_with("iVBOR"), "a PNG");
        let s = seen.summary();
        assert!(s.contains("console.error: oops") && s.contains("nope is not defined"), "{s}");
    }
}
