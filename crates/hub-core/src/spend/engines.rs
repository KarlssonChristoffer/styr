//! Claude's and Codex's own use, counted for the spend page: Claude Code's marks and sessions, and
//! Codex's session logs with the API prices they would have cost. Kept apart from `spend.rs`
//! (`spend/engines.rs`), so the totals and lanes there stay engine-neutral.

use crate::locked::{LockExt, ResultExt};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use anyhow::Result;

use crate::activity::claude_code::{CcMark, CcSession};
use crate::config::Price;
use crate::hub::Hub;
use crate::model::{Spend, SpendAccount, Thread};
use super::{Known, Tally, source_of};

/// Codex outside Hub (a spend lane key).
pub const CODEX: &str = "codex";
pub use crate::plan_meter::{CLAUDE_CLOUD, CLAUDE_CODE};

/// The lanes every count starts with beside Hub's own: each engine used on its own, outside Hub.
pub const OWN_LANES: &[&str] = &[CLAUDE_CODE, CODEX];

/// The lanes that are one account's whichever machine counts them (one login, one plan), so
/// another machine's count of them is left out.
pub const ONE_ACCOUNT_LANES: &[&str] = &[CLAUDE_CLOUD, crate::plan_meter::LANE];

/// The first moment an engine's own records (kept outside Hub's store) hold any use, as the
/// start of that day in time zone `tz`.
pub async fn first_use(tz: i32) -> Option<i64> {
    let codex = match crate::engine_home::engine_home("codex") {
        Some(dir) => tokio::task::spawn_blocking(move || codex_first(&dir)).await.ok().flatten(),
        None => None,
    };
    codex.and_then(|d| crate::activity::start_of(&d, tz))
}

/// Counts every engine's answers in `from..to` into `tally`, Hub's and the rest, from the
/// engines' own transcripts, logs and cloud sessions.
pub async fn count(hub: &Arc<Hub>, tally: &mut Tally, known: &Known<'_>, from: i64, to: i64) -> Result<()> {
    crate::activity::claude_code::import(&hub.store).await.warn_on_err("import the activity log");
    let marks = hub.store.cc_marks(from, to).await?;
    let cc_sessions = hub.store.cc_sessions().await?;
    claude(tally, &marks, &cc_sessions, known, &hub.all_prices(), &std::env::temp_dir().display().to_string());
    if let Some(dir) = crate::engine_home::engine_home("codex") {
        // Codex's own count of the whole account comes with its limits; asked in the background
        // (it starts `codex app-server`), and the page shows the last answer.
        let ask = dir.clone();
        tokio::spawn(async move { crate::codex_live::read(&ask).await });
        let logs = tokio::task::spawn_blocking(move || codex_logs(&dir, from)).await?;
        codex(tally, &logs, known, from, to);
    }
    // Read in the background: the first pass over a long history takes a while, and the page
    // asks again soon enough.
    tokio::spawn(crate::cloud_usage::sync(hub.store.clone()));
    for m in hub.store.cloud_marks(from, to).await? {
        tally.lump(CLAUDE_CLOUD, m.ts, m.usage.cost, m.usage.tokens());
    }
    Ok(())
}

/// Adds what the engines say beyond their answers to a finished count: a provider's own count
/// of the whole account, and a note when their cloud sessions could not be read.
pub async fn finish(hub: &Arc<Hub>, spend: &mut Spend) {
    if let Some(t) = crate::codex_live::account_tokens() {
        spend.accounts = Some(vec![account("codex", &t, &spend.dates)]);
    }
    if let Some(note) = hub.store.get_value(crate::cloud_usage::NOTE_KEY).await.ok().flatten().filter(|n| !n.is_empty()) {
        spend.notes = Some(vec![format!("Cloud sessions: {note}")]);
    }
}

/// A provider's daily count laid on this page's dates.
fn account(key: &str, t: &crate::codex_live::AccountTokens, dates: &[String]) -> SpendAccount {
    let daily: Vec<i64> = dates.iter().map(|d| t.daily.iter().filter(|(day, _)| day == d).map(|(_, n)| n).sum()).collect();
    SpendAccount { key: key.to_owned(), tokens: daily.iter().sum(), daily, lifetime: t.lifetime }
}

