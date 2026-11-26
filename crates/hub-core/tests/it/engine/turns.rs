//! Turns: replies, permissions, queue, stop, restart, titles, archive and delete.

use super::*;

#[tokio::test]
async fn a_message_streams_a_reply_into_the_log() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("hello world")).await.unwrap();
    assert_eq!(thread.title, "hello world");

    let end = next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(end.data["stop_reason"], "end_turn");
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert_eq!(events[0].kind, EventKind::UserMessage);
    assert_eq!(reply_text(&events), "Echo: hello world");
    let stored = hub.store.thread(&thread.id).await.unwrap().unwrap();
    assert!(stored.engine_session_id.is_some());
}

#[tokio::test]
async fn command_output_streams_into_the_log_with_its_exit_code() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("compile it")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;

    // Hub asks for output chunks, so the command's output arrives line by line, then its exit.
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let call = |e: &&Event| e.kind == EventKind::Update && e.data["toolCallId"] == "compile-1";
    let chunks: String = events.iter().filter(call).filter_map(|e| e.data["_meta"]["terminal_output_delta"]["data"].as_str()).collect();
    assert_eq!(chunks.matches("\r\n").count(), 13);
    assert!(chunks.contains("Finished"));
    let exit = events.iter().filter(call).find_map(|e| e.data["_meta"]["terminal_exit"]["exit_code"].as_i64());
    assert_eq!(exit, Some(0));
}

#[tokio::test]
async fn a_connector_call_shows_under_the_thread_connections() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("make a picture with the connector")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;

    let links = hub_core::connections::of_thread(&hub.ctx(), &thread.id, false).await.unwrap();
    let eleven = links.used.iter().find(|c| c.label == "ElevenLabs").expect("ElevenLabs is used");
    assert_eq!(eleven.source, hub_core::model::ConnectionSource::ClaudeAi);
    assert_eq!((eleven.uses, eleven.threads), (1, 1));
    assert_eq!(eleven.tools[0].name, "creative_generate_image");
    // Hub's own tools are there for every thread, unused.
    assert!(links.available.iter().any(|c| c.name == "hub"));
}

#[tokio::test]
async fn permission_requests_wait_for_the_user() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("needs permission")).await.unwrap();

    let ask = next_event(&mut rx, EventKind::PermissionRequest).await;
    wait_status(&hub, &thread.id, ThreadStatus::WaitingOnYou).await;
    let request_id = ask.data["id"].as_str().unwrap().to_owned();
    assert_eq!(ask.data["options"][0]["optionId"], "allow");

    hub.answer_permission(&thread.id, &request_id, Some("allow".into())).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(events.iter().any(|e| e.data["status"] == "completed"));
    assert!(events.iter().any(|e| e.kind == EventKind::PermissionResolved));
}

/// A question nobody answers puts the session to sleep: its engine stops, the thread keeps
/// waiting on the user, and the answer starts the engine again and the turn goes on.
#[tokio::test]
async fn a_session_waiting_on_the_user_sleeps_and_wakes_with_the_answer() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("needs permission")).await.unwrap();
    let ask = next_event(&mut rx, EventKind::PermissionRequest).await;
    wait_status(&hub, &thread.id, ThreadStatus::WaitingOnYou).await;
    let request_id = ask.data["id"].as_str().unwrap().to_owned();

    // Not asleep while the question is young.
    assert!(hub.sleep_waiting(Duration::from_secs(3600)).await.is_empty());
    assert_eq!(hub.engines(), vec![thread.id.clone()]);

    assert_eq!(hub.sleep_waiting(Duration::ZERO).await, vec![thread.id.clone()]);
    assert!(hub.engines().is_empty(), "an asleep session keeps no engine");
    no_processes_left(&thread.id).await;
    wait_status(&hub, &thread.id, ThreadStatus::WaitingOnYou).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(events.iter().any(|e| e.kind == EventKind::Slept && e.data["ids"][0] == request_id.as_str()));
    assert!(!events.iter().any(|e| e.kind == EventKind::PermissionResolved), "the question is still open");

    hub.answer_permission(&thread.id, &request_id, Some("allow".into())).await.unwrap();
    // The answer puts the thread back to work at once; it is idle again when the turn ends.
    assert_eq!(hub.store.thread(&thread.id).await.unwrap().unwrap().status, ThreadStatus::Running);
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let woke = events.iter().position(|e| e.kind == EventKind::PermissionResolved && e.data["woke"] == true).expect("the answer is logged");
    let said = events.iter().position(|e| e.kind == EventKind::UserMessage && e.data["text"].as_str().is_some_and(|t| t.contains("put to sleep"))).expect("the engine is told");
    assert!(said > woke);
    let end = events.iter().skip(said).find(|e| e.kind == EventKind::TurnEnd).expect("the turn carried on and ended");
    assert_ne!(end.data["stop_reason"], "cancelled");
    assert_eq!(hub.engines(), vec![thread.id.clone()]);

    // Answering twice finds nothing open.
    assert!(hub.answer_permission(&thread.id, &request_id, Some("allow".into())).await.is_err());
}

