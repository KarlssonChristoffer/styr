//! Customs: everything from outside that agents build with passes here. Packages (npm, crates,
//! PyPI), CSS libraries from a CDN, CLI tools, MCP servers, Docker images and web services all
//! go through one list. What is new goes through the review (`customs_review`) when an agent
//! first needs it: what clears has the stamp for every agent on every machine (`merge`; the loop
//! that swaps lists is `Hub::share_customs` in `hub/customs_share.rs`), and only what cannot be
//! verified is held (quarantine) until the user clears it. The whole
//! design is in docs/ARCHITECTURE.md under Customs.
//!
//! The list starts from what projects already use: `import` reads a folder's manifests
//! (package.json, Cargo.toml, requirements.txt) and clears their direct dependencies as
//! `existing`, so nothing that works today is stopped.
//!
//! After each turn `in_turn` finds what the turn brought in: dependencies new in a manifest, and
//! libraries linked from a CDN in the lines it added. `hold` puts anything unknown on the list
//! as held for the review, and the core tells the agent about what stays held or is refused.

use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Result, bail};
use regex::Regex;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::checkpoint::FileDiff;
use crate::customs_log::{Note, note, word};

use crate::ctx::Ctx;
use crate::model::{CustomsEdit, CustomsImport, CustomsItem, CustomsPrefs, CustomsStatus, now_ms};

const KEY: &str = "customs";
/// One change at a time, so two agents asking at once do not lose either request.
static LOCK: Mutex<()> = Mutex::const_new(());
/// How deep `import` looks for manifests: a monorepo's `app/` and `crates/*/` are within reach.
const DEPTH: usize = 3;

async fn load(ctx: &Ctx<'_>) -> Result<Vec<CustomsItem>> {
    match ctx.store.get_value(KEY).await? {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => Ok(Vec::new()),
    }
}

async fn save(ctx: &Ctx<'_>, list: &mut [CustomsItem]) -> Result<()> {
    list.sort_by(|a, b| (&a.kind, &a.name).cmp(&(&b.kind, &b.name)));
    ctx.store.set_value(KEY, &serde_json::to_string(list)?).await
}

const PREFS: &str = "customs.prefs";

pub async fn prefs(ctx: &Ctx<'_>) -> Result<CustomsPrefs> {
    Ok(match ctx.store.get_value(PREFS).await? {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => CustomsPrefs::default(),
    })
}

pub async fn set_prefs(ctx: &Ctx<'_>, prefs: &CustomsPrefs) -> Result<()> {
    ctx.store.set_value(PREFS, &serde_json::to_string(prefs)?).await
}

/// One item, by kind and name as given.
pub async fn get(ctx: &Ctx<'_>, kind: &str, name: &str) -> Result<Option<CustomsItem>> {
    let name = normalize(kind, name);
    Ok(load(ctx).await?.into_iter().find(|m| m.kind == kind && m.name == name))
}

/// Changes one item in place, if it is still on the list, and answers it as it is now.
pub(crate) async fn change(ctx: &Ctx<'_>, kind: &str, name: &str, f: impl FnOnce(&mut CustomsItem)) -> Result<Option<CustomsItem>> {
    let name = normalize(kind, name);
    let _guard = LOCK.lock().await;
    let mut list = load(ctx).await?;
    let Some(m) = find_mut(&mut list, kind, &name) else { return Ok(None) };
    f(m);
    m.updated_at = now_ms();
    let out = m.clone();
    save(ctx, &mut list).await?;
    Ok(Some(out))
}

/// Everything on the list, by kind and name.
pub async fn list(ctx: &Ctx<'_>) -> Result<Vec<CustomsItem>> {
    load(ctx).await
}

/// The same name the way its ecosystem compares names: crates and PyPI ignore case, and PyPI
/// treats `-`, `_` and `.` alike.
pub fn normalize(kind: &str, name: &str) -> String {
    let name = name.trim();
    match kind {
        "crates" => name.to_lowercase(),
        "pypi" => {
            let mut out = String::new();
            for c in name.to_lowercase().chars() {
                let c = if matches!(c, '_' | '.') { '-' } else { c };
                if !(c == '-' && out.ends_with('-')) {
                    out.push(c);
                }
            }
            out
        }
        _ => name.to_owned(),
    }
}

