//! Claude Code sessions that ran in Anthropic's cloud on the same login (claude.ai/code, the
//! project threads in the Claude app, Cowork): their tokens never reach a transcript on this
//! machine, yet they spend the same plan. Each session's running total is read from the Sessions
//! API that Claude Code itself uses for `--teleport` and Remote Control (`GET /v1/sessions`,
//! `GET /v1/sessions/{id}`, its `external_metadata.usage`), with Claude Code's own login, and what
//! grew since the last look goes to `cloud_marks`. The first look at a session spreads what it
//! already had over the days it ran.
//!
//! Sessions that run on a machine of yours (Remote Control, `environment_kind` "bridge") are left
//! out: their transcripts are on that machine and counted there. The API is not documented, so
//! an answer it stops giving leaves the cloud lane empty with a note, never an error. Hub never
//! renews Claude Code's login itself (that would sign Claude Code out); an expired token waits
//! until Claude Code has run again.

use crate::locked::ResultExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::store::Store;
use crate::model::now_ms;
pub use crate::model::rows::{CloudMark, Seen, Usage};

const BASE: &str = "https://api.anthropic.com";
/// What Claude Code sends to reach the sessions it can teleport.
const BETA: &str = "ccr-byoc-2025-07-29";
/// The listing leaves tagged sessions out unless asked for them by tag.
const TAGS: &[&str] = &["hearth-thread", "cowork-local", "cowork-remote"];
/// At most this often; the page reads the numbers far more often than that.
const EVERY_MS: i64 = 10 * 60_000;
const SYNCED_KEY: &str = "cloud.synced_at";
pub const NOTE_KEY: &str = "cloud.note";
const DAY: i64 = 86_400_000;
/// Sessions older than this are not looked at.
const BACK_MS: i64 = 90 * DAY;
/// Session lookups per pass, so a first pass over a long history does not hammer the API.
const LOOKUPS: usize = 150;


impl Usage {
    fn minus(&self, before: &Usage) -> Usage {
        Usage {
            input: (self.input - before.input).max(0),
            output: (self.output - before.output).max(0),
            cache_write: (self.cache_write - before.cache_write).max(0),
            cache_read: (self.cache_read - before.cache_read).max(0),
            cost: (self.cost - before.cost).max(0.0),
        }
    }

    fn share(&self, part: f64) -> Usage {
        let n = |v: i64| (v as f64 * part).round() as i64;
        Usage { input: n(self.input), output: n(self.output), cache_write: n(self.cache_write), cache_read: n(self.cache_read), cost: self.cost * part }
    }

    fn is_empty(&self) -> bool {
        self.cost <= 0.0 && self.input + self.output + self.cache_write + self.cache_read == 0
    }

    /// Fresh input plus output, without cache reads, as the other lanes count tokens.
    pub fn tokens(&self) -> i64 {
        self.input + self.cache_write + self.output
    }
}



/// One row of the sessions listing.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Runs on a machine of yours, whose transcripts count there.
    pub local: bool,
}

/// The rows of a listing page, the cursor for the next page and whether there is one.
pub fn parse_list(v: &Value) -> (Vec<Listed>, Option<String>, bool) {
    let rows = v["data"].as_array().map(Vec::as_slice).unwrap_or_default();
    let listed = rows
        .iter()
        .filter_map(|s| {
            Some(Listed {
                id: s["id"].as_str()?.to_owned(),
                created_at: s["created_at"].as_str().and_then(crate::usage::iso_ms).unwrap_or(0),
                updated_at: s["updated_at"].as_str().and_then(crate::usage::iso_ms).unwrap_or(0),
                local: s["environment_kind"] == "bridge",
            })
        })
        .collect();
    (listed, v["last_id"].as_str().map(str::to_owned), v["has_more"] == true)
}

/// A session's running total, from `external_metadata.usage`.
pub fn parse_usage(session: &Value) -> Option<Usage> {
    let u = session.pointer("/external_metadata/usage")?;
    let n = |k: &str| u[k].as_i64().unwrap_or(0);
    Some(Usage {
        input: n("input_tokens"),
        output: n("output_tokens"),
        cache_write: n("cache_write_tokens"),
        cache_read: n("cache_read_tokens"),
        cost: u["cost_usd"].as_f64().unwrap_or(0.0),
    })
}

/// What to record for a session that now has `usage`: what grew since `before`, at `now`; or, on
/// a first look, all of it spread evenly over the days from `created_at` to `updated_at`.
pub fn marks_for(id: &str, before: Option<&Seen>, usage: Usage, created_at: i64, updated_at: i64, now: i64) -> Vec<CloudMark> {
    if let Some(seen) = before {
        let grew = usage.minus(&seen.usage);
        return if grew.is_empty() { Vec::new() } else { vec![CloudMark { session_id: id.to_owned(), ts: updated_at.min(now), usage: grew }] };
    }
    if usage.is_empty() {
        return Vec::new();
    }
    let end = updated_at.clamp(created_at, now);
    let days = ((end - created_at) / DAY + 1).max(1);
    let step = (end - created_at) / days;
    (0..days)
        .map(|i| CloudMark { session_id: id.to_owned(), ts: created_at + step * i + step / 2, usage: usage.share(1.0 / days as f64) })
        .collect()
}

