//! Changing a problem: one `ProblemPatch` (stamps, note, status and archive, merge, unmerge) applied
//! in one transaction with each change noted in its activity, by the user, Vakten or a thread; a
//! code finding is resolved only by an analysis (`analysis.rs`). Also linking a problem to the
//! thread working on it and the brief that thread is told.

use super::*;

/// Everything that can be changed on a problem, by the user ("you"), Vakten or a thread. Each
/// change is noted in its activity.
pub async fn change(hub: &Hub, id: i64, patch: ProblemPatch, by: &str) -> Result<Problem> {
    let by = by.to_owned();
    let merged = hub
        .store
        .with(move |c| {
            let tx = c.transaction()?;
            let exists: Option<String> = tx.query_row("SELECT status FROM problems WHERE id = ?1", [id], |r| r.get(0)).optional()?;
            let Some(was) = exists else { bail!("no problem {id}") };
            let source: String = tx.query_row("SELECT source FROM problems WHERE id = ?1", [id], |r| r.get(0))?;
            if source == analysis::SOURCE && (patch.resolve.is_some() || patch.status == Some(ProblemStatus::Resolved) || patch.merge_into.is_some()) {
                bail!("code findings are resolved only by a successful comparable analysis; archive to dismiss");
            }
            if let Some(target) = patch.merge_into {
                let source: Option<String> = tx.query_row("SELECT source FROM problems WHERE id = ?1", [target], |r| r.get(0)).optional()?;
                if source.as_deref() == Some(analysis::SOURCE) { bail!("cannot merge a runtime problem into a code finding"); }
            }
            let stamped_before: Option<String> = tx.query_row("SELECT stamped_by FROM problems WHERE id = ?1", [id], |r| r.get(0))?;
            let mut said: Vec<(String, String)> = Vec::new();
            apply_stamps(&tx, id, &patch, &by, stamped_before.as_deref(), &mut said)?;
            apply_note(&tx, id, &patch, &mut said)?;
            if patch.reviewed == Some(true) {
                tx.execute("UPDATE problems SET reviewed = 1 WHERE id = ?1", [id])?;
            }
            apply_status(&tx, id, &patch, &by, &was, &mut said)?;
            let merged = apply_merge(&tx, id, &patch, &by, &mut said)?;
            apply_unmerge(&tx, id, &patch, &by, &mut said)?;
            for (kind, text) in &said {
                log(&tx, id, &by, kind, text)?;
            }
            tx.commit()?;
            Ok(merged)
        })
        .await?;
    if let Some(into) = merged
        && let Some(p) = get(hub, into).await?
    {
        hub.publish(StreamMsg::Problem { problem: p });
    }
    let problem = get(hub, id).await?.context("the problem is gone")?;
    hub.publish(StreamMsg::Problem { problem: problem.clone() });
    Ok(problem)
}

/// "Until worse" keeps the daily rate the problem had when it was archived, to compare with.
pub(super) fn archive_with_rate(c: &rusqlite::Connection, id: i64, archive: Archive) -> rusqlite::Result<Archive> {
    Ok(match archive {
        Archive::Worse { .. } => {
            let today = now_ms().div_euclid(DAY_MS);
            let week: i64 = c.query_row(
                "SELECT COALESCE(SUM(count), 0) FROM problem_days WHERE problem_id = ?1 AND day > ?2",
                params![id, today - 7],
                |r| r.get(0),
            )?;
            Archive::Worse { rate: week as f64 / 7.0 }
        }
        other => other,
    })
}

pub(super) fn describe_archive(archive: &Archive) -> String {
    match archive {
        Archive::Worse { .. } => "until it gets worse".into(),
        Archive::Time { at } => {
            let hours = ((*at - now_ms()) as f64 / 3_600_000.0).round().max(1.0) as i64;
            if hours < 48 { format!("for {hours} hours") } else { format!("for {} days", (hours + 12) / 24) }
        }
        Archive::Count { count } => format!("until it has happened {count} times"),
        Archive::Forever => "for good".into(),
    }
}