/// An agent that ends its turn with a question still open (its own approval timed out) leaves
/// the thread idle, and answering the stale card afterwards does not mark it running again.
#[tokio::test]
async fn a_question_left_open_ends_with_the_turn() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("permission walkaway")).await.unwrap();
    let ask = next_event(&mut rx, EventKind::PermissionRequest).await;
    let request_id = ask.data["id"].as_str().unwrap().to_owned();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(events.iter().any(|e| e.kind == EventKind::PermissionResolved && e.data["id"] == request_id.as_str()));

    let _ = hub.answer_permission(&thread.id, &request_id, Some("allow".into())).await;
    let status = hub.store.thread(&thread.id).await.unwrap().unwrap().status;
    assert_eq!(status, ThreadStatus::Idle);
}

#[tokio::test]
async fn permission_rules_answer_for_the_user() {
    let hub = hub(Store::in_memory().unwrap());
    let prefs = hub_core::model::PermissionPrefs { allow: vec!["cargo test".into()], ..Default::default() };
    hub.set_permission_prefs(prefs).await.unwrap();
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("needs permission")).await.unwrap();
    let auto = next_event(&mut rx, EventKind::PermissionAuto).await;
    assert_eq!(auto.data["allowed"], true);
    assert_eq!(auto.data["option_id"], "allow");
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(events.iter().any(|e| e.data["status"] == "completed"));
    assert!(!events.iter().any(|e| e.kind == EventKind::PermissionRequest));

    // A refusal rule wins, and the agent hears no.
    let prefs = hub_core::model::PermissionPrefs { allow: vec!["*".into()], deny: vec!["rm *".into()], ..Default::default() };
    hub.set_permission_prefs(prefs).await.unwrap();
    let thread = hub.create_thread(new_thread("danger")).await.unwrap();
    let auto = next_event(&mut rx, EventKind::PermissionAuto).await;
    assert_eq!(auto.data["allowed"], false);
    assert_eq!(auto.data["option_id"], "reject");
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    assert!(events.iter().any(|e| e.data["status"] == "failed"));
}

#[tokio::test]
async fn cancel_stops_a_running_turn() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("slow one two three four five six")).await.unwrap();
    next_event(&mut rx, EventKind::Update).await;
    hub.cancel(&thread.id).await.unwrap();
    let end = next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(end.data["stop_reason"], "cancelled");
}

#[tokio::test]
async fn shutting_down_stops_the_turn_and_the_engine_and_the_next_message_resumes() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("slow one two three four five six")).await.unwrap();
    next_event(&mut rx, EventKind::Update).await;
    assert_eq!(hub.engines(), vec![thread.id.clone()]);

    // The workers view finds the engine as Hub's child carrying the thread's id.
    let engines = hub.engines();
    let workers = tokio::task::spawn_blocking(move || hub_core::workers::snapshot(&engines)).await.unwrap();
    let work = workers.threads.iter().find(|w| w.thread_id == thread.id).expect("thread listed");
    assert!(work.engine_running);
    assert_eq!(work.engine, Some(work.processes[0].pid));

    hub.shutdown(&thread.id, true).await.unwrap();
    let end = next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(end.data["stop_reason"], "cancelled");
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    assert!(hub.engines().is_empty());
    no_processes_left(&thread.id).await;

    hub.send(&thread.id, "again".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(hub.engines(), vec![thread.id.clone()]);
}

