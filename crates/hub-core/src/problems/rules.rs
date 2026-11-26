//! Rules for what comes in: group, stamp, archive. They live in the project's repo as
//! `.hub/problems.toml` and, for everything, in Settings (kept in `kv` as the same TOML). Vakten
//! suggests one when the user corrects it the same way three times.
//!
//! ```toml
//! [[rule]]
//! name = "ssh hiccups"
//! match = "could not reach * over ssh"   # the message or title, * for anything
//! path = "crates/hub-core/src/remote.rs" # a file in the stack or where it happened
//! project = "Hub"                        # also: source, machine, level (the lowest)
//! group = "ssh unreachable"             # one problem for everything that matches
//! prio = "P3"                            # also: lane, category
//! archive = "worse"                      # or "forever"
//! ```
//!
//! Every rule that matches applies, in order; a later one wins for the same stamp. Hub's own
//! first pass ([`built_in`]) comes first, then the global rules, then the project's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use anyhow::{Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Archive, Hit, Lane, Level, Prio, ProblemPatch};
use crate::hub::Hub;
use crate::model::now_ms;

/// The global rules, as TOML, in `kv`.
const GLOBAL: &str = "problems.rules";
/// Where a project keeps its own, inside its folder.
pub const FILE: &str = ".hub/problems.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Rule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Words in the message or title, `*` for anything in between.
    #[serde(default, rename = "match", skip_serializing_if = "Option::is_none")]
    #[ts(optional, rename = "match")]
    pub pattern: Option<String>,
    /// A file in the stack, or where it happened, `*` for anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub machine: Option<String>,
    /// The lowest level it applies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub level: Option<Level>,
    /// Everything that matches is one problem, known by these words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub prio: Option<Prio>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lane: Option<Lane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
    /// "forever" or "worse".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub archive: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct RuleFile {
    #[serde(default)]
    rule: Vec<Rule>,
}

/// What the rules said about something coming in.
#[derive(Debug, Clone, Default)]
pub struct Ruled {
    pub fingerprint: Option<Vec<String>>,
    pub prio: Option<Prio>,
    pub lane: Option<Lane>,
    pub category: Option<String>,
    pub archive: Option<Archive>,
    pub names: Vec<String>,
}

impl Ruled {
    /// Nothing to stamp on a new problem (grouping alone is done by then).
    pub fn is_empty(&self) -> bool {
        self.prio.is_none() && self.lane.is_none() && self.category.is_none() && self.archive.is_none()
    }
}

/// The rules page: the global rules, the project files Hub knows of, and what Vakten suggests.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProblemRules {
    pub global: String,
    pub files: Vec<RuleFileInfo>,
    pub suggestions: Vec<Suggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RuleFileInfo {
    pub project: String,
    pub path: String,
    #[ts(type = "number")]
    pub rules: usize,
}

/// A rule that the user's own choices point to.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Suggestion {
    /// In words: "Archive warnings from Aftermath's sentry for good".
    pub text: String,
    /// Why: "you did it 4 times".
    pub why: String,
    pub rule: Rule,
}

/// Parsed files by path, read again when they change.
static FILES: Mutex<Option<HashMap<PathBuf, (SystemTime, Vec<Rule>)>>> = Mutex::new(None);
/// Project folders by project name, looked up at most once a minute.
static FOLDERS: Mutex<Option<HashMap<String, (i64, Option<String>)>>> = Mutex::new(None);

pub fn parse(text: &str) -> Result<Vec<Rule>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let file: RuleFile = toml::from_str(text).context("the rules are not valid TOML ([[rule]] tables)")?;
    for rule in &file.rule {
        if let Some(a) = &rule.archive
            && !matches!(a.as_str(), "forever" | "worse")
        {
            anyhow::bail!("archive is \"forever\" or \"worse\", not {a:?}");
        }
    }
    Ok(file.rule)
}

pub fn to_toml(rules: &[Rule]) -> String {
    toml::to_string_pretty(&RuleFile { rule: rules.to_vec() }).unwrap_or_default()
}

pub async fn global_text(hub: &Hub) -> String {
    hub.store.get_value(GLOBAL).await.ok().flatten().unwrap_or_default()
}

pub async fn set_global(hub: &Hub, text: &str) -> Result<()> {
    parse(text)?;
    hub.store.set_value(GLOBAL, text).await
}

/// Adds one rule to the global ones.
pub async fn add_global(hub: &Hub, rule: Rule) -> Result<()> {
    let mut text = global_text(hub).await;
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    if !text.is_empty() {
        text.push('\n');
    }
    text.push_str(&to_toml(&[rule]));
    set_global(hub, &text).await
}

