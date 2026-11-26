//! A project's markdown as linked notes, the way Obsidian reads a folder: every `.md` file in the
//! project is a doc, `[[Doc]]`, `[[Doc#Heading]]` and `[[Doc|shown text]]` link docs by name,
//! `#tags` and YAML front matter describe them. The index finds the links both ways, the tags
//! and the headings, and searches the text. It is kept per project and only rereads the files
//! that changed, so the docs tab and the agents' tools can ask it often.
//!
//! The format is plain Obsidian-flavoured markdown on purpose, so the same folder opens in
//! Obsidian, Foam or any editor, and agents read and write it with ordinary file tools.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use anyhow::{Result, bail};
use serde::Serialize;
use ts_rs::TS;

/// More than this many docs is not a docs folder but something vendored; the rest is left out.
const MAX_DOCS: usize = 3000;
/// Bigger markdown files are generated, not written; they are listed but not read.
const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct VaultMap {
    pub docs: Vec<DocEntry>,
    /// Every tag with how many docs carry it, most used first.
    pub tags: Vec<TagCount>,
    /// Docs that were left out because the project has too many.
    #[ts(type = "number")]
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DocEntry {
    /// Relative to the project, with forward slashes.
    pub rel: String,
    pub title: String,
    /// From the front matter (`summary:` or `description:`), else the first prose.
    pub summary: String,
    pub tags: Vec<String>,
    #[ts(type = "number")]
    pub modified: i64,
    /// Links to other docs that resolve, and links from other docs to this one.
    #[ts(type = "number")]
    pub links_out: usize,
    #[ts(type = "number")]
    pub links_in: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TagCount {
    pub tag: String,
    #[ts(type = "number")]
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Heading {
    #[ts(type = "number")]
    pub level: u8,
    pub text: String,
    /// The anchor the reader gives the heading.
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DocLink {
    /// What the link says, before `#` and `|`: a doc name, a path, or a relative file link.
    pub target: String,
    pub heading: Option<String>,
    /// The doc it leads to, or None when there is no such doc yet.
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Backlink {
    pub from: String,
    pub title: String,
    /// The line around the link or the mention.
    pub context: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Prop {
    pub key: String,
    pub value: String,
}

/// One doc with everything around it, for the reader and for agents.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DocPage {
    pub rel: String,
    pub title: String,
    pub summary: String,
    /// The markdown without its front matter.
    pub body: String,
    pub props: Vec<Prop>,
    pub tags: Vec<String>,
    pub headings: Vec<Heading>,
    pub links: Vec<DocLink>,
    pub backlinks: Vec<Backlink>,
    /// Other docs that name this one without linking it.
    pub mentions: Vec<Backlink>,
    #[ts(type = "number")]
    pub modified: i64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct SearchHit {
    pub rel: String,
    pub title: String,
    /// The section the best match is in.
    pub heading: Option<Heading>,
    /// The matching line, shortened around the match.
    pub snippet: String,
    pub score: f32,
}

/// A parsed doc, kept between requests until its file changes.
#[derive(Debug)]
struct Doc {
    rel: String,
    stem: String,
    title: String,
    summary: String,
    props: Vec<Prop>,
    tags: Vec<String>,
    headings: Vec<Heading>,
    /// Raw links with the line they are on.
    links: Vec<(RawLink, usize)>,
    body: String,
    /// Body lines in lower case, for search.
    lower: Vec<String>,
    modified: i64,
    len: u64,
    truncated: bool,
    /// Lines the front matter took, so body line `n` is file line `n + offset + 1`.
    offset: usize,
    /// The body line of each heading, in the order of `headings`.
    heading_lines: Vec<usize>,
    /// What code the doc says it describes: `covers:` in the front matter (the whole doc, None)
    /// and `<!-- covers: ... -->` lines (the section of the heading above, by index).
    covers: Vec<(Option<usize>, Vec<String>)>,
}

#[derive(Debug, Clone, PartialEq)]
struct RawLink {
    target: String,
    heading: Option<String>,
    /// A `[text](file.md)` link, resolved against the doc's folder rather than by name.
    relative: bool,
}

struct Index {
    docs: Vec<Arc<Doc>>,
    skipped: usize,
}

impl Index {
    fn by_rel(&self, rel: &str) -> Option<&Arc<Doc>> {
        self.docs.iter().find(|d| d.rel == rel)
    }

    /// Where a link leads, the way Obsidian resolves it: a path when it has one, else the doc
    /// with that file name, preferring the one nearest to the doc that links.
    fn resolve(&self, from: &str, link: &RawLink) -> Option<String> {
        let target = link.target.trim().trim_start_matches("./");
        if target.is_empty() {
            return Some(from.to_owned());
        }
        let lower = target.to_lowercase();
        let lower = lower.strip_suffix(".md").unwrap_or(&lower);
        if link.relative {
            let dir = from.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            let joined = normalize(&if dir.is_empty() { lower.to_owned() } else { format!("{dir}/{lower}") });
            return self.docs.iter().find(|d| d.rel.to_lowercase().trim_end_matches(".md") == joined).map(|d| d.rel.clone());
        }
        if lower.contains('/') {
            let tail = format!("/{}", lower.trim_start_matches('/'));
            return self
                .docs
                .iter()
                .filter(|d| {
                    let r = d.rel.to_lowercase();
                    let r = r.trim_end_matches(".md");
                    r == lower.trim_start_matches('/') || r.ends_with(&tail)
                })
                .min_by_key(|d| d.rel.len())
                .map(|d| d.rel.clone());
        }
        let near = |d: &Doc| common_dirs(from, &d.rel);
        self.docs
            .iter()
            .filter(|d| d.stem.to_lowercase() == lower)
            .max_by(|a, b| near(a).cmp(&near(b)).then(b.rel.len().cmp(&a.rel.len())))
            .or_else(|| self.docs.iter().find(|d| d.title.to_lowercase() == lower))
            .map(|d| d.rel.clone())
    }
}

fn common_dirs(a: &str, b: &str) -> usize {
    a.split('/').zip(b.split('/')).take_while(|(x, y)| x == y).count()
}

fn normalize(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

type Cache = Mutex<HashMap<PathBuf, HashMap<String, Arc<Doc>>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Every markdown file in the project, skipping what git ignores, hidden folders and
/// dependencies, read again only when it changed.
fn index(root: &Path) -> Index {
    let mut found: Vec<(String, PathBuf, i64, u64)> = Vec::new();
    let mut skipped = 0;
    let walk = ignore::WalkBuilder::new(root)
        .hidden(true)
        .filter_entry(|e| !matches!(e.file_name().to_str(), Some("node_modules" | "target" | "dist" | "build" | "vendor")))
        .build();
    for entry in walk.flatten() {
        let path = entry.path();
        if !entry.file_type().is_some_and(|t| t.is_file()) || !path.extension().is_some_and(|x| x.eq_ignore_ascii_case("md")) {
            continue;
        }
        if found.len() >= MAX_DOCS {
            skipped += 1;
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else { continue };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let meta = entry.metadata().ok();
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default();
        found.push((rel, path.to_owned(), modified, meta.map(|m| m.len()).unwrap_or_default()));
    }

    let mut cache = cache().lock().unwrap_or_else(|e| e.into_inner());
    let known = cache.entry(root.to_owned()).or_default();
    let mut docs = Vec::with_capacity(found.len());
    let mut fresh = HashMap::with_capacity(found.len());
    for (rel, path, modified, len) in found {
        let doc = match known.get(&rel) {
            Some(d) if d.modified == modified && d.len == len => d.clone(),
            _ => Arc::new(read_doc(&rel, &path, modified, len)),
        };
        fresh.insert(rel, doc.clone());
        docs.push(doc);
    }
    *known = fresh;
    docs.sort_by(|a, b| a.rel.cmp(&b.rel));
    Index { docs, skipped }
}

fn read_doc(rel: &str, path: &Path, modified: i64, len: u64) -> Doc {
    let truncated = len > MAX_BYTES;
    let text = if truncated { String::new() } else { std::fs::read_to_string(path).unwrap_or_default() };
    parse(rel, &text, modified, len, truncated)
}

/// The project's docs with their tags and how they link, newest first.
pub fn map(root: &Path) -> VaultMap {
    let index = index(root);
    let mut links_in: HashMap<String, usize> = HashMap::new();
    let mut links_out: HashMap<&str, usize> = HashMap::new();
    for d in &index.docs {
        let targets: HashSet<String> = d.links.iter().filter_map(|(l, _)| index.resolve(&d.rel, l)).filter(|t| *t != d.rel).collect();
        links_out.insert(&d.rel, targets.len());
        for t in targets {
            *links_in.entry(t).or_default() += 1;
        }
    }
    let mut tags: HashMap<String, (String, usize)> = HashMap::new();
    for d in &index.docs {
        for t in &d.tags {
            tags.entry(t.to_lowercase()).or_insert_with(|| (t.clone(), 0)).1 += 1;
        }
    }
    let mut tags: Vec<TagCount> = tags.into_values().map(|(tag, count)| TagCount { tag, count }).collect();
    tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
    let mut docs: Vec<DocEntry> = index
        .docs
        .iter()
        .map(|d| DocEntry {
            rel: d.rel.clone(),
            title: d.title.clone(),
            summary: d.summary.clone(),
            tags: d.tags.clone(),
            modified: d.modified,
            links_out: links_out.get(d.rel.as_str()).copied().unwrap_or_default(),
            links_in: links_in.get(&d.rel).copied().unwrap_or_default(),
        })
        .collect();
    docs.sort_by_key(|d| std::cmp::Reverse(d.modified));
    VaultMap { docs, tags, skipped: index.skipped }
}

/// One doc, with where its links lead and what links to it. `name` is a path or a doc name, the
/// way a link would say it.
pub fn doc(root: &Path, name: &str) -> Result<DocPage> {
    let index = index(root);
    let rel = match index.by_rel(name) {
        Some(d) => d.rel.clone(),
        None => match index.resolve("", &RawLink { target: name.to_owned(), heading: None, relative: false }) {
            Some(rel) => rel,
            None => bail!("no doc called {name}"),
        },
    };
    let d = index.by_rel(&rel).expect("resolved to a doc").clone();
    let mut links: Vec<DocLink> = Vec::new();
    for (l, _) in &d.links {
        let link = DocLink { target: l.target.clone(), heading: l.heading.clone(), to: index.resolve(&d.rel, l) };
        if !links.iter().any(|x| x.target == link.target && x.heading == link.heading) {
            links.push(link);
        }
    }
    let mut backlinks = Vec::new();
    let mut mentions = Vec::new();
    let name = d.title.to_lowercase();
    let stem = d.stem.to_lowercase();
    for other in index.docs.iter().filter(|o| o.rel != d.rel) {
        let lines: Vec<&str> = other.body.lines().collect();
        let mut linked = false;
        for (l, n) in &other.links {
            if index.resolve(&other.rel, l).as_deref() == Some(&d.rel) {
                backlinks.push(Backlink { from: other.rel.clone(), title: other.title.clone(), context: context(lines.get(*n).unwrap_or(&"")) });
                linked = true;
                break;
            }
        }
        if linked || name.chars().count() < 4 {
            continue;
        }
        if let Some(n) = other.lower.iter().position(|l| mentions_name(l, &name) || (stem != name && stem.len() >= 4 && mentions_name(l, &stem))) {
            mentions.push(Backlink { from: other.rel.clone(), title: other.title.clone(), context: context(lines.get(n).unwrap_or(&"")) });
        }
    }
    Ok(DocPage {
        rel: d.rel.clone(),
        title: d.title.clone(),
        summary: d.summary.clone(),
        body: d.body.clone(),
        props: d.props.clone(),
        tags: d.tags.clone(),
        headings: d.headings.clone(),
        links,
        backlinks,
        mentions,
        modified: d.modified,
        truncated: d.truncated,
    })
}

/// The name as a whole word, outside a link.
fn mentions_name(line: &str, name: &str) -> bool {
    let mut from = 0;
    while let Some(at) = line[from..].find(name) {
        let start = from + at;
        let end = start + name.len();
        let before = line[..start].chars().next_back();
        let after = line[end..].chars().next();
        let word = !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric);
        let in_link = line[..start].ends_with("[[") || line[..start].ends_with('[') || line[..start].ends_with('/');
        if word && !in_link {
            return true;
        }
        from = end;
    }
    false
}

/// Docs matching every word of `query`, the last word as a prefix since it is still being typed.
/// `#tag` words only match docs with that tag. Title hits rank above headings, headings above text.
pub fn search(root: &Path, query: &str, limit: usize) -> Vec<SearchHit> {
    let index = index(root);
    let mut tags: Vec<String> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    for w in query.split_whitespace() {
        match w.strip_prefix('#') {
            Some(t) if !t.is_empty() => tags.push(t.to_lowercase()),
            _ => words.push(w.to_lowercase()),
        }
    }
    if words.is_empty() && tags.is_empty() {
        return Vec::new();
    }
    let total = index.docs.len().max(1) as f32;
    // Rarer words count for more.
    let weight: Vec<f32> = words
        .iter()
        .map(|w| {
            let with = index.docs.iter().filter(|d| d.lower.iter().any(|l| l.contains(w.as_str())) || d.title.to_lowercase().contains(w.as_str())).count();
            (1.0 + total / (1.0 + with as f32)).ln()
        })
        .collect();
    let mut hits: Vec<SearchHit> = Vec::new();
    for d in &index.docs {
        if !tags.iter().all(|t| d.tags.iter().any(|dt| dt.to_lowercase() == *t || dt.to_lowercase().starts_with(&format!("{t}/")))) {
            continue;
        }
        let title = d.title.to_lowercase();
        let path = d.rel.to_lowercase();
        let mut score = 0.0;
        let mut all = true;
        for (w, weight) in words.iter().zip(&weight) {
            let in_title = title.contains(w.as_str());
            let in_path = path.contains(w.as_str());
            let in_heads = d.headings.iter().any(|h| h.text.to_lowercase().contains(w.as_str()));
            let count = d.lower.iter().map(|l| l.matches(w.as_str()).count()).sum::<usize>();
            if !in_title && !in_path && !in_heads && count == 0 {
                all = false;
                break;
            }
            let starts = title.starts_with(w.as_str()) || title.split(|c: char| !c.is_alphanumeric()).any(|t| t.starts_with(w.as_str()));
            score += weight
                * (if in_title { 8.0 } else { 0.0 }
                    + if starts { 4.0 } else { 0.0 }
                    + if in_path { 2.0 } else { 0.0 }
                    + if in_heads { 3.0 } else { 0.0 }
                    + (1.0 + count as f32).ln());
        }
        if !all {
            continue;
        }
        if words.is_empty() {
            score = 1.0;
        }
        let (line, heading) = best_line(d, &words);
        let snippet = match line.filter(|&n| !d.body.lines().nth(n).unwrap_or("").trim_start().starts_with('#')) {
            Some(n) => around(d.body.lines().nth(n).unwrap_or(""), words.first().map(String::as_str).unwrap_or("")),
            None => d.summary.clone(),
        };
        hits.push(SearchHit { rel: d.rel.clone(), title: d.title.clone(), heading, snippet, score });
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.rel.cmp(&b.rel)));
    hits.truncate(limit);
    hits
}

/// The body line holding the most query words, and the heading it sits under.
fn best_line(d: &Doc, words: &[String]) -> (Option<usize>, Option<Heading>) {
    if words.is_empty() {
        return (None, None);
    }
    let mut best: Option<(usize, usize)> = None;
    for (n, l) in d.lower.iter().enumerate() {
        let hits = words.iter().filter(|w| l.contains(w.as_str())).count();
        if hits > 0 && best.is_none_or(|(_, b)| hits > b) {
            best = Some((n, hits));
        }
    }
    let Some((line, _)) = best else { return (None, None) };
    // The heading is the last one at or above the line.
    let mut heading = None;
    let mut seen = 0;
    let mut fence = false;
    for (n, l) in d.body.lines().enumerate().take(line + 1) {
        let t = l.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if !fence && let Some((level, text)) = heading_of(t) {
            if n <= line {
                heading = d.headings.get(seen).cloned().or(Some(Heading { level, slug: slug(&text), text }));
            }
            seen += 1;
        }
    }
    (Some(line), heading)
}

fn heading_of(line: &str) -> Option<(u8, String)> {
    heading(line).map(|(l, t)| (l, plain_heading(t)))
}

/// About 160 characters of the line around the first match.
fn around(line: &str, word: &str) -> String {
    let line = line.trim();
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= 160 {
        return line.to_owned();
    }
    let lower = line.to_lowercase();
    let at = lower.find(word).map(|b| lower[..b].chars().count()).unwrap_or(0);
    let start = at.saturating_sub(60);
    let end = (start + 160).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    if start > 0 {
        out = format!("…{}", out.trim_start());
    }
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// The docs as an outline for an agent: folders, each doc's title, summary, tags and links.
/// Short enough to read in one go before deciding what to open.
pub fn outline(root: &Path) -> String {
    let index = index(root);
    let vault = map(root);
    if vault.docs.is_empty() {
        return "This project has no markdown docs yet.".to_owned();
    }
    let mut docs = vault.docs.clone();
    docs.sort_by(|a, b| a.rel.cmp(&b.rel));
    let mut out = format!("{} docs. Link docs as [[Doc name]] or [[Doc name#Heading]].\n", docs.len());
    let mut folder = String::from("\u{0}");
    for d in &docs {
        let dir = d.rel.rsplit_once('/').map(|(f, _)| f).unwrap_or("").to_owned();
        if dir != folder {
            out.push_str(&format!("\n{}/\n", if dir.is_empty() { "." } else { &dir }));
            folder = dir;
        }
        let summary: String = d.summary.chars().take(140).collect();
        out.push_str(&format!("- {} ({})", d.title, d.rel));
        if !summary.is_empty() {
            out.push_str(&format!(": {summary}"));
        }
        if !d.tags.is_empty() {
            out.push_str(&format!(" #{}", d.tags.join(" #")));
        }
        if d.links_in > 0 {
            out.push_str(&format!(" [{} linking here]", d.links_in));
        }
        out.push('\n');
    }
    let missing: HashSet<String> = index
        .docs
        .iter()
        .flat_map(|d| d.links.iter().filter(|(l, _)| !l.relative && index.resolve(&d.rel, l).is_none()).map(|(l, _)| l.target.clone()))
        .collect();
    if !missing.is_empty() {
        let mut missing: Vec<String> = missing.into_iter().collect();
        missing.sort();
        out.push_str(&format!("\nLinked but not written yet: {}\n", missing.join(", ")));
    }
    if vault.skipped > 0 {
        out.push_str(&format!("\n({} more markdown files were left out.)\n", vault.skipped));
    }
    out
}

/// Checks that `rel` is a markdown file inside the folder: relative, no `..`, ending in `.md`.
pub fn check_rel(rel: &str) -> Result<()> {
    let parts: Vec<&str> = rel.split('/').collect();
    if rel.starts_with('/') || rel.contains('\\') || rel.contains(':') || parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..") {
        bail!("{rel} is not a path inside the folder");
    }
    if !rel.to_lowercase().ends_with(".md") {
        bail!("only markdown docs can be written here");
    }
    Ok(())
}

/// Writes a doc the user edited. A new doc's folders are made.
pub fn save(root: &Path, rel: &str, body: &str) -> Result<()> {
    check_rel(rel)?;
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, body)?;
    Ok(())
}

/// A doc for an agent: its text with a footer of where its links lead and what links to it.
pub fn read_for_agent(root: &Path, name: &str) -> Result<String> {
    let page = doc(root, name)?;
    let mut out = format!("# {} ({})\n", page.title, page.rel);
    for p in &page.props {
        out.push_str(&format!("{}: {}\n", p.key, p.value));
    }
    out.push('\n');
    out.push_str(&page.body);
    if page.truncated {
        out.push_str("\n\n(The file is too big to show.)");
    }
    let resolved: Vec<String> = page.links.iter().filter_map(|l| l.to.clone()).collect::<HashSet<_>>().into_iter().collect();
    let missing: Vec<&str> = page.links.iter().filter(|l| l.to.is_none()).map(|l| l.target.as_str()).collect();
    out.push_str("\n\n---\n");
    if !resolved.is_empty() {
        out.push_str(&format!("Links to: {}\n", resolved.join(", ")));
    }
    if !missing.is_empty() {
        out.push_str(&format!("Links to docs not written yet: {}\n", missing.join(", ")));
    }
    if page.backlinks.is_empty() {
        out.push_str("Nothing links here yet.\n");
    } else {
        out.push_str("Linked from:\n");
        for b in &page.backlinks {
            out.push_str(&format!("- {} ({}): {}\n", b.title, b.from, b.context));
        }
    }
    if !page.mentions.is_empty() {
        out.push_str(&format!(
            "Named without a link in: {}\n",
            page.mentions.iter().map(|m| m.from.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(out)
}

/// Search results for an agent, one per line with the section and the matching text.
pub fn search_for_agent(root: &Path, query: &str) -> String {
    let hits = search(root, query, 15);
    if hits.is_empty() {
        return format!("No docs match \"{query}\".");
    }
    hits.iter()
        .map(|h| {
            let at = h.heading.as_ref().map(|x| format!(" › {}", x.text)).unwrap_or_default();
            format!("- {}{at} ({}): {}", h.title, h.rel, h.snippet)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

mod parsing;
pub use parsing::*;

#[cfg(test)]
mod tests;
