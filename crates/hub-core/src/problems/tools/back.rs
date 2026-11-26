//! The one-time backfill of tool-call counts: the calls and failure texts already in the event
//! log are counted into `tool_calls` and `tool_errors` once (stamped in the store), so the Tools
//! tab has a history from the start. Counting new calls as they are logged is `tools.rs` (`saw`).

use super::*;

/// Counts the calls already in the event log, once, so the Tools tab has a history from the
/// start, and what the failed ones said (once more for a database that counted its calls before
/// errors were counted). Failures found there are counted but not made into problems.
pub(crate) async fn backfill(hub: Arc<Hub>) -> Result<()> {
    let calls_too = hub.store.get_value(COUNTED).await?.is_none();
    let errors = hub.store.get_value(ERRORS_COUNTED).await?.is_none();
    if !calls_too && !errors {
        return Ok(());
    }
    let until = now_ms();
    if calls_too {
        hub.store.set_value(COUNTED, &until.to_string()).await?;
    }
    if errors {
        hub.store.set_value(ERRORS_COUNTED, &until.to_string()).await?;
    }
    let since = until - BACKFILL_DAYS * DAY_MS;
    let blobs = hub.store.blob_dir();
    hub.store
        .with(move |c| {
            let mut calls: HashMap<(String, String), (String, i64, Value)> = HashMap::new();
            {
                let mut stmt = c.prepare(
                    "SELECT e.thread_id, t.engine, e.ts, e.data FROM events e JOIN threads t ON t.id = e.thread_id
                     WHERE e.kind = 'update' AND e.ts >= ?1 AND e.ts < ?2
                       AND json_extract(e.data, '$.sessionUpdate') IN ('tool_call', 'tool_call_update')
                     ORDER BY e.thread_id, e.seq",
                )?;
                let rows = stmt.query_map(params![since, until], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?))
                })?;
                // (thread, call) -> (engine, first ts, merged call)
                for row in rows {
                    let (thread, engine, ts, data) = row?;
                    let d: Value = serde_json::from_str(&data).unwrap_or_default();
                    let Some(id) = d["toolCallId"].as_str() else { continue };
                    let entry = calls.entry((thread, id.to_owned())).or_insert_with(|| (engine, ts, json!({})));
                    merge_into(&mut entry.2, &d);
                }
            }
            let mut counts: HashMap<(i64, String, String), (i64, i64)> = HashMap::new();
            let mut said_back: HashMap<(i64, String, String, String), i64> = HashMap::new();
            for (engine, ts, mut call) in calls.into_values() {
                // Only a failed call's text is read, so only then is a picture fetched from its file.
                if call["status"] == "failed"
                    && let Some(dir) = &blobs {
                        crate::blobs::inline(dir, &mut call);
                    }
                let text = said(&call);
                let failed = call["status"] == "failed" && !stopped(&text);
                let (day, tool) = (ts.div_euclid(DAY_MS), tool_name(&call));
                if failed {
                    *said_back.entry((day, engine.clone(), tool.clone(), gist(&text))).or_default() += 1;
                }
                let c = counts.entry((day, engine, tool)).or_default();
                c.0 += 1;
                c.1 += i64::from(failed);
            }
            let tx = c.transaction()?;
            if calls_too {
                for ((day, engine, tool), (n, failed)) in counts {
                    tx.execute(
                        "INSERT INTO tool_calls (day, engine, tool, calls, failed) VALUES (?1, ?2, ?3, ?4, ?5)
                         ON CONFLICT(day, engine, tool) DO UPDATE SET calls = calls + excluded.calls, failed = failed + excluded.failed",
                        params![day, engine, tool, n, failed],
                    )?;
                }
            }
            if errors {
                for ((day, engine, tool, gist), n) in said_back {
                    add_error(&tx, day, &engine, &tool, &gist, n)?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
}