fn file_rules(path: &Path) -> Vec<Rule> {
    let Ok(meta) = std::fs::metadata(path) else { return Vec::new() };
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let mut files = FILES.lock().unwrap_or_else(|p| p.into_inner());
    let files = files.get_or_insert_with(HashMap::new);
    if let Some((at, rules)) = files.get(path)
        && *at == modified
    {
        return rules.clone();
    }
    let rules = std::fs::read_to_string(path).ok().and_then(|t| parse(&t).ok()).unwrap_or_default();
    files.insert(path.to_owned(), (modified, rules.clone()));
    rules
}

async fn folder_of(hub: &Arc<Hub>, project: &str) -> Option<String> {
    let now = now_ms();
    if let Some((at, folder)) = FOLDERS.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).get(project)
        && now - at < 60_000
    {
        return folder.clone();
    }
    let folder = crate::folders::project_folder(&hub.store, project).await.ok();
    FOLDERS.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).insert(project.to_owned(), (now, folder.clone()));
    folder
}

/// The rules for a project: the global ones, then its own file's.
pub async fn rules_for(hub: &Arc<Hub>, project: &str, folder: Option<&str>) -> Vec<Rule> {
    let mut rules = parse(&global_text(hub).await).unwrap_or_default();
    let folder = match folder {
        Some(f) => Some(f.to_owned()),
        None => folder_of(hub, project).await,
    };
    if let Some(folder) = folder {
        let path = Path::new(&folder).join(FILE);
        rules.extend(tokio::task::spawn_blocking(move || file_rules(&path)).await.unwrap_or_default());
    }
    rules
}

/// Hub's own first pass, before any AI: failures whose kind the words alone tell. They go first,
/// so the user's rules win where both say something. Only the sign-in one sets a lane (nobody but
/// the user can sign in again) and only failed tool calls are archived (until they get worse);
/// the rest give Vakten a head start and still go to it.
pub fn built_in() -> Vec<Rule> {
    let rule = |name: &str, pattern: &str, source: Option<&str>, prio: Prio, lane: Option<Lane>, category: &str, group: Option<&str>| Rule {
        name: Some(format!("Hub: {name}")),
        pattern: Some(pattern.into()),
        source: source.map(Into::into),
        prio: Some(prio),
        lane,
        category: Some(category.into()),
        group: group.map(Into::into),
        ..Default::default()
    };
    let mut out = Vec::new();
    for p in ["authentication required", "not logged in", "please log in", "please run /login", "invalid api key", "oauth token has expired", "sign in again"] {
        out.push(rule("sign-in lost", p, None, Prio::P1, Some(Lane::Ask), "config", Some("engine sign-in lost")));
    }
    for p in ["rate limit", "overloaded", "too many requests"] {
        out.push(rule("busy service", p, None, Prio::P3, None, "external", None));
    }
    for p in ["connection refused", "connection reset", "timed out", "could not resolve host", "network is unreachable", "no route to host"] {
        out.push(rule("network", p, None, Prio::P3, None, "environment", None));
    }
    out.push(rule("engine internal error", "internal error", Some("thread"), Prio::P2, None, "external", None));
    // The chat met a file of a kind it cannot show (an Excel sheet, a Blender render). The app
    // reports it as "No viewer for .xlsx" with the extension as its fingerprint; Vakten reads
    // these as a request to build the missing viewer (see its brief).
    out.push(Rule {
        name: Some("Hub: missing viewer".into()),
        pattern: Some("No viewer for *".into()),
        source: Some("app".into()),
        project: Some("Hub".into()),
        prio: Some(Prio::P2),
        category: Some("missing-viewer".into()),
        ..Default::default()
    });
    // An agent's tool call that failed (a grep that found nothing, a missing file) is the agent
    // finding its way, not a fault: counted, archived until it clearly happens more.
    out.push(Rule {
        name: Some("Hub: failed tool calls".into()),
        source: Some("tool".into()),
        prio: Some(Prio::P3),
        category: Some("noise".into()),
        archive: Some("worse".into()),
        ..Default::default()
    });
    out
}

/// What the rules say about a hit coming in: Hub's own first, then the user's.
pub async fn apply(hub: &Arc<Hub>, project: &str, source: &str, level: Level, folder: Option<&str>, hit: &Hit) -> Ruled {
    let mut rules = built_in();
    rules.extend(rules_for(hub, project, folder).await);
    let machine = hub.remotes.local_name().to_owned();
    decide(&rules, project, source, &machine, level, hit)
}

