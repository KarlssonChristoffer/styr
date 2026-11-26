//! What happens at Customs, kept so its page can show it: what came through (declared by an
//! agent, found after a turn, asked for by a package manager), what the review or the user
//! decided, and what the daily check and the monthly round did. The list in `customs.rs` says
//! how things stand; this says how they got there. Append-only, kept for half a year, and each
//! machine keeps its own.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use anyhow::Result;
use rusqlite::params;

use crate::store::Store;
use crate::model::{CustomsActivity, CustomsLogEntry, CustomsReviewing, CustomsStatus, now_ms};

/// How long the log is kept.
const KEEP_MS: i64 = 180 * 86_400_000;

/// One line to write. Only what is set is kept.
#[derive(Debug, Default, Clone)]
pub struct Note {
    pub what: &'static str,
    pub kind: String,
    pub name: String,
    pub by: &'static str,
    pub detail: String,
    pub thread_id: Option<String>,
    pub took_ms: Option<i64>,
}

impl Note {
    pub fn new(what: &'static str, by: &'static str) -> Self {
        Self { what, by, ..Self::default() }
    }

    pub fn item(mut self, kind: &str, name: &str) -> Self {
        self.kind = kind.to_owned();
        self.name = name.to_owned();
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn thread(mut self, id: Option<&str>) -> Self {
        self.thread_id = id.map(str::to_owned);
        self
    }

    pub fn took(mut self, ms: i64) -> Self {
        self.took_ms = Some(ms);
        self
    }
}

/// The log word for a status.
pub fn word(status: CustomsStatus) -> &'static str {
    match status {
        CustomsStatus::Cleared => "cleared",
        CustomsStatus::Held => "held",
        CustomsStatus::Refused => "refused",
    }
}

/// Writes one line. A log that cannot be written never stops Customs itself, so this only warns.
pub async fn note(store: &Store, n: Note) {
    let ts = now_ms();
    let done = store
        .with(move |c| {
            c.execute(
                "INSERT INTO customs_log (ts, what, kind, name, by, detail, thread_id, took_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![ts, n.what, n.kind, n.name, n.by, n.detail, n.thread_id, n.took_ms],
            )?;
            Ok(())
        })
        .await;
    if let Err(e) = done {
        tracing::warn!("customs log: {e:#}");
    }
}

/// Drops what is older than half a year.
pub async fn prune(store: &Store) -> Result<usize> {
    let before = now_ms() - KEEP_MS;
    store.with(move |c| Ok(c.execute("DELETE FROM customs_log WHERE ts < ?1", [before])?)).await
}

/// The log, newest first: everything since `since`, at most `limit` lines, of one item when
/// `item` is given. Each line has its thread's title and folder when it came from one.
pub async fn entries(store: &Store, since: i64, limit: usize, item: Option<(String, String)>) -> Result<Vec<CustomsLogEntry>> {
    store
        .with(move |c| {
            let (kind, name) = item.unwrap_or_default();
            let mut q = c.prepare(
                "SELECT l.id, l.ts, l.what, l.kind, l.name, l.by, l.detail, l.thread_id, t.title, t.cwd, l.took_ms
                 FROM customs_log l LEFT JOIN threads t ON t.id = l.thread_id
                 WHERE l.ts >= ?1 AND (?2 = '' OR (l.kind = ?2 AND l.name = ?3))
                 ORDER BY l.ts DESC, l.id DESC LIMIT ?4",
            )?;
            let rows = q.query_map(params![since, kind, name, limit as i64], |r| {
                Ok(CustomsLogEntry {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    what: r.get(2)?,
                    kind: r.get(3)?,
                    name: r.get(4)?,
                    by: r.get(5)?,
                    detail: r.get(6)?,
                    thread_id: r.get(7)?,
                    thread_title: r.get(8)?,
                    cwd: r.get(9)?,
                    took_ms: r.get(10)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
}

/// What is being reviewed now, by kind and name, since when.
static NOW: LazyLock<Mutex<HashMap<(String, String), i64>>> = LazyLock::new(Default::default);

/// Marks a review as running until the answer is dropped.
pub fn reviewing(kind: &str, name: &str) -> Running {
    let key = (kind.to_owned(), name.to_owned());
    NOW.lock().unwrap_or_else(|p| p.into_inner()).insert(key.clone(), now_ms());
    Running(key)
}

pub struct Running((String, String));

impl Drop for Running {
    fn drop(&mut self) {
        NOW.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.0);
    }
}

/// What the page shows: the last 30 days (at most 500 lines), or all of one item's, and the
/// reviews running now.
pub async fn activity(store: &Store, item: Option<(String, String)>) -> Result<CustomsActivity> {
    let since = if item.is_some() { 0 } else { now_ms() - 30 * 86_400_000 };
    let entries = entries(store, since, 500, item).await?;
    let mut reviewing: Vec<CustomsReviewing> = NOW
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .map(|((kind, name), since)| CustomsReviewing { kind: kind.clone(), name: name.clone(), since: *since })
        .collect();
    reviewing.sort_by_key(|r| r.since);
    Ok(CustomsActivity { entries, reviewing })
}