pub async fn set_thread(hub: &Hub, id: i64, thread: &str, by: &str) -> Result<()> {
    let (thread, by) = (thread.to_owned(), by.to_owned());
    hub.store
        .with(move |c| {
            c.execute("UPDATE problems SET thread_id = ?2, reviewed = 1 WHERE id = ?1", params![id, thread])?;
            log(c, id, &by, "thread", &format!("Started a thread on it: hub://thread/{thread}"))?;
            Ok(())
        })
        .await?;
    if let Some(problem) = get(hub, id).await? {
        hub.publish(StreamMsg::Problem { problem });
    }
    Ok(())
}

/// What a thread started on a problem is told: enough to start, and how to read more.
pub fn brief(detail: &ProblemDetail) -> String {
    let p = &detail.problem;
    let mut out = format!(
        "Hub caught this problem and I want it fixed, or explained if it is not a bug.\n\n\
         **{}**\n- Project: {} · source: {} · level: {} · machine: {}\n- Seen {} times, last at {} (Unix ms {})\n",
        p.title,
        p.project,
        p.source,
        p.level.as_str(),
        p.machine,
        p.count,
        p.last_seen,
        p.last_seen
    );
    if let Some(culprit) = &p.culprit {
        out.push_str(&format!("- Where: {culprit}\n"));
    }
    if let Some(hit) = detail.hits.first() {
        if hit.message.trim() != p.title.trim() {
            out.push_str(&format!("\nMessage:\n```\n{}\n```\n", hit.message.trim()));
        }
        if !hit.frames.is_empty() {
            out.push_str("\nStack (innermost last):\n```\n");
            for f in hit.frames.iter().rev().take(15).rev() {
                out.push_str(&format!(
                    "{}{} in {}{}\n",
                    if f.own { "" } else { "(lib) " },
                    f.file.as_deref().unwrap_or("?"),
                    f.function.as_deref().unwrap_or("?"),
                    f.line.map(|l| format!(", line {l}")).unwrap_or_default()
                ));
            }
            out.push_str("```\n");
        } else if let Some(trace) = &hit.trace {
            out.push_str(&format!("\nStack:\n```\n{}\n```\n", trace.lines().take(20).collect::<Vec<_>>().join("\n")));
        }
        if !hit.crumbs.is_empty() {
            out.push_str("\nWhat happened just before:\n```\n");
            for c in hit.crumbs.iter().rev().take(10).rev() {
                out.push_str(&format!("{} {}\n", c.level.as_deref().unwrap_or(""), c.message));
            }
            out.push_str("```\n");
        }
    }
    if let Some(note) = &p.note {
        out.push_str(&format!("\nVakten's reading: {} Likely cause: {} Plan: {}\n", note.what, note.cause, note.plan));
    }
    out.push_str(&format!(
        "\nRead more with Hub's tools `problem` (id {id}) and `problem_dossier` (suspect commits, problems like it, what else \
         went wrong at the same time, earlier attempts). Find the cause before changing code, then fix it the smallest way that holds.",
        id = p.id
    ));
    out
}

/// Priority, lane, category, confidence and fixable of a patch, stamped by `by`.
fn apply_stamps(tx: &rusqlite::Transaction, id: i64, patch: &ProblemPatch, by: &str, stamped_before: Option<&str>, said: &mut Vec<(String, String)>) -> Result<()> {
        let mut stamped = Vec::new();
        if let Some(prio) = patch.prio {
            tx.execute("UPDATE problems SET prio = ?2 WHERE id = ?1", params![id, format!("{prio:?}")])?;
            stamped.push(format!("{prio:?}"));
        }
        if let Some(lane) = patch.lane {
            let name = match lane {
                Lane::Quiet => "quiet",
                Lane::Tell => "tell",
                Lane::Ask => "ask",
            };
            tx.execute("UPDATE problems SET lane = ?2 WHERE id = ?1", params![id, name])?;
            stamped.push(match lane {
                Lane::Quiet => "quiet".into(),
                Lane::Tell => "do and tell".into(),
                Lane::Ask => "ask first".into(),
            });
        }
        for (col, value) in [("category", &patch.category), ("confidence", &patch.confidence), ("fixable", &patch.fixable)] {
            if let Some(v) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                tx.execute(&format!("UPDATE problems SET {col} = ?2 WHERE id = ?1"), params![id, v.to_lowercase()])?;
                stamped.push(if col == "category" { v.to_lowercase() } else { format!("{col} {}", v.to_lowercase()) });
            }
        }
        if !stamped.is_empty() {
            tx.execute("UPDATE problems SET stamped_by = ?2 WHERE id = ?1", params![id, by])?;
            let why = patch.why.as_deref().map(|w| format!(": {w}")).unwrap_or_default();
            let verb = if by == "you" && stamped_before == Some("Vakten") { "Changed Vakten's stamps to" } else { "Stamped" };
            said.push(("stamped".into(), format!("{verb} {}{why}", stamped.join(", "))));
        }
    Ok(())
}

