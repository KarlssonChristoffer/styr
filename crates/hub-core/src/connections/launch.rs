//! How an engine is started so the connections switched off stay off (`Launch`: arguments,
//! environment, session `_meta` for Claude Code; `-c` overrides for Codex), with extra folders and
//! the lean start of an everyday chat. At the bottom, the engines' wire names Hub reads elsewhere:
//! the tool name of a call, a turn's model, and the command that adds Hub's tools to a CLI.

use super::*;

/// What an engine is started with so that what is switched off stays off in Hub's threads:
/// more arguments, more environment, and `_meta` for its sessions.
#[derive(Debug, Default, PartialEq)]
pub struct Launch {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub session_meta: Option<serde_json::Map<String, Value>>,
}

/// Claude Code (through claude-agent-acp) takes tools to leave out and settings for each
/// session in `_meta.claudeCode.options`, and an environment variable that drops every claude.ai
/// connector. Codex (codex-acp) takes `-c key=value` overrides of its config.toml.
pub fn launch(prefs: &ConnectionPrefs, engine: &str, command: &str) -> Launch {
    let off = prefs.off.get(engine).cloned().unwrap_or_default();
    let account_off = prefs.account_off.iter().any(|e| e == engine);
    let plugins_off = prefs.plugins_off.get(engine).cloned().unwrap_or_default();
    let mut out = Launch::default();
    match kind(command) {
        Kind::Claude => {
            if account_off {
                out.env.push(("ENABLE_CLAUDEAI_MCP_SERVERS".into(), "false".into()));
            }
            let mut options = serde_json::Map::new();
            if !off.is_empty() {
                // A rule naming only the server covers every tool it has.
                let tools: Vec<Value> = off.iter().map(|n| Value::String(format!("mcp__{}", key(n)))).collect();
                options.insert("disallowedTools".into(), Value::Array(tools));
            }
            if !plugins_off.is_empty() {
                let plugins: serde_json::Map<String, Value> = plugins_off.iter().map(|p| (p.clone(), Value::Bool(false))).collect();
                options.insert("settings".into(), serde_json::json!({ "enabledPlugins": plugins }));
            }
            if !options.is_empty() {
                let mut meta = serde_json::Map::new();
                meta.insert("claudeCode".into(), serde_json::json!({ "options": options }));
                out.session_meta = Some(meta);
            }
        }
        Kind::Codex => {
            if account_off {
                out.args.extend(["-c".into(), "features.apps=false".into()]);
            }
            // A plugin off in Hub: each of its skills off by name, so it leaves Codex's skills
            // list (and its budget) whether it came from config.toml or from the ChatGPT account,
            // and the plugin itself off for its tools.
            if !plugins_off.is_empty() {
                out.args.extend(codex_plugins_off(&plugins_off, &codex_plugins()));
            }
            // Only servers config.toml has: an override for one it lacks would be half a server.
            let known = codex_file(engine, None).connections;
            for name in off {
                let bare = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
                if bare && known.iter().any(|c| c.name == name) {
                    out.args.extend(["-c".into(), format!("mcp_servers.{name}.enabled=false")]);
                }
            }
        }
        Kind::Other => {}
    }
    out
}

/// More folders for one session. Claude Code takes them as `additionalDirectories` in its
/// options; the other agents get theirs in the ACP session request when they advertise it.
pub fn add_dirs(launch: &mut Launch, command: &str, dirs: &[PathBuf]) {
    if dirs.is_empty() || kind(command) != Kind::Claude {
        return;
    }
    let meta = launch.session_meta.get_or_insert_with(Default::default);
    let code = meta.entry("claudeCode").or_insert_with(|| Value::Object(Default::default()));
    if let Some(options) = code.as_object_mut().map(|c| c.entry("options").or_insert_with(|| Value::Object(Default::default()))).and_then(|o| o.as_object_mut()) {
        let list = dirs.iter().map(|d| Value::String(d.to_string_lossy().into_owned())).collect();
        options.insert("additionalDirectories".into(), Value::Array(list));
    }
}

pub use crate::projects_root::{is_chat_folder, is_scratch_folder};

