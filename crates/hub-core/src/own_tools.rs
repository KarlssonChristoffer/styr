//! Your own tools in Hub: a program that keeps a `hub.toml` in its folder (Forman, Knegarn,
//! Aftermath…) shows up in the Workers view with what it is doing, what waits on you and the
//! commands Hub may run on it, and the agents get the same through Hub's tools.
//!
//! Hub knows nothing about any one tool. The file names commands that print one JSON document,
//! and where in that document the headline, the state and the lists are; actions are command
//! lines too, so Hub never has a button the tool's own CLI lacks. Commands run in the tool's
//! folder on the machine it is on, without a shell: an input fills a whole argument or part of
//! one, and is never parsed. Each machine's core finds the tools in its own folders; the app asks
//! the others with `?host=`. The format is in ARCHITECTURE.md, "Your tools".

use crate::locked::{LockExt, ResultExt};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::ctx::Ctx;
use crate::model::{
    EventKind, HandedOffData, HandoffBrief, HandoffOfferData, HandoffResultData, OwnTool, OwnToolFound, OwnToolAction, OwnToolHandoff, OwnToolItem, OwnToolList, OwnToolRan, RunOwnToolAction, now_ms,
};

pub const FILE: &str = "hub.toml";
/// How long a command may take before it counts as failed.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(30);
/// How long a reading is reused, so a view polling every few seconds does not run a tool's status
/// command every few seconds.
const FRESH: Duration = Duration::from_secs(10);
/// What an action's output may hold when it comes back.
const OUTPUT_CHARS: usize = 4_000;
/// Items a list shows unless its file says otherwise.
const LIMIT: usize = 8;

static READINGS: LazyLock<Mutex<HashMap<PathBuf, (Instant, OwnTool)>>> = LazyLock::new(Mutex::default);

/// A `hub.toml`, as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    #[serde(default)]
    pub about: Option<String>,
    /// Environment for every command, on top of the core's own. `${VAR}` and `~` work in values.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub status: Option<Status>,
    #[serde(default, rename = "list")]
    pub lists: Vec<List>,
    #[serde(default, rename = "action")]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub handoff: Option<Handoff>,
    #[serde(default)]
    pub worker: Option<Worker>,
}

/// A Worker: a tool that does whole jobs by itself, each a run Hub shows as a thread
/// (`worker_runs.rs`). `runs` prints the runs; `events` prints one run's events after `{seq}`,
/// so Hub replays each run once with its own cursor. Pointers say where things are; the event
/// pointers have defaults that fit a journal of `{ seq, kind, phase, round, detail, data }`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Worker {
    /// What one run is called, such as "Knegarn run".
    pub label: String,
    pub runs: Vec<String>,
    #[serde(default = "Worker::runs_at")]
    pub items: String,
    pub id: String,
    pub title: String,
    /// The work reference a handoff gave it (`hub:thread:<id>`), which ties the run to that thread.
    #[serde(default)]
    pub work: Option<String>,
    /// When it started, so runs that ended long ago do not each get a thread.
    #[serde(default)]
    pub started: Option<String>,
    /// Empty or null while the run goes on.
    pub ended: String,
    /// Its own word for how it ended, such as "completed".
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub said: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub cost: Option<String>,
    pub events: Vec<String>,
    #[serde(default = "Worker::events_at")]
    pub event_items: String,
    #[serde(default = "Worker::seq_at")]
    pub seq: String,
    #[serde(default = "Worker::phase_at")]
    pub phase: String,
    #[serde(default = "Worker::round_at")]
    pub round: String,
    #[serde(default = "Worker::kind_at")]
    pub kind: String,
    #[serde(default = "Worker::text_at")]
    pub text: String,
    /// Which agent an event is about.
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// A field that reads "pause" when the run paused and "resume" when it went on.
    #[serde(default)]
    pub pause: Option<String>,
    #[serde(default, rename = "step")]
    pub steps: Vec<WorkerStepDef>,
    #[serde(default, rename = "control")]
    pub controls: Vec<WorkerControlDef>,
    /// Its ready check, run after it is installed: `{ ok, checks: [{ section, status, label,
    /// detail }] }`.
    #[serde(default)]
    pub ready: Option<Vec<String>>,
}

