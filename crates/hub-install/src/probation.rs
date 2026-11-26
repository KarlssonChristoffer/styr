//! A new version is on probation until it has shown it is healthy. The files that decide this live
//! beside `current`, and the decision itself ([`verdict`]) is made here, in one place, so that the
//! launcher, the app's shell and `hubd` can all reach the same answer without sharing any logic
//! beyond a call to this module. No dependencies: the file is flat JSON read and written by hand.
//!
//! ```text
//! <data dir>/
//!   last-good         the newest version that got through its probation
//!   probation.json    only while a version is on probation
//!   rolled-back.json  the last time a version was rolled back (shown until dismissed)
//! ```
//!
//! This module only keeps the files and answers; moving `current` back, the database and stopping
//! the broken core are the callers' job (`hub_core::update`).

use std::path::{Path, PathBuf};

use crate::{bin_dir, current_version, exe, set_current};

/// The programs a version folder needs to be worth going back to.
pub const PROGRAMS: [&str; 3] = ["hubd", "hub-app", "hub-launcher"];

/// How long a version is watched, and what counts against it. Defaults match `update.probation_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Unexpected restarts of the core that fail the probation.
    pub crashes: u32,
    /// Seconds a window may take, after loading the new pages, to say it is alive.
    pub alive_secs: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self { crashes: 2, alive_secs: 90 }
    }
}

/// Why a version on probation is rolled back. `Display` is the text shown to the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    Crashed(u32),
    NoWindow,
    Missing,
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reason::Crashed(n) => write!(f, "the core stopped unexpectedly {n} times"),
            Reason::NoWindow => write!(f, "the window showed no sign of life after loading the new pages"),
            Reason::Missing => write!(f, "the version's programs are missing"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Through the probation: [`confirm_healthy`] may record it as the last good version.
    Healthy,
    /// Not yet decided.
    Wait,
    RollBack(Reason),
}

/// The state of a version on probation (`probation.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probation {
    pub version: String,
    /// What `current` pointed at before this version took over.
    pub previous: Option<String>,
    /// The file name in `backups/` of the database copy taken before this version first started.
    pub db_backup: Option<String>,
    pub started_ms: u64,
    /// Starts of the core of this version, the takeover included.
    pub starts: u32,
    /// Stops the person asked for (settings restart, install, closing Hub); they are not crashes.
    pub clean_exits: u32,
    /// The core has been up for the whole probation time with its health check answering.
    pub core_ok: bool,
    /// A window needs to confirm the new pages (set by the app's shell when it starts).
    pub window_needed: bool,
    /// When a window loaded this version's pages; the clock for [`Limits::alive_secs`].
    pub window_loaded_ms: Option<u64>,
    /// A window loaded after the probation began has rendered the new pages.
    pub window_ok: bool,
}

impl Probation {
    pub fn new(version: &str, previous: Option<&str>, db_backup: Option<&str>, now_ms: u64) -> Self {
        Self {
            version: version.to_owned(),
            previous: previous.filter(|p| *p != version).map(str::to_owned),
            db_backup: db_backup.map(str::to_owned),
            started_ms: now_ms,
            starts: 1,
            clean_exits: 0,
            core_ok: false,
            window_needed: false,
            window_loaded_ms: None,
            window_ok: false,
        }
    }

    /// Restarts of the core nobody asked for.
    pub fn crashes(&self) -> u32 {
        self.starts.saturating_sub(1).saturating_sub(self.clean_exits)
    }
}

/// The last rollback (`rolled-back.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RolledBack {
    pub from: String,
    pub to: String,
    pub reason: String,
    pub at_ms: u64,
    /// File name in `backups/` of the database as it was when the version was rolled back, set
    /// aside because the version before could not open it.
    pub db_set_aside: Option<String>,
}

pub fn last_good_file(data: &Path) -> PathBuf {
    data.join("last-good")
}

pub fn probation_file(data: &Path) -> PathBuf {
    data.join("probation.json")
}

pub fn rolled_back_file(data: &Path) -> PathBuf {
    data.join("rolled-back.json")
}

/// The newest version that got through its probation.
pub fn last_good(data: &Path) -> Option<String> {
    let v = std::fs::read_to_string(last_good_file(data)).ok()?;
    let v = v.trim();
    (!v.is_empty()).then(|| v.to_owned())
}