/// Claude's answers. `temp` is the system temp folder, where Hub's helper calls run.
pub fn claude(tally: &mut Tally, marks: &[CcMark], cc_sessions: &[CcSession], known: &Known, prices: &[Price], temp: &str) {
    let owners = known.owners();
    let by_id: HashMap<&str, &Thread> = known.threads.iter().map(|t| (t.id.as_str(), t)).collect();
    let temp = temp.trim_end_matches(['/', '\\']);
    // A slip names the session a one-shot went into; one from before the slips is known by the
    // temp folder it ran in.
    let helpers: HashSet<&str> = cc_sessions
        .iter()
        .filter(|s| !temp.is_empty() && s.cwd.as_deref().is_some_and(|c| c.starts_with(temp)))
        .map(|s| s.id.as_str())
        .chain(known.helpers.iter().map(String::as_str))
        .collect();
    for m in marks.iter().filter(|m| m.prompt.is_none() && m.model.is_some()) {
        let price = crate::stats::cost(m, prices);
        let (cost, priced) = (price.unwrap_or(0.0), price.is_some());
        match owners.get(m.session_id.as_str()).copied() {
            Some(id) => {
                let thread = by_id.get(id).copied();
                if thread.is_some_and(|t| known.paid_engines.contains(&t.engine)) {
                    continue;
                }
                tally.hub("claude", &source_of(thread, known.chats), Some(id), m.ts, cost, m.tokens, priced);
            }
            None if helpers.contains(m.session_id.as_str()) => tally.hub("claude", "helper", None, m.ts, cost, m.tokens, priced),
            None => tally.alone(CLAUDE_CODE, m.ts, cost, m.tokens, priced),
        }
    }
}

/// One Codex answer, as a `token_count` event reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct CodexUse {
    pub ts: i64,
    pub model: String,
    /// Input tokens, cached ones included.
    pub input: i64,
    pub cached: i64,
    pub output: i64,
}

/// One Codex session log.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodexSession {
    pub id: String,
    /// What started it: `codex_cli_rs`, `codex_vscode`, the ACP adapter Hub uses, ...
    pub originator: String,
    pub uses: Vec<CodexUse>,
}

