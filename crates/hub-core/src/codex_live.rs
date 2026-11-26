//! Codex's own answer about the account: its plan and where the 5-hour and weekly windows stand
//! right now. `codex app-server` speaks JSON-RPC over stdio; `account/read` names the plan and
//! `account/rateLimits/read` gives the windows, so neither has to be guessed from the login file
//! or from the last session log (`codex_live::usage`, which stays as the fallback when Codex is not
//! installed, not signed in, or does not answer).
//!
//! The same read asks `account/usage/read` too: the tokens the whole account used per day,
//! wherever it ran (the CLI on any machine, ChatGPT's Codex, Codex in the cloud). A Codex too old
//! to know it leaves that out and the rest stands.
//!
//! One short-lived `codex app-server` per read, kept at most [`MAX_AGE_MS`]; a failed read is not
//! tried again for [`RETRY_MS`], so a machine without it never pays for a spawn on every look.

use crate::locked::ResultExt;
use crate::locked::LockExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::model::now_ms;
use crate::usage::{AccountUsage, PlanLimit, PlanUsage, iso_ms, window_label};

const MAX_AGE_MS: i64 = 2 * 60 * 1000;
const RETRY_MS: i64 = 10 * 60 * 1000;
const TIMEOUT: Duration = Duration::from_secs(15);

/// What the last read said, or when it last failed.
#[derive(Clone)]
enum Last {
    Read(i64, PlanUsage),
    Failed(i64),
}

static LAST: Mutex<Option<Last>> = Mutex::new(None);
static TOKENS: Mutex<Option<AccountTokens>> = Mutex::new(None);

/// What `account/usage/read` said: tokens per day for the whole account.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AccountTokens {
    pub lifetime: Option<i64>,
    /// (YYYY-MM-DD, tokens), as Codex dates them.
    pub daily: Vec<(String, i64)>,
}

/// The account's tokens per day as Codex last reported them.
pub fn account_tokens() -> Option<AccountTokens> {
    TOKENS.locked().clone()
}
static BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The plan and limits Codex reports now, or `None` when it cannot say (the caller falls back to
/// the session logs).
pub async fn read(dir: &Path) -> Option<PlanUsage> {
    if let Some(answer) = fresh() {
        return answer;
    }
    // Two lookers at once share one read.
    let _turn = BUSY.lock().await;
    if let Some(answer) = fresh() {
        return answer;
    }
    let result = ask(dir).await;
    let now = now_ms();
    let mut last = LAST.locked();
    match result {
        Ok(usage) => {
            // Settings shows the plan without asking (`engine_home::engine_accounts`).
            crate::engine_home::note_plan("codex", usage.plan.clone());
            *last = Some(Last::Read(now, usage.clone()));
            Some(usage)
        }
        Err(err) => {
            tracing::debug!("could not ask Codex for its limits: {err:#}");
            crate::engine_home::note_plan("codex", None);
            // The retry waits.
            *last = Some(Last::Failed(now));
            None
        }
    }
}

/// `Some(answer)` when a recent read or failure settles the question without asking again.
fn fresh() -> Option<Option<PlanUsage>> {
    let now = now_ms();
    match LAST.locked().as_ref()? {
        Last::Read(at, usage) if now - at < MAX_AGE_MS => Some(Some(usage.clone())),
        Last::Failed(at) if now - at < RETRY_MS => Some(None),
        _ => None,
    }
}

async fn ask(dir: &Path) -> Result<PlanUsage> {
    let program = crate::proc::find_program("codex").context("codex is not installed")?;
    let mut cmd = crate::proc::tokio_command(program);
    cmd.arg("app-server").env("CODEX_HOME", dir);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = cmd.spawn().context("starting codex app-server")?;
    let mut stdin = child.stdin.take().context("no stdin")?;
    let mut lines = BufReader::new(child.stdout.take().context("no stdout")?).lines();
    let exchange = async {
        let mut send = async |msg: Value| -> Result<()> {
            stdin.write_all(format!("{msg}\n").as_bytes()).await?;
            Ok(stdin.flush().await?)
        };
        send(json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "hub", "title": "Hub", "version": env!("CARGO_PKG_VERSION") } } })).await?;
        let init = reply(&mut lines, 1).await?;
        init.get("error").is_none().then_some(()).context("codex refused to start")?;
        send(json!({ "method": "initialized" })).await?;
        send(json!({ "id": 2, "method": "account/read", "params": { "refreshToken": false } })).await?;
        let account = reply(&mut lines, 2).await?;
        send(json!({ "id": 3, "method": "account/rateLimits/read" })).await?;
        let limits = reply(&mut lines, 3).await?;
        send(json!({ "id": 4, "method": "account/usage/read" })).await?;
        let tokens = reply(&mut lines, 4).await?;
        Ok::<_, anyhow::Error>((account, limits, tokens))
    };
    let (account, limits, tokens) = tokio::time::timeout(TIMEOUT, exchange).await.context("codex took too long")??;
    child.start_kill().warn_on_err("end the live-usage process");
    if let Some(t) = parse_tokens(&tokens) {
        *TOKENS.locked() = Some(t);
    }
    parse(&account, &limits, now_ms())
}