pub(crate) fn decide(rules: &[Rule], project: &str, source: &str, machine: &str, level: Level, hit: &Hit) -> Ruled {
    let mut out = Ruled::default();
    let text = format!("{} {}", hit.kind.as_deref().unwrap_or_default(), hit.message);
    let files: Vec<&str> = hit.frames.iter().filter_map(|f| f.file.as_deref()).collect();
    for (i, rule) in rules.iter().enumerate() {
        let is = |want: &Option<String>, have: &str| want.as_deref().is_none_or(|w| w.eq_ignore_ascii_case(have));
        if !is(&rule.project, project) || !is(&rule.source, source) || !is(&rule.machine, machine) {
            continue;
        }
        if rule.level.is_some_and(|l| level < l) {
            continue;
        }
        if let Some(p) = &rule.pattern
            && !glob(p, &text)
        {
            continue;
        }
        if let Some(p) = &rule.path
            && !files.iter().any(|f| glob(p, f))
        {
            continue;
        }
        if rule.pattern.is_none() && rule.path.is_none() && rule.project.is_none() && rule.source.is_none() {
            // A rule with nothing to match on would take everything; it is more likely half written.
            continue;
        }
        out.names.push(rule.name.clone().unwrap_or_else(|| format!("rule {}", i + 1)));
        if let Some(g) = &rule.group {
            out.fingerprint = Some(vec![format!("rule:{g}")]);
        }
        out.prio = rule.prio.or(out.prio);
        out.lane = rule.lane.or(out.lane);
        out.category = rule.category.clone().or(out.category.take());
        out.archive = match rule.archive.as_deref() {
            Some("forever") => Some(Archive::Forever),
            Some("worse") => Some(Archive::Worse { rate: 0.0 }),
            _ => out.archive.take(),
        };
    }
    out
}

/// `*` for anything, case aside, anywhere in the text.
pub(crate) fn glob(pattern: &str, text: &str) -> bool {
    let parts: Vec<String> = pattern.split('*').map(regex::escape).collect();
    regex::RegexBuilder::new(&parts.join(".*")).case_insensitive(true).build().is_ok_and(|re| re.is_match(text))
}

/// Stamps (or archives) a problem that just came in the way the rules said.
pub async fn stamp_new(hub: &Hub, id: i64, ruled: &Ruled) -> Result<()> {
    let patch = ProblemPatch {
        prio: ruled.prio,
        lane: ruled.lane,
        category: ruled.category.clone(),
        archive: ruled.archive.clone(),
        why: Some(format!("by {}", ruled.names.join(", "))),
        ..Default::default()
    };
    super::change(hub, id, patch, "rule").await.map(|_| ())
}

/// The rules page.
pub async fn page(hub: &Arc<Hub>) -> Result<ProblemRules> {
    let global = global_text(hub).await;
    let mut files = Vec::new();
    for p in hub.store.projects().await.unwrap_or_default() {
        let path = Path::new(&p.path).join(FILE);
        if path.is_file() {
            let rules = file_rules(&path).len();
            files.push(RuleFileInfo { project: p.name, path: path.to_string_lossy().into_owned(), rules });
        }
    }
    let suggestions = suggestions(hub).await.unwrap_or_default();
    Ok(ProblemRules { global, files, suggestions })
}

/// Rules the user's own choices point to: the same stamp, or archiving, three times or more for
/// the same project and source, that no rule does yet.
pub async fn suggestions(hub: &Arc<Hub>) -> Result<Vec<Suggestion>> {
    let rows = hub
        .store
        .with(|c| {
            let mut out = Vec::new();
            let mut q = c.prepare(
                "SELECT project, source, category, lane, COUNT(*) FROM problems
                 WHERE stamped_by = 'you' AND (category IS NOT NULL OR lane IS NOT NULL) AND merged_into IS NULL
                 GROUP BY project, source, category, lane HAVING COUNT(*) >= 3",
            )?;
            for r in q.query_map([], |r| Ok(("stamp", r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, i64>(4)?)))? {
                out.push(r?);
            }
            let mut q = c.prepare(
                "SELECT p.project, p.source, p.level, NULL, COUNT(DISTINCT p.id) FROM problems p
                 JOIN problem_activity a ON a.problem_id = p.id AND a.by = 'you' AND a.kind = 'status' AND a.text LIKE 'Archived%'
                 WHERE p.status = 'archived' AND p.merged_into IS NULL
                 GROUP BY p.project, p.source, p.level HAVING COUNT(DISTINCT p.id) >= 3",
            )?;
            for r in q.query_map([], |r| Ok(("archive", r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, i64>(4)?)))? {
                out.push(r?);
            }
            Ok(out)
        })
        .await?;
    let existing = parse(&global_text(hub).await).unwrap_or_default();
    let mut out = Vec::new();
    for (kind, project, source, a, b, n) in rows {
        let mut rule = Rule { project: Some(project.clone()), source: Some(source.clone()), ..Default::default() };
        let text = if kind == "stamp" {
            rule.category = a.clone();
            rule.lane = b.as_deref().and_then(|l| serde_json::from_value(serde_json::Value::String(l.into())).ok());
            let what: Vec<String> = [a.clone(), b.clone().map(|l| format!("lane {l}"))].into_iter().flatten().collect();
            rule.name = Some(format!("{project} {source}: {}", what.join(", ")));
            format!("Stamp everything from {project}'s {source} as {}", what.join(", "))
        } else {
            rule.level = a.as_deref().map(Level::parse);
            rule.archive = Some("forever".into());
            rule.name = Some(format!("{project} {source} {}s archived", a.as_deref().unwrap_or("problem")));
            format!("Archive {}s from {project}'s {source} as they come", a.as_deref().unwrap_or("problem"))
        };
        let known = existing.iter().any(|r| {
            r.project == rule.project && r.source == rule.source && r.category == rule.category && r.archive == rule.archive && r.level == rule.level
        });
        if !known {
            out.push(Suggestion { text, why: format!("you did it {n} times"), rule });
        }
    }
    Ok(out)
}

