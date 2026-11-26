//! One repository's page on the Plugins page, read from what GitHub returns: its README without
//! pictures, what the top-level files say it is, the npm scripts that run on install, and Hub's
//! own first-look checks of it (`checks_of`). Pure; the fetching is `plugins.rs`.

use super::*;

/// Markdown without pictures: the app cannot show outside pictures, and a broken one is noise.
pub fn without_pictures(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut rest = md;
    loop {
        let img = rest.find("![");
        let tag = rest.to_lowercase().find("<img");
        let next = match (img, tag) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => break,
        };
        out.push_str(&rest[..next]);
        let tail = &rest[next..];
        let end = if tail.starts_with("![") {
            // ![alt](url) or ![alt][ref]; an image inside a link leaves the link's brackets.
            tail.find(']').and_then(|close| {
                let after = &tail[close + 1..];
                match after.chars().next() {
                    Some('(') => after.find(')').map(|p| close + 1 + p + 1),
                    Some('[') => after.find(']').map(|p| close + 1 + p + 1),
                    _ => Some(close + 1),
                }
            })
        } else {
            tail.find('>').map(|p| p + 1)
        };
        match end {
            Some(end) => rest = &tail[end..],
            None => {
                out.push_str(tail);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    // Links left empty by a picture inside them.
    out.replace("[](", "[link](")
}

/// What the files at the top of the repository say it is.
pub fn inside_of(files: &[String], package: Option<&str>, pyproject: Option<&str>) -> Vec<String> {
    let has = |n: &str| files.iter().any(|f| f.eq_ignore_ascii_case(n));
    let mut out = Vec::new();
    for (folder, what) in crate::connections::MADE_FOR.plugin_folders {
        if has(folder) {
            out.push((*what).into());
        }
    }
    if has("SKILL.md") {
        out.push("A skill (SKILL.md at the top)".into());
    }
    if has("skills") {
        out.push("A skills folder".into());
    }
    if has(".mcp.json") {
        out.push("MCP settings (.mcp.json)".into());
    }
    if let Some(p) = package {
        if p.contains("@modelcontextprotocol/sdk") {
            out.push("An MCP server in Node (package.json uses the MCP SDK)".into());
        } else {
            out.push("A Node package (package.json)".into());
        }
        if p.contains("\"bin\"") {
            out.push("Gives a command line program (bin in package.json)".into());
        }
    }
    if let Some(p) = pyproject {
        let lower = p.to_lowercase();
        if lower.contains("\"mcp") || lower.contains("fastmcp") || lower.contains("'mcp") {
            out.push("An MCP server in Python (pyproject uses mcp)".into());
        } else {
            out.push("A Python package (pyproject.toml)".into());
        }
        if lower.contains("[project.scripts]") {
            out.push("Gives a command line program ([project.scripts])".into());
        }
    }
    if has("Cargo.toml") {
        out.push("A Rust program (Cargo.toml)".into());
    }
    if has("go.mod") {
        out.push("A Go program (go.mod)".into());
    }
    if has("Dockerfile") {
        out.push("Can run in Docker (Dockerfile)".into());
    }
    out
}

/// Scripts that npm runs by itself when the package is installed.
pub fn install_scripts(package: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(package) else { return Vec::new() };
    ["preinstall", "install", "postinstall", "prepare"]
        .iter()
        .filter(|k| v["scripts"][**k].is_string())
        .map(|k| (*k).to_owned())
        .collect()
}

/// Hub's own first look, from what GitHub says.
pub fn checks_of(repo: &PluginRepo, owner_since: Option<i64>, scripts: &[String], now: i64) -> Vec<PluginCheck> {
    let ok = |t: String| PluginCheck { level: "ok".into(), text: t };
    let warn = |t: String| PluginCheck { level: "warn".into(), text: t };
    let mut out = Vec::new();
    let age = (now - repo.created_at) / DAY;
    out.push(if age >= 30 { ok(format!("{} old", span(age))) } else { warn(format!("Only {} old", span(age))) });
    out.push(match &repo.license {
        Some(l) => ok(format!("Licence: {l}")),
        None => warn("No licence: using it may not be allowed".into()),
    });
    let idle = (now - repo.pushed_at) / DAY;
    out.push(if repo.archived {
        warn("Archived: nobody works on it any more".into())
    } else if idle <= 90 {
        ok(format!("Worked on {}", if idle == 0 { "today".into() } else { format!("{} ago", span(idle)) }))
    } else {
        warn(format!("Not worked on for {}", span(idle)))
    });
    if let Some(since) = owner_since {
        let years = (now - since) / (365 * DAY);
        out.push(if years >= 1 {
            ok(format!("Owner on GitHub for {years} {}", if years == 1 { "year" } else { "years" }))
        } else {
            warn("Owner's account is less than a year old".into())
        });
    }
    if !scripts.is_empty() {
        out.push(warn(format!("Runs {} when installed with npm", scripts.join(", "))));
    }
    out
}
