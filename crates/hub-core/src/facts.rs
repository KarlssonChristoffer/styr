//! What Hub tells one of your own tools about a finished stretch of a development thread, so the
//! tool (Aftermath) can keep one record of work done on every machine without being installed on
//! each.
//!
//! A stretch is the work a thread did between two quiet moments: when a thread has had no event
//! for [`QUIET_MS`] and its log holds a finished turn since the last stretch, everything since the
//! last stretch is one stretch. It is written once as a small JSON file (the contract is
//! Aftermath's `docs/contracts/118_HUB_THREAD_FACTS_V1.md`; Hub imports nothing from it), kept in
//! an outbox, and delivered to every machine that has a tool taking `hub-thread-facts-v1`. A
//! machine that is off keeps the file waiting.
//!
//! A tool says it takes them in a `hub-facts.toml` beside its `hub.toml` (a separate file, so a
//! hubd from before this does not reject the tool's `hub.toml`). Hub knows nothing else about it.
//!
//! The file is a pure function of the log: its name and contents come from the stretch alone, so
//! writing it twice gives the same bytes and the tool takes it once. Nothing the user or an agent
//! wrote crosses: no prompt, reply, path, branch or repository, only counts, closed words and
//! plain names (anything that is not one is left out, because the tool refuses a file with a value
//! outside its vocabulary and stops on it).
//!
//! Hub's own proof and second opinion are the agent's side of the work, so they go as labelled
//! facts and never as a verdict.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::model::{Event, EventKind, Thread, now_ms};

/// The format a tool names in its `hub-facts.toml` to take these files.
pub const KIND: &str = "hub-thread-facts-v1";
/// The file beside `hub.toml`.
pub const FILE: &str = "hub-facts.toml";
/// How long a thread must be silent before its work so far counts as a stretch.
const QUIET_MS: i64 = 20 * 60_000;
/// A thread idle for longer than this when Hub first looks is not turned into a stretch.
const BACKFILL_MS: i64 = 7 * 24 * 3_600_000;
/// An undelivered file older than this is dropped.
const KEEP_MS: i64 = 14 * 24 * 3_600_000;
const EVERY: Duration = Duration::from_secs(120);
const WAIT: Duration = Duration::from_secs(30);
/// A file the receiver takes at most.
const MAX_BYTES: usize = 64 * 1024;

/// `hub-facts.toml`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    /// The format it takes, [`KIND`].
    pub kind: String,
    /// The folder on the tool's machine its files go into (as `pending/<name>` inside it). `${VAR}`
    /// and `~` work, relative to the tool's folder otherwise.
    pub dir: String,
    /// Run in the tool's folder after files arrived, so it reads them. Not run through a shell.
    #[serde(default)]
    pub ingest: Option<Vec<String>>,
    /// Environment for that command, as in `hub.toml`.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// What a machine answers to `GET /api/tools/facts`: the tools that take files, by format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Accepts {
    pub tool: String,
    pub kind: String,
}

/// One file, as sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactFile {
    /// `<sha256 hex>.json`.
    pub name: String,
    pub text: String,
}

/// `POST /api/tools/facts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delivery {
    pub kind: String,
    pub files: Vec<FactFile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Delivered {
    /// Tools that took the files.
    pub tools: Vec<String>,
    /// Files newly written (a file already there is not written again).
    pub written: u32,
}

// ---- the file --------------------------------------------------------------------------

fn plain(s: &str, extra: &str, max: usize) -> Option<String> {
    let ok = !s.is_empty() && s.len() <= max && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c));
    ok.then(|| s.to_owned())
}

/// A UTC `YYYY-MM-DDTHH:MM:SSZ` from unix milliseconds.
fn utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

fn text_fact(name: &str, value: &str, source: &str, provenance: &str) -> Value {
    json!({ "fact": format!("hub.stretch.{name}"), "value_text": value, "source_type": source, "provenance": provenance })
}

/// The file name a stretch's observation has in the tool's spool: the SHA-256 of
/// `hub`, a NUL and the reference, as hex, so the same stretch is always the same file.
pub fn file_name(reference: &str) -> String {
    let digest = Sha256::digest(format!("hub\0{reference}").as_bytes());
    format!("{}.json", digest.iter().map(|b| format!("{b:02x}")).collect::<String>())
}

