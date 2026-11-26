//! Problems: errors and warnings from Hub itself (its core's log, the app window, threads that
//! fail) and from the user's own projects, gathered in one place. The same error is one problem
//! with a count, as in GlitchTip; each time it happened is a hit, a few of which are kept with
//! their stack and the log lines before it.
//!
//! Projects report with any Sentry SDK: Hub takes Sentry's envelope format at
//! `/api/<n>/envelope/` (and the older `/api/<n>/store/`), with a DSN Hub hands out per source.
//! Hub's own parts report with [`capture`] (the core's log) or `POST /api/problems/report`.
//! Everything is kept in Hub's own tables, so nothing here depends on Sentry beyond its format.

use std::collections::VecDeque;
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::hub::Hub;
use crate::locked::Ignore;
use crate::model::{StreamMsg, now_ms};

pub mod dossier;
pub mod analysis;
pub mod fixnow;
pub mod metrics;
pub mod otlp;
pub mod quiet;
pub mod report;
pub mod rules;
pub mod search;
pub mod tools;
pub mod vakten;
pub mod watch;

mod changes;
mod fix;
mod ingest;
mod routes;
mod rows;
mod types;

pub use changes::*;
pub use fix::*;
pub use ingest::*;
pub(crate) use rows::*;
pub use types::*;

pub use routes::{GlobalRules, MetricReport, SavedView, is_ingest, router, saved_views};
use routes::answer;

#[cfg(test)]
mod tests;

/// Hits kept per problem; older ones go, the count stays.
const KEEP_HITS: i64 = 30;
/// Days of counts kept for each problem's sparkline.
const KEEP_DAYS: i64 = 90;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// Where the sources (each with its DSN key) are kept in `kv`.
const SOURCES: &str = "problems.sources";
/// Log lines kept in memory to go with a warning from the core.
const CRUMBS: usize = 20;

// ---------------------------------------------------------------------------------------------
// Taking problems in

pub use crate::problem_inbox::{Incoming, capture};

/// Makes Hub hand Problems the events it looks at once they are logged (`Hub::on_look`). `start`
/// does this; a test that wants a failed thread kept as a problem calls it on its own hub.
pub fn install(hub: &Hub) {
    hub.on_look(|hub, event| Box::pin(looked_at(hub, event)));
}