#[tokio::test]
async fn settling_or_archiving_an_idle_thread_stops_its_engine_but_not_one_in_a_turn() {
    use hub_core::model::ThreadPatch;
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let idle = hub.create_thread(new_thread("first")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &idle.id, ThreadStatus::Idle).await;
    assert_eq!(hub.engines(), vec![idle.id.clone()]);
    hub.patch_thread(&idle.id, ThreadPatch { settled: Some(true), ..Default::default() }).await.unwrap();
    assert!(hub.engines().is_empty(), "a settled, idle thread keeps no engine");
    no_processes_left(&idle.id).await;

    // A thread in the middle of a turn is not cut off by being put away.
    let busy = hub.create_thread(new_thread("slow one two three four five six")).await.unwrap();
    next_event(&mut rx, EventKind::Update).await;
    hub.patch_thread(&busy.id, ThreadPatch { archived: Some(true), ..Default::default() }).await.unwrap();
    assert_eq!(hub.engines(), vec![busy.id.clone()]);
    hub.shutdown(&busy.id, true).await.unwrap();
}

#[tokio::test]
async fn a_crashed_engine_is_restarted_and_resumed() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("first")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;

    hub.send(&thread.id, "crash".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::Error).await;
    wait_status(&hub, &thread.id, ThreadStatus::Error).await;

    hub.send(&thread.id, "again".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let text = reply_text(&events);
    assert!(text.ends_with("Echo: again"), "{text}");
    assert!(!text.contains("replayed"), "replayed history leaked into the log: {text}");
}

#[tokio::test]
async fn queued_messages_can_be_edited_or_removed_until_the_engine_takes_them() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("slow first")).await.unwrap();
    next_event(&mut rx, EventKind::TurnStart).await;
    for text in ["second", "third", "fourth"] {
        hub.send(&thread.id, text.into(), vec![], None).await.unwrap();
    }
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let seq = |text: &str| events.iter().find(|e| e.kind == EventKind::UserMessage && e.data["text"] == text).unwrap().seq;
    let (first, third, fourth) = (seq("slow first"), seq("third"), seq("fourth"));

    assert!(hub.change_queued(&thread.id, first, None).await.is_err(), "the running message stays");
    assert!(hub.change_queued(&thread.id, third, Some("  ".into())).await.is_err(), "an edit is never empty");
    hub.change_queued(&thread.id, third, Some("third, edited".into())).await.unwrap();
    hub.change_queued(&thread.id, fourth, None).await.unwrap();
    assert!(hub.change_queued(&thread.id, fourth, Some("back".into())).await.is_err(), "a removed message stays removed");

    for _ in 0..3 {
        next_event(&mut rx, EventKind::TurnEnd).await;
    }
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let text = reply_text(&events);
    assert!(text.ends_with("Echo: secondEcho: third, edited"), "{text}");
    assert!(!text.contains("fourth"), "{text}");
    let started: Vec<i64> = events.iter().filter(|e| e.kind == EventKind::TurnStart).filter_map(|e| e.data["prompt"].as_i64()).collect();
    assert!(!started.contains(&fourth));
    assert!(hub.change_queued(&thread.id, third, Some("late".into())).await.is_err(), "too late once it ran");
}

#[tokio::test]
async fn steering_stops_the_turn_and_goes_ahead_of_the_queue() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("slow first")).await.unwrap();
    next_event(&mut rx, EventKind::TurnStart).await;
    hub.send(&thread.id, "second".into(), vec![], None).await.unwrap();
    hub.send_once(&thread.id, "change of plan".into(), vec![], None, None, true).await.unwrap();
    let ends: Vec<Event> = {
        let mut ends = Vec::new();
        for _ in 0..3 {
            ends.push(next_event(&mut rx, EventKind::TurnEnd).await);
        }
        ends
    };
    assert_eq!(ends[0].data["stop_reason"], "cancelled");
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let said = |seq: i64| events.iter().find(|e| e.seq == seq).unwrap().data["text"].as_str().unwrap().to_owned();
    let order: Vec<String> = events.iter().filter(|e| e.kind == EventKind::TurnStart).map(|e| said(e.data["prompt"].as_i64().unwrap())).collect();
    assert_eq!(order, ["slow first", "change of plan", "second"]);
    assert!(events.iter().any(|e| e.kind == EventKind::Queue && e.data["action"] == "steered"));
}

