//! Keeps a voice chat quiet while dictating: Hub holds down a key that the chat app (Discord, say)
//! has as its "Push to Mute" keybind, and lets go when dictation ends. A held key cannot get out
//! of step the way a toggle would, and F13 to F24 are on no keyboard, so holding one never
//! changes what is typed meanwhile.

use crate::locked::LockExt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A held key is let go this long after the app last said dictation is still on, so a closed
/// window or a crash does not leave the chat muted.
const LEASE: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct Mute {
    /// The key held down and until when, while one is.
    held: std::sync::Arc<Mutex<Option<(u8, Instant)>>>,
}

impl Mute {
    /// Holds `key` (a name like "F14") down, or keeps holding it, for another lease.
    pub fn hold(&self, key: &str) -> anyhow::Result<()> {
        let vk = virtual_key(key).ok_or_else(|| anyhow::anyhow!("{key} is not a key Hub can hold (F13 to F24)"))?;
        let mut held = self.held.locked();
        match *held {
            Some((old, _)) if old == vk => {}
            Some((old, _)) => {
                press(old, false);
                press(vk, true);
            }
            None => {
                press(vk, true);
                self.release_when_stale();
            }
        }
        *held = Some((vk, Instant::now() + LEASE));
        Ok(())
    }

    pub fn release(&self) {
        if let Some((vk, _)) = self.held.locked().take() {
            press(vk, false);
        }
    }

    /// Presses and lets go of `key` once, so the chat app's keybind recorder can learn it.
    pub fn tap(key: &str) -> anyhow::Result<()> {
        let vk = virtual_key(key).ok_or_else(|| anyhow::anyhow!("{key} is not a key Hub can press (F13 to F24)"))?;
        press(vk, true);
        std::thread::sleep(Duration::from_millis(120));
        press(vk, false);
        Ok(())
    }

    fn release_when_stale(&self) {
        let held = self.held.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let mut h = held.locked();
                match *h {
                    Some((vk, until)) if Instant::now() > until => {
                        press(vk, false);
                        *h = None;
                        return;
                    }
                    Some(_) => {}
                    None => return,
                }
            }
        });
    }
}

impl Drop for Mute {
    fn drop(&mut self) {
        self.release();
    }
}

/// F13 to F24, the keys no keyboard has; anything else would clash with typing.
fn virtual_key(name: &str) -> Option<u8> {
    let n: u8 = name.trim().strip_prefix(['F', 'f'])?.parse().ok()?;
    (13..=24).contains(&n).then(|| 0x7C + (n - 13))
}

#[cfg(windows)]
fn press(vk: u8, down: bool) {
    #[link(name = "user32", kind = "raw-dylib")]
    unsafe extern "system" {
        fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize);
        fn MapVirtualKeyW(code: u32, map: u32) -> u32;
    }
    const KEYEVENTF_KEYUP: u32 = 0x0002;
    // SAFETY: plain calls with values; keybd_event only queues input.
    unsafe {
        let scan = MapVirtualKeyW(vk as u32, 0) as u8;
        keybd_event(vk, scan, if down { 0 } else { KEYEVENTF_KEYUP }, 0);
    }
}

#[cfg(not(windows))]
fn press(_vk: u8, _down: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_keys_no_keyboard_has() {
        assert_eq!(virtual_key("F13"), Some(0x7C));
        assert_eq!(virtual_key("f24"), Some(0x87));
        assert_eq!(virtual_key("F12"), None);
        assert_eq!(virtual_key("M"), None);
    }

    #[tokio::test]
    async fn holds_then_lets_go() {
        let mute = Mute::default();
        assert!(mute.hold("F1").is_err());
        mute.hold("F14").unwrap();
        assert_eq!(mute.held.locked().map(|(vk, _)| vk), Some(0x7D));
        mute.hold("F14").unwrap();
        mute.release();
        assert!(mute.held.locked().is_none());
    }
}
