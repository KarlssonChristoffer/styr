//! A program Hub starts must never open a console window on Windows (it flashes on screen and
//! vanishes). All spawns go through `hub_core::proc` (`hub_install::proc` for the launcher, app
//! and hubd), which sets `CREATE_NO_WINDOW`. This fails on a raw `Command::new` so a new spawn
//! cannot bring the flashing back.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target" || n == "node_modules") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The raw constructors; `AvailableCommand::new` and the like are other types.
fn raw_spawn(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or("");
    ["Command::new(", "Command::from("].iter().any(|needle| {
        code.match_indices(needle).any(|(at, _)| {
            let before = &code[..at];
            !before.ends_with(|c: char| c.is_alphanumeric() || c == '_')
        })
    })
}

#[test]
fn every_spawn_goes_through_proc() {
    let root = hub_core::source_tree::root();
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("app/src-tauri/src"), &mut files);
    assert!(files.len() > 50, "found only {} source files; is the layout still crates/ and app/src-tauri?", files.len());

    let mut bad = Vec::new();
    for file in files {
        let rel = file.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        // Test code may spawn what it likes (test files, and each `#[cfg(test)]` item: `test_code.rs`);
        // the helpers themselves are the one allowed place.
        if rel.contains("/tests/") || rel.ends_with("/tests.rs") || rel.ends_with("/proc.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap();
        for (n, line) in crate::test_code::code_lines(&text) {
            if raw_spawn(line) {
                bad.push(format!("{rel}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "start programs with hub_core::proc::command / tokio_command (hub_install::proc::command outside hub-core) so no console window flashes on Windows:\n{}",
        bad.join("\n")
    );
}

#[test]
fn the_check_spots_a_raw_spawn() {
    assert!(raw_spawn("let c = std::process::Command::new(\"git\");"));
    assert!(raw_spawn("let c = Command::new(program);"));
    assert!(raw_spawn("x(tokio::process::Command::from(c))"));
    assert!(!raw_spawn("AvailableCommand::new(\"compact\", \"x\")"));
    assert!(!raw_spawn("// Command::new(\"git\") is what this replaces"));
}
