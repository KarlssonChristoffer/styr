//! A harness on a provider, picked per thread.
//!
//! An engine that runs a harness able to use other providers (Hermes, OpenCode) is not tied to
//! one of them. Its threads pick a provider and model in the model picker, from the providers
//! the harness runs on and the vault has a key for. Hub offers that pick as one more model
//! setting, `hub_route`, grouped by provider, and starts the agent with the provider's
//! environment and key. Picking another one restarts the agent on it and loads the same session.
//! The catalog in `engines.rs` says which harness runs on what; nothing here names one.

use crate::locked::LockExt;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::config::EngineConfig;
use crate::engines::{Harness, Provider, harness, matching_keys, providers};
use crate::model::SecretInfo;

/// The setting a thread's provider and model are kept under.
pub const ROUTE: &str = "hub_route";
/// The harness on its own login and settings, with nothing from Hub.
pub const OWN: &str = "own";

/// Whether the agent is also told its model over ACP when Hub picks the route, besides what
/// its environment says.
pub fn sets_model(name: &str, engine: &EngineConfig) -> bool {
    harness(name, engine).is_none_or(|h| h.set_model)
}

/// The model as the provider names it, without the harness's provider prefix.
fn plain_model(provider: &str, model: &str) -> String {
    model
        .strip_prefix(&format!("{provider}:"))
        .or_else(|| model.strip_prefix(&format!("{provider}/")))
        .unwrap_or(model)
        .to_owned()
}

/// The route a thread starts on when it picked none: the engine's provider and model, or the
/// harness's own setup.
pub fn default_route(engine: &EngineConfig) -> String {
    match (&engine.provider, &engine.model) {
        (Some(p), Some(m)) => format!("{p}:{}", plain_model(p, m)),
        (Some(p), None) => providers().into_iter().find(|x| &x.id == p).and_then(|x| x.models.first().map(|m| format!("{p}:{m}"))).unwrap_or_else(|| OWN.into()),
        _ => OWN.into(),
    }
}

fn split(route: &str) -> Option<(&str, &str)> {
    route.split_once(':').filter(|(p, m)| !p.is_empty() && !m.is_empty())
}

/// The providers a harness can run on here: the ones it knows that the vault has a key for.
pub fn usable(h: &Harness, vault: &[SecretInfo]) -> Vec<(Provider, String)> {
    providers()
        .into_iter()
        .filter(|p| h.runs_on(&p.id))
        .filter_map(|p| {
            let key = matching_keys(&p, vault).into_iter().next()?.name;
            Some((p, key))
        })
        .collect()
}

/// The engine as it starts on `route`: the harness's environment for that provider and model,
/// and the provider's key under the name the harness reads.
pub fn apply(name: &str, engine: &EngineConfig, route: &str, vault: &[SecretInfo]) -> EngineConfig {
    let Some(h) = harness(name, engine) else { return engine.clone() };
    let mut out = engine.clone();
    for k in h.env.keys() {
        out.env.remove(k);
    }
    let mut known: Vec<String> = providers().into_iter().map(|p| p.secret).collect();
    known.extend(h.key_env.clone());
    out.secrets.retain(|s| !known.contains(s));
    out.keys.retain(|var, _| !known.contains(var));
    let Some((pid, model)) = split(route) else {
        out.provider = None;
        out.model = None;
        return out;
    };
    let Some(p) = providers().into_iter().find(|p| p.id == pid) else { return engine.clone() };
    let full = h.model_format.replace("{provider}", pid).replace("{model}", model);
    let url = p.anthropic_url.as_deref().unwrap_or("");
    for (k, v) in &h.env {
        out.env.insert(
            k.clone(),
            v.replace("{provider}", pid).replace("{model}", model).replace("{full}", &full).replace("{url}", url),
        );
    }
    let var = h.key_env.clone().unwrap_or_else(|| p.secret.clone());
    match matching_keys(&p, vault).into_iter().next() {
        Some(k) if k.name == var => out.secrets.push(var),
        Some(k) => {
            out.keys.insert(var, k.name);
        }
        // No key here yet: the harness says so when it starts.
        None => out.secrets.push(p.secret.clone()),
    }
    out.provider = Some(pid.to_owned());
    out.model = Some(full);
    out
}

/// The model setting Hub offers for an engine with providers, as an ACP config option: its own
/// setup first, then each provider's models in a group of their own.
/// Models a provider's API listed, by provider id; see [`live_models`].
pub type Live = BTreeMap<String, Vec<String>>;

