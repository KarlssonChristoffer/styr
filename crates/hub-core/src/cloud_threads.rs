//! Threads that run on Anthropic's servers instead of a machine of yours: a Claude Code cloud
//! session, started with the `claude` command on the subscription's own login and followed
//! through the sessions Claude Code itself reads. `Thread::cloud` holds where the session stands.
//!
//! Hub starts one and tells it more, and shows status, branch and pull request. The conversation
//! itself lives on claude.ai (the thread's "Open"), since the service gives no way to read it
//! live. There is no engine behind such a thread: its messages go to the session, and its state
//! comes from reading the session now and then (`watch`).
//!
//! A Codex cloud task is started with `codex cloud exec --env <environment>` on the ChatGPT login and
//! read with `codex cloud list --json`. The environment is the id earlier tasks of the repository ran
//! in (their label is the repository), else the repository's name; when Codex knows none, the thread
//! says to create one in ChatGPT. Codex takes no follow-up from the
//! command line, so a second message to such a thread is refused.
//!
//! Only the subscription is used. When it is full the session stops; Hub never falls over to an
//! API key.

mod codex_env;
use codex_env::codex_envs;
use crate::locked::ResultExt;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use futures::future::BoxFuture;

use crate::model::{CloudPr, CloudRun, Event, EventData, EventKind, Thread, ThreadStatus, now_ms};
use crate::store::Store;

/// What a cloud thread needs from Hub: its store, its card, its status and its events. Hub
/// answers it (`hub/cloud_threads.rs`), so this module sits below Hub.
pub trait Threads: Send + Sync + 'static {
    fn store(&self) -> &Store;
    fn edit_thread<'a>(&'a self, thread_id: &'a str, f: Box<dyn FnOnce(&mut Thread) + Send>) -> BoxFuture<'a, Result<Thread>>;
    fn set_status<'a>(&'a self, thread_id: &'a str, status: ThreadStatus) -> BoxFuture<'a, Result<Thread>>;
    fn set_status_because<'a>(&'a self, thread_id: &'a str, status: ThreadStatus, why: &'a str) -> BoxFuture<'a, Result<Thread>>;
    fn emit<'a>(&'a self, thread_id: &'a str, kind: EventKind, data: Value) -> BoxFuture<'a, Result<Event>>;

    fn update_thread<'a>(&'a self, thread_id: &'a str, f: impl FnOnce(&mut Thread) + Send + 'static) -> BoxFuture<'a, Result<Thread>> {
        self.edit_thread(thread_id, Box::new(f))
    }

    fn emit_typed<'a, T: EventData>(&'a self, thread_id: &'a str, data: &T) -> BoxFuture<'a, Result<Event>> {
        self.emit(thread_id, T::KIND, data.to_json())
    }
}

/// How often the sessions of cloud threads that are still going are read.
const EVERY: Duration = Duration::from_secs(30);
/// How long starting a session may take before Hub gives up on it.
const START_LIMIT: Duration = Duration::from_secs(180);

/// Which cloud an engine starts a thread in. Only the plain Claude and Codex harnesses on their
/// own subscription have one.
pub fn provider_of(engine: &str) -> Option<&'static str> {
    match engine {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        _ => None,
    }
}

/// What a new cloud thread starts as, or why it cannot.
pub fn begin(engine: &str, has_task: bool) -> Result<CloudRun> {
    let Some(provider) = provider_of(engine) else {
        bail!("{engine} has no cloud to run in; use the Claude or Codex harness on its own subscription");
    };
    if !has_task {
        bail!("a cloud thread starts with its task: write it first");
    }
    Ok(CloudRun { provider: provider.into(), state: "starting".into(), ..Default::default() })
}

/// The session's id and address in what `claude --cloud` printed, or in the JSON a follow-up
/// answers with.
pub fn parse_started(output: &str) -> Option<(String, Option<String>)> {
    if let Ok(v) = serde_json::from_str::<Value>(output.trim()) {
        let id = v["session_id"].as_str().or_else(|| v["id"].as_str());
        if let Some(id) = id {
            return Some((id.to_owned(), v["url"].as_str().map(str::to_owned)));
        }
    }
    let at = output.find("claude.ai/code/")?;
    let rest = &output[at + "claude.ai/code/".len()..];
    let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')).collect();
    if id.len() < 8 {
        return None;
    }
    let start = output[..at].rfind(|c: char| c.is_whitespace()).map_or(0, |i| i + 1);
    let url = format!("{}claude.ai/code/{id}", &output[start..at]);
    let url = if url.starts_with("http") { url } else { format!("https://claude.ai/code/{id}") };
    Some((id, Some(url)))
}

