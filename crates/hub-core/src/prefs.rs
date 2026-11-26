//! What the settings page reads: this Hub's version and files, the logins the engines use, and
//! the keys and SSH setup on this machine. Everything is read from where it already lives each
//! time; secrets are looked at to say whether they exist and are never returned.

use crate::locked::ResultExt;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::{self, Config};
use crate::model::{
    About, Accounts, Change, ConfigEdit, CoreSettings, CoreValues, KeyStatus, SshHost, SshInfo, SshKey,
    ToolInfo,
};

static RESTART: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Asks `hubd` to start a fresh copy of itself, e.g. after settings changed.
pub fn request_restart() {
    RESTART.notify_one();
}

/// Resolves when a restart was asked for.
pub async fn restart_requested() {
    RESTART.notified().await
}

fn values(cfg: &Config, version: &str) -> CoreValues {
    CoreValues {
        default_engine: cfg.default_engine.clone(),
        // The simulator is not an engine to pick.
        engines: cfg.engines.iter().filter(|(k, _)| *k != crate::sim::ENGINE).map(|(k, v)| (k.clone(), v.command.clone())).collect(),
        engine_labels: cfg.engines.iter().filter_map(|(k, v)| Some((k.clone(), crate::engines::label(k, v)?))).collect(),
        checkpoints: cfg.checkpoints,
        tidy_model: cfg.tidy.model.clone(),
        voice_language: cfg.voice.language.clone(),
        voice_mute_key: cfg.voice.mute_key.clone(),
        voice_polish: cfg.voice.polish,
        update_enabled: cfg.update.is_enabled(version),
        update_branch: cfg.update.branch.clone(),
        update_ask: cfg.update.ask,
        update_target_dir: cfg.update.target_dir.as_ref().map(|p| p.display().to_string()),
        update_interval_secs: cfg.update.interval_secs,
        update_build_mode: serde_json::to_value(cfg.update.build_mode).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default(),
        update_build_merges: cfg.update.build_merges,
        update_build_notice_at: cfg.update.build_notice_at.clone(),
        update_build_max_wait_mins: cfg.update.build_max_wait_mins,
        update_build_times: cfg.update.build_times.clone(),
        editor: cfg.open.editor.clone(),
        terminal: cfg.open.terminal.clone(),
    }
}

pub fn core_settings(running: &Config, version: &str, busy: bool) -> anyhow::Result<CoreSettings> {
    let path = config::config_path();
    let saved: Config = serde_json::from_value(config::read_file()?)?;
    let mut running = values(running, version);
    // Open commands are read on each use, so they never wait for a restart.
    let open = open_config(&saved);
    (running.editor, running.terminal) = (open.editor, open.terminal);
    Ok(CoreSettings {
        path: path.display().to_string(),
        running,
        saved: values(&saved, version),
        default_engines: Config::default().engines.into_iter().map(|(k, v)| (k, v.command)).collect(),
        busy,
    })
}

/// Writes one setting to `hub.json`, leaving everything else in the file as it was.
pub fn edit_config(edit: &ConfigEdit) -> anyhow::Result<()> {
    config::edit_file(|file| {
        apply_edit(file, edit)?;
        let parsed: Config = serde_json::from_value(file.clone()).map_err(|e| anyhow::anyhow!("not a valid value: {e}"))?;
        if !parsed.engines.contains_key(&parsed.default_engine) {
            anyhow::bail!("there is no engine called {:?}", parsed.default_engine);
        }
        if parsed.update.interval_secs < 10 {
            anyhow::bail!("check for updates at most every 10 seconds");
        }
        Ok(())
    })
}

