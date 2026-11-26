//! API keys and other secrets Hub hands to engines. Values live in the operating system's
//! credential store (Windows Credential Manager), never in `hub.json`, the log or the logs; on
//! other systems, where a headless host has no such store, in `<data dir>/secrets.json` readable
//! only by the user. The core keeps the names in the database so it can list them.
//!
//! Only the real Hub, the one on the default data folder, uses the credential store. Its entries
//! are one per name for the whole Windows user, so anything else that opened a vault there (the
//! test suite the updater runs before each build, the dev core on its copy of the data) wrote
//! over the real keys: a test's DeepSeek key ending in 7890, a test deleting the API connection's
//! key. Those keep their values in `secrets.json` in their own data folder instead.
//!
//! An engine gets the secrets its config names in `secrets`, as environment variables of the same
//! name. Nothing is passed by default: an `ANTHROPIC_API_KEY` in Claude Code's environment would
//! switch it from the subscription to paying per token.
//!
//! Any secret value that turns up in what an engine says is replaced by `[secret NAME]` before it
//! is logged.
//!
//! Settings > Key vault shows every key by its end only (`text::key_hint`), with when it was added,
//! last replaced and last handed out, and what uses it (`Hub::key_vault` in `apis/vault.rs`): engines that name it,
//! API connections that hold it, the projects those are on and Hub's own features. Those dates
//! are kept in the store next to the names (`secret_meta`); the values never are.

use crate::locked::RwLockExt;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result, bail};

use crate::model::{SecretInfo, now_ms};
use crate::store::Store;

const NAMES: &str = "secret_names";
const META: &str = "secret_meta";
/// Handing a key out is noted at most this often, so engine starts don't write each time.
const USED_EVERY_MS: i64 = 10 * 60_000;
#[cfg(windows)]
const SERVICE: &str = "hub";

/// The vault's values as last loaded, for the one-answer helpers (`engine::complete`) that run
/// an engine without the hub at hand. An engine added with a key needs it there too.
static SHARED: RwLock<BTreeMap<String, String>> = RwLock::new(BTreeMap::new());

fn shared(values: &BTreeMap<String, String>) {
    *SHARED.write_locked() = values.clone();
}

/// An engine's keys as last loaded, each under the variable the engine reads it from.
pub(crate) fn shared_engine_env(engine: &crate::config::EngineConfig) -> Vec<(String, String)> {
    let values = SHARED.read_locked().clone();
    engine_env_from(engine, &values)
}

fn engine_env_from(engine: &crate::config::EngineConfig, values: &BTreeMap<String, String>) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> =
        engine.secrets.iter().filter_map(|n| Some((n.clone(), values.get(n)?.clone()))).collect();
    env.extend(engine.keys.iter().filter_map(|(var, name)| Some((var.clone(), values.get(name)?.clone()))));
    env
}

pub struct Vault {
    store: Store,
    file: PathBuf,
    /// The operating system's credential store, only for the real Hub (see the top).
    #[cfg_attr(not(windows), allow(dead_code))]
    os_store: bool,
    /// Keys live only in `cache`: never written, never read from disk (`Config::keys_in_memory`).
    in_memory: bool,
    /// Loaded once, so redaction and engine starts never wait on the credential store.
    cache: RwLock<Option<Arc<BTreeMap<String, String>>>>,
    meta: RwLock<Option<BTreeMap<String, Meta>>>,
    /// One change at a time: `set` and `delete` read the whole vault, change one key and write
    /// the whole vault (and its list of names) back, so two at once would lose each other's key.
    changing: tokio::sync::Mutex<()>,
    /// Counts changes, so engines started on an older key can be started again (`renew_engines`).
    changed: tokio::sync::watch::Sender<u64>,
}

/// When a key came, last changed and was last handed out, in ms.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct Meta {
    pub(crate) created: i64,
    pub(crate) changed: i64,
    pub(crate) used: Option<i64>,
}

impl Vault {
    pub fn new(store: Store, data_dir: &std::path::Path) -> Self {
        Self {
            store,
            file: data_dir.join("secrets.json"),
            os_store: cfg!(windows) && !cfg!(test) && same_dir(data_dir, &hub_install::data_dir()),
            in_memory: false,
            cache: RwLock::new(None),
            meta: RwLock::new(None),
            changing: tokio::sync::Mutex::new(()),
            changed: tokio::sync::watch::Sender::new(0),
        }
    }