impl Worker {
    fn runs_at() -> String {
        "/runs".into()
    }
    fn events_at() -> String {
        "/events".into()
    }
    fn seq_at() -> String {
        "/seq".into()
    }
    fn phase_at() -> String {
        "/phase".into()
    }
    fn round_at() -> String {
        "/round".into()
    }
    fn kind_at() -> String {
        "/kind".into()
    }
    fn text_at() -> String {
        "/detail".into()
    }
}

/// A step of the chain in the inbox, and the tool's phases that are in it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerStepDef {
    pub id: String,
    pub label: String,
    pub phases: Vec<String>,
}

/// Something the user may do to a run: `{run}` is its id, `{authorization}` a reference to the
/// user pressing it, and `ask` what is typed first.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerControlDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub about: Option<String>,
    pub run: Vec<String>,
    #[serde(default)]
    pub ask: Vec<String>,
    #[serde(default)]
    pub danger: bool,
    /// "running" or "paused": only offered then. Always while the run goes on when left out.
    #[serde(default)]
    pub when: Option<String>,
}

/// Taking a thread's work: Hub drafts a brief from the thread with `guide`, the user edits and
/// approves it, and `run` gets it as `{brief_file}`, with `{repo}` (the thread's folder),
/// `{title}` and `{work}` (`hub:thread:<id>`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Handoff {
    pub label: String,
    #[serde(default)]
    pub about: Option<String>,
    pub guide: String,
    pub run: Vec<String>,
    /// How the work stands, run with `{work}` until it says "done" or "stopped".
    #[serde(default)]
    pub follow: Option<Vec<String>>,
    /// Arguments added when the user ticks "Let it build".
    #[serde(default)]
    pub build: Option<Vec<String>>,
    /// The exit code that means "answer these first": the tool prints `{ said, ask: [{ name,
    /// label, hint }] }`, and the handoff is tried again with each answer as `--name value`.
    #[serde(default)]
    pub needs_exit: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub run: Vec<String>,
    #[serde(default)]
    pub headline: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct List {
    pub title: String,
    /// Its own command; the status command's document when left out.
    #[serde(default)]
    pub run: Option<Vec<String>>,
    /// Where the array is in the document, as a JSON pointer.
    pub items: String,
    /// Only items whose field (a pointer without the slash, or a plain key) equals the value, or
    /// one of the values when it is a list.
    #[serde(default, rename = "where")]
    pub filter: BTreeMap<String, toml::Value>,
    #[serde(default)]
    pub id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    /// Newest last in the document: show it the other way round.
    #[serde(default)]
    pub reverse: bool,
    /// Its items wait on the user.
    #[serde(default)]
    pub needs_you: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub run: Vec<String>,
    /// The list it acts on an item of; `{id}` is that item's id.
    #[serde(default)]
    pub on: Option<String>,
    /// What the user (or agent) types, each filling `{name}`.
    #[serde(default)]
    pub ask: Vec<String>,
    /// Stops or undoes something: the app asks twice, and agents need full access.
    #[serde(default)]
    pub danger: bool,
    #[serde(default)]
    pub about: Option<String>,
    /// "pause" or "resume": run by the panic button when it pauses or resumes every agent.
    #[serde(default)]
    pub panic: Option<String>,
}

pub fn parse(text: &str) -> Result<Manifest> {
    let manifest: Manifest = toml::from_str(text)?;
    if manifest.name.trim().is_empty() {
        bail!("name is empty");
    }
    let titles: HashSet<&str> = manifest.lists.iter().map(|l| l.title.as_str()).collect();
    for list in &manifest.lists {
        if list.run.is_none() && manifest.status.is_none() {
            bail!("list \"{}\" has no run and there is no [status] to read from", list.title);
        }
    }
    for action in &manifest.actions {
        if action.run.is_empty() {
            bail!("action \"{}\" has an empty run", action.id);
        }
        if let Some(on) = &action.on
            && !titles.contains(on.as_str())
        {
            bail!("action \"{}\" is on \"{on}\", which is not a list", action.id);
        }
        if let Some(panic) = &action.panic {
            if panic != "pause" && panic != "resume" {
                bail!("action \"{}\" has panic = \"{panic}\"; it is \"pause\" or \"resume\"", action.id);
            }
            if action.on.is_some() || !action.ask.is_empty() {
                bail!("action \"{}\" runs from the panic button, so it cannot ask for anything or act on an item", action.id);
            }
        }
    }
    if manifest.handoff.as_ref().is_some_and(|h| h.run.is_empty() || h.guide.trim().is_empty() || h.follow.as_ref().is_some_and(Vec::is_empty)) {
        bail!("[handoff] needs a run and a guide, and a follow that is not empty");
    }
    if manifest.status.as_ref().is_some_and(|s| s.run.is_empty()) {
        bail!("[status] has an empty run");
    }
    if let Some(worker) = &manifest.worker {
        if worker.runs.is_empty() || worker.events.is_empty() {
            bail!("[worker] needs runs and events");
        }
        for control in &worker.controls {
            if control.run.is_empty() {
                bail!("worker control \"{}\" has an empty run", control.id);
            }
            if control.when.as_deref().is_some_and(|w| w != "running" && w != "paused") {
                bail!("worker control \"{}\" has when = {:?}; it is \"running\" or \"paused\"", control.id, control.when);
            }
        }
    }
    Ok(manifest)
}

