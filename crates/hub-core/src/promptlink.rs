//! Prompt links, after T3 Code's: `http://127.0.0.1:7878/new?prompt=…` opens Hub with a draft
//! ready to read and send. A link never runs anything and changes no setting; it only puts text in
//! the message box, so it needs no token and works from a note, a pull request comment or a
//! script on this machine. Characters you cannot see are taken out first, since they can hide
//! instructions for the agent, and the app says so when there were any.
//!
//! The axum handler that publishes the draft lives in `api/projects.rs`; this file only reads the
//! request and words the page, so it needs neither the core nor the stream.

use axum::http::HeaderMap;
use serde::Deserialize;

use crate::model::PromptLink;

/// The path a link opens.
pub const PATH: &str = "/new";
/// Longer prompts are cut here, as T3 Code does.
const LONGEST: usize = 8000;

#[derive(Deserialize)]
pub struct LinkQuery {
    #[serde(default)]
    prompt: String,
    /// A project folder on this machine.
    folder: Option<String>,
    /// A GitHub repository, `owner/name`; the project whose folder is called `name` gets it.
    repo: Option<String>,
}

/// Zero-width, direction and tag characters: invisible on screen, read by a model.
fn hidden(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{E0000}'..='\u{E007F}')
}

/// The prompt as it may be shown, and whether anything was taken out.
pub fn clean(prompt: &str) -> (String, bool) {
    let kept: String = prompt.chars().filter(|c| !hidden(*c)).take(LONGEST).collect();
    let stripped = prompt.chars().any(hidden);
    (kept, stripped)
}

/// Whether a browser request is a link being opened (a tab or window), and not a web page
/// loading the address quietly as an image, a fetch or a frame to plant a draft that reads as the
/// user's own. Browsers say so in `Sec-Fetch-Mode` and `Sec-Fetch-Dest`; a script or `curl`
/// sends neither and is let through.
pub fn is_navigation(headers: &HeaderMap) -> bool {
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim);
    get("sec-fetch-mode").is_none_or(|m| m == "navigate") && get("sec-fetch-dest").is_none_or(|d| d == "document")
}

/// The draft a request asks for, or why it is refused (a page loading the address quietly).
pub fn draft(headers: &HeaderMap, q: LinkQuery) -> Result<PromptLink, &'static str> {
    if !is_navigation(headers) {
        return Err("a prompt link is opened as a page, not loaded by one");
    }
    let (prompt, stripped) = clean(&q.prompt);
    Ok(PromptLink { prompt, folder: q.folder.filter(|f| !f.trim().is_empty()), repo: q.repo.filter(|r| !r.trim().is_empty()), stripped })
}

/// The page the browser tab shows once the draft is out; `seen` is whether a window was open to take it.
pub fn page(seen: bool) -> String {
    let said = if seen {
        "The prompt is in Hub's message box, ready to read and send. You can close this tab."
    } else {
        "Hub is not open, so the prompt went nowhere. Open Hub and click the link again."
    };
    format!(
        "<!doctype html><meta charset=utf-8><title>Hub</title><body style=\"font:15px system-ui;background:#1a1918;color:#ece8e2;display:grid;place-items:center;height:90vh\"><p>{said}</p><script>setTimeout(()=>window.close(),1500)</script>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_out_what_cannot_be_seen() {
        assert_eq!(clean("fix the build"), ("fix the build".into(), false));
        let (text, stripped) = clean("fix\u{200B} it\u{E0041}\u{E0042}");
        assert_eq!(text, "fix it");
        assert!(stripped);
        assert_eq!(clean(&"a".repeat(9000)).0.len(), LONGEST);
    }

    #[test]
    fn only_a_link_opened_as_a_page_counts() {
        let with = |pairs: &[(&str, &str)]| {
            let mut h = HeaderMap::new();
            for (k, v) in pairs {
                h.insert(axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(), v.parse().unwrap());
            }
            h
        };
        assert!(is_navigation(&with(&[])));
        assert!(is_navigation(&with(&[("sec-fetch-mode", "navigate"), ("sec-fetch-dest", "document")])));
        assert!(!is_navigation(&with(&[("sec-fetch-mode", "no-cors"), ("sec-fetch-dest", "image")])));
        assert!(!is_navigation(&with(&[("sec-fetch-mode", "cors"), ("sec-fetch-dest", "empty")])));
        assert!(!is_navigation(&with(&[("sec-fetch-mode", "navigate"), ("sec-fetch-dest", "iframe")])));
    }
}