    /// A vault that keeps its keys in memory only: what other machines lend it is gone when
    /// hubd stops, and the next push hands it over again. Nothing of it is on disk.
    pub fn in_memory(mut self, on: bool) -> Self {
        self.in_memory = on;
        self
    }

    /// Whether the keys live in memory only (lent by another machine), so nothing put here
    /// survives a restart.
    pub fn is_in_memory(&self) -> bool {
        self.in_memory
    }

    /// One key's value, if the vault has it.
    pub async fn value(&self, name: &str) -> Option<String> {
        self.values().await.ok()?.get(name).cloned()
    }

    pub async fn list(&self) -> Result<Vec<SecretInfo>> {
        let values = self.values().await?;
        Ok(values.iter().map(|(name, value)| SecretInfo { name: name.clone(), hint: crate::text::key_hint(value) }).collect())
    }

    /// Where the values are, in words for the vault page.
    pub fn kept_in(&self) -> String {
        if self.in_memory {
            "memory only, lent by another machine".into()
        } else if self.os_store {
            "Windows Credential Manager".into()
        } else {
            format!("{}, readable only by you", self.file.display())
        }
    }

    /// Adds a key that is not there yet. A model provider's own key is refused: it has no use
    /// but moving an engine off its subscription.
    pub async fn create(&self, name: &str, value: &str) -> Result<()> {
        if crate::config::ENGINE_KEYS.contains(&name.trim()) {
            bail!("{} would make an engine pay per token instead of using your subscription, so the vault doesn't take it", name.trim());
        }
        if self.known_names().await?.iter().any(|n| n == name.trim()) {
            bail!("{} is already in the vault; replace it instead", name.trim());
        }
        self.set(name, value).await
    }

    /// Replaces a key that is there, keeping its name and everything that uses it.
    pub async fn rotate(&self, name: &str, value: &str) -> Result<()> {
        if !self.known_names().await?.iter().any(|n| n == name) {
            bail!("{name} is not in the vault");
        }
        if self.values().await?.get(name).is_some_and(|old| old == value.trim()) {
            bail!("that is the key {name} already has");
        }
        self.set(name, value).await
    }

