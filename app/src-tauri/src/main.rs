//! The app is a window onto the core. It starts `hubd` when the core is not already running,
//! and leaves it running on exit so threads keep going.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use hub_install::ignore::Ignore;

mod card;
mod live;
mod toast;

const CORE: ([u8; 4], u16) = ([127, 0, 0, 1], 7878);

fn main() {
    // A window that uses the Hub on another machine (Settings > About > Core) keeps no core here.
    if local_core() {
        ensure_core();
        watch_core();
    }
    let mut context = tauri::generate_context!();
    let embedded = context.set_assets(Box::new(Pages::placeholder()));
    context.set_assets(Box::new(Pages::new(embedded)));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            relaunch,
            open_dev_window,
            open_viewer_window,
            let_foreground,
            toast::toast_check,
            set_local_core,
            core_token,
            live::live_go,
            live::live_leave,
            live::live_beat,
            live::live_take
        ])
        .setup(|app| {
            use tauri::Manager;
            toast::register();
            if WINDOW_PING && local_core() && hub_install::running_version() != "dev" {
                // This machine has a window, so a version on probation must hear from the pages
                // before it counts as healthy (`POST /api/update/alive`). The clock starts now.
                let data = hub_install::data_dir();
                hub_install::probation::note_window_needed(&data, &hub_install::running_version(), hub_install::probation::now_ms());
                let handle = app.handle().clone();
                std::thread::spawn(move || follow_rollback(handle));
            }
            // The window the version before this one had open, so a restart keeps its place.
            if let (Ok(spot), Some(window)) = (std::env::var(WINDOW_VAR), app.get_webview_window("main")) {
                place(&window, &spot);
            }
            Ok(())
        })
        .run(context)
        .expect("error while running Hub");
}

/// The pages a window shows: the ones of the version the launcher points at, when that version
/// was built with this same shell (`SHELL` beside it, `crates/hub-core/src/shell.rs`), else the
/// ones built into this program. So an update that only changed the pages reloads the windows in
/// place instead of restarting the app.
struct Pages<R: tauri::Runtime> {
    embedded: Option<Box<dyn tauri::Assets<R>>>,
    /// This program's own shell stamp; none for a development build.
    stamp: Option<String>,
    /// The folder the page on screen came from, picked again each time a window loads.
    dir: std::sync::RwLock<Option<PathBuf>>,
}

