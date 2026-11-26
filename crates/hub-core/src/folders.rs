//! Folders on this machine, for picking where a thread runs. The app asks the machine the thread
//! will run on (`?host=` forwards it), so a folder on the Raspberry Pi is browsed on the Pi.
//!
//! Repositories are known by their remote, as in T3 Code: the same repository cloned on two
//! machines is one project, so moving a thread to another machine finds its folder there.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::model::{FolderEntry, FolderListing, ProjectFolder, ProjectFolders};

/// More than this in one folder is not something to pick from a list.
const MAX_ENTRIES: usize = 400;

/// How deep below each search root a repository is looked for.
const FIND_DEPTH: usize = 3;

/// The user's home folder, or the root when the environment names none.
pub fn home() -> PathBuf {
    home_dir().unwrap_or_else(|| "/".into())
}

pub use crate::home_files::home_dir;

/// Whether two paths name the same folder, read as text: slashes either way, a trailing slash or
/// not, and case do not count. For paths typed in or copied from different places, not symlinks.
pub fn same_folder(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().trim_end_matches(['/', '\\']).replace('\\', "/").to_lowercase();
    norm(a) == norm(b)
}

/// `~` and `~/x` in the home folder; everything else as given.
pub fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    let full = match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        _ if path == "~" || path.is_empty() => home(),
        Some(rest) => home().join(rest),
        None => PathBuf::from(path),
    };
    // "~/srv/" is the folder ~/srv.
    full.components().collect()
}

/// The folders in `path` (the home folder when empty), repositories marked. Hidden folders only
/// with `hidden`.
pub fn list(path: &str, hidden: bool) -> Result<FolderListing> {
    let dir = expand(path);
    if cfg!(not(windows)) && looks_like_windows(path) {
        bail!("{path} is a Windows path, and this machine is not Windows");
    }
    let read = std::fs::read_dir(&dir).with_context(|| format!("cannot open {}", dir.display()))?;
    let mut entries: Vec<FolderEntry> = read
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir() || (t.is_symlink() && e.path().is_dir())).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !hidden && (name.starts_with('.') || is_junk(&name) || is_hidden(&e)) {
                return None;
            }
            let path = e.path();
            Some(FolderEntry { repo: path.join(".git").exists(), path: path.display().to_string(), name })
        })
        .collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    let more = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);
    Ok(FolderListing {
        path: dir.display().to_string(),
        parent: dir.parent().map(|p| p.display().to_string()),
        home: home().display().to_string(),
        sep: std::path::MAIN_SEPARATOR.to_string(),
        repo: dir.join(".git").exists(),
        roots: roots(),
        entries,
        more,
    })
}

/// Folders nobody starts a thread in: what Windows keeps in a user's home, system folders and
/// build output. Shown again with `hidden`.
const JUNK: &[&str] = &[
    "AppData", "Application Data", "Cookies", "Contacts", "Favorites", "Links", "Local Settings", "My Documents",
    "NetHood", "PrintHood", "Recent", "Saved Games", "Searches", "SendTo", "Start Menu", "Templates", "3D Objects",
    "IntelGraphicsProfiles", "MicrosoftEdgeBackups", "System Volume Information", "Config.Msi", "Recovery",
    "PerfLogs", "node_modules", "__pycache__", "snap",
];

fn is_junk(name: &str) -> bool {
    name.starts_with('$') || JUNK.iter().any(|j| j.eq_ignore_ascii_case(name))
}

/// Marked hidden or system on Windows, as AppData and the old profile links are.
#[cfg(windows)]
fn is_hidden(e: &std::fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    e.metadata().map(|m| m.file_attributes() & 0x6 != 0).unwrap_or(false)
}
#[cfg(not(windows))]
fn is_hidden(_: &std::fs::DirEntry) -> bool {
    false
}

