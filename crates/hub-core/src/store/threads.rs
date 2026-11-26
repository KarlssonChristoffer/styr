//! The store's rows besides the event log: threads (add, change, mark, delete, moved to and from
//! another machine), projects, small key-value settings, and the history search over messages,
//! Claude Code prompts and devlog days.

use super::*;

impl Store {
    pub async fn insert_thread(&self, thread: Thread) -> Result<Thread> {
        self.with(move |c| {
            c.execute(
                "INSERT INTO threads (id, title, cwd, engine, status, engine_session_id, created_at, updated_at, title_locked, archived,
                 returned_at, seen_at, settled_at, sent_at, pinned_at, snoozed_until, auto_settle, parent_id, reading, owner, cloud)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                params![
                    thread.id,
                    thread.title,
                    thread.cwd,
                    thread.engine,
                    status_str(thread.status),
                    thread.engine_session_id,
                    thread.created_at,
                    thread.updated_at,
                    thread.title_locked,
                    thread.archived,
                    thread.returned_at,
                    thread.seen_at,
                    thread.settled_at,
                    thread.sent_at,
                    thread.pinned_at,
                    thread.snoozed_until,
                    thread.auto_settle,
                    thread.parent,
                    thread.reading,
                    owner_json(&thread.owner),
                    cloud_json(&thread.cloud)
                ],
            )?;
            Ok(thread)
        })
        .await
    }

    /// Where the thread went, when it was moved off this machine.
    pub async fn moved_to(&self, id: &str) -> Result<Option<String>> {
        self.get_value(&format!("{MOVED}{id}")).await
    }

    /// Notes that the thread went to machine `to` and drops it from `threads()`.
    pub async fn set_moved_to(&self, id: &str, to: &str) -> Result<()> {
        self.set_value(&format!("{MOVED}{id}"), to).await
    }

    /// Forgets that the thread was moved away (it came back).
    pub async fn clear_moved(&self, id: &str) -> Result<()> {
        self.take_value(&format!("{MOVED}{id}")).await.map(|_| ())
    }

    /// Adds a thread that was moved here, with its log as it was, seqs and times included.
    pub async fn import_thread(&self, thread: Thread, events: Vec<Event>) -> Result<()> {
        let blobs = self.blobs.clone();
        self.with(move |c| {
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "INSERT INTO threads (id, title, cwd, engine, status, engine_session_id, created_at, updated_at, title_locked, archived,
                 returned_at, seen_at, settled_at, sent_at, pinned_at, snoozed_until, auto_settle, parent_id, reading, owner, cloud)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                params![
                    thread.id,
                    thread.title,
                    thread.cwd,
                    thread.engine,
                    status_str(thread.status),
                    thread.engine_session_id,
                    thread.created_at,
                    thread.updated_at,
                    thread.title_locked,
                    thread.archived,
                    thread.returned_at,
                    thread.seen_at,
                    thread.settled_at,
                    thread.sent_at,
                    thread.pinned_at,
                    thread.snoozed_until,
                    thread.auto_settle,
                    thread.parent,
                    thread.reading,
                    owner_json(&thread.owner),
                    cloud_json(&thread.cloud)
                ],
            )?;
            for e in &events {
                let kept = if e.kind == EventKind::Update { kept_data(blobs.as_deref(), &e.data) } else { None };
                tx.execute(
                    "INSERT INTO events (thread_id, seq, ts, kind, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![thread.id, e.seq, e.ts, kind_str(e.kind), serde_json::to_string(kept.as_ref().unwrap_or(&e.data))?],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn threads(&self) -> Result<Vec<Thread>> {
        self.with(|c| {
            // A thread moved to another machine is not listed here any more (`moving.rs`).
            let mut stmt = c.prepare(&format!(
                "{THREAD_SELECT} WHERE id NOT IN (SELECT substr(key, {}) FROM kv WHERE key LIKE 'moved:%') ORDER BY updated_at DESC",
                MOVED.len() + 1
            ))?;
            let rows = stmt.query_map([], thread_from_row)?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Settings whose key starts with `prefix`, with the prefix cut off.
    pub async fn values_with_prefix(&self, prefix: &str) -> Result<Vec<(String, String)>> {
        let prefix = prefix.to_owned();
        self.with(move |c| {
            let mut stmt = c.prepare("SELECT substr(key, length(?1) + 1), value FROM kv WHERE substr(key, 1, length(?1)) = ?1 ORDER BY key")?;
            let rows = stmt.query_map([&prefix], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Your messages to threads, Claude Code prompts and devlog days that contain `q`, newest
    /// first. Matching ignores case for ASCII letters only, as SQLite's LIKE does.
    pub async fn search_history(&self, q: &str, limit: usize) -> Result<Vec<crate::model::HistoryHit>> {
        use crate::model::{HistoryHit, HistoryKind};
        let pattern = format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        self.with(move |c| {
            let mut hits = Vec::new();
            let mut stmt = c.prepare(
                "SELECT e.thread_id, e.ts, json_extract(e.data, '$.text'), t.title, t.cwd FROM events e
                 LEFT JOIN threads t ON t.id = e.thread_id
                 WHERE e.kind = 'user_message' AND json_extract(e.data, '$.text') LIKE ?1 ESCAPE '\\'
                 ORDER BY e.ts DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![pattern, limit as i64], |r| {
                Ok(HistoryHit {
                    kind: HistoryKind::Message,
                    ts: r.get(1)?,
                    date: None,
                    text: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    thread_id: Some(r.get(0)?),
                    title: r.get(3)?,
                    cwd: r.get(4)?,
                })
            })?;
            hits.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
            let mut stmt = c.prepare(
                "SELECT m.ts, m.prompt, s.title, s.cwd FROM cc_marks m LEFT JOIN cc_sessions s ON s.id = m.session_id
                 WHERE m.prompt LIKE ?1 ESCAPE '\\' ORDER BY m.ts DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![pattern, limit as i64], |r| {
                Ok(HistoryHit {
                    kind: HistoryKind::Prompt,
                    ts: r.get(0)?,
                    date: None,
                    text: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    thread_id: None,
                    title: r.get(2)?,
                    cwd: r.get(3)?,
                })
            })?;
            hits.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
            let mut stmt = c.prepare(
                "SELECT substr(key, 18), value FROM kv WHERE key LIKE 'activity.summary.%' AND value LIKE ?1 ESCAPE '\\'
                 ORDER BY key DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![pattern, limit as i64], |r| {
                Ok(HistoryHit {
                    kind: HistoryKind::Day,
                    ts: 0,
                    date: Some(r.get(0)?),
                    text: r.get(1)?,
                    thread_id: None,
                    title: None,
                    cwd: None,
                })
            })?;
            hits.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
            Ok(hits)
        })
        .await
    }

    /// A small setting kept in the database, e.g. the folder new projects go in.
    pub async fn get_value(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_owned();
        self.with(move |c| Ok(c.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0)).optional()?))
            .await
    }

    pub async fn set_value(&self, key: &str, value: &str) -> Result<()> {
        let (key, value) = (key.to_owned(), value.to_owned());
        self.with(move |c| {
            c.execute("INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", [key, value])?;
            Ok(())
        })
        .await
    }

    /// Reads a value and removes it, so only one reader gets it.
    pub async fn take_value(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_owned();
        self.with(move |c| {
            Ok(c.query_row("DELETE FROM kv WHERE key = ?1 RETURNING value", [key], |r| r.get(0)).optional()?)
        })
        .await
    }

    pub async fn projects(&self) -> Result<Vec<Project>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT path, name, created_at FROM projects ORDER BY created_at DESC")?;
            let rows = stmt.query_map([], |r| Ok(Project { path: r.get(0)?, name: r.get(1)?, created_at: r.get(2)? }))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Adds the project, or returns the one already stored for its folder.
    pub async fn insert_project(&self, project: Project) -> Result<Project> {
        self.with(move |c| {
            c.execute(
                "INSERT OR IGNORE INTO projects (path, name, created_at) VALUES (?1, ?2, ?3)",
                params![project.path, project.name, project.created_at],
            )?;
            Ok(c.query_row("SELECT path, name, created_at FROM projects WHERE path = ?1", [&project.path], |r| {
                Ok(Project { path: r.get(0)?, name: r.get(1)?, created_at: r.get(2)? })
            })?)
        })
        .await
    }

    pub async fn delete_project(&self, path: &str) -> Result<()> {
        let path = path.to_owned();
        self.with(move |c| {
            c.execute("DELETE FROM projects WHERE path = ?1", [path])?;
            Ok(())
        })
        .await
    }

    pub async fn thread(&self, id: &str) -> Result<Option<Thread>> {
        let id = id.to_owned();
        self.with(move |c| {
            Ok(c.query_row(&format!("{THREAD_SELECT} WHERE id = ?1"), [id], thread_from_row)
                .optional()?)
        })
        .await
    }

    /// Applies `f` to the stored thread and returns the new version.
    pub async fn update_thread(
        &self,
        id: &str,
        f: impl FnOnce(&mut Thread) + Send + 'static,
    ) -> Result<Thread> {
        self.change_thread(id, true, f).await
    }

    /// Like `update_thread`, but leaves `updated_at` alone: for marks such as seen or settled,
    /// which say nothing about the work in the thread.
    pub async fn mark_thread(
        &self,
        id: &str,
        f: impl FnOnce(&mut Thread) + Send + 'static,
    ) -> Result<Thread> {
        self.change_thread(id, false, f).await
    }

    pub(super) async fn change_thread(
        &self,
        id: &str,
        bump: bool,
        f: impl FnOnce(&mut Thread) + Send + 'static,
    ) -> Result<Thread> {
        let id = id.to_owned();
        self.with(move |c| {
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let mut thread = tx
                .query_row(&format!("{THREAD_SELECT} WHERE id = ?1"), [&id], thread_from_row)
                .optional()?
                .with_context(|| format!("no thread {id}"))?;
            f(&mut thread); crate::thread_display::refill(&mut thread);
            if bump {
                thread.updated_at = now_ms();
            }
            tx.execute(
                "UPDATE threads SET title = ?2, status = ?3, engine_session_id = ?4, updated_at = ?5,
                 title_locked = ?6, archived = ?7, cwd = ?8, returned_at = ?9, seen_at = ?10,
                 settled_at = ?11, engine = ?12, sent_at = ?13, pinned_at = ?14, snoozed_until = ?15,
                 auto_settle = ?16, reading = ?17, owner = ?18, cloud = ?19 WHERE id = ?1",
                params![
                    thread.id,
                    thread.title,
                    status_str(thread.status),
                    thread.engine_session_id,
                    thread.updated_at,
                    thread.title_locked,
                    thread.archived,
                    thread.cwd,
                    thread.returned_at,
                    thread.seen_at,
                    thread.settled_at,
                    thread.engine,
                    thread.sent_at,
                    thread.pinned_at,
                    thread.snoozed_until,
                    thread.auto_settle,
                    thread.reading,
                    owner_json(&thread.owner),
                    cloud_json(&thread.cloud)
                ],
            )?;
            tx.commit()?;
            Ok(thread)
        })
        .await
    }

    /// Removes a thread and its whole log.
    pub async fn delete_thread(&self, id: &str) -> Result<()> {
        let id = id.to_owned();
        let blobs = self.blobs.clone();
        self.with(move |c| {
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            // The pictures this thread's events point at; their files go too unless another
            // thread's events still point at them (the same picture is one file, `blobs.rs`).
            let mut names = std::collections::HashSet::new();
            if blobs.is_some() {
                let mut stmt = tx.prepare("SELECT data FROM events WHERE thread_id = ?1 AND kind = 'update' AND instr(data, '\"blob\"') > 0")?;
                let rows = stmt.query_map([&id], |r| r.get::<_, String>(0))?;
                for data in rows {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data?) {
                        crate::blobs::names_in(&v, &mut names);
                    }
                }
            }
            tx.execute("DELETE FROM events WHERE thread_id = ?1", [&id])?;
            tx.execute("DELETE FROM threads WHERE id = ?1", [&id])?;
            let mut gone = Vec::new();
            for name in names {
                let used: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM events WHERE instr(data, ?1) > 0)", [&name], |r| r.get(0))?;
                if !used {
                    gone.push(name);
                }
            }
            tx.commit()?;
            if let Some(dir) = &blobs {
                for name in gone {
                    crate::locked::clean_file(dir.join(name));
                }
            }
            Ok(())
        })
        .await
    }
}
