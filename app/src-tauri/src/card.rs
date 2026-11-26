//! The card Hub shows on the desktop while it installs a new version (`relaunch` in `main.rs`).
//! Hub's windows are hidden, so it looks closed, and a small window of its own stays on top
//! saying what happens, the way an installer does. It shows the app's pages in card mode
//! (`InstallWindow.svelte`, picked by `__HUB_INSTALL_CARD__`), which talk to no core.

use hub_install::ignore::Ignore;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub const LABEL: &str = "install";

/// The card's size in logical pixels: the card itself, nothing around it.
const SIZE: (f64, f64) = (360.0, 116.0);

/// Opens the card over the middle of the main window and hides every other window but dev mode's
/// (which shows the dev clone and reconnects by itself). Returns the windows it hid, to show again.
pub fn show(app: &AppHandle) -> Vec<WebviewWindow> {
    let main = app.get_webview_window("main");
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .initialization_script("window.__HUB_INSTALL_CARD__ = true;")
        .title("Hub · installing")
        .inner_size(SIZE.0, SIZE.1)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .background_color(tauri::window::Color(0x23, 0x22, 0x21, 0xff));
    match main.as_ref().and_then(middle) {
        Some((x, y)) => builder = builder.position(x, y),
        None => builder = builder.center(),
    }
    if builder.build().is_err() {
        // Without the card the windows stay, and their pages show the card themselves.
        return Vec::new();
    }
    let mut hidden = Vec::new();
    for (label, window) in app.webview_windows() {
        if label == LABEL || label == "dev" || !window.is_visible().unwrap_or(false) {
            continue;
        }
        if window.hide().is_ok() {
            hidden.push(window);
        }
    }
    hidden
}

/// Brings back the windows the card hid and closes it. A reloaded window gets a moment to draw its
/// page first, so it does not come back blank.
pub async fn done(app: &AppHandle, hidden: &[WebviewWindow]) {
    if !hidden.is_empty() {
        tauri::async_runtime::spawn_blocking(|| std::thread::sleep(std::time::Duration::from_millis(700))).await.ok();
    }
    // A window that does not come back is Hub gone from the screen: say so.
    for window in hidden {
        if let Err(err) = window.show() {
            eprintln!("could not show the window {} again after the update card: {err}", window.label());
        }
    }
    if let Some(main) = hidden.iter().find(|w| w.label() == "main") {
        main.set_focus().ignore("focus is a nicety; Windows may refuse to hand it over, and the window is shown");
    }
    if let Some(card) = app.get_webview_window(LABEL)
        && let Err(err) = card.close()
    {
        eprintln!("could not close the update card: {err}");
    }
}

/// Where the card goes so it sits in the middle of the main window, in logical pixels.
fn middle(main: &WebviewWindow) -> Option<(f64, f64)> {
    let scale = main.scale_factor().ok()?;
    let at = main.outer_position().ok()?.to_logical::<f64>(scale);
    let size = main.outer_size().ok()?.to_logical::<f64>(scale);
    Some((at.x + (size.width - SIZE.0) / 2.0, at.y + (size.height - SIZE.1) / 2.0))
}
