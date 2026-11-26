//! What the project page shows about a folder: its stack, living docs, plan, git and GitHub
//! state, and its files. A project is the folder threads run in; the docs and the plan live in
//! the repository as markdown, so every engine can read and update them.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::git;
use crate::project_folder::{PLAN, add_rule, checkbox, inside};
use crate::proc::{GhError, gh_within};
use crate::vault::preview;

/// The rule that asks agents to keep the docs and the plan current.
pub const DOCS_RULE: &str = "Keep the living docs up to date as you work, without asking: \
`docs/plan.md` is a checklist of what is done and what comes next (tick off what you finish, add \
new steps you find instead of doing them on the side), and the other files in `docs/` describe \
the project as it is now (update them when what they describe changes). Write them as linked \
notes: a `summary:` line in YAML front matter, `[[Doc name]]` links to related docs, and Hub's \
`docs_map` and `docs_search` tools to find the doc to update before you add one. Say which code a \
doc describes with `covers: path, folder/` in its front matter or a `<!-- covers: ... -->` line \
under a heading, and before you change a file ask `docs_for` which docs cover it, so they stay true.";

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ProjectInfo {
    pub name: String,
    pub path: String,
    /// What the project is, from the first paragraph of its README.
    pub summary: Option<String>,
    /// Branch, or the short commit when detached. None outside a repository.
    pub branch: Option<String>,
    #[ts(type = "number | null")]
    pub changed: Option<u32>,
    #[ts(type = "number | null")]
    pub commits: Option<u32>,
    pub recent: Vec<Commit>,
    /// The `origin` remote as a web link, when it is on GitHub.
    pub github: Option<String>,
    /// Languages and frameworks found in the project's manifests.
    pub stack: Vec<String>,
    pub docs: Vec<DocFile>,
    /// Every markdown file the docs tab shows.
    #[ts(type = "number")]
    pub doc_count: usize,
    pub plan: Vec<PlanItem>,
    pub has_plan: bool,
    /// Where the project is going and what counts as done, from `docs/FOCUS.md`.
    pub focus: crate::focus::Focus,
    /// The folder's CLAUDE.md asks agents to keep the docs current.
    pub docs_rule: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    #[ts(type = "number")]
    pub ts: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DocFile {
    /// Relative to the project, with forward slashes.
    pub rel: String,
    /// The first heading, or the file name.
    pub title: String,
    /// The start of the text under the heading, for the doc card.
    pub preview: String,
    #[ts(type = "number")]
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PlanItem {
    pub text: String,
    pub done: bool,
    /// Line in the plan file, from 0, used to tick it off.
    #[ts(type = "number")]
    pub line: u32,
}

/// A change to the plan: tick or untick the item on `line`, or add `text` at the end.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct PlanEdit {
    pub path: String,
    #[serde(default)]
    #[ts(type = "number", optional)]
    pub line: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub done: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TreeEntry {
    pub name: String,
    /// Relative to the project, with forward slashes.
    pub rel: String,
    pub dir: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FileText {
    pub rel: String,
    pub text: String,
    /// The file was longer and only its start is in `text`.
    pub truncated: bool,
}

pub fn info(root: &Path) -> Result<ProjectInfo> {
    if !root.is_dir() {
        bail!("{} is not a folder", root.display());
    }
    let git_info = git::info(root);
    let in_repo = git_info.branch.is_some();
    let plan_text = std::fs::read_to_string(root.join(PLAN)).ok();
    let rules = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap_or_default();
    Ok(ProjectInfo {
        name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        path: root.display().to_string(),
        summary: std::fs::read_to_string(root.join("README.md"))
            .ok()
            .map(|t| preview(&t, 280))
            .filter(|p| !p.is_empty()),
        branch: git_info.branch,
        changed: git_info.changed,
        commits: in_repo
            .then(|| git::run(root, &["rev-list", "--count", "HEAD"]))
            .flatten()
            .and_then(|n| n.trim().parse().ok()),
        recent: if in_repo { recent_commits(root, 5) } else { Vec::new() },
        github: in_repo
            .then(|| git::run(root, &["remote", "get-url", "origin"]))
            .flatten()
            .and_then(|url| github_web_url(url.trim())),
        stack: stack(root),
        docs: docs(root),
        doc_count: crate::vault::map(root).docs.len(),
        has_plan: plan_text.is_some(),
        focus: crate::focus::read(root),
        plan: plan_text.as_deref().map(parse_plan).unwrap_or_default(),
        docs_rule: rules.contains(PLAN),
    })
}

fn recent_commits(root: &Path, n: usize) -> Vec<Commit> {
    let count = format!("-n{n}");
    let Some(out) = git::run(root, &["log", &count, "--format=%h%x1f%s%x1f%ct"]) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\u{1f}');
            let hash = parts.next()?.to_owned();
            let subject = parts.next()?.to_owned();
            let ts = parts.next()?.trim().parse::<i64>().ok()? * 1000;
            Some(Commit { hash, subject, ts })
        })
        .collect()
}

/// `git@github.com:o/r.git` or `https://github.com/o/r.git` as `https://github.com/o/r`.
fn github_web_url(remote: &str) -> Option<String> {
    let rest = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .or_else(|| {
            // https://user@github.com/o/r or a token in the URL: keep only the path.
            let after = remote.strip_prefix("https://")?;
            let (auth, path) = after.split_once("@github.com/")?;
            (!auth.contains('/')).then_some(path)
        })?;
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = rest.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    (!owner.is_empty() && !repo.is_empty()).then(|| format!("https://github.com/{owner}/{repo}"))
}

/// The checklist items of a markdown file, `- [ ] step` and `- [x] step`.
pub fn parse_plan(text: &str) -> Vec<PlanItem> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let (done, text) = checkbox(line)?;
            Some(PlanItem { text: text.to_owned(), done, line: i as u32 })
        })
        .collect()
}