/// How to open folders, read from `hub.json` on each use so a change applies at once.
pub fn open_config(running: &Config) -> crate::config::OpenConfig {
    config::read_file()
        .ok()
        .and_then(|v| v.get("open").cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(|| running.open.clone())
}

/// The settings page edits these keys only, so a typo cannot reach other parts of the file.
fn apply_edit(file: &mut Value, edit: &ConfigEdit) -> anyhow::Result<()> {
    let value = match &edit.value {
        Value::String(s) if s.trim().is_empty() => Value::Null,
        Value::String(s) => Value::String(s.trim().to_owned()),
        v => v.clone(),
    };
    // A list typed in one box: "16:30, 22:30".
    let value = match (edit.key.as_str(), value) {
        ("update.build_times", Value::String(s)) => {
            Value::Array(s.split([',', ' ', ';']).map(str::trim).filter(|t| !t.is_empty()).map(|t| Value::String(t.to_owned())).collect())
        }
        (_, value) => value,
    };
    let root = file.as_object_mut().ok_or_else(|| anyhow::anyhow!("hub.json is not a JSON object"))?;
    if let Some(name) = edit.key.strip_prefix("engines.") {
        // A listed `engines` replaces the built-in list, so the whole list is written.
        let mut engines: serde_json::Map<String, Value> = match root.get("engines") {
            Some(Value::Object(m)) => m.clone(),
            _ => Config::default()
                .engines
                .into_iter()
                .map(|(k, v)| (k, serde_json::json!({ "command": v.command })))
                .collect(),
        };
        match value {
            Value::Null => match Config::default().engines.get(name) {
                Some(builtin) => {
                    engines.insert(name.into(), serde_json::json!({ "command": builtin.command }));
                }
                None => {
                    engines.remove(name);
                }
            },
            // Only the command changes; what else the engine has (its key, model, provider) stays.
            Value::String(command) => match engines.get_mut(name) {
                Some(Value::Object(engine)) => {
                    engine.insert("command".into(), Value::String(command));
                }
                _ => {
                    engines.insert(name.into(), serde_json::json!({ "command": command }));
                }
            },
            _ => anyhow::bail!("an engine is a command"),
        }
        root.insert("engines".into(), Value::Object(engines));
        return Ok(());
    }
    let path: &[&str] = match edit.key.as_str() {
        "default_engine" => &["default_engine"],
        "checkpoints" => &["checkpoints"],
        "tidy.model" => &["tidy", "model"],
        "voice.language" => &["voice", "language"],
        "voice.mute_key" => &["voice", "mute_key"],
        "voice.polish" => &["voice", "polish"],
        "update.enabled" => &["update", "enabled"],
        "update.branch" => &["update", "branch"],
        "update.ask" => &["update", "ask"],
        "update.target_dir" => &["update", "target_dir"],
        "update.interval_secs" => &["update", "interval_secs"],
        "update.build_mode" => &["update", "build_mode"],
        "update.build_merges" => &["update", "build_merges"],
        "update.build_notice_at" => &["update", "build_notice_at"],
        "update.build_max_wait_mins" => &["update", "build_max_wait_mins"],
        "update.build_times" => &["update", "build_times"],
        "update.build_utc_offset_min" => &["update", "build_utc_offset_min"],
        "open.editor" => &["open", "editor"],
        "open.terminal" => &["open", "terminal"],
        other => anyhow::bail!("{other} cannot be changed from here"),
    };
    let Some((last, parents)) = path.split_last() else { anyhow::bail!("no setting named") };
    let mut obj = &mut *root;
    for key in parents {
        let entry = obj.entry(key.to_string()).or_insert_with(|| Value::Object(Default::default()));
        obj = entry.as_object_mut().ok_or_else(|| anyhow::anyhow!("{key} in hub.json is not an object"))?;
    }
    if value.is_null() {
        obj.remove(*last);
        // Leave no empty `{}` behind once the last setting in a group is back to default.
        if let [group] = parents
            && root.get(*group).and_then(Value::as_object).is_some_and(|o| o.is_empty())
        {
            root.remove(*group);
        }
    } else {
        obj.insert(last.to_string(), value);
    }
    Ok(())
}

/// The programs Hub and the engines lean on: name, label and what for. The built-in engines' own
/// programs and labels come from their harness presets (`cli`, `label`).
fn tool_list() -> Vec<(String, String, String)> {
    let row = |name: &str, label: &str, purpose: &str| (name.to_owned(), label.to_owned(), purpose.to_owned());
    let built_in = Config::default().engines;
    let engines = crate::engines::harnesses().into_iter().filter(|h| built_in.contains_key(&h.id)).filter_map(|h| {
        let mut name = h.id.clone();
        name[..1].make_ascii_uppercase();
        Some((h.cli?, h.label, format!("Logs in the {name} engine")))
    });
    let mut list = vec![
        row("git", "Git", "Branches, changes, undo and updates"),
        row("gh", "GitHub CLI", "Pull requests, CI and new repositories"),
        row("node", "Node.js", "Runs the Claude Code and Codex engines"),
        row("npm", "npm", "Builds Hub when it updates"),
    ];
    list.extend(engines);
    list.extend([
        row("cargo", "Rust", "Builds Hub's core when it updates"),
        row("code", "VS Code", "The default editor for Open in editor"),
        row("docker", "Docker", "Containers on the project page"),
        row("python", "Python", "Scripts and tools of your own"),
    ]);
    list
}

/// The programs Hub and the engines lean on, and whether this computer has them.
pub fn tools() -> Vec<ToolInfo> {
    let handles: Vec<_> = tool_list()
        .into_iter()
        .map(|(name, label, purpose)| {
            std::thread::spawn(move || {
                let out = quick(&name, &["--version"]);
                let detail = match (name.as_str(), &out) {
                    ("git", Some(_)) => {
                        let who = quick("git", &["config", "--global", "user.name"]).filter(|s| !s.is_empty());
                        let mail = quick("git", &["config", "--global", "user.email"]).filter(|s| !s.is_empty());
                        match (who, mail) {
                            (Some(w), Some(m)) => Some(format!("Commits as {w} <{m}>")),
                            (Some(w), None) => Some(format!("Commits as {w}")),
                            _ => Some("No name set for commits (git config --global user.name)".into()),
                        }
                    }
                    ("gh", Some(_)) => Some(gh_account()),
                    _ => None,
                };
                ToolInfo {
                    name,
                    label,
                    purpose,
                    found: out.is_some(),
                    version: out.as_deref().and_then(version_in),
                    detail,
                }
            })
        })
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}

fn gh_account() -> String {
    // `gh auth status` writes to stderr in older versions and stdout in newer ones.
    let Some(text) = quick_all("gh", &["auth", "status"]) else { return "Not logged in (gh auth login)".into() };
    match text.split("account ").nth(1).and_then(|r| r.split_whitespace().next()) {
        Some(account) => format!("Logged in as {account}"),
        None if text.contains("Logged in") => "Logged in".into(),
        None => "Not logged in (gh auth login)".into(),
    }
}

/// The first version-looking word, e.g. "2.47.1" from "git version 2.47.1.windows.1".
fn version_in(text: &str) -> Option<String> {
    let line = text.lines().next()?;
    line.split(|c: char| c.is_whitespace() || c == ',' || c == '(')
        .map(|w| w.trim_start_matches('v'))
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))
        .map(|w| w.split('.').take(3).collect::<Vec<_>>().join("."))
}

