//! Carrying a project to another machine, as T3 Code does: the machine it is on packs its git
//! history into a bundle together with the ignored env files a clone would not have (`.env`,
//! `.env.*`, `.envrc`, `.dev.vars`), and the machine it goes to clones from that bundle and
//! points origin back at the same remote. The other machine needs no GitHub login and no copy
//! of your secrets by hand. Only the core the app talks to moves the pack between the two.

use crate::locked::ResultExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use crate::model::{Brought, PackedFile, ProjectPack, UnpackProject};

/// A bundle bigger than this is not carried; the other machine clones from the remote instead.
const MAX_BUNDLE: u64 = 40 * 1024 * 1024;
/// Per env file, as T3 Code does.
const MAX_FILE: u64 = 1024 * 1024;
/// All env files together.
const MAX_FILES: u64 = 4 * 1024 * 1024;

/// Folders of other people's code, whose env files are theirs.
const DEPENDENCIES: &[&str] = &["node_modules", "vendor", "target", ".venv", "venv", "dist", "build"];

/// Whether an ignored file is one a project needs to run and a clone leaves out.
pub fn is_env_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name == ".env" || name.starts_with(".env.") || name == ".envrc" || name == ".dev.vars"
}

/// Uncommitted changes bigger than this are not carried with a thread.
const MAX_CHANGES: u64 = 20 * 1024 * 1024;

/// Packs the repository at `path` on this machine.
pub fn pack(path: &str) -> Result<ProjectPack> {
    pack_for(path, false)
}

/// Packs the repository at `path`; with `changes`, also what is not committed yet (to move a
/// thread, which carries on with its work as it stands).
pub fn pack_for(path: &str, changes: bool) -> Result<ProjectPack> {
    let dir = crate::folders::expand(path);
    if !dir.join(".git").exists() {
        bail!("{} is not a git repository", dir.display());
    }
    pack_repo(path, &dir, changes)
}

/// Packs any folder to move a thread: a repository as `pack_for` does, anything else (a Scratch
/// thread's folder) as the plain files in it.
pub fn pack_folder(path: &str) -> Result<ProjectPack> {
    let dir = crate::folders::expand(path);
    if dir.join(".git").exists() {
        return pack_for(path, true);
    }
    if !dir.is_dir() {
        bail!("{} is not a folder on this machine", dir.display());
    }
    Ok(ProjectPack { files: plain_files(&dir)?, plain: true, ..Default::default() })
}

fn pack_repo(path: &str, dir: &Path, changes: bool) -> Result<ProjectPack> {
    let dir = dir.to_path_buf();
    let url = crate::folders::clone_url(path);
    let branch = git(&dir, &["rev-parse", "--abbrev-ref", "HEAD"]).ok().map(|b| b.trim().to_owned()).filter(|b| b != "HEAD");
    let dirty = git(&dir, &["status", "--porcelain", "--untracked-files=no"]).map(|o| o.lines().count() as u32).unwrap_or(0);
    let mut committed = git(&dir, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok();
    let mut files = env_files(&dir)?;
    // A project made a moment ago (New project in Hub is `git init` and nothing more) has no
    // history and no remote: its files are all there is, so they come along as they are.
    let mut as_files = false;
    if !committed && url.is_none() {
        match working_files(&dir) {
            Ok(working) => {
                let more: Vec<PackedFile> = working.into_iter().filter(|w| !files.iter().any(|f| f.rel == w.rel)).collect();
                files.extend(more);
                as_files = true;
            }
            // Too many files to send one by one: save them as a commit and carry that as history.
            Err(e) if e.is::<TooBig>() => {
                checkpoint(&dir)?;
                committed = true;
            }
            Err(e) => return Err(e),
        }
    }
    // Uncommitted work too big to send as a patch is saved as a commit the same way.
    let mut patch_and_new = None;
    if changes && committed {
        match patch(&dir).and_then(|p| untracked(&dir).map(|u| (p, u))) {
            Ok(both) => patch_and_new = Some(both),
            Err(e) if e.is::<TooBig>() => {
                checkpoint(&dir)?;
                patch_and_new = Some((None, Vec::new()));
            }
            Err(e) => return Err(e),
        }
    }
    let bundle = bundle(&dir)?;
    if !as_files && bundle.is_none() && url.is_none() {
        bail!("{} is too big to carry and has no remote to clone from", dir.display());
    }
    let mut pack = ProjectPack { url, branch, bundle, files, dirty, ..Default::default() };
    if let Some((patch, untracked)) = patch_and_new {
        pack.head = git(&dir, &["rev-parse", "HEAD"]).ok().map(|h| h.trim().to_owned());
        pack.patch = patch;
        pack.untracked = untracked;
    }
    Ok(pack)
}

/// Past the size limits for carrying as files or as a patch; `checkpoint` is the way round it.
#[derive(Debug)]
struct TooBig(String);

impl std::fmt::Display for TooBig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TooBig {}

/// The message of the commit Hub makes when work is too big to carry as it stands.
pub const CHECKPOINT_MESSAGE: &str = "Hub checkpoint: where we left off";

/// Saves everything git would not ignore as one commit, so the work moves as history instead of as
/// files and the other machine carries on from the same point. Other people's code (`node_modules`,
/// `target`, ...) is kept out through the repository's own `.git/info/exclude`, never its
/// `.gitignore`, so nothing in the project changes. Nothing is pushed.
fn checkpoint(dir: &Path) -> Result<()> {
    let exclude = dir.join(".git/info/exclude");
    let now = std::fs::read_to_string(&exclude).unwrap_or_default();
    let missing: Vec<String> = DEPENDENCIES.iter().filter(|d| !now.lines().any(|l| l.trim().trim_matches('/') == **d)).map(|d| format!("{d}/")).collect();
    if !missing.is_empty() {
        if let Some(parent) = exclude.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lead = if now.is_empty() || now.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&exclude, format!("{now}{lead}# Hub: other people's code stays out of its checkpoints\n{}\n", missing.join("\n")))?;
    }
    git(dir, &["add", "--all"]).context("saving where you left off")?;
    // Their own name on it when git knows it; Hub's when it does not.
    let known = git(dir, &["config", "user.email"]).map(|e| !e.trim().is_empty()).unwrap_or(false);
    let mut args = vec!["-c", "commit.gpgsign=false"];
    if !known {
        args.extend(["-c", "user.name=Hub", "-c", "user.email=hub@localhost"]);
    }
    args.extend(["commit", "--quiet", "--no-verify", "--allow-empty", "-m", CHECKPOINT_MESSAGE]);
    git(dir, &args).context("saving where you left off")?;
    Ok(())
}

