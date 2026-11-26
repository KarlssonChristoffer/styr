//! A harness's own settings, changed from Hub: what tools it may use, its memory, when it
//! compresses a long conversation, how hard it thinks when a thread picks nothing.
//!
//! They live in the harness's own config on each machine (Hermes: `~/.hermes/config.yaml`), and
//! Hub reads and writes them through the harness's own command line (`hermes config get` and
//! `set`), so Hub never parses or rewrites its files. Which settings there are, and the commands,
//! are the catalog (`harness_settings/catalog.rs`), data only; a harness without an entry has no
//! settings page.
//!
//! The values you change are kept on the machine you change them on (store kv
//! `harness_settings`) and travel to every other machine with the engines (`engine_share::share`),
//! where `take_in` sets the ones that differ there.
//!
//! Which program a harness has is the engine catalog's to say: callers pass `engines::harness_program`
//! in as a `Find`, and say `engines::share_soon` after `write`, so this file does not need the catalog.

use crate::locked::LockExt;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

/// The harness's commands for its config, after its program, with `{key}` and `{value}`.
#[derive(Debug, Clone)]
struct Commands {
    /// Prints the value at `{key}` as JSON, a whole section when the key is one.
    get: &'static str,
    set: &'static str,
    /// Puts `{key}` back to the harness's default.
    unset: &'static str,
}

/// How a setting is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SettingKind {
    /// On or off.
    Toggle,
    /// One of `choices`.
    Choice,
    /// A list of names the harness leaves out; shown as `choices` that are on unless listed.
    OffList,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SettingChoice {
    #[ts(type = "string | number | boolean | null")]
    pub value: Value,
    pub name: String,
    pub what: String,
}

/// One setting of a harness, with its value on this machine.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HarnessSetting {
    /// Its key in the harness's config, e.g. `memory.memory_enabled`.
    pub key: String,
    pub label: String,
    pub what: String,
    pub kind: SettingKind,
    pub choices: Vec<SettingChoice>,
    /// What the harness does when the key is not set.
    #[ts(type = "string | number | boolean | null | string[]")]
    pub default: Value,
    /// Its value here; the default when the config does not set it.
    #[ts(type = "string | number | boolean | null | string[]")]
    pub value: Value,
    /// Changed from Hub, so every machine gets it.
    pub shared: bool,
    /// The thinking level threads start on. The composer offers it when the harness offers no
    /// thinking setting per thread (Hermes before its config options).
    pub thinking: bool,
}

/// A harness's settings page.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HarnessSettings {
    pub harness: String,
    pub label: String,
    /// Its version on this machine, when it is installed.
    pub version: Option<String>,
    pub settings: Vec<HarnessSetting>,
    /// Why the values could not be read, e.g. it is not installed here.
    pub problem: Option<String>,
}

fn choice(value: Value, name: &str, what: &str) -> SettingChoice {
    SettingChoice { value, name: name.into(), what: what.into() }
}

/// What Hub knows of each harness's settings (`harness_settings/catalog.rs`).
mod catalog;
use catalog::catalog;

/// The harnesses that have settings Hub can change.
pub fn has_settings(harness: &str) -> bool {
    catalog(harness).is_some()
}

const KV: &str = "harness_settings";

/// What was changed from Hub here, by harness and key.
pub type Wanted = BTreeMap<String, BTreeMap<String, Value>>;

pub async fn wanted(store: &crate::store::Store) -> Wanted {
    store.get_value(KV).await.ok().flatten().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// A harness as its settings need it: the name to show, and its program here or why there is none.
pub struct Found {
    pub label: String,
    pub program: Result<std::path::PathBuf>,
}

/// Looks a harness up by id (`engines::harness_program`). It may search the disk, so it runs off
/// the async threads.
pub type Find = fn(&str) -> Found;

/// What `find` says of `harness`, asked on a blocking thread.
async fn found(find: Find, harness: &str) -> Found {
    let harness = harness.to_owned();
    let asked = harness.clone();
    tokio::task::spawn_blocking(move || find(&asked))
        .await
        .unwrap_or_else(|e| Found { label: harness, program: Err(e.into()) })
}

/// Runs the harness's program with `args`; its standard output, or why it failed.
async fn run(path: &std::path::Path, args: &[String]) -> Result<String> {
    let mut cmd = crate::proc::tokio_command(path);
    cmd.args(args).stdin(std::process::Stdio::null()).kill_on_drop(true);
    cmd.env("NO_COLOR", "1").env("CI", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.as_std_mut().creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = tokio::time::timeout(Duration::from_secs(60), cmd.output()).await.context("it took over a minute")??;
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let said = stderr.lines().chain(stdout.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("no output");
        bail!("{said}");
    }
    Ok(stdout)
}

/// The words of a command template, with `{key}` and `{value}` filled in. A value is one word
/// even with spaces in it.
fn words(template: &str, key: &str, value: &str) -> Vec<String> {
    template
        .split_whitespace()
        .map(|w| w.replace("{key}", key).replace("{value}", value))
        .collect()
}

/// A value as the harness's `set` takes it: lists as a JSON array, the rest as plain text.
fn as_arg(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Array(_) => value.to_string(),
        other => other.to_string(),
    }
}

/// The value at a dotted `path` under a section's JSON.
fn at<'a>(mut v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for p in path {
        v = v.get(p)?;
    }
    Some(v)
}

