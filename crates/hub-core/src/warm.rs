//! Engines started for a thread that does not exist yet, while its first message is typed.
//!
//! Starting an engine is most of the wait before a new thread answers: the harness has to start
//! (Hermes also brings up its own gateway, about ten seconds on Windows), and the ACP handshake
//! and `session/new` follow. The new-thread composer asks for a warm engine on the first key it
//! sees (`POST /api/threads/warm`) and gets the id the thread will have. The engine runs as that
//! thread's runtime up to the point where the session is open, then waits. When the thread is
//! created with the id (`NewThread.warm`) its first message goes to the engine that is already
//! up. Nothing is sent to the agent before then, so a warm engine spends nothing; one nobody
//! claims stops after [`WARM_FOR`].
//!
//! The engine started with what the composer showed: the engine, the folder and the provider
//! pick. When the thread comes out different (another folder, another pick), the waiting engine
//! steps aside and the message goes to a fresh one, as without warming.
//!
//! Any engine that speaks ACP works this way; nothing here knows one from another.
//! <!-- see docs/ARCHITECTURE.md "Fast first answers" -->

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

/// How long a warm engine waits for its thread.
pub const WARM_FOR: Duration = Duration::from_secs(180);
/// At most this many engines wait at once; the oldest goes first.
pub const AT_MOST: usize = 2;

/// An engine waiting for the thread `id`.
#[derive(Debug, Clone)]
pub struct Warm {
    pub id: String,
    pub engine: String,
    pub cwd: String,
    /// The settings it started with; the provider pick (`routes.rs`) among them shapes the start.
    pub settings: Vec<(String, Value)>,
    pub at: Instant,
    /// A thread was created with the id.
    pub claimed: bool,
}

impl Warm {
    pub fn route(&self) -> Option<String> {
        crate::routes::picked(&self.settings)
    }

    /// The thread this engine runs as until the real one is created: the same id, an empty title
    /// and the folder and engine the composer showed. The engine runs on it from the start, and
    /// Hub's tools (`mcp.rs`) answer to it while the message is still being typed.
    pub fn stand_in(&self) -> crate::model::Thread {
        let now = crate::model::now_ms();
        crate::model::Thread {
            id: self.id.clone(),
            title: String::new(),
            cwd: self.cwd.clone(),
            engine: self.engine.clone(),
            status: crate::model::ThreadStatus::Idle,
            engine_session_id: None,
            title_locked: false,
            archived: false,
            returned_at: None,
            seen_at: None,
            settled_at: None,
            sent_at: None,
            pinned_at: None,
            snoozed_until: None,
            auto_settle: true,
            parent: None,
            reading: None,
            owner: None,
            cloud: None,
            host: None, project: None, display: None,
            created_at: now,
            updated_at: now,
        }
    }
}

static WARMS: LazyLock<Mutex<Vec<Warm>>> = LazyLock::new(Default::default);

fn warms() -> std::sync::MutexGuard<'static, Vec<Warm>> {
    WARMS.lock().unwrap_or_else(|e| e.into_inner())
}

/// The warm engine for thread `id`, while it waits.
pub fn pending(id: &str) -> Option<Warm> {
    warms().iter().find(|w| w.id == id).cloned()
}

/// One that would do for a thread on `engine` in `cwd` with the provider pick `route`, with
/// time left to wait.
pub fn reusable(engine: &str, cwd: &str, route: &Option<String>) -> Option<String> {
    warms()
        .iter()
        .find(|w| !w.claimed && w.engine == engine && w.cwd == cwd && &w.route() == route && w.at.elapsed() + Duration::from_secs(30) < WARM_FOR)
        .map(|w| w.id.clone())
}

/// Notes a new warm engine; returns the ids of those it pushed out, whose engines should stop.
pub fn add(warm: Warm) -> Vec<String> {
    let mut all = warms();
    all.retain(|w| w.claimed || w.at.elapsed() < WARM_FOR);
    all.push(warm);
    let mut out = Vec::new();
    while all.iter().filter(|w| !w.claimed).count() > AT_MOST {
        let Some(at) = all.iter().position(|w| !w.claimed) else { break };
        out.push(all.remove(at).id);
    }
    out
}

/// A thread is being created with `id` on `engine`: true when a warm engine waits for it and
/// nothing claimed it before. It is the thread's engine from now on.
pub fn claim(id: &str, engine: &str) -> bool {
    let mut all = warms();
    match all.iter_mut().find(|w| w.id == id && w.engine == engine && !w.claimed) {
        Some(w) => {
            w.claimed = true;
            true
        }
        None => false,
    }
}

/// The engine for `id` took its thread, or gave up waiting.
pub fn forget(id: &str) {
    warms().retain(|w| w.id != id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn warm(id: &str, route: Option<&str>) -> Warm {
        let settings = route.map(|r| vec![(crate::routes::ROUTE.to_owned(), json!(r))]).unwrap_or_default();
        Warm { id: id.into(), engine: "hermes".into(), cwd: "/tmp/w".into(), settings, at: Instant::now(), claimed: false }
    }

    #[test]
    fn a_warm_engine_is_claimed_once_and_only_for_its_engine() {
        add(warm("warm-a", None));
        assert!(!claim("warm-a", "claude"));
        assert!(claim("warm-a", "hermes"));
        assert!(!claim("warm-a", "hermes"));
        assert!(pending("warm-a").is_some_and(|w| w.claimed));
        forget("warm-a");
        assert!(pending("warm-a").is_none());
    }

    #[test]
    fn reuse_needs_the_same_folder_and_provider_pick() {
        add(warm("warm-b", Some("deepseek:deepseek-flash")));
        assert_eq!(reusable("hermes", "/tmp/w", &Some("deepseek:deepseek-flash".into())), Some("warm-b".into()));
        assert_eq!(reusable("hermes", "/tmp/w", &Some("openrouter:x".into())), None);
        assert_eq!(reusable("hermes", "/tmp/other", &Some("deepseek:deepseek-flash".into())), None);
        forget("warm-b");
    }

    #[test]
    fn the_oldest_unclaimed_engine_makes_room() {
        let ids = ["warm-c1", "warm-c2", "warm-c3"];
        let mut out = Vec::new();
        for id in ids {
            let mut w = warm(id, None);
            w.engine = "only-in-this-test".into();
            out.extend(add(w));
        }
        // Other tests may hold warm engines too; the first of these three always goes.
        assert!(pending("warm-c1").is_none());
        assert!(!out.contains(&"warm-c3".to_owned()));
        for id in ids {
            forget(id);
        }
    }
}
