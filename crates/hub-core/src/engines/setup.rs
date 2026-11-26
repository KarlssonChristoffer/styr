//! From a draft on Settings > Engines to an entry in `hub.json`: the catalog with what is
//! installed here and which keys the vault holds, building an engine from harness, provider and
//! model (its name, environment and key names, refusing one that would pay per token), and saving
//! or removing it in the file.

use super::*;

/// The catalog with what is installed here and which keys the vault has.
pub fn catalog(running: &Config, vault: &[crate::model::SecretInfo]) -> Result<EngineCatalog> {
    let keys: Vec<String> = vault.iter().map(|s| s.name.clone()).collect();
    let saved = saved_config()?;
    let builtins = Config::default().engines;
    let harnesses: Vec<Harness> = harnesses()
        .into_iter()
        .map(|mut h| {
            if let Some(command) = resolve(&h.command) {
                h.installed = true;
                h.command = command;
            }
            h.settings = crate::harness_settings::has_settings(&h.id);
            h
        })
        .collect();
    let providers = providers()
        .into_iter()
        .map(|mut p| {
            p.keys = matching_keys(&p, vault);
            p.has_key = !p.keys.is_empty();
            p
        })
        .collect();
    let engines = saved
        .engines
        .iter()
        .filter(|(name, _)| *name != crate::sim::ENGINE)
        .map(|(name, e)| {
            let harness = e.harness.clone().or_else(|| harnesses.iter().find(|h| h.id == *name).map(|h| h.id.clone()));
            let label = label(name, e).unwrap_or_else(|| {
                harnesses.iter().find(|h| Some(&h.id) == harness.as_ref()).map(|h| h.label.clone()).unwrap_or_else(|| name.clone())
            });
            EngineEntry {
                name: name.clone(),
                label,
                command: e.command.clone(),
                harness,
                provider: e.provider.clone(),
                model: e.model.clone(),
                secrets: e.key_names().into_iter().map(|s| { let has = keys.contains(&s); (s, has) }).collect(),
                default: saved.default_engine == *name,
                running: running.engines.get(name).is_some_and(|r| same(r, e)),
                builtin: builtins.contains_key(name),
                shared_from: e.shared_from.clone(),
            }
        })
        .collect();
    Ok(EngineCatalog { harnesses, providers, engines })
}

/// Two engine settings start the same thing.
pub(super) fn same(a: &EngineConfig, b: &EngineConfig) -> bool {
    a.command == b.command && a.secrets == b.secrets && a.keys == b.keys && a.model == b.model && a.env == b.env
}

pub(crate) fn saved_config() -> Result<Config> {
    let file = read_file()?;
    serde_json::from_value(file).context("hub.json")
}

pub(super) fn read_file() -> Result<Value> {
    config::read_file()
}

/// The engines listed in the file, or the built-in ones when it lists none: a listed `engines`
/// replaces the built-in list, so the whole list is always written.
pub(super) fn engines_in(file: &Value) -> serde_json::Map<String, Value> {
    match file.get("engines") {
        Some(Value::Object(m)) => m.clone(),
        _ => Config::default()
            .engines
            .into_iter()
            .map(|(k, v)| (k, serde_json::to_value(v).unwrap_or_default()))
            .collect(),
    }
}

/// A name made of lower case letters, digits and `-`, from the harness and provider.
pub fn name_for(harness: &str, provider: &str) -> String {
    let raw = if provider.is_empty() { harness.to_owned() } else { format!("{harness}-{provider}") };
    let mut name: String = raw
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while name.contains("--") {
        name = name.replace("--", "-");
    }
    name.trim_matches('-').to_owned()
}

pub(super) fn fill(template: &str, provider: &str, model: &str, full: &str, url: &str) -> String {
    template.replace("{provider}", provider).replace("{model}", model).replace("{full}", full).replace("{url}", url)
}