/// The task's id and address in what `codex cloud exec` printed (`https://chatgpt.com/codex/tasks/<id>`).
pub fn parse_codex_started(output: &str) -> Option<(String, Option<String>)> {
    let at = output.find("/codex/tasks/")?;
    let rest = &output[at + "/codex/tasks/".len()..];
    let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')).collect();
    if id.len() < 6 {
        return None;
    }
    let start = output[..at].rfind(|c: char| c.is_whitespace()).map_or(0, |i| i + 1);
    let url = format!("{}/codex/tasks/{id}", &output[start..at]);
    Some((id, Some(url).filter(|u| u.starts_with("http"))))
}

/// What `codex cloud list --json` says of task `id`. The listing may be an array or an object
/// holding one; a task is matched by its id, and a status word Hub does not know stays unknown.
pub fn read_codex_list(list: &Value, id: &str) -> Option<Read> {
    let tasks = list.as_array().or_else(|| ["tasks", "items", "data"].iter().find_map(|k| list[*k].as_array()))?;
    let task = tasks.iter().find(|t| t["id"].as_str() == Some(id) || t["id"].as_str().is_some_and(|i| i.ends_with(id) || id.ends_with(i)))?;
    let state = ["status", "state"].iter().find_map(|k| task[*k].as_str()).and_then(state_of);
    let last = task["title"].as_str().or_else(|| task["summary"].as_str()).map(|s| s.chars().take(200).collect());
    Some(Read { state, branch: None, pr: find_pr(&task.to_string()), last })
}

/// `owner/name` of a GitHub remote address.
pub fn repo_of(remote: &str) -> Option<String> {
    let rest = remote.trim().trim_end_matches(".git").trim_end_matches('/');
    let tail = rest.split("github.com").nth(1)?.trim_start_matches([':', '/']);
    let mut parts = tail.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    (!owner.is_empty() && !name.is_empty()).then(|| format!("{owner}/{name}"))
}

/// A pull request address found anywhere in what the service said.
pub fn find_pr(text: &str) -> Option<CloudPr> {
    let re = regex::Regex::new(r"https://github\.com/[\w.-]+/[\w.-]+/pull/(\d+)").ok()?;
    let m = re.captures(text)?;
    Some(CloudPr { number: m[1].parse().ok()?, title: None, url: m[0].to_owned() })
}

/// What reading a session said, as far as Hub can tell.
#[derive(Debug, Default, PartialEq)]
pub struct Read {
    pub state: Option<&'static str>,
    pub branch: Option<String>,
    pub pr: Option<CloudPr>,
    pub last: Option<String>,
}

/// The state of a session from the words the service uses for it, with or without its
/// `SESSION_STATUS_` prefix. A session that is `idle` has answered and waits for you: that is
/// "waiting", never "done", which only a session that has really ended is. Unknown words are not
/// a state.
fn state_of(word: &str) -> Option<&'static str> {
    let word = word.to_ascii_lowercase();
    let word = word.trim_start_matches("session_status_bucket_").trim_start_matches("session_status_");
    match word {
        "running" | "active" | "working" | "in_progress" | "starting" | "pending" => Some("working"),
        "completed" | "complete" | "done" | "finished" | "archived" | "ready" | "applied" => Some("done"),
        "idle" | "review_ready" | "requires_action" | "blocked" | "waiting" | "needs_input" | "needs_action" => Some("waiting"),
        "failed" | "error" | "errored" => Some("failed"),
        "stopped" | "cancelled" | "canceled" | "interrupted" | "terminated" => Some("stopped"),
        _ => None,
    }
}

