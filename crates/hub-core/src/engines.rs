//! Adding engines from the settings page. An engine is a **harness**, the agent that does the
//! work (Claude Code, Codex, Hermes, OpenCode, Gemini CLI), on a **provider**, where its model
//! comes from: the harness's own login, or a key such as DeepSeek's. Picking the two and a model
//! writes one entry under `engines` in `hub.json`: the harness's command, the provider's key by
//! name in `secrets` (the value goes to the vault), the model in `model` and anything the harness
//! reads its provider and model from in `env`. From there it is an engine like any other: the
//! core never looks at which harness or provider it is.
//!
//! What Hub knows of harnesses and providers is the catalog below, data only. A harness or
//! provider that is not in it is added by hand: "Other" in both lists takes a command, a key
//! name and a provider id.
//!
//! Keys: Claude and Codex stay on your subscriptions, so the providers here never take
//! `config::ENGINE_KEYS`. A provider such as DeepSeek has no subscription, so a key you add for it
//! by hand is the same exception as an API connection.
//!
//! Engines are per machine: each hubd has its own `hub.json` and vault, so the settings page adds
//! an engine to another machine by sending the same request there (`?host=`).

use crate::locked::LockExt;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::config::{self, Config, EngineConfig};

/// The agent an engine runs.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Harness {
    pub id: String,
    pub label: String,
    pub what: String,
    /// The command that starts it as an ACP agent on this machine.
    pub command: String,
    /// How to install it on this machine, as a command to paste in a terminal.
    pub install: Option<String>,
    pub docs: Option<String>,
    /// It can use its own login (a subscription), with no key from Hub.
    pub own_login: bool,
    /// What to run once to log in to its own account, when it has one.
    pub login: Option<String>,
    /// The providers it can use, by id, besides its own login. Its threads pick among them in the
    /// model picker (`routes.rs`).
    pub providers: Vec<String>,
    /// Providers it can run on as an engine of its own, added by hand beside its login: Claude
    /// Code on DeepSeek's key. Together with `providers` this is the matrix of which harness runs
    /// which provider. A provider with a subscription (Claude, OpenAI) is never in it: those stay
    /// in their own harness. An older hubd sends none.
    pub pairs: Vec<String>,
    /// How it names a provider's model, with `{provider}` and `{model}`, e.g. `{provider}/{model}`.
    pub model_format: String,
    /// Environment it reads its provider and model from, with `{provider}`, `{model}` and
    /// `{full}` (the model as `model_format` names it).
    #[ts(type = "Record<string, string>")]
    pub env: BTreeMap<String, String>,
    /// The file it reads rules from in a folder.
    pub rules_file: Option<String>,
    /// A file of its own whose keys win over the environment Hub starts it with, under the home
    /// folder. Hub's key for a provider has to win, so a different value for the same variable
    /// there is commented out before the harness starts (`clear_stale_keys`).
    #[serde(default)]
    pub key_file: Option<String>,
    /// Resuming one of its sessions in a terminal, with `{session}`.
    pub terminal: Option<String>,
    /// The environment variable it reads the provider's key from when that is not the
    /// provider's own (`ANTHROPIC_AUTH_TOKEN` for Claude Code); the key goes there by its vault name.
    #[serde(skip)]
    #[ts(skip)]
    pub key_env: Option<String>,
    /// The model is also sent to the agent with ACP's `session/set_model` when a provider is
    /// picked. Hermes takes it no other way; Claude Code reads it from its environment.
    #[serde(skip)]
    #[ts(skip)]
    pub set_model: bool,
    /// Its program is found on this machine.
    pub installed: bool,
    /// The program you run yourself (`claude`), for its version and updates. Its ACP command may
    /// be another program.
    pub cli: Option<String>,
    /// Its npm package, when npm is one way to have it: where the newest version is read and how
    /// an npm install of it is updated.
    pub package: Option<String>,
    /// Its own update command after the program, e.g. `update` for `claude update`.
    pub self_update: Option<String>,
    /// Where its release notes are, for Settings > Updates: `md:<url>` for a Markdown file with
    /// one `## <version>` heading per release, `github:<owner>/<repo>` for its GitHub releases.
    pub changelog: Option<String>,
    /// Hub can change its own settings (`harness_settings.rs`), from its row in Settings.
    pub settings: bool,
    /// What to run after the program to have a background service of its own up before a
    /// thread needs it, and to start it again when it is gone (`standby.rs`). Hermes's ACP agent
    /// is a viewer of its gateway, which takes 10 to 20 seconds to start on Windows.
    pub standby: Option<String>,
    /// What to run instead when the keys Hub hands the service changed since it was started, so
    /// it takes the new ones (`standby.rs`).
    pub standby_again: Option<String>,
    /// It puts an MCP server's instructions in front of the model (Claude Code does). One that
    /// does not is given Hub's instructions in its first message instead
    /// (`EngineTraits::mcp_instructions`).
    #[serde(skip)]
    #[ts(skip)]
    pub mcp_instructions: bool,
    /// It takes an MCP server over HTTP in its session request although it does not say so when it
    /// starts (Hermes does not), so Hub's tools are handed to it anyway (`EngineTraits::mcp_http`).
    #[serde(skip)]
    #[ts(skip)]
    pub mcp_http: bool,
    /// It does not show the headers it is given for an MCP server (Hermes), so the tool token
    /// also goes in the address (`EngineTraits::mcp_token_in_url`).
    #[serde(skip)]
    #[ts(skip)]
    pub mcp_token_in_url: bool,
}