#[tokio::test]
async fn an_engine_that_ignores_stop_is_stopped_anyway_and_the_queue_goes_on() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("deaf one")).await.unwrap();
    next_event(&mut rx, EventKind::Update).await;
    hub.send(&thread.id, "after".into(), vec![], None).await.unwrap();
    hub.cancel(&thread.id).await.unwrap();
    // The mock agent sleeps through cancel for half a minute; Hub gives it ten seconds.
    let end = next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(end.data["stop_reason"], "cancelled");
    assert_eq!(end.data["forced"], true);
    // The queued message runs on a fresh engine.
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
    let text = reply_text(&events);
    assert!(text.ends_with("Echo: after"), "{text}");
    // Only the fresh engine is left; the deaf one did not live on as an orphan.
    assert_eq!(hub.engines(), vec![thread.id.clone()]);
    hub.shutdown(&thread.id, true).await.unwrap();
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    no_processes_left(&thread.id).await;
}

#[tokio::test]
async fn threads_survive_a_restart_of_the_core() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.db");
    let id = {
        let hub = hub(Store::open(&path).unwrap());
        let mut rx = hub.subscribe();
        let thread = hub.create_thread(new_thread("before")).await.unwrap();
        next_event(&mut rx, EventKind::TurnEnd).await;
        thread.id
    };
    let hub = hub(Store::open(&path).unwrap());
    let mut rx = hub.subscribe();
    hub.send(&id, "after".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let text = reply_text(&hub.store.events(&id, 0, 1000).await.unwrap());
    assert_eq!(text, "Echo: beforeEcho: after");
}

#[tokio::test]
async fn a_greeting_title_is_replaced_by_the_first_real_message() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("Hej!")).await.unwrap();
    assert_eq!(thread.title, "Hej!");
    next_event(&mut rx, EventKind::TurnEnd).await;
    hub.send(&thread.id, "Fix the flaky login test in the auth crate".into(), vec![], None).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    hub.send(&thread.id, "And then update the changelog".into(), vec![], None).await.unwrap();
    let title = hub.store.thread(&thread.id).await.unwrap().unwrap().title;
    assert_eq!(title, "Fix the flaky login test in the auth crate");
}

#[tokio::test]
async fn renaming_locks_the_title_and_archiving_hides_until_used() {
    use hub_core::model::ThreadPatch;
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("hi")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let patch = ThreadPatch { title: Some(" Atlas build ".into()), archived: Some(true), ..Default::default() };
    let renamed = hub.patch_thread(&thread.id, patch).await.unwrap();
    assert_eq!(renamed.title, "Atlas build");
    assert!(renamed.archived);

    hub.send(&thread.id, "now a long and descriptive message".into(), vec![], None).await.unwrap();
    let after = hub.store.thread(&thread.id).await.unwrap().unwrap();
    assert_eq!(after.title, "Atlas build", "a renamed thread keeps its name");
    assert!(!after.archived, "sending to an archived thread brings it back");
}

#[tokio::test]
async fn a_thread_whose_folder_is_gone_says_so_instead_of_the_agents_cwd_error() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let folder = hub_core::janitor::test_dir().join("gone");
    std::fs::create_dir_all(&folder).unwrap();
    let mut new = new_thread("hi");
    new.cwd = folder.display().to_string();
    let thread = hub.create_thread(new).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    hub.shutdown(&thread.id, true).await.unwrap();
    std::fs::remove_dir_all(&folder).unwrap();

    hub.send(&thread.id, "are you there".into(), vec![], None).await.unwrap();
    let error = next_event(&mut rx, EventKind::Error).await;
    let message = error.data["message"].as_str().unwrap();
    assert!(message.contains("folder is gone"), "{message}");
    assert!(!message.contains("cwd"), "{message}");
}

#[tokio::test]
async fn a_finished_turn_comes_back_unread_and_settling_holds_until_used() {
    use hub_core::model::ThreadPatch;
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("hi")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let back = hub.store.thread(&thread.id).await.unwrap().unwrap();
    assert!(back.returned_at.is_some(), "a finished turn hands the ball back");
    assert!(back.seen_at.is_none());
    assert!(back.sent_at.is_some(), "the first message counts as sending it work");

    let seen = ThreadPatch { seen: Some(true), settled: Some(true), ..Default::default() };
    let marked = hub.patch_thread(&thread.id, seen).await.unwrap();
    assert!(marked.seen_at >= back.returned_at && marked.settled_at.is_some());

    hub.send(&thread.id, "again".into(), vec![], None).await.unwrap();
    let after = hub.store.thread(&thread.id).await.unwrap().unwrap();
    assert!(after.settled_at.is_none(), "writing to a settled thread brings it back");
    assert!(after.sent_at > back.sent_at, "the working list orders by the latest message");
}

