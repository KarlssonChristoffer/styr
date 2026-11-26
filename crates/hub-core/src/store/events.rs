//! A thread's event log in the store: appending (with a per-thread `seq`, once-only when asked)
//! and the narrow reads of it (by kind, after a seq or the last marker, the last turn, the newest
//! events, reply chunks), so a caller reads only the part of the log it needs.

use super::*;

impl Store {
    pub async fn append_event(
        &self,
        thread_id: &str,
        kind: EventKind,
        data: serde_json::Value,
    ) -> Result<Event> {
        let thread_id = thread_id.to_owned();
        let blobs = self.blobs.clone();
        self.with(move |c| {
            let kept = if kind == EventKind::Update { kept_data(blobs.as_deref(), &data) } else { None };
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut event = insert_event(&tx, thread_id, kind, kept.unwrap_or_else(|| data.clone()))?;
            tx.commit()?;
            // Whoever is told about the event gets the picture in it, as before.
            event.data = data;
            Ok(event)
        })
        .await
    }

    /// Appends the event unless a request with `client_id` was already carried out, and notes
    /// the request in the same transaction, so a retry after a crash cannot log it twice.
    /// `None` when it was there already.
    pub async fn append_event_once(
        &self,
        thread_id: &str,
        kind: EventKind,
        data: serde_json::Value,
        client_id: &str,
    ) -> Result<Option<Event>> {
        let (thread_id, client_id) = (thread_id.to_owned(), client_id.to_owned());
        let blobs = self.blobs.clone();
        self.with(move |c| {
            let kept = if kind == EventKind::Update { kept_data(blobs.as_deref(), &data) } else { None };
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let done: Option<String> =
                tx.query_row("SELECT result FROM requests WHERE client_id = ?1", [&client_id], |r| r.get(0)).optional()?;
            if done.is_some() {
                return Ok(None);
            }
            let mut event = insert_event(&tx, thread_id, kind, kept.unwrap_or_else(|| data.clone()))?;
            tx.execute(
                "INSERT INTO requests (client_id, result, created_at) VALUES (?1, ?2, ?3)",
                params![client_id, event.thread_id, event.ts],
            )?;
            tx.commit()?;
            event.data = data;
            Ok(Some(event))
        })
        .await
    }

    /// Where the thread's turns stand: the message whose turn started and never ended, if one
    /// did, and the user messages after the last turn that started (or ended, before any
    /// started). Turns run one at a time in the order they were sent, so those messages have not
    /// reached the engine. Messages from before Hub logged turn starts are left alone.
    pub async fn last_turn(&self, thread_id: &str) -> Result<LastTurn> {
        let thread_id = thread_id.to_owned();
        self.with(move |c| {
            let start: Option<(i64, String)> = c
                .query_row(
                    "SELECT seq, data FROM events WHERE thread_id = ?1 AND kind = 'turn_start' ORDER BY seq DESC LIMIT 1",
                    [&thread_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let (cut_off, after) = match start {
                Some((start_seq, data)) => {
                    let data: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
                    let prompt = data["prompt"].as_i64().unwrap_or(0);
                    let ended: bool = c.query_row(
                        "SELECT EXISTS (SELECT 1 FROM events WHERE thread_id = ?1 AND seq > ?2 AND (kind IN ('turn_end', 'error')
                           OR (kind = 'restart' AND json_extract(data, '$.state') = 'stopped' AND json_extract(data, '$.prompt') = ?3)))",
                        params![thread_id, start_seq, prompt],
                        |r| r.get(0),
                    )?;
                    ((!ended).then_some(prompt), prompt)
                }
                None => {
                    let end: Option<i64> = c.query_row(
                        "SELECT MAX(seq) FROM events WHERE thread_id = ?1 AND kind IN ('turn_end', 'error')",
                        [&thread_id],
                        |r| r.get(0),
                    )?;
                    (None, end.unwrap_or(0))
                }
            };
            let since: i64 = c
                .query_row("SELECT value FROM kv WHERE key = ?1", [TURNS_SINCE], |r| r.get::<_, String>(0))
                .optional()?
                .and_then(|v| v.parse().ok())
                .unwrap_or(i64::MAX);
            let mut stmt = c.prepare(
                "SELECT thread_id, seq, ts, kind, data FROM events
                 WHERE thread_id = ?1 AND seq > ?2 AND ts >= ?3 AND kind = 'user_message'
                   AND NOT EXISTS (SELECT 1 FROM events q WHERE q.thread_id = events.thread_id AND q.kind = 'queue'
                         AND json_extract(q.data, '$.seq') = events.seq AND json_extract(q.data, '$.action') IN ('removed', 'dropped'))
                 ORDER BY seq",
            )?;
            let queued = stmt.query_map(params![thread_id, after, since], row_event)?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(LastTurn { cut_off, queued })
        })
        .await
    }

