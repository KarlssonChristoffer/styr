//! Hub's settings, `hub.json` in the data folder (or `HUB_CONFIG`): the `Config` shape and its
//! defaults (hosts, updates and build steps, engines and the `EngineTraits` an engine's name
//! presets), and the file itself, loaded at start (a broken one set aside, Hub on defaults) and
//! changed under one lock and written whole (`edit_file`).

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::Context;
use crate::locked::Ignore;

use serde::{Deserialize, Serialize};

/// Keys that would move an engine off your subscription onto paying per token.
pub const ENGINE_KEYS: [&str; 6] =
    ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", "OPENAI_API_KEY", "CODEX_API_KEY", "GEMINI_API_KEY"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Where the database lives.
    pub data_dir: PathBuf,
    /// Loopback only until remote hosts get real auth.
    pub bind: SocketAddr,
    pub default_engine: String,
    /// Engine name to the command that starts an ACP agent on stdio.
    pub engines: BTreeMap<String, EngineConfig>,
    /// Browser origins allowed to call the API (the app's webview and the dev server).
    pub allowed_origins: Vec<String>,
    pub update: UpdateConfig,
    pub tidy: TidyConfig,
    pub voice: crate::voice_config::VoiceConfig,
    /// Snapshot a thread's repository before each message, so its work can be rolled back.
    pub checkpoints: bool,
    /// How folders are opened in an editor or a terminal.
    pub open: OpenConfig,
    /// Give each thread's agent Hub's own tools (a second opinion) as an MCP server.
    pub agent_tools: bool,
    /// What this machine is called on host chips. Defaults to "PC" on Windows and the hostname
    /// elsewhere.
    pub name: Option<String>,
    /// Other machines running hubd, whose threads show and can be started here. More can be
    /// added from the app.
    pub hosts: Vec<HostConfig>,
    /// Keep every thread with a running engine in its own tmux session (`hub-<id>`) showing it
    /// live, so `tmux attach` reaches it and Enter takes it over. Only where tmux is installed;
    /// never on Windows.
    pub tmux: bool,
    /// When the core restarts (an update, new settings, a crash) in the middle of a turn, the
    /// next core asks the agent to carry on where it was cut off.
    pub resume_turns: bool,
    /// Run each engine under a small keeper process (`keeper.rs`) that outlives the core, so an
    /// update or a restart of the core does not cut the turn off: the next core takes the engine
    /// over where it is.
    pub keep_engines: bool,
    /// The program that keeps engines (`hubd keep`); unset, the running `hubd`. Tests point it at
    /// a built `hubd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keeper: Option<PathBuf>,
    /// Keep the computer from sleeping while a thread here is working (`awake.rs`).
    pub keep_awake: bool,
    /// Fast-forward each project's default branch when it is clean (`autopull.rs`).
    pub auto_pull: bool,
    /// Engines that run shell commands through `CLAUDE_CODE_SHELL_PREFIX` (Claude Code) run them
    /// through Hub's `hub-shell`, so their output can be watched while they run (`live_output.rs`).
    pub live_output: bool,
    /// Which engine does a kind of side job (`children.rs`), e.g. `"image": "codex"`, since
    /// not every engine can draw. A kind left out runs on the engine of the thread that asked.
    pub side_jobs: BTreeMap<String, String>,
    /// Where engines here reach Customs' registry (`customs_registry`), e.g. the main machine's
    /// `https://pc.tail1234.ts.net`. Unset uses this hubd's own address.
    pub customs_url: Option<String>,
    /// Keep every JSON-RPC line between Hub and an engine in `<data dir>/engines/<thread>.trace.jsonl`
    /// (keys redacted), to record a real session as a test fixture. Off unless asked for.
    pub trace_engines: bool,
    /// Keep the key vault in memory only (`secrets.rs`): keys other machines lend this one
    /// (`engine_share::share`) are never written to disk and are gone when hubd stops, until the
    /// sender hands them over again. For a machine that is exposed, such as a VPS.
    pub keys_in_memory: bool,
    /// Hand on the engines, keys and sign-ins other machines lent this one, as well as its own
    /// (`engine_share::outgoing`). Set on a machine that is always on, such as tincan, so a machine
    /// that keeps keys in memory gets them again after a restart while the PC is off.
    pub relay_keys: bool,
}

