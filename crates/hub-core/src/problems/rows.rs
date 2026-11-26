//! A problem from a hit and from its row. First what makes two hits the same problem (fingerprint
//! from own frames or a normalized message) and its title and culprit; then reading a `problems`
//! row with its last seven days, and what is read live, not kept: its fix thread's pull request
//! and whether it needs the user.

use super::*;

/// Same project, source and fingerprint words: the same problem. Without words of its own, a
/// problem is known by its error's type and the project's own frames (not their line numbers,
/// which move with every edit), and without a stack by its message with numbers and ids taken out.
pub(super) fn fingerprint(project: &str, source: &str, given: Option<&[String]>, hit: &Hit) -> String {
    let default = || {
        let mut parts = vec![hit.kind.clone().unwrap_or_default()];
        let own: Vec<&Frame> = hit.frames.iter().filter(|f| f.own).collect();
        let frames = if own.is_empty() { hit.frames.iter().collect() } else { own };
        let frames: Vec<String> = frames
            .iter()
            .rev()
            .take(5)
            .map(|f| format!("{}:{}", f.file.as_deref().unwrap_or_default(), f.function.as_deref().unwrap_or_default()))
            .collect();
        if frames.iter().all(|f| f == ":") {
            parts.push(normalize(&hit.message));
        } else {
            parts.extend(frames);
            // A log line has one frame, where it was logged: one call site can say different things.
            if hit.kind.is_none() {
                parts.push(normalize(&hit.message));
            }
        }
        parts
    };
    let parts: Vec<String> = match given {
        Some(words) if !words.is_empty() => {
            words.iter().flat_map(|w| if w == "{{ default }}" || w == "{{default}}" { default() } else { vec![w.clone()] }).collect()
        }
        _ => default(),
    };
    let mut hash = Sha256::new();
    for part in [project, source].into_iter().chain(parts.iter().map(String::as_str)) {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    hash.finalize().iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// A message with what changes from one time to the next (numbers, ids, quoted names, paths)
/// taken out, so the same error with another file or id groups together.
pub(crate) fn normalize(message: &str) -> String {
    static RES: OnceLock<Vec<(regex::Regex, &str)>> = OnceLock::new();
    let res = RES.get_or_init(|| {
        [
            (r#""[^"]*"|'[^']*'|`[^`]*`"#, "<s>"),
            (r"(?:[A-Za-z]:)?[\\/][^\s:,;()]+", "<path>"),
            (r"\b[0-9a-fA-F]{8,}\b|\b[0-9a-fA-F-]{36}\b", "<id>"),
            (r"\d+", "<n>"),
        ]
        .into_iter()
        .map(|(re, to)| (regex::Regex::new(re).expect("problem pattern"), to))
        .collect()
    });
    let first = message.lines().next().unwrap_or_default();
    let mut out = first.to_owned();
    for (re, to) in res {
        out = re.replace_all(&out, *to).into_owned();
    }
    out.chars().take(200).collect()
}

pub(super) fn title(hit: &Hit) -> String {
    let first = hit.message.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default();
    let title = match hit.kind.as_deref() {
        Some(kind) if !first.is_empty() && !first.starts_with(kind) => format!("{kind}: {first}"),
        Some(kind) if first.is_empty() => kind.to_owned(),
        _ => first.to_owned(),
    };
    let title = if title.is_empty() { "(no message)".to_owned() } else { title };
    title.chars().take(200).collect()
}

pub(super) fn culprit(frames: &[Frame]) -> Option<String> {
    let frame = frames.iter().rev().find(|f| f.own).or_else(|| frames.last())?;
    let file = frame.file.as_deref()?;
    let mut out = file.to_owned();
    if let Some(function) = &frame.function {
        out.push_str(&format!(" in {function}"));
    }
    if let Some(line) = frame.line {
        out.push_str(&format!(", line {line}"));
    }
    Some(out)
}

// ---------------------------------------------------------------------------------------------
// Reading and changing

pub(super) fn row(r: &rusqlite::Row) -> rusqlite::Result<Problem> {
    let json = |col: &str| -> rusqlite::Result<Option<String>> { r.get::<_, Option<String>>(col) };
    let prio = json("prio")?.and_then(|p| serde_json::from_value(Value::String(p)).ok());
    let lane = json("lane")?.and_then(|l| serde_json::from_value(Value::String(l)).ok());
    Ok(Problem {
        id: r.get("id")?,
        project: r.get("project")?,
        source: r.get("source")?,
        level: Level::parse(&r.get::<_, String>("level")?),
        title: r.get("title")?,
        culprit: r.get("culprit")?,
        folder: r.get("folder")?,
        machine: r.get("machine")?,
        first_seen: r.get("first_seen")?,
        last_seen: r.get("last_seen")?,
        count: r.get("count")?,
        status: ProblemStatus::parse(&r.get::<_, String>("status")?),
        regressed: r.get::<_, i64>("regressed")? != 0,
        thread: r.get("thread_id")?,
        days: Vec::new(),
        prio,
        lane,
        category: r.get("category")?,
        confidence: r.get("confidence")?,
        fixable: r.get("fixable")?,
        note: json("summary")?.and_then(|n| serde_json::from_str(&n).ok()),
        stamped_by: r.get("stamped_by")?,
        archive: json("archive")?.and_then(|a| serde_json::from_str(&a).ok()),
        resolved_at: r.get("resolved_at")?,
        resolved_by: r.get("resolved_by")?,
        next_release: json("resolved_releases")?.is_some(),
        pr: json("pr")?.and_then(|p| serde_json::from_str(&p).ok()),
        merged_into: r.get("merged_into")?,
        escalating: r.get::<_, i64>("escalating")? != 0,
        new: false,
        needs_you: false,
    })
}

pub(super) fn with_days(c: &rusqlite::Connection, mut problems: Vec<Problem>) -> rusqlite::Result<Vec<Problem>> {
    let today = now_ms().div_euclid(DAY_MS);
    let mut stmt = c.prepare("SELECT day, count FROM problem_days WHERE problem_id = ?1 AND day > ?2")?;
    let reviewed: std::collections::HashSet<i64> = {
        let mut q = c.prepare("SELECT id FROM problems WHERE reviewed = 1")?;
        q.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    for p in &mut problems {
        let mut days = vec![0; 7];
        for row in stmt.query_map(params![p.id, today - 7], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (day, count) = row?;
            if let Some(slot) = days.get_mut((6 - (today - day)) as usize) {
                *slot = count;
            }
        }
        p.days = days;
        p.new = now_ms() - p.first_seen < 7 * DAY_MS && !reviewed.contains(&p.id);
    }
    Ok(problems)
}

/// What is not kept but read live: the pull request of the thread on it, and whether it needs the
/// user (which depends on whether Vakten is on).
pub(super) fn finish(hub: &Hub, vakten_on: bool, problems: &mut [Problem]) {
    let prs = hub.thread_prs();
    for p in problems.iter_mut() {
        if let Some(pr) = p.thread.as_ref().and_then(|t| prs.get(t)) {
            p.pr = Some(ProblemPr { number: pr.number, url: pr.url.clone(), state: pr.state.clone() });
        }
        p.needs_you = needs_you(p, vakten_on);
    }
}

pub(super) fn needs_you(p: &Problem, vakten_on: bool) -> bool {
    if p.status != ProblemStatus::Open || p.merged_into.is_some() {
        return false;
    }
    let urgent = matches!(p.prio, Some(Prio::P0 | Prio::P1));
    p.lane == Some(Lane::Ask)
        || p.regressed
        || (urgent && p.thread.is_none())
        || (p.prio.is_none() && p.level == Level::Error && p.thread.is_none() && !vakten_on)
}
