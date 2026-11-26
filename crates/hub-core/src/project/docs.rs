//! The living docs of `project.rs`: README, agent instructions and `docs/`.

use super::*;

/// README, agent instructions and the markdown in `docs/`, newest first. The plan is shown on its
/// own, so it is left out.
pub(super) fn docs(root: &Path) -> Vec<DocFile> {
    let mut paths: Vec<PathBuf> = ["README.md", "CLAUDE.md", "AGENTS.md"]
        .iter()
        .map(|f| root.join(f))
        .filter(|p| p.is_file())
        .collect();
    if let Ok(entries) = std::fs::read_dir(root.join("docs")) {
        paths.extend(entries.flatten().map(|e| e.path()).filter(|p| {
            p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md"))
        }));
    }
    let mut docs: Vec<DocFile> = paths
        .into_iter()
        .filter_map(|path| {
            let rel = path.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/");
            if rel.eq_ignore_ascii_case(PLAN) {
                return None;
            }
            let modified = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or_default();
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            // README and agent files often share the project's name as heading; their name says more.
            let title = if !rel.contains('/') { rel.trim_end_matches(".md").to_owned() } else { text
                .lines()
                .find_map(|l| l.strip_prefix("# "))
                .map(|t| t.trim().to_owned())
                .unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned()) };
            Some(DocFile { rel, title, preview: preview(&text, 220), modified })
        })
        .collect();
    docs.sort_by_key(|d| std::cmp::Reverse(d.modified));
    docs
}
