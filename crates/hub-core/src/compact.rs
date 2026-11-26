//! Old streamed replies, folded to one row each. An agent's answer arrives as many
//! `agent_message_chunk` (and `agent_thought_chunk`) updates, one log row per chunk, and every row
//! stays for ever. Once a turn is over and the chunks are a couple of days old nobody reads them
//! one by one, so a run of text chunks that follow each other becomes one row: the first chunk's
//! `seq` and `ts`, all the text in order. Nothing is lost but the `ts` of the later chunks (same words, same order, same place in
//! the log); only the seams and the later chunks' timestamps go (nothing reads them). The rows that vanish leave gaps in `seq`, which no
//! reader counts on in old history (live streaming still checks `last + 1`, but only for new rows).
//!
//! A run is only joined when every chunk has the same shape apart from its text (same kind, same
//! extra fields), is plain text (no picture, no blob), has an ended turn (`turn_end` or `error`)
//! after it, and the whole run is older than [`MIN_AGE_MS`]. A copy of the database is taken and
//! checked first (the copy is kept under its final name only once it checks out), the job is one transaction per thread, and running it again finds nothing.
//! <!-- covers: crates/hub-core/src/compact.rs -->

use std::path::Path;

use anyhow::{bail, Result};
use rusqlite::{params, Connection, TransactionBehavior};
use serde_json::Value;

use crate::model::now_ms;
use crate::store::Store;

/// A run must be older than this (its last chunk) to be folded.
pub const MIN_AGE_MS: i64 = 2 * 24 * 3600 * 1000;
/// The job looks at the log at most this often.
const EVERY_MS: i64 = 24 * 3600 * 1000;
const LAST_RUN: &str = "chunks.compacted";

/// One run of chunks to fold: the row that stays, its new data, and the rows that go.
#[derive(Debug, PartialEq)]
pub struct Run {
    pub keep: i64,
    pub data: Value,
    pub drop: Vec<i64>,
}

struct Open {
    key: Value,
    keep: i64,
    last_ts: i64,
    first: Value,
    text: String,
    drop: Vec<i64>,
}

/// The shape of a text chunk without its text, and the text; `None` for anything else.
fn split(data: &Value) -> Option<(Value, String)> {
    let kind = crate::model::update_kind(data)?;
    if !matches!(kind, "agent_message_chunk" | "agent_thought_chunk") {
        return None;
    }
    let content = data["content"].as_object()?;
    let text = content.get("text")?.as_str()?.to_owned();
    if content.get("type").is_some_and(|t| t != "text") || content.contains_key("blob") || content.contains_key("data") {
        return None;
    }
    let mut key = data.clone();
    key["content"]["text"] = Value::Null;
    Some((key, text))
}

/// The runs of one thread's rows (`seq`, `ts`, `kind`, `data`, in `seq` order) that may be folded.
/// Pure: the tests use it on rows of the shape a real database has.
pub fn runs(rows: impl IntoIterator<Item = (i64, i64, String, String)>, cutoff: i64) -> Vec<Run> {
    let mut out = Vec::new();
    let mut open: Option<Open> = None;
    // Runs wait here until a turn end shows they are finished.
    let mut pending: Vec<Run> = Vec::new();
    let close = |open: &mut Option<Open>, pending: &mut Vec<Run>| {
        if let Some(o) = open.take()
            && !o.drop.is_empty() && o.last_ts < cutoff {
                let mut data = o.first;
                data["content"]["text"] = Value::String(o.text);
                pending.push(Run { keep: o.keep, data, drop: o.drop });
            }
    };
    for (seq, ts, kind, text) in rows {
        let chunk = (kind == "update" && text.contains("_chunk\"")).then(|| serde_json::from_str::<Value>(&text).ok()).flatten().and_then(|d| split(&d).map(|(k, t)| (k, t, d)));
        match chunk {
            Some((key, t, d)) => match open.as_mut() {
                Some(o) if o.key == key => {
                    o.text.push_str(&t);
                    o.last_ts = ts;
                    o.drop.push(seq);
                }
                _ => {
                    close(&mut open, &mut pending);
                    open = Some(Open { key, keep: seq, last_ts: ts, first: d, text: t, drop: Vec::new() });
                }
            },
            None => {
                close(&mut open, &mut pending);
                if kind == "turn_end" || kind == "error" {
                    out.append(&mut pending);
                }
            }
        }
    }
    // Whatever is still open or pending has no ended turn after it: a reply in progress.
    out
}

