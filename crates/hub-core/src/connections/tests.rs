//! Guards the connections list and the engine launch: Claude's and Codex's lists and files are
//! read right, tool calls land on their connection, what is switched off reaches the engine
//! (Codex plugins as overrides), a chat starts Claude without the user's setup, and the engines'
//! wire names are read in one place.

use super::*;
use serde_json::json;

#[test]
fn extra_folders_join_claude_codes_options_and_nothing_else() {
    let mut prefs = ConnectionPrefs::default();
    prefs.off.insert("claude".into(), vec!["Trello".into()]);
    let mut claude = launch(&prefs, "claude", "claude-agent-acp");
    add_dirs(&mut claude, "claude-agent-acp", &[PathBuf::from("/srv/atlas")]);
    let options = &claude.session_meta.unwrap()["claudeCode"]["options"];
    assert_eq!(options["additionalDirectories"], json!(["/srv/atlas"]));
    assert_eq!(options["disallowedTools"], json!(["mcp__Trello"]));
    let mut codex = launch(&prefs, "codex", "codex-acp");
    add_dirs(&mut codex, "codex-acp", &[PathBuf::from("/srv/atlas")]);
    assert!(codex.session_meta.is_none());
    let mut none = launch(&prefs, "claude", "claude-agent-acp");
    add_dirs(&mut none, "claude-agent-acp", &[]);
    assert!(none.session_meta.unwrap()["claudeCode"]["options"].get("additionalDirectories").is_none());
}

#[test]
fn reads_claude_mcp_list() {
    let text = "Checking MCP server health…\n\n\
        claude.ai ElevenLabs: https://mcp.elevenlabs.io/mcp?key=abc (HTTP) - ✓ Connected\n\
        claude.ai Gmail: https://gmail.mcp.claude.com/mcp (HTTP) - ! Needs authentication\n\
        plugin:engineering:slack: https://mcp.slack.com/mcp (HTTP) - ✗ Failed to connect — ERR_PROXY_TUNNEL: 403\n\
        playwright: npx @playwright/mcp@latest --api-key=s3cret - ✓ Connected\n\
        plugin:engineering:gmail:  (HTTP) - - Not configured\n";
    let list = parse_claude_list(text, "claude");
    assert_eq!(list.len(), 5);
    let eleven = &list[0];
    assert_eq!((eleven.label.as_str(), eleven.source, eleven.state), ("ElevenLabs", ConnectionSource::ClaudeAi, ConnectionState::Connected));
    assert_eq!(eleven.target.as_deref(), Some("https://mcp.elevenlabs.io/mcp"));
    assert_eq!(eleven.manage_url.as_deref(), Some(CLAUDE_AI_CONNECTORS));
    assert_eq!(list[1].state, ConnectionState::NeedsLogin);
    let slack = &list[2];
    assert_eq!((slack.label.as_str(), slack.source, slack.state), ("slack", ConnectionSource::Plugin, ConnectionState::Failed));
    assert_eq!(slack.detail.as_deref(), Some("ERR_PROXY_TUNNEL: 403"));
    assert_eq!(list[3].target.as_deref(), Some("npx @playwright/mcp@latest"));
    assert_eq!(list[3].transport.as_deref(), Some("stdio"));
    assert_eq!(list[4].state, ConnectionState::Off);
}

#[test]
fn finds_the_server_of_a_tool_call() {
    let none = json!({});
    assert_eq!(
        mcp_tool(&["Generate image", "mcp__claude_ai_ElevenLabs__creative_generate_image"], &none),
        Some(("claude_ai_ElevenLabs".into(), "creative_generate_image".into()))
    );
    assert_eq!(mcp_tool(&["Tool: github/create_issue"], &none), Some(("github".into(), "create_issue".into())));
    assert_eq!(
        mcp_tool(&["Search"], &json!({ "server": "docs", "tool": "search", "arguments": {} })),
        Some(("docs".into(), "search".into()))
    );
    assert_eq!(mcp_tool(&["Bash", "Read file"], &none), None);
    assert_eq!(key("claude.ai ElevenLabs"), "claude_ai_ElevenLabs");
    assert_eq!(key("plugin:engineering:slack"), "plugin_engineering_slack");
}

