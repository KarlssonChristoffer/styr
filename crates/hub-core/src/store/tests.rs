//! Guards the store: pictures kept once as files, per-thread event numbering and the narrow log
//! reads matching the whole log, migration from the first schema and refusal of a newer one,
//! restoring a backup, unknown kinds kept, moved threads, and running threads reset on reopen.

fn picture_update(data: &str) -> serde_json::Value {
    serde_json::json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": "img",
        "content": [{ "type": "content", "content": { "type": "image", "mimeType": "image/png", "data": data } }],
        "rawOutput": { "result": data },
    })
}

fn picture_text() -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode((0..9000u32).map(|i| (i % 253) as u8).collect::<Vec<_>>())
}

#[tokio::test]
async fn a_picture_is_stored_once_as_a_file_and_read_back_whole() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let store = Store::open(&dir.path().join("hub.db"))?;
    store.insert_thread(thread("t1")).await?;
    let data = picture_text();
    let live = store.append_event("t1", EventKind::Update, picture_update(&data)).await?;
    assert_eq!(live.data["rawOutput"]["result"], data.as_str(), "whoever is told gets the event as it came");
    let row: String = store.with(|c| Ok(c.query_row("SELECT data FROM events", [], |r| r.get(0))?)).await?;
    assert!(row.len() < 1000, "the row is small: {}", row.len());
    assert_eq!(std::fs::read_dir(dir.path().join("blobs"))?.count(), 1);
    let read = store.events("t1", 0, 10).await?;
    assert_eq!(read[0].data["content"][0]["content"]["data"], data.as_str());
    Ok(())
}

#[tokio::test]
async fn deleting_a_thread_removes_its_pictures_unless_another_thread_uses_them() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let store = Store::open(&dir.path().join("hub.db"))?;
    store.insert_thread(thread("t1")).await?;
    store.insert_thread(thread("t2")).await?;
    let (a, mut b) = (picture_text(), picture_text());
    b.insert_str(0, "QUJD");
    for (t, d) in [("t1", &a), ("t1", &b), ("t2", &a)] {
        store.append_event(t, EventKind::Update, picture_update(d)).await?;
    }
    assert_eq!(std::fs::read_dir(dir.path().join("blobs"))?.count(), 2);
    store.delete_thread("t1").await?;
    assert_eq!(std::fs::read_dir(dir.path().join("blobs"))?.count(), 1, "only the shared picture stays");
    assert_eq!(store.events("t2", 0, 10).await?[0].data["content"][0]["content"]["data"], a.as_str());
    store.delete_thread("t2").await?;
    assert_eq!(std::fs::read_dir(dir.path().join("blobs"))?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn old_pictures_move_out_after_a_checked_copy_and_only_once() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let store = Store::open(&dir.path().join("hub.db"))?;
    store.insert_thread(thread("t1")).await?;
    let data = picture_text();
    // As an older version wrote it: the whole picture, twice, in the row.
    let old = picture_update(&data);
    store
        .with(move |c| {
            c.execute(
                "INSERT INTO events (thread_id, seq, ts, kind, data) VALUES ('t1', 1, 1, 'update', ?1)",
                [old.to_string()],
            )?;
            Ok(())
        })
        .await?;
    let copy = dir.path().join("backups").join("before-pictures.db");
    assert_eq!(store.lift_old_pictures(&copy).await?, 1);
    assert!(copy.is_file(), "the copy was made first");
    let row: String = store.with(|c| Ok(c.query_row("SELECT data FROM events", [], |r| r.get(0))?)).await?;
    assert!(row.len() < 1000);
    let read = store.events("t1", 0, 10).await?;
    assert_eq!(read[0].data["content"][0]["content"]["data"], data.as_str());
    assert_eq!(store.lift_old_pictures(&copy).await?, 0, "it runs once");
    // The copy still holds the picture as it was.
    let kept: String = Connection::open(&copy)?.query_row("SELECT data FROM events", [], |r| r.get(0))?;
    assert!(kept.len() > 18_000);
    Ok(())
}
use super::*;

fn thread(id: &str) -> Thread {
    Thread {
        id: id.into(),
        title: "t".into(),
        cwd: "/tmp".into(),
        engine: "claude".into(),
        status: ThreadStatus::Running,
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
        created_at: 1,
        updated_at: 1,
    }
}

