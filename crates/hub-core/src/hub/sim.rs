//! The inbox simulator's start and clear (Labs): the threads `sim.rs` names, run on its engine.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::json;

use super::Hub;
use crate::model::NewThread;
use crate::sim::{ENGINE, THREADS};

impl Hub {
    /// Starts the simulated threads in `<data dir>/Simulation`, staggered over a few seconds.
    /// Returns how many will start.
    pub fn start_sim(self: &Arc<Self>) -> Result<usize> {
        if !self.config.engines.contains_key(ENGINE) {
            bail!("this core has no simulator engine; it comes with hubd, so update Hub first");
        }
        let dir = self.config.data_dir.join("Simulation");
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let cwd = dir.display().to_string();
        let hub = self.clone();
        tokio::spawn(async move {
            let mut at = 0;
            for (title, message, scenario, delay) in THREADS {
                tokio::time::sleep(Duration::from_secs(delay - at)).await;
                at = *delay;
                let new: NewThread = serde_json::from_value(json!({
                    "cwd": cwd,
                    "title": title,
                    "engine": ENGINE,
                    "message": message,
                    "settings": { "scenario": scenario },
                }))
                .expect("a valid new thread");
                if let Err(err) = hub.create_thread(new).await {
                    tracing::warn!("could not start a simulated thread: {err:#}");
                }
            }
        });
        Ok(THREADS.len())
    }

    /// Removes every simulated thread. Returns how many went.
    pub async fn clear_sim(&self) -> Result<usize> {
        let threads = self.store.threads().await?;
        let mut removed = 0;
        for thread in threads.iter().filter(|t| t.engine == ENGINE) {
            self.delete_thread(&thread.id).await?;
            removed += 1;
        }
        Ok(removed)
    }
}