    /// Adds or replaces a key. A model provider's own key is refused here too, whichever way it
    /// comes in (Settings, Add engine, the API): engines stay on subscriptions.
    pub async fn set(&self, name: &str, value: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            bail!("a secret's name is letters, digits and _, like OPENAI_API_KEY");
        }
        if crate::config::ENGINE_KEYS.contains(&name) {
            bail!("{name} would make an engine pay per token instead of using your subscription, so the vault doesn't take it");
        }
        if value.trim().is_empty() {
            bail!("an empty secret");
        }
        let _one_at_a_time = self.changing.lock().await;
        let mut values = self.values().await?;
        self.write(name, Some(value.trim()), &values)?;
        values.insert(name.to_owned(), value.trim().to_owned());
        self.save_names(&values).await?;
        shared(&values);
        *self.cache.write_locked() = Some(Arc::new(values));
        self.changed.send_modify(|n| *n += 1);
        let now = now_ms();
        self.edit_meta(|meta| {
            let m = meta.entry(name.to_owned()).or_insert(Meta { created: now, ..Meta::default() });
            m.changed = now;
        })
        .await;
        Ok(())
    }

    pub async fn delete(&self, name: &str) -> Result<()> {
        let _one_at_a_time = self.changing.lock().await;
        let mut values = self.values().await?;
        values.remove(name);
        self.write(name, None, &values)?;
        self.save_names(&values).await?;
        shared(&values);
        *self.cache.write_locked() = Some(Arc::new(values));
        self.changed.send_modify(|n| *n += 1);
        self.edit_meta(|meta| {
            meta.remove(name);
        })
        .await;
        Ok(())
    }

    /// The named secrets that exist, for an engine's environment.
    pub async fn env(&self, names: &[String]) -> Result<Vec<(String, String)>> {
        let values = self.values().await?;
        let env: Vec<(String, String)> = names.iter().filter_map(|n| Some((n.clone(), values.get(n)?.clone()))).collect();
        let now = now_ms();
        let meta = self.meta().await;
        if env.iter().any(|(n, _)| meta.get(n).is_none_or(|m| m.used.is_none_or(|u| now - u > USED_EVERY_MS))) {
            self.edit_meta(|meta| {
                for (n, _) in &env {
                    meta.entry(n.clone()).or_default().used = Some(now);
                }
            })
            .await;
        }
        Ok(env)
    }

    /// An engine's keys, each under the variable the engine reads it from (`EngineConfig::keys`).
    pub async fn engine_env(&self, engine: &crate::config::EngineConfig) -> Result<Vec<(String, String)>> {
        let names = engine.key_names();
        let values: BTreeMap<String, String> = self.env(&names).await?.into_iter().collect();
        Ok(engine_env_from(engine, &values))
    }

    /// Replaces every known secret value in `data` by its name, and anything else that looks
    /// like a key by a hint of it (see `leak.rs`).
    pub fn redact(&self, data: &mut serde_json::Value) {
        crate::leak::redact_json(data, &self.cached());
    }

    /// The secrets as loaded, without waiting on the credential store; empty before `load`.
    /// Shared, not copied: redaction asks for it on every streamed token.
    pub fn cached(&self) -> Arc<BTreeMap<String, String>> {
        self.cache.read_locked().clone().unwrap_or_default()
    }

    /// Wakes on every key added, replaced or removed.
    pub fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.subscribe()
    }

    /// The vault keys whose values are among `env`'s, by name: what an engine started with.
    pub fn used_in(&self, env: &[(String, String)]) -> BTreeMap<String, String> {
        self.cached().iter().filter(|(_, v)| env.iter().any(|(_, e)| e == *v)).map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Fills the cache; called when the core starts.
    pub async fn load(&self) -> Result<()> {
        self.values().await.map(drop)
    }

    pub(crate) async fn values(&self) -> Result<BTreeMap<String, String>> {
        if let Some(values) = self.cache.read_locked().clone() {
            return Ok((*values).clone());
        }
        let names = self.store_names().await?;
        let values = self.read(&names)?;
        shared(&values);
        *self.cache.write_locked() = Some(Arc::new(values.clone()));
        Ok(values)
    }

    /// The names as listed, with ones whose value went missing from the credential store.
    pub(crate) async fn store_names(&self) -> Result<Vec<String>> {
        if self.in_memory {
            return Ok(Vec::new());
        }
        Ok(match self.store.get_value(NAMES).await? {
            Some(json) => serde_json::from_str(&json)?,
            None => Vec::new(),
        })
    }

    pub(crate) async fn meta(&self) -> BTreeMap<String, Meta> {
        if let Some(meta) = self.meta.read_locked().clone() {
            return meta;
        }
        let meta: BTreeMap<String, Meta> = match self.store.get_value(META).await {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            _ => BTreeMap::new(),
        };
        *self.meta.write_locked() = Some(meta.clone());
        meta
    }

    /// Dates are a help, not the key: failing to keep one is logged, never an error.
    async fn edit_meta(&self, change: impl FnOnce(&mut BTreeMap<String, Meta>)) {
        let mut meta = self.meta().await;
        change(&mut meta);
        *self.meta.write_locked() = Some(meta.clone());
        match serde_json::to_string(&meta) {
            Ok(json) => {
                if let Err(err) = self.store.set_value(META, &json).await {
                    tracing::warn!("could not keep the vault's dates: {err:#}");
                }
            }
            Err(err) => tracing::warn!("could not keep the vault's dates: {err:#}"),
        }
    }

    /// The names the vault has: those on record, or in memory mode those held now.
    async fn known_names(&self) -> Result<Vec<String>> {
        if self.in_memory {
            return Ok(self.values().await?.into_keys().collect());
        }
        self.store_names().await
    }

    async fn save_names(&self, values: &BTreeMap<String, String>) -> Result<()> {
        if self.in_memory {
            return Ok(());
        }
        let names: Vec<&String> = values.keys().collect();
        self.store.set_value(NAMES, &serde_json::to_string(&names)?).await
    }

    fn read(&self, names: &[String]) -> Result<BTreeMap<String, String>> {
        if self.in_memory {
            return Ok(BTreeMap::new());
        }
        #[cfg(windows)]
        if self.os_store {
            let mut values = BTreeMap::new();
            for name in names {
                match keyring::Entry::new(SERVICE, name)?.get_password() {
                    Ok(value) => {
                        values.insert(name.clone(), value);
                    }
                    Err(keyring::Error::NoEntry) => {}
                    Err(err) => tracing::warn!("could not read secret {name}: {err}"),
                }
            }
            return Ok(values);
        }
        let all: BTreeMap<String, String> = match std::fs::read_to_string(&self.file) {
            Ok(json) => serde_json::from_str(&json).with_context(|| format!("reading {}", self.file.display()))?,
            Err(_) => BTreeMap::new(),
        };
        Ok(all.into_iter().filter(|(k, _)| names.contains(k)).collect())
    }

    fn write(&self, name: &str, value: Option<&str>, all: &BTreeMap<String, String>) -> Result<()> {
        if self.in_memory {
            return Ok(());
        }
        #[cfg(windows)]
        if self.os_store {
            let entry = keyring::Entry::new(SERVICE, name)?;
            match value {
                Some(value) => entry.set_password(value).context("saving to Windows Credential Manager")?,
                None => match entry.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => {}
                    Err(err) => return Err(err).context("removing from Windows Credential Manager"),
                },
            }
            return Ok(());
        }
        use std::io::Write;
        let mut all = all.clone();
        match value {
            Some(value) => all.insert(name.to_owned(), value.to_owned()),
            None => all.remove(name),
        };
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        let mut open = std::fs::OpenOptions::new();
        open.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
        let mut file = open.open(&tmp)?;
        file.write_all(serde_json::to_string_pretty(&all)?.as_bytes())?;
        drop(file);
        std::fs::rename(&tmp, &self.file).with_context(|| format!("writing {}", self.file.display()))?;
        Ok(())
    }
}

