//! Usage records and per-thread "latest" reads: what was read of Claude Code's files and cloud
//! sessions, the plan meter's samples, use and outcomes between two moments, and each thread's
//! latest context usage, event of a kind, session state, settings and MCP tool calls.

use super::*;

impl Store {
    /// How far each Claude Code session file has been read, by path.
    pub async fn cc_offsets(&self) -> Result<std::collections::HashMap<String, u64>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT substr(key, 11), value FROM kv WHERE key LIKE 'cc.offset.%'")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut out = std::collections::HashMap::new();
            for row in rows {
                let (path, offset) = row?;
                out.insert(path, offset.parse().unwrap_or(0));
            }
            Ok(out)
        })
        .await
    }

    /// Stores what was read from Claude Code's files, and how far, in one go. An answer seen
    /// twice keeps its largest token count, since its first lines can be partial.
    pub async fn cc_import(
        &self,
        marks: Vec<crate::model::rows::CcMark>,
        sessions: Vec<crate::model::rows::CcSession>,
        offsets: Vec<(String, u64)>,
    ) -> Result<()> {
        self.with(move |c| {
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            {
                let mut mark = tx.prepare(
                    "INSERT INTO cc_marks (key, session_id, ts, prompt, tokens, model, input, output, cache_write, cache_read)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(key) DO UPDATE SET model = coalesce(excluded.model, model),
                        input = max(input, excluded.input), output = max(output, excluded.output),
                        cache_write = max(cache_write, excluded.cache_write), cache_read = max(cache_read, excluded.cache_read),
                        tokens = max(tokens, excluded.tokens)",
                )?;
                for m in &marks {
                    mark.execute(params![
                        m.key, m.session_id, m.ts, m.prompt, m.tokens, m.model, m.input, m.output, m.cache_write, m.cache_read
                    ])?;
                }
                let mut session = tx.prepare(
                    "INSERT INTO cc_sessions (id, cwd, title) VALUES (?1, ?2, ?3)
                     ON CONFLICT(id) DO UPDATE SET cwd = coalesce(cwd, excluded.cwd), title = coalesce(excluded.title, title)",
                )?;
                for s in &sessions {
                    session.execute(params![s.id, s.cwd, s.title])?;
                }
                let mut offset = tx.prepare(
                    "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
                )?;
                for (path, at) in &offsets {
                    offset.execute(params![format!("cc.offset.{path}"), at.to_string()])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// Every cloud session as it was last looked at (`cloud_usage.rs`).
    pub async fn cloud_seen(&self) -> Result<std::collections::HashMap<String, crate::model::rows::Seen>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, updated_at, input, output, cache_write, cache_read, cost FROM cloud_sessions")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    crate::model::rows::Seen {
                        updated_at: r.get(1)?,
                        usage: crate::model::rows::Usage { input: r.get(2)?, output: r.get(3)?, cache_write: r.get(4)?, cache_read: r.get(5)?, cost: r.get(6)? },
                    },
                ))
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Stores what cloud sessions have now and what grew, together.
    pub async fn cloud_record(&self, seen: Vec<(String, crate::model::rows::Seen)>, marks: Vec<crate::model::rows::CloudMark>) -> Result<()> {
        self.with(move |c| {
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            {
                let mut row = tx.prepare(
                    "INSERT INTO cloud_sessions (id, updated_at, input, output, cache_write, cache_read, cost) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(id) DO UPDATE SET updated_at = ?2, input = ?3, output = ?4, cache_write = ?5, cache_read = ?6, cost = ?7",
                )?;
                for (id, s) in &seen {
                    let u = &s.usage;
                    row.execute(params![id, s.updated_at, u.input, u.output, u.cache_write, u.cache_read, u.cost])?;
                }
                let mut mark = tx.prepare(
                    "INSERT INTO cloud_marks (session_id, ts, input, output, cache_write, cache_read, cost) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )?;
                for m in &marks {
                    let u = &m.usage;
                    mark.execute(params![m.session_id, m.ts, u.input, u.output, u.cache_write, u.cache_read, u.cost])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    pub async fn add_plan_sample(&self, sample: &crate::model::rows::Sample) -> Result<()> {
        let s = sample.clone();
        self.with(move |c| {
            c.execute("INSERT INTO plan_samples (ts, percent, window) VALUES (?1, ?2, ?3)", params![s.ts, s.percent, s.window])?;
            Ok(())
        })
        .await
    }

    /// The plan meter's readings since `from`, oldest first.
    pub async fn plan_samples(&self, from: i64) -> Result<Vec<crate::model::rows::Sample>> {
        self.with(move |c| {
            let mut stmt = c.prepare("SELECT ts, percent, window FROM plan_samples WHERE ts >= ?1 ORDER BY ts")?;
            let rows = stmt.query_map([from], |r| Ok(crate::model::rows::Sample { ts: r.get(0)?, percent: r.get(1)?, window: r.get(2)? }))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// The first moment any answer was counted: Claude Code's marks or a cloud session's.
    pub async fn first_use(&self) -> Result<Option<i64>> {
        self.with(|c| {
            Ok(c.query_row("SELECT MIN(ts) FROM (SELECT MIN(ts) AS ts FROM cc_marks UNION ALL SELECT MIN(ts) FROM cloud_marks)", [], |r| r.get(0))?)
        })
        .await
    }

    /// Cloud use between two moments, oldest first.
    pub async fn cloud_marks(&self, from: i64, to: i64) -> Result<Vec<crate::model::rows::CloudMark>> {
        self.with(move |c| {
            let mut stmt = c.prepare(
                "SELECT session_id, ts, input, output, cache_write, cache_read, cost FROM cloud_marks WHERE ts >= ?1 AND ts < ?2 ORDER BY ts",
            )?;
            let rows = stmt.query_map(params![from, to], |r| {
                Ok(crate::model::rows::CloudMark {
                    session_id: r.get(0)?,
                    ts: r.get(1)?,
                    usage: crate::model::rows::Usage { input: r.get(2)?, output: r.get(3)?, cache_write: r.get(4)?, cache_read: r.get(5)?, cost: r.get(6)? },
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Claude Code prompts and answers between two moments, oldest first.
    pub async fn cc_marks(&self, from: i64, to: i64) -> Result<Vec<crate::model::rows::CcMark>> {
        self.with(move |c| {
            let mut stmt = c.prepare(
                "SELECT key, session_id, ts, prompt, tokens, model, input, output, cache_write, cache_read
                 FROM cc_marks WHERE ts >= ?1 AND ts < ?2 ORDER BY ts",
            )?;
            let rows = stmt.query_map(params![from, to], |r| {
                Ok(crate::model::rows::CcMark {
                    key: r.get(0)?,
                    session_id: r.get(1)?,
                    ts: r.get(2)?,
                    prompt: r.get(3)?,
                    tokens: r.get(4)?,
                    model: r.get(5)?,
                    input: r.get(6)?,
                    output: r.get(7)?,
                    cache_write: r.get(8)?,
                    cache_read: r.get(9)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Every engine session each thread has had, as `(thread, session)`.
    pub async fn thread_sessions(&self) -> Result<Vec<(String, String)>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT DISTINCT thread_id, json_extract(data, '$.session_id') FROM events
                 WHERE kind = 'session_started' AND json_extract(data, '$.session_id') IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Handed-off work that ended between two moments.
    pub async fn handed(&self, from: i64, to: i64) -> Result<Vec<crate::model::rows::Handed>> {
        self.with(move |c| {
            let mut stmt = c.prepare(
                "SELECT thread_id, json_extract(data, '$.model'), json_extract(data, '$.cost'), json_extract(data, '$.proven')
                 FROM events WHERE kind = 'handoff_result' AND ts >= ?1 AND ts < ?2",
            )?;
            let rows = stmt.query_map(params![from, to], |r| {
                Ok(crate::model::rows::Handed {
                    thread_id: r.get(0)?,
                    model: r.get(1)?,
                    cost: r.get(2)?,
                    proven: r.get::<_, Option<bool>>(3)?.unwrap_or(false),
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// Turn ends and proof verdicts between two moments, in log order.
    pub async fn outcomes(&self, from: i64, to: i64) -> Result<Vec<crate::model::rows::Outcome>> {
        self.with(move |c| {
            let mut stmt = c.prepare(
                "SELECT thread_id, seq, ts, CASE kind WHEN 'proof' THEN json_extract(data, '$.reply') END,
                        CASE kind WHEN 'proof' THEN json_extract(data, '$.state') END
                 FROM events WHERE kind IN ('turn_end', 'proof') AND ts >= ?1 AND ts < ?2 ORDER BY thread_id, seq",
            )?;
            let rows = stmt.query_map(params![from, to], |r| {
                Ok(crate::model::rows::Outcome { thread_id: r.get(0)?, seq: r.get(1)?, ts: r.get(2)?, reply: r.get(3)?, state: r.get(4)? })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    pub async fn cc_sessions(&self) -> Result<Vec<crate::model::rows::CcSession>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, cwd, title FROM cc_sessions")?;
            let rows = stmt.query_map([], |r| {
                Ok(crate::model::rows::CcSession { id: r.get(0)?, cwd: r.get(1)?, title: r.get(2)? })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// The latest context usage the engine of each thread reported.
    pub async fn latest_usage(&self) -> Result<Vec<crate::model::ThreadUsage>> {
        self.with(|c| {
            // SQLite returns the row holding MAX(seq) for the bare columns.
            let mut stmt = c.prepare(
                "SELECT thread_id, data, MAX(seq) FROM events
                 WHERE kind = 'update' AND json_extract(data, '$.sessionUpdate') = 'usage_update'
                 GROUP BY thread_id",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut usage = Vec::new();
            for row in rows {
                let (thread_id, data) = row?;
                let d: serde_json::Value = serde_json::from_str(&data)?;
                usage.push(crate::model::ThreadUsage {
                    thread_id,
                    used: d["used"].as_u64().unwrap_or_default(),
                    size: d["size"].as_u64().unwrap_or_default(),
                    cost: d["cost"]["amount"].as_f64(),
                    currency: d["cost"]["currency"].as_str().map(str::to_owned),
                });
            }
            Ok(usage)
        })
        .await
    }

    /// Every tool call that went to an MCP server, in one thread or all, oldest first.
    pub async fn mcp_calls(&self, thread_id: Option<String>) -> Result<Vec<crate::model::rows::Call>> {
        self.with(move |c| {
            let mut stmt = c.prepare(
                "SELECT e.thread_id, t.engine, e.ts, json_extract(e.data, '$.title'),
                        json_extract(e.data, '$._meta.claudeCode.toolName'), json_extract(e.data, '$.rawInput')
                 FROM events e JOIN threads t ON t.id = e.thread_id
                 WHERE e.kind = 'update' AND json_extract(e.data, '$.sessionUpdate') = 'tool_call'
                   AND (?1 IS NULL OR e.thread_id = ?1)
                 ORDER BY e.ts",
            )?;
            let rows = stmt.query_map(params![thread_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })?;
            let mut calls = Vec::new();
            for row in rows {
                let (thread_id, engine, ts, title, tool_name, raw) = row?;
                let raw: serde_json::Value = raw.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default();
                let names: Vec<&str> = [tool_name.as_deref(), title.as_deref()].into_iter().flatten().collect();
                if let Some((server, tool)) = crate::model::rows::mcp_tool(&names, &raw) {
                    calls.push(crate::model::rows::Call { thread_id, engine, ts, server, tool });
                }
            }
            Ok(calls)
        })
        .await
    }

    /// The latest event of `kind` in each thread that has one.
    pub async fn latest_of_kind(&self, kind: EventKind) -> Result<Vec<Event>> {
        self.with(move |c| {
            // SQLite returns the row holding MAX(seq) for the bare columns.
            let mut stmt = c.prepare(
                "SELECT thread_id, MAX(seq), ts, data FROM events WHERE kind = ?1 GROUP BY thread_id",
            )?;
            let rows = stmt.query_map(params![kind_str(kind)], |r| {
                let data: String = r.get(3)?;
                Ok(Event {
                    thread_id: r.get(0)?,
                    seq: r.get(1)?,
                    ts: r.get(2)?,
                    kind,
                    data: serde_json::from_str(&data).unwrap_or(serde_json::Value::Null),
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
        .await
    }

    /// How the thread runs without the user, when it does (its `unattended` event).
    pub async fn unattended(&self, thread_id: &str) -> Result<Option<crate::model::Unattended>> {
        let thread_id = thread_id.to_owned();
        self.with(move |c| {
            let data: Option<String> = c
                .query_row(
                    "SELECT data FROM events WHERE thread_id = ?1 AND kind = 'unattended' ORDER BY seq LIMIT 1",
                    [thread_id],
                    |r| r.get(0),
                )
                .optional()?;
            Ok(data.and_then(|d| serde_json::from_str(&d).ok()))
        })
        .await
    }

    /// The latest `session_state` of each thread: what its engine said can be set (models, modes).
    pub async fn latest_session_states(&self) -> Result<Vec<(String, serde_json::Value)>> {
        self.with(|c| {
            // SQLite returns the row holding MAX(seq) for the bare columns.
            let mut stmt = c.prepare(
                "SELECT thread_id, data, MAX(seq) FROM events WHERE kind = 'session_state' GROUP BY thread_id",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut states = Vec::new();
            for row in rows {
                let (thread_id, data) = row?;
                states.push((thread_id, serde_json::from_str(&data)?));
            }
            Ok(states)
        })
        .await
    }

    /// The latest value the user chose for each session setting of a thread, in the order they
    /// were first set.
    pub async fn settings(&self, thread_id: &str) -> Result<Vec<(String, serde_json::Value)>> {
        let thread_id = thread_id.to_owned();
        self.with(move |c| {
            let mut stmt = c.prepare(
                "SELECT kind, data FROM events WHERE thread_id = ?1 AND kind IN ('setting', 'engine_switch') ORDER BY seq",
            )?;
            let rows = stmt.query_map([thread_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut settings: Vec<(String, serde_json::Value)> = Vec::new();
            for row in rows {
                let (kind, data) = row?;
                // Settings before a switch were the old engine's.
                if kind == "engine_switch" {
                    settings.clear();
                    continue;
                }
                let data: serde_json::Value = serde_json::from_str(&data)?;
                let Some(id) = data["config_id"].as_str() else { continue };
                let value = data["value"].clone();
                match settings.iter_mut().find(|(k, _)| k == id) {
                    Some(entry) => entry.1 = value,
                    None => settings.push((id.to_owned(), value)),
                }
            }
            Ok(settings)
        })
        .await
    }
}
