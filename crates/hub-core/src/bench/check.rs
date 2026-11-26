//! The hard checks of a bench case, read from the thread's event log and its run folder after the
//! run. Pure apart from reading files and asking git for the commit count: nothing here starts or
//! answers anything. Each check word reads exactly what docs/architecture/bench.md says.
//!
//! What the log cannot say: whether the thread's status is WaitingOnYou. `settled` reads the
//! permission questions and `ask_user` questions in the log; the runner (PR 3) also holds the
//! thread's status after the last prompt and should fail `settled` itself when it waits on the
//! person (or pass that in here as a new argument).

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::suite::{Case, Check, ProofWant, safe_path};
use crate::model::{AskData, Event, EventKind, PermissionAutoData, PermissionRequestData, PermissionResolvedData, ProofData, TurnEndData, UserMessageData};
use crate::text::clip;

/// One check's verdict. `check` is the check as the suite wrote it (`Check::label`), `why` says
/// what was found when it failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub check: String,
    pub ok: bool,
    pub why: Option<String>,
}

/// How much of a reply or file a failure quotes.
const QUOTE: usize = 200;

/// Every check of `case` over a finished run: the thread's `events`, its `folder`, and `sent`, the
/// seq of each prompt's `user_message`, in order.
pub fn check(case: &Case, events: &[Event], folder: &Path, sent: &[i64]) -> Vec<CheckResult> {
    case.checks
        .iter()
        .map(|c| {
            let verdict = one(c, case, events, folder, sent);
            CheckResult { check: c.label(), ok: verdict.is_ok(), why: verdict.err() }
        })
        .collect()
}

fn one(check: &Check, case: &Case, events: &[Event], folder: &Path, sent: &[i64]) -> Result<(), String> {
    match check {
        Check::Ended(want) => ended(events, sent, want),
        Check::NoError => match events.iter().find(|e| e.kind == EventKind::Error) {
            Some(e) => Err(format!("error at {}: {}", e.seq, clip(e.failure().unwrap_or_default(), QUOTE))),
            None => Ok(()),
        },
        Check::Settled => settled(events),
        Check::RefusedMax(max) => {
            let refused: Vec<PermissionAutoData> = events.iter().filter_map(Event::data_as::<PermissionAutoData>).filter(|p| !p.allowed).collect();
            match refused.first() {
                Some(first) if refused.len() as u64 > *max => Err(format!("{} refused (at most {max}); the first: {}", refused.len(), clip(&first.why, QUOTE))),
                _ => Ok(()),
            }
        }
        Check::File { path, matches, lines } => {
            let text = read(folder, path)?;
            if let Some(n) = lines {
                let have = text.lines().filter(|l| !l.trim().is_empty()).count() as u64;
                if have != *n {
                    return Err(format!("{path} has {have} lines, not {n}"));
                }
            }
            match matches {
                Some(re) if !re.is_match(&text) => Err(format!("{path} does not match {:?}: {}", re.as_str(), clip(&text, QUOTE))),
                _ => Ok(()),
            }
        }
        Check::Unchanged(paths) => {
            let changed: Vec<&str> = paths
                .iter()
                .filter(|p| read(folder, p).ok().as_deref() != case.files.get(*p).map(String::as_str))
                .map(String::as_str)
                .collect();
            if changed.is_empty() { Ok(()) } else { Err(format!("changed or gone: {}", changed.join(", "))) }
        }
        Check::Reply(re) => {
            let reply = crate::replies::last_reply(events);
            if re.is_match(&reply) { Ok(()) } else { Err(format!("the last reply does not match: {}", clip(&reply, QUOTE))) }
        }
        Check::Names(names) => {
            let reply = crate::replies::last_reply(events);
            let found = files_in(folder);
            let mut wrong = Vec::new();
            for name in names {
                if !named(&reply, name) {
                    wrong.push(format!("the reply does not name {name}"));
                } else if !found.iter().any(|f| f == &name.to_lowercase() || f.ends_with(&format!("/{}", name.to_lowercase()))) {
                    wrong.push(format!("no {name} in the folder"));
                }
            }
            if wrong.is_empty() { Ok(()) } else { Err(wrong.join("; ")) }
        }
        Check::HubTool { name, min } => {
            let calls = hub_calls(events, name);
            if calls >= *min { Ok(()) } else { Err(format!("{name} called {calls} times")) }
        }
        Check::Proof(want) => {
            let last = events.iter().rev().find_map(Event::data_as::<ProofData>);
            let got = match last.as_ref().map(|p| p.state.as_str()) {
                Some("shown") => ProofWant::Shown,
                None | Some("skipped") => ProofWant::NotAsked,
                Some(_) => ProofWant::Failed,
            };
            if got == *want { Ok(()) } else { Err(format!("the last proof is {}", last.map_or("absent".to_owned(), |p| p.state))) }
        }
        Check::Page => {
            if events.iter().any(|e| e.kind == EventKind::Page) { Ok(()) } else { Err("no page was shown".into()) }
        }
        Check::Steps(n) => steps(events, *n),
        Check::Commits(n) => {
            let all = crate::git::checked(folder, &["rev-list", "--count", "HEAD"]).map_err(|e| format!("{e:#}"))?;
            let after = all.trim().parse::<u64>().map_err(|e| format!("git counted {all:?}: {e}"))?.saturating_sub(1);
            if after == *n { Ok(()) } else { Err(format!("{after} commits after the fixture")) }
        }
        Check::Logged { kind, name, matches } => logged(events, *kind, name, matches.as_ref()),
    }
}

