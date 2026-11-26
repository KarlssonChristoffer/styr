//! Copies of the database in `<data dir>/backups`: one a day (the last three kept) and one before
//! each new version first starts (the last two kept). A copy is a complete SQLite file taken
//! while Hub runs (`VACUUM INTO`); to go back, stop Hub and put it in place of `hub.db`.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::store::Store;

const DAILY: &str = "daily-";
const PRE_UPDATE: &str = "pre-";
const ROLLED_BACK: &str = "rolledback-";
/// Days a database set aside by a rollback is kept.
const ROLLED_BACK_DAYS: u64 = 14;

/// How long after the core starts before the first daily copy is considered.
const START_DELAY: Duration = Duration::from_secs(120);

pub fn dir(data: &Path) -> PathBuf {
    data.join("backups")
}

/// Where the database is copied before `version` first starts.
pub fn before_update(data: &Path, version: &str) -> PathBuf {
    dir(data).join(format!("{PRE_UPDATE}{version}.db"))
}

/// Where the database is set aside when `version` is rolled back and the version before cannot
/// open what it left (`rollback::settle_database`).
pub fn rolled_back_name(version: &str) -> String {
    format!("{ROLLED_BACK}{version}.db")
}

/// Removes databases set aside by a rollback once they are two weeks old.
pub fn prune_rolled_back(data: &Path) {
    let limit = Duration::from_secs(ROLLED_BACK_DAYS * 24 * 3600);
    for (at, path) in copies(data, ROLLED_BACK) {
        if at.elapsed().unwrap_or_default() >= limit {
            crate::locked::clean_file(path);
        }
    }
}

/// Takes a copy of `store` into `<data>/backups` when the newest daily one is a day old, checking
/// every hour. Takes the store and the data folder, not the whole core, so it stays out of the ring.
pub async fn daily(store: Store, data: PathBuf) {
    // Not on the start path: the copy reads the whole database while the app is loading.
    tokio::time::sleep(START_DELAY).await;
    loop {
        let newest = newest(&data, DAILY);
        let due = newest.is_none_or(|t| t.elapsed().unwrap_or_default() >= Duration::from_secs(24 * 3600));
        if due {
            let path = dir(&data).join(format!("{DAILY}{}.db", crate::model::now_ms()));
            match store.backup(&path).await {
                Ok(()) => tracing::info!("database copied to {}", path.display()),
                Err(err) => tracing::warn!("daily copy of the database failed: {err:#}"),
            }
            prune(&data, DAILY, 3);
        }
        prune_rolled_back(&data);
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}

/// Takes the machine tokens out of the `hosts` value in the daily copies in `backups/`. Older versions
/// kept them there; they live in the vault now, and a copy is no place for them. A copy with no
/// kv table or that is not a database is left alone; a copy that cannot be opened is still logged.
pub fn scrub_host_tokens(data: &Path) {
    let Ok(entries) = std::fs::read_dir(dir(data)) else { return };
    for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "db")) {
        // The copy taken before an update is what a failed update goes back to, and the version
        // that wrote it looks for the token there.
        if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with(PRE_UPDATE)) {
            continue;
        }
        if let Err(err) = scrub_one(&path) {
            tracing::warn!("clearing host tokens from {}: {err:#}", path.display());
        }
    }
}

fn scrub_one(path: &Path) -> anyhow::Result<()> {
    let conn = rusqlite::Connection::open(path)?;
    // A copy from before the kv table existed, or a file that is not a database, has no hosts value.
    let has_kv = match conn.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'kv' COLLATE NOCASE", [], |r| r.get::<_, i64>(0)) {
        Ok(n) => n > 0,
        Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::NotADatabase => false,
        Err(err) => return Err(err.into()),
    };
    if !has_kv {
        return Ok(());
    }
    let json: Option<String> = match conn.query_row("SELECT value FROM kv WHERE key = 'hosts'", [], |r| r.get(0)) {
        Ok(json) => Some(json),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(err) => return Err(err.into()),
    };
    let Some(json) = json else { return Ok(()) };
    let Ok(mut hosts) = serde_json::from_str::<serde_json::Value>(&json) else { return Ok(()) };
    let mut changed = false;
    for host in hosts.as_array_mut().into_iter().flatten() {
        if let Some(obj) = host.as_object_mut()
            && obj.get("token").is_some_and(|t| !t.is_null()) {
                obj.insert("token".into(), serde_json::Value::Null);
                changed = true;
            }
    }
    if changed {
        conn.execute("UPDATE kv SET value = ?1 WHERE key = 'hosts'", [hosts.to_string()])?;
        // Free pages can still hold the old text.
        conn.execute_batch("VACUUM")?;
    }
    Ok(())
}