/// Whether an update step builds the release (after the tests) rather than checks the code.
pub fn is_build_step(step: &BuildStep) -> bool {
    step.command.iter().any(|a| a == "build" || a == "--release")
}

/// The update's steps up to the release build: what CI is, where Hub runs.
pub fn default_checks() -> Vec<BuildStep> {
    UpdateConfig::default_steps().into_iter().take_while(|s| !is_build_step(s)).collect()
}

/// Another machine running hubd. Reached through an SSH tunnel to its loopback port, so it
/// needs no open port, or at a URL (Tailscale).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostConfig {
    pub name: String,
    /// An alias from `~/.ssh/config` (or `user@host`). Keys only; Hub never asks for a password.
    #[serde(default)]
    pub ssh: Option<String>,
    /// Where hubd answers, e.g. `https://tincan.tail1234.ts.net` (Tailscale serve). Tried before
    /// `ssh` when both are set.
    #[serde(default)]
    pub url: Option<String>,
    /// hubd's port on the other machine, for `ssh`.
    #[serde(default = "default_port")]
    pub port: u16,
    /// A token the other hubd accepts: one it gave this machine for a pairing code, or its own.
    /// Read over SSH from its data folder when not set.
    #[serde(default)]
    pub token: Option<String>,
    /// At least this many minutes between two versions this machine sends it on its own (the
    /// ten-minute check, `remote/ship.rs`), so a day of merges does not restart it each time.
    /// Update now, and a host with no version yet, still get one at once. Unset: no gap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_quiet_mins: Option<u64>,
}

fn default_port() -> u16 {
    7878
}

/// Commands that open a folder, e.g. `cursor` or `"C:\\Tools\\idea64.exe" {dir}`. The folder is
/// added at the end unless the command says where with `{dir}`. Unset uses VS Code and Windows
/// Terminal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenConfig {
    pub editor: Option<String>,
    pub terminal: Option<String>,
}

/// Turning a long, messy dump into a clear prompt: one short answer from a small model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TidyConfig {
    /// Engine to ask. Defaults to the default engine.
    pub engine: Option<String>,
    /// Picks the first model whose id or name contains this, when the engine offers models.
    pub model: Option<String>,
}

impl Default for TidyConfig {
    fn default() -> Self {
        Self { engine: None, model: Some("haiku".into()) }
    }
}

