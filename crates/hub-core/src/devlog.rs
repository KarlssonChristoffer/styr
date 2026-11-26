//! The devlog and the searchable history. Each day you worked gets a devblog entry (a one-line
//! gist, then Shipped / Worked / Blocked / Next), written in the night after it without you asking
//! and kept, so opening a day on the activity map reads it instead of working anything out. Searching
//! looks through those days, your messages and prompts, and your commits. The parts that need the
//! running core (the nightly writer, "While you were away" and the search) are in `hub/devlog.rs`.

use anyhow::Result;

use crate::activity::{date_of, start_of};
use crate::ctx::Ctx;
use crate::model::{now_ms, DayRef, DevlogDay, ProjectDay};

/// Where the summaries are kept, by date.
pub const SUMMARY: &str = "activity.summary.";
/// Marks a day whose summary is the devblog entry, not the older few lines; older ones in reach
/// are written again.
pub const BLOG: &str = "devlog.blog.";
/// The time zone the app last asked in, so days end at your midnight.
pub const TZ: &str = "activity.tz";
/// How far back a missing day is still written.
pub(crate) const BACK_DAYS: i64 = 7;

/// One day's written entry, read as it is, so opening a day is instant.
pub async fn day(ctx: &Ctx<'_>, date: &str) -> Result<Option<DevlogDay>> {
    Ok(ctx.store.get_value(&format!("{SUMMARY}{date}")).await?.map(|summary| DevlogDay { date: date.to_owned(), summary }))
}

/// The written days, newest first.
pub async fn days(ctx: &Ctx<'_>, limit: usize) -> Result<Vec<DevlogDay>> {
    let mut days: Vec<DevlogDay> = ctx
        .store
        .values_with_prefix(SUMMARY)
        .await?
        .into_iter()
        .map(|(date, summary)| DevlogDay { date, summary })
        .collect();
    days.reverse();
    days.truncate(limit);
    Ok(days)
}

/// One project's last `days`, newest first: the commits in its repository and the threads in it
/// that were written in, each day. Read as it is, so it needs no model and is never late.
pub async fn project_days(ctx: &Ctx<'_>, root: &str, days: u32, tz: i32) -> Result<Vec<ProjectDay>> {
    let today = date_of(now_ms(), tz);
    let Some(start) = start_of(&today, tz) else { return Ok(Vec::new()) };
    let (from, to) = (start - (days.clamp(1, 365) as i64 - 1) * 86_400_000, start + 86_400_000);
    let mut out: std::collections::BTreeMap<String, ProjectDay> = Default::default();
    fn day(out: &mut std::collections::BTreeMap<String, ProjectDay>, date: String) -> &mut ProjectDay {
        out.entry(date.clone()).or_insert_with(|| ProjectDay { date, ..Default::default() })
    }

    let folder = root.to_owned();
    let commits = tokio::task::spawn_blocking(move || crate::activity::commits(&std::collections::BTreeSet::from([folder]), from)).await?;
    for c in commits.into_iter().filter(|c| c.ts < to) {
        let d = day(&mut out, date_of(c.ts, tz));
        d.commits += 1;
        if d.subjects.len() < 5 {
            d.subjects.push(c.subject);
        }
    }

    let threads = ctx.store.threads().await?;
    for mark in ctx.store.activity(from, to).await? {
        let Some(t) = threads.iter().find(|t| t.id == mark.thread_id) else { continue };
        if !crate::stats::inside(&t.cwd, root) {
            continue;
        }
        let d = day(&mut out, date_of(mark.ts, tz));
        if !d.threads.iter().any(|r| r.id == t.id) {
            d.threads.push(DayRef { id: t.id.clone(), title: t.title.clone() });
        }
    }
    Ok(out.into_values().rev().collect())
}

/// About 200 characters of `text` around the first match, on one line.
pub(crate) fn around(text: &str, needle: &str) -> String {
    let flat: Vec<char> = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    let lower: String = flat.iter().collect::<String>().to_lowercase();
    // Lowercasing can change lengths outside ASCII, so find the match by characters.
    let at = lower.find(needle).map(|b| lower[..b].chars().count()).unwrap_or(0).min(flat.len());
    let start = at.saturating_sub(80);
    let end = (start + 200).min(flat.len());
    let mut out: String = flat[start..end].iter().collect();
    if start > 0 {
        out.insert(0, '…');
    }
    if end < flat.len() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippets_centre_on_the_match_on_one_line() {
        assert_eq!(around("fix the\n  port leak", "port"), "fix the port leak");
        let long = format!("{} needle {}", "a ".repeat(100), "b ".repeat(200));
        let s = around(&long, "needle");
        assert!(s.starts_with('…') && s.ends_with('…') && s.contains("needle"), "{s}");
        assert!(s.chars().count() <= 202);
        assert_eq!(around("Åsa och ÖL", "öl"), "Åsa och ÖL");
    }
}