#[tokio::test]
async fn marks_keep_the_activity_time() {
    let store = Store::in_memory().unwrap();
    store.insert_thread(thread("a")).await.unwrap();
    let marked = store.mark_thread("a", |t| t.seen_at = Some(5)).await.unwrap();
    assert_eq!((marked.updated_at, marked.seen_at), (1, Some(5)));
    let read = store.thread("a").await.unwrap().unwrap();
    assert_eq!(read.seen_at, Some(5));
    assert!(store.update_thread("a", |t| t.title = "u".into()).await.unwrap().updated_at > 1);
}

#[tokio::test]
async fn events_get_sequential_numbers_per_thread() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    store.insert_thread(thread("b")).await?;
    for _ in 0..3 {
        store.append_event("a", EventKind::Update, serde_json::json!({})).await?;
    }
    let b = store.append_event("b", EventKind::UserMessage, serde_json::json!({"text": "hi"})).await?;
    assert_eq!(b.seq, 1);
    let after_first = store.events("a", 1, 100).await?;
    assert_eq!(after_first.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![2, 3]);
    Ok(())
}

#[tokio::test]
async fn events_after_last_starts_at_the_last_marker() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    let kinds = [EventKind::UserMessage, EventKind::Update, EventKind::TurnEnd, EventKind::Update, EventKind::Update];
    for k in kinds {
        store.append_event("a", k, serde_json::json!({})).await?;
    }
    let got = store.events_after_last("a", EventKind::TurnEnd, &[EventKind::Update]).await?;
    assert!(got.is_empty());
    let got = store.events_after_last("a", EventKind::TurnEnd, &[]).await?;
    assert_eq!(got.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![4, 5]);
    let got = store.events_after_last("a", EventKind::Design, &[EventKind::Update]).await?;
    assert_eq!(got.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 3]);
    Ok(())
}

#[tokio::test]
async fn events_of_reads_only_the_kinds_asked_for() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    store.insert_thread(thread("b")).await?;
    store.append_event("a", EventKind::Update, serde_json::json!({ "big": "x".repeat(1000) })).await?;
    store.append_event("a", EventKind::Design, serde_json::json!({ "on": true })).await?;
    store.append_event("a", EventKind::Update, serde_json::json!({})).await?;
    store.append_event("a", EventKind::UserMessage, serde_json::json!({ "text": "hi" })).await?;
    store.append_event("b", EventKind::Design, serde_json::json!({ "on": false })).await?;
    let kinds = [EventKind::Design, EventKind::UserMessage];
    let got = store.events_of("a", 0, &kinds).await?;
    assert_eq!(got.iter().map(|e| (e.seq, e.kind)).collect::<Vec<_>>(), vec![(2, EventKind::Design), (4, EventKind::UserMessage)]);
    assert_eq!(store.events_of("a", 2, &kinds).await?.len(), 1);
    assert!(store.events_of("a", 0, &[]).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn narrow_reads_of_a_log() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    let chunk = |t: &str| serde_json::json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": t } });
    store.append_event("a", EventKind::UserMessage, serde_json::json!({ "text": "q" })).await?; // 1
    store.append_event("a", EventKind::Update, chunk("he")).await?; // 2
    store.append_event("a", EventKind::Update, serde_json::json!({ "sessionUpdate": "tool_call" })).await?; // 3
    store.append_event("a", EventKind::Update, chunk("llo")).await?; // 4
    store.append_event("a", EventKind::TurnEnd, serde_json::json!({})).await?; // 5
    store.append_event("a", EventKind::Reported, serde_json::json!({})).await?; // 6
    assert_eq!(store.last_seq("a", Some(EventKind::TurnEnd)).await?, Some(5));
    assert_eq!(store.last_seq("a", None).await?, Some(6));
    assert_eq!(store.last_seq("a", Some(EventKind::Error)).await?, None);
    let chunks = store.reply_chunks("a", 1, 5).await?;
    assert_eq!(chunks.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![2, 4]);
    let tail = store.events_except("a", 1, &[EventKind::Update]).await?;
    assert_eq!(tail.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![5, 6]);
    let last = store.last_events("a", 2, &[EventKind::Reported]).await?;
    assert_eq!(last.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![4, 5]);
    Ok(())
}