/// The tracked files' uncommitted changes (staged or not) as one patch, base64.
fn patch(dir: &Path) -> Result<Option<String>> {
    let out = git_bytes(dir, &["diff", "--binary", "--no-ext-diff", "HEAD"])?;
    if out.is_empty() {
        return Ok(None);
    }
    if out.len() as u64 > MAX_CHANGES {
        return Err(TooBig("the uncommitted changes are too big to carry".into()).into());
    }
    Ok(Some(STANDARD.encode(out)))
}

/// Files git does not track and does not ignore.
fn untracked(dir: &Path) -> Result<Vec<PackedFile>> {
    let list = git(dir, &["ls-files", "-z", "--others", "--exclude-standard"])?;
    let mut total = 0;
    let mut files = Vec::new();
    for rel in list.split('\0').filter(|r| !r.is_empty()) {
        let Ok(meta) = std::fs::symlink_metadata(dir.join(rel)) else { continue };
        if !meta.is_file() {
            continue;
        }
        total += meta.len();
        if total > MAX_CHANGES {
            return Err(TooBig("the new files are too big to carry".into()).into());
        }
        let data = std::fs::read(dir.join(rel)).with_context(|| format!("reading {rel}"))?;
        files.push(PackedFile { rel: rel.to_owned(), data: STANDARD.encode(data) });
    }
    Ok(files)
}