#[test]
fn calls_land_on_their_connection() {
    let mut list = parse_claude_list("claude.ai ElevenLabs: https://x.io/mcp (HTTP) - ✓ Connected\nother: https://y.io (HTTP) - ✓ Connected", "claude");
    let call = |t: &str, server: &str, tool: &str, ts| Call { thread_id: t.into(), engine: "claude".into(), ts, server: server.into(), tool: tool.into() };
    let calls = [
        call("a", "claude_ai_ElevenLabs", "creative_generate_image", 1),
        call("a", "claude_ai_ElevenLabs", "creative_get_flow_run_status", 2),
        call("b", "claude_ai_ElevenLabs", "creative_get_flow_run_status", 3),
        call("b", "old_server", "thing", 4),
        Call { engine: "codex".into(), ..call("c", "other", "x", 5) },
    ];
    merge("claude", &mut list, &calls);
    let eleven = &list[0];
    assert_eq!((eleven.label.as_str(), eleven.uses, eleven.threads, eleven.last_used), ("ElevenLabs", 3, 2, Some(3)));
    assert_eq!(eleven.tools[0].name, "creative_get_flow_run_status");
    let old = list.iter().find(|c| c.name == "old_server").unwrap();
    assert_eq!(old.source, ConnectionSource::Seen);
    assert_eq!(list.iter().find(|c| c.name == "other").unwrap().uses, 0);
}

#[test]
fn switched_off_reaches_the_claude_engine() {
    let mut prefs = ConnectionPrefs::default();
    let claude = "npx -y @agentclientprotocol/claude-agent-acp@0.107";
    assert_eq!(launch(&prefs, "claude", claude), Launch::default());
    prefs.off.insert("claude".into(), vec!["claude.ai ElevenLabs".into()]);
    prefs.account_off.push("claude".into());
    prefs.plugins_off.insert("claude".into(), vec!["engineering@synced".into()]);
    let l = launch(&prefs, "claude", claude);
    assert_eq!(l.env, vec![("ENABLE_CLAUDEAI_MCP_SERVERS".to_owned(), "false".to_owned())]);
    let meta = Value::Object(l.session_meta.unwrap());
    assert_eq!(meta["claudeCode"]["options"]["disallowedTools"], json!(["mcp__claude_ai_ElevenLabs"]));
    assert_eq!(meta["claudeCode"]["options"]["settings"]["enabledPlugins"], json!({ "engineering@synced": false }));
    // Another engine is left alone.
    assert_eq!(launch(&prefs, "other", claude), Launch::default());
}

