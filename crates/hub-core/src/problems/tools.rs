//! Agents' tool calls, from every engine: each call is counted per day, engine and tool
//! (`tool_calls`), and one that fails becomes a problem (source `tool`), grouped by engine, tool,
//! the program a command ran and the error's first telling line. A failed call that said nothing
//! at all names the command it ran instead, so the page says what was tried. Calls the user or
//! Hub's permission settings refused, and calls cut off by a stop, are not failures.
//!
//! Hub reads them from the event log as they are logged ([`saw`], from `Hub::look_at`), so the
//! engines need nothing of their own: Claude Code names its tools in `_meta.claudeCode.toolName`,
//! MCP tools are `server · tool` whichever engine called them, and the rest go by the ACP `kind`
//! ("execute" is "Command"). What a failed call said is counted too (`tool_errors`), so
//! [`stats`], which answers the Problems page's Tools tab, can list the most common errors.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::{Crumb, DAY_MS, Frame, Hit, Incoming, Level, normalize};
use crate::hub::Hub;
use crate::model::{Event, EventKind, now_ms};

/// Set once the calls already in the event log were counted.
const COUNTED: &str = "problems.tools.counted";
/// Set once the errors of failed calls already in the event log were counted.
const ERRORS_COUNTED: &str = "problems.tools.errors_counted";
/// How far back the first count reads.
const BACKFILL_DAYS: i64 = 30;
/// What a failed call says when it says nothing at all.
const SILENT: &str = "It failed without saying why";

/// Counts a tool call that started, and keeps one that failed as a problem.
pub(crate) async fn saw(hub: &Arc<Hub>, event: &Event) -> Result<()> {
    let d = &event.data;
    if !crate::model::worth_a_look(d) {
        return Ok(());
    }
    let Some(id) = d["toolCallId"].as_str() else {
        return Ok(());
    };
    let Some(thread) = hub.store.thread(&event.thread_id).await? else {
        return Ok(());
    };
    let updates = call_updates(hub, &thread.id, id).await?;
    let call = merged(updates.iter().filter(|e| e.seq <= event.seq));
    let tool = tool_name(&call);
    let day = event.ts.div_euclid(DAY_MS);
    if crate::model::update_kind(d) == Some("tool_call") {
        count(hub, day, &thread.engine, &tool, 1, 0).await?;
    }
    let failed_before = updates
        .iter()
        .any(|e| e.seq < event.seq && e.data["status"] == "failed");
    if d["status"] != "failed" || failed_before {
        return Ok(());
    }
    let mut text = said(&call);
    if text.is_empty() {
        text = terminal_said(updates.iter().filter(|e| e.seq <= event.seq));
    }
    if stopped(&text) || refused(hub, &thread.id, id).await? {
        return Ok(());
    }
    count(hub, day, &thread.engine, &tool, 0, 1).await?;

    let gist = gist(&text);
    count_error(hub, day, &thread.engine, &tool, &gist).await?;
    let program = program(&call["rawInput"]);
    let mut fingerprint = vec![thread.engine.clone(), tool.clone()];
    if let Some(p) = &program {
        fingerprint.push(p.clone());
    }
    fingerprint.push(normalize(&gist));
    let kind = match &program {
        Some(p) => format!("{tool} · {p} failed"),
        None => format!("{tool} failed"),
    };
    let message = failed_message(&text, &gist, &call["rawInput"]);
    let input = clip(&call["rawInput"].to_string(), 2000);
    let incoming = Incoming {
        project: project_of(hub, &thread.cwd).await,
        source: "tool".into(),
        level: Level::Warning,
        folder: Some(thread.cwd.clone()),
        fingerprint: Some(fingerprint),
        hit: Hit {
            ts: event.ts,
            message,
            kind: Some(kind),
            frames: vec![Frame {
                file: Some(culprit(&thread.engine, &tool)),
                own: true,
                ..Default::default()
            }],
            trace: None,
            crumbs: crumbs(hub, &thread.id, event.seq).await.unwrap_or_default(),
            release: None,
            environment: None,
            extra: json!({
                "thread": thread.id, "title": thread.title, "engine": thread.engine, "seq": event.seq,
                "tool": tool, "tool_call": call["title"], "input": input,
            }),
        },
    };
    super::record(hub, incoming).await.map(|_| ())
}

