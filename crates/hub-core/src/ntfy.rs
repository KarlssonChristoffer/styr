//! Notices on the phone through the ntfy app, which is free: Apple only lets paid developer
//! accounts push, so hubd posts to an ntfy topic instead and the phone's ntfy app shows it. A tap
//! opens the thread in Hub's iPhone app (`hub://thread/<id>`).
//!
//! The iPhone app sets this up per machine (`GET`/`PUT /api/phone/notify`); each machine sends for
//! its own threads only, so a thread is announced once. Quiet hours hold back what is done, never
//! what waits for you. While the app is in front it says which thread it shows
//! (`POST /api/phone/looking`), and nothing is sent about that thread, nor "Done" for the others.
//! The desktop does the same for the thread open in its window while you are at the computer
//! (`POST /api/presence`): no notice on the phone about what you see in front of you, and a
//! question or approval that came while you looked is checked again before it is let go.

use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::ctx::Ctx;
use crate::model::{Thread, ThreadStatus};

const KEY: &str = "phone.ntfy";

/// What the phone asked for. An empty topic sends nothing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Notify {
    pub server: String,
    pub topic: String,
    /// A permission question.
    pub approvals: bool,
    /// The agent asks something and waits.
    pub questions: bool,
    /// Failed, or stopped at a plan limit.
    pub failures: bool,
    /// A turn finished.
    pub done: bool,
    /// Hours (0 to 23, on the phone's clock) when only what waits for you comes through.
    pub quiet_from: Option<u8>,
    pub quiet_to: Option<u8>,
    /// The phone's offset from UTC in minutes, so quiet hours follow its clock, not the machine's.
    pub utc_offset_min: i32,
    /// No notice about the thread open in Hub on the computer you are using (`Presence`).
    pub quiet_at_pc: bool,
}

impl Default for Notify {
    fn default() -> Self {
        Notify {
            server: "https://ntfy.sh".into(),
            topic: String::new(),
            approvals: true,
            questions: true,
            failures: true,
            done: true,
            quiet_from: None,
            quiet_to: None,
            utc_offset_min: 0,
            quiet_at_pc: true,
        }
    }
}

pub async fn load(ctx: &Ctx<'_>) -> Notify {
    load_from(ctx.store).await
}

async fn load_from(store: &crate::store::Store) -> Notify {
    match store.get_value(KEY).await {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        _ => Notify::default(),
    }
}

pub async fn save(ctx: &Ctx<'_>, notify: &Notify) -> Result<()> {
    ctx.store.set_value(KEY, &serde_json::to_string(notify)?).await
}

/// What a status change is worth telling, if anything.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Approval,
    Question,
    Failed,
    Done,
}

pub fn kind(before: ThreadStatus, thread: &Thread, asks_permission: bool) -> Option<Kind> {
    match (before, thread.status) {
        (b, ThreadStatus::WaitingOnYou) if b != ThreadStatus::WaitingOnYou => {
            Some(if asks_permission { Kind::Approval } else { Kind::Question })
        }
        (b, ThreadStatus::Error) if b != ThreadStatus::Error => Some(Kind::Failed),
        (ThreadStatus::Running, ThreadStatus::Idle) => Some(Kind::Done),
        _ => None,
    }
}

/// Whether `n` wants this kind at `hour` (0 to 23).
pub fn wanted(n: &Notify, kind: Kind, hour: u8) -> bool {
    if n.topic.trim().is_empty() {
        return false;
    }
    let on = match kind {
        Kind::Approval => n.approvals,
        Kind::Question => n.questions,
        Kind::Failed => n.failures,
        Kind::Done => n.done,
    };
    if !on {
        return false;
    }
    let quiet = match (n.quiet_from, n.quiet_to) {
        (Some(from), Some(to)) if from != to => {
            if from < to { hour >= from && hour < to } else { hour >= from || hour < to }
        }
        _ => false,
    };
    // Approvals and questions still come through at night: the work waits on them.
    !quiet || matches!(kind, Kind::Approval | Kind::Question)
}

/// The hour on the phone's clock at `now_ms`.
pub fn phone_hour(now_ms: i64, utc_offset_min: i32) -> u8 {
    let minutes = now_ms / 60_000 + i64::from(utc_offset_min);
    (minutes.rem_euclid(24 * 60) / 60) as u8
}

/// What the phone has on screen, as it last said (`POST /api/phone/looking`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Looking {
    /// The app is in front.
    pub active: bool,
    /// The thread open in it, if one is.
    pub thread: Option<String>,
}

/// How long the phone's word holds: it says it again every minute while it is in front, so a phone
/// that went away without saying so hears about everything again soon.
const LOOK_MS: i64 = 90_000;

static LOOK: Mutex<Option<(Looking, i64)>> = Mutex::new(None);

pub fn look(l: Looking) {
    let until = crate::model::now_ms() + LOOK_MS;
    *LOOK.lock().unwrap_or_else(|e| e.into_inner()) = l.active.then_some((l, until));
}

