//! The notes a thread's agent reads and writes: the project's markdown docs (`docs_*`, so it finds
//! what is written before it writes more) and the user's library (`library_*`). Their
//! declarations and what each call does; the reading itself is `vault.rs`, `covers.rs` and
//! `library.rs`.

use std::sync::Arc;

use serde_json::{Value, json};

use super::{on_thread, read_only};
use crate::hub::Hub;
use crate::hub_tools::tool::{Tool, text};
use crate::run;

/// The project's docs as linked notes, so an agent finds what is written before it writes more.
pub(super) fn docs_tools() -> Vec<Tool> {
    let defs = vec![
        json!({
            "name": "docs_map",
            "description": "Every markdown doc in this project, by folder, with its title, summary, tags and how many docs link to it, plus links to docs not written yet. Read this before writing docs, to update the right one instead of adding another.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only(),
        }),
        json!({
            "name": "docs_search",
            "description": "Search this project's markdown docs. Every word must match; titles rank first, then headings, then text. `#tag` words keep docs with that tag. Returns each doc with the section and line that matched.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": { "type": "string", "description": "Words to find, and #tags." } },
                "required": ["query"],
            },
            "annotations": read_only(),
        }),
        json!({
            "name": "docs_for",
            "description": "Before you change code: which docs describe these files, with the text of each section and its lines. Docs say what they cover with `covers:` in their front matter or a `<!-- covers: ... -->` line under a heading. Keep what it says true: when your change makes it wrong or incomplete, update that section in the same turn. A file no doc covers is named too.",
            "inputSchema": {
                "type": "object",
                "properties": { "paths": { "type": "array", "items": { "type": "string" }, "description": "Files or folders relative to the project, such as crates/core/src/proof.rs." } },
                "required": ["paths"],
            },
            "annotations": read_only(),
        }),
        json!({
            "name": "docs_read",
            "description": "Read one doc by path or by name, as a [[link]] would say it, with where its links lead, what links to it and which docs name it without a link.",
            "inputSchema": {
                "type": "object",
                "properties": { "doc": { "type": "string", "description": "A path such as docs/engines.md, or a doc name such as Engines." } },
                "required": ["doc"],
            },
            "annotations": read_only(),
        }),
    ];
    defs.into_iter().map(|def| Tool::of(def, run!(|c| thread!(c, |t| text(docs_call(c.name, &t.cwd, c.args).await))))).collect()
}

async fn docs_call(name: &str, cwd: &str, args: &Value) -> Result<String, String> {
    let root = std::path::PathBuf::from(cwd);
    let name = name.to_owned();
    let arg = |k: &str| args[k].as_str().unwrap_or_default().trim().to_owned();
    let (query, doc) = (arg("query"), arg("doc"));
    let mut paths: Vec<String> = args["paths"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(str::to_owned)).collect();
    if let Some(one) = args["paths"].as_str().or(args["path"].as_str()) {
        paths.push(one.to_owned());
    }
    tokio::task::spawn_blocking(move || match name.as_str() {
        "docs_map" => Ok(crate::vault::outline(&root)),
        "docs_for" => Ok(crate::covers::for_agent(&root, &paths)),
        "docs_search" => Ok(crate::vault::search_for_agent(&root, &query)),
        _ => crate::vault::read_for_agent(&root, &doc).map_err(|e| format!("{e:#}")),
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The user's library, shared by every thread: what they run and build and how it connects.
pub(super) fn library_tools() -> Vec<Tool> {
    let page = json!({ "type": "string", "description": "A path such as tech/Node.js.md, or a note's name such as Node.js." });
    let defs = vec![
        json!({
            "name": "library_map",
            "description": "Every note in the user's library, by folder (repos, tech, machines, notes), with its title, summary and links. Read it to see what is known before answering or writing.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only(),
        }),
        json!({
            "name": "library_search",
            "description": "Search the user's library. Every word must match; titles rank first. Returns each note with the line that matched.",
            "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] },
            "annotations": read_only(),
        }),
        json!({
            "name": "library_read",
            "description": "Read one note from the user's library, with what it links to and what links to it.",
            "inputSchema": { "type": "object", "properties": { "page": page }, "required": ["page"] },
            "annotations": read_only(),
        }),
        json!({
            "name": "library_write",
            "description": "Write a whole note in the user's library: an existing one by name or path, or a new one (put it in repos/, tech/, tools/, machines/ or concepts/, e.g. concepts/Event loop.md). Send the complete markdown: front matter with kind and a one-line summary, a # title, then short sections. Link other notes with [[Name]] and cite sources for facts you looked up. Hub keeps its own \"What Hub knows\" block as it is, so leave it out or copy it unchanged.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "page": page,
                    "content": { "type": "string", "description": "The note's full markdown." },
                    "why": { "type": "string", "description": "A few words for the library's log, e.g. \"explained how hubd reaches Plåtnicklas\"." },
                },
                "required": ["page", "content"],
            },
        }),
        json!({
            "name": "library_understood",
            "description": "Mark a note as understood by the user, today. Only after they passed a learning check on it that you graded: they explained it back and answered your questions well.",
            "inputSchema": {
                "type": "object",
                "properties": { "page": page, "how": { "type": "string", "description": "How they showed it, e.g. \"3 of 3, explained the event loop with hubd\"." } },
                "required": ["page"],
            },
        }),
    ];
    defs.into_iter().map(|def| Tool::of(def, run!(|c| thread!(c, |_t| text(library_call(c.hub, c.name, c.args).await))))).collect()
}

async fn library_call(hub: &Arc<Hub>, name: &str, args: &Value) -> Result<String, String> {
    let root = hub.library().await.map_err(|e| format!("{e:#}"))?;
    let name = name.to_owned();
    let arg = |k: &str| args[k].as_str().unwrap_or_default().to_owned();
    let (query, page, content, why, how) = (arg("query"), arg("page"), arg("content"), arg("why"), arg("how"));
    tokio::task::spawn_blocking(move || match name.as_str() {
        "library_map" => Ok(crate::vault::outline(&root)),
        "library_search" => Ok(crate::vault::search_for_agent(&root, query.trim())),
        "library_read" => crate::vault::read_for_agent(&root, page.trim()).map_err(|e| format!("{e:#}")),
        "library_write" => crate::library::write(&root, &page, &content, &why)
            .map(|rel| format!("Wrote {rel} in the library."))
            .map_err(|e| format!("{e:#}")),
        _ => crate::library::understood(&root, &page, &how)
            .map(|rel| format!("Marked {rel} as understood today."))
            .map_err(|e| format!("{e:#}")),
    })
    .await
    .map_err(|e| e.to_string())?
}
