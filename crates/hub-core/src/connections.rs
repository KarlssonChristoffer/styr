//! Which connections (MCP servers and claude.ai connectors) each engine's agents can reach, and
//! which of their tools the threads called. Claude Code is asked with `claude mcp list`, which
//! also lists the connectors on the claude.ai account it is logged in to and checks that each one
//! answers; Codex with `codex mcp list --json`. When a CLI is missing, their own files are read
//! instead. What the threads called comes from the event log, so a connection that is gone from
//! the settings but was used still shows.

use crate::locked::ResultExt;
use crate::locked::LockExt;
use crate::locked::Ignore;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;

use crate::ctx::Ctx;
use crate::model::{
    Connection, ConnectionPrefs, ConnectionSource, ConnectionState, Connections, DoctorReport, EngineConnections,
    EnginePlugin, ThreadConnections, ToolUse, now_ms,
};

/// Where claude.ai connectors are switched on and off.
pub const CLAUDE_AI_CONNECTORS: &str = "https://claude.ai/customize/connectors";

/// How a repository says it is made for an engine's agents.
pub struct MadeFor {
    /// GitHub topics, in the order they are searched.
    pub topics: &'static [&'static str],
    /// Words in a lower-cased description that say it is a skill.
    pub skill_phrases: &'static [&'static str],
    /// A file or folder at the top of the repository, and what it says the repository is.
    pub plugin_folders: &'static [(&'static str, &'static str)],
}

/// What the engines' own agents look for on GitHub (the Plugins page).
pub const MADE_FOR: MadeFor = MadeFor {
    topics: &["claude-code", "claude-skills", "codex"],
    skill_phrases: &["claude skill"],
    plugin_folders: &[(".claude-plugin", "A Claude Code plugin (.claude-plugin)")],
};

/// Asking Claude Code checks every server, which takes a while, so an answer is kept this long.
const KEEP_MS: i64 = 10 * 60 * 1000;

/// What a CLI said, by engine and folder, with when.
static CACHE: LazyLock<Mutex<HashMap<(String, PathBuf), Kept>>> = LazyLock::new(Default::default);

type Kept = (i64, Listing);

#[derive(Debug, Clone, Default)]
struct Listing {
    source: String,
    error: Option<String>,
    connections: Vec<Connection>,
    plugins: Vec<EnginePlugin>,
}

/// Which CLI an engine is, by the command that starts it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Kind {
    Claude,
    Codex,
    Other,
}