/// The models to offer for `p`: what its API lists when that is a short list, else the catalog's
/// that it still lists (OpenRouter lists hundreds), else the catalog's.
fn models_for(p: &Provider, live: &Live) -> Vec<String> {
    match live.get(&p.id) {
        Some(l) if !l.is_empty() && l.len() <= 30 => l.clone(),
        Some(l) if !l.is_empty() => {
            let kept: Vec<String> = p.models.iter().filter(|m| l.contains(m)).cloned().collect();
            if kept.is_empty() { p.models.clone() } else { kept }
        }
        _ => p.models.clone(),
    }
}

pub fn option(name: &str, engine: &EngineConfig, current: &str, vault: &[SecretInfo], live: &Live) -> Option<Value> {
    let h = harness(name, engine)?;
    let mut groups = Vec::new();
    let mut listed = Vec::new();
    // The harness's own setup is what it was set up to use outside Hub, which Hub cannot see
    // and which often is nothing (Hermes then sends its default model to whatever provider it
    // finds). It is offered only when no provider with a key is there to pick instead.
    let usable_now = usable(&h, vault);
    // A harness that only runs on a provider as an engine of its own has no setup of its own to
    // fall back on there: that would be its subscription, under an engine that says DeepSeek.
    if h.own_login && !h.providers.is_empty() && usable_now.is_empty() {
        listed.push(OWN.to_owned());
        groups.push(json!({
            "group": OWN,
            "name": format!("{}'s own setup", h.label),
            "options": [{ "value": OWN, "name": "Own setup", "description": format!("What {} is set up to use on this machine", h.label) }],
        }));
    }
    let mut usable = usable_now;
    // A provider picked before its key left the vault still shows, so the pick reads right.
    if let Some((pid, _)) = split(current)
        && !usable.iter().any(|(p, _)| p.id == pid)
        && let Some(p) = providers().into_iter().find(|p| p.id == pid && h.runs_on(&p.id))
    {
        usable.push((p, String::new()));
    }
    for (p, _) in usable {
        let mut models = models_for(&p, live);
        // A model of the provider's that the list lacks stays when nothing says it is gone; one
        // its API no longer lists (an old name) gives way to the provider's first.
        if let Some((pid, m)) = split(current)
            && pid == p.id
            && !models.iter().any(|x| x == m)
            && !live.get(&p.id).is_some_and(|l| l.len() <= 30 && !l.is_empty())
        {
            models.push(m.to_owned());
        }
        let options: Vec<Value> = models
            .iter()
            .map(|m| {
                let value = format!("{}:{m}", p.id);
                listed.push(value.clone());
                json!({ "value": value, "name": m, "description": p.label })
            })
            .collect();
        groups.push(json!({ "group": p.id, "name": p.label, "options": options }));
    }
    let same_provider = split(current).and_then(|(pid, _)| listed.iter().find(|v| split(v).is_some_and(|(p, _)| p == pid)).cloned());
    let current = if listed.iter().any(|v| v == current) { current.to_owned() } else { same_provider.or_else(|| listed.first().cloned())? };
    Some(json!({
        "id": ROUTE,
        "name": "Model",
        "description": "Provider and model; another one restarts the agent on it",
        "category": "model",
        "type": "select",
        "currentValue": current,
        "options": groups,
    }))
}

static LIVE: LazyLock<Mutex<BTreeMap<String, (Instant, Vec<String>)>>> = LazyLock::new(Default::default);

/// The models each provider a harness can run on lists now, asked with its key at most once an
/// hour, so the picker offers the names the API takes today. A provider that does not answer
/// is left out and its catalog models stand.
pub async fn live_models(secrets: &crate::secrets::Vault, name: &str, engine: &EngineConfig, vault: &[SecretInfo]) -> Live {
    let Some(h) = harness(name, engine) else { return Live::new() };
    let mut out = Live::new();
    for (p, key) in usable(&h, vault) {
        let cached = LIVE.locked().get(&p.id).filter(|(at, _)| at.elapsed() < Duration::from_secs(3600)).map(|(_, m)| m.clone());
        if let Some(models) = cached {
            out.insert(p.id, models);
            continue;
        }
        let Some(url) = p.url.clone() else { continue };
        let Ok(mut values) = secrets.env(std::slice::from_ref(&key)).await else { continue };
        let Some((_, value)) = values.pop() else { continue };
        let (ok, _, models) = crate::engines::check_key(&url, &value).await;
        if ok && !models.is_empty() {
            LIVE.locked().insert(p.id.clone(), (Instant::now(), models.clone()));
            out.insert(p.id, models);
        }
    }
    out
}