fn clean_kind(kind: &str) -> Result<String> {
    let kind = kind.trim().to_lowercase();
    if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        bail!("kind must be a short word such as npm, crates, pypi, cdn, cli, mcp, docker or service");
    }
    Ok(kind)
}

fn find_mut<'a>(list: &'a mut [CustomsItem], kind: &str, name: &str) -> Option<&'a mut CustomsItem> {
    list.iter_mut().find(|m| m.kind == kind && m.name == name)
}

/// The user adds something, sorts it, or decides on it.
pub async fn edit(ctx: &Ctx<'_>, edit: CustomsEdit) -> Result<CustomsItem> {
    let kind = clean_kind(&edit.kind)?;
    let name = normalize(&kind, &edit.name);
    if name.is_empty() {
        bail!("give it a name");
    }
    let _guard = LOCK.lock().await;
    let mut list = load(ctx).await?;
    let now = now_ms();
    let before = find_mut(&mut list, &kind, &name).map(|m| m.status);
    if before.is_none() {
        list.push(CustomsItem {
            kind: kind.clone(),
            name: name.clone(),
            status: CustomsStatus::Cleared,
            category: String::new(),
            summary: String::new(),
            versions: String::new(),
            reason: String::new(),
            decided_by: "you".into(),
            reviewed_at: None,
            advisories: Vec::new(),
            created_at: now,
            updated_at: now,
        });
    }
    let m = find_mut(&mut list, &kind, &name).expect("just added");
    if let Some(status) = edit.status {
        if status != m.status {
            m.decided_by = "you".into();
        }
        m.status = status;
    }
    let set = |field: &mut String, value: Option<String>| {
        if let Some(v) = value {
            *field = v.trim().to_owned();
        }
    };
    set(&mut m.category, edit.category);
    set(&mut m.summary, edit.summary);
    set(&mut m.versions, edit.versions);
    set(&mut m.reason, edit.reason);
    m.updated_at = now;
    let out = m.clone();
    save(ctx, &mut list).await?;
    drop(_guard);
    match before {
        None => note(ctx.store, Note::new("added", "you").item(&kind, &name).detail(format!("Added as {}", word(out.status)))).await,
        Some(was) if was != out.status => note(ctx.store, Note::new(word(out.status), "you").item(&kind, &name).detail(format!("Was {}", word(was)))).await,
        _ => {}
    }
    Ok(out)
}

pub async fn remove(ctx: &Ctx<'_>, kind: &str, name: &str) -> Result<()> {
    let name = normalize(kind, name);
    let _guard = LOCK.lock().await;
    let mut list = load(ctx).await?;
    list.retain(|m| !(m.kind == kind && m.name == name));
    // Remembered, so another machine that still has it does not bring it back.
    let mut gone = removed(ctx).await?;
    gone.insert(format!("{kind}/{name}"), now_ms());
    ctx.store.set_value(REMOVED, &serde_json::to_string(&gone)?).await?;
    save(ctx, &mut list).await?;
    drop(_guard);
    note(ctx.store, Note::new("removed", "you").item(kind, &name).detail("Taken off the list; an agent can declare it again")).await;
    Ok(())
}

/// What was taken off the list, by `kind/name`, and when.
const REMOVED: &str = "customs.removed";

