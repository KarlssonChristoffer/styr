//! Sessions that wait for you go to sleep. An agent that has asked a permission question and got
//! no answer holds its engine (a process and a few hundred MB) for as long as you take, which can
//! be a night. After [`AFTER`] the core stops that engine and keeps the question (`Hub::sleep_waiting`
//! logs a `slept` event). When you answer, the same session starts again with your answer as a
//! message and carries on (`Hub::wake_from_sleep`). Nothing is asked of the agent: it sees the
//! question cancelled, then a message that says what you answered.

use std::sync::Arc;
use std::time::Duration;

use crate::hub::Hub;

/// How long a question waits before its engine is stopped.
pub const AFTER: Duration = Duration::from_secs(5 * 60);

/// Looks for sleepers twice a minute, for as long as the core runs.
pub async fn watch(hub: Arc<Hub>) {
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tick.tick().await;
        hub.sleep_waiting(AFTER).await;
    }
}