/// What asking a provider with its key found.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ProviderTest {
    pub ok: bool,
    pub message: String,
    /// The models its API lists now.
    pub models: Vec<String>,
}

/// Tries the vault's key for provider `id` against its API, and keeps the models it lists for
/// the model picker.
pub async fn test_provider(secrets: &crate::secrets::Vault, id: &str) -> anyhow::Result<ProviderTest> {
    let p = providers().into_iter().find(|p| p.id == id).ok_or_else(|| anyhow::anyhow!("Hub knows no provider {id:?}"))?;
    let vault = secrets.list().await?;
    let Some(key) = matching_keys(&p, &vault).into_iter().next() else {
        return Ok(ProviderTest { ok: false, message: format!("The key vault has no key for {}", p.label), models: Vec::new() });
    };
    let Some(url) = p.url.clone() else {
        return Ok(ProviderTest { ok: true, message: "Saved; Hub cannot check this provider's key".into(), models: Vec::new() });
    };
    let value = secrets.env(std::slice::from_ref(&key.name)).await?.pop().map(|(_, v)| v).unwrap_or_default();
    let (ok, message, models) = crate::engines::check_key(&url, &value).await;
    if ok {
        LIVE.locked().insert(p.id.clone(), (Instant::now(), models.clone()));
    }
    let message = if ok { format!("The key works; {} lists {} model{}", p.label, models.len(), if models.len() == 1 { "" } else { "s" }) } else { message };
    Ok(ProviderTest { ok, message, models })
}

/// The route a thread picked, from its settings.
pub fn picked(settings: &[(String, Value)]) -> Option<String> {
    settings.iter().rev().find(|(id, _)| id == ROUTE).and_then(|(_, v)| v.as_str().map(str::to_owned))
}

/// Keys the providers a harness engine can run on need, by the vault's name, for sending to
/// other machines along with the engine.
pub fn key_names(name: &str, engine: &EngineConfig, vault: &[SecretInfo]) -> Vec<String> {
    match harness(name, engine) {
        Some(h) => usable(&h, vault).into_iter().map(|(_, k)| k).collect(),
        None => Vec::new(),
    }
}