/// What the agent said in the events of a session (`GET /v1/sessions/{id}/events`), oldest first,
/// and the id of the last event of the page. The events are not documented, so an event is read
/// as an assistant turn wherever it keeps that (`assistant`, or `type` "assistant"), only its text
/// blocks count, and a sub-agent's turns (with a parent tool call) are left out.
pub fn read_events(page: &Value) -> (Vec<String>, Option<String>) {
    let events = page["data"].as_array().or_else(|| page["events"].as_array()).map(Vec::as_slice).unwrap_or_default();
    let mut said = Vec::new();
    for e in events {
        let body = e.get("assistant").unwrap_or(e);
        let assistant = e.get("assistant").is_some() || e["type"] == "assistant" || body["type"] == "assistant";
        let content = ["/internal_anthropic_catchall/message/content", "/message/content", "/content"].iter().find_map(|p| body.pointer(p)).and_then(Value::as_array);
        let sub = ["/parent_tool_use_id", "/internal_anthropic_catchall/parent_tool_use_id"].iter().any(|p| body.pointer(p).is_some_and(|v| !v.is_null()));
        if !assistant || sub {
            continue;
        }
        let text: Vec<&str> = content.into_iter().flatten().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).map(str::trim).filter(|t| !t.is_empty()).collect();
        if !text.is_empty() {
            said.push(text.join("\n\n"));
        }
    }
    let last = page["last_id"].as_str().map(str::to_owned).or_else(|| {
        events.last().and_then(|e| ["/assistant/uuid", "/uuid", "/id", "/event_id"].iter().find_map(|p| e.pointer(p)).and_then(Value::as_str)).map(str::to_owned)
    });
    (said, last)
}

/// Reads a session as `GET /v1/sessions/{id}` gives it. The shape is not documented, so each
/// field is looked for where Claude Code's own client keeps it, and one that is not there is
/// just left unknown.
pub fn read_session(session: &Value) -> Read {
    let state = ["/session_status", "/status", "/status_bucket", "/external_metadata/state", "/external_metadata/status"]
        .iter()
        .find_map(|p| session.pointer(p).and_then(Value::as_str))
        .and_then(state_of);
    let branch = session
        .pointer("/session_context/outcomes")
        .and_then(Value::as_array)
        .and_then(|o| o.iter().find_map(|o| o.pointer("/git_info/branches/0").and_then(Value::as_str)))
        .or_else(|| session.pointer("/external_metadata/branch").and_then(Value::as_str))
        .map(str::to_owned);
    let last = ["/external_metadata/summary", "/external_metadata/current_activity"]
        .iter()
        .find_map(|p| session.pointer(p).and_then(Value::as_str))
        .map(|s| s.chars().take(200).collect());
    Read { state, branch, pr: find_pr(&session.to_string()), last }
}

/// The folder's GitHub repository and branch, for the card.
async fn where_is(cwd: &Path) -> (Option<String>, Option<String>) {
    async fn git(cwd: &Path, args: &[&str]) -> Option<String> {
        let out = crate::git::output_async(cwd, args, Duration::from_secs(5)).await.ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned()).filter(|s| !s.is_empty())
    }
    let repo = git(cwd, &["remote", "get-url", "origin"]).await.and_then(|r| repo_of(&r));
    (repo, git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).await)
}

/// What a message to a cloud thread does: the first one starts the session, later ones are
/// told to it. Returns at once; the command runs in the background and the thread shows the
/// result.
pub async fn prompt<H: Threads>(hub: &Arc<H>, thread: &Thread, text: String) -> Result<()> {
    let cloud = thread.cloud.clone().context("not a cloud thread")?;
    if matches!(cloud.state.as_str(), "starting") && !cloud.session_id.is_empty() {
        bail!("the cloud session is still starting; wait a moment");
    }
    if cloud.provider == "codex" && !cloud.session_id.is_empty() {
        bail!("a Codex cloud task takes no follow-up from Hub; answer it on chatgpt.com or start a new thread");
    }
    hub.set_status_because(&thread.id, ThreadStatus::Running, "cloud_prompt").await?;
    let (id, cwd) = (thread.id.clone(), thread.cwd.clone());
    let hub = hub.clone();
    tokio::spawn(async move {
        let result = if cloud.session_id.is_empty() {
            if cloud.provider == "codex" { start_codex(&hub, &id, &cwd, &text).await } else { start(&hub, &id, &cwd, &text).await }
        } else {
            follow_up(&hub, &id, &cloud.session_id, &cwd, &text).await
        };
        if let Err(e) = result {
            fail(&hub, &id, &format!("{e:#}")).await;
        }
    });
    Ok(())
}

async fn said<H: Threads>(hub: &Arc<H>, id: &str, text: &str) {
    let data = json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": text } });
    hub.emit(id, EventKind::Update, data).await.warn_on_err("emit Update");
}

