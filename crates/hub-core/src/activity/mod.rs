//! The activity map: how much you worked each day, read from the event log and from git.
//! Days are counted in the asker's time zone, given as minutes east of UTC.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

pub mod claude_code;
pub mod github;

use crate::git;
use crate::model::{ActivityDay, ActivityDetail, DayCommit, DayCount, DayPull, DayRepo, DaySession, DayThread, LinesDay, LongThread, Thread};
/// Reads the engines' own transcripts of work done outside Hub into the store.
pub use claude_code::{CcMark, CcSession, import as import_outside};
pub use crate::model::rows::{Mark};

const DAY_MS: i64 = 86_400_000;


/// `YYYY-MM-DD` of a moment, in a time zone `tz` minutes east of UTC.
pub fn date_of(ts: i64, tz: i32) -> String {
    let (y, m, d) = civil((ts + tz as i64 * 60_000).div_euclid(DAY_MS));
    format!("{y:04}-{m:02}-{d:02}")
}

/// The first moment of a `YYYY-MM-DD` day in a time zone `tz` minutes east of UTC.
pub fn start_of(date: &str, tz: i32) -> Option<i64> {
    let mut parts = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d) * DAY_MS - tz as i64 * 60_000)
}

/// What the outside sources add to the log and the local commits.
pub struct Outside<'a> {
    pub transcripts: &'a [CcMark],
    /// Claude Code sessions that are Hub threads: their prompts are already counted as messages.
    pub hub_sessions: &'a HashSet<String>,
    pub github: Option<&'a github::Github>,
}

/// Counts per day; days where nothing happened are left out.
pub fn days(marks: &[Mark], commits: &[DayCommit], outside: &Outside, tz: i32) -> Vec<ActivityDay> {
    let mut days: BTreeMap<String, (ActivityDay, HashSet<&str>)> = BTreeMap::new();
    fn day<'a, 'm>(days: &'a mut BTreeMap<String, (ActivityDay, HashSet<&'m str>)>, date: String) -> &'a mut (ActivityDay, HashSet<&'m str>) {
        days.entry(date.clone()).or_insert_with(|| (ActivityDay { date, ..Default::default() }, HashSet::new()))
    }
    for m in marks {
        let (d, threads) = day(&mut days, date_of(m.ts, tz));
        threads.insert(&m.thread_id);
        if m.message.is_some() {
            d.messages += 1;
        } else {
            d.turns += 1;
        }
    }
    for c in commits {
        day(&mut days, date_of(c.ts, tz)).0.commits += 1;
    }
    let mut sessions: HashSet<(String, &str)> = HashSet::new();
    for m in outside.transcripts {
        let date = date_of(m.ts, tz);
        let outside_hub = !outside.hub_sessions.contains(&m.session_id);
        if m.prompt.is_some() && outside_hub {
            sessions.insert((date.clone(), &m.session_id));
        }
        let (d, _) = day(&mut days, date);
        d.tokens += m.tokens;
        if m.prompt.is_some() && outside_hub {
            d.prompts += 1;
        }
    }
    for (date, _) in sessions {
        day(&mut days, date).0.sessions += 1;
    }
    for (date, n) in outside.github.map(|g| &g.days).into_iter().flatten() {
        day(&mut days, date.clone()).0.github += n;
    }
    for (date, _, n) in outside.github.map(|g| &g.commits).into_iter().flatten() {
        day(&mut days, date.clone()).0.github_commits += n;
    }
    days.into_values()
        .map(|(mut d, threads)| {
            d.threads = threads.len() as u32;
            d
        })
        .collect()
}