/// The file in `bin/<version>/` naming the programs the update installed there, one per line. A
/// machine that only runs the core gets `hubd` alone, and must not be judged by the other two.
pub const PROGRAMS_FILE: &str = "PROGRAMS";

/// The programs `dir` has to hold: what its [`PROGRAMS_FILE`] lists (of the known ones, and always
/// `hubd`), or all of [`PROGRAMS`] for a folder without one, as every version used to be. A folder
/// without a list that holds `hubd` alone is a core pushed by a Hub from before the list existed:
/// it is core-only, not a full version that lost two programs (folders are moved in whole).
pub fn required(dir: &Path) -> Vec<&'static str> {
    let text = match std::fs::read_to_string(dir.join(PROGRAMS_FILE)) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let has = |p: &str| dir.join(exe(p)).is_file();
            let core_only = has("hubd") && !has("hub-app") && !has("hub-launcher");
            return if core_only { vec!["hubd"] } else { PROGRAMS.to_vec() };
        }
        // Unreadable for now (a scanner's lock): ask only for the core, which every version has.
        Err(_) => return vec!["hubd"],
    };
    PROGRAMS.into_iter().filter(|p| *p == "hubd" || text.lines().any(|l| l.trim() == *p)).collect()
}

/// Finishes a version folder just assembled in `dest`: its `VERSION`, and the [`PROGRAMS_FILE`]
/// naming the programs that were copied into it.
pub fn stamp_version(dest: &Path, version: &str) -> std::io::Result<()> {
    let here: Vec<&str> = PROGRAMS.into_iter().filter(|p| dest.join(exe(p)).is_file()).collect();
    std::fs::write(dest.join(PROGRAMS_FILE), here.join("\n"))?;
    std::fs::write(dest.join("VERSION"), version)
}

/// Whether `bin/<version>` has the programs it was installed with.
pub fn complete(data: &Path, version: &str) -> bool {
    let dir = bin_dir(data).join(version);
    required(&dir).iter().all(|p| dir.join(exe(p)).is_file())
}

pub fn read(data: &Path) -> Option<Probation> {
    let fields = parse(&std::fs::read_to_string(probation_file(data)).ok()?)?;
    Some(Probation {
        version: fields.text("version")?,
        previous: fields.text("previous"),
        db_backup: fields.text("db_backup"),
        started_ms: fields.number("started_ms")?,
        starts: fields.number("starts").unwrap_or(1) as u32,
        clean_exits: fields.number("clean_exits").unwrap_or(0) as u32,
        core_ok: fields.flag("core_ok"),
        window_needed: fields.flag("window_needed"),
        window_loaded_ms: fields.number("window_loaded_ms"),
        window_ok: fields.flag("window_ok"),
    })
}

fn write(data: &Path, p: &Probation) -> std::io::Result<()> {
    let text = object(&[
        ("version", Some(Value::Text(&p.version))),
        ("previous", p.previous.as_deref().map(Value::Text)),
        ("db_backup", p.db_backup.as_deref().map(Value::Text)),
        ("started_ms", Some(Value::Number(p.started_ms))),
        ("starts", Some(Value::Number(p.starts.into()))),
        ("clean_exits", Some(Value::Number(p.clean_exits.into()))),
        ("core_ok", Some(Value::Flag(p.core_ok))),
        ("window_needed", Some(Value::Flag(p.window_needed))),
        ("window_loaded_ms", p.window_loaded_ms.map(Value::Number)),
        ("window_ok", Some(Value::Flag(p.window_ok))),
    ]);
    atomic(&probation_file(data), &text)
}

fn atomic(path: &Path, text: &str) -> std::io::Result<()> {
    // A name of its own, so two writers never rename each other's file away.
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}