/// Folders that may hold a tool: the projects, and every folder right inside the home folder and
/// the projects folder, where tools like `~/Forman` live.
pub fn candidates(projects: &[String], roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut add = |dir: PathBuf| {
        if dir.join(FILE).is_file() && seen.insert(std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone())) {
            out.push(dir);
        }
    };
    for p in projects {
        add(PathBuf::from(p));
    }
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        let mut dirs: Vec<PathBuf> = entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect();
        dirs.sort();
        for dir in dirs {
            add(dir);
        }
    }
    out
}

static APPROVALS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const ALLOWED: &str = "own_tools.allowed";
const MIGRATED: &str = "own_tools.migrated";

fn same(a: &Path, b: &Path) -> bool {
    std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf()) == std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf())
}

/// The folders the user has approved once (`own_tools.allowed`). A `hub.toml` names commands Hub
/// and the agents run, so one in a folder that only happens to sit in the home folder (a cloned
/// repository) does not count until it is approved here.
async fn allowed(ctx: &Ctx<'_>) -> Vec<PathBuf> {
    let text = ctx.store.get_value(ALLOWED).await.ok().flatten().unwrap_or_default();
    serde_json::from_str::<Vec<String>>(&text).unwrap_or_default().into_iter().map(PathBuf::from).collect()
}

async fn scanned(ctx: &Ctx<'_>) -> (Vec<String>, Vec<PathBuf>) {
    let projects: Vec<String> = ctx.store.projects().await.unwrap_or_default().into_iter().map(|p| p.path).collect();
    let mut roots = vec![crate::folders::home()];
    if let Ok(root) = crate::projects_root::get(ctx.store).await {
        roots.push(PathBuf::from(root));
    }
    // The folder walk runs on every look of the followers (every 15 s and more); a tool folder that
    // appears is found within the minute.
    let key = (projects.clone(), roots.clone());
    if let Some((at, k, found)) = SCAN.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && *k == key
        && at.elapsed() < SCAN_FRESH
    {
        return (projects, found.clone());
    }
    let p = projects.clone();
    let found = tokio::task::spawn_blocking(move || candidates(&p, &roots)).await.unwrap_or_default();
    *SCAN.lock().unwrap_or_else(|e| e.into_inner()) = Some((std::time::Instant::now(), key, found.clone()));
    (projects, found)
}

type ScanKey = (Vec<String>, Vec<PathBuf>);
static SCAN: std::sync::Mutex<Option<(std::time::Instant, ScanKey, Vec<PathBuf>)>> = std::sync::Mutex::new(None);
const SCAN_FRESH: std::time::Duration = std::time::Duration::from_secs(60);

async fn save(ctx: &Ctx<'_>, list: &[PathBuf]) -> Result<()> {
    let list: Vec<String> = list.iter().map(|f| f.display().to_string()).collect();
    ctx.store.set_value(ALLOWED, &serde_json::to_string(&list)?).await
}

/// The first time this runs on a machine, the tools it finds now are approved, so the setup that
/// worked before keeps working.
async fn migrate(ctx: &Ctx<'_>, found: &[PathBuf]) {
    let _one = APPROVALS.lock().await;
    if ctx.store.get_value(MIGRATED).await.ok().flatten().is_some() {
        return;
    }
    if save(ctx, found).await.is_ok() {
        ctx.store.set_value(MIGRATED, "1").await.warn_on_err("save MIGRATED");
    }
}