/// Self-update: the core follows a branch, builds new versions and restarts into them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateConfig {
    /// Off for development builds unless set; on for installed builds.
    pub enabled: Option<bool>,
    /// The updater's own clone. Defaults to `<data dir>/src`.
    pub source_dir: Option<PathBuf>,
    pub remote: String,
    pub branch: String,
    pub interval_secs: u64,
    /// Ask before switching to a version that was built and tested, instead of switching as soon
    /// as no thread is working.
    pub ask: bool,
    /// Where Rust builds go, e.g. on another drive. Defaults to `target` in the source folder.
    pub target_dir: Option<PathBuf>,
    /// Gigabytes that must be free on the build folder's drive before a build starts.
    pub min_free_gb: u64,
    /// The build folder is trimmed (incremental caches, then the tests' debug build) down below
    /// this many gigabytes after each update.
    pub max_build_gb: u64,
    /// Minutes one build step may take before it is stopped and the update counts as failed, so
    /// a hanging test or build cannot hold the update for ever.
    pub step_timeout_mins: u64,
    /// Build commands, run in order. Defaults to the tests, then the release build of the core and
    /// the app.
    pub steps: Option<Vec<BuildStep>>,
    /// Built files to install, relative to the source folder.
    pub artifacts: Option<Vec<PathBuf>>,
    /// Versions come from the PC, which builds each one for this machine and sends it (see
    /// `crossbuild`), instead of being fetched and built here. For Linux machines that only run
    /// the core, such as a Raspberry Pi; they need neither Rust nor a GitHub login.
    pub pushed: bool,
    /// A version whose Rust code and app shell are the same as an installed one's keeps that one's
    /// programs: only the checks of the pages and their build run, and no Rust is tested or built
    /// (see `update::pages_only_donor`). Off builds everything every time.
    pub fast_pages: bool,
    /// A version that has to build the programs (its Rust changed) waits until the branch has
    /// been quiet this many minutes, so Rust changes merged one after another build as one
    /// version with one set of patch notes (default 2). "Build now" under Updates skips the wait;
    /// 0 never waits. A version that only changes the pages never waits, and neither does one
    /// merged while the last build ran: that build was its wait.
    pub rust_quiet_mins: u64,
    /// When a version that changes Rust starts building (`BuildMode`). Read again on every look,
    /// so a change under Settings > Updates counts at once.
    pub build_mode: BuildMode,
    /// `BuildMode::Merges`: builds once this many Rust changes (merged pull requests) are waiting.
    pub build_merges: u32,
    /// `BuildMode::Merges`: the counts at which the window says how many wait (3 and 5).
    pub build_notice_at: Vec<u32>,
    /// `BuildMode::Merges`: builds anyway this many minutes after the oldest waiting Rust change,
    /// so one pull request never waits for ever.
    pub build_max_wait_mins: u64,
    /// `BuildMode::Times`: clock times (`HH:MM`, on the clock of `build_utc_offset_min`) at which
    /// what is waiting builds.
    pub build_times: Vec<String>,
    /// The offset from UTC, in minutes, of the clock `build_times` are on; Settings sends the
    /// window's own.
    pub build_utc_offset_min: i32,
    /// Tells GitHub where each version of the branch stands on this machine, as a commit status
    /// `hub/update · <machine>` on its commit: waiting, building (which step), ready, installed or
    /// failed. Threads that opened the pull requests read it instead of guessing.
    pub report_status: bool,
    /// The name of a machine (Settings > Machines) that runs the tests, `npm run check` and
    /// `cargo test`, while this one builds the release: the tests take most of the memory of an
    /// update. The version is installed once that machine says they passed. A machine that cannot
    /// (offline, too old, no room) leaves the tests to this one, as before. Unset runs them here.
    #[serde(default)]
    pub tests_on: Option<String>,
    /// With `tests_on`: install the version as soon as it is built and let the machine's tests
    /// finish after. A failing test then shows up as a problem (Hub's Problems) instead of
    /// stopping the version. If the machine cannot run them, they do not run, and a warning says so.
    #[serde(default)]
    pub tests_after: bool,
    /// Minutes a new version's core must have been up, with `/api/health` answering, before the
    /// version counts as healthy (`rollback.rs`). Until then it is on probation and is rolled back
    /// if it fails.
    pub probation_mins: u64,
    /// Restarts nobody asked for that fail a version on probation.
    pub probation_crashes: u32,
}