/// Whether the phone, looking at the app now, has no need of this notice: anything about the thread
/// on its screen (the reply comes in before its eyes), and "Done" for the others (the inbox shows it).
pub fn held_back(look: Option<&(Looking, i64)>, thread_id: &str, kind: Kind, now_ms: i64) -> bool {
    let Some((l, until)) = look else { return false };
    if !l.active || now_ms > *until {
        return false;
    }
    l.thread.as_deref() == Some(thread_id) || kind == Kind::Done
}

/// What a computer's Hub window shows, as it last said (`POST /api/presence`).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Presence {
    /// Tells windows apart, so two computers do not overwrite each other.
    pub client: String,
    /// The window is in front.
    pub active: bool,
    /// The thread open in it.
    pub thread: Option<String>,
    /// How long ago the last key or mouse movement was, when it said so.
    pub idle_ms: i64,
}

impl Default for Presence {
    fn default() -> Self {
        Presence { client: "desktop".into(), active: false, thread: None, idle_ms: 0 }
    }
}

/// A computer is "at" Hub while its window is in front and the last key or mouse movement is
/// younger than this.
pub const AT_PC_MS: i64 = 180_000;
/// How long a question or approval that came while you looked waits before it is checked again.
pub const RECHECK: Duration = Duration::from_secs(45);

static PRESENCE: Mutex<Vec<(Presence, i64)>> = Mutex::new(Vec::new());

pub fn present(p: Presence) {
    let now = crate::model::now_ms();
    let mut all = PRESENCE.lock().unwrap_or_else(|e| e.into_inner());
    all.retain(|(o, until)| o.client != p.client && *until > now);
    if p.active {
        all.push((p, now + LOOK_MS));
    }
}

/// Whether a window of Hub in front of someone shows this thread, going by what the windows said:
/// the word is not too old and the keys or mouse were used lately. The idle time they sent grows
/// by the time since they sent it.
pub fn at_pc(all: &[(Presence, i64)], thread_id: &str, now_ms: i64) -> bool {
    all.iter().any(|(p, until)| {
        let age = now_ms - (until - LOOK_MS);
        p.active && now_ms <= *until && p.idle_ms + age < AT_PC_MS && p.thread.as_deref() == Some(thread_id)
    })
}

static CLIENT: LazyLock<reqwest::Client> =
    LazyLock::new(|| reqwest::Client::builder().timeout(Duration::from_secs(10)).build().unwrap_or_default());

/// Posts one notice. ntfy takes the title, tags and the link a tap opens as headers.
pub async fn send(n: &Notify, title: &str, body: &str, click: Option<&str>, urgent: bool) -> Result<()> {
    let url = format!("{}/{}", n.server.trim_end_matches('/'), n.topic.trim());
    let mut req = CLIENT.post(url).header("Title", title).body(body.to_owned());
    if let Some(click) = click {
        req = req.header("Click", click);
    }
    if urgent {
        req = req.header("Priority", "high");
    }
    let answer = req.send().await?;
    anyhow::ensure!(answer.status().is_success(), "ntfy answered {}", answer.status());
    Ok(())
}

/// Called as a thread's status changes, next to the desk note (`Hub::set_status`).
pub async fn note(ctx: &Ctx<'_>, before: ThreadStatus, thread: &Thread) {
    // Side jobs report to their parent, and a thread on another machine is told by that machine.
    if thread.parent.is_some() || thread.host.is_some() {
        return;
    }
    let n = load(ctx).await;
    if n.topic.trim().is_empty() {
        return;
    }
    let asks_permission = matches!(thread.status, ThreadStatus::WaitingOnYou) && waits_for_permission(ctx.store, &thread.id).await;
    let Some(kind) = kind(before, thread, asks_permission) else { return };
    let now = crate::model::now_ms();
    let look = LOOK.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if held_back(look.as_ref(), &thread.id, kind, now) {
        return;
    }
    let hour = phone_hour(now, n.utc_offset_min);
    if !wanted(&n, kind, hour) {
        return;
    }
    if n.quiet_at_pc && at_pc(&PRESENCE.lock().unwrap_or_else(|e| e.into_inner()), &thread.id, now) {
        // Done and failed are in front of you. What waits for you is looked at again: if you have
        // left by then and it still waits, it goes out.
        if matches!(kind, Kind::Approval | Kind::Question) {
            let (store, id) = (ctx.store.clone(), thread.id.clone());
            tokio::spawn(async move {
                tokio::time::sleep(RECHECK).await;
                let Ok(Some(now_thread)) = store.thread(&id).await else { return };
                if now_thread.status != ThreadStatus::WaitingOnYou {
                    return;
                }
                if at_pc(&PRESENCE.lock().unwrap_or_else(|e| e.into_inner()), &id, crate::model::now_ms()) {
                    return;
                }
                let n = load_from(&store).await;
                deliver(&n, kind, &now_thread);
            });
        }
        return;
    }
    deliver(&n, kind, thread);
}

