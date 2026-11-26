//! Reports: the activity map, pulse, lines, stats, the day summary and second-opinion reviews.

use crate::locked::ResultExt;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde_json::json;

use super::*;

impl Hub {
    /// Folders whose repositories count towards the activity map: every thread's and project's,
    /// every folder Claude Code was used in, and the folders in the projects root (D:\Dev), so
    /// repositories Hub never opened count too.
    pub(crate) async fn activity_folders(&self) -> Result<std::collections::BTreeSet<String>> {
        let mut folders: std::collections::BTreeSet<String> =
            self.store.threads().await?.into_iter().filter(|t| t.host.is_none()).map(|t| t.cwd).collect();
        folders.extend(self.store.projects().await?.into_iter().map(|p| p.path));
        folders.extend(self.store.cc_sessions().await?.into_iter().filter_map(|s| s.cwd));
        let root = self.projects_root().await?;
        if let Ok(entries) = std::fs::read_dir(&root) {
            folders.extend(entries.flatten().filter(|e| e.path().is_dir()).map(|e| e.path().display().to_string()));
        }
        Ok(folders)
    }

    /// Each day of the last `days` that something happened, in a time zone `tz` minutes east
    /// of UTC, from Hub's log, local git, Claude Code and GitHub.
    pub async fn activity(&self, days: u32, tz: i32) -> Result<crate::model::ActivityReport> {
        // The devlog writes days as they end where you are.
        self.store.set_value(crate::devlog::TZ, &tz.to_string()).await?;
        let today = crate::activity::date_of(now_ms(), tz);
        let to = crate::activity::start_of(&today, tz).context("bad date")? + 86_400_000;
        let from = to - days.clamp(1, 800) as i64 * 86_400_000;
        let mut notes = Vec::new();
        if let Err(e) = crate::activity::import_outside(&self.store).await {
            notes.push(format!("{e:#}"));
        }
        // GitHub answers for a year at most.
        let github_from = crate::activity::date_of(from.max(to - 365 * 86_400_000), tz);
        let github = match self.github(&github_from).await {
            Ok(g) => Some(g),
            Err(e) => {
                notes.push(e);
                None
            }
        };
        let marks = self.store.activity(from, to).await?;
        let transcripts = self.store.cc_marks(from, to).await?;
        let hub_sessions = self.hub_sessions().await?;
        let folders = self.activity_folders().await?;
        let commits = tokio::task::spawn_blocking(move || crate::activity::commits(&folders, from)).await?;
        let outside = crate::activity::Outside { transcripts: &transcripts, hub_sessions: &hub_sessions, github: github.as_deref() };
        Ok(crate::model::ActivityReport { days: crate::activity::days(&marks, &commits, &outside, tz), notes })
    }

    /// The overview's playful cards over the last `days`: when you wrote, pull requests per day
    /// and the longest thread. Read from the database only, so it answers at once.
    pub async fn pulse(&self, days: u32, tz: i32) -> Result<crate::model::Pulse> {
        let today = crate::activity::date_of(now_ms(), tz);
        let to = crate::activity::start_of(&today, tz).context("bad date")? + 86_400_000;
        let from = to - days.clamp(1, 800) as i64 * 86_400_000;
        let mut notes = Vec::new();
        if let Err(e) = crate::activity::import_outside(&self.store).await {
            notes.push(format!("{e:#}"));
        }
        let marks = self.store.activity(from, to).await?;
        let hub_sessions = self.hub_sessions().await?;
        let mut moments: Vec<i64> = marks.iter().filter(|m| m.message.is_some()).map(|m| m.ts).collect();
        moments.extend(
            self.store
                .cc_marks(from, to)
                .await?
                .into_iter()
                .filter(|m| m.prompt.is_some() && !hub_sessions.contains(&m.session_id))
                .map(|m| m.ts),
        );
        moments.sort_unstable();
        // Only GitHub's cached answer (the activity map asks for it): the Pulse should not wait
        // on the network.
        let cached = self.github.lock().await.as_ref().map(|(_, g)| g.clone());
        let pulls = cached.map(|g| crate::activity::pull_days(&g)).unwrap_or_default();
        Ok(crate::model::Pulse {
            moments,
            pulls,
            longest_thread: crate::activity::longest_thread(&marks, &self.store.threads().await?),
            notes,
        })
    }

