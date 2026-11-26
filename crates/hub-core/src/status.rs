//! What a thread's status is, worked out from its event log (D3, step 1 of
//! the D3 plan (project file `teknisk-skuld/d3-status-fran-handelser-plan.md`)).
//!
//! `next_status` is a pure state machine: the status before an event and the event give the status
//! after it. Nothing calls it yet; the next steps run it beside `Hub::set_status` (shadow mode),
//! then make the log the truth and `threads.status` a cache written in the same transaction.
//!
//! It never reads the log. What it needs besides the event is how many permission questions are
//! still open (`open_questions`), because the thread goes back to work only when the last one is
//! answered, and whether one of them is asleep (`asleep`, see `Slept`). [`replay`] folds a whole
//! log and keeps both itself.
//!
//! No engine event says `InTerminal`, a cloud session, a queued message taken back or an engine
//! stopped by park/`stop_engines`. They are logged as a `Status` marker event
//! (`Hub::set_status_because`, D3 step 4) and the marker names the status itself. A log from before
//! the markers has none, so there a status from one of those is never changed by this machine
//! (`InTerminal` stays put until a marker moves it).

use std::collections::HashSet;

use crate::model::{ENGINE_GONE, Event, EventKind, PermissionResolvedData, SleptData, StatusData, ThreadStatus};

/// The status a thread has after `event`, or `None` when the event leaves it as it was.
/// `open_questions` is how many permission questions are still waiting once this event is counted
/// (so for `permission_resolved` it is the number left after the answer). `asleep` is true while
/// a question is unanswered because its session was put to sleep: the engine's turn then ends
/// (cancelled) with `turn_end`, but the user is still being waited for.
pub fn next_status(prev: ThreadStatus, event: &Event, open_questions: usize, asleep: bool) -> Option<ThreadStatus> {
    use ThreadStatus::*;
    // A marker says the new status itself (terminal in and out, cloud, queue, parked engine,
    // start-up repair), so it applies in every state, also out of a terminal.
    if event.kind == EventKind::Status {
        let to = event.data_as::<StatusData>()?.to()?;
        return (to != prev).then_some(to);
    }
    // A terminal has the session; Hub's engine is stopped, so none of its events apply.
    if prev == InTerminal {
        return None;
    }
    let to = match event.kind {
        // A message to an idle thread starts work. To a working one it only queues.
        EventKind::UserMessage => matches!(prev, Idle | Error).then_some(Running)?,
        EventKind::TurnStart => Running,
        EventKind::PermissionRequest | EventKind::Slept => WaitingOnYou,
        // Back to work only from waiting, and only when no other question is open: an answer to a
        // stale card (the turn is over) must not make an idle thread run.
        // An answer that wakes a sleeping session always puts the thread to work (`wake_from_sleep`),
        // however many other slept questions are still open.
        EventKind::PermissionResolved => {
            let answer = event.data_as::<PermissionResolvedData>().unwrap_or_default();
            if answer.woke == Some(true) {
                Running
            } else if answer.closed_elsewhere() {
                // Closed by a restart or a move; the status is set there, not by the answer.
                return None;
            } else {
                (prev == WaitingOnYou && open_questions == 0).then_some(Running)?
            }
        }
        EventKind::PermissionAuto => (prev == WaitingOnYou && open_questions == 0).then_some(Running)?,
        // A turn handed over to a new message ends without the thread going idle.
        EventKind::TurnEnd if event.handed_over() => return None,
        EventKind::TurnEnd | EventKind::Error if prev == WaitingOnYou && asleep => return None,
        EventKind::TurnEnd => Idle,
        EventKind::Error => error_leaves(event),
        _ => return None,
    };
    (to != prev).then_some(to)
}

/// Where an `error` event leaves the thread. A newer log says so in `leaves`; older ones are read
/// as the code before did: a turn that failed (`prompt`) is idle again, an engine that failed
/// outside a turn, or went away in the middle of one, needs you.
fn error_leaves(error: &Event) -> ThreadStatus {
    match error.leaves() {
        Some("idle") => ThreadStatus::Idle,
        Some("error") => ThreadStatus::Error,
        _ if error.failure() == Some(ENGINE_GONE) => ThreadStatus::Error,
        _ if error.prompt().is_some() => ThreadStatus::Idle,
        _ => ThreadStatus::Error,
    }
}