/// Branches and tags as a bundle, base64. None when there is nothing committed or it is too big.
fn bundle(dir: &Path) -> Result<Option<String>> {
    if git(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_err() {
        return Ok(None);
    }
    let file = std::env::temp_dir().join(format!("hub-carry-{}.bundle", uuid::Uuid::new_v4().simple()));
    let made = git(dir, &["bundle", "create", "--quiet", &file.display().to_string(), "HEAD", "--branches", "--tags"]);
    let read = made.and_then(|_| {
        let size = std::fs::metadata(&file)?.len();
        Ok(if size > MAX_BUNDLE { None } else { Some(STANDARD.encode(std::fs::read(&file)?)) })
    });
    crate::locked::clean_file(&file);
    read.context("packing the git history")
}

/// The ignored env files in the project, smallest first until the limit.
fn env_files(dir: &Path) -> Result<Vec<PackedFile>> {
    let list = git(
        dir,
        &[
            "ls-files", "-z", "--others", "--ignored", "--exclude-standard", "--",
            ":(glob)**/.env", ":(glob)**/.env.*", ":(glob)**/.envrc", ":(glob)**/.dev.vars",
        ],
    );
    let Ok(list) = list else { return Ok(Vec::new()) };
    let mut found: Vec<(String, u64)> = list
        .split('\0')
        .filter(|rel| !rel.is_empty() && is_env_file(rel) && !rel.split('/').any(|part| DEPENDENCIES.contains(&part)))
        .filter_map(|rel| {
            let meta = std::fs::symlink_metadata(dir.join(rel)).ok()?;
            (meta.is_file() && meta.len() <= MAX_FILE).then(|| (rel.to_owned(), meta.len()))
        })
        .collect();
    found.sort_by_key(|(_, size)| *size);
    let mut total = 0;
    let mut files = Vec::new();
    for (rel, size) in found {
        total += size;
        if total > MAX_FILES {
            break;
        }
        let data = std::fs::read(dir.join(&rel)).with_context(|| format!("reading {rel}"))?;
        files.push(PackedFile { rel, data: STANDARD.encode(data) });
    }
    Ok(files)
}

/// Every file of a project with no commits yet that git would not ignore, as long as they
/// stay within the limits; past them it is too big to carry this way.
fn working_files(dir: &Path) -> Result<Vec<PackedFile>> {
    let list = git(dir, &["ls-files", "-z", "--others", "--cached", "--exclude-standard"])?;
    let mut total = 0;
    let mut files = Vec::new();
    for rel in list.split('\0').filter(|rel| !rel.is_empty()) {
        let Ok(meta) = std::fs::symlink_metadata(dir.join(rel)) else { continue };
        if !meta.is_file() {
            continue;
        }
        total += meta.len();
        if meta.len() > MAX_FILE || total > MAX_FILES {
            return Err(TooBig(format!("{} has nothing committed and is too big to carry as files", dir.display())).into());
        }
        let data = std::fs::read(dir.join(rel)).with_context(|| format!("reading {rel}"))?;
        files.push(PackedFile { rel: rel.to_owned(), data: STANDARD.encode(data) });
    }
    Ok(files)
}

/// Every file in a folder that is not a repository, except other people's code (`node_modules`
/// and the like) and git's own folder, as long as they stay within the limits.
fn plain_files(dir: &Path) -> Result<Vec<PackedFile>> {
    fn walk(root: &Path, at: &Path, total: &mut u64, out: &mut Vec<PackedFile>) -> Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(at).with_context(|| format!("reading {}", at.display()))?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if name != ".git" && !DEPENDENCIES.contains(&name.as_str()) {
                    walk(root, &entry.path(), total, out)?;
                }
            } else if kind.is_file() {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                *total += size;
                if *total > MAX_CHANGES {
                    bail!("{} is too big to carry; it holds more than {} MB", root.display(), MAX_CHANGES / (1024 * 1024));
                }
                let rel = entry.path().strip_prefix(root)?.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
                let data = std::fs::read(entry.path()).with_context(|| format!("reading {rel}"))?;
                out.push(PackedFile { rel, data: STANDARD.encode(data) });
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut 0, &mut files)?;
    Ok(files)
}

/// Lays `pack` out at `path` on this machine. A folder that already holds the same repository
/// only gets the env files it lacks; files already there are never overwritten.
pub fn unpack(req: UnpackProject) -> Result<Brought> {
    let UnpackProject { path, pack, sync, resume } = req;
    let dir = crate::folders::expand(&path);
    let has_files = dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false);
    if pack.plain {
        // A folder of files, no repository: it goes in an empty (or new) folder, as it is.
        if has_files && !resume {
            bail!("{} already has other files in it", dir.display());
        }
        std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
        let written = write_files(&dir, &pack.files, resume)?;
        return Ok(Brought { folder: dir.display().to_string(), files: written, dirty: 0, via: "files".into() });
    }
    let via = if has_files {
        let same = (resume && dir.join(".git").exists())
            || match (&pack.url, crate::folders::repo_key(&path)) {
                (Some(url), Some(here)) => crate::folders::key_of(url).as_deref() == Some(here.as_str()),
                _ => false,
            };
        if !same {
            bail!("{} already has other files in it", dir.display());
        }
        if sync {
            bring_up_to(&dir, &pack, resume)?;
        }
        "existing"
    } else if let Some(bundle) = &pack.bundle {
        clone_bundle(&dir, bundle, pack.url.as_deref(), pack.branch.as_deref())?;
        "bundle"
    } else if let Some(url) = &pack.url {
        crate::folders::clone(url, &path)?;
        "clone"
    } else {
        // Nothing committed and no remote: a fresh repository with the files written into it.
        std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
        git(&dir, &["init", "--quiet"])?;
        if let Some(branch) = &pack.branch {
            git(&dir, &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")]).warn_on_err("point HEAD at the carried branch");
        }
        "files"
    };
    if sync && via != "existing"
        && let Some(head) = pack.head.as_deref().filter(|_| pack.branch.is_none()) {
            git(&dir, &["checkout", "--quiet", head]).context("checking out the commit it was on")?;
        }
    if sync {
        apply_changes(&dir, &pack)?;
    }
    let written = write_files(&dir, &pack.files, resume)?;
    Ok(Brought { folder: dir.display().to_string(), files: written, dirty: pack.dirty, via: via.into() })
}