impl Harness {
    /// Whether it can run on provider `id`: one its threads pick among, or one it is paired with.
    pub fn runs_on(&self, id: &str) -> bool {
        self.providers.iter().chain(&self.pairs).any(|p| p == id)
    }

    /// Whether `id` is a provider it runs on only as an engine of its own (Claude Code on DeepSeek).
    pub fn paired(&self, id: &str) -> bool {
        !self.providers.iter().any(|p| p == id) && self.pairs.iter().any(|p| p == id)
    }
}

/// Where an engine's model comes from.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Provider {
    pub id: String,
    pub label: String,
    pub what: String,
    /// The environment variable its key goes in, also its name in the vault.
    pub secret: String,
    /// Its OpenAI-style API, for checking the key and listing models.
    pub url: Option<String>,
    /// Its Anthropic-style API, which Claude Code speaks (`{url}` in a harness's environment).
    #[serde(skip)]
    #[ts(skip)]
    pub anthropic_url: Option<String>,
    /// Models to start from, best first. The harness may offer more.
    pub models: Vec<String>,
    /// Where the key is made.
    pub keys_at: Option<String>,
    /// The vault has a key for it, under its own name or one like it.
    pub has_key: bool,
    /// Keys in the vault that look like its key, best first: its own name, then names with the
    /// provider's name in them, like `DEEPSEEK` for `DEEPSEEK_API_KEY`.
    pub keys: Vec<VaultMatch>,
    /// The thinking levels its models really have, by the names harnesses send them under
    /// (`reasoning_effort`), least first. A harness that offers more levels shows only these;
    /// empty shows what the harness offers.
    pub efforts: Vec<Effort>,
    /// Other words a key for it may be named by, upper case, e.g. `MOONSHOT` for Kimi.
    #[serde(skip)]
    #[ts(skip)]
    pub aliases: Vec<String>,
}

/// One thinking level of a provider's models.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Effort {
    /// As harnesses send it, e.g. `none` or `high`.
    pub value: String,
    pub name: String,
    pub what: String,
    /// The provider's own default.
    pub default: bool,
}

/// A vault key, by name and its last four characters.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct VaultMatch {
    pub name: String,
    pub hint: String,
}

/// An engine as the settings page lists it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EngineEntry {
    pub name: String,
    pub label: String,
    pub command: String,
    pub harness: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    /// Keys it is handed, and whether the vault has each.
    pub secrets: Vec<(String, bool)>,
    /// New threads start on it.
    pub default: bool,
    /// The running core has it; otherwise it was added or changed and waits for a restart.
    pub running: bool,
    /// Hub ships it, so removing it puts it back as it was.
    pub builtin: bool,
    /// The machine it was added on, when that is not this one; it is changed there.
    pub shared_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EngineCatalog {
    pub harnesses: Vec<Harness>,
    pub providers: Vec<Provider>,
    pub engines: Vec<EngineEntry>,
}