/// What happened in each thread on one day, busiest first. `marks` are that day's only.
pub fn day_threads(marks: &[Mark], threads: &[Thread]) -> Vec<DayThread> {
    let mut out: Vec<DayThread> = Vec::new();
    for m in marks {
        let i = match out.iter().position(|t| t.id == m.thread_id) {
            Some(i) => i,
            None => {
                let t = threads.iter().find(|t| t.id == m.thread_id);
                out.push(DayThread {
                    id: m.thread_id.clone(),
                    title: t.map(|t| t.title.clone()).unwrap_or_else(|| "Deleted thread".into()),
                    cwd: t.map(|t| t.cwd.clone()).unwrap_or_default(),
                    messages: Vec::new(),
                    turns: 0,
                    tokens: 0,
                });
                out.len() - 1
            }
        };
        match &m.message {
            Some(text) => out[i].messages.push(first_line(text)),
            None => out[i].turns += 1,
        }
    }
    out.sort_by_key(|t| std::cmp::Reverse(t.messages.len() as u32 + t.turns));
    out
}

/// One day's Claude Code work: tokens go to the Hub thread a session belongs to, and the other
/// sessions are listed with their prompts, busiest first. `marks` are that day's only.
pub fn day_sessions(marks: &[CcMark], sessions: &[CcSession], threads: &mut [DayThread], all: &[Thread]) -> Vec<DaySession> {
    let mut out: Vec<DaySession> = Vec::new();
    for m in marks {
        if let Some(t) = all.iter().find(|t| t.engine_session_id.as_deref() == Some(m.session_id.as_str())) {
            if let Some(day) = threads.iter_mut().find(|d| d.id == t.id) {
                day.tokens += m.tokens;
            }
            continue;
        }
        let i = match out.iter().position(|s| s.id == m.session_id) {
            Some(i) => i,
            None => {
                let s = sessions.iter().find(|s| s.id == m.session_id);
                out.push(DaySession {
                    id: m.session_id.clone(),
                    title: s.and_then(|s| s.title.clone()),
                    cwd: s.and_then(|s| s.cwd.clone()),
                    prompts: Vec::new(),
                    tokens: 0,
                });
                out.len() - 1
            }
        };
        out[i].tokens += m.tokens;
        if let Some(p) = &m.prompt {
            out[i].prompts.push(p.clone());
        }
    }
    // A session that only finished an answer after midnight is not worth a row.
    out.retain(|s| !s.prompts.is_empty());
    out.sort_by_key(|s| std::cmp::Reverse(s.prompts.len()));
    out
}

/// One day of GitHub: commits per repository and the pull requests opened.
pub fn day_github(g: &github::Github, date: &str) -> (Vec<DayRepo>, Vec<DayPull>) {
    let mut repos: Vec<DayRepo> = Vec::new();
    for (d, repo, n) in &g.commits {
        if d != date {
            continue;
        }
        match repos.iter_mut().find(|r| &r.repo == repo) {
            Some(r) => r.commits += n,
            None => repos.push(DayRepo { repo: repo.clone(), commits: *n }),
        }
    }
    repos.sort_by_key(|r| std::cmp::Reverse(r.commits));
    let pulls = g
        .pulls
        .iter()
        .filter(|p| p.0 == date)
        .map(|(_, repo, title, url)| DayPull { repo: repo.clone(), title: title.clone(), url: url.clone() })
        .collect();
    (repos, pulls)
}

/// `since` (ms) as the seconds git's `--since=@N` should get. git reads a number below
/// 100000000 as a day of the month, not as the epoch: `@1` on the 1st meant "from right now"
/// and hid every commit, so anything earlier than 1973 is asked as 1973.
fn since_secs(since: i64) -> i64 {
    (since / 1000).max(100_000_000)
}

/// Author emails of coding agents. Their commits are yours: they made them for you.
const AGENTS: &[&str] = crate::engine_home::AGENT_AUTHORS;

