//! Which docs describe which code. A doc says it with `covers:` in its front matter (the whole
//! doc) or a `<!-- covers: path, folder/, glob/**/*.rs -->` line under a heading (that heading's
//! section). Agents ask `docs_for` before they change a file, so they know what to keep true,
//! and Hub uses the same map after a turn to see whether code changed under a doc nobody looked at.

use std::path::Path;

use regex::Regex;

use crate::model::{Event, EventKind};
use crate::vault::{Covering, coverings};

/// Whether `pattern` (from a `covers:` line) takes in `path` (relative, forward slashes). A plain
/// path matches itself and, as a folder, everything under it; `*` matches within one folder,
/// `**` across folders.
pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim().trim_start_matches("./");
    let path = path.trim_start_matches("./");
    if pattern.is_empty() {
        return false;
    }
    if !pattern.contains(['*', '?']) {
        let folder = pattern.trim_end_matches('/');
        return path == folder || path.strip_prefix(folder).is_some_and(|rest| rest.starts_with('/'));
    }
    let mut re = String::from("^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    re.push_str("(?:.*/)?");
                } else {
                    re.push_str(".*");
                }
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            c => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    re.push('$');
    Regex::new(&re).is_ok_and(|r| r.is_match(path))
}

/// The docs and sections that cover `path`, most specific (a section) first.
pub fn for_path<'a>(all: &'a [Covering], path: &str) -> Vec<&'a Covering> {
    let mut found: Vec<&Covering> = all.iter().filter(|c| c.patterns.iter().any(|p| matches(p, path))).collect();
    found.sort_by_key(|c| c.heading.is_none());
    found
}

/// Where a covering is, for a person or an agent: `docs/ARCHITECTURE.md › Proof before done`.
pub fn place(c: &Covering) -> String {
    match &c.heading {
        Some(h) => format!("{} › {}", c.rel, h.text),
        None => c.rel.clone(),
    }
}

/// Longer sections are cut to this many lines for `docs_for`; the agent reads the rest by line.
const SHOW_LINES: usize = 60;

/// What the docs say about the files an agent is about to change, for `docs_for`.
pub fn for_agent(root: &Path, paths: &[String]) -> String {
    let all = coverings(root);
    if all.is_empty() {
        return "No doc in this project says which code it covers yet. When you write or update a doc about some code, add \
                `covers: path/to/file.rs, folder/` to its front matter, or a `<!-- covers: ... -->` line right under the \
                heading of the section about it, so the next agent finds it."
            .to_owned();
    }
    let mut out = String::new();
    let mut shown: Vec<(String, usize)> = Vec::new();
    let mut bare: Vec<&str> = Vec::new();
    for path in paths.iter().map(|p| p.trim().trim_start_matches("./")).filter(|p| !p.is_empty()) {
        let found = for_path(&all, path);
        if found.is_empty() {
            bare.push(path);
            continue;
        }
        out.push_str(&format!("{path} is described in:\n"));
        for c in found {
            out.push_str(&format!("- {} (lines {}-{})\n", place(c), c.first, c.last));
            let key = (c.rel.clone(), c.first);
            if !shown.contains(&key) {
                shown.push(key);
            }
        }
        out.push('\n');
    }
    for (rel, first) in &shown {
        let Some(c) = all.iter().find(|c| &c.rel == rel && c.first == *first) else { continue };
        let lines: Vec<&str> = c.text.lines().collect();
        out.push_str(&format!("--- {} (lines {}-{}) ---\n", place(c), c.first, c.last));
        out.push_str(&lines[..lines.len().min(SHOW_LINES)].join("\n"));
        if lines.len() > SHOW_LINES {
            out.push_str(&format!("\n(… {} more lines, read them in the file)", lines.len() - SHOW_LINES));
        }
        out.push_str("\n\n");
    }
    if !bare.is_empty() {
        out.push_str(&format!(
            "No doc covers {}. If the change is something a doc should describe, write it into the doc that fits \
             (docs_map, docs_search) and add the path to that doc's `covers:`.\n",
            bare.join(", ")
        ));
    }
    if !shown.is_empty() {
        out.push_str("When your change makes any of this untrue or incomplete, update it in the same turn.\n");
    }
    out.trim_end().to_owned()
}

/// What each tool call in a turn named (title, input and locations), with forward slashes, to see
/// which docs the agent opened or asked about.
pub fn looked(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter(|e| e.kind == EventKind::Update && matches!(crate::model::update_kind(&e.data), Some("tool_call" | "tool_call_update")))
        .map(|e| {
            let d = &e.data;
            format!("{} {} {}", d["title"].as_str().unwrap_or_default(), d["rawInput"], d["locations"]).replace("\\\\", "/").replace('\\', "/")
        })
        .collect()
}

/// A section that covers code a turn changed, with those files.
#[derive(Debug, Clone)]
pub struct Touched {
    pub covering: Covering,
    pub files: Vec<String>,
}

/// The sections covering the `code` a turn changed, each with whether the turn kept it in mind:
/// it changed the doc, opened it, or asked `docs_for` about the file.
pub fn touched(root: &Path, code: &[String], changed: &[String], looked: &[String]) -> Vec<(Touched, bool)> {
    let all = coverings(root);
    let mut out: Vec<(Touched, bool)> = Vec::new();
    for path in code {
        for c in for_path(&all, path) {
            match out.iter_mut().find(|(t, _)| t.covering.rel == c.rel && t.covering.first == c.first) {
                Some((t, _)) => t.files.push(path.clone()),
                None => out.push((Touched { covering: c.clone(), files: vec![path.clone()] }, false)),
            }
        }
    }
    for (t, seen) in &mut out {
        *seen = kept_in_mind(&t.covering.rel, &t.files, changed, looked);
    }
    out
}

