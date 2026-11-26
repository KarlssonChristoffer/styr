//! The data half of sharing engines between machines: what travels (`SharedEngine`), how each
//! engine stands here (`here`, the tries in `CHECKS`) and writing what arrived into hub.json.
//! The `Hub` half that sends and receives is `engine_share.rs`.

use super::*;

/// An engine as sent to another machine: its settings, and the values of the keys it uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedEngine {
    pub name: String,
    pub config: EngineConfig,
    /// Vault key name to its value. Sent over the same connection as the machine's threads.
    pub keys: BTreeMap<String, String>,
}

/// How an engine stands on one machine.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineHere {
    pub name: String,
    /// Its program is found there.
    pub installed: bool,
    /// The core there runs it; otherwise it waits for that core's restart.
    pub running: bool,
    pub harness: Option<String>,
    /// A real try on that machine: the agent started and the provider took the key there.
    /// Missing from a hubd too old to try.
    #[serde(default)]
    pub check: Option<EngineCheck>,
    /// Its hubd tries engines and picks their provider per thread; false from an older one.
    #[serde(default)]
    pub can_try: bool,
    /// It is being tried there right now. False with no `check` means it waits for its try.
    #[serde(default)]
    pub checking: bool,
}

/// What trying a shared engine on the machine it was sent to found.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EngineCheck {
    pub ok: bool,
    pub message: String,
    /// The provider and model it was tried on, e.g. `deepseek:deepseek-flash`.
    pub route: Option<String>,
    #[ts(type = "number")]
    pub at: i64,
}

/// The last try of each engine here, by name, with what was tried (so a change tries again).
pub(crate) static CHECKS: LazyLock<Mutex<BTreeMap<String, (String, EngineCheck)>>> = LazyLock::new(Default::default);
pub(crate) static CHECKING: LazyLock<Mutex<std::collections::BTreeSet<String>>> = LazyLock::new(Default::default);
pub(crate) const CHECK_AGAIN: Duration = Duration::from_secs(6 * 3600);

/// The longest one try of an engine may take, from reading the provider's models to the agent's
/// session, before it counts as failed. The agent alone gets two minutes in `test`.
pub(crate) const CHECK_AT_MOST: Duration = Duration::from_secs(180);

/// Takes `name` off the list of engines being tried however its try ends, even by a panic, so
/// a try that went wrong never leaves the engine "being tried" until the core restarts.
pub(crate) struct Checking(pub(crate) String);

impl Drop for Checking {
    fn drop(&mut self) {
        CHECKING.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
    }
}

/// Whether engines besides the built-in ones are tried here: every engine another machine asks
/// about, sent from there or added here by hand under the same name.
pub(crate) fn tried_here(name: &str) -> bool {
    name != crate::sim::ENGINE && !Config::default().engines.contains_key(name)
}

/// Harnesses this hubd installed by itself since it started, so a failed install is tried once.
pub(crate) static AUTO_INSTALLED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Installs a harness another machine added and this one lacks, once: adding it there was the
/// ask. One that fails keeps its Install button and the installer's lines on the Engines page.
pub(crate) fn install_once(harness: &str) {
    if harnesses().iter().all(|h| h.id != harness || h.install.is_none()) {
        return;
    }
    {
        let mut done = AUTO_INSTALLED.locked();
        if done.iter().any(|h| h == harness) {
            return;
        }
        done.push(harness.to_owned());
    }
    match start_install(harness) {
        Ok(_) => tracing::info!(%harness, "installing a harness another machine added"),
        Err(e) => tracing::warn!(%harness, "could not install a harness another machine added: {e:#}"),
    }
}

/// What a machine answered when sent the engines.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MachineEngines {
    pub machine: String,
    pub engines: Vec<EngineHere>,
    /// Why the last send failed, e.g. it is offline or its hubd is too old.
    pub error: Option<String>,
    #[ts(type = "number")]
    pub at: i64,
}

pub(crate) static MACHINES: LazyLock<Mutex<BTreeMap<String, MachineEngines>>> = LazyLock::new(Default::default);
pub(crate) static SHARE_NOW: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Sends the engines to the other machines now instead of at the next minute.
pub fn share_soon() {
    SHARE_NOW.notify_one();
}

/// Whether an engine another machine handed on (`Config::relay_keys`) is one this machine made.
pub(crate) fn comes_back(engine: &SharedEngine, local: &str) -> bool {
    engine.config.shared_from.as_deref() == Some(local)
}

/// Writes the engines from `from` into hub.json. True when the file changed.
pub(crate) fn take_in(from: &str, engines: Vec<SharedEngine>) -> Result<bool> {
    let mut changed = false;
    config::edit_file(|file| {
        changed = merge_shared(file, from, engines)?;
        Ok(())
    })?;
    Ok(changed)
}

/// Puts the engines from `from` into a hub.json value. True when it changed.
pub(crate) fn merge_shared(file: &mut Value, from: &str, engines: Vec<SharedEngine>) -> Result<bool> {
    let before = file.clone();
    let mut list = engines_in(file);
    let builtins = Config::default().engines;
    let default = serde_json::from_value::<Config>(file.clone()).map(|c| c.default_engine).unwrap_or_default();
    // Gone from there: gone here, unless it is this machine's default.
    list.retain(|name, v| {
        v.get("shared_from").and_then(|f| f.as_str()) != Some(from) || engines.iter().any(|e| &e.name == name) || *name == default
    });
    for SharedEngine { name, mut config, .. } in engines {
        // An engine added here by hand under the same name stays as it is.
        if builtins.contains_key(&name) || name == crate::sim::ENGINE {
            continue;
        }
        if let Some(mine) = list.get(&name)
            && mine.get("shared_from").and_then(|f| f.as_str()) != Some(from)
        {
            continue;
        }
        config.command = resolve(&config.command).unwrap_or(config.command);
        config.shared_from = Some(from.to_owned());
        list.insert(name, serde_json::to_value(config)?);
    }
    let root = file.as_object_mut().context("hub.json is not a JSON object")?;
    root.insert("engines".into(), Value::Object(list));
    if *file == before {
        return Ok(false);
    }
    serde_json::from_value::<Config>(file.clone()).context("the engines would make hub.json invalid")?;
    Ok(true)
}

/// How each saved engine stands on this machine.
pub(crate) fn here(running: &Config) -> Result<Vec<EngineHere>> {
    let saved = saved_config()?;
    Ok(saved
        .engines
        .iter()
        .filter(|(name, _)| *name != crate::sim::ENGINE)
        .map(|(name, e)| EngineHere {
            name: name.clone(),
            installed: installed(&e.command),
            running: running.engines.get(name).is_some_and(|r| same(r, e)),
            harness: e.harness.clone(),
            check: CHECKS.locked().get(name).map(|(_, c)| c.clone()),
            can_try: true,
            checking: CHECKING.locked().contains(name),
        })
        .collect())
}

