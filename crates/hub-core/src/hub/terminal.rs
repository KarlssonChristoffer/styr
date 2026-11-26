//! The `Hub` half of taking a thread over in a terminal (`terminal.rs`): letting go of the thread
//! for `hubd attach`, opening a terminal on it, and carrying on the turn taking over stopped. The
//! CLI side and the log readings stay in `terminal.rs`.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde_json::json;

use super::Hub;
use crate::model::{EventKind, Takeover, TerminalData, TerminalOpened, Thread, ThreadStatus};
use crate::terminal::*;

impl Hub {
    /// Lets go of a thread so `hubd attach` (process `pid`) can run the engine's CLI on its
    /// session: stops its turn and its engine, and refuses messages from Hub until it is handed
    /// back.
    pub async fn take_over(&self, thread_id: &str, pid: Option<u32>) -> Result<Takeover> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let engine = self.config.engines.get(&thread.engine);
        let template = engine
            .and_then(|e| terminal_command(&thread.engine, e))
            .with_context(|| format!("the {} engine has no terminal command (`terminal` in hub.json)", thread.engine))?;
        let session = thread
            .engine_session_id
            .clone()
            .context("this thread has no session yet; send it a first message, then take it over")?;
        // Only the kinds `taken_by` and `session_prompts` read, not the whole log.
        let kinds = [EventKind::UserMessage, EventKind::TurnEnd, EventKind::Error, EventKind::SessionStarted, EventKind::Terminal];
        let events = self.store.events_of(thread_id, 0, &kinds).await?;
        if thread.status == ThreadStatus::InTerminal
            && let Some(holder) = taken_by(&events).filter(|p| Some(*p) != pid && alive(*p))
        {
            bail!("the thread is already open in a terminal (process {holder})");
        }
        // An engine's CLI resumes only a session that has had a conversation: one that was started
        // ahead of its first message (or after a move or an engine switch) has no transcript, and
        // `claude --resume` says "No conversation found". Such a session is started under its id.
        let fresh = if session_prompts(&events) > 0 || self.transcript_exists(&thread.engine, &session).await {
            None
        } else {
            let own = self.config.traits(&thread.engine).terminal_new.filter(|t| !t.trim().is_empty());
            Some(own.with_context(|| {
                format!("nothing has been said in this session yet, so the {} CLI has nothing to resume; send a message first, then take it over", thread.engine)
            })?)
        };
        // A turn in the middle of its work is stopped, and carried on when the thread comes back
        // unless the terminal finishes it (`carry_on_after_terminal`).
        let cut = if self.engines().iter().any(|t| t == thread_id) { self.store.last_turn(thread_id).await?.cut_off } else { None };
        self.shutdown(thread_id, false).await?;
        let command = fresh.as_ref().unwrap_or(&template).replace("{session}", &session);
        let taken = TerminalData { pid: Some(pid.map(u64::from)), command: Some(command.clone()), cut, ..TerminalData::state("taken") };
        self.emit_typed(thread_id, &taken).await?;
        let thread = self.set_status_because(thread_id, ThreadStatus::InTerminal, "terminal").await?;
        let env = engine.map(|e| e.env.clone()).unwrap_or_default();
        Ok(Takeover { thread_id: thread.id, title: thread.title, cwd: thread.cwd, command, stopped_turn: cut.is_some(), fresh: fresh.is_some(), env })
    }

    /// The engine has a log of this session on this machine (`session_log`).
    async fn transcript_exists(&self, engine: &str, session: &str) -> bool {
        let Some(pattern) = self.config.traits(engine).session_log else { return false };
        let session = session.to_owned();
        tokio::task::spawn_blocking(move || crate::usage_detail::find(&pattern, &session).is_some()).await.unwrap_or(false)
    }

    /// Takes a thread back from the terminal and starts its engine right away, which copies what
    /// was said there into the log (see `acp.rs`).
    pub async fn hand_back(self: &Arc<Self>, thread_id: &str) -> Result<Thread> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        if thread.status != ThreadStatus::InTerminal {
            return Ok(thread);
        }
        self.emit_typed(thread_id, &TerminalData::state("back")).await?;
        let thread = self.set_status_because(thread_id, ThreadStatus::Idle, "terminal_back").await?;
        if let Err(err) = self.runtime(thread.clone()) {
            tracing::warn!(thread = %thread_id, "could not load the session after the terminal: {err:#}");
        }
        Ok(thread)
    }

    /// Opens the thread in a terminal on this machine, taking it over there: a Windows Terminal
    /// window on Windows, a tmux session elsewhere. Says how to get to it either way.
    pub async fn open_terminal(&self, thread_id: &str) -> Result<TerminalOpened> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        let short = short_id(&thread.id);
        let attach = format!("hubd attach {short}");
        let Some(exe) = hubd_exe() else {
            return Ok(TerminalOpened { opened: false, session: None, attach });
        };
        if cfg!(windows) {
            let exe = exe.display().to_string();
            // The agent writes the title, and cmd would run what it put after a `&` or a `"`.
            let title = crate::machine::window_title(&format!("Hub · {}", thread.title));
            let tries: [(&str, Vec<&str>); 2] = [
                ("wt", vec!["new-tab", "--title", &title, "-d", &thread.cwd, &exe, ATTACH_ARG, &thread.id, "--if-idle"]),
                ("cmd", vec!["/C", "start", &title, "/D", &thread.cwd, &exe, ATTACH_ARG, &thread.id, "--if-idle"]),
            ];
            let mut last = None;
            for (program, args) in tries {
                if program == "cmd" && !crate::machine::cmd_safe(args.iter().copied()) {
                    bail!("could not open a terminal: the folder's name has a character cmd would run; install Windows Terminal (wt)");
                }
                match crate::proc::command_with_console(program).args(&args).current_dir(&thread.cwd).spawn() {
                    Ok(_) => return Ok(TerminalOpened { opened: true, session: None, attach }),
                    Err(e) => last = Some(e),
                }
            }
            bail!("could not open a terminal: {}", last.map(|e| e.to_string()).unwrap_or_default());
        }
        if !tmux_available() {
            return Ok(TerminalOpened { opened: false, session: None, attach });
        }
        let name = {
            let thread = thread.clone();
            tokio::task::spawn_blocking(move || start_tmux(&thread, true)).await??
        };
        Ok(TerminalOpened { opened: false, attach: format!("tmux attach -t {name}"), session: Some(name) })
    }

    /// Carries on a turn taking over stopped (see `carry_on_after_terminal`), once the engine that
    /// just loaded the session can take a prompt.
    pub(crate) fn carry_on_after_takeover(self: &Arc<Self>, thread_id: &str, taken: i64) {
        let hub = self.clone();
        let thread_id = thread_id.to_owned();
        tokio::spawn(async move {
            let mark = Some(("terminal", json!(taken)));
            if let Err(err) = hub.send_from(&thread_id, CARRY_ON_TERMINAL.to_owned(), Vec::new(), None, mark, None).await {
                tracing::warn!(thread = %thread_id, "could not carry on after the terminal: {err:#}");
            }
        });
    }
}
