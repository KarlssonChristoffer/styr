//! Hub's own tools for agents, as a tiny MCP server over HTTP (one JSON-RPC message per POST,
//! answered with plain JSON). Each thread's engine gets `/mcp/<thread id>`, so a tool call always
//! acts on the thread that made it. The engine is a program like any other, so it shows the
//! connection token (`auth.rs`) to get in. Agents outside Hub use `/mcp` with an agent key and get
//! Hub's tools only (`hub_tools.rs`).
//!
//! This file is the protocol: the JSON-RPC methods, which tools a caller gets and in what order
//! (`thread_tools`), and the call that runs one. The tools only a thread gets live by group in
//! `mcp/`, each file with its declarations and bodies: `talk.rs`, `pages.rs`, `side.rs`,
//! `docs.rs`, `guard.rs`.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::hub::Hub;
use crate::hub_tools::{self, Caller};
use crate::model::AgentAccess;
use crate::hub_tools::tool::{self, Call, Out, Tool};

/// Runs `$body` on the thread that called; a caller that is no thread has no such tool.
macro_rules! thread {
    ($c:ident, |$t:ident| $body:expr) => {
        match on_thread($c) {
            Ok($t) => $body,
            Err(e) => Err(e),
        }
    };
}

/// Answers one JSON-RPC message from the engine of thread `thread_id`, or nothing for a
/// notification.
pub async fn handle(hub: &Arc<Hub>, thread_id: &str, msg: Value) -> Option<Value> {
    // An engine started ahead of its thread (`warm.rs`) connects to its tools as it starts, before
    // the thread exists. Turned away, Claude Code reports the server as failed for the whole
    // session, so it is answered as the thread it will be.
    let found = match hub.store.thread(thread_id).await {
        Ok(Some(thread)) => Some(thread),
        _ => crate::warm::pending(thread_id).map(|warm| warm.stand_in()),
    };
    let Some(caller) = found else {
        let id = msg.get("id")?.clone();
        return Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32602, "message": format!("no thread {thread_id}") } }));
    };
    handle_as(hub, &Caller::Thread(caller), msg).await
}

/// Answers one JSON-RPC message from `caller`, or nothing for a notification.
pub async fn handle_as(hub: &Arc<Hub>, caller: &Caller, msg: Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg["method"].as_str().unwrap_or_default();
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "hub", "version": hub.update_status().version },
            "instructions": crate::tool_instructions::instructions(&hub.ctx(), caller.thread()).await,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut tools = hub_tools::tools(caller.access());
            tools.extend(thread_tools(hub, caller).await.into_iter().map(|t| t.def));
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => Ok(call(hub, caller, &msg["params"]).await),
        _ => Err(json!({ "code": -32601, "message": format!("no method {method}") })),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

/// Every tool this caller may call: Hub's tools for every agent, and the ones only a thread's own
/// engine gets.
async fn registry(hub: &Arc<Hub>, caller: &Caller) -> Vec<Tool> {
    let mut tools = hub_tools::registry(AgentAccess::Full);
    tools.extend(thread_tools(hub, caller).await);
    tools
}

/// The tools only a thread's own engine gets, as the thread is what they act on.
async fn thread_tools(hub: &Arc<Hub>, caller: &Caller) -> Vec<Tool> {
    let mut tools = Vec::new();
    if thread_of(caller).is_none() {
        return tools;
    }
    talk_tools(&mut tools, hub);
    page_tools(&mut tools);
    side_tools(&mut tools, caller);
    tools.extend(docs_tools());
    tools.extend(library_tools());
    tools.extend(guard_tools(hub, caller).await);
    tools
}

async fn call(hub: &Arc<Hub>, caller: &Caller, params: &Value) -> Value {
    let name = params["name"].as_str().unwrap_or_default();
    let reply = match tool::call(hub, caller, &registry(hub, caller).await, name, &params["arguments"]).await {
        Some(reply) => reply,
        None => Err(format!("no tool {name}")),
    };
    match reply {
        Ok(Out::Text(text)) => json!({ "content": [{ "type": "text", "text": text }], "isError": false }),
        Ok(Out::Content(content)) => json!({ "content": content, "isError": false }),
        Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
    }
}

/// Runs a thread tool on the thread that called, which only its own engine does.
fn on_thread<'a>(c: &'a Call<'a>) -> Result<&'a crate::model::Thread, String> {
    thread_of(c.caller).ok_or_else(|| format!("no tool {}", c.name))
}

fn thread_of(caller: &Caller) -> Option<&crate::model::Thread> {
    match caller {
        Caller::Thread(thread) => Some(thread),
        _ => None,
    }
}

/// MCP's hint that a tool changes nothing, which some engines use to skip asking.
fn read_only() -> Value {
    json!({ "readOnlyHint": true, "destructiveHint": false, "openWorldHint": false })
}

mod docs;
use docs::{docs_tools, library_tools};

mod guard;
use guard::guard_tools;

mod pages;
use pages::page_tools;

mod side;
use side::side_tools;

mod talk;
use talk::talk_tools;

#[cfg(test)]
mod tests;