/// The version whose pages the windows show, which an update has to move past.
static SHOWN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Locks that survive a poisoned lock, as `hub_core::locked` (the shell does not depend on
/// hub-core, so it keeps its own copy): a panic on one thread must not take the window down later.
trait LockExt<T> {
    fn locked(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> LockExt<T> for std::sync::Mutex<T> {
    fn locked(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

trait RwLockExt<T> {
    fn read_locked(&self) -> std::sync::RwLockReadGuard<'_, T>;
    fn write_locked(&self) -> std::sync::RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for std::sync::RwLock<T> {
    fn read_locked(&self) -> std::sync::RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    fn write_locked(&self) -> std::sync::RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl<R: tauri::Runtime> Pages<R> {
    fn placeholder() -> Self {
        Self { embedded: None, stamp: None, dir: Default::default() }
    }

    fn new(embedded: Box<dyn tauri::Assets<R>>) -> Self {
        let stamp = std::env::current_exe().ok().and_then(|p| hub_install::shell_stamp(p.parent()?));
        Self { embedded: Some(embedded), stamp, dir: Default::default() }
    }

    fn pick(&self) -> Option<PathBuf> {
        let data = hub_install::data_dir();
        let picked = self.stamp.as_ref().and_then(|own| {
            let dir = hub_install::current_dir(&data)?;
            let pages = dir.join(hub_install::UI_DIR);
            (hub_install::shell_stamp(&dir).as_ref() == Some(own) && pages.join("index.html").is_file()).then_some(pages)
        });
        let shown = match &picked {
            Some(_) => hub_install::current_version(&data).unwrap_or_else(hub_install::running_version),
            None => hub_install::running_version(),
        };
        // The clock for the pages to say they are alive starts when a window loads them. Only a
        // window that uses this machine's core can tell it so.
        if WINDOW_PING && local_core() && shown != "dev" {
            hub_install::probation::note_window_loaded(&data, &shown, hub_install::probation::now_ms());
        }
        *SHOWN.locked() = Some(shown);
        picked
    }
}

impl<R: tauri::Runtime> tauri::Assets<R> for Pages<R> {
    fn get(&self, key: &tauri::utils::assets::AssetKey) -> Option<std::borrow::Cow<'_, [u8]>> {
        let rel = key.as_ref().trim_start_matches('/');
        if rel == "index.html" {
            *self.dir.write_locked() = self.pick();
        }
        let safe = !rel.is_empty() && rel.split(['/', '\\']).all(|p| !p.is_empty() && p != "." && p != "..");
        if safe && let Some(dir) = self.dir.read_locked().as_ref() && let Ok(bytes) = std::fs::read(dir.join(rel)) {
            return Some(bytes.into());
        }
        self.embedded.as_ref()?.get(key)
    }

    fn iter(&self) -> Box<tauri::utils::assets::AssetsIter<'_>> {
        match &self.embedded {
            Some(e) => e.iter(),
            None => Box::new(std::iter::empty()),
        }
    }

    fn csp_hashes(&self, html_path: &tauri::utils::assets::AssetKey) -> Box<dyn Iterator<Item = tauri::utils::assets::CspHash<'_>> + '_> {
        // The built pages have no inline scripts or styles, so the built-in hashes cover any.
        match &self.embedded {
            Some(e) => e.csp_hashes(html_path),
            None => Box::new(std::iter::empty()),
        }
    }
}

/// Whether the pages can tell the core they are alive (`POST /api/update/alive`, D7 PR B and D).
/// Until they can, a window must not be marked as needed: it could never say so, and the version
/// would be rolled back after 90 seconds. The core takes the ping and `core_ok` now (D7 PR B).
const WINDOW_PING: bool = true;

/// Where the version before this one had its main window: `x,y,width,height,maximized`.
const WINDOW_VAR: &str = "HUB_WINDOW";

fn spot(window: &tauri::WebviewWindow) -> Option<String> {
    let at = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    let max = window.is_maximized().unwrap_or(false);
    Some(format!("{},{},{},{},{}", at.x, at.y, size.width, size.height, max as u8))
}

fn place(window: &tauri::WebviewWindow, spot: &str) {
    let n: Vec<i64> = spot.split(',').filter_map(|p| p.parse().ok()).collect();
    let [x, y, w, h, max] = n[..] else { return };
    if w < 200 || h < 200 {
        return;
    }
    // Putting the window back where it was is a nicety: if it fails, it opens where Tauri put it.
    window.set_position(tauri::PhysicalPosition::new(x as i32, y as i32)).ignore("the window keeps Tauri's position");
    window.set_size(tauri::PhysicalSize::new(w as u32, h as u32)).ignore("the window keeps Tauri's size");
    if max == 1 {
        window.maximize().ignore("the window stays unmaximized at its last size");
    }
}

/// Switches to the version the launcher points at, once the new core has taken over. The core
/// moves `current` only when the new version is up, so this waits for it a while. When the new
/// version has this same shell, every window reloads in place with its pages (see [`Pages`]):
/// same windows, places and pages. Otherwise the new app starts where this window was and this
/// one exits. Returns false when nothing newer came up (or this is a development build).
#[tauri::command]
async fn relaunch(app: tauri::AppHandle) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tauri::Manager;
    // Every window hears the core restart and asks; one switch does for all of them.
    static SWITCHING: AtomicBool = AtomicBool::new(false);
    if hub_install::running_version() == "dev" || SWITCHING.swap(true, Ordering::SeqCst) {
        return false;
    }
    let shown = SHOWN.locked().clone().unwrap_or_else(hub_install::running_version);
    // Hub looks closed while it installs: its windows go away and a small card on the desktop
    // says what is happening, until the new version is up (or nothing newer came).
    let hidden = card::show(&app);
    let data = hub_install::data_dir();
    let newer = tauri::async_runtime::spawn_blocking(move || {
        for _ in 0..120 {
            if let Some(current) = hub_install::current_version(&data)
                && current != shown
            {
                return hub_install::current_dir(&data);
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        None
    });
    let Ok(Some(dir)) = newer.await else {
        card::done(&app, &hidden).await;
        SWITCHING.store(false, Ordering::SeqCst);
        return false;
    };
    let own = std::env::current_exe().ok().and_then(|p| hub_install::shell_stamp(p.parent()?));
    let same_shell = own.is_some() && hub_install::shell_stamp(&dir) == own && dir.join(hub_install::UI_DIR).join("index.html").is_file();
    if same_shell {
        // Dev mode's window shows the dev clone, not these pages; it reconnects by itself.
        for (label, window) in app.webview_windows() {
            if label == "dev" || label == card::LABEL {
                continue;
            }
            // The live pages come from a server the new core starts again; the reload waits for it.
            if live::shows_live_pages(&window) {
                tauri::async_runtime::spawn_blocking(live::wait_for_pages)
                    .await
                    .ignore("wait_for_pages only waits; had it panicked, reloading now is still the best left to do");
            }
            // A window that does not reload keeps the old version's pages.
            if let Err(err) = window.reload() {
                eprintln!("could not reload the window {label} for the new version: {err}");
            }
        }
        card::done(&app, &hidden).await;
        SWITCHING.store(false, Ordering::SeqCst);
        return true;
    }
    // From the data folder, as the launcher does: a folder a running program is in cannot be
    // removed on Windows, and the updater removes old versions.
    let mut cmd = hub_install::proc::command(dir.join(hub_install::exe("hub-app")));
    cmd.current_dir(hub_install::data_dir());
    if let Some(spot) = app.get_webview_window("main").as_ref().and_then(spot) {
        cmd.env(WINDOW_VAR, spot);
    }
    match cmd.spawn() {
        Ok(_) => {
            app.exit(0);
            true
        }
        Err(err) => {
            eprintln!("could not start the new version: {err}");
            card::done(&app, &hidden).await;
            SWITCHING.store(false, Ordering::SeqCst);
            false
        }
    }
}

/// Asks the probation's verdict every few seconds (the same call the launcher makes before it
/// starts anything) and follows `current` when it moves back: a rollback done here, by the core or
/// by the launcher alike. `relaunch` does the rest, reloading the windows in place when the older
/// version has this shell, else starting the older app.
fn follow_rollback(app: tauri::AppHandle) {
    use hub_install::probation::{self, Acted};
    let data = hub_install::data_dir();
    loop {
        std::thread::sleep(Duration::from_secs(3));
        if !local_core() {
            continue;
        }
        let acted = probation::act(&data);
        if let Acted::NoTarget(reason) = &acted {
            eprintln!("the new version is failing ({reason}) but there is nothing to go back to");
        }
        let shown = SHOWN.locked().clone().unwrap_or_else(hub_install::running_version);
        let moved_back = matches!(acted, Acted::RolledBack(_))
            || (probation::read_rolled_back(&data).is_some_and(|r| r.from == shown) && hub_install::current_version(&data).is_some_and(|c| c != shown));
        if moved_back && !tauri::async_runtime::block_on(relaunch(app.clone())) {
            // Nothing came up to switch to: do not ask again at once.
            std::thread::sleep(Duration::from_secs(30));
        }
    }
}

/// Lets the next window another process brings up come to the front, such as the Explorer window
/// the core opens to show a file in its folder, which Windows otherwise opens behind Hub.
#[tauri::command]
fn let_foreground() {
    #[cfg(windows)]
    // SAFETY: a plain Win32 call with no pointers; ASFW_ANY (all bits set) means any process.
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(u32::MAX);
    }
}

/// Pops a viewer out of the main window into one of its own. What it shows travels as a script
/// that runs before the page, since an app URL keeps no query; the page is the app's own.
#[tauri::command]
async fn open_viewer_window(app: tauri::AppHandle, viewing: String) -> Result<(), String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    // Only data goes into the script: it must parse as JSON, and is written back out as JSON.
    let viewing: serde_json::Value = serde_json::from_str(&viewing).map_err(|e| e.to_string())?;
    let label = format!("viewer-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    tauri::WebviewWindowBuilder::new(&app, label, tauri::WebviewUrl::App("index.html".into()))
        .initialization_script(format!("window.__HUB_VIEWER__ = {viewing};"))
        .title("Hub · viewer")
        .inner_size(1000.0, 760.0)
        .min_inner_size(420.0, 320.0)
        .background_color(tauri::window::Color(0x1a, 0x19, 0x18, 0xff))
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Opens dev mode's live UI (`crates/hub-core/src/dev.rs`) in a window of its own, so a change
/// that breaks it leaves this one working. An open one is brought to the front and reloaded.
#[tauri::command]
async fn open_dev_window(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri::Manager;
    let url: tauri::Url = url.parse().map_err(|e| format!("{e}"))?;
    if let Some(window) = app.get_webview_window("dev") {
        window.navigate(url).map_err(|e| e.to_string())?;
        window.unminimize().ignore("set_focus below brings it forward, and says so if that fails");
        return window.set_focus().map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(&app, "dev", tauri::WebviewUrl::External(url))
        .title("Hub · dev")
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 560.0)
        .background_color(tauri::window::Color(0x1a, 0x19, 0x18, 0xff))
        .disable_drag_drop_handler()
        .decorations(false)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Where the app notes that it uses the Hub on another machine, so it starts no core here.
fn remote_home_flag() -> PathBuf {
    hub_install::data_dir().join("app-uses-another-hub")
}

fn local_core() -> bool {
    !remote_home_flag().exists()
}

/// The dev core's port (`hub_core::dev::CORE_PORT`); its data folder is `dev-core` in this one's.
const DEV_CORE_PORT: u16 = 7879;

/// The connection token of the core on this computer at `core` (the main one, or dev mode's), so
/// the pages show it like every other client and the core need not trust their origin alone
/// (`app/src/lib/home.ts`). Only a core on this machine; it waits a little for a core that is
/// still starting to write its token. None when there is none.
#[tauri::command]
async fn core_token(core: String) -> Option<String> {
    let url: tauri::Url = core.parse().ok()?;
    if !matches!(url.host_str(), Some("127.0.0.1" | "localhost")) {
        return None;
    }
    let data = hub_install::data_dir();
    let path = if url.port() == Some(DEV_CORE_PORT) { data.join("dev-core").join("token") } else { data.join("token") };
    tauri::async_runtime::spawn_blocking(move || {
        for _ in 0..40 {
            if let Ok(token) = std::fs::read_to_string(&path)
                && !token.trim().is_empty()
            {
                return Some(token.trim().to_owned());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        None
    })
    .await
    .ok()
    .flatten()
}

/// Whether to keep a core running on this computer: off while the window uses the Hub on another
/// machine, on again when it goes back to this one's (which starts it now).
#[tauri::command]
fn set_local_core(on: bool) -> Result<(), String> {
    let flag = remote_home_flag();
    if on {
        // A flag left behind keeps this window on the other machine's Hub: say so to the page.
        if let Err(err) = std::fs::remove_file(&flag)
            && err.kind() != std::io::ErrorKind::NotFound
        {
            return Err(format!("could not go back to this computer's Hub: {err}"));
        }
        ensure_core();
        Ok(())
    } else {
        if let Some(dir) = flag.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&flag, "Hub's window uses the Hub on another machine; see Settings > About > Core.\n").map_err(|e| e.to_string())
    }
}

/// Starts the core again when it has been gone for a while, e.g. after a crash. An update hands
/// over within seconds (the new core retries the port for 15), so only a longer gap means
/// nobody is coming.
fn watch_core() {
    std::thread::spawn(|| {
        let mut down_since: Option<std::time::Instant> = None;
        loop {
            std::thread::sleep(Duration::from_secs(5));
            if core_is_up() || !local_core() {
                down_since = None;
            } else if down_since.get_or_insert_with(std::time::Instant::now).elapsed() >= Duration::from_secs(25) {
                eprintln!("the core has been down for 25 seconds; starting it");
                ensure_core();
                down_since = None;
            }
        }
    });
}

fn core_is_up() -> bool {
    TcpStream::connect_timeout(&SocketAddr::from(CORE), Duration::from_millis(200)).is_ok()
}

fn ensure_core() {
    if core_is_up() {
        return;
    }
    let exe = if cfg!(windows) { "hubd.exe" } else { "hubd" };
    // Next to the app when installed, in the cargo target folder during development.
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe)))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from(exe));

    let mut cmd = hub_install::proc::command(&path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    match cmd.spawn() {
        Ok(_) => {
            for _ in 0..50 {
                if core_is_up() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        // The window still opens and shows that the core is unreachable.
        Err(err) => eprintln!("could not start {}: {err}", path.display()),
    }
}
