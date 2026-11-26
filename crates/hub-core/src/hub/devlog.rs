//! The `Hub` half of `devlog.rs`: the nightly writer of each day's entry, "While you were away" and
//! the history search. They ask the running core (an engine, the GitHub cache),
//! so they live here; `devlog.rs` keeps the stored days and names nothing above it.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use super::Hub;
use crate::activity::{date_of, start_of};
use crate::devlog;
use crate::model::{now_ms, AwayRepo, AwayReport, EventKind, HistoryHit, HistoryKind};

impl Hub {
    /// Writes the entry of past days that have none (or only the older few lines), checking every
    /// hour, so a day is written within the hour after midnight. Today waits until it is
    /// over. A day where nothing happened is remembered as such, so it is not asked about again.
    pub async fn devlog_daily(self: Arc<Self>) {
        // Let the core settle after a start before using an engine.
        tokio::time::sleep(Duration::from_secs(120)).await;
        loop {
            if let Err(err) = self.devlog_catch_up().await {
                tracing::warn!("devlog: {err:#}");
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    }

    async fn devlog_catch_up(&self) -> Result<()> {
        let tz: i32 = self.store.get_value(devlog::TZ).await?.and_then(|v| v.parse().ok()).unwrap_or(0);
        let today = date_of(now_ms(), tz);
        let Some(start) = start_of(&today, tz) else { return Ok(()) };
        for back in 1..=devlog::BACK_DAYS {
            let date = date_of(start - back * 86_400_000, tz);
            let blog = self.store.get_value(&format!("{}{date}", devlog::BLOG)).await?.is_some();
            let empty = self.store.get_value(&format!("devlog.empty.{date}")).await?.is_some();
            // A day written in the old short form gets its devblog entry.
            if blog || empty {
                continue;
            }
            match self.summarize_day(&date, tz).await {
                Ok(_) => tracing::info!("devlog: wrote {date}"),
                Err(err) if err.to_string().contains("nothing happened") => {
                    self.store.set_value(&format!("devlog.empty.{date}"), "1").await?;
                }
                // An engine that is not set up fails the same way every hour; try again next time.
                Err(err) => return Err(err.context(format!("writing {date}"))),
            }
        }
        Ok(())
    }

    /// What happened since `since` (ms) besides the threads themselves, for "While you were away".
    /// The projects the health check found wanting are added by the caller (`health::troubled_since`),
    /// so the core does not name the health check.
    pub async fn away(&self, since: i64, tz: i32) -> Result<AwayReport> {
        let mut report = AwayReport::default();

        let folders = self.activity_folders().await?;
        let commits = tokio::task::spawn_blocking(move || crate::activity::commits(&folders, since)).await?;
        for c in commits {
            let repo = match report.repos.iter_mut().find(|r| r.path == c.repo) {
                Some(r) => r,
                None => {
                    let name = std::path::Path::new(&c.repo).file_name().map_or(c.repo.clone(), |n| n.to_string_lossy().into_owned());
                    report.repos.push(AwayRepo { path: c.repo.clone(), name, ..Default::default() });
                    report.repos.last_mut().expect("just pushed")
                }
            };
            repo.commits += 1;
            if repo.subjects.len() < 3 {
                repo.subjects.push(c.subject);
            }
        }
        report.repos.sort_by_key(|r| std::cmp::Reverse(r.commits));

        if let Some(g) = self.cached_github().await {
            let from = date_of(since, tz);
            report.pulls = g
                .pulls
                .iter()
                .filter(|p| p.0 >= from)
                .map(|(_, repo, title, url)| crate::model::DayPull { repo: repo.clone(), title: title.clone(), url: url.clone() })
                .collect();
        }

        let kinds = [EventKind::PermissionAuto, EventKind::QuotaWait, EventKind::Restart, EventKind::Review, EventKind::Child];
        for e in self.store.events_since(since, &kinds).await? {
            let state = e.data["state"].as_str();
            match e.kind {
                EventKind::PermissionAuto => report.permissions += 1,
                EventKind::QuotaWait if state == Some("resumed") => report.resumed += 1,
                EventKind::Restart if state == Some("continued") => report.resumed += 1,
                EventKind::Review if state == Some("done") => report.reviews += 1,
                EventKind::Child if state == Some("done") => report.side_jobs += 1,
                _ => {}
            }
        }

        Ok(report)
    }

    /// Everything that mentions `q`, newest first.
    pub async fn history_search(&self, q: &str, tz: i32) -> Result<Vec<HistoryHit>> {
        let q = q.trim();
        if q.chars().count() < 2 {
            return Ok(Vec::new());
        }
        const LIMIT: usize = 60;
        let mut hits = self.store.search_history(q, LIMIT).await?;
        let folders = self.activity_folders().await?;
        let needle = q.to_lowercase();
        let commits = tokio::task::spawn_blocking(move || crate::activity::commits(&folders, 0)).await?;
        hits.extend(commits.into_iter().filter(|c| c.subject.to_lowercase().contains(&needle)).take(LIMIT).map(|c| {
            let repo = std::path::Path::new(&c.repo).file_name().map(|n| n.to_string_lossy().into_owned());
            HistoryHit { kind: HistoryKind::Commit, ts: c.ts, date: None, text: c.subject, thread_id: None, title: repo, cwd: Some(c.repo) }
        }));
        for h in &mut hits {
            if h.kind == HistoryKind::Day {
                // A day sorts at its noon.
                h.ts = h.date.as_deref().and_then(|d| start_of(d, tz)).map_or(0, |t| t + 12 * 3_600_000);
            } else {
                h.date = Some(date_of(h.ts, tz));
            }
            h.text = crate::devlog::around(&h.text, &needle);
        }
        hits.sort_by_key(|h| std::cmp::Reverse(h.ts));
        hits.truncate(LIMIT);
        Ok(hits)
    }
}
