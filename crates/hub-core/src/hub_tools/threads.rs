//! The thread tools' handlers for seeing and starting: `hub_brief`, listing and reading a thread
//! (its log folded into a conversation within a character budget), waiting for a message's answer,
//! starting a thread with a brief and attached files, and parking an idea. Acting on an existing
//! thread is `manage.rs`; handing work on and reporting back is `delegate.rs`.

use super::*;

pub(super) async fn brief_tool(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let hours = args["hours"].as_i64().unwrap_or(12).clamp(1, 24 * 30);
    let since = now_ms() - hours * 3_600_000;
    let threads: Vec<Thread> = hub.all_threads().await?.into_iter().filter(|t| !t.archived).collect();
    let (mut needs_you, mut working, mut finished, mut failed) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for t in &threads {
        // Threads on other machines are listed by state only; their logs are not here. Each
        // thread asks the log for the one or two events it shows, never the log itself.
        let local = t.host.is_none();
        let mut item = brief(t);
        match t.status {
            ThreadStatus::WaitingOnYou => {
                let asking = if local { hub.store.last_event(&t.id, EventKind::PermissionRequest).await? } else { None };
                item["asking"] = match asking {
                    Some(e) => json!(format!("to allow: {}", e.tool_call()["title"].as_str().unwrap_or("a tool"))),
                    None => json!(tail(&reply_of(hub, t, local).await?, BRIEF_REPLY)),
                };
                needs_you.push(item);
            }
            ThreadStatus::Running => {
                let asked = if local { hub.store.last_event(&t.id, EventKind::UserMessage).await? } else { None };
                if let Some(e) = &asked {
                    item["working_on"] = json!(clip(e.said().unwrap_or_default(), 200));
                    item["since"] = json!(crate::text::ago(now_ms() - e.ts));
                }
                let doing = if local { hub.store.last_tool_call(&t.id).await? } else { None };
                if let Some(e) = doing.filter(|e| asked.as_ref().is_none_or(|a| e.seq > a.seq)) {
                    item["doing"] = json!(e.data["title"].as_str().unwrap_or("a tool"));
                }
                working.push(item);
            }
            ThreadStatus::Error => {
                let error = if local { hub.store.last_event(&t.id, EventKind::Error).await? } else { None };
                item["error"] = json!(error.map(|e| clip(e.failure().unwrap_or_default(), 400)));
                failed.push(item);
            }
            _ if t.returned_at.unwrap_or(t.updated_at) >= since && finished.len() < BRIEF_FINISHED => {
                item["unread"] = json!(t.returned_at.is_some_and(|r| t.seen_at.is_none_or(|s| r > s)));
                if local {
                    item["reply"] = json!(tail(&reply_of(hub, t, local).await?, BRIEF_REPLY));
                }
                finished.push(item);
            }
            _ => {}
        }
    }
    let unread = finished.iter().filter(|t| t["unread"] == true).count();
    Ok(json!({
        "summary": format!(
            "{} need the user, {} working, {} finished in the last {hours} h ({unread} unread), {} failed",
            needs_you.len(), working.len(), finished.len(), failed.len()
        ),
        "needs_you": needs_you,
        "working": working,
        "finished": finished,
        "failed": failed,
    }))
}

/// The reply of the thread's last turn, read from that turn's chunks only (empty for a thread on
/// another machine).
async fn reply_of(hub: &Arc<Hub>, t: &Thread, local: bool) -> Result<String> {
    if !local {
        return Ok(String::new());
    }
    Ok(reply(&hub.store.last_reply_chunks(&t.id).await?, i64::MAX))
}

pub(super) fn brief(t: &Thread) -> Value {
    let mut v = json!({
        "id": t.id,
        "title": t.title,
        "link": link("thread", &t.id),
        "folder": t.cwd,
        "project_link": link("project", &t.cwd),
        "engine": t.engine,
        "status": t.status,
        "updated": crate::text::ago(now_ms() - t.updated_at),
    });
    if let Some(host) = &t.host {
        v["host"] = json!(host);
    }
    v
}

