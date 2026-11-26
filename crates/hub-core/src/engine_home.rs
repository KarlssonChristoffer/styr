//! Where each engine keeps its login and files on this machine, and who is signed in there:
//! `engine_home` answers for a harness id, `engine_accounts` reads every login. Engine knowledge
//! (folder names, login file formats, environment variables) stays here and in the other engine
//! files, so `prefs.rs`, the spend pass and the rest of the core only ask. It also says what the
//! agents leave in git (`AGENT_AUTHORS`, `AGENT_BRANCHES`), for the activity map and the clean-up.
//! (Moved out of `prefs.rs` unchanged.)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use crate::locked::LockExt;
use crate::model::EngineAccount;
use crate::home_files::{read_json, text};

/// The folders under the home folder where engines keep their logins, which a phone may never
/// read (`phone_files::secret`).
pub const LOGIN_FOLDERS: &[&str] = &[".claude", ".codex", ".hermes"];

/// What the agents Hub runs leave in git: the author emails of their commits (made for you, so
/// they count as yours), and the branch prefixes of their own worktrees (`AGENT_BRANCHES`).
pub const AGENT_AUTHORS: &[&str] = &["noreply@anthropic.com", "claude@anthropic.com", "noreply@openai.com", "codex@openai.com"];
/// The branch prefixes of the agents' own worktrees, kept while they hold unpushed work.
pub const AGENT_BRANCHES: &[&str] = &["claude/"];

/// The folder an engine keeps its login and logs in for this user (`CLAUDE_CONFIG_DIR`,
/// `CODEX_HOME` or the default under the home folder), or `None` for an engine Hub does not read
/// files of, or when there is no home folder.
pub fn engine_home(engine: &str) -> Option<PathBuf> {
    let home = crate::home_files::home_dir()?;
    match engine {
        "claude" => Some(claude_dir(&home)),
        "codex" => Some(codex_dir(&home)),
        _ => None,
    }
}

/// Where Claude Code keeps its login and account.
pub(crate) fn claude_dir(home: &Path) -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| home.join(".claude"))
}

/// Claude Code keeps its login in `.credentials.json` and the account in `.claude.json`.
pub(crate) fn claude(home: &Path, command: Option<String>) -> EngineAccount {
    let dir = claude_dir(home);
    let creds_path = dir.join(".credentials.json");
    let creds = read_json(&creds_path);
    let account = read_json(&dir.join(".claude.json")).or_else(|| read_json(&home.join(".claude.json")));
    let oauth = creds.as_ref().and_then(|c| c.get("claudeAiOauth")).filter(|o| o.get("refreshToken").is_some());
    let api_key = std::env::var("ANTHROPIC_API_KEY").is_ok_and(|k| !k.trim().is_empty());
    let method = if oauth.is_some() { Some("subscription") } else if api_key { Some("api_key") } else { None };
    EngineAccount {
        engine: "claude".into(),
        label: "Claude Code".into(),
        command,
        logged_in: method.is_some(),
        method: method.map(str::to_owned),
        plan: oauth.and_then(|o| text(o, "/subscriptionType")),
        email: account.as_ref().and_then(|a| text(a, "/oauthAccount/emailAddress")),
        organization: account.as_ref().and_then(|a| text(a, "/oauthAccount/organizationName")),
        source: oauth.map(|_| creds_path.display().to_string()),
    }
}

/// Codex keeps either a ChatGPT login or an API key in `auth.json`. `live_plan` is the plan Codex
/// itself last reported (`note_plan`).
pub(crate) fn codex(home: &Path, command: Option<String>, live_plan: Option<String>) -> EngineAccount {
    let path = codex_dir(home).join("auth.json");
    let auth = read_json(&path);
    let chatgpt = auth.as_ref().is_some_and(|a| a.pointer("/tokens/refresh_token").is_some());
    let api_key = auth.as_ref().is_some_and(|a| text(a, "/OPENAI_API_KEY").is_some());
    let method = if chatgpt { Some("subscription") } else if api_key { Some("api_key") } else { None };
    // The ID token is a JWT whose middle part says who is signed in and on which plan. Only that
    // part is read; nothing from the token itself is kept or shown.
    let claims = chatgpt
        .then(|| auth.as_ref().and_then(|a| text(a, "/tokens/id_token")))
        .flatten()
        .and_then(|t| t.split('.').nth(1).and_then(base64url))
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    EngineAccount {
        engine: "codex".into(),
        label: "Codex".into(),
        command,
        logged_in: method.is_some(),
        method: method.map(str::to_owned),
        // What Codex itself last reported beats the login file, which only changes when the token does.
        plan: method.and(live_plan).or_else(|| claims.as_ref().and_then(|c| text(c, "/https:~1~1api.openai.com~1auth/chatgpt_plan_type"))),
        email: claims.as_ref().and_then(|c| text(c, "/email")),
        organization: None,
        source: method.map(|_| path.display().to_string()),
    }
}

/// Where Codex keeps its login and session logs.
pub fn codex_dir(home: &Path) -> PathBuf {
    std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".codex"))
}

/// The plan each engine itself last reported, by engine id; filled by the engines' own readers
/// (`codex_live`).
static LIVE_PLANS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// Notes the plan `engine` itself just reported, or forgets it when a read failed or said none.
pub fn note_plan(engine: &str, plan: Option<String>) {
    let mut plans = LIVE_PLANS.locked();
    match plan {
        Some(plan) => plans.insert(engine.to_owned(), plan),
        None => plans.remove(engine),
    };
}

/// Who is signed in to each engine on this machine, with the plan it last reported. `command`
/// gives the command each engine is set to start with.
pub fn engine_accounts(home: &Path, command: impl Fn(&str) -> Option<String>) -> Vec<EngineAccount> {
    let live = LIVE_PLANS.locked().get("codex").cloned();
    vec![claude(home, command("claude")), codex(home, command("codex"), live)]
}

/// Decodes base64url without padding, as JWTs use it.
fn base64url(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0);
    for c in text.trim_end_matches('=').bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        bits = (bits << 6) | v as u32;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_engine_account_is_read_in_order_with_the_plan_it_last_reported() {
        // A login folder set by the environment would be read in place of the temp home.
        if std::env::var_os("CODEX_HOME").is_some() || std::env::var_os("CLAUDE_CONFIG_DIR").is_some() {
            return;
        }
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".codex")).unwrap();
        std::fs::write(home.path().join(".codex/auth.json"), r#"{"tokens":{"refresh_token":"r"}}"#).unwrap();
        note_plan("codex", Some("pro".into()));
        let accounts = engine_accounts(home.path(), |e| Some(format!("{e}-acp")));
        let engines: Vec<_> = accounts.iter().map(|a| (a.engine.as_str(), a.command.as_deref())).collect();
        assert_eq!(engines, vec![("claude", Some("claude-acp")), ("codex", Some("codex-acp"))]);
        assert_eq!(accounts[1].plan.as_deref(), Some("pro"), "the plan Codex last reported");
        // A failed read forgets it, and the login file's own plan (none here) is shown.
        note_plan("codex", None);
        assert_eq!(engine_accounts(home.path(), |_| None)[1].plan, None);
    }
}
