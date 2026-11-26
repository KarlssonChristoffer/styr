//! Windows shows a toast only for an app it knows by an AppUserModelID. Hub runs from a launcher
//! and is not installed the usual way, so it may have none: the sound plays and nothing is drawn,
//! not even in the Action Center. `register` gives the id its name, `toast_check` says why a toast
//! would still not show.

/// The id the notification plugin sends toasts under: the app's identifier.
#[cfg(windows)]
const APP_ID: &str = "se.karlsson.hub";

#[cfg(windows)]
fn reg(args: &[&str]) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let out = hub_install::proc::command("reg.exe").args(args).creation_flags(0x0800_0000).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The DWORD under a key, when set.
#[cfg(windows)]
fn dword(key: &str, name: &str) -> Option<u32> {
    let out = reg(&["query", key, "/v", name])?;
    let hex = out.split_whitespace().last()?.strip_prefix("0x")?.to_string();
    u32::from_str_radix(&hex, 16).ok()
}

/// Names Hub to Windows, so its toasts are drawn and listed under Settings > Notifications.
pub fn register() {
    #[cfg(windows)]
    {
        let key = format!(r"HKCU\Software\Classes\AppUserModelId\{APP_ID}");
        reg(&["add", &key, "/v", "DisplayName", "/t", "REG_SZ", "/d", "Hub", "/f"]);
        reg(&["add", &key, "/v", "ShowInSettings", "/t", "REG_DWORD", "/d", "1", "/f"]);
    }
}

/// Registers Hub and says what would keep a toast from showing, or an empty string when nothing
/// is known to. The window shows this beside the result of "Send test".
#[tauri::command]
pub fn toast_check() -> String {
    #[cfg(windows)]
    {
        register();
        if dword(r"HKCU\Software\Microsoft\Windows\CurrentVersion\PushNotifications", "ToastEnabled") == Some(0) {
            return "Windows has notifications switched off (Settings > System > Notifications).".into();
        }
        let own = format!(r"HKCU\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\{APP_ID}");
        if dword(&own, "Enabled") == Some(0) {
            return "Notifications from Hub are switched off in Windows (Settings > System > Notifications > Hub).".into();
        }
        // SAFETY: a plain Win32 call that fills one out value.
        let mut state = 0i32;
        let ok = unsafe { windows_sys::Win32::UI::Shell::SHQueryUserNotificationState(&mut state) } >= 0;
        if ok {
            return match state {
                2 | 3 => "Windows is holding notices back because an app is full screen.".into(),
                4 => "Windows is holding notices back because you are presenting.".into(),
                6 => "Windows is holding notices back: Do not disturb or Focus is on, so the sound plays and no notice is drawn.".into(),
                _ => String::new(),
            };
        }
    }
    String::new()
}
