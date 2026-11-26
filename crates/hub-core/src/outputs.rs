//! A project's library: what its threads made, where each thing is now, and what happened to it.
//! Read from the threads' logs each time it is asked, so nothing new is stored:
//!
//! - pages an agent showed (`page` events), one per page and its later versions;
//! - pictures, from image blocks with a file behind them and from side jobs' `child` events;
//! - docs the agents wrote or changed (`.md` and other prose), and new files that are not code;
//! - servers the project's threads started that listen on a port now (`runtime.rs`).
//!
//! For a file it looks at where it is: on this machine (or gone), and whether git has it
//! committed on the branch checked out, changed, or not at all. The life of a thing is when it was
//! made, its versions or the turns that changed it, and when git first committed it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;

use crate::hub::Hub;
use crate::model::{EventKind, Output, OutputOpen, OutputPlace, OutputStep, OutputUse, Thread};

/// Files the library lists however they came: prose and pictures.
const DOCS: &[&str] = &["md", "mdx", "markdown", "txt", "rst", "adoc"];
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "bmp"];
/// New files with these endings are the code itself, not something made to look at or keep.
const CODE: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "svelte", "vue", "py", "go", "java", "kt", "c", "h", "cpp", "hpp", "cs",
    "rb", "php", "swift", "css", "scss", "toml", "lock", "yaml", "yml", "json", "xml", "gitignore", "sql", "snap",
];
/// At most this many files get their git history read, newest first.
const GIT_LOOKS: usize = 80;

/// The folder a thread works for: its own, or the project's when it works in a worktree of it.
pub fn root_of(cwd: &str) -> String {
    let unified = cwd.replace('\\', "/");
    for marker in ["/.git/hub-attempts/", "/.hub-threads/"] {
        if let Some(at) = unified.find(marker) {
            return cwd[..at].to_owned();
        }
    }
    cwd.trim_end_matches(['/', '\\']).to_owned()
}

fn same(a: &str, b: &str) -> bool {
    crate::desk::norm(a) == crate::desk::norm(b)
}

/// One thing as it is gathered, before git and the disk are asked.
struct Found {
    kind: &'static str,
    title: String,
    thread: Thread,
    made_at: i64,
    changed_at: i64,
    versions: u32,
    steps: Vec<OutputStep>,
    users: Vec<(String, String)>,
    /// A file on this machine, absolute.
    path: Option<PathBuf>,
    /// Kept in Hub: a page's thread and id, or a picture only in the log.
    page: Option<String>,
    in_log: bool,
    /// A picture only in the log: the update's `seq` and which image block of it.
    log: Option<(i64, usize)>,
}

impl Found {
    fn new(kind: &'static str, title: String, thread: &Thread, at: i64) -> Self {
        Found {
            kind,
            title,
            thread: thread.clone(),
            made_at: at,
            changed_at: at,
            versions: 1,
            steps: vec![OutputStep { at, what: format!("Made in {}", thread.title), thread: Some(thread.id.clone()) }],
            users: Vec::new(),
            path: None,
            page: None,
            in_log: false,
            log: None,
        }
    }
}

/// What the project at `root` made, newest change first.
pub async fn list(hub: &Arc<Hub>, root: &str) -> Result<Vec<Output>> {
    let (threads, found) = gathered(hub, root).await?;
    let machine = hub.remotes.local_name().to_owned();
    let root_path = PathBuf::from(root);
    let data_dir = hub.config.data_dir.clone();
    let mut outputs = tokio::task::spawn_blocking(move || {
        let mut items: Vec<(String, Found)> = found.into_iter().collect();
        items.sort_by_key(|(_, f)| std::cmp::Reverse(f.changed_at));
        items.into_iter().enumerate().map(|(n, (key, f))| place(key, f, &root_path, &data_dir, &machine, n < GIT_LOOKS)).collect::<Vec<_>>()
    })
    .await?;
    outputs.extend(servers(hub, root, &threads).await);
    outputs.sort_by_key(|o| (std::cmp::Reverse(o.live), std::cmp::Reverse(o.changed_at)));
    Ok(outputs)
}

/// What one thing of the library holds, to show it inside Hub.
pub enum Content {
    /// A file on this machine.
    File(PathBuf),
    /// A picture only in a thread's log.
    Picture { mime: String, bytes: Vec<u8> },
}