/// A note adds to the one there: a field left empty keeps what it said.
fn apply_note(tx: &rusqlite::Transaction, id: i64, patch: &ProblemPatch, said: &mut Vec<(String, String)>) -> Result<()> {
        if let Some(given) = &patch.note {
            // A note adds to the one there: a field left empty keeps what it said.
            let old: Option<String> = tx.query_row("SELECT summary FROM problems WHERE id = ?1", [id], |r| r.get(0))?;
            let mut note: Note = old.and_then(|o| serde_json::from_str(&o).ok()).unwrap_or_default();
            for (field, value) in [
                (&mut note.what, &given.what),
                (&mut note.cause, &given.cause),
                (&mut note.plan, &given.plan),
                (&mut note.next, &given.next),
                (&mut note.repro, &given.repro),
                (&mut note.done, &given.done),
            ] {
                if !value.trim().is_empty() {
                    *field = value.clone();
                }
            }
            tx.execute("UPDATE problems SET summary = ?2 WHERE id = ?1", params![id, serde_json::to_string(&note)?])?;
            said.push(("note".into(), format!("Wrote what it makes of it: {}", if given.what.is_empty() { &note.what } else { &given.what })));
        }
    Ok(())
}

/// Resolve, archive or set the status of a patch.
fn apply_status(tx: &rusqlite::Transaction, id: i64, patch: &ProblemPatch, by: &str, was: &str, said: &mut Vec<(String, String)>) -> Result<()> {
        if let Some(how) = patch.resolve {
            let releases: Option<String> = match how {
                Resolve::Now => None,
                Resolve::NextRelease => {
                    let mut q = tx.prepare("SELECT DISTINCT COALESCE(json_extract(data, '$.release'), '') FROM problem_hits WHERE problem_id = ?1")?;
                    let seen: Vec<String> = q.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
                    Some(serde_json::to_string(&seen)?)
                }
            };
            tx.execute(
                "UPDATE problems SET status = 'resolved', regressed = 0, reviewed = 1, resolved_at = ?2, resolved_by = ?3,
                    resolved_releases = ?4, archive = NULL, escalating = 0 WHERE id = ?1",
                params![id, now_ms(), by, releases],
            )?;
            said.push(("status".into(), match how {
                Resolve::Now => "Resolved: it comes back if it happens again".into(),
                Resolve::NextRelease => "Resolved in the next release: hits from the releases it was seen in do not bring it back".into(),
            }));
        } else if let Some(archive) = &patch.archive {
            tx.execute(
                "UPDATE problems SET status = 'archived', reviewed = 1, archive = ?2, regressed = 0 WHERE id = ?1",
                params![id, serde_json::to_string(&archive_with_rate(tx, id, archive.clone())?)?],
            )?;
            said.push(("status".into(), format!("Archived {}", describe_archive(archive))));
        } else if let Some(status) = patch.status {
            match status {
                ProblemStatus::Open => {
                    tx.execute(
                        "UPDATE problems SET status = 'open', archive = NULL, resolved_at = NULL, resolved_releases = NULL WHERE id = ?1",
                        [id],
                    )?;
                    if was != "open" {
                        said.push(("status".into(), "Opened again".into()));
                    }
                }
                ProblemStatus::Resolved => {
                    tx.execute(
                        "UPDATE problems SET status = 'resolved', regressed = 0, reviewed = 1, resolved_at = ?2, resolved_by = ?3,
                            resolved_releases = NULL, archive = NULL WHERE id = ?1",
                        params![id, now_ms(), by],
                    )?;
                    said.push(("status".into(), "Resolved".into()));
                }
                ProblemStatus::Archived => {
                    tx.execute(
                        "UPDATE problems SET status = 'archived', reviewed = 1, regressed = 0, archive = ?2 WHERE id = ?1",
                        params![id, serde_json::to_string(&Archive::Forever)?],
                    )?;
                    said.push(("status".into(), "Archived for good".into()));
                }
            }
        }
    Ok(())
}

