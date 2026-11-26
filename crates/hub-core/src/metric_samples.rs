//! Where timings gather before Problems sweeps them (`problems/metrics.rs`): `sample` for a reading,
//! and the clock from a turn's start to the engine's first word. Hub's event observers
//! (`hub/observers.rs`) and the API call these, so they sit below `hub` and know nothing of it.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::model::now_ms;

/// The version of Hub that runs, for Hub's own metrics (set by hubd at start).
pub static RELEASE: OnceLock<String> = OnceLock::new();
/// Samples gathered by (name, release, project).
pub(crate) type Gathered = HashMap<(String, String, String), Vec<f64>>;
/// Samples since the last sweep.
static SAMPLES: Mutex<Option<Gathered>> = Mutex::new(None);
/// Turns started and not yet answered, by thread.
static TURNS: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

/// A timing or a reading, in milliseconds (or megabytes for memory).
pub fn sample(name: &str, value: f64, release: Option<&str>, project: &str) {
    if !value.is_finite() || value < 0.0 {
        return;
    }
    let release = release.map(str::to_owned).or_else(|| RELEASE.get().cloned()).unwrap_or_else(|| "dev".into());
    let mut all = SAMPLES.lock().unwrap_or_else(|p| p.into_inner());
    let list = all.get_or_insert_with(HashMap::new).entry((name.to_owned(), release, project.to_owned())).or_default();
    if list.len() < 10_000 {
        list.push(value);
    }
}

/// Everything gathered since the last call, emptied.
pub(crate) fn take() -> Gathered {
    SAMPLES.lock().unwrap_or_else(|p| p.into_inner()).take().unwrap_or_default()
}

/// A turn started: the clock for Send to first word runs.
pub fn turn_started(thread: &str) {
    TURNS.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).insert(thread.to_owned(), now_ms());
}

/// Something came back from the engine; the first answer of a turn stops the clock.
pub fn engine_said(thread: &str, data: &Value) {
    if !matches!(crate::model::update_kind(data), Some("agent_message_chunk" | "agent_thought_chunk" | "tool_call")) {
        return;
    }
    let started = TURNS.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).remove(thread);
    if let Some(at) = started {
        sample("first_token_ms", (now_ms() - at) as f64, None, "Hub");
    }
}