/// Output of a short command, or None when it is missing, fails or hangs.
fn quick(program: &str, args: &[&str]) -> Option<String> {
    run_quick(program, args).filter(|(ok, _, _)| *ok).map(|(_, out, _)| out)
}

/// stdout and stderr together, whatever the exit code, when the program exists.
fn quick_all(program: &str, args: &[&str]) -> Option<String> {
    run_quick(program, args).map(|(_, out, err)| format!("{out}\n{err}"))
}

fn run_quick(program: &str, args: &[&str]) -> Option<(bool, String, String)> {
    use std::io::Read;
    use std::process::Stdio;
    // Through cmd on Windows, so .cmd shims such as npm, claude and code are found.
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = crate::proc::command("cmd");
        c.arg("/C").arg(program).args(args).creation_flags(0x0800_0000);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = crate::proc::command(program);
        c.args(args);
        c
    };
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < std::time::Duration::from_secs(8) => {
                std::thread::sleep(std::time::Duration::from_millis(40))
            }
            _ => {
                child.kill().warn_on_err("kill the timed-out program");
                return None;
            }
        }
    };
    let (mut out, mut err) = (String::new(), String::new());
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    child.stderr.take()?.read_to_string(&mut err).ok()?;
    // cmd answers for programs that do not exist with exit code 1 and "is not recognized".
    if !status.success() && (err.contains("not recognized") || err.contains("känns inte igen")) {
        return None;
    }
    Some((status.success(), out.trim().to_owned(), err.trim().to_owned()))
}

