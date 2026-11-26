//! The panic button: one click pauses every agent on every machine, and one click resumes them.
//!
//! Pausing stops every engine the way a core that hands over does (`Hub::stop_engines`): the turn
//! is cut off without a `turn_end`, so the log still says where it was. While paused no engine
//! starts here. Messages, from the user, a routine, a side job or another agent, are logged and
//! wait; scheduled routines hold back. The state is kept in the store, so a restart while paused
//! stays paused. Resuming carries each cut-off turn on and sends what waited, with the same code
//! that carries turns on after a restart (`restart::carry_on`, `by: "pause"`).
//!
//! The machine where it is pressed asks every other online machine to do the same, and your
//! tools whose `hub.toml` has an action with `panic = "pause"` (or `"resume"`) get it run. A
//! machine that is offline is named in the answer, since it was not paused.

use crate::locked::{LockExt, ResultExt};
use std::sync::Arc;

use anyhow::Result;

use crate::hub::Hub;
use crate::model::{Halt, HaltReach, HaltRequest, PausedData, Resumed, StreamMsg, now_ms};

/// Where the state is kept while paused.
const KEY: &str = "halt";

/// Reads a pause that was in force when the core stopped. Run before anything starts an engine.
pub async fn load(hub: &Hub) -> Result<()> {
    if let Some(json) = hub.store.get_value(KEY).await? {
        let halt: Halt = serde_json::from_str(&json)?;
        tracing::info!("every agent is paused since {} (from {})", halt.since, halt.by);
        *hub.halt.locked() = Some(halt);
    }
    Ok(())
}

/// The pause in force here, if any.
pub fn current(hub: &Hub) -> Option<Halt> {
    hub.halt.locked().clone()
}

/// Pauses every agent here and, unless `ask.here`, on every other machine.
pub async fn pause(hub: &Arc<Hub>, ask: HaltRequest) -> Result<Halt> {
    let by = ask.by.unwrap_or_else(|| hub.remotes.local_name().to_owned());
    // Held first, so nothing starts while the engines stop.
    let mut halt = current(hub).unwrap_or(Halt { since: now_ms(), by: by.clone(), threads: Vec::new(), hosts: Vec::new(), tools: Vec::new() });
    set(hub, Some(halt.clone())).await?;

    for id in hub.stop_engines().await {
        hub.emit_typed(&id, &PausedData { by: by.clone() }).await.warn_on_err("emit Paused");
        if !halt.threads.contains(&id) {
            halt.threads.push(id);
        }
    }
    halt.tools = tools(hub, "pause").await;
    if !ask.here {
        let n = |a: &Halt| a.threads.len();
        halt.hosts = others(hub, "/api/halt/pause", &by, |a: &Halt| (format!("{} {} stopped", n(a), turns(n(a))), 0)).await.0;
    }
    set(hub, Some(halt.clone())).await?;
    tracing::warn!("paused every agent ({} {} cut off)", halt.threads.len(), turns(halt.threads.len()));
    Ok(halt)
}

/// Lets agents run again here and, unless `ask.here`, everywhere else, carrying on what the
/// pause cut off.
pub async fn resume(hub: &Arc<Hub>, ask: HaltRequest) -> Result<Resumed> {
    set(hub, None).await?;
    let mut threads = crate::restart::carry_on(hub, "pause").await? as u32;
    let tools = tools(hub, "resume").await;
    let mut hosts = Vec::new();
    if !ask.here {
        let by = hub.remotes.local_name().to_owned();
        let (reached, carried) = others(hub, "/api/halt/resume", &by, |r: &Resumed| (format!("{} carried on", r.threads), r.threads)).await;
        hosts = reached;
        threads += carried;
    }
    tracing::info!("resumed every agent ({threads} carried on)");
    Ok(Resumed { threads, hosts, tools })
}

async fn set(hub: &Hub, halt: Option<Halt>) -> Result<()> {
    match &halt {
        Some(h) => hub.store.set_value(KEY, &serde_json::to_string(h)?).await?,
        None => {
            hub.store.take_value(KEY).await?;
        }
    }
    *hub.halt.locked() = halt.clone();
    hub.publish(StreamMsg::Halt { halt });
    Ok(())
}

/// Asks every other machine to do the same, and says how each one answered (`said` gives the
/// line and a count to add up). A machine that is not online is named too, as not reached.
async fn others<T: serde::de::DeserializeOwned>(hub: &Hub, path: &str, by: &str, said: impl Fn(&T) -> (String, u32)) -> (Vec<HaltReach>, u32) {
    let body = serde_json::to_value(HaltRequest { here: true, by: Some(by.to_owned()) }).unwrap_or_default();
    let mut total = 0;
    let mut out = Vec::new();
    for (name, answer) in hub.remotes.exchange::<T>(path, &body).await {
        out.push(match answer {
            Ok(a) => {
                let (line, n) = said(&a);
                total += n;
                HaltReach { name, ok: true, detail: Some(line) }
            }
            Err(e) => HaltReach { name, ok: false, detail: Some(format!("{e:#}")) },
        });
    }
    for host in hub.remotes.list() {
        if !host.local && !out.iter().any(|r| r.name == host.name) {
            out.push(HaltReach { name: host.name, ok: false, detail: Some("offline, not reached".into()) });
        }
    }
    (out, total)
}

/// Runs the action marked `panic = <which>` of each of your tools on this machine.
async fn tools(hub: &Hub, which: &str) -> Vec<HaltReach> {
    let mut out = Vec::new();
    for folder in crate::own_tools::folders(&hub.ctx()).await {
        let Some((name, action)) = crate::own_tools::panic_action(&folder, which) else { continue };
        let ask = crate::model::RunOwnToolAction { folder: folder.display().to_string(), action, item: None, inputs: Default::default() };
        let reach = match crate::own_tools::act(&folder, &ask, true).await {
            Ok(ran) => HaltReach { name, ok: ran.ok, detail: (!ran.output.is_empty()).then(|| ran.output.lines().last().unwrap_or_default().to_owned()) },
            Err(e) => HaltReach { name, ok: false, detail: Some(format!("{e:#}")) },
        };
        out.push(reach);
    }
    out
}

fn turns(n: usize) -> &'static str {
    if n == 1 { "turn" } else { "turns" }
}