/// Where a tool's failures point, as the problem's culprit: "claude · Bash".
fn culprit(engine: &str, tool: &str) -> String {
    format!("{engine} · {tool}")
}

/// Every update about tool call `id` in a thread, oldest first.
async fn call_updates(hub: &Hub, thread: &str, id: &str) -> Result<Vec<Event>> {
    let (thread, id) = (thread.to_owned(), id.to_owned());
    let blobs = hub.store.blob_dir();
    hub.store
        .with(move |c| {
            let mut stmt = c.prepare(
                "SELECT seq, ts, data FROM events WHERE thread_id = ?1 AND kind = 'update'
                   AND json_extract(data, '$.toolCallId') = ?2 ORDER BY seq",
            )?;
            let rows = stmt.query_map(params![thread, id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (seq, ts, data) = row?;
                // A picture is a file reference in the row (`blobs.rs`); put it back.
                let mut data: Value = serde_json::from_str(&data).unwrap_or_default();
                if let Some(dir) = &blobs {
                    crate::blobs::inline(dir, &mut data);
                }
                out.push(Event { thread_id: thread.clone(), seq, ts, kind: EventKind::Update, data });
            }
            Ok(out)
        })
        .await
}

/// Everything known about a call, since each update only carries what changed.
fn merged<'a>(updates: impl Iterator<Item = &'a Event>) -> Value {
    let mut call = json!({});
    for e in updates {
        merge_into(&mut call, &e.data);
    }
    call
}

fn merge_into(call: &mut Value, update: &Value) {
    if let (Some(into), Some(from)) = (call.as_object_mut(), update.as_object()) {
        for (k, v) in from {
            if v.is_null() || (k == "content" && v.as_array().is_some_and(|a| a.is_empty())) {
                continue;
            }
            into.insert(k.clone(), v.clone());
        }
    }
}

/// The tool's name, the same for every call to it: Claude Code's own name, an MCP server's tool
/// as `server · tool`, else what kind of tool the engine said it was.
pub fn tool_name(call: &Value) -> String {
    let own = crate::connections::call_tool_name(call);
    let title = call["title"].as_str();
    let names: Vec<&str> = [own, title].into_iter().flatten().collect();
    if let Some((server, tool)) = crate::connections::mcp_tool(&names, &call["rawInput"]) {
        return format!("{server} · {tool}");
    }
    if let Some(own) = own.filter(|o| !o.trim().is_empty()) {
        return own.trim().to_owned();
    }
    let by_kind = match call["kind"].as_str() {
        Some("execute") => "Command",
        Some("read") => "Read",
        Some("edit") => "Edit",
        Some("delete") => "Delete",
        Some("move") => "Move",
        Some("search") => "Search",
        Some("fetch") => "Fetch",
        Some("think") => "Think",
        Some("switch_mode") => "Switch mode",
        _ => "",
    };
    if !by_kind.is_empty() {
        return by_kind.into();
    }
    title
        .and_then(|t| t.split_whitespace().next())
        .map(|w| w.trim_end_matches(':').to_owned())
        .filter(|w| !w.is_empty())
        .unwrap_or_else(|| "Other".into())
}

/// The command a call ran, whole: a string, or Codex's `["bash", "-lc", "cargo test"]`.
fn command_of(input: &Value) -> Option<String> {
    let line = match &input["command"] {
        Value::String(s) => s.clone(),
        Value::Array(a) => {
            let parts: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
            match parts.as_slice() {
                [shell, flag, script, ..]
                    if flag.starts_with('-') && flag.contains('c') && is_shell(shell) =>
                {
                    (*script).to_owned()
                }
                _ => parts.join(" "),
            }
        }
        _ => return None,
    };
    (!line.trim().is_empty()).then_some(line)
}