/// What an everyday chat starts its Claude with: none of the user's plugins, skills, hooks or
/// settings files, no MCP server of theirs (neither the ones in their files nor the claude.ai
/// connectors); only the tools Claude Code has itself and Hub's own, which ACP passes in the
/// session. Claude Code connects every MCP server before it asks the model anything, and a
/// setup of 90 slash commands and a few hundred deferred tools is 44 000 tokens of context
/// before the first word; a chat of a few lines waited 8 s of 20 on that. Other engines are left
/// alone, and a dev thread keeps everything it has.
///
/// `model` is the one the user's settings.json names (`settings_model`). claude-agent-acp
/// reads that file itself and reports its model as the session's, so it goes along as the
/// session's model: else the chat said Sonnet and Claude Code, skipping the file, ran Opus.
pub fn lean(launch: &mut Launch, command: &str, model: Option<&str>) {
    if kind(command) != Kind::Claude {
        return;
    }
    if !launch.env.iter().any(|(k, _)| k == "ENABLE_CLAUDEAI_MCP_SERVERS") {
        launch.env.push(("ENABLE_CLAUDEAI_MCP_SERVERS".into(), "false".into()));
    }
    let meta = launch.session_meta.get_or_insert_with(Default::default);
    // How a chat talks, added to Claude Code's own prompt (the adapter's `systemPrompt.append`):
    // heavier than the tools' instructions, which models weigh lightly (`tool_instructions::CHAT_STYLE`).
    meta.insert("systemPrompt".into(), serde_json::json!({ "append": crate::tool_instructions::CHAT_STYLE }));
    let code = meta.entry("claudeCode").or_insert_with(|| Value::Object(Default::default()));
    if let Some(code) = code.as_object_mut() {
        let options = code.entry("options").or_insert_with(|| Value::Object(Default::default()));
        if let Some(options) = options.as_object_mut() {
            options.insert("strictMcpConfig".into(), Value::Bool(true));
            options.insert("settingSources".into(), Value::Array(Vec::new()));
            if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
                options.insert("model".into(), Value::String(model.to_owned()));
            }
        }
    }
}

/// The `model` in the user's Claude Code settings.json (`$CLAUDE_CONFIG_DIR` or `~/.claude`).
/// None under `ANTHROPIC_MODEL`, which both the adapter and Claude Code put before the file.
pub fn settings_model() -> Option<String> {
    if std::env::var_os("ANTHROPIC_MODEL").is_some() {
        return None;
    }
    let dir = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| crate::folders::home_dir().map(|h| h.join(".claude")))?;
    crate::prefs::read_json(&dir.join("settings.json"))?.get("model")?.as_str().map(str::to_owned)
}

/// The command that adds Hub's tools to the engine's own CLI as an MCP server at `url`, which
/// `token` opens (shown once when an agent key is made).
pub fn mcp_add_command(url: &str, token: &str) -> String {
    format!("claude mcp add --transport http hub {url} --header \"Authorization: Bearer {token}\"")
}

/// The name the engine itself gave a tool call, when it says one (`_meta.claudeCode.toolName`).
pub fn call_tool_name(call: &Value) -> Option<&str> {
    call["_meta"]["claudeCode"]["toolName"].as_str()
}

/// The model an engine's `usage_update` says answered the turn (`_meta._claude/model`).
pub fn update_model(update: &Value) -> Option<String> {
    update["_meta"]["_claude/model"].as_str().map(str::to_owned)
}

/// `-c` overrides that switch Codex plugins off: each of their skills by name, so they leave
/// Codex's skills list (and its budget) whether they came from config.toml or from the ChatGPT
/// account, and the plugin itself for its tools.
pub(super) fn codex_plugins_off(off: &[String], installed: &[EnginePlugin]) -> Vec<String> {
    let mut args = Vec::new();
    let mut rules = Vec::new();
    for id in off {
        if let Some(p) = installed.iter().find(|p| &p.id == id) {
            rules.extend(p.skills.iter().map(|s| format!("{{name={},enabled=false}}", toml_string(s))));
        }
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '@')) {
            args.extend(["-c".into(), format!("plugins.{id}.enabled=false")]);
        }
    }
    if !rules.is_empty() {
        args.extend(["-c".into(), format!("skills.config=[{}]", rules.join(","))]);
    }
    args
}

/// A TOML basic string.
pub(super) fn toml_string(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}
