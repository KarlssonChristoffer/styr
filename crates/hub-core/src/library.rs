//! The library: one folder of linked notes about everything the user runs and builds, the tech it
//! is built with, the machines it runs on and how they connect. It is written mainly for agents:
//! they look things up in it before they search elsewhere, and write back what they learn, so the
//! next agent starts from a proper understanding instead of from scratch.
//!
//! It is the same Obsidian-flavoured markdown as a project's docs (`vault.rs`), so it opens in
//! Obsidian too. Hub writes what it knows for a fact into each note, between `<!-- hub:facts -->`
//! and `<!-- /hub:facts -->`: which projects use a technology, a project's folder and stack, a
//! machine's address. That part is rewritten every time Hub looks, so it never goes stale; the
//! rest of a note belongs to the agents and the user. Hub adds a note the first time it sees
//! something new, and `log.md` records every change, newest first.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::model::now_ms;

/// Where the user put the library, when not `<projects root>/library`.
pub const ROOT_KEY: &str = "library.root";

const FACTS_START: &str = "<!-- hub:facts -->";
const FACTS_END: &str = "<!-- /hub:facts -->";

/// What Hub tells every agent about the library as it connects.
pub const RULE: &str = "The library: the user keeps one shared library of linked markdown notes about \
everything he runs and builds: his repos, the tech they use, his tools and machines, and how they connect. \
It is meant for you. Before you search the web or read a lot of code to answer a question about his setup \
or a technology he uses, look in it (library_search, library_read). When you learn something that will \
still be true next week, such as what a repo is for, how two parts connect, why a choice was made, or a \
clear explanation of a technology the way he uses it, write it back with library_write: update the note \
that exists rather than adding another, link related notes with [[Name]], and give a source for facts \
you looked up. Keep notes short and correct; fix what you find wrong.";

/// A note Hub knows about: what kind of thing it is and the facts it can vouch for.
struct Known {
    rel: String,
    title: String,
    kind: &'static str,
    facts: String,
}

/// Something Hub can describe from what it already knows.
pub struct Source {
    pub projects: Vec<(String, PathBuf)>,
    pub machines: Vec<crate::model::HostInfo>,
}

/// A file name for a note's title. Characters Windows does not allow in names become `-`.
pub fn file_name(title: &str) -> String {
    let name: String = title.trim().chars().map(|c| if r#"/\:*?"<>|"#.contains(c) || c.is_control() { '-' } else { c }).collect();
    let name = name.trim_matches(['.', ' ']).to_owned();
    if name.is_empty() { "Untitled".into() } else { name }
}

fn known(source: &Source) -> Vec<Known> {
    let mut out: Vec<Known> = Vec::new();
    let mut tech: Vec<(String, Vec<String>)> = Vec::new();
    for (name, path) in &source.projects {
        let stack = crate::project_folder::stack(path);
        for s in &stack {
            match tech.iter_mut().find(|(t, _)| t == s) {
                Some((_, users)) => users.push(name.clone()),
                None => tech.push((s.clone(), vec![name.clone()])),
            }
        }
        let mut facts = format!("- Folder: `{}`\n", path.display());
        if !stack.is_empty() {
            facts += &format!("- Built with: {}\n", links(&stack));
        }
        if let Some(remote) = crate::git::run(path, &["remote", "get-url", "origin"]) {
            let url = remote.trim().to_owned();
            if !url.is_empty() {
                facts += &format!("- Remote: {url}\n");
            }
        }
        out.push(Known { rel: format!("repos/{}.md", file_name(name)), title: name.clone(), kind: "repo", facts });
    }
    for (name, users) in tech {
        let facts = format!("- Used in: {}\n", links(&users));
        out.push(Known { rel: format!("tech/{}.md", file_name(&name)), title: name, kind: "tech", facts });
    }
    for m in &source.machines {
        let mut facts = String::new();
        facts += if m.local { "- The machine Hub runs on.\n" } else { "" };
        if !m.address.is_empty() {
            facts += &format!("- Reached by: `{}`\n", m.address);
        }
        if let Some(v) = &m.version {
            facts += &format!("- hubd: {v}\n");
        }
        out.push(Known { rel: format!("machines/{}.md", file_name(&m.name)), title: m.name.clone(), kind: "machine", facts });
    }
    out
}

fn links(names: &[String]) -> String {
    names.iter().map(|n| format!("[[{n}]]")).collect::<Vec<_>>().join(", ")
}

/// Makes the library if it is not there, adds a note for anything new and rewrites the facts in
/// each note Hub knows about. Returns how many notes it added.
pub fn refresh(root: &Path, source: &Source) -> Result<usize> {
    std::fs::create_dir_all(root).with_context(|| format!("could not make the library at {}", root.display()))?;
    let mut added = 0;
    let known = known(source);
    for k in &known {
        let path = root.join(&k.rel);
        let block = format!("{FACTS_START}\n## What Hub knows\n\n{}{FACTS_END}", k.facts);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => {
                let next = with_facts(&text, &block);
                if next == text {
                    continue;
                }
                next
            }
            Err(_) => {
                added += 1;
                format!(
                    "---\nkind: {}\n---\n# {}\n\n_Not written yet. Ask about it, and an agent fills this in._\n\n{block}\n",
                    k.kind, k.title
                )
            }
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, text)?;
    }
    write_index(root, &known)?;
    if !root.join("log.md").exists() {
        std::fs::write(root.join("log.md"), "---\ntitle: Log\nsummary: What changed in the library, newest first.\n---\n# Log\n\n")?;
    }
    if added > 0 {
        log(root, &format!("Hub added {added} notes for things it found"))?;
    }
    Ok(added)
}

