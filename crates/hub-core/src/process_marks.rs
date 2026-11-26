//! The environment variables Hub stamps on what it starts, which is how it finds those processes
//! again: a thread's engine and what it runs (`HUB_THREAD_ID`, `HUB_PROJECT`), a project's
//! service (`HUB_SERVICE`). Kept this low so `runtime`, `services`, `workers` and `host` can read
//! them without naming each other.

pub const THREAD_VAR: &str = "HUB_THREAD_ID";
pub const PROJECT_VAR: &str = "HUB_PROJECT";
pub const SERVICE_VAR: &str = "HUB_SERVICE";
pub(crate) const SERVICE_VAR_EQ: &str = "HUB_SERVICE=";
