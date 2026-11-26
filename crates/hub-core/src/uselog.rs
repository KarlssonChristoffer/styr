//! Which parts of Hub you use, kept so the Overview can show it as a map and an agent can say
//! what is in the way. One row per use: the feature's name (`thread.composer`), the area it sits
//! in, the feature used just before it and sometimes how long it took. Never anything typed,
//! opened or named: `clean` only lets short lower-case names through. Append-only, kept for 400
//! days, local to this machine, and switched off or cleared from the page.

use anyhow::Result;
use rusqlite::params;

use crate::hub::Hub;
use crate::model::{UseCount, UseFlow, UseHit, UseReport, now_ms};

const KEEP_MS: i64 = 400 * 86_400_000;
/// The kv key that holds "off".
const OFF: &str = "usage.off";
/// Most lines one call may bring.
const MOST: usize = 200;

/// A name is 1 to 48 of `a-z 0-9 . _ -`, so no sentence, path or title can pass as one.
pub fn clean(name: &str) -> bool {
    !name.is_empty() && name.len() <= 48 && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
}

pub async fn is_off(hub: &Hub) -> bool {
    hub.store.get_value(OFF).await.ok().flatten().as_deref() == Some("1")
}

pub async fn set_off(hub: &Hub, off: bool) -> Result<()> {
    hub.store.set_value(OFF, if off { "1" } else { "0" }).await
}

/// Writes what the page reported; returns how many lines were kept. Nothing is kept while the log is off.
pub async fn record(hub: &Hub, hits: Vec<UseHit>) -> Result<usize> {
    if is_off(hub).await {
        return Ok(0);
    }
    let now = now_ms();
    let hits: Vec<UseHit> = hits
        .into_iter()
        .take(MOST)
        .filter(|h| clean(&h.feature) && clean(&h.area) && (h.prev.is_empty() || clean(&h.prev)))
        // A clock that is far off must not put lines in the future or the far past.
        .map(|mut h| {
            h.ts = h.ts.clamp(now - 86_400_000, now);
            h.took_ms = h.took_ms.filter(|m| (0..3_600_000).contains(m));
            h
        })
        .collect();
    let n = hits.len();
    if n == 0 {
        return Ok(0);
    }
    hub.store
        .with(move |c| {
            let tx = c.transaction()?;
            {
                let mut ins = tx.prepare("INSERT INTO use_log (ts, feature, area, prev, took_ms) VALUES (?1, ?2, ?3, ?4, ?5)")?;
                for h in &hits {
                    ins.execute(params![h.ts, h.feature, h.area, h.prev, h.took_ms])?;
                }
            }
            tx.execute("DELETE FROM use_log WHERE ts < ?1", [now - KEEP_MS])?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    Ok(n)
}

/// Throws the whole log away; returns how many lines went.
pub async fn clear(hub: &Hub) -> Result<usize> {
    hub.store.with(|c| Ok(c.execute("DELETE FROM use_log", [])?)).await
}

/// Counts per feature and flows over the last `days`.
pub async fn report(hub: &Hub, days: u32) -> Result<UseReport> {
    let from = now_ms() - days.clamp(1, 400) as i64 * 86_400_000;
    let off = is_off(hub).await;
    let (features, flows, total, since) = hub
        .store
        .with(move |c| {
            let mut stmt = c.prepare("SELECT feature, area, ts, took_ms FROM use_log WHERE ts >= ?1 ORDER BY ts")?;
            let rows: Vec<(String, String, i64, Option<i64>)> =
                stmt.query_map([from], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
            let mut by: std::collections::HashMap<String, (String, i64, i64, Vec<i64>)> = Default::default();
            for (f, a, ts, took) in rows {
                let e = by.entry(f).or_insert((a, 0, 0, Vec::new()));
                e.1 += 1;
                e.2 = e.2.max(ts);
                e.3.extend(took);
            }
            let mut features: Vec<UseCount> = by
                .into_iter()
                .map(|(feature, (area, count, last_ts, mut times))| {
                    times.sort_unstable();
                    UseCount { feature, area, count, last_ts, median_ms: times.get(times.len() / 2).copied() }
                })
                .collect();
            features.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.feature.cmp(&b.feature)));
            let mut stmt = c.prepare(
                "SELECT prev, feature, COUNT(*) FROM use_log WHERE ts >= ?1 AND prev <> '' AND prev <> feature
                 GROUP BY prev, feature ORDER BY COUNT(*) DESC LIMIT 60",
            )?;
            let flows = stmt
                .query_map([from], |r| Ok(UseFlow { from: r.get(0)?, to: r.get(1)?, count: r.get(2)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let (total, since): (i64, Option<i64>) = c.query_row("SELECT COUNT(*), MIN(ts) FROM use_log", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok((features, flows, total, since))
        })
        .await?;
    Ok(UseReport { off, features, flows, total, since })
}