/// Keeps the `keep` newest copies whose names start with `prefix`.
pub fn prune(data: &Path, prefix: &str, keep: usize) {
    let mut copies = copies(data, prefix);
    copies.sort();
    for (_, path) in copies.iter().rev().skip(keep) {
        crate::locked::clean_file(path);
    }
}

pub fn prune_before_update(data: &Path) {
    prune(data, PRE_UPDATE, 2);
}


fn newest(data: &Path, prefix: &str) -> Option<SystemTime> {
    copies(data, prefix).into_iter().map(|(t, _)| t).max()
}

fn copies(data: &Path, prefix: &str) -> Vec<(SystemTime, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir(data)) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_newest_copies_are_kept() {
        let data = tempfile::tempdir().unwrap();
        for v in ["a", "b", "c", "d"] {
            let path = before_update(data.path(), v);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, v).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        std::fs::write(dir(data.path()).join("daily-1.db"), "").unwrap();
        prune(data.path(), PRE_UPDATE, 2);
        let mut left: Vec<_> = std::fs::read_dir(dir(data.path()))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, vec!["daily-1.db", "pre-c.db", "pre-d.db"]);
    }

    #[test]
    fn host_tokens_are_cleared_and_copies_without_them_are_skipped() {
        let data = tempfile::tempdir().unwrap();
        let dir = dir(data.path());
        std::fs::create_dir_all(&dir).unwrap();

        // A copy with a hosts value and a token: the token goes, and the old text with it.
        let with_hosts = dir.join("daily-1.db");
        let conn = rusqlite::Connection::open(&with_hosts).unwrap();
        conn.execute_batch("CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT)").unwrap();
        conn.execute(
            "INSERT INTO kv (key, value) VALUES ('hosts', ?1)",
            [r#"[{"name":"pc","token":"secret-token-value"}]"#],
        )
        .unwrap();
        drop(conn);
        scrub_one(&with_hosts).unwrap();
        let bytes = std::fs::read(&with_hosts).unwrap();
        assert!(!bytes.windows(b"secret-token-value".len()).any(|w| w == b"secret-token-value"));
        let conn = rusqlite::Connection::open(&with_hosts).unwrap();
        let hosts: String = conn.query_row("SELECT value FROM kv WHERE key = 'hosts'", [], |r| r.get(0)).unwrap();
        assert_eq!(hosts, r#"[{"name":"pc","token":null}]"#);
        drop(conn);

        // A copy from before the kv table existed: nothing to scrub, so no error and no change.
        let without_kv = dir.join("before-pictures.db");
        let conn = rusqlite::Connection::open(&without_kv).unwrap();
        conn.execute_batch("CREATE TABLE events (seq INTEGER PRIMARY KEY)").unwrap();
        drop(conn);
        let before = std::fs::read(&without_kv).unwrap();
        scrub_one(&without_kv).unwrap();
        assert_eq!(std::fs::read(&without_kv).unwrap(), before);

        // A file that is not a database at all: left alone, no error.
        let not_a_db = dir.join("daily-2.db");
        std::fs::write(&not_a_db, "this is not a SQLite database, just some text in a file\n").unwrap();
        scrub_one(&not_a_db).unwrap();
        assert_eq!(std::fs::read(&not_a_db).unwrap(), b"this is not a SQLite database, just some text in a file\n");

        // An empty file is an empty database with no tables: no error and no change.
        let empty = dir.join("daily-3.db");
        std::fs::write(&empty, b"").unwrap();
        scrub_one(&empty).unwrap();
        assert_eq!(std::fs::read(&empty).unwrap(), b"");
    }

    #[test]
    fn scrub_host_tokens_leaves_pre_update_copies_alone() {
        let data = tempfile::tempdir().unwrap();
        let dir = dir(data.path());
        std::fs::create_dir_all(&dir).unwrap();
        // The copy a failed update goes back to keeps its token: scrub_host_tokens skips it by name.
        let pre = before_update(data.path(), "1.0.0");
        let conn = rusqlite::Connection::open(&pre).unwrap();
        conn.execute_batch("CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT)").unwrap();
        conn.execute(
            "INSERT INTO kv (key, value) VALUES ('hosts', ?1)",
            [r#"[{"name":"pc","token":"keep-me-token"}]"#],
        )
        .unwrap();
        drop(conn);

        scrub_host_tokens(data.path());

        let conn = rusqlite::Connection::open(&pre).unwrap();
        let hosts: String = conn.query_row("SELECT value FROM kv WHERE key = 'hosts'", [], |r| r.get(0)).unwrap();
        assert_eq!(hosts, r#"[{"name":"pc","token":"keep-me-token"}]"#);
    }
}