#[tokio::test]
async fn pins_snoozes_and_unread_marks_park_a_thread() {
    use hub_core::model::{ThreadPatch, now_ms};
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("hi")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &thread.id, ThreadStatus::Idle).await;
    let patch = |p: ThreadPatch| hub.patch_thread(&thread.id, p);

    let pinned = patch(ThreadPatch { pinned: Some(true), ..Default::default() }).await.unwrap();
    assert!(pinned.pinned_at.is_some());
    let settled = patch(ThreadPatch { settled: Some(true), ..Default::default() }).await.unwrap();
    assert!(settled.pinned_at.is_none(), "settling takes the pin off");
    let moved = patch(ThreadPatch { pin_order: Some(42), ..Default::default() }).await.unwrap();
    assert_eq!(moved.pinned_at, Some(42));
    assert!(moved.settled_at.is_none(), "pinning brings a settled thread back");

    let later = now_ms() + 3_600_000;
    let snoozed = patch(ThreadPatch { snooze_until: Some(later), ..Default::default() }).await.unwrap();
    assert_eq!(snoozed.snoozed_until, Some(later));
    let woken = patch(ThreadPatch { snooze_until: Some(0), ..Default::default() }).await.unwrap();
    assert!(woken.snoozed_until.is_none() && woken.returned_at >= snoozed.returned_at);

    let seen = patch(ThreadPatch { seen: Some(true), ..Default::default() }).await.unwrap();
    assert!(seen.seen_at.is_some());
    let unread = patch(ThreadPatch { seen: Some(false), ..Default::default() }).await.unwrap();
    assert!(unread.seen_at.is_none() && unread.returned_at.is_some());
    assert_eq!(unread.updated_at, thread.updated_at.max(unread.updated_at), "marks never move the thread");

    patch(ThreadPatch { snooze_until: Some(later), ..Default::default() }).await.unwrap();
    hub.send(&thread.id, "again".into(), vec![], None).await.unwrap();
    let after = hub.store.thread(&thread.id).await.unwrap().unwrap();
    assert!(after.snoozed_until.is_none(), "writing to a snoozed thread wakes it");
}

#[tokio::test]
async fn idle_threads_settle_by_themselves_unless_kept() {
    use hub_core::model::ThreadPatch;
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let old = hub.create_thread(new_thread("hi")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    let kept = hub.create_thread(new_thread("hi")).await.unwrap();
    next_event(&mut rx, EventKind::TurnEnd).await;
    wait_status(&hub, &old.id, ThreadStatus::Idle).await;
    wait_status(&hub, &kept.id, ThreadStatus::Idle).await;
    let long_ago = |t: &mut hub_core::model::Thread| {
        t.returned_at = Some(1);
        t.sent_at = Some(1);
    };
    hub.store.mark_thread(&old.id, long_ago).await.unwrap();
    hub.store.mark_thread(&kept.id, long_ago).await.unwrap();
    hub.patch_thread(&kept.id, ThreadPatch { auto_settle: Some(false), ..Default::default() }).await.unwrap();

    hub.tidy_once().await.unwrap();
    assert!(hub.store.thread(&old.id).await.unwrap().unwrap().settled_at.is_some(), "three idle days settle it");
    assert!(hub.store.thread(&kept.id).await.unwrap().unwrap().settled_at.is_none(), "unless auto-settle is off");
}

#[tokio::test]
async fn deleting_a_thread_stops_it_and_removes_its_log() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("slow one two three four five six seven")).await.unwrap();
    next_event(&mut rx, EventKind::Update).await;
    hub.delete_thread(&thread.id).await.unwrap();
    assert!(hub.store.thread(&thread.id).await.unwrap().is_none());
    assert!(hub.store.events(&thread.id, 0, 100).await.unwrap().is_empty());
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(hub.store.events(&thread.id, 0, 100).await.unwrap().is_empty(), "no events after delete");
}

/// The pid a "stray" agent said it started (`Started 1234.`).
fn stray_pid(events: &[Event]) -> u32 {
    let text = reply_text(events);
    let after = text.split("Started ").nth(1).expect("the agent said which process it started");
    after.split('.').next().unwrap().trim().parse().unwrap()
}

/// Whether process `pid` still runs (a zombie, kept only for its parent to read, does not).
#[cfg(target_os = "linux")]
fn running(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| stat.rsplit(')').next().is_some_and(|rest| !rest.trim_start().starts_with('Z')))
}

