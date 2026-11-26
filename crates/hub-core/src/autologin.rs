//! Signing engines in on the other machines without being asked: a machine that comes online
//! with an engine not signed in gets that engine's link-and-code sign-in started by the PC
//! (`login.rs` there), and the link and code reach you where you already are: the list below
//! for Home and the machine's page, and a notice on the phone through ntfy.
//!
//! This is the answer for engines that have no lasting token to hand out (Codex): you type a
//! code once per machine, and the engine keeps its own login alive while it is used. Engines
//! with a lasting token (Claude Code) get it with the engines (`signin.rs`) and are not handled
//! here. Nothing names an engine: a check is wanted when its fix is `SignInHere`.

use crate::locked::LockExt;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use crate::hub::Hub;
use crate::login::LoginJob;
use crate::model::{HostState, MachineReady, ReadyFix, ReadyState, now_ms};

/// How often the machines are looked at.
const EVERY: Duration = Duration::from_secs(60);
/// A sign-in nobody finished is offered again after this, with a new notice, never sooner.
const AGAIN_MS: i64 = 6 * 3_600_000;
/// How long to wait for the engine to print its link and code.
const WAIT_FOR_CODE: Duration = Duration::from_secs(90);

/// A sign-in waiting for you on a machine: open the link and type the code.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PendingSignIn {
    pub machine: String,
    pub engine: String,
    /// The engine as people know it ("Codex").
    pub label: String,
    pub url: Option<String>,
    pub code: Option<String>,
    /// The engine is still waiting for the code. False once it gave up; a new code is offered
    /// by the machine's Try again, or by itself after a few hours.
    pub running: bool,
    pub message: String,
    #[ts(type = "number")]
    pub at: i64,
}

/// What was started or seen, by `(machine, engine)`.
#[derive(Default)]
struct Seen {
    pending: BTreeMap<(String, String), PendingSignIn>,
    /// When a sign-in was last started there, and the notice sent for it.
    started: BTreeMap<(String, String), i64>,
}

static SEEN: LazyLock<Mutex<Seen>> = LazyLock::new(Default::default);

/// Sign-ins waiting for you now, oldest first.
pub fn pending() -> Vec<PendingSignIn> {
    let mut list: Vec<PendingSignIn> = SEEN.locked().pending.values().cloned().collect();
    list.sort_by_key(|p| p.at);
    list
}

/// Engines a ready check says need a link-and-code sign-in, as `(engine, label)`.
pub fn wanted(ready: &MachineReady) -> Vec<(String, String)> {
    ready
        .checks
        .iter()
        .filter(|c| c.state == ReadyState::Missing)
        .filter_map(|c| match &c.fix {
            Some(ReadyFix::SignInHere { engine }) => Some((engine.clone(), c.label.clone())),
            _ => None,
        })
        .collect()
}

/// Whether a sign-in may be started again: never while one runs, and not within `AGAIN_MS` of the
/// last start.
fn may_start(last_started: Option<i64>, running: bool, now: i64) -> bool {
    !running && last_started.is_none_or(|t| now - t >= AGAIN_MS)
}

/// Keeps looking at the machines, a minute apart.
pub async fn watch(hub: Arc<Hub>) {
    // Let the machines connect first.
    tokio::time::sleep(Duration::from_secs(30)).await;
    loop {
        look(&hub).await;
        tokio::time::sleep(EVERY).await;
    }
}

/// One look at every machine online.
pub async fn look(hub: &Arc<Hub>) {
    let hosts: Vec<String> = hub.remotes.list().into_iter().filter(|h| !h.local && h.state == HostState::Online).map(|h| h.name).collect();
    // What is waiting on a machine that went away is not shown as waiting.
    SEEN.locked().pending.retain(|(machine, _), _| hosts.contains(machine));
    for host in hosts {
        // A hubd too old for the check, or one that does not answer now: next time.
        let Ok(ready) = hub.remotes.get_from::<MachineReady>(&host, "/api/machine/ready").await else { continue };
        let wanted = wanted(&ready);
        // Signed in since: no longer pending.
        SEEN.locked().pending.retain(|(machine, engine), _| machine != &host || wanted.iter().any(|(e, _)| e == engine));
        for (engine, label) in wanted {
            if let Err(e) = one(hub, &host, &engine, &label).await {
                tracing::debug!(%host, %engine, "signing in there: {e:#}");
            }
        }
    }
}