/// What Problems does with an event Hub has logged: a failed thread becomes a problem and gets its
/// fix card, a finished fix is told to its thread and to Guard, and a tool call is counted.
async fn looked_at(hub: Arc<Hub>, event: crate::model::Event) {
    use crate::model::EventKind;
    match event.kind {
        EventKind::Error => {
            match thread_failed(&hub, &event).await {
                Ok(Some(problem)) => {
                    if let Err(e) = fixnow::reported(&hub, &event, &problem).await {
                        tracing::debug!(target: "hub_core::problems", thread = %event.thread_id, "showing a failure's card: {e:#}");
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::debug!(target: "hub_core::problems", thread = %event.thread_id, "keeping a failed thread as a problem: {e:#}");
                }
            }
            fix_ended(&hub, &event).await;
        }
        EventKind::Update => {
            if crate::model::worth_a_look(&event.data)
                && let Err(e) = tools::saw(&hub, &event).await
            {
                tracing::debug!(target: "hub_core::problems", thread = %event.thread_id, "counting a tool call: {e:#}");
            }
        }
        EventKind::TurnEnd => fix_ended(&hub, &event).await,
        _ => {}
    }
}

/// A thread that ended or failed: its fix card is closed and Guard hears of it.
async fn fix_ended(hub: &Arc<Hub>, event: &crate::model::Event) {
    if let Err(e) = fixnow::fix_finished(hub, event).await {
        tracing::debug!(target: "hub_core::problems", thread = %event.thread_id, "telling a failed thread its fix ended: {e:#}");
    }
    if let Err(e) = report::turn_ended(hub, event).await {
        tracing::debug!(target: "hub_core::problems", thread = %event.thread_id, "reporting a fix thread to Guard: {e:#}");
    }
}

/// Starts taking problems in: what [`capture`] got so far, then everything after.
pub fn start(hub: Arc<Hub>) {
    install(&hub);
    let Some(mut rx) = crate::problem_inbox::open() else {
        // A second core in the same process (tests): it records what it is given directly.
        return;
    };
    {
        let hub = hub.clone();
        crate::supervise::forever("problems::vakten", move || vakten::run(hub.clone()));
    }
    let counting = hub.clone();
    tokio::spawn(async move {
        if let Err(e) = tools::backfill(counting).await {
            tracing::debug!(target: "hub_core::problems", "could not count the tool calls in the log: {e:#}");
        }
    });
    tokio::spawn(async move {
        while let Some(incoming) = rx.recv().await {
            if let Err(e) = record(&hub, incoming).await {
                // Logged under this module, which the log layer leaves out, so it cannot loop.
                tracing::debug!(target: "hub_core::problems", "could not keep a problem: {e:#}");
            }
        }
    });
}

/// Groups a problem with the ones like it and keeps the hit. Returns the problem.
pub async fn record(hub: &Arc<Hub>, incoming: Incoming) -> Result<Problem> {
    let mut hit = incoming.hit;
    if hit.ts == 0 {
        hit.ts = now_ms();
    }
    hit.message = hit.message.chars().take(4000).collect();
    if hit.extra.is_null() {
        hit.extra = json!({});
    }
    // Secrets never reach the table: the same masking as the event log.
    let mut data = serde_json::to_value(&hit)?;
    hub.secrets.redact(&mut data);
    let hit: Hit = serde_json::from_value(data.clone())?;

    // The user's rules first: they may group, stamp or archive what comes in.
    let ruled = rules::apply(hub, &incoming.project, &incoming.source, incoming.level, incoming.folder.as_deref(), &hit).await;
    let given = ruled.fingerprint.clone().or(incoming.fingerprint);
    let fingerprint = fingerprint(&incoming.project, &incoming.source, given.as_deref(), &hit);
    let title = title(&hit);
    let culprit = culprit(&hit.frames);
    let machine = hub.remotes.local_name().to_owned();
    let (project, source, level, folder) = (incoming.project, incoming.source, incoming.level, incoming.folder);
    let ts = hit.ts;
    let release = hit.release.clone().unwrap_or_default();
    let (id, happened) = hub
        .store
        .with(move |c| {
            let tx = c.transaction()?;
            let seen = Seen { ts, title: &title, level, folder: folder.as_deref(), culprit: culprit.as_deref(), release: &release };
            // A fingerprint merged into another problem counts there.
            let target: Option<i64> = tx
                .query_row("SELECT problem_id FROM problem_aliases WHERE fingerprint = ?1", [&fingerprint], |r| r.get(0))
                .optional()?;
            let found: Option<Known> = tx
                .query_row(
                    "SELECT id, status, archive, resolved_releases, count, first_seen, escalating, prio
                     FROM problems WHERE id = ?1 OR (?1 IS NULL AND fingerprint = ?2)",
                    params![target, fingerprint],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get::<_, i64>(6)? != 0, r.get(7)?)),
                )
                .optional()?;
            let mut happened = Vec::new();
            let id = match found {
                Some(found) => bump_known(&tx, found, &seen, &mut happened)?,
                None => insert_new(&tx, &fingerprint, (&project, &source), &seen, &machine, &mut happened)?,
            };
            tx.execute("INSERT INTO problem_hits (problem_id, ts, data) VALUES (?1, ?2, ?3)", params![id, ts, data.to_string()])?;
            tx.execute(
                "INSERT INTO problem_days (problem_id, day, count) VALUES (?1, ?2, 1)
                 ON CONFLICT(problem_id, day) DO UPDATE SET count = count + 1",
                params![id, ts.div_euclid(DAY_MS)],
            )?;
            tx.execute(
                "DELETE FROM problem_hits WHERE problem_id = ?1 AND rowid NOT IN
                    (SELECT rowid FROM problem_hits WHERE problem_id = ?1 ORDER BY ts DESC LIMIT ?2)",
                params![id, KEEP_HITS],
            )?;
            tx.execute("DELETE FROM problem_days WHERE problem_id = ?1 AND day < ?2", params![id, ts.div_euclid(DAY_MS) - KEEP_DAYS])?;
            tx.commit()?;
            Ok((id, happened))
        })
        .await?;
    // A rule stamps or archives a new problem the way the user said.
    if happened.contains(&"new") && !ruled.is_empty() {
        rules::stamp_new(hub, id, &ruled).await?;
    }
    let problem = get(hub, id).await?.context("the problem was just kept")?;
    hub.publish(StreamMsg::Problem { problem: problem.clone() });
    if !happened.is_empty() {
        vakten::nudge();
    }
    Ok(problem)
}