/// When a version that changes the Rust code starts building. A version that only changes the
/// pages never waits, whatever this says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildMode {
    /// After `rust_quiet_mins` of a quiet branch: today's behaviour.
    #[default]
    RightAway,
    /// Once `build_merges` Rust changes wait, or `build_max_wait_mins` after the oldest.
    Merges,
    /// At the clock times in `build_times`.
    Times,
    /// Only when someone presses Build now.
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStep {
    /// Folder relative to the source folder.
    #[serde(default)]
    pub dir: PathBuf,
    pub command: Vec<String>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: None,
            source_dir: None,
            remote: "origin".into(),
            branch: "main".into(),
            interval_secs: 180,
            ask: true,
            target_dir: None,
            min_free_gb: 8,
            max_build_gb: 10,
            step_timeout_mins: 45,
            steps: None,
            artifacts: None,
            pushed: false,
            fast_pages: true,
            rust_quiet_mins: 2,
            build_mode: BuildMode::RightAway,
            build_merges: 8,
            build_notice_at: vec![3, 5],
            build_max_wait_mins: 60,
            build_times: Vec::new(),
            build_utc_offset_min: 0,
            report_status: true,
            tests_on: None,
            tests_after: false,
            probation_mins: 5,
            probation_crashes: 2,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EngineConfig {
    pub command: String,
    /// What Hub calls it, e.g. "Hermes · DeepSeek". Unset, its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The harness in Hub's catalog it was added from (`engines.rs`), e.g. `hermes`. Only for
    /// the settings page; the command is what runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness: Option<String>,
    /// Where its model comes from, e.g. `deepseek`, or unset for the harness's own login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The model a new session starts on, matched like a model name against what the engine
    /// offers (e.g. `deepseek:deepseek-chat`). A model picked in the thread wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Vault keys handed to it under another name: the environment variable it reads to the key's
    /// name in the vault, e.g. `DEEPSEEK_API_KEY` to a key saved as `DEEPSEEK`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, String>,
    /// Environment variables that are not secret, handed to the engine as they are.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// The machine this engine came from, when it was added there and shared here
    /// (`engine_share::receive`). Such an engine follows that machine: changed or removed there, it
    /// changes or goes here too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_from: Option<String>,
    /// How the core talks to it. See `engine/mod.rs` for what every driver provides.
    #[serde(default)]
    pub driver: Driver,
    /// Secrets kept by Hub to hand this engine as environment variables, by name.
    #[serde(default)]
    pub secrets: Vec<String>,
    /// The engine's own CLI resuming a session in a terminal, with `{session}` where the session
    /// id goes, e.g. `claude --resume {session}`. Lets `hubd attach` take a thread over; unset,
    /// the thread cannot be taken over.
    #[serde(default)]
    pub terminal: Option<String>,
    /// What Hub needs to know about this engine that ACP does not say. Unset fields take the
    /// built-in values for the engine's name (`EngineTraits::preset`), so a config written
    /// before they existed works as it did.
    #[serde(default, flatten)]
    pub traits: EngineTraits,
}

/// The parts of an engine that differ between engines and that ACP has no call for. The core
/// reads them from here and never from the engine's name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineTraits {
    /// A prompt the engine answers with its plan limits as Markdown (`usage::parse`), e.g.
    /// Claude's `/usage`, which its adapter answers without a model call. Only for an engine that
    /// answers it for free: Hub asks it every couple of minutes while threads work. Unset, Hub
    /// does not ask.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_probe: Option<String>,
    /// Words that start the engine's message for a plan limit that ran out, lower case, like
    /// "you've hit your". A message with one of them and the word "limit" is a limit hit; `|` and
    /// a Unix time after it say when it resets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_phrases: Option<Vec<String>>,
    /// The file in a folder the engine reads its rules from, e.g. `AGENTS.md`. Hub saves rules
    /// there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_file: Option<String>,
    /// Models for second opinions, strongest first, matched against the engine's model ids and
    /// names. The first is what an unset or "default" model is. A thread on the first gets the
    /// second; otherwise the first, or the last when a plan limit runs low.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_models: Option<Vec<String>>,
    /// API prices of the engine's models, for the cost in the stats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prices: Option<Vec<Price>>,
    /// Words in the engine's error, lower case, that mean its sign-in is gone, like "oauth
    /// session expired". Hub then shows the machine as not signed in and offers a new sign-in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_phrases: Option<Vec<String>>,
    /// The command that signs the engine in on a machine without a browser, run by
    /// `hubd sign-in <engine>` in a terminal there, e.g. `codex login --device-auth`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<String>,
    /// When `sign_in` prints a long-lived token instead of saving a login, how Hub keeps it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sign_in_token: Option<SignInToken>,
    /// Where the engine writes a session's log as Messages-style JSONL, with `{session}` for the
    /// session id, `~` for the home folder and one `*` for any folder, e.g.
    /// `~/.claude/projects/*/{session}.jsonl`. Hub reads it to show what a thread's tokens went
    /// to (`usage_detail.rs`). Unset, there is no breakdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_log: Option<String>,
    /// The engine's own CLI starting a new conversation under a session id Hub already holds,
    /// with `{session}`, e.g. `claude --session-id {session}`. Taking over a thread whose
    /// session has had no prompt yet needs it, because `terminal` finds nothing to resume
    /// there. Unset, such a thread cannot be taken over until something is said in it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_new: Option<String>,
    /// Whether the engine puts an MCP server's `instructions` in front of the model, as Claude
    /// Code does. One that does not (or may not) gets Hub's instructions in its first message of
    /// each session instead (`engine/acp.rs`), since the text that says what the chat shows and
    /// how to show a diagram has to reach the model. Unset counts as no.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_instructions: Option<bool>,
    /// Whether to hand the engine Hub's tools (an MCP server over HTTP) although it does not say
    /// it takes one when it starts. Hermes takes them and does not say so; without this it never
    /// had `show_page`, the picture side job or `page_check`. Unset follows what the engine says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_http: Option<bool>,
    /// It does not show the headers it is given for an MCP server (Hermes), so Hub's tools get
    /// the thread's tool token in the address as well (`?token=`, which `guard::check` reads).
    /// Without it the engine was refused Hub's tools and its thread fell back on raw shell calls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_token_in_url: Option<bool>,
}

