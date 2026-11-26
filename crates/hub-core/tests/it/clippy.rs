//! Clippy guard: the total of `cargo clippy` warnings per lint across the workspace may only get
//! fewer. The frozen list (`frozen/clippy.txt`, lines `lint path count`) holds what exists today per
//! file, so you see where; the ratchet (`ratchet.rs`) is the total per lint, so a warning that moves
//! with its code to another file passes, and a lint whose total grew fails, naming the files that rose
//! or are new. Like `rules::`, but it runs clippy, which takes minutes, so it only runs when asked:
//! `HUB_CLIPPY=1 cargo test -p hub-core --test it clippy::`. Run it before you call Rust work done. It
//! is meant for Linux (code behind `cfg(windows)` is not seen there, so the list is written from
//! Linux); other systems skip it. After a clean-up: `HUB_CLIPPY=1 CLIPPY_BLESS=1 ...` rewrites the
//! lines to what is true now; it is refused while any lint's total grew.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use crate::ratchet;

type Hits = BTreeMap<(String, String), usize>;

/// Warnings from `cargo clippy` over the workspace (without the Tauri shell, which does not build
/// everywhere), as `(lint, file) -> count`. A hit seen by several targets (lib and its tests) counts once.
fn clippy_now() -> Hits {
    let root = hub_core::source_tree::root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let out = Command::new(cargo)
        .current_dir(&root)
        .args(["clippy", "--workspace", "--exclude", "hub-app", "--all-targets", "--message-format=json"])
        .output()
        .expect("cargo clippy did not start; is the clippy component installed?");
    let mut seen = BTreeSet::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v["reason"] != "compiler-message" {
            continue;
        }
        let m = &v["message"];
        let (Some(lint), Some(spans)) = (m["code"]["code"].as_str(), m["spans"].as_array()) else { continue };
        let Some(sp) = spans.iter().find(|s| s["is_primary"] == true) else { continue };
        let Some(file) = sp["file_name"].as_str() else { continue };
        if file.starts_with('/') || file.contains(".cargo") || !lint.starts_with("clippy::") {
            continue;
        }
        seen.insert((lint.to_string(), file.replace('\\', "/"), sp["line_start"].as_u64().unwrap_or(0), sp["column_start"].as_u64().unwrap_or(0)));
    }
    let mut hits = Hits::new();
    for (lint, file, _, _) in seen {
        *hits.entry((lint, file)).or_default() += 1;
    }
    hits
}

fn parse(text: &str) -> Hits {
    text.replace("\r\n", "\n")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            Some(((p.next()?.to_string(), p.next()?.to_string()), p.next()?.parse().ok()?))
        })
        .collect()
}

fn render(h: &Hits) -> String {
    h.iter().map(|((l, f), n)| format!("{l} {f} {n}\n")).collect()
}

#[test]
fn clippy_only_gets_fewer_warnings() {
    if std::env::var_os("HUB_CLIPPY").is_none() || !cfg!(target_os = "linux") {
        return;
    }
    let path = hub_core::source_tree::hub_core().join("tests/it/frozen/clippy.txt");
    let now = clippy_now();
    let old = std::fs::read_to_string(&path).map(|t| parse(&t)).ok().filter(|h| !h.is_empty());
    let grown = if std::env::var_os("CLIPPY_BLESS").is_some() {
        // A first write (no list yet) takes everything; later ones rewrite the lines unless a lint's total grew.
        match ratchet::bless(old.as_ref(), &now) {
            Ok(out) => {
                std::fs::write(&path, render(&out)).expect("frozen/clippy.txt could not be written");
                return;
            }
            Err(grown) => grown,
        }
    } else {
        ratchet::grown(&old.unwrap_or_default(), &now)
    };
    let broken: Vec<String> = grown.iter().map(say).collect();
    assert!(broken.is_empty(), "{}\nFix the warning (cargo clippy shows it); do not add a line to frozen/clippy.txt.\nMoving code between files needs nothing; after a clean-up: HUB_CLIPPY=1 CLIPPY_BLESS=1 cargo test -p hub-core --test it clippy::", broken.join("\n"));
}

/// A lint whose total grew, and the files that rose or are new.
fn say(g: &ratchet::Grown) -> String {
    let files: Vec<String> = g
        .files
        .iter()
        .map(|(file, was, n)| match was {
            None => format!("  {file}: new ({n})"),
            Some(o) => format!("  {file}: grew from {o} to {n}"),
        })
        .collect();
    format!("`{}` grew from {} to {} in total:\n{}", g.rule, g.frozen, g.now, files.join("\n"))
}

#[test]
fn the_reader_parses_its_own_list() {
    let h = parse("clippy::too_many_arguments crates/a/src/x.rs 3\n\nclippy::x crates/b.rs 1\n");
    assert_eq!(h.len(), 2);
    assert_eq!(render(&h), "clippy::too_many_arguments crates/a/src/x.rs 3\nclippy::x crates/b.rs 1\n");
}
