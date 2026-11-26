//! The `Hub` half of `focus.rs`: asking whether a new thread belongs to its project's stage. The
//! focus file's reading and editing stay in `focus.rs`, which then names nothing above it.

use std::path::Path;
use std::sync::Arc;

use super::Hub;
use crate::focus::{check_prompt, read, read_check};

impl Hub {
    /// Asks the decision model (`reading.rs`), or else a small model, whether a new thread
    /// belongs to its project's stage now, and logs a `focus` event when it does not, so the
    /// thread can offer to park it. The thread does not wait for this; any failure just means no
    /// notice.
    pub async fn focus_check(self: Arc<Self>, thread_id: String, cwd: String, ask: String) {
        if ask.split_whitespace().count() < 4 || !self.guard_prefs().await.unwrap_or_default().focus_check {
            return;
        }
        let folder = cwd.clone();
        let Ok(focus) = tokio::task::spawn_blocking(move || read(Path::new(&folder))).await else {
            return;
        };
        let Some(stage) = focus.active.and_then(|i| focus.stages.get(i)) else {
            return;
        };
        // A decision model answers in a moment; a small model on the subscription when it is
        // unsure or not connected.
        if let Some((fits, why)) = self.focus_reading(&thread_id, stage, &ask).await {
            if !fits {
                let data = crate::model::FocusData { fits: false, stage: stage.title.clone(), line: stage.line, why };
                if let Err(e) = self.emit_typed(&thread_id, &data).await {
                    tracing::warn!("focus check for {thread_id}: {e:#}");
                }
            }
            return;
        }
        let (name, tidy_model) = self.tidy_target(Some(&cwd)).await;
        let Some(engine) = self.config.engines.get(&name) else {
            return;
        };
        let answer = crate::engine::complete("focus_check", engine, &cwd, tidy_model.as_deref(), check_prompt(stage, &ask));
        let Ok(Ok(text)) = tokio::time::timeout(std::time::Duration::from_secs(120), answer).await else {
            return;
        };
        if let Some((false, why)) = read_check(&text) {
            let data = crate::model::FocusData { fits: false, stage: stage.title.clone(), line: stage.line, why };
            if let Err(e) = self.emit_typed(&thread_id, &data).await {
                tracing::warn!("focus check for {thread_id}: {e:#}");
            }
        }
    }
}
