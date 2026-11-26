//! A problem's dossier: what a thread working on it should know beyond the stack. The commits
//! that last touched the files in its stack (suspect commits), problems like it, what else went
//! wrong anywhere in the minutes around it, and what was tried before.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Activity, Hit};
use crate::hub::Hub;

/// How far around the latest hit other problems count as "at the same time".
const AROUND_MS: i64 = 5 * 60_000;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Dossier {
    pub suspects: Vec<Suspect>,
    pub similar: Vec<Similar>,
    pub around: Vec<Around>,
    /// Threads, pull requests, comebacks and what Vakten told, oldest first.
    pub attempts: Vec<Activity>,
    /// The folder the commits were read in.
    pub folder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Suspect {
    pub sha: String,
    pub author: String,
    #[ts(type = "number")]
    pub ts: i64,
    pub subject: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Similar {
    #[ts(type = "number")]
    pub id: i64,
    pub title: String,
    pub project: String,
    pub status: super::ProblemStatus,
    #[ts(type = "number")]
    pub count: i64,
    /// "same place", "same words" or both.
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Around {
    #[ts(type = "number")]
    pub ts: i64,
    #[ts(type = "number")]
    pub problem: i64,
    pub project: String,
    pub source: String,
    pub machine: String,
    pub level: super::Level,
    pub message: String,
}

pub async fn dossier(hub: &Arc<Hub>, id: i64) -> Result<Dossier> {
    let detail = super::detail(hub, id, 1).await?.with_context(|| format!("no problem {id}"))?;
    let p = detail.problem.clone();
    let latest: Option<Hit> = detail.hits.first().cloned();
    let folder = match &p.folder {
        Some(f) => Some(f.clone()),
        None => crate::folders::project_folder(&hub.store, &p.project).await.ok(),
    }
    .filter(|f| Path::new(f).is_dir());
    // Files in the stack, the project's own first.
    let mut files: Vec<String> = Vec::new();
    if let Some(hit) = &latest {
        let mut frames: Vec<&super::Frame> = hit.frames.iter().filter(|f| f.own).collect();
        frames.extend(hit.frames.iter().filter(|f| !f.own));
        for f in frames.into_iter().rev() {
            if let Some(file) = &f.file
                && !files.contains(file)
            {
                files.push(file.clone());
            }
        }
    }
    if files.is_empty()
        && let Some(c) = &p.culprit
    {
        files.push(c.split(" in ").next().unwrap_or(c).split(", line").next().unwrap_or(c).to_owned());
    }
    let suspects = match &folder {
        Some(folder) => {
            let (folder, files) = (folder.clone(), files.clone());
            tokio::task::spawn_blocking(move || suspects(Path::new(&folder), &files)).await.unwrap_or_default()
        }
        None => Vec::new(),
    };
    let at = latest.as_ref().map(|h| h.ts).unwrap_or(p.last_seen);
    let (title, culprit, project) = (p.title.clone(), p.culprit.clone(), p.project.clone());
    let (similar, around, attempts) = hub
        .store
        .with(move |c| {
            // Problems like it: in the same place, or with mostly the same words.
            let mut q = c.prepare("SELECT id, title, project, status, count, culprit FROM problems WHERE id != ?1 AND merged_into IS NULL ORDER BY last_seen DESC LIMIT 2000")?;
            let words = word_set(&title);
            let place = culprit.as_deref().map(place_of);
            let mut similar: Vec<(f64, Similar)> = Vec::new();
            let rows = q.query_map([id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<String>>(5)?))
            })?;
            for row in rows {
                let (oid, otitle, oproject, status, count, oculprit) = row?;
                let same_place = place.is_some() && oculprit.as_deref().map(place_of) == place;
                let overlap = jaccard(&words, &word_set(&otitle));
                let same_words = overlap >= 0.5;
                if !same_place && !same_words {
                    continue;
                }
                let why = match (same_place, same_words) {
                    (true, true) => "same place and words",
                    (true, false) => "same place",
                    _ => "same words",
                };
                let score = overlap + if same_place { 0.6 } else { 0.0 } + if oproject == project { 0.1 } else { 0.0 };
                similar.push((score, Similar { id: oid, title: otitle, project: oproject, status: super::ProblemStatus::parse(&status), count, why: why.into() }));
            }
            similar.sort_by(|a, b| b.0.total_cmp(&a.0));
            let similar: Vec<Similar> = similar.into_iter().take(8).map(|(_, s)| s).collect();
            // Everything else that went wrong just before and after, from any source.
            let mut q = c.prepare(
                "SELECT h.ts, h.problem_id, p.project, p.source, p.machine, p.level, json_extract(h.data, '$.message')
                 FROM problem_hits h JOIN problems p ON p.id = h.problem_id
                 WHERE h.problem_id != ?1 AND h.ts BETWEEN ?2 AND ?3 ORDER BY h.ts LIMIT 40",
            )?;
            let around = q
                .query_map(params![id, at - AROUND_MS, at + AROUND_MS], |r| {
                    let message: Option<String> = r.get(6)?;
                    Ok(Around {
                        ts: r.get(0)?,
                        problem: r.get(1)?,
                        project: r.get(2)?,
                        source: r.get(3)?,
                        machine: r.get(4)?,
                        level: super::Level::parse(&r.get::<_, String>(5)?),
                        message: message.unwrap_or_default().lines().next().unwrap_or_default().chars().take(300).collect(),
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut q = c.prepare(
                "SELECT ts, by, kind, text FROM problem_activity WHERE problem_id = ?1
                   AND kind IN ('thread', 'pr', 'regressed', 'told', 'status', 'merged') ORDER BY ts, rowid",
            )?;
            let attempts = q
                .query_map([id], |r| Ok(Activity { ts: r.get(0)?, by: r.get(1)?, kind: r.get(2)?, text: r.get(3)? }))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((similar, around, attempts))
        })
        .await?;
    Ok(Dossier { suspects, similar, around, attempts, folder })
}

/// The last commits that touched each file in the stack, newest first.
fn suspects(folder: &Path, files: &[String]) -> Vec<Suspect> {
    let mut by_sha: HashMap<String, Suspect> = HashMap::new();
    for file in files.iter().take(6) {
        // A path from the stack may be absolute or start above the repository; git wants it from the folder.
        let rel = relative(folder, file);
        let Some(out) = crate::git::run(folder, &["log", "-n", "3", "--format=%H%x1f%an%x1f%at%x1f%s", "--", &rel]) else { continue };
        for line in out.lines() {
            let parts: Vec<&str> = line.split('\u{1f}').collect();
            let [sha, author, at, subject] = parts.as_slice() else { continue };
            let s = by_sha.entry(sha.to_string()).or_insert_with(|| Suspect {
                sha: sha.chars().take(10).collect(),
                author: author.to_string(),
                ts: at.parse::<i64>().unwrap_or(0) * 1000,
                subject: subject.to_string(),
                files: Vec::new(),
            });
            if !s.files.contains(&rel) {
                s.files.push(rel.clone());
            }
        }
    }
    let mut out: Vec<Suspect> = by_sha.into_values().collect();
    out.sort_by_key(|x| std::cmp::Reverse(x.ts));
    out.truncate(8);
    out
}

fn relative(folder: &Path, file: &str) -> String {
    let path = Path::new(file);
    if let Ok(rest) = path.strip_prefix(folder) {
        return rest.to_string_lossy().replace('\\', "/");
    }
    let file = file.replace('\\', "/");
    // Find the longest tail of the path that exists in the folder.
    let parts: Vec<&str> = file.split('/').filter(|p| !p.is_empty()).collect();
    for start in 0..parts.len() {
        let tail = parts[start..].join("/");
        if folder.join(&tail).exists() {
            return tail;
        }
    }
    file
}

fn place_of(culprit: &str) -> String {
    culprit.split(", line").next().unwrap_or(culprit).to_owned()
}

fn word_set(text: &str) -> HashSet<String> {
    super::normalize(text)
        .split(|c: char| !c.is_alphanumeric() && c != '<' && c != '>')
        .filter(|w| w.len() > 2)
        .map(str::to_lowercase)
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.intersection(b).count() as f64 / a.union(b).count() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_alike() {
        let a = word_set("could not reach tincan over ssh: timed out");
        let b = word_set("could not reach clanker over ssh: refused");
        assert!(jaccard(&a, &b) >= 0.5);
        assert!(jaccard(&a, &word_set("KeyError: 'session_id'")) < 0.2);
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "").unwrap();
        assert_eq!(relative(dir.path(), "/home/ci/build/repo/src/a.rs"), "src/a.rs");
    }
}