/// `account/usage/read`: `summary.lifetimeTokens` and `dailyUsageBuckets` of
/// `{ startDate, tokens }`. `None` when Codex does not know the call or would not say.
fn parse_tokens(answer: &Value) -> Option<AccountTokens> {
    let result = answer.get("result")?;
    let daily = result
        .get("dailyUsageBuckets")?
        .as_array()?
        .iter()
        .filter_map(|b| Some((b["startDate"].as_str()?.get(..10)?.to_owned(), b["tokens"].as_i64()?)))
        .collect();
    Some(AccountTokens { lifetime: result.pointer("/summary/lifetimeTokens").and_then(Value::as_i64), daily })
}

/// The next line that answers request `id`; notifications in between are skipped.
async fn reply(lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>, id: i64) -> Result<Value> {
    while let Some(line) = lines.next_line().await? {
        if let Ok(v) = serde_json::from_str::<Value>(&line)
            && v.get("id").and_then(Value::as_i64) == Some(id)
        {
            return Ok(v);
        }
    }
    bail!("codex closed before it answered")
}

/// The plan and windows out of the two answers. `account/read` says who is signed in and on what;
/// `account/rateLimits/read` carries `primary` and `secondary` windows
/// (`usedPercent`, `windowDurationMins`, `resetsAt` in seconds) and its own `planType`.
fn parse(account: &Value, limits: &Value, now: i64) -> Result<PlanUsage> {
    if let Some(err) = limits.get("error") {
        bail!("{}", err["message"].as_str().unwrap_or("codex would not say"));
    }
    let snapshot = limits.pointer("/result/rateLimits").context("no rate limits in the answer")?;
    let mut out = Vec::new();
    for (key, fallback) in [("primary", "5-hour limit"), ("secondary", "Weekly limit")] {
        let Some(w) = snapshot.get(key).filter(|w| w.is_object()) else { continue };
        let Some(percent) = w.get("usedPercent").and_then(Value::as_f64) else { continue };
        let minutes = w.get("windowDurationMins").and_then(Value::as_i64);
        let reset = w.get("resetsAt").and_then(Value::as_i64).map(|s| s * 1000);
        // A window that has reset since starts again from nothing.
        let (percent, resets_at) = match reset {
            Some(r) if r <= now => (0.0, None),
            r => (percent as f32, r),
        };
        out.push(PlanLimit { label: window_label(minutes, fallback), percent, resets: None, resets_at });
    }
    if out.is_empty() {
        bail!("codex reported no windows");
    }
    let plan = [account.pointer("/result/account/planType"), snapshot.get("planType")]
        .into_iter()
        .flatten()
        .find_map(|p| p.as_str().filter(|p| !p.is_empty() && *p != "unknown").map(str::to_owned));
    Ok(PlanUsage { plan, limits: out, checked_at: now })
}