/// The provider the draft picks: its id, the name of the key it needs and its label. Fails when the
/// harness does not run on it.
fn pick_provider(draft: &EngineDraft, harness: Option<&Harness>) -> Result<(Option<String>, Option<String>, Option<String>)> {
    let provider_pick = draft.provider.trim();
    let (provider_id, secret, provider_label) = match provider_pick {
        "" => (None, None, None),
        "other" => {
            let id = draft.provider_id.trim();
            if id.is_empty() {
                bail!("say what the harness calls the provider, e.g. kimi-coding");
            }
            let secret = draft.secret.trim();
            (Some(id.to_owned()), (!secret.is_empty()).then(|| secret.to_owned()), Some(id.to_owned()))
        }
        id => {
            let p = providers().into_iter().find(|p| p.id == id).with_context(|| format!("Hub knows no provider {id:?}"))?;
            (Some(p.id), Some(p.secret), Some(p.label))
        }
    };
    if let (Some(h), Some(p)) = (harness, &provider_id)
        && provider_pick != "other"
        && !h.runs_on(p)
    {
        bail!("{} does not run on {}", h.label, provider_label.as_deref().unwrap_or(p));
    }
    // What a harness reads from the provider's own address (Claude Code: `{url}`) has to be there.
    let provider_def = provider_id.as_deref().and_then(|id| providers().into_iter().find(|p| p.id == id));
    if let Some(h) = harness
        && provider_id.is_some()
        && h.env.values().any(|v| v.contains("{url}"))
        && provider_def.as_ref().is_none_or(|p| p.anthropic_url.is_none())
    {
        bail!("{} does not run on {}", h.label, provider_label.as_deref().unwrap_or("that provider"));
    }

    Ok((provider_id, secret, provider_label))
}

/// A key's name must not move an engine onto paying per token, and looks like an environment
/// variable.
fn check_secret(secret: Option<&str>) -> Result<()> {
    if let Some(s) = secret {
        if crate::config::ENGINE_KEYS.contains(&s) {
            bail!("{s} would move an engine off your subscription onto paying per token; Hub keeps Claude, Codex and Gemini on subscriptions");
        }
        if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            bail!("a key's name is like an environment variable, e.g. DEEPSEEK_API_KEY");
        }
    }

    Ok(())
}

/// The environment a harness is started with for its provider and model.
fn harness_env(harness: Option<&Harness>, provider_id: Option<&str>, provider_def: Option<&Provider>, model: &str, full: Option<&str>) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    if let (Some(h), Some(p)) = (harness, provider_id) {
        let url = provider_def.and_then(|d| d.anthropic_url.as_deref()).unwrap_or("");
        for (k, v) in &h.env {
            if v.contains("{model}") || v.contains("{full}") {
                if let Some(full) = full {
                    env.insert(k.clone(), fill(v, p, model, full, url));
                }
            } else {
                env.insert(k.clone(), fill(v, p, model, model, url));
            }
        }
    }
    env
}

/// Which vault keys the engine gets and under what names. Returns the key pasted now (to be saved
/// under its name), the names to hand out and the secret still to be asked for.
fn key_names(
    draft: &EngineDraft,
    harness: Option<&Harness>,
    provider_id: Option<&str>,
    provider_pick: &str,
    secret: Option<String>,
) -> Result<(Option<(String, String)>, BTreeMap<String, String>, Option<String>)> {
    let given = secret.clone().zip((!draft.key.trim().is_empty()).then(|| draft.key.trim().to_owned()));
    // A key already in the vault under another name is handed out under the name the harness
    // reads; a key pasted now wins and is saved under that name.
    let mut keys = BTreeMap::new();
    let mut secret = secret;
    let chosen = draft.key_name.trim();
    if given.is_none()
        && let Some(var) = &secret
        && !chosen.is_empty()
        && chosen != var
    {
        if crate::config::ENGINE_KEYS.contains(&chosen) {
            bail!("{chosen} would move an engine off your subscription onto paying per token");
        }
        keys.insert(var.clone(), chosen.to_owned());
        secret = None;
    }
    // A harness that reads the key from a variable of its own gets it there, by its vault name.
    if let Some(var) = harness.and_then(|h| h.key_env.clone())
        && provider_id.is_some()
        && provider_pick != "other"
    {
        let vault_name = secret.take().or_else(|| keys.values().next().cloned());
        keys.clear();
        if let Some(name) = vault_name {
            keys.insert(var, name);
        }
    }
    Ok((given, keys, secret))
}

/// A harness's program here and its label, for its settings (`harness_settings::Find`).
pub fn harness_program(id: &str) -> crate::harness_settings::Found {
    let Some(h) = harnesses().into_iter().find(|h| h.id == id) else {
        return crate::harness_settings::Found { label: id.to_owned(), program: Err(anyhow::anyhow!("Hub knows no harness {id:?}")) };
    };
    let program = h
        .cli
        .clone()
        .with_context(|| format!("{} has no program to ask", h.label))
        .and_then(|cli| locate(&cli).with_context(|| format!("{} is not installed on this machine", h.label)));
    crate::harness_settings::Found { label: h.label, program }
}