/// The settings with the route left out; the agent never sees it.
pub fn without(settings: Vec<(String, Value)>) -> Vec<(String, Value)> {
    settings.into_iter().filter(|(id, _)| id != ROUTE).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault(names: &[&str]) -> Vec<SecretInfo> {
        names.iter().map(|n| SecretInfo { name: (*n).into(), hint: "efa8".into() }).collect()
    }

    fn hermes() -> EngineConfig {
        EngineConfig { command: "hermes acp".into(), harness: Some("hermes".into()), ..Default::default() }
    }

    #[test]
    fn hermes_lists_every_provider_with_a_key_and_its_own_setup_only_without() {
        let o = option("hermes", &hermes(), OWN, &vault(&["DEEPSEEK", "OPENROUTER_API_KEY", "GITHUB_TOKEN"]), &Live::new()).unwrap();
        let groups: Vec<&str> = o["options"].as_array().unwrap().iter().map(|g| g["group"].as_str().unwrap()).collect();
        assert_eq!(groups, ["deepseek", "openrouter"]);
        assert_eq!(o["category"], "model");
        // A thread that was on its own setup moves to the first provider with a key.
        assert_eq!(o["currentValue"], "deepseek:deepseek-v4-pro");
        // With no key at all, its own setup is all there is.
        let o = option("hermes", &hermes(), OWN, &vault(&[]), &Live::new()).unwrap();
        assert_eq!(o["currentValue"], "own");
        // It reads as an ACP config option.
        let _: agent_client_protocol::schema::v1::SessionConfigOption = serde_json::from_value(o).unwrap();
    }

    #[test]
    fn a_route_sets_the_provider_env_and_key() {
        let e = apply("hermes", &hermes(), "deepseek:deepseek-reasoner", &vault(&["DEEPSEEK"]));
        assert_eq!(e.env["HERMES_INFERENCE_PROVIDER"], "deepseek");
        assert_eq!(e.env["HERMES_MODEL"], "deepseek-reasoner");
        assert_eq!(e.keys["DEEPSEEK_API_KEY"], "DEEPSEEK");
        assert_eq!(e.model.as_deref(), Some("deepseek:deepseek-reasoner"));
        // Switching to OpenRouter drops DeepSeek's key and env.
        let e = apply("hermes", &e, "openrouter:moonshotai/kimi-k2", &vault(&["DEEPSEEK", "OPENROUTER_API_KEY"]));
        assert_eq!(e.env["HERMES_INFERENCE_PROVIDER"], "openrouter");
        assert!(e.keys.is_empty());
        assert_eq!(e.secrets, ["OPENROUTER_API_KEY"]);
        // Its own setup gets nothing from Hub.
        let e = apply("hermes", &e, OWN, &vault(&[]));
        assert!(e.env.is_empty() && e.secrets.is_empty() && e.model.is_none());
    }

    #[test]
    fn an_engine_made_for_one_provider_starts_on_it() {
        let draft = crate::engines::EngineDraft { harness: "hermes".into(), provider: "deepseek".into(), model: "deepseek-chat".into(), ..Default::default() };
        let (name, engine, _) = crate::engines::build(&draft).unwrap();
        assert_eq!(default_route(&engine), "deepseek:deepseek-chat");
        assert!(option(&name, &engine, &default_route(&engine), &vault(&["DEEPSEEK"]), &Live::new()).is_some());
        // Claude and Codex have no providers to pick.
        assert!(option("claude", &crate::config::Config::default().engines["claude"], OWN, &vault(&["DEEPSEEK"]), &Live::new()).is_none());
    }

    fn claude_on_deepseek() -> EngineConfig {
        let draft = crate::engines::EngineDraft { harness: "claude".into(), provider: "deepseek".into(), key_name: "DEEPSEEK".into(), ..Default::default() };
        crate::engines::build(&draft).unwrap().1
    }

    #[test]
    fn claude_code_on_deepseek_lists_deepseek_only_and_never_its_own_setup() {
        let e = claude_on_deepseek();
        // Claude Code itself has no route; the pairing does, and keeps its own name in lists.
        assert!(harness("claude", &crate::config::Config::default().engines["claude"]).is_none());
        assert!(harness("claude-deepseek", &e).is_some());
        assert_eq!(crate::engines::label("claude-deepseek", &e).as_deref(), Some("Claude Code · DeepSeek"));
        let o = option("claude-deepseek", &e, &default_route(&e), &vault(&["DEEPSEEK"]), &Live::new()).unwrap();
        let groups: Vec<&str> = o["options"].as_array().unwrap().iter().map(|g| g["group"].as_str().unwrap()).collect();
        assert_eq!(groups, ["deepseek"]);
        assert_eq!(o["currentValue"], "deepseek:deepseek-v4-pro");
        // Without a key there is nothing to pick: not Claude's subscription under DeepSeek's name.
        assert!(option("claude-deepseek", &e, OWN, &vault(&[]), &Live::new()).is_none());
    }

    #[test]
    fn picking_another_deepseek_model_in_claude_code_changes_what_it_reads() {
        let e = claude_on_deepseek();
        let e = apply("claude-deepseek", &e, "deepseek:deepseek-flash", &vault(&["DEEPSEEK"]));
        assert_eq!(e.env["ANTHROPIC_MODEL"], "deepseek-flash");
        assert_eq!(e.env["ANTHROPIC_BASE_URL"], "https://api.deepseek.com/anthropic");
        assert_eq!(e.keys["ANTHROPIC_AUTH_TOKEN"], "DEEPSEEK");
        assert!(e.secrets.is_empty());
        assert_eq!(e.model.as_deref(), Some("deepseek-flash"));
        // Claude Code reads the model from its environment; Hermes is told over ACP.
        assert!(!sets_model("claude-deepseek", &e));
        assert!(sets_model("hermes", &hermes()));
    }

    #[test]
    fn the_api_s_model_names_replace_an_old_one() {
        let mut live = Live::new();
        live.insert("deepseek".into(), vec!["deepseek-flash".into(), "deepseek-v4-pro".into()]);
        let o = option("hermes", &hermes(), "deepseek:deepseek-chat", &vault(&["DEEPSEEK"]), &live).unwrap();
        assert_eq!(o["currentValue"], "deepseek:deepseek-flash");
        let deepseek = o["options"].as_array().unwrap().iter().find(|g| g["group"] == "deepseek").unwrap();
        let names: Vec<&str> = deepseek["options"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["deepseek-flash", "deepseek-v4-pro"]);
    }
}
