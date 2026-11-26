//! Starting an engine again when a key it started with changed in the vault (`secrets`). Its own
//! module above `hub`, so neither the vault nor the engine catalog (`engines.rs`) knows `Hub`.

use crate::hub::Hub;
use crate::locked::LockExt;

/// An engine keeps the environment it started with, so one started before a key was replaced
/// would go on sending the old key (DeepSeek answering 401 in a thread while Settings, which
/// reads the vault, says the key works). After every change, each engine that started with a
/// key that is now different or gone is stopped once its turn is over; the next message starts
/// it again on the new key and loads the same session.
pub async fn renew_engines(hub: std::sync::Arc<Hub>) {
    let mut changes = hub.secrets.changes();
    loop {
        tokio::select! {
            changed = changes.changed() => if changed.is_err() { return },
            // A thread in a turn when the key changed is looked at again until it is done.
            _ = tokio::time::sleep(std::time::Duration::from_secs(20)) => {}
        }
        renew_once(&hub).await;
    }
}

/// Stops the idle engines whose keys changed since they started. The threads they were on.
pub async fn renew_once(hub: &Hub) -> Vec<String> {
    let now = hub.secrets.cached();
    let stale: Vec<String> = hub
        .engine_keys
        .locked()
        .iter()
        .filter(|(_, keys)| keys.iter().any(|(name, value)| now.get(name) != Some(value)))
        .map(|(id, _)| id.clone())
        .collect();
    let mut renewed = Vec::new();
    for id in stale {
        let busy = matches!(
            hub.store.thread(&id).await.ok().flatten().map(|t| t.status),
            Some(crate::model::ThreadStatus::Running | crate::model::ThreadStatus::WaitingOnYou)
        );
        if busy {
            continue;
        }
        hub.engine_keys.locked().remove(&id);
        if !hub.engines().contains(&id) {
            continue;
        }
        tracing::info!(thread = %id, "a key it started with changed; starting its engine again on the new one");
        if let Err(e) = hub.shutdown(&id, false).await {
            tracing::warn!(thread = %id, "could not stop the engine for its new key: {e:#}");
        }
        renewed.push(id);
    }
    renewed.extend(hub.renew_reach().await);
    renewed
}
