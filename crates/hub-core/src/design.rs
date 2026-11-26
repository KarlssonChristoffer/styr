//! Design mode: for something big, the thread plans before it builds. The agent (the
//! "elephant", which holds the whole conversation) talks the feature through with the user and
//! writes one design document under `docs/design/`, and changes nothing else: Hub refuses other
//! edits and commands that change state while the mode is on (`permit::design`). Then fresh
//! sessions that never saw the conversation (the "goldfish") read only the document and look for
//! what it leaves out: one explains it back, two look for gaps and for what it misses in the code,
//! and one says whether it is ready to build from. Their findings go back to the agent, which
//! settles them with the user. The idea is Google's Elephant-Goldfish Model; this is Hub's own
//! take on it, run on whatever engine the thread uses.
//!
//! Everything is a `design` event in the thread's log (see `EventKind::Design`).

use crate::locked::LockExt;
use std::path::Path;
use std::sync::Mutex;

use anyhow::{Result, bail};

use crate::ctx::Ctx;
use crate::model::{DesignCheck, DesignData, Event, EventKind};

/// Where design documents live, relative to the thread's folder.
pub const FOLDER: &str = "docs/design";

/// Told to the agent with the first message after design mode was turned on.
pub const ON_NOTE: &str = "Hub: design mode is on. Plan this with the user before any code. Ask \
questions until the goal, the scope and the trade-offs are clear; read the code you need. Then \
write the design in one Markdown file under docs/design/, named after the feature (for example \
docs/design/<feature>.md), with: the problem, the plan, the alternatives you rejected and why, \
every file to add or change with what changes in it (data types, function signatures, API \
shapes), how it will be tested, and the open questions. Change no other file, commit nothing and \
install nothing: Hub refuses it while design mode is on. When the document is ready, call Hub's \
review_design tool (or tell the user to press Review design): fresh reviewers who never saw this \
conversation read only the document and look for what it leaves out.";

/// Told to the agent with the first message after design mode was turned off.
pub const OFF_NOTE: &str = "Hub: design mode is off. You may change code now. Build from the \
design document under docs/design/, and update the document where the build has to differ.";

/// Sent to the agent with the goldfish's findings.
pub(crate) const SEND_BACK: &str = "Fresh reviewers who never saw our conversation read only your design \
document. Go through what they found with the user: fix the document where they are right, ask \
the user where it is a decision only they can make, and say briefly why for anything you leave \
as it is. Do not write code yet. When the document is updated, say whether it is worth another \
review.";

/// One goldfish: what it is called in the log, what the user sees, what it is told, the verdict
/// that means it found nothing, and whether it gets the review model.
pub(crate) struct Check {
    pub(crate) name: &'static str,
    pub(crate) title: &'static str,
    pub(crate) prompt: &'static str,
    fine: &'static str,
    pub(crate) strong: bool,
}

pub(crate) const CHECKS: &[Check] = &[
    Check {
        name: "explain",
        title: "Explains it back",
        prompt: "You are a fresh reader of a design document for a software change. You have \
not seen the conversation it came from, and you must not read any other file: judge only what \
the document says. Explain back, in your own words, what will be built, how it will work and \
which files change in what order. Then, under \"Had to guess\", list every point where the \
document left you guessing or could be read two ways, and what you guessed. The first line of \
your answer is only the verdict: \"Clear\" when you had to guess nothing, otherwise \"Unclear in \
N places\".",
        fine: "clear",
        strong: false,
    },
    Check {
        name: "gaps",
        title: "Looks for gaps",
        prompt: "You are a skeptical reviewer of a design document for a software change, and \
you have not seen the conversation it came from. Find what would go wrong if it were built as \
written: missed edge cases, failure and error paths, races, data that can be lost, security \
holes, steps that contradict each other, and requirements that are assumed but never stated. \
You may read the repository for context, but change nothing and run nothing that changes \
state. Number your findings and mark each Blocking or Minor, with what to change in the \
document. Skip style. The first line of your answer is only the verdict: \"No gaps\", or \"N \
blocking, M minor\".",
        fine: "no gaps",
        strong: true,
    },
    Check {
        name: "fit",
        title: "Checks it against the code",
        prompt: "You review a design document for a change to the repository you are in, and \
you have not seen the conversation it came from. Read the code the document touches and check \
that it fits: file paths, types and functions it names exist (or it says it adds them), hidden \
dependencies and callers it does not mention, things it would break, and existing pieces it \
duplicates instead of reusing. Change nothing and run nothing that changes state. Number your \
findings with the file each is about. The first line of your answer is only the verdict: \"Fits \
the code\", or \"N problems\".",
        fine: "fits",
        strong: false,
    },
    Check {
        name: "ready",
        title: "Ready to build?",
        prompt: "You decide whether a design document for a software change is ready to be \
built by an engineer who cannot ask its author anything. Check that it names every file to add \
or change, the data structures and function signatures, the API shapes, how it is tested, and \
that nothing essential is left as an open question. You may read the repository to check names. \
Change nothing. The first line of your answer is only the verdict: \"Ready to build\" or \"Not \
ready\". Then list exactly what is missing, if anything.",
        fine: "ready to build",
        strong: true,
    },
];