/// The program a command ran, without its path: `cargo` for `cd app && cargo test`.
pub fn program(input: &Value) -> Option<String> {
    let line = command_of(input)?;
    let segment = doing(&line)?;
    let word = segment
        .split_whitespace()
        .find(|w| !w.contains('=') || w.starts_with('-'))?;
    let word = word.trim_matches(|c| c == '"' || c == '\'');
    let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
    let name = name.strip_suffix(".exe").unwrap_or(name);
    (!name.is_empty()).then(|| name.to_owned())
}

/// The part of a command that did the work: the first line or segment that is not a bare `cd`,
/// so `cd app && npm run check` gives `npm run check`.
fn doing(line: &str) -> Option<&str> {
    line.split(['&', ';', '|', '\n'])
        .map(str::trim)
        .find(|s| !s.is_empty() && !s.starts_with("cd ") && *s != "cd")
}

/// The line of a command that ran something: a line that only changes folder is skipped, so a
/// script's shown line is the one its program comes from.
fn line_that_ran(line: &str) -> Option<&str> {
    let lines: Vec<&str> = line.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines
        .iter()
        .copied()
        .find(|l| doing(l).is_some())
        .or_else(|| lines.first().copied())
}

fn is_shell(s: &str) -> bool {
    let name = s.rsplit(['/', '\\']).next().unwrap_or(s);
    matches!(
        name,
        "bash"
            | "sh"
            | "zsh"
            | "pwsh"
            | "powershell"
            | "cmd"
            | "bash.exe"
            | "pwsh.exe"
            | "powershell.exe"
            | "cmd.exe"
    )
}

/// What the call said back: its text content, else its raw output.
pub fn said(call: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in call["content"].as_array().into_iter().flatten() {
        if let Some(t) = c["content"]["text"].as_str() {
            parts.push(t.to_owned());
        }
    }
    if parts.is_empty() {
        let raw = &call["rawOutput"];
        match raw {
            Value::String(s) => parts.push(s.clone()),
            Value::Object(o) => {
                for key in [
                    "error",
                    "stderr",
                    "message",
                    "formatted_output",
                    "output",
                    "stdout",
                ] {
                    if let Some(s) = o
                        .get(key)
                        .and_then(Value::as_str)
                        .filter(|s| !s.trim().is_empty())
                    {
                        parts.push(s.to_owned());
                        break;
                    }
                }
                if parts.is_empty() {
                    parts.push(raw.to_string());
                }
            }
            Value::Null => {}
            other => parts.push(other.to_string()),
        }
    }
    let text = parts.join("\n");
    text.replace("<tool_use_error>", "")
        .replace("</tool_use_error>", "")
        .trim()
        .to_owned()
}

/// What a command printed and how it ended, for a call whose engine streams the terminal and
/// puts nothing in the call itself: the end of the output, else the exit code. Without it every
/// failed command read "It failed without saying why".
fn terminal_said<'a>(updates: impl IntoIterator<Item = &'a Event>) -> String {
    let (mut out, mut code) = (String::new(), None);
    for e in updates {
        if let Some(printed) = crate::model::terminal_output(&e.data, &out) {
            out = printed;
        }
        if let Some(n) = e.data["_meta"]["terminal_exit"]["exit_code"].as_i64() {
            code = Some(n);
        }
    }
    // Under the 3000 the message is clipped at, so the end (where the error is) is what stays.
    let out = crate::text::tail(out.trim(), 2900);
    match code {
        Some(n) if out.is_empty() => format!("Exit code {n}"),
        _ => out,
    }
}

/// The line that says what went wrong: one that names an error, else the first that says
/// anything beyond the exit code.
pub fn gist(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(60)
        .collect();
    let bare = |l: &str| {
        let lower = l.to_ascii_lowercase();
        // A line of only marks (a ``` fence, `---`, `}`) says nothing.
        let says_nothing = !l.chars().any(char::is_alphanumeric);
        !(says_nothing
            || lower.starts_with("exit code")
            || lower.starts_with("exited with")
            || lower == "error"
            || lower == "error:")
    };
    let telling = lines.iter().copied().filter(|l| bare(l)).find(|l| {
        let lower = l.to_ascii_lowercase();
        lower.contains("error")
            || lower.contains("not found")
            || lower.contains("denied")
            || lower.contains("failed")
            || lower.contains("no such")
    });
    let line = telling
        .or_else(|| lines.iter().copied().find(|l| bare(l)))
        .or_else(|| lines.iter().copied().find(|l| l.chars().any(char::is_alphanumeric)))
        .unwrap_or(SILENT);
    line.chars().take(300).collect()
}