/// The harness's lists may be stored as a JSON array in a string (`hermes config set` does that).
fn normal(kind: SettingKind, value: Value) -> Value {
    match (kind, &value) {
        (SettingKind::OffList, Value::String(s)) if s.trim().starts_with('[') => serde_json::from_str(s).unwrap_or(json!([])),
        (SettingKind::OffList, Value::String(s)) if s.trim().is_empty() => json!([]),
        (SettingKind::OffList, Value::String(s)) => json!(s.split(',').map(str::trim).filter(|x| !x.is_empty()).collect::<Vec<_>>()),
        (SettingKind::OffList, Value::Null) => json!([]),
        _ => value,
    }
}

/// Reads the harness's settings on this machine: one ask per section of its config, together.
async fn read_here(harness: &str, program: Result<std::path::PathBuf>) -> Result<(Option<String>, Vec<HarnessSetting>)> {
    let (commands, mut settings) = catalog(harness).with_context(|| format!("Hub has no settings for {harness}"))?;
    let path = program?;
    let mut sections: Vec<String> = settings.iter().map(|s| s.key.split('.').next().unwrap_or("").to_owned()).collect();
    sections.dedup();
    sections.sort();
    sections.dedup();
    let version = {
        let path = path.clone();
        async move { run(&path, &["--version".into()]).await.ok().and_then(|o| crate::text::parse_version(&o)) }
    };
    let reads = sections.iter().map(|section| {
        let path = path.clone();
        let args = words(commands.get, section, "");
        async move { (section.clone(), run(&path, &args).await) }
    });
    let (version, reads) = tokio::join!(version, futures::future::join_all(reads));
    let mut values = BTreeMap::new();
    let mut failed = None;
    for (section, read) in reads {
        match read.and_then(|out| Ok(serde_json::from_str::<Value>(out.trim())?)) {
            Ok(v) => {
                values.insert(section, v);
            }
            // A section the config does not have reads as its defaults.
            Err(e) if format!("{e:#}").contains("not set") => {}
            Err(e) => failed = Some(format!("{e:#}")),
        }
    }
    if values.is_empty()
        && let Some(e) = failed
    {
        bail!("could not read its settings: {e}");
    }
    for s in &mut settings {
        let mut parts = s.key.split('.');
        let section = parts.next().unwrap_or("");
        let rest: Vec<&str> = parts.collect();
        if let Some(v) = values.get(section).and_then(|v| at(v, &rest)) {
            s.value = normal(s.kind, v.clone());
        }
        // An effort the config names that Hub does not list (`xhigh`) still shows as itself.
        if s.kind == SettingKind::Choice
            && !s.value.is_null()
            && !s.choices.iter().any(|c| c.value == s.value)
            && let Some(name) = s.value.as_str().map(str::to_owned).or_else(|| Some(s.value.to_string()))
        {
            s.choices.push(choice(s.value.clone(), &name, "Set outside Hub"));
        }
    }
    Ok((version, settings))
}

/// The harness's settings page for this machine.
pub async fn read(store: &crate::store::Store, find: Find, harness: &str) -> HarnessSettings {
    let Found { label, program } = found(find, harness).await;
    let wanted = wanted(store).await;
    let mine = wanted.get(harness);
    match read_here(harness, program).await {
        Ok((version, mut settings)) => {
            for s in &mut settings {
                s.shared = mine.is_some_and(|m| m.contains_key(&s.key));
            }
            HarnessSettings { harness: harness.into(), label, version, settings, problem: None }
        }
        Err(e) => {
            let settings = catalog(harness).map(|(_, s)| s).unwrap_or_default();
            HarnessSettings { harness: harness.into(), label, version: None, settings, problem: Some(format!("{e:#}")) }
        }
    }
}