/// The usual places code is kept on a machine, those that exist: the projects folder, `Dev`,
/// `code`, `src` and the like in home, and `Dev`, `Code` or `Projects` at the top of each drive.
pub fn code_roots(projects_root: &str) -> Vec<PathBuf> {
    let home = home();
    let mut all = vec![expand(projects_root)];
    for name in ["Dev", "dev", "code", "Code", "src", "projects", "Projects", "repos", "git", "source/repos", "Documents/GitHub", "srv"] {
        all.push(home.join(name));
    }
    if cfg!(windows) {
        for drive in roots() {
            for name in ["Dev", "Code", "Projects", "src", "repos", "git"] {
                all.push(Path::new(&drive).join(name));
            }
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for p in all {
        // Case-insensitive file systems would list ~/Dev and ~/dev twice.
        let canon = std::fs::canonicalize(&p).ok().filter(|c| c.is_dir());
        if let Some(c) = canon
            && !out.iter().any(|o| std::fs::canonicalize(o).ok().as_ref() == Some(&c)) {
                out.push(p);
            }
    }
    out
}

/// At most this many projects; past it the picker's search still finds the rest by path.
const MAX_PROJECTS: usize = 600;

/// The repositories below the code folders (three levels down), those right in the home folder
/// (where a server or a Raspberry Pi often keeps them: `~/Desk`, `~/Forman`) and the `known`
/// folders threads ran in, newest first.
pub fn projects(projects_root: &str, known: &[String]) -> ProjectFolders {
    let roots = code_roots(projects_root);
    let mut found = scan(&roots, known);
    for p in in_home(&home()) {
        if !found.iter().any(|f| f.path == p.path) {
            found.push(p);
        }
    }
    found.sort_by(|a, b| b.touched.cmp(&a.touched).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    fill_behind(&mut found);
    ProjectFolders {
        projects: found,
        roots: roots.iter().map(|r| r.display().to_string()).collect(),
        root: expand(projects_root).display().to_string(),
        home: home().display().to_string(),
        sep: std::path::MAIN_SEPARATOR.to_string(),
    }
}

fn scan(roots: &[PathBuf], known: &[String]) -> Vec<ProjectFolder> {
    let mut found: Vec<ProjectFolder> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for path in known {
        let path = expand(path);
        if path.join(".git").exists() && seen.insert(path.clone()) {
            found.push(project(&path));
        }
    }
    for start in roots {
        let mut stack = vec![(start.clone(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            if found.len() >= MAX_PROJECTS {
                break;
            }
            if dir.join(".git").exists() {
                if seen.insert(dir.clone()) {
                    found.push(project(&dir));
                }
                continue;
            }
            if depth >= FIND_DEPTH {
                continue;
            }
            let Ok(read) = std::fs::read_dir(&dir) else { continue };
            for e in read.flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || SKIP.contains(&name.as_ref()) || is_junk(&name) {
                    continue;
                }
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push((e.path(), depth + 1));
                }
            }
        }
    }
    found.sort_by(|a, b| b.touched.cmp(&a.touched).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    found
}

/// The repositories directly in `home`, one level only: home holds too much else to walk.
fn in_home(home: &Path) -> Vec<ProjectFolder> {
    let Ok(read) = std::fs::read_dir(home) else { return Vec::new() };
    read.flatten()
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.') && !SKIP.contains(&name.as_ref()) && !is_junk(&name) && e.path().join(".git").exists()
        })
        .take(MAX_PROJECTS)
        .map(|e| project(&e.path()))
        .collect()
}

/// One repository, read from its `.git` without running git, so a hundred of them stay quick.
fn project(dir: &Path) -> ProjectFolder {
    let git = git_dir(dir);
    let head = git.as_ref().and_then(|g| std::fs::read_to_string(g.join("HEAD")).ok());
    let branch = head.as_deref().and_then(|h| h.trim().strip_prefix("ref: refs/heads/")).map(str::to_owned);
    // A worktree keeps its config in the main repository.
    let common = git.as_ref().map(|g| {
        std::fs::read_to_string(g.join("commondir")).map(|c| clean(&g.join(c.trim()))).unwrap_or_else(|_| g.clone())
    });
    let key = common.as_ref().and_then(|c| std::fs::read_to_string(c.join("config")).ok()).and_then(|c| origin_url(&c)).and_then(|u| key_of(&u));
    let touched = git
        .iter()
        .flat_map(|g| [g.join("index"), g.join("HEAD"), g.join("logs").join("HEAD")])
        .filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .max()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let linked = git.as_ref().is_some_and(|g| g.join("commondir").is_file());
    // origin/HEAD names the branch origin treats as its own.
    let main = common
        .as_ref()
        .and_then(|c| std::fs::read_to_string(c.join("refs/remotes/origin/HEAD")).ok())
        .and_then(|h| h.trim().strip_prefix("ref: refs/remotes/origin/").map(str::to_owned));
    // The repository a linked worktree belongs to: the folder holding the common `.git`.
    let worktree_of = common
        .as_ref()
        .filter(|c| linked && c.file_name().is_some_and(|n| n == ".git"))
        .and_then(|c| c.parent())
        .map(|p| p.display().to_string());
    let worktrees = if linked {
        0
    } else {
        common
            .as_ref()
            .and_then(|c| std::fs::read_dir(c.join("worktrees")).ok())
            .map(|r| r.flatten().filter(|e| e.path().is_dir()).count() as u32)
            .unwrap_or(0)
    };
    ProjectFolder {
        name: dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string()),
        path: dir.display().to_string(),
        branch,
        key,
        touched,
        main,
        behind: None,
        worktree_of,
        worktrees,
    }
}

/// How many projects get a "behind origin" count: the newest ones, which are what the picker shows.
const BEHIND_FOR: usize = 30;

/// Counts the commits each of the newest projects lacks from its upstream, as of the last fetch.
/// One `git rev-list` each, side by side, so thirty stay quick.
fn fill_behind(found: &mut [ProjectFolder]) {
    let n = found.len().min(BEHIND_FOR);
    std::thread::scope(|scope| {
        for p in found[..n].iter_mut().filter(|p| p.branch.is_some()) {
            scope.spawn(move || p.behind = behind(Path::new(&p.path)));
        }
    });
}

fn behind(dir: &Path) -> Option<u32> {
    let out = crate::git::output(dir, &["rev-list", "--count", "HEAD..@{upstream}"], crate::git::TIMEOUT).ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// The path with `.` and `..` resolved as text (`a/b/../c` is `a/c`), without touching the disk.
fn clean(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir if out.file_name().is_some() => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// `.git` itself, or where a `.git` file (a worktree or submodule) points.
fn git_dir(dir: &Path) -> Option<PathBuf> {
    let dot = dir.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let text = std::fs::read_to_string(&dot).ok()?;
    let to = text.trim().strip_prefix("gitdir:")?.trim();
    Some(dir.join(to))
}

/// The `url` of `[remote "origin"]` in a git config.
fn origin_url(config: &str) -> Option<String> {
    let mut inside = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line.replace(' ', "").eq_ignore_ascii_case("[remote\"origin\"]");
        } else if inside
            && let Some((k, v)) = line.split_once('=')
                && k.trim() == "url" {
                    return Some(v.trim().to_owned());
                }
    }
    None
}

fn looks_like_windows(path: &str) -> bool {
    let b = path.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Where browsing can start besides home: drives on Windows, `/` elsewhere.
fn roots() -> Vec<String> {
    if cfg!(windows) {
        (b'C'..=b'Z').map(|d| format!("{}:\\", d as char)).filter(|d| Path::new(d).is_dir()).collect()
    } else {
        vec!["/".into()]
    }
}

/// Makes a folder for a new project, with `git init` when asked. An existing empty folder is fine;
/// one with files in it is left alone unless it is only being opened.
pub fn create(path: &str, git: bool) -> Result<String> {
    let dir = expand(path);
    if dir.as_os_str().is_empty() || dir.parent().is_none() {
        bail!("pick a folder to create");
    }
    if dir.is_file() {
        bail!("{} is a file", dir.display());
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    if git && !dir.join(".git").exists() {
        let out = crate::git::output(&dir, &["init"], crate::git::TIMEOUT).context("git is not installed on this machine, or took too long")?;
        if !out.status.success() {
            bail!("git init failed: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
    }
    Ok(dir.display().to_string())
}

/// The repository `path` belongs to, as `host/owner/repo` (no scheme, user or `.git`), so the
/// same repository has the same key on every machine whichever URL form it was cloned with.
pub fn repo_key(path: &str) -> Option<String> {
    let url = crate::git::run(&expand(path), &["remote", "get-url", "origin"])?;
    key_of(url.trim())
}

/// The origin as a URL that can be cloned elsewhere, without any token in it.
pub fn clone_url(path: &str) -> Option<String> {
    let url = crate::git::run(&expand(path), &["remote", "get-url", "origin"])?;
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        let rest = rest.split_once('@').filter(|(auth, _)| !auth.contains('/')).map(|(_, r)| r).unwrap_or(rest);
        return Some(format!("https://{rest}"));
    }
    key_of(url).is_some().then(|| url.to_owned())
}

pub(crate) fn key_of(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let rest = if let Some((_, rest)) = url.split_once("://") {
        // scheme://[user@]host[:port]/path
        let rest = rest.split_once('@').filter(|(auth, _)| !auth.contains('/')).map(|(_, r)| r).unwrap_or(rest);
        let (host, path) = rest.split_once('/')?;
        format!("{}/{path}", host.split(':').next().unwrap_or(host))
    } else {
        // scp style: [user@]host:path. A local path (C:\x, /x, ./x) has no remote identity.
        let (host, path) = url.split_once(':')?;
        if host.contains('/') || host.contains('\\') || host.len() == 1 {
            return None;
        }
        let host = host.rsplit('@').next().unwrap_or(host);
        format!("{host}/{}", path.trim_start_matches('/'))
    };
    let key = rest.trim_end_matches(".git").to_lowercase();
    (key.matches('/').count() >= 2).then_some(key)
}

/// Folders on this machine holding the repository with `key` (see [`repo_key`]): looked for in
/// `known` folders first, then up to three levels below the home folder and `roots`.
pub fn find_repo(key: &str, known: &[String], roots: &[String]) -> Vec<String> {
    let key = key.trim().to_lowercase();
    let mut found: Vec<String> = Vec::new();
    let mut add = |p: &Path| {
        let s = p.display().to_string();
        if !found.contains(&s) {
            found.push(s);
        }
    };
    for path in known {
        let path = expand(path);
        if path.join(".git").exists() && repo_key(&path.display().to_string()).as_deref() == Some(&key) {
            add(&path);
        }
    }
    let mut starts: Vec<PathBuf> = roots.iter().map(|r| expand(r)).collect();
    starts.push(home());
    let mut seen = std::collections::HashSet::new();
    for start in starts {
        let mut stack = vec![(start, 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            if !seen.insert(dir.clone()) {
                continue;
            }
            if dir.join(".git").exists() {
                if repo_key(&dir.display().to_string()).as_deref() == Some(&key) {
                    add(&dir);
                }
                // Repositories inside repositories are rare; worktrees are found as their own.
                continue;
            }
            if depth >= FIND_DEPTH {
                continue;
            }
            let Ok(read) = std::fs::read_dir(&dir) else { continue };
            for e in read.flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                if name.starts_with('.') || SKIP.contains(&name.as_ref()) {
                    continue;
                }
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push((e.path(), depth + 1));
                }
            }
        }
    }
    found
}

/// Folders never worth walking into when looking for a repository.
const SKIP: &[&str] = &["node_modules", "target", "AppData", "Library", "snap", "go", "venv", "__pycache__"];

/// Clones `url` into `path` (which must not exist yet or be empty) and returns the folder.
pub fn clone(url: &str, path: &str) -> Result<String> {
    let dir = expand(path);
    if dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false) {
        bail!("{} already has files in it", dir.display());
    }
    let parent = dir.parent().context("pick a folder to clone into")?;
    std::fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))?;
    let out = crate::git::output(parent, &["clone", "--", url, &dir.display().to_string()], std::time::Duration::from_secs(600))
        .context("git is not installed on this machine, or the clone took too long")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("git clone failed");
        bail!("{}", last.trim());
    }
    Ok(dir.display().to_string())
}

