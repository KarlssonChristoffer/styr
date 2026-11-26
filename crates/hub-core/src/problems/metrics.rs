//! Timings with a baseline per version: how long from Send to the first word of an answer, how
//! long the core's API takes, how much memory hubd holds, and how long the user's programs'
//! transactions take (from a Sentry SDK). Samples gather in memory; each sweep keeps one median
//! per metric and release. When a release is clearly slower than the one before it (half as slow
//! again, and by more than a little), that is a performance problem like any other.

use std::sync::Arc;

use anyhow::Result;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::{Hit, Incoming, Level, Source};
use crate::hub::Hub;
use crate::model::now_ms;

pub use crate::metric_samples::{RELEASE, engine_said, sample, turn_started};
/// Rows kept per metric.
const KEEP: i64 = 2000;
/// Rows a release needs before it is compared.
const ENOUGH: usize = 6;

/// One metric in the current release next to the one before.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MetricSummary {
    pub name: String,
    /// What it measures, in words.
    pub label: String,
    pub unit: String,
    pub release: String,
    pub median: f64,
    #[ts(type = "number")]
    pub samples: i64,
    pub previous_release: Option<String>,
    pub previous_median: Option<f64>,
    /// The medians kept for the current release, oldest first, for a sparkline.
    pub trend: Vec<f64>,
}

/// A Sentry transaction: its name and how long it took.
pub async fn transaction(_hub: &Hub, source: &Source, item: &Value) -> Result<()> {
    let (Some(start), Some(end)) = (item["start_timestamp"].as_f64(), item["timestamp"].as_f64()) else { return Ok(()) };
    let name = item["transaction"].as_str().unwrap_or("transaction");
    sample(&format!("tx:{name}"), (end - start) * 1000.0, item["release"].as_str(), &source.name);
    Ok(())
}

