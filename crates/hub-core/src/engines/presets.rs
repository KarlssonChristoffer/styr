//! The catalog itself, as data: every harness Hub knows (Claude Code, Codex, Hermes, OpenCode,
//! Gemini CLI: command, installer, how it takes a provider and model) and every provider (`providers`).
//! What is installed and which keys the vault holds is added on top in `setup.rs` (`catalog`).

use super::*;

pub(super) fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| s.to_string()).collect()
}

pub(super) fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// The harnesses Hub knows, before it looks at what is installed.
pub fn harnesses() -> Vec<Harness> {
    let keyed = strings(&["deepseek", "openrouter", "kimi", "zai", "xai"]);
    let defaults = Config::default();
    let builtin = |name: &str| defaults.engines.get(name).map(|e| e.command.clone()).unwrap_or_default();
    let mut all = vec![
        Harness {
            id: "claude".into(),
            label: "Claude Code".into(),
            what: "Anthropic's agent, on your Claude subscription.".into(),
            command: builtin("claude"),
            install: None,
            docs: Some("https://code.claude.com/docs".into()),
            own_login: true,
            login: Some("claude, then /login".into()),
            providers: Vec::new(),
            // Claude Code speaks Anthropic's API, which DeepSeek also offers; the engine that
            // pairs them points it there with DeepSeek's key and names the model for every size
            // Claude Code asks for (`{url}` is the provider's Anthropic address).
            model_format: "{model}".into(),
            env: env(&[
                ("ANTHROPIC_BASE_URL", "{url}"),
                ("ANTHROPIC_MODEL", "{model}"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "{model}"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "{model}"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "{model}"),
                ("ANTHROPIC_SMALL_FAST_MODEL", "{model}"),
                ("CLAUDE_CODE_SUBAGENT_MODEL", "{model}"),
                ("API_TIMEOUT_MS", "600000"),
                ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
            ]),
            rules_file: Some("CLAUDE.md".into()),
            key_file: None,
            terminal: Some("claude --resume {session}".into()),
            pairs: strings(&["deepseek"]),
            key_env: Some("ANTHROPIC_AUTH_TOKEN".into()),
            set_model: false,
            installed: false,
            cli: Some("claude".into()),
            package: Some("@anthropic-ai/claude-code".into()),
            self_update: Some("update".into()),
            changelog: Some("md:https://raw.githubusercontent.com/anthropics/claude-code/main/CHANGELOG.md".into()),
            settings: false,
            standby: None,
            standby_again: None,
            mcp_instructions: true,
            mcp_http: false,
            mcp_token_in_url: false,
        },
        Harness {
            id: "codex".into(),
            label: "Codex".into(),
            what: "OpenAI's agent, on your ChatGPT subscription.".into(),
            command: builtin("codex"),
            install: None,
            docs: Some("https://developers.openai.com/codex".into()),
            own_login: true,
            login: Some("codex login".into()),
            providers: Vec::new(),
            model_format: "{model}".into(),
            env: BTreeMap::new(),
            rules_file: Some("AGENTS.md".into()),
            key_file: None,
            terminal: Some("codex resume {session}".into()),
            pairs: Vec::new(),
            key_env: None,
            set_model: false,
            installed: false,
            cli: Some("codex".into()),
            package: Some("@openai/codex".into()),
            self_update: None,
            changelog: Some("github:openai/codex".into()),
            settings: false,
            standby: None,
            standby_again: None,
            mcp_instructions: false,
            mcp_http: false,
            mcp_token_in_url: false,
        },
    ];
    all.extend(keyed_harnesses(keyed));
    all
}

