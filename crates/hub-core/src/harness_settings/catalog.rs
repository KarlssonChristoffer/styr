//! The settings catalog: for each harness Hub can change settings of, the commands of its own
//! config and the settings Hub shows, as data. It names the harnesses and their settings, so it is
//! an engine file (`ENGINE_FILES` in `tests/it/rules.rs`); the rest of `harness_settings.rs` reads
//! and writes through it without knowing any harness. It stays in this module, not in `engines`,
//! so `harness_settings` still does not need the engine catalog.

use serde_json::{Value, json};

use super::{Commands, HarnessSetting, SettingChoice, SettingKind, choice};

fn setting(key: &str, label: &str, what: &str, kind: SettingKind, choices: Vec<SettingChoice>, default: Value) -> HarnessSetting {
    HarnessSetting { key: key.into(), label: label.into(), what: what.into(), kind, choices, default: default.clone(), value: default, shared: false, thinking: false }
}

fn thinking(mut s: HarnessSetting) -> HarnessSetting {
    s.thinking = true;
    s
}

/// What Hub knows of each harness's settings: its config commands and the settings it shows.
pub(super) fn catalog(harness: &str) -> Option<(Commands, Vec<HarnessSetting>)> {
    use SettingKind::*;
    match harness {
        // https://hermes-agent.nousresearch.com/docs/user-guide/configuration
        "hermes" => Some((
            Commands { get: "config get {key} --json", set: "config set {key} {value}", unset: "config unset {key}" },
            vec![
                thinking(setting(
                    "agent.reasoning_effort",
                    "Thinking when a thread picks none",
                    "How hard Hermes thinks. A thread can pick its own when Hermes offers it (newer Hermes does).",
                    Choice,
                    vec![
                        choice(json!(""), "Model's default", "What the provider does by itself"),
                        choice(json!("none"), "Off", "Answers at once"),
                        choice(json!("low"), "Low", "Thinks briefly"),
                        choice(json!("medium"), "Medium", "Some thinking"),
                        choice(json!("high"), "High", "Thinks it through"),
                        choice(json!("max"), "Max", "Thinks longest"),
                    ],
                    json!(""),
                )),
                setting(
                    "agent.disabled_toolsets",
                    "Tools",
                    "What Hermes may use in Hub's threads. Click one to turn it off or on.",
                    OffList,
                    vec![
                        choice(json!("file"), "Files", "Read, write and search files"),
                        choice(json!("terminal"), "Terminal", "Run commands"),
                        choice(json!("web"), "Web", "Search and read web pages"),
                        choice(json!("browser"), "Browser", "Drive a real browser (needs Chromium)"),
                        choice(json!("code_execution"), "Run code", "Python scripts that call tools"),
                        choice(json!("delegation"), "Helpers", "Hand parts of a task to sub-agents"),
                        choice(json!("memory"), "Memory", "Notes it keeps between threads"),
                        choice(json!("skills"), "Skills", "Load and write its skills"),
                        choice(json!("session_search"), "Past chats", "Search earlier conversations"),
                        choice(json!("vision"), "Pictures", "Look at images"),
                        choice(json!("todo"), "To-dos", "Plan multi-step work"),
                    ],
                    json!([]),
                ),
                setting("memory.memory_enabled", "Memory", "Hermes keeps notes between threads and reads them at the start of each.", Toggle, vec![], json!(true)),
                setting("memory.user_profile_enabled", "Profile of you", "It also keeps a short profile of you and how you work.", Toggle, vec![], json!(true)),
                setting("memory.write_approval", "Ask before writing memory", "Memory it wants to keep waits for your yes (/memory).", Toggle, vec![], json!(false)),
                setting("skills.write_approval", "Ask before writing skills", "Skills it writes or changes wait for your yes (/skills).", Toggle, vec![], json!(false)),
                setting("compression.enabled", "Compress long conversations", "Summarizes the older part of a thread when it gets long.", Toggle, vec![], json!(true)),
                setting(
                    "compression.threshold",
                    "Compress at",
                    "How full the context gets before it compresses.",
                    Choice,
                    vec![
                        choice(json!(0.3), "30%", "Early: cheaper, forgets detail sooner"),
                        choice(json!(0.5), "50%", "Hermes's default"),
                        choice(json!(0.7), "70%", "Later"),
                        choice(json!(0.85), "85%", "Only when nearly full"),
                    ],
                    json!(0.5),
                ),
                setting(
                    "agent.max_turns",
                    "Most steps per message",
                    "Tool calls Hermes may make for one message before it stops.",
                    Choice,
                    vec![
                        choice(Value::Null, "No limit", "Works until it is done"),
                        choice(json!(30), "30", ""),
                        choice(json!(60), "60", ""),
                        choice(json!(90), "90", ""),
                        choice(json!(150), "150", ""),
                    ],
                    Value::Null,
                ),
            ],
        )),
        _ => None,
    }
}