/// Whether two data folders are the same one, however they are spelled.
fn same_dir(a: &std::path::Path, b: &std::path::Path) -> bool {
    let norm = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_owned());
    norm(a) == norm(b)
}

/// Kept tokens by engine command, as `(variable, value)`, for engines started without the hub at
/// hand (`engine::complete`). Commands, not names, because that is what those callers have.
static SIGNED_IN: RwLock<BTreeMap<String, (String, String)>> = RwLock::new(BTreeMap::new());

/// The kept sign-in (`signin.rs`) for the engine run by `command`, as environment variables.
pub(crate) fn signed_in_env(command: &str) -> Vec<(String, String)> {
    SIGNED_IN.read_locked().get(command).cloned().into_iter().collect()
}

/// Puts the kept sign-ins (`Hub::load_sign_ins`) where [`signed_in_env`] reads them.
pub(crate) fn set_signed_in_env(env: BTreeMap<String, (String, String)>) {
    *SIGNED_IN.write_locked() = env;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn redaction_borrows_the_cache_and_follows_changes() -> Result<()> {
        let data = tempfile::tempdir()?;
        let vault = Vault::new(Store::in_memory()?, data.path());
        vault.set("A_KEY", "sk-abcdefghijklmnop1234").await?;
        assert!(Arc::ptr_eq(&vault.cached(), &vault.cached()), "no copy per call");
        vault.set("B_KEY", "plainsecretvalue5678").await?;
        let mut said = serde_json::json!({ "t": "sk-abcdefghijklmnop1234 and plainsecretvalue5678" });
        vault.redact(&mut said);
        assert_eq!(said["t"], "[secret A_KEY] and [secret B_KEY]");
        vault.delete("A_KEY").await?;
        assert!(!vault.cached().contains_key("A_KEY"));
        Ok(())
    }

    #[tokio::test]
    async fn secrets_are_kept_listed_by_name_and_redacted() -> Result<()> {
        let data = tempfile::tempdir()?;
        let vault = Vault::new(Store::in_memory()?, data.path());
        vault.set("TYPESAFE_API_KEY", "sk-abcdefghijklmnop1234").await?;
        let listed = vault.list().await?;
        assert_eq!(listed[0].name, "TYPESAFE_API_KEY");
        assert_eq!(listed[0].hint, "…1234");

        let mut said = serde_json::json!({ "content": { "text": "key is sk-abcdefghijklmnop1234." } });
        vault.redact(&mut said);
        assert_eq!(said["content"]["text"], "key is [secret TYPESAFE_API_KEY].");

        assert_eq!(vault.env(&["TYPESAFE_API_KEY".into(), "NOPE".into()]).await?.len(), 1);
        vault.delete("TYPESAFE_API_KEY").await?;
        assert!(vault.list().await?.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn keys_set_at_the_same_time_all_stay() -> Result<()> {
        let data = tempfile::tempdir()?;
        let vault = std::sync::Arc::new(Vault::new(Store::in_memory()?, data.path()));
        let sets = (0..12).map(|i| {
            let vault = vault.clone();
            tokio::spawn(async move { vault.set(&format!("KEY_{i}"), &format!("value-{i}-abcdefghij")).await })
        });
        for s in sets {
            s.await??;
        }
        let names: Vec<String> = vault.list().await?.into_iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 12, "{names:?}");
        // A fresh vault reads the same twelve back from where they are kept.
        let again = Vault::new(vault.store.clone(), data.path());
        assert_eq!(again.list().await?.len(), 12);
        // A provider's own key is refused however it comes in.
        let err = vault.set("ANTHROPIC_API_KEY", "sk-ant-12345678901234567890").await.unwrap_err();
        assert!(err.to_string().contains("subscription"), "{err}");
        Ok(())
    }

    #[tokio::test]
    async fn keys_are_created_once_rotated_in_place_and_dated() -> Result<()> {
        let data = tempfile::tempdir()?;
        let vault = Vault::new(Store::in_memory()?, data.path());
        vault.create("WEATHER_API_KEY", "wx-abcdefghijklmnop1111").await?;
        assert!(vault.create("WEATHER_API_KEY", "wx-other").await.is_err(), "a second create would hide a rotation");
        assert!(vault.create("ANTHROPIC_API_KEY", "sk-ant-abcdefghijklmnop").await.is_err());
        assert!(vault.rotate("NOPE_KEY", "x-abcdefghijklmnop").await.is_err());
        assert!(vault.rotate("WEATHER_API_KEY", "wx-abcdefghijklmnop1111").await.is_err(), "the same key again is no rotation");

        let first = vault.meta().await["WEATHER_API_KEY"].clone();
        assert!(first.created > 0 && first.used.is_none());
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        vault.rotate("WEATHER_API_KEY", "wx-abcdefghijklmnop2222").await?;
        let after = vault.meta().await["WEATHER_API_KEY"].clone();
        assert_eq!(after.created, first.created);
        assert!(after.changed > first.changed);
        assert_eq!(vault.list().await?[0].hint, "…2222");

        vault.env(&["WEATHER_API_KEY".into()]).await?;
        assert!(vault.meta().await["WEATHER_API_KEY"].used.is_some());

        // The dates outlive the cache: a core started again reads them from the store.
        let again = Vault::new(vault.store.clone(), data.path());
        assert_eq!(again.meta().await["WEATHER_API_KEY"].created, first.created);
        vault.delete("WEATHER_API_KEY").await?;
        assert!(!vault.meta().await.contains_key("WEATHER_API_KEY"));
        Ok(())
    }

    #[tokio::test]
    async fn a_vault_on_another_data_folder_keeps_its_keys_there() -> Result<()> {
        // Tests and the dev core must never reach the real Hub's keys.
        let data = tempfile::tempdir()?;
        let vault = Vault::new(Store::in_memory()?, data.path());
        assert!(!vault.os_store);
        vault.set("DEEPSEEK", "ds-abcdefghijklmnop1234567890").await?;
        let kept = std::fs::read_to_string(data.path().join("secrets.json"))?;
        assert!(kept.contains("ds-abcdefghijklmnop1234567890"));
        assert!(vault.kept_in().contains("secrets.json"));
        Ok(())
    }

    #[tokio::test]
    async fn a_vault_in_memory_writes_nothing_and_forgets_on_restart() -> Result<()> {
        let data = tempfile::tempdir()?;
        let store = Store::in_memory()?;
        let vault = Vault::new(store.clone(), data.path()).in_memory(true);
        vault.create("DEEPSEEK", "ds-abcdefghijklmnop1234567890").await?;
        assert_eq!(vault.env(&["DEEPSEEK".into()]).await?[0].1, "ds-abcdefghijklmnop1234567890");
        assert!(vault.kept_in().contains("memory only"));
        vault.rotate("DEEPSEEK", "ds-zzzzzzzzzzzzzzzzzzzzzzzz").await?;
        assert!(!data.path().join("secrets.json").exists(), "nothing is written to disk");
        assert!(store.get_value("secret_names").await?.is_none(), "not even the names");
        // The same data folder after a restart: the key is gone, and an old file is not read.
        let again = Vault::new(store, data.path()).in_memory(true);
        assert!(again.list().await?.is_empty());
        std::fs::write(data.path().join("secrets.json"), r#"{"OLD":"old-value-from-before"}"#)?;
        let later = Vault::new(Store::in_memory()?, data.path()).in_memory(true);
        assert!(later.list().await?.is_empty() && later.cached().is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn names_must_be_environment_names() -> Result<()> {
        let data = tempfile::tempdir()?;
        let vault = Vault::new(Store::in_memory()?, data.path());
        assert!(vault.set("my key", "x").await.is_err());
        Ok(())
    }
}