fn label(name: &str) -> (String, &'static str) {
    match name {
        "first_token_ms" => ("Send to first word of the answer".into(), "ms"),
        "api_ms" => ("Hub's API, per request".into(), "ms"),
        "hubd_memory_mb" => ("Memory hubd holds".into(), "MB"),
        n => (n.strip_prefix("tx:").map(|t| format!("Transaction {t}")).unwrap_or_else(|| n.to_owned()), "ms"),
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Keeps one median per metric from what gathered, reads hubd's memory, then compares releases.
pub async fn sweep(hub: &Arc<Hub>) -> Result<()> {
    if let Some(mb) = own_memory_mb() {
        sample("hubd_memory_mb", mb, None, "Hub");
    }
    let taken = crate::metric_samples::take();
    let machine = hub.remotes.local_name().to_owned();
    let now = now_ms();
    let rows: Vec<(String, String, f64)> = taken
        .into_iter()
        .map(|((name, release, project), mut values)| {
            // The project rides in the name so one metric of two programs stays apart.
            let name = if project == "Hub" { name } else { format!("{project}/{name}") };
            (name, release, median(&mut values))
        })
        .collect();
    if !rows.is_empty() {
        hub.store
            .with(move |c| {
                for (name, release, value) in &rows {
                    c.execute(
                        "INSERT INTO problem_metrics (name, release, machine, ts, value) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![name, release, machine, now, value],
                    )?;
                    c.execute(
                        "DELETE FROM problem_metrics WHERE name = ?1 AND rowid NOT IN
                           (SELECT rowid FROM problem_metrics WHERE name = ?1 ORDER BY ts DESC LIMIT ?2)",
                        params![name, KEEP],
                    )?;
                }
                Ok(())
            })
            .await?;
    }
    for m in summaries(hub).await? {
        report(hub, &m, now).await?;
    }
    Ok(())
}

/// Records a release that is clearly slower than the one before it, as a hit on its problem.
pub async fn report(hub: &Arc<Hub>, m: &MetricSummary, now: i64) -> Result<()> {
    let (Some(prev), Some(prev_release)) = (m.previous_median, m.previous_release.as_deref()) else { return Ok(()) };
    if m.samples < ENOUGH as i64 || prev <= 0.0 {
        return Ok(());
    }
    let floor = if m.unit == "MB" { 50.0 } else { 100.0 };
    if m.median > prev * 1.5 && m.median - prev > floor {
        let (project, short) = match m.name.split_once('/') {
            Some((p, n)) => (p.to_owned(), n.to_owned()),
            None => ("Hub".to_owned(), m.name.clone()),
        };
        let words = vec!["slower".to_owned(), short, m.release.clone()];
        // A steady difference is one hit: a sweep only counts when the median grew by a tenth over
        // the one the last hit recorded (a new release is a new problem, so it starts with one).
        let key = super::fingerprint(&project, "metrics", Some(&words), &Hit::default());
        let Some(sweeps) = sweeps_to_report(hub, key, &m.name, &m.release, m.median, now).await? else { return Ok(()) };
        let message = format!(
            "{} is {:.1}× {} in {} ({:.0} {} against {:.0} {} in {prev_release})",
            m.label,
            m.median / prev,
            if m.unit == "MB" { "larger" } else { "slower" },
            m.release,
            m.median,
            m.unit,
            prev,
            m.unit
        );
        let incoming = Incoming {
            project,
            source: "metrics".into(),
            level: Level::Warning,
            folder: None,
            fingerprint: Some(words),
            hit: Hit {
                ts: now,
                message,
                release: Some(m.release.clone()),
                extra: json!({ "metric": m.name, "median": m.median, "before": prev, "before_release": prev_release, "sweeps": sweeps }),
                ..Default::default()
            },
        };
        let p = super::record(hub, incoming).await?;
        if p.category.is_none() {
            let id = p.id;
            hub.store.with(move |c| Ok(c.execute("UPDATE problems SET category = 'performance' WHERE id = ?1 AND category IS NULL", [id])?)).await?;
        }
    }
    Ok(())
}

/// How many sweeps the metric has stayed slow since its problem's first hit, when this sweep is
/// worth a hit: the problem's first, or one whose median grew by a tenth over the last hit's.
async fn sweeps_to_report(hub: &Hub, key: String, name: &str, release: &str, median: f64, now: i64) -> Result<Option<i64>> {
    let (name, release) = (name.to_owned(), release.to_owned());
    hub.store
        .with(move |c| {
            // The problem may have been merged into another: `record` follows the alias first, so
            // the hits are found the same way (matching on `problems.fingerprint` alone missed
            // them and every sweep counted).
            let problem: Option<i64> = c
                .query_row(
                    "SELECT COALESCE((SELECT problem_id FROM problem_aliases WHERE fingerprint = ?1),
                                     (SELECT id FROM problems WHERE fingerprint = ?1))",
                    params![key],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            let last: Option<Option<f64>> = c
                .query_row(
                    "SELECT json_extract(data, '$.extra.median') FROM problem_hits
                     WHERE problem_id = ?1 AND json_extract(data, '$.release') = ?2 ORDER BY ts DESC LIMIT 1",
                    params![problem, release],
                    |r| r.get(0),
                )
                .optional()?;
            if matches!(last, Some(Some(recorded)) if median < recorded * 1.1) {
                return Ok(None);
            }
            let since: i64 = c.query_row(
                "SELECT COALESCE(MIN(ts), ?3) FROM problem_hits WHERE problem_id = ?1 AND json_extract(data, '$.release') = ?2",
                params![problem, release, now],
                |r| r.get(0),
            )?;
            let sweeps: i64 = c.query_row(
                "SELECT COUNT(*) FROM problem_metrics WHERE name = ?1 AND release = ?2 AND ts >= ?3",
                params![name, release, since],
                |r| r.get(0),
            )?;
            Ok(Some(sweeps))
        })
        .await
}

/// Every metric, the latest release against the one before it.
pub async fn summaries(hub: &Hub) -> Result<Vec<MetricSummary>> {
    hub.store
        .with(|c| {
            let names: Vec<String> = {
                let mut q = c.prepare("SELECT DISTINCT name FROM problem_metrics ORDER BY name")?;
                q.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
            };
            let mut out = Vec::new();
            for name in names {
                // Releases in the order they were first seen, newest last.
                let releases: Vec<String> = {
                    let mut q = c.prepare("SELECT release FROM problem_metrics WHERE name = ?1 GROUP BY release ORDER BY MIN(ts)")?;
                    q.query_map([&name], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
                };
                let Some(release) = releases.last().cloned() else { continue };
                let values = |release: &str| -> rusqlite::Result<Vec<f64>> {
                    let mut q = c.prepare("SELECT value FROM problem_metrics WHERE name = ?1 AND release = ?2 ORDER BY ts DESC LIMIT 200")?;
                    let mut v: Vec<f64> = q.query_map(params![name, release], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
                    v.reverse();
                    Ok(v)
                };
                let trend = values(&release)?;
                let previous = releases.len().checked_sub(2).map(|i| releases[i].clone());
                let previous_median = match &previous {
                    Some(r) => {
                        let mut v = values(r)?;
                        (v.len() >= ENOUGH).then(|| median(&mut v))
                    }
                    None => None,
                };
                let (label, unit) = label(name.split_once('/').map(|(_, n)| n).unwrap_or(&name));
                out.push(MetricSummary {
                    median: median(&mut trend.clone()),
                    samples: trend.len() as i64,
                    trend: trend.iter().rev().take(60).rev().copied().collect(),
                    name,
                    label,
                    unit: unit.into(),
                    release,
                    previous_release: previous,
                    previous_median,
                });
            }
            Ok(out)
        })
        .await
}

fn own_memory_mb() -> Option<f64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = Pid::from_u32(std::process::id());
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory());
    sys.process(pid).map(|p| p.memory() as f64 / 1_048_576.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medians() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 2.0, 3.0]), 2.5);
        assert_eq!(median(&mut []), 0.0);
    }
}