/// An engine to try or add, as picked on the settings page.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct EngineDraft {
    /// Its name in Hub; made from the harness and provider when empty.
    pub name: String,
    /// A harness id from the catalog, or `other` with `command`.
    pub harness: String,
    /// A provider id from the catalog, `other` with `provider_id` and `secret`, or empty for the
    /// harness's own login.
    pub provider: String,
    pub model: String,
    /// The key, when the vault does not have it yet. Saved to the vault on add, never to
    /// `hub.json`.
    pub key: String,
    /// A key already in the vault to use instead, by its name there, e.g. `DEEPSEEK`.
    pub key_name: String,
    /// A command of your own instead of the harness's.
    pub command: String,
    /// For provider `other`: its id as the harness names it, e.g. `kimi-coding`.
    pub provider_id: String,
    /// For provider `other`: the environment variable its key goes in.
    pub secret: String,
}

/// What trying an engine found.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct EngineTest {
    pub ok: bool,
    /// One line on what happened.
    pub message: String,
    /// What to do about it, e.g. the install command.
    pub fix: Option<String>,
    /// The provider took the key (None when there is no key to check).
    pub key_ok: Option<bool>,
    /// The agent's own name and version.
    pub agent: Option<String>,
    /// Every model the engine offers, as (value, name).
    pub models: Vec<(String, String)>,
    /// The model a new thread starts on.
    pub current: Option<String>,
    /// The model asked for was found among what it offers.
    pub model_found: bool,
}

/// Whether a session mode an engine offers lets its agent act without asking: Claude Code's
/// `bypassPermissions`, Codex's full access, and the like by name. A phone may not switch a
/// thread to one (`api::set_setting`).
pub fn asks_nothing(mode: &str) -> bool {
    let m = mode.to_ascii_lowercase().replace(['-', '_', ' '], "");
    ["bypass", "fullaccess", "dangerfull", "yolo", "neverask", "autoapprove"].iter().any(|w| m.contains(w))
}

/// Whether the program a command starts is found on this machine.
pub fn installed(command: &str) -> bool {
    resolve(command).is_some()
}

/// The command as it can run here: as it is when its program is on Hub's PATH, or with the
/// program's full path when it was installed after Hub started (an installer adds its folder to
/// the PATH of new shells only). None when the program is nowhere.
pub fn resolve(command: &str) -> Option<String> {
    let words = crate::project::split_command(command);
    let (program, rest) = words.split_first()?;
    let path = std::path::Path::new(program);
    if path.components().count() > 1 {
        return path.is_file().then(|| command.to_owned());
    }
    if on_path(program, std::env::var_os("PATH")) {
        return Some(command.to_owned());
    }
    let found = fresh_lookup(program)?;
    let found = found.display().to_string();
    let quoted = if found.contains(' ') { format!("\"{found}\"") } else { found };
    Some(std::iter::once(quoted).chain(rest.iter().map(|w| if w.contains(' ') { format!("\"{w}\"") } else { w.clone() })).collect::<Vec<_>>().join(" "))
}

/// The full path of a program, on PATH or where a new login shell finds it.
pub fn locate(program: &str) -> Option<std::path::PathBuf> {
    let names = names(program);
    std::env::var_os("PATH")
        .and_then(|dirs| std::env::split_paths(&dirs).find_map(|d| names.iter().map(|n| d.join(n)).find(|p| p.is_file())))
        .or_else(|| fresh_lookup(program))
}

fn names(program: &str) -> Vec<String> {
    if cfg!(windows) && std::path::Path::new(program).extension().is_none() {
        ["exe", "cmd", "bat"].iter().map(|e| format!("{program}.{e}")).collect()
    } else {
        vec![program.to_owned()]
    }
}

fn on_path(program: &str, dirs: Option<std::ffi::OsString>) -> bool {
    let Some(dirs) = dirs else { return false };
    let names = names(program);
    std::env::split_paths(&dirs).any(|d| names.iter().any(|n| d.join(n).is_file()))
}