/// The file or picture behind the library item `id` of the project at `root`. Only things the
/// project's threads made can be read this way, never any other path.
pub async fn content(hub: &Arc<Hub>, root: &str, id: &str) -> Result<Content> {
    let (_, mut found) = gathered(hub, root).await?;
    let Some(f) = found.remove(id) else { anyhow::bail!("{id} is not in this project's library") };
    if let Some(path) = f.path {
        return Ok(Content::File(real(&path)));
    }
    let Some((seq, n)) = f.log else { anyhow::bail!("{} has no file to show", f.title) };
    let event = hub.store.events(&f.thread.id, seq - 1, 1).await?.into_iter().next();
    let block = event.as_ref().and_then(|e| image_blocks(&e.data).into_iter().nth(n).cloned());
    let Some(block) = block else { anyhow::bail!("the picture is no longer in the thread") };
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(block["data"].as_str().unwrap_or_default().trim())?;
    let mime = block["mimeType"].as_str().filter(|m| m.starts_with("image/")).unwrap_or("image/png").to_owned();
    Ok(Content::Picture { mime, bytes })
}

/// The path as the disk spells it (`D:\Dev\hub\docs\x.md` for a logged `d:/dev/hub/docs/x.md`),
/// or as it was when it is gone.
fn real(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(p) => {
            let s = p.to_string_lossy();
            match s.strip_prefix(r"\\?\UNC\") {
                Some(rest) => PathBuf::from(format!(r"\\{rest}")),
                None => s.strip_prefix(r"\\?\").map(PathBuf::from).unwrap_or_else(|| p.clone()),
            }
        }
        Err(_) => path.to_owned(),
    }
}

/// The project's threads and everything they made, keyed by library id.
async fn gathered(hub: &Arc<Hub>, root: &str) -> Result<(Vec<Thread>, BTreeMap<String, Found>)> {
    let threads: Vec<Thread> = hub.store.threads().await?.into_iter().filter(|t| same(&root_of(&t.cwd), root)).collect();
    let desks = crate::desk::ids(&hub.ctx()).await.unwrap_or_default();
    let mut logs = Vec::new();
    for thread in threads.iter().filter(|t| !desks.contains(&t.id)) {
        // The kinds `gather` reads; the rest of a log names no file.
        logs.push((thread, hub.store.events_of(&thread.id, 0, &[EventKind::Page, EventKind::Child, EventKind::Update]).await?));
    }
    // Every thread's events in the order they happened, so the thread that made a file first
    // is the one that made it, whichever thread is read first.
    let mut all: Vec<(&Thread, &crate::model::Event)> = logs.iter().flat_map(|(t, events)| events.iter().map(move |e| (*t, e))).collect();
    all.sort_by_key(|(_, e)| e.ts);
    let mut found: BTreeMap<String, Found> = BTreeMap::new();
    let mut families: BTreeMap<String, String> = BTreeMap::new();
    for (thread, e) in all {
        gather(thread, e, &mut found, &mut families);
    }
    drop(logs);
    Ok((threads, found))
}

