//! Safety net for splitting `hub.rs` into `hub/*.rs`: the sorted list of every function, constant
//! and static in `hub.rs` and `hub/**` is frozen in `frozen/hub_items.txt`. A move between those
//! files keeps the list the same; a dropped or doubled item changes it and fails here.
//!
//! The list holds one line per item (`fn name`, `const NAME`, `static NAME`), duplicates kept.
//! When you add or remove an item on purpose: `HUB_BLESS=1 cargo test -p hub-core --test it hub_items::`.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// `fn name` / `const NAME` / `static NAME` at the start of a line, after any visibility and qualifiers.
fn items(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim_start();
        if rest.starts_with("//") || rest.starts_with('#') {
            continue;
        }
        if let Some(r) = rest.strip_prefix("pub") {
            rest = r.trim_start();
            if rest.starts_with('(') {
                match rest.find(')') {
                    Some(i) => rest = rest[i + 1..].trim_start(),
                    None => continue,
                }
            }
        }
        for q in ["async ", "unsafe ", "const fn", "extern "] {
            if let Some(r) = rest.strip_prefix(q) {
                rest = if q == "const fn" { &rest[6..] } else { r };
            }
        }
        for kind in ["fn", "const", "static"] {
            if let Some(r) = rest.strip_prefix(kind).and_then(|r| r.strip_prefix(' ')) {
                let name: String = r.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !name.is_empty() && name != "_" {
                    found.push(format!("{kind} {name}"));
                }
                break;
            }
        }
    }
    found
}

#[test]
fn hub_rs_and_its_children_keep_every_item() {
    let src = hub_core::source_tree::hub_core().join("src");
    let mut files = vec![src.join("hub.rs")];
    rust_files(&src.join("hub"), &mut files);
    let mut all = Vec::new();
    for f in &files {
        all.extend(items(&std::fs::read_to_string(f).unwrap_or_else(|e| panic!("{}: {e}", f.display()))));
    }
    all.sort();
    let now = all.join("\n") + "\n";
    let frozen_path = hub_core::source_tree::hub_core().join("tests/it/frozen/hub_items.txt");
    if std::env::var_os("HUB_BLESS").is_some() {
        std::fs::write(&frozen_path, &now).unwrap();
        return;
    }
    let frozen = std::fs::read_to_string(&frozen_path).expect("frozen/hub_items.txt is missing: HUB_BLESS=1 writes it").replace("\r\n", "\n");
    if frozen != now {
        let (a, b): (Vec<&str>, Vec<&str>) = (frozen.lines().collect(), now.lines().collect());
        let gone: Vec<&&str> = a.iter().filter(|l| a.iter().filter(|x| x == l).count() > b.iter().filter(|x| x == l).count()).collect();
        let new: Vec<&&str> = b.iter().filter(|l| b.iter().filter(|x| x == l).count() > a.iter().filter(|x| x == l).count()).collect();
        panic!("items in hub.rs and hub/ changed.\nlost or fewer: {gone:?}\nnew or more: {new:?}\nIf on purpose: HUB_BLESS=1 cargo test -p hub-core --test it hub_items::");
    }
}