/// A long-lived sign-in token an engine prints (`claude setup-token`). Hub keeps it in that
/// machine's vault and hands it to the engine as `env`, so nothing has to refresh it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SignInToken {
    /// The environment variable the engine reads it from.
    pub env: String,
    /// How every such token starts, to find it in what the command printed.
    pub prefix: String,
    /// How long one lasts.
    pub days: u32,
}

/// Dollars per million tokens for models whose id starts with `model`. A cache write costs 1.25
/// times input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Price {
    pub model: String,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
}

impl EngineConfig {
    /// Every vault key it is handed, by its name in the vault.
    pub fn key_names(&self) -> Vec<String> {
        self.secrets.iter().chain(self.keys.values()).cloned().collect()
    }
}

impl EngineTraits {
    /// What Hub knows of the engines it ships with, by their names in the default config.
    pub fn preset(engine: &str) -> Self {
        let words = |w: &[&str]| Some(w.iter().map(|s| s.to_string()).collect());
        match engine {
            "claude" => Self {
                usage_probe: Some("/usage".into()),
                limit_phrases: words(&[
                    "usage limit reached",
                    "you've hit your",
                    "you've reached your",
                    "you’ve hit your",
                    "you’ve reached your",
                ]),
                rules_file: Some("CLAUDE.md".into()),
                review_models: words(&["opus", "sonnet", "haiku"]),
                prices: Some(claude_prices()),
                session_log: Some(claude_session_log()),
                terminal_new: Some("claude --session-id {session}".into()),
                mcp_instructions: Some(true),
                mcp_http: None,
                mcp_token_in_url: None,
                auth_phrases: words(&[
                    "oauth session expired",
                    "oauth token has expired",
                    "failed to authenticate",
                    "authentication required",
                    "invalid bearer token",
                    "invalid_grant",
                    "please run /login",
                ]),
                // A year-long token on the subscription, made for machines without a browser.
                // Unlike the normal login it is never refreshed, so it cannot be refreshed away.
                sign_in: Some("claude setup-token".into()),
                sign_in_token: Some(SignInToken {
                    env: "CLAUDE_CODE_OAUTH_TOKEN".into(),
                    prefix: "sk-ant-oat".into(),
                    days: 365,
                }),
            },
            "codex" => Self {
                // Codex's message reads "You've hit your usage limit ... try again at ...".
                limit_phrases: words(&["you've hit your", "you’ve hit your", "usage limit reached"]),
                rules_file: Some("AGENTS.md".into()),
                auth_phrases: words(&[
                    "refresh token was already used",
                    "refresh_token_reused",
                    "refresh token has expired",
                    "token_expired",
                    "please log in again",
                    "sign in again",
                ]),
                sign_in: Some("codex login --device-auth".into()),
                ..Self::default()
            },
            _ => Self::default(),
        }
    }