/// Whether design mode is on: the last `{ on }` design event in the log says so.
pub fn on_in(events: &[Event]) -> bool {
    events
        .iter()
        .rev()
        .filter_map(Event::data_as::<DesignData>)
        .find_map(|d| d.on)
        .unwrap_or(false)
}

pub async fn is_on(ctx: &Ctx<'_>, thread_id: &str) -> bool {
    ctx.store.events_of(thread_id, 0, &[EventKind::Design]).await.map(|e| on_in(&e)).unwrap_or(false)
}

/// The note for the agent when design mode was turned on or off among `since`, the events since
/// the last user message, newest first.
pub fn note_since(since: &[&Event]) -> Option<&'static str> {
    let on = since.iter().filter_map(|e| e.data_as::<DesignData>()).find_map(|d| d.on)?;
    Some(if on { ON_NOTE } else { OFF_NOTE })
}

/// Whether `path` (absolute, or relative to `cwd`) is under the folder's `docs/design/`.
pub fn in_design_folder(path: &str, cwd: &Path) -> bool {
    let norm = |p: &str| {
        let p = p.replace('\\', "/");
        if cfg!(windows) { p.to_lowercase() } else { p }
    };
    let path = norm(path);
    let cwd = norm(&cwd.display().to_string());
    let cwd = cwd.trim_end_matches('/');
    let rel = match path.strip_prefix(cwd) {
        Some(r) if r.starts_with('/') => r.trim_start_matches('/'),
        _ if Path::new(&path).is_absolute() || path.starts_with('/') => return false,
        _ => path.trim_start_matches("./"),
    };
    rel.starts_with(&format!("{FOLDER}/")) && !rel.split('/').any(|part| part == "..")
}

/// The design document to review: `doc` when given, else the last one the thread wrote under
/// `docs/design/`, else the newest Markdown file there. Relative to the folder.
pub fn find_doc(events: &[Event], cwd: &Path, doc: Option<&str>) -> Result<String> {
    if let Some(doc) = doc.map(str::trim).filter(|d| !d.is_empty()) {
        let rel = relative(doc, cwd);
        if !cwd.join(&rel).is_file() {
            bail!("{doc} is not a file in the thread's folder");
        }
        return Ok(rel);
    }
    let written = events
        .iter()
        .rev()
        .filter(|e| e.kind == EventKind::Update)
        .flat_map(|e| crate::clash::edited(&e.data, cwd))
        .map(|p| relative(&p, cwd))
        .find(|p| in_design_folder(p, cwd) && p.ends_with(".md") && cwd.join(p).is_file());
    if let Some(found) = written {
        return Ok(found);
    }
    let newest = std::fs::read_dir(cwd.join(FOLDER))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.file_name().to_string_lossy().into_owned())))
        .max();
    match newest {
        Some((_, name)) => Ok(format!("{FOLDER}/{name}")),
        None => bail!("no design document yet: the agent writes one under {FOLDER}/ first"),
    }
}

/// `path` relative to `cwd`, with `/` separators, when it is inside it.
fn relative(path: &str, cwd: &Path) -> String {
    let path = path.replace('\\', "/");
    let base = cwd.display().to_string().replace('\\', "/");
    let base = base.trim_end_matches('/');
    let same = |a: &str, b: &str| if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b };
    if path.len() > base.len() && same(&path[..base.len()], base) && path[base.len()..].starts_with('/') {
        path[base.len() + 1..].to_owned()
    } else {
        path.trim_start_matches("./").to_owned()
    }
}

/// The verdict line of an answer, short enough to show.
pub(crate) fn verdict(text: &str) -> String {
    let first = text.trim().lines().next().unwrap_or_default().replace(['*', '_', '#', '"'], "");
    let first = first.trim().trim_end_matches(['.', ':']);
    if first.chars().count() <= 60 { first.to_owned() } else { String::new() }
}

/// Whether a check found nothing: its verdict starts with what "nothing" reads as.
fn fine(check: &Check, verdict: &str) -> bool {
    let v = verdict.trim().to_lowercase();
    v.starts_with(check.fine) && !v.starts_with("not ")
}

/// Whether one of the thread's design reviews is still running.
pub(crate) fn review_running(events: &[Event]) -> bool {
    let mut states = std::collections::HashMap::new();
    for d in events.iter().filter_map(Event::data_as::<DesignData>) {
        if let (Some(id), Some(state)) = (d.review, d.state) {
            states.insert(id, state);
        }
    }
    states.values().any(|s| s == "running")
}

