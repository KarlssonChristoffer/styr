//! Claude Code's own record of your work: every session is a JSONL file under
//! `~/.claude/projects`. Hub copies what the activity map needs (prompts and tokens with their
//! time) into its database as the files grow, because Claude Code deletes old sessions after 30
//! days by default.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{days_from_civil, first_line};
pub use crate::model::rows::{CcMark, CcSession};



/// Where Claude Code keeps its sessions.
pub fn projects_dir() -> Option<PathBuf> {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| {
        std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(|h| PathBuf::from(h).join(".claude"))
    })?;
    Some(dir.join("projects"))
}

/// What a file has gained since `offset`: its new marks and sessions, and the offset to read
/// from next time. Only whole lines are read, so a line being written is picked up later. A file
/// smaller than `offset` was rewritten and is read again from the start.
pub fn read_from(path: &Path, offset: u64) -> std::io::Result<(Vec<CcMark>, Vec<CcSession>, u64)> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let offset = if len < offset { 0 } else { offset };
    if len == offset {
        return Ok((Vec::new(), Vec::new(), offset));
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = Vec::with_capacity((len - offset) as usize);
    file.take(len - offset).read_to_end(&mut bytes)?;
    let Some(end) = bytes.iter().rposition(|b| *b == b'\n') else {
        return Ok((Vec::new(), Vec::new(), offset));
    };
    let (marks, sessions) = parse(&String::from_utf8_lossy(&bytes[..end]));
    Ok((marks, sessions, offset + end as u64 + 1))
}

pub fn parse(text: &str) -> (Vec<CcMark>, Vec<CcSession>) {
    let mut marks = Vec::new();
    let mut sessions: Vec<CcSession> = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let Some(session_id) = v["sessionId"].as_str() else { continue };
        let i = match sessions.iter().position(|s| s.id == session_id) {
            Some(i) => i,
            None => {
                sessions.push(CcSession { id: session_id.to_owned(), ..Default::default() });
                sessions.len() - 1
            }
        };
        if let Some(cwd) = v["cwd"].as_str() {
            sessions[i].cwd.get_or_insert_with(|| cwd.to_owned());
        }
        if v["type"] == "ai-title" {
            if let Some(title) = v["aiTitle"].as_str() {
                sessions[i].title = Some(title.to_owned());
            }
            continue;
        }
        if v["isMeta"] == true {
            continue;
        }
        // A side chain is a subagent's own conversation. Its prompts are the agent's, not yours,
        // but its answers are use like any other and count towards spend.
        let side = v["isSidechain"] == true;
        let Some(ts) = v["timestamp"].as_str().and_then(parse_time) else { continue };
        match v["type"].as_str() {
            Some("user") if !side => {
                let Some(text) = prompt_text(&v["message"]["content"]) else { continue };
                let Some(uuid) = v["uuid"].as_str() else { continue };
                marks.push(CcMark {
                    key: uuid.to_owned(),
                    session_id: session_id.to_owned(),
                    ts,
                    prompt: Some(first_line(text)),
                    ..Default::default()
                });
            }
            Some("assistant") => {
                let m = &v["message"];
                let Some(id) = m["id"].as_str() else { continue };
                let u = &m["usage"];
                let n = |k: &str| u[k].as_i64().unwrap_or(0);
                marks.push(CcMark {
                    key: format!("msg:{id}"),
                    session_id: session_id.to_owned(),
                    ts,
                    prompt: None,
                    tokens: n("input_tokens") + n("cache_creation_input_tokens") + n("output_tokens"),
                    // "<synthetic>" marks answers Claude Code wrote itself, such as errors.
                    model: m["model"].as_str().filter(|m| !m.starts_with('<')).map(str::to_owned),
                    input: n("input_tokens"),
                    output: n("output_tokens"),
                    cache_write: n("cache_creation_input_tokens"),
                    cache_read: n("cache_read_input_tokens"),
                });
            }
            _ => {}
        }
    }
    (marks, sessions)
}

/// The text of a prompt you typed. Tool results, slash command plumbing and shell output are
/// also stored as user lines, and are not prompts.
fn prompt_text(content: &Value) -> Option<&str> {
    let text = match content {
        Value::String(s) => s.as_str(),
        Value::Array(parts) => parts.iter().find(|p| p["type"] == "text")?["text"].as_str()?,
        _ => return None,
    };
    let t = text.trim_start();
    let plumbing = ["<command-", "<local-command", "<bash-", "<system-reminder>", "<task-notification>", "Caveat:"];
    if t.is_empty() || plumbing.iter().any(|p| t.starts_with(p)) || t.starts_with("[Request interrupted") {
        return None;
    }
    Some(text)
}

/// `2026-10-29T23:28:07.123Z` in ms since the epoch. Claude Code always writes UTC.
fn parse_time(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let ms = if b.get(19) == Some(&b'.') {
        let frac: String = s[20..].chars().take_while(char::is_ascii_digit).take(3).collect();
        format!("{frac:0<3}").parse::<i64>().ok()?
    } else {
        0
    };
    Some(((days_from_civil(y, mo, d) * 24 + h) * 60 + mi) * 60_000 + sec * 1000 + ms)
}

/// Every session file, with the path used as its key. Subagents write their own files under
/// `<project>/<session>/subagents/`; those are read too, since what they spend is the session's.
pub fn files(dir: &Path) -> Vec<PathBuf> {
    let Ok(projects) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for project in projects.flatten() {
        let Ok(entries) = std::fs::read_dir(project.path()) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "jsonl") {
                out.push(p);
            } else if p.is_dir() {
                jsonl_in(&p.join("subagents"), &mut out);
            }
        }
    }
    out
}