#[test]
fn a_chat_starts_claude_without_the_users_setup() {
    let claude = "npx -y @agentclientprotocol/claude-agent-acp@0.107";
    assert!(is_chat_folder(r"D:\Dev", r"D:\Dev\chats\2026-11-02-172757"));
    assert!(is_chat_folder("d:/dev/", r"D:\DEV\Chats\2026-11-02-172757\"));
    assert!(!is_chat_folder(r"D:\Dev", r"D:\Dev\scratch\2026-11-02-172757"), "no project is not a chat");
    assert!(!is_chat_folder(r"D:\Dev", r"D:\Dev\chats"), "the folder of chats is no chat");
    assert!(!is_chat_folder(r"D:\Dev", r"D:\Dev\hub\chats\x"), "only the projects root's own chats");
    assert!(!is_chat_folder("", "/chats/x"));
    assert!(is_scratch_folder(r"D:\Dev", r"D:\Dev\scratch\2026-11-02-172757"));
    assert!(!is_scratch_folder(r"D:\Dev", r"D:\Dev\chats\2026-11-02-172757"));
    assert!(!is_scratch_folder(r"D:\Dev", r"D:\Dev\hub"));
    let made = crate::projects_root::is_made_for_the_person;
    assert!(made(r"D:\Dev", r"D:\Dev\chats\2026-11-02-172757") && made(r"D:\Dev", r"D:\Dev\scratch\2026-11-02-172757"));
    assert!(!made(r"D:\Dev", r"D:\Dev\hub"), "a project's work is code");

    // What was switched off by hand stays in the meta beside it.
    let mut prefs = ConnectionPrefs::default();
    prefs.plugins_off.insert("claude".into(), vec!["x@y".into()]);
    let mut l = launch(&prefs, "claude", claude);
    lean(&mut l, claude, None);
    assert_eq!(l.env, vec![("ENABLE_CLAUDEAI_MCP_SERVERS".to_owned(), "false".to_owned())]);
    let meta = Value::Object(l.session_meta.unwrap());
    assert_eq!(meta["claudeCode"]["options"]["strictMcpConfig"], json!(true));
    assert_eq!(meta["claudeCode"]["options"]["settingSources"], json!([]));
    assert_eq!(meta["claudeCode"]["options"]["settings"]["enabledPlugins"], json!({ "x@y": false }));
    assert!(meta["claudeCode"]["options"].get("model").is_none(), "no model of the user's: Claude Code's own");
    assert_eq!(meta["systemPrompt"]["append"], json!(crate::tool_instructions::CHAT_STYLE), "a chat is told how to talk");

    // The model the user's settings name still goes along, so the chat runs what it reports.
    let mut l = Launch::default();
    lean(&mut l, claude, Some("sonnet"));
    assert_eq!(Value::Object(l.session_meta.unwrap())["claudeCode"]["options"]["model"], json!("sonnet"));

    // The account's connectors already off: one variable, not two.
    prefs.account_off.push("claude".into());
    let mut l = launch(&prefs, "claude", claude);
    lean(&mut l, claude, None);
    assert_eq!(l.env.len(), 1);

    // Other engines are left alone.
    let mut l = Launch::default();
    lean(&mut l, "codex-acp", Some("sonnet"));
    assert_eq!(l, Launch::default());
}

#[test]
fn reads_codex_plugins_from_its_cache() {
    let dir = tempfile::tempdir().unwrap();
    let cache = dir.path();
    let skill = |p: &str, front: &str| {
        let d = cache.join(p);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("SKILL.md"), front).unwrap();
    };
    skill("openai-curated-remote/figma/17.0.0/skills/figma-use", "---\nname: figma-use\ndescription: x\n---\n# Figma");
    skill("openai-curated-remote/figma/17.0.0/skills/spec", "---\nname: \"figma-spec\"\n---\n");
    skill("openai-primary-runtime/pdf/1.0/skills/pdf", "no front matter");
    std::fs::create_dir_all(cache.join("openai-bundled/plugin-install-AbC/sites/1/skills")).unwrap();
    let config = "[plugins.\"pdf@openai-primary-runtime\"]\nenabled = false\n";
    let plugins = read_codex_plugins(cache, config);
    let ids: Vec<_> = plugins.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["figma@openai-curated-remote", "pdf@openai-primary-runtime"]);
    assert_eq!(plugins[0].skills, ["figma:figma-spec", "figma:figma-use"]);
    assert_eq!(plugins[0].version.as_deref(), Some("17.0.0"));
    assert!(plugins[0].enabled);
    assert_eq!(plugins[1].skills, ["pdf:pdf"]);
    assert!(!plugins[1].enabled);
}

#[test]
fn codex_plugins_off_become_overrides() {
    let figma = EnginePlugin {
        id: "figma@openai-curated-remote".into(),
        version: None,
        scope: None,
        enabled: true,
        off: false,
        skills: vec!["figma:figma-use".into(), "figma:figma-spec".into()],
    };
    let args = codex_plugins_off(&["figma@openai-curated-remote".into(), "gone@x".into()], &[figma]);
    assert_eq!(
        args,
        [
            "-c",
            "plugins.figma@openai-curated-remote.enabled=false",
            "-c",
            "plugins.gone@x.enabled=false",
            "-c",
            "skills.config=[{name=\"figma:figma-use\",enabled=false},{name=\"figma:figma-spec\",enabled=false}]",
        ]
    );
    // The value is TOML Codex can read.
    let value: toml::Table = format!("v={}", args[5].trim_start_matches("skills.config=")).parse().unwrap();
    assert_eq!(value["v"][1]["name"].as_str(), Some("figma:figma-spec"));
    assert!(codex_plugins_off(&[], &[]).is_empty());
}

#[test]
fn switched_off_marks_the_connection() {
    let mut list = parse_claude_list("claude.ai ElevenLabs: https://x.io/mcp (HTTP) - ✓ Connected\nclaude.ai Gmail: https://g.io (HTTP) - ✓ Connected\nother: https://y.io (HTTP) - ✓ Connected", "claude");
    let mut prefs = ConnectionPrefs::default();
    prefs.off.insert("claude".into(), vec!["other".into()]);
    mark_off(&prefs, "claude", &mut list);
    assert_eq!(list.iter().map(|c| c.off).collect::<Vec<_>>(), [false, false, true]);
    prefs.account_off.push("claude".into());
    mark_off(&prefs, "claude", &mut list);
    assert_eq!(list.iter().map(|c| c.off).collect::<Vec<_>>(), [true, true, true]);
}