/// Puts `version` on probation, as it takes over. `previous` is what `current` pointed at until
/// now. A dev build never is.
pub fn begin(data: &Path, version: &str, previous: Option<&str>, db_backup: Option<&str>, now_ms: u64) -> std::io::Result<()> {
    if version == "dev" {
        return Ok(());
    }
    // A version that is started again while on probation keeps what it has counted so far.
    if let Some(mut p) = read(data).filter(|p| p.version == version) {
        p.starts += 1;
        return write(data, &p);
    }
    // A version still on probation was never proven: go back to the last good one instead.
    let unproven = read(data).filter(|p| p.version != version);
    let previous = match &unproven {
        Some(p) => last_good(data).or_else(|| p.previous.clone()),
        None => previous.map(str::to_owned),
    };
    write(data, &Probation::new(version, previous.as_deref(), db_backup, now_ms))
}

/// Changes the probation of `version` and nothing else's. Returns whether there was one.
fn update(data: &Path, version: &str, change: impl FnOnce(&mut Probation)) -> bool {
    let Some(mut p) = read(data).filter(|p| p.version == version) else { return false };
    change(&mut p);
    write(data, &p).is_ok()
}

/// The core of `version` started without being asked to by the takeover (a restart after a crash).
pub fn note_start(data: &Path, version: &str) -> bool {
    update(data, version, |p| p.starts += 1)
}

/// The core stopped because it was asked to, so the start that follows is not a crash.
pub fn note_clean_exit(data: &Path, version: &str) -> bool {
    update(data, version, |p| p.clean_exits += 1)
}

/// The core has been healthy for the whole probation time.
pub fn note_core_ok(data: &Path, version: &str) -> bool {
    update(data, version, |p| p.core_ok = true)
}

/// A window (the app's shell) started and will tell when the new pages have rendered.
pub fn note_window_needed(data: &Path, version: &str, now_ms: u64) -> bool {
    update(data, version, |p| {
        p.window_needed = true;
        // The clock for the window to say it is alive starts when the shell does; a shell
        // started again (or a machine that slept) gets a fresh one.
        p.window_loaded_ms = Some(now_ms);
    })
}

/// The window clock starts over (the computer slept, so the silence is no sign of a blank window).
pub fn restart_window_clock(data: &Path, version: &str, now_ms: u64) -> bool {
    update(data, version, |p| {
        if p.window_loaded_ms.is_some() && !p.window_ok {
            p.window_loaded_ms = Some(now_ms);
        }
    })
}

/// A window loaded the pages of `version`.
pub fn note_window_loaded(data: &Path, version: &str, now_ms: u64) -> bool {
    update(data, version, |p| {
        p.window_needed = true;
        if p.window_loaded_ms.is_none() {
            p.window_loaded_ms = Some(now_ms);
        }
    })
}

/// The pages of `version` have rendered (`POST /api/update/alive`).
pub fn note_alive(data: &Path, version: &str) -> bool {
    update(data, version, |p| p.window_ok = true)
}

/// What the probation of the version on it says now, from the files alone.
pub fn verdict(data: &Path, now_ms: u64, limits: Limits) -> Verdict {
    let Some(p) = read(data) else { return Verdict::Healthy };
    verdict_of(data, &p, now_ms, limits)
}

pub fn verdict_of(data: &Path, p: &Probation, now_ms: u64, limits: Limits) -> Verdict {
    if p.version == "dev" {
        return Verdict::Healthy;
    }
    if p.core_ok && (!p.window_needed || p.window_ok) {
        return Verdict::Healthy;
    }
    // Only a probation `current` still points at is judged: a version already left behind by
    // something else must not move `current` again.
    if current_version(data).is_some_and(|c| c != p.version) {
        return Verdict::Healthy;
    }
    if !complete(data, &p.version) {
        return Verdict::RollBack(Reason::Missing);
    }
    // A start loop before anything got done is the same thing as crashing twice: every start that
    // nobody asked for counts, and one asked for (`clean_exits`) never does.
    let crashes = p.crashes();
    if crashes >= limits.crashes {
        return Verdict::RollBack(Reason::Crashed(crashes));
    }
    if p.window_needed && !p.window_ok
        && let Some(loaded) = p.window_loaded_ms
            && now_ms.saturating_sub(loaded) > limits.alive_secs * 1000 {
                return Verdict::RollBack(Reason::NoWindow);
            }
    Verdict::Wait
}

mod roll_back;
pub use roll_back::{roll_back_candidates, roll_back_target};