/// The session could not start or be told: the card says why, and the thread asks for you.
async fn fail<H: Threads>(hub: &Arc<H>, id: &str, why: &str) {
    let why = why.to_owned();
    let note = why.clone();
    hub
        .update_thread(id, move |t| {
            if let Some(c) = &mut t.cloud {
                c.note = Some(note);
                if c.session_id.is_empty() {
                    c.state = "failed".into();
                }
            }
        })
        .await.warn_on_err("save the cloud failure note");
    hub.emit_typed(id, &crate::model::ErrorData::new(why)).await.warn_on_err("emit Error");
    hub.set_status(id, ThreadStatus::Error).await.warn_on_err("set status Error");
}

/// Where `program` is on this machine, found the way the engines are (the npm global folder and
/// the `.cmd` shims on Windows included), not by the bare name a child process would be given.
fn program_path(program: &str) -> Result<std::path::PathBuf> {
    crate::engines::locate(program).with_context(|| format!("the {program} command is not installed on this machine"))
}

/// The real program behind an npm `.cmd` shim (`"%dp0%\\node_modules\\...\\claude.exe"`), so the
/// task text is never given to `cmd.exe` to read.
fn unshim(program: &Path) -> Option<std::path::PathBuf> {
    if !program.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd")) {
        return None;
    }
    let text = std::fs::read_to_string(program).ok()?;
    let re = regex::Regex::new(r#"%(?:~dp0|dp0%)\\?([^"%\r\n]*\.exe)"#).ok()?;
    // The shim's last such path is the program; earlier ones can be the `node.exe` it checks for.
    let rel = re.captures_iter(&text).filter_map(|c| c.get(1).map(|m| m.as_str().to_owned())).collect::<Vec<_>>().into_iter().rfind(|p| !p.to_ascii_lowercase().ends_with("node.exe"))?.replace('\\', "/");
    let real = program.parent()?.join(rel);
    real.is_file().then_some(real)
}

/// What a terminal program asks of the terminal that Hub answers: where the cursor is (`ESC[6n`) and
/// what kind of terminal this is (`ESC[c`). Without the first, Windows' ConPTY prints nothing at all.
fn answer_queries(chunk: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(chunk);
    let mut out = Vec::new();
    for _ in 0..text.matches("\x1b[6n").count() {
        out.extend_from_slice(b"\x1b[1;1R");
    }
    for _ in 0..text.matches("\x1b[c").count() + text.matches("\x1b[0c").count() {
        out.extend_from_slice(b"\x1b[?1;2c");
    }
    out
}

/// The last lines a failed start printed, on one line and short, so the card shows the real reason.
fn tail_of(seen: &str) -> String {
    let lines: Vec<&str> = seen.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(6)..].join(" | ");
    if tail.chars().count() > 500 { tail.chars().skip(tail.chars().count() - 500).collect() } else { tail }
}

/// Runs `program args` in a pseudo-terminal in `cwd` and reads what it prints until the cloud
/// session's address shows (or `START_LIMIT` passes), then ends it. Returns all it printed, with
/// the escape codes taken out, and the session found in it.
async fn in_terminal(program: &Path, args: &[String], cwd: &str) -> Result<(String, Option<(String, Option<String>)>)> {
    let (program, args, cwd) = (program.to_owned(), args.to_vec(), cwd.to_owned());
    tokio::task::spawn_blocking(move || {
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};
        use std::io::Read;
        let pair = native_pty_system().openpty(PtySize { rows: 50, cols: 500, pixel_width: 0, pixel_height: 0 }).context("opening a terminal")?;
        let program = unshim(&program).unwrap_or(program);
        let shim = cfg!(windows) && program.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
        if shim && args.iter().any(|a| a.contains(['%', '"', '^', '&', '|', '<', '>', '\n', '\r'])) {
            bail!("the claude command is a .cmd shim that cannot take this text safely; install Claude Code's own program (claude.ai/install) or leave out % \" ^ & | < > and line breaks");
        }
        let mut cmd = if shim {
            let mut c = CommandBuilder::new("cmd.exe");
            c.args(["/C".to_owned(), program.to_string_lossy().into_owned()]);
            c
        } else {
            CommandBuilder::new(&program)
        };
        cmd.args(&args);
        cmd.cwd(&cwd);
        cmd.env("TERM", "xterm-256color");
        let mut child = pair.slave.spawn_command(cmd).with_context(|| format!("starting {}", program.display()))?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let mut writer = pair.master.take_writer()?;
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let deadline = std::time::Instant::now() + START_LIMIT;
        let mut raw = Vec::new();
        let mut seen = String::new();
        let mut carry: Vec<u8> = Vec::new();
        let mut found = None;
        while found.is_none() {
            let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) else { break };
            match rx.recv_timeout(left) {
                Ok(chunk) => {
                    // Windows' terminal layer asks where the cursor is and prints nothing until told.
                    let mut window = carry.clone();
                    window.extend_from_slice(&chunk);
                    let answers = answer_queries(&window);
                    carry = window[window.len().saturating_sub(3)..].to_vec();
                    if !answers.is_empty() {
                        std::io::Write::write_all(&mut writer, &answers).warn_on_err("answer the terminal query");
                    }
                    raw.extend_from_slice(&chunk);
                    seen = crate::text::strip_ansi(&String::from_utf8_lossy(&raw));
                    found = parse_started(&seen);
                }
                Err(_) => break,
            }
        }
        // The session exists on the service once its address is out; the command only waits on it.
        let ended = child.try_wait().ok().flatten().map(|s| format!("the command ended with {s}"));
        child.kill().warn_on_err("end the cloud start command");
        if found.is_none() && !seen.trim().is_empty() {
            tracing::debug!("cloud start printed: {seen}");
        }
        if found.is_none() && seen.trim().is_empty() {
            seen = ended.unwrap_or_else(|| "it printed nothing before the time ran out".into());
        }
        drop(pair.master);
        Ok((seen, found))
    })
    .await?
}

