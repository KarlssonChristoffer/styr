//! What a phone may read of a project's files: the browser in the iPhone app (feature
//! `phone.files`). It is the desktop's file tree with three more limits, because a phone token
//! is the narrowest one Hub hands out: the folder must be a project Hub knows (`api::phone_root`),
//! a path may not leave the project even through a link, and what looks like a secret (keys,
//! `.env`, credentials, `.git`) is neither listed nor opened.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::project::{FileText, TreeEntry};

/// The largest picture or PDF sent to a phone.
const MAX_RAW: u64 = 20 * 1024 * 1024;
/// How many names a search returns.
const MAX_FOUND: usize = 40;

/// Whether a file or folder name looks like a secret.
fn secret_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if n == ".git" || n == ".ssh" || n == ".aws" || n == ".gnupg" || n == ".kube" || n == ".docker" {
        return true;
    }
    if n.starts_with(".env") {
        return !(n.ends_with(".example") || n.ends_with(".sample") || n.ends_with(".template"));
    }
    if n.starts_with("id_") || n.starts_with("secrets") || n.contains("credentials") || n.starts_with("service-account") {
        return !n.ends_with(".pub");
    }
    // Where CLIs keep their logins (`gh` in `.config/gh/hosts.yml`, the engines' own folders,
    // Windows' `AppData`) and Hub's own database and settings.
    if matches!(n.as_str(), ".config" | "appdata" | ".git-credentials" | "hub.db" | "hub.json")
        || crate::engine_home::LOGIN_FOLDERS.contains(&n.as_str())
    {
        return true;
    }
    matches!(n.as_str(), ".npmrc" | ".netrc" | ".pypirc" | "token" | "tokens.json" | "auth.json")
        || [".pem", ".key", ".p12", ".pfx", ".keystore", ".jks", ".kdbx", ".ovpn"].iter().any(|e| n.ends_with(e))
}

/// Whether a project folder is too wide to show a phone: the filesystem's root, the home folder or
/// a folder above it, one that holds Hub's own data folder or lies inside it (but for the dev
/// checkout and the Home desk), or one inside a folder of logins (`~/.config`, `~/.ssh`, the
/// engines' folders). A phone token can add any folder as a project (`POST /api/projects`), so
/// without this it could add one of those and read the logins the names in `secret` guard.
/// A folder below home is fine (`~/code/x`) unless a folder of logins is on the way down from
/// home (`~/AppData/...`, so on Windows `%TEMP%` too); the names of the folders home itself lies in
/// are not judged. Inside Hub's data folder only the part below it is judged: on Windows that
/// folder is itself in `AppData`, and the dev checkout and the Home desk there are allowed.
pub fn too_wide(root: &Path, data_dir: &Path) -> bool {
    too_wide_under(root, data_dir, crate::folders::home_dir().as_deref())
}

/// `too_wide` with the home folder given, so a test can lay out a home of its own.
pub(crate) fn too_wide_under(root: &Path, data_dir: &Path, home: Option<&Path>) -> bool {
    let real = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let (root, data, home) = (real(root), real(data_dir), home.map(real));
    if root.parent().is_none() {
        return true;
    }
    if home.as_ref().is_some_and(|h| h.starts_with(&root)) || data.starts_with(&root) {
        return true;
    }
    let judged = if let Ok(inside) = root.strip_prefix(&data) {
        let first = inside.components().next().map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase());
        if !matches!(first.as_deref(), Some("dev-core" | "desk" | "platschef")) {
            return true;
        }
        inside
    } else {
        home.as_ref().and_then(|h| root.strip_prefix(h).ok()).unwrap_or(root.as_path())
    };
    judged.components().any(|c| secret_name(&c.as_os_str().to_string_lossy()))
}

/// Whether a project-relative path passes through a secret.
pub fn secret(rel: &str) -> bool {
    rel.split(['/', '\\']).any(secret_name)
}