/// Merging: this problem's hits and counts go to the other, and so do later hits. Returns the
/// problem it merged into.
fn apply_merge(tx: &rusqlite::Transaction, id: i64, patch: &ProblemPatch, by: &str, said: &mut Vec<(String, String)>) -> Result<Option<i64>> {
    let mut merged = None;
        if let Some(into) = patch.merge_into.filter(|into| *into != id) {
            let target: Option<i64> = tx.query_row("SELECT id FROM problems WHERE id = ?1 AND merged_into IS NULL", [into], |r| r.get(0)).optional()?;
            let Some(into) = target else { bail!("no problem {into} to merge into") };
            let (fp, title, count, first, last): (String, String, i64, i64, i64) = tx.query_row(
                "SELECT fingerprint, title, count, first_seen, last_seen FROM problems WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
            tx.execute("INSERT OR REPLACE INTO problem_aliases (fingerprint, problem_id) VALUES (?1, ?2)", params![fp, into])?;
            tx.execute("UPDATE problem_aliases SET problem_id = ?2 WHERE problem_id = ?1", params![id, into])?;
            tx.execute("UPDATE problem_hits SET problem_id = ?2 WHERE problem_id = ?1", params![id, into])?;
            tx.execute(
                "INSERT INTO problem_days (problem_id, day, count) SELECT ?2, day, count FROM problem_days WHERE problem_id = ?1
                 ON CONFLICT(problem_id, day) DO UPDATE SET count = problem_days.count + excluded.count",
                params![id, into],
            )?;
            tx.execute(
                "UPDATE problems SET count = count + ?2, first_seen = MIN(first_seen, ?3), last_seen = MAX(last_seen, ?4) WHERE id = ?1",
                params![into, count, first, last],
            )?;
            tx.execute("UPDATE problems SET merged_into = ?2, status = 'resolved', resolved_by = ?3 WHERE id = ?1", params![id, into, by])?;
            log(tx, into, by, "merged", &format!("Took in #{id} \"{title}\" ({count} hits)"))?;
            said.push(("merged".into(), format!("Merged into #{into}")));
            merged = Some(into);
        }
    Ok(merged)
}

/// Lets a merged problem go again; its earlier hits stay with the other.
fn apply_unmerge(tx: &rusqlite::Transaction, id: i64, patch: &ProblemPatch, by: &str, said: &mut Vec<(String, String)>) -> Result<()> {
        if patch.unmerge == Some(true) {
            let into: Option<i64> = tx.query_row("SELECT merged_into FROM problems WHERE id = ?1", [id], |r| r.get(0))?;
            if let Some(into) = into {
                let fp: String = tx.query_row("SELECT fingerprint FROM problems WHERE id = ?1", [id], |r| r.get(0))?;
                tx.execute("DELETE FROM problem_aliases WHERE fingerprint = ?1", [&fp])?;
                tx.execute("UPDATE problems SET merged_into = NULL, status = 'open' WHERE id = ?1", [id])?;
                log(tx, into, by, "merged", &format!("Let #{id} go again; its earlier hits stay here"))?;
                said.push(("merged".into(), format!("Split off from #{into} again; new hits count here")));
            }
        }
    Ok(())
}