/// The engine a draft makes, with its name and the key it needs (name, value given now).
pub fn build(draft: &EngineDraft) -> Result<(String, EngineConfig, Option<(String, String)>)> {
    let harness_id = draft.harness.trim();
    let harness = harnesses().into_iter().find(|h| h.id == harness_id);
    let command = match (draft.command.trim(), &harness) {
        // Installed after Hub started, the program may only be found by its full path.
        ("", Some(h)) => resolve(&h.command).unwrap_or_else(|| h.command.clone()),
        ("", None) => bail!("say which command starts the agent"),
        (own, _) => own.to_owned(),
    };
    let provider_pick = draft.provider.trim();
    let (provider_id, secret, provider_label) = pick_provider(draft, harness.as_ref())?;
    let provider_def = provider_id.as_deref().and_then(|id| providers().into_iter().find(|p| p.id == id));
    check_secret(secret.as_deref())?;
    // A harness that runs on a provider only as an engine of its own has no picker to choose a
    // model later, so it starts on the provider's first.
    let paired = harness.as_ref().zip(provider_id.as_deref()).is_some_and(|(h, p)| h.paired(p));
    let model = match (draft.model.trim(), &provider_def) {
        ("", Some(p)) if paired => p.models.first().map(String::as_str).unwrap_or(""),
        (m, _) => m,
    };
    let format = harness.as_ref().map(|h| h.model_format.as_str()).unwrap_or("{model}");
    let full = match (&provider_id, model) {
        (_, "") => None,
        (Some(p), m) if !m.starts_with(&format!("{p}:")) && !m.starts_with(&format!("{p}/")) => Some(fill(format, p, m, m, "")),
        (_, m) => Some(m.to_owned()),
    };
    let env = harness_env(harness.as_ref(), provider_id.as_deref(), provider_def.as_ref(), model, full.as_deref());
    let name = match draft.name.trim() {
        "" => name_for(harness_id, provider_id.as_deref().unwrap_or("")),
        n => n.to_owned(),
    };
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("an engine's name is letters, digits and -, like hermes-deepseek");
    }
    if name == crate::sim::ENGINE {
        bail!("{name} is Hub's simulator");
    }
    let label = match (&harness, &provider_label) {
        (Some(h), Some(p)) => Some(format!("{} · {p}", h.label)),
        (Some(h), None) => Some(h.label.clone()),
        (None, Some(p)) => Some(format!("{name} · {p}")),
        (None, None) => None,
    };
    let (given, keys, secret) = key_names(draft, harness.as_ref(), provider_id.as_deref(), provider_pick, secret)?;
    let engine = EngineConfig {
        command,
        label,
        harness: harness.as_ref().map(|h| h.id.clone()),
        provider: provider_id,
        model: full,
        env,
        secrets: secret.into_iter().collect(),
        keys,
        // The harness's own CLI would not have the provider's address and key, so a thread on
        // such a pairing is not taken over in a terminal.
        terminal: if paired { None } else { harness.as_ref().and_then(|h| h.terminal.clone()) },
        traits: crate::config::EngineTraits {
            rules_file: harness.as_ref().and_then(|h| h.rules_file.clone()),
            mcp_instructions: harness.as_ref().map(|h| h.mcp_instructions),
            mcp_http: harness.as_ref().map(|h| h.mcp_http),
            mcp_token_in_url: harness.as_ref().map(|h| h.mcp_token_in_url),
            ..Default::default()
        },
        ..Default::default()
    };
    Ok((name, engine, given))
}

/// Writes an engine to `hub.json`, replacing one of the same name.
pub fn save(name: &str, engine: &EngineConfig) -> Result<()> {
    config::edit_file(|file| {
        let mut engines = engines_in(file);
        engines.insert(name.to_owned(), serde_json::to_value(engine)?);
        let root = file.as_object_mut().context("hub.json is not a JSON object")?;
        root.insert("engines".into(), Value::Object(engines));
        serde_json::from_value::<Config>(file.clone()).context("the engine would make hub.json invalid")?;
        Ok(())
    })
}

/// Takes an engine out of `hub.json`. One Hub ships goes back to how it shipped.
pub fn remove(name: &str) -> Result<()> {
    config::edit_file(|file| {
        let saved: Config = serde_json::from_value(file.clone())?;
        if saved.default_engine == name {
            bail!("{name} is the default engine; pick another default first");
        }
        let mut engines = engines_in(file);
        match Config::default().engines.remove(name) {
            Some(builtin) => {
                engines.insert(name.to_owned(), serde_json::to_value(builtin)?);
            }
            None => {
                engines.remove(name).with_context(|| format!("there is no engine called {name}"))?;
            }
        }
        let root = file.as_object_mut().context("hub.json is not a JSON object")?;
        root.insert("engines".into(), Value::Object(engines));
        Ok(())
    })
}