/// The folders on this machine that hold a tool and may run: those of the projects the user
/// added, and those approved once.
pub async fn folders(ctx: &Ctx<'_>) -> Vec<PathBuf> {
    let (projects, found) = scanned(ctx).await;
    migrate(ctx, &found).await;
    let allowed = allowed(ctx).await;
    found.into_iter().filter(|f| projects.iter().any(|p| same(Path::new(p), f)) || allowed.iter().any(|a| same(a, f))).collect()
}

/// Whether one of the tools that may run (`folders`, where `offer` looks too) takes handoffs: its
/// `hub.toml` has a `[handoff]`. Reads the files only; runs no tool. The bench's `handoff_tool`
/// need (`bench/run.rs`).
pub async fn takes_handoffs(ctx: &Ctx<'_>) -> bool {
    folders(ctx).await.iter().any(|dir| std::fs::read_to_string(dir.join(FILE)).ok().and_then(|t| parse(&t).ok()).is_some_and(|m| m.handoff.is_some()))
}

/// A folder with a `hub.toml` that has not been approved: its name, for the user to decide on.
pub async fn unapproved(ctx: &Ctx<'_>) -> Vec<OwnToolFound> {
    let ok = folders(ctx).await;
    let (_, found) = scanned(ctx).await;
    found
        .into_iter()
        .filter(|f| !ok.iter().any(|o| same(o, f)))
        .map(|f| {
            let name = std::fs::read_to_string(f.join(FILE)).ok().and_then(|t| parse(&t).ok()).map(|m| m.name);
            OwnToolFound { folder: f.display().to_string(), name: name.unwrap_or_else(|| f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()) }
        })
        .collect()
}

/// Approves a folder that holds a `hub.toml` (the user's decision, or an install they asked for).
pub async fn allow(ctx: &Ctx<'_>, folder: &str) -> Result<()> {
    let dir = PathBuf::from(folder);
    if !dir.join(FILE).is_file() {
        bail!("no {FILE} in {folder}");
    }
    // Only a folder Hub itself found, so a caller cannot approve a path of its own choosing.
    if !scanned(ctx).await.1.iter().any(|f| same(f, &dir)) {
        bail!("{folder} is not a folder Hub looks in");
    }
    approve(ctx, dir).await
}

async fn approve(ctx: &Ctx<'_>, dir: PathBuf) -> Result<()> {
    let _one = APPROVALS.lock().await;
    let mut list = allowed(ctx).await;
    if !list.iter().any(|a| same(a, &dir)) {
        list.push(dir);
        save(ctx, &list).await?;
    }
    READINGS.locked().clear();
    Ok(())
}

/// Takes an approval back.
pub async fn forget(ctx: &Ctx<'_>, folder: &str) -> Result<()> {
    let dir = PathBuf::from(folder);
    let _one = APPROVALS.lock().await;
    let list: Vec<PathBuf> = allowed(ctx).await.into_iter().filter(|a| !same(a, &dir)).collect();
    save(ctx, &list).await?;
    READINGS.locked().clear();
    Ok(())
}

/// Your tools on this machine, as the Workers view and the agents see them.
pub async fn list(ctx: &Ctx<'_>, fresh: bool) -> Vec<OwnTool> {
    read_all(folders(ctx).await, fresh).await
}

/// Runs an action of a tool found on this machine, and only of one found here.
pub async fn run(ctx: &Ctx<'_>, ask: &RunOwnToolAction, danger_ok: bool) -> Result<OwnToolRan> {
    let wanted = std::fs::canonicalize(&ask.folder).unwrap_or_else(|_| PathBuf::from(&ask.folder));
    let Some(folder) = folders(ctx).await.into_iter().find(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()) == wanted) else {
        bail!("no tool with a {FILE} in {}", ask.folder)
    };
    act(&folder, ask, danger_ok).await
}

/// The manifest of a tool found on this machine, by its folder.
pub(crate) async fn found(ctx: &Ctx<'_>, folder: &str) -> Result<(PathBuf, Manifest)> {
    let wanted = std::fs::canonicalize(folder).unwrap_or_else(|_| PathBuf::from(folder));
    let Some(dir) = folders(ctx).await.into_iter().find(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()) == wanted) else {
        bail!("no tool with a {FILE} in {folder}")
    };
    let manifest = parse(&std::fs::read_to_string(dir.join(FILE))?)?;
    Ok((dir, manifest))
}

/// How often work handed to a tool is asked about, and for how long.
pub(crate) const FOLLOW_EVERY: Duration = Duration::from_secs(120);
const FOLLOW_FOR_MS: i64 = 7 * 86_400_000;

