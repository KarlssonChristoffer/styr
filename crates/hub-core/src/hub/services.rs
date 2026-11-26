//! The `Hub` half of `services.rs`: Stop all on a project's page also shuts down the engines of
//! the project's threads and what those started, except threads open in a terminal. Starting,
//! listing and the logs take the folder and the data folder and live in `services.rs`, which
//! names nothing above it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::Hub;
use crate::model::{ServicesDone, ThreadStatus};
use crate::runtime::same_or_under;

impl Hub {
    /// Stops one service, or with no name everything the project runs: its services, compose,
    /// and the engines of its threads with what they started. Threads open in a terminal are
    /// left alone, and so is what was started by hand.
    pub async fn stop_services(self: &Arc<Self>, root: &str, name: Option<&str>) -> Result<ServicesDone> {
        let root = PathBuf::from(root);
        let mut done = crate::services::stop_named(&root, name).await?;
        if name.is_some() {
            return Ok(done);
        }
        self.stop_project_threads(&root, &mut done).await?;
        Ok(done)
    }

    async fn stop_project_threads(self: &Arc<Self>, root: &Path, done: &mut ServicesDone) -> Result<()> {
        let engines = self.engines();
        for thread in self.store.threads().await? {
            if !same_or_under(Path::new(&thread.cwd), root) {
                continue;
            }
            if thread.status == ThreadStatus::InTerminal {
                done.in_terminal += 1;
                continue;
            }
            if engines.contains(&thread.id) {
                self.shutdown(&thread.id, false).await?;
                done.engines += 1;
            }
            let id = thread.id.clone();
            let left = tokio::task::spawn_blocking(move || {
                let pids = crate::workers::thread_processes(&id);
                pids.iter().filter(|pid| crate::workers::act(**pid, crate::workers::Action::Stop).is_ok()).count()
            })
            .await?;
            done.processes += left as u32;
        }
        Ok(())
    }
}