fn thread_runs(c: &Connection, thread: &str, cutoff: i64) -> Result<Vec<Run>> {
    let mut stmt = c.prepare("SELECT seq, ts, kind, CASE WHEN kind = 'update' AND instr(data, '_chunk\"') > 0 THEN data ELSE '' END FROM events WHERE thread_id = ?1 ORDER BY seq")?;
    let rows = stmt.query_map([thread], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(runs(rows, cutoff))
}

impl Store {
    /// Folds old chunk runs as the module says. Takes a checked copy of the database at
    /// `backup_to` before it changes anything, and only when there is something to fold. Returns
    /// how many rows went. `now` is a parameter for tests; `force` skips the once-a-day limit.
    pub async fn compact_old_chunks(&self, backup_to: &Path, now: i64, force: bool) -> Result<usize> {
        if !force
            && let Some(last) = self.get_value(LAST_RUN).await?.and_then(|v| v.parse::<i64>().ok())
                && now - last < EVERY_MS {
                    return Ok(0);
                }
        // A failed run is not tried again on every start: it waits for the next day.
        let done = self.fold(backup_to, now).await;
        self.set_value(LAST_RUN, &now.to_string()).await?;
        done
    }

    async fn fold(&self, backup_to: &Path, now: i64) -> Result<usize> {
        let cutoff = now - MIN_AGE_MS;
        let threads: Vec<String> = self
            .with(move |c| {
                let mut stmt = c.prepare(
                    "SELECT DISTINCT thread_id FROM events WHERE kind = 'update' AND ts < ?1 AND instr(data, '_chunk\"') > 0",
                )?;
                let rows = stmt.query_map([cutoff], |r| r.get(0))?;
                Ok(rows.collect::<rusqlite::Result<_>>()?)
            })
            .await?;
        // A first look that changes nothing, so a copy is only taken when there is work.
        let mut work = 0usize;
        for t in &threads {
            let t = t.clone();
            work += self.with(move |c| Ok(thread_runs(c, &t, cutoff)?.iter().map(|r| r.drop.len()).sum::<usize>())).await?;
        }
        if work == 0 {
            return Ok(0);
        }
        let tmp = backup_to.with_extension("db.tmp");
        self.backup(&tmp).await?;
        let expect = self.with(|c| Ok(c.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))?)).await?;
        let path = tmp.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let copy = Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            let verdict: String = copy.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            let n: i64 = copy.query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
            // Events appended between the two counts only make the live one larger.
            if verdict != "ok" || n > expect || n < expect - 1000 {
                bail!("the backup {} did not check out ({verdict}, {n} events, {expect} expected)", path.display());
            }
            Ok(())
        })
        .await??;
        std::fs::rename(&tmp, backup_to)?;
        let mut gone = 0;
        for t in threads {
            gone += self
                .with(move |c| {
                    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    let mut gone = 0;
                    for run in thread_runs(&tx, &t, cutoff)? {
                        let text = serde_json::to_string(&run.data)?;
                        tx.execute("UPDATE events SET data = ?1 WHERE thread_id = ?2 AND seq = ?3", params![text, t, run.keep])?;
                        for seq in &run.drop {
                            tx.execute("DELETE FROM events WHERE thread_id = ?1 AND seq = ?2", params![t, seq])?;
                        }
                        gone += run.drop.len();
                    }
                    tx.commit()?;
                    Ok(gone)
                })
                .await?;
        }
        // The file only shrinks after a VACUUM. Best effort: another connection may hold the lock.
        if let Err(err) = self.with(|c| Ok(c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?)).await {
            tracing::warn!("vacuum after folding chunks failed: {err:#}");
        }
        Ok(gone)
    }
}