/// Codex's limits as its session logs have them: the fallback when `codex_live` cannot ask it.
/// Codex writes them with every answer (`token_count` events in
/// `sessions/YYYY/MM/DD/rollout-*.jsonl`); the newest one says where the 5-hour and weekly
/// windows stood the last time Codex was used.
pub fn usage(dir: &std::path::Path) -> AccountUsage {
    let found = newest_rollouts(&dir.join("sessions"), 8)
        .into_iter()
        .find_map(|file| std::fs::read_to_string(file).ok()?.lines().rev().find_map(|l| codex_limits(l, now_ms())));
    AccountUsage {
        engine: "codex".into(),
        label: "Codex".into(),
        note: found.is_none().then(|| "Codex reports its limits as it works; none yet".into()),
        plan: found,
        balance: None,
        spent_today: None,
    }
}

/// The newest session logs first, at most `n`, by their dated folders and names.
fn newest_rollouts(root: &std::path::Path, n: usize) -> Vec<std::path::PathBuf> {
    fn sorted(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut entries: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
        entries.sort();
        entries.reverse();
        entries
    }
    let mut out = Vec::new();
    for year in sorted(root) {
        for month in sorted(&year) {
            for day in sorted(&month) {
                for file in sorted(&day) {
                    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                        out.push(file);
                        if out.len() == n {
                            return out;
                        }
                    }
                }
            }
        }
    }
    out
}