async fn removed(ctx: &Ctx<'_>) -> Result<std::collections::BTreeMap<String, i64>> {
    Ok(ctx.store.get_value(REMOVED).await?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

/// The list as machines share it: every item, and what was taken off.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Shared {
    pub items: Vec<CustomsItem>,
    pub removed: std::collections::BTreeMap<String, i64>,
}

/// Takes another machine's list in: of the same item the one changed last wins, and what was
/// taken off after it last changed stays off. Answers the list as it is after, for the other
/// machine to take in the same way, so both end up with one list.
pub async fn merge(ctx: &Ctx<'_>, theirs: Shared) -> Result<Shared> {
    let _guard = LOCK.lock().await;
    let mut list = load(ctx).await?;
    let mut gone = removed(ctx).await?;
    let before = (serde_json::to_string(&list)?, gone.len());
    for (key, at) in theirs.removed {
        let at = gone.get(&key).copied().unwrap_or(0).max(at);
        gone.insert(key, at);
    }
    for item in theirs.items {
        match list.iter_mut().find(|m| m.kind == item.kind && m.name == item.name) {
            Some(mine) if mine.updated_at >= item.updated_at => {}
            Some(mine) => *mine = item,
            None => list.push(item),
        }
    }
    list.retain(|m| gone.get(&format!("{}/{}", m.kind, m.name)).is_none_or(|at| m.updated_at > *at));
    // A tombstone older than any machine is likely to be offline is not needed any more.
    let now = now_ms();
    gone.retain(|_, at| now - *at < 90 * 86_400_000);
    list.sort_by(|a, b| (&a.kind, &a.name).cmp(&(&b.kind, &b.name)));
    if serde_json::to_string(&list)? != before.0 || gone.len() != before.1 {
        ctx.store.set_value(REMOVED, &serde_json::to_string(&gone)?).await?;
        save(ctx, &mut list).await?;
    }
    Ok(Shared { items: list, removed: gone })
}

/// This machine's list as it is shared.
pub async fn shared(ctx: &Ctx<'_>) -> Result<Shared> {
    Ok(Shared { items: load(ctx).await?, removed: removed(ctx).await? })
}

/// An agent declares something that is not on the list (`by` "agent", from `thread`), or a
/// package manager asks for it through Hub's registry (`by` "registry"). It is held until the
/// review or the user clears it; something known is answered as it stands.
pub async fn request(ctx: &Ctx<'_>, kind: &str, name: &str, why: &str, summary: &str, by: &'static str, thread: Option<&str>) -> Result<CustomsItem> {
    let kind = clean_kind(kind)?;
    let name = normalize(&kind, name);
    if name.is_empty() {
        bail!("name is missing");
    }
    let _guard = LOCK.lock().await;
    let mut list = load(ctx).await?;
    if let Some(m) = find_mut(&mut list, &kind, &name) {
        return Ok(m.clone());
    }
    let now = now_ms();
    let why = why.trim();
    let m = CustomsItem {
        kind,
        name,
        status: CustomsStatus::Held,
        category: String::new(),
        summary: summary.trim().to_owned(),
        versions: String::new(),
        reason: if why.is_empty() { "Declared by an agent; held for review.".into() } else { format!("Declared: {why}") },
        decided_by: "declared".into(),
        reviewed_at: None,
        advisories: Vec::new(),
        created_at: now,
        updated_at: now,
    };
    list.push(m.clone());
    save(ctx, &mut list).await?;
    drop(_guard);
    let (what, detail) = if by == "registry" { ("asked", String::new()) } else { ("declared", why.to_owned()) };
    note(ctx.store, Note::new(what, by).item(&m.kind, &m.name).detail(detail).thread(thread)).await;
    Ok(m)
}

/// What matches `query` in name, category or summary, cleared first.
pub fn search<'a>(list: &'a [CustomsItem], query: &str, kind: Option<&str>, category: Option<&str>) -> Vec<&'a CustomsItem> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut hits: Vec<&CustomsItem> = list
        .iter()
        .filter(|m| kind.is_none_or(|k| m.kind.eq_ignore_ascii_case(k)))
        .filter(|m| category.is_none_or(|c| m.category.eq_ignore_ascii_case(c)))
        .filter(|m| {
            let text = format!("{} {} {}", m.name, m.category, m.summary).to_lowercase();
            words.iter().all(|w| text.contains(w))
        })
        .collect();
    hits.sort_by_key(|m| (m.status != CustomsStatus::Cleared, m.kind.clone(), m.name.clone()));
    hits
}