    /// Lines your commits added and removed per day over the last `days`. Each commit is diffed
    /// once ever: the counts are kept by hash in `kv` (`activity.lines`), so only new commits
    /// cost anything.
    pub async fn lines(&self, days: u32, tz: i32) -> Result<crate::model::LinesReport> {
        let today = crate::activity::date_of(now_ms(), tz);
        let to = crate::activity::start_of(&today, tz).context("bad date")? + 86_400_000;
        let from = to - days.clamp(1, 800) as i64 * 86_400_000;
        let folders = self.activity_folders().await?;
        // One count at a time: two cards asking at once should not diff the same commits twice.
        let mut cache = self.lines_cache.lock().await;
        if cache.is_none() {
            let saved = self.store.get_value(LINES_CACHE).await?;
            *cache = Some(saved.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default());
        }
        let mut known = cache.take().unwrap_or_default();
        let before = known.len();
        let (commits, known) = tokio::task::spawn_blocking(move || {
            let mut commits = crate::activity::commits(&folders, from);
            crate::activity::count_lines(&mut commits, &mut known);
            (commits, known)
        })
        .await?;
        let diffed = (known.len() - before) as u32;
        if diffed > 0 {
            self.store.set_value(LINES_CACHE, &serde_json::to_string(&known)?).await?;
        }
        *cache = Some(known);
        Ok(crate::model::LinesReport { days: crate::activity::lines(&commits, tz), diffed, commits: commits.len() as u32 })
    }

    /// One day in detail: the threads you worked in, Claude Code sessions, your commits and
    /// GitHub, and its summary if written.
    pub async fn activity_day(&self, date: &str, tz: i32) -> Result<crate::model::ActivityDetail> {
        let from = crate::activity::start_of(date, tz).context("a date looks like 2026-10-30")?;
        let to = from + 86_400_000;
        crate::activity::import_outside(&self.store).await.warn_on_err("import the activity log");
        let marks = self.store.activity(from, to).await?;
        let threads = self.store.threads().await?;
        let folders = self.activity_folders().await?;
        let mut commits = tokio::task::spawn_blocking(move || crate::activity::commits(&folders, from)).await?;
        commits.retain(|c| c.ts < to);
        let mut day_threads = crate::activity::day_threads(&marks, &threads);
        let cc = self.store.cc_marks(from, to).await?;
        let sessions = crate::activity::day_sessions(&cc, &self.store.cc_sessions().await?, &mut day_threads, &threads);
        // Only GitHub's cached answer: a day's detail should not wait on the network.
        let cached = self.github.lock().await.as_ref().map(|(_, g)| g.clone());
        let (github, pulls) = cached.map(|g| crate::activity::day_github(&g, date)).unwrap_or_default();
        Ok(crate::model::ActivityDetail {
            date: date.to_owned(),
            threads: day_threads,
            commits,
            sessions,
            github,
            pulls,
            summary: self.store.get_value(&format!("{}{date}", crate::devlog::SUMMARY)).await?,
        })
    }