/// Deleting a thread ends its engine and what the engine started, even when the engine ignores
/// Stop and a tool it started never ends in a process group of its own (a `find` that hangs).
#[cfg(target_os = "linux")]
#[tokio::test]
async fn deleting_a_thread_ends_an_engine_that_ignores_stop_and_the_tool_it_left_running() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("stray deaf one")).await.unwrap();
    next_event(&mut rx, EventKind::Update).await;
    // The pid arrives with the first chunks of the reply.
    let mut pid = None;
    for _ in 0..200 {
        let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
        if reply_text(&events).contains("Started ") && reply_text(&events).contains(". ") {
            pid = Some(stray_pid(&events));
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let pid = pid.expect("the tool started");
    assert!(running(pid));

    let started = std::time::Instant::now();
    hub.delete_thread(&thread.id).await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(8), "deleting waited for the engine: {:?}", started.elapsed());
    assert!(hub.engines().is_empty());
    assert!(hub.store.thread(&thread.id).await.unwrap().is_none());
    no_processes_left(&thread.id).await;
    assert!(!running(pid), "the tool the engine started is still running");
}

/// Stop on an engine that ignores it ends the tool it was waiting on too, not only the engine.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn stopping_an_engine_that_ignores_stop_ends_the_tool_it_waited_on() {
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();
    let thread = hub.create_thread(new_thread("stray deaf one")).await.unwrap();
    next_event(&mut rx, EventKind::Update).await;
    let mut pid = None;
    for _ in 0..200 {
        let events = hub.store.events(&thread.id, 0, 1000).await.unwrap();
        if reply_text(&events).contains("Started ") && reply_text(&events).contains(". ") {
            pid = Some(stray_pid(&events));
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let pid = pid.expect("the tool started");
    assert!(running(pid));
    hub.cancel(&thread.id).await.unwrap();
    let end = next_event(&mut rx, EventKind::TurnEnd).await;
    assert_eq!(end.data["forced"], true);
    for _ in 0..200 {
        if !running(pid) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!running(pid), "the tool the engine waited on is still running");
    hub.shutdown(&thread.id, true).await.unwrap();
    no_processes_left(&thread.id).await;
}

/// Collects the warnings logged on this test's thread (the engines' tasks run there too).
pub(super) struct Warnings(pub(super) Arc<std::sync::Mutex<Vec<String>>>);

impl tracing::Subscriber for Warnings {
    fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::sometimes()
    }
    fn enabled(&self, meta: &tracing::Metadata<'_>) -> bool {
        meta.is_event() && *meta.level() <= tracing::Level::WARN
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        struct Text<'a>(&'a mut String);
        impl tracing::field::Visit for Text<'_> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                use std::fmt::Write;
                let _ = write!(self.0, "{}={value:?} ", field.name());
            }
        }
        let mut text = String::new();
        event.record(&mut Text(&mut text));
        self.0.lock().unwrap().push(text);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

/// What the engine still says about a thread that was removed under it (deleted mid-turn, or
/// replaced by a move back from another machine) has nowhere to go: it is dropped, and neither
/// the agent's updates nor the engine's end are logged as failures (problem #13).
#[tokio::test]
async fn a_thread_removed_under_its_engine_ends_it_quietly() {
    let warnings = Arc::new(std::sync::Mutex::new(Vec::new()));
    let _logging = tracing::subscriber::set_default(Warnings(warnings.clone()));
    let hub = hub(Store::in_memory().unwrap());
    let mut rx = hub.subscribe();

    // Removed from the store while the agent keeps streaming.
    let streaming = hub.create_thread(new_thread("slow one two three four five six seven")).await.unwrap();
    next_event(&mut rx, EventKind::Update).await;
    hub.store.delete_thread(&streaming.id).await.unwrap();
    engine_gone(&hub, &streaming.id).await;

    // Deleted while it waits on a question, as clearing the inbox simulator does.
    let asking = hub.create_thread(new_thread("needs permission")).await.unwrap();
    next_event(&mut rx, EventKind::PermissionRequest).await;
    hub.delete_thread(&asking.id).await.unwrap();
    engine_gone(&hub, &asking.id).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert!(hub.store.events(&streaming.id, 0, 100).await.unwrap().is_empty());
    assert!(hub.store.events(&asking.id, 0, 100).await.unwrap().is_empty());
    let warnings = warnings.lock().unwrap();
    assert!(warnings.is_empty(), "{warnings:#?}");
}