/// Writes the files that are not there yet; one that is stays as it is, unless `overwrite`.
fn write_files(dir: &Path, files: &[PackedFile], overwrite: bool) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for file in files {
        let Some(rel) = safe_rel(&file.rel) else { continue };
        let to = dir.join(&rel);
        if to.exists() && !overwrite {
            continue;
        }
        let data = STANDARD.decode(&file.data).with_context(|| format!("{} came garbled", file.rel))?;
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&to, data).with_context(|| format!("writing {}", to.display()))?;
        written.push(file.rel.clone());
    }
    Ok(written)
}

/// Makes the clone already here match the pack's commit and branch, without touching work of its
/// own: it must have no uncommitted changes, and its branch may only move forward. A folder the
/// thread itself left (`resume`) still holds what it had then: that is put in a stash, not lost.
fn bring_up_to(dir: &Path, pack: &ProjectPack, resume: bool) -> Result<()> {
    let status = git(dir, &["status", "--porcelain", "--untracked-files=no"])?;
    let Some(head) = pack.head.as_deref() else {
        if !status.trim().is_empty() {
            bail!("{} has uncommitted changes of its own; commit or stash them there first", dir.display());
        }
        return Ok(());
    };
    if resume && !git(dir, &["status", "--porcelain"])?.trim().is_empty() && git(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_ok() {
        git(dir, &["-c", "user.name=Hub", "-c", "user.email=hub@localhost", "stash", "push", "--quiet", "--include-untracked", "-m", "Hub: what was here before the thread came back"])
            .context("setting aside what this folder still held from before the thread left")?;
    } else if !status.trim().is_empty() {
        bail!("{} has uncommitted changes of its own; commit or stash them there first", dir.display());
    }
    if let Some(bundle) = &pack.bundle {
        let bytes = STANDARD.decode(bundle).context("the history came garbled")?;
        let file = std::env::temp_dir().join(format!("hub-carry-{}.bundle", uuid::Uuid::new_v4().simple()));
        std::fs::write(&file, bytes)?;
        let fetched = git(dir, &["fetch", "--quiet", file.to_str().context("temp folder is not text")?, "HEAD:refs/hub/moved"]);
        crate::locked::clean_file(&file);
        fetched.context("fetching the history from the bundle")?;
    } else {
        // Too big to bundle: the commits must be on the remote.
        git(dir, &["fetch", "--quiet", "origin"]).warn_on_err("fetch origin");
    }
    if git(dir, &["cat-file", "-e", &format!("{head}^{{commit}}")]).is_err() {
        bail!("the commit it is on is not in {}, and was too big to carry; push it first", dir.display());
    }
    match pack.branch.as_deref() {
        Some(branch) => {
            let local = format!("refs/heads/{branch}");
            if git(dir, &["rev-parse", "--verify", "--quiet", &local]).is_err() {
                git(dir, &["checkout", "--quiet", "-b", branch, head])?;
            } else {
                git(dir, &["checkout", "--quiet", branch])?;
                if git(dir, &["merge-base", "--is-ancestor", head, "HEAD"]).is_ok() {
                    // This clone is already ahead of (or at) the commit; keep its commits.
                } else if git(dir, &["merge", "--ff-only", "--quiet", head]).is_err() {
                    bail!("branch {branch} here has commits that are not on the machine it came from; sort that out first");
                }
            }
        }
        None => {
            git(dir, &["checkout", "--quiet", head])?;
        }
    }
    Ok(())
}

/// Applies the uncommitted changes of a pack on top of the commit it is at, and writes its files
/// git does not track yet. A file that is already there is a clash and stops it before anything
/// is written.
fn apply_changes(dir: &Path, pack: &ProjectPack) -> Result<()> {
    for file in &pack.untracked {
        let rel = safe_rel(&file.rel).with_context(|| format!("{} is not a path inside the project", file.rel))?;
        if dir.join(&rel).exists() {
            bail!("{} is already in {}", file.rel, dir.display());
        }
    }
    if let Some(patch) = &pack.patch {
        let bytes = STANDARD.decode(patch).context("the changes came garbled")?;
        let file = std::env::temp_dir().join(format!("hub-carry-{}.patch", uuid::Uuid::new_v4().simple()));
        std::fs::write(&file, bytes)?;
        let applied = git(dir, &["apply", "--binary", "--whitespace=nowarn", file.to_str().context("temp folder is not text")?]);
        crate::locked::clean_file(&file);
        applied.context("applying the uncommitted changes")?;
    }
    for file in &pack.untracked {
        let Some(rel) = safe_rel(&file.rel) else { continue };
        let to = dir.join(rel);
        let data = STANDARD.decode(&file.data).with_context(|| format!("{} came garbled", file.rel))?;
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&to, data).with_context(|| format!("writing {}", to.display()))?;
    }
    Ok(())
}