    /// Models, tokens and cost of the last `days`, in a time zone `tz` minutes east of UTC; of
    /// one project's folder when `project` is given.
    pub async fn stats(&self, days: u32, tz: i32, project: Option<&str>) -> Result<crate::model::Stats> {
        let days = days.clamp(1, 800);
        let today = crate::activity::date_of(now_ms(), tz);
        let to = crate::activity::start_of(&today, tz).context("bad date")? + 86_400_000;
        let from = to - days as i64 * 86_400_000;
        crate::activity::import_outside(&self.store).await.warn_on_err("import the activity log");
        let sessions = self.store.cc_sessions().await?;
        let mut marks = self.store.cc_marks(from, to).await?;
        if let Some(root) = project {
            marks = crate::stats::within(marks, &sessions, root);
        }
        // A thread's folder is a project too, even when it was never added as one.
        let mut projects = self.store.projects().await?;
        let threads = self.store.threads().await?;
        for t in &threads {
            if !projects.iter().any(|p| p.path == t.cwd) {
                let name = std::path::Path::new(&t.cwd).file_name().map(|n| n.to_string_lossy().into_owned());
                projects.push(crate::model::Project { name: name.unwrap_or_else(|| t.cwd.clone()), path: t.cwd.clone(), created_at: 0 });
            }
        }
        let prices = self.all_prices();
        let mut stats = crate::stats::stats(&marks, &sessions, &projects, days, &prices);
        let outcomes = self.store.outcomes(from, to).await?;
        let mut handed = self.store.handed(from, to).await?;
        if let Some(root) = project {
            handed.retain(|h| threads.iter().any(|t| t.id == h.thread_id && crate::stats::inside(&t.cwd, root)));
        }
        crate::stats::results(&mut stats, &marks, &outcomes, &handed, &threads, &self.store.thread_sessions().await?, &prices);
        Ok(stats)
    }

    /// Every engine's prices: answers are read from logs, not from one engine's threads.
    pub(crate) fn all_prices(&self) -> Vec<crate::config::Price> {
        self.config.engines.keys().flat_map(|e| self.config.traits(e).prices.unwrap_or_default()).collect()
    }

    /// Claude Code session ids of Hub's threads.
    async fn hub_sessions(&self) -> Result<std::collections::HashSet<String>> {
        Ok(self.store.threads().await?.into_iter().filter_map(|t| t.engine_session_id).collect())
    }

    /// GitHub's year as last asked, without asking again.
    pub(crate) async fn cached_github(&self) -> Option<Arc<crate::activity::github::Github>> {
        self.github.lock().await.as_ref().map(|(_, g)| g.clone())
    }

    /// GitHub's year, asked at most every ten minutes.
    async fn github(&self, from: &str) -> std::result::Result<Arc<crate::activity::github::Github>, String> {
        let mut cached = self.github.lock().await;
        if let Some((at, g)) = cached.as_ref()
            && now_ms() - at < 10 * 60_000 {
                return Ok(g.clone());
            }
        let g = Arc::new(crate::activity::github::year(from).await?);
        *cached = Some((now_ms(), g.clone()));
        Ok(g)
    }

    /// The day's devblog entry: a one-line gist, then what shipped, what worked, what was
    /// blocked and what is next, written by the default engine and kept so opening the day is
    /// instant.
    pub async fn summarize_day(&self, date: &str, tz: i32) -> Result<String> {
        let detail = self.activity_day(date, tz).await?;
        if detail.is_empty() {
            bail!("nothing happened that day");
        }
        let mut day = crate::activity::describe(&detail);
        // What a day's devblog needs beyond the activity: versions that went out, and threads
        // that ended stuck.
        let from = crate::activity::start_of(date, tz).context("a date looks like 2026-10-30")?;
        let to = from + 86_400_000;
        for n in crate::notes::list(&self.store).await.unwrap_or_default().into_iter().filter(|n| n.created_at >= from && n.created_at < to && !n.text.trim().is_empty()) {
            let text: String = n.text.chars().take(1_500).collect();
            day.push_str(&format!("\nReleased Hub version {}:\n{}\n", n.version, text.trim()));
        }
        let threads = self.store.threads().await?;
        for t in &detail.threads {
            if let Some(live) = threads.iter().find(|x| x.id == t.id) {
                let state = match live.status {
                    ThreadStatus::Error => Some("failed"),
                    ThreadStatus::WaitingOnYou => Some("is waiting on the developer"),
                    _ => None,
                };
                if let Some(state) = state {
                    day.push_str(&format!("\nThread \"{}\" {state}.\n", t.title));
                }
            }
        }
        // A real devblog is longer than tidying's small model writes well: the default engine
        // and its own model, on the subscription.
        let engine = self
            .config
            .engines
            .get(&self.config.default_engine)
            .with_context(|| format!("unknown engine {:?}", self.config.default_engine))?;
        let cwd = std::env::temp_dir().display().to_string();
        let prompt = format!("{DAY_SUMMARY}\n\n<day>\n{day}</day>");
        let answer = engine::acp::complete("day_summary", engine, &cwd, None, prompt);
        let summary = tokio::time::timeout(std::time::Duration::from_secs(300), answer)
            .await
            .context("the report took too long")??;
        let summary = summary.trim().to_owned();
        if summary.is_empty() {
            bail!("the model wrote nothing");
        }
        self.store.set_value(&format!("{}{date}", crate::devlog::SUMMARY), &summary).await?;
        self.store.set_value(&format!("{}{date}", crate::devlog::BLOG), "1").await?;
        Ok(summary)
    }