/// Sets one setting in the harness's config here. `Null` or the default puts it back to the
/// harness's own default.
async fn set_here(find: Find, harness: &str, key: &str, value: &Value) -> Result<()> {
    let (commands, settings) = catalog(harness).with_context(|| format!("Hub has no settings for {harness}"))?;
    let s = settings.iter().find(|s| s.key == key).with_context(|| format!("{harness} has no setting {key} in Hub"))?;
    let path = found(find, harness).await.program?;
    let empty = value.is_null() || value.as_str() == Some("") || value.as_array().is_some_and(|a| a.is_empty());
    let args = if empty && s.default != *value && !s.default.is_null() && s.default != json!([]) && s.default != json!("") {
        // A cleared value whose default is something else: the default, said out loud.
        words(commands.set, key, &as_arg(&s.default))
    } else if empty {
        words(commands.unset, key, "")
    } else {
        words(commands.set, key, &as_arg(value))
    };
    match run(&path, &args).await {
        Ok(_) => Ok(()),
        // Unsetting a key the config never had is already done.
        Err(e) if empty && format!("{e:#}").contains("not set") => Ok(()),
        Err(e) => Err(e.context(format!("could not set {key}"))),
    }
}

/// Changes a setting here and keeps it for the other machines (the caller then says
/// `engines::share_soon`).
pub async fn write(store: &crate::store::Store, find: Find, harness: &str, key: &str, value: Value) -> Result<()> {
    set_here(find, harness, key, &value).await?;
    let mut all = wanted(store).await;
    all.entry(harness.to_owned()).or_default().insert(key.to_owned(), value.clone());
    store.set_value(KV, &serde_json::to_string(&all)?).await?;
    APPLIED.locked().entry(harness.to_owned()).or_default().insert(key.to_owned(), value);
    Ok(())
}

/// What `take_in` last set here, so the same values coming every minute are not set again.
static APPLIED: LazyLock<Mutex<Wanted>> = LazyLock::new(Default::default);

/// Sets what another machine changed from Hub, where it differs from what was set here last.
/// A harness that is not installed here is left for when it is.
pub async fn take_in(find: Find, from: Wanted) {
    for (harness, keys) in from {
        if catalog(&harness).is_none() {
            continue;
        }
        let todo: Vec<(String, Value)> = {
            let applied = APPLIED.locked();
            keys.into_iter().filter(|(k, v)| applied.get(&harness).and_then(|a| a.get(k)) != Some(v)).collect()
        };
        if todo.is_empty() {
            continue;
        }
        let Ok(program) = found(find, &harness).await.program else {
            continue;
        };
        // What it holds now, so only real differences are written.
        let now = read_here(&harness, Ok(program)).await.map(|(_, s)| s).unwrap_or_default();
        for (key, value) in todo {
            let same = now.iter().any(|s| s.key == key && (s.value == value || (s.value == s.default && value.is_null())));
            if !same && let Err(e) = set_here(find, &harness, &key, &value).await {
                tracing::warn!(%harness, %key, "harness settings: {e:#}");
                continue;
            }
            APPLIED.locked().entry(harness.clone()).or_default().insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermes_has_settings_and_others_not() {
        assert!(has_settings("hermes"));
        assert!(!has_settings("claude"));
        let (_, settings) = catalog("hermes").unwrap();
        assert!(settings.iter().any(|s| s.key == "agent.disabled_toolsets" && s.kind == SettingKind::OffList));
    }

    #[test]
    fn lists_read_in_every_shape_hermes_stores_them() {
        assert_eq!(normal(SettingKind::OffList, json!(r#"["web","browser"]"#)), json!(["web", "browser"]));
        assert_eq!(normal(SettingKind::OffList, json!("web, browser")), json!(["web", "browser"]));
        assert_eq!(normal(SettingKind::OffList, json!(["web"])), json!(["web"]));
        assert_eq!(normal(SettingKind::OffList, Value::Null), json!([]));
    }

    #[test]
    fn values_go_to_the_command_as_one_word() {
        assert_eq!(as_arg(&json!(["web", "browser"])), r#"["web","browser"]"#);
        assert_eq!(as_arg(&json!(true)), "true");
        assert_eq!(as_arg(&json!(0.5)), "0.5");
        assert_eq!(as_arg(&json!("high")), "high");
        assert_eq!(words("config set {key} {value}", "agent.max_turns", "60"), ["config", "set", "agent.max_turns", "60"]);
        // A list with a space in it stays one argument.
        assert_eq!(words("config set {key} {value}", "k", r#"["a", "b"]"#).len(), 4);
    }

    #[test]
    fn nested_keys_are_found_under_their_section() {
        let agent = json!({ "reasoning_effort": "max", "nested": { "deep": 1 } });
        assert_eq!(at(&agent, &["reasoning_effort"]), Some(&json!("max")));
        assert_eq!(at(&agent, &["nested", "deep"]), Some(&json!(1)));
        assert_eq!(at(&agent, &["missing"]), None);
    }
}