pub(crate) struct Login {
    pub(crate) token: String,
    /// From Claude Code's settings; asked of the account when they do not say.
    pub(crate) org: Option<String>,
}

fn claude_dir() -> Option<PathBuf> {
    crate::engine_home::engine_home("claude")
}

fn read_json(path: &std::path::Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Claude Code's login on this machine, while its token is still good.
pub(crate) fn login(now: i64) -> std::result::Result<Login, &'static str> {
    let dir = claude_dir().ok_or("no home folder")?;
    let creds = read_json(&dir.join(".credentials.json")).ok_or("Claude Code keeps its login where Hub cannot read it on this machine")?;
    let oauth = creds.get("claudeAiOauth").ok_or("Claude Code is not signed in with a Claude plan")?;
    let token = oauth["accessToken"].as_str().ok_or("Claude Code is not signed in with a Claude plan")?.to_owned();
    if oauth["expiresAt"].as_i64().is_some_and(|at| at < now + 60_000) {
        return Err("Claude Code's sign-in is out of date; it renews the next time Claude Code runs");
    }
    let home = crate::home_files::home_dir();
    let account = read_json(&dir.join(".claude.json")).or_else(|| home.and_then(|h| read_json(&h.join(".claude.json"))));
    let org = account.as_ref().and_then(|a| a.pointer("/oauthAccount/organizationUuid")).and_then(Value::as_str).map(str::to_owned);
    Ok(Login { token, org })
}

/// The organization the sign-in belongs to, from the account's own profile, as Claude Code
/// itself asks when its settings do not say (`GET /api/oauth/profile`, `organization.uuid`).
pub(crate) async fn profile_org(client: &reqwest::Client, token: &str) -> Result<String> {
    let answer = client
        .get(format!("{BASE}/api/oauth/profile"))
        .bearer_auth(token)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .context("could not reach Claude's account")?;
    let status = answer.status();
    if !status.is_success() {
        bail!("Claude's account answered {status}");
    }
    org_of(&answer.json().await?).context("Claude's account did not say which organization it is in")
}

fn org_of(profile: &Value) -> Option<String> {
    profile.pointer("/organization/uuid").and_then(Value::as_str).map(str::to_owned)
}

pub(crate) async fn get(client: &reqwest::Client, login: &Login, path: &str) -> Result<Value> {
    let answer = client
        .get(format!("{BASE}{path}"))
        .bearer_auth(&login.token)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", BETA)
        .header("x-organization-uuid", login.org.as_deref().unwrap_or_default())
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .context("could not reach Claude's sessions")?;
    let status = answer.status();
    if !status.is_success() {
        bail!("Claude's sessions answered {status}");
    }
    Ok(answer.json().await?)
}

static RUNNING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Reads the cloud sessions again when the last pass is ten minutes old. What went wrong is kept
/// as a note (`NOTE_KEY`) for the page, and cleared on a pass that worked.
pub async fn sync(store: Store) {
    let Ok(_running) = RUNNING.try_lock() else { return };
    let now = now_ms();
    let last = store.get_value(SYNCED_KEY).await.ok().flatten().and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
    if now - last < EVERY_MS {
        return;
    }
    store.set_value(SYNCED_KEY, &now.to_string()).await.warn_on_err("save SYNCED_KEY");
    let note = match pass(&store, now).await {
        Ok(()) => String::new(),
        Err(e) => format!("{e:#}"),
    };
    if !note.is_empty() {
        tracing::info!("cloud sessions not read: {note}");
    }
    store.set_value(NOTE_KEY, &note).await.warn_on_err("save NOTE_KEY");
}