#[tokio::test]
async fn events_with_updates_is_the_whole_log_filtered() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    store.insert_thread(thread("b")).await?;
    let update = |kind: serde_json::Value| serde_json::json!({ "sessionUpdate": kind, "content": { "text": "x" } });
    store.append_event("a", EventKind::UserMessage, serde_json::json!({ "text": "q" })).await?; // 1
    store.append_event("a", EventKind::Update, update("agent_message_chunk".into())).await?; // 2
    store.append_event("a", EventKind::Update, update("tool_call".into())).await?; // 3
    store.append_event("a", EventKind::Update, update("tool_call_update".into())).await?; // 4
    store.append_event("a", EventKind::Update, update(5.into())).await?; // 5: not a name
    store.append_event("a", EventKind::Update, serde_json::json!({ "content": "none" })).await?; // 6: no sessionUpdate
    store.append_event("a", EventKind::Setting, serde_json::json!({ "sessionUpdate": "agent_message_chunk" })).await?; // 7
    store.append_event("a", EventKind::Rewind, serde_json::json!({ "from": 2 })).await?; // 8
    store.append_event("a", EventKind::TurnEnd, serde_json::json!({})).await?; // 9
    store.append_event("a", EventKind::Update, update("agent_message_chunk".into())).await?; // 10
    store.append_event("b", EventKind::Update, update("agent_message_chunk".into())).await?;
    let whole = store.events("a", 0, i64::MAX).await?;
    let cases: &[(&[EventKind], &[&str])] = &[
        (&[EventKind::UserMessage, EventKind::Rewind], &["agent_message_chunk", "tool_call"]),
        (&[EventKind::TurnEnd, EventKind::Error], &["agent_message_chunk"]),
        (&[EventKind::Setting], &[]),
        (&[], &["tool_call_update"]),
        (&[], &[]),
        (&[EventKind::Update], &["tool_call"]),
    ];
    for (kinds, updates) in cases {
        for after in [0, 1, 2, 9, 10, 99] {
            let want: Vec<i64> = whole
                .iter()
                .filter(|e| e.seq > after)
                .filter(|e| kinds.contains(&e.kind) || (e.kind == EventKind::Update && crate::model::update_kind(&e.data).is_some_and(|k| updates.contains(&k))))
                .map(|e| e.seq)
                .collect();
            let got: Vec<i64> = store.events_with_updates("a", after, kinds, updates).await?.iter().map(|e| e.seq).collect();
            assert_eq!(got, want, "kinds {kinds:?}, updates {updates:?}, after {after}");
        }
    }
    assert_eq!(
        store.events_with_updates("a", 0, &[EventKind::Rewind], &["agent_message_chunk"]).await?.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![2, 8, 10],
        "another thread's chunk is not read"
    );
    assert!(store.events_with_updates("nobody", 0, &[EventKind::UserMessage], &["tool_call"]).await?.is_empty());
    Ok(())
}

/// `replies::conversation` hands `conversation_log` and `handover_log` the same text as the whole log.
#[tokio::test]
async fn a_conversation_read_writes_the_same_log_as_the_whole_log() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    let update = |kind: &str, text: &str| serde_json::json!({ "sessionUpdate": kind, "title": text, "content": { "text": text } });
    let big = "word ".repeat(2_000);
    for turn in 0..8 {
        store.append_event("a", EventKind::UserMessage, serde_json::json!({ "text": format!("ask {turn} {big}") })).await?;
        store.append_event("a", EventKind::TurnStart, serde_json::json!({})).await?;
        store.append_event("a", EventKind::Update, update("agent_message_chunk", "look")).await?;
        store.append_event("a", EventKind::Update, update("tool_call", "Read x")).await?;
        store.append_event("a", EventKind::Update, update("tool_call_update", "output")).await?;
        store.append_event("a", EventKind::Update, update("agent_message_chunk", &format!("done {turn}"))).await?;
        store.append_event("a", EventKind::TurnEnd, serde_json::json!({})).await?;
        if turn == 3 {
            store.append_event("a", EventKind::Rewind, serde_json::json!({ "from": 8 })).await?;
        }
    }
    let whole = store.events("a", 0, i64::MAX).await?;
    let narrow = crate::replies::conversation(&store, "a", &[]).await?;
    assert!(narrow.len() < whole.len());
    assert_eq!(crate::hub::conversation_log(&narrow), crate::hub::conversation_log(&whole));
    assert_eq!(crate::hub::handover_log("a", &narrow), crate::hub::handover_log("a", &whole));
    assert!(crate::replies::conversation(&store, "nobody", &[]).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_brief_reads_one_turn_and_the_newest_events_not_the_log() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    let chunk = |t: &str| serde_json::json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": t } });
    let tool = |t: &str| serde_json::json!({ "sessionUpdate": "tool_call", "title": t });
    store.append_event("a", EventKind::UserMessage, serde_json::json!({ "text": "one" })).await?; // 1
    store.append_event("a", EventKind::Update, chunk("old")).await?; // 2
    store.append_event("a", EventKind::TurnEnd, serde_json::json!({})).await?; // 3
    store.append_event("a", EventKind::UserMessage, serde_json::json!({ "text": "two" })).await?; // 4
    store.append_event("a", EventKind::Update, tool("first")).await?; // 5
    store.append_event("a", EventKind::Update, chunk("he")).await?; // 6
    store.append_event("a", EventKind::Update, tool("second")).await?; // 7
    store.append_event("a", EventKind::Update, chunk("llo")).await?; // 8
    store.append_event("a", EventKind::TurnEnd, serde_json::json!({})).await?; // 9
    store.append_event("a", EventKind::Update, chunk("next")).await?; // 10
    let seqs = |v: Vec<Event>| v.iter().map(|e| e.seq).collect::<Vec<_>>();
    assert_eq!(seqs(store.last_reply_chunks("a").await?), vec![6, 8]);
    assert_eq!(store.last_event("a", EventKind::UserMessage).await?.map(|e| e.seq), Some(4));
    assert_eq!(store.last_event("a", EventKind::Error).await?.map(|e| e.seq), None);
    assert_eq!(store.last_tool_call("a").await?.map(|e| e.seq), Some(7));
    // A turn that has not ended yet: what it has said since the last end or error.
    store.insert_thread(thread("b")).await?;
    store.append_event("b", EventKind::Update, chunk("x")).await?; // 1
    store.append_event("b", EventKind::Error, serde_json::json!({})).await?; // 2
    store.append_event("b", EventKind::Update, chunk("y")).await?; // 3
    assert_eq!(seqs(store.last_reply_chunks("b").await?), vec![3]);
    Ok(())
}

