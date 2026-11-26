//! Where projects and chats go: the projects root the user set (Settings), its default, and
//! whether a folder is an everyday chat's or a scratch thread's under it. Low on purpose: the
//! modules that only need to tell a chat from a project (`handoff.rs` among them) do not have to
//! name `hub` or `connections`, which sit in the dependency ring (`tests/it/frozen/ring.txt`).

use std::path::Path;

use anyhow::Result;

use crate::store::Store;

/// The setting's key in the database.
pub(crate) const KEY: &str = "projects.root";

/// The projects root: the one the user set, else `default_root`.
pub async fn get(store: &Store) -> Result<String> {
    if let Some(root) = store.get_value(KEY).await? {
        return Ok(root);
    }
    Ok(default_root())
}

/// D:\Dev on Windows when that drive has it, otherwise Dev in the home folder.
pub fn default_root() -> String {
    if cfg!(windows) && Path::new(r"D:\Dev").is_dir() {
        return r"D:\Dev".into();
    }
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    Path::new(&home).join("Dev").display().to_string()
}

/// Whether `cwd` is an everyday chat's folder: `<projects root>/chats/<time>` (the phone's Chat
/// world makes them). Read as text, as `folders::same_folder` does.
pub fn is_chat_folder(projects_root: &str, cwd: &str) -> bool {
    let norm = |s: &str| s.trim().trim_end_matches(['/', '\\']).replace('\\', "/").to_lowercase();
    let (root, cwd) = (norm(projects_root), norm(cwd));
    !root.is_empty() && cwd.strip_prefix(&format!("{root}/chats/")).is_some_and(|rest| !rest.is_empty())
}

/// Whether what is made in `cwd` is for the person and not code that ships: a scratch thread's
/// folder or an everyday chat's. Hub asks no proof there.
pub fn is_made_for_the_person(projects_root: &str, cwd: &str) -> bool {
    is_scratch_folder(projects_root, cwd) || is_chat_folder(projects_root, cwd)
}

/// Whether `cwd` is a scratch thread's folder: `<projects root>/scratch/<time>` (a quick thread
/// with no project). Nothing in it is shipped code, so Hub asks no proof of what is made there.
pub fn is_scratch_folder(projects_root: &str, cwd: &str) -> bool {
    let norm = |s: &str| s.trim().trim_end_matches(['/', '\\']).replace('\\', "/").to_lowercase();
    let (root, cwd) = (norm(projects_root), norm(cwd));
    !root.is_empty() && cwd.strip_prefix(&format!("{root}/scratch/")).is_some_and(|rest| !rest.is_empty())
}