/// The status of one thread as events arrive one by one: [`replay`] for a log that grows. It
/// counts open and slept questions as the core does: a request opens one, its answer closes it,
/// and the end of a turn closes the rest (the core cancels what the turn left open), except those
/// whose session slept: they stay open until the user answers.
#[derive(Debug, Clone)]
pub struct Tracker {
    pub status: ThreadStatus,
    open: HashSet<String>,
    slept: HashSet<String>,
}

impl Tracker {
    pub fn new(start: ThreadStatus) -> Self {
        Self { status: start, open: HashSet::new(), slept: HashSet::new() }
    }

    /// Takes the next event of the log; true when the status changed.
    pub fn feed(&mut self, event: &Event) -> bool {
        let before = self.status;
        match event.kind {
            EventKind::PermissionRequest => {
                if let Some(id) = event.request_id().as_str() {
                    self.open.insert(id.to_owned());
                }
            }
            EventKind::PermissionResolved => {
                if let Some(answer) = event.data_as::<PermissionResolvedData>() {
                    self.open.remove(&answer.id);
                    self.slept.remove(&answer.id);
                }
            }
            // The engine was stopped: its questions live in the log only, and the answer wakes it.
            EventKind::Slept => {
                self.slept.extend(event.data_as::<SleptData>().unwrap_or_default().ids);
            }
            _ => {}
        }
        if let Some(next) = next_status(self.status, event, self.open.len(), !self.slept.is_empty()) {
            self.status = next;
        }
        if matches!(event.kind, EventKind::TurnEnd | EventKind::Error | EventKind::Status) {
            let slept = &self.slept;
            self.open.retain(|id| slept.contains(id));
            if self.status != ThreadStatus::WaitingOnYou {
                self.slept.clear();
                self.open.clear();
            }
        }
        self.status != before
    }
}