/// The engines that run on a key (several providers behind one command).
fn keyed_harnesses(keyed: Vec<String>) -> Vec<Harness> {
    vec![
        Harness {
            id: "hermes".into(),
            label: "Hermes".into(),
            what: "Nous Research's open agent. Runs on almost any provider, with memory and skills of its own.".into(),
            command: "hermes acp".into(),
            install: Some(if cfg!(windows) {
                "iex (irm https://hermes-agent.nousresearch.com/install.ps1)".into()
            } else {
                "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash".into()
            }),
            docs: Some("https://hermes-agent.nousresearch.com/docs/user-guide/features/acp".into()),
            own_login: true,
            login: Some("hermes model".into()),
            providers: keyed.clone(),
            model_format: "{provider}:{model}".into(),
            env: env(&[("HERMES_INFERENCE_PROVIDER", "{provider}"), ("HERMES_MODEL", "{model}")]),
            rules_file: Some("AGENTS.md".into()),
            // Hermes reads ~/.hermes/.env over its environment for every provider key.
            key_file: Some(".hermes/.env".into()),
            // An ACP session lives in Hermes's own session database, which its CLI reads.
            terminal: Some("hermes --resume {session}".into()),
            pairs: Vec::new(),
            key_env: None,
            set_model: true,
            installed: false,
            cli: Some("hermes".into()),
            package: None,
            self_update: Some("update".into()),
            changelog: Some("github:NousResearch/hermes-agent".into()),
            settings: false,
            standby: Some("gateway ensure --timeout 120".into()),
            standby_again: Some("gateway restart --detach".into()),
            mcp_instructions: false,
            mcp_http: true,
            mcp_token_in_url: true,
        },
        Harness {
            id: "opencode".into(),
            label: "OpenCode".into(),
            what: "Open source coding agent that works with most providers.".into(),
            command: "opencode acp".into(),
            install: Some("npm install -g opencode-ai".into()),
            docs: Some("https://opencode.ai/docs/acp/".into()),
            own_login: true,
            login: Some("opencode auth login".into()),
            providers: keyed,
            model_format: "{provider}/{model}".into(),
            env: env(&[("OPENCODE_CONFIG_CONTENT", r#"{"model":"{full}"}"#)]),
            rules_file: Some("AGENTS.md".into()),
            key_file: None,
            terminal: Some("opencode --session {session}".into()),
            pairs: Vec::new(),
            key_env: None,
            set_model: true,
            installed: false,
            cli: Some("opencode".into()),
            package: Some("opencode-ai".into()),
            self_update: Some("upgrade".into()),
            changelog: Some("github:sst/opencode".into()),
            settings: false,
            standby: None,
            standby_again: None,
            mcp_instructions: false,
            mcp_http: false,
            mcp_token_in_url: false,
        },
        Harness {
            id: "gemini".into(),
            label: "Gemini CLI".into(),
            what: "Google's agent, on your Google account.".into(),
            command: "gemini --experimental-acp".into(),
            install: Some("npm install -g @google/gemini-cli".into()),
            docs: Some("https://github.com/google-gemini/gemini-cli".into()),
            own_login: true,
            login: Some("gemini".into()),
            providers: Vec::new(),
            model_format: "{model}".into(),
            env: BTreeMap::new(),
            rules_file: Some("GEMINI.md".into()),
            key_file: None,
            terminal: None,
            pairs: Vec::new(),
            key_env: None,
            set_model: false,
            installed: false,
            cli: Some("gemini".into()),
            package: Some("@google/gemini-cli".into()),
            self_update: None,
            changelog: Some("github:google-gemini/gemini-cli".into()),
            settings: false,
            standby: None,
            standby_again: None,
            mcp_instructions: false,
            mcp_http: false,
            mcp_token_in_url: false,
        },
    ]
}

/// The providers Hub knows, before it looks in the vault.
pub fn providers() -> Vec<Provider> {
    let p = |id: &str, label: &str, what: &str, secret: &str, url: &str, models: &[&str], keys_at: &str| Provider {
        id: id.into(),
        label: label.into(),
        what: what.into(),
        secret: secret.into(),
        url: Some(url.into()),
        anthropic_url: None,
        models: strings(models),
        keys_at: Some(keys_at.into()),
        has_key: false,
        keys: Vec::new(),
        efforts: Vec::new(),
        aliases: Vec::new(),
    };
    let also = |mut p: Provider, words: &[&str]| {
        p.aliases = strings(words);
        p
    };
    let anthropic = |mut p: Provider, url: &str| {
        p.anthropic_url = Some(url.into());
        p
    };
    let efforts = |mut p: Provider, levels: &[(&str, &str, &str)], default: &str| {
        p.efforts = levels
            .iter()
            .map(|(value, name, what)| Effort { value: (*value).into(), name: (*name).into(), what: (*what).into(), default: *value == default })
            .collect();
        p
    };
    vec![
        // DeepSeek V4 thinks by default at high; it has three levels and maps every other
        // name onto them (https://api-docs.deepseek.com/guides/thinking_mode).
        anthropic(efforts(
            p(
                "deepseek",
                "DeepSeek",
                "DeepSeek's own API. Pay per token, cheap.",
                "DEEPSEEK_API_KEY",
                "https://api.deepseek.com",
                &["deepseek-v4-pro", "deepseek-flash"],
                "https://platform.deepseek.com/api_keys",
            ),
            &[
                ("none", "Off", "Answers at once: fastest and cheapest"),
                ("low", "Low", "Thinks briefly"),
                ("high", "High", "Thinks it through before answering"),
                ("max", "Max", "Thinks longest; best for agent work, most tokens"),
            ],
            "high",
        ), "https://api.deepseek.com/anthropic"),
        p(
            "openrouter",
            "OpenRouter",
            "One key for hundreds of models from many makers. Pay per token.",
            "OPENROUTER_API_KEY",
            "https://openrouter.ai/api/v1",
            &["deepseek/deepseek-chat", "moonshotai/kimi-k2", "z-ai/glm-4.6", "qwen/qwen3-coder"],
            "https://openrouter.ai/settings/keys",
        ),
        also(p(
            "kimi",
            "Kimi (Moonshot)",
            "Moonshot's Kimi models. Pay per token.",
            "KIMI_API_KEY",
            "https://api.moonshot.ai/v1",
            &["kimi-k2"],
            "https://platform.moonshot.ai/console/api-keys",
        ), &["MOONSHOT"]),
        also(p("zai", "Z.ai (GLM)", "Zhipu's GLM models. Pay per token or the GLM coding plan.", "GLM_API_KEY", "https://api.z.ai/api/paas/v4", &["glm-4.6"], "https://z.ai/manage-apikey/apikey-list"), &["GLM", "ZHIPU"]),
        also(p("xai", "xAI (Grok)", "xAI's Grok models. Pay per token.", "XAI_API_KEY", "https://api.x.ai/v1", &["grok-code-fast-1", "grok-4"], "https://console.x.ai"), &["GROK"]),
    ]
}
