//! The models an engine offers, as its latest session said (the ACP `config_options` entry of
//! category `model`), and picking one by name. Read from the store, so a side job (`children`) and
//! Hub's tools (`hub_tools`) both ask it without the other.

use anyhow::{Context, Result, bail};

use crate::store::Store;

/// The model setting an engine offers and its choices, `(config id, [(value, name)])`, as its
/// latest session said.
pub async fn offered(store: &Store, engine: &str) -> Option<(String, Vec<(String, String)>)> {
    let threads = store.threads().await.ok()?;
    let mut states = store.latest_session_states().await.ok()?;
    let updated = |id: &str| threads.iter().find(|t| t.id == id).filter(|t| t.engine == engine).map(|t| t.updated_at);
    states.retain(|(id, _)| updated(id).is_some());
    states.sort_by_key(|(id, _)| std::cmp::Reverse(updated(id)));
    states.iter().find_map(|(_, state)| {
        let option = state["config_options"].as_array()?.iter().find(|o| o["category"] == "model")?;
        let choices = option["options"]
            .as_array()?
            .iter()
            .filter_map(|c| Some((c["value"].as_str()?.to_owned(), c["name"].as_str().unwrap_or_default().to_owned())))
            .collect();
        Some((option["id"].as_str()?.to_owned(), choices))
    })
}

/// The setting that picks `model` on `engine`: an exact value, else a value or name containing it.
pub async fn pick(store: &Store, engine: &str, model: &str) -> Result<(String, String)> {
    let (id, choices) = offered(store, engine)
        .await
        .with_context(|| format!("engine {engine} has not said which models it offers yet; leave model out"))?;
    let want = model.trim().to_lowercase();
    let found = choices.iter().find(|(v, _)| v.to_lowercase() == want).or_else(|| {
        choices.iter().find(|(v, n)| v.to_lowercase().contains(&want) || n.to_lowercase().contains(&want))
    });
    match found {
        Some((value, _)) => Ok((id, value.clone())),
        None => {
            let names: Vec<&str> = choices.iter().map(|(v, _)| v.as_str()).collect();
            bail!("{engine} offers no model {model:?}; it offers {}", names.join(", "))
        }
    }
}