/// Ticks, unticks or adds a plan item. Creates the plan when adding to a project without one.
pub fn edit_plan(root: &Path, edit: &PlanEdit) -> Result<Vec<PlanItem>> {
    let path = root.join(PLAN);
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    if let Some(line) = edit.line {
        let done = edit.done.context("say whether the step is done")?;
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let target = lines.get_mut(line as usize).context("the plan changed, reload it")?;
        if checkbox(target).is_none() {
            bail!("the plan changed, reload it");
        }
        let (from, to) = if done { ("[ ]", "[x]") } else { ("[x]", "[ ]") };
        *target = target.replacen(from, to, 1).replacen(&from.to_uppercase(), to, 1);
        text = lines.join("\n") + "\n";
    } else if let Some(step) = edit.text.as_deref() {
        let step = step.split_whitespace().collect::<Vec<_>>().join(" ");
        if step.is_empty() {
            bail!("an empty step");
        }
        if text.trim().is_empty() {
            text = "# Plan\n\n".into();
        } else if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("- [ ] {step}\n"));
    } else {
        bail!("nothing to change");
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
    Ok(parse_plan(&text))
}

/// Starts a plan when there is none, and asks agents in CLAUDE.md to keep the docs current.
pub fn set_up_docs(root: &Path) -> Result<()> {
    let plan = root.join(PLAN);
    if !plan.exists() {
        if let Some(dir) = plan.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&plan, "# Plan\n\nDone steps stay ticked; the next steps come after them.\n\n")?;
    }
    add_rule(&root.join("CLAUDE.md"), DOCS_RULE)?;
    add_rule(&root.join("CLAUDE.md"), crate::focus::FOCUS_RULE)
}

/// One level of the file tree, folders first, skipping what git ignores and hidden files.
pub fn tree(root: &Path, dir: &str) -> Result<Vec<TreeEntry>> {
    let base = inside(root, dir)?;
    let walk = ignore::WalkBuilder::new(&base).hidden(true).max_depth(Some(1)).build();
    let mut entries: Vec<TreeEntry> = walk
        .flatten()
        .skip(1)
        .filter_map(|e| {
            let rel = e.path().strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/");
            Some(TreeEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                rel,
                dir: e.file_type().is_some_and(|t| t.is_dir()),
            })
        })
        .collect();
    entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(entries)
}

const MAX_TEXT: usize = 512 * 1024;
/// The largest file shown as it is (pictures, PDFs, sound, video).
const MAX_RAW: u64 = 64 * 1024 * 1024;

/// What a file is, for showing it as it is: pictures, PDFs, sound and video. Anything else is
/// read as text instead.
pub fn raw_mime(rel: &str) -> Option<&'static str> {
    let ext = rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => return None,
    })
}

/// A picture, PDF, sound or video file in the project as its bytes, with its type.
pub fn raw(root: &Path, rel: &str) -> Result<(&'static str, Vec<u8>)> {
    let mime = raw_mime(rel).with_context(|| format!("{rel} is not a picture, PDF, sound or video"))?;
    let path = inside(root, rel)?;
    let size = std::fs::metadata(&path).with_context(|| format!("reading {rel}"))?.len();
    if size > MAX_RAW {
        bail!("{rel} is {} MB, too big to show here", size / (1024 * 1024));
    }
    Ok((mime, std::fs::read(&path).with_context(|| format!("reading {rel}"))?))
}