pub(super) async fn list(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let query = args["query"].as_str().map(str::to_lowercase);
    let folder = match args["project"].as_str() {
        Some(p) => Some(project_folder(hub, p).await?),
        None => None,
    };
    let status = args["status"].as_str();
    let archived = args["archived"].as_bool().unwrap_or(false);
    let limit = args["limit"].as_u64().unwrap_or(20).clamp(1, 200) as usize;
    let threads: Vec<Value> = hub
        .all_threads()
        .await?
        .iter()
        .filter(|t| archived || !t.archived)
        .filter(|t| folder.as_ref().is_none_or(|f| same_folder(&t.cwd, f)))
        .filter(|t| status.is_none_or(|s| json!(t.status) == s))
        .filter(|t| {
            query.as_ref().is_none_or(|q| t.title.to_lowercase().contains(q) || t.cwd.to_lowercase().contains(q))
        })
        .take(limit)
        .map(brief)
        .collect();
    Ok(json!({ "threads": threads }))
}

/// A thread on this machine. Threads on other hosts are listed but not reached from here yet.
pub(super) async fn local_thread(hub: &Arc<Hub>, args: &Value) -> Result<Thread> {
    let id = args["thread_id"].as_str().context("thread_id is missing")?;
    if let Some(thread) = hub.store.thread(id).await? {
        return Ok(thread);
    }
    if hub.remotes.threads().iter().any(|t| t.id == id) {
        bail!("thread {id} runs on another machine; Hub's tools reach threads on this one only so far");
    }
    bail!("no thread {id}; list_threads shows the ids")
}

pub(super) async fn read(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let thread = local_thread(hub, args).await?;
    let activity = args["view"] == "activity";
    let after = args["after_seq"].as_i64();
    let from = after.unwrap_or(0);
    // Where the log ends, read first so `next_seq` never passes an event logged while reading.
    let last = hub.store.last_seq(&thread.id, None).await?.unwrap_or(0).max(from);
    let kinds = [EventKind::UserMessage, EventKind::PermissionRequest, EventKind::TurnEnd, EventKind::Error];
    let mut events = hub.store.events_with_updates(&thread.id, from, &kinds, crate::replies::SAID).await?;
    events.retain(|e| e.seq <= last);
    let items = fold(&events, activity);
    let mut out = json!({ "thread": brief(&thread) });
    match after {
        // Reading on: from the start, until the budget is spent.
        Some(_) => {
            let taken = within(items.iter(), READ_CHARS);
            let more = taken < items.len();
            let next = if more { items[taken].seq - 1 } else { last };
            out["items"] = json!(items[..taken]);
            out["next_seq"] = json!(next);
            out["more"] = json!(more);
        }
        // The latest part: from the end, back until the budget is spent.
        None => {
            let taken = within(items.iter().rev(), READ_CHARS);
            let skipped = items.len() - taken;
            out["items"] = json!(items[skipped..]);
            out["next_seq"] = json!(last);
            if skipped > 0 {
                out["earlier"] = json!(format!(
                    "{skipped} earlier items left out; read_thread with after_seq 0 reads from the start"
                ));
            }
        }
    }
    Ok(out)
}

/// How many of `items` fit in `budget` characters; always at least one.
pub(super) fn within<'a>(items: impl Iterator<Item = &'a Item>, budget: usize) -> usize {
    let mut spent = 0;
    let mut taken = 0;
    for item in items {
        let size = item.text.len() + 40;
        if taken > 0 && spent + size > budget {
            break;
        }
        spent += size;
        taken += 1;
    }
    taken
}

#[derive(serde::Serialize)]
pub(super) struct Item {
    pub(super) seq: i64,
    pub(super) who: &'static str,
    pub(super) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) from: Option<String>,
}

