//! The handshake of an update sent to another machine. This machine sends a version and then
//! follows it on the other machine's own updater (`GET /api/version`), step by step: sent,
//! received, building, ready, restarting, done, or failed with the reason. A step only moves
//! forward, so Settings > Updates never falls back to "sent" while the machine works; a machine
//! that never picks the version up ends as failed with what it said, instead of for ever as sent.
//!
//! `advance` is the whole decision, from what was seen last, so it is checked without a network.

use crate::model::{UpdateState, UpdateStatus, UpdateStep, UpdateTrack};

/// How long a delivered version may go without the other machine's updater showing any sign of it.
/// Its updater looks every three minutes, and an ask from this machine wakes it at once.
pub(crate) const STALL_MS: i64 = 5 * 60_000;
/// How long a machine may be gone while it restarts into the new version.
pub(crate) const RESTART_MS: i64 = 10 * 60_000;

/// What one look at the other machine showed.
pub(crate) struct Seen<'a> {
    pub online: bool,
    /// Its updater's state, when it answered; `None` when it could not be read.
    pub status: Option<&'a UpdateStatus>,
    /// The hubd version it runs.
    pub version: Option<&'a str>,
}

/// A track for a version this machine starts to send.
pub(crate) fn begin(version: &str, detail: impl Into<String>, now: i64) -> UpdateTrack {
    UpdateTrack { version: version.to_owned(), step: UpdateStep::Sending, detail: Some(detail.into()), sent_at: None, received_at: None, changed_at: now }
}

/// The track once the version is in the other machine's folder.
pub(crate) fn delivered(mut track: UpdateTrack, now: i64) -> UpdateTrack {
    track.step = UpdateStep::Sent;
    track.detail = Some("delivered; waiting for its updater to say it has it".into());
    track.sent_at = Some(now);
    track.changed_at = now;
    track
}

/// The track when sending did not work.
pub(crate) fn failed(mut track: UpdateTrack, why: impl Into<String>, now: i64) -> UpdateTrack {
    track.step = UpdateStep::Failed;
    track.detail = Some(why.into());
    track.changed_at = now;
    track
}

/// Whether the track still moves: not done, and not failed for good (a send that failed has
/// nothing to follow; a stalled one may still start).
pub(crate) fn moving(track: &UpdateTrack) -> bool {
    match track.step {
        UpdateStep::Done => false,
        UpdateStep::Failed => track.sent_at.is_some(),
        _ => true,
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_owned()
}

/// Where the update stands after `seen`, from where it stood.
pub(crate) fn advance(track: &UpdateTrack, seen: &Seen, now: i64) -> UpdateTrack {
    if track.step == UpdateStep::Done {
        return track.clone();
    }
    if seen.version == Some(track.version.as_str()) || seen.status.is_some_and(|s| s.version == track.version) {
        return set(track, UpdateStep::Done, Some("now runs it".into()), now);
    }
    // A send that failed here has nothing on the other machine to look at.
    if track.step == UpdateStep::Sending || (track.step == UpdateStep::Failed && track.sent_at.is_none()) {
        return track.clone();
    }
    let since = |at: Option<i64>| at.map_or(0, |at| now - at);
    let seen_step = match (seen.online, seen.status) {
        (false, _) if track.step >= UpdateStep::Ready && track.step != UpdateStep::Failed => {
            // Restarting into the new version takes it off the network for a while.
            if now - track.changed_at > RESTART_MS && track.step == UpdateStep::Restarting {
                return set(track, UpdateStep::Failed, Some(format!("did not come back within {} min after it restarted", RESTART_MS / 60_000)), now);
            }
            return set(track, UpdateStep::Restarting.max(track.step), Some("offline while it restarts into it".into()), now);
        }
        // Offline before it got going: nothing to say about its updater until it is back.
        (false, _) if track.step == UpdateStep::Failed => return track.clone(),
        (false, _) => return set(track, track.step, Some("offline; carries on when it is back".into()), now),
        (true, Some(s)) => from_status(s, &track.version),
        (true, None) => None,
    };
    match seen_step {
        Some((UpdateStep::Failed, detail)) => set(track, UpdateStep::Failed, Some(detail), now),
        Some((step, detail)) => {
            // Never back, except out of a stall that was only the other machine being slow.
            let step = if track.step == UpdateStep::Failed { step } else { step.max(track.step) };
            let mut next = set(track, step, Some(detail), now);
            if next.received_at.is_none() && step >= UpdateStep::Received {
                next.received_at = Some(now);
            }
            next
        }
        None => {
            // Nothing from its updater yet.
            let said = match seen.status {
                None => "it does not answer with its update state".to_owned(),
                Some(s) if s.state == UpdateState::Disabled => "its updater is switched off there (a development build, or off in its hub.json)".to_owned(),
                Some(s) => format!("its updater says {}", state_word(s.state)),
            };
            let disabled = seen.status.is_some_and(|s| s.state == UpdateState::Disabled);
            if track.step < UpdateStep::Received && (disabled || since(track.sent_at) > STALL_MS) {
                let why = if disabled { said } else { format!("not picked up {} min after it was delivered: {said}", since(track.sent_at) / 60_000) };
                return set(track, UpdateStep::Failed, Some(why), now);
            }
            if track.step == UpdateStep::Sent {
                return set(track, UpdateStep::Sent, Some(format!("delivered; its updater has not shown it yet ({said})")), now);
            }
            track.clone()
        }
    }
}

fn state_word(state: UpdateState) -> &'static str {
    match state {
        UpdateState::Disabled => "off",
        UpdateState::Idle => "idle",
        UpdateState::Checking => "checking",
        UpdateState::Building => "building",
        UpdateState::Ready => "ready",
        UpdateState::Restarting => "restarting",
        UpdateState::Failed => "failed",
    }
}