/// Your commits since `since` (ms) in the repositories holding these folders, newest first.
/// Yours means written under your git email or name, or by a coding agent. Worktrees share
/// commits and a rebase copies them, so a commit with the same author time and subject is
/// listed once.
pub fn commits(folders: &BTreeSet<String>, since: i64) -> Vec<DayCommit> {
    let mut repos = BTreeSet::new();
    for folder in folders {
        let path = Path::new(folder);
        if path.is_dir()
            && let Some(top) = git::run(path, &["rev-parse", "--show-toplevel"]) {
                repos.insert(top.trim().to_owned());
            }
    }
    let since_arg = format!("--since=@{}", since_secs(since));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for repo in repos {
        let path = Path::new(&repo);
        let config = |key: &str| git::run(path, &["config", key]).map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty());
        let (my_email, my_name) = (config("user.email"), config("user.name"));
        let Some(log) = git::run(
            path,
            &["log", "--all", "--no-merges", &since_arg, "--format=%x1e%H%x1f%at%x1f%ae%x1f%an%x1f%s"],
        ) else {
            continue;
        };
        for entry in log.split('\x1e').filter(|e| !e.trim().is_empty()) {
            let Some(head) = entry.lines().next() else { continue };
            let mut f = head.splitn(5, '\x1f');
            let (Some(hash), Some(at), Some(email), Some(name), Some(subject)) =
                (f.next(), f.next(), f.next(), f.next(), f.next())
            else {
                continue;
            };
            let email = email.to_lowercase();
            let mine = (my_email.is_none() && my_name.is_none())
                || my_email.as_deref() == Some(email.as_str())
                || my_name.as_deref() == Some(name.trim().to_lowercase().as_str())
                || AGENTS.contains(&email.as_str());
            if !mine {
                continue;
            }
            let Ok(at) = at.parse::<i64>() else { continue };
            // git filters by commit time; the map counts when the work was written.
            if at * 1000 < since || !seen.insert((at, subject.to_owned())) {
                continue;
            }
            out.push(DayCommit { repo: repo.clone(), hash: hash.to_owned(), subject: subject.to_owned(), ts: at * 1000, added: 0, removed: 0 });
        }
    }
    out.sort_by_key(|c| std::cmp::Reverse(c.ts));
    out
}

/// Lines added and removed from `git log --numstat` lines. Lock files are written by tools and
/// binaries have no lines, so neither counts.
fn numstat<'a>(lines: impl Iterator<Item = &'a str>) -> (u32, u32) {
    const LOCKS: &[&str] = &["package-lock.json", "pnpm-lock.yaml", "yarn.lock", "Cargo.lock", "bun.lockb", "poetry.lock"];
    let (mut added, mut removed) = (0u32, 0u32);
    for line in lines {
        let mut f = line.splitn(3, '\t');
        let (Some(a), Some(r), Some(file)) = (f.next(), f.next(), f.next()) else { continue };
        let name = file.rsplit(['/', '\\']).next().unwrap_or(file).trim_end_matches('}');
        if LOCKS.contains(&name) || name.ends_with(".lock") {
            continue;
        }
        added = added.saturating_add(a.parse().unwrap_or(0));
        removed = removed.saturating_add(r.parse().unwrap_or(0));
    }
    (added, removed)
}

/// Fills in the lines each commit added and removed. Diffing is the slow part of reading git
/// (minutes for a few thousand commits on Windows), and a commit never changes, so what is
/// already in `known` (by hash) is not diffed again; new commits are diffed in batches and added.
pub fn count_lines(commits: &mut [DayCommit], known: &mut HashMap<String, (u32, u32)>) {
    let mut wanted: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for c in commits.iter().filter(|c| !known.contains_key(&c.hash)) {
        wanted.entry(&c.repo).or_default().push(&c.hash);
    }
    let mut found = Vec::new();
    for (repo, hashes) in wanted {
        for batch in hashes.chunks(200) {
            let mut args = vec!["show", "--numstat", "--no-renames", "--format=%x1e%H"];
            args.extend(batch.iter().copied());
            let Some(out) = git::run(Path::new(repo), &args) else { continue };
            for entry in out.split('\x1e').filter(|e| !e.trim().is_empty()) {
                let mut lines = entry.lines();
                let Some(hash) = lines.next().map(str::trim) else { continue };
                found.push((hash.to_owned(), numstat(lines)));
            }
        }
    }
    known.extend(found);
    for c in commits.iter_mut() {
        if let Some(&(a, r)) = known.get(&c.hash) {
            (c.added, c.removed) = (a, r);
        }
    }
}

