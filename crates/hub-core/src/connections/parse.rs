//! Reading what each engine's CLI and files say its connections are: `claude mcp list` and Claude
//! Code's own settings and `.mcp.json` files (also where each server comes from), `codex mcp list
//! --json` and, without the CLI, the `[mcp_servers.*]` tables of Codex's `config.toml`.

use super::*;

/// Reads `claude mcp list`: lines such as
/// `claude.ai ElevenLabs: https://mcp.elevenlabs.io/mcp (HTTP) - ✓ Connected` or
/// `playwright: npx @playwright/mcp@latest - ✗ Failed to connect`.
pub(super) fn parse_claude_list(text: &str, engine: &str) -> Vec<Connection> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((head, status)) = line.rsplit_once(" - ") else { continue };
        let Some((name, target)) = head.split_once(": ").or_else(|| head.strip_suffix(':').map(|n| (n, ""))) else { continue };
        let name = name.trim();
        if name.is_empty() || name.contains("  ") {
            continue;
        }
        let mut c = blank(engine, name);
        let mut target = target.trim();
        for (suffix, transport) in [("(HTTP)", "http"), ("(SSE)", "sse"), ("(stdio)", "stdio")] {
            if let Some(t) = target.strip_suffix(suffix) {
                target = t.trim();
                c.transport = Some(transport.into());
            }
        }
        if c.transport.is_none() && !target.is_empty() {
            c.transport = Some(if target.starts_with("http") { "http" } else { "stdio" }.into());
        }
        c.target = safe_target(target);
        let s = status.to_lowercase();
        c.state = if s.contains("needs authentication") || s.contains("needs auth") {
            ConnectionState::NeedsLogin
        } else if s.contains("failed") || s.contains("error") {
            ConnectionState::Failed
        } else if s.contains("not configured") || s.contains("disabled") {
            ConnectionState::Off
        } else if s.contains("connected") {
            ConnectionState::Connected
        } else {
            ConnectionState::Unknown
        };
        if c.state == ConnectionState::Failed
            && let Some((_, why)) = status.split_once('—').or_else(|| status.split_once(" - "))
        {
            c.detail = Some(why.trim().to_owned());
        }
        out.push(c);
    }
    out
}

/// Claude Code's own files: servers from your settings, this folder's and the project's
/// `.mcp.json`, and the claude.ai connectors it has connected before. Also says where each
/// listed server comes from, which `claude mcp list` does not.
pub(super) fn claude_files(home: &Path, cwd: &Path) -> BTreeMap<String, ConnectionSource> {
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
    let global = dir
        .as_ref()
        .and_then(|d| crate::prefs::read_json(&d.join(".claude.json")))
        .or_else(|| crate::prefs::read_json(&home.join(".claude.json")));
    let mut out = BTreeMap::new();
    let names = |v: Option<&Value>| v.and_then(Value::as_object).map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
    if let Some(g) = &global {
        for n in names(g.get("mcpServers")) {
            out.insert(n, ConnectionSource::User);
        }
        // Folders are keyed with forward slashes, even on Windows.
        let here = cwd.to_string_lossy().replace('\\', "/");
        if let Some(projects) = g.get("projects").and_then(Value::as_object) {
            for (path, p) in projects {
                if path.replace('\\', "/").eq_ignore_ascii_case(&here) {
                    for n in names(p.get("mcpServers")) {
                        out.insert(n, ConnectionSource::Local);
                    }
                }
            }
        }
        for n in g.get("claudeAiMcpEverConnected").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
            out.entry(n.to_owned()).or_insert(ConnectionSource::ClaudeAi);
        }
    }
    if let Some(project) = crate::prefs::read_json(&cwd.join(".mcp.json")) {
        for n in names(project.get("mcpServers")) {
            out.insert(n, ConnectionSource::Project);
        }
    }
    out
}

/// Reads `codex mcp list --json`, an array of servers with their transport.
pub(super) fn parse_codex_json(raw: &Value, engine: &str) -> Vec<Connection> {
    let mut out = Vec::new();
    for s in raw.as_array().into_iter().flatten() {
        let Some(name) = s.get("name").and_then(Value::as_str) else { continue };
        let mut c = blank(engine, name);
        let t = s.get("transport").unwrap_or(s);
        if let Some(url) = t.get("url").and_then(Value::as_str) {
            c.transport = Some("http".into());
            c.target = safe_target(url);
        } else if let Some(cmd) = t.get("command").and_then(Value::as_str) {
            c.transport = Some("stdio".into());
            let args: Vec<&str> = t.get("args").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
            c.target = safe_target(&format!("{cmd} {}", args.join(" ")));
        }
        let auth = s.get("auth_status").and_then(Value::as_str).unwrap_or_default().to_lowercase();
        c.state = if s.get("enabled").and_then(Value::as_bool) == Some(false) {
            ConnectionState::Off
        } else if auth.contains("not_logged_in") || auth.contains("not logged in") {
            ConnectionState::NeedsLogin
        } else {
            ConnectionState::Unknown
        };
        out.push(c);
    }
    out
}

/// The `[mcp_servers.<name>]` tables of Codex's `config.toml`, read by line: the name, and the
/// command or URL and whether it is switched off.
pub(super) fn parse_codex_toml(text: &str, engine: &str) -> Vec<Connection> {
    let mut out: Vec<Connection> = Vec::new();
    let mut current = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            current = false;
            let inner = line.trim_matches(|c| c == '[' || c == ']').trim();
            if let Some(name) = inner.strip_prefix("mcp_servers.") {
                let name = name.trim_matches('"');
                // Sub-tables such as `[mcp_servers.x.env]` belong to the server above.
                if !name.contains('.') {
                    out.push(blank(engine, name));
                    current = true;
                }
            }
            continue;
        }
        let (Some(c), true) = (out.last_mut(), current) else { continue };
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim().trim_matches('"');
        match k.trim() {
            "url" => {
                c.transport = Some("http".into());
                c.target = safe_target(v);
            }
            "command" => {
                c.transport = Some("stdio".into());
                c.target = safe_target(v);
            }
            "enabled" if v == "false" => c.state = ConnectionState::Off,
            _ => {}
        }
    }
    out
}