async fn start<H: Threads>(hub: &Arc<H>, id: &str, cwd: &str, task: &str) -> Result<()> {
    let (repo, branch) = where_is(Path::new(cwd)).await;
    if repo.is_none() {
        bail!("the cloud works on a copy from GitHub, and this folder has no GitHub remote");
    }
    // `claude --cloud <text>` only creates a session from a terminal (without one it insists on an
    // existing session id), so it runs in a pseudo-terminal until its address shows.
    let program = program_path("claude")?;
    let (seen, found) = in_terminal(&program, &["--cloud".to_owned(), task.to_owned()], cwd).await?;
    let Some((session_id, url)) = found else {
        bail!("the cloud session did not start: {}", tail_of(&seen));
    };
    let link = url.clone();
    let (r, b) = (repo.clone(), branch.clone());
    hub.update_thread(id, move |t| {
        if let Some(c) = &mut t.cloud {
            c.session_id = session_id;
            c.url = url;
            c.repo = r;
            c.branch = b;
            c.state = "working".into();
            c.note = None;
            c.awaiting = true;
        }
    })
    .await?;
    said(hub, id, &format!("Started in the cloud{}. It works on a copy of {} and ends in a branch and a pull request.", link.map(|l| format!(": {l}")).unwrap_or_default(), repo.unwrap_or_default())).await;
    Ok(())
}

/// The environments a Codex task for `repo` may run in, best first: the ids of environments that
/// earlier tasks in `list` (the `codex cloud list --json` answer) ran in and whose label is this
/// repository (or its bare name), then the repository's own name and its bare name, which Codex
/// matches against labels itself.
pub fn codex_env_candidates(list: &Value, repo: &str) -> Vec<String> {
    let name = repo.rsplit('/').next().unwrap_or(repo);
    let mut out: Vec<String> = Vec::new();
    let tasks = list.as_array().or_else(|| ["tasks", "items", "data"].iter().find_map(|k| list[*k].as_array()));
    for task in tasks.into_iter().flatten() {
        let field = |keys: &[&str]| keys.iter().find_map(|k| task[*k].as_str()).map(str::to_owned);
        let (Some(label), Some(id)) = (field(&["environment_label", "environmentLabel", "env_label"]), field(&["environment_id", "environmentId", "env_id"])) else { continue };
        let label = label.trim();
        if (label.eq_ignore_ascii_case(repo) || label.eq_ignore_ascii_case(name)) && !out.contains(&id) {
            out.push(id);
        }
    }
    for fallback in [repo, name] {
        if !out.iter().any(|c| c == fallback) {
            out.push(fallback.to_owned());
        }
    }
    out
}

