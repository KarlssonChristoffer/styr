//! Acting on an existing thread: sending it a message marked as the caller's, the managing tools
//! (each a `ThreadPatch` like the user's own buttons) and stopping it, with the rule that a desk
//! acts on its own project's threads only. Also the project-folder and model lookups the starting
//! tools in `threads.rs` use.

use super::*;

pub(super) async fn send(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let thread = local_thread(hub, args).await?;
    if matches!(caller, Caller::Thread(own) if own.id == thread.id) {
        bail!("that is your own thread");
    }
    let message = args["message"].as_str().map(str::trim).filter(|m| !m.is_empty()).context("message is missing")?;
    let busy = thread.status == ThreadStatus::Running;
    let asked = deliver(hub, caller, &thread.id, message, vec![]).await?;
    let mut out = json!({ "thread_id": thread.id, "queued": busy, "seq": asked });
    if args["wait"].as_bool().unwrap_or(false) {
        out["result"] = wait(hub, &thread.id, asked, timeout(args)).await?;
    }
    Ok(out)
}

/// Sends `message` marked as the caller's, and returns its seq.
pub(super) async fn deliver(hub: &Arc<Hub>, caller: &Caller, thread_id: &str, message: &str, attachments: Vec<Attachment>) -> Result<i64> {
    hub.send_from(thread_id, message.to_owned(), attachments, None, Some(("from", caller.mark())), None).await?;
    let events = hub.store.events_of(thread_id, 0, &[EventKind::UserMessage]).await?;
    let mark = caller.mark();
    events
        .iter()
        .rev()
        .find(|e| e.kind == EventKind::UserMessage && *e.mark("from") == mark)
        .map(|e| e.seq)
        .context("the message was not logged")
}

/// The thread a managing tool acts on: the one named by `thread_id`, or the caller's own. A desk
/// manages its own project's threads only.
pub(super) async fn target(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Thread> {
    let named = args["thread_id"].as_str().map(str::trim).filter(|id| !id.is_empty());
    let thread = match (named, caller) {
        (Some(_), _) => local_thread(hub, args).await?,
        (None, Caller::Thread(own)) => hub.store.thread(&own.id).await?.context("your thread is gone")?,
        (None, Caller::Agent { .. }) => bail!("thread_id is missing; list_threads shows the ids"),
    };
    if let Caller::Thread(own) = caller
        && own.id != thread.id
        && !same_folder(&thread.cwd, &own.cwd)
        && crate::desk::is_desk(&hub.ctx(), own).await
    {
        bail!("you are the desk of {}; manage threads in this project only", own.cwd);
    }
    Ok(thread)
}

/// What a managing tool changes, as the patch the user's own buttons send.
pub(super) fn patch_for(name: &str, args: &Value) -> Result<ThreadPatch> {
    Ok(match name {
        "settle_thread" => ThreadPatch { settled: Some(!args["back"].as_bool().unwrap_or(false)), ..Default::default() },
        "rename_thread" => {
            let title = args["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).context("title is missing")?;
            ThreadPatch { title: Some(title.chars().take(120).collect()), ..Default::default() }
        }
        "pin_thread" => ThreadPatch { pinned: Some(args["pinned"].as_bool().unwrap_or(true)), ..Default::default() },
        "snooze_thread" => {
            let hours = args["hours"].as_f64().context("hours is missing")?;
            if !(0.0..=24.0 * 365.0).contains(&hours) {
                bail!("hours is from 0 to a year");
            }
            let until = if hours == 0.0 { 0 } else { now_ms() + (hours * 3_600_000.0) as i64 };
            ThreadPatch { snooze_until: Some(until), ..Default::default() }
        }
        other => bail!("no tool {other}"),
    })
}

pub(super) async fn manage(hub: &Arc<Hub>, caller: &Caller, name: &str, args: &Value) -> Result<Value> {
    let patch = patch_for(name, args)?;
    let thread = target(hub, caller, args).await?;
    let own = matches!(caller, Caller::Thread(t) if t.id == thread.id);
    let thread = hub.patch_thread(&thread.id, patch).await?;
    let note = match name {
        "settle_thread" if thread.settled_at.is_some() && own => {
            "Settled: this thread is on the Settled shelf. Say in one line what you did, if you have not, and end your turn."
        }
        "settle_thread" if thread.settled_at.is_some() => "Settled.",
        "settle_thread" => "Back in the list.",
        "rename_thread" => "Renamed.",
        "pin_thread" if thread.pinned_at.is_some() => "Pinned on top of the list.",
        "pin_thread" => "Unpinned.",
        _ if thread.snoozed_until.is_some() => "Snoozed.",
        _ => "Awake, back in the list.",
    };
    Ok(json!({ "thread": brief(&thread), "note": note }))
}

pub(super) async fn stop(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let thread = local_thread(hub, args).await?;
    if matches!(caller, Caller::Thread(own) if own.id == thread.id) {
        bail!("that is your own thread; end your turn instead");
    }
    let working = thread.status == ThreadStatus::Running;
    hub.cancel(&thread.id).await?;
    Ok(json!({ "thread_id": thread.id, "stopped": working }))
}

/// The folder of the project called (or in) `name` (`folders::project_folder`).
pub(super) async fn project_folder(hub: &Arc<Hub>, name: &str) -> Result<String> {
    crate::folders::project_folder(&hub.store, name).await
}

/// The model setting an engine offers and its choices (`engine_models::offered`).
pub(super) async fn models(hub: &Arc<Hub>, engine: &str) -> Option<(String, Vec<(String, String)>)> {
    crate::engine_models::offered(&hub.store, engine).await
}

/// The setting that picks `model` on `engine` (`engine_models::pick`).
pub(super) async fn pick_model(hub: &Arc<Hub>, engine: &str, model: &str) -> Result<(String, String)> {
    crate::engine_models::pick(&hub.store, engine, model).await
}