/// Lines added and removed per day, from commits, oldest first; days without commits are left out.
pub fn lines(commits: &[DayCommit], tz: i32) -> Vec<LinesDay> {
    let mut days: BTreeMap<String, LinesDay> = BTreeMap::new();
    for c in commits {
        let date = date_of(c.ts, tz);
        let d = days.entry(date.clone()).or_insert_with(|| LinesDay { date, ..Default::default() });
        d.added = d.added.saturating_add(c.added);
        d.removed = d.removed.saturating_add(c.removed);
        d.commits += 1;
    }
    days.into_values().collect()
}

/// The thread with the most finished turns among `marks`.
pub fn longest_thread(marks: &[Mark], threads: &[Thread]) -> Option<LongThread> {
    let mut turns: BTreeMap<&str, u32> = BTreeMap::new();
    for m in marks.iter().filter(|m| m.message.is_none()) {
        *turns.entry(&m.thread_id).or_default() += 1;
    }
    let (id, turns) = turns.into_iter().max_by_key(|(id, n)| (*n, std::cmp::Reverse(*id)))?;
    let title = threads.iter().find(|t| t.id == id).map(|t| t.title.clone()).unwrap_or_else(|| "Deleted thread".into());
    Some(LongThread { id: id.to_owned(), title, turns })
}

/// Pull requests opened per day, from GitHub.
pub fn pull_days(g: &github::Github) -> Vec<DayCount> {
    let mut days: BTreeMap<&str, u32> = BTreeMap::new();
    for (date, ..) in &g.pulls {
        *days.entry(date).or_default() += 1;
    }
    days.into_iter().map(|(date, count)| DayCount { date: date.to_owned(), count }).collect()
}

/// The day written out for a model to summarise.
pub fn describe(detail: &ActivityDetail) -> String {
    let mut text = format!("Day: {}\n", detail.date);
    for t in &detail.threads {
        text.push_str(&format!("\nThread \"{}\" in {} ({} agent turns)\n", t.title, t.cwd, t.turns));
        for m in t.messages.iter().take(15) {
            text.push_str(&format!("- you: {m}\n"));
        }
    }
    for s in &detail.sessions {
        let title = s.title.as_deref().unwrap_or("Claude Code session");
        text.push_str(&format!("\nClaude Code session \"{title}\" in {}\n", s.cwd.as_deref().unwrap_or("?")));
        for p in s.prompts.iter().take(15) {
            text.push_str(&format!("- you: {p}\n"));
        }
    }
    for p in &detail.pulls {
        text.push_str(&format!("\nOpened pull request in {}: {}\n", p.repo, p.title));
    }
    if !detail.commits.is_empty() {
        text.push_str("\nCommits:\n");
        for c in detail.commits.iter().take(60) {
            let repo = Path::new(&c.repo).file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
            text.push_str(&format!("- {repo}: {}\n", c.subject));
        }
    }
    text
}

impl ActivityDetail {
    pub fn is_empty(&self) -> bool {
        self.threads.is_empty() && self.commits.is_empty() && self.sessions.is_empty() && self.github.is_empty() && self.pulls.is_empty()
    }
}

pub(crate) fn first_line(text: &str) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or_default().trim();
    if line.chars().count() > 160 { format!("{}…", line.chars().take(160).collect::<String>()) } else { line.to_owned() }
}