async fn pass(store: &Store, now: i64) -> Result<()> {
    let mut login = login(now).map_err(|why| anyhow::anyhow!(why))?;
    let client = reqwest::Client::new();
    if login.org.is_none() {
        login.org = Some(profile_org(&client, &login.token).await?);
    }
    let mut listed: HashMap<String, Listed> = HashMap::new();
    let mut listings: Vec<Option<&str>> = vec![None];
    listings.extend(TAGS.iter().map(|t| Some(*t)));
    for tag in listings {
        let mut after: Option<String> = None;
        for _ in 0..20 {
            let mut path = "/v1/sessions?limit=100".to_owned();
            if let Some(t) = tag {
                path.push_str(&format!("&tags={t}"));
            }
            if let Some(a) = &after {
                path.push_str(&format!("&after_id={a}"));
            }
            let page = match get(&client, &login, &path).await {
                Ok(page) => page,
                // A tag the listing does not take is not worth failing the rest for.
                Err(e) if tag.is_some() => {
                    tracing::debug!("cloud sessions tagged {tag:?}: {e:#}");
                    break;
                }
                Err(e) => return Err(e),
            };
            let (rows, last, more) = parse_list(&page);
            let old = rows.iter().all(|r| r.updated_at.max(r.created_at) < now - BACK_MS);
            for r in rows {
                listed.entry(r.id.clone()).or_insert(r);
            }
            if !more || old || last.is_none() {
                break;
            }
            after = last;
        }
    }
    let seen = store.cloud_seen().await?;
    let mut due: Vec<&Listed> = listed
        .values()
        .filter(|r| !r.local && r.updated_at >= now - BACK_MS)
        .filter(|r| seen.get(&r.id).is_none_or(|s| r.updated_at > s.updated_at))
        .collect();
    due.sort_by_key(|r| std::cmp::Reverse(r.updated_at));
    let mut rows = Vec::new();
    let mut marks = Vec::new();
    for r in due.into_iter().take(LOOKUPS) {
        let Ok(session) = get(&client, &login, &format!("/v1/sessions/{}", r.id)).await else { continue };
        let Some(usage) = parse_usage(&session) else {
            // Nothing counted yet; look again when it moves.
            rows.push((r.id.clone(), Seen { updated_at: r.updated_at, usage: Usage::default() }));
            continue;
        };
        marks.extend(marks_for(&r.id, seen.get(&r.id), usage, r.created_at, r.updated_at, now));
        rows.push((r.id.clone(), Seen { updated_at: r.updated_at, usage }));
    }
    store.cloud_record(rows, marks).await
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"{"data":[
        {"id":"session_a","created_at":"2026-11-03T12:20:16.45Z","updated_at":"2026-11-03T18:30:03Z","environment_kind":"anthropic_cloud"},
        {"id":"session_b","created_at":"2026-11-03T10:18:01Z","updated_at":"2026-11-03T10:24:45Z","environment_kind":"bridge"},
        {"title":"no id"}
    ],"has_more":true,"first_id":"session_a","last_id":"session_b"}"#;

    #[test]
    fn a_listing_says_which_sessions_run_on_a_machine_of_yours() {
        let (rows, last, more) = parse_list(&serde_json::from_str(LIST).unwrap());
        assert_eq!(rows.len(), 2);
        assert!(!rows[0].local && rows[1].local);
        assert!(rows[0].updated_at > rows[0].created_at);
        assert_eq!((last.as_deref(), more), (Some("session_b"), true));
    }

    #[test]
    fn the_organization_comes_from_the_profile() {
        let profile = serde_json::json!({ "account": { "uuid": "a1", "email_address": "x@y" }, "organization": { "uuid": "o1", "name": "Personal" } });
        assert_eq!(org_of(&profile).as_deref(), Some("o1"));
        assert_eq!(org_of(&serde_json::json!({ "account": {} })), None);
    }

    #[test]
    fn a_session_gives_its_running_total() {
        let s: Value = serde_json::from_str(
            r#"{"id":"session_a","external_metadata":{"usage":{"cache_read_tokens":149718702,"cache_write_tokens":1663306,"cost_usd":48.38,"input_tokens":21329,"output_tokens":392692}}}"#,
        )
        .unwrap();
        let u = parse_usage(&s).unwrap();
        assert_eq!((u.input, u.output, u.cache_write, u.cache_read), (21329, 392692, 1663306, 149718702));
        assert!((u.cost - 48.38).abs() < 1e-9);
        assert_eq!(u.tokens(), 21329 + 1663306 + 392692);
        assert!(parse_usage(&serde_json::json!({"id": "x"})).is_none());
    }

    #[test]
    fn a_first_look_spreads_over_the_days_and_later_looks_count_what_grew() {
        let usage = Usage { input: 30, output: 300, cache_write: 0, cache_read: 3000, cost: 3.0 };
        let start = 1_793_000_000_000;
        let first = marks_for("s", None, usage, start, start + 2 * DAY + 3_600_000, start + 4 * DAY);
        assert_eq!(first.len(), 3);
        assert!(first.iter().all(|m| (m.usage.cost - 1.0).abs() < 1e-9 && m.ts >= start && m.ts <= start + 2 * DAY + 3_600_000));
        let seen = Seen { updated_at: start, usage };
        let more = Usage { output: 400, cost: 4.5, ..usage };
        let later = marks_for("s", Some(&seen), more, start, start + 5 * DAY, start + 5 * DAY);
        assert_eq!(later.len(), 1);
        assert_eq!((later[0].usage.output, later[0].usage.input), (100, 0));
        assert!((later[0].usage.cost - 1.5).abs() < 1e-9);
        assert!(marks_for("s", Some(&Seen { updated_at: start, usage: more }), more, start, start + 6 * DAY, start + 6 * DAY).is_empty());
        assert!(marks_for("s", None, Usage::default(), start, start, start).is_empty());
    }
}