/// The handoffs to ask about: each thread's latest successful `handed_off` of the last week
/// that no `handoff_result` has settled yet, as (thread, handoff).
pub(crate) async fn open_handoffs(ctx: &Ctx<'_>) -> Result<Vec<(String, HandedOffData)>> {
    let ended: HashMap<String, i64> =
        ctx.store.latest_of_kind(EventKind::HandoffResult).await?.into_iter().map(|e| (e.thread_id, e.seq)).collect();
    let now = crate::model::now_ms();
    let mut open = Vec::new();
    for e in ctx.store.latest_of_kind(EventKind::HandedOff).await? {
        let Some(d) = e.data_as::<HandedOffData>() else { continue };
        if d.folder.is_none() || d.work.is_none() || !d.ok || now - e.ts > FOLLOW_FOR_MS || ended.get(&e.thread_id).is_some_and(|s| *s > e.seq) {
            continue;
        }
        open.push((e.thread_id, d));
    }
    Ok(open)
}

/// Asks the tool that took a handoff how it went: the `handoff_result` to note once its work
/// ended, or None while it goes on (or when the tool cannot be asked).
pub(crate) async fn follow(ctx: &Ctx<'_>, d: &HandedOffData) -> Result<Option<HandoffResultData>> {
    let (Some(folder), Some(work)) = (d.folder.as_deref(), d.work.as_deref()) else { return Ok(None) };
    let Ok((dir, manifest)) = found(ctx, folder).await else { return Ok(None) };
    let Some(follow) = manifest.handoff.as_ref().and_then(|h| h.follow.clone()) else { return Ok(None) };
    let inputs = HashMap::from([("work".to_owned(), work.to_owned())]);
    let asked = command(&dir, &manifest.env, &follow, &inputs)?.output();
    let Ok(Ok(ran)) = tokio::time::timeout(TIMEOUT, asked).await else { return Ok(None) };
    let Ok(doc) = serde_json::from_str::<Value>(String::from_utf8_lossy(&ran.stdout).trim()) else { return Ok(None) };
    Ok(settled(&manifest.name, &d.title, &doc))
}

/// The `handoff_result` for a follow answer that says the work ended, or None while it goes on.
fn settled(tool: &str, title: &str, doc: &Value) -> Option<HandoffResultData> {
    let state = doc["state"].as_str()?;
    if state != "done" && state != "stopped" {
        return None;
    }
    let text = |key: &str| doc[key].as_str().map(|s| s.chars().take(OUTPUT_CHARS).collect::<String>());
    Some(HandoffResultData {
        tool: tool.to_owned(),
        title: title.to_owned(),
        state: state.to_owned(),
        proven: Some(doc["proven"].as_bool()),
        model: Some(text("model")),
        cost: Some(doc["cost"].as_f64()),
        said: Some(text("said")),
        link: Some(text("link")),
    })
}

