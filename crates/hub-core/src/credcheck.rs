//! "Do this machine's keys and sign-ins work?" tried for real, one row each: every engine (its
//! sign-in or provider key, by starting the agent and asking for a session, which spends nothing)
//! and every API connection (its cheap GET). It reads nothing the readiness page does not: the
//! row says what happened and when a kept sign-in ends, never a key.

use std::time::Duration;

use futures::future::join_all;

use crate::hub::Hub;
use crate::model::{CredentialCheck, CredentialRow, CredentialState, ReadyState, now_ms};

/// One try never holds the answer longer than this.
const AT_MOST: Duration = Duration::from_secs(130);

pub async fn check(hub: &Hub) -> CredentialCheck {
    let sign_ins = hub.sign_ins().await;
    let ready = crate::ready::check(&hub.config, sign_ins.clone()).await;
    let mut names: Vec<&String> = hub.config.engines.keys().filter(|n| n.as_str() != crate::sim::ENGINE).collect();
    names.sort();
    let engines = names.into_iter().map(|name| engine_row(hub, name, &sign_ins, &ready));
    let apis = apis(hub);
    let (mut rows, apis) = tokio::join!(join_all(engines), apis);
    rows.extend(apis);
    CredentialCheck { rows, at: now_ms() }
}

async fn engine_row(hub: &Hub, name: &str, sign_ins: &crate::signin::SignIns, ready: &crate::model::MachineReady) -> CredentialRow {
    let key = format!("engine:{name}");
    let check = ready.checks.iter().find(|c| c.key == key);
    let label = check.map(|c| c.label.clone()).unwrap_or_else(|| name.to_owned());
    let until = sign_ins.kept.get(name).map(|k| k.until).filter(|u| *u > now_ms());
    let row = |state, detail: String, fix| CredentialRow { key: key.clone(), label: label.clone(), kind: "engine".into(), state, detail, until, fix };
    let fix = check.and_then(|c| c.fix.clone());
    // Not there, or lost by an earlier failure: nothing to try.
    if let Some(c) = check.filter(|c| c.state == ReadyState::Missing) {
        let lost = sign_ins.lost.contains_key(name) && !c.detail.contains("not found") && !c.detail.starts_with("not signed in");
        return row(if lost { CredentialState::Invalid } else { CredentialState::Missing }, c.detail.clone(), fix);
    }
    let Some(engine) = hub.config.engines.get(name) else { return row(CredentialState::Unknown, "not configured".into(), None) };
    // Whatever an agent or service said goes through the vault's redaction and is cut short.
    let row = |state, detail: String, fix| row(state, tidy(hub, &detail), fix);
    let secrets = match hub.secrets.engine_env(engine).await {
        Ok(s) => s,
        Err(e) => return row(CredentialState::Unknown, format!("the key vault could not be read: {e:#}"), None),
    };
    match tokio::time::timeout(AT_MOST, crate::engine_probe::test(name, engine, secrets)).await {
        Err(_) => row(CredentialState::Unknown, "did not answer within two minutes".into(), fix),
        Ok(t) if t.ok => {
            let detail = check.map(|c| c.detail.clone()).filter(|d| d != "installed").map(|d| format!("{d} · session opened")).unwrap_or_else(|| "session opened".into());
            row(CredentialState::Ok, detail, check.and_then(|c| c.fix.clone()).filter(|_| until.is_some_and(|u| u - now_ms() < crate::signin::SOON_MS)))
        }
        Ok(t) => {
            let state = if t.message.contains("no ") && t.message.contains("in the key vault") { CredentialState::Missing } else { CredentialState::Invalid };
            row(state, t.message, fix)
        }
    }
}

/// A line of what a try said with every known key replaced and at most 240 characters.
fn tidy(hub: &Hub, text: &str) -> String {
    let mut value = serde_json::Value::String(text.to_owned());
    hub.secrets.redact(&mut value);
    let line = value.as_str().unwrap_or_default().to_owned();
    if line.chars().count() > 240 { format!("{}…", line.chars().take(240).collect::<String>()) } else { line }
}

async fn apis(hub: &Hub) -> Vec<CredentialRow> {
    let Ok(prefs) = hub.api_prefs().await else { return Vec::new() };
    join_all(prefs.connections.iter().map(|c| async move {
        let row = |state, detail: String| CredentialRow { key: format!("api:{}", c.name), label: c.label.clone(), kind: "api".into(), state, detail: tidy(hub, &detail), until: None, fix: None };
        match tokio::time::timeout(AT_MOST, hub.test_api(&c.name)).await {
            Err(_) => row(CredentialState::Unknown, "did not answer".into()),
            // No key yet (or not allowed yet) is an error before anything is sent.
            Ok(Err(e)) => row(CredentialState::Missing, format!("{e:#}")),
            Ok(Ok(t)) if t.ok => row(CredentialState::Ok, format!("works · {} ms", t.ms)),
            Ok(Ok(t)) if t.message.starts_with("No answer") => row(CredentialState::Unknown, t.message),
            Ok(Ok(t)) => row(CredentialState::Invalid, t.message),
        }
    }))
    .await
}
