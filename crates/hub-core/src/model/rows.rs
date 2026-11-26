//! Model types: plain rows the store reads and writes for features above it (activity marks, cloud use, MCP calls, plan samples, stats rows). They live here so the store knows no function module.


/// A session's running total as the API gives it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Usage {
    pub input: i64,
    pub output: i64,
    pub cache_write: i64,
    pub cache_read: i64,
    pub cost: f64,
}

/// What a session had when it was last looked at.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Seen {
    pub updated_at: i64,
    pub usage: Usage,
}

/// Use found at a moment.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudMark {
    pub session_id: String,
    pub ts: i64,
    pub usage: Usage,
}

/// One MCP tool call from the log.
pub struct Call {
    pub thread_id: String,
    pub engine: String,
    pub ts: i64,
    pub server: String,
    pub tool: String,
}

/// One reading of the weekly meter.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub ts: i64,
    pub percent: f64,
    /// Which week it belongs to: when it resets, as Claude words or times it.
    pub window: String,
}

/// Work a thread handed to one of your tools that has ended, as the tool reported it.
#[derive(Debug, Clone)]
pub struct Handed {
    pub thread_id: String,
    pub model: Option<String>,
    pub cost: Option<f64>,
    pub proven: bool,
}

/// A turn's end, or Hub's proof verdict on a turn that changed files, from a thread's log.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub thread_id: String,
    pub seq: i64,
    pub ts: i64,
    /// For a proof verdict: the `turn_end` it is about, and "shown", "failed", "asked" or
    /// "missing". None for a turn's end.
    pub reply: Option<i64>,
    pub state: Option<String>,
}

/// A prompt you wrote, or one answer with its tokens.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CcMark {
    /// The line's uuid for a prompt, `msg:<id>` for an answer (one answer spans several lines).
    pub key: String,
    pub session_id: String,
    pub ts: i64,
    /// First line of a prompt; None for an answer.
    pub prompt: Option<String>,
    /// Fresh input plus output; cache reads are left out because they are re-reads.
    pub tokens: i64,
    /// The model that answered, for the leaderboard and the cost; None for a prompt.
    pub model: Option<String>,
    /// The answer's tokens by kind, as they are priced.
    pub input: i64,
    pub output: i64,
    pub cache_write: i64,
    pub cache_read: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CcSession {
    pub id: String,
    pub cwd: Option<String>,
    pub title: Option<String>,
}

/// A user message (with its text) or a finished turn, as the log has it.
#[derive(Debug, Clone)]
pub struct Mark {
    pub thread_id: String,
    pub ts: i64,
    pub message: Option<String>,
}

use serde_json::Value;

/// The server and tool a tool call went to, when it went to an MCP server. Claude Code names
/// them `mcp__<server>__<tool>` (in `_meta.claudeCode.toolName`, and often the title); Codex
/// titles them `Tool: <server>/<tool>` and puts both in its input.
pub fn mcp_tool(names: &[&str], raw_input: &Value) -> Option<(String, String)> {
    for n in names {
        let n = n.trim();
        if let Some(rest) = n.strip_prefix("mcp__")
            && let Some((server, tool)) = rest.split_once("__")
            && !server.is_empty()
            && !tool.is_empty()
        {
            return Some((server.to_owned(), tool.to_owned()));
        }
        if let Some(rest) = n.strip_prefix("Tool: ")
            && let Some((server, tool)) = rest.split_once('/')
            && !server.trim().is_empty()
            && !tool.trim().is_empty()
        {
            return Some((server.trim().to_owned(), tool.trim().to_owned()));
        }
    }
    let server = raw_input.get("server").and_then(Value::as_str)?;
    let tool = raw_input.get("tool").and_then(Value::as_str)?;
    Some((server.to_owned(), tool.to_owned()))
}
