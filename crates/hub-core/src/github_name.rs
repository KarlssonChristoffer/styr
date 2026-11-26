//! A GitHub repository's `owner/name`: checking one, reading one from a link to the repository or
//! anything in it, and the kind Customs files a whole repository under. Plain text work that
//! names no other module, so the plugin finder (`plugins.rs`) and Customs (`customs_known.rs`,
//! `customs_review.rs` and its `repo.rs`) share it without naming each other.

use anyhow::{Result, bail};

/// Customs' kind for a repository brought in whole.
pub const CUSTOMS_KIND: &str = "github-repo";

/// `owner/name` with only the characters GitHub allows, or an error.
pub fn check_name(full: &str) -> Result<&str> {
    let full = full.trim().trim_matches('/');
    let ok_part = |p: &str| !p.is_empty() && p != "." && p != ".." && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    match full.split_once('/') {
        Some((o, n)) if ok_part(o) && ok_part(n) => Ok(full),
        _ => bail!("not a GitHub repository: {full}"),
    }
}

/// `owner/name` from `owner/name` or a GitHub link to the repository or anything in it.
pub fn repo_from_link(text: &str) -> Option<String> {
    let t = text.trim().trim_end_matches(".git");
    let path = t.strip_prefix("https://github.com/").or_else(|| t.strip_prefix("http://github.com/")).or_else(|| t.strip_prefix("github.com/")).unwrap_or(t);
    let mut parts = path.split(['/', '?', '#']).filter(|p| !p.is_empty());
    let full = format!("{}/{}", parts.next()?, parts.next()?.trim_end_matches(".git"));
    check_name(&full).ok().map(str::to_owned)
}