async fn one(hub: &Arc<Hub>, host: &str, engine: &str, label: &str) -> anyhow::Result<()> {
    let key = (host.to_owned(), engine.to_owned());
    let path = format!("/api/engines/{engine}/login");
    let now = now_ms();
    let job: Option<LoginJob> = hub.remotes.get_from(host, &path).await.unwrap_or(None);
    let running = job.as_ref().is_some_and(|j| j.running);
    let last = SEEN.locked().started.get(&key).copied();
    let started_now = !running && may_start(last, running, now);
    let job = if started_now {
        SEEN.locked().started.insert(key.clone(), now);
        let started: LoginJob = hub.remotes.post_to(host, &path, &json!({}), Duration::from_secs(30)).await?;
        Some(wait_for_code(hub, host, &path, started).await)
    } else {
        job
    };
    let Some(job) = job else { return Ok(()) };
    let entry = {
        let mut seen = SEEN.locked();
        // Waiting since the first look at this code; a new code starts the wait again.
        let at = seen.pending.get(&key).map(|p| p.at).filter(|_| !started_now).unwrap_or(now);
        let entry = PendingSignIn {
            machine: host.to_owned(),
            engine: engine.to_owned(),
            label: label.to_owned(),
            url: job.url.clone(),
            code: job.code.clone(),
            running: job.running,
            message: job.message.clone(),
            at,
        };
        seen.pending.insert(key, entry.clone());
        entry
    };
    if started_now && entry.running && entry.url.is_some() && entry.code.is_some() {
        tell(hub, &entry).await;
    }
    Ok(())
}

/// Waits until the engine has printed its link and code, or gave up.
async fn wait_for_code(hub: &Arc<Hub>, host: &str, path: &str, mut job: LoginJob) -> LoginJob {
    let start = tokio::time::Instant::now();
    while job.running && !(job.url.is_some() && job.code.is_some()) && start.elapsed() < WAIT_FOR_CODE {
        tokio::time::sleep(Duration::from_secs(3)).await;
        if let Ok(Some(j)) = hub.remotes.get_from::<Option<LoginJob>>(host, path).await {
            job = j;
        }
    }
    job
}

/// The phone gets a notice with the code; a tap opens the engine's page. Sent like a question
/// that waits for you, so quiet hours do not hold it back.
async fn tell(hub: &Hub, p: &PendingSignIn) {
    let n = crate::ntfy::load(&hub.ctx()).await;
    if n.topic.trim().is_empty() || !n.approvals {
        return;
    }
    let title = format!("Sign in {} on {}", p.label, p.machine);
    let body = format!("Open the link and type {}. It waits a few minutes.", p.code.as_deref().unwrap_or_default());
    if let Err(e) = crate::ntfy::send(&n, &title, &body, p.url.as_deref(), true).await {
        tracing::debug!("notice about signing in {} on {}: {e:#}", p.label, p.machine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ReadyCheck;

    fn check(key: &str, state: ReadyState, fix: Option<ReadyFix>) -> ReadyCheck {
        ReadyCheck { key: key.into(), label: key.into(), state, detail: String::new(), fix }
    }

    #[test]
    fn only_engines_missing_a_link_and_code_sign_in_are_asked_for() {
        let ready = MachineReady {
            checks: vec![
                check("engine:codex", ReadyState::Missing, Some(ReadyFix::SignInHere { engine: "codex".into() })),
                // Signed in, with a way to sign in again: nothing to do.
                check("engine:other", ReadyState::Ok, Some(ReadyFix::SignInHere { engine: "other".into() })),
                // A lasting token comes with the engines, not from here.
                check("engine:claude", ReadyState::Missing, Some(ReadyFix::KeepSignedIn { engine: "claude".into() })),
                check("git", ReadyState::Missing, Some(ReadyFix::GitIdentity)),
            ],
            git: None,
        };
        assert_eq!(wanted(&ready), vec![("codex".to_owned(), "engine:codex".to_owned())]);
    }

    #[test]
    fn a_sign_in_is_offered_again_only_after_a_few_hours() {
        let now = 100 * 3_600_000;
        assert!(may_start(None, false, now));
        assert!(!may_start(Some(now - 3_600_000), false, now));
        assert!(may_start(Some(now - AGAIN_MS), false, now));
        assert!(!may_start(None, true, now));
    }
}