/// `text` with its facts block replaced by `block`, or with `block` added at the end, after
/// what the agents and the user wrote.
fn with_facts(text: &str, block: &str) -> String {
    if let (Some(a), Some(b)) = (text.find(FACTS_START), text.find(FACTS_END))
        && a < b
    {
        return format!("{}{block}{}", &text[..a], &text[b + FACTS_END.len()..]);
    }
    format!("{}\n\n{block}\n", text.trim_end())
}

/// The front page: what the library is, then every note Hub knows of, by kind.
fn write_index(root: &Path, known: &[Known]) -> Result<()> {
    let path = root.join("index.md");
    let mut block = format!("{FACTS_START}\n");
    for (kind, heading) in [("repo", "Repos"), ("tech", "Tech"), ("machine", "Machines")] {
        let mut names: Vec<&str> = known.iter().filter(|k| k.kind == kind).map(|k| k.title.as_str()).collect();
        if names.is_empty() {
            continue;
        }
        names.sort_by_key(|n| n.to_lowercase());
        block += &format!("## {heading}\n\n{}\n\n", names.iter().map(|n| format!("[[{n}]]")).collect::<Vec<_>>().join(" · "));
    }
    block += FACTS_END;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => with_facts(&text, &block),
        Err(_) => format!(
            "---\ntitle: Library\nsummary: What is in the library and where to start.\n---\n# Library\n\nEverything I run and build, the tech it uses and how it connects. \
             Agents look things up here first and write back what they learn. Ask anything in the box above.\n\n{block}\n"
        ),
    };
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, text)?;
    }
    Ok(())
}

/// Adds a line to the top of `log.md`.
pub fn log(root: &Path, line: &str) -> Result<()> {
    let path = root.join("log.md");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| "---\ntitle: Log\nsummary: What changed in the library, newest first.\n---\n# Log\n\n".into());
    let entry = format!("- {} · {line}\n", crate::activity::date_of(now_ms(), 0));
    let at = text.find("\n- ").map(|i| i + 1).unwrap_or(text.len());
    std::fs::write(&path, format!("{}{entry}{}", &text[..at], &text[at..]))?;
    Ok(())
}

/// Where a note an agent names goes: an existing note by path or name, else a new path. A name
/// without a folder lands in `notes/`.
fn place(root: &Path, page: &str) -> Result<String> {
    let page = page.trim().trim_start_matches(['/', '\\']).replace('\\', "/");
    if page.is_empty() {
        bail!("say which note, e.g. tech/Node.js.md or Node.js");
    }
    if let Ok(found) = crate::vault::doc(root, &page) {
        return Ok(found.rel);
    }
    let rel = if page.contains('/') { page } else { format!("notes/{}", file_name(&page)) };
    let rel = if rel.to_lowercase().ends_with(".md") { rel } else { format!("{rel}.md") };
    crate::vault::check_rel(&rel)?;
    Ok(rel)
}

/// Writes a note for an agent. The facts block Hub keeps stays as it is, whatever the agent sent.
pub fn write(root: &Path, page: &str, content: &str, why: &str) -> Result<String> {
    let rel = place(root, page)?;
    let path = root.join(&rel);
    let old = std::fs::read_to_string(&path).ok();
    let mut text = content.trim_end().to_owned() + "\n";
    if let Some(old) = &old
        && let (Some(a), Some(b)) = (old.find(FACTS_START), old.find(FACTS_END))
    {
        text = with_facts(&text, &old[a..b + FACTS_END.len()]);
    }
    // Passing a learning check is the user's, not the agent's: a rewrite keeps the mark.
    if let Some(old) = &old
        && let Some(date) = prop(old, "understood")
        && prop(&text, "understood").is_none()
    {
        text = set_prop(&text, "understood", &date);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, &text)?;
    let name = rel.rsplit('/').next().unwrap_or(&rel).trim_end_matches(".md").to_owned();
    let what = if old.is_some() { "updated" } else { "added" };
    let why = why.trim();
    log(root, &format!("{what} [[{name}]]{}", if why.is_empty() { String::new() } else { format!(": {why}") }))?;
    Ok(rel)
}

/// Marks a note as understood today, after the user passed a learning check on it.
pub fn understood(root: &Path, page: &str, how: &str) -> Result<String> {
    let rel = crate::vault::doc(root, page)?.rel;
    let path = root.join(&rel);
    let text = std::fs::read_to_string(&path)?;
    let today = crate::activity::date_of(now_ms(), 0);
    std::fs::write(&path, set_prop(&text, "understood", &today))?;
    let name = rel.rsplit('/').next().unwrap_or(&rel).trim_end_matches(".md").to_owned();
    let how = how.trim();
    log(root, &format!("understood [[{name}]]{}", if how.is_empty() { String::new() } else { format!(": {how}") }))?;
    Ok(rel)
}

