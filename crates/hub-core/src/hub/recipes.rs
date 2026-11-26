//! The `Hub` half of `recipes.rs`: starting a run of a recipe as a new thread. Reading and
//! changing the recipes stay in `recipes.rs`, which then names nothing above the database.

use std::sync::Arc;

use anyhow::{Result, bail};

use super::Hub;
use crate::model::NewThread;
use crate::recipes::Start;

impl Hub {
    /// Starts recipe `id` in `folder` (its own folder wins) and returns the new thread.
    pub async fn start_recipe(self: &Arc<Self>, id: &str, folder: Option<&str>, how: Start) -> Result<String> {
        if self.halted() {
            bail!("every agent is paused; resume them first");
        }
        let (recipe, folder) = crate::recipes::to_start(&self.ctx(), id, folder).await?;
        let thread = self
            .create_thread(NewThread {
                cwd: folder.clone(),
                title: Some(recipe.name.clone()),
                engine: recipe.engine.clone(),
                message: None,
                attachments: Vec::new(),
                settings: how.settings,
                original: None,
                client_id: None,
                attempts: Vec::new(),
                warm: None,
                design: false,
                unattended: None,
                reach: None,
                cloud: false,
            })
            .await?;
        let (brief, mark) = crate::recipes::started(&self.ctx(), &recipe, &thread.id, &folder, how.note.as_deref(), how.by).await?;
        self.send_from(&thread.id, brief, Vec::new(), None, Some(("recipe", mark)), None).await?;
        Ok(thread.id)
    }
}
