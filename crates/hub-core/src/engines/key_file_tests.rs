//! Tests of the harness key file.

#[test]
fn an_older_key_in_the_harness_file_is_set_aside() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".env");
    std::fs::write(&file, "OPENROUTER_API_KEY=or-keep\nexport DEEPSEEK_API_KEY=\"ds-old-7890\"\nOTHER=1\n").unwrap();
    let keys = vec![("DEEPSEEK_API_KEY".to_owned(), "ds-new-f52f".to_owned()), ("OPENROUTER_API_KEY".to_owned(), "or-keep".to_owned())];
    assert!(super::clear_stale_keys(&file, &keys).unwrap());
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("ds-old-7890"), "{text}");
    assert!(text.contains("OPENROUTER_API_KEY=or-keep\n") && text.ends_with("OTHER=1\n"), "{text}");
    // Nothing left to set aside, and no file is fine.
    assert!(!super::clear_stale_keys(&file, &keys).unwrap());
    assert!(!super::clear_stale_keys(&dir.path().join("none"), &keys).unwrap());
}