    /// These traits, with what is unset taken from `other`.
    pub fn or(self, other: Self) -> Self {
        Self {
            usage_probe: self.usage_probe.or(other.usage_probe),
            limit_phrases: self.limit_phrases.or(other.limit_phrases),
            rules_file: self.rules_file.or(other.rules_file),
            review_models: self.review_models.or(other.review_models),
            prices: self.prices.or(other.prices),
            auth_phrases: self.auth_phrases.or(other.auth_phrases),
            sign_in: self.sign_in.or(other.sign_in),
            sign_in_token: self.sign_in_token.or(other.sign_in_token),
            session_log: self.session_log.or(other.session_log),
            terminal_new: self.terminal_new.or(other.terminal_new),
            mcp_instructions: self.mcp_instructions.or(other.mcp_instructions),
            mcp_http: self.mcp_http.or(other.mcp_http),
            mcp_token_in_url: self.mcp_token_in_url.or(other.mcp_token_in_url),
        }
    }

    /// The rules file, `CLAUDE.md` for an engine that names none, as Hub always wrote.
    pub fn rules_file(&self) -> &str {
        self.rules_file.as_deref().unwrap_or("CLAUDE.md")
    }
}

/// Claude Code's sessions, under `CLAUDE_CONFIG_DIR` when that is set.
fn claude_session_log() -> String {
    match std::env::var("CLAUDE_CONFIG_DIR") {
        Ok(dir) if !dir.trim().is_empty() => format!("{}/projects/*/{{session}}.jsonl", dir.trim_end_matches(['/', '\\'])),
        _ => "~/.claude/projects/*/{session}.jsonl".into(),
    }
}

/// The more specific id goes first, since `claude-opus-5` is a prefix of `claude-opus-5-5`.
pub(crate) fn claude_prices() -> Vec<Price> {
    [
        ("claude-fable-5-1", 10.0, 50.0, 0.25),
        ("claude-fable-5", 10.0, 50.0, 1.0),
        ("claude-mythos-5", 10.0, 50.0, 1.0),
        ("claude-opus-5-5", 4.0, 20.0, 0.2),
        ("claude-opus-5", 5.0, 25.0, 0.5),
        ("claude-opus-4-8", 5.0, 25.0, 0.5),
        ("claude-opus-4-7", 5.0, 25.0, 0.5),
        ("claude-opus-4-6", 5.0, 25.0, 0.5),
        ("claude-opus-4-5", 5.0, 25.0, 0.5),
        ("claude-opus-4", 15.0, 75.0, 1.5),
        ("claude-sonnet-5", 2.0, 10.0, 0.2),
        ("claude-sonnet-4", 3.0, 15.0, 0.3),
        ("claude-3-7-sonnet", 3.0, 15.0, 0.3),
        ("claude-haiku-5-5", 0.1, 0.5, 0.01),
        ("claude-haiku-4-5", 1.0, 5.0, 0.1),
        ("claude-3-5-haiku", 0.8, 4.0, 0.08),
    ]
    .into_iter()
    .map(|(model, input, output, cache_read)| Price { model: model.into(), input, output, cache_read })
    .collect()
}

