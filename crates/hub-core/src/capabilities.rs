//! What each model can do, for Auto to route by: a public list of models (LiteLLM's, which tracks
//! every provider's models with their context size and abilities) matched to the models the engines
//! offer. The list is fetched at most once a day and kept in `<data dir>/models/catalog.json`.
//! `<data dir>/models/overrides.json` corrects it: `{ "codex/gpt-6.1-sol": { "catalog":
//! "gpt-6.1-sol", "context": 922000, "web_search": true } }` names the entry a model is, or sets
//! what it can do, by engine and model value.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::hub::Hub;
use crate::model::{EngineModels, ModelCaps, ModelTable, now_ms};

pub const SOURCE: &str = "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json";

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The providers whose models engines offer; the rest of the list stays out of the cache.
const PROVIDERS: [&str; 4] = ["anthropic", "openai", "deepseek", "gemini"];

/// One model in the public list, with only what Hub reads.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub provider: String,
    pub context: Option<u64>,
    pub image_in: bool,
    pub pdf_in: bool,
    pub tools: bool,
    pub reasoning: bool,
    pub web_search: bool,
    /// US dollars per million output tokens. Lists cached before it was read have none.
    #[serde(default)]
    pub cost: Option<f64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Catalog {
    fetched_at: i64,
    models: BTreeMap<String, Entry>,
}

fn folder(data_dir: &Path) -> PathBuf {
    data_dir.join("models")
}

/// Reads LiteLLM's list into entries: chat models of the providers engines use, one per name.
pub fn parse(raw: &Value) -> BTreeMap<String, Entry> {
    let mut out = BTreeMap::new();
    for (name, m) in raw.as_object().into_iter().flatten() {
        let provider = m["litellm_provider"].as_str().unwrap_or_default();
        let mode = m["mode"].as_str().unwrap_or_default();
        if !PROVIDERS.contains(&provider) || !matches!(mode, "chat" | "responses") || name.contains('/') {
            continue;
        }
        let yes = |key: &str| m[key].as_bool().unwrap_or(false);
        out.insert(
            name.clone(),
            Entry {
                provider: provider.to_owned(),
                context: m["max_input_tokens"].as_u64(),
                image_in: yes("supports_vision") || yes("supports_image_input"),
                pdf_in: yes("supports_pdf_input"),
                tools: yes("supports_function_calling"),
                reasoning: yes("supports_reasoning"),
                web_search: yes("supports_web_search"),
                cost: m["output_cost_per_token"].as_f64().map(|c| c * 1_000_000.0),
            },
        );
    }
    out
}

/// The words of a model's id or name, lowercase: "Sol 6.1" and "gpt-6.1-sol" both give `sol`,
/// `6.1`. Words that say nothing about which model it is are left out.
fn words(text: &str) -> Vec<String> {
    const NOISE: [&str; 12] = ["default", "latest", "recommended", "high", "medium", "low", "xhigh", "minimal", "1m", "fast", "thinking", "preview"];
    text.to_lowercase()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
        .map(|w| w.trim_matches('.').to_owned())
        .filter(|w| !w.is_empty() && !NOISE.contains(&w.as_str()))
        .collect()
}

/// A model name's version, as numbers to compare: "claude-opus-5-5" is [5, 5], "gpt-6.1-sol" [6, 1].
fn version(name: &str) -> Vec<u32> {
    name.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty() && s.len() < 5).filter_map(|s| s.parse().ok()).collect()
}

/// The entry an engine's model is: every word of its value and name found in the entry's name,
/// the newest, then the one with the fewest other words ("gpt-5.5" over "gpt-5.5-pro"). "opus"
/// is the newest Claude Opus; "Luna 6"
/// is gpt-6-luna and not gpt-5.6-luna, since "6" must be a word of its own. Dated names are left
/// for the plain one.
pub fn find(models: &BTreeMap<String, Entry>, value: &str, name: &str) -> Option<String> {
    let mut want = words(value);
    for w in words(name) {
        if !want.contains(&w) {
            want.push(w);
        }
    }
    // A value like "gpt-6.1-sol" says it all; the name may be prose ("Sol 6.1 · fastest").
    let exact = value.trim().to_lowercase();
    if models.contains_key(&exact) {
        return Some(exact);
    }
    let by_value = words(value);
    let candidates = |want: &[String]| -> Option<String> {
        if want.is_empty() {
            return None;
        }
        models
            .keys()
            .filter(|k| !dated(k))
            .filter(|k| {
                let have = words(k);
                want.iter().all(|w| have.contains(w))
            })
            .min_by(|a, b| {
                let extra = |k: &str| words(k).len();
                version(b).cmp(&version(a)).then_with(|| extra(a).cmp(&extra(b)))
            })
            .cloned()
    };
    candidates(&want).or_else(|| candidates(&by_value))
}