pub(crate) fn kind(command: &str) -> Kind {
    // By program and package names, not the folders they are in.
    let c = command
        .split_whitespace()
        .map(|w| w.rsplit(['/', '\\']).next().unwrap_or(w).to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    if c.contains("claude") {
        Kind::Claude
    } else if c.contains("codex") {
        Kind::Codex
    } else {
        Kind::Other
    }
}

/// A server name as it appears in a tool's name: Claude Code turns `claude.ai ElevenLabs` into
/// `claude_ai_ElevenLabs` in `mcp__claude_ai_ElevenLabs__creative_generate_image`.
pub fn key(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}


/// The readable name: `ElevenLabs` for `claude.ai ElevenLabs` or `claude_ai_ElevenLabs`, `slack`
/// for `plugin:engineering:slack`, and Hub's own server by what it is.
fn label(name: &str) -> String {
    if name == "hub" {
        return "Hub's tools".into();
    }
    if let Some(rest) = name.strip_prefix("claude.ai ").or_else(|| name.strip_prefix("claude_ai_")) {
        return rest.replace('_', " ");
    }
    if name.starts_with("plugin:") {
        return name.rsplit(':').next().unwrap_or(name).to_owned();
    }
    name.to_owned()
}

fn source_of(name: &str) -> ConnectionSource {
    if name == "hub" {
        ConnectionSource::Hub
    } else if name.starts_with("claude.ai ") || name.starts_with("claude_ai_") {
        ConnectionSource::ClaudeAi
    } else if name.starts_with("plugin:") || name.starts_with("plugin_") {
        ConnectionSource::Plugin
    } else {
        ConnectionSource::User
    }
}

fn blank(engine: &str, name: &str) -> Connection {
    let source = source_of(name);
    Connection {
        engine: engine.to_owned(),
        name: name.to_owned(),
        label: label(name),
        source,
        state: ConnectionState::Unknown,
        transport: None,
        target: None,
        detail: name.strip_prefix("plugin:").and_then(|r| r.split(':').next()).map(|p| format!("{p} plugin")),
        manage_url: (source == ConnectionSource::ClaudeAi).then(|| CLAUDE_AI_CONNECTORS.to_owned()),
        tools: Vec::new(),
        uses: 0,
        threads: 0,
        last_used: None,
        off: false,
    }
}

/// Where a server answers, without anything that could be a secret: a URL loses its query and
/// login, a command keeps its program and plain words up to the first that looks like a value.
fn safe_target(target: &str) -> Option<String> {
    let t = target.trim();
    if t.is_empty() {
        return None;
    }
    if t.starts_with("http://") || t.starts_with("https://") {
        let cut = t.find(['?', '#']).unwrap_or(t.len());
        let url = &t[..cut];
        let (scheme, rest) = url.split_once("://")?;
        let rest = rest.split_once('@').map_or(rest, |(_, r)| r);
        return Some(format!("{scheme}://{rest}"));
    }
    let mut kept = Vec::new();
    for word in t.split_whitespace().take(4) {
        let lower = word.to_lowercase();
        if word.contains('=') || ["key", "token", "secret", "password", "auth"].iter().any(|s| lower.contains(s)) {
            break;
        }
        kept.push(word);
    }
    (!kept.is_empty()).then(|| kept.join(" "))
}

/// Runs a CLI in a folder and gives its output, or why it could not.
fn run(program: &str, args: &[&str], cwd: &Path, timeout: Duration) -> Result<String, String> {
    use std::io::Read;
    use std::process::Stdio;
    // Through cmd on Windows, so .cmd shims such as claude and codex from npm are found.
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = crate::proc::command("cmd");
        c.arg("/C").arg(program).args(args).creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = crate::proc::command(program);
        c.args(args);
        c
    };
    if cwd.is_dir() {
        cmd.current_dir(cwd);
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program} could not start: {e}"))?;
    // Read while it runs, so a long answer cannot fill the pipe and stall it.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = stdout.as_mut() {
            o.read_to_string(&mut s).ignore("the output is shown as far as it could be read");
        }
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = stderr.as_mut() {
            e.read_to_string(&mut s).ignore("the output is shown as far as it could be read");
        }
        s
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                child.kill().warn_on_err("kill the timed-out program");
                return Err(format!("{program} took longer than {} seconds", timeout.as_secs()));
            }
        }
    };
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    if !status.success() {
        let err = err.trim();
        if err.contains("not recognized") || err.contains("känns inte igen") || err.contains("not found") {
            return Err(format!("{program} is not installed here"));
        }
        let first = err.lines().next().unwrap_or("").trim();
        return Err(if first.is_empty() { format!("{program} failed") } else { format!("{program}: {first}") });
    }
    Ok(out)
}

fn claude_listing(engine: &str, cwd: &Path) -> Listing {
    let home = crate::prefs::home().unwrap_or_default();
    let files = claude_files(&home, cwd);
    let (mut connections, error) = match run("claude", &["mcp", "list"], cwd, Duration::from_secs(60)) {
        Ok(text) => (parse_claude_list(&text, engine), None),
        Err(e) => (Vec::new(), Some(e)),
    };
    for c in &mut connections {
        if let Some(s) = files.get(&c.name)
            && c.source == ConnectionSource::User
        {
            c.source = *s;
        }
    }
    // Without the CLI the files are all there is; claude.ai connectors only by name.
    if error.is_some() {
        for (name, source) in &files {
            if !connections.iter().any(|c| &c.name == name) {
                let mut c = blank(engine, name);
                c.source = *source;
                connections.push(c);
            }
        }
    }
    let plugins = run("claude", &["plugin", "list", "--json"], cwd, Duration::from_secs(20))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(text.trim()).ok())
        .map(|raw| parse_claude_plugins(&raw))
        .unwrap_or_default();
    Listing { source: "claude mcp list".into(), error, connections, plugins }
}

/// Reads `claude plugin list --json`: `[{ "id": "engineering@synced", "version", "scope", "enabled" }]`.
fn parse_claude_plugins(raw: &Value) -> Vec<EnginePlugin> {
    raw.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some(EnginePlugin {
                id: p.get("id")?.as_str()?.to_owned(),
                version: p.get("version").and_then(Value::as_str).map(str::to_owned),
                scope: p.get("scope").and_then(Value::as_str).map(str::to_owned),
                enabled: p.get("enabled").and_then(Value::as_bool).unwrap_or(true),
                off: false,
                skills: Vec::new(),
            })
        })
        .collect()
}