impl Config {
    /// The traits of a configured engine, its own settings first, then the preset for its name.
    pub fn traits(&self, engine: &str) -> EngineTraits {
        let own = self.engines.get(engine).map(|e| e.traits.clone()).unwrap_or_default();
        own.or(EngineTraits::preset(engine))
    }

    /// Whether `engine` runs on a subscription: one with plan limits, which `limit_phrases`
    /// recognises. A pay-per-token engine has none. Hub moves a thread between subscription
    /// engines when a limit runs out, never to one that bills per use.
    pub fn is_subscription(&self, engine: &str) -> bool {
        self.engines.contains_key(engine) && self.traits(engine).limit_phrases.is_some_and(|p| p.iter().any(|w| !w.trim().is_empty()))
    }

    /// The engine whose plan limits Hub reads, with the prompt that reads them: the default
    /// engine, when it has a usage probe.
    pub fn plan_engine(&self) -> Option<(&str, &EngineConfig, String)> {
        let engine = self.engines.get(&self.default_engine)?;
        let probe = self.traits(&self.default_engine).usage_probe.filter(|p| !p.trim().is_empty())?;
        Some((self.default_engine.as_str(), engine, probe))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Driver {
    /// An Agent Client Protocol agent on stdio.
    #[default]
    Acp,
}

impl Default for Config {
    fn default() -> Self {
        // `npx` is a batch file on Windows and has to be named with its extension.
        let npx = if cfg!(windows) { "npx.cmd" } else { "npx" };
        let mut engines = BTreeMap::new();
        engines.insert(
            "claude".into(),
            EngineConfig {
                command: format!("{npx} -y @agentclientprotocol/claude-agent-acp@0.107"),
                driver: Driver::Acp,
                secrets: Vec::new(),
                terminal: Some("claude --resume {session}".into()),
                harness: Some("claude".into()),
                ..EngineConfig::default()
            },
        );
        engines.insert(
            "codex".into(),
            EngineConfig {
                command: format!("{npx} -y @agentclientprotocol/codex-acp@2.7"),
                driver: Driver::Acp,
                secrets: Vec::new(),
                terminal: Some("codex resume {session}".into()),
                harness: Some("codex".into()),
                ..EngineConfig::default()
            },
        );
        Self {
            data_dir: default_data_dir(),
            bind: SocketAddr::from(([127, 0, 0, 1], 7878)),
            default_engine: "claude".into(),
            engines,
            allowed_origins: vec![
                "tauri://localhost".into(),
                "http://tauri.localhost".into(),
                "http://localhost:1420".into(),
                // Vite listens on 127.0.0.1 (vite.config.ts).
                "http://127.0.0.1:1420".into(),
            ],
            update: UpdateConfig::default(),
            tidy: TidyConfig::default(),
            voice: Default::default(),
            checkpoints: true,
            open: OpenConfig::default(),
            agent_tools: true,
            name: None,
            hosts: Vec::new(),
            tmux: true,
            resume_turns: true,
            keep_engines: true,
            keeper: None,
            keep_awake: true,
            auto_pull: true,
            live_output: true,
            // Codex draws with the ChatGPT plan's image tool; Claude has none.
            side_jobs: BTreeMap::from([("image".into(), "codex".into())]),
            customs_url: None,
            trace_engines: false,
            keys_in_memory: false,
            relay_keys: false,
        }
    }
}

/// `hub.json` in the data folder, or the path in `HUB_CONFIG`.
pub fn config_path() -> PathBuf {
    std::env::var_os("HUB_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_data_dir().join("hub.json"))
}

/// One lock for every change to `hub.json`: Settings, Add engine and engines shared from other
/// machines all read the file, change one part and write it back, and two of those at once must
/// not lose each other's part.
static FILE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// What went wrong with `hub.json` at start, when Hub runs on defaults because of it.
static PROBLEM: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Why Hub runs on default settings although `hub.json` exists, if that is so.
pub fn problem() -> Option<String> {
    PROBLEM.get().cloned()
}

/// The settings file as JSON: an empty object when there is no file or it is empty, an error
/// when it is not valid JSON.
pub fn read_file() -> anyhow::Result<serde_json::Value> {
    read_json(&config_path())
}

fn read_json(path: &Path) -> anyhow::Result<serde_json::Value> {
    if !path.exists() {
        return Ok(serde_json::Value::Object(Default::default()));
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(serde_json::Value::Object(Default::default()));
    }
    serde_json::from_str(&text).map_err(|e| anyhow::anyhow!("{} is not valid JSON: {e}", path.display()))
}

/// Writes the settings file whole: to a file beside it first, then moved into place, so a crash
/// or a power cut while writing leaves the old file, never a cut-off one that stops Hub from
/// starting.
pub fn write_file(file: &serde_json::Value) -> anyhow::Result<()> {
    write_json(&config_path(), file)
}

fn write_json(path: &Path, file: &serde_json::Value) -> anyhow::Result<()> {
    let dir = path.parent().context("the settings file has no folder")?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = dir.join(format!(
        "{}.{}.tmp",
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "hub.json".into()),
        uuid::Uuid::new_v4().simple()
    ));
    let text = serde_json::to_string_pretty(file)? + "\n";
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut f, text.as_bytes())?;
        f.sync_all()
    })();
    if let Err(e) = written {
        crate::locked::clean_file(&tmp);
        return Err(e).with_context(|| format!("writing {}", tmp.display()));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        crate::locked::clean_file(&tmp);
        return Err(e).with_context(|| format!("moving {} into place", path.display()));
    }
    Ok(())
}