/// The environments in the ChatGPT account as far as earlier tasks show them; empty when the
/// listing fails (the start then tries the repository's name alone).
async fn codex_list(program: &Path) -> Value {
    let mut cmd = crate::proc::tokio_command(program);
    cmd.args(["cloud", "list", "--json", "--limit", "20"]).stdin(std::process::Stdio::null()).kill_on_drop(true);
    match tokio::time::timeout(Duration::from_secs(30), cmd.output()).await {
        Ok(Ok(out)) if out.status.success() => serde_json::from_slice(&out.stdout).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// `codex cloud exec` in environment `env`: the task and its address, or what Codex said instead.
async fn codex_exec(program: &Path, cwd: &str, env: &str, task: &str) -> Result<std::result::Result<(String, Option<String>), String>> {
    let mut cmd = crate::proc::tokio_command(program);
    cmd.args(["cloud", "exec", "--env", env, task]).current_dir(cwd).stdin(std::process::Stdio::null()).kill_on_drop(true);
    let out = tokio::time::timeout(START_LIMIT, cmd.output())
        .await
        .context("Codex did not answer in time")?
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { anyhow::anyhow!("the codex command is not installed on this machine") } else { e.into() })?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if let Some(started) = out.status.success().then(|| parse_codex_started(&stdout)).flatten() {
        return Ok(Ok(started));
    }
    let said: Vec<&str> = stderr.lines().chain(stdout.lines()).map(str::trim).filter(|l| !l.is_empty()).take(3).collect();
    Ok(Err(if said.is_empty() { "it gave no task".into() } else { said.join(" ") }))
}

async fn start_codex<H: Threads>(hub: &Arc<H>, id: &str, cwd: &str, task: &str) -> Result<()> {
    let (repo, branch) = where_is(Path::new(cwd)).await;
    let Some(repo) = repo else { bail!("the cloud works on a copy from GitHub, and this folder has no GitHub remote") };
    let program = program_path("codex")?;
    let (picked, all) = codex_envs(&repo).await;
    let mut candidates: Vec<String> = picked.into_iter().map(|e| e.id).collect();
    for c in codex_env_candidates(&codex_list(&program).await, &repo) {
        if !candidates.contains(&c) {
            candidates.push(c);
        }
    }
    let mut started = None;
    let mut last = String::new();
    for env in &candidates {
        match codex_exec(&program, cwd, env, task).await? {
            Ok(found) => {
                started = Some(found);
                break;
            }
            // Only "no such environment" moves on to the next name; anything else is the answer.
            Err(said) if said.to_ascii_lowercase().contains("environment") && said.contains("not found") => last = said,
            Err(said) => bail!("the Codex cloud task did not start: {said}"),
        }
    }
    let Some((task_id, url)) = started else {
        let have = if all.is_empty() { String::new() } else { format!(" The account has: {}.", all.iter().map(|e| if e.label.is_empty() { e.id.clone() } else { e.label.clone() }).collect::<Vec<_>>().join(", ")) };
        bail!("Codex has no cloud environment for {repo} ({}).{have} Connect the repository to one in ChatGPT (Codex, Settings, Environments) and start the thread again.", last.trim());
    };
    let link = url.clone();
    hub.update_thread(id, move |t| {
        if let Some(c) = &mut t.cloud {
            c.session_id = task_id;
            c.url = url;
            c.repo = Some(repo);
            c.branch = branch;
            c.state = "working".into();
            c.note = None;
        }
    })
    .await?;
    said(hub, id, &format!("Started in the Codex cloud{}. It works on a copy from GitHub; open it there to review and open the pull request.", link.map(|l| format!(": {l}")).unwrap_or_default())).await;
    Ok(())
}

async fn follow_up<H: Threads>(hub: &Arc<H>, id: &str, session: &str, cwd: &str, text: &str) -> Result<()> {
    let mut cmd = crate::proc::tokio_command(program_path("claude")?);
    cmd.args(["-p", text, "--cloud", session, "--output-format", "json"]).current_dir(cwd);
    cmd.stdin(std::process::Stdio::null()).kill_on_drop(true);
    let out = tokio::time::timeout(START_LIMIT, cmd.output()).await.context("the cloud session did not answer in time")??;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().chain(stdout.lines()).find(|l| !l.trim().is_empty()).unwrap_or("it refused");
        bail!("could not tell the cloud session: {}", first.trim());
    }
    if let Ok(v) = serde_json::from_str::<Value>(stdout.trim())
        && v["ok"] == false
    {
        bail!("the cloud session did not take the message");
    }
    hub.update_thread(id, |t| {
        if let Some(c) = &mut t.cloud {
            c.state = "working".into();
            c.note = None;
            c.awaiting = true;
        }
    })
    .await?;
    Ok(())
}