    /// Asks another model for a second opinion on what the thread changed since the last review
    /// (or since checkpoint `from`, or since the thread started). A small model first cuts a
    /// large pile of changes into parts and says which need a look; each of those is reviewed in a
    /// throwaway session that can read but not change anything. Progress and the result are
    /// `review` events with one id. Unless everything looks good, the findings go back to the
    /// agent as a message, when `send_back`.
    pub async fn review(self: &Arc<Self>, thread_id: &str, request: ReviewRequest) -> Result<ReviewStarted> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let events = self
            .store
            .events_of(thread_id, 0, &[EventKind::Checkpoint, EventKind::Review, EventKind::SessionState, EventKind::Setting, EventKind::UserMessage])
            .await?;
        let upto = self.store.last_seq(thread_id, None).await?.unwrap_or_default();
        let (base, root, since) = review_base(&thread, &events, request.from.as_deref())?;
        let id = uuid::Uuid::now_v7().to_string();
        let keep = format!("{thread_id}/review-{id}");
        let (at, from) = (root.clone(), base.clone());
        let changes =
            tokio::task::spawn_blocking(move || crate::checkpoint::diff_since(&at, &from, &keep)).await??;
        if changes.files.is_empty() {
            bail!("nothing to review: no files changed since the last review");
        }
        let model = match request.model.filter(|m| !m.trim().is_empty()) {
            Some(m) => m,
            None => self.reviewer(&thread.engine, &events).await?,
        };
        let mut data = ReviewData {
            id: id.clone(),
            state: "running".into(),
            model: model.clone(),
            files: changes.files.iter().map(|f| f.path.clone()).collect(),
            base,
            head: Some(changes.head.clone()),
            seq: upto,
            by: if request.reason.is_some() { "agent" } else { "you" }.into(),
            reason: Some(request.reason),
            ..Default::default()
        };
        self.emit_typed(thread_id, &data).await?;

