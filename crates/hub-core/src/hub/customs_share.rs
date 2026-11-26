//! The `Hub` half of customs sharing: the loop that swaps the customs list with every host online.
//! It needs the remotes, so it lives here; `customs.rs` keeps the list itself and names nothing above it.

use std::sync::Arc;

use super::Hub;
use crate::customs::{Shared, merge, shared};

impl Hub {
    /// Keeps one list on every machine: every minute this machine swaps lists with each host
    /// online, so a stamp given anywhere holds everywhere. Hosts answer; the machine that has them
    /// asks.
    pub async fn share_customs(self: Arc<Self>) {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let mine = match shared(&self.ctx()).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("customs: reading the list to share: {e:#}");
                    continue;
                }
            };
            let Ok(body) = serde_json::to_value(&mine) else { continue };
            for (host, answer) in self.remotes.exchange::<Shared>("/api/customs/share", &body).await {
                match answer {
                    Ok(theirs) => {
                        if let Err(e) = merge(&self.ctx(), theirs).await {
                            tracing::warn!(%host, "customs: taking the list in: {e:#}");
                        }
                    }
                    // An older hubd there does not share yet; it will after its update.
                    Err(e) => tracing::debug!(%host, "customs: sharing the list: {e:#}"),
                }
            }
        }
    }
}
