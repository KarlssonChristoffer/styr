//! What a thread's tokens went to: the engine's own session log, read when the user opens the
//! breakdown. ACP's `usage_update` only gives a total, so Hub reads the log the engine writes
//! anyway. Where the log is comes from the engine's traits (`EngineTraits::session_log`), never
//! from its name. The log is read as Messages-style JSONL: answers carry `message.usage`, tools
//! are `tool_use` blocks and their output `tool_result` blocks. Token counts per tool are
//! estimated from text size (about four characters a token); the totals are the engine's own.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::usage::{ContextDetail, ToolBurn};

/// About four characters a token, as tokenizers average on code and English.
fn est(chars: usize) -> u64 {
    chars.div_ceil(4) as u64
}

/// The session log for `session`, from a pattern like `~/.claude/projects/*/{session}.jsonl`:
/// `~` is the home folder and one `*` is any folder.
pub fn find(pattern: &str, session: &str) -> Option<PathBuf> {
    if session.is_empty() || session.contains(['/', '\\', '.']) {
        return None;
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    let pattern = pattern.replace("{session}", session);
    let pattern = match (pattern.strip_prefix("~/").or_else(|| pattern.strip_prefix("~\\")), home) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => pattern,
    };
    let pattern = pattern.replace('\\', "/");
    let Some((head, tail)) = pattern.split_once("/*/") else {
        let p = PathBuf::from(&pattern);
        return p.is_file().then_some(p);
    };
    let mut hits: Vec<PathBuf> = std::fs::read_dir(head)
        .ok()?
        .flatten()
        .map(|e| e.path().join(tail))
        .filter(|p| p.is_file())
        .collect();
    // The newest, if a session was copied between folders.
    hits.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    hits.pop()
}

/// Reads the log at `path`.
pub fn read(path: &Path) -> std::io::Result<ContextDetail> {
    Ok(parse(&std::fs::read_to_string(path)?))
}

fn text_len(content: &Value) -> usize {
    match content {
        Value::String(s) => s.len(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| match p["type"].as_str() {
                Some("text") => p["text"].as_str().map_or(0, str::len),
                // A picture costs roughly the same whatever its bytes; count it as ~1.5k tokens.
                Some("image") => 6_000,
                _ => text_len(&p["content"]),
            })
            .sum(),
        _ => 0,
    }
}

/// `mcp__playwright__browser_click` is server "playwright"; built-in tools have none.
fn server_of(tool: &str) -> Option<String> {
    let rest = tool.strip_prefix("mcp__")?;
    Some(rest.split("__").next().unwrap_or(rest).to_owned())
}

pub fn parse(text: &str) -> ContextDetail {
    let mut tally = Tally::default();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        tally.line(&v);
    }
    tally.finish()
}

/// The tokens of one answer.
#[derive(Default)]
struct Answer {
    input: u64,
    output: u64,
    cache_write: u64,
    cache_read: u64,
}

/// What a session file adds up to, line by line.
#[derive(Default)]
struct Tally {
    answers: Vec<(String, Answer)>,
    tool_of: HashMap<String, String>,
    tools: HashMap<String, ToolBurn>,
    said: usize,
    asked: usize,
    thought: usize,
    compactions: u32,
}

impl Tally {
    fn line(&mut self, v: &Value) {
        if v["type"] == "system" && v["subtype"] == "compact_boundary" {
            self.compactions += 1;
            return;
        }
        let m = &v["message"];
        match v["type"].as_str() {
            Some("assistant") => self.assistant(m),
            Some("user") => match &m["content"] {
                Value::String(s) => self.asked += s.len(),
                Value::Array(parts) => parts.iter().for_each(|part| self.user_part(part)),
                _ => {}
            },
            _ => {}
        }
    }

    fn assistant(&mut self, m: &Value) {
        if let Some(id) = m["id"].as_str() {
            let u = &m["usage"];
            let n = |k: &str| u[k].as_u64().unwrap_or(0);
            let a = Answer {
                input: n("input_tokens"),
                output: n("output_tokens"),
                cache_write: n("cache_creation_input_tokens"),
                cache_read: n("cache_read_input_tokens"),
            };
            // One answer spans several lines with the same usage; keep the last.
            match self.answers.iter_mut().find(|(k, _)| k == id) {
                Some((_, old)) => *old = a,
                None => self.answers.push((id.to_owned(), a)),
            }
        }
        for part in m["content"].as_array().into_iter().flatten() {
            match part["type"].as_str() {
                Some("text") => self.said += part["text"].as_str().map_or(0, str::len),
                Some("thinking") => self.thought += part["thinking"].as_str().map_or(0, str::len),
                Some("tool_use") => {
                    let name = part["name"].as_str().unwrap_or("tool").to_owned();
                    if let Some(id) = part["id"].as_str() {
                        self.tool_of.insert(id.to_owned(), name.clone());
                    }
                    let t = self.tool(name);
                    t.calls += 1;
                    t.tokens += est(part["input"].to_string().len());
                }
                _ => {}
            }
        }
    }