/// A snapshot's name, like "claude-haiku-4-5-20251001" or "gpt-5.5-2026-04-23".
fn dated(name: &str) -> bool {
    let parts: Vec<&str> = name.split('-').collect();
    let digits = |p: &&str, n: usize| p.len() == n && p.chars().all(|c| c.is_ascii_digit());
    parts.iter().any(|p| digits(p, 8)) || parts.windows(3).any(|w| digits(&w[0], 4) && digits(&w[1], 2) && digits(&w[2], 2))
}

impl Hub {
    /// The table as it stands, fetching the list first when there is none or it is a day old.
    /// A failed fetch keeps the last list and says why.
    pub async fn model_table(self: &Arc<Self>, refresh: bool) -> Result<ModelTable> {
        let dir = folder(&self.config.data_dir);
        let cached = read_catalog(&dir);
        let stale = cached.as_ref().is_none_or(|c| now_ms() - c.fetched_at > DAY_MS);
        let mut error = None;
        let catalog = if refresh || stale {
            match fetch().await {
                Ok(models) => {
                    let catalog = Catalog { fetched_at: now_ms(), models };
                    if let Err(e) = write_catalog(&dir, &catalog) {
                        tracing::warn!("keeping the model list: {e:#}");
                    }
                    Some(catalog)
                }
                Err(e) => {
                    error = Some(format!("{e:#}"));
                    cached
                }
            }
        } else {
            cached
        };
        let overrides_path = dir.join("overrides.json");
        let overrides = read_overrides(&overrides_path);
        let models = catalog.as_ref().map(|c| &c.models);
        let empty = BTreeMap::new();
        let mut engines = Vec::new();
        for engine in self.config.engines.keys() {
            let offered = self.offered_models(engine).await;
            let rows: Vec<ModelCaps> = offered
                .into_iter()
                .map(|(value, name)| caps(models.unwrap_or(&empty), &overrides, engine, &value, &name))
                .collect();
            let draws = self.config.side_jobs.get("image") == Some(engine);
            let fast = fastest(&rows, &overrides);
            engines.push(EngineModels { engine: engine.clone(), draws, models: rows, fast });
        }
        Ok(ModelTable {
            source: SOURCE.into(),
            fetched_at: catalog.as_ref().map(|c| c.fetched_at),
            known: catalog.as_ref().map_or(0, |c| c.models.len() as u32),
            error,
            overrides: overrides_path.display().to_string(),
            engines,
        })
    }

    /// The model and effort settings `engine` offered in its latest session, as the agent sent
    /// them: what the phone shows before a new thread has run, since Hub already knows it.
    pub async fn known_options(&self, engine: &str) -> Vec<Value> {
        let (Ok(threads), Ok(states)) = (self.store.threads().await, self.store.latest_session_states().await) else {
            return Vec::new();
        };
        let mut states: Vec<(i64, Value)> = states
            .into_iter()
            .filter_map(|(id, state)| threads.iter().find(|t| t.id == id && t.engine == engine).map(|t| (t.updated_at, state)))
            .collect();
        states.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        states
            .iter()
            .find_map(|(_, state)| {
                let kept: Vec<Value> = state["config_options"]
                    .as_array()?
                    .iter()
                    .filter(|o| matches!(o["category"].as_str(), Some("model" | "thought_level")))
                    .cloned()
                    .collect();
                (!kept.is_empty()).then_some(kept)
            })
            .unwrap_or_default()
    }