/// What [`act`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Acted {
    /// The version is healthy, still waiting, or not on probation.
    Nothing,
    /// `current` was moved back (see [`roll_back`]).
    RolledBack(RolledBack),
    /// The probation failed but there is nothing to go back to; `current` was left alone.
    NoTarget(Reason),
}

/// Milliseconds since the Unix epoch, the clock of every `*_ms` field here (a copy of the core's on purpose: the launcher does not link it).
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// What the launcher and the app's shell do before they start anything: ask [`verdict`] and, when
/// it says to roll back, do it. They hold no logic of their own beyond this call.
pub fn act(data: &Path) -> Acted {
    act_at(data, now_ms(), Limits::default())
}

pub fn act_at(data: &Path, now_ms: u64, limits: Limits) -> Acted {
    let Some(p) = read(data) else { return Acted::Nothing };
    match verdict_of(data, &p, now_ms, limits) {
        Verdict::RollBack(reason) => match roll_back(data, &p.version, &reason.to_string(), now_ms) {
            Some(r) => Acted::RolledBack(r),
            None => Acted::NoTarget(reason),
        },
        _ => Acted::Nothing,
    }
}

/// Moves `current` from the failing version back to [`roll_back_target`], remembers the failing
/// commit so the updater does not build it again, writes `rolled-back.json` and ends the
/// probation. None when there is nowhere to go back to, or `current` could not be written.
///
/// Only the files move. The database is the core's to put right when the older `hubd` starts:
/// `db_set_aside` stays empty here.
pub fn roll_back(data: &Path, failing: &str, reason: &str, now_ms: u64) -> Option<RolledBack> {
    if failing == "dev" {
        return None;
    }
    let to = roll_back_target(data, failing)?;
    set_current(data, &to).ok()?;
    // `current` has moved, so the roll-back happened; what follows is bookkeeping that is
    // reported when it fails (the launcher and the shell log to stderr; this crate has no logger).
    // The updater reads this file (`update::mark_failed`) to skip a commit that failed: without it,
    // the updater may build the failing version again.
    if let Err(err) = std::fs::write(data.join("update-failed"), failing) {
        eprintln!("rolled back from {failing}, but could not note it as failed for the updater: {err}");
    }
    let done = RolledBack { from: failing.to_owned(), to, reason: reason.to_owned(), at_ms: now_ms, db_set_aside: None };
    if let Err(err) = write_rolled_back(data, &done) {
        eprintln!("rolled back from {failing}, but could not write the notice that says so: {err}");
    }
    if let Err(err) = end_of(data, failing) {
        eprintln!("rolled back from {failing}, but its probation could not be ended: {err}");
    }
    Some(done)
}

/// The version has been healthy: it is the one to go back to from now on, and the probation ends.
pub fn confirm_healthy(data: &Path, version: &str) -> std::io::Result<()> {
    if version == "dev" {
        return Ok(());
    }
    // Only the version that is the one in use is good; one left behind was never proven.
    if current_version(data).is_some_and(|c| c != version) {
        return Ok(());
    }
    atomic(&last_good_file(data), version)?;
    // A probation file left behind would keep judging a version that is already good: the caller
    // tries again on its next round.
    if read(data).is_some_and(|p| p.version == version) {
        remove(&probation_file(data))?;
    }
    Ok(())
}

/// Ends a probation without calling the version good (it was rolled back). Already ended is fine.
pub fn end(data: &Path) -> std::io::Result<()> {
    remove(&probation_file(data))
}

/// Ends the probation of `version` only: one a version that took over since has begun stays.
pub fn end_of(data: &Path, version: &str) -> std::io::Result<()> {
    if read(data).is_some_and(|p| p.version == version) {
        end(data)?;
    }
    Ok(())
}

/// Removes a file of this module; one that is already gone is not a failure.
fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err),
        _ => Ok(()),
    }
}

pub fn write_rolled_back(data: &Path, r: &RolledBack) -> std::io::Result<()> {
    let text = object(&[
        ("from", Some(Value::Text(&r.from))),
        ("to", Some(Value::Text(&r.to))),
        ("reason", Some(Value::Text(&r.reason))),
        ("at_ms", Some(Value::Number(r.at_ms))),
        ("db_set_aside", r.db_set_aside.as_deref().map(Value::Text)),
    ]);
    atomic(&rolled_back_file(data), &text)
}