/// The step the other machine's updater shows for `version`, with the line to say, or nothing
/// when it shows nothing of it (idle, switched off, or busy with a failure of an older version).
fn from_status(s: &UpdateStatus, version: &str) -> Option<(UpdateStep, String)> {
    let ours = s.target.as_deref() == Some(version);
    let other = |what: &str| match s.target.as_deref() {
        Some(t) if t != version => format!("{what} {t}, a different version"),
        _ => what.to_owned(),
    };
    match s.state {
        UpdateState::Checking => Some((UpdateStep::Received, other("looking at it"))),
        UpdateState::Building => {
            let step = s.step.as_deref().map(first_line).filter(|l| !l.is_empty());
            Some((UpdateStep::Building, step.unwrap_or_else(|| other("building it"))))
        }
        UpdateState::Ready => Some((UpdateStep::Ready, "built; switches when no thread works there".into())),
        UpdateState::Restarting => Some((UpdateStep::Restarting, "restarting into it".into())),
        UpdateState::Failed if ours => {
            let why = s.message.as_deref().map(first_line).filter(|l| !l.is_empty()).unwrap_or_else(|| "see its log".into());
            Some((UpdateStep::Failed, why))
        }
        UpdateState::Failed | UpdateState::Idle | UpdateState::Disabled => None,
    }
}