/// Reads `hub.json`, lets `f` change it, checks that the result still reads as a `Config` and
/// writes it back, all under the file's lock. Nothing is written when `f` or the check fails.
pub fn edit_file(f: impl FnOnce(&mut serde_json::Value) -> anyhow::Result<()>) -> anyhow::Result<()> {
    let _held = FILE.lock().unwrap_or_else(|p| p.into_inner());
    let mut file = read_file()?;
    f(&mut file)?;
    serde_json::from_value::<Config>(file.clone()).map_err(|e| anyhow::anyhow!("not a valid value: {e}"))?;
    write_file(&file)
}

/// The settings at start: `hub.json`, or the defaults when there is none. A file that cannot be
/// read is moved aside as `hub.json.broken-<time>` and Hub starts on defaults, with the reason
/// in `problem()` and the log, instead of not starting at all.
pub fn load() -> Config {
    let (config, problem) = load_from(&config_path());
    if let Some(problem) = problem {
        tracing::error!("{problem}");
        PROBLEM.set(problem).ignore("load runs once per process; a second problem would keep the first");
    }
    config
}

fn load_from(path: &Path) -> (Config, Option<String>) {
    match read_json(path).and_then(|v| serde_json::from_value::<Config>(v).map_err(|e| anyhow::anyhow!("{} does not read as settings: {e}", path.display()))) {
        Ok(config) => (config, None),
        Err(e) => {
            let aside = path.with_file_name(format!(
                "{}.broken-{}",
                path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "hub.json".into()),
                crate::model::now_ms()
            ));
            let moved = std::fs::rename(path, &aside);
            let message = match moved {
                Ok(()) => format!("{e:#}. It is kept as {}; Hub runs on default settings until it is put right.", aside.display()),
                Err(m) => format!("{e:#}. It could not be moved aside ({m}); Hub runs on default settings."),
            };
            (Config::default(), Some(message))
        }
    }
}

fn default_data_dir() -> PathBuf {
    hub_install::data_dir()
}

mod update;

#[cfg(test)]
mod tests;
