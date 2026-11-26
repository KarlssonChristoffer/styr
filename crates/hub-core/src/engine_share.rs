//! Keeping the engines the same on every machine: sends this machine's engines and their keys
//! to the others (`share`), takes in what they send (`receive`) and tries each engine that came
//! from elsewhere (`check_shared`). The `Hub` half; the data it moves is in `engines/share.rs`, so
//! the engine catalog itself knows nothing of `Hub`.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{Config, EngineConfig};
use crate::engines::{
    CHECK_AGAIN, CHECK_AT_MOST, CHECKING, CHECKS, Checking, EngineCheck, EngineHere, MACHINES, MachineEngines, SHARE_NOW,
    SharedEngine, comes_back, harness_program, harnesses, here, install_once, installed, saved_config, take_in, tried_here,
};
use crate::hub::Hub;
use crate::locked::LockExt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shipment {
    /// The machine sending, by its name.
    pub from: String,
    pub engines: Vec<SharedEngine>,
    /// Lasting sign-ins made there (`signin.rs`), so one sign-in does every machine.
    #[serde(default)]
    pub sign_ins: Vec<crate::signin::SharedSignIn>,
    /// Who git commits as there, for a machine where git knows no one yet.
    #[serde(default)]
    pub git: Option<crate::model::GitIdentity>,
    /// Harness settings changed from Hub there (`harness_settings.rs`); an older hubd sends none.
    #[serde(default)]
    pub settings: crate::harness_settings::Wanted,
    /// How every agent starts (`agent_prefs.rs`), the newest set on any machine.
    #[serde(default)]
    pub agent_prefs: Option<crate::agent_prefs::Shared>,
    /// Who does which kind of task (`handoff.rs`), the newest set on any machine.
    #[serde(default)]
    pub handoff: Option<crate::handoff::Shared>,
}

/// Tries every engine another machine sees here, as a thread here would start it: on the
/// provider a new thread picks, with this machine's keys. Spends nothing: the agent is asked for
/// a session without a prompt and the provider for its models. Every try ends in a check, ok or
/// not, within `CHECK_AT_MOST`.
pub async fn check_shared(hub: std::sync::Arc<Hub>) {
    let Ok(Ok(saved)) = tokio::task::spawn_blocking(saved_config).await else { return };
    for (name, e) in saved.engines {
        if e.shared_from.is_some() && !installed(&e.command) {
            install_once(e.harness.as_deref().unwrap_or(&name));
        }
        if !tried_here(&name) || !installed(&e.command) {
            continue;
        }
        if !CHECKING.locked().insert(name.clone()) {
            continue;
        }
        let _checking = Checking(name.clone());
        let tried = tokio::time::timeout(CHECK_AT_MOST, check_one(&hub, &name, &e)).await;
        let (print, check) = match tried {
            Ok(None) => continue,
            Ok(Some(done)) => done,
            Err(_) => (
                String::new(),
                EngineCheck {
                    ok: false,
                    message: format!("trying it took over {} minutes; it tries again in a minute", CHECK_AT_MOST.as_secs() / 60),
                    route: None,
                    at: crate::model::now_ms(),
                },
            ),
        };
        CHECKS.locked().insert(name.clone(), (print, check));
    }
}