/// Clears the direct dependencies of every manifest in `folder` that are not on the list yet.
pub async fn import(ctx: &Ctx<'_>, folder: &Path) -> Result<CustomsImport> {
    if !folder.is_dir() {
        bail!("{} is not a folder", folder.display());
    }
    let found = scan(folder);
    let _guard = LOCK.lock().await;
    let mut list = load(ctx).await?;
    let now = now_ms();
    let (mut added, mut known) = (0, 0);
    for (kind, name) in found {
        if list.iter().any(|m| m.kind == kind && m.name == name) {
            known += 1;
            continue;
        }
        added += 1;
        list.push(CustomsItem {
            kind: kind.into(),
            name,
            status: CustomsStatus::Cleared,
            category: String::new(),
            summary: String::new(),
            versions: String::new(),
            reason: format!("Already used in {}", folder.display()),
            decided_by: "existing".into(),
            reviewed_at: None,
            advisories: Vec::new(),
            created_at: now,
            updated_at: now,
        });
    }
    save(ctx, &mut list).await?;
    drop(_guard);
    note(ctx.store, Note::new("imported", "you").detail(format!("Read {}: cleared {added} already in use, {known} known", folder.display()))).await;
    Ok(CustomsImport { added, known })
}

/// The direct dependencies named in `folder`'s manifests, without duplicates. Files git ignores
/// (node_modules, target) are skipped.
fn scan(folder: &Path) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let walk = ignore::WalkBuilder::new(folder).max_depth(Some(DEPTH)).build();
    for entry in walk.flatten() {
        let path = entry.path();
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        let found = match path.file_name().and_then(|n| n.to_str()) {
            Some("package.json") => npm(&text).into_iter().map(|n| ("npm", n)).collect(),
            Some("Cargo.toml") => cargo(&text).into_iter().map(|n| ("crates", n)).collect(),
            Some("requirements.txt") => requirements(&text).into_iter().map(|n| ("pypi", n)).collect(),
            _ => Vec::new(),
        };
        for (kind, name) in found {
            let name = normalize(kind, &name);
            if !out.iter().any(|(k, n)| *k == kind && *n == name) {
                out.push((kind, name));
            }
        }
    }
    out
}

/// package.json: every kind of dependency except local ones.
fn npm(text: &str) -> Vec<String> {
    let Ok(json) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let mut out = Vec::new();
    for table in ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"] {
        let Some(deps) = json[table].as_object() else { continue };
        for (name, spec) in deps {
            let spec = spec.as_str().unwrap_or_default();
            if !["file:", "link:", "workspace:", "portal:"].iter().any(|p| spec.starts_with(p)) {
                out.push(name.clone());
            }
        }
    }
    out
}

/// Cargo.toml: the dependency tables, without path dependencies (the project's own crates) and
/// without `workspace = true` (counted where the workspace names them).
fn cargo(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_deps = false;
    // `[dependencies.serde]` names the crate in the header; its lines follow.
    let mut table: Option<(String, bool)> = None;
    let flush = |table: &mut Option<(String, bool)>, out: &mut Vec<String>| {
        if let Some((name, local)) = table.take()
            && !local
        {
            out.push(name);
        }
    };
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') {
            flush(&mut table, &mut out);
            let header = line.trim_matches(|c| c == '[' || c == ']').trim();
            let is_deps = |h: &str| h.ends_with("dependencies") && (h == "dependencies" || h.ends_with(".dependencies") || h.ends_with("-dependencies"));
            in_deps = is_deps(header);
            if !in_deps && let Some((head, name)) = header.rsplit_once('.') && is_deps(head) {
                table = Some((name.trim_matches('"').to_owned(), false));
            }
            continue;
        }
        if let Some((_, local)) = table.as_mut() {
            let key = line.split('=').next().unwrap_or_default().trim();
            if key == "path" || key == "workspace" {
                *local = true;
            }
            continue;
        }
        if !in_deps || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.trim().trim_matches('"');
        let (name, dotted) = match key.split_once('.') {
            Some((n, rest)) => (n, Some(rest.trim())),
            None => (key, None),
        };
        let value = value.trim();
        let local = dotted == Some("workspace") || dotted == Some("path") || value.contains("path =") || value.contains("path=") || value.contains("workspace = true") || value.contains("workspace=true");
        if !local && !name.is_empty() {
            out.push(name.to_owned());
        }
    }
    flush(&mut table, &mut out);
    out
}