/// An event of `kind` is in the log; with `matches`, one whose payload as JSON text matches.
fn logged(events: &[Event], kind: EventKind, name: &str, matches: Option<&regex::Regex>) -> Result<(), String> {
    let of_kind: Vec<String> = events.iter().filter(|e| e.kind == kind).map(|e| e.data.to_string()).collect();
    match (matches, of_kind.last()) {
        (_, None) => Err(format!("no {name} event")),
        (None, Some(_)) => Ok(()),
        (Some(re), Some(last)) if !of_kind.iter().any(|d| re.is_match(d)) => {
            Err(format!("{} {name} events, none matches {:?}; the last: {}", of_kind.len(), re.as_str(), clip(last, QUOTE)))
        }
        (Some(_), Some(_)) => Ok(()),
    }
}

/// The turn that answered each sent prompt ended with `want`.
fn ended(events: &[Event], sent: &[i64], want: &str) -> Result<(), String> {
    if sent.is_empty() {
        return Err("no prompt was sent".into());
    }
    let ends: Vec<TurnEndData> = events.iter().filter_map(Event::data_as::<TurnEndData>).collect();
    let mut wrong = Vec::new();
    for seq in sent {
        match ends.iter().find(|end| end.prompt == Some(*seq)) {
            None => wrong.push(format!("prompt {seq} has no turn end")),
            Some(end) if end.stop_reason != want => wrong.push(format!("prompt {seq} ended {:?}", end.stop_reason)),
            Some(_) => {}
        }
    }
    if wrong.is_empty() { Ok(()) } else { Err(wrong.join("; ")) }
}

/// Nothing waits on the person: every permission question has its answer, and every question an
/// agent asked with `ask_user` (an `ask` event; the agent carries on with its assumption) has a
/// later message from the person that quotes it, as the app's answer does.
fn settled(events: &[Event]) -> Result<(), String> {
    let answered: HashSet<String> = events.iter().filter_map(Event::data_as::<PermissionResolvedData>).map(|r| r.id).collect();
    let mut waiting: Vec<String> = events
        .iter()
        .filter_map(Event::data_as::<PermissionRequestData>)
        .filter(|r| !answered.contains(&r.id))
        .map(|r| format!("permission {}", clip(r.tool_call["title"].as_str().unwrap_or("question"), 80)))
        .collect();
    for (at, ask) in events.iter().filter_map(|e| Some((e.seq, e.data_as::<AskData>()?))) {
        let answer = events.iter().filter(|e| e.seq > at).filter_map(Event::data_as::<UserMessageData>).any(|m| m.text.contains(&ask.question));
        if !answer {
            waiting.push(format!("question {:?}", clip(&ask.question, 80)));
        }
    }
    if waiting.is_empty() { Ok(()) } else { Err(format!("left waiting: {}", waiting.join("; "))) }
}

