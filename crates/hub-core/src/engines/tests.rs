//! Tests of the engine presets.

#[test]
fn only_hermes_shows_the_tool_token_in_the_address() {
    for h in harnesses() {
        assert_eq!(h.mcp_token_in_url, h.id == "hermes", "{}", h.id);
    }
}

#[test]
fn only_hermes_is_given_hub_tools_without_saying_it_takes_them() {
    for h in harnesses() {
        assert_eq!(h.mcp_http, h.id == "hermes", "{}", h.id);
    }
}

use super::*;

fn shared(name: &str) -> SharedEngine {
    let draft = EngineDraft { harness: "hermes".into(), provider: "deepseek".into(), model: "deepseek-chat".into(), ..Default::default() };
    let (_, config, _) = build(&draft).unwrap();
    SharedEngine { name: name.into(), config, keys: BTreeMap::new() }
}

#[test]
fn engines_from_the_pc_land_once_and_leave_with_it() {
    let mut file = serde_json::json!({});
    assert!(merge_shared(&mut file, "PC", vec![shared("hermes-deepseek")]).unwrap());
    let got: Config = serde_json::from_value(file.clone()).unwrap();
    assert_eq!(got.engines["hermes-deepseek"].shared_from.as_deref(), Some("PC"));
    // The built-in engines stay.
    assert!(got.engines.contains_key("claude"));
    // Sending the same again changes nothing, so the core is not restarted.
    assert!(!merge_shared(&mut file, "PC", vec![shared("hermes-deepseek")]).unwrap());
    // Removed on the PC: removed here.
    assert!(merge_shared(&mut file, "PC", vec![]).unwrap());
    let got: Config = serde_json::from_value(file).unwrap();
    assert!(!got.engines.contains_key("hermes-deepseek"));
}

#[test]
fn a_relayed_engine_is_not_taken_back_by_the_machine_that_made_it() {
    let mut relayed = shared("hermes-deepseek");
    relayed.config.shared_from = Some("PC".into());
    assert!(comes_back(&relayed, "PC"));
    assert!(!comes_back(&relayed, "vps"));
    assert!(!comes_back(&shared("hermes-deepseek"), "PC"));
}

#[test]
fn engines_added_by_hand_are_not_overwritten() {
    let mut file = serde_json::json!({});
    let mut mine = shared("hermes-deepseek").config;
    mine.label = Some("Mine".into());
    let mut list = engines_in(&file);
    list.insert("hermes-deepseek".into(), serde_json::to_value(&mine).unwrap());
    file["engines"] = Value::Object(list);
    assert!(!merge_shared(&mut file, "PC", vec![shared("hermes-deepseek"), shared("claude")]).unwrap());
    // Nor removed when the PC stops sending it.
    assert!(!merge_shared(&mut file, "PC", vec![]).unwrap());
    let got: Config = serde_json::from_value(file).unwrap();
    assert_eq!(got.engines["hermes-deepseek"].label.as_deref(), Some("Mine"));
    assert!(got.engines["hermes-deepseek"].shared_from.is_none());
}

#[test]
fn claude_code_runs_on_deepseek_as_an_engine_of_its_own() {
    let draft = EngineDraft { harness: "claude".into(), provider: "deepseek".into(), key_name: "DEEPSEEK".into(), ..Default::default() };
    let (name, engine, _) = build(&draft).unwrap();
    assert_eq!(name, "claude-deepseek");
    assert_eq!(engine.label.as_deref(), Some("Claude Code · DeepSeek"));
    assert_eq!(engine.provider.as_deref(), Some("deepseek"));
    // It starts on the provider's first model, named for every size Claude Code asks for.
    assert_eq!(engine.model.as_deref(), Some("deepseek-v4-pro"));
    assert_eq!(engine.env["ANTHROPIC_BASE_URL"], "https://api.deepseek.com/anthropic");
    assert_eq!(engine.env["ANTHROPIC_MODEL"], "deepseek-v4-pro");
    assert_eq!(engine.env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "deepseek-v4-pro");
    // The key goes where Claude Code reads it, by its vault name; the vault never holds that name.
    assert_eq!(engine.keys["ANTHROPIC_AUTH_TOKEN"], "DEEPSEEK");
    assert!(engine.secrets.is_empty());
    // The CLI on its own would not have the address or key, so no take over in a terminal.
    assert!(engine.terminal.is_none());
    // It is no subscription: a thread never moves to it when a plan limit runs out.
    let mut config = Config::default();
    config.engines.insert(name.clone(), engine);
    assert!(!config.is_subscription(&name));
    assert!(config.is_subscription("claude"));
    // A key pasted now is saved as DeepSeek's own name and handed out as the token.
    let draft = EngineDraft { harness: "claude".into(), provider: "deepseek".into(), key: "sk-test".into(), ..Default::default() };
    let (_, engine, given) = build(&draft).unwrap();
    assert_eq!(given, Some(("DEEPSEEK_API_KEY".into(), "sk-test".into())));
    assert_eq!(engine.keys["ANTHROPIC_AUTH_TOKEN"], "DEEPSEEK_API_KEY");
}