/// requirements.txt: one requirement a line; options, includes and URLs are skipped.
fn requirements(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.split('#').next().unwrap_or_default().trim())
        .filter(|l| !l.is_empty() && !l.starts_with('-') && !l.contains("://"))
        .filter_map(|l| {
            let end = l.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))).unwrap_or(l.len());
            (end > 0).then(|| l[..end].to_owned())
        })
        .collect()
}

/// Something a turn brought in.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub kind: String,
    pub name: String,
    /// Where, relative to the repository.
    pub path: String,
}

/// What the turn from `base` (a commit in the repository at `root`) to `files` brought in:
/// dependencies a manifest names now and did not before, and libraries the added lines link
/// from a CDN. Each once.
pub fn in_turn(root: &Path, base: &str, files: &[FileDiff]) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    let mut push = |kind: &str, name: String, path: &str| {
        let name = normalize(kind, &name);
        if !out.iter().any(|f| f.kind == kind && f.name == name) {
            out.push(Found { kind: kind.into(), name, path: path.into() });
        }
    };
    for file in files {
        let path = file.path.as_str();
        let base_name = path.rsplit('/').next().unwrap_or(path);
        if let Some((kind, parse)) = manifest(base_name) {
            let now = std::fs::read_to_string(root.join(path)).unwrap_or_default();
            let before = crate::checkpoint::git(root, &["show", &format!("{base}:{path}")]).unwrap_or_default();
            let had = parse(&before);
            for name in parse(&now) {
                if !had.contains(&name) {
                    push(kind, name, path);
                }
            }
        } else if links_count(path) {
            for line in file.diff.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")) {
                for name in cdn_links(line) {
                    push("cdn", name, path);
                }
            }
        }
    }
    out
}

type Parse = fn(&str) -> Vec<String>;

fn manifest(file: &str) -> Option<(&'static str, Parse)> {
    match file {
        "package.json" => Some(("npm", npm as Parse)),
        "Cargo.toml" => Some(("crates", cargo as Parse)),
        "requirements.txt" => Some(("pypi", requirements as Parse)),
        _ => None,
    }
}

/// Files where a CDN link loads code: pages, components and scripts, not docs or lockfiles.
fn links_count(path: &str) -> bool {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "html" | "htm" | "svelte" | "vue" | "astro" | "jsx" | "tsx" | "js" | "mjs" | "ts" | "css" | "scss" | "php" | "erb" | "hbs" | "ejs")
        && !path.contains("node_modules/")
}

/// The libraries a line links from the common CDNs, by their package name.
pub fn cdn_links(line: &str) -> Vec<String> {
    static PACKAGE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?:cdn\.jsdelivr\.net/npm|unpkg\.com|esm\.sh|cdn\.skypack\.dev|ga\.jspm\.io/npm:)/?((?:@[a-z0-9._-]+/)?[a-z0-9._-]+)").expect("constant regex")
    });
    static CDNJS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)cdnjs\.cloudflare\.com/ajax/libs/([a-z0-9._-]+)").expect("constant regex"));
    static NAMED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(code\.jquery\.com|cdn\.tailwindcss\.com|(?:stackpath|maxcdn)\.bootstrapcdn\.com/bootstrap|ajax\.googleapis\.com/ajax/libs/([a-z0-9._-]+))").expect("constant regex")
    });
    let mut out = Vec::new();
    for c in PACKAGE.captures_iter(line).chain(CDNJS.captures_iter(line)) {
        // A version is cut off: `bootstrap@5.3.3` is bootstrap.
        let name = c[1].to_lowercase();
        let name = match name.strip_prefix('@') {
            Some(rest) => format!("@{}", rest.split('@').next().unwrap_or(rest)),
            None => name.split('@').next().unwrap_or(&name).to_owned(),
        };
        out.push(name.trim_end_matches(['.', '-']).to_owned());
    }
    for c in NAMED.captures_iter(line) {
        let host = c[1].to_lowercase();
        out.push(match c.get(2) {
            Some(lib) => lib.as_str().to_lowercase(),
            None if host.starts_with("code.jquery") => "jquery".into(),
            None if host.starts_with("cdn.tailwindcss") => "tailwindcss".into(),
            None => "bootstrap".into(),
        });
    }
    out
}

