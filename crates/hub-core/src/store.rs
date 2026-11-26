//! SQLite storage: threads plus an append-only event log. All calls run on the blocking pool so
//! the async runtime never waits on disk.

use crate::locked::ResultExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use rusqlite_migration::{M, Migrations};

use crate::model::{Event, EventData, EventKind, Project, Thread, ThreadId, ThreadStatus, now_ms};

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
    /// Where generated pictures live as files (`blobs.rs`); none for a database in memory.
    blobs: Option<PathBuf>,
}

mod schema;
pub use schema::schema_version;
use schema::*;

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::init(conn, crate::blobs::dir_of(path))
    }

    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(mut conn: Connection, blobs: Option<PathBuf>) -> Result<Self> {
        // Another connection (the previous core during an update, a backup) may hold the write
        // lock for a moment; wait for it instead of failing with "database is locked". Writes
        // take the lock when they begin (IMMEDIATE): a transaction that read first and then
        // tried to write after the other connection committed would fail at once, wait or not.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // The WAL file is reset after a checkpoint but keeps its size; cap what is left (tincan: 45 MB).
        conn.pragma_update(None, "journal_size_limit", 8 * 1024 * 1024)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn)?;
        conn.execute("INSERT OR IGNORE INTO kv (key, value) VALUES (?1, ?2)", params![TURNS_SINCE, now_ms().to_string()])?;
        // The engines went with the previous core, so nothing is running any more, and the
        // threads it cut off come back to the user; `restart::resume` carries their turns on.
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let cut: Vec<String> = tx
            .prepare("SELECT id FROM threads WHERE status IN ('running', 'waiting_on_you')")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for id in cut {
            insert_event(&tx, id, EventKind::Status, crate::model::StatusData::new(ThreadStatus::Idle, "core_started").to_json())?;
        }
        tx.execute(
            "UPDATE threads SET status = 'idle', returned_at = ?1, settled_at = NULL
             WHERE status IN ('running', 'waiting_on_you')",
            [now_ms()],
        )?;
        tx.commit()?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)), blobs })
    }

    pub(crate) async fn with<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            // A panic in one call must not take the whole store with it: the connection is
            // still sound (each call is its own transaction), so later calls go on.
            let mut conn = conn.lock().unwrap_or_else(|p| p.into_inner());
            f(&mut conn)
        })
        .await?
    }

    /// [`with`](Self::with) for a read that returns events: the pictures lifted out of the rows
    /// (`blobs.rs`) are put back, so a reader sees an event as the engine sent it.
    async fn with_events(&self, f: impl FnOnce(&mut Connection) -> Result<Vec<Event>> + Send + 'static) -> Result<Vec<Event>> {
        let blobs = self.blobs.clone();
        self.with(move |c| {
            let mut events = f(c)?;
            if let Some(dir) = &blobs {
                for e in &mut events {
                    if e.kind == EventKind::Update {
                        crate::blobs::inline(dir, &mut e.data);
                    }
                }
            }
            Ok(events)
        })
        .await
    }

    /// Where the picture files live, for a reader of raw rows that needs them (`blobs::inline`).
    pub(crate) fn blob_dir(&self) -> Option<PathBuf> {
        self.blobs.clone()
    }

    /// Writes a consistent copy of the whole database to `path`, while it stays in use.
    pub async fn backup(&self, path: &Path) -> Result<()> {
        let path = path.to_owned();
        self.with(move |c| {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            crate::locked::clean_file(&path);
            c.execute("VACUUM INTO ?1", [path.display().to_string()])
                .with_context(|| format!("backing up to {}", path.display()))?;
            Ok(())
        })
        .await
    }

    /// The one-time job that moves the pictures in old rows out to files (`blobs.rs`), the same
    /// content with 200 MB less in the database. It first copies the database to `backup_to` and
    /// opens the copy to check it (intact, same number of events); if that fails, nothing is
    /// touched. Each row is changed only after its file is written and read back, and the job
    /// notes in the store that it is done, so it runs once. Returns how many rows it moved.
    pub async fn lift_old_pictures(&self, backup_to: &Path) -> Result<usize> {
        const DONE: &str = "blobs.lifted";
        let Some(dir) = self.blobs.clone() else { return Ok(0) };
        if self.get_value(DONE).await?.is_some() {
            return Ok(0);
        }
        let find = "SELECT thread_id, seq FROM events WHERE kind = 'update' AND length(data) > 4096 AND data LIKE '%\"type\":\"image\"%' AND data LIKE '%\"data\":\"%'";
        let ids: Vec<(String, i64)> = self
            .with(move |c| {
                let mut stmt = c.prepare(find)?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                Ok(rows.collect::<rusqlite::Result<_>>()?)
            })
            .await?;
        if ids.is_empty() {
            self.set_value(DONE, &now_ms().to_string()).await?;
            return Ok(0);
        }
        self.backup(backup_to).await?;
        let (path, expect) = (backup_to.to_owned(), self.with(|c| Ok(c.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))?)).await?);
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
        let mut moved = 0;
        for chunk in ids.chunks(10) {
            let (chunk, dir) = (chunk.to_vec(), dir.clone());
            moved += self
                .with(move |c| {
                    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
                    let mut moved = 0;
                    for (thread, seq) in chunk {
                        let text: String = tx.query_row("SELECT data FROM events WHERE thread_id = ?1 AND seq = ?2", params![thread, seq], |r| r.get(0))?;
                        let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                        let Some(lifted) = crate::blobs::lift(&dir, &data) else { continue };
                        // Back from the file, it must be the same event as it was.
                        let mut check = lifted.clone();
                        crate::blobs::inline(&dir, &mut check);
                        let mut original = data;
                        strip_copies(&mut original, &lifted);
                        if check != original {
                            continue;
                        }
                        tx.execute("UPDATE events SET data = ?1 WHERE thread_id = ?2 AND seq = ?3", params![serde_json::to_string(&lifted)?, thread, seq])?;
                        moved += 1;
                    }
                    tx.commit()?;
                    Ok(moved)
                })
                .await?;
        }
        self.set_value(DONE, &now_ms().to_string()).await?;
        // The file only shrinks after a VACUUM; the copies of the database shrink on their own.
        // Best effort: another connection (the core handing over) may hold the lock.
        self.with(|c| Ok(c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")?)).await.warn_on_err("checkpoint and VACUUM the database");
        Ok(moved)
    }

    /// Replaces the database with a copy made by `backup`, e.g. when a newer version that
    /// already migrated it did not come up and this one takes over again.
    pub async fn restore(&self, path: &Path) -> Result<()> {
        let path = path.to_owned();
        self.with(move |c| {
            c.restore(rusqlite::MAIN_DB, &path, None::<fn(rusqlite::backup::Progress)>)
                .with_context(|| format!("restoring {}", path.display()))?;
            Ok(())
        })
        .await
    }

    /// What a request with this client id gave the first time, if it was carried out.
    pub async fn recall(&self, client_id: &str) -> Result<Option<String>> {
        let client_id = client_id.to_owned();
        self.with(move |c| {
            Ok(c.query_row("SELECT result FROM requests WHERE client_id = ?1", [client_id], |r| r.get(0)).optional()?)
        })
        .await
    }

    /// Notes that the request with this client id was carried out. Retries come within
    /// minutes, so ids older than a day are forgotten.
    pub async fn remember(&self, client_id: &str, result: &str) -> Result<()> {
        let (client_id, result) = (client_id.to_owned(), result.to_owned());
        self.with(move |c| {
            let now = now_ms();
            c.execute("DELETE FROM requests WHERE created_at < ?1", [now - 24 * 3600 * 1000])?;
            c.execute(
                "INSERT OR REPLACE INTO requests (client_id, result, created_at) VALUES (?1, ?2, ?3)",
                params![client_id, result, now],
            )?;
            Ok(())
        })
        .await
    }

    /// The schema version the database is at, which a newer build may have raised.
    pub async fn version(&self) -> Result<i64> {
        self.with(|c| Ok(c.pragma_query_value(None, "user_version", |r| r.get(0))?)).await
    }

    /// The rules saved from chat, oldest first: `(thread id, thread title, when, text)`. The
    /// Rules tab dates a rule that is not committed yet from this and names the thread.
    pub async fn rules_said(&self) -> Result<Vec<(String, String, i64, String)>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT e.thread_id, COALESCE(t.title, ''), e.ts, json_extract(e.data, '$.text')
                 FROM events e LEFT JOIN threads t ON t.id = e.thread_id
                 WHERE e.kind = 'rule_saved' ORDER BY e.ts",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get::<_, Option<String>>(3)?.unwrap_or_default())))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Your messages and the agents' finished turns between two moments, oldest first: what the
    /// activity map counts.
    pub async fn activity(&self, from: i64, to: i64) -> Result<Vec<crate::model::rows::Mark>> {
        self.with(move |c| {
            let mut stmt = c.prepare(
                "SELECT thread_id, ts, CASE kind WHEN 'user_message' THEN json_extract(data, '$.text') END
                 FROM events WHERE kind IN ('user_message', 'turn_end') AND ts >= ?1 AND ts < ?2 ORDER BY ts",
            )?;
            let rows = stmt.query_map(params![from, to], |r| {
                Ok(crate::model::rows::Mark { thread_id: r.get(0)?, ts: r.get(1)?, message: r.get(2)? })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

}

/// Prefix of the keys that note where a moved thread went.
const MOVED: &str = "moved:";

const THREAD_SELECT: &str =
    "SELECT id, title, cwd, engine, status, engine_session_id, created_at, updated_at, title_locked, archived,
     returned_at, seen_at, settled_at, sent_at, pinned_at, snoozed_until, auto_settle, parent_id, reading, owner, cloud FROM threads";

/// Brings the database up to this build's schema. A database a newer build already moved past
/// it is refused rather than misread; rolling back restores the copy taken before the update.
fn migrate(conn: &mut Connection) -> Result<()> {
    match Migrations::new(steps()).to_latest(conn) {
        Err(rusqlite_migration::Error::MigrationDefinition(
            rusqlite_migration::MigrationDefinitionError::DatabaseTooFarAhead,
        )) => {
            let found: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
            bail!(TooNew { found, known: schema_version() })
        }
        other => other.context("migrating the database"),
    }
}

/// Where a thread's turns stood, from [`Store::last_turn`].
#[derive(Debug, Clone)]
pub struct LastTurn {
    /// The user message whose turn started and neither ended, failed, nor was let go.
    pub cut_off: Option<i64>,
    /// User messages that did not reach the engine, oldest first.
    pub queued: Vec<Event>,
}

/// Key in the store for when this database started logging turn starts (ms). Messages from
/// before then are history, not work a restart cut off.
const TURNS_SINCE: &str = "turns.logged_since";

/// Takes out of `original` what `lifted` no longer has: the copies of a picture (`rawOutput.result`).
fn strip_copies(original: &mut serde_json::Value, lifted: &serde_json::Value) {
    use serde_json::Value;
    match (original, lifted) {
        (Value::Object(o), Value::Object(l)) => {
            o.retain(|k, _| l.contains_key(k) || k == "data");
            for (k, child) in o.iter_mut() {
                if let Some(other) = l.get(k) {
                    strip_copies(child, other);
                }
            }
        }
        (Value::Array(o), Value::Array(l)) if o.len() == l.len() => {
            for (child, other) in o.iter_mut().zip(l) {
                strip_copies(child, other);
            }
        }
        _ => {}
    }
}

/// What the row of an `update` event keeps, when it is not the data as it came (`blobs::lift`).
fn kept_data(blobs: Option<&std::path::Path>, data: &serde_json::Value) -> Option<serde_json::Value> {
    crate::blobs::lift(blobs?, data)
}

/// Inserts the thread's next event inside an open transaction.
fn insert_event(tx: &rusqlite::Transaction, thread_id: String, kind: EventKind, data: serde_json::Value) -> Result<Event> {
    let seq: i64 =
        tx.query_row("SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE thread_id = ?1", [&thread_id], |r| r.get(0))?;
    let event = Event { thread_id, seq, ts: now_ms(), kind, data };
    tx.execute(
        "INSERT INTO events (thread_id, seq, ts, kind, data) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![event.thread_id, event.seq, event.ts, kind_str(kind), serde_json::to_string(&event.data)?],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _) if f.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY => {
            anyhow::Error::new(ThreadGone(event.thread_id.clone()))
        }
        e => e.into(),
    })?;
    Ok(event)
}

/// An event for a thread the store does not have (any more): deleted, or replaced by a move back
/// from another machine, while its engine was still talking.
#[derive(Debug, thiserror::Error)]
#[error("thread {0} is gone: no row in threads for events.thread_id")]
pub struct ThreadGone(pub ThreadId);

/// Whether `err` is a [`ThreadGone`].
pub fn gone(err: &anyhow::Error) -> bool {
    err.downcast_ref::<ThreadGone>().is_some()
}

/// An event from a `SELECT thread_id, seq, ts, kind, data` row.
fn row_event(r: &rusqlite::Row) -> rusqlite::Result<Event> {
    let kind: String = r.get(3)?;
    let data: String = r.get(4)?;
    Ok(Event {
        thread_id: r.get(0)?,
        seq: r.get(1)?,
        ts: r.get(2)?,
        kind: parse_kind(&kind),
        data: serde_json::from_str(&data).unwrap_or(serde_json::Value::Null),
    })
}

/// The database was written by a newer Hub than this one.
#[derive(Debug, thiserror::Error)]
#[error("the database has schema version {found}, but this Hub only knows up to {known}")]
pub struct TooNew {
    pub found: i64,
    pub known: usize,
}

/// Columns added before the schema had versions.
fn add_missing_columns(tx: &rusqlite::Transaction) -> rusqlite::Result<()> {
    let columns: Vec<String> = tx
        .prepare("SELECT name FROM pragma_table_info('threads')")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for (name, ddl) in [
        ("title_locked", "ALTER TABLE threads ADD COLUMN title_locked INTEGER NOT NULL DEFAULT 0"),
        ("archived", "ALTER TABLE threads ADD COLUMN archived INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !columns.iter().any(|c| c == name) {
            tx.execute(ddl, [])?;
        }
    }
    Ok(())
}

fn owner_json(owner: &Option<crate::model::Owner>) -> Option<String> {
    owner.as_ref().and_then(|o| serde_json::to_string(o).ok())
}

fn thread_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Thread> {
    let status: String = r.get(4)?;
    Ok(crate::thread_display::filled(Thread {
        id: r.get::<_, ThreadId>(0)?,
        title: r.get(1)?,
        cwd: r.get(2)?,
        engine: r.get(3)?,
        status: parse_status(&status),
        engine_session_id: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
        title_locked: r.get(8)?,
        archived: r.get(9)?,
        returned_at: r.get(10)?,
        seen_at: r.get(11)?,
        settled_at: r.get(12)?,
        sent_at: r.get(13)?,
        pinned_at: r.get(14)?,
        snoozed_until: r.get(15)?,
        auto_settle: r.get(16)?,
        parent: r.get(17)?,
        reading: r.get(18)?,
        owner: r.get::<_, Option<String>>(19)?.and_then(|o| serde_json::from_str(&o).ok()),
        cloud: r.get::<_, Option<String>>(20)?.and_then(|c| serde_json::from_str(&c).ok()),
        host: None, project: None, display: None,
    }))
}

fn cloud_json(c: &Option<crate::model::CloudRun>) -> Option<String> {
    c.as_ref().and_then(|c| serde_json::to_string(c).ok())
}

fn status_str(s: ThreadStatus) -> &'static str {
    match s {
        ThreadStatus::Idle => "idle",
        ThreadStatus::Running => "running",
        ThreadStatus::WaitingOnYou => "waiting_on_you",
        ThreadStatus::Error => "error",
        ThreadStatus::InTerminal => "in_terminal",
    }
}

fn parse_status(s: &str) -> ThreadStatus {
    match s {
        "running" => ThreadStatus::Running,
        "waiting_on_you" => ThreadStatus::WaitingOnYou,
        "error" => ThreadStatus::Error,
        "in_terminal" => ThreadStatus::InTerminal,
        _ => ThreadStatus::Idle,
    }
}

fn kind_str(k: EventKind) -> &'static str {
    match k {
        EventKind::UserMessage => "user_message",
        EventKind::Update => "update",
        EventKind::PermissionRequest => "permission_request",
        EventKind::PermissionResolved => "permission_resolved",
        EventKind::TurnEnd => "turn_end",
        EventKind::Error => "error",
        EventKind::SessionState => "session_state",
        EventKind::Setting => "setting",
        EventKind::RuleSaved => "rule_saved",
        EventKind::Checkpoint => "checkpoint",
        EventKind::Restored => "restored",
        EventKind::Review => "review",
        EventKind::QuotaSwitch => "quota_switch",
        EventKind::SessionStarted => "session_started",
        EventKind::Clash => "clash",
        EventKind::Leak => "leak",
        EventKind::Proof => "proof",
        EventKind::Docs => "docs",
        EventKind::Attempt => "attempt",
        EventKind::Terminal => "terminal",
        EventKind::QuotaWait => "quota_wait",
        EventKind::TurnStart => "turn_start",
        EventKind::Restart => "restart",
        EventKind::EngineSwitch => "engine_switch",
        EventKind::Paused => "paused",
        EventKind::Slept => "slept",
        EventKind::Page => "page",
        EventKind::Child => "child",
        EventKind::Auto => "auto",
        EventKind::PermissionAuto => "permission_auto",
        EventKind::Customs => "customs",
        EventKind::Queue => "queue",
        EventKind::HandedOff => "handed_off",
        EventKind::HandoffResult => "handoff_result",
        EventKind::HandoffOffer => "handoff_offer",
        EventKind::Focus => "focus",
        EventKind::Output => "output",
        EventKind::Rewind => "rewind",
        EventKind::Reading => "reading",
        EventKind::DeskNote => "desk_note",
        EventKind::SignInLost => "sign_in_lost",
        EventKind::PluginSuggested => "plugin_suggested",
        EventKind::Replies => "replies",
        EventKind::Reported => "reported",
        EventKind::Moved => "moved",
        EventKind::MovedAway => "moved_away",
        EventKind::WorkerRun => "worker_run",
        EventKind::Design => "design",
        EventKind::Unattended => "unattended",
        EventKind::Gist => "gist",
        EventKind::Reaction => "reaction",
        EventKind::Task => "task",
        EventKind::Ask => "ask",
        EventKind::Status => "status",
        EventKind::Unknown => "unknown",
    }
}

fn parse_kind(s: &str) -> EventKind {
    match s {
        "user_message" => EventKind::UserMessage,
        "queue" => EventKind::Queue,
        "permission_request" => EventKind::PermissionRequest,
        "permission_resolved" => EventKind::PermissionResolved,
        "turn_end" => EventKind::TurnEnd,
        "error" => EventKind::Error,
        "session_state" => EventKind::SessionState,
        "setting" => EventKind::Setting,
        "rule_saved" => EventKind::RuleSaved,
        "checkpoint" => EventKind::Checkpoint,
        "restored" => EventKind::Restored,
        "review" => EventKind::Review,
        "quota_switch" => EventKind::QuotaSwitch,
        "update" => EventKind::Update,
        "session_started" => EventKind::SessionStarted,
        "clash" => EventKind::Clash,
        "leak" => EventKind::Leak,
        "proof" => EventKind::Proof,
        "docs" => EventKind::Docs,
        "attempt" => EventKind::Attempt,
        "terminal" => EventKind::Terminal,
        "quota_wait" => EventKind::QuotaWait,
        "turn_start" => EventKind::TurnStart,
        "restart" => EventKind::Restart,
        "engine_switch" => EventKind::EngineSwitch,
        "paused" => EventKind::Paused,
        "slept" => EventKind::Slept,
        "page" => EventKind::Page,
        "child" => EventKind::Child,
        "auto" => EventKind::Auto,
        "permission_auto" => EventKind::PermissionAuto,
        "customs" => EventKind::Customs,
        "handed_off" => EventKind::HandedOff,
        "handoff_result" => EventKind::HandoffResult,
        "handoff_offer" => EventKind::HandoffOffer,
        "focus" => EventKind::Focus,
        "output" => EventKind::Output,
        "rewind" => EventKind::Rewind,
        "reading" => EventKind::Reading,
        "desk_note" => EventKind::DeskNote,
        "sign_in_lost" => EventKind::SignInLost,
        "plugin_suggested" => EventKind::PluginSuggested,
        "replies" => EventKind::Replies,
        "reported" => EventKind::Reported,
        "moved" => EventKind::Moved,
        "moved_away" => EventKind::MovedAway,
        "worker_run" => EventKind::WorkerRun,
        "design" => EventKind::Design,
        "unattended" => EventKind::Unattended,
        "gist" => EventKind::Gist,
        "reaction" => EventKind::Reaction,
        "task" => EventKind::Task,
        "ask" => EventKind::Ask,
        "status" => EventKind::Status,
        // Written by a newer version. Surfaces skip it; the row itself is left as it is.
        _ => EventKind::Unknown,
    }
}

mod threads;

mod events;

mod usage;

#[cfg(test)]
mod tests;