/// The log as a conversation: messages, the agent's replies (chunks joined), questions to the
/// user and errors; with `activity` also tool calls and turn ends.
pub(super) fn fold(events: &[Event], activity: bool) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    for e in events {
        let d = &e.data;
        let (who, text, from) = match e.kind {
            EventKind::UserMessage => {
                let from = e.mark("from")["title"].as_str().or(e.mark("from")["agent"].as_str()).map(str::to_owned);
                let who = if ["review", "leak", "customs", "proof", "docs"].iter().any(|k| e.has(k)) { "hub" } else { "user" };
                (if from.is_some() { "agent" } else { who }, e.said().unwrap_or_default().to_owned(), from)
            }
            EventKind::Update if crate::model::update_kind(d) == Some("agent_message_chunk") => {
                let text = d["content"]["text"].as_str().unwrap_or_default();
                if let Some(last) = items.last_mut()
                    && last.who == "reply"
                {
                    last.text.push_str(text);
                    continue;
                }
                ("reply", text.to_owned(), None)
            }
            EventKind::Update if activity && crate::model::update_kind(d) == Some("tool_call") => {
                ("tool", d["title"].as_str().unwrap_or("a tool").to_owned(), None)
            }
            EventKind::PermissionRequest => {
                ("question", format!("Waiting for the user to allow: {}", e.tool_call()["title"].as_str().unwrap_or("a tool")), None)
            }
            EventKind::TurnEnd if activity => ("turn_end", e.stop_reason().unwrap_or("end_turn").to_owned(), None),
            EventKind::Error => ("error", e.failure().unwrap_or_default().to_owned(), None),
            _ => continue,
        };
        items.push(Item { seq: e.seq, who, text, from });
    }
    for item in &mut items {
        item.text = clip(&item.text, ITEM_CHARS);
    }
    items
}

use crate::text::clip_counted as clip;

pub(super) async fn wait_tool(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let thread = local_thread(hub, args).await?;
    let Some(asked) = hub.store.last_event(&thread.id, EventKind::UserMessage).await?.map(|e| e.seq) else {
        return Ok(json!({ "thread_id": thread.id, "state": "idle", "reply": null, "note": "nothing was sent to it yet" }));
    };
    wait(hub, &thread.id, asked, timeout(args)).await
}

pub(super) fn timeout(args: &Value) -> Duration {
    Duration::from_secs(args["timeout_s"].as_u64().unwrap_or(WAIT).clamp(1, WAIT_MAX))
}

/// Keeps an idea in the inbox and places it, then says where it went. A project given by name
/// is used as the place to look first; the caller's own folder is the guess otherwise.
pub(super) async fn park_idea(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let text = args["idea"].as_str().map(str::trim).filter(|m| !m.is_empty()).context("idea is missing")?;
    let here = match (args["project"].as_str().filter(|p| !p.trim().is_empty()), caller) {
        (Some(p), _) => Some(project_folder(hub, p).await?),
        (None, Caller::Thread(t)) => Some(t.cwd.clone()),
        (None, Caller::Agent { .. }) => None,
    };
    let named = args["project"].as_str().is_some_and(|p| !p.trim().is_empty());
    let mut idea = hub.capture_idea(crate::model::NewIdea { text: text.to_owned(), here: here.clone() }, true).await?;
    // A project named by the caller is where it goes, whatever the guess said.
    if let (true, Some(folder)) = (named, here)
        && idea.project.as_deref().is_none_or(|p| !same_folder(p, &folder)) {
            idea = hub.move_idea(&idea.id, crate::model::MoveIdea { project: folder, stage: None }).await?;
        }
    let place = match (&idea.name, &idea.stage) {
        (Some(n), Some(s)) => format!("in {n}'s focus, under the ideas of the stage \"{s}\""),
        (Some(n), None) => format!("in {n}'s focus, under Parked"),
        _ => "in Hub's idea inbox; no project could be guessed, so the user picks one there".to_owned(),
    };
    Ok(json!({ "idea": idea.text, "kept": place, "link": idea.project.as_deref().map(|p| link("project", p)) }))
}

pub(super) async fn start(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let (thread, asked) = open(hub, caller, args).await?;
    let mut out = json!({ "thread": brief(&thread) });
    if args["wait"].as_bool().unwrap_or(false) {
        out["result"] = wait(hub, &thread.id, asked, timeout(args)).await?;
    }
    Ok(out)
}

