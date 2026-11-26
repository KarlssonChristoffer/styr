//! Live mode: the main window shows dev mode's live pages (the open pull requests that change only
//! the pages, served by Vite in the dev clone, `crates/hub-core/src/dev.rs`) instead of the
//! installed ones, and goes back with one click. The installed pages are never at risk: a live page
//! that stops showing signs of life, or says it broke, sends the window back by itself.
//!
//! Each origin keeps its own storage, so the page that leaves hands its storage (drafts, zoom,
//! where it was) to the one that comes, through `live_take`, together with why it came back when
//! it fell back.

use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use hub_install::ignore::Ignore;
use tauri::{AppHandle, Manager, Url};

/// Dev mode's live pages (`dev::PORT`). The window goes live there only.
const PORT: u16 = 1421;
/// How long the live pages get to come up: Vite bundles the packages on its first load.
const FIRST: Duration = Duration::from_secs(90);
/// How long the live pages may go without a sign of life before the window goes back.
const QUIET: Duration = Duration::from_secs(15);

#[derive(Default)]
struct Live {
    /// The installed pages, to go back to; set while the window is live.
    back: Option<Url>,
    /// By when the live pages must show a sign of life.
    due: Option<Instant>,
    /// What the next page to start takes over: `{ storage?, note? }` as JSON.
    parcel: Option<String>,
    watching: bool,
}

static LIVE: LazyLock<Mutex<Live>> = LazyLock::new(Default::default);

fn live() -> MutexGuard<'static, Live> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The window is on the live pages (dev mode's Vite), not the installed ones.
pub fn shows_live_pages(window: &tauri::WebviewWindow) -> bool {
    window.url().is_ok_and(|u| matches!(u.host_str(), Some("127.0.0.1" | "localhost")) && u.port() == Some(PORT))
}

/// One look at the live pages' server: does it answer a plain request with 200?
fn pages_answer(addr: std::net::SocketAddr) -> bool {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    let Ok(mut conn) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else { return false };
    conn.set_read_timeout(Some(Duration::from_secs(2))).ignore("it fails only for a zero timeout, and this one is two seconds");
    if conn.write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").is_err() {
        return false;
    }
    let mut head = [0u8; 12];
    conn.read_exact(&mut head).is_ok() && head.ends_with(b" 200")
}

/// Waits until the live pages' server answers, twice in a row with a pause between, so a reload
/// does not meet the gap while the core's restart starts the server again (the window would show
/// the browser's "refused the connection" page). Gives up after a minute and lets the reload go.
pub fn wait_for_pages() {
    // The core starts the server again as it comes up, a moment after it takes over.
    std::thread::sleep(Duration::from_secs(3));
    let until = Instant::now() + Duration::from_secs(60);
    let mut answered = 0;
    while Instant::now() < until {
        if pages_answer(std::net::SocketAddr::from(([127, 0, 0, 1], PORT))) {
            answered += 1;
            if answered == 2 {
                return;
            }
        } else {
            answered = 0;
        }
        std::thread::sleep(Duration::from_millis(1500));
    }
}

/// Shows the live pages at `url` in the main window. `parcel` is the installed page's storage.
#[tauri::command]
pub fn live_go(app: AppHandle, url: String, parcel: String) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("there is no main window")?;
    let mut to: Url = url.parse().map_err(|e| format!("{e}"))?;
    if !matches!(to.host_str(), Some("127.0.0.1" | "localhost")) || to.port() != Some(PORT) {
        return Err(format!("live pages come from dev mode, on port {PORT}"));
    }
    to.set_query(Some("carry"));
    {
        let mut live = live();
        if live.back.is_none() {
            let mut back = window.url().map_err(|e| e.to_string())?;
            back.set_path("/");
            back.set_query(None);
            back.set_fragment(None);
            live.back = Some(back);
        }
        live.parcel = Some(parcel);
        live.due = Some(Instant::now() + FIRST);
    }
    watch(app.clone());
    window.navigate(to).map_err(|e| e.to_string())
}

/// Back to the installed pages, carrying `parcel` (the live page's storage, and a note when it
/// broke). Nothing happens when the window is not live.
#[tauri::command]
pub fn live_leave(app: AppHandle, parcel: Option<String>) -> Result<(), String> {
    leave(&app, parcel)
}

/// A sign of life from a live page that is up and drawn. Answers whether the window is live.
#[tauri::command]
pub fn live_beat() -> bool {
    let mut live = live();
    if live.back.is_none() {
        return false;
    }
    live.due = Some(Instant::now() + QUIET);
    true
}

/// What the page starting now takes over from the one before it, once.
#[tauri::command]
pub fn live_take() -> Option<String> {
    live().parcel.take()
}

fn leave(app: &AppHandle, parcel: Option<String>) -> Result<(), String> {
    let back = {
        let mut live = live();
        let Some(back) = live.back.take() else { return Ok(()) };
        live.due = None;
        live.parcel = parcel;
        back
    };
    let window = app.get_webview_window("main").ok_or("there is no main window")?;
    let mut back = back;
    back.set_query(Some("carry"));
    window.navigate(back).map_err(|e| e.to_string())
}

/// Sends the window back when the live pages stop showing signs of life: they did not load,
/// broke without saying so, or Vite went away.
fn watch(app: AppHandle) {
    {
        let mut live = live();
        if live.watching {
            return;
        }
        live.watching = true;
    }
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let late = {
                let live = live();
                live.back.is_some() && live.due.is_some_and(|due| Instant::now() > due)
            };
            if late {
                let note = serde_json::json!({ "note": "The live pages stopped answering, so the window went back to the installed ones." });
                if let Err(err) = leave(&app, Some(note.to_string())) {
                    eprintln!("could not leave the live pages: {err}");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn a_server_that_answers_200_counts_and_one_that_is_gone_does_not() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for status in ["200 OK", "502 Bad Gateway"] {
                let (mut conn, _) = listener.accept().unwrap();
                let mut buf = [0u8; 256];
                let _ = conn.read(&mut buf);
                conn.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n").as_bytes()).unwrap();
            }
        });
        assert!(pages_answer(addr));
        assert!(!pages_answer(addr), "an error answer is not the page");
        server.join().unwrap();
        assert!(!pages_answer(addr), "nothing listens any more");
    }
}