/// Where a new login shell would find `program`, asked at most every half minute per program.
fn fresh_lookup(program: &str) -> Option<std::path::PathBuf> {
    static SEEN: LazyLock<Mutex<BTreeMap<String, (Instant, Option<std::path::PathBuf>)>>> = LazyLock::new(Default::default);
    if CLEAR.swap(false, std::sync::atomic::Ordering::SeqCst) {
        SEEN.locked().clear();
    }
    if let Some((at, found)) = SEEN.locked().get(program)
        && at.elapsed() < Duration::from_secs(30)
    {
        return found.clone();
    }
    let found = ask_shell(program).or_else(|| {
        // Where installers commonly put a program for the user, in case the shell knows nothing.
        let home = crate::folders::home();
        let mut spots = vec![home.join(".local").join("bin"), home.join(".hermes").join("bin"), home.join(".opencode").join("bin")];
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from) {
            spots.push(local.join("hermes").join("bin"));
            spots.push(local.join("Programs").join("hermes"));
        }
        if let Some(roaming) = std::env::var_os("APPDATA").map(std::path::PathBuf::from) {
            spots.push(roaming.join("npm"));
        }
        let names = names(program);
        spots.into_iter().find_map(|d| names.iter().map(|n| d.join(n)).find(|p| p.is_file()))
    });
    SEEN.locked().insert(program.to_owned(), (Instant::now(), found.clone()));
    found
}

fn ask_shell(program: &str) -> Option<std::path::PathBuf> {
    if !program.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
        return None;
    }
    let mut cmd = if cfg!(windows) {
        let mut c = crate::proc::command("powershell");
        c.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!(
                "$env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User'); (Get-Command {program} -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1).Source"
            ),
        ]);
        c
    } else {
        let mut c = crate::proc::command("bash");
        c.args(["-lc", &format!("command -v {program}")]);
        c
    };
    cmd.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    let line = String::from_utf8_lossy(&out.stdout).lines().map(str::trim).find(|l| !l.is_empty())?.to_owned();
    let path = std::path::PathBuf::from(line);
    path.is_file().then_some(path)
}

// ---- Installing a harness from the settings page ----

/// The vault keys that look like a provider's key, best first: its own name, then names with the
/// provider's id or another of its words as a whole part (`DEEPSEEK`, `MY_DEEPSEEK_KEY`).
pub fn matching_keys(p: &Provider, vault: &[crate::model::SecretInfo]) -> Vec<VaultMatch> {
    let words: Vec<String> = std::iter::once(p.id.to_uppercase()).chain(p.aliases.iter().cloned()).collect();
    let mut found: Vec<(u8, VaultMatch)> = vault
        .iter()
        .filter_map(|s| {
            let name = s.name.to_uppercase();
            let parts: Vec<&str> = name.split(|c: char| !c.is_ascii_alphanumeric()).collect();
            let rank = if s.name == p.secret {
                0
            } else if words.iter().any(|w| parts.contains(&w.as_str())) {
                1
            } else if words.iter().any(|w| w.len() >= 4 && name.contains(w.as_str())) {
                2
            } else {
                return None;
            };
            Some((rank, VaultMatch { name: s.name.clone(), hint: s.hint.clone() }))
        })
        .filter(|(_, m)| !crate::config::ENGINE_KEYS.contains(&m.name.as_str()))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.name.cmp(&b.1.name)));
    found.into_iter().map(|(_, m)| m).collect()
}

static CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder().user_agent(concat!("hub/", env!("CARGO_PKG_VERSION"))).build().unwrap_or_default()
});

