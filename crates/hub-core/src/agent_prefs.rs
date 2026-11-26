//! How Hub starts every agent, set once in Settings > Engines: when it compacts a long
//! conversation and how it writes. Kept in the store (kv `agent_prefs`) and handed to the engine
//! by `connections::apply_agent_prefs`, beside what is switched off: Claude Code takes environment
//! variables and settings for each session; an engine with no way to be told keeps its own default.
//!
//! Also finds models served on a machine itself (Ollama, LM Studio), for Settings to show.

use std::time::Duration;

use anyhow::Result;
use serde_json::Value;

use crate::ctx::Ctx;
use crate::model::{AgentPrefs, LocalModels, LocalServer};

const PREFS: &str = "agent_prefs";
const PREFS_AT: &str = "agent_prefs_at";

/// Claude Code's output styles besides its default.
pub const STYLES: [&str; 2] = ["Explanatory", "Learning"];

pub async fn prefs(ctx: &Ctx<'_>) -> Result<AgentPrefs> {
    Ok(ctx.store.get_value(PREFS).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

pub async fn set_prefs(ctx: &Ctx<'_>, prefs: AgentPrefs) -> Result<AgentPrefs> {
    save(ctx, prefs, crate::model::now_ms()).await
}

async fn save(ctx: &Ctx<'_>, mut prefs: AgentPrefs, at: i64) -> Result<AgentPrefs> {
    prefs.compact_at = prefs.compact_at.map(|p| p.clamp(10, 95));
    prefs.output_style = prefs.output_style.filter(|s| STYLES.contains(&s.as_str()));
    for p in prefs.projects.values_mut() {
        p.compact = p.compact.take().filter(|c| compact(c).is_some());
        p.output_style = p.output_style.take().filter(|s| s.is_empty() || STYLES.contains(&s.as_str()));
    }
    prefs.projects.retain(|folder, p| !folder.trim().is_empty() && (p.compact.is_some() || p.output_style.is_some()));
    ctx.store.set_value(PREFS, &serde_json::to_string(&prefs)?).await?;
    ctx.store.set_value(PREFS_AT, &at.to_string()).await?;
    Ok(prefs)
}

/// When they were last set, on any machine; 0 when never.
async fn set_at(ctx: &Ctx<'_>) -> i64 {
    ctx.store.get_value(PREFS_AT).await.ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// What travels with the engines to every other machine (`engine_share::Shipment`), so a thread
/// started there works the same way. None when they were never set here.
pub async fn outgoing(ctx: &Ctx<'_>) -> Option<Shared> {
    let at = set_at(ctx).await;
    if at == 0 {
        return None;
    }
    Some(Shared { prefs: prefs(ctx).await.ok()?, at })
}

/// Takes in what another machine sent when it was set later than what is here; the newest wins,
/// so a change made on any machine reaches all of them and an echo changes nothing.
pub async fn take_in(ctx: &Ctx<'_>, shared: Shared) -> Result<bool> {
    if shared.at <= set_at(ctx).await {
        return Ok(false);
    }
    save(ctx, shared.prefs, shared.at).await?;
    Ok(true)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Shared {
    pub prefs: AgentPrefs,
    /// When they were set, in ms.
    pub at: i64,
}

/// A project's compaction word as (off, at): `engine` is (false, None), `never` (true, None).
fn compact(word: &str) -> Option<(bool, Option<u8>)> {
    match word {
        "engine" => Some((false, None)),
        "never" => Some((true, None)),
        n => n.parse::<u8>().ok().map(|p| (false, Some(p.clamp(10, 95)))),
    }
}

/// What a thread in `folder` starts with: everywhere's prefs, with the project it is in (the
/// longest folder that holds it) laid over them.
pub fn for_folder(prefs: &AgentPrefs, folder: &str) -> AgentPrefs {
    let norm = |p: &str| p.trim().trim_end_matches(['/', '\\']).replace('\\', "/").to_lowercase();
    let at = norm(folder);
    let own = prefs
        .projects
        .iter()
        .filter(|(p, _)| {
            let p = norm(p);
            !p.is_empty() && (at == p || at.starts_with(&format!("{p}/")))
        })
        .max_by_key(|(p, _)| p.len())
        .map(|(_, own)| own.clone());
    let mut out = AgentPrefs { projects: Default::default(), ..prefs.clone() };
    if let Some(own) = own {
        if let Some((off, at)) = own.compact.as_deref().and_then(compact) {
            out.auto_compact_off = off;
            out.compact_at = at;
        }
        if let Some(style) = own.output_style {
            out.output_style = Some(style).filter(|s| !s.is_empty());
        }
    }
    out
}

/// The local model servers Hub knows, where they answer by default.
fn servers() -> Vec<LocalServer> {
    let one = |id: &str, label: &str, url: &str, get_at: &str| LocalServer {
        id: id.into(),
        label: label.into(),
        url: url.into(),
        running: false,
        models: Vec::new(),
        get_at: get_at.into(),
    };
    vec![
        one("ollama", "Ollama", "http://127.0.0.1:11434", "https://ollama.com/download"),
        one("lmstudio", "LM Studio", "http://127.0.0.1:1234", "https://lmstudio.ai"),
    ]
}

/// Asks each local server on this machine which models it has. One that does not answer within
/// two seconds is not running.
pub async fn local_models() -> LocalModels {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(2)).no_proxy().build().unwrap_or_default();
    let mut out = Vec::new();
    for mut s in servers() {
        // Both speak the OpenAI-style list; Ollama's own list has more of its models' names.
        let path = if s.id == "ollama" { "/api/tags" } else { "/v1/models" };
        if let Ok(r) = http.get(format!("{}{path}", s.url)).send().await
            && r.status().is_success()
        {
            s.running = true;
            s.models = r.json::<Value>().await.map(|v| model_names(&v)).unwrap_or_default();
        }
        out.push(s);
    }
    LocalModels { servers: out }
}

/// Model names from Ollama's `{"models":[{"name"}]}` or the OpenAI-style `{"data":[{"id"}]}`.
fn model_names(v: &Value) -> Vec<String> {
    let from = |list: &Value, key: &str| {
        list.as_array().into_iter().flatten().filter_map(|m| m[key].as_str().map(str::to_owned)).collect::<Vec<_>>()
    };
    let mut names = from(&v["models"], "name");
    names.extend(from(&v["data"], "id"));
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_project_differs_where_it_says() {
        let mut prefs = AgentPrefs { compact_at: Some(80), output_style: Some("Learning".into()), ..Default::default() };
        prefs.projects.insert(r"D:\Dev\knegarn".into(), crate::model::ProjectPrefs { compact: Some("never".into()), output_style: Some(String::new()) });
        prefs.projects.insert(r"D:\Dev\knegarn\docs".into(), crate::model::ProjectPrefs { compact: Some("60".into()), ..Default::default() });

        let in_it = for_folder(&prefs, r"d:\dev\Knegarn\src");
        assert!(in_it.auto_compact_off);
        assert_eq!(in_it.compact_at, None);
        assert_eq!(in_it.output_style, None, "empty is the agent's own style");

        let deeper = for_folder(&prefs, r"D:\Dev\knegarn\docs");
        assert_eq!((deeper.auto_compact_off, deeper.compact_at), (false, Some(60)));
        assert_eq!(deeper.output_style.as_deref(), Some("Learning"), "the nearest project only sets compaction");

        let elsewhere = for_folder(&prefs, r"D:\Dev\knegarn-old");
        assert_eq!((elsewhere.compact_at, elsewhere.output_style.as_deref()), (Some(80), Some("Learning")));
        assert!(elsewhere.projects.is_empty());
    }

    #[test]
    fn reads_both_model_lists() {
        assert_eq!(model_names(&json!({ "models": [{ "name": "qwen3:8b" }] })), vec!["qwen3:8b"]);
        assert_eq!(model_names(&json!({ "data": [{ "id": "gemma-3" }] })), vec!["gemma-3"]);
    }
}