/// Stop: the session itself cannot be stopped from here (the service has no call for it that
/// Hub may use), so the thread is marked stopped and its page opens for the real thing.
pub async fn stop<H: Threads>(hub: &Arc<H>, id: &str) -> Result<()> {
    let Some(thread) = hub.store().thread(id).await? else { return Ok(()) };
    if thread.cloud.is_none() {
        return Ok(());
    }
    hub.update_thread(id, |t| {
        if let Some(c) = &mut t.cloud
            && matches!(c.state.as_str(), "starting" | "working" | "waiting")
        {
            c.state = "stopped".into();
            c.note = Some("Hub stopped following it. The session keeps running on claude.ai until you stop it there.".into());
        }
    })
    .await?;
    hub.set_status_because(id, ThreadStatus::Idle, "cloud_following_stopped").await?;
    Ok(())
}

/// Reads the sessions of the cloud threads that are still going, and moves each thread to where
/// its session stands.
pub async fn watch<H: Threads>(hub: Arc<H>) {
    loop {
        tokio::time::sleep(EVERY).await;
        if let Err(e) = pass(&hub).await {
            tracing::debug!("cloud threads: {e:#}");
        }
    }
}

async fn pass<H: Threads>(hub: &Arc<H>) -> Result<()> {
    let going: Vec<Thread> = hub
        .store()
        .threads()
        .await?
        .into_iter()
        .filter(|t| t.cloud.as_ref().is_some_and(|c| !c.session_id.is_empty() && matches!(c.state.as_str(), "working" | "waiting")))
        .collect();
    let (codex, live): (Vec<Thread>, Vec<Thread>) = going.into_iter().partition(|t| t.cloud.as_ref().is_some_and(|c| c.provider == "codex"));
    if !codex.is_empty() {
        pass_codex(hub, codex).await;
    }
    if live.is_empty() {
        return Ok(());
    }
    let client = reqwest::Client::new();
    let mut login = match crate::cloud_usage::login(now_ms()) {
        Ok(l) => l,
        Err(why) => {
            for t in &live {
                note(hub, &t.id, Some(format!("Hub cannot read the session: {why}."))).await;
            }
            return Ok(());
        }
    };
    if login.org.is_none() {
        login.org = Some(crate::cloud_usage::profile_org(&client, &login.token).await?);
    }
    for t in live {
        let Some(c) = &t.cloud else { continue };
        match crate::cloud_usage::get(&client, &login, &format!("/v1/sessions/{}", c.session_id)).await {
            Ok(session) => {
                let mut read = read_session(&session);
                let events = fetch_events(&client, &login, c).await;
                match events {
                    Ok((texts, last)) => {
                        let replied = !texts.is_empty();
                        for text in &texts {
                            said(hub, &t.id, text).await;
                        }
                        let (seen, awaiting) = (last.or_else(|| c.seen.clone()), c.awaiting && !replied);
                        if (seen != c.seen || awaiting != c.awaiting)
                            && let Err(e) = hub.update_thread(&t.id, move |t| {
                                if let Some(c) = &mut t.cloud {
                                    c.seen = seen;
                                    c.awaiting = awaiting;
                                }
                            })
                            .await
                        {
                            tracing::debug!("cloud threads: {e:#}");
                        }
                        // A session that looks idle before its answer has been read is still on it.
                        if awaiting && read.state == Some("waiting") {
                            read.state = Some("working");
                        }
                        if replied {
                            read.last = texts.last().map(|t| t.chars().take(200).collect());
                        }
                    }
                    Err(e) => tracing::debug!("cloud threads: cannot read events of {}: {e:#}", c.session_id),
                }
                // Re-read the thread: the events above changed it.
                let t = hub.store().thread(&t.id).await.ok().flatten().unwrap_or(t);
                apply(hub, &t, read).await;
            }
            Err(e) => note(hub, &t.id, Some(format!("Hub cannot read the session now: {e:#}"))).await,
        }
    }
    Ok(())
}

/// The agent's new replies in the session since the last look (all of them on the first), and the
/// id to continue from.
async fn fetch_events(client: &reqwest::Client, login: &crate::cloud_usage::Login, c: &CloudRun) -> Result<(Vec<String>, Option<String>)> {
    let mut said = Vec::new();
    let mut after = c.seen.clone();
    for _ in 0..5 {
        let path = match &after {
            Some(a) => format!("/v1/sessions/{}/events?limit=100&after_id={a}", c.session_id),
            None => format!("/v1/sessions/{}/events?limit=100", c.session_id),
        };
        let page = crate::cloud_usage::get(client, login, &path).await?;
        let (texts, last) = read_events(&page);
        said.extend(texts);
        let more = page["has_more"] == true && last.is_some() && last != after;
        after = last.or(after);
        if !more || c.seen.is_none() {
            break;
        }
    }
    Ok((said, after))
}