/// Distinct tool calls the log recognises as Hub's tool `name`, under any engine's naming.
fn hub_calls(events: &[Event], name: &str) -> u64 {
    let ids: BTreeSet<String> = events
        .iter()
        .filter(|e| e.kind == EventKind::Update && matches!(crate::model::update_kind(&e.data), Some("tool_call" | "tool_call_update")))
        .filter(|e| crate::permit::hub_tool_name(&e.data).as_deref() == Some(name))
        .map(|e| e.data["toolCallId"].as_str().map_or_else(|| format!("seq {}", e.seq), str::to_owned))
        .collect();
    ids.len() as u64
}

/// The last steps list (`update_steps` writes it as ACP's `plan` update) has `n` entries, all done.
fn steps(events: &[Event], n: u64) -> Result<(), String> {
    let Some(plan) = events.iter().rev().find(|e| e.kind == EventKind::Update && crate::model::update_kind(&e.data) == Some("plan")) else {
        return Err("no steps were listed".into());
    };
    let entries = plan.data["entries"].as_array().map(Vec::as_slice).unwrap_or_default();
    let done = entries.iter().filter(|s| s["status"] == "completed").count();
    if entries.len() as u64 == n && done == entries.len() {
        Ok(())
    } else {
        Err(format!("the last list has {} steps, {done} done", entries.len()))
    }
}

/// A file of the run folder as text; a path outside the folder or a missing file is a failure.
fn read(folder: &Path, path: &str) -> Result<String, String> {
    let rel = safe_path(path).map_err(|e| format!("{e:#}"))?;
    std::fs::read_to_string(folder.join(rel)).map_err(|e| format!("{path}: {e}"))
}

/// Every file in the folder (not `.git`), as a lower-case `/`-separated relative path.
fn files_in(folder: &Path) -> Vec<String> {
    ignore::WalkBuilder::new(folder)
        .standard_filters(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| Some(e.path().strip_prefix(folder).ok()?.to_string_lossy().replace('\\', "/").to_lowercase()))
        .collect()
}

/// The reply names `name` as a file: the name standing on its own (after a space, a `/`, a quote
/// or a tick, and not running on into a longer name), in any case, the way a reader or the app's
/// file cards would pick it out.
pub fn named(reply: &str, name: &str) -> bool {
    let (reply, name) = (reply.to_lowercase(), name.to_lowercase());
    let part_of_name = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    reply.match_indices(&name).any(|(at, _)| {
        let before = reply[..at].chars().next_back();
        let mut after = reply[at + name.len()..].chars();
        let next = after.next();
        let starts = before.is_none_or(|c| !part_of_name(c) && c != '.');
        // A full stop ends the name unless a longer name goes on (`chart.svg.bak`).
        let ends = match next {
            None => true,
            Some('.') => !after.next().is_some_and(part_of_name),
            Some(c) => !part_of_name(c),
        };
        starts && ends
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_counts_only_on_its_own() {
        assert!(named("I wrote `output/orders.csv` and output/chart.svg.", "orders.csv"));
        assert!(named("I wrote `output/orders.csv` and output/chart.svg.", "chart.svg"));
        assert!(named("Read README.md and docs/guide.md", "readme.md"));
        assert!(!named("see old_orders.csv", "orders.csv"));
        assert!(!named("see chart.svg.bak", "chart.svg"));
        assert!(!named("nothing here", "chart.svg"));
    }

    #[test]
    fn an_event_is_found_by_kind_and_payload() {
        let ev = |seq, kind, data: serde_json::Value| Event { thread_id: "t".into(), seq, ts: seq, kind, data };
        let log = [
            ev(1, EventKind::Child, serde_json::json!({ "id": "c", "state": "working" })),
            ev(2, EventKind::Child, serde_json::json!({ "id": "c", "state": "done" })),
        ];
        let done = regex::Regex::new(r#""state":"done""#).unwrap();
        assert_eq!(logged(&log, EventKind::Child, "child", None), Ok(()));
        assert_eq!(logged(&log, EventKind::Child, "child", Some(&done)), Ok(()));
        let working_only = &log[..1];
        let why = logged(working_only, EventKind::Child, "child", Some(&done)).unwrap_err();
        assert!(why.contains("1 child events, none matches") && why.contains("working"), "{why}");
        let none = logged(&log, EventKind::HandoffOffer, "handoff_offer", None).unwrap_err();
        assert_eq!(none, "no handoff_offer event");
    }
}
