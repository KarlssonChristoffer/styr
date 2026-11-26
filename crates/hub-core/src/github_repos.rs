//! The repositories of the GitHub account the GitHub CLI is signed in as on this machine, and
//! the sign-in itself. Both go through `gh`: the list is one GraphQL question (everything the
//! account owns, collaborates on or reaches through an organization), the sign-in is
//! `gh auth login --web`, GitHub's own device login. Hub shows the one-time code it prints and
//! never sees a password or a token. Afterwards `gh auth setup-git` lets plain `git clone` and
//! `git fetch` on that machine use the same sign-in.

use crate::locked::LockExt;
use crate::locked::Ignore;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

use crate::model::{GithubConnect, GithubRepo, GithubRepos};
use crate::proc::{GhError, gh_within};
use crate::triage::parse_time;

const QUERY: &str = "query($endCursor: String) { viewer { login repositories(first: 100, after: $endCursor, \
  affiliations: [OWNER, COLLABORATOR, ORGANIZATION_MEMBER], orderBy: {field: PUSHED_AT, direction: DESC}) { \
  pageInfo { hasNextPage endCursor } nodes { nameWithOwner name description isPrivate isArchived pushedAt url \
  owner { login } } } } }";

/// How long a list stays good. Cloning or creating a repository asks again with `fresh`.
const FRESH: Duration = Duration::from_secs(300);
/// GitHub's device code lives 15 minutes; the sign-in is given up a little after.
const SIGN_IN_LIMIT: Duration = Duration::from_secs(16 * 60);

static CACHE: Mutex<Option<(Instant, GithubRepos)>> = Mutex::new(None);

fn gh_message(e: GhError) -> String {
    match e {
        GhError::Missing => "The GitHub CLI (gh) is not installed on this machine.".into(),
        GhError::Failed(msg) => msg,
    }
}

/// Every repository the account reaches, most recently pushed first.
pub async fn repos(fresh: bool) -> GithubRepos {
    if !fresh
        && let Some((at, list)) = CACHE.locked().as_ref()
            && at.elapsed() < FRESH {
                return list.clone();
            }
    let query = format!("query={QUERY}");
    let args = ["api", "graphql", "--paginate", "-f", query.as_str()];
    let out = match gh_within(&std::env::temp_dir(), &args, Duration::from_secs(90)).await {
        Ok(out) => out,
        Err(e) => return GithubRepos { login: None, message: Some(gh_message(e)), repos: Vec::new() },
    };
    let (login, repos) = parse_repos(&out);
    let list = GithubRepos { login, message: None, repos };
    *CACHE.locked() = Some((Instant::now(), list.clone()));
    list
}

/// Reads what `gh api graphql --paginate` printed: one JSON document per page.
pub fn parse_repos(out: &str) -> (Option<String>, Vec<GithubRepo>) {
    let mut login = None;
    let mut repos: Vec<GithubRepo> = Vec::new();
    for page in serde_json::Deserializer::from_str(out).into_iter::<Value>().flatten() {
        let viewer = &page["data"]["viewer"];
        if login.is_none() {
            login = viewer["login"].as_str().map(str::to_owned);
        }
        for n in viewer["repositories"]["nodes"].as_array().into_iter().flatten() {
            let Some(full) = n["nameWithOwner"].as_str() else { continue };
            if repos.iter().any(|r| r.full.eq_ignore_ascii_case(full)) {
                continue;
            }
            let (owner, name) = full.split_once('/').unwrap_or(("", full));
            repos.push(GithubRepo {
                full: full.to_owned(),
                name: n["name"].as_str().unwrap_or(name).to_owned(),
                owner: n["owner"]["login"].as_str().unwrap_or(owner).to_owned(),
                description: n["description"].as_str().filter(|d| !d.trim().is_empty()).map(str::to_owned),
                private: n["isPrivate"].as_bool().unwrap_or(false),
                archived: n["isArchived"].as_bool().unwrap_or(false),
                pushed: n["pushedAt"].as_str().and_then(parse_time).unwrap_or(0),
                url: n["url"].as_str().map(str::to_owned).unwrap_or_else(|| format!("https://github.com/{full}")),
            });
        }
    }
    repos.sort_by_key(|x| std::cmp::Reverse(x.pushed));
    (login, repos)
}

// ---- The sign-in ----------------------------------------------------------------------------

struct Job {
    id: u64,
    code: Option<String>,
    url: Option<String>,
    /// The last thing `gh` said, for when it ends badly.
    last: Option<String>,
    /// How it ended: None while it runs.
    ended: Option<Result<(), String>>,
    cancel: std::sync::Arc<tokio::sync::Notify>,
}