/// The turn changed the doc, opened it, or asked `docs_for` about one of the files.
pub fn kept_in_mind(doc: &str, files: &[String], changed: &[String], looked: &[String]) -> bool {
    changed.iter().any(|c| c == doc)
        || looked.iter().any(|l| l.contains(doc) || (l.contains("docs_for") && files.iter().any(|f| l.contains(f.as_str()))))
}

/// `covers:` entries that match no file in the project any more, as `doc › section: pattern`.
pub fn stale(root: &Path) -> Vec<String> {
    let all = coverings(root);
    if all.is_empty() {
        return Vec::new();
    }
    let files = files(root);
    let mut out = Vec::new();
    for c in &all {
        for p in &c.patterns {
            if !files.iter().any(|f| matches(p, f)) {
                out.push(format!("{}: {p}", place(c)));
            }
        }
    }
    out
}

/// Every file in the project that git does not ignore, relative with forward slashes.
fn files(root: &Path) -> Vec<String> {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|e| !matches!(e.file_name().to_str(), Some(".git" | "node_modules" | "target")))
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| e.path().strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn patterns_match_files_folders_and_globs() {
        assert!(matches("crates/hub-core/src/proof.rs", "crates/hub-core/src/proof.rs"));
        assert!(!matches("crates/hub-core/src/proof.rs", "crates/hub-core/src/proof.rs.bak"));
        assert!(matches("crates/hub-core/src/engine", "crates/hub-core/src/engine/acp.rs"));
        assert!(matches("crates/hub-core/src/engine/", "crates/hub-core/src/engine/acp.rs"));
        assert!(!matches("crates/hub-core/src/eng", "crates/hub-core/src/engine/acp.rs"));
        assert!(matches("app/src/lib/components/Docs*.svelte", "app/src/lib/components/DocsView.svelte"));
        assert!(!matches("app/src/*.svelte", "app/src/lib/X.svelte"));
        assert!(matches("app/**/*.svelte", "app/src/lib/X.svelte"));
        assert!(matches("app/**/X.svelte", "app/X.svelte"));
        assert!(matches("./README.md", "README.md"));
    }

    #[test]
    fn finds_sections_and_whole_docs_with_their_lines() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "src/proof.rs", "");
        write(root, "src/leak.rs", "");
        write(
            root,
            "docs/arch.md",
            "---\nsummary: How it works\ncovers: src/\n---\n# Arch\n\nIntro.\n\n## Proof\n<!-- covers: src/proof.rs -->\nProof text.\n\n### Detail\nMore.\n\n## Leak\n<!-- covers: src/leak.rs, src/gone.rs -->\nLeak text.\n",
        );
        let all = coverings(root);
        assert_eq!(all.len(), 3);
        let found = for_path(&all, "src/proof.rs");
        assert_eq!(found.len(), 2);
        assert_eq!(place(found[0]), "docs/arch.md › Proof");
        // The section runs through its subheadings to the next heading at its level.
        assert_eq!((found[0].first, found[0].last), (9, 14));
        assert!(found[0].text.contains("More.") && !found[0].text.contains("Leak"));
        assert_eq!(place(found[1]), "docs/arch.md");
        let text = for_agent(root, &["src/proof.rs".into(), "other/x.rs".into()]);
        assert!(text.contains("docs/arch.md › Proof (lines 9-14)"));
        assert!(text.contains("No doc covers other/x.rs"));
        assert_eq!(stale(root), vec!["docs/arch.md › Leak: src/gone.rs".to_owned()]);
    }

    #[test]
    fn a_turn_keeps_a_doc_in_mind_by_changing_reading_or_asking() {
        let files = vec!["src/proof.rs".to_owned()];
        assert!(kept_in_mind("docs/arch.md", &files, &["docs/arch.md".into()], &[]));
        assert!(kept_in_mind("docs/arch.md", &files, &[], &["Read C:/dev/hub/docs/arch.md {}".into()]));
        assert!(kept_in_mind("docs/arch.md", &files, &[], &["mcp__hub__docs_for {\"paths\":[\"src/proof.rs\"]}".into()]));
        assert!(!kept_in_mind("docs/arch.md", &files, &["src/proof.rs".into()], &["Read src/proof.rs".into(), "mcp__hub__docs_for {\"paths\":[\"src/x.rs\"]}".into()]));
        let windows = Event {
            thread_id: "t".into(),
            seq: 1,
            ts: 0,
            kind: EventKind::Update,
            data: serde_json::json!({ "sessionUpdate": "tool_call", "title": "Read", "rawInput": { "file_path": "D:\\Dev\\hub\\docs\\arch.md" } }),
        };
        assert!(kept_in_mind("docs/arch.md", &files, &[], &looked(&[windows])));
    }

    /// Hub's own docs: every `covers:` still points at something.
    #[test]
    fn hubs_own_covers_point_at_real_files() {
        let root = crate::source_tree::root();
        let stale = stale(&root);
        assert!(stale.is_empty(), "these covers: match no file any more, fix the doc:\n{}", stale.join("\n"));
    }
}
