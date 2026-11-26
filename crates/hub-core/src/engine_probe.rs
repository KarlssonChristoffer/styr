//! Trying an engine before it is saved or after it arrived from another machine: asks the
//! provider about the key, then starts the agent and asks it for a session, which spends nothing.
//! Above both the engine catalog (`engines.rs`, data only) and the ACP client (`engine/acp`), so
//! the catalog does not need the client.

use std::time::Duration;

use crate::config::EngineConfig;
use crate::engines::{EngineTest, check_key, harnesses, installed, providers};
use crate::locked::ResultExt;

/// Tries an engine without saving it: checks the key with the provider, then starts the agent
/// and asks it for a session, which spends nothing.
pub async fn test(name: &str, engine: &EngineConfig, secrets: Vec<(String, String)>) -> EngineTest {
    let mut test = EngineTest {
        ok: false,
        message: String::new(),
        fix: None,
        key_ok: None,
        agent: None,
        models: Vec::new(),
        current: None,
        model_found: false,
    };
    let harness = engine.harness.as_deref().and_then(|h| harnesses().into_iter().find(|x| x.id == h));
    if !installed(&engine.command) {
        let program = crate::project::split_command(&engine.command).into_iter().next().unwrap_or_default();
        test.message = match harness.as_ref().filter(|h| h.install.is_some()) {
            Some(h) => format!("{} is not installed on this machine yet. Press Install {} above.", program, h.label),
            None => format!("{program} is not installed on this machine."),
        };
        return test;
    }
    let provider = engine.provider.as_deref().and_then(|p| providers().into_iter().find(|x| x.id == p));
    for (var, name) in engine.secrets.iter().map(|s| (s, s)).chain(engine.keys.iter()) {
        if !secrets.iter().any(|(n, _)| n == var) {
            test.message = format!("There is no {name} in the key vault. Paste the key.");
            test.key_ok = Some(false);
            return test;
        }
    }
    if let Some(p) = &provider
        && let Some(url) = &p.url
        && let Some((_, key)) = secrets.iter().find(|(n, _)| *n == p.secret)
    {
        let (ok, said, _) = check_key(url, key).await;
        test.key_ok = Some(ok);
        if !ok {
            test.message = format!("{}: {said}.", p.label);
            test.fix = p.keys_at.clone();
            return test;
        }
    }
    let dir = std::env::temp_dir().join(format!("hub-engine-test-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).warn_on_err("create the folder to try an engine in");
    let probed =
        tokio::time::timeout(Duration::from_secs(120), crate::engine::acp::probe(engine, &dir.display().to_string(), secrets))
            .await;
    crate::locked::clean_dir(&dir);
    match probed {
        Err(_) => {
            test.message = format!("{name} did not answer within two minutes.");
            test.fix = harness.and_then(|h| h.login.map(|l| format!("Try it in a terminal first: {l}")));
        }
        Ok(Err(e)) => {
            test.message = format!("{name} would not start a session: {e}");
            test.fix = harness.and_then(|h| h.login.map(|l| format!("Log in once in a terminal: {l}")));
        }
        Ok(Ok(p)) => {
            let wanted = engine.model.as_deref().map(str::to_lowercase);
            test.model_found = match &wanted {
                None => true,
                Some(w) => p.models.iter().any(|(v, n)| v.to_lowercase().contains(w) || n.to_lowercase().contains(w)),
            };
            test.ok = true;
            test.message = match (&wanted, test.model_found, p.models.is_empty()) {
                (Some(w), false, true) => format!(
                    "It works, but it lets no one pick its model; it starts on what its own settings say, not {w}."
                ),
                (Some(w), false, false) => format!("It works, but offers no {w}. Pick one of its models below."),
                _ => "It works.".into(),
            };
            (test.agent, test.models, test.current) = (p.agent, p.models, p.current);
        }
    }
    test
}
