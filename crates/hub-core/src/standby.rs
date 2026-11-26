//! Background services a harness wants up before a thread needs them.
//!
//! Some harnesses are a front for a service of their own: Hermes's ACP agent (`hermes acp`) is a
//! viewer of the Hermes gateway, and when no gateway is running it starts one and waits for it.
//! On Christoffer's PC that start was 10 to 20 of the 15 to 24 seconds before a Hermes thread
//! answered (measured 2026-11-01 from `hub.db` and Hermes's own logs); with the gateway already up
//! the same start took 3 to 6 seconds. A gateway an ACP agent starts ends itself after ten idle
//! minutes, so most new threads paid for it again.
//!
//! So the core asks each such harness to have its service up, the way the harness's own desktop
//! app does (`hermes gateway ensure`): once shortly after the core starts and every few minutes
//! after, which also brings it back when something ended it. A harness says what to run in its
//! catalog entry (`Harness::standby`); nothing here knows one harness from another. It runs only
//! for harnesses an engine on this machine uses, with the keys those engines get, and not while
//! every agent is paused (`halt.rs`). The service keeps the keys it started with, so when one of
//! them changes in Hub's vault the harness is asked to start it again instead
//! (`Harness::standby_again`, Hermes's `gateway restart --detach`, which lets running turns end).
//! <!-- see docs/ARCHITECTURE.md "Fast first answers" -->

use crate::locked::ResultExt;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::EngineConfig;
use crate::engines::Harness;
use crate::hub::Hub;

/// How often each service is asked for again.
pub const EVERY: Duration = Duration::from_secs(5 * 60);
/// Longest a single ask may take; the harness's own deadline is shorter.
const AT_MOST: Duration = Duration::from_secs(150);

/// What to run for one harness here.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub harness: String,
    pub label: String,
    /// The program as the engines start it (a full path when an engine names one), else the
    /// harness's program name to look up.
    pub program: String,
    pub args: Vec<String>,
    /// What to run instead when the keys changed since the last ask; `args` when none.
    pub again: Vec<String>,
    /// The engines on this harness, by name, whose keys the service gets.
    pub engines: Vec<String>,
    /// The harness's own key file, under the home folder (`Harness::key_file`).
    pub key_file: Option<String>,
}

/// One plan per harness that has a standby and is used by an engine in `engines`.
pub fn plans(engines: &BTreeMap<String, EngineConfig>, catalog: &[Harness]) -> Vec<Plan> {
    let mut out: Vec<Plan> = Vec::new();
    for (name, engine) in engines {
        let id = engine.harness.as_deref().unwrap_or(name);
        let Some(h) = catalog.iter().find(|h| h.id == id) else { continue };
        let Some(standby) = h.standby.as_deref() else { continue };
        let first = crate::project::split_command(&engine.command).into_iter().next().unwrap_or_default();
        if let Some(plan) = out.iter_mut().find(|p| p.harness == h.id) {
            plan.engines.push(name.clone());
            // An engine that names the program's full path says best where it is.
            if is_path(&first) && !is_path(&plan.program) {
                plan.program = first;
            }
            continue;
        }
        let program = if is_path(&first) { first } else { h.cli.clone().unwrap_or(first) };
        if program.is_empty() {
            continue;
        }
        out.push(Plan {
            harness: h.id.clone(),
            label: h.label.clone(),
            program,
            args: standby.split_whitespace().map(str::to_owned).collect(),
            again: h.standby_again.as_deref().unwrap_or(standby).split_whitespace().map(str::to_owned).collect(),
            engines: vec![name.clone()],
            key_file: h.key_file.clone(),
        });
    }
    out
}

/// A program given by its path rather than a name to look up.
fn is_path(program: &str) -> bool {
    program.contains('/') || program.contains('\\')
}