/// Whether what a call said is nothing a reader can use: the fallback, a bare exit code, or the
/// raw output's own JSON (`{"exit_code":1}`).
fn says_nothing(gist: &str) -> bool {
    let lower = gist.trim().to_ascii_lowercase();
    gist == SILENT
        || lower.starts_with("exit code")
        || lower.starts_with("exited with")
        || (lower.starts_with('{') && (lower.contains("\"exit_code\"") || lower.contains("\"exitcode\"")))
}

/// A failed call's message: what it said, or, when it said nothing at all, the command it ran
/// (the problem's kind already names the program), so a wrong flag reads differently from a
/// broken machine. The fallback stays for a call with no command.
fn failed_message(text: &str, gist: &str, input: &Value) -> String {
    if says_nothing(gist) {
        let command = command_of(input);
        let ran = command.as_deref().and_then(line_that_ran);
        if let Some(ran) = ran {
            let tail = if text.trim().is_empty() { "no output" } else { "said nothing" };
            return format!("`{}` - {tail}", clip(ran, 160));
        }
    }
    if text.trim().is_empty() || text.trim() == gist {
        gist.to_owned()
    } else {
        format!("{gist}\n\n{}", clip(text, 3000))
    }
}

/// A call cut off because the user stopped the turn, not one that failed.
fn stopped(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "interrupted by user",
        "doesn't want to proceed",
        "does not want to proceed",
        "the user rejected",
        "denied by the user",
        "request was aborted",
        "operation was cancelled",
        "operation was canceled",
    ]
    .iter()
    .any(|p| lower.contains(p))
}

