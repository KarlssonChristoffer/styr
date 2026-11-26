//! Keeps `docs/MAP.md` honest. The map is the first thing an agent reads, so it must stay short and
//! must not send anyone to a file, doc section or function that is gone. Checks: under 150 lines;
//! every backticked path exists; every `§Heading` is a heading in `ARCHITECTURE.md`, `docs/architecture/` or `APP.md`;
//! every backticked identifier still appears in the code; every doc in `docs/` says on one of its
//! first lines who it is for (`<!-- för: agent -->` or `<!-- för: Christoffer -->`).
//! Short paths in the map resolve under `R/` (hub-core's src), `L/` (app/src/lib) or `C/` (components).

use std::path::{Path, PathBuf};

const MAX_LINES: usize = 150;

fn root() -> PathBuf {
    hub_core::source_tree::root()
}

fn read(path: &str) -> String {
    std::fs::read_to_string(root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}")).replace("\r\n", "\n")
}

/// Every `code` span on a line of the map.
fn spans(map: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in map.lines() {
        for (i, part) in line.split('`').enumerate() {
            if i % 2 == 1 {
                out.push(part.to_string());
            }
        }
    }
    out
}

fn exists(token: &str) -> bool {
    const BASES: &[&str] = &[
        "",
        "crates/hub-core/src/",
        "crates/hub-core/tests/it/",
        "crates/hub-core/tests/it/frozen/",
        "app/src/lib/",
        "app/src/lib/components/",
        "docs/",
    ];
    let token = token.trim_end_matches('/');
    let (head, tail) = token.split_once('/').map_or((token, ""), |(h, t)| (h, t));
    let expanded = match head {
        "R" => Some("crates/hub-core/src"),
        "L" => Some("app/src/lib"),
        "C" => Some("app/src/lib/components"),
        _ => None,
    };
    if let Some(base) = expanded {
        return root().join(base).join(tail).exists();
    }
    BASES.iter().any(|b| root().join(format!("{b}{token}")).exists())
}

fn is_path(token: &str) -> bool {
    const EXTS: &[&str] = &["rs", "ts", "svelte", "md", "txt", "swift", "cjs", "html", "css", "toml"];
    if token.contains(['*', ' ', '<']) || token == "x.rs" || token == "x/" {
        return false;
    }
    token.ends_with('/') || token.rsplit_once('.').is_some_and(|(_, ext)| EXTS.contains(&ext))
}

fn headings() -> Vec<String> {
    let mut files = vec!["docs/ARCHITECTURE.md".to_string(), "docs/APP.md".to_string()];
    let mut parts: Vec<_> = std::fs::read_dir(root().join("docs/architecture")).into_iter().flatten().flatten().map(|e| format!("docs/architecture/{}", e.file_name().to_string_lossy())).collect();
    parts.sort();
    files.extend(parts);
    files
        .iter()
        .flat_map(|f| read(f).lines().filter_map(|l| l.trim_start_matches('#').strip_prefix(' ').map(str::to_string)).collect::<Vec<_>>())
        .collect()
}

fn all_code() -> String {
    fn walk(dir: &Path, out: &mut String) {
        let Ok(read) = std::fs::read_dir(dir) else { return };
        for entry in read.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if name == "node_modules" || name == "target" || name == "bindings" {
                continue;
            }
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| matches!(e.to_str(), Some("rs" | "ts" | "svelte"))) {
                out.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    let mut out = String::new();
    walk(&root().join("crates"), &mut out);
    walk(&root().join("app/src"), &mut out);
    out
}

#[test]
fn map_is_short() {
    let lines = read("docs/MAP.md").lines().count();
    assert!(lines < MAX_LINES, "docs/MAP.md is {lines} lines; keep it under {MAX_LINES}: one row per feature, no prose");
}

#[test]
fn map_paths_and_sections_exist() {
    let map = read("docs/MAP.md");
    let heads = headings();
    let mut bad = Vec::new();
    for token in spans(&map) {
        if let Some(section) = token.strip_prefix('§') {
            if !heads.iter().any(|h| h == section) {
                bad.push(format!("no section `{section}` in ARCHITECTURE.md or APP.md"));
            }
        } else if is_path(&token) && !exists(&token) {
            bad.push(format!("path `{token}` does not exist"));
        }
    }
    assert!(bad.is_empty(), "docs/MAP.md points at things that are gone:\n{}", bad.join("\n"));
}

#[test]
fn map_names_still_appear_in_the_code() {
    let map = read("docs/MAP.md");
    let code = all_code();
    let mut bad = Vec::new();
    for token in spans(&map) {
        if token.starts_with('§') || is_path(&token) {
            continue;
        }
        let last = token.rsplit("::").next().unwrap_or(&token).trim_end_matches("()");
        let ident = !last.is_empty() && last.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if ident && !code.contains(last) {
            bad.push(format!("`{token}` is not in the code any more"));
        }
    }
    assert!(bad.is_empty(), "docs/MAP.md names things that are gone:\n{}", bad.join("\n"));
}

#[test]
fn every_doc_says_who_it_is_for() {
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(root().join("docs")).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let head: Vec<&str> = text.lines().take(3).collect();
        let ok = head.iter().any(|l| {
            let l = l.trim();
            l.starts_with("<!-- för: ") && l.ends_with("-->") && (l.contains("agent") || l.contains("Christoffer"))
        });
        if !ok {
            missing.push(path.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    assert!(missing.is_empty(), "add `<!-- för: agent -->` or `<!-- för: Christoffer -->` within the first 3 lines of: {missing:?}");
}
