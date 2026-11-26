//! What a thread can reach beyond its own folder (`ThreadReach`): more folders the agent may
//! read, and connections switched off for this thread only. Kept under `reach:<thread id>`. It
//! applies when the thread's engine starts: folders go to the session as its additional
//! directories (ACP `additionalDirectories`, and Claude Code's own option of that name), the
//! connections join what `ConnectionPrefs` switches off. A change stops an idle engine so the next
//! message starts it on the new reach; one in a turn is stopped when the turn is over. That half,
//! which needs the running engines, is `hub/reach.rs`; this one only reads and writes the database.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::model::{ReachFolder, ThreadReach};
use crate::ctx::Ctx;

/// More than this is a different way of working.
pub const MAX_FOLDERS: usize = 8;

fn key(thread_id: &str) -> String {
    format!("reach:{thread_id}")
}

pub async fn get(ctx: &Ctx<'_>, thread_id: &str) -> Result<ThreadReach> {
    Ok(ctx.store.get_value(&key(thread_id)).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

/// The folders as directories to hand the session: those still there, apart from its own.
pub fn directories(reach: &ThreadReach, cwd: &str) -> Vec<PathBuf> {
    reach
        .folders
        .iter()
        .map(|f| PathBuf::from(&f.path))
        .filter(|p| p.is_dir() && !same(p, Path::new(cwd)))
        .collect()
}

/// Whether two paths name one folder, ignoring a trailing separator and, as Windows does, case.
fn same(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        let s = s.trim_end_matches('/').to_owned();
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(a) == norm(b)
}

/// Tidies what a client sent: folders must be absolute and there, none twice, none the thread's
/// own; connection names sorted and without repeats.
pub fn normalize(cwd: &str, mut reach: ThreadReach) -> Result<ThreadReach> {
    let mut folders: Vec<ReachFolder> = Vec::new();
    for f in std::mem::take(&mut reach.folders) {
        let path = f.path.trim().to_owned();
        if path.is_empty() {
            continue;
        }
        let p = Path::new(&path);
        if !p.is_absolute() {
            bail!("{path:?} is not an absolute folder");
        }
        if !p.is_dir() {
            bail!("folder {path:?} does not exist on this machine");
        }
        if same(p, Path::new(cwd)) || folders.iter().any(|g| same(Path::new(&g.path), p)) {
            continue;
        }
        let name = f.name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
        folders.push(ReachFolder { path, name });
    }
    if folders.len() > MAX_FOLDERS {
        bail!("a thread can read at most {MAX_FOLDERS} extra folders");
    }
    reach.folders = folders;
    let mut off: Vec<String> = reach.connections_off.iter().map(|c| c.trim().to_owned()).filter(|c| !c.is_empty()).collect();
    off.sort();
    off.dedup();
    reach.connections_off = off;
    Ok(reach)
}

/// Keeps `reach` for the thread, tidied, and whether it differs from what was kept before.
pub async fn save(ctx: &Ctx<'_>, thread_id: &str, reach: ThreadReach) -> Result<(ThreadReach, bool)> {
    let thread = ctx.store.thread(thread_id).await?.ok_or_else(|| anyhow::anyhow!("no thread {thread_id}"))?;
    let reach = normalize(&thread.cwd, reach)?;
    let before = get(ctx, thread_id).await?;
    if reach.folders.is_empty() && reach.connections_off.is_empty() {
        ctx.store.set_value(&key(thread_id), "").await?;
    } else {
        ctx.store.set_value(&key(thread_id), &serde_json::to_string(&reach)?).await?;
    }
    let changed = before != reach;
    Ok((reach, changed))
}

/// Keeps what a thread started with. The thread exists already, so its folder is known.
pub async fn start(ctx: &Ctx<'_>, thread_id: &str, cwd: &str, reach: ThreadReach) -> Result<()> {
    let reach = normalize(cwd, reach)?;
    if !reach.folders.is_empty() || !reach.connections_off.is_empty() {
        ctx.store.set_value(&key(thread_id), &serde_json::to_string(&reach)?).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(path: &str) -> ReachFolder {
        ReachFolder { path: path.into(), name: None }
    }

    #[test]
    fn folders_must_be_there_once_and_not_the_threads_own() {
        let own = crate::janitor::test_dir().to_string_lossy().into_owned();
        let other = crate::janitor::test_dir().to_string_lossy().into_owned();
        let got = normalize(
            &own,
            ThreadReach {
                folders: vec![folder(&own), folder(&format!("{other}/")), folder(&other), folder("  ")],
                connections_off: vec!["Trello".into(), " ".into(), "Trello".into(), "Gmail".into()],
            },
        )
        .unwrap();
        assert_eq!(got.folders.len(), 1);
        assert_eq!(got.connections_off, vec!["Gmail".to_owned(), "Trello".to_owned()]);
        assert_eq!(directories(&got, &own).len(), 1);
    }

    #[test]
    fn a_missing_or_relative_folder_is_refused() {
        let own = crate::janitor::test_dir().to_string_lossy().into_owned();
        let missing = format!("{own}/nope");
        assert!(normalize(&own, ThreadReach { folders: vec![folder(&missing)], ..Default::default() }).is_err());
        assert!(normalize(&own, ThreadReach { folders: vec![folder("relative/dir")], ..Default::default() }).is_err());
    }
}