static JOB: Mutex<Option<Job>> = Mutex::new(None);
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Where this machine stands: a sign-in in progress, or what `gh` says about the account.
pub async fn status() -> GithubConnect {
    {
        let mut job = JOB.locked();
        if let Some(j) = job.as_ref() {
            match &j.ended {
                None => return waiting(j),
                // Told once, then forgotten, so the next look asks `gh` again.
                Some(Err(message)) => {
                    let message = message.clone();
                    *job = None;
                    return GithubConnect { state: "failed".into(), message: Some(message), ..Default::default() };
                }
                Some(Ok(())) => *job = None,
            }
        }
    }
    account("gh").await
}

fn waiting(j: &Job) -> GithubConnect {
    GithubConnect { state: "waiting".into(), code: j.code.clone(), url: j.url.clone(), ..Default::default() }
}

async fn account(program: &str) -> GithubConnect {
    let mut cmd = crate::proc::tokio_command(program);
    // `-i` puts GitHub's headers first: `X-OAuth-Scopes` says what this sign-in may do.
    cmd.args(["api", "-i", "user"]).current_dir(std::env::temp_dir()).kill_on_drop(true).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let out = match tokio::time::timeout(Duration::from_secs(12), cmd.output()).await {
        Err(_) => return GithubConnect { state: "none".into(), message: Some("GitHub did not answer in time.".into()), ..Default::default() },
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => return GithubConnect { state: "missing".into(), message: Some(gh_message(GhError::Missing)), ..Default::default() },
        Ok(Err(e)) => return GithubConnect { state: "none".into(), message: Some(e.to_string()), ..Default::default() },
        Ok(Ok(out)) => out,
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    if out.status.success() {
        return parse_account(&stdout);
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("Not signed in to GitHub.").trim().to_owned();
    if rejected(&stdout, &first) {
        return GithubConnect { state: "lost".into(), message: Some("GitHub no longer accepts the saved sign-in on this machine.".into()), ..Default::default() };
    }
    GithubConnect { state: "none".into(), message: Some(first), ..Default::default() }
}

/// A sign-in gh still holds but GitHub refuses (revoked, expired), as against none at all.
fn rejected(headers: &str, error: &str) -> bool {
    headers.lines().next().is_some_and(|l| l.contains(" 401")) || error.contains("401") || error.to_ascii_lowercase().contains("bad credentials")
}

/// `gh api -i user`: the status line and headers, an empty line, then the account as JSON.
fn parse_account(text: &str) -> GithubConnect {
    let (head, body) = text.split_once("\r\n\r\n").or_else(|| text.split_once("\n\n")).unwrap_or(("", text));
    let scopes = head
        .lines()
        .find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case("x-oauth-scopes")).map(|(_, v)| v))
        .map(|v| v.split(',').map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let user: Value = serde_json::from_str(body.trim()).unwrap_or_default();
    let text = |k: &str| user[k].as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    GithubConnect { state: "ready".into(), login: text("login"), name: text("name"), avatar: text("avatar_url"), scopes, ..Default::default() }
}

/// Signs this machine out of GitHub (`gh auth logout`). Git's own credentials stay as they are.
pub async fn disconnect() -> GithubConnect {
    disconnect_with("gh").await
}

async fn disconnect_with(program: &str) -> GithubConnect {
    let login = account(program).await.login;
    let mut cmd = crate::proc::tokio_command(program);
    cmd.args(["auth", "logout", "--hostname", "github.com"]);
    if let Some(l) = &login {
        cmd.args(["--user", l]);
    }
    cmd.current_dir(std::env::temp_dir()).env("GH_PROMPT_DISABLED", "1").kill_on_drop(true).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    *CACHE.locked() = None;
    match tokio::time::timeout(Duration::from_secs(15), cmd.output()).await {
        Ok(Ok(out)) if !out.status.success() => {
            let err = String::from_utf8_lossy(&out.stderr);
            let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("Could not sign out.").trim().to_owned();
            return GithubConnect { state: "failed".into(), message: Some(first), ..Default::default() };
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => return GithubConnect { state: "missing".into(), message: Some(gh_message(GhError::Missing)), ..Default::default() },
        Ok(Err(e)) => return GithubConnect { state: "failed".into(), message: Some(e.to_string()), ..Default::default() },
        Err(_) => return GithubConnect { state: "failed".into(), message: Some("GitHub did not answer in time.".into()), ..Default::default() },
        Ok(Ok(_)) => {}
    }
    account(program).await
}

/// Starts `gh auth login` and answers once it has printed its code (or ended).
pub async fn start() -> GithubConnect {
    start_with("gh").await
}

async fn start_with(program: &str) -> GithubConnect {
    if JOB.locked().as_ref().is_some_and(|j| j.ended.is_none()) {
        return status().await;
    }
    let mut cmd = crate::proc::tokio_command(program);
    cmd.args(["auth", "login", "--hostname", "github.com", "--git-protocol", "https", "--web"])
        .env("GH_PROMPT_DISABLED", "1")
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return GithubConnect { state: "missing".into(), message: Some(gh_message(GhError::Missing)), ..Default::default() };
        }
        Err(e) => return GithubConnect { state: "failed".into(), message: Some(e.to_string()), ..Default::default() },
    };
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let cancel = std::sync::Arc::new(tokio::sync::Notify::new());
    *JOB.locked() = Some(Job { id, code: None, url: None, last: None, ended: None, cancel: cancel.clone() });
    if let Some(out) = child.stdout.take() {
        tokio::spawn(read_lines(out, id));
    }
    if let Some(err) = child.stderr.take() {
        tokio::spawn(read_lines(err, id));
    }
    tokio::spawn(async move {
        let result = tokio::select! {
            r = child.wait() => match r {
                Ok(s) if s.success() => Ok(()),
                Ok(_) => Err("GitHub sign-in did not finish.".to_owned()),
                Err(e) => Err(e.to_string()),
            },
            _ = cancel.notified() => { child.kill().await.ignore("gh may have ended on its own"); Err("Cancelled.".to_owned()) }
            _ = tokio::time::sleep(SIGN_IN_LIMIT) => { child.kill().await.ignore("gh may have ended on its own"); Err("The code ran out. Start again.".to_owned()) }
        };
        if result.is_ok() {
            // So `git clone` and `git fetch` on this machine use the same sign-in.
            if let Err(crate::proc::GhError::Failed(why)) = gh_within(&std::env::temp_dir(), &["auth", "setup-git", "--hostname", "github.com"], Duration::from_secs(15)).await {
                tracing::warn!("gh auth setup-git failed, so git here may not use the GitHub sign-in: {why}");
            }
            *CACHE.locked() = None;
        }
        let mut job = JOB.locked();
        if let Some(j) = job.as_mut().filter(|j| j.id == id) {
            j.ended = Some(result.map_err(|e| j.last.clone().filter(|_| e.starts_with("GitHub sign-in")).unwrap_or(e)));
        }
    });
    // The code is printed within a second or so; wait for it, but never long.
    for _ in 0..100 {
        {
            let job = JOB.locked();
            if job.as_ref().is_none_or(|j| j.id != id || j.code.is_some() || j.ended.is_some()) {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    status().await
}

async fn read_lines(from: impl AsyncRead + Unpin, id: u64) {
    let mut lines = BufReader::new(from).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut job = JOB.locked();
        let Some(j) = job.as_mut().filter(|j| j.id == id) else { return };
        if j.code.is_none() {
            j.code = one_time_code(&line);
        }
        if j.url.is_none() {
            j.url = line.split_whitespace().find(|w| w.starts_with("https://")).map(|w| w.trim_end_matches(['.', ',']).to_owned());
        }
        if !line.trim().is_empty() {
            j.last = Some(line.trim().trim_start_matches('!').trim().to_owned());
        }
    }
}

/// The code in a line such as `! First copy your one-time code: ABCD-1234`.
pub fn one_time_code(line: &str) -> Option<String> {
    line.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-')).find_map(|w| {
        let (a, b) = w.split_once('-')?;
        let ok = |s: &str| s.len() == 4 && s.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        (ok(a) && ok(b)).then(|| w.to_owned())
    })
}

