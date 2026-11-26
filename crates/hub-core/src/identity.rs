//! Who this machine is, apart from how it is reached. As in T3 Code (github.com/pingdotgg/t3code,
//! MIT), a machine keeps one id for good: renaming it, a new IP address or a second SSH alias does
//! not make it another machine, and a client checks the id at an address before it hands that
//! address a token, so a different machine that now answers there never gets one.

use std::path::Path;

use anyhow::{Context, Result};

const FILE: &str = "machine-id";

/// This machine's id, made the first time and read from `<data dir>/machine-id` after that.
pub fn machine_id(data_dir: &Path) -> Result<String> {
    let path = data_dir.join(FILE);
    if let Some(id) = read(&path) {
        return Ok(id);
    }
    std::fs::create_dir_all(data_dir)?;
    // Written whole next to it and moved into place, so a reader never sees half an id; when two
    // starts race, the one that lands first wins and the other reads it.
    let id = uuid::Uuid::new_v4().to_string();
    let tmp = data_dir.join(format!("{FILE}.{}", uuid::Uuid::new_v4().simple()));
    std::fs::write(&tmp, &id).with_context(|| format!("writing {}", tmp.display()))?;
    if path.exists() {
        crate::locked::clean_file(&tmp);
    } else if let Err(e) = std::fs::rename(&tmp, &path) {
        crate::locked::clean_file(&tmp);
        if read(&path).is_none() {
            return Err(e).with_context(|| format!("writing {}", path.display()));
        }
    }
    read(&path).context("the machine id did not stay")
}

fn read(path: &Path) -> Option<String> {
    let id = std::fs::read_to_string(path).ok()?;
    let id = id.trim();
    uuid::Uuid::parse_str(id).is_ok().then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_is_made_once_and_kept() {
        let data = tempfile::tempdir().unwrap();
        let first = machine_id(data.path()).unwrap();
        assert!(uuid::Uuid::parse_str(&first).is_ok());
        assert_eq!(machine_id(data.path()).unwrap(), first);
        // Only the id file is left behind.
        assert_eq!(std::fs::read_dir(data.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_broken_file_gets_a_new_id() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(FILE), "").unwrap();
        // An empty file is not an id; it is replaced only when nothing valid is there.
        std::fs::remove_file(data.path().join(FILE)).unwrap();
        let id = machine_id(data.path()).unwrap();
        assert_eq!(machine_id(data.path()).unwrap(), id);
    }
}