/// Notes what happened to a problem, inside the caller's transaction.
pub(crate) fn log(c: &rusqlite::Connection, id: i64, by: &str, kind: &str, text: &str) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO problem_activity (problem_id, ts, by, kind, text) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, now_ms(), by, kind, text],
    )
    .map(|_| ())
}

/// [`log`] on its own.
pub async fn note_activity(hub: &Hub, id: i64, by: &str, kind: &str, text: &str) -> Result<()> {
    let (by, kind, text) = (by.to_owned(), kind.to_owned(), text.to_owned());
    hub.store.with(move |c| Ok(log(c, id, &by, &kind, &text)?)).await
}

/// Lists problems by a view and a search (see [`search::Query`]).
pub async fn list(hub: &Hub, filter: ProblemFilter) -> Result<Vec<Problem>> {
    let query = search::Query::parse(filter.query.as_deref().unwrap_or_default());
    let release = query.release.clone();
    let all = hub
        .store
        .with(move |c| {
            // Release is the one thing only the hits know.
            let mut stmt = c.prepare(
                "SELECT * FROM problems p WHERE (?1 IS NULL OR EXISTS
                    (SELECT 1 FROM problem_hits h WHERE h.problem_id = p.id AND json_extract(h.data, '$.release') = ?1))
                 ORDER BY last_seen DESC LIMIT 5000",
            )?;
            let problems: Vec<Problem> = stmt.query_map(params![release], row)?.collect::<rusqlite::Result<_>>()?;
            Ok(with_days(c, problems)?)
        })
        .await?;
    let vakten_on = vakten::watched(hub).await;
    let mut all = all;
    finish(hub, vakten_on, &mut all);
    let view = filter.view.unwrap_or(match filter.status {
        Some(ProblemStatus::Resolved) => View::Resolved,
        Some(ProblemStatus::Archived) => View::Archived,
        _ => View::Open,
    });
    let level = filter.level.unwrap_or(Level::Info);
    let limit = filter.limit.unwrap_or(500).min(2000) as usize;
    let week = now_ms() - 7 * DAY_MS;
    Ok(all
        .into_iter()
        .filter(|p| match view {
            View::NeedsYou => p.needs_you,
            View::Vakten => p.status == ProblemStatus::Open && p.merged_into.is_none() && p.thread.is_some() && !p.needs_you,
            View::Open => p.status == ProblemStatus::Open && p.merged_into.is_none(),
            View::Fixed => p.status == ProblemStatus::Resolved && p.resolved_at.is_some_and(|t| t >= week),
            View::Back => p.status == ProblemStatus::Open && p.regressed,
            View::Archived => p.status == ProblemStatus::Archived && p.merged_into.is_none(),
            View::Resolved => p.status == ProblemStatus::Resolved,
            View::Quiet => p.status == ProblemStatus::Open && p.source == "monitor",
            View::Slower => p.status == ProblemStatus::Open && p.source == "metrics",
            View::All => p.merged_into.is_none(),
        })
        .filter(|p| p.level >= level)
        .filter(|p| filter.project.as_deref().is_none_or(|w| w.trim().is_empty() || p.project.eq_ignore_ascii_case(w.trim())))
        .filter(|p| filter.source.as_deref().is_none_or(|w| w.trim().is_empty() || p.source == w.trim()))
        .filter(|p| query.matches(p))
        .take(limit)
        .collect())
}

