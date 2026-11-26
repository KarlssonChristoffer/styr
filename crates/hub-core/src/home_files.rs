//! The home folder and the small JSON files engines and tools keep under it: find the folder, read
//! one file, take one text field out of it. Plain std and serde, nothing of Hub, so the low layers
//! (`engine_home`) can read a login file without naming `prefs` or `folders`.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The user's home folder as the environment names it (`USERPROFILE` on Windows, else `HOME`).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

/// The file at `path` as JSON, or `None` when it is missing or not JSON.
pub fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The string at a JSON pointer, or `None` when it is missing, not a string or empty.
pub fn text(v: &Value, pointer: &str) -> Option<String> {
    v.pointer(pointer).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned)
}