/// The stretch `events` make up, as the tool's file; None when there is nothing to tell.
/// `events` are the thread's events after the previous stretch, oldest first, ending at the
/// moment the thread went quiet. `model` is the model the thread last set, when it set one.
pub fn envelope(thread: &Thread, machine: &str, model: Option<&str>, events: &[Event]) -> Option<Value> {
    let last = events.last()?;
    let finished = events.iter().rposition(|e| matches!(e.kind, EventKind::TurnEnd | EventKind::Error))?;
    let thread_id = plain(&thread.id, "_-", 64)?;
    // A machine's name may hold letters the tool's vocabulary does not (Plåtnicklas).
    let machine: String = machine.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' }).collect();
    let machine = plain(&machine, "._-", 64)?;
    let reference = format!("hub:stretch:{thread_id}:{}", last.seq);

    let cut_off = events[finished + 1..].iter().any(|e| e.kind == EventKind::TurnStart);
    let (end, lifecycle) = if cut_off {
        ("cut_off", "ABORTED")
    } else if events[finished].kind == EventKind::Error {
        ("error", "FAILED")
    } else if events[finished].cancelled() {
        ("stopped", "ABORTED")
    } else {
        ("settled", "COMPLETED")
    };
    let ended_ms = if cut_off { last.ts } else { events[finished].ts };
    let started_ms = events.first()?.ts.min(ended_ms);
    let turns = events.iter().filter(|e| e.kind == EventKind::TurnEnd).count();

    let own = ("source_observation", "SOURCE_FACT");
    let measured = ("harness_measured", "HARNESS_MEASURED");
    let mut facts = vec![
        text_fact("ref", &reference, own.0, own.1),
        text_fact("machine", &machine, own.0, own.1),
        text_fact("end", end, own.0, own.1),
        json!({ "fact": "hub.stretch.turns", "value_number": turns, "source_type": own.0, "provenance": own.1 }),
    ];
    // Hub's own proof: the last verdict Hub reached for the stretch. Absent when it never looked.
    if let Some(state) = events
        .iter()
        .rev()
        .filter_map(Event::data_as::<crate::model::ProofData>)
        .map(|p| p.state)
        .find(|s| ["shown", "failed", "asked", "missing"].contains(&s.as_str()))
    {
        facts.push(text_fact("proof", &state, measured.0, measured.1));
    }
    // A second opinion by another model: that it happened, never that the change passed it.
    let mut reviews: BTreeMap<String, String> = BTreeMap::new();
    for r in events.iter().filter_map(Event::data_as::<crate::model::ReviewData>) {
        if !r.id.is_empty() && !r.state.is_empty() {
            reviews.insert(r.id, r.state);
        }
    }
    if reviews.values().any(|s| s == "done") {
        facts.push(text_fact("review", "ai_reviewed", measured.0, measured.1));
    } else if reviews.values().any(|s| s == "failed") {
        facts.push(text_fact("review", "ai_review_failed", measured.0, measured.1));
    }

    let mut strategy = serde_json::Map::new();
    if let Some(runtime) = plain(&thread.engine, "._:/+-", 64) {
        strategy.insert("runtime".into(), json!(runtime));
    }
    if let Some(model) = model.and_then(|m| plain(m, "._:/+-", 64)) {
        strategy.insert("model".into(), json!(model));
    }
    Some(json!({
        "schema_version": "execution_observation_v0",
        "execution": {
            "execution_id": reference,
            "work_ref": crate::runtime::work_ref(&thread_id),
            "started_at": started_ms.div_euclid(1000),
            "ended_at": ended_ms.div_euclid(1000),
        },
        "observation": {
            "source_system": "hub",
            "external_observation_ref": reference,
            "execution_lifecycle": lifecycle,
            "capture_lifecycle": "COMPLETE",
            "observed_at": utc(ended_ms),
        },
        "strategy_identity": strategy,
        "facts": facts,
    }))
}

/// The model the thread last chose, from its `setting` events.
fn last_model(events: &[Event]) -> Option<String> {
    events
        .iter()
        .rev()
        .filter_map(Event::data_as::<crate::model::SettingData>)
        .find_map(|s| s.model().map(str::to_owned))
}