/// Days since 1970-01-01 to (year, month, day). From Howard Hinnant's date algorithms.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(thread: &str, ts: i64, message: Option<&str>) -> Mark {
        Mark { thread_id: thread.into(), ts, message: message.map(Into::into) }
    }

    #[test]
    fn dates_follow_the_time_zone() {
        // 2026-10-29 23:30 UTC is already the 30th in Stockholm (UTC+1 in winter).
        let ts = start_of("2026-10-29", 0).unwrap() + 23 * 3_600_000 + 30 * 60_000;
        assert_eq!(date_of(ts, 0), "2026-10-29");
        assert_eq!(date_of(ts, 60), "2026-10-30");
        assert_eq!(date_of(start_of("2026-10-30", 60).unwrap(), 60), "2026-10-30");
        assert_eq!(date_of(start_of("2026-10-30", 60).unwrap() - 1, 60), "2026-10-29");
        assert_eq!(date_of(start_of("2024-02-29", -300).unwrap(), -300), "2024-02-29");
        assert_eq!(start_of("2026-13-01", 0), None);
        assert_eq!(start_of("nonsense", 0), None);
    }

    #[test]
    fn days_count_messages_turns_threads_and_commits() {
        let d1 = start_of("2026-10-28", 0).unwrap();
        let d2 = d1 + DAY_MS;
        let marks = [
            mark("a", d1 + 10, Some("fix the port bug")),
            mark("a", d1 + 20, None),
            mark("b", d1 + 30, Some("hello")),
            mark("b", d2 + 5, None),
        ];
        let commits = [DayCommit { repo: "/r".into(), hash: "h".into(), subject: "s".into(), ts: d2 + 9, added: 0, removed: 0 }];
        let cc = [
            CcMark { key: "u".into(), session_id: "cli".into(), ts: d2 + 1, prompt: Some("p".into()), tokens: 0, ..Default::default() },
            CcMark { key: "m".into(), session_id: "cli".into(), ts: d2 + 2, prompt: None, tokens: 50, ..Default::default() },
            CcMark { key: "h".into(), session_id: "hub".into(), ts: d2 + 3, prompt: Some("in hub".into()), tokens: 7, ..Default::default() },
        ];
        let hub_sessions = HashSet::from(["hub".to_owned()]);
        let gh = github::Github {
            days: BTreeMap::from([("2026-10-28".to_owned(), 4)]),
            commits: vec![("2026-10-28".to_owned(), "me/r".to_owned(), 3)],
            ..Default::default()
        };
        let outside = Outside { transcripts: &cc, hub_sessions: &hub_sessions, github: Some(&gh) };
        let days = days(&marks, &commits, &outside, 0);
        assert_eq!(days.len(), 2);
        assert_eq!((days[0].date.as_str(), days[0].threads, days[0].messages, days[0].turns, days[0].commits), ("2026-10-28", 2, 2, 1, 0));
        assert_eq!((days[1].date.as_str(), days[1].threads, days[1].messages, days[1].turns, days[1].commits), ("2026-10-29", 1, 0, 1, 1));
        assert_eq!((days[0].github, days[1].github), (4, 0));
        assert_eq!(days[0].github_commits, 3, "pull requests and reviews are not commits");
        assert_eq!((days[1].prompts, days[1].sessions, days[1].tokens), (1, 1, 57));
    }

    #[test]
    fn day_threads_are_busiest_first_with_first_lines() {
        let marks = [mark("a", 1, Some("one")), mark("b", 2, Some("\n  two\nmore")), mark("b", 3, None), mark("b", 4, None)];
        let threads = day_threads(&marks, &[]);
        assert_eq!(threads[0].id, "b");
        assert_eq!(threads[0].messages, vec!["two"]);
        assert_eq!(threads[0].turns, 2);
        assert_eq!(threads[0].title, "Deleted thread");
        assert_eq!(threads[1].messages, vec!["one"]);
    }

    #[test]
    fn since_is_always_read_as_a_moment_not_a_day() {
        assert_eq!(since_secs(0), 100_000_000);
        assert_eq!(since_secs(1_000), 100_000_000);
        assert_eq!(since_secs(1_793_491_740_000), 1_793_491_740);
    }

    #[test]
    fn commits_are_read_from_git_once_per_repository() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let git = |args: &[&str]| assert!(git::run(repo, args).is_some(), "git {args:?}");
        git(&["init", "-q"]);
        git(&["config", "user.email", "me@example.com"]);
        git(&["config", "user.name", "Me"]);
        git(&["commit", "-q", "--allow-empty", "-m", "first"]);
        git(&["-c", "user.email=other@example.com", "-c", "user.name=Other", "commit", "-q", "--allow-empty", "-m", "theirs"]);
        // Under a different email but your name, and by an agent: both yours.
        git(&["-c", "user.email=me@users.noreply.github.com", "commit", "-q", "--allow-empty", "-m", "from github"]);
        git(&["-c", "user.email=noreply@anthropic.com", "-c", "user.name=Claude", "commit", "-q", "--allow-empty", "-m", "by claude"]);
        // A copy, as a rebase or cherry-pick makes: same author time and subject.
        git(&["commit", "-q", "--allow-empty", "-C", "HEAD~3"]);
        // Every coding agent's author email counts.
        for (email, subject) in [("claude@anthropic.com", "by claude too"), ("noreply@openai.com", "by codex"), ("codex@openai.com", "by codex too")] {
            git(&["-c", &format!("user.email={email}"), "-c", "user.name=Agent", "commit", "-q", "--allow-empty", "-m", subject]);
        }
        std::fs::create_dir(repo.join("sub")).unwrap();
        let folders = BTreeSet::from([repo.display().to_string(), repo.join("sub").display().to_string()]);
        let found = commits(&folders, 0);
        let mut subjects: Vec<_> = found.iter().map(|c| c.subject.as_str()).collect();
        subjects.sort();
        assert_eq!(subjects, vec!["by claude", "by claude too", "by codex", "by codex too", "first", "from github"], "{found:?}");
    }

    #[test]
    fn commits_count_lines_without_lock_files() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let git = |args: &[&str]| assert!(git::run(repo, args).is_some(), "git {args:?}");
        git(&["init", "-q"]);
        git(&["config", "user.email", "me@example.com"]);
        git(&["config", "user.name", "Me"]);
        std::fs::write(repo.join("a.rs"), "1\n2\n3\n").unwrap();
        std::fs::write(repo.join("Cargo.lock"), "x\ny\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "add"]);
        std::fs::write(repo.join("a.rs"), "1\n").unwrap();
        git(&["commit", "-q", "-am", "trim"]);
        let mut found = commits(&BTreeSet::from([repo.display().to_string()]), 0);
        assert!(found.iter().all(|c| c.added == 0 && c.removed == 0), "reading the log does not diff");
        let mut known = HashMap::new();
        count_lines(&mut found, &mut known);
        assert_eq!(known.len(), 2);
        let by = |s: &str| found.iter().find(|c| c.subject == s).map(|c| (c.added, c.removed));
        assert_eq!(by("add"), Some((3, 0)));
        assert_eq!(by("trim"), Some((0, 2)));
        let days = lines(&found, 0);
        assert_eq!(days.len(), 1);
        assert_eq!((days[0].added, days[0].removed, days[0].commits), (3, 2, 2));
        // Known commits are taken from the cache, not diffed again.
        let first = found[0].hash.clone();
        known.insert(first.clone(), (7, 7));
        count_lines(&mut found, &mut known);
        assert_eq!(found.iter().find(|c| c.hash == first).map(|c| (c.added, c.removed)), Some((7, 7)));
    }

    #[test]
    fn longest_thread_counts_finished_turns() {
        let marks = [mark("a", 1, Some("hi")), mark("a", 2, None), mark("b", 3, None), mark("b", 4, None), mark("b", 5, Some("x"))];
        let t = longest_thread(&marks, &[]).unwrap();
        assert_eq!((t.id.as_str(), t.turns), ("b", 2));
        assert!(longest_thread(&[mark("a", 1, Some("hi"))], &[]).is_none());
    }
}