/// Asks a provider's API for its models with the key: whether it takes the key, and the models.
pub(crate) async fn check_key(url: &str, key: &str) -> (bool, String, Vec<String>) {
    let sent = CLIENT.get(format!("{}/models", url.trim_end_matches('/'))).bearer_auth(key).timeout(Duration::from_secs(15)).send().await;
    match sent {
        Err(e) => (false, format!("no answer from {url}: {e}"), Vec::new()),
        Ok(r) if r.status() == 401 || r.status() == 403 => (false, format!("the key was refused ({})", r.status()), Vec::new()),
        Ok(r) if !r.status().is_success() => (false, format!("{url} answered {}", r.status()), Vec::new()),
        Ok(r) => {
            let body: Value = r.json().await.unwrap_or_default();
            let models = body["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m["id"].as_str().map(str::to_owned))
                .collect();
            (true, "the key works".into(), models)
        }
    }
}

/// The harness an engine runs, when it can run on providers Hub hands it: one that runs on many
/// (Hermes, OpenCode), or one paired with the provider the engine was added for (Claude Code on
/// DeepSeek's key). The harness on its own login (`claude`) is neither, so it has no route.
pub fn harness(name: &str, engine: &EngineConfig) -> Option<Harness> {
    let id = engine.harness.as_deref().unwrap_or(name);
    harnesses().into_iter().find(|h| h.id == id && (!h.providers.is_empty() || engine.provider.as_deref().is_some_and(|p| h.paired(p))))
}

/// What an engine is called in lists: a harness that runs on many providers goes by the
/// harness alone, since its threads pick the provider; one paired with a provider keeps the
/// name it was added with ("Claude Code · DeepSeek"), so it is not taken for Claude Code itself.
pub fn label(name: &str, engine: &EngineConfig) -> Option<String> {
    harness(name, engine).filter(|h| !h.providers.is_empty()).map(|h| h.label).or_else(|| engine.label.clone())
}

mod share;
pub use share::*;

mod presets;
pub use presets::*;

mod install;
pub use install::*;

mod setup;
pub use setup::*;


#[cfg(test)]
mod tests;

// ---- One list of engines on every machine ----
//
// Engines are added once, on the machine that knows the others (the PC). Every minute, and right
// after an engine is added or removed, it sends the engines it added (not the built-in ones, and
// not ones it got from elsewhere) to each machine online, with the keys they use. Each machine
// saves them as its own, marked `shared_from`, with its own command for the harness, so a
// harness installed differently there still starts. An engine removed on the PC goes on the
// others too. What each machine answers (installed, running) is kept to show on the page.

/// Keys that pay per token and that hubd may carry in its own environment (a shell profile, a
/// service file). An engine started from hubd would inherit them and bill the API instead of the
/// subscription. Hub's own sign-in token (`CLAUDE_CODE_OAUTH_TOKEN`) is the subscription and stays.
const PAID_KEYS: [&str; 5] = ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "OPENAI_API_KEY", "CODEX_API_KEY", "GEMINI_API_KEY"];

/// The pay-per-token keys this process would hand an engine, as `(name, "")` to blank them,
/// except those Hub sets on purpose (`explicit`: the engine's own env, its vault keys, a project's
/// APIs). `lookup` reads the environment; a key that is empty there is not inherited.
pub fn inherited_paid_keys(explicit: &[&str], lookup: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    PAID_KEYS
        .iter()
        .filter(|k| !explicit.contains(k) && lookup(k).is_some_and(|v| !v.trim().is_empty()))
        .map(|k| ((*k).to_owned(), String::new()))
        .collect()
}

/// Comments out each of `keys`' variables that `file` sets to another value, so the harness takes
/// the one Hub hands it. True when the file changed. The old value is not kept.
pub fn clear_stale_keys(file: &std::path::Path, keys: &[(String, String)]) -> Result<bool> {
    let Ok(text) = std::fs::read_to_string(file) else { return Ok(false) };
    let mut changed = false;
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            let body = line.trim_start();
            let body = body.strip_prefix("export ").unwrap_or(body);
            let Some((var, value)) = body.split_once('=') else { return line.to_owned() };
            let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
            match keys.iter().find(|(k, _)| k == var.trim()) {
                Some((k, v)) if !value.is_empty() && value != v => {
                    changed = true;
                    format!("# {k}: Hub hands this key over from its vault (Settings > Engines > Providers)")
                }
                _ => line.to_owned(),
            }
        })
        .collect();
    if changed {
        let mut out = lines.join("\n");
        if text.ends_with('\n') {
            out.push('\n');
        }
        std::fs::write(file, out).with_context(|| format!("writing {}", file.display()))?;
    }
    Ok(changed)
}

#[cfg(test)]
mod key_file_tests;


#[cfg(test)]
mod inherited_keys_tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        match name {
            "ANTHROPIC_API_KEY" | "OPENAI_API_KEY" => Some("sk-inherited".into()),
            "GEMINI_API_KEY" => Some("  ".into()),
            "CLAUDE_CODE_OAUTH_TOKEN" => Some("sk-ant-oat-hub".into()),
            _ => None,
        }
    }

    #[test]
    fn inherited_keys_are_blanked_but_the_sign_in_token_stays() {
        let got = inherited_paid_keys(&[], env);
        assert_eq!(got, [("ANTHROPIC_API_KEY".to_owned(), String::new()), ("OPENAI_API_KEY".to_owned(), String::new())]);
    }

    #[test]
    fn a_key_hub_sets_on_purpose_is_left_alone() {
        let got = inherited_paid_keys(&["OPENAI_API_KEY"], env);
        assert_eq!(got, [("ANTHROPIC_API_KEY".to_owned(), String::new())]);
    }
}