/// A front matter value.
fn prop(text: &str, key: &str) -> Option<String> {
    let rest = text.strip_prefix("---\n")?;
    let head = &rest[..rest.find("\n---")?];
    head.lines().find_map(|l| l.strip_prefix(&format!("{key}:"))).map(|v| v.trim().to_owned())
}

/// `text` with front matter `key: value`, replacing the key when it is there.
fn set_prop(text: &str, key: &str, value: &str) -> String {
    let line = format!("{key}: {value}");
    if let Some(rest) = text.strip_prefix("---\n")
        && let Some(end) = rest.find("\n---")
    {
        let mut props: Vec<&str> = rest[..end].lines().filter(|l| !l.starts_with(&format!("{key}:"))).collect();
        props.push(&line);
        return format!("---\n{}{}", props.join("\n"), &rest[end..]);
    }
    format!("---\n{line}\n---\n{text}")
}

/// The first message of a thread that teaches the user one note and checks they got it.
pub fn learn_prompt(note: &str) -> String {
    format!(
        "Teach me [[{note}]] from my library, then check that I understood it. Keep it short and talk like a friend, not a textbook.\n\n\
         1. Read the note with library_read (and the notes it links to if you need them). If the note is thin, look it up and \
         improve it with library_write first.\n\
         2. Explain it in a few short paragraphs, tied to how I actually use it in my own repos and machines.\n\
         3. Then test me, one question at a time, waiting for my answer each time: first ask me to explain it back in my own \
         words, then ask me to predict what happens in a concrete case from my own setup, then one tricky question about a \
         common misunderstanding. Grade each answer honestly and say what was right and what was missing.\n\
         4. If I got the gist of all three, call library_understood for the note and tell me I passed. If not, explain what I \
         missed, and offer to try again another day without marking it."
    )
}

/// The first message of a thread that answers a question from the library and writes back.
pub fn ask_prompt(question: &str) -> String {
    format!(
        "{}\n\nAnswer from my library first (library_search, library_read). If it does not have the answer, find out \
         (my repos, my machines, the web), answer, and write what you learned back with library_write, with sources. \
         Link the notes you used.",
        question.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hub-library-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn adds_notes_and_keeps_what_others_wrote() {
        let base = temp("refresh");
        let repo = base.join("app");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("Cargo.toml"), "[package]\nname = \"app\"\n").unwrap();
        let lib = base.join("library");
        let source = Source { projects: vec![("app".into(), repo.clone())], machines: vec![] };
        assert_eq!(refresh(&lib, &source).unwrap(), 2);
        let rust = std::fs::read_to_string(lib.join("tech/Rust.md")).unwrap();
        assert!(rust.contains("Used in: [[app]]"), "{rust}");
        assert!(std::fs::read_to_string(lib.join("index.md")).unwrap().contains("[[Rust]]"));

        // An agent rewrites the note without the facts; Hub's block survives, its text too.
        write(&lib, "Rust", "---\nkind: tech\nsummary: A fast language.\n---\n# Rust\n\nSafe and fast.\n", "explained").unwrap();
        let rust = std::fs::read_to_string(lib.join("tech/Rust.md")).unwrap();
        assert!(rust.contains("Safe and fast.") && rust.contains("Used in: [[app]]"), "{rust}");
        assert_eq!(refresh(&lib, &source).unwrap(), 0);
        assert!(std::fs::read_to_string(lib.join("tech/Rust.md")).unwrap().contains("Safe and fast."));
        assert!(std::fs::read_to_string(lib.join("log.md")).unwrap().contains("updated [[Rust]]: explained"));

        // A new name lands in notes/, and passing a check marks it.
        assert_eq!(write(&lib, "Event loop", "# Event loop\n\nOne thread.\n", "").unwrap(), "notes/Event loop.md");
        understood(&lib, "Event loop", "3 of 3").unwrap();
        let text = std::fs::read_to_string(lib.join("notes/Event loop.md")).unwrap();
        assert!(text.starts_with("---\nunderstood: "), "{text}");
        write(&lib, "Event loop", "---\nkind: concept\n---\n# Event loop\n\nOne thread, many waits.\n", "").unwrap();
        let text = std::fs::read_to_string(lib.join("notes/Event loop.md")).unwrap();
        assert!(text.contains("understood: ") && text.contains("many waits"), "{text}");
        assert!(write(&lib, "../escape", "x", "").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn sets_a_property_in_place() {
        assert_eq!(set_prop("---\nkind: tech\nunderstood: 2020-01-01\n---\n# A\n", "understood", "2026-10-30"), "---\nkind: tech\nunderstood: 2026-10-30\n---\n# A\n");
        assert_eq!(set_prop("# A\n", "understood", "2026-10-30"), "---\nunderstood: 2026-10-30\n---\n# A\n");
    }
}