/// Keeps the services up while the core runs.
pub async fn keep(hub: Arc<Hub>) {
    // After the core is listening and has taken its engines over.
    tokio::time::sleep(Duration::from_secs(5)).await;
    let mut said: BTreeMap<String, String> = BTreeMap::new();
    // The keys each service was last asked with.
    let mut started_on: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut keys_changed = hub.secrets.changes();
    loop {
        if crate::halt::current(&hub).is_none() {
            for plan in plans(&hub.config.engines, &crate::engines::harnesses()) {
                let outcome = match ask(&hub, &plan, &mut started_on).await {
                    Ok(Some(state)) => format!("{} is {state}", plan.label),
                    Ok(None) => continue, // not installed here
                    Err(e) => format!("could not have {}'s service up: {e:#}", plan.label),
                };
                // Said once, and again only when it changes.
                if said.get(&plan.harness) != Some(&outcome) {
                    tracing::info!("standby: {outcome}");
                    said.insert(plan.harness.clone(), outcome);
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(EVERY) => {}
            _ = keys_changed.changed() => {}
        }
    }
}

/// Asks the harness to have its service up; the state it reports, or None when its program is
/// not on this machine.
async fn ask(hub: &Hub, plan: &Plan, started_on: &mut BTreeMap<String, Vec<(String, String)>>) -> Result<Option<String>> {
    let given = plan.program.clone();
    let program = tokio::task::spawn_blocking(move || {
        let path = std::path::PathBuf::from(&given);
        if is_path(&given) { path.is_file().then_some(path) } else { crate::engines::locate(&given) }
    })
    .await?;
    let Some(program) = program else { return Ok(None) };

    // The keys the engines would start with, so the service the harness starts has them too.
    let mut keys: Vec<(String, String)> = Vec::new();
    for name in &plan.engines {
        if let Some(engine) = hub.config.engines.get(name) {
            for kv in hub.secrets.engine_env(engine).await.unwrap_or_default() {
                if !keys.iter().any(|(k, _)| k == &kv.0) {
                    keys.push(kv);
                }
            }
        }
    }
    // A key file of its own would win over them with an older key, as when an engine starts.
    if let (Some(file), Some(home)) = (plan.key_file.clone(), crate::folders::home_dir())
        && !keys.is_empty()
    {
        let given = keys.clone();
        tokio::task::spawn_blocking(move || crate::engines::clear_stale_keys(&home.join(file), &given)).await.warn_on_err("clear stale keys");
    }

    keys.sort();
    // The first ask after the core started may find a service an engine started with keys Hub
    // no longer has; only a change seen here starts it again.
    let args = match started_on.get(&plan.harness) {
        Some(before) if before != &keys => &plan.again,
        _ => &plan.args,
    };
    let out = tokio::time::timeout(AT_MOST, run(&program, args, &keys)).await.context("it did not answer")??;
    started_on.insert(plan.harness.clone(), keys);
    Ok(Some(state_of(&out)))
}

async fn run(program: &std::path::Path, args: &[String], keys: &[(String, String)]) -> Result<String> {
    let make = || {
        let mut cmd = crate::proc::tokio_command(program);
        cmd.args(args)
            .envs(keys.iter().cloned())
            .env("NO_COLOR", "1")
            .env_remove(crate::process_marks::THREAD_VAR)
            .env_remove(crate::process_marks::PROJECT_VAR)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        cmd
    };
    let mut cmd = make();
    // Outside any job the core runs in, when that job allows it: the service outlives this ask
    // and should not end with the core.
    #[cfg(windows)]
    let output = {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        let child = match cmd.spawn() {
            Ok(child) => child,
            Err(_) => {
                let mut cmd = make();
                cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
                cmd.spawn()?
            }
        };
        child.wait_with_output().await?
    };
    #[cfg(not(windows))]
    let output = {
        cmd.process_group(0);
        cmd.output().await?
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !output.status.success() && stdout.trim().is_empty() {
        anyhow::bail!("it ended with {}", output.status);
    }
    Ok(stdout)
}

/// What a harness said about its service: the `state` of a JSON answer (Hermes's `ensure`), or
/// its last line.
fn state_of(out: &str) -> String {
    let last = out.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("");
    match serde_json::from_str::<serde_json::Value>(last) {
        Ok(v) => match (v["state"].as_str(), v["reason_code"].as_str()) {
            (Some(state), Some(reason)) => format!("{state} ({reason})"),
            (Some(state), None) => state.to_owned(),
            _ => "asked".to_owned(),
        },
        Err(_) if last.is_empty() => "asked".to_owned(),
        Err(_) => last.chars().take(200).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(command: &str, harness: Option<&str>) -> EngineConfig {
        serde_json::from_value(serde_json::json!({ "command": command, "harness": harness })).unwrap()
    }

    #[test]
    fn only_harnesses_with_a_standby_that_an_engine_uses_get_one() {
        let mut engines = BTreeMap::new();
        engines.insert("claude".to_owned(), engine("npx -y @agentclientprotocol/claude-agent-acp", Some("claude")));
        engines.insert("hermes-deepseek".to_owned(), engine(r"C:\Users\me\AppData\Local\hermes\bin\hermes.exe acp", Some("hermes")));
        engines.insert("hermes-kimi".to_owned(), engine("hermes acp", Some("hermes")));
        let plans = plans(&engines, &crate::engines::harnesses());
        assert_eq!(plans.len(), 1);
        let p = &plans[0];
        assert_eq!(p.harness, "hermes");
        assert_eq!(p.program, r"C:\Users\me\AppData\Local\hermes\bin\hermes.exe");
        assert_eq!(p.args, ["gateway", "ensure", "--timeout", "120"]);
        assert_eq!(p.again, ["gateway", "restart", "--detach"]);
        assert_eq!(p.engines, ["hermes-deepseek", "hermes-kimi"]);
        assert_eq!(p.key_file.as_deref(), Some(".hermes/.env"));
    }

    #[test]
    fn a_bare_program_name_is_looked_up_by_the_harness_cli() {
        let mut engines = BTreeMap::new();
        engines.insert("hermes".to_owned(), engine("hermes acp", None));
        let found = plans(&engines, &crate::engines::harnesses());
        assert_eq!(found[0].program, "hermes");
        assert!(plans(&BTreeMap::new(), &crate::engines::harnesses()).is_empty());
    }

    #[test]
    fn the_state_is_read_from_the_answer() {
        assert_eq!(state_of("{\"endpoint\":{},\"state\":\"ready\"}\n"), "ready");
        assert_eq!(state_of("noise\n{\"state\":\"absent\",\"reason_code\":\"deadline\"}"), "absent (deadline)");
        assert_eq!(state_of(""), "asked");
        assert_eq!(state_of("Started"), "Started");
    }
}