    pub async fn events(&self, thread_id: &str, after: i64, limit: i64) -> Result<Vec<Event>> {
        let thread_id = thread_id.to_owned();
        self.with_events(move |c| {
            let mut stmt = c.prepare(
                "SELECT thread_id, seq, ts, kind, data FROM events
                 WHERE thread_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![thread_id, after, limit], row_event)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// [`events`](Self::events) of only these kinds. A log is mostly `update` payloads (one thread
    /// here holds 44 MB of them), so a reader that wants a setting, a checkpoint or a user message
    /// asks for those instead of parsing the whole log into memory each time.
    pub async fn events_of(&self, thread_id: &str, after: i64, kinds: &[EventKind]) -> Result<Vec<Event>> {
        let thread_id = thread_id.to_owned();
        let list = kinds.iter().map(|k| format!("'{}'", kind_str(*k))).collect::<Vec<_>>().join(",");
        self.with_events(move |c| {
            let mut stmt = c.prepare(&format!(
                "SELECT thread_id, seq, ts, kind, data FROM events
                 WHERE thread_id = ?1 AND seq > ?2 AND kind IN ({list}) ORDER BY seq"
            ))?;
            let rows = stmt.query_map(params![thread_id, after], row_event)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// [`events_of`](Self::events_of) these kinds, and of the `update`s only those whose
    /// `sessionUpdate` is in `updates` (`agent_message_chunk`, `tool_call`), oldest first. The
    /// conversation of a log without the tool output and other updates that are most of its size.
    pub async fn events_with_updates(&self, thread_id: &str, after: i64, kinds: &[EventKind], updates: &[&str]) -> Result<Vec<Event>> {
        let thread_id = thread_id.to_owned();
        let list = kinds.iter().map(|k| format!("'{}'", kind_str(*k))).collect::<Vec<_>>().join(",");
        let updates = serde_json::to_string(updates)?;
        self.with_events(move |c| {
            let mut stmt = c.prepare(&format!(
                "SELECT thread_id, seq, ts, kind, data FROM events
                 WHERE thread_id = ?1 AND seq > ?2 AND (kind IN ({list}) OR (kind = 'update'
                   AND json_extract(data, '$.sessionUpdate') IN (SELECT value FROM json_each(?3)))) ORDER BY seq"
            ))?;
            let rows = stmt.query_map(params![thread_id, after, updates], row_event)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Every event after `after` except those of the kinds in `skip`, oldest first. The tail of a
    /// log without its `update` payloads, for a reader that only looks for markers.
    pub async fn events_except(&self, thread_id: &str, after: i64, skip: &[EventKind]) -> Result<Vec<Event>> {
        let thread_id = thread_id.to_owned();
        let list = skip.iter().map(|k| format!("'{}'", kind_str(*k))).collect::<Vec<_>>().join(",");
        self.with_events(move |c| {
            let mut stmt = c.prepare(&format!(
                "SELECT thread_id, seq, ts, kind, data FROM events
                 WHERE thread_id = ?1 AND seq > ?2 AND kind NOT IN ({list}) ORDER BY seq"
            ))?;
            let rows = stmt.query_map(params![thread_id, after], row_event)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// The events after the last one of `kind`, without those of the kinds in `skip`; the whole
    /// log when there is none. What happened since the last turn end or user message, without
    /// reading what came before.
    pub async fn events_after_last(&self, thread_id: &str, kind: EventKind, skip: &[EventKind]) -> Result<Vec<Event>> {
        let after = self.last_seq(thread_id, Some(kind)).await?.unwrap_or(0);
        self.events_except(thread_id, after, skip).await
    }

    /// The highest `seq` of an event of this kind in the thread, or of any kind when `kind` is None.
    pub async fn last_seq(&self, thread_id: &str, kind: Option<EventKind>) -> Result<Option<i64>> {
        let thread_id = thread_id.to_owned();
        let kind = kind.map(|k| kind_str(k).to_owned());
        self.with(move |c| {
            Ok(c.query_row(
                "SELECT MAX(seq) FROM events WHERE thread_id = ?1 AND (?2 IS NULL OR kind = ?2)",
                params![thread_id, kind],
                |r| r.get(0),
            )?)
        })
        .await
    }

    /// The last `n` events of the thread that are not of the kinds in `skip`, oldest first.
    pub async fn last_events(&self, thread_id: &str, n: usize, skip: &[EventKind]) -> Result<Vec<Event>> {
        let thread_id = thread_id.to_owned();
        let list = skip.iter().map(|k| format!("'{}'", kind_str(*k))).collect::<Vec<_>>().join(",");
        self.with_events(move |c| {
            let mut stmt = c.prepare(&format!(
                "SELECT thread_id, seq, ts, kind, data FROM events
                 WHERE thread_id = ?1 AND kind NOT IN ({list}) ORDER BY seq DESC LIMIT ?2"
            ))?;
            let mut rows = stmt.query_map(params![thread_id, n as i64], row_event)?.collect::<rusqlite::Result<Vec<_>>>()?;
            rows.reverse();
            Ok(rows)
        })
        .await
    }

    /// The agent's reply text chunks with `after < seq < before`, oldest first: the part of a turn
    /// `turn_text` reads, without the tool calls and other updates in between.
    pub async fn reply_chunks(&self, thread_id: &str, after: i64, before: i64) -> Result<Vec<Event>> {
        let thread_id = thread_id.to_owned();
        self.with_events(move |c| {
            let mut stmt = c.prepare(
                "SELECT thread_id, seq, ts, kind, data FROM events
                 WHERE thread_id = ?1 AND seq > ?2 AND seq < ?3 AND kind = 'update'
                   AND json_extract(data, '$.sessionUpdate') = 'agent_message_chunk' ORDER BY seq",
            )?;
            let rows = stmt.query_map(params![thread_id, after, before], row_event)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// The newest event of this kind in the thread, if any. One row, however long the log is.
    pub async fn last_event(&self, thread_id: &str, kind: EventKind) -> Result<Option<Event>> {
        let thread_id = thread_id.to_owned();
        let kind = kind_str(kind);
        Ok(self
            .with_events(move |c| {
                let mut stmt = c.prepare(
                    "SELECT thread_id, seq, ts, kind, data FROM events
                     WHERE thread_id = ?1 AND kind = ?2 ORDER BY seq DESC LIMIT 1",
                )?;
                let rows = stmt.query_map(params![thread_id, kind], row_event)?;
                Ok(rows.collect::<rusqlite::Result<_>>()?)
            })
            .await?
            .pop())
    }

    /// The newest tool call the agent started, if any (one row).
    pub async fn last_tool_call(&self, thread_id: &str) -> Result<Option<Event>> {
        let thread_id = thread_id.to_owned();
        Ok(self
            .with_events(move |c| {
                let mut stmt = c.prepare(
                    "SELECT thread_id, seq, ts, kind, data FROM events
                     WHERE thread_id = ?1 AND kind = 'update' AND json_extract(data, '$.sessionUpdate') = 'tool_call'
                     ORDER BY seq DESC LIMIT 1",
                )?;
                let rows = stmt.query_map(params![thread_id], row_event)?;
                Ok(rows.collect::<rusqlite::Result<_>>()?)
            })
            .await?
            .pop())
    }

    /// The reply chunks of the last turn that ended (or of the running turn when none has), the
    /// same part of the log `hub_tools::last_reply` reads from a whole log, without reading the rest.
    pub async fn last_reply_chunks(&self, thread_id: &str) -> Result<Vec<Event>> {
        let end = self.last_seq(thread_id, Some(EventKind::TurnEnd)).await?;
        let thread_id = thread_id.to_owned();
        let last = end.unwrap_or(i64::MAX);
        let after: i64 = {
            let thread_id = thread_id.clone();
            self.with(move |c| {
                Ok(c.query_row(
                    "SELECT COALESCE(MAX(seq), 0) FROM events WHERE thread_id = ?1 AND kind IN ('turn_end', 'error') AND seq < ?2",
                    params![thread_id, last],
                    |r| r.get(0),
                )?)
            })
            .await?
        };
        self.reply_chunks(&thread_id, after, last.saturating_add(1)).await
    }

    /// Events of these kinds in any thread since `since`, oldest first.
    pub async fn events_since(&self, since: i64, kinds: &[EventKind]) -> Result<Vec<Event>> {
        let kinds: Vec<&'static str> = kinds.iter().map(|k| kind_str(*k)).collect();
        self.with_events(move |c| {
            let list = kinds.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(",");
            let mut stmt = c.prepare(&format!(
                "SELECT thread_id, seq, ts, kind, data FROM events WHERE ts >= ?1 AND kind IN ({list}) ORDER BY ts"
            ))?;
            let rows = stmt.query_map(params![since], row_event)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }
}
