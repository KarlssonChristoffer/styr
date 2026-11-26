//! The one list of Hub's tools for agents. A tool is one entry: its name, the schema an agent
//! reads, whether it changes anything (and so needs supervised access) and the function that runs
//! it. `hub_tools.rs` registers the tools every caller may get, `mcp.rs` the ones only a thread's
//! own engine gets. Listing (`tools/list`) and running (`tools/call`) both read this list, so a new
//! tool is one entry in one place.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::{Value, json};

use crate::hub::Hub;
use super::Caller;
use crate::model::AgentAccess;

/// What a tool answered: plain text, or MCP `content` blocks when it returns more than text (a
/// picture).
pub enum Out {
    Text(String),
    Content(Vec<Value>),
}

pub type Reply = Result<Out, String>;

/// One call of a tool.
pub struct Call<'a> {
    pub hub: &'a Arc<Hub>,
    pub caller: &'a Caller,
    pub name: &'a str,
    pub args: &'a Value,
}

pub type Run = for<'a> fn(&'a Call<'a>) -> Pin<Box<dyn Future<Output = Reply> + Send + 'a>>;

pub struct Tool {
    /// The tool as an MCP `tools/list` entry.
    pub def: Value,
    /// Changes something, so a read-only agent key may not call it.
    pub writes: bool,
    run: Run,
}

impl Tool {
    /// A tool from its MCP definition, which changes nothing outside the thread that called it.
    pub fn of(def: Value, run: Run) -> Self {
        Self { def, writes: false, run }
    }

    /// A tool with the usual schema; one that is not `read_only` needs supervised access.
    pub fn new(name: &str, description: &str, properties: Value, required: &[&str], read_only: bool, run: Run) -> Self {
        let def = json!({
            "name": name,
            "description": description,
            "inputSchema": { "type": "object", "properties": properties, "required": required },
            "annotations": { "readOnlyHint": read_only, "destructiveHint": false, "openWorldHint": false },
        });
        Self { def, writes: !read_only, run }
    }

    /// Changes something yet stays open to a read-only key (it only writes what the agent itself
    /// made, such as a declaration of its own tool).
    pub fn ungated(mut self) -> Self {
        self.writes = false;
        self
    }

    pub fn name(&self) -> &str {
        self.def["name"].as_str().unwrap_or_default()
    }
}

/// Runs the tool called `name` from `tools`, or None when the list has no such tool.
pub async fn call(hub: &Arc<Hub>, caller: &Caller, tools: &[Tool], name: &str, args: &Value) -> Option<Reply> {
    let tool = tools.iter().find(|t| t.name() == name)?;
    if tool.writes && caller.access() < AgentAccess::Supervised {
        return Some(Err(format!("{name} needs supervised or full access; this agent key is read only")));
    }
    Some((tool.run)(&Call { hub, caller, name, args }).await)
}

/// A handler written as a closure over the call: `run!(|c| text(c.hub.thing(c.args).await))`.
#[macro_export]
macro_rules! run {
    (|$c:ident| $body:expr) => {
        |$c| Box::pin(async move { $body })
    };
}

/// A tool's JSON answer as the pretty-printed text agents read.
pub fn json_out(answer: anyhow::Result<Value>) -> Reply {
    answer.map(|v| Out::Text(serde_json::to_string_pretty(&v).unwrap_or_default())).map_err(|e| format!("{e:#}"))
}

/// A tool's text answer.
pub fn text(answer: Result<String, String>) -> Reply {
    answer.map(Out::Text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_is_listed_once_and_gated_like_its_hint() {
        let tools = crate::hub_tools::registry(AgentAccess::Full);
        let mut names: Vec<&str> = tools.iter().map(Tool::name).collect();
        let all = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all, "two tools share a name");
        for tool in &tools {
            // A tool that is not read-only needs supervised access, but for the two that only
            // write what the agent itself made.
            let open = ["customs_declare", "suggest_recipe"].contains(&tool.name());
            assert_eq!(tool.writes, tool.def["annotations"]["readOnlyHint"] == false && !open, "{}", tool.name());
        }
    }
}
