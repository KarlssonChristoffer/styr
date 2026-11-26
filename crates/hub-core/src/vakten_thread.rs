//! Which thread is Vakten's. Hub asks this when it picks a permission mode, answers a permission
//! and gives old threads their owners, so the question sits below `hub` and reads only the
//! database; Vakten itself (`problems/vakten.rs`) keeps the rest of the state in the same value.

use serde::Deserialize;

use crate::store::Store;

/// The setting that holds Vakten's state as JSON.
pub(crate) const STATE: &str = "problems.vakten.state";

/// The one field of the state this reads; the rest is Vakten's own.
#[derive(Deserialize, Default)]
struct Held {
    thread: Option<String>,
}

/// Vakten's thread, `None` before it has started one.
pub async fn thread_id(store: &Store) -> Option<String> {
    let value = store.get_value(STATE).await.ok().flatten()?;
    serde_json::from_str::<Held>(&value).ok()?.thread
}

/// Whether `thread_id` is Vakten's thread.
pub async fn is_vakten(store: &Store, thread_id: &str) -> bool {
    self::thread_id(store).await.as_deref() == Some(thread_id)
}