fn deliver(n: &Notify, kind: Kind, thread: &Thread) {
    let project = std::path::Path::new(&thread.cwd).file_name().and_then(|s| s.to_str()).unwrap_or("Hub").to_owned();
    let what = match kind {
        Kind::Approval => "Needs your approval",
        Kind::Question => "Needs you",
        Kind::Failed => "Failed",
        Kind::Done => "Done",
    };
    let title = format!("{project} · {what}");
    let body = if thread.title.trim().is_empty() { "A thread".to_owned() } else { thread.title.clone() };
    let click = format!("hub://thread/{}", thread.id);
    let urgent = matches!(kind, Kind::Approval | Kind::Question);
    let n2 = n.clone();
    tokio::spawn(async move {
        if let Err(err) = send(&n2, &title, &body, Some(&click), urgent).await {
            tracing::warn!("ntfy: notice not sent: {err:#}");
        }
    });
}

async fn waits_for_permission(store: &crate::store::Store, thread_id: &str) -> bool {
    use crate::model::EventKind;
    let Ok(events) = store.events(thread_id, 0, 100_000).await else { return false };
    events.iter().rev().find(|e| e.kind == EventKind::PermissionRequest).is_some_and(|req| {
        !events.iter().any(|e| e.seq > req.seq && e.data_as::<crate::model::PermissionResolvedData>().is_some_and(|a| Some(a.id) == req.data_as::<crate::model::PermissionRequestData>().map(|r| r.id)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locked::LockExt;

    fn on() -> Notify {
        Notify { topic: "hub-test".into(), ..Default::default() }
    }

    #[test]
    fn nothing_goes_out_without_a_topic() {
        assert!(!wanted(&Notify::default(), Kind::Approval, 12));
        assert!(wanted(&on(), Kind::Approval, 12));
    }

    #[test]
    fn quiet_hours_hold_back_what_is_done_but_not_what_waits() {
        let n = Notify { quiet_from: Some(23), quiet_to: Some(7), ..on() };
        assert!(!wanted(&n, Kind::Done, 2));
        assert!(!wanted(&n, Kind::Failed, 23));
        assert!(wanted(&n, Kind::Approval, 2));
        assert!(wanted(&n, Kind::Question, 3));
        assert!(wanted(&n, Kind::Done, 12));
    }

    #[test]
    fn the_hour_is_the_phones() {
        // 2026-11-01 22:30 UTC is 23:30 in Stockholm (UTC+1).
        let at = 1_793_572_200_000;
        assert_eq!(phone_hour(at, 0), 22);
        assert_eq!(phone_hour(at, 60), 23);
        assert_eq!(phone_hour(at, 120), 0);
    }

    #[test]
    fn what_the_phone_shows_is_not_announced() {
        let here = (Looking { active: true, thread: Some("t1".into()) }, 1_000);
        assert!(held_back(Some(&here), "t1", Kind::Done, 500));
        assert!(held_back(Some(&here), "t1", Kind::Approval, 500), "the question shows in the open thread");
        assert!(held_back(Some(&here), "t2", Kind::Done, 500), "the inbox shows what finished");
        assert!(!held_back(Some(&here), "t2", Kind::Question, 500), "another thread waiting still comes through");
        assert!(!held_back(Some(&here), "t1", Kind::Done, 1_001), "the word runs out");
        let away = (Looking { active: false, thread: None }, 1_000);
        assert!(!held_back(Some(&away), "t1", Kind::Done, 500));
        assert!(!held_back(None, "t1", Kind::Done, 500));
    }

    fn here(thread: &str, idle_ms: i64) -> Vec<(Presence, i64)> {
        vec![(Presence { client: "pc".into(), active: true, thread: Some(thread.into()), idle_ms }, 100_000 + LOOK_MS)]
    }

    #[test]
    fn the_thread_open_on_the_pc_is_at_the_pc() {
        let now = 100_000;
        assert!(at_pc(&here("t1", 0), "t1", now));
        assert!(!at_pc(&here("t1", 0), "t2", now), "another thread is not in front of you");
        assert!(!at_pc(&here("t1", AT_PC_MS), "t1", now), "no keys or mouse for three minutes");
        assert!(at_pc(&here("t1", 100_000), "t1", now + 60_000), "idle grows while the word is old, still under the limit");
        assert!(!at_pc(&here("t1", 100_000), "t1", now + 85_000), "idle plus age passes the limit");
        assert!(!at_pc(&here("t1", 0), "t1", now + LOOK_MS + 1), "the word runs out");
        assert!(!at_pc(&[], "t1", now));
    }

    #[test]
    fn a_window_that_leaves_is_forgotten() {
        present(Presence { client: "unit".into(), active: true, thread: Some("tx".into()), idle_ms: 0 });
        let now = crate::model::now_ms();
        assert!(at_pc(&PRESENCE.locked(), "tx", now));
        present(Presence { client: "unit".into(), active: false, thread: None, idle_ms: 0 });
        assert!(!at_pc(&PRESENCE.locked(), "tx", now));
    }

    #[test]
    fn each_kind_can_be_turned_off() {
        let n = Notify { done: false, ..on() };
        assert!(!wanted(&n, Kind::Done, 12));
        assert!(wanted(&n, Kind::Failed, 12));
    }
}