pub(crate) fn plain(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn toml_plain(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Runs a command and reads the one JSON document it prints. A tool may exit non-zero and still
/// answer (a health check that is not healthy), so the document counts, not the exit code.
pub(crate) async fn document(folder: &Path, env: &BTreeMap<String, String>, run: &[String]) -> Result<Value> {
    document_for(folder, env, run, TIMEOUT).await
}

/// [`document`], allowed `limit` instead of the usual time.
pub(crate) async fn document_for(folder: &Path, env: &BTreeMap<String, String>, run: &[String], limit: Duration) -> Result<Value> {
    let out = command(folder, env, run, &HashMap::new())?.output();
    let out = tokio::time::timeout(limit, out).await.with_context(|| format!("{} took over {}s", run.join(" "), limit.as_secs()))??;
    let stdout = String::from_utf8_lossy(&out.stdout);
    match serde_json::from_str::<Value>(stdout.trim()) {
        Ok(doc) => Ok(doc),
        Err(_) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let said = stderr.trim().lines().last().or(stdout.trim().lines().last()).unwrap_or("no output");
            bail!("{} did not print JSON ({}): {said}", run.join(" "), out.status)
        }
    }
}

/// The command line with `${VAR}`, `~` and `{input}` filled in, ready to run in `folder`.
pub(crate) fn command(folder: &Path, env: &BTreeMap<String, String>, run: &[String], inputs: &HashMap<String, String>) -> Result<tokio::process::Command> {
    let mut args = Vec::with_capacity(run.len());
    for arg in run {
        args.push(fill(&expand(arg, env), inputs)?);
    }
    let mut program = args.remove(0);
    // `python3` is what the Linux hosts call it; on Windows it is `python`.
    if cfg!(windows) && program == "python3" {
        program = "python".into();
    }
    let mut cmd = crate::proc::tokio_command(&program);
    cmd.args(&args).current_dir(folder).kill_on_drop(true).stdin(std::process::Stdio::null());
    for (key, value) in env {
        cmd.env(key, expand(value, env));
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    Ok(cmd)
}

/// `~` at the start, and `${VAR}` or `${VAR:-default}` from the file's env or the core's.
pub fn expand(arg: &str, env: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = arg;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find('}') else { break };
        let inner = &rest[start + 2..start + len];
        let (name, default) = inner.split_once(":-").map_or((inner, None), |(n, d)| (n, Some(d)));
        let value = env.get(name).cloned().or_else(|| std::env::var(name).ok()).filter(|v| !v.is_empty());
        out.push_str(&value.unwrap_or_else(|| default.unwrap_or_default().to_owned()));
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    if out == "~" || out.starts_with("~/") || out.starts_with("~\\") {
        return crate::folders::expand(&out).display().to_string();
    }
    out
}

/// Fills `{name}` with what was typed. Every name the command uses must be given.
fn fill(arg: &str, inputs: &HashMap<String, String>) -> Result<String> {
    let mut out = String::new();
    let mut rest = arg;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else { break };
        let name = &rest[start + 1..start + len];
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            out.push_str(&rest[..start + len + 1]);
        } else {
            out.push_str(&rest[..start]);
            match inputs.get(name) {
                Some(v) => out.push_str(v),
                None => bail!("{{{name}}} was not given"),
            }
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The tool's name and its action marked `panic = <which>`, when it has one.
pub fn panic_action(folder: &Path, which: &str) -> Option<(String, String)> {
    let manifest = parse(&std::fs::read_to_string(folder.join(FILE)).ok()?).ok()?;
    let action = manifest.actions.iter().find(|a| a.panic.as_deref() == Some(which))?;
    Some((manifest.name, action.id.clone()))
}

/// Runs one of a tool's actions. `danger_ok` is false for callers that may not stop things.
pub async fn act(folder: &Path, ask: &RunOwnToolAction, danger_ok: bool) -> Result<OwnToolRan> {
    let text = std::fs::read_to_string(folder.join(FILE)).with_context(|| format!("no {FILE} in {}", folder.display()))?;
    let manifest = parse(&text)?;
    let Some(action) = manifest.actions.iter().find(|a| a.id == ask.action) else {
        bail!("{} has no action {}", manifest.name, ask.action)
    };
    if action.danger && !danger_ok {
        bail!("{} on {} stops or undoes something; it needs full access", action.label, manifest.name);
    }
    let mut inputs: HashMap<String, String> = HashMap::new();
    for name in &action.ask {
        let value = ask.inputs.get(name).map(|v| v.trim()).unwrap_or_default();
        if value.is_empty() {
            bail!("{} needs {name}", action.label);
        }
        inputs.insert(name.clone(), value.to_owned());
    }
    if action.on.is_some() {
        match ask.item.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
            Some(id) => inputs.insert("id".into(), id.to_owned()),
            None => bail!("{} is done on one item; say which", action.label),
        };
    }
    let out = command(folder, &manifest.env, &action.run, &inputs)?.output();
    let out = tokio::time::timeout(TIMEOUT, out).await.with_context(|| format!("{} took over {}s", action.label, TIMEOUT.as_secs()))??;
    READINGS.lock().unwrap_or_else(|e| e.into_inner()).remove(folder);
    Ok(OwnToolRan { ok: out.status.success(), output: said(&out), asks: Vec::new() })
}

/// What a command printed, stdout then stderr, the end of it when long.
fn said(out: &std::process::Output) -> String {
    let mut output = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !stderr.trim().is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(stderr.trim());
    }
    if output.chars().count() > OUTPUT_CHARS {
        let skip = output.chars().count() - OUTPUT_CHARS;
        output = format!("…{}", output.chars().skip(skip).collect::<String>());
    }
    output
}

mod handoff;
pub(crate) use handoff::*;

mod reading;
pub use reading::*;

#[cfg(test)]
mod tests;
