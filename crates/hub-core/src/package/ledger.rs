//! The slips: what was sent to an AI, kept so you can see it and count it.
//!
//! Recording must never hold anything up or fail a send. [`record`] only puts the slip on a
//! channel; a task started with [`writer`] writes them to the store, a few at a time. Without
//! that task (tests, a tool that has no hub) the slip is dropped. Slips are kept for
//! [`KEEP_MS`] and each machine keeps its own.

use crate::locked::ResultExt;
use crate::locked::Ignore;
use std::sync::Mutex;

use anyhow::Result;
use rusqlite::{Row, params, params_from_iter, types::Value};
use tokio::sync::mpsc;

use crate::model::{PackageDay, PackageSummary, PackageTotal, Slip, SlipSection, now_ms};
use crate::store::Store;

/// How long a slip is kept.
pub const KEEP_MS: i64 = 90 * 86_400_000;

const DAY_MS: i64 = 86_400_000;

static SINK: Mutex<Option<mpsc::UnboundedSender<Slip>>> = Mutex::new(None);

/// Starts a task that writes the slips it is sent to `store`. None outside an async runtime.
pub(crate) fn writer(store: Store) -> Option<mpsc::UnboundedSender<Slip>> {
    let runtime = tokio::runtime::Handle::try_current().ok()?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Slip>();
    runtime.spawn(async move {
        if let Err(e) = prune(&store).await {
            tracing::warn!("slips: could not clear old ones: {e:#}");
        }
        let mut written = 0u32;
        while let Some(slip) = rx.recv().await {
            if let Err(e) = insert(&store, slip).await {
                tracing::warn!("slips: could not write one: {e:#}");
            }
            written += 1;
            if written.is_multiple_of(500) {
                prune(&store).await.warn_on_err("prune the ledger");
            }
        }
    });
    Some(tx)
}

/// Lets code that has no hub in reach (the one-shot questions, `engine::complete`) write slips
/// too. The latest call wins, which is only ever the one hub of a running core; a message to a
/// thread uses its own hub's writer, so tests that run several hubs at once keep to their own.
pub(crate) fn share(tx: mpsc::UnboundedSender<Slip>) {
    *SINK.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
}

/// Hands a slip to the writer. Never blocks and never fails.
pub fn record(slip: Slip) {
    if let Some(tx) = SINK.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        tx.send(slip).ignore("the writer ends with the runtime of its hub (a test's may be gone)");
    }
}

pub async fn insert(store: &Store, r: Slip) -> Result<()> {
    store
        .with(move |c| {
            c.execute(
                "INSERT OR IGNORE INTO slips
                 (id, at, purpose, thread_id, seq, engine, model, tokens, estimated, attachments, recipe, fallback, head, sections, session)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    r.id,
                    r.at,
                    r.purpose,
                    r.thread,
                    r.seq,
                    r.engine,
                    r.model,
                    r.tokens,
                    r.estimated,
                    r.attachments,
                    r.recipe,
                    r.fallback,
                    r.head,
                    serde_json::to_string(&r.sections)?,
                    r.session,
                ],
            )?;
            Ok(())
        })
        .await
}

/// Removes what is older than [`KEEP_MS`].
pub async fn prune(store: &Store) -> Result<()> {
    let before = now_ms() - KEEP_MS;
    store
        .with(move |c| {
            c.execute("DELETE FROM slips WHERE at < ?1", [before])?;
            Ok(())
        })
        .await
}

/// Which slips to list.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub thread: Option<String>,
    pub purpose: Option<String>,
    /// Only those that went into this engine session.
    pub session: Option<String>,
    /// Only those before this moment, to page backwards.
    pub before: Option<i64>,
    pub limit: Option<usize>,
}

fn read(row: &Row) -> rusqlite::Result<Slip> {
    let sections: String = row.get(13)?;
    Ok(Slip {
        id: row.get(0)?,
        at: row.get(1)?,
        purpose: row.get(2)?,
        thread: row.get(3)?,
        seq: row.get(4)?,
        engine: row.get(5)?,
        model: row.get(6)?,
        tokens: row.get(7)?,
        estimated: row.get(8)?,
        attachments: row.get(9)?,
        recipe: row.get(10)?,
        fallback: row.get(11)?,
        head: row.get(12)?,
        sections: serde_json::from_str::<Vec<SlipSection>>(&sections).unwrap_or_default(),
        session: row.get(14)?,
    })
}