/// Whether `found` has cleared: by its own kind, or for a CDN link, as the npm package it is.
fn cleared(list: &[CustomsItem], found: &Found) -> Option<CustomsStatus> {
    let status = |kind: &str| list.iter().find(|m| m.kind == kind && m.name == found.name).map(|m| m.status);
    let own = status(&found.kind);
    if found.kind == "cdn" && own != Some(CustomsStatus::Refused) && status("npm") == Some(CustomsStatus::Cleared) {
        return Some(CustomsStatus::Cleared);
    }
    own
}

/// What in `found` has not cleared, with its status. Anything unknown is put on the list as
/// held, saying where `thread` brought it in.
pub async fn hold(ctx: &Ctx<'_>, found: &[Found], thread: &str, thread_id: Option<&str>) -> Result<Vec<(Found, CustomsStatus)>> {
    if found.is_empty() {
        return Ok(Vec::new());
    }
    let _guard = LOCK.lock().await;
    let mut list = load(ctx).await?;
    let now = now_ms();
    let mut out = Vec::new();
    let mut added = Vec::new();
    for f in found {
        match cleared(&list, f) {
            Some(CustomsStatus::Cleared) => {}
            Some(status) => out.push((f.clone(), status)),
            None => {
                list.push(CustomsItem {
                    kind: f.kind.clone(),
                    name: f.name.clone(),
                    status: CustomsStatus::Held,
                    category: String::new(),
                    summary: String::new(),
                    versions: String::new(),
                    reason: format!("Brought in by \"{thread}\" in {} without clearing Customs.", f.path),
                    decided_by: "found".into(),
                    reviewed_at: None,
                    advisories: Vec::new(),
                    created_at: now,
                    updated_at: now,
                });
                added.push(f.clone());
                out.push((f.clone(), CustomsStatus::Held));
            }
        }
    }
    if !added.is_empty() {
        save(ctx, &mut list).await?;
    }
    drop(_guard);
    for f in added {
        note(ctx.store, Note::new("found", "agent").item(&f.kind, &f.name).detail(format!("Brought in in {}", f.path)).thread(thread_id)).await;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_package_json() {
        let text = r#"{"dependencies":{"svelte":"^5","local":"file:../x"},"devDependencies":{"vite":"^7","ws":"workspace:*"}}"#;
        assert_eq!(npm(text), vec!["svelte", "vite"]);
        assert!(npm("not json").is_empty());
    }

    #[test]
    fn reads_cargo_toml() {
        let text = r#"
[package]
name = "x"
version = "1"

[dependencies]
serde = { version = "1", features = ["derive"] }
anyhow.workspace = true
hub-core = { path = "../hub-core" }
regex = "1" # comment

[target.'cfg(windows)'.dependencies]
win32job = "2"

[dev-dependencies]
tempfile = "3"

[workspace.dependencies]
tokio = { version = "1" }

[dependencies.reqwest]
version = "0.12"

[dependencies.mine]
path = "../mine"

[features]
default = []
"#;
        assert_eq!(cargo(text), vec!["serde", "regex", "win32job", "tempfile", "tokio", "reqwest"]);
    }

    #[test]
    fn a_turn_brings_in_only_what_is_new() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| crate::checkpoint::git(root, args).unwrap();
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(root.join("package.json"), r#"{"dependencies":{"svelte":"^5"}}"#).unwrap();
        git(&["add", "."]);
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "first"]);
        let base = git(&["rev-parse", "HEAD"]).trim().to_owned();
        std::fs::write(root.join("package.json"), r#"{"dependencies":{"svelte":"^5","left-pad":"^1"}}"#).unwrap();
        std::fs::create_dir(root.join("py")).unwrap();
        std::fs::write(root.join("py/requirements.txt"), "Requests>=2\n").unwrap();
        let files = crate::checkpoint::diff_now(root, &base).unwrap();
        let found: Vec<(String, String, String)> = in_turn(root, &base, &files).into_iter().map(|f| (f.kind, f.name, f.path)).collect();
        assert_eq!(
            found,
            vec![
                ("npm".into(), "left-pad".into(), "package.json".into()),
                ("pypi".into(), "requests".into(), "py/requirements.txt".into()),
            ]
        );
    }

    #[test]
    fn finds_cdn_links() {
        let line = r#"+<link href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css"><script src="https://unpkg.com/@popperjs/core@2"></script>"#;
        assert_eq!(cdn_links(line), vec!["bootstrap", "@popperjs/core"]);
        assert_eq!(cdn_links("+import x from 'https://esm.sh/preact@10'"), vec!["preact"]);
        assert_eq!(cdn_links("+<script src=\"https://cdnjs.cloudflare.com/ajax/libs/lodash.js/4.17.21/lodash.min.js\">"), vec!["lodash.js"]);
        assert_eq!(cdn_links("+<script src=\"https://code.jquery.com/jquery-3.7.1.min.js\">"), vec!["jquery"]);
        assert_eq!(cdn_links("+<script src=\"https://cdn.tailwindcss.com\"></script>"), vec!["tailwindcss"]);
        assert!(cdn_links("+const url = 'https://example.com/x.js'").is_empty());
    }

    #[test]
    fn a_cdn_link_clears_as_its_npm_package() {
        let item = |kind: &str, status| CustomsItem {
            kind: kind.into(),
            name: "bootstrap".into(),
            status,
            category: String::new(),
            summary: String::new(),
            versions: String::new(),
            reason: String::new(),
            decided_by: "you".into(),
            reviewed_at: None,
            advisories: Vec::new(),
            created_at: 0,
            updated_at: 0,
        };
        let found = Found { kind: "cdn".into(), name: "bootstrap".into(), path: "index.html".into() };
        assert_eq!(cleared(&[item("npm", CustomsStatus::Cleared)], &found), Some(CustomsStatus::Cleared));
        assert_eq!(cleared(&[item("cdn", CustomsStatus::Refused)], &found), Some(CustomsStatus::Refused));
        assert_eq!(cleared(&[], &found), None);
    }

    #[test]
    fn reads_requirements() {
        let text = "requests>=2\n# note\n-r other.txt\nDjango==5.0 ; python_version > '3'\nnumpy\ngit+https://x/y\n";
        assert_eq!(requirements(text), vec!["requests", "Django", "numpy"]);
    }

    #[test]
    fn names_compare_like_their_ecosystem() {
        assert_eq!(normalize("pypi", "Zope.Interface__x"), "zope-interface-x");
        assert_eq!(normalize("crates", "Serde"), "serde");
        assert_eq!(normalize("npm", "@sveltejs/kit"), "@sveltejs/kit");
    }

    #[test]
    fn search_puts_cleared_first() {
        let m = |name: &str, status, summary: &str| CustomsItem {
            kind: "npm".into(),
            name: name.into(),
            status,
            category: "UI".into(),
            summary: summary.into(),
            versions: String::new(),
            reason: String::new(),
            decided_by: "you".into(),
            reviewed_at: None,
            advisories: Vec::new(),
            created_at: 0,
            updated_at: 0,
        };
        let list = vec![m("a-ui", CustomsStatus::Held, "Buttons"), m("b-ui", CustomsStatus::Cleared, "Buttons and menus")];
        let hits: Vec<&str> = search(&list, "buttons", None, Some("ui")).iter().map(|m| m.name.as_str()).collect();
        assert_eq!(hits, vec!["b-ui", "a-ui"]);
        assert!(search(&list, "menus", Some("crates"), None).is_empty());
    }
}