/// Starts a thread with the caller's brief: the thread and the seq of the brief.
pub(super) async fn open(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<(Thread, i64)> {
    let message = args["message"].as_str().map(str::trim).filter(|m| !m.is_empty()).context("message is missing")?;
    let own = match caller {
        Caller::Thread(t) => Some(t),
        Caller::Agent { .. } => None,
    };
    let folder = match (args["folder"].as_str(), args["project"].as_str()) {
        (Some(f), _) if !f.trim().is_empty() => f.trim().to_owned(),
        (_, Some(p)) => project_folder(hub, p).await?,
        _ => own.map(|t| t.cwd.clone()).context("say which project or folder the thread works in")?,
    };
    // A project's desk steers its own project only.
    if let Some(own) = own
        && !same_folder(&folder, &own.cwd)
        && crate::desk::is_desk(&hub.ctx(), own).await
    {
        bail!("you are the desk of {}; start threads in this project only", own.cwd);
    }
    // Files are where the caller works: "review.md" is the one it just wrote.
    let attachments = attach(args, own.map_or(folder.as_str(), |t| t.cwd.as_str()))?;
    // Before the thread exists, so a handover that fails leaves no empty thread behind.
    let text = brief_with(hub, own, message, args).await?;
    let engine = match args["engine"].as_str() {
        Some(e) => e.to_owned(),
        None => own.map(|t| t.engine.clone()).unwrap_or_else(|| hub.config.default_engine.clone()),
    };
    // A thread inherits its starter's settings, so it is never allowed more than the starter.
    let mut settings: BTreeMap<String, Value> = match own {
        Some(t) if t.engine == engine => hub.store.settings(&t.id).await?.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    if let Some(model) = args["model"].as_str() {
        let (id, value) = pick_model(hub, &engine, model).await?;
        settings.insert(id, json!(value));
    }
    if let Some(given) = args["settings"].as_object() {
        if caller.access() < AgentAccess::Full {
            bail!("settings need full access; pick a model with model");
        }
        settings.extend(given.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    let thread = hub
        .create_thread_as(
            NewThread {
            cwd: folder,
            title: args["title"].as_str().map(str::to_owned),
            engine: Some(engine),
            message: None,
            attachments: vec![],
            settings,
            original: None,
            client_id: None,
            attempts: vec![],
            warm: None,
                design: false,
                reach: None,
                cloud: false,
                unattended: None,
            },
            None,
            crate::system::agent(own),
        )
        .await?;
    if args["title"].as_str().is_none_or(|t| t.trim().is_empty()) {
        // Named after the brief, as a thread the user starts is; the engine may rename it.
        let title = crate::hub::title_from_message(message);
        hub.update_thread(&thread.id, move |t| t.title = title).await?;
    }
    let asked = deliver(hub, caller, &thread.id, &text, attachments).await?;
    let thread = hub.store.thread(&thread.id).await?.unwrap_or(thread);
    Ok((thread, asked))
}

/// The brief with as much of the caller's own thread as `context` asks for, as a side job gets it.
pub(super) async fn brief_with(hub: &Arc<Hub>, own: Option<&Thread>, message: &str, args: &Value) -> Result<String> {
    let context = args["context"].as_str().unwrap_or("brief");
    if !CONTEXTS.contains(&context) {
        bail!("unknown context {context:?}; one of {}", CONTEXTS.join(", "));
    }
    let Some(own) = own.filter(|_| context != "brief") else {
        if context != "brief" {
            bail!("context {context:?} is for agents in a Hub thread; put what the thread needs in message");
        }
        return Ok(message.to_owned());
    };
    Ok(match context {
        "handover" => {
            let handover = hub.handoff(&own.id).await.context("writing the handover")?;
            format!("{message}\n\nWhere the thread \"{}\" that started this one stands:\n\n{handover}", own.title)
        }
        _ => {
            let log = crate::hub::conversation_log(&crate::replies::conversation(&hub.store, &own.id, &[]).await?);
            format!("{message}\n\nThe conversation of the thread \"{}\" that started this one, for context only:\n\n<conversation>{log}\n</conversation>", own.title)
        }
    })
}

/// The files an agent attaches to a brief, as links the new thread's engine reads itself.
/// A relative path is in `folder`. Every file has to be there.
pub(super) fn attach(args: &Value, folder: &str) -> Result<Vec<Attachment>> {
    let Some(files) = args["files"].as_array() else { return Ok(vec![]) };
    let mut out = Vec::new();
    for f in files.iter().filter_map(Value::as_str).map(str::trim).filter(|f| !f.is_empty()).take(20) {
        let path = std::path::Path::new(folder).join(f);
        if !path.is_file() {
            bail!("no file {f} (looked at {}); attach files that exist, or put the text in message", path.display());
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| f.to_owned());
        out.push(Attachment { mime: mime_of(&name).to_owned(), name, data: None, text: None, path: Some(path.to_string_lossy().into_owned()), url: None });
    }
    Ok(out)
}

/// A file's type by its name, close enough for an engine to know how to read it.
pub(super) fn mime_of(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" => "text/markdown",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "csv" => "text/csv",
        _ => "text/plain",
    }
}