        // What the person asked for since then, minus earlier second opinions.
        let asked = events
            .iter()
            .filter(|e| e.kind == EventKind::UserMessage && e.seq > since && e.mark("review").is_null())
            .filter_map(|e| e.said())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        let engine = self.config.engines.get(&thread.engine).context("the thread's engine is not configured")?.clone();
        let splitter = self.config.tidy.model.clone();
        let send_back = request.send_back.unwrap_or(true);
        let (hub, thread_id, title) = (self.clone(), thread_id.to_owned(), thread.title.clone());
        let review_id = id.clone();
        let wanted = model.clone();
        tokio::spawn(async move {
            let cwd = root.display().to_string();
            data.chunks = split_review(&engine, &cwd, splitter.as_deref(), &changes.files, &asked).await;
            hub.emit_typed(&thread_id, &data).await.warn_on_err("emit Review");
            let parts = data.chunks.iter().filter(|c| c.review).count();
            let mut n = 0;
            for i in 0..data.chunks.len() {
                if !data.chunks[i].review {
                    continue;
                }
                n += 1;
                let names = &data.chunks[i].files;
                let diff: String = changes.files.iter().filter(|f| names.contains(&f.path)).map(|f| f.diff.as_str()).collect();
                let part = if parts > 1 {
                    format!("This is part {n} of {parts}, \"{}\"; the other parts are reviewed separately.\n\n", data.chunks[i].title)
                } else {
                    String::new()
                };
                let prompt = format!(
                    "{REVIEW}\n\n{part}Thread: {title}\n\n<request>\n{}\n</request>\n\n<diff>\n{}\n</diff>",
                    clip(&asked, 8_000),
                    clip(&diff, 150_000),
                );
                data.chunks[i].state = "running".into();
                hub.emit_typed(&thread_id, &data).await.warn_on_err("emit Review");
                run_chunk(&mut data.chunks[i], &engine, &cwd, &wanted, prompt).await;
            }

            finish_review(&mut data, parts, send_back);
            hub.emit_typed(&thread_id, &data).await.warn_on_err("emit Review");
            if data.sent == Some(true) {
                let findings = data.text.as_deref().unwrap_or_default();
                let message = format!("{SEND_BACK}\n\n{findings}");
                if let Err(e) = hub.send_from(&thread_id, message, Vec::new(), None, Some(("review", json!(review_id))), None).await {
                    tracing::warn!(thread = %thread_id, "could not send the review back: {e:#}");
                }
            }
        });
        Ok(ReviewStarted { id, model })
    }
}

/// Asks the reviewer about one part and writes what came back into the part.
async fn run_chunk(chunk: &mut ReviewChunk, engine: &crate::config::EngineConfig, cwd: &str, model: &str, prompt: String) {
    let answer = engine::complete("review", engine, cwd, Some(model), prompt);
    match tokio::time::timeout(std::time::Duration::from_secs(15 * 60), answer).await {
        Ok(Ok(text)) => {
            chunk.state = "done".into();
            chunk.verdict = Some(verdict(&text));
            chunk.text = Some(text);
        }
        Ok(Err(e)) => {
            chunk.state = "failed".into();
            chunk.error = Some(format!("{e:#}"));
        }
        Err(_) => {
            chunk.state = "failed".into();
            chunk.error = Some("took more than 15 minutes".into());
        }
    }
}

/// Sets how the review ended from what its parts said.
fn finish_review(data: &mut ReviewData, parts: usize, send_back: bool) {
    let done = data.chunks.iter().filter(|c| c.state == "done").count();
    if done == 0 && parts > 0 {
        data.state = "failed".into();
        data.error = Some(data.chunks.iter().find_map(|c| c.error.as_deref()).unwrap_or("the review failed").to_owned());
    } else {
        let text = combined(&data.chunks);
        data.state = "done".into();
        data.sent = Some(send_back && !verdict_is_fine(&text));
        data.text = Some(text);
    }
}

const DAY_SUMMARY: &str = "Below is what a developer did on one day: the threads they worked in \
with coding agents, their own messages, their commits and pull requests, versions released and threads \
that ended stuck. Do not use tools. Write the day's entry in their devblog, so that reading it later \
they understand what happened that day. Write in the language most of their messages are in.

Markdown, nothing before or after it:
- First one plain sentence with the gist of the day (it is shown alone in lists).
- Then short sections, leaving out any with nothing in it: `### Shipped` (versions released and \
what merged, with PR numbers), `### Worked` (what got done and works, grouped by project), \
`### Blocked` (what failed, got stuck or is waiting on them, and why if it shows), `### Next` (at most \
three things left open).
- Bullets of one line each, plain words about what changed for the user, not file names. Say what \
was done, not which threads existed. Keep the whole entry under 250 words.";