/// The latest slips, newest first.
pub async fn list(store: &Store, filter: Filter) -> Result<Vec<Slip>> {
    let limit = filter.limit.unwrap_or(100).clamp(1, 500);
    store
        .with(move |c| {
            let mut sql = String::from(
                "SELECT id, at, purpose, thread_id, seq, engine, model, tokens, estimated, attachments, recipe, fallback, head, sections, session
                 FROM slips WHERE 1 = 1",
            );
            let mut args: Vec<Value> = Vec::new();
            if let Some(t) = filter.thread {
                sql.push_str(" AND thread_id = ?");
                args.push(t.into());
            }
            if let Some(p) = filter.purpose {
                sql.push_str(" AND purpose = ?");
                args.push(p.into());
            }
            if let Some(s) = filter.session {
                sql.push_str(" AND session = ?");
                args.push(s.into());
            }
            if let Some(b) = filter.before {
                sql.push_str(" AND at < ?");
                args.push(b.into());
            }
            sql.push_str(" ORDER BY at DESC, id DESC LIMIT ?");
            args.push((limit as i64).into());
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(params_from_iter(args), read)?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
}

/// The engine sessions of one-shots (slips that went to no thread) since `from`: Hub's helper
/// calls, which Where it goes (`spend.rs`) counts as Hub's whatever folder they ran in.
pub async fn helper_sessions(store: &Store, from: i64) -> Result<Vec<String>> {
    store
        .with(move |c| {
            let mut stmt = c.prepare(
                "SELECT DISTINCT session FROM slips WHERE thread_id IS NULL AND session IS NOT NULL AND session != '' AND at >= ?1",
            )?;
            let rows = stmt.query_map(params![from], |r| r.get::<_, String>(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
}

/// What was sent per day and purpose over the last `days` days, in a time zone `tz` minutes east
/// of UTC, newest day first and the heaviest purpose first within a day.
pub async fn summary(store: &Store, days: u32, tz: i32) -> Result<PackageSummary> {
    let shift = tz as i64 * 60_000;
    let today = (now_ms() + shift).div_euclid(DAY_MS);
    let first = today - i64::from(days.clamp(1, crate::model::Spend::MAX_DAYS)) + 1;
    let rows: Vec<(i64, String, u32, u64)> = store
        .with(move |c| {
            let mut stmt = c.prepare(
                "SELECT (at + ?1) / ?2 AS day, purpose, COUNT(*), SUM(tokens) FROM slips
                 WHERE (at + ?1) / ?2 >= ?3 GROUP BY day, purpose",
            )?;
            let rows = stmt.query_map(params![shift, DAY_MS, first], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, i64>(3)? as u64)))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await?;
    let mut out: Vec<PackageDay> = Vec::new();
    for day in (first..=today).rev() {
        let mut purposes: Vec<PackageTotal> = rows
            .iter()
            .filter(|(d, ..)| *d == day)
            .map(|(_, purpose, calls, tokens)| PackageTotal { purpose: purpose.clone(), calls: *calls, tokens: *tokens })
            .collect();
        purposes.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.purpose.cmp(&b.purpose)));
        out.push(PackageDay { day: crate::activity::date_of(day * DAY_MS, 0), purposes });
    }
    Ok(PackageSummary { days: out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::Sent;

    fn slip(purpose: &str, thread: Option<&str>, at: i64, text: &str) -> Slip {
        let mut r = Sent { purpose, thread, seq: thread.map(|_| 1), engine: "claude", model: None, session: None, text, parts: &[], attachments: 0, recipe: "legacy" }.slip();
        r.at = at;
        r
    }

    #[tokio::test]
    async fn a_slip_is_written_and_read_back_whole() {
        let store = Store::in_memory().unwrap();
        let mut r = slip("switch", Some("t1"), now_ms(), &"x".repeat(400));
        r.sections = vec![SlipSection { id: "handover".into(), tokens: 90, chars: 360, cut: true }];
        r.model = Some("opus".into());
        r.session = Some("sess-1".into());
        r.attachments = 2;
        insert(&store, r.clone()).await.unwrap();
        insert(&store, r.clone()).await.unwrap(); // the same id twice is one row
        let got = list(&store, Filter::default()).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, r.id);
        assert_eq!(got[0].tokens, 100);
        assert_eq!(got[0].model.as_deref(), Some("opus"));
        assert_eq!(got[0].sections.len(), 1);
        assert!(got[0].sections[0].cut);
        assert_eq!(got[0].attachments, 2);
        assert_eq!(got[0].thread.as_deref(), Some("t1"));
    }

    #[tokio::test]
    async fn helper_sessions_are_the_one_shots_with_a_session() {
        let store = Store::in_memory().unwrap();
        let now = now_ms();
        for (purpose, thread, session) in [("gist", None, Some("g-1")), ("message", Some("a"), Some("t-1")), ("title", None, None), ("notes", None, Some("g-1"))] {
            let mut r = slip(purpose, thread, now, "hello");
            r.session = session.map(str::to_owned);
            insert(&store, r).await.unwrap();
        }
        let old = { let mut r = slip("tidy", None, now - 10 * DAY_MS, "hi"); r.session = Some("g-old".into()); r };
        insert(&store, old).await.unwrap();
        assert_eq!(helper_sessions(&store, now - DAY_MS).await.unwrap(), ["g-1"]);
    }

    #[tokio::test]
    async fn the_list_is_newest_first_and_filters() {
        let store = Store::in_memory().unwrap();
        let now = now_ms();
        for (i, (purpose, thread)) in [("message", Some("a")), ("gist", None), ("message", Some("b")), ("switch", Some("a"))].into_iter().enumerate() {
            insert(&store, slip(purpose, thread, now - 1000 + i as i64, "hello")).await.unwrap();
        }
        let all = list(&store, Filter::default()).await.unwrap();
        assert_eq!(all.iter().map(|r| r.purpose.as_str()).collect::<Vec<_>>(), ["switch", "message", "gist", "message"]);
        let a = list(&store, Filter { thread: Some("a".into()), ..Default::default() }).await.unwrap();
        assert_eq!(a.len(), 2);
        let gists = list(&store, Filter { purpose: Some("gist".into()), ..Default::default() }).await.unwrap();
        assert_eq!(gists.len(), 1);
        assert!(!gists[0].head.is_empty(), "a one-shot keeps its first words");
        let before = list(&store, Filter { before: Some(now - 998), limit: Some(1), ..Default::default() }).await.unwrap();
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].purpose, "gist");
    }

    #[tokio::test]
    async fn slips_are_found_by_the_engine_session_they_went_into() {
        let store = Store::in_memory().unwrap();
        let mut a = slip("gist", None, now_ms(), "a");
        a.session = Some("s-a".into());
        insert(&store, a).await.unwrap();
        insert(&store, slip("tidy", None, now_ms(), "b")).await.unwrap();
        let got = list(&store, Filter { session: Some("s-a".into()), ..Default::default() }).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].purpose, "gist");
        assert!(list(&store, Filter { session: Some("nope".into()), ..Default::default() }).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn old_slips_go_and_new_ones_stay() {
        let store = Store::in_memory().unwrap();
        insert(&store, slip("old", None, now_ms() - KEEP_MS - 1000, "x")).await.unwrap();
        insert(&store, slip("new", None, now_ms(), "x")).await.unwrap();
        prune(&store).await.unwrap();
        let left = list(&store, Filter::default()).await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].purpose, "new");
    }

    #[tokio::test]
    async fn the_summary_counts_per_day_and_purpose_heaviest_first() {
        let store = Store::in_memory().unwrap();
        let now = now_ms();
        insert(&store, slip("gist", None, now, &"x".repeat(40))).await.unwrap();
        insert(&store, slip("gist", None, now, &"x".repeat(40))).await.unwrap();
        insert(&store, slip("switch", Some("t"), now, &"x".repeat(4000))).await.unwrap();
        insert(&store, slip("gist", None, now - 2 * DAY_MS, &"x".repeat(8))).await.unwrap();
        let s = summary(&store, 7, 0).await.unwrap();
        assert_eq!(s.days.len(), 7);
        let today = &s.days[0];
        assert_eq!(today.purposes[0].purpose, "switch");
        assert_eq!(today.purposes[0].tokens, 1000);
        assert_eq!((today.purposes[1].purpose.as_str(), today.purposes[1].calls, today.purposes[1].tokens), ("gist", 2, 20));
        assert!(s.days[1].purposes.is_empty());
        assert_eq!(s.days[2].purposes[0].calls, 1);
        // A shorter window leaves the older day out.
        assert_eq!(summary(&store, 1, 0).await.unwrap().days.len(), 1);
    }

    #[tokio::test]
    async fn the_writer_puts_what_it_is_sent_in_the_store() {
        let store = Store::in_memory().unwrap();
        let tx = writer(store.clone()).expect("inside a runtime");
        let r = slip("through-the-writer", None, now_ms(), "hello");
        let id = r.id.clone();
        tx.send(r).unwrap();
        let mut found = false;
        for _ in 0..200 {
            if list(&store, Filter::default()).await.unwrap().iter().any(|r| r.id == id) {
                found = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(found, "the slip reached the store");
    }

    #[test]
    fn recording_outside_a_runtime_is_harmless() {
        // No writer can start here, and recording still returns at once.
        assert!(writer(Store::in_memory().unwrap()).is_none());
        record(slip("stray", None, now_ms(), "x"));
    }
}