/// Reads the Codex tasks once for all the threads that wait on one.
async fn pass_codex<H: Threads>(hub: &Arc<H>, threads: Vec<Thread>) {
    let Ok(program) = program_path("codex") else { return };
    let mut cmd = crate::proc::tokio_command(program);
    cmd.args(["cloud", "list", "--json"]).stdin(std::process::Stdio::null()).kill_on_drop(true);
    let out = match tokio::time::timeout(Duration::from_secs(30), cmd.output()).await {
        Ok(Ok(out)) if out.status.success() => out,
        other => {
            let why = match other {
                Ok(Ok(out)) => String::from_utf8_lossy(&out.stderr).lines().find(|l| !l.trim().is_empty()).unwrap_or("it refused").trim().to_owned(),
                Ok(Err(e)) => e.to_string(),
                Err(_) => "it did not answer in time".into(),
            };
            for t in &threads {
                note(hub, &t.id, Some(format!("Hub cannot read the Codex task now: {why}"))).await;
            }
            return;
        }
    };
    let Ok(list) = serde_json::from_slice::<Value>(&out.stdout) else { return };
    for t in threads {
        let Some(c) = &t.cloud else { continue };
        if let Some(read) = read_codex_list(&list, &c.session_id) {
            apply(hub, &t, read).await;
        }
    }
}

async fn note<H: Threads>(hub: &Arc<H>, id: &str, text: Option<String>) {
    let same = hub.store().thread(id).await.ok().flatten().and_then(|t| t.cloud).is_some_and(|c| c.note == text);
    if !same {
        hub
            .update_thread(id, move |t| {
                if let Some(c) = &mut t.cloud {
                    c.note = text;
                }
            })
            .await.warn_on_err("save the cloud note");
    }
}

/// Moves a thread to what its session said, and says in the log what is new.
async fn apply<H: Threads>(hub: &Arc<H>, t: &Thread, read: Read) {
    let Some(before) = &t.cloud else { return };
    let new_pr = read.pr.as_ref().filter(|p| before.pr.as_ref().is_none_or(|b| b.number != p.number)).cloned();
    let state = read.state.unwrap_or(match before.state.as_str() {
        "working" => "working",
        "waiting" => "waiting",
        _ => "working",
    });
    let changed = state != before.state
        || read.branch.as_ref().is_some_and(|b| before.branch.as_ref() != Some(b))
        || new_pr.is_some()
        || read.last.as_ref().is_some_and(|l| before.last.as_ref() != Some(l))
        || before.note.is_some();
    if !changed {
        return;
    }
    let (branch, pr, last) = (read.branch.clone(), read.pr.clone(), read.last.clone());
    let to = state;
    hub
        .update_thread(&t.id, move |t| {
            if let Some(c) = &mut t.cloud {
                c.state = to.into();
                if branch.is_some() {
                    c.branch = branch;
                }
                if pr.is_some() {
                    c.pr = pr;
                }
                if last.is_some() {
                    c.last = last;
                }
                c.note = None;
            }
        })
        .await.warn_on_err("save the cloud state");
    if let Some(pr) = new_pr {
        said(hub, &t.id, &format!("Pull request #{} is open: {}", pr.number, pr.url)).await;
    }
    if state != before.state {
        match state {
            "done" => {
                said(hub, &t.id, "The cloud session has ended.").await;
                hub.set_status_because(&t.id, ThreadStatus::Idle, "cloud_done").await.warn_on_err("set status Idle");
            }
            "waiting" => {
                hub.set_status_because(&t.id, ThreadStatus::WaitingOnYou, "cloud_waiting").await.warn_on_err("set status WaitingOnYou");
            }
            "failed" => fail(hub, &t.id, "The cloud session failed. Open it on claude.ai to see why.").await,
            "stopped" => {
                hub.set_status_because(&t.id, ThreadStatus::Idle, "cloud_stopped").await.warn_on_err("set status Idle");
            }
            _ => {
                hub.set_status_because(&t.id, ThreadStatus::Running, "cloud_working").await.warn_on_err("set status Running");
            }
        }
    }
}

#[cfg(test)]
mod tests;