pub async fn get(hub: &Hub, id: i64) -> Result<Option<Problem>> {
    let problem = hub
        .store
        .with(move |c| {
            let problem = c.query_row("SELECT * FROM problems WHERE id = ?1", [id], row).optional()?;
            Ok(match problem {
                Some(p) => with_days(c, vec![p])?.pop(),
                None => None,
            })
        })
        .await?;
    let Some(mut problem) = problem else { return Ok(None) };
    let vakten_on = vakten::watched(hub).await;
    finish(hub, vakten_on, std::slice::from_mut(&mut problem));
    Ok(Some(problem))
}

pub async fn detail(hub: &Hub, id: i64, hits: u32) -> Result<Option<ProblemDetail>> {
    let Some(problem) = get(hub, id).await? else { return Ok(None) };
    let (hits, activity, tags) = hub
        .store
        .with(move |c| {
            let mut stmt = c.prepare("SELECT data FROM problem_hits WHERE problem_id = ?1 ORDER BY ts DESC LIMIT ?2")?;
            let rows = stmt.query_map(params![id, hits], |r| r.get::<_, String>(0))?;
            let hits: Vec<Hit> = rows.filter_map(|r| r.ok()).filter_map(|d| serde_json::from_str::<Hit>(&d).ok()).collect();
            let mut stmt = c.prepare("SELECT ts, by, kind, text FROM problem_activity WHERE problem_id = ?1 ORDER BY ts DESC, rowid DESC LIMIT 200")?;
            let activity = stmt
                .query_map([id], |r| Ok(Activity { ts: r.get(0)?, by: r.get(1)?, kind: r.get(2)?, text: r.get(3)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            // Tags over the hits kept (the latest 30), as Sentry shows them.
            let mut stmt = c.prepare(
                "SELECT 'release', COALESCE(json_extract(data, '$.release'), '(none)'), COUNT(*) FROM problem_hits WHERE problem_id = ?1 GROUP BY 2
                 UNION ALL
                 SELECT 'environment', json_extract(data, '$.environment'), COUNT(*) FROM problem_hits WHERE problem_id = ?1 AND json_extract(data, '$.environment') IS NOT NULL GROUP BY 2
                 UNION ALL
                 SELECT 'engine', json_extract(data, '$.extra.engine'), COUNT(*) FROM problem_hits WHERE problem_id = ?1 AND json_extract(data, '$.extra.engine') IS NOT NULL GROUP BY 2
                 ORDER BY 1, 3 DESC",
            )?;
            let tags = stmt
                .query_map([id], |r| Ok(ProblemTag { key: r.get(0)?, value: r.get::<_, Option<String>>(1)?.unwrap_or_default(), count: r.get(2)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((hits, activity, tags))
        })
        .await?;
    Ok(Some(ProblemDetail { problem, hits, activity, tags }))
}

/// Resolve, archive or open a problem again: [`change`] with only a status.
pub async fn set_status(hub: &Hub, id: i64, status: ProblemStatus) -> Result<Problem> {
    change(hub, id, ProblemPatch { status: Some(status), ..Default::default() }, "you").await
}

// ---------------------------------------------------------------------------------------------
// Sources: programs that report with a Sentry SDK

pub async fn sources(hub: &Hub) -> Vec<Source> {
    let stored: Vec<Source> =
        hub.store.get_value(SOURCES).await.ok().flatten().and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    let addr = hub.config.bind;
    let heard = quiet::last_heard(hub).await;
    stored
        .into_iter()
        .map(|mut s| {
            s.dsn = format!("http://{}@127.0.0.1:{}/{}", s.key, addr.port(), s.id);
            s.otlp = format!("http://127.0.0.1:{}/api/{}/otlp", addr.port(), s.id);
            s.last_heard = heard.get(&s.name).copied();
            s
        })
        .collect()
}

/// A source for a program, or the one it already has under that name.
pub async fn add_source(hub: &Hub, new: NewSource) -> Result<Source> {
    let name = new.name.trim().to_owned();
    if name.is_empty() {
        bail!("give the program a name");
    }
    let mut all = sources(hub).await;
    let folder = new.folder.filter(|f| !f.trim().is_empty());
    if let Some(s) = all.iter_mut().find(|s| s.name.eq_ignore_ascii_case(&name)) {
        if new.quiet_after_hours.is_none() && folder.is_none() {
            return Ok(s.clone());
        }
        if let Some(h) = new.quiet_after_hours {
            s.quiet_after_hours = (h > 0).then_some(h);
        }
        if folder.is_some() {
            s.folder = folder;
        }
        let id = s.id;
        hub.store.set_value(SOURCES, &serde_json::to_string(&all)?).await?;
        return sources(hub).await.into_iter().find(|s| s.id == id).context("the source was just kept");
    }
    let key: String = uuid::Uuid::new_v4().simple().to_string();
    let id = all.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    all.push(Source {
        id,
        name,
        key,
        folder,
        dsn: String::new(),
        quiet_after_hours: new.quiet_after_hours.filter(|h| *h > 0),
        last_heard: None,
        otlp: String::new(),
    });
    hub.store.set_value(SOURCES, &serde_json::to_string(&all)?).await?;
    let id_now = id;
    sources(hub).await.into_iter().find(|s| s.id == id_now).context("the source was just kept")
}

pub async fn remove_source(hub: &Hub, id: u32) -> Result<()> {
    let all: Vec<Source> = sources(hub).await.into_iter().filter(|s| s.id != id).collect();
    hub.store.set_value(SOURCES, &serde_json::to_string(&all)?).await
}

// ---------------------------------------------------------------------------------------------
// Hub's own sources

/// The core's own log: lines kept in memory to go with the next warning, as what came before.
pub struct LogTrail {
    lines: Mutex<VecDeque<Crumb>>,
    release: OnceLock<String>,
}

impl LogTrail {
    pub const fn new() -> Self {
        Self { lines: Mutex::new(VecDeque::new()), release: OnceLock::new() }
    }

    /// The version of Hub that is running, put on each problem from the log.
    pub fn set_release(&self, version: &str) {
        self.release.set(version.to_owned()).ignore("the running version is set once; a second call changes nothing");
        metrics::RELEASE.set(version.to_owned()).ignore("the running version is set once; a second call changes nothing");
    }

    /// A log line from the core. Warnings and errors become problems; every line is kept for a
    /// while as what came before.
    pub fn line(&self, level: &str, target: &str, file: Option<&str>, message: String) {
        // The problems module logs only about itself; reporting that would loop.
        if target.starts_with("hub_core::problems") {
            return;
        }
        let crumb = Crumb { ts: Some(now_ms()), level: Some(level.to_ascii_lowercase()), category: Some(target.to_owned()), message: message.clone() };
        let crumbs: Vec<Crumb> = {
            let mut lines = self.lines.lock().unwrap_or_else(|p| p.into_inner());
            let before = lines.iter().cloned().collect();
            lines.push_back(crumb);
            while lines.len() > CRUMBS {
                lines.pop_front();
            }
            before
        };
        let level = Level::parse(level);
        if level < Level::Warning {
            return;
        }
        capture(Incoming {
            project: "Hub".into(),
            source: "hubd".into(),
            level,
            folder: None,
            fingerprint: None,
            hit: Hit {
                ts: now_ms(),
                message,
                kind: None,
                frames: vec![Frame { file: file.map(str::to_owned), function: Some(target.to_owned()), line: None, code: None, own: true }],
                trace: None,
                crumbs,
                release: self.release.get().cloned(),
                environment: None,
                extra: json!({}),
            },
        });
    }
}

impl Default for LogTrail {
    fn default() -> Self {
        Self::new()
    }
}

/// A thread's engine failed: one problem per kind of failure, not per thread. Plan limits are
/// left out, since Hub carries those threads on by itself. Returns the problem it went to.
pub(crate) async fn thread_failed(hub: &Arc<Hub>, event: &crate::model::Event) -> Result<Option<Problem>> {
    let Some(thread) = hub.store.thread(&event.thread_id).await? else { return Ok(None) };
    let message = event.failure().unwrap_or_default();
    let phrases = hub.config.traits(&thread.engine).limit_phrases.unwrap_or_default();
    if crate::resume::limit_hit(message, &phrases).is_some() || vakten::backoff::skip_failure(hub, &thread.id, event.ts, message).await {
        return Ok(None);
    }
    let projects = hub.store.projects().await.unwrap_or_default();
    // An engine failing is the engine's fault, not the folder's: a thread outside every project
    // (a dated folder from Home, say) counts under Hub, so the same failure is one problem and
    // a fix goes to Hub's own code rather than to a folder that has none.
    let known = projects.iter().find(|p| crate::folders::same_folder(&p.path, &thread.cwd));
    let project = known.map(|p| p.name.clone()).unwrap_or_else(|| "Hub".into());
    let folder = known.map(|_| thread.cwd.clone());
    let detail = event.detail().as_str().map(str::to_owned).or_else(|| (!event.detail().is_null()).then(|| event.detail().to_string()));
    let incoming = Incoming {
        project,
        source: "thread".into(),
        level: Level::Error,
        folder,
        fingerprint: Some(vec![thread.engine.clone(), normalize(message)]),
        hit: Hit {
            ts: event.ts,
            message: message.to_owned(),
            kind: Some(format!("{} failed", thread.engine)),
            frames: Vec::new(),
            trace: detail,
            crumbs: tools::crumbs(hub, &thread.id, event.seq).await.unwrap_or_default(),
            release: None,
            environment: None,
            extra: json!({ "thread": thread.id, "title": thread.title, "engine": thread.engine, "seq": event.seq }),
        },
    };
    record(hub, incoming).await.map(Some)
}

/// What starting a thread on a problem gave: the thread, or a draft when Hub does not know
/// which folder the problem's code is in.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProblemThread {
    pub thread: Option<crate::model::Thread>,
    pub folder: Option<String>,
    pub prompt: String,
}

/// What a hit says about its problem, for the rows it touches.
struct Seen<'a> {
    ts: i64,
    title: &'a str,
    level: Level,
    folder: Option<&'a str>,
    culprit: Option<&'a str>,
    release: &'a str,
}

/// The row a problem already has: id, status, archive, resolved releases, count, first seen,
/// escalating, prio.
type Known = (i64, String, Option<String>, Option<String>, i64, i64, bool, Option<String>);

/// Counts a hit on a problem that exists: it may come back from resolved or from the archive, and
/// it may start escalating. Returns the problem's id.
fn bump_known(tx: &rusqlite::Transaction, found: Known, seen: &Seen, happened: &mut Vec<&'static str>) -> Result<i64> {
    let (id, status, archive, releases, count, first_seen, escalating, prio) = found;
    let Seen { ts, title, level, folder, culprit, release } = *seen;
        let day = ts.div_euclid(DAY_MS);
        let last_day: i64 = tx.query_row(
            "SELECT COALESCE(SUM(count), 0) FROM problem_days WHERE problem_id = ?1 AND day >= ?2",
            params![id, day - 1],
            |r| r.get(0),
        )?;
        // Resolved: back, unless it was resolved for the next release and this hit
        // still comes from a release it was seen in before.
        let back = status == "resolved"
            && match releases.as_deref().and_then(|r| serde_json::from_str::<Vec<String>>(r).ok()) {
                Some(seen) => !seen.iter().any(|r| r == release),
                None => true,
            };
        let unarchived = status == "archived"
            && match archive.as_deref().and_then(|a| serde_json::from_str::<Archive>(a).ok()) {
                Some(Archive::Forever) => false,
                Some(Archive::Time { at }) => ts >= at,
                Some(Archive::Count { count: n }) => count + 1 >= n,
                Some(Archive::Worse { rate }) => (last_day + 1) as f64 > (2.0 * rate).max(10.0),
                None => false,
            };
        tx.execute(
            "UPDATE problems SET last_seen = MAX(last_seen, ?2), count = count + 1, title = ?3,
                level = CASE WHEN ?4 > level_rank THEN ?5 ELSE level END,
                level_rank = MAX(level_rank, ?4),
                status = CASE WHEN ?6 OR ?9 THEN 'open' ELSE status END,
                regressed = CASE WHEN ?6 THEN 1 ELSE regressed END,
                archive = CASE WHEN ?9 THEN NULL ELSE archive END,
                resolved_at = CASE WHEN ?6 THEN NULL ELSE resolved_at END,
                folder = COALESCE(folder, ?7), culprit = COALESCE(?8, culprit)
             WHERE id = ?1",
            params![id, ts, title, level as i64, level.as_str(), back, folder, culprit, unarchived],
        )?;
        if back {
            let from = if release.is_empty() { String::new() } else { format!(" in release {release}") };
            log(tx, id, "Hub", "regressed", &format!("Came back{from} after it was resolved"))?;
            happened.push("regressed");
            // A fix that did not hold: what to do next is the user's call.
            if tx.execute("UPDATE problems SET lane = 'ask' WHERE id = ?1 AND thread_id IS NOT NULL AND COALESCE(lane, '') != 'ask'", [id])? > 0 {
                log(tx, id, "Hub", "stamped", "Moved to ask first: it came back after a fix")?;
            }
        }
        // Escalating: today clearly over the daily rate of the week before, for a
        // problem older than a day.
        let today: i64 = tx.query_row(
            "SELECT COALESCE(SUM(count), 0) FROM problem_days WHERE problem_id = ?1 AND day = ?2",
            params![id, day],
            |r| r.get::<_, i64>(0),
        )? + 1;
        let week: i64 = tx.query_row(
            "SELECT COALESCE(SUM(count), 0) FROM problem_days WHERE problem_id = ?1 AND day >= ?2 AND day < ?3",
            params![id, day - 7, day],
            |r| r.get(0),
        )?;
        let rate = week as f64 / 7.0;
        if !escalating && ts - first_seen > DAY_MS && today >= 10 && today as f64 > (3.0 * rate).max(10.0) {
            let up = match prio.as_deref() {
                Some("P3") => Some("P2"),
                Some("P2") => Some("P1"),
                Some("P1") => Some("P0"),
                _ => None,
            };
            tx.execute("UPDATE problems SET escalating = 1, prio = COALESCE(?2, prio) WHERE id = ?1", params![id, up])?;
            let raised = up.map(|p| format!("; prio raised to {p}")).unwrap_or_default();
            log(tx, id, "Hub", "escalating", &format!("Escalating: {today} times today against {rate:.1} a day before{raised}"))?;
            happened.push("escalating");
        }
        if unarchived {
            log(tx, id, "Hub", "status", "Back from the archive: its condition was met")?;
            happened.push("unarchived");
        }
    Ok(id)
}

/// Opens a problem for a hit that no problem has. Returns its id.
fn insert_new(
    tx: &rusqlite::Transaction,
    fingerprint: &str,
    (project, source): (&str, &str),
    seen: &Seen,
    machine: &str,
    happened: &mut Vec<&'static str>,
) -> Result<i64> {
    let Seen { ts, title, level, folder, culprit, release } = *seen;
        tx.execute(
            "INSERT INTO problems (fingerprint, project, source, level, level_rank, title, culprit, folder, machine,
                first_seen, last_seen, count, status, regressed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, 1, 'open', 0)",
            params![fingerprint, project, source, level.as_str(), level as i64, title, culprit, folder, machine, ts],
        )?;
        let id = tx.last_insert_rowid();
        let from = if release.is_empty() { String::new() } else { format!(", release {release}") };
        log(tx, id, "Hub", "seen", &format!("First seen on {machine}{from}"))?;
        happened.push("new");
    Ok(id)
}
