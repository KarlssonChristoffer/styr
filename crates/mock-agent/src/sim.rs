//! The inbox simulator: a fake agent with no AI behind it. Each thread gets a scenario (the
//! `scenario` setting) and plays it on a timer: it works for a while with made-up tool calls,
//! then finishes, asks for permission, asks a question in its reply, or fails. Anything
//! written to it afterwards gets a short follow-up turn. Hub's Labs page starts a handful of these
//! (`hub_core::sim`) so the Activity list can be tried without spending tokens.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    PermissionOption, PermissionOptionKind, PromptRequest, RequestPermissionOutcome, RequestPermissionRequest,
    SessionNotification, SessionUpdate, StopReason, ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields,
    ToolKind,
};
use agent_client_protocol::{Client, ConnectionTo, Result};

use crate::{LockExt, text};

/// How a scenario's first turn ends.
enum End {
    Done(&'static str),
    /// Ends the turn with a question in the reply, so it needs an answer but not a click.
    Question(&'static str),
    /// The turn fails with this error.
    Fail(&'static str),
}

struct Script {
    /// Seconds of fake work before anything else.
    work: u64,
    /// A permission question after the work, and the seconds of work once it is allowed.
    ask: Option<(&'static str, u64)>,
    end: End,
}

fn script(name: &str) -> Script {
    match name {
        "ask" => Script {
            work: 12,
            ask: Some(("Run `npm install @tauri-apps/plugin-store`", 10)),
            end: End::Done("Added the toggle under Settings > General. It follows the system theme until you pick one, and remembers your pick."),
        },
        "late_ask" => Script {
            work: 45,
            ask: Some(("Run `npm run migrate` on the dev database", 12)),
            end: End::Done("Migrated: 1 240 rows moved to the new table. The old columns stay until you say they can go."),
        },
        "question" => Script {
            work: 35,
            ask: None,
            end: End::Question(
                "I found two ways to do this:\n\n1. **Cache results on the server.** Fast for everyone, about 20 lines.\n2. **Debounce in the browser.** Simpler, but only helps while typing.\n\nWhich one do you want?",
            ),
        },
        "fail" => Script {
            work: 25,
            ask: None,
            end: End::Fail("Build failed: linking with `link.exe` failed: exit code 1120 (simulated)"),
        },
        "long" => Script {
            work: 80,
            ask: None,
            end: End::Done("Wrote `docs/pairing.md`: how a code is made, how long it lives and what to do when pairing fails. Linked it from the README."),
        },
        _ => Script {
            work: 20,
            ask: None,
            end: End::Done("Fixed. The test waited on a timer that raced the redirect; it now waits for the redirect itself. Ran it 50 times, all green."),
        },
    }
}

/// Made-up steps shown while it works, in turn.
const STEPS: &[(&str, ToolKind)] = &[
    ("Read src/main.rs", ToolKind::Read),
    ("Search for \"redirect\"", ToolKind::Search),
    ("Read src/lib/api.ts", ToolKind::Read),
    ("Edit src/lib/api.ts", ToolKind::Edit),
    ("Run `cargo test`", ToolKind::Execute),
    ("Read README.md", ToolKind::Read),
    ("Edit src/settings.rs", ToolKind::Edit),
    ("Run `npm run check`", ToolKind::Execute),
];

/// Made-up tests a command prints, a line a tick.
const TESTS: &[&str] = &[
    "auth::pairing_code_expires", "auth::redirect_waits_for_session", "api::lists_threads",
    "api::refuses_stale_token", "store::appends_in_order", "store::reopens_after_crash",
    "settings::saves_theme", "settings::keeps_unknown_keys",
];

#[derive(Default)]
pub struct Sim {
    scenario: Mutex<String>,
    /// Turns played by this process. A resumed session has had its first turn already.
    turns: Mutex<u32>,
}

impl Sim {
    pub fn pick(&self, name: &str) {
        *self.scenario.locked() = name.to_owned();
    }

    pub fn resumed(&self) {
        *self.turns.locked() += 1;
    }

    pub async fn turn(&self, req: PromptRequest, cx: &ConnectionTo<Client>, cancelled: &AtomicBool) -> Result<StopReason> {
        let first = {
            let mut turns = self.turns.locked();
            *turns += 1;
            *turns == 1
        };
        let session = req.session_id.clone();
        let say = |update: SessionUpdate| cx.send_notification(SessionNotification::new(session.clone(), update));
        let mut step = 0;

        if !first {
            if !work(&say, &mut step, 10, cancelled).await? {
                return Ok(StopReason::Cancelled);
            }
            return reply(&say, "Done, the way you said. Everything still passes.", cancelled).await;
        }

        let script = script(&self.scenario.locked());
        if !work(&say, &mut step, script.work, cancelled).await? {
            return Ok(StopReason::Cancelled);
        }
        if let Some((title, after)) = script.ask {
            say(SessionUpdate::ToolCall(ToolCall::new("ask", title).kind(ToolKind::Execute).status(ToolCallStatus::InProgress)))?;
            let ask = RequestPermissionRequest::new(
                req.session_id.clone(),
                ToolCallUpdate::new("ask", ToolCallUpdateFields::new().title(title)),
                vec![
                    PermissionOption::new("allow", "Allow", PermissionOptionKind::AllowOnce),
                    PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
                ],
            );
            let answer = cx.send_request(ask).block_task().await?;
            let allowed = matches!(&answer.outcome, RequestPermissionOutcome::Selected(s) if &*s.option_id.0 == "allow");
            let status = if allowed { ToolCallStatus::Completed } else { ToolCallStatus::Failed };
            say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new("ask", ToolCallUpdateFields::new().status(status))))?;
            if cancelled.load(Ordering::SeqCst) {
                return Ok(StopReason::Cancelled);
            }
            if !allowed {
                return reply(&say, "Ok, I left that alone. Tell me if you want it done another way.", cancelled).await;
            }
            if !work(&say, &mut step, after, cancelled).await? {
                return Ok(StopReason::Cancelled);
            }
        }
        match script.end {
            End::Done(text) | End::Question(text) => reply(&say, text, cancelled).await,
            End::Fail(why) => Err(agent_client_protocol::Error::new(-32603, why)),
        }
    }
}

/// Fake work for `secs`: a tool call every few seconds. False when the turn was stopped.
async fn work(say: &impl Fn(SessionUpdate) -> Result<()>, step: &mut usize, secs: u64, cancelled: &AtomicBool) -> Result<bool> {
    let mut left = secs * 4;
    while left > 0 {
        let (title, kind) = STEPS[*step % STEPS.len()];
        let id = format!("step-{step}");
        *step += 1;
        say(SessionUpdate::ToolCall(ToolCall::new(id.clone(), title).kind(kind).status(ToolCallStatus::InProgress)))?;
        // 2 to 4 seconds a step, so the list keeps moving.
        let ticks = (8 + (*step as u64 * 5) % 9).min(left);
        for tick in 0..ticks {
            if cancelled.load(Ordering::SeqCst) {
                return Ok(false);
            }
            // A command prints a line a tick, as codex-acp streams it, so its terminal moves.
            if kind == ToolKind::Execute {
                let line = format!("test {} ... \x1b[32mok\x1b[0m\r\n", TESTS[(*step + tick as usize) % TESTS.len()]);
                let chunk = serde_json::json!({ "terminal_output_delta": { "terminal_id": id, "data": line } });
                say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id.clone(), ToolCallUpdateFields::new()).meta(chunk.as_object().cloned())))?;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        left -= ticks;
        say(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id, ToolCallUpdateFields::new().status(ToolCallStatus::Completed))))?;
    }
    Ok(true)
}

async fn reply(say: &impl Fn(SessionUpdate) -> Result<()>, words: &str, cancelled: &AtomicBool) -> Result<StopReason> {
    for word in words.split_inclusive(' ') {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(StopReason::Cancelled);
        }
        say(SessionUpdate::AgentMessageChunk(text(word)))?;
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    Ok(StopReason::EndTurn)
}