/// Adds up the checks once they have all ended: the review's verdict, and what to send back to
/// the agent (the checks that found something).
pub(crate) fn settle(data: &Mutex<DesignData>) -> (DesignData, String) {
        let mut d = data.locked();
        let checks: Vec<DesignCheck> = d.checks.clone().unwrap_or_default();
        if !checks.iter().any(|c| c.state == "done") {
            d.state = Some("failed".into());
            d.error = Some(checks.iter().find_map(|c| c.error.as_deref()).unwrap_or("the review failed").to_owned());
            (d.clone(), String::new())
        } else {
            let mut findings = Vec::new();
            for (check, c) in CHECKS.iter().zip(&checks) {
                if c.state != "done" || fine(check, c.verdict.as_deref().unwrap_or_default()) {
                    continue;
                }
                findings.push(format!("## {}\n\n{}", check.title, c.text.as_deref().unwrap_or_default()));
            }
            let ready = checks
                .iter()
                .zip(CHECKS)
                .find(|(_, check)| check.name == "ready")
                .is_some_and(|(c, check)| c.state == "done" && fine(check, c.verdict.as_deref().unwrap_or_default()));
            d.state = Some("done".into());
            d.ready = Some(ready);
            d.verdict = Some(if findings.is_empty() {
                "Ready to build".to_owned()
            } else if ready {
                format!("Ready to build, {} {} found more", findings.len(), if findings.len() == 1 { "check" } else { "checks" })
            } else {
                "Not ready".to_owned()
            });
            d.sent = Some(!findings.is_empty());
            (d.clone(), findings.join("\n\n"))
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_json::Value;

    fn event(seq: i64, kind: EventKind, data: Value) -> Event {
        Event { thread_id: "t".into(), seq, ts: seq, kind, data }
    }

    #[test]
    fn on_follows_the_last_switch() {
        let mut log = vec![event(1, EventKind::Design, json!({ "on": true, "by": "you" }))];
        assert!(on_in(&log));
        log.push(event(2, EventKind::Design, json!({ "review": "r", "state": "done" })));
        assert!(on_in(&log), "a review is not a switch");
        log.push(event(3, EventKind::Design, json!({ "on": false, "by": "you" })));
        assert!(!on_in(&log));
        assert!(!on_in(&[]));
    }

    #[test]
    fn the_note_goes_with_the_next_message() {
        let on = event(1, EventKind::Design, json!({ "on": true }));
        let off = event(2, EventKind::Design, json!({ "on": false }));
        assert_eq!(note_since(&[&on]), Some(ON_NOTE));
        // Newest first: turned on, then off again before writing.
        assert_eq!(note_since(&[&off, &on]), Some(OFF_NOTE));
        assert_eq!(note_since(&[]), None);
    }

    #[test]
    fn only_the_design_folder_counts() {
        let cwd = Path::new("/work/hub");
        assert!(in_design_folder("/work/hub/docs/design/plugins.md", cwd));
        assert!(in_design_folder("docs/design/plugins.md", cwd));
        assert!(in_design_folder("./docs/design/sub/x.md", cwd));
        assert!(!in_design_folder("/work/hub/docs/ARCHITECTURE.md", cwd));
        assert!(!in_design_folder("/work/hub/crates/hub-core/src/hub.rs", cwd));
        assert!(!in_design_folder("/work/hub/docs/design/../../src/main.rs", cwd));
        assert!(!in_design_folder("/elsewhere/docs/design/x.md", cwd));
        assert!(!in_design_folder("/work/hub2/docs/design/x.md", cwd));
    }

    #[test]
    fn verdicts_that_found_nothing() {
        let by = |name: &str| CHECKS.iter().find(|c| c.name == name).unwrap();
        assert!(fine(by("explain"), "Clear"));
        assert!(!fine(by("explain"), "Unclear in 3 places"));
        assert!(fine(by("gaps"), "No gaps"));
        assert!(!fine(by("gaps"), "2 blocking, 1 minor"));
        assert!(fine(by("fit"), "Fits the code"));
        assert!(fine(by("ready"), "Ready to build"));
        assert!(!fine(by("ready"), "Not ready"));
        assert_eq!(verdict("**Ready to build.**\n\nAll there."), "Ready to build");
    }

    #[test]
    fn finds_the_document_the_thread_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path();
        std::fs::create_dir_all(cwd.join(FOLDER)).unwrap();
        std::fs::write(cwd.join("docs/design/old.md"), "# Old").unwrap();
        std::fs::write(cwd.join("docs/design/plugins.md"), "# Plugins").unwrap();
        let wrote = event(
            1,
            EventKind::Update,
            json!({
                "sessionUpdate": "tool_call",
                "kind": "edit",
                "status": "completed",
                "content": [{ "type": "diff", "path": cwd.join("docs/design/plugins.md").display().to_string() }],
            }),
        );
        assert_eq!(find_doc(&[wrote], cwd, None).unwrap(), "docs/design/plugins.md");
        assert_eq!(find_doc(&[], cwd, Some("docs/design/old.md")).unwrap(), "docs/design/old.md");
        assert!(find_doc(&[], cwd, Some("docs/design/missing.md")).is_err());
        assert!(find_doc(&[], cwd, None).unwrap().starts_with("docs/design/"));
        let empty = tempfile::tempdir().unwrap();
        assert!(find_doc(&[], empty.path(), None).is_err());
    }
}