#[test]
fn claude_code_stays_on_its_subscription_and_runs_only_on_what_is_paired() {
    // On its own login it has no environment from Hub.
    let (_, own, _) = build(&EngineDraft { harness: "claude".into(), name: "claude2".into(), ..Default::default() }).unwrap();
    assert!(own.env.is_empty() && own.keys.is_empty() && own.provider.is_none());
    // OpenRouter is not paired with it, and a provider the catalog lacks has no address for it.
    assert!(build(&EngineDraft { harness: "claude".into(), provider: "openrouter".into(), ..Default::default() }).is_err());
    let other = EngineDraft { harness: "claude".into(), provider: "other".into(), provider_id: "x".into(), secret: "X_KEY".into(), ..Default::default() };
    assert!(build(&other).is_err());
    // The matrix: a subscription's provider is never paired, Hermes runs on keys by picking.
    let all = harnesses();
    let by = |id: &str| all.iter().find(|h| h.id == id).unwrap();
    assert!(by("claude").paired("deepseek") && !by("claude").runs_on("openrouter"));
    assert!(by("hermes").runs_on("deepseek") && !by("hermes").paired("deepseek"));
    assert!(by("codex").pairs.is_empty() && by("gemini").pairs.is_empty());
}

#[test]
fn hermes_on_deepseek_is_one_engine_entry() {
    let draft = EngineDraft {
        harness: "hermes".into(),
        provider: "deepseek".into(),
        model: "deepseek-chat".into(),
        key: "sk-test".into(),
        ..Default::default()
    };
    let (name, engine, key) = build(&draft).unwrap();
    assert_eq!(name, "hermes-deepseek");
    // Where Hermes is installed (as on Christoffer's PC) the command names its full path.
    assert_eq!(engine.command, resolve("hermes acp").unwrap_or_else(|| "hermes acp".into()));
    assert_eq!(engine.secrets, ["DEEPSEEK_API_KEY"]);
    assert_eq!(engine.model.as_deref(), Some("deepseek:deepseek-chat"));
    assert_eq!(engine.env["HERMES_INFERENCE_PROVIDER"], "deepseek");
    assert_eq!(engine.env["HERMES_MODEL"], "deepseek-chat");
    assert_eq!(engine.label.as_deref(), Some("Hermes · DeepSeek"));
    assert_eq!(engine.traits.rules_file.as_deref(), Some("AGENTS.md"));
    assert_eq!(key, Some(("DEEPSEEK_API_KEY".into(), "sk-test".into())));
    // What is written to hub.json reads back as the same engine, and holds no key.
    let text = serde_json::to_string(&engine).unwrap();
    assert!(!text.contains("sk-test"));
    let back: EngineConfig = serde_json::from_str(&text).unwrap();
    assert!(same(&back, &engine));
}

