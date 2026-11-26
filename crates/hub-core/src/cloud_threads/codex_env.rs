//! Which Codex cloud environment a repository's tasks go to: asked of ChatGPT the way `codex cloud` asks.

use std::time::Duration;

use serde_json::Value;

/// One environment of the ChatGPT account.
#[derive(Debug, Clone, PartialEq)]
pub struct CodexEnv {
    pub id: String,
    pub label: String,
}

/// The environments in an answer of ChatGPT's environment calls (an array, or an object holding one).
pub fn parse_codex_envs(v: &Value) -> Vec<CodexEnv> {
    let rows = v.as_array().or_else(|| ["environments", "data", "items"].iter().find_map(|k| v[*k].as_array()));
    rows.into_iter()
        .flatten()
        .filter_map(|e| {
            let id = e["id"].as_str()?.to_owned();
            let label = ["label", "name", "repo", "repository"].iter().find_map(|k| e[*k].as_str()).unwrap_or_default().to_owned();
            Some(CodexEnv { id, label })
        })
        .collect()
}

/// Which of the account's environments a task for `repo` goes to: the ones ChatGPT itself ties to
/// the repository (`by_repo`), else the ones labelled with the repository or its name, else the
/// only environment there is when the account has just one.
pub fn pick_codex_envs(by_repo: &[CodexEnv], all: &[CodexEnv], repo: &str) -> Vec<CodexEnv> {
    if !by_repo.is_empty() {
        return by_repo.to_vec();
    }
    let name = repo.rsplit('/').next().unwrap_or(repo);
    let labelled: Vec<CodexEnv> = all.iter().filter(|e| e.label.trim().eq_ignore_ascii_case(repo) || e.label.trim().eq_ignore_ascii_case(name)).cloned().collect();
    if !labelled.is_empty() {
        return labelled;
    }
    if all.len() == 1 { all.to_vec() } else { Vec::new() }
}

/// What Codex's own login (`auth.json`) gives to ask ChatGPT's backend: the token and account.
fn codex_login() -> Option<(String, Option<String>)> {
    let path = crate::engine_home::engine_home("codex")?.join("auth.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let token = v.pointer("/tokens/access_token")?.as_str()?.to_owned();
    Some((token, v.pointer("/tokens/account_id").and_then(Value::as_str).map(str::to_owned)))
}

/// The environments of the account that suit `repo`, asked the way `codex cloud` asks (the ones
/// tied to the repository first, then all of them), and every label there is for the error text.
/// An answer Hub cannot get leaves both empty.
pub(super) async fn codex_envs(repo: &str) -> (Vec<CodexEnv>, Vec<CodexEnv>) {
    let Some((token, account)) = codex_login() else { return (Vec::new(), Vec::new()) };
    let client = reqwest::Client::new();
    let ask = |path: String| {
        let mut r = client.get(format!("https://chatgpt.com/backend-api{path}")).bearer_auth(&token).header("User-Agent", "codex_cli_rs").timeout(Duration::from_secs(15));
        if let Some(a) = &account {
            r = r.header("ChatGPT-Account-Id", a);
        }
        async move {
            let answer = r.send().await.ok()?;
            if !answer.status().is_success() {
                return None;
            }
            answer.json::<Value>().await.ok()
        }
    };
    let by_repo = ask(format!("/wham/environments/by-repo/github/{repo}")).await.map(|v| parse_codex_envs(&v)).unwrap_or_default();
    let all = ask("/wham/environments".to_owned()).await.map(|v| parse_codex_envs(&v)).unwrap_or_default();
    let picked = pick_codex_envs(&by_repo, &all, repo);
    (picked, all)
}