    /// The models `engine` offered in its latest session, as value and name.
    async fn offered_models(&self, engine: &str) -> Vec<(String, String)> {
        let (Ok(threads), Ok(states)) = (self.store.threads().await, self.store.latest_session_states().await) else {
            return Vec::new();
        };
        let mut states: Vec<(i64, Value)> = states
            .into_iter()
            .filter_map(|(id, state)| threads.iter().find(|t| t.id == id && t.engine == engine).map(|t| (t.updated_at, state)))
            .collect();
        states.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        states
            .iter()
            .find_map(|(_, state)| {
                let option = state["config_options"].as_array()?.iter().find(|o| o["category"] == "model")?;
                // Grouped choices are flattened.
                let choices = option["options"].as_array()?.iter().flat_map(|c| c["options"].as_array().cloned().unwrap_or_else(|| vec![c.clone()]));
                Some(
                    choices
                        .filter_map(|c| Some((c["value"].as_str()?.to_owned(), c["name"].as_str().unwrap_or_default().to_owned())))
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_default()
    }
}

fn caps(models: &BTreeMap<String, Entry>, overrides: &BTreeMap<String, Value>, engine: &str, value: &str, name: &str) -> ModelCaps {
    let fix = overrides.get(&format!("{engine}/{value}"));
    let catalog = fix.and_then(|f| f["catalog"].as_str().map(str::to_owned)).or_else(|| find(models, value, name));
    let entry = catalog.as_ref().and_then(|c| models.get(c)).cloned().unwrap_or_default();
    let mut row = ModelCaps {
        engine: engine.to_owned(),
        value: value.to_owned(),
        name: if name.is_empty() { value.to_owned() } else { name.to_owned() },
        found: catalog.as_ref().is_some_and(|c| models.contains_key(c)),
        catalog,
        context: entry.context,
        image_in: entry.image_in,
        pdf_in: entry.pdf_in,
        tools: entry.tools,
        reasoning: entry.reasoning,
        web_search: entry.web_search,
        cost: entry.cost,
        corrected: fix.is_some(),
    };
    if let Some(fix) = fix {
        let flag = |key: &str, into: &mut bool| {
            if let Some(v) = fix[key].as_bool() {
                *into = v;
            }
        };
        flag("image_in", &mut row.image_in);
        flag("pdf_in", &mut row.pdf_in);
        flag("tools", &mut row.tools);
        flag("reasoning", &mut row.reasoning);
        flag("web_search", &mut row.web_search);
        if let Some(c) = fix["context"].as_u64() {
            row.context = Some(c);
        }
        if let Some(c) = fix["cost"].as_f64() {
            row.cost = Some(c);
        }
    }
    row
}

/// The model Auto moves a quick question to: one the overrides file marks `"fast": true`, else
/// the cheapest by the list, if a dearer one is offered too. Without costs (a list cached before
/// Hub read them, or models it does not know) a name that says it is small: Haiku, mini, Flash.
pub fn fastest(rows: &[ModelCaps], overrides: &BTreeMap<String, Value>) -> Option<String> {
    if let Some(r) = rows.iter().find(|r| overrides.get(&format!("{}/{}", r.engine, r.value)).is_some_and(|f| f["fast"] == true)) {
        return Some(r.value.clone());
    }
    let priced: Vec<(&ModelCaps, f64)> = rows.iter().filter_map(|r| Some((r, r.cost?))).collect();
    let cheapest = priced.iter().min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((r, cost)) = cheapest
        && priced.iter().any(|(_, c)| c > cost)
    {
        return Some(r.value.clone());
    }
    const SMALL: [&str; 5] = ["haiku", "mini", "nano", "flash", "lite"];
    rows.iter()
        .find(|r| SMALL.iter().any(|w| words(&r.value).iter().chain(words(&r.name).iter()).any(|x| x == w)))
        .map(|r| r.value.clone())
}

async fn fetch() -> Result<BTreeMap<String, Entry>> {
    let client = reqwest::Client::builder().user_agent("hubd").timeout(Duration::from_secs(60)).build()?;
    let raw: Value = client.get(SOURCE).send().await?.error_for_status()?.json().await.context("reading the model list")?;
    let models = parse(&raw);
    anyhow::ensure!(!models.is_empty(), "the model list had no models Hub knows");
    Ok(models)
}

fn read_catalog(dir: &Path) -> Option<Catalog> {
    serde_json::from_slice(&std::fs::read(dir.join("catalog.json")).ok()?).ok()
}

fn write_catalog(dir: &Path, catalog: &Catalog) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("catalog.json"), serde_json::to_vec(catalog)?)?;
    // An empty overrides file, so there is something to open and fill in.
    let overrides = dir.join("overrides.json");
    if !overrides.exists() {
        std::fs::write(&overrides, serde_json::to_vec_pretty(&json!({}))?)?;
    }
    Ok(())
}

fn read_overrides(path: &Path) -> BTreeMap<String, Value> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> BTreeMap<String, Entry> {
        let raw = json!({
            "claude-opus-4-8": { "litellm_provider": "anthropic", "mode": "chat", "max_input_tokens": 1000000, "supports_vision": true },
            "claude-opus-5-5": { "litellm_provider": "anthropic", "mode": "chat", "max_input_tokens": 1000000, "supports_vision": true, "supports_reasoning": true },
            "claude-sonnet-5-5": { "litellm_provider": "anthropic", "mode": "chat" },
            "claude-haiku-4-5-20251001": { "litellm_provider": "anthropic", "mode": "chat" },
            "claude-haiku-4-5": { "litellm_provider": "anthropic", "mode": "chat" },
            "gpt-5.6-luna": { "litellm_provider": "openai", "mode": "chat" },
            "gpt-6-luna": { "litellm_provider": "openai", "mode": "chat", "max_input_tokens": 922000 },
            "gpt-6.1-sol": { "litellm_provider": "openai", "mode": "chat", "supports_web_search": true },
            "gpt-6.1-sol-2026-09-01": { "litellm_provider": "openai", "mode": "chat" },
            "gpt-6.1-sol-pro": { "litellm_provider": "openai", "mode": "chat" },
            "gpt-image-1": { "litellm_provider": "openai", "mode": "image_generation" },
            "azure/gpt-6.1-sol": { "litellm_provider": "openai", "mode": "chat" },
            "llama-9": { "litellm_provider": "meta", "mode": "chat" },
        });
        parse(&raw)
    }