#[tokio::test]
async fn a_message_removed_or_dropped_from_the_queue_is_not_queued_again() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    store.set_value(TURNS_SINCE, "0").await?;
    let mut seqs = Vec::new();
    for _ in 0..3 {
        seqs.push(store.append_event("a", EventKind::UserMessage, serde_json::json!({"text": "hi"})).await?.seq);
    }
    store.append_event("a", EventKind::Queue, serde_json::json!({ "seq": seqs[0], "action": "dropped" })).await?;
    store.append_event("a", EventKind::Queue, serde_json::json!({ "seq": seqs[1], "action": "edited", "text": "x" })).await?;
    let left: Vec<i64> = store.last_turn("a").await?.queued.iter().map(|e| e.seq).collect();
    assert_eq!(left, vec![seqs[1], seqs[2]]);
    Ok(())
}

#[tokio::test]
async fn an_event_for_a_gone_thread_says_which_thread() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    store.delete_thread("a").await?;
    let err = store.append_event("a", EventKind::Update, serde_json::json!({})).await.unwrap_err();
    assert!(gone(&err));
    assert_eq!(err.to_string(), "thread a is gone: no row in threads for events.thread_id");
    let err = store.append_event_once("a", EventKind::UserMessage, serde_json::json!({}), "c1").await.unwrap_err();
    assert!(gone(&err));
    Ok(())
}

#[tokio::test]
async fn activity_has_messages_with_text_and_turns() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    store.append_event("a", EventKind::UserMessage, serde_json::json!({"text": "hi"})).await?;
    store.append_event("a", EventKind::Update, serde_json::json!({})).await?;
    store.append_event("a", EventKind::TurnEnd, serde_json::json!({"stop_reason": "end_turn"})).await?;
    let marks = store.activity(0, i64::MAX).await?;
    assert_eq!(marks.iter().map(|m| m.message.as_deref()).collect::<Vec<_>>(), vec![Some("hi"), None]);
    assert!(store.activity(now_ms() + 1000, i64::MAX).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn history_finds_messages_prompts_and_days() -> Result<()> {
    use crate::model::rows::{CcMark, CcSession};
    use crate::model::HistoryKind;
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    store.append_event("a", EventKind::UserMessage, serde_json::json!({"text": "fix the Port leak"})).await?;
    store.append_event("a", EventKind::UserMessage, serde_json::json!({"text": "100% done"})).await?;
    let prompt = CcMark { key: "u".into(), session_id: "s".into(), ts: 5, prompt: Some("why does the port stay open".into()), ..Default::default() };
    let session = CcSession { id: "s".into(), cwd: Some("/w/knegarn".into()), title: Some("Ports".into()) };
    store.cc_import(vec![prompt], vec![session], vec![]).await?;
    store.set_value("activity.summary.2026-10-29", "Fixed the port leak in Hub.").await?;
    let hits = store.search_history("port", 10).await?;
    let kinds: Vec<_> = hits.iter().map(|h| h.kind).collect();
    assert_eq!(kinds, vec![HistoryKind::Message, HistoryKind::Prompt, HistoryKind::Day]);
    assert_eq!(hits[1].title.as_deref(), Some("Ports"));
    assert_eq!(hits[2].date.as_deref(), Some("2026-10-29"));
    // LIKE's wildcards are taken literally.
    assert_eq!(store.search_history("0%", 10).await?.len(), 1);
    assert!(store.search_history("_", 10).await?.is_empty());
    assert_eq!(store.values_with_prefix("activity.summary.").await?, vec![("2026-10-29".into(), "Fixed the port leak in Hub.".into())]);
    Ok(())
}

#[tokio::test]
async fn databases_from_the_first_version_are_migrated() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("hub.db");
    {
        let old = Connection::open(&path)?;
        old.execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, title TEXT NOT NULL, cwd TEXT NOT NULL,
             engine TEXT NOT NULL, status TEXT NOT NULL, engine_session_id TEXT,
             created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             INSERT INTO threads VALUES ('a', 'Hej!', '/tmp', 'claude', 'idle', 's1', 1, 1);",
        )?;
    }
    let thread = Store::open(&path)?.thread("a").await?.unwrap();
    assert_eq!(thread.title, "Hej!");
    assert!(!thread.archived && !thread.title_locked);
    Ok(())
}