fn codex_listing(engine: &str, cwd: &Path) -> Listing {
    match run("codex", &["mcp", "list", "--json"], cwd, Duration::from_secs(20)) {
        Ok(text) => match serde_json::from_str::<Value>(text.trim()) {
            Ok(raw) => Listing {
                source: "codex mcp list".into(),
                connections: parse_codex_json(&raw, engine),
                plugins: codex_plugins(),
                ..Default::default()
            },
            Err(e) => codex_file(engine, Some(format!("codex mcp list gave no list: {e}"))),
        },
        Err(e) => codex_file(engine, Some(e)),
    }
}

fn codex_file(engine: &str, error: Option<String>) -> Listing {
    let path = crate::engine_home::engine_home("codex").map(|h| h.join("config.toml"));
    let text = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    Listing {
        source: path.map_or("config.toml".into(), |p| p.display().to_string()),
        error,
        connections: parse_codex_toml(&text, engine),
        plugins: codex_plugins(),
    }
}

/// Codex's plugins on this machine. Codex keeps each in `plugins/cache/<marketplace>/<plugin>/
/// <version>`, whether it came from config.toml or from the ChatGPT account, and lists every skill
/// of every plugin to the model as `<plugin>:<skill>`. That list has a budget (2% of the context
/// window), so a few plugins full of skills (Figma brings 16) crowd out the rest.
fn codex_plugins() -> Vec<EnginePlugin> {
    let Some(home) = crate::engine_home::engine_home("codex") else { return Vec::new() };
    let config = std::fs::read_to_string(home.join("config.toml")).unwrap_or_default();
    read_codex_plugins(&home.join("plugins").join("cache"), &config)
}