    fn user_part(&mut self, part: &Value) {
        match part["type"].as_str() {
            Some("tool_result") => {
                let id = part["tool_use_id"].as_str().unwrap_or_default();
                let name = self.tool_of.get(id).cloned().unwrap_or_else(|| "tool".into());
                self.tool(name).tokens += est(text_len(&part["content"]));
            }
            Some("text") => self.asked += part["text"].as_str().map_or(0, str::len),
            Some("image") => self.asked += 6_000,
            _ => {}
        }
    }

    /// The tool's row, made when it is first seen.
    fn tool(&mut self, name: String) -> &mut ToolBurn {
        self.tools.entry(name.clone()).or_insert_with(|| ToolBurn { server: server_of(&name), name, calls: 0, tokens: 0 })
    }

    fn finish(self) -> ContextDetail {
        let Tally { answers, tools, said, asked, thought, compactions, .. } = self;
        let sum = |f: fn(&Answer) -> u64| answers.iter().map(|(_, a)| f(a)).sum::<u64>();
        let context = |a: &Answer| a.input + a.cache_write + a.cache_read;
        let mut tools: Vec<ToolBurn> = tools.into_values().collect();
        tools.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(a.name.cmp(&b.name)));
        let mut servers: Vec<ToolBurn> = Vec::new();
        for t in &tools {
            let name = t.server.clone().unwrap_or_else(|| "Built-in tools".into());
            match servers.iter_mut().find(|s| s.name == name) {
                Some(s) => {
                    s.calls += t.calls;
                    s.tokens += t.tokens;
                }
                None => servers.push(ToolBurn { name, server: t.server.clone(), calls: t.calls, tokens: t.tokens }),
            }
        }
        servers.sort_by_key(|x| std::cmp::Reverse(x.tokens));
        ContextDetail {
            found: true,
            note: None,
            answers: answers.len() as u32,
            compactions,
            input: sum(|a| a.input),
            output: sum(|a| a.output),
            cache_write: sum(|a| a.cache_write),
            cache_read: sum(|a| a.cache_read),
            start: answers.first().map(|(_, a)| context(a)).unwrap_or(0),
            now: answers.last().map(|(_, a)| context(a)).unwrap_or(0),
            conversation: est(said + asked),
            thinking: est(thought),
            tools,
            servers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_tokens_by_tool_and_server() {
        let log = [
            r#"{"type":"user","message":{"role":"user","content":"fix the bug please"}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"Looking."}],"usage":{"input_tokens":10,"cache_creation_input_tokens":20000,"cache_read_input_tokens":0,"output_tokens":5}}}"#,
            r#"{"type":"assistant","message":{"id":"m1","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"a.rs"}}],"usage":{"input_tokens":10,"cache_creation_input_tokens":20000,"cache_read_input_tokens":0,"output_tokens":40}}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"0123456789012345678901234567890123456789"}]}}"#,
            r#"{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t2","name":"mcp__playwright__browser_snapshot","input":{}}],"usage":{"input_tokens":3,"cache_creation_input_tokens":100,"cache_read_input_tokens":20000,"output_tokens":7}}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t2","content":[{"type":"text","text":"PAGE PAGE PAGE PAGE PAGE PAGE PAGE PAGE PAGE PAGE PAGE PAGE PAGE PAGE PAGE PAGE"}]}]}}"#,
            r#"{"type":"system","subtype":"compact_boundary"}"#,
            "not json",
        ]
        .join("\n");
        let d = parse(&log);
        assert_eq!(d.answers, 2);
        assert_eq!(d.compactions, 1);
        assert_eq!(d.output, 47);
        assert_eq!(d.cache_read, 20_000);
        assert_eq!(d.start, 20_010);
        assert_eq!(d.now, 20_103);
        assert_eq!(d.tools[0].name, "mcp__playwright__browser_snapshot");
        assert_eq!(d.tools[0].server.as_deref(), Some("playwright"));
        assert_eq!(d.tools[0].calls, 1);
        assert_eq!(d.servers.iter().find(|s| s.name == "Built-in tools").unwrap().calls, 1);
        assert!(d.conversation > 0);
    }

    #[test]
    fn finds_a_log_in_any_folder() {
        let dir = std::env::temp_dir().join(format!("hub-usage-detail-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("proj")).unwrap();
        std::fs::write(dir.join("proj/abc.jsonl"), "").unwrap();
        let pattern = format!("{}/*/{{session}}.jsonl", dir.display());
        assert!(find(&pattern, "abc").is_some());
        assert!(find(&pattern, "zzz").is_none());
        assert!(find(&pattern, "../x").is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