const REVIEW: &str = "You give a second opinion on changes another coding agent made in this \
repository, before they are merged. Do not change files and do not run anything that changes \
state; read files when you need context. Look for bugs, regressions, security problems, missed \
edge cases, and places where the change does not do what the person asked. Skip style nits. \
Write in the language of the person's request. Start with one line: Looks good, Minor issues or \
Needs changes. Then one bullet per finding: the severity (High, Medium or Low) in bold, \
`path:line`, what is wrong and how to fix it. With nothing worth fixing, say so in one sentence. \
Output only the review.";

const SEND_BACK: &str = "A second opinion from another model reviewed your changes. Go through its \
findings: fix what is right, and say briefly why for anything you leave as it is.";

/// Where a review starts: checkpoint `from` or an earlier review's end; else where the last
/// finished review ended; else the thread's first checkpoint. With the repository's folder and the
/// log position after which the person's messages count as the request.
fn review_base(thread: &Thread, events: &[Event], from: Option<&str>) -> Result<(String, PathBuf, i64)> {
    let first = events
        .iter()
        .find(|e| e.kind == EventKind::Checkpoint)
        .context("nothing to review: the thread has no checkpoint, so its folder is not a git repository")?
        .data_as::<CheckpointData>()
        .unwrap_or_default();
    let root = PathBuf::from(written(&first.root).unwrap_or(&thread.cwd));
    // A finished review, with where it ended.
    let reviewed = |e: &Event| e.data_as::<ReviewData>().filter(|r| r.state == "done").and_then(|r| Some((r.head?, r.seq)));
    let (base, since) = match from {
        Some(from) => {
            if let Some(cp) = events.iter().filter_map(Event::data_as::<CheckpointData>).find(|c| written(&c.commit) == Some(from)) {
                (from.to_owned(), cp.seq.unwrap_or(1) - 1)
            } else if let Some((_, seq)) = events.iter().filter_map(reviewed).find(|(head, _)| head == from) {
                (from.to_owned(), seq)
            } else {
                bail!("this thread has no such checkpoint");
            }
        }
        None => match events.iter().rev().find_map(reviewed) {
            Some(r) => r,
            None => (written(&first.commit).context("bad checkpoint")?.to_owned(), 0),
        },
    };
    Ok((base, root, since))
}

/// Parts of a pile of changes to review on their own: `{ title, files, review, why, state }`.
/// Small piles are one part. A larger one is cut by a small model, which also says which parts
/// need no review (formatting, generated files, docs). Every file ends up in some part.
async fn split_review(
    engine: &crate::config::EngineConfig,
    cwd: &str,
    model: Option<&str>,
    files: &[crate::checkpoint::FileDiff],
    asked: &str,
) -> Vec<ReviewChunk> {
    let all: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let part = |title: &str, files: &[&str], review: bool, why: &str| ReviewChunk {
        title: title.to_owned(),
        files: files.iter().map(|f| f.to_string()).collect(),
        review,
        why: why.to_owned(),
        state: if review { "waiting" } else { "skipped" }.to_owned(),
        ..Default::default()
    };
    let whole = || vec![part("All changes", &all, true, "")];
    let lines: usize = files.iter().map(|f| f.lines).sum();
    if files.len() < 3 && lines < 300 {
        return whole();
    }
    let mut outline = String::new();
    for f in files {
        outline.push_str(&format!("\n### {} ({} lines changed)\n", f.path, f.lines));
        outline.push_str(&f.diff.lines().take(40).collect::<Vec<_>>().join("\n"));
    }
    let prompt = format!("{SPLIT}\n\n<request>\n{}\n</request>\n\n<changes>{}\n</changes>", clip(asked, 4_000), clip(&outline, 40_000));
    let answer = engine::complete("review_split", engine, cwd, model, prompt);
    let Ok(Ok(text)) = tokio::time::timeout(std::time::Duration::from_secs(180), answer).await else {
        return whole();
    };
    let Some(parts) = text.find('[').zip(text.rfind(']')).and_then(|(a, b)| serde_json::from_str::<Vec<serde_json::Value>>(&text[a..=b]).ok()) else {
        return whole();
    };
    let mut seen = std::collections::HashSet::new();
    let mut chunks: Vec<ReviewChunk> = parts
        .into_iter()
        .filter_map(|p| {
            let names: Vec<&str> = p["files"].as_array()?.iter().filter_map(|f| f.as_str()).filter(|f| all.contains(f) && seen.insert(f.to_string())).collect();
            if names.is_empty() {
                return None;
            }
            Some(part(p["title"].as_str().unwrap_or("Changes"), &names, p["review"].as_bool().unwrap_or(true), p["why"].as_str().unwrap_or_default()))
        })
        .collect();
    let rest: Vec<&str> = all.iter().copied().filter(|f| !seen.contains(*f)).collect();
    if !rest.is_empty() {
        chunks.push(part("Other changes", &rest, true, ""));
    }
    if chunks.is_empty() { whole() } else { chunks }
}

