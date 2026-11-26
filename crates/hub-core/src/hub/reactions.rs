//! The `Hub` half of `reactions.rs`: logging a reaction as an event. What the reaction is, and
//! whether it changes anything, is decided in `reactions.rs`, which then names nothing above the database.

use anyhow::Result;

use super::Hub;
use crate::model::Event;

impl Hub {
    /// Puts a reaction on a message, or takes it away. `by` is "you" or "agent". Returns the event,
    /// or None when it would change nothing (it was on already, or never on).
    pub async fn react(&self, thread_id: &str, seq: i64, emoji: &str, on: bool, by: &str) -> Result<Option<Event>> {
        let Some(data) = crate::reactions::reaction(&self.ctx(), thread_id, seq, emoji, on, by).await? else {
            return Ok(None);
        };
        let event = self.emit_typed(thread_id, &data).await?;
        if on
            && by == "you"
            && let Err(err) = crate::reactions::count(&self.ctx(), emoji).await
        {
            tracing::warn!("could not count the emoji: {err:#}");
        }
        Ok(Some(event))
    }
}