/// Keeps the problems tables small: activity older than a year goes.
pub(crate) fn prune(c: &rusqlite::Connection) -> rusqlite::Result<()> {
    c.execute("DELETE FROM problem_activity WHERE ts < ?1", params![now_ms() - 365 * 86_400_000]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_group_and_stamp() {
        let rules = parse(
            r#"
            [[rule]]
            name = "ssh"
            match = "could not reach * over ssh"
            group = "ssh unreachable"
            prio = "P3"
            lane = "ask"

            [[rule]]
            path = "src/remote.rs"
            category = "environment"
            archive = "worse"
            "#,
        )
        .unwrap();
        let hit = |m: &str, file: &str| Hit {
            message: m.into(),
            frames: vec![super::super::Frame { file: Some(file.into()), ..Default::default() }],
            ..Default::default()
        };
        let a = decide(&rules, "Hub", "hubd", "PC", Level::Warning, &hit("could not reach tincan over ssh: timed out", "crates/hub-core/src/remote.rs"));
        assert_eq!(a.fingerprint, Some(vec!["rule:ssh unreachable".to_owned()]));
        assert_eq!((a.prio, a.lane, a.category.as_deref()), (Some(Prio::P3), Some(Lane::Ask), Some("environment")));
        assert!(matches!(a.archive, Some(Archive::Worse { .. })));
        let b = decide(&rules, "Hub", "hubd", "PC", Level::Warning, &hit("disk full", "src/x.rs"));
        assert!(b.is_empty() && b.fingerprint.is_none());
        assert!(parse("[[rule]]\narchive = \"later\"").is_err());
    }

    #[test]
    fn hub_sorts_what_the_words_tell() {
        let hit = |m: &str| Hit { message: m.into(), kind: Some("claude failed".into()), ..Default::default() };
        let rules = built_in();
        let a = decide(&rules, "Hub", "thread", "PC", Level::Error, &hit("Authentication required"));
        assert_eq!((a.prio, a.lane, a.category.as_deref()), (Some(Prio::P1), Some(Lane::Ask), Some("config")));
        assert_eq!(a.fingerprint, Some(vec!["rule:engine sign-in lost".to_owned()]));
        let b = decide(&rules, "Hub", "thread", "PC", Level::Error, &hit("Internal error"));
        assert_eq!((b.prio, b.lane, b.category.as_deref()), (Some(Prio::P2), None, Some("external")));
        // Only a thread's engine failing counts as the engine's internal error.
        assert!(decide(&rules, "Aftermath", "sentry", "PC", Level::Error, &hit("Internal error")).is_empty());
        assert!(decide(&rules, "Hub", "thread", "PC", Level::Error, &hit("index out of range")).is_empty());
        let v = decide(&rules, "Hub", "app", "PC", Level::Warning, &hit("No viewer for .xlsx files"));
        assert_eq!((v.prio, v.lane, v.category.as_deref()), (Some(Prio::P2), None, Some("missing-viewer")), "left for Vakten, not given a lane");
        assert!(decide(&rules, "Aftermath", "sentry", "PC", Level::Warning, &hit("No viewer for .xlsx files")).is_empty());
        let d = decide(&rules, "Hub", "tool", "PC", Level::Warning, &hit("grep: no such file"));
        assert!(matches!(d.archive, Some(Archive::Worse { .. })) && d.prio == Some(Prio::P3));
        // The user's rules come after and win.
        let mut mine = built_in();
        mine.extend(parse("[[rule]]\nmatch = \"internal error\"\nlane = \"quiet\"\ncategory = \"flaky\"").unwrap());
        let c = decide(&mine, "Hub", "thread", "PC", Level::Error, &hit("Internal error"));
        assert_eq!((c.lane, c.category.as_deref()), (Some(Lane::Quiet), Some("flaky")));
    }
}
