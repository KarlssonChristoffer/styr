//! A project's desk: one long-lived thread per project that you talk to instead of to each
//! thread. It works in the project's folder with Hub's tools (`hub_tools.rs`) held to that
//! project, so it can say how the work goes, start threads and steer them, but it does not write
//! code itself. What the project's other threads do comes back to it as `desk_note` events: a
//! thread finished, needs you or failed. Those are quiet lines in the desk; they do not start a
//! turn.
//!
//! Which thread is a folder's desk is kept in the kv table under `desk:<folder>`, the folder
//! normalised as `same_folder` compares them. Making a desk and leaving notes in it need the
//! running core and are `Hub` methods in `hub/desk.rs`.

use anyhow::Result;

use crate::ctx::Ctx;
use crate::model::Thread;

const PREFIX: &str = "desk:";

/// A folder as desks are keyed: forward slashes, no trailing slash, lower case.
pub fn norm(folder: &str) -> String {
    folder.trim().trim_end_matches(['/', '\\']).replace('\\', "/").to_lowercase()
}

pub(crate) fn key(folder: &str) -> String {
    format!("{PREFIX}{}", norm(folder))
}

/// The ids of every desk thread.
pub async fn ids(ctx: &Ctx<'_>) -> Result<Vec<String>> {
    Ok(ctx.store.values_with_prefix(PREFIX).await?.into_iter().map(|(_, id)| id).collect())
}

/// The desk of `folder`, when it has one that still exists.
pub async fn of(ctx: &Ctx<'_>, folder: &str) -> Result<Option<Thread>> {
    let Some(id) = ctx.store.get_value(&key(folder)).await? else { return Ok(None) };
    ctx.store.thread(&id).await
}

/// Whether `thread` is its folder's desk.
pub async fn is_desk(ctx: &Ctx<'_>, thread: &Thread) -> bool {
    ctx.store.get_value(&key(&thread.cwd)).await.ok().flatten().is_some_and(|id| id == thread.id)
}

/// What a desk's agent is told as it connects to Hub's tools, after Hub's own instructions.
pub fn instructions(folder: &str) -> String {
    format!(
        "You are this project's desk. The user comes here to steer all the work in the project \
         ({folder}) from one place, often by voice, instead of talking to each thread.\n\
         - You do not write code or edit files yourself. Hand work to threads: `delegate` (its \
         reply comes back here) or `start_thread`, always in this project and with a complete brief, \
         since a new thread sees nothing else. Then say in one line which thread is on it.\n\
         - Steer running threads with `send_to_thread` and `stop_thread`, after `read_thread`. \
         Never redirect a thread the user is working in unless they asked.\n\
         - \"How is it going?\": `hub_brief` or `list_threads` with project set to this folder, and \
         answer only about this project: what needs the user, what is working, what finished.\n\
         - Read `docs/FOCUS.md` when the user asks what is next or what to test: it holds the \
         stage now, what it delivers and what the user must test by hand.\n\
         - Hub shows the user when this project's threads finish, ask or fail as lines in this \
         chat; you are not told. Look with the tools when the user asks.\n\
         - Keep answers short and plain, in the language the user writes in, and link every \
         thread you mention with the `link` the tools give."
    )
}