/// A text file in the project, for the doc reader and the file tree.
pub fn read(root: &Path, rel: &str) -> Result<FileText> {
    let path = inside(root, rel)?;
    text_of(&path, rel)
}

/// The text of a file Hub already knows by its full path, under the name `rel`; refuses binary files.
pub fn text_of(path: &Path, rel: &str) -> Result<FileText> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {rel}"))?;
    let truncated = bytes.len() > MAX_TEXT;
    let head = &bytes[..bytes.len().min(MAX_TEXT)];
    if head.contains(&0) {
        bail!("{rel} is not a text file");
    }
    Ok(FileText { rel: rel.to_owned(), text: String::from_utf8_lossy(head).into_owned(), truncated })
}

/// Opens a web link in the default browser of the machine the core runs on. The desktop shell
/// has no opener of its own, and only http(s) links are accepted.
pub fn open_url(url: &str) -> Result<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) || url.chars().any(char::is_whitespace) {
        bail!("only web links can be opened");
    }
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = crate::proc::command("rundll32");
        c.args(["url.dll,FileProtocolHandler", url]).creation_flags(0x0800_0000);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = crate::proc::command("open");
        c.arg(url);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = crate::proc::command("xdg-open");
        c.arg(url);
        c
    };
    cmd.spawn().with_context(|| format!("opening {url}"))?;
    Ok(())
}

/// Saves a file downloaded from the app, such as a picture an agent made, in the Downloads folder
/// of the machine the core runs on, under a name not taken yet. Returns where it went.
pub fn save_download(name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).context("no home folder")?;
    save_in(&PathBuf::from(home).join("Downloads"), name, bytes)
}

/// Adds a file, such as a picture an agent made, to a project's `docs/images`, where every
/// thread's agent in that project can find it. Returns where it went.
pub fn add_image(project: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    if !project.is_dir() {
        bail!("{} is not a folder", project.display());
    }
    save_in(&project.join("docs").join("images"), name, bytes)
}

fn save_in(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
    // Only a file name, whatever was asked, and one Windows accepts.
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = base.chars().map(|c| if c.is_control() || r#"<>:"|?*"#.contains(c) { '_' } else { c }).collect();
    let clean = clean.trim().trim_matches('.').to_owned();
    let clean = if clean.is_empty() { "download".to_owned() } else { clean };
    let (stem, ext) = match clean.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), format!(".{ext}")),
        _ => (clean.clone(), String::new()),
    };
    for n in 0..1000 {
        let path = dir.join(if n == 0 { format!("{stem}{ext}") } else { format!("{stem} ({n}){ext}") });
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                use std::io::Write;
                file.write_all(bytes).with_context(|| format!("could not write {}", path.display()))?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("could not write {}", path.display())),
        }
    }
    bail!("no free name for {clean} in {}", dir.display())
}

/// Shows a file or folder in the file manager of the machine the core runs on. A file is shown
/// selected in its folder; nothing is run or opened with its program.
pub fn reveal(path: &Path) -> Result<()> {
    // Explorer only understands backslashes; given `d:/dev/x.md` it opens its default folder.
    #[cfg(windows)]
    let fixed = PathBuf::from(path.to_string_lossy().replace('/', "\\"));
    #[cfg(windows)]
    let path = fixed.as_path();
    let meta = std::fs::metadata(path).with_context(|| format!("{} does not exist", path.display()))?;
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = crate::proc::command("explorer");
        if meta.is_dir() {
            c.arg(path);
        } else {
            // Explorer parses its own command line: /select, and the path must be one argument.
            c.raw_arg(format!("/select,\"{}\"", path.display()));
        }
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = crate::proc::command("open");
        if meta.is_dir() {
            c.arg(path);
        } else {
            c.arg("-R").arg(path);
        }
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = crate::proc::command("xdg-open");
        c.arg(if meta.is_dir() { path } else { path.parent().unwrap_or(path) });
        c
    };
    cmd.spawn().with_context(|| format!("showing {}", path.display()))?;
    Ok(())
}

mod docs;
use docs::docs;
use crate::project_folder::stack;

mod github;
pub use github::*;
pub use crate::project_github::{CiRun, GithubInfo, PullRequest, github};

mod open;
pub use open::*;

#[cfg(test)]
mod tests;