#[test]
fn opencode_names_models_with_a_slash() {
    let draft = EngineDraft { harness: "opencode".into(), provider: "openrouter".into(), model: "moonshotai/kimi-k2".into(), ..Default::default() };
    let (name, engine, _) = build(&draft).unwrap();
    assert_eq!(name, "opencode-openrouter");
    assert_eq!(engine.model.as_deref(), Some("openrouter/moonshotai/kimi-k2"));
    assert_eq!(engine.env["OPENCODE_CONFIG_CONTENT"], r#"{"model":"openrouter/moonshotai/kimi-k2"}"#);
}

#[test]
fn subscription_keys_and_unknown_pairs_are_refused() {
    let other = EngineDraft { harness: "hermes".into(), provider: "other".into(), provider_id: "anthropic".into(), secret: "ANTHROPIC_API_KEY".into(), ..Default::default() };
    assert!(build(&other).unwrap_err().to_string().contains("subscription"));
    let wrong = EngineDraft { harness: "gemini".into(), provider: "deepseek".into(), ..Default::default() };
    assert!(build(&wrong).is_err());
    let own = EngineDraft { harness: "gemini".into(), ..Default::default() };
    let (name, engine, key) = build(&own).unwrap();
    assert_eq!((name.as_str(), engine.secrets.len(), key), ("gemini", 0, None));
}

#[test]
fn a_provider_the_catalog_lacks_is_added_by_hand() {
    let draft = EngineDraft {
        harness: "hermes".into(),
        provider: "other".into(),
        provider_id: "kimi-coding".into(),
        secret: "KIMI_API_KEY".into(),
        model: "kimi-k2".into(),
        ..Default::default()
    };
    let (name, engine, _) = build(&draft).unwrap();
    assert_eq!(name, "hermes-kimi-coding");
    assert_eq!(engine.model.as_deref(), Some("kimi-coding:kimi-k2"));
    assert_eq!(engine.secrets, ["KIMI_API_KEY"]);
}

#[test]
fn a_key_saved_under_another_name_is_found_and_handed_out_as_the_harness_reads_it() {
    let vault: Vec<crate::model::SecretInfo> = ["TYPESAFE_API_KEY", "DEEPSEEK", "OLD_DEEPSEEK_KEY", "MOONSHOT_KEY"]
        .iter()
        .map(|n| crate::model::SecretInfo { name: n.to_string(), hint: "…efa8".into() })
        .collect();
    let deepseek = providers().into_iter().find(|p| p.id == "deepseek").unwrap();
    let found: Vec<String> = matching_keys(&deepseek, &vault).into_iter().map(|m| m.name).collect();
    assert_eq!(found, ["DEEPSEEK", "OLD_DEEPSEEK_KEY"]);
    let kimi = providers().into_iter().find(|p| p.id == "kimi").unwrap();
    assert_eq!(matching_keys(&kimi, &vault)[0].name, "MOONSHOT_KEY");

    let draft = EngineDraft { harness: "hermes".into(), provider: "deepseek".into(), key_name: "DEEPSEEK".into(), ..Default::default() };
    let (_, engine, given) = build(&draft).unwrap();
    assert!(engine.secrets.is_empty() && given.is_none());
    assert_eq!(engine.keys["DEEPSEEK_API_KEY"], "DEEPSEEK");
    assert_eq!(engine.key_names(), ["DEEPSEEK"]);
    // A key pasted now wins over the one picked from the vault.
    let pasted = EngineDraft { key: "sk-new".into(), ..draft };
    let (_, engine, given) = build(&pasted).unwrap();
    assert!(engine.keys.is_empty());
    assert_eq!(given, Some(("DEEPSEEK_API_KEY".into(), "sk-new".into())));
}

#[test]
fn installer_output_loses_its_colours() {
    assert_eq!(plain("\x1b[1;32m✓ Installed\x1b[0m hermes\r"), "✓ Installed hermes");
}

#[cfg(unix)]
#[tokio::test]
async fn an_install_keeps_its_last_lines_and_fails_on_a_bad_exit() {
    JOBS.locked().insert("t".into(), InstallJob { harness: "t".into(), running: true, ..Default::default() });
    run_install("t", "echo one; echo two >&2").await.unwrap();
    let lines = JOBS.locked()["t"].lines.clone();
    assert!(lines.contains(&"one".to_string()) && lines.contains(&"two".to_string()), "{lines:?}");
    assert!(run_install("t", "exit 3").await.is_err());
    // Nothing can be typed into an installer: a question gets an empty answer.
    run_install("t", "read -r answer; echo \"got [$answer]\"").await.unwrap();
    assert!(JOBS.locked()["t"].lines.contains(&"got []".to_string()));
}

#[cfg(unix)]
#[test]
fn a_program_by_full_path_resolves_as_it_is() {
    let dir = std::env::temp_dir().join(format!("hub-resolve-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("agent");
    std::fs::write(&exe, "#!/bin/sh\n").unwrap();
    let command = format!("{} acp", exe.display());
    assert_eq!(resolve(&command).as_deref(), Some(command.as_str()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_try_that_panics_is_not_left_running() {
    CHECKING.locked().insert("panics-while-tried".into());
    let _ = std::panic::catch_unwind(|| {
        let _checking = Checking("panics-while-tried".into());
        panic!("the try went wrong");
    });
    assert!(!CHECKING.locked().contains("panics-while-tried"));
    // Engines added by hand under a shared name are tried too; the built-in ones are not.
    assert!(tried_here("hermes-deepseek"));
    assert!(!tried_here("claude"));
}

#[test]
fn a_missing_program_is_not_installed() {
    assert!(!installed("surely-not-a-program-hub-knows acp"));
    assert!(!installed(""));
}

#[test]
fn modes_that_ask_nothing_are_told_apart() {
    for m in ["bypassPermissions", "full-access", "danger-full-access", "yolo"] {
        assert!(super::asks_nothing(m), "{m}");
    }
    for m in ["default", "plan", "acceptEdits", "read-only", "auto"] {
        assert!(!super::asks_nothing(m), "{m}");
    }
}
