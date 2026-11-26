//! "Rising" on the Plugins page, counted from stargazers: reading the newest stargazer pages
//! through `gh`, how many pages reach back a week, the stars of the last 7 days (counted, or
//! projected from the rate), and the curve a repository's card draws.

use super::*;

/// When the newest stars were given, newest first: the last stargazer pages back to `since` or
/// at most `pages` pages. None when GitHub will not list them (over 40 000 stars, or an error).
pub(super) async fn recent_stars(full: &str, stars: u64, since: i64, pages: u64) -> Option<Vec<i64>> {
    if stars == 0 || pages == 0 {
        return Some(Vec::new());
    }
    let last = stars.div_ceil(100);
    if last > MAX_STAR_PAGE {
        return None;
    }
    let mut page = last;
    let mut out = star_page(full, page).await?;
    // The count GitHub gives can run ahead of the list; an empty last page is no answer.
    if out.is_empty() && page > 1 {
        page -= 1;
        out = star_page(full, page).await?;
    }
    let left = pages.saturating_sub(last - page + 1).min(page - 1);
    let reached = out.last().is_some_and(|t| *t < since);
    if !reached && left > 0 && out.len() >= 2 {
        // How far back one page reaches says how many more reach `since`; read those at once.
        let n = pages_to_reach(&out, since, left);
        let more = futures::future::join_all((page - n..page).rev().map(|p| star_page(full, p))).await;
        for m in more {
            out.extend(m?);
        }
        out.sort_unstable_by(|a, b| b.cmp(a));
    }
    Some(out)
}

/// How many more stargazer pages, at most `left`, reach back to `since`, judged by how long the
/// stars already read (newest first) took to come in, plus one to be safe.
pub(super) fn pages_to_reach(read: &[i64], since: i64, left: u64) -> u64 {
    let (Some(&newest), Some(&oldest)) = (read.first(), read.last()) else { return left };
    if newest <= oldest || oldest <= since {
        return left.min(1);
    }
    let per_ms = read.len() as f64 / (newest - oldest) as f64;
    (((oldest - since) as f64 * per_ms / 100.0).ceil() as u64 + 1).clamp(1, left.max(1)).min(left)
}

/// One page of a repository's stargazers, as when each star was given, newest first.
pub(super) async fn star_page(full: &str, page: u64) -> Option<Vec<i64>> {
    let _slot = STAR_SLOTS.acquire().await.ok()?;
    let path = format!("repos/{full}/stargazers?per_page=100&page={page}");
    let list = gh_json(&["api", "-H", "Accept: application/vnd.github.star+json", &path], 15).await.ok()?;
    let mut times: Vec<i64> = list.as_array()?.iter().filter_map(|s| s["starred_at"].as_str().and_then(parse_time)).collect();
    times.sort_unstable_by(|a, b| b.cmp(a));
    Some(times)
}

/// A running total of the sample's stars in `buckets` equal steps, oldest first, and the time it
/// spans: `max_span` when the sample reaches back that far (or holds every star), otherwise back
/// to its oldest star. Busy repositories get a shorter line rather than none.
pub fn curve(times: &[i64], now: i64, max_span: i64, buckets: usize, complete: bool) -> (Vec<u32>, i64) {
    let reach = times.last().map(|oldest| now - oldest).unwrap_or(0);
    let span = if complete || reach >= max_span { max_span } else { reach };
    if span <= 0 || buckets == 0 || times.is_empty() {
        return (Vec::new(), 0);
    }
    let start = now - span;
    let mut counts = vec![0u32; buckets];
    for t in times.iter().filter(|t| **t >= start && **t <= now) {
        let i = (((t - start) as f64 / span as f64) * buckets as f64) as usize;
        counts[i.min(buckets - 1)] += 1;
    }
    let mut sum = 0;
    (counts.into_iter().map(|c| {
        sum += c;
        sum
    }).collect(), span)
}

/// Stars of the last 7 days: counted when the sample reaches back that far, otherwise
/// projected from its rate. The second value says it was projected.
pub fn week_of(times: &[i64], now: i64, complete: bool) -> Option<(u64, bool)> {
    let since = now - 7 * DAY;
    if complete || times.last().is_some_and(|t| *t < since) {
        return Some((times.iter().filter(|t| **t >= since).count() as u64, false));
    }
    let oldest = *times.last()?;
    let span = (now - oldest).max(60 * 60 * 1000);
    Some(((times.len() as f64 * (7 * DAY) as f64 / span as f64).round() as u64, true))
}

/// Fills in a repository's last week from its stargazers, reading at most `pages` pages.
pub(super) async fn add_trend(repo: &mut PluginRepo, now: i64, pages: u64) {
    let Some(times) = recent_stars(&repo.full_name, repo.stars, now - 14 * DAY, pages).await else { return };
    let complete = times.len() as u64 >= repo.stars;
    if let Some((week, estimated)) = week_of(&times, now, complete) {
        repo.week = Some(week);
        repo.week_estimated = estimated;
    }
    (repo.spark, repo.spark_span) = curve(&times, now, 14 * DAY, 14, complete);
}