/// Reads a session log. Codex sometimes writes the same count twice, so an event counts only
/// when the session's running total grew.
pub fn read_rollout(text: &str) -> CodexSession {
    let mut out = CodexSession::default();
    let mut model = String::new();
    let mut total = 0i64;
    for line in text.lines() {
        if !(line.contains("session_meta") || line.contains("turn_context") || line.contains("token_count")) {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let p = &v["payload"];
        match v["type"].as_str() {
            Some("session_meta") => {
                out.id = p["id"].as_str().unwrap_or_default().to_owned();
                out.originator = p["originator"].as_str().unwrap_or_default().to_owned();
            }
            Some("turn_context") => {
                if let Some(m) = p["model"].as_str() {
                    model = m.to_owned();
                }
            }
            Some("event_msg") if p["type"] == "token_count" => {
                let info = &p["info"];
                let now = info["total_token_usage"]["total_tokens"].as_i64().unwrap_or(0);
                if now <= total {
                    continue;
                }
                total = now;
                let last = &info["last_token_usage"];
                let n = |k: &str| last[k].as_i64().unwrap_or(0);
                let Some(ts) = v["timestamp"].as_str().and_then(crate::usage::iso_ms) else { continue };
                out.uses.push(CodexUse { ts, model: model.clone(), input: n("input_tokens"), cached: n("cached_input_tokens"), output: n("output_tokens") });
            }
            _ => {}
        }
    }
    out
}

/// Dollars per million tokens through OpenAI's API: input, cached input, output. A model not in
/// the list is priced as GPT-5 and marked as an estimate.
const CODEX_PRICES: &[(&str, f64, f64, f64)] = &[
    ("gpt-5-mini", 0.25, 0.025, 2.0),
    ("gpt-5-nano", 0.05, 0.005, 0.4),
    ("gpt-5", 1.25, 0.125, 10.0),
    ("gpt-4.1", 2.0, 0.5, 8.0),
    ("o4-mini", 1.1, 0.275, 4.4),
    ("o3", 2.0, 0.5, 8.0),
];

/// What a Codex answer would cost through the API, and whether its model's price is known.
pub fn codex_cost(u: &CodexUse) -> (f64, bool) {
    let known = CODEX_PRICES.iter().find(|(m, ..)| u.model.starts_with(m));
    let (_, input, cached, output) = known.copied().unwrap_or(CODEX_PRICES[2]);
    let fresh = (u.input - u.cached).max(0);
    ((fresh as f64 * input + u.cached as f64 * cached + u.output as f64 * output) / 1_000_000.0, known.is_some())
}

/// Codex's answers in `from..to`.
pub fn codex(tally: &mut Tally, logs: &[Arc<CodexSession>], known: &Known, from: i64, to: i64) {
    let owners = known.owners();
    let by_id: HashMap<&str, &Thread> = known.threads.iter().map(|t| (t.id.as_str(), t)).collect();
    for log in logs {
        let thread = owners.get(log.id.as_str()).copied();
        let helper = thread.is_none() && known.helpers.contains(&log.id);
        let in_hub = thread.is_some() || helper || log.originator.to_ascii_lowercase().contains("acp");
        let source = if helper { "helper".to_owned() } else { source_of(thread.and_then(|id| by_id.get(id).copied()), known.chats) };
        for u in log.uses.iter().filter(|u| u.ts >= from && u.ts < to) {
            let (cost, priced) = codex_cost(u);
            let tokens = (u.input - u.cached).max(0) + u.output;
            if in_hub {
                tally.hub("codex", &source, thread, u.ts, cost, tokens, priced);
            } else {
                tally.alone(CODEX, u.ts, cost, tokens, priced);
            }
        }
    }
}

type ReadLogs = HashMap<PathBuf, (u64, Arc<CodexSession>)>;

/// Session logs already read, by path, with the size they had then: a log only grows, so one
/// of the same size is the same log.
static READ: LazyLock<Mutex<ReadLogs>> = LazyLock::new(Default::default);

/// The session logs that can hold answers from `from` on: the dated folders from two days before
/// (a session runs on past midnight), and archived logs by the date in their names.
pub(crate) fn codex_logs(dir: &Path, from: i64) -> Vec<Arc<CodexSession>> {
    let since = crate::activity::date_of(from - 2 * 86_400_000, 0);
    let mut files = Vec::new();
    let names = |d: &Path| -> Vec<PathBuf> { std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).collect() };
    let name = |p: &Path| p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
    for year in names(&dir.join("sessions")) {
        for month in names(&year) {
            for day in names(&month) {
                if format!("{}-{}-{}", name(&year), name(&month), name(&day)) >= since {
                    files.extend(names(&day));
                }
            }
        }
    }
    // rollout-2026-11-03T10-00-00-<id>.jsonl
    files.extend(names(&dir.join("archived_sessions")).into_iter().filter(|p| name(p).get(8..18).is_some_and(|d| d >= since.as_str())));
    files.retain(|p| {
        let n = name(p);
        n.starts_with("rollout-") && n.ends_with(".jsonl")
    });
    let mut read = READ.locked();
    let mut out = Vec::new();
    for path in files {
        let Ok(size) = std::fs::metadata(&path).map(|m| m.len()) else { continue };
        if let Some((_, log)) = read.get(&path).filter(|(seen, _)| *seen == size) {
            out.push(log.clone());
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let log = Arc::new(read_rollout(&text));
        read.insert(path, (size, log.clone()));
        out.push(log);
    }
    out
}


/// The first day Codex kept a session log for: its oldest dated folder or archived log.
pub(crate) fn codex_first(dir: &Path) -> Option<String> {
    let names = |d: &Path| -> Vec<String> {
        std::fs::read_dir(d).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str().map(str::to_owned)).collect()
    };
    let mut days = Vec::new();
    let sessions = dir.join("sessions");
    for year in names(&sessions) {
        for month in names(&sessions.join(&year)) {
            if let Some(day) = names(&sessions.join(&year).join(&month)).into_iter().min() {
                days.push(format!("{year}-{month}-{day}"));
            }
        }
    }
    // rollout-2026-11-03T10-00-00-<id>.jsonl
    days.extend(names(&dir.join("archived_sessions")).into_iter().filter(|n| n.starts_with("rollout-")).filter_map(|n| n.get(8..18).map(str::to_owned)));
    days.into_iter().filter(|d| crate::activity::start_of(d, 0).is_some()).min()
}