/// One review out of the parts': an overall verdict line, then each part's findings.
fn combined(chunks: &[ReviewChunk]) -> String {
    let done: Vec<&ReviewChunk> = chunks.iter().filter(|c| c.state == "done").collect();
    let verdicts: Vec<&str> = done.iter().filter_map(|c| c.verdict.as_deref()).collect();
    let overall = if verdicts.iter().any(|v| v.to_lowercase().contains("needs") || v.to_lowercase().contains("behöver")) {
        "Needs changes"
    } else if done.iter().all(|c| verdict_is_fine(c.text.as_deref().unwrap_or_default())) {
        "Looks good"
    } else {
        "Minor issues"
    };
    if done.len() == 1 && chunks.len() == 1 {
        return done[0].text.clone().unwrap_or_default();
    }
    let mut text = format!("{overall}\n");
    for c in &done {
        let body = c.text.as_deref().unwrap_or_default().trim();
        let body = body.split_once('\n').map(|(_, rest)| rest.trim()).unwrap_or("");
        text.push_str(&format!("\n### {} · {}\n\n{}\n", c.title, c.verdict.as_deref().unwrap_or_default(), body));
    }
    let skipped: Vec<String> = chunks
        .iter()
        .filter(|c| c.state == "skipped" || c.state == "failed")
        .map(|c| format!("{} ({})", c.title, written(&c.why).or(c.error.as_deref()).unwrap_or("not reviewed")))
        .collect();
    if !skipped.is_empty() {
        text.push_str(&format!("\nNot reviewed: {}\n", skipped.join(", ")));
    }
    text
}

/// The verdict a review starts with, as its first line says it.
fn verdict(review: &str) -> String {
    let first = review.trim().lines().next().unwrap_or_default().replace(['*', '_', '#'], "");
    let first = first.trim().trim_end_matches(['.', ':']);
    if first.chars().count() <= 60 { first.to_owned() } else { String::new() }
}

const SPLIT: &str = "You prepare a code review. Below are the changes a coding agent made, file by \
file (each cut short). Group the files into a few parts that make sense to review on their own, \
such as one feature, one module or one kind of change; small piles can be a single part. Mark a \
part \"review\": false only when it needs no review at all: formatting, lock files, generated or \
vendored files, plain docs or copy. Answer with only a JSON array, no prose: \
[{\"title\": \"short name\", \"files\": [\"path\", ...], \"review\": true, \"why\": \"a few words\"}]. \
Every file goes in exactly one part; use the paths exactly as given.";

/// The review's first line says it found nothing worth fixing.
fn verdict_is_fine(review: &str) -> bool {
    let first = review.trim().lines().next().unwrap_or_default().replace(['*', '_', '#'], "").to_lowercase();
    ["looks good", "ser bra ut", "lgtm"].iter().any(|v| first.trim().starts_with(v))
}