/// The real path of `rel` inside `root`, refusing secrets and anything that ends up outside the
/// project, links included.
fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    if secret(rel) {
        bail!("{rel} is not shown on a phone");
    }
    let wanted = crate::project_folder::inside(root, rel)?;
    let real = wanted.canonicalize().with_context(|| format!("reading {rel}"))?;
    let base = root.canonicalize().context("the project folder")?;
    if !real.starts_with(&base) {
        bail!("{rel} is outside the project");
    }
    Ok(real)
}

/// One level of a folder: folders first, no hidden files, no secrets, nothing that leads out.
pub fn tree(root: &Path, dir: &str) -> Result<Vec<TreeEntry>> {
    if !dir.is_empty() {
        resolve(root, dir)?;
    }
    let base = root.canonicalize().context("the project folder")?;
    let mut entries = crate::project::tree(root, dir)?;
    entries.retain(|e| !secret(&e.rel) && root.join(&e.rel).canonicalize().is_ok_and(|p| p.starts_with(&base)));
    Ok(entries)
}

/// A text file, cut when long.
pub fn file(root: &Path, rel: &str) -> Result<FileText> {
    let path = resolve(root, rel)?;
    crate::project::text_of(&path, rel)
}

/// A picture or a PDF as it is, small enough for a phone.
pub fn raw(root: &Path, rel: &str) -> Result<(&'static str, Vec<u8>)> {
    let mime = crate::project::raw_mime(rel).filter(|m| m.starts_with("image/") || *m == "application/pdf");
    let mime = mime.with_context(|| format!("{rel} is not a picture or a PDF"))?;
    let path = resolve(root, rel)?;
    let size = std::fs::metadata(&path).with_context(|| format!("reading {rel}"))?.len();
    if size > MAX_RAW {
        bail!("{rel} is {} MB, too big for a phone", size / (1024 * 1024));
    }
    Ok((mime, std::fs::read(&path).with_context(|| format!("reading {rel}"))?))
}

/// A picture a thread's agent named in its answer by its path (`gw.png`, `D:\\Dev\\chats\\…\\gw.png`),
/// as it is, for the phone to show under the answer. Only pictures, and only a file that really lies
/// inside the thread's own folder once links and `..` are resolved, so a path in a reply can never
/// reach anything else on the machine. SVG is left out: it can carry scripts.
pub fn thread_picture(cwd: &Path, path: &str) -> Result<(&'static str, Vec<u8>)> {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        _ => bail!("only pictures are shown"),
    };
    let asked = Path::new(path.trim());
    let file = if asked.is_absolute() { asked.to_path_buf() } else { cwd.join(asked) };
    let (file, base) = (file.canonicalize().context("no such picture")?, cwd.canonicalize().context("the thread's folder")?);
    if !file.starts_with(&base) {
        bail!("that picture is not in the thread's folder");
    }
    let size = std::fs::metadata(&file).context("reading the picture")?.len();
    if size > MAX_RAW {
        bail!("the picture is {} MB, too big for a phone", size / (1024 * 1024));
    }
    Ok((mime, std::fs::read(&file).context("reading the picture")?))
}