/// What hubd calls at start.
pub async fn at_start(store: &Store, data_dir: &Path) {
    let copy = crate::backup::dir(data_dir).join("before-chunks.db");
    match store.compact_old_chunks(&copy, now_ms(), false).await {
        Ok(0) => {}
        Ok(n) => tracing::info!("folded {n} old chunk rows into their replies (copy of the database: {})", copy.display()),
        Err(err) => tracing::warn!("folding old chunks failed, nothing was changed: {err:#}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(seq: i64, ts: i64, kind: &str, data: Value) -> (i64, i64, String, String) {
        (seq, ts, kind.into(), data.to_string())
    }
    fn chunk(t: &str) -> Value {
        json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": t } })
    }

    #[test]
    fn a_finished_old_run_becomes_one_row_with_all_the_text() {
        let rows = vec![
            row(1, 10, "user_message", json!({ "text": "q" })),
            row(2, 20, "update", chunk("Hel")),
            row(3, 21, "update", chunk("lo ")),
            row(4, 22, "update", chunk("there")),
            row(5, 23, "update", json!({ "sessionUpdate": "tool_call" })),
            row(6, 24, "update", chunk("A")),
            row(7, 25, "turn_end", json!({})),
        ];
        let got = runs(rows, 1000);
        assert_eq!(got, vec![Run { keep: 2, data: chunk("Hello there"), drop: vec![3, 4] }]);
    }

    #[test]
    fn a_reply_still_going_a_young_run_or_a_mixed_shape_stays() {
        // No turn end after it.
        assert!(runs(vec![row(1, 1, "update", chunk("a")), row(2, 2, "update", chunk("b"))], 1000).is_empty());
        // Too young.
        assert!(runs(vec![row(1, 1, "update", chunk("a")), row(2, 5000, "update", chunk("b")), row(3, 5001, "turn_end", json!({}))], 1000).is_empty());
        // Thoughts and messages are different series; a picture or an extra field breaks the run.
        let thought = json!({ "sessionUpdate": "agent_thought_chunk", "content": { "type": "text", "text": "hm" } });
        let pic = json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "image", "data": "x" } });
        let tagged = json!({ "sessionUpdate": "agent_message_chunk", "messageId": "m2", "content": { "type": "text", "text": "c" } });
        let rows = vec![
            row(1, 1, "update", chunk("a")),
            row(2, 2, "update", thought),
            row(3, 3, "update", pic),
            row(4, 4, "update", chunk("b")),
            row(5, 5, "update", tagged),
            row(6, 6, "turn_end", json!({})),
        ];
        assert!(runs(rows, 1000).is_empty());
    }

    #[test]
    fn boundaries_error_ender_alternation_and_untyped_content() {
        let thought = |t: &str| json!({ "sessionUpdate": "agent_thought_chunk", "content": { "text": t } });
        let plain = |t: &str| json!({ "sessionUpdate": "agent_message_chunk", "content": { "text": t } });
        // First chunk old, last young: stays.
        assert!(runs(vec![row(1, 1, "update", plain("a")), row(2, 5000, "update", plain("b")), row(3, 5001, "error", json!({}))], 1000).is_empty());
        // Alternation makes separate runs; error ends the turn; a user message breaks a run.
        let rows = vec![
            row(1, 1, "update", plain("a")),
            row(2, 2, "update", plain("b")),
            row(3, 3, "update", thought("x")),
            row(4, 4, "update", thought("y")),
            row(5, 5, "update", plain("c")),
            row(6, 6, "user_message", json!({ "text": "steer" })),
            row(7, 7, "update", plain("d")),
            row(8, 8, "update", plain("e")),
            row(9, 9, "error", json!({})),
        ];
        let got = runs(rows, 1000);
        assert_eq!(got.iter().map(|r| (r.keep, r.drop.clone())).collect::<Vec<_>>(), vec![(1, vec![2]), (3, vec![4]), (7, vec![8])]);
        assert_eq!(got[2].data["content"]["text"], "de");
    }

    /// A database the way a real one looks (WITHOUT ROWID events, several threads, other kinds
    /// between), compacted twice.
    #[tokio::test]
    async fn compacts_a_copy_of_a_real_shaped_database_without_loss_and_only_once() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let store = Store::open(&dir.path().join("hub.db"))?;
        for id in ["a", "b"] {
            store
                .insert_thread(crate::model::Thread {
                    id: id.into(),
                    title: "t".into(),
                    cwd: "/tmp".into(),
                    engine: "claude".into(),
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
                    created_at: 1,
                    updated_at: 1,
                })
                .await?;
        }
        let day = 24 * 3600 * 1000;
        let now = 100 * day;
        let old = now - 5 * day;
        let words = ["The ", "answer ", "is ", "forty", "-two. ", "Åäö ", "😀"];
        store
            .with(move |c| {
                let ins = |t: &str, seq: i64, ts: i64, kind: &str, data: Value| {
                    c.execute("INSERT INTO events (thread_id, seq, ts, kind, data) VALUES (?1, ?2, ?3, ?4, ?5)", params![t, seq, ts, kind, data.to_string()]).unwrap();
                };
                ins("a", 1, old, "user_message", json!({ "text": "q" }));
                for (i, w) in words.iter().enumerate() {
                    ins("a", 2 + i as i64, old + 1 + i as i64, "update", chunk(w));
                }
                ins("a", 9, old + 20, "turn_end", json!({}));
                // A young finished run and an unfinished one in thread b.
                ins("b", 1, now - 1000, "update", chunk("new "));
                ins("b", 2, now - 999, "update", chunk("run"));
                ins("b", 3, now - 998, "turn_end", json!({}));
                ins("b", 4, old, "update", chunk("x"));
                ins("b", 5, old, "update", chunk("y"));
                Ok(())
            })
            .await?;
        let before = store.events("a", 0, 100).await?;
        let copy = dir.path().join("backups").join("before-chunks.db");
        assert_eq!(store.compact_old_chunks(&copy, now, false).await?, words.len() - 1);
        assert!(copy.is_file(), "the copy was made first");
        let after = store.events("a", 0, 100).await?;
        assert_eq!(after.len(), 3);
        assert_eq!((after[1].seq, after[1].ts), (2, old + 1), "first chunk's place and time");
        assert_eq!(after[1].data["content"]["text"], words.concat().as_str());
        assert_eq!(after[0].data, before[0].data);
        assert_eq!(after[2].seq, 9);
        assert_eq!(store.events("b", 0, 100).await?.len(), 5, "young and unfinished runs stay");
        // The copy still holds every chunk.
        let kept: i64 = Connection::open(&copy)?.query_row("SELECT COUNT(*) FROM events WHERE thread_id = 'a'", [], |r| r.get(0))?;
        assert_eq!(kept, 9);
        assert_eq!(store.compact_old_chunks(&copy, now, true).await?, 0, "run again: nothing to do");
        assert_eq!(store.events("a", 0, 100).await?.len(), 3);
        // And the once-a-day limit.
        assert_eq!(store.compact_old_chunks(&copy, now + 1, false).await?, 0);
        Ok(())
    }
}