pub fn read_rolled_back(data: &Path) -> Option<RolledBack> {
    let fields = parse(&std::fs::read_to_string(rolled_back_file(data)).ok()?)?;
    Some(RolledBack {
        from: fields.text("from")?,
        to: fields.text("to")?,
        reason: fields.text("reason").unwrap_or_default(),
        at_ms: fields.number("at_ms").unwrap_or(0),
        db_set_aside: fields.text("db_set_aside"),
    })
}

/// The person has seen it. A notice that could not be removed shows again.
pub fn dismiss_rolled_back(data: &Path) -> std::io::Result<()> {
    remove(&rolled_back_file(data))
}

/// The versions a prune must not remove: the running and `current` ones are the caller's, these
/// are the ones going back needs.
pub fn keep(data: &Path) -> Vec<String> {
    let mut keep = Vec::new();
    keep.extend(last_good(data));
    if let Some(p) = read(data) {
        keep.extend(p.previous);
        keep.push(p.version);
    }
    keep
}

// --- the flat JSON of the files above ---

enum Value<'a> {
    Text(&'a str),
    Number(u64),
    Flag(bool),
}

fn object(fields: &[(&str, Option<Value>)]) -> String {
    let mut out = String::from("{\n");
    let present: Vec<_> = fields.iter().filter_map(|(k, v)| v.as_ref().map(|v| (k, v))).collect();
    for (i, (key, value)) in present.iter().enumerate() {
        out.push_str(&format!("  \"{key}\": "));
        match value {
            Value::Text(s) => quote(s, &mut out),
            Value::Number(n) => out.push_str(&n.to_string()),
            Value::Flag(b) => out.push_str(if *b { "true" } else { "false" }),
        }
        out.push_str(if i + 1 < present.len() { ",\n" } else { "\n" });
    }
    out.push_str("}\n");
    out
}

fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Fields(Vec<(String, Raw)>);

enum Raw {
    Text(String),
    Number(u64),
    Flag(bool),
    Null,
}

impl Fields {
    fn get(&self, key: &str) -> Option<&Raw> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    fn text(&self, key: &str) -> Option<String> {
        match self.get(key)? {
            Raw::Text(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        }
    }
    fn number(&self, key: &str) -> Option<u64> {
        match self.get(key)? {
            Raw::Number(n) => Some(*n),
            _ => None,
        }
    }
    fn flag(&self, key: &str) -> bool {
        matches!(self.get(key), Some(Raw::Flag(true)))
    }
}

/// Reads one flat JSON object. Unknown keys are kept and ignored, so a later version may add some.
fn parse(text: &str) -> Option<Fields> {
    let mut chars = text.chars().peekable();
    let skip = |c: &mut std::iter::Peekable<std::str::Chars>| {
        while c.peek().is_some_and(|c| c.is_whitespace()) {
            c.next();
        }
    };
    skip(&mut chars);
    if chars.next()? != '{' {
        return None;
    }
    let mut fields = Vec::new();
    loop {
        skip(&mut chars);
        match chars.peek()? {
            '}' => break,
            ',' => {
                chars.next();
                continue;
            }
            _ => {}
        }
        let key = string(&mut chars)?;
        skip(&mut chars);
        if chars.next()? != ':' {
            return None;
        }
        skip(&mut chars);
        let value = match chars.peek()? {
            '"' => Raw::Text(string(&mut chars)?),
            c if c.is_ascii_digit() => {
                let mut n = String::new();
                while chars.peek().is_some_and(|c| c.is_ascii_digit()) {
                    n.push(chars.next()?);
                }
                Raw::Number(n.parse().ok()?)
            }
            _ => {
                let mut word = String::new();
                while chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    word.push(chars.next()?);
                }
                match word.as_str() {
                    "true" => Raw::Flag(true),
                    "false" => Raw::Flag(false),
                    "null" => Raw::Null,
                    _ => return None,
                }
            }
        };
        fields.push((key, value));
    }
    Some(Fields(fields))
}

fn string(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    if chars.next()? != '"' {
        return None;
    }
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                c => out.push(c),
            },
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests;