pub fn about(cfg: &Config, version: &str) -> About {
    let src = source_dir(cfg);
    let remote_url = src.as_deref().and_then(|s| git(s, &["remote", "get-url", &cfg.update.remote]));
    let config_path = config::config_path();
    About {
        version: version.to_owned(),
        update_enabled: cfg.update.is_enabled(version),
        remote: cfg.update.remote.clone(),
        branch: cfg.update.branch.clone(),
        interval_secs: cfg.update.interval_secs,
        repo_url: remote_url.as_deref().and_then(github_url),
        data_dir: cfg.data_dir.display().to_string(),
        config_exists: config_path.exists(),
        config_problem: config::problem(),
        config_path: config_path.display().to_string(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        user: std::env::var("USERNAME").or_else(|_| std::env::var("USER")).ok(),
        changelog: src.as_deref().map(changelog).unwrap_or_default(),
    }
}

/// The updater's clone when there is one, else the checkout a dev build was compiled from.
pub fn source_dir(cfg: &Config) -> Option<PathBuf> {
    let updater = cfg.update.source_in(&cfg.data_dir);
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    [updater, dev].into_iter().find(|d| d.join(".git").exists())
}

fn changelog(src: &Path) -> Vec<Change> {
    let Some(out) = git(src, &["log", "-n", "40", "--format=%h%x1f%ct%x1f%s", "HEAD"]) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\u{1f}');
            let hash = parts.next()?.to_owned();
            let ts = parts.next()?.parse::<i64>().ok()? * 1000;
            Some(Change { hash, ts, subject: parts.next()?.to_owned() })
        })
        .collect()
}

/// `git@github.com:a/b.git` or `https://github.com/a/b.git` as a web link.
fn github_url(remote: &str) -> Option<String> {
    let path = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))?;
    Some(format!("https://github.com/{}", path.trim_end_matches('/').trim_end_matches(".git")))
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    crate::git::run(dir, args).map(|s| s.trim().to_owned())
}

pub(crate) use crate::home_files::{home_dir as home, read_json};

/// API keys engines and tools commonly read from the environment.
const KNOWN_KEYS: &[(&str, &str)] = &[
    ("ANTHROPIC_API_KEY", "Anthropic"),
    ("OPENAI_API_KEY", "OpenAI"),
    ("GEMINI_API_KEY", "Google Gemini"),
    ("GOOGLE_API_KEY", "Google"),
    ("DEEPSEEK_API_KEY", "DeepSeek"),
    ("MISTRAL_API_KEY", "Mistral"),
    ("GROQ_API_KEY", "Groq"),
    ("OPENROUTER_API_KEY", "OpenRouter"),
    ("ELEVENLABS_API_KEY", "ElevenLabs"),
    ("HF_TOKEN", "Hugging Face"),
    ("GITHUB_TOKEN", "GitHub"),
    ("GH_TOKEN", "GitHub"),
];

/// The well-known keys set in the core's environment, by their end.
pub fn env_keys() -> Vec<KeyStatus> {
    KNOWN_KEYS
        .iter()
        .filter_map(|(name, service)| {
            let value = std::env::var(name).ok().filter(|v| !v.trim().is_empty())?;
            Some(KeyStatus { name: (*name).into(), service: (*service).into(), source: "environment".into(), hint: hint(&value) })
        })
        .collect()
}

pub fn accounts(cfg: &Config) -> Accounts {
    let home = home().unwrap_or_default();
    let keys = env_keys();
    let command = |name: &str| cfg.engines.get(name).map(|e| e.command.clone());
    Accounts { engines: crate::engine_home::engine_accounts(&home, command), keys }
}

pub(crate) use crate::text::key_hint as hint;