/// The limits in one session log line, if it is a `token_count` event that carries them.
fn codex_limits(line: &str, now: i64) -> Option<PlanUsage> {
    if !line.contains("rate_limits") {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let payload = v.get("payload").unwrap_or(&v);
    if payload.get("type").and_then(|t| t.as_str()) != Some("token_count") {
        return None;
    }
    let limits = payload.get("rate_limits").filter(|r| r.is_object())?;
    let at = v.get("timestamp").and_then(|t| t.as_str()).and_then(iso_ms).unwrap_or(now);
    let mut out = Vec::new();
    for (key, fallback) in [("primary", "5-hour limit"), ("secondary", "Weekly limit")] {
        // Newer logs nest each window; older ones flatten them as primary_used_percent etc.
        let (percent, minutes, reset) = match limits.get(key) {
            Some(w) if w.is_object() => (
                w.get("used_percent").and_then(|p| p.as_f64()),
                w.get("window_minutes").and_then(|m| m.as_i64()),
                w.get("resets_at").and_then(|r| r.as_i64()).map(|s| s * 1000)
                    .or_else(|| w.get("resets_in_seconds").and_then(|r| r.as_i64()).map(|s| at + s * 1000)),
            ),
            _ => (
                limits.get(format!("{key}_used_percent")).and_then(|p| p.as_f64()),
                limits.get(format!("{key}_window_minutes")).and_then(|m| m.as_i64()),
                None,
            ),
        };
        let Some(percent) = percent else { continue };
        // A window that has reset since starts again from nothing.
        let (percent, resets_at) = match reset {
            Some(r) if r <= now => (0.0, None),
            r => (percent as f32, r),
        };
        out.push(PlanLimit { label: window_label(minutes, fallback), percent, resets: None, resets_at });
    }
    let plan = limits.get("plan_type").and_then(|p| p.as_str()).map(str::to_owned);
    (!out.is_empty()).then_some(PlanUsage { plan, limits: out, checked_at: at })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_codex_limits_from_a_session_log() {
        let line = r#"{"timestamp":"2026-10-30T17:00:00.000Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":42.5,"window_minutes":300,"resets_in_seconds":600},"secondary":{"used_percent":12.0,"window_minutes":10080,"resets_at":1893456000}}}}"#;
        let now = iso_ms("2026-10-30T17:05:00Z").unwrap();
        let usage = codex_limits(line, now).unwrap();
        assert_eq!(usage.checked_at, iso_ms("2026-10-30T17:00:00Z").unwrap());
        let got: Vec<_> = usage.limits.iter().map(|l| (l.label.as_str(), l.percent, l.resets_at)).collect();
        assert_eq!(got, [("5-hour limit", 42.5, Some(usage.checked_at + 600_000)), ("Weekly limit", 12.0, Some(1_893_456_000_000))]);
        // Ten minutes later the 5-hour window has reset.
        let later = codex_limits(line, now + 600_000).unwrap();
        assert_eq!((later.limits[0].percent, later.limits[0].resets_at), (0.0, None));
        // Older logs flatten the windows.
        let old = r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary_used_percent":7.0,"secondary_used_percent":3.0,"primary_window_minutes":300,"secondary_window_minutes":10080}}}"#;
        assert_eq!(codex_limits(old, now).unwrap().limits.len(), 2);
        assert!(codex_limits(r#"{"payload":{"type":"agent_message"}}"#, now).is_none());
    }

    #[test]
    fn reads_plan_and_windows() {
        let account = json!({ "id": 2, "result": { "account": { "type": "chatgpt", "email": "a@b.c", "planType": "promax", "workspaceName": null } } });
        let limits = json!({ "id": 3, "result": { "rateLimits": {
            "limitId": "codex", "planType": "pro",
            "primary": { "usedPercent": 12.0, "windowDurationMins": 300, "resetsAt": 2_000_000_000 },
            "secondary": { "usedPercent": 40.5, "windowDurationMins": 10080, "resetsAt": 1 }
        } } });
        let usage = parse(&account, &limits, 1_900_000_000_000).unwrap();
        assert_eq!(usage.plan.as_deref(), Some("promax"));
        let seen: Vec<_> = usage.limits.iter().map(|l| (l.label.as_str(), l.percent, l.resets_at)).collect();
        // The weekly window had reset already, so it starts again from nothing.
        assert_eq!(seen, [("5-hour limit", 12.0, Some(2_000_000_000_000)), ("Weekly limit", 0.0, None)]);
    }

    #[test]
    fn falls_back_to_the_limits_plan_and_refuses_errors() {
        let limits = json!({ "result": { "rateLimits": { "planType": "plus", "primary": { "usedPercent": 1.0, "windowDurationMins": 300 } } } });
        assert_eq!(parse(&json!({ "result": { "account": null } }), &limits, 0).unwrap().plan.as_deref(), Some("plus"));
        let refused = json!({ "error": { "code": -32600, "message": "codex account authentication required to read rate limits" } });
        assert!(parse(&json!({}), &refused, 0).unwrap_err().to_string().contains("authentication required"));
    }

    #[test]
    fn reads_the_accounts_tokens_per_day() {
        let answer = json!({ "id": 4, "result": {
            "summary": { "lifetimeTokens": 123_456_789, "peakDailyTokens": 9, "longestRunningTurnSec": null },
            "dailyUsageBuckets": [{ "startDate": "2026-11-02", "tokens": 5_000 }, { "startDate": "2026-11-03", "tokens": 7_000 }, { "tokens": 1 }]
        } });
        let t = parse_tokens(&answer).unwrap();
        assert_eq!(t.lifetime, Some(123_456_789));
        assert_eq!(t.daily, vec![("2026-11-02".to_owned(), 5_000), ("2026-11-03".to_owned(), 7_000)]);
        assert!(parse_tokens(&json!({ "id": 4, "error": { "code": -32601, "message": "method not found" } })).is_none());
        assert!(parse_tokens(&json!({ "id": 4, "result": { "summary": {}, "dailyUsageBuckets": null } })).is_none());
    }

    #[tokio::test]
    async fn a_missing_codex_is_not_asked_again_for_a_while() {
        *LAST.locked() = Some(Last::Failed(now_ms()));
        assert!(read(Path::new("/nonexistent")).await.is_none());
        assert!(fresh().is_some());
        *LAST.locked() = None;
    }

    /// Needs a real `codex` on the PATH: it answers over the wire, and without a login says why.
    #[tokio::test]
    #[ignore]
    async fn real_codex_without_a_login_is_refused() {
        let dir = crate::janitor::test_dir();
        let err = ask(&dir).await.unwrap_err();
        assert!(format!("{err:#}").contains("authentication required"), "{err:#}");
    }
}
