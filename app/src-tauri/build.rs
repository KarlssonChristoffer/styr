// The app's own commands get permissions (`allow-<command>`), so a capability can grant them. The
// installed pages get all of them (`capabilities/default.json`); Live mode's pages, which come
// from Vite on dev mode's port, only what they need (`capabilities/live.json`). Tauri lets a page
// from another origin call no app command at all unless a capability names it.
const COMMANDS: &[&str] = &[
    "relaunch",
    "open_dev_window",
    "open_viewer_window",
    "let_foreground",
    "set_local_core",
    "core_token",
    "live_go",
    "live_leave",
    "live_beat",
    "live_take",
];

fn main() {
    let attributes = tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