/// Reads one event of `thread` into `found`, keyed so the same thing from two threads is one.
/// `families` maps a page id to the key of its first version, as later versions name the one
/// they replace.
fn gather(thread: &Thread, e: &crate::model::Event, found: &mut BTreeMap<String, Found>, families: &mut BTreeMap<String, String>) {
    let cwd = PathBuf::from(&thread.cwd);
    {
        match e.kind {
            EventKind::Page => {
                let Some(page) = e.data_as::<crate::model::PageData>() else { return };
                let Some(id) = crate::model::written(&page.id) else { return };
                let title = crate::model::written(&page.title).unwrap_or("Page").to_owned();
                let family = page.replaces.as_deref().and_then(|r| families.get(r).cloned());
                match family.and_then(|k| found.get_mut(&k).map(|f| (k, f))) {
                    Some((key, f)) => {
                        f.versions += 1;
                        f.changed_at = e.ts;
                        f.title = title;
                        f.page = Some(id.to_owned());
                        f.steps.push(OutputStep { at: e.ts, what: format!("Version {}", f.versions), thread: Some(thread.id.clone()) });
                        families.insert(id.to_owned(), key);
                    }
                    None => {
                        let key = format!("page:{}:{id}", thread.id);
                        let mut f = Found::new("page", title, thread, e.ts);
                        f.page = Some(id.to_owned());
                        found.insert(key.clone(), f);
                        families.insert(id.to_owned(), key);
                    }
                }
            }
            EventKind::Child => {
                let Some(job) = e.data_as::<crate::model::ChildData>().filter(|job| job.state == "done") else { return };
                for path in job.images.iter().flatten() {
                    let what = format!("Drawn by side job {}", crate::model::written(&job.title).unwrap_or("for this thread"));
                    file(found, thread, &cwd.join(path), e.ts, Some(what));
                }
            }
            EventKind::Update => {
                for (n, image) in images(&e.data).into_iter().enumerate() {
                    match image {
                        Some(uri) => file(found, thread, &cwd.join(uri.trim_start_matches("file://")), e.ts, None),
                        None => {
                            let mut f = Found::new("picture", format!("Picture in {}", thread.title), thread, e.ts);
                            f.in_log = true;
                            f.log = Some((e.seq, n));
                            found.insert(format!("log:{}:{}:{n}", thread.id, e.seq), f);
                        }
                    }
                }
                let new_files = created(&e.data);
                for path in crate::clash::edited(&e.data, &cwd) {
                    let path = PathBuf::from(&path);
                    let ext = ext(&path);
                    let listed = DOCS.contains(&ext.as_str()) || PICTURES.contains(&ext.as_str());
                    let fresh = new_files.iter().any(|n| same(&cwd.join(n).to_string_lossy(), &path.to_string_lossy()));
                    let key = format!("file:{}", crate::desk::norm(&path.to_string_lossy()));
                    if listed || (fresh && !CODE.contains(&ext.as_str())) || found.contains_key(&key) {
                        file(found, thread, &path, e.ts, None);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Notes a file made or changed by `thread` at `at`.
fn file(found: &mut BTreeMap<String, Found>, thread: &Thread, path: &Path, at: i64, how: Option<String>) {
    let key = format!("file:{}", crate::desk::norm(&path.to_string_lossy()));
    if let Some(f) = found.get_mut(&key) {
        // Each turn that changes it counts once.
        if at - f.changed_at > 60_000 || f.thread.id != thread.id {
            f.versions += 1;
            f.steps.push(OutputStep { at, what: format!("Changed in {}", thread.title), thread: Some(thread.id.clone()) });
        }
        f.changed_at = f.changed_at.max(at);
        if f.thread.id != thread.id && !f.users.iter().any(|(id, _)| *id == thread.id) {
            f.users.push((thread.id.clone(), thread.title.clone()));
        }
        return;
    }
    let ext = ext(path);
    let kind = if PICTURES.contains(&ext.as_str()) {
        "picture"
    } else if DOCS.contains(&ext.as_str()) {
        "doc"
    } else {
        "file"
    };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut f = Found::new(kind, name, thread, at);
    if let Some(how) = how {
        f.steps[0].what = how;
    }
    f.path = Some(path.to_owned());
    found.insert(key, f);
}

/// The type of a picture file by its ending; None for anything else.
pub fn picture_mime(path: &Path) -> Option<&'static str> {
    Some(match ext(path).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        _ => return None,
    })
}

fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| {
        // `.gitignore` and the like have a name and no extension.
        path.file_name().map(|n| n.to_string_lossy().trim_start_matches('.').to_lowercase()).unwrap_or_default()
    })
}

/// The image blocks of an update: Some(uri) for one with a file, None for one only in the log.
fn images(update: &Value) -> Vec<Option<String>> {
    image_blocks(update).into_iter().map(|b| b["uri"].as_str().map(str::to_owned)).collect()
}

fn image_blocks(update: &Value) -> Vec<&Value> {
    let mut blocks: Vec<&Value> = Vec::new();
    if update["content"]["type"] == "image" {
        blocks.push(&update["content"]);
    }
    for c in update["content"].as_array().into_iter().flatten() {
        if c["type"] == "content" && c["content"]["type"] == "image" {
            blocks.push(&c["content"]);
        }
    }
    blocks
}

/// Paths an update wrote from nothing: diffs without an old text.
fn created(update: &Value) -> Vec<String> {
    update["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["type"] == "diff" && c["oldText"].is_null())
        .filter_map(|c| c["path"].as_str().map(str::to_owned))
        .collect()
}

/// Where one thing is now, and its life with git's part added.
fn place(key: String, f: Found, root: &Path, data_dir: &Path, machine: &str, ask_git: bool) -> Output {
    let mut places = Vec::new();
    let mut steps = f.steps;
    let mut open = OutputOpen::default();
    if let Some(page) = &f.page {
        places.push(OutputPlace {
            kind: "hub".into(),
            label: format!("Kept in Hub on {machine}"),
            detail: data_dir.join("pages").join(&f.thread.id).join(format!("{page}.html")).display().to_string(),
            state: if crate::pages::exists(data_dir, &f.thread.id, page) { "kept".into() } else { "gone".into() },
        });
        open.thread = Some(f.thread.id.clone());
        open.page = Some(page.clone());
    }
    if f.in_log {
        places.push(OutputPlace { kind: "hub".into(), label: "In the thread".into(), detail: f.thread.title.clone(), state: "kept".into() });
        open.thread = Some(f.thread.id.clone());
    }
    let path = f.path.as_deref().map(real);
    if let Some(path) = &path {
        let here = path.exists();
        places.push(OutputPlace {
            kind: "folder".into(),
            label: machine.to_owned(),
            detail: path.display().to_string(),
            state: if here { "kept".into() } else { "gone".into() },
        });
        open.path = Some(path.display().to_string());
        if ask_git && here
            && let Some((place, committed)) = in_git(path, root) {
                places.insert(0, place);
                if let Some(at) = committed {
                    steps.push(OutputStep { at, what: "Committed".into(), thread: None });
                }
            }
    }
    steps.sort_by_key(|s| s.at);
    Output {
        id: key,
        kind: f.kind.into(),
        title: f.title,
        thread: Some(f.thread.id.clone()),
        thread_title: Some(f.thread.title.clone()),
        made_at: f.made_at,
        changed_at: f.changed_at,
        versions: f.versions,
        live: false,
        places,
        life: steps,
        used_in: f.users.into_iter().map(|(thread, title)| OutputUse { thread, title }).collect(),
        open,
    }
}

/// The file in git: on which branch, whether it is committed, changed or not tracked, and when it
/// was first committed (milliseconds).
fn in_git(path: &Path, root: &Path) -> Option<(OutputPlace, Option<i64>)> {
    let dir = path.parent()?;
    let top = PathBuf::from(crate::git::run(dir, &["rev-parse", "--show-toplevel"])?.trim());
    let rel = path.strip_prefix(&top).ok().or_else(|| path.strip_prefix(root).ok())?.to_string_lossy().replace('\\', "/");
    let branch = crate::git::branch_of(dir).unwrap_or_else(|| "HEAD".into());
    let repo = top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let status = crate::git::run(&top, &["status", "--porcelain=v1", "--", &rel]).unwrap_or_default();
    let first = crate::git::run(&top, &["log", "--diff-filter=A", "--format=%ct", "--", &rel])
        .and_then(|out| out.lines().last().and_then(|l| l.trim().parse::<i64>().ok()))
        .map(|s| s * 1000);
    let state = if status.starts_with("??") {
        "untracked"
    } else if !status.trim().is_empty() {
        "uncommitted"
    } else if first.is_some() {
        "committed"
    } else {
        return None;
    };
    Some((OutputPlace { kind: "repo".into(), label: format!("{repo} · {branch}"), detail: rel, state: state.into() }, first))
}

/// Servers the project's threads started that listen now.
async fn servers(hub: &Arc<Hub>, root: &str, threads: &[Thread]) -> Vec<Output> {
    let machine = hub.remotes.local_name().to_owned();
    let root_path = PathBuf::from(root);
    let all = threads.to_vec();
    let Ok(runtime) = tokio::task::spawn_blocking(move || crate::runtime::processes(&root_path, &all)).await else { return Vec::new() };
    let now = crate::model::now_ms();
    runtime
        .servers
        .into_iter()
        .map(|s| {
            let thread = s.thread_id.as_ref().and_then(|id| threads.iter().find(|t| &t.id == id));
            let port = s.ports.first().copied().unwrap_or_default();
            let url = format!("http://localhost:{port}");
            let started = thread.map(|t| format!("Started by {}", t.title)).unwrap_or_else(|| "Started in the project folder".into());
            Output {
                id: format!("server:{}", s.pid),
                kind: "server".into(),
                title: s.service.clone().unwrap_or_else(|| s.name.clone()),
                thread: thread.map(|t| t.id.clone()),
                thread_title: thread.map(|t| t.title.clone()),
                made_at: now,
                changed_at: now,
                versions: 1,
                live: true,
                places: s
                    .ports
                    .iter()
                    .map(|p| OutputPlace { kind: "server".into(), label: format!("{machine} :{p}"), detail: format!("http://localhost:{p} · {}", s.command), state: "live".into() })
                    .collect(),
                life: vec![OutputStep { at: now, what: started, thread: thread.map(|t| t.id.clone()) }],
                used_in: Vec::new(),
                open: OutputOpen { url: Some(url), ..Default::default() },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktrees_belong_to_their_project() {
        assert_eq!(root_of("D:\\Dev\\hub\\.hub-threads\\abc"), "D:\\Dev\\hub");
        assert_eq!(root_of("/home/c/hub/.git/hub-attempts/x/y"), "/home/c/hub");
        assert_eq!(root_of("/home/c/hub/"), "/home/c/hub");
    }

    #[test]
    fn new_files_and_images_are_read_from_updates() {
        let write = serde_json::json!({
            "sessionUpdate": "tool_call", "kind": "edit",
            "content": [{ "type": "diff", "path": "notes/plan.md", "oldText": null, "newText": "x" },
                        { "type": "diff", "path": "src/a.rs", "oldText": "a", "newText": "b" }],
        });
        assert_eq!(created(&write), ["notes/plan.md"]);
        let chunk = serde_json::json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "image", "data": "", "uri": "file:///tmp/sun.png" } });
        assert_eq!(images(&chunk), [Some("file:///tmp/sun.png".to_owned())]);
        assert_eq!(ext(Path::new("/a/.gitignore")), "gitignore");
    }
}
