//! Engines do the actual work. The core talks to every engine through the same two calls,
//! [`run`] and [`complete`]; how a driver reaches its engine is its own business.
//!
//! What a driver owes the core, whatever it speaks underneath:
//!
//! - `session_started` once its session is up, naming the driver, the command and the protocol
//!   version, and the engine's session id stored on the thread so the next start can resume it.
//! - Everything the engine says as `update` events shaped like ACP `SessionUpdate`s
//!   (`agent_message_chunk`, `tool_call`, `plan`, `usage_update`, ...). ACP is Hub's internal
//!   language: a driver for something that speaks another protocol translates into it, so the
//!   log, the surfaces and the fold in `transcript.ts` stay one shape.
//! - Permission questions through `Hub::ask_permission`, and `turn_end` or `error` when a turn
//!   is over.
//!
//! Today there is one driver, `acp`: an ACP agent started as a child process, so Claude Code,
//! Codex or anything else that speaks ACP plugs in through configuration. A driver for
//! `claude -p --output-format stream-json` or a terminal session is a new [`Driver`] variant.

pub mod acp;
pub mod unprompted;

use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::mpsc;

use crate::config::{Config, Driver, EngineConfig};
use crate::hub::Hub;
use crate::model::{Attachment, Event, Thread};

pub(crate) enum Command {
    Prompt(Prompt),
    /// Change a session setting such as the model, effort or permission mode.
    SetConfig { config_id: String, value: serde_json::Value },
    Cancel,
    /// Stop the running turn and take queued user message `seq` next, ahead of the rest.
    Steer(i64),
}

#[derive(Clone)]
pub(crate) struct Prompt {
    pub text: String,
    pub attachments: Vec<Attachment>,
    /// The user message it is, in the log.
    pub seq: i64,
    /// What the user wrote, which ends `text`; Hub's notes go before it. Swapped for the new
    /// text when the message is edited while it waits.
    pub said: String,
    /// What the message is for, for its slip (`package/`): `message` when you wrote it, `switch`,
    /// `rewind` or `moved` when it carries a conversation to a fresh session, `hub:<mark>` for a
    /// message Hub wrote.
    pub purpose: String,
    /// What Hub put in `text` besides `said`, as (part, characters) in the order it was added.
    pub parts: Vec<(String, usize)>,
    /// The recipe that built what Hub put in `text` (`package/`): `legacy` before recipes, or the
    /// name of one such as `handover-v1`.
    pub recipe: &'static str,
}

impl Prompt {
    /// Its slip (`package/`) on the way to `thread_id`'s engine. `parts` holds what Hub put in
    /// front of the words, last added first, so the sections come out in the order of the text.
    pub(crate) fn slip(&self, thread_id: &str, engine: &str, model: Option<&str>, session: &str) -> crate::model::Slip {
        let mut parts: Vec<(String, usize)> = self.parts.iter().rev().cloned().collect();
        let words = if self.purpose.starts_with("hub:") { "hub" } else { "you" };
        parts.push((words.to_owned(), crate::package::chars(&self.said)));
        crate::package::Sent {
            purpose: &self.purpose,
            thread: Some(thread_id),
            seq: Some(self.seq),
            engine,
            model,
            session: Some(session),
            text: &self.text,
            parts: &parts,
            attachments: self.attachments.len(),
            recipe: self.recipe,
        }
        .slip()
    }

    /// The prompt a logged user message was, sent again (after a restart, or a question answered
    /// while the session slept).
    pub(crate) fn resend(e: &Event) -> Self {
        let attachments: Vec<Attachment> = serde_json::from_value(e.mark("attachments").clone()).unwrap_or_default();
        let text = e.said().unwrap_or_default().to_owned();
        Prompt { said: text.clone(), text, attachments, seq: e.seq, purpose: "resend".into(), parts: Vec::new(), recipe: "legacy" }
    }
}

/// Runs one thread's engine until its command channel closes or the engine exits. Prompts the
/// driver had taken from the channel but not yet given the engine when it ended (behind a turn
/// that was stopped by force, or an engine that died) are left in `leftover`, so the caller can
/// hand them to a fresh engine instead of losing them.
pub(crate) async fn run(
    hub: Arc<Hub>,
    thread: Thread,
    engine: EngineConfig,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    leftover: &mut Vec<Prompt>,
    link: Option<crate::keeper::Link>,
) -> Result<()> {
    match engine.driver {
        Driver::Acp => acp::run(hub, thread, engine, commands, leftover, link).await,
    }
}

/// One answer to one prompt in a throwaway session that is not logged, for small helpers such
/// as tidying a message. The engine may only answer in text. `purpose` says what it is for, and
/// goes on the slip (`package/`); every caller names one, so nothing is sent unlabelled.
pub(crate) async fn complete(purpose: &'static str, engine: &EngineConfig, cwd: &str, model: Option<&str>, prompt: String) -> Result<String> {
    match engine.driver {
        Driver::Acp => acp::complete(purpose, engine, cwd, model, prompt).await,
    }
}

/// What the tidy engine (its own pick, else the default engine) answers to `prompt` with the tidy
/// model, trimmed, within `wait`. It runs outside any repository, so it answers from the prompt
/// alone.
pub(crate) async fn ask_tidy(config: &Config, purpose: &'static str, prompt: String, wait: std::time::Duration) -> Result<String> {
    let name = config.tidy.engine.as_ref().unwrap_or(&config.default_engine);
    let engine = config.engines.get(name).with_context(|| format!("unknown engine {name:?}"))?;
    let cwd = std::env::temp_dir().display().to_string();
    let answer = complete(purpose, engine, &cwd, config.tidy.model.as_deref(), prompt);
    let text = tokio::time::timeout(wait, answer).await.context("the model took too long")??;
    Ok(text.trim().to_owned())
}
