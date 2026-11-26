//! Signing an engine in on a machine from the app, for engines whose sign-in is a link and a code
//! (`codex login --device-auth`): Hub runs the engine's `sign_in` command there in the background,
//! and the app shows the link and the code it prints. You open the link on any device, type the
//! code, and the engine saves its own login on that machine. No terminal, no SSH window.
//!
//! Engines whose sign-in prints a lasting token (Claude Code's `setup-token`) are signed in once
//! on one machine with `hubd sign-in` instead; that token then goes to every machine
//! (`signin::SharedSignIn`). Nothing here names an engine: the command comes from its traits.

use crate::locked::LockExt;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::hub::Hub;

/// How long a link and code stay good is up to the engine; Hub stops waiting after this.
const WAIT: Duration = Duration::from_secs(15 * 60);
const KEEP_LINES: usize = 40;
const INSTALL_WAIT: Duration = Duration::from_secs(10 * 60);

/// A sign-in running or ran on this machine since the core started.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LoginJob {
    pub engine: String,
    pub running: bool,
    /// Signed in (true) or not (false). None while it waits.
    pub ok: Option<bool>,
    /// The page to open, once the engine printed it.
    pub url: Option<String>,
    /// The code to type there.
    pub code: Option<String>,
    pub message: String,
    /// What the engine printed, without colours.
    pub lines: Vec<String>,
}

static JOBS: LazyLock<Mutex<BTreeMap<String, LoginJob>>> = LazyLock::new(Default::default);

pub fn job(engine: &str) -> Option<LoginJob> {
    JOBS.locked().get(engine).cloned()
}

/// The command that signs `engine` in with a link and a code, or why there is none.
fn command(hub: &Hub, engine: &str) -> Result<String> {
    if !hub.config.engines.contains_key(engine) {
        bail!("no engine called {engine} here");
    }
    let traits = hub.config.traits(engine);
    match (traits.sign_in, traits.sign_in_token) {
        (Some(_), Some(_)) => bail!("{engine} signs in once for every machine: sign in on one with Keep signed in"),
        (Some(cmd), None) => Ok(cmd),
        (None, _) => bail!("Hub doesn't know how {engine} signs in; open its CLI there"),
    }
}

/// Starts signing `engine` in here. A sign-in already waiting is shown again, not started anew.
pub fn start(hub: Arc<Hub>, engine: &str) -> Result<LoginJob> {
    let cmd = command(&hub, engine)?;
    {
        let mut jobs = JOBS.locked();
        if let Some(j) = jobs.get(engine).filter(|j| j.running) {
            return Ok(j.clone());
        }
        jobs.insert(
            engine.to_owned(),
            LoginJob { engine: engine.to_owned(), running: true, message: "Asking for a sign-in link…".into(), ..Default::default() },
        );
    }
    let name = engine.to_owned();
    let install = crate::engines::install_command(&hub.config, engine, &cmd);
    tokio::spawn(async move {
        let sink_name = name.clone();
        let sink = move |line: String| update(&sink_name, |j| take_line(j, line));
        let program = crate::project::split_command(&cmd).first().cloned().unwrap_or_default();
        // The CLI that signs in may be missing where only the engine's ACP adapter was set up.
        if crate::proc::find_program(&program).is_none() {
            let Some(install) = install else {
                finish(&name, false, format!("{program} is not installed on this machine, and Hub doesn't know how to install it."));
                return;
            };
            update(&name, |j| j.message = format!("Installing {program} first…"));
            match tokio::time::timeout(INSTALL_WAIT, crate::engines::run_shell(&crate::engines::with_path(&install), sink.clone())).await {
                Ok(Ok(())) if crate::proc::find_program(&program).is_some() => {}
                Ok(Ok(())) => return finish(&name, false, format!("{install} ran, but {program} is still not found.")),
                Ok(Err(e)) => return finish(&name, false, format!("Installing {program} failed ({install}): {e:#}")),
                Err(_) => return finish(&name, false, format!("Installing {program} took over {} minutes and was stopped.", INSTALL_WAIT.as_secs() / 60)),
            }
            update(&name, |j| j.message = "Asking for a sign-in link…".into());
        }
        let ran = tokio::time::timeout(WAIT, crate::engines::run_shell(&crate::engines::with_path(&cmd), sink)).await;
        let (ok, message) = match ran {
            Ok(Ok(())) => (true, "Signed in.".to_owned()),
            Ok(Err(e)) if format!("{e:#}").contains("127") => (false, format!("{program} is not found on this machine's path.")),
            Ok(Err(e)) => (false, format!("The sign-in did not finish: {e:#}")),
            Err(_) => (false, format!("Nobody typed the code within {} minutes; start again for a new one.", WAIT.as_secs() / 60)),
        };
        if ok && let Err(e) = hub.signed_in(&name, None).await {
            tracing::warn!(engine = %name, "after signing in: {e:#}");
        }
        finish(&name, ok, message);
    });
    Ok(job(engine).unwrap_or_default())
}

fn finish(engine: &str, ok: bool, message: String) {
    update(engine, |j| {
        j.running = false;
        j.ok = Some(ok);
        j.message = message;
    });
}

fn update(engine: &str, f: impl FnOnce(&mut LoginJob)) {
    if let Some(j) = JOBS.locked().get_mut(engine) {
        f(j);
    }
}

/// Keeps a line and reads the link and the code from it.
fn take_line(job: &mut LoginJob, line: String) {
    if job.url.is_none() {
        job.url = url_in(&line);
    }
    if job.code.is_none() {
        job.code = code_in(&line);
    }
    if job.url.is_some() && job.code.is_some() && job.ok.is_none() {
        job.message = "Open the link and type the code.".into();
    }
    job.lines.push(line);
    let extra = job.lines.len().saturating_sub(KEEP_LINES);
    job.lines.drain(..extra);
}

fn url_in(line: &str) -> Option<String> {
    static URL: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"https://[^\s<>\x22']+").expect("constant regex"));
    URL.find(line).map(|m| m.as_str().trim_end_matches(['.', ',', ')']).to_owned())
}

/// A one-time code as engines print them: groups of capitals and digits joined by a dash.
fn code_in(line: &str) -> Option<String> {
    static CODE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"\b[A-Z0-9]{4,5}-[A-Z0-9]{4,5}\b").expect("constant regex"));
    let without_url = url_in(line).map(|u| line.replace(&u, "")).unwrap_or_else(|| line.to_owned());
    CODE.find(&without_url).map(|m| m.as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_and_code_are_read_from_what_the_engine_prints() {
        let mut job = LoginJob::default();
        for line in [
            "Welcome to Codex",
            "1. Open this link in your browser and sign in to your account",
            "   https://auth.openai.com/codex/device",
            "2. Enter this one-time code (expires in 15 minutes)",
            "   K7QD-9XWPM",
        ] {
            take_line(&mut job, line.to_owned());
        }
        assert_eq!(job.url.as_deref(), Some("https://auth.openai.com/codex/device"));
        assert_eq!(job.code.as_deref(), Some("K7QD-9XWPM"));
        assert_eq!(job.message, "Open the link and type the code.");
        // A dashed word in a link is not a code.
        assert_eq!(code_in("see https://x.io/ABCD-EFGH now"), None);
    }
}