/// Tries one engine, unless it was tried as it is now within `CHECK_AGAIN`.
async fn check_one(hub: &Hub, name: &str, e: &EngineConfig) -> Option<(String, EngineCheck)> {
    let vault = hub.secrets.list().await.unwrap_or_default();
    let live = crate::routes::live_models(&hub.secrets, name, e, &vault).await;
    let route = crate::routes::option(name, e, &crate::routes::default_route(e), &vault, &live)
        .and_then(|o| o["currentValue"].as_str().map(str::to_owned));
    let applied = match &route {
        Some(r) => crate::routes::apply(name, e, r, &vault),
        None => e.clone(),
    };
    let print = format!("{}|{}", serde_json::to_string(&applied).unwrap_or_default(), route.clone().unwrap_or_default());
    let fresh = CHECKS
        .locked()
        .get(name)
        .is_some_and(|(p, c)| *p == print && crate::model::now_ms() - c.at < CHECK_AGAIN.as_millis() as i64);
    if fresh {
        return None;
    }
    let secrets = hub.secrets.engine_env(&applied).await.unwrap_or_default();
    let t = crate::engine_probe::test(name, &applied, secrets).await;
    let on = route
        .as_deref()
        .filter(|r| *r != crate::routes::OWN)
        .map(|r| format!(" on {}", r.split_once(':').map(|(_, m)| m).unwrap_or(r)))
        .unwrap_or_default();
    let message = if t.ok { format!("tried{on}: works") } else { t.message.trim_end_matches('.').to_owned() };
    Some((print, EngineCheck { ok: t.ok, message, route, at: crate::model::now_ms() }))
}

/// The engines this machine sends: the ones added here, with their keys.
pub async fn outgoing(hub: &Hub) -> Result<Vec<SharedEngine>> {
    let saved = tokio::task::spawn_blocking(saved_config).await??;
    let builtins = Config::default().engines;
    let vault = hub.secrets.list().await?;
    let wanted = |name: &String, e: &EngineConfig| -> Vec<String> {
        let mut names = e.key_names();
        names.extend(crate::routes::key_names(name, e, &vault));
        names
    };
    let values = hub.secrets.env(&saved.engines.iter().flat_map(|(n, e)| wanted(n, e)).collect::<Vec<_>>()).await?;
    let values: BTreeMap<String, String> = values.into_iter().collect();
    Ok(saved
        .engines
        .into_iter()
        .filter(|(name, e)| !builtins.contains_key(name) && name != crate::sim::ENGINE && (e.shared_from.is_none() || hub.config.relay_keys))
        .map(|(name, mut config)| {
            // An engine's own subscription login is never copied; only keys added by hand go (never
            // `ENGINE_KEYS`). The year-long sign-in Hub keeps travels separately (`signin.rs`). A machine that
            // relays (`Config::relay_keys`) hands on the keys it was lent too.
            let keys = wanted(&name, &config)
                .into_iter()
                .filter(|k| !crate::config::ENGINE_KEYS.contains(&k.as_str()))
                .filter_map(|k| Some((k.clone(), values.get(&k)?.clone())))
                .collect();
            // The other machine finds its own way to start a harness it knows.
            if let Some(h) = config.harness.as_deref().and_then(|h| harnesses().into_iter().find(|x| x.id == h)) {
                config.command = h.command;
            }
            SharedEngine { name, config, keys }
        })
        .collect())
}