fn jsonl_in(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "jsonl") {
            out.push(p);
        }
    }
}

/// Copies what Claude Code's session files gained since last time into the database. An error
/// says it came from Claude Code ("Claude Code: ..."), as the activity page's notes show it.
pub async fn import(store: &crate::store::Store) -> anyhow::Result<()> {
    use anyhow::Context;
    import_files(store).await.context("Claude Code")
}

async fn import_files(store: &crate::store::Store) -> anyhow::Result<()> {
    let Some(dir) = projects_dir().filter(|d| d.is_dir()) else { return Ok(()) };
    let offsets = store.cc_offsets().await?;
    let (marks, sessions, moved) = tokio::task::spawn_blocking(move || {
        let (mut marks, mut sessions, mut moved) = (Vec::new(), Vec::new(), Vec::new());
        for path in files(&dir) {
            let key = path.display().to_string();
            let at = offsets.get(&key).copied().unwrap_or(0);
            if let Ok((m, s, next)) = read_from(&path, at)
                && next != at {
                    marks.extend(m);
                    sessions.extend(s);
                    moved.push((key, next));
                }
        }
        (marks, sessions, moved)
    })
    .await?;
    if !moved.is_empty() {
        store.cc_import(marks, sessions, moved).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: &str = r#"{"type":"user","sessionId":"s1","cwd":"/work/atlas","uuid":"u1","timestamp":"2026-10-29T23:28:07.120Z","message":{"role":"user","content":"Fix the login\nmore detail"}}
{"type":"user","sessionId":"s1","uuid":"u2","isMeta":true,"timestamp":"2026-10-29T23:28:08Z","message":{"content":"meta"}}
{"type":"user","sessionId":"s1","uuid":"u3","timestamp":"2026-10-29T23:28:09Z","message":{"content":[{"type":"tool_result","content":"ok"}]}}
{"type":"user","sessionId":"s1","uuid":"u4","timestamp":"2026-10-29T23:28:10Z","message":{"content":"<command-name>/clear</command-name>"}}
{"type":"assistant","sessionId":"s1","uuid":"a1","timestamp":"2026-10-29T23:28:11Z","message":{"id":"m1","model":"claude-opus-4-8","usage":{"input_tokens":2,"cache_creation_input_tokens":100,"cache_read_input_tokens":5000,"output_tokens":30}}}
{"type":"assistant","sessionId":"s1","uuid":"a2","isSidechain":true,"timestamp":"2026-10-29T23:28:12Z","message":{"id":"m2","usage":{"output_tokens":9}}}
{"type":"ai-title","sessionId":"s1","aiTitle":"Login fix"}
not json
"#;

    #[test]
    fn prompts_and_answers_are_read_and_plumbing_is_not() {
        let (marks, sessions) = parse(LINES);
        assert_eq!(marks.len(), 3, "{marks:?}");
        // A subagent's answer counts as use; its prompts do not show as yours.
        assert_eq!((marks[2].key.as_str(), marks[2].output), ("msg:m2", 9));
        assert_eq!(marks[0].prompt.as_deref(), Some("Fix the login"));
        assert_eq!(marks[0].ts % 1000, 120);
        assert_eq!(super::super::date_of(marks[0].ts, 60), "2026-10-30");
        assert_eq!((marks[1].key.as_str(), marks[1].tokens), ("msg:m1", 132));
        assert_eq!(marks[1].model.as_deref(), Some("claude-opus-4-8"));
        assert_eq!((marks[1].input, marks[1].output, marks[1].cache_write, marks[1].cache_read), (2, 30, 100, 5000));
        assert_eq!(
            sessions,
            vec![CcSession { id: "s1".into(), cwd: Some("/work/atlas".into()), title: Some("Login fix".into()) }]
        );
    }

    #[test]
    fn files_are_read_in_whole_lines_from_where_they_were_left() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s1.jsonl");
        let first = LINES.lines().next().unwrap();
        std::fs::write(&path, format!("{first}\n{{\"half")).unwrap();
        let (marks, _, offset) = read_from(&path, 0).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(offset as usize, first.len() + 1);
        let (marks, _, again) = read_from(&path, offset).unwrap();
        assert!(marks.is_empty());
        assert_eq!(again, offset);
        // Rewritten shorter: start over.
        std::fs::write(&path, "\n").unwrap();
        assert_eq!(read_from(&path, offset).unwrap().2, 1);
    }

    #[test]
    fn subagent_files_are_found_beside_their_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("-work-atlas");
        let sub = project.join("s1").join("subagents");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(project.join("s1.jsonl"), "").unwrap();
        std::fs::write(sub.join("agent-a1.jsonl"), "").unwrap();
        std::fs::write(sub.join("notes.txt"), "").unwrap();
        let mut found = files(dir.path());
        found.sort();
        assert_eq!(found, vec![project.join("s1").join("subagents").join("agent-a1.jsonl"), project.join("s1.jsonl")]);
    }

    #[test]
    fn times_parse_with_and_without_fractions() {
        assert_eq!(parse_time("1970-01-01T00:00:01Z"), Some(1000));
        assert_eq!(parse_time("1970-01-01T00:00:01.5Z"), Some(1500));
        assert_eq!(parse_time("garbage"), None);
    }
}
