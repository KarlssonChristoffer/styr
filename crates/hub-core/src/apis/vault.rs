//! Settings > Key vault: every key in the vault (`secrets`) by its end, its dates, and what uses
//! it (engines, API connections, their projects and Hub's own features). Lives with the API
//! connections, which hold most of what it reports, so the vault itself knows nothing of `Hub`.

use std::collections::BTreeMap;

use anyhow::Result;

use crate::hub::Hub;
use crate::model::{KeyUse, KeyVault, VaultKey};

impl Hub {
    /// Settings > Key vault: every key by its end, its dates, and what uses it.
    pub async fn key_vault(&self) -> Result<KeyVault> {
        let values = self.secrets.values().await?;
        let names = self.secrets.store_names().await?;
        let meta = self.secrets.meta().await;
        let prefs = self.api_prefs().await?;
        let checks = self.api_state().await.map(|s| s.status).unwrap_or_default();
        // Hub's own decisions (`apis::FEATURES`) and the connection that makes each.
        let helping = [
            ("turns", "Reads where each finished turn left the work"),
            ("focus", "Checks new threads against the stage"),
        ];
        let mut feature_helper = BTreeMap::new();
        for (feature, _) in &helping {
            feature_helper.insert(*feature, self.helper(feature).await);
        }
        let mut keys: Vec<VaultKey> = names
            .iter()
            .map(|name| {
                let m = meta.get(name).cloned().unwrap_or_default();
                let mut uses = Vec::new();
                for (engine, cfg) in &self.config.engines {
                    if cfg.key_names().contains(name) {
                        uses.push(KeyUse { kind: "engine".into(), label: engine.clone(), target: engine.clone(), via: None, ok: None });
                    }
                }
                for c in prefs.connections.iter().filter(|c| &c.secret == name) {
                    let check = checks.iter().find(|s| s.name == c.name).and_then(|s| s.check.as_ref());
                    uses.push(KeyUse {
                        kind: "api".into(),
                        label: c.label.clone(),
                        target: c.name.clone(),
                        via: None,
                        ok: check.map(|c| c.ok),
                    });
                    for (feature, label) in &helping {
                        if feature_helper.get(*feature).is_some_and(|h| h.as_deref() == Some(c.name.as_str())) {
                            uses.push(KeyUse {
                                kind: "hub".into(),
                                label: (*label).into(),
                                target: c.name.clone(),
                                via: Some(c.label.clone()),
                                ok: None,
                            });
                        }
                    }
                    for p in &c.projects {
                        uses.push(KeyUse { kind: "project".into(), label: p.clone(), target: p.clone(), via: Some(c.label.clone()), ok: None });
                    }
                }
                VaultKey {
                    name: name.clone(),
                    hint: values.get(name).map(|v| crate::prefs::hint(v)),
                    created: (m.created > 0).then_some(m.created),
                    changed: (m.changed > 0).then_some(m.changed),
                    used: m.used,
                    uses,
                    engine_key: super::ENGINE_KEYS.contains(&name.as_str()),
                    in_env: std::env::var(name).is_ok_and(|v| !v.trim().is_empty()),
                }
            })
            .collect();
        keys.sort_by(|a, b| a.name.cmp(&b.name));
        let elsewhere = crate::prefs::env_keys().into_iter().filter(|k| !names.contains(&k.name)).collect();
        Ok(KeyVault { kept_in: self.secrets.kept_in(), keys, elsewhere })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    #[tokio::test]
    async fn the_vault_shows_what_uses_each_key_and_never_the_value() -> Result<()> {
        let data = tempfile::tempdir()?;
        let mut config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
        if let Some(engine) = config.engines.values_mut().next() {
            engine.secrets.push("WEATHER_API_KEY".into());
        }
        let engine = config.engines.keys().next().cloned();
        let hub = Hub::new(Store::in_memory()?, config);
        hub.secrets.create("WEATHER_API_KEY", "wx-abcdefghijklmnop1234").await?;
        hub.set_api_prefs(crate::apis::ApiPrefs {
            connections: vec![crate::apis::ApiConnection {
                name: "weather".into(),
                kind: "custom".into(),
                label: "Weather".into(),
                url: "https://api.example.com".into(),
                secret: "WEATHER_API_KEY".into(),
                model: None,
                test_path: String::new(),
                docs: None,
                daily_tokens: 0,
                projects: vec!["D:\\Dev\\site".into()],
                auth: Default::default(),
                client_id: None,
                credit: None,
            }],
            helper: None,
            off: Vec::new(),
            sure: Default::default(),
        })
        .await?;

        let vault = hub.key_vault().await?;
        let key = &vault.keys[0];
        assert_eq!(key.hint.as_deref(), Some("…1234"));
        let kinds: Vec<&str> = key.uses.iter().map(|u| u.kind.as_str()).collect();
        assert!(kinds.contains(&"api") && kinds.contains(&"project"));
        assert_eq!(kinds.contains(&"engine"), engine.is_some());
        assert_eq!(key.uses.iter().find(|u| u.kind == "project").unwrap().via.as_deref(), Some("Weather"));
        assert!(!serde_json::to_string(&vault)?.contains("abcdefghijklmnop"));
        Ok(())
    }
}
