//! Tests of the configuration.

use super::*;

#[test]
fn the_settings_file_is_written_whole_and_the_old_one_stays_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.json");
    write_json(&path, &serde_json::json!({ "checkpoints": false })).unwrap();
    assert!(read_json(&path).unwrap()["checkpoints"] == false);
    // Nothing is left beside it.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    // A write that cannot land (the folder is gone) leaves no half file.
    let gone = dir.path().join("missing").join("hub.json");
    std::fs::create_dir_all(gone.parent().unwrap()).unwrap();
    std::fs::remove_dir(gone.parent().unwrap()).unwrap();
    let _ = write_json(&gone, &serde_json::json!({}));
    assert!(read_json(&path).unwrap()["checkpoints"] == false);
}

#[test]
fn a_broken_file_is_moved_aside_and_hub_starts_on_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.json");
    std::fs::write(&path, "{ \"checkpoints\": ").unwrap();
    let (config, problem) = load_from(&path);
    assert!(config.checkpoints == Config::default().checkpoints);
    let problem = problem.expect("a reason");
    assert!(problem.contains("not valid JSON") && problem.contains("hub.json.broken-"), "{problem}");
    assert!(!path.exists(), "the broken file is out of the way");
    let kept: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(kept.len() == 1 && kept[0].starts_with("hub.json.broken-"), "{kept:?}");
    // A wrong value is a problem too; a missing file is none.
    std::fs::write(&path, "{ \"bind\": 7 }").unwrap();
    assert!(load_from(&path).1.is_some_and(|p| p.contains("does not read as settings")));
    assert!(load_from(&path).1.is_none());
}

#[test]
fn an_empty_or_missing_file_reads_as_no_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hub.json");
    assert_eq!(read_json(&path).unwrap(), serde_json::json!({}));
    std::fs::write(&path, "  \n").unwrap();
    assert_eq!(read_json(&path).unwrap(), serde_json::json!({}));
    std::fs::write(&path, "{ \"checkpoints\": ").unwrap();
    assert!(read_json(&path).is_err());
}