fn clone_bundle(dir: &Path, bundle: &str, url: Option<&str>, branch: Option<&str>) -> Result<()> {
    let parent = dir.parent().context("pick a folder to put it in")?;
    std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    let bytes = STANDARD.decode(bundle).context("the history came garbled")?;
    let file = std::env::temp_dir().join(format!("hub-carry-{}.bundle", uuid::Uuid::new_v4().simple()));
    std::fs::write(&file, bytes)?;
    let target = dir.display().to_string();
    let mut args = vec!["clone", "--quiet"];
    if let Some(branch) = branch {
        args.extend(["--branch", branch]);
    }
    args.extend(["--", file.to_str().context("temp folder is not text")?, &target]);
    let cloned = git(parent, &args);
    crate::locked::clean_file(&file);
    cloned.context("cloning from the bundle")?;
    // Origin pointed at the bundle, which is gone: point it where the project pushes, so the
    // new clone pulls and pushes like the one it came from.
    match url {
        Some(url) => git(dir, &["remote", "set-url", "origin", url])?,
        None => git(dir, &["remote", "remove", "origin"])?,
    };
    Ok(())
}

/// A relative path that stays inside the project: no `..`, no root, no drive.
fn safe_rel(rel: &str) -> Option<PathBuf> {
    let path = Path::new(rel);
    path.components().all(|c| matches!(c, Component::Normal(_))).then(|| path.to_path_buf()).filter(|p| p.components().count() > 0)
}

/// What `git` printed, as it came (a patch may hold bytes that are not text).
fn git_bytes(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = crate::git::output(cwd, args, crate::git::LONG).context("git is not installed on this machine, or took too long")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("git failed").trim());
    }
    Ok(out.stdout)
}

fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = crate::git::output(cwd, args, crate::git::LONG).context("git is not installed on this machine, or took too long")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("git failed").trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        git(dir, args).unwrap();
    }

    fn repo(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        sh(dir, &["init", "--quiet", "--initial-branch", "main"]);
        sh(dir, &["config", "user.name", "Test"]);
        sh(dir, &["config", "user.email", "test@example.com"]);
        sh(dir, &["remote", "add", "origin", "https://github.com/someone/thing.git"]);
        std::fs::write(dir.join(".gitignore"), ".env*\n.dev.vars\nnode_modules/\nbuild.log\n").unwrap();
        std::fs::write(dir.join("README.md"), "hello\n").unwrap();
        sh(dir, &["add", "."]);
        sh(dir, &["commit", "--quiet", "-m", "first"]);
        sh(dir, &["checkout", "--quiet", "-b", "feature"]);
        std::fs::write(dir.join("README.md"), "hello again\n").unwrap();
        sh(dir, &["commit", "--quiet", "-am", "second"]);
    }

    #[test]
    fn a_project_with_no_commits_and_no_remote_moves_as_files() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("tincan/Demo");
        std::fs::create_dir_all(from.join("src")).unwrap();
        sh(&from, &["init", "--quiet", "--initial-branch", "master"]);
        std::fs::write(from.join(".gitignore"), "build.log\n").unwrap();
        std::fs::write(from.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(from.join("build.log"), "noise\n").unwrap();
        std::fs::write(from.join(".env"), "A=1\n").unwrap();

        let pack = pack(&from.display().to_string()).unwrap();
        assert!(pack.bundle.is_none() && pack.url.is_none());
        let to = tmp.path().join("pc/Dev/Demo");
        let brought = unpack(UnpackProject { path: to.display().to_string(), pack, sync: false, resume: false }).unwrap();
        assert_eq!(brought.via, "files");
        assert_eq!(std::fs::read_to_string(to.join("src/main.rs")).unwrap(), "fn main() {}\n");
        assert_eq!(std::fs::read_to_string(to.join(".env")).unwrap(), "A=1\n");
        assert!(!to.join("build.log").exists());
        assert!(to.join(".git").exists());
        assert_eq!(git(&to, &["symbolic-ref", "--short", "HEAD"]).unwrap().trim(), "master");
    }

    #[test]
    fn a_folder_too_big_for_files_is_saved_as_a_checkpoint_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("pc/Hubtest");
        std::fs::create_dir_all(from.join("node_modules/x")).unwrap();
        sh(&from, &["init", "--quiet", "--initial-branch", "main"]);
        std::fs::write(from.join("big.bin"), vec![b'a'; (MAX_FILE + 10) as usize]).unwrap();
        std::fs::write(from.join("small.txt"), "hi\n").unwrap();
        std::fs::write(from.join("node_modules/x/dep.js"), "no\n").unwrap();

        let pack = pack_for(&from.display().to_string(), true).unwrap();
        assert!(pack.bundle.is_some(), "carried as history");
        assert!(git(&from, &["log", "-1", "--format=%s"]).unwrap().contains(CHECKPOINT_MESSAGE));
        assert!(!from.join(".gitignore").exists(), "the project itself is not changed");
        let to = tmp.path().join("server/Hubtest");
        let brought = unpack(UnpackProject { path: to.display().to_string(), pack, sync: true, resume: false }).unwrap();
        assert_eq!(brought.via, "bundle");
        assert!(to.join("big.bin").exists() && to.join("small.txt").exists());
        assert!(!to.join("node_modules").exists());
    }

    #[test]
    fn uncommitted_work_too_big_for_a_patch_is_saved_as_a_checkpoint_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("pc/thing");
        repo(&from);
        std::fs::write(from.join("blob.bin"), vec![b'b'; (MAX_CHANGES + 10) as usize]).unwrap();
        let pack = pack_for(&from.display().to_string(), true).unwrap();
        assert!(pack.untracked.is_empty() && pack.patch.is_none());
        assert!(git(&from, &["log", "-1", "--format=%s"]).unwrap().contains(CHECKPOINT_MESSAGE));
        assert_eq!(pack.head.as_deref(), Some(git(&from, &["rev-parse", "HEAD"]).unwrap().trim()));
    }

    #[test]
    fn a_folder_outside_git_moves_as_plain_files() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("tincan/scratch/2026-11-01-130000");
        std::fs::create_dir_all(from.join("notes/deep")).unwrap();
        std::fs::create_dir_all(from.join("node_modules/x")).unwrap();
        std::fs::write(from.join("a.txt"), "one\n").unwrap();
        std::fs::write(from.join("notes/deep/b.md"), "two\n").unwrap();
        std::fs::write(from.join("node_modules/x/c.js"), "no\n").unwrap();

        let pack = pack_folder(&from.display().to_string()).unwrap();
        assert!(pack.plain);
        let to = tmp.path().join("pc/scratch/2026-11-01-130000");
        let brought = unpack(UnpackProject { path: to.display().to_string(), pack: pack.clone(), sync: true, resume: false }).unwrap();
        assert_eq!(brought.via, "files");
        assert_eq!(std::fs::read_to_string(to.join("notes/deep/b.md")).unwrap(), "two\n");
        assert_eq!(std::fs::read_to_string(to.join("a.txt")).unwrap(), "one\n");
        assert!(!to.join("node_modules").exists());
        assert!(!to.join(".git").exists(), "it is not made a repository");
        // A folder with something in it is not written over.
        assert!(unpack(UnpackProject { path: to.display().to_string(), pack, sync: true, resume: false }).is_err());
        // Not a folder at all.
        assert!(pack_folder(&tmp.path().join("nope").display().to_string()).is_err());
    }

    #[test]
    fn a_project_with_commits_and_no_remote_moves_by_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("tincan/Demo");
        repo(&from);
        sh(&from, &["remote", "remove", "origin"]);
        let pack = pack(&from.display().to_string()).unwrap();
        let to = tmp.path().join("pc/Dev/Demo");
        let brought = unpack(UnpackProject { path: to.display().to_string(), pack, sync: false, resume: false }).unwrap();
        assert_eq!(brought.via, "bundle");
        assert_eq!(std::fs::read_to_string(to.join("README.md")).unwrap().replace("\r\n", "\n"), "hello again\n");
    }

    #[test]
    fn env_files_are_the_ones_a_project_needs() {
        for yes in [".env", ".env.local", "api/.env.production", ".envrc", "worker/.dev.vars"] {
            assert!(is_env_file(yes), "{yes}");
        }
        for no in ["env", ".environment", "src/env.ts", "build.log", ".envy/x"] {
            assert!(!is_env_file(no), "{no}");
        }
    }

    #[test]
    fn a_project_moves_with_its_history_branch_and_env_files() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("pc/thing");
        repo(&from);
        std::fs::write(from.join(".env"), "TOKEN=abc\n").unwrap();
        std::fs::create_dir_all(from.join("api")).unwrap();
        std::fs::write(from.join("api/.env.local"), "PORT=1\n").unwrap();
        std::fs::create_dir_all(from.join("node_modules/x")).unwrap();
        std::fs::write(from.join("node_modules/x/.env"), "nope\n").unwrap();
        std::fs::write(from.join("build.log"), "noise\n").unwrap();
        std::fs::write(from.join("README.md"), "not committed\n").unwrap();

        let pack = pack(&from.display().to_string()).unwrap();
        assert_eq!(pack.branch.as_deref(), Some("feature"));
        assert_eq!(pack.url.as_deref(), Some("https://github.com/someone/thing.git"));
        assert_eq!(pack.dirty, 1);
        let mut rels: Vec<&str> = pack.files.iter().map(|f| f.rel.as_str()).collect();
        rels.sort();
        assert_eq!(rels, [".env", "api/.env.local"]);

        let to = tmp.path().join("clanker/code/thing");
        let brought = unpack(UnpackProject { path: to.display().to_string(), pack: pack.clone(), sync: false, resume: false }).unwrap();
        assert_eq!(brought.via, "bundle");
        assert_eq!(brought.dirty, 1);
        assert_eq!(std::fs::read_to_string(to.join(".env")).unwrap(), "TOKEN=abc\n");
        assert_eq!(std::fs::read_to_string(to.join("api/.env.local")).unwrap(), "PORT=1\n");
        // Checked out with this machine's git settings: CRLF with Git for Windows' default.
        assert_eq!(std::fs::read_to_string(to.join("README.md")).unwrap().replace("\r\n", "\n"), "hello again\n");
        assert!(!to.join("build.log").exists());
        assert_eq!(git(&to, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap().trim(), "feature");
        assert_eq!(git(&to, &["remote", "get-url", "origin"]).unwrap().trim(), "https://github.com/someone/thing.git");
        assert!(git(&to, &["rev-parse", "--verify", "--quiet", "origin/main"]).is_ok(), "other branches come along");

        // Carried again: the clone is there, so only files it lacks are added, never overwritten.
        std::fs::write(to.join(".env"), "TOKEN=changed-there\n").unwrap();
        std::fs::remove_file(to.join("api/.env.local")).unwrap();
        let again = unpack(UnpackProject { path: to.display().to_string(), pack, sync: false, resume: false }).unwrap();
        assert_eq!(again.via, "existing");
        assert_eq!(again.files, ["api/.env.local"]);
        assert_eq!(std::fs::read_to_string(to.join(".env")).unwrap(), "TOKEN=changed-there\n");
    }

    /// A repo with uncommitted work: a changed tracked file, a staged one and a new file.
    fn messy(dir: &Path) {
        repo(dir);
        std::fs::write(dir.join("README.md"), "edited after the commit\n").unwrap();
        std::fs::write(dir.join("staged.txt"), "staged\n").unwrap();
        sh(dir, &["add", "staged.txt"]);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/new.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.join("build.log"), "ignored\n").unwrap();
    }

    #[test]
    fn a_thread_moves_with_its_uncommitted_changes_to_a_fresh_clone() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("pc/thing");
        messy(&from);
        let pack = pack_for(&from.display().to_string(), true).unwrap();
        assert!(pack.patch.is_some());
        assert_eq!(pack.untracked.iter().map(|f| f.rel.as_str()).collect::<Vec<_>>(), ["src/new.rs"]);

        let to = tmp.path().join("server/thing");
        let brought = unpack(UnpackProject { path: to.display().to_string(), pack, sync: true, resume: false }).unwrap();
        assert_eq!(brought.via, "bundle");
        let read = |rel: &str| std::fs::read_to_string(to.join(rel)).unwrap().replace("\r\n", "\n");
        assert_eq!(read("README.md"), "edited after the commit\n");
        assert_eq!(read("staged.txt"), "staged\n");
        assert_eq!(read("src/new.rs"), "fn main() {}\n");
        assert!(!to.join("build.log").exists(), "ignored files stay");
        assert_eq!(git(&to, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap().trim(), "feature");
    }

    #[test]
    fn an_existing_clone_is_brought_forward_and_its_own_changes_are_not_mixed() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("pc/thing");
        repo(&from);
        // The other machine has the clone at the first commit of the branch.
        let to = tmp.path().join("server/thing");
        let first = pack(&from.display().to_string()).unwrap();
        unpack(UnpackProject { path: to.display().to_string(), pack: first, sync: false, resume: false }).unwrap();
        sh(&to, &["config", "user.name", "Test"]);
        sh(&to, &["config", "user.email", "test@example.com"]);
        sh(&to, &["reset", "--quiet", "--hard", "HEAD~1"]);

        // The thread went on: a commit and uncommitted work.
        std::fs::write(from.join("later.txt"), "later\n").unwrap();
        sh(&from, &["add", "later.txt"]);
        sh(&from, &["commit", "--quiet", "-m", "third"]);
        std::fs::write(from.join("README.md"), "uncommitted\n").unwrap();
        let moving = pack_for(&from.display().to_string(), true).unwrap();

        // Its own changes there stop it.
        std::fs::write(to.join("README.md"), "mine\n").unwrap();
        let err = unpack(UnpackProject { path: to.display().to_string(), pack: moving.clone(), sync: true, resume: false }).unwrap_err();
        assert!(format!("{err:#}").contains("uncommitted changes of its own"), "{err:#}");
        assert_eq!(std::fs::read_to_string(to.join("README.md")).unwrap(), "mine\n");

        sh(&to, &["checkout", "--quiet", "--", "README.md"]);
        let brought = unpack(UnpackProject { path: to.display().to_string(), pack: moving, sync: true, resume: false }).unwrap();
        assert_eq!(brought.via, "existing");
        assert_eq!(std::fs::read_to_string(to.join("README.md")).unwrap().replace("\r\n", "\n"), "uncommitted\n");
        assert!(to.join("later.txt").exists());
        assert_eq!(git(&to, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap().trim(), "feature");
        assert_eq!(git(&to, &["rev-parse", "HEAD"]).unwrap(), git(&from, &["rev-parse", "HEAD"]).unwrap());
    }

    #[test]
    fn a_thread_moves_back_to_the_folder_it_left() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("tincan/thing");
        messy(&first);
        sh(&first, &["remote", "remove", "origin"]);
        // Out to the PC and a bit of work there...
        let out = pack_for(&first.display().to_string(), true).unwrap();
        let pc = tmp.path().join("pc/thing");
        unpack(UnpackProject { path: pc.display().to_string(), pack: out, sync: true, resume: false }).unwrap();
        sh(&pc, &["config", "user.name", "Test"]);
        sh(&pc, &["config", "user.email", "test@example.com"]);
        std::fs::write(pc.join("README.md"), "worked on at the PC\n").unwrap();
        sh(&pc, &["add", "README.md"]);
        sh(&pc, &["commit", "--quiet", "-m", "pc work"]);
        std::fs::write(pc.join("later.txt"), "later\n").unwrap();
        let back = pack_for(&pc.display().to_string(), true).unwrap();

        // ...and back to the old folder, which still holds the changes it had when it left. Without
        // `resume` it is somebody else's folder.
        assert!(unpack(UnpackProject { path: first.display().to_string(), pack: back.clone(), sync: true, resume: false }).is_err());
        let brought = unpack(UnpackProject { path: first.display().to_string(), pack: back, sync: true, resume: true }).unwrap();
        assert_eq!(brought.via, "existing");
        let read = |rel: &str| std::fs::read_to_string(first.join(rel)).unwrap().replace("\r\n", "\n");
        assert_eq!(read("README.md"), "worked on at the PC\n");
        assert_eq!(read("later.txt"), "later\n");
        assert_eq!(git(&first, &["rev-parse", "HEAD"]).unwrap(), git(&pc, &["rev-parse", "HEAD"]).unwrap());
        assert!(git(&first, &["stash", "list"]).unwrap().contains("came back"), "what was there before is kept");
    }

    #[test]
    fn a_folder_with_no_commits_moves_back_over_its_old_files() {
        let tmp = tempfile::tempdir().unwrap();
        let first = tmp.path().join("tincan/test");
        std::fs::create_dir_all(&first).unwrap();
        sh(&first, &["init", "--quiet"]);
        std::fs::write(first.join("a.txt"), "old\n").unwrap();
        let out = pack_for(&first.display().to_string(), true).unwrap();
        let pc = tmp.path().join("pc/test");
        unpack(UnpackProject { path: pc.display().to_string(), pack: out, sync: true, resume: false }).unwrap();
        std::fs::write(pc.join("a.txt"), "new\n").unwrap();
        let back = pack_for(&pc.display().to_string(), true).unwrap();
        let brought = unpack(UnpackProject { path: first.display().to_string(), pack: back, sync: true, resume: true }).unwrap();
        assert_eq!(brought.via, "existing");
        assert_eq!(std::fs::read_to_string(first.join("a.txt")).unwrap(), "new\n");
    }

    #[test]
    fn another_projects_folder_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("a");
        repo(&from);
        let to = tmp.path().join("b");
        std::fs::create_dir_all(&to).unwrap();
        std::fs::write(to.join("mine.txt"), "x").unwrap();
        let pack = pack(&from.display().to_string()).unwrap();
        assert!(unpack(UnpackProject { path: to.display().to_string(), pack, sync: false, resume: false }).is_err());
    }

    #[test]
    fn files_cannot_climb_out_of_the_project() {
        assert!(safe_rel("../.env").is_none());
        assert!(safe_rel("/etc/.env").is_none());
        assert!(safe_rel("a/../../.env").is_none());
        assert!(safe_rel("").is_none());
        assert_eq!(safe_rel("api/.env"), Some(PathBuf::from("api/.env")));
    }
}