/// An HTML file a thread's agent named in its answer, as a path relative to the thread's folder, for
/// the phone to run as a live page (`pages::save_file`). Only `.html` and `.htm`, and only a file that
/// lies inside the thread's own folder once links and `..` are resolved.
pub fn thread_html(cwd: &Path, path: &str) -> Result<String> {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    if ext != "html" && ext != "htm" {
        bail!("only HTML files are shown as pages");
    }
    let asked = Path::new(path.trim());
    let file = if asked.is_absolute() { asked.to_path_buf() } else { cwd.join(asked) };
    let (file, base) = (file.canonicalize().context("no such file")?, cwd.canonicalize().context("the thread's folder")?);
    let rel = file.strip_prefix(&base).map_err(|_| anyhow::anyhow!("that file is not in the thread's folder"))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

/// Files whose path holds `query`, best (shortest path) first.
pub fn search(root: &Path, query: &str) -> Result<Vec<TreeEntry>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let base = root.canonicalize().context("the project folder")?;
    let walk = ignore::WalkBuilder::new(root).hidden(true).max_depth(Some(10)).follow_links(false).build();
    let mut found: Vec<TreeEntry> = Vec::new();
    for e in walk.flatten().skip(1) {
        let Ok(rel) = e.path().strip_prefix(&base).or_else(|_| e.path().strip_prefix(root)) else { continue };
        let rel = rel.to_string_lossy().replace('\\', "/");
        if secret(&rel) || !rel.to_lowercase().contains(&query) {
            continue;
        }
        found.push(TreeEntry {
            name: e.file_name().to_string_lossy().into_owned(),
            rel,
            dir: e.file_type().is_some_and(|t| t.is_dir()),
        });
    }
    found.sort_by(|a, b| a.rel.len().cmp(&b.rel.len()).then_with(|| a.rel.cmp(&b.rel)));
    found.truncate(MAX_FOUND);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::write(p.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(p.join("README.md"), "# Hi").unwrap();
        std::fs::write(p.join(".env"), "KEY=1").unwrap();
        std::fs::write(p.join("server.pem"), "-----BEGIN").unwrap();
        std::fs::write(p.join("logo.png"), [137, 80, 78, 71]).unwrap();
        std::fs::write(p.join("clip.mp4"), [0, 0]).unwrap();
        dir
    }

    #[test]
    fn keys_and_env_files_are_secrets_but_examples_are_not() {
        for s in [".env", ".env.local", "a/.git/config", "id_ed25519", "x/server.pem", "credentials.json", ".ssh/known_hosts", "Secrets.toml"] {
            assert!(secret(s), "{s}");
        }
        for s in [".env.example", "src/main.rs", "id_rsa.pub", "README.md", "docs/keys.md"] {
            assert!(!secret(s), "{s}");
        }
    }

    #[test]
    fn logins_of_clis_and_hubs_own_files_are_secrets() {
        for s in [".config/gh/hosts.yml", "x/.credentials.json", ".git-credentials", "AppData/Local/hub/hub.db", "hub/hub.json"] {
            assert!(secret(s), "{s}");
        }
        for folder in crate::engine_home::LOGIN_FOLDERS {
            assert!(secret(&format!("{folder}/auth.json")) && secret(folder), "{folder}");
        }
    }

    #[test]
    fn the_root_the_home_folder_and_hubs_data_are_too_wide_for_a_phone() {
        // A home of the test's own, laid out as on Windows: Hub's data in `AppData/Local/hub`.
        // The machine's own home and temp folder are left out: on Windows the temp folder is in
        // `AppData`, a folder of logins, so a project there is rightly too wide.
        let home = tempfile::tempdir().unwrap();
        let home = home.path();
        let data = home.join("AppData").join("Local").join("hub");
        let dir = home.join("code").join("app");
        for d in [&data, &dir] {
            std::fs::create_dir_all(d).unwrap();
        }
        let wide = |root: &Path| too_wide_under(root, &data, Some(home));
        assert!(!wide(&dir), "a project below home");
        // The home folder, a folder above it, the root, and a project that holds Hub's data folder.
        assert!(wide(home) && wide(home.parent().unwrap()) && wide(Path::new("/")));
        assert!(wide(&home.join("AppData")) && wide(data.parent().unwrap()));
        // Inside Hub's data or a folder of logins, the temp folder in AppData too.
        let engines = data.join("engines");
        let config = dir.join(".config");
        let temp = home.join("AppData").join("Local").join("Temp").join("x");
        for d in [&engines, &config.join("gh"), &temp] {
            std::fs::create_dir_all(d).unwrap();
        }
        assert!(wide(&engines) && wide(&temp));
        assert!(wide(&config) && wide(&config.join("gh")));
        // The dev checkout and the Home desk in Hub's data are allowed, though the data folder is in AppData.
        for name in ["dev-core", "desk"] {
            std::fs::create_dir_all(data.join(name).join("src")).unwrap();
            assert!(!wide(&data.join(name)) && !wide(&data.join(name).join("src")), "{name}");
        }
        assert!(wide(&data.join("dev-core").join(".ssh")));
    }

    #[test]
    fn the_tree_lists_files_without_secrets() {
        let dir = project();
        let names: Vec<String> = tree(dir.path(), "").unwrap().into_iter().map(|e| e.rel).collect();
        assert!(names.contains(&"src".to_owned()) && names.contains(&"README.md".to_owned()));
        assert!(!names.iter().any(|n| n.contains("pem") || n.contains(".env")));
        assert_eq!(tree(dir.path(), "src").unwrap()[0].rel, "src/main.rs");
    }

    #[test]
    fn a_secret_or_a_path_outside_is_refused() {
        let dir = project();
        assert!(file(dir.path(), ".env").is_err());
        assert!(file(dir.path(), "server.pem").is_err());
        assert!(file(dir.path(), "../x").is_err());
        assert!(file(dir.path(), "/etc/passwd").is_err());
        assert_eq!(file(dir.path(), "README.md").unwrap().text, "# Hi");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_project_is_refused_and_not_listed() {
        let dir = project();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("private.txt"), "no").unwrap();
        std::os::unix::fs::symlink(outside.path().join("private.txt"), dir.path().join("link.txt")).unwrap();
        assert!(file(dir.path(), "link.txt").is_err());
        assert!(!tree(dir.path(), "").unwrap().iter().any(|e| e.rel == "link.txt"));
    }

    #[test]
    fn only_pictures_and_pdfs_come_raw() {
        let dir = project();
        assert_eq!(raw(dir.path(), "logo.png").unwrap().0, "image/png");
        assert!(raw(dir.path(), "clip.mp4").is_err());
        assert!(raw(dir.path(), "README.md").is_err());
    }

    #[test]
    fn search_finds_by_path_and_leaves_secrets_out() {
        let dir = project();
        let found: Vec<String> = search(dir.path(), "main").unwrap().into_iter().map(|e| e.rel).collect();
        assert_eq!(found, vec!["src/main.rs"]);
        assert!(search(dir.path(), "pem").unwrap().is_empty());
        assert!(search(dir.path(), "  ").unwrap().is_empty());
    }

    #[test]
    fn a_picture_named_in_an_answer_is_served_only_from_the_threads_own_folder() {
        let dir = project();
        let root = dir.path();
        std::fs::create_dir_all(root.join("out")).unwrap();
        std::fs::write(root.join("out/gw.png"), [137, 80, 78, 71]).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.png"), [137, 80, 78, 71]).unwrap();
        // As the agent wrote it: relative, or absolute.
        assert_eq!(thread_picture(root, "out/gw.png").unwrap().0, "image/png");
        assert_eq!(thread_picture(root, &root.join("out/gw.png").to_string_lossy()).unwrap().1, vec![137, 80, 78, 71]);
        // Nothing but pictures, nothing outside the folder, however it is spelled.
        assert!(thread_picture(root, "README.md").is_err() && thread_picture(root, "logo.svg").is_err());
        assert!(thread_picture(root, &outside.path().join("secret.png").to_string_lossy()).is_err());
        assert!(thread_picture(root, &format!("../{}/secret.png", outside.path().file_name().unwrap().to_string_lossy())).is_err());
        assert!(thread_picture(root, "out/missing.png").is_err());
    }

    #[test]
    fn an_html_file_named_in_an_answer_is_run_only_from_the_threads_own_folder() {
        let dir = project();
        let root = dir.path();
        std::fs::create_dir_all(root.join("out")).unwrap();
        std::fs::write(root.join("out/chart.html"), "<p>hi</p>").unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.html"), "x").unwrap();
        assert_eq!(thread_html(root, "out/chart.html").unwrap(), "out/chart.html");
        assert_eq!(thread_html(root, &root.join("out/chart.html").to_string_lossy()).unwrap(), "out/chart.html");
        assert!(thread_html(root, "README.md").is_err());
        assert!(thread_html(root, &outside.path().join("secret.html").to_string_lossy()).is_err());
        assert!(thread_html(root, &format!("../{}/secret.html", outside.path().file_name().unwrap().to_string_lossy())).is_err());
        assert!(thread_html(root, "out/missing.html").is_err());
    }
}