/// The folder of the project called (or in) `name`.
pub async fn project_folder(store: &crate::store::Store, name: &str) -> Result<String> {
    let projects = store.projects().await?;
    let want = name.trim().to_lowercase();
    let found = projects
        .iter()
        .find(|p| p.name.to_lowercase() == want || same_folder(&p.path, name))
        .or_else(|| projects.iter().find(|p| p.name.to_lowercase().contains(&want)));
    if let Some(p) = found {
        return Ok(p.path.clone());
    }
    // Folders threads work in count as projects too.
    let threads = store.threads().await?;
    let folder = threads.iter().map(|t| &t.cwd).find(|cwd| {
        PathBuf::from(cwd).file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == want)
    });
    folder.cloned().with_context(|| {
        let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
        format!("no project {name:?}; the projects are {}", names.join(", "))
    })
}

#[cfg(test)]
fn git_cmd(cwd: &Path, args: &[&str]) -> std::process::Command {
    let mut cmd = crate::proc::command("git");
    cmd.args(args).current_dir(cwd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_repository_has_one_key_whatever_the_url() {
        let want = Some("github.com/karlssonchristoffer/hub".to_owned());
        assert_eq!(key_of("https://github.com/KarlssonChristoffer/hub.git"), want);
        assert_eq!(key_of("https://x-access-token:abc@github.com/KarlssonChristoffer/hub"), want);
        assert_eq!(key_of("git@github.com:KarlssonChristoffer/hub.git"), want);
        assert_eq!(key_of("ssh://git@github.com:22/KarlssonChristoffer/hub.git/"), want);
        assert_eq!(key_of("C:\\Dev\\hub"), None);
        assert_eq!(key_of("/srv/git/hub.git"), None);
        assert_eq!(key_of("../hub"), None);
    }

    fn repo(dir: &Path, url: &str) {
        std::fs::create_dir_all(dir).unwrap();
        assert!(git_cmd(dir, &["init", "-q"]).status().unwrap().success());
        assert!(git_cmd(dir, &["remote", "add", "origin", url]).status().unwrap().success());
    }

    #[test]
    fn lists_folders_and_finds_a_repository_by_its_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        repo(&root.join("dev/hub"), "git@github.com:KarlssonChristoffer/hub.git");
        repo(&root.join("dev/other"), "https://github.com/someone/other");
        std::fs::create_dir_all(root.join("dev/node_modules/hub")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("file.txt"), "x").unwrap();

        let listing = list(&root.display().to_string(), false).unwrap();
        assert_eq!(listing.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["dev"]);
        let dev = list(&root.join("dev").display().to_string(), false).unwrap();
        assert!(dev.entries.iter().find(|e| e.name == "hub").unwrap().repo);
        assert!(!dev.entries.iter().any(|e| e.name == "node_modules"), "build output is not offered");

        let key = repo_key(&root.join("dev/hub").display().to_string()).unwrap();
        assert_eq!(key, "github.com/karlssonchristoffer/hub");
        let found = find_repo("github.com/KarlssonChristoffer/hub", &[], &[root.display().to_string()]);
        // The home folder is searched too, so a real clone of Hub there comes after this one.
        assert_eq!(found.first(), Some(&root.join("dev").join("hub").display().to_string()));
        assert!(!found.iter().any(|f| f.ends_with("other")));
    }

    #[test]
    fn creates_a_project_folder_with_git_and_clones_into_a_new_one() {
        let tmp = tempfile::tempdir().unwrap();
        let made = create(&tmp.path().join("a/new-game").display().to_string(), true).unwrap();
        assert!(Path::new(&made).join(".git").is_dir());

        std::fs::write(Path::new(&made).join("README.md"), "hi").unwrap();
        assert!(git_cmd(Path::new(&made), &["add", "."]).status().unwrap().success());
        let mut commit = git_cmd(Path::new(&made), &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "x"]);
        assert!(commit.status().unwrap().success());
        let copy = clone(&made, &tmp.path().join("b/copy").display().to_string()).unwrap();
        assert!(Path::new(&copy).join("README.md").is_file());
        assert!(clone(&made, &copy).is_err(), "never clones over files");
    }

    #[test]
    fn finds_projects_in_code_folders_and_hides_junk() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("code");
        repo(&root.join("hub"), "git@github.com:KarlssonChristoffer/hub.git");
        repo(&root.join("games/atlas"), "https://github.com/o/atlas");
        std::fs::create_dir_all(root.join("node_modules/x/.git")).unwrap();
        std::fs::create_dir_all(root.join("AppData/y/.git")).unwrap();
        std::fs::create_dir_all(root.join("notes")).unwrap();

        let found = scan(std::slice::from_ref(&root), &[]);
        let mut names: Vec<_> = found.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["atlas", "hub"]);
        assert!(code_roots(&root.display().to_string()).contains(&root));
        let hub = found.iter().find(|p| p.name == "hub").unwrap();
        assert_eq!(hub.key.as_deref(), Some("github.com/karlssonchristoffer/hub"));
        assert!(hub.branch.is_some());
        assert!(hub.touched > 0);

        let listing = list(&root.display().to_string(), false).unwrap();
        assert_eq!(listing.entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["games", "hub", "notes"]);

        // Right in home, as on tincan: found one level down, not deeper.
        let home = tmp.path().join("home");
        repo(&home.join("Desk"), "https://github.com/o/desk");
        repo(&home.join("stuff/deep"), "https://github.com/o/deep");
        let names: Vec<_> = in_home(&home).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Desk"]);
        assert_eq!(list(&root.display().to_string(), true).unwrap().entries.len(), 5);
    }

    #[test]
    fn knows_worktrees_the_main_branch_and_how_far_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("hub");
        let commit = |dir: &Path, msg: &str| {
            let mut c = git_cmd(dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", msg]);
            assert!(c.status().unwrap().success());
        };
        // An "origin" with two commits, and a clone that has only the first.
        let origin = tmp.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        assert!(git_cmd(&origin, &["init", "-q", "-b", "main"]).status().unwrap().success());
        commit(&origin, "one");
        assert!(git_cmd(tmp.path(), &["clone", "-q", origin.to_str().unwrap(), main.to_str().unwrap()]).status().unwrap().success());
        commit(&origin, "two");
        assert!(git_cmd(&main, &["fetch", "-q"]).status().unwrap().success());
        let wt = tmp.path().join("hub-wt");
        assert!(git_cmd(&main, &["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()]).status().unwrap().success());

        let mut found = scan(std::slice::from_ref(&tmp.path().to_path_buf()), &[]);
        fill_behind(&mut found);
        let hub = found.iter().find(|p| p.path == main.display().to_string()).unwrap();
        assert_eq!(hub.main.as_deref(), Some("main"));
        assert_eq!(hub.worktrees, 1);
        assert_eq!(hub.worktree_of, None);
        assert_eq!(hub.behind, Some(1), "one commit on origin that this folder lacks");
        let linked = found.iter().find(|p| p.path == wt.display().to_string()).unwrap();
        assert_eq!(linked.worktree_of.as_deref(), Some(main.display().to_string().as_str()));
        assert_eq!(linked.branch.as_deref(), Some("feature"));
        assert_eq!(linked.behind, None, "a new branch has no upstream");
    }

    #[test]
    fn reads_origin_from_a_git_config() {
        let config = "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = https://x/y/z\n[remote \"origin\"]\n\turl = git@github.com:o/r.git\n";
        assert_eq!(origin_url(config).as_deref(), Some("git@github.com:o/r.git"));
    }

    #[test]
    fn clone_url_drops_tokens() {
        let tmp = tempfile::tempdir().unwrap();
        repo(tmp.path(), "https://x-access-token:secret@github.com/o/r.git");
        assert_eq!(clone_url(&tmp.path().display().to_string()).as_deref(), Some("https://github.com/o/r.git"));
    }
}