/// Takes in the engines another machine sent: saves their keys and settings, removes the ones it
/// no longer has, and restarts the core when anything changed. Answers how each engine stands here.
pub async fn receive(hub: &std::sync::Arc<Hub>, shipment: Shipment) -> Result<Vec<EngineHere>> {
    let from = shipment.from.trim().to_owned();
    if from.is_empty() {
        bail!("say which machine the engines are from");
    }
    for engine in &shipment.engines {
        // A key handed on by another machine comes back to the one that made it: this machine
        // keeps its own, which may be newer.
        if comes_back(engine, hub.remotes.local_name()) {
            continue;
        }
        for (name, value) in &engine.keys {
            if crate::config::ENGINE_KEYS.contains(&name.as_str()) {
                continue;
            }
            let have = hub.secrets.env(std::slice::from_ref(name)).await?.pop().map(|(_, v)| v);
            if have.as_deref() != Some(value.as_str()) {
                hub.secrets.set(name, value).await?;
            }
        }
    }
    if let Err(e) = hub.take_sign_ins(&from, shipment.sign_ins).await {
        tracing::warn!(%from, "engines: taking the sign-ins: {e:#}");
    }
    if let Some(id) = shipment.git {
        // Only where git knows no one: a name set on this machine stays.
        let set = tokio::task::spawn_blocking(move || match crate::git_identity::identity() {
            None => crate::git_identity::set_identity(&id).map(|()| true),
            Some(_) => Ok(false),
        })
        .await?;
        match set {
            Ok(true) => tracing::info!(%from, "git commits as on {from} here too"),
            Ok(false) => {}
            Err(e) => tracing::warn!(%from, "engines: setting who git commits as: {e:#}"),
        }
    }
    if let Some(prefs) = shipment.agent_prefs
        && let Err(e) = crate::agent_prefs::take_in(&hub.ctx(), prefs).await
    {
        tracing::warn!(%from, "engines: taking how agents start: {e:#}");
    }
    if let Some(shared) = shipment.handoff
        && let Err(e) = crate::handoff::take_in(&hub.ctx(), shared).await
    {
        tracing::warn!(%from, "engines: taking who does which task: {e:#}");
    }
    if !shipment.settings.is_empty() {
        // Set in the background: each one runs the harness's own program.
        tokio::spawn(crate::harness_settings::take_in(harness_program, shipment.settings));
    }
    let running = hub.config.clone();
    let changed = tokio::task::spawn_blocking(move || take_in(&from, shipment.engines)).await??;
    if changed {
        crate::prefs::request_restart();
    } else {
        // Tried in the background; the answer to the next send says how it went.
        tokio::spawn(check_shared(hub.clone()));
    }
    tokio::task::spawn_blocking(move || here(&running)).await?
}

/// Every machine Hub knows besides this one, with what it last answered.
pub fn machines(hub: &Hub) -> Vec<MachineEngines> {
    let known = MACHINES.locked().clone();
    hub.remotes
        .list()
        .into_iter()
        .filter(|h| !h.local)
        .map(|h| {
            let mut m = known.get(&h.name).cloned().unwrap_or(MachineEngines {
                machine: h.name.clone(),
                engines: Vec::new(),
                error: None,
                at: 0,
            });
            if h.state != crate::model::HostState::Online {
                m.error = Some("offline; it gets the engines when it is back".into());
            }
            m
        })
        .collect()
}

/// Sends the engines to every machine online now and keeps what each answers.
pub async fn share_once(hub: &Hub) {
    let engines = match outgoing(hub).await {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("engines: reading the engines to share: {e:#}");
            return;
        }
    };
    let sign_ins = hub.outgoing_sign_ins().await;
    let git = tokio::task::spawn_blocking(crate::git_identity::identity).await.ok().flatten();
    let settings = crate::harness_settings::wanted(&hub.store).await;
    let agent_prefs = crate::agent_prefs::outgoing(&hub.ctx()).await;
    let handoff = crate::handoff::outgoing(&hub.ctx()).await;
    let shipment = Shipment { from: hub.remotes.local_name().to_owned(), engines, sign_ins, git, settings, agent_prefs, handoff };
    let Ok(body) = serde_json::to_value(&shipment) else { return };
    for (machine, answer) in hub.remotes.exchange::<Vec<EngineHere>>("/api/engines/receive", &body).await {
        let entry = match answer {
            Ok(engines) => MachineEngines { machine: machine.clone(), engines, error: None, at: crate::model::now_ms() },
            Err(e) => {
                tracing::debug!(%machine, "engines: sharing: {e:#}");
                let error = if crate::remote::too_old(&e) {
                    hub.remotes.wake_push();
                    "its hubd is too old for shared engines; Hub is updating it (Settings > Updates)".to_owned()
                } else {
                    format!("{e:#}")
                };
                MachineEngines { machine: machine.clone(), engines: Vec::new(), error: Some(error), at: crate::model::now_ms() }
            }
        };
        MACHINES.locked().insert(machine, entry);
    }
}

/// Keeps the engines the same on every machine: a minute apart, or at once after a change here.
pub async fn share(hub: std::sync::Arc<Hub>) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(60)) => {}
            _ = SHARE_NOW.notified() => {}
        }
        share_once(&hub).await;
    }
}