// ---- the stretches ----------------------------------------------------------------------

fn outbox(hub: &crate::Hub) -> PathBuf {
    hub.config.data_dir.join("facts").join("outbox")
}

fn closed_key(thread: &str) -> String {
    format!("facts.closed.{thread}")
}

/// Writes the files for threads that went quiet, then delivers what waits. Runs for as long as
/// the core does.
pub async fn sweeper(hub: Arc<crate::Hub>) {
    tokio::time::sleep(Duration::from_secs(90)).await;
    loop {
        if let Err(err) = sweep(&hub).await {
            tracing::warn!("facts: {err:#}");
        }
        tokio::time::sleep(EVERY).await;
    }
}

async fn sweep(hub: &Arc<crate::Hub>) -> Result<()> {
    let places = destinations(hub).await;
    // Nothing is written while no machine takes the files, so a Hub without the tool collects
    // nothing; a stretch that ended earlier is still there when one turns up.
    if places.is_empty() {
        return Ok(());
    }
    let now = now_ms();
    let root = hub.projects_root().await.unwrap_or_default();
    let machine = hub.remotes.local_name().to_owned();
    for thread in hub.store.threads().await? {
        if thread.host.is_some() || crate::connections::is_chat_folder(&root, &thread.cwd) {
            continue;
        }
        let idle = now - thread.updated_at;
        if idle < QUIET_MS || (idle > BACKFILL_MS && thread_not_seen(hub, &thread.id).await) {
            continue;
        }
        let closed: i64 = hub.store.get_value(&closed_key(&thread.id)).await?.and_then(|v| v.parse().ok()).unwrap_or(0);
        let events = hub.store.events(&thread.id, closed, 200_000).await?;
        if events.last().is_none_or(|e| e.seq <= closed) {
            continue;
        }
        let all_settings;
        let model = match last_model(&events) {
            Some(m) => Some(m),
            None if closed > 0 => {
                all_settings = hub.store.events(&thread.id, 0, 200_000).await?;
                last_model(&all_settings)
            }
            None => None,
        };
        if let Some(file) = envelope(&thread, &machine, model.as_deref(), &events) {
            let name = file_name(file["observation"]["external_observation_ref"].as_str().unwrap_or_default());
            let dir = outbox(hub);
            tokio::fs::create_dir_all(&dir).await?;
            write_once(&dir.join(&name), &serde_json::to_vec(&file)?).await?;
        }
        // Whether or not it made a file (nothing finished), these events are looked at.
        hub.store.set_value(&closed_key(&thread.id), &events.last().map(|e| e.seq).unwrap_or(closed).to_string()).await?;
    }
    deliver(hub, &places).await
}

/// A thread never turned into a stretch and idle for a week: not worth reading.
async fn thread_not_seen(hub: &crate::Hub, id: &str) -> bool {
    hub.store.get_value(&closed_key(id)).await.ok().flatten().is_none()
}