fn set(track: &UpdateTrack, step: UpdateStep, detail: Option<String>, now: i64) -> UpdateTrack {
    let mut next = track.clone();
    next.step = step;
    next.detail = detail;
    if next.step != track.step || next.detail != track.detail {
        next.changed_at = now;
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: UpdateState, target: Option<&str>) -> UpdateStatus {
        UpdateStatus { version: "6e26cff".into(), state, target: target.map(Into::into), message: None, building: None, step: None, checked_at: None, waiting_until: None, waiting: None, rolled_back: None, probation: None }
    }

    fn sent(at: i64) -> UpdateTrack {
        delivered(begin("762a45f", "sending", at - 1000), at)
    }

    fn seen<'a>(status: Option<&'a UpdateStatus>) -> Seen<'a> {
        Seen { online: true, status, version: Some("6e26cff") }
    }

    #[test]
    fn an_ask_that_it_answers_is_received_and_then_follows_the_build() {
        let t = sent(0);
        let t = advance(&t, &seen(Some(&status(UpdateState::Checking, Some("762a45f")))), 1000);
        assert_eq!(t.step, UpdateStep::Received);
        assert_eq!(t.received_at, Some(1000));
        let mut building = status(UpdateState::Building, Some("762a45f"));
        building.step = Some("Step 3 of 5: cargo build\nmore".into());
        let t = advance(&t, &seen(Some(&building)), 2000);
        assert_eq!((t.step, t.detail.as_deref()), (UpdateStep::Building, Some("Step 3 of 5: cargo build")));
        let t = advance(&t, &seen(Some(&status(UpdateState::Ready, Some("762a45f")))), 3000);
        assert_eq!(t.step, UpdateStep::Ready);
        let t = advance(&t, &seen(Some(&status(UpdateState::Restarting, Some("762a45f")))), 4000);
        assert_eq!(t.step, UpdateStep::Restarting);
        let t = advance(&t, &Seen { online: false, status: None, version: None }, 5000);
        assert_eq!((t.step, t.detail.as_deref()), (UpdateStep::Restarting, Some("offline while it restarts into it")));
        let t = advance(&t, &Seen { online: true, status: None, version: Some("762a45f") }, 6000);
        assert_eq!(t.step, UpdateStep::Done);
        // Done stays done.
        assert_eq!(advance(&t, &seen(Some(&status(UpdateState::Idle, None))), 7000).step, UpdateStep::Done);
    }

    #[test]
    fn a_step_never_goes_back_when_the_updater_flickers() {
        let t = advance(&sent(0), &seen(Some(&status(UpdateState::Building, Some("762a45f")))), 1000);
        assert_eq!(t.step, UpdateStep::Building);
        // The updater goes idle between two rounds, or a read fails: still building as far as we know.
        assert_eq!(advance(&t, &seen(Some(&status(UpdateState::Idle, None))), 2000).step, UpdateStep::Building);
        assert_eq!(advance(&t, &seen(None), 3000).step, UpdateStep::Building);
        assert_eq!(advance(&t, &seen(Some(&status(UpdateState::Checking, Some("762a45f")))), 4000).step, UpdateStep::Building);
    }

    #[test]
    fn a_machine_that_shows_nothing_stays_sent_and_then_fails_with_what_it_said() {
        let idle = status(UpdateState::Idle, None);
        let t = advance(&sent(0), &seen(Some(&idle)), 60_000);
        assert_eq!(t.step, UpdateStep::Sent);
        assert!(t.detail.unwrap().contains("idle"));
        let t = advance(&sent(0), &seen(Some(&idle)), STALL_MS + 1);
        assert_eq!(t.step, UpdateStep::Failed);
        assert!(t.detail.as_deref().unwrap().contains("not picked up"));
        // A read that fails says that instead.
        let t = advance(&sent(0), &seen(None), STALL_MS + 1);
        assert!(t.detail.as_deref().unwrap().contains("does not answer"));
        // Slow, not dead: when it starts after all, the failure goes away.
        let t = advance(&t, &seen(Some(&status(UpdateState::Building, Some("762a45f")))), STALL_MS + 2);
        assert_eq!(t.step, UpdateStep::Building);
    }

    #[test]
    fn a_switched_off_updater_fails_at_once_with_that_reason() {
        let t = advance(&sent(0), &seen(Some(&status(UpdateState::Disabled, None))), 1000);
        assert_eq!(t.step, UpdateStep::Failed);
        assert!(t.detail.unwrap().contains("switched off"));
    }

    #[test]
    fn its_own_failure_is_shown_with_its_reason_but_an_older_failure_is_not() {
        let mut failed_now = status(UpdateState::Failed, Some("762a45f"));
        failed_now.message = Some("cargo build failed: out of memory\nmore".into());
        let t = advance(&sent(0), &seen(Some(&failed_now)), 1000);
        assert_eq!((t.step, t.detail.as_deref()), (UpdateStep::Failed, Some("cargo build failed: out of memory")));
        let older = status(UpdateState::Failed, Some("1111111"));
        assert_eq!(advance(&sent(0), &seen(Some(&older)), 1000).step, UpdateStep::Sent);
    }

    #[test]
    fn a_machine_that_does_not_come_back_from_the_restart_fails() {
        let mut t = sent(0);
        t.step = UpdateStep::Restarting;
        t.changed_at = 0;
        let t = advance(&t, &Seen { online: false, status: None, version: None }, RESTART_MS + 1);
        assert_eq!(t.step, UpdateStep::Failed);
    }

    #[test]
    fn a_send_that_failed_stays_failed_and_one_that_is_going_waits() {
        let f = failed(begin("762a45f", "sending", 0), "ssh refused", 10);
        assert!(!moving(&f));
        assert_eq!(advance(&f, &seen(Some(&status(UpdateState::Building, Some("762a45f")))), 20).step, UpdateStep::Failed);
        let going = begin("762a45f", "sending", 0);
        assert_eq!(advance(&going, &seen(None), 20).step, UpdateStep::Sending);
        assert!(moving(&going) && moving(&sent(0)));
    }

    fn hub() -> std::sync::Arc<crate::Hub> {
        let config = crate::Config { data_dir: tempfile::tempdir().unwrap().keep(), ..Default::default() };
        crate::Hub::new(crate::Store::in_memory().unwrap(), config)
    }

    #[tokio::test]
    async fn an_idle_updater_asked_to_look_names_the_version_as_its_receipt() {
        let hub = hub();
        // Switched off, it says so and pretends nothing.
        assert_eq!(hub.update_look(Some("762a45f")).state, UpdateState::Disabled);
        hub.set_update(|s| s.state = UpdateState::Idle);
        let receipt = hub.update_look(Some("762a45f"));
        assert_eq!((receipt.state, receipt.target.as_deref()), (UpdateState::Checking, Some("762a45f")));
        // The sender reads that as received.
        let track = advance(&sent(0), &seen(Some(&receipt)), 10);
        assert_eq!(track.step, UpdateStep::Received);
        // Busy building, it is not disturbed.
        hub.set_update(|s| {
            s.state = UpdateState::Building;
            s.target = Some("1111111".into());
        });
        let receipt = hub.update_look(Some("762a45f"));
        assert_eq!((receipt.state, receipt.target.as_deref()), (UpdateState::Building, Some("1111111")));
    }
}