fn read_codex_plugins(cache: &Path, config: &str) -> Vec<EnginePlugin> {
    let config: toml::Table = config.parse().unwrap_or_default();
    let settings = config.get("plugins").and_then(toml::Value::as_table);
    let mut out = Vec::new();
    for market in dirs(cache) {
        let Some(market_name) = name_of(&market) else { continue };
        for plugin in dirs(&market) {
            let Some(plugin_name) = name_of(&plugin) else { continue };
            // An unfinished install, not a plugin.
            if plugin_name.starts_with("plugin-install-") {
                continue;
            }
            // The newest version is the one Codex runs.
            let Some(version) = dirs(&plugin).into_iter().max_by_key(|v| std::fs::metadata(v).and_then(|m| m.modified()).ok())
            else {
                continue;
            };
            let mut skills: Vec<String> = dirs(&version.join("skills"))
                .iter()
                .filter(|d| d.join("SKILL.md").is_file())
                .filter_map(|d| skill_name(d))
                .map(|s| format!("{plugin_name}:{s}"))
                .collect();
            skills.sort();
            let id = format!("{plugin_name}@{market_name}");
            let enabled = settings
                .and_then(|t| t.get(&id))
                .and_then(|p| p.get("enabled"))
                .and_then(toml::Value::as_bool)
                .unwrap_or(true);
            out.push(EnginePlugin {
                version: name_of(&version),
                scope: Some(market_name.clone()),
                id,
                enabled,
                off: false,
                skills,
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn dirs(path: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !name_of(p).is_some_and(|n| n.starts_with('.')))
        .collect()
}

fn name_of(path: &Path) -> Option<String> {
    path.file_name().and_then(|n| n.to_str()).map(str::to_owned)
}

/// A skill's name from its `SKILL.md` front matter, else its folder.
fn skill_name(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("SKILL.md")).unwrap_or_default();
    let named = text
        .strip_prefix("---")
        .and_then(|rest| rest.split("\n---").next())
        .and_then(|front| front.lines().find_map(|l| l.trim().strip_prefix("name:")))
        .map(|n| n.trim().trim_matches(['"', '\'']).to_owned())
        .filter(|n| !n.is_empty());
    named.or_else(|| name_of(dir))
}

/// What an engine lists for a folder, from the cache when it is fresh.
fn listing(engine: &str, command: &str, cwd: &Path, refresh: bool) -> Listing {
    let cache_key = (engine.to_owned(), cwd.to_path_buf());
    if !refresh
        && let Some((at, l)) = CACHE.locked().get(&cache_key)
        && now_ms() - at < KEEP_MS
    {
        return l.clone();
    }
    let l = match kind(command) {
        Kind::Claude => claude_listing(engine, cwd),
        Kind::Codex => codex_listing(engine, cwd),
        Kind::Other => Listing { source: "the threads".into(), ..Default::default() },
    };
    CACHE.locked().insert(cache_key, (now_ms(), l.clone()));
    l
}


/// Adds the calls to the connections they went to, adding one for a server the engine did not
/// list. Most used first, Hub's own last.
fn merge(engine: &str, connections: &mut Vec<Connection>, calls: &[Call]) {
    let mut threads: HashMap<String, std::collections::HashSet<&str>> = HashMap::new();
    for call in calls.iter().filter(|c| c.engine == engine) {
        let k = key(&call.server);
        let i = match connections.iter().position(|c| key(&c.name) == k) {
            Some(i) => i,
            None => {
                let mut c = blank(engine, &call.server);
                if c.source != ConnectionSource::Hub && c.source != ConnectionSource::ClaudeAi {
                    c.source = ConnectionSource::Seen;
                }
                connections.push(c);
                connections.len() - 1
            }
        };
        let c = &mut connections[i];
        c.uses += 1;
        c.last_used = Some(c.last_used.unwrap_or(0).max(call.ts));
        threads.entry(c.name.clone()).or_default().insert(&call.thread_id);
        match c.tools.iter_mut().find(|t| t.name == call.tool) {
            Some(t) => {
                t.count += 1;
                t.last_used = t.last_used.max(call.ts);
            }
            None => c.tools.push(ToolUse { name: call.tool.clone(), count: 1, last_used: call.ts }),
        }
    }
    for c in connections.iter_mut() {
        c.threads = threads.get(&c.name).map_or(0, |t| t.len() as u32);
        c.tools.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
    }
    connections.sort_by(|a, b| {
        (a.source == ConnectionSource::Hub)
            .cmp(&(b.source == ConnectionSource::Hub))
            .then(b.uses.cmp(&a.uses))
            .then(a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });
}

fn hub_connection(ctx: &Ctx<'_>, engine: &str) -> Option<Connection> {
    ctx.config.agent_tools.then(|| {
        let mut c = blank(engine, "hub");
        c.state = ConnectionState::Connected;
        c.transport = Some("http".into());
        c
    })
}

/// Every engine's connections, asked from your home folder, with what all threads called.
pub async fn all(ctx: &Ctx<'_>, refresh: bool) -> Result<Connections> {
    let calls = ctx.store.mcp_calls(None).await?;
    let prefs = prefs(ctx).await?;
    let home = crate::prefs::home().unwrap_or_default();
    let mut engines = Vec::new();
    let mut tasks = Vec::new();
    for (name, e) in ctx.config.engines.iter().filter(|(n, _)| *n != crate::sim::ENGINE) {
        let (name, command, home) = (name.clone(), e.command.clone(), home.clone());
        tasks.push(tokio::task::spawn_blocking(move || (name.clone(), listing(&name, &command, &home, refresh))));
    }
    for t in tasks {
        let (engine, l) = t.await?;
        let mut connections = l.connections;
        if let Some(h) = hub_connection(ctx, &engine) {
            connections.push(h);
        }
        merge(&engine, &mut connections, &calls);
        mark_off(&prefs, &engine, &mut connections);
        let kind = ctx.config.engines.get(&engine).map_or(Kind::Other, |e| kind(&e.command));
        let off_plugins = prefs.plugins_off.get(&engine);
        let plugins = l
            .plugins
            .into_iter()
            .map(|mut p| {
                p.off = off_plugins.is_some_and(|o| o.contains(&p.id));
                p
            })
            .collect();
        engines.push(EngineConnections {
            account: account_label(kind).map(str::to_owned),
            account_off: prefs.account_off.contains(&engine),
            doctor: doctor_command(kind).map(|c| c.join(" ")),
            engine,
            source: l.source,
            checked_at: now_ms(),
            error: l.error,
            connections,
            plugins,
        });
    }
    Ok(Connections { engines })
}

/// What one thread's agent can reach from its folder, and which of those it called.
pub async fn of_thread(ctx: &Ctx<'_>, thread_id: &str, refresh: bool) -> Result<ThreadConnections> {
    let thread = ctx.store.thread(thread_id).await?.ok_or_else(|| anyhow::anyhow!("no thread {thread_id}"))?;
    let calls = ctx.store.mcp_calls(Some(thread_id.to_owned())).await?;
    let engine = thread.engine.clone();
    let command = ctx.config.engines.get(&engine).map(|e| e.command.clone()).unwrap_or_default();
    let cwd = PathBuf::from(&thread.cwd);
    let l = {
        let engine = engine.clone();
        tokio::task::spawn_blocking(move || listing(&engine, &command, &cwd, refresh)).await?
    };
    let mut connections = l.connections;
    if let Some(h) = hub_connection(ctx, &engine) {
        connections.push(h);
    }
    merge(&engine, &mut connections, &calls);
    mark_off(&prefs(ctx).await?, &engine, &mut connections);
    let (used, available) = connections.into_iter().partition(|c| c.uses > 0);
    Ok(ThreadConnections { engine, used, available, error: l.error })
}

/// Where Hub keeps what it switches off.
const PREFS: &str = "connections";

pub async fn prefs(ctx: &Ctx<'_>) -> Result<ConnectionPrefs> {
    Ok(ctx.store.get_value(PREFS).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

pub async fn set_prefs(ctx: &Ctx<'_>, mut prefs: ConnectionPrefs) -> Result<ConnectionPrefs> {
    for list in prefs.off.values_mut().chain(prefs.plugins_off.values_mut()).chain([&mut prefs.account_off]) {
        list.sort();
        list.dedup();
    }
    prefs.off.retain(|_, l| !l.is_empty());
    prefs.plugins_off.retain(|_, l| !l.is_empty());
    ctx.store.set_value(PREFS, &serde_json::to_string(&prefs)?).await?;
    Ok(prefs)
}

fn is_off(prefs: &ConnectionPrefs, engine: &str, name: &str) -> bool {
    let k = key(name);
    prefs.off.get(engine).is_some_and(|off| off.iter().any(|o| key(o) == k))
}

fn mark_off(prefs: &ConnectionPrefs, engine: &str, connections: &mut [Connection]) {
    for c in connections {
        c.off = c.source != ConnectionSource::Hub
            && (is_off(prefs, engine, &c.name)
                || (c.source == ConnectionSource::ClaudeAi && prefs.account_off.iter().any(|e| e == engine)));
    }
}

fn account_label(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Claude => Some("claude.ai connectors"),
        Kind::Codex => Some("ChatGPT apps"),
        Kind::Other => None,
    }
}

fn doctor_command(kind: Kind) -> Option<Vec<&'static str>> {
    match kind {
        Kind::Claude => Some(vec!["claude", "doctor"]),
        Kind::Codex => Some(vec!["codex", "doctor"]),
        Kind::Other => None,
    }
}

/// Runs an engine's check of its own installation.
pub fn doctor(command: &str, cwd: &Path) -> Result<DoctorReport> {
    let words = doctor_command(kind(command)).ok_or_else(|| anyhow::anyhow!("Hub knows no check for this engine"))?;
    let (program, args) = words.split_first().expect("a command");
    let (ok, output) = match run(program, args, cwd, Duration::from_secs(90)) {
        Ok(out) => (true, out),
        Err(e) => (false, e),
    };
    Ok(DoctorReport { command: words.join(" "), ok, output: output.trim().to_owned() })
}

/// Adds what `prefs` asks for to how the engine started by `command` is launched.
pub fn apply_agent_prefs(launch: &mut Launch, prefs: &crate::model::AgentPrefs, command: &str) {
    if kind(command) != Kind::Claude {
        return;
    }
    if prefs.auto_compact_off {
        launch.env.push(("DISABLE_AUTO_COMPACT".into(), "1".into()));
    } else if let Some(at) = prefs.compact_at {
        launch.env.push(("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE".into(), at.to_string()));
    }
    if let Some(style) = &prefs.output_style {
        // Beside what `connections::launch` already put in the session's settings.
        let meta = launch.session_meta.get_or_insert_with(Default::default);
        let code = meta.entry("claudeCode").or_insert_with(|| serde_json::json!({}));
        let options = &mut code["options"];
        if !options.is_object() {
            *options = serde_json::json!({});
        }
        let settings = &mut options["settings"];
        if !settings.is_object() {
            *settings = serde_json::json!({});
        }
        settings["outputStyle"] = Value::String(style.clone());
    }
}

mod parse;
use parse::*;

mod launch;
pub use launch::*;
pub use crate::model::rows::{Call, mcp_tool};

#[cfg(test)]
mod tests;