async fn write_once(path: &Path, bytes: &[u8]) -> Result<()> {
    if tokio::fs::try_exists(path).await.unwrap_or(false) {
        return Ok(());
    }
    let tmp = path.with_extension("tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

// ---- delivery ---------------------------------------------------------------------------

/// Where files go: this machine, when a tool here takes them, and each online machine that has
/// one. None means the machine itself.
async fn destinations(hub: &Arc<crate::Hub>) -> Vec<Option<String>> {
    let mut places = Vec::new();
    if !accepts_here(hub).await.is_empty() {
        places.push(None);
    }
    for name in hub.remotes.online() {
        // An older hubd has no such call; that is "takes nothing".
        if hub.remotes.get_from::<Vec<Accepts>>(&name, "/api/tools/facts").await.is_ok_and(|a| a.iter().any(|t| t.kind == KIND)) {
            places.push(Some(name));
        }
    }
    places
}

/// Sends what waits in the outbox to every place; a file goes only once every place has it.
async fn deliver(hub: &Arc<crate::Hub>, places: &[Option<String>]) -> Result<()> {
    let dir = outbox(hub);
    let mut files = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else { return Ok(()) };
    let now = std::time::SystemTime::now();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let old = entry.metadata().await.and_then(|m| m.modified()).ok().and_then(|m| now.duration_since(m).ok()).is_some_and(|age| age.as_millis() as i64 > KEEP_MS);
        if old {
            crate::locked::clean_file_async(&path).await;
            continue;
        }
        let (Some(name), Ok(text)) = (path.file_name().map(|n| n.to_string_lossy().into_owned()), tokio::fs::read_to_string(&path).await) else { continue };
        files.push(FactFile { name, text });
        if files.len() >= 200 {
            break;
        }
    }
    if files.is_empty() {
        return Ok(());
    }
    let batch = Delivery { kind: KIND.into(), files };
    let mut everywhere = true;
    for place in places {
        let sent = match place {
            None => receive(hub, &batch).await,
            Some(name) => hub.remotes.post_to::<Delivered>(name, "/api/tools/facts", &batch, WAIT).await,
        };
        if let Err(err) = sent {
            tracing::warn!("facts: {}: {err:#}", place.as_deref().unwrap_or("here"));
            everywhere = false;
        }
    }
    if everywhere {
        for file in &batch.files {
            crate::locked::clean_file_async(dir.join(&file.name)).await;
        }
    }
    Ok(())
}

// ---- the receiving end ------------------------------------------------------------------

/// A tool of this machine that takes [`KIND`], with its folder and what it says.
async fn tools_here(hub: &crate::Hub) -> Vec<(PathBuf, Facts, String)> {
    let mut found = Vec::new();
    for folder in crate::own_tools::folders(&hub.ctx()).await {
        let Ok(text) = std::fs::read_to_string(folder.join(FILE)) else { continue };
        let Ok(facts) = toml::from_str::<Facts>(&text) else { continue };
        if facts.kind != KIND {
            continue;
        }
        let name = std::fs::read_to_string(folder.join(crate::own_tools::FILE))
            .ok()
            .and_then(|t| crate::own_tools::parse(&t).ok())
            .map(|m| m.name)
            .unwrap_or_else(|| folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        found.push((folder, facts, name));
    }
    found
}

/// The tools of this machine that take files, for `GET /api/tools/facts`.
pub async fn accepts_here(hub: &crate::Hub) -> Vec<Accepts> {
    tools_here(hub).await.into_iter().map(|(_, f, tool)| Accepts { tool, kind: f.kind }).collect()
}

fn safe_name(name: &str) -> bool {
    name.len() == 69 && name.ends_with(".json") && name[..64].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Puts the files in each tool's folder and lets it read them (`POST /api/tools/facts`). A file
/// already there is left alone. Only well-formed JSON of a sensible size goes in.
pub async fn receive(hub: &crate::Hub, delivery: &Delivery) -> Result<Delivered> {
    if delivery.kind != KIND {
        bail!("this machine takes {KIND}, not {}", delivery.kind);
    }
    for file in &delivery.files {
        if !safe_name(&file.name) {
            bail!("{} is not a file name for a fact", file.name);
        }
        if file.text.len() > MAX_BYTES || serde_json::from_str::<Value>(&file.text).is_err() {
            bail!("{} is too large or not JSON", file.name);
        }
    }
    let mut out = Delivered::default();
    for (folder, facts, tool) in tools_here(hub).await {
        let dir = {
            let expanded = crate::own_tools::expand(&facts.dir, &facts.env);
            let path = PathBuf::from(&expanded);
            if path.is_absolute() { path } else { folder.join(path) }
        };
        let pending = dir.join("pending");
        tokio::fs::create_dir_all(&pending).await.with_context(|| format!("{}", pending.display()))?;
        let mut wrote = 0;
        for file in &delivery.files {
            let path = pending.join(&file.name);
            // Taken before, whether it is still waiting or already read.
            if tokio::fs::try_exists(&path).await.unwrap_or(false) || tokio::fs::try_exists(dir.join("done").join(&file.name)).await.unwrap_or(false) {
                continue;
            }
            write_once(&path, file.text.as_bytes()).await?;
            wrote += 1;
        }
        if wrote > 0
            && let Some(run) = facts.ingest.as_ref().filter(|r| !r.is_empty())
        {
            let cmd = crate::own_tools::command(&folder, &facts.env, run, &Default::default())?.output();
            match tokio::time::timeout(Duration::from_secs(120), cmd).await {
                Ok(Ok(o)) if o.status.success() => {}
                Ok(Ok(o)) => tracing::warn!("facts: {tool} could not read them: {}", String::from_utf8_lossy(&o.stderr).trim()),
                Ok(Err(err)) => tracing::warn!("facts: {tool}: {err}"),
                Err(_) => tracing::warn!("facts: {tool} took too long to read them"),
            }
        }
        out.written += wrote;
        out.tools.push(tool);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ThreadStatus;

    fn thread() -> Thread {
        Thread {
            id: "cmsg_01VUp".into(),
            title: "secret title".into(),
            cwd: "/home/x/secret/project".into(),
            engine: "claude".into(),
            status: ThreadStatus::Idle,
            engine_session_id: None,
            title_locked: false,
            archived: false,
            host: None, project: None, display: None,
            created_at: 0,
            updated_at: 0,
            returned_at: None,
            seen_at: None,
            settled_at: None,
            sent_at: None,
            pinned_at: None,
            snoozed_until: None,
            auto_settle: true,
            parent: None,
            reading: None,
            owner: None,
            cloud: None,
        }
    }

    fn ev(seq: i64, ts: i64, kind: EventKind, data: Value) -> Event {
        Event { thread_id: "cmsg_01VUp".into(), seq, ts, kind, data }
    }

    fn turn(first: i64) -> Vec<Event> {
        vec![
            ev(first, 1_793_000_000_000, EventKind::UserMessage, json!({ "text": "please delete everything" })),
            ev(first + 1, 1_793_000_001_000, EventKind::TurnStart, json!({ "prompt": first })),
            ev(first + 2, 1_793_000_900_000, EventKind::TurnEnd, json!({ "stop_reason": "end_turn", "prompt": first })),
        ]
    }

    #[test]
    fn a_finished_turn_is_one_stretch_with_a_stable_name() {
        let events = turn(1);
        let a = envelope(&thread(), "PC", Some("sonnet-5"), &events).unwrap();
        let b = envelope(&thread(), "PC", Some("sonnet-5"), &events).unwrap();
        assert_eq!(a, b, "the same log is the same file");
        assert_eq!(a["execution"]["execution_id"], "hub:stretch:cmsg_01VUp:3");
        assert_eq!(a["execution"]["work_ref"], "work://hub/cmsg_01VUp");
        assert_eq!(a["execution"]["started_at"], 1_793_000_000);
        assert_eq!(a["execution"]["ended_at"], 1_793_000_900);
        assert_eq!(a["observation"]["execution_lifecycle"], "COMPLETED");
        assert_eq!(a["observation"]["observed_at"], utc(1_793_000_900_000));
        assert_eq!(a["strategy_identity"], json!({ "runtime": "claude", "model": "sonnet-5" }));
        let name = file_name("hub:stretch:cmsg_01VUp:3");
        assert!(safe_name(&name), "{name}");
        assert_eq!(name, file_name("hub:stretch:cmsg_01VUp:3"));
    }

    #[test]
    fn utc_is_the_calendar_date() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(1_793_000_900_000), "2026-10-26T07:48:20Z");
        assert_eq!(utc(1_709_164_800_000), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn nothing_the_person_or_agent_wrote_leaves() {
        let mut events = turn(1);
        events.push(ev(4, 1_793_000_901_000, EventKind::Update, json!({ "sessionUpdate": "agent_message_chunk", "content": "my password is hunter2" })));
        let text = envelope(&thread(), "PC", None, &events).unwrap().to_string();
        for leak in ["please delete", "hunter2", "secret", "/home/x"] {
            assert!(!text.contains(leak), "{leak} crossed: {text}");
        }
    }

    #[test]
    fn how_it_ended_follows_the_last_thing_that_happened() {
        let end = |events: Vec<Event>| envelope(&thread(), "PC", None, &events).unwrap();
        let mut failed = turn(1);
        failed[2] = ev(3, 1_793_000_900_000, EventKind::Error, json!({ "message": "boom" }));
        assert_eq!(end(failed)["observation"]["execution_lifecycle"], "FAILED");
        let mut stopped = turn(1);
        stopped[2].data = json!({ "stop_reason": "cancelled" });
        assert_eq!(end(stopped)["observation"]["execution_lifecycle"], "ABORTED");
        let mut cut = turn(1);
        cut.push(ev(4, 1_793_000_950_000, EventKind::TurnStart, json!({ "prompt": 4 })));
        let cut = end(cut);
        assert_eq!(cut["observation"]["execution_lifecycle"], "ABORTED");
        assert_eq!(cut["execution"]["ended_at"], 1_793_000_950, "a cut-off turn ends where the log stops");
    }

    #[test]
    fn a_stretch_without_a_finished_turn_is_nothing() {
        let events = vec![ev(1, 1, EventKind::UserMessage, json!({ "text": "hi" })), ev(2, 2, EventKind::TurnStart, json!({}))];
        assert!(envelope(&thread(), "PC", None, &events).is_none());
        assert!(envelope(&thread(), "PC", None, &[]).is_none());
    }

    #[test]
    fn proof_and_second_opinion_are_labelled_facts_and_never_a_verdict() {
        let mut events = turn(1);
        events.push(ev(4, 1_793_000_901_000, EventKind::Proof, json!({ "reply": 3, "state": "shown", "kind": "tests", "files": 2 })));
        events.push(ev(5, 1_793_000_902_000, EventKind::Review, json!({ "id": "r1", "state": "working" })));
        events.push(ev(6, 1_793_000_903_000, EventKind::Review, json!({ "id": "r1", "state": "done", "text": "looks wrong in line 4" })));
        let file = envelope(&thread(), "PC", None, &events).unwrap();
        let facts = file["facts"].as_array().unwrap();
        let get = |n: &str| facts.iter().find(|f| f["fact"] == format!("hub.stretch.{n}")).cloned();
        assert_eq!(get("proof").unwrap()["value_text"], "shown");
        assert_eq!(get("proof").unwrap()["source_type"], "harness_measured");
        assert_eq!(get("review").unwrap()["value_text"], "ai_reviewed");
        assert!(file.get("verifications").is_none() && file.get("artifacts").is_none());
        assert!(!file.to_string().contains("looks wrong"));
    }

    #[test]
    fn a_name_that_is_not_plain_is_left_out_not_sent() {
        // A value outside the tool's vocabulary would stop its whole queue.
        let file = envelope(&thread(), "PC", Some("opus[1m]"), &turn(1)).unwrap();
        assert_eq!(file["strategy_identity"], json!({ "runtime": "claude" }));
        let mut odd = thread();
        odd.engine = "my engine/with spaces".into();
        assert_eq!(envelope(&odd, "PC", None, &turn(1)).unwrap()["strategy_identity"], json!({}));
        odd.id = "has a space".into();
        assert!(envelope(&odd, "PC", None, &turn(1)).is_none());
        let file = envelope(&thread(), "Plåt nicklas", None, &turn(1)).unwrap();
        assert!(file.to_string().contains("\"Pl-t-nicklas\""), "{file}");
        assert!(envelope(&thread(), "", None, &turn(1)).is_none());
    }

    #[test]
    fn only_a_name_made_of_hex_is_a_file_in_a_spool() {
        assert!(!safe_name("../../x.json"));
        assert!(!safe_name(&format!("{}.json", "A".repeat(64))));
        assert!(safe_name(&format!("{}.json", "a".repeat(64))));
    }

    #[test]
    fn the_model_is_the_last_one_chosen() {
        let events = vec![
            ev(1, 1, EventKind::Setting, json!({ "config_id": "model", "value": "opus" })),
            ev(2, 2, EventKind::Setting, json!({ "config_id": "effort", "value": "high" })),
            ev(3, 3, EventKind::Setting, json!({ "config_id": "model", "value": "sonnet-5" })),
        ];
        assert_eq!(last_model(&events).as_deref(), Some("sonnet-5"));
    }

    #[test]
    fn hub_facts_toml_reads_and_refuses_what_it_does_not_know() {
        let ok: Facts = toml::from_str("kind = \"hub-thread-facts-v1\"\ndir = \"~/.aftermath/hub-spool\"\ningest = [\"python3\", \"x.py\"]\n").unwrap();
        assert_eq!(ok.kind, KIND);
        assert!(toml::from_str::<Facts>("kind = \"x\"\ndir = \"d\"\nnope = 1\n").is_err());
    }
}