#[test]
fn a_database_from_a_newer_version_is_refused() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("hub.db");
    drop(Store::open(&path)?);
    let newer = schema_version() as i64 + 1;
    Connection::open(&path)?.pragma_update(None, "user_version", newer)?;
    let err = Store::open(&path).err().expect("opening should fail");
    assert!(err.downcast_ref::<TooNew>().is_some(), "{err:#}");
    Ok(())
}

#[tokio::test]
async fn a_backup_can_be_restored_into_a_running_store() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let store = Store::open(&dir.path().join("hub.db"))?;
    store.insert_thread(thread("a")).await?;
    let copy = dir.path().join("backups/copy.db");
    store.backup(&copy).await?;
    store.insert_thread(thread("b")).await?;
    store.restore(&copy).await?;
    let ids: Vec<_> = store.threads().await?.into_iter().map(|t| t.id).collect();
    assert_eq!(ids, vec!["a"]);
    Ok(())
}

#[tokio::test]
async fn kinds_from_a_newer_version_are_kept_unknown() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    store
        .with(|c| {
            c.execute("INSERT INTO events VALUES ('a', 1, 1, 'from_the_future', '{}')", [])?;
            Ok(())
        })
        .await?;
    assert_eq!(store.events("a", 0, 10).await?[0].kind, EventKind::Unknown);
    Ok(())
}

#[tokio::test]
async fn a_moved_thread_keeps_its_log_and_leaves_the_list() -> Result<()> {
    let store = Store::in_memory()?;
    store.insert_thread(thread("a")).await?;
    store.insert_thread(thread("b")).await?;
    store.append_event("a", EventKind::UserMessage, serde_json::json!({ "text": "hi" })).await?;
    let log = store.events("a", 0, 100).await?;

    // On the machine it went to: the same id, the same seqs and times.
    let there = Store::in_memory()?;
    there.import_thread(store.thread("a").await?.unwrap(), log.clone()).await?;
    let got = there.events("a", 0, 100).await?;
    assert_eq!((got[0].seq, got[0].ts, got[0].data["text"].as_str()), (log[0].seq, log[0].ts, Some("hi")));
    let next = there.append_event("a", EventKind::Moved, serde_json::json!({ "from": "PC" })).await?;
    assert_eq!(next.seq, log[0].seq + 1, "the log goes on where it was");

    // On the machine it left: not listed, but still readable, and it can come back.
    store.set_moved_to("a", "tincan").await?;
    let listed: Vec<String> = store.threads().await?.into_iter().map(|t| t.id).collect();
    assert_eq!(listed, ["b"]);
    assert!(store.thread("a").await?.is_some());
    assert_eq!(store.moved_to("a").await?.as_deref(), Some("tincan"));
    store.clear_moved("a").await?;
    assert_eq!(store.threads().await?.len(), 2);
    Ok(())
}

#[test]
fn reopening_resets_running_threads() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("hub.db");
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let store = Store::open(&path)?;
        store.insert_thread(thread("a")).await
    })?;
    let thread = rt.block_on(async { Store::open(&path)?.thread("a").await })?.unwrap();
    assert_eq!(thread.status, ThreadStatus::Idle);
    assert!(thread.returned_at.is_some(), "a cut-off thread comes back to the user");
    Ok(())
}