/// Gives up a sign-in that waits for a code.
pub async fn cancel() -> GithubConnect {
    let job = JOB.locked().take();
    if let Some(j) = job {
        j.cancel.notify_one();
    }
    account("gh").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_is_found_in_gh_output() {
        assert_eq!(one_time_code("! First copy your one-time code: A1B2-C3D4"), Some("A1B2-C3D4".into()));
        assert_eq!(one_time_code("Open this URL to continue in your web browser: https://github.com/login/device"), None);
        assert_eq!(one_time_code("- Authentication complete."), None);
    }

    #[test]
    fn pages_are_joined_and_repeated_repositories_dropped() {
        let page1 = r#"{"data":{"viewer":{"login":"me","repositories":{"pageInfo":{"hasNextPage":true,"endCursor":"x"},"nodes":[
            {"nameWithOwner":"me/shuffle","name":"shuffle","description":"","isPrivate":true,"isArchived":false,"pushedAt":"2026-11-02T10:00:00Z","url":"https://github.com/me/shuffle","owner":{"login":"me"}}]}}}}"#;
        let page2 = r#"{"data":{"viewer":{"login":"me","repositories":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[
            {"nameWithOwner":"crew/Cowboy-Game-Jam","name":"Cowboy-Game-Jam","description":"A jam","isPrivate":false,"isArchived":true,"pushedAt":"2026-11-03T10:00:00Z","url":"https://github.com/crew/Cowboy-Game-Jam","owner":{"login":"crew"}},
            {"nameWithOwner":"ME/shuffle","name":"shuffle","isPrivate":true,"pushedAt":null,"url":"u","owner":{"login":"me"}}]}}}}"#;
        let (login, repos) = parse_repos(&format!("{page1}{page2}"));
        assert_eq!(login.as_deref(), Some("me"));
        assert_eq!(repos.len(), 2, "the same repository twice is one");
        assert_eq!(repos[0].full, "crew/Cowboy-Game-Jam", "newest push first");
        assert!(repos[0].archived && repos[0].description.as_deref() == Some("A jam"));
        assert!(repos[1].private && repos[1].description.is_none() && repos[1].pushed > 0);
    }

    #[test]
    fn account_is_read_from_headers_and_body() {
        let text = "HTTP/2.0 200 OK\r\nContent-Type: application/json\r\nX-Oauth-Scopes: gist, read:org, repo, workflow\r\n\r\n{\"login\":\"me\",\"name\":\"Me Myself\",\"avatar_url\":\"https://avatars.githubusercontent.com/u/1\"}";
        let a = parse_account(text);
        assert_eq!((a.state.as_str(), a.login.as_deref(), a.name.as_deref()), ("ready", Some("me"), Some("Me Myself")));
        assert_eq!(a.scopes, ["gist", "read:org", "repo", "workflow"]);
        assert_eq!(a.avatar.as_deref(), Some("https://avatars.githubusercontent.com/u/1"));
        // A token without scopes (fine-grained) and an account without a name are fine.
        let b = parse_account("HTTP/2.0 200 OK\n\n{\"login\":\"me\",\"name\":null}");
        assert!(b.scopes.is_empty() && b.name.is_none() && b.login.as_deref() == Some("me"));
    }

    #[test]
    fn a_refused_sign_in_is_told_from_no_sign_in() {
        assert!(rejected("HTTP/2.0 401 Unauthorized\n", "gh: Bad credentials (HTTP 401)"));
        assert!(rejected("", "gh: Bad credentials (HTTP 401)"));
        assert!(!rejected("", "To get started with GitHub CLI, please run:  gh auth login"));
    }

    /// A stand-in `gh` that answers `api` with a rejected token and signs out on `auth logout`.
    #[cfg(unix)]
    #[tokio::test]
    async fn disconnect_signs_out_and_reports_the_state_after() {
        let dir = crate::janitor::test_dir();
        let marker = dir.join("out");
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = auth ]; then echo \"$@\" > {m}; exit 0; fi\nif [ -f {m} ]; then echo 'To get started, run: gh auth login' >&2; exit 4; fi\nprintf 'HTTP/2.0 200 OK\\n\\n{{\"login\":\"me\"}}'\n",
            m = marker.display()
        );
        let fake = crate::janitor::test_script(&dir, "gh", &script);
        let got = disconnect_with(fake.to_str().unwrap()).await;
        assert_eq!(got.state, "none");
        assert_eq!(std::fs::read_to_string(&marker).unwrap().trim(), "auth logout --hostname github.com --user me");
    }

    #[test]
    fn nothing_is_not_an_error() {
        assert_eq!(parse_repos("").1.len(), 0);
        assert_eq!(parse_repos("not json").1.len(), 0);
    }

    /// A stand-in `gh` that prints a code and waits, the way `gh auth login --web` does.
    #[cfg(unix)]
    #[tokio::test]
    async fn sign_in_shows_its_code_and_can_be_given_up() {
        let dir = crate::janitor::test_dir();
        let fake = crate::janitor::test_script(&dir, "gh", "#!/bin/sh\necho '! First copy your one-time code: A1B2-C3D4' >&2\necho 'Open this URL to continue in your web browser: https://github.com/login/device' >&2\nsleep 30\n");
        let mut got = start_with(fake.to_str().unwrap()).await;
        assert_eq!(got.state, "waiting");
        assert_eq!(got.code.as_deref(), Some("A1B2-C3D4"));
        // `start` answers once the code is in; the address is the line after it, which a slow
        // machine may not have read yet (the app then shows github.com/login/device itself).
        for _ in 0..50 {
            if got.url.is_some() || got.state != "waiting" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            got = status().await;
        }
        assert_eq!(got.url.as_deref(), Some("https://github.com/login/device"));
        // Asking again while it runs does not start a second one.
        assert_eq!(start_with(fake.to_str().unwrap()).await.code.as_deref(), Some("A1B2-C3D4"));
        let job = JOB.locked().take().expect("a job");
        job.cancel.notify_one();
    }
}