#[test]
fn knows_the_cli_by_its_program() {
    assert_eq!(kind("npx.cmd -y @agentclientprotocol/claude-agent-acp@0.107"), Kind::Claude);
    assert_eq!(kind("npx -y @agentclientprotocol/codex-acp@2.7"), Kind::Codex);
    assert_eq!(kind("/home/claude/hub/target/debug/mock-agent"), Kind::Other);
    assert_eq!(kind(r"C:\Users\claude\mock-agent.exe"), Kind::Other);
}

#[test]
fn reads_codex() {
    let raw = json!([
        { "name": "docs", "enabled": true, "transport": { "type": "streamable_http", "url": "https://docs.example/mcp?token=x" } },
        { "name": "fs", "enabled": false, "transport": { "type": "stdio", "command": "npx", "args": ["-y", "@mcp/fs", "--token", "x"] } }
    ]);
    let list = parse_codex_json(&raw, "codex");
    assert_eq!(list[0].target.as_deref(), Some("https://docs.example/mcp"));
    assert_eq!(list[1].target.as_deref(), Some("npx -y @mcp/fs"));
    assert_eq!(list[1].state, ConnectionState::Off);

    let toml = "model = \"x\"\n[mcp_servers.docs]\nurl = \"https://docs.example/mcp\"\n[mcp_servers.docs.env]\nKEY = \"y\"\n[mcp_servers.\"fs\"]\ncommand = \"npx\"\nenabled = false\n[other]\nurl = \"no\"\n";
    let list = parse_codex_toml(toml, "codex");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].target.as_deref(), Some("https://docs.example/mcp"));
    assert_eq!((list[1].name.as_str(), list[1].state), ("fs", ConnectionState::Off));
}

const CLAUDE: &str = "npx -y @agentclientprotocol/claude-agent-acp@0.107";

#[test]
fn nothing_asked_changes_nothing() {
    let mut l = Launch::default();
    apply_agent_prefs(&mut l, &crate::model::AgentPrefs::default(), CLAUDE);
    assert_eq!(l, Launch::default());
}

#[test]
fn claude_gets_compaction_and_style() {
    let prefs = crate::model::AgentPrefs { compact_at: Some(70), output_style: Some("Learning".into()), ..Default::default() };
    let mut l = launch(
        &crate::model::ConnectionPrefs {
            plugins_off: [("claude".to_owned(), vec!["x@y".to_owned()])].into(),
            ..Default::default()
        },
        "claude",
        CLAUDE,
    );
    apply_agent_prefs(&mut l, &prefs, CLAUDE);
    assert_eq!(l.env, vec![("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE".to_owned(), "70".to_owned())]);
    let meta = Value::Object(l.session_meta.unwrap());
    assert_eq!(meta["claudeCode"]["options"]["settings"]["outputStyle"], json!("Learning"));
    // What was switched off is still there.
    assert_eq!(meta["claudeCode"]["options"]["settings"]["enabledPlugins"], json!({ "x@y": false }));

    let mut l = Launch::default();
    apply_agent_prefs(&mut l, &crate::model::AgentPrefs { auto_compact_off: true, compact_at: Some(70), ..Default::default() }, CLAUDE);
    assert_eq!(l.env, vec![("DISABLE_AUTO_COMPACT".to_owned(), "1".to_owned())]);
}

#[test]
fn other_engines_keep_their_own() {
    let mut l = Launch::default();
    apply_agent_prefs(&mut l, &crate::model::AgentPrefs { auto_compact_off: true, output_style: Some("Learning".into()), ..Default::default() }, "codex-acp");
    assert_eq!(l, Launch::default());
}

#[test]
fn the_engine_wire_names_are_read_in_one_place() {
    let call = json!({ "_meta": { "claudeCode": { "toolName": "Bash" } } });
    assert_eq!(call_tool_name(&call), Some("Bash"));
    assert_eq!(call_tool_name(&json!({})), None);
    assert_eq!(update_model(&json!({ "_meta": { "_claude/model": "m" } })).as_deref(), Some("m"));
    assert_eq!(update_model(&json!({})), None);
    assert!(mcp_add_command("http://h/mcp", "t").contains("http://h/mcp --header \"Authorization: Bearer t\""));
}