pub fn ssh() -> SshInfo {
    let dir = home().unwrap_or_default().join(".ssh");
    let mut keys = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("pub") {
                continue;
            }
            let Ok(line) = std::fs::read_to_string(&path) else { continue };
            let mut parts = line.split_whitespace();
            let kind = parts.next().unwrap_or_default().to_owned();
            let _blob = parts.next();
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64);
            keys.push(SshKey {
                file: entry.file_name().to_string_lossy().into_owned(),
                kind,
                comment: parts.collect::<Vec<_>>().join(" "),
                has_private: path.with_extension("").exists(),
                modified,
            });
        }
    }
    keys.sort_by(|a, b| a.file.cmp(&b.file));
    let hosts = std::fs::read_to_string(dir.join("config")).map(|t| parse_ssh_config(&t)).unwrap_or_default();
    let known_hosts = std::fs::read_to_string(dir.join("known_hosts"))
        .map(|t| t.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).count() as u32)
        .unwrap_or(0);
    SshInfo { dir: dir.display().to_string(), keys, hosts, known_hosts }
}

/// The `Host` blocks of an OpenSSH config, one per block: `Host tincan 192.168.0.174` is one
/// machine with two names. Patterns such as `*` are left out; they are defaults, not places to
/// connect to.
pub fn parse_ssh_config(text: &str) -> Vec<SshHost> {
    let mut hosts: Vec<SshHost> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once(|c: char| c.is_whitespace() || c == '=') {
            Some((k, v)) => (k.to_ascii_lowercase(), v.trim_start_matches(|c: char| c.is_whitespace() || c == '=').trim()),
            None => continue,
        };
        let value = value.trim_matches('"').to_owned();
        match key.as_str() {
            "host" => {
                current.clear();
                let mut names = value.split_whitespace().filter(|a| !a.contains(['*', '?', '!'])).map(str::to_owned);
                if let Some(alias) = names.next() {
                    current.push(hosts.len());
                    hosts.push(SshHost {
                        alias,
                        aliases: names.collect(),
                        hostname: None,
                        user: None,
                        port: None,
                        identity_file: None,
                        proxy_jump: None,
                    });
                }
            }
            "match" => current.clear(),
            _ => {
                for &i in &current {
                    let h = &mut hosts[i];
                    // As in ssh, the first value given for a host wins.
                    match key.as_str() {
                        "hostname" => h.hostname.get_or_insert(value.clone()),
                        "user" => h.user.get_or_insert(value.clone()),
                        "identityfile" => h.identity_file.get_or_insert(value.clone()),
                        "proxyjump" => h.proxy_jump.get_or_insert(value.clone()),
                        "port" => {
                            if h.port.is_none() {
                                h.port = value.parse().ok();
                            }
                            continue;
                        }
                        _ => continue,
                    };
                }
            }
        }
    }
    hosts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_engines_programs_come_from_their_harnesses() {
        let built_in = Config::default().engines;
        let harnesses: Vec<_> = crate::engines::harnesses().into_iter().filter(|h| built_in.contains_key(&h.id)).collect();
        let list = tool_list();
        let names: Vec<&str> = list.iter().map(|(name, ..)| name.as_str()).collect();
        assert_eq!(names[..4], ["git", "gh", "node", "npm"]);
        let engines: Vec<(&str, &str)> = list[4..6].iter().map(|(name, label, _)| (name.as_str(), label.as_str())).collect();
        let want: Vec<(&str, &str)> = harnesses.iter().map(|h| (h.cli.as_deref().unwrap_or_default(), h.label.as_str())).collect();
        assert_eq!(engines, want);
        assert_eq!(names[6..], ["cargo", "code", "docker", "python"]);
        assert!(list[4..6].iter().all(|(_, _, purpose)| purpose.starts_with("Logs in the ") && purpose.ends_with(" engine")));
    }

    #[test]
    fn reads_hosts_from_ssh_config() {
        let hosts = parse_ssh_config(
            "Host *\n  User nobody\n\nHost platnicklas pn\n  HostName 192.168.1.20\n  User christoffer\n  Port 2222\n  IdentityFile ~/.ssh/id_ed25519\n\n# comment\nHost=server\n  Hostname=server.local\n  ProxyJump platnicklas\n",
        );
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].alias, "platnicklas");
        assert_eq!(hosts[0].aliases, ["pn"]);
        assert_eq!(hosts[0].hostname.as_deref(), Some("192.168.1.20"));
        assert_eq!(hosts[0].port, Some(2222));
        assert_eq!(hosts[0].identity_file.as_deref(), Some("~/.ssh/id_ed25519"));
        assert_eq!(hosts[1].hostname.as_deref(), Some("server.local"));
        assert_eq!(hosts[1].user, None);
        assert_eq!(hosts[1].proxy_jump.as_deref(), Some("platnicklas"));
    }

    #[test]
    fn github_remotes_become_links() {
        assert_eq!(github_url("git@github.com:a/hub.git").as_deref(), Some("https://github.com/a/hub"));
        assert_eq!(github_url("https://github.com/a/hub").as_deref(), Some("https://github.com/a/hub"));
        assert_eq!(github_url("https://gitlab.com/a/hub"), None);
    }

    #[test]
    fn versions_are_found_in_tool_output() {
        assert_eq!(version_in("git version 2.47.1.windows.1").as_deref(), Some("2.47.1"));
        assert_eq!(version_in("v22.11.0").as_deref(), Some("22.11.0"));
        assert_eq!(version_in("2.1.4 (Claude Code)").as_deref(), Some("2.1.4"));
        assert_eq!(version_in("gh version 2.62.0 (2024-11-14)").as_deref(), Some("2.62.0"));
        assert_eq!(version_in("Docker version 27.3.1, build ce12230").as_deref(), Some("27.3.1"));
        assert_eq!(version_in("nothing here"), None);
    }

    fn edited(file: Value, key: &str, value: Value) -> Value {
        let mut file = file;
        apply_edit(&mut file, &ConfigEdit { key: key.into(), value }).unwrap();
        file
    }

    #[test]
    fn the_set_build_times_are_typed_in_one_box() {
        let file = edited(serde_json::json!({}), "update.build_times", serde_json::json!("16:30, 22:30"));
        assert_eq!(file["update"]["build_times"], serde_json::json!(["16:30", "22:30"]));
        let file = edited(file, "update.build_times", Value::Null);
        assert!(file["update"]["build_times"].is_null());
    }

    #[test]
    fn edits_touch_only_their_key() {
        let file = serde_json::json!({ "bind": "127.0.0.1:7979", "update": { "branch": "dev" } });
        let file = edited(file, "update.interval_secs", serde_json::json!(600));
        assert_eq!(file["update"], serde_json::json!({ "branch": "dev", "interval_secs": 600 }));
        assert_eq!(file["bind"], "127.0.0.1:7979");
        let file = edited(file, "update.branch", Value::Null);
        assert_eq!(file["update"], serde_json::json!({ "interval_secs": 600 }));
        let file = edited(file, "open.editor", serde_json::json!("  cursor "));
        assert_eq!(file["open"]["editor"], "cursor");
        let file = edited(file, "open.editor", serde_json::json!(""));
        assert!(file.get("open").is_none());
        let mut bad = file.clone();
        assert!(apply_edit(&mut bad, &ConfigEdit { key: "bind".into(), value: serde_json::json!("0.0.0.0:1") }).is_err());
    }

    #[test]
    fn editing_an_engine_keeps_the_others() {
        let file = edited(serde_json::json!({}), "engines.claude", serde_json::json!("claude-acp"));
        let cfg: Config = serde_json::from_value(file.clone()).unwrap();
        assert_eq!(cfg.engines["claude"].command, "claude-acp");
        assert_eq!(cfg.engines["codex"].command, Config::default().engines["codex"].command);
        let file = edited(file, "engines.claude", Value::Null);
        let cfg: Config = serde_json::from_value(file).unwrap();
        assert_eq!(cfg.engines["claude"].command, Config::default().engines["claude"].command);
    }

    #[test]
    fn commands_split_like_a_shell() {
        assert_eq!(
            crate::project::split_command(r#""C:\Program Files\JetBrains\idea64.exe" {dir} --new"#),
            vec![r"C:\Program Files\JetBrains\idea64.exe", "{dir}", "--new"]
        );
        assert_eq!(crate::project::split_command("cursor"), vec!["cursor"]);
    }

    #[test]
    fn hints_show_only_the_end() {
        assert_eq!(hint("sk-ant-0123456789abcd"), "…abcd");
        assert_eq!(hint("short"), "set");
    }
}