    #[test]
    fn only_chat_models_of_known_providers_are_kept() {
        let l = list();
        assert!(l.contains_key("gpt-6.1-sol") && !l.contains_key("gpt-image-1") && !l.contains_key("llama-9"));
        assert!(!l.contains_key("azure/gpt-6.1-sol"));
        assert_eq!(l["gpt-6-luna"].context, Some(922000));
        assert!(l["claude-opus-5-5"].image_in && l["claude-opus-5-5"].reasoning);
    }

    #[test]
    fn engine_models_are_matched_by_their_words() {
        let l = list();
        assert_eq!(find(&l, "gpt-6.1-sol", "Sol 6.1").as_deref(), Some("gpt-6.1-sol"));
        assert_eq!(find(&l, "luna", "Luna 6").as_deref(), Some("gpt-6-luna"));
        assert_eq!(find(&l, "opus", "Opus").as_deref(), Some("claude-opus-5-5"));
        assert_eq!(find(&l, "haiku", "Haiku").as_deref(), Some("claude-haiku-4-5"));
        assert_eq!(find(&l, "sonnet[1m]", "Sonnet (1M context)").as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(find(&l, "default", "Default (recommended)"), None);
    }

    #[test]
    fn the_fast_model_is_the_cheapest_unless_one_is_marked() {
        let row = |value: &str, cost: Option<f64>| ModelCaps { engine: "e".into(), value: value.into(), name: value.into(), catalog: None, found: true, corrected: false, context: None, image_in: false, pdf_in: false, tools: true, reasoning: false, web_search: false, cost };
        let none = BTreeMap::new();
        let rows = [row("opus", Some(25.0)), row("sonnet", Some(15.0)), row("haiku", Some(5.0))];
        assert_eq!(fastest(&rows, &none).as_deref(), Some("haiku"));
        let mut fixes = BTreeMap::new();
        fixes.insert("e/sonnet".to_owned(), json!({ "fast": true }));
        assert_eq!(fastest(&rows, &fixes).as_deref(), Some("sonnet"));
        // Without costs, by name; with one model only, none.
        assert_eq!(fastest(&[row("default", None), row("haiku", None)], &none).as_deref(), Some("haiku"));
        assert_eq!(fastest(&[row("gpt-6.1-sol", Some(30.0))], &none), None);
    }

    #[test]
    fn an_override_names_the_entry_and_sets_what_it_can_do() {
        let l = list();
        let mut fixes = BTreeMap::new();
        fixes.insert("claude/default".to_owned(), json!({ "catalog": "claude-opus-5-5", "web_search": true }));
        let row = caps(&l, &fixes, "claude", "default", "Default (recommended)");
        assert_eq!(row.catalog.as_deref(), Some("claude-opus-5-5"));
        assert!(row.found && row.corrected && row.web_search && row.image_in);
        let plain = caps(&l, &BTreeMap::new(), "claude", "default", "Default (recommended)");
        assert!(!plain.found && plain.catalog.is_none());
    }
}