/// Whether the call was refused when it asked for permission, by the user or by Hub's settings.
async fn refused(hub: &Hub, thread: &str, id: &str) -> Result<bool> {
    let (thread, id) = (thread.to_owned(), id.to_owned());
    let asks: Vec<(String, Value)> = hub
        .store
        .with(move |c| {
            let mut stmt = c.prepare(
                "SELECT kind, data FROM events WHERE thread_id = ?1
                   AND kind IN ('permission_request', 'permission_resolved', 'permission_auto') ORDER BY seq",
            )?;
            let rows = stmt.query_map(params![thread], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut out = Vec::new();
            for row in rows {
                let (kind, data) = row?;
                out.push((kind, serde_json::from_str(&data).unwrap_or_default()));
            }
            Ok(out)
        })
        .await?;
    Ok(refused_in(&asks, &id))
}

fn refused_in(asks: &[(String, Value)], id: &str) -> bool {
    let mut options: HashMap<String, Vec<Value>> = HashMap::new();
    for (kind, d) in asks {
        match kind.as_str() {
            "permission_auto" if d["tool_call"]["toolCallId"] == id && d["allowed"] == false => {
                return true;
            }
            "permission_request" if d["tool_call"]["toolCallId"] == id => {
                let key = d["id"].to_string();
                options.insert(key, d["options"].as_array().cloned().unwrap_or_default());
            }
            "permission_resolved" => {
                let Some(offered) = options.get(&d["id"].to_string()) else {
                    continue;
                };
                let Some(picked) = d["option_id"].as_str() else {
                    return true;
                };
                let kind = offered
                    .iter()
                    .find(|o| o["optionId"] == picked || o["option_id"] == picked)
                    .and_then(|o| o["kind"].as_str());
                if kind.is_some_and(|k| k.starts_with("reject")) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// What happened in the thread before `seq`: the user's message and the tool calls, newest
/// last, as a problem's breadcrumbs.
pub(crate) async fn crumbs(hub: &Hub, thread: &str, seq: i64) -> Result<Vec<Crumb>> {
    let events = hub.store.events(thread, (seq - 400).max(0), 400).await?;
    let mut out: Vec<Crumb> = Vec::new();
    let mut titles: HashMap<String, String> = HashMap::new();
    for e in events.iter().filter(|e| e.seq < seq) {
        let d = &e.data;
        let crumb = match e.kind {
            EventKind::UserMessage => Some((
                "info",
                "message",
                format!("You: {}", clip(e.said().unwrap_or_default(), 200)),
            )),
            EventKind::Error => Some((
                "error",
                "engine",
                format!(
                    "Engine failed: {}",
                    clip(e.failure().unwrap_or_default(), 200)
                ),
            )),
            EventKind::TurnEnd => Some((
                "info",
                "turn",
                format!(
                    "Turn ended ({})",
                    e.stop_reason().unwrap_or("end")
                ),
            )),
            EventKind::Update => {
                let id = d["toolCallId"].as_str().unwrap_or_default().to_owned();
                if let Some(t) = d["title"].as_str() {
                    titles.insert(id.clone(), t.to_owned());
                }
                match (crate::model::update_kind(d), d["status"].as_str()) {
                    (
                        Some("tool_call" | "tool_call_update"),
                        Some(status @ ("completed" | "failed")),
                    ) => {
                        let title = titles
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(|| "Tool call".into());
                        let level = if status == "failed" {
                            "warning"
                        } else {
                            "info"
                        };
                        Some((level, "tool", format!("{} · {status}", clip(&title, 200))))
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some((level, category, message)) = crumb {
            out.push(Crumb {
                ts: Some(e.ts),
                level: Some(level.into()),
                category: Some(category.into()),
                message,
            });
        }
    }
    let skip = out.len().saturating_sub(super::CRUMBS);
    Ok(out.split_off(skip))
}

/// The project a folder belongs to, by name: the registered project holding it, else its folder's name.
pub(crate) async fn project_of(hub: &Hub, cwd: &str) -> String {
    let projects = hub.store.projects().await.unwrap_or_default();
    let cwd = &crate::thread_display::project_root(cwd);
    projects
        .iter()
        .find(|p| crate::folders::same_folder(&p.path, cwd))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| {
            std::path::Path::new(cwd)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Hub".into())
        })
}

use crate::text::clip;

async fn count(
    hub: &Hub,
    day: i64,
    engine: &str,
    tool: &str,
    calls: i64,
    failed: i64,
) -> Result<()> {
    let (engine, tool) = (engine.to_owned(), tool.to_owned());
    hub.store
        .with(move |c| {
            c.execute(
                "INSERT INTO tool_calls (day, engine, tool, calls, failed) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(day, engine, tool) DO UPDATE SET calls = calls + excluded.calls, failed = failed + excluded.failed",
                params![day, engine, tool, calls, failed],
            )?;
            Ok(())
        })
        .await
}

async fn count_error(hub: &Hub, day: i64, engine: &str, tool: &str, gist: &str) -> Result<()> {
    let (engine, tool, gist) = (engine.to_owned(), tool.to_owned(), gist.to_owned());
    hub.store
        .with(move |c| {
            add_error(c, day, &engine, &tool, &gist, 1)?;
            Ok(())
        })
        .await
}

fn add_error(c: &rusqlite::Connection, day: i64, engine: &str, tool: &str, gist: &str, n: i64) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO tool_errors (day, engine, tool, gist, count) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(day, engine, tool, gist) DO UPDATE SET count = count + excluded.count",
        params![day, engine, tool, gist, n],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The Tools tab

/// One tool on one engine over the days asked for.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolRate {
    pub engine: String,
    pub tool: String,
    #[ts(type = "number")]
    pub calls: i64,
    #[ts(type = "number")]
    pub failed: i64,
    /// Failed calls on each day, oldest first.
    #[ts(type = "number[]")]
    pub days: Vec<i64>,
    /// Its most frequent failures, as problems.
    pub problems: Vec<ToolProblem>,
    /// What its failed calls said most often, most first.
    pub errors: Vec<ToolError>,
}

/// One thing a tool's failed calls said, and how often, over the days asked for.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolError {
    pub gist: String,
    #[ts(type = "number")]
    pub count: i64,
    /// The problem these failures are kept as, when there is one (failures from before Hub
    /// counted tool calls have none).
    #[ts(type = "number | null")]
    pub problem: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolProblem {
    #[ts(type = "number")]
    pub id: i64,
    pub title: String,
    #[ts(type = "number")]
    pub count: i64,
    pub status: super::ProblemStatus,
}

/// Tool calls and how many failed, on this machine.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolStats {
    pub days: u32,
    #[ts(type = "number")]
    pub calls: i64,
    #[ts(type = "number")]
    pub failed: i64,
    /// Most failures first.
    pub tools: Vec<ToolRate>,
}

/// The problem a failed call's gist is kept as: the title that holds the same first line, or, for
/// a call that said nothing, the one kept with the command it ran instead.
fn linked(gist: &str, problems: &[ToolProblem]) -> Option<i64> {
    let key: String = gist.chars().take(60).collect();
    if let Some(p) = problems.iter().find(|p| p.title.contains(&key)) {
        return Some(p.id);
    }
    if !says_nothing(gist) {
        return None;
    }
    problems
        .iter()
        .find(|p| p.title.ends_with(" - no output") || p.title.ends_with(" - said nothing"))
        .map(|p| p.id)
}

pub async fn stats(hub: &Hub, days: u32) -> Result<ToolStats> {
    let days = days.clamp(1, 90);
    let today = now_ms().div_euclid(DAY_MS);
    let from = today - i64::from(days) + 1;
    hub.store
        .with(move |c| {
            let mut stmt = c.prepare("SELECT day, engine, tool, calls, failed FROM tool_calls WHERE day >= ?1")?;
            let rows = stmt.query_map([from], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
            })?;
            let mut by: HashMap<(String, String), ToolRate> = HashMap::new();
            let mut out = ToolStats { days, calls: 0, failed: 0, tools: Vec::new() };
            for row in rows {
                let (day, engine, tool, calls, failed) = row?;
                out.calls += calls;
                out.failed += failed;
                let rate = by.entry((engine.clone(), tool.clone())).or_insert_with(|| ToolRate {
                    engine,
                    tool,
                    calls: 0,
                    failed: 0,
                    days: vec![0; days as usize],
                    problems: Vec::new(),
                    errors: Vec::new(),
                });
                rate.calls += calls;
                rate.failed += failed;
                if let Some(slot) = rate.days.get_mut((day - from) as usize) {
                    *slot += failed;
                }
            }
            let mut problems = c.prepare(
                "SELECT id, title, count, status FROM problems
                 WHERE source = 'tool' AND culprit = ?1 AND merged_into IS NULL ORDER BY count DESC LIMIT 10",
            )?;
            for rate in by.values_mut().filter(|r| r.failed > 0) {
                let found = problems.query_map([culprit(&rate.engine, &rate.tool)], |r| {
                    let status: String = r.get(3)?;
                    Ok(ToolProblem {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        count: r.get(2)?,
                        status: serde_json::from_value(Value::String(status)).unwrap_or(super::ProblemStatus::Open),
                    })
                })?;
                rate.problems = found.collect::<rusqlite::Result<_>>()?;
            }
            let mut said = c.prepare(
                "SELECT gist, SUM(count) AS n FROM tool_errors WHERE day >= ?1 AND engine = ?2 AND tool = ?3
                 GROUP BY gist ORDER BY n DESC, gist LIMIT 5",
            )?;
            for rate in by.values_mut().filter(|r| r.failed > 0) {
                let found = said.query_map(params![from, rate.engine, rate.tool], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
                for row in found {
                    let (gist, count) = row?;
                    let problem = linked(&gist, &rate.problems);
                    rate.errors.push(ToolError { gist, count, problem });
                }
            }
            out.tools = by.into_values().collect();
            out.tools.sort_by(|a, b| b.failed.cmp(&a.failed).then(b.calls.cmp(&a.calls)));
            Ok(out)
        })
        .await
}

mod review;
pub use review::*;

mod back;
pub(crate) use back::*;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------------------------
// Vakten's review