/// The status after playing `events` (oldest first) from `start`.
pub fn replay(start: ThreadStatus, events: &[Event]) -> ThreadStatus {
    let mut tracker = Tracker::new(start);
    for event in events {
        tracker.feed(event);
    }
    tracker.status
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use ThreadStatus::*;

    fn ev(seq: i64, kind: EventKind, data: Value) -> Event {
        Event { thread_id: "t".into(), seq, ts: seq, kind, data }
    }

    fn log(items: Vec<(EventKind, Value)>) -> Vec<Event> {
        items.into_iter().enumerate().map(|(i, (k, d))| ev(i as i64 + 1, k, d)).collect()
    }

    fn step(prev: ThreadStatus, kind: EventKind, data: Value, open: usize) -> Option<ThreadStatus> {
        next_status(prev, &ev(1, kind, data), open, false)
    }

    #[test]
    fn a_marker_names_the_status_in_every_state() {
        let marker = |to: &str| (EventKind::Status, json!({ "status": to, "why": "test" }));
        let events = log(vec![marker("in_terminal"), (EventKind::TurnStart, json!({})), marker("idle"), marker("bogus")]);
        assert_eq!(replay(Idle, &events[..1]), InTerminal);
        assert_eq!(replay(Idle, &events[..2]), InTerminal, "a terminal has the session");
        assert_eq!(replay(Idle, &events[..3]), Idle, "a marker takes it back");
        assert_eq!(replay(Running, &events[3..]), Running, "an unknown word changes nothing");
        // A marker that closes work closes its questions too.
        let asked = log(vec![(EventKind::PermissionRequest, json!({ "id": "a" })), marker("idle"), (EventKind::PermissionResolved, json!({ "id": "a" }))]);
        assert_eq!(replay(Running, &asked), Idle);
    }

    #[test]
    fn a_plain_turn_runs_and_comes_back_idle() {
        let events = log(vec![
            (EventKind::UserMessage, json!({ "text": "hi" })),
            (EventKind::TurnStart, json!({ "prompt": 1 })),
            (EventKind::Update, json!({})),
            (EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 1 })),
        ]);
        assert_eq!(replay(Idle, &events[..1]), Running);
        assert_eq!(replay(Idle, &events[..3]), Running);
        assert_eq!(replay(Idle, &events), Idle);
    }

    #[test]
    fn a_message_only_starts_work_on_a_thread_that_is_not_working() {
        assert_eq!(step(Idle, EventKind::UserMessage, json!({}), 0), Some(Running));
        assert_eq!(step(Error, EventKind::UserMessage, json!({}), 0), Some(Running));
        // Queued behind the running turn, or typed while a question waits: no change.
        assert_eq!(step(Running, EventKind::UserMessage, json!({}), 0), None);
        assert_eq!(step(WaitingOnYou, EventKind::UserMessage, json!({}), 1), None);
        assert_eq!(step(InTerminal, EventKind::UserMessage, json!({}), 0), None);
    }

    #[test]
    fn a_question_waits_for_you_and_work_resumes_with_the_last_answer() {
        let events = log(vec![
            (EventKind::UserMessage, json!({})),
            (EventKind::TurnStart, json!({ "prompt": 1 })),
            (EventKind::PermissionRequest, json!({ "id": "a" })),
            (EventKind::PermissionRequest, json!({ "id": "b" })),
            (EventKind::PermissionResolved, json!({ "id": "a", "option_id": "allow" })),
            (EventKind::PermissionResolved, json!({ "id": "b", "option_id": "allow" })),
            (EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 1 })),
        ]);
        assert_eq!(replay(Idle, &events[..3]), WaitingOnYou);
        assert_eq!(replay(Idle, &events[..4]), WaitingOnYou);
        assert_eq!(replay(Idle, &events[..5]), WaitingOnYou, "one question is still open");
        assert_eq!(replay(Idle, &events[..6]), Running);
        assert_eq!(replay(Idle, &events), Idle);
    }

    #[test]
    fn a_rule_that_answers_for_you_does_not_stop_the_thread() {
        // Answered at once, so the thread never waited.
        assert_eq!(step(Running, EventKind::PermissionAuto, json!({}), 0), None);
        // While a real question is open it keeps waiting; once none is, it works again.
        assert_eq!(step(WaitingOnYou, EventKind::PermissionAuto, json!({}), 1), None);
        assert_eq!(step(WaitingOnYou, EventKind::PermissionAuto, json!({}), 0), Some(Running));
    }

    #[test]
    fn a_question_left_open_ends_with_the_turn_and_a_stale_answer_changes_nothing() {
        let events = log(vec![
            (EventKind::UserMessage, json!({})),
            (EventKind::TurnStart, json!({ "prompt": 1 })),
            (EventKind::PermissionRequest, json!({ "id": "a" })),
            (EventKind::PermissionResolved, json!({ "id": "a", "option_id": null })),
            (EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 1 })),
            // The card is answered after the turn: Idle stays Idle.
            (EventKind::PermissionResolved, json!({ "id": "a", "option_id": "allow" })),
        ]);
        assert_eq!(replay(Idle, &events), Idle);
    }

    #[test]
    fn a_sleeping_session_keeps_waiting_until_the_answer_wakes_it() {
        let events = log(vec![
            (EventKind::UserMessage, json!({})),
            (EventKind::TurnStart, json!({ "prompt": 1 })),
            (EventKind::PermissionRequest, json!({ "id": "a" })),
            (EventKind::Slept, json!({ "ids": ["a"] })),
        ]);
        assert_eq!(replay(Idle, &events), WaitingOnYou);
        let mut woken = events.clone();
        woken.push(ev(5, EventKind::PermissionResolved, json!({ "id": "a", "option_id": "allow", "woke": true })));
        assert_eq!(replay(Idle, &woken), Running);
        woken.push(ev(6, EventKind::UserMessage, json!({ "text": "you were put to sleep" })));
        woken.push(ev(7, EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": 1 })));
        assert_eq!(replay(Idle, &woken), Idle);
    }

    #[test]
    fn the_turn_end_a_sleep_causes_leaves_the_thread_waiting() {
        let events = log(vec![
            (EventKind::UserMessage, json!({})),
            (EventKind::TurnStart, json!({ "prompt": 1 })),
            (EventKind::PermissionRequest, json!({ "id": "a" })),
            (EventKind::Slept, json!({ "ids": ["a"] })),
            (EventKind::TurnEnd, json!({ "stop_reason": "cancelled", "prompt": 1 })),
        ]);
        assert_eq!(replay(Idle, &events), WaitingOnYou);
        // Without the sleep the same end is idle.
        assert_eq!(next_status(WaitingOnYou, &events[4], 1, false), Some(Idle));
    }

    #[test]
    fn a_wake_handover_and_restart_resolve_follow_what_the_core_does() {
        // Two slept questions: the first answer wakes the session.
        assert_eq!(step(WaitingOnYou, EventKind::PermissionResolved, json!({ "woke": true }), 1), Some(Running));
        // An unprompted turn handed over to a message does not make the thread idle.
        assert_eq!(step(Running, EventKind::TurnEnd, json!({ "handed_over": true, "unprompted": true }), 0), None);
        // Questions closed by a restart or a move leave the status to the code that did it.
        assert_eq!(step(WaitingOnYou, EventKind::PermissionResolved, json!({ "restart": true }), 0), None);
        assert_eq!(step(WaitingOnYou, EventKind::PermissionResolved, json!({ "moved": true }), 0), None);
    }

    #[test]
    fn a_stopped_turn_is_idle_whether_it_ended_by_cancel_or_by_force() {
        let events = log(vec![
            (EventKind::UserMessage, json!({})),
            (EventKind::TurnStart, json!({ "prompt": 1 })),
            (EventKind::TurnEnd, json!({ "stop_reason": "cancelled", "forced": true, "prompt": 1 })),
        ]);
        assert_eq!(replay(Idle, &events), Idle);
    }

    #[test]
    fn an_error_leaves_the_thread_as_the_code_did_before_the_field_existed() {
        // A turn that failed: idle, the next message just goes.
        assert_eq!(step(Running, EventKind::Error, json!({ "message": "boom", "prompt": 3 }), 0), Some(Idle));
        // An engine that failed outside a turn needs you.
        assert_eq!(step(Running, EventKind::Error, json!({ "message": "spawn failed" }), 0), Some(Error));
        // The engine went away in the middle of a turn: it has a prompt, and still needs you.
        assert_eq!(step(Running, EventKind::Error, json!({ "message": ENGINE_GONE, "prompt": 3 }), 0), Some(Error));
        // A log that says where it leaves the thread is believed over the guess.
        assert_eq!(step(Running, EventKind::Error, json!({ "message": "x", "prompt": 3, "leaves": "error" }), 0), Some(Error));
        assert_eq!(step(Running, EventKind::Error, json!({ "message": "x", "leaves": "idle" }), 0), Some(Idle));
        // An error from a thread already in Error changes nothing.
        assert_eq!(step(Error, EventKind::Error, json!({ "message": "x" }), 0), None);
    }

    #[test]
    fn a_failed_turn_then_a_new_message_runs_again() {
        let events = log(vec![
            (EventKind::UserMessage, json!({})),
            (EventKind::TurnStart, json!({ "prompt": 1 })),
            (EventKind::Error, json!({ "message": ENGINE_GONE, "prompt": 1 })),
            (EventKind::UserMessage, json!({})),
        ]);
        assert_eq!(replay(Idle, &events[..3]), Error);
        assert_eq!(replay(Idle, &events), Running);
    }

    #[test]
    fn a_core_that_died_mid_turn_leaves_a_log_that_reads_as_running() {
        // turn_start without turn_end: the log says Running. (Today the start-up repair turns the
        // field to Idle; that is what `repair_status` will decide with `store.last_turn`.)
        let events = log(vec![(EventKind::UserMessage, json!({})), (EventKind::TurnStart, json!({ "prompt": 1 }))]);
        assert_eq!(replay(Idle, &events), Running);
    }

    #[test]
    fn a_thread_in_a_terminal_keeps_it_whatever_the_log_says() {
        for kind in [EventKind::TurnStart, EventKind::TurnEnd, EventKind::PermissionRequest, EventKind::Error, EventKind::Slept] {
            assert_eq!(step(InTerminal, kind, json!({}), 0), None, "{kind:?}");
        }
    }

    #[test]
    fn events_that_are_not_about_work_change_nothing() {
        for kind in [EventKind::Update, EventKind::Setting, EventKind::Checkpoint, EventKind::SessionStarted, EventKind::Queue] {
            for prev in [Idle, Running, WaitingOnYou, Error] {
                assert_eq!(step(prev, kind, json!({}), 0), None, "{prev:?} {kind:?}");
            }
        }
    }

    #[test]
    fn an_event_that_gives_the_status_it_already_has_is_no_change() {
        assert_eq!(step(Running, EventKind::TurnStart, json!({}), 0), None);
        assert_eq!(step(Idle, EventKind::TurnEnd, json!({}), 0), None);
        assert_eq!(step(WaitingOnYou, EventKind::PermissionRequest, json!({}), 2), None);
    }
}
