//! A project's focus: where it is going (the direction), the stages on the way there with what
//! counts as delivered, what the user still has to test by hand, and the ideas parked for later.
//! It lives in the repository as `docs/FOCUS.md`, plain markdown that agents keep current:
//!
//! ```markdown
//! ## Direction: Hub replaces Claude Code as my daily driver
//! Done when:
//! - [ ] A week without opening Claude Code
//!
//! ## Stage 1: Proof of concept (active)
//! Delivered when:
//! - [x] Threads survive a restart
//! To test:
//! - [ ] Restart hubd mid turn: the thread carries on
//! Ideas: Windows Job Objects
//!
//! ## Parked
//! - Rubber duck
//! ```
//!
//! Every `##` heading other than Direction and Parked is a stage, in order. One stage is active:
//! the one marked `(active)`, or else the first that is not delivered yet.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::project_folder::{add_rule, checkbox};

mod edits;
use edits::{edit_activate, edit_add, edit_idea, edit_park, edit_tick};

/// The focus file, relative to the project folder.
pub const FOCUS: &str = "docs/FOCUS.md";

/// The rule that asks agents to keep the focus current.
pub const FOCUS_RULE: &str = "`docs/FOCUS.md` holds the project's direction and its stages. When \
you finish work that belongs to the active stage, tick off what it delivered under \"Delivered \
when\" and add what the user must test by hand under \"To test\" (one line each: what to do and \
what should happen). Ideas that do not belong to the active stage go under \"Parked\" instead of \
being built on the side.";

const ACTIVE: &str = " (active)";

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct Focus {
    /// The file is there. The rest is empty when it is not.
    pub exists: bool,
    pub direction: Option<Stage>,
    pub stages: Vec<Stage>,
    /// Index in `stages` of the stage being worked on. None when every stage is delivered.
    #[ts(type = "number | null")]
    pub active: Option<usize>,
    pub parked: Vec<Idea>,
    /// The folder's CLAUDE.md asks agents to keep the focus current.
    pub rule: bool,
}

/// A stage, or the direction, which has the same parts but no tests.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct Stage {
    pub title: String,
    /// The text under the heading that is not a list.
    pub about: String,
    /// Line of the heading, from 0, used to add to the stage or make it active.
    #[ts(type = "number")]
    pub line: u32,
    /// The heading says `(active)`.
    pub marked: bool,
    /// What counts as delivered ("Delivered when", or "Done when" for the direction).
    pub done: Vec<Check>,
    pub test: Vec<Check>,
    pub ideas: Vec<Idea>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Check {
    pub text: String,
    pub done: bool,
    #[ts(type = "number")]
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Idea {
    pub text: String,
    #[ts(type = "number")]
    pub line: u32,
}

impl Stage {
    /// Everything it promised is ticked off.
    pub fn delivered(&self) -> bool {
        !self.done.is_empty() && self.done.iter().all(|c| c.done)
    }
}

/// One line about a project's focus, for lists of projects.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FocusSummary {
    /// The stage now, or None when every stage is delivered.
    pub stage: Option<String>,
    /// Which stage it is, from 1, and how many there are.
    #[ts(type = "number")]
    pub n: usize,
    #[ts(type = "number")]
    pub stages: usize,
    /// Its "Delivered when" items ticked, and how many there are.
    #[ts(type = "number")]
    pub done: usize,
    #[ts(type = "number")]
    pub of: usize,
    /// Its "To test" items not ticked yet.
    #[ts(type = "number")]
    pub to_test: usize,
}

/// The focus of a project in a line, None when it has no focus file.
pub fn summary(root: &Path) -> Option<FocusSummary> {
    let f = read(root);
    if !f.exists {
        return None;
    }
    let stage = f.active.and_then(|i| f.stages.get(i));
    Some(FocusSummary {
        stage: stage.map(|s| s.title.clone()),
        n: f.active.map_or(f.stages.len(), |i| i + 1),
        stages: f.stages.len(),
        done: stage.map_or(0, |s| s.done.iter().filter(|c| c.done).count()),
        of: stage.map_or(0, |s| s.done.len()),
        to_test: stage.map_or(0, |s| s.test.iter().filter(|c| !c.done).count()),
    })
}

/// A change to the focus. `op` is one of:
/// - `tick`: tick or untick the checkbox on `line` (`done`).
/// - `add`: add `text` to `list` ("done" or "test") of the stage whose heading is on `line`.
/// - `park`: add `text` under Parked.
/// - `idea`: add `text` to the ideas of the stage whose heading is on `line`.
/// - `activate`: make the stage whose heading is on `line` the active one.
/// - `start`: write a first focus file when there is none, and add the rule.
/// - `rule`: ask agents in CLAUDE.md to keep the focus current.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct FocusEdit {
    pub path: String,
    pub op: String,
    #[serde(default)]
    #[ts(type = "number", optional)]
    pub line: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub done: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub list: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub text: Option<String>,
}

pub fn read(root: &Path) -> Focus {
    let rule = std::fs::read_to_string(root.join("CLAUDE.md")).unwrap_or_default().contains(FOCUS);
    match std::fs::read_to_string(root.join(FOCUS)) {
        Ok(text) => Focus { rule, ..parse(&text) },
        Err(_) => Focus { rule, ..Focus::default() },
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Direction,
    Stage,
    Parked,
}

#[derive(Clone, Copy, PartialEq)]
enum List {
    About,
    Done,
    Test,
    Ideas,
}

/// What a `##` heading is: its part and its title without `Stage 2:` or `(active)`.
fn heading(text: &str) -> (Part, String, bool) {
    let text = text.trim();
    let lower = text.to_lowercase();
    let marked = lower.trim_end().ends_with("(active)") || lower.trim_end().ends_with("(aktiv)");
    let mut title = text;
    if marked {
        title = title[..title.rfind('(').unwrap_or(title.len())].trim_end();
    }
    for (word, part) in [("direction", Part::Direction), ("riktning", Part::Direction), ("parked", Part::Parked), ("parkerat", Part::Parked)] {
        if lower.starts_with(word) {
            let rest = title[word.len()..].trim_start_matches([':', ' ', '-', '–']).trim();
            return (part, rest.to_owned(), marked);
        }
    }
    // "Stage 2: Daily driver", "Etapp 2 – Daily driver", or just "Daily driver".
    for word in ["stage", "etapp"] {
        if lower.starts_with(word) && !title[word.len()..].starts_with(char::is_alphabetic) {
            let rest = title[word.len()..].trim_start_matches(|c: char| c.is_ascii_digit() || " :.-–".contains(c));
            if !rest.is_empty() {
                title = rest;
            }
        }
    }
    (Part::Stage, title.to_owned(), marked)
}

/// A line that starts a list: "Delivered when:", "To test:", "Ideas: a, b". Returns the list and
/// what follows the colon.
fn label(line: &str) -> Option<(List, &str)> {
    let (head, rest) = line.split_once(':')?;
    let head = head.trim().trim_matches('*').trim().to_lowercase();
    let list = match head.as_str() {
        "done when" | "delivered when" | "klart när" | "levererat när" => List::Done,
        "to test" | "att testa" => List::Test,
        _ if head.starts_with("ideas") || head.starts_with("idéer") => List::Ideas,
        _ => return None,
    };
    Some((list, rest.trim()))
}

fn bullet(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix(['-', '*', '+'])?.strip_prefix(' ')?.trim();
    (!rest.is_empty()).then_some(rest)
}

pub fn parse(text: &str) -> Focus {
    let mut focus = Focus { exists: true, ..Focus::default() };
    // The section being read: its part and the stage it fills (the direction is a stage too).
    let mut part: Option<Part> = None;
    let mut stage = Stage::default();
    let mut list = List::About;
    let close = |part: Option<Part>, stage: Stage, focus: &mut Focus| match part {
        Some(Part::Direction) => focus.direction = Some(stage),
        Some(Part::Stage) => focus.stages.push(stage),
        _ => {}
    };
    for (i, raw) in text.lines().enumerate() {
        let line = i as u32;
        if let Some(h) = raw.strip_prefix("## ") {
            close(part, std::mem::take(&mut stage), &mut focus);
            let (p, title, marked) = heading(h);
            part = Some(p);
            stage = Stage { title, line, marked, ..Stage::default() };
            list = if p == Part::Parked { List::Ideas } else { List::About };
            continue;
        }
        if raw.starts_with('#') || raw.trim().is_empty() {
            continue;
        }
        if part == Some(Part::Parked) {
            if let Some(text) = bullet(raw) {
                focus.parked.push(Idea { text: text.to_owned(), line });
            }
            continue;
        }
        if part.is_none() {
            continue;
        }
        if let Some((done, text)) = checkbox(raw) {
            let check = Check { text: text.to_owned(), done, line };
            match list {
                List::Test => stage.test.push(check),
                _ => stage.done.push(check),
            }
        } else if let Some((l, rest)) = label(raw) {
            list = l;
            if !rest.is_empty() {
                if l == List::Ideas {
                    stage.ideas.push(Idea { text: rest.to_owned(), line });
                } else {
                    push_about(&mut stage.about, rest);
                }
            }
        } else if let (List::Ideas, Some(text)) = (list, bullet(raw)) {
            stage.ideas.push(Idea { text: text.to_owned(), line });
        } else {
            list = List::About;
            push_about(&mut stage.about, raw.trim());
        }
    }
    close(part, stage, &mut focus);
    focus.active = focus
        .stages
        .iter()
        .position(|s| s.marked)
        .or_else(|| focus.stages.iter().position(|s| !s.delivered()));
    focus
}

fn push_about(about: &mut String, text: &str) {
    if !about.is_empty() {
        about.push(' ');
    }
    about.push_str(text);
}

const STARTER: &str = "# Focus

Where this project is going and what counts as done. Hub shows it on the project page; agents
tick off what they deliver and add what you need to test by hand.

## Direction: Say in one line where the project is going

Done when:
- [ ] How you will know the project has got there

## Stage 1: The first stage on the way (active)

Delivered when:
- [ ] What this stage delivers

To test:

## Parked

";

pub fn edit(root: &Path, edit: &FocusEdit) -> Result<Focus> {
    let path = root.join(FOCUS);
    match edit.op.as_str() {
        "rule" => {
            add_rule(&root.join("CLAUDE.md"), FOCUS_RULE)?;
            return Ok(read(root));
        }
        "start" => {
            if !path.exists() {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&path, STARTER)?;
            }
            add_rule(&root.join("CLAUDE.md"), FOCUS_RULE)?;
            return Ok(read(root));
        }
        _ => {}
    }
    let text = std::fs::read_to_string(&path).context("the project has no docs/FOCUS.md yet")?;
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    match edit.op.as_str() {
        "tick" => edit_tick(&mut lines, edit)?,
        "activate" => edit_activate(&mut lines, edit)?,
        "add" => edit_add(&mut lines, edit)?,
        "idea" => edit_idea(&mut lines, edit)?,
        "park" => edit_park(&mut lines, edit)?,
        other => bail!("unknown change {other:?}"),
    }
    let text = lines.join("\n") + "\n";
    std::fs::write(&path, &text).with_context(|| format!("writing {}", path.display()))?;
    Ok(read(root))
}

/// Takes an idea out again: a bullet under Parked or a stage's ideas, or one part of an
/// `Ideas: a, b` line. Returns whether it was found.
pub fn remove_idea(root: &Path, text: &str) -> Result<bool> {
    let path = root.join(FOCUS);
    let Ok(old) = std::fs::read_to_string(&path) else {
        return Ok(false);
    };
    let want = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut lines: Vec<String> = old.lines().map(str::to_owned).collect();
    let mut found = false;
    let mut part = None;
    let mut list = List::About;
    for i in 0..lines.len() {
        let raw = lines[i].clone();
        if let Some(h) = raw.strip_prefix("## ") {
            part = Some(heading(h).0);
            list = if part == Some(Part::Parked) { List::Ideas } else { List::About };
            continue;
        }
        if let Some((l, rest)) = label(&raw) {
            list = l;
            if l == List::Ideas && !rest.is_empty() {
                let parts: Vec<&str> = rest.split(", ").collect();
                if let Some(k) = parts.iter().position(|p| p.trim() == want) {
                    let left: Vec<&str> = parts.iter().enumerate().filter(|&(j, _)| j != k).map(|(_, p)| *p).collect();
                    let head = &raw[..raw.find(':').map_or(0, |c| c + 1)];
                    lines[i] = if left.is_empty() { head.to_owned() } else { format!("{head} {}", left.join(", ")) };
                    found = true;
                    break;
                }
            }
            continue;
        }
        if list == List::Ideas && part.is_some() && checkbox(&raw).is_none() && bullet(&raw) == Some(want.as_str()) {
            lines.remove(i);
            found = true;
            break;
        }
    }
    if found {
        std::fs::write(&path, lines.join("\n") + "\n").with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(found)
}

/// The line after a section: the next `##` heading, or the end.
pub(super) fn section_end(lines: &[String], at: usize) -> usize {
    (at + 1..lines.len()).find(|&i| lines[i].starts_with("## ")).unwrap_or(lines.len())
}

/// The first message of a thread that writes the focus file for the user to react to.
pub fn draft_prompt() -> String {
    format!(
        "Draft `{FOCUS}` for this project so I can react to it. Read the README, `docs/ROADMAP.md`, \
`docs/plan.md` and the other docs if they are there, and the last few weeks of git history.\n\n\
Use this shape:\n\n{STARTER}\n\
- **Direction**: where the project is going, in one line, with a short \"Done when\" list.\n\
- **Stages**: 2 to 5 stages on the way, in order, each with \"Delivered when\" (tick what is \
already built) and \"To test\" for things that were built but I have probably not tried yet. \
Mark the stage we are in with `(active)`. Add `Ideas:` with roadmap ideas that belong to the \
stage.\n\
- **Parked**: good ideas that do not move the direction forward right now.\n\n\
Keep each line short. Then show me the stages and ask what to change."
    )
}

const CHECK: &str = "Below is the stage a software project is in now and the first message of a \
new thread in it. Do not use tools. Say whether the thread's work belongs to this stage. It does \
when it helps deliver one of the stage's items, fixes or tests something the stage covers, or is \
small upkeep any stage needs (a bug fix, a question, a chore, docs). It does not when it starts a \
new feature or idea that belongs to a later stage or to none. When unsure, it belongs. Answer with \
one line of JSON and nothing else: {\"fits\": true or false, \"why\": \"at most 12 words\"}.";

/// What a small model is asked about a new thread and the stage now.
pub fn check_prompt(stage: &Stage, ask: &str) -> String {
    let mut text = format!("{CHECK}\n\n<stage>\n{}\n", stage.title);
    if !stage.about.is_empty() {
        text.push_str(&format!("{}\n", stage.about));
    }
    for c in &stage.done {
        text.push_str(&format!("- {}{}\n", c.text, if c.done { " (done)" } else { "" }));
    }
    for i in &stage.ideas {
        text.push_str(&format!("Ideas: {}\n", i.text));
    }
    let ask: String = ask.chars().take(3_000).collect();
    text.push_str(&format!("</stage>\n\n<message>\n{ask}\n</message>"));
    text
}

/// The model's verdict: whether it fits, and why. None when the answer is not the JSON asked for.
pub fn read_check(answer: &str) -> Option<(bool, String)> {
    let (a, b) = (answer.find('{')?, answer.rfind('}')?);
    let v: serde_json::Value = serde_json::from_str(answer.get(a..=b)?).ok()?;
    Some((v["fits"].as_bool()?, v["why"].as_str().unwrap_or_default().trim().to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUB: &str = "# Focus

Intro text.

## Direction: Hub replaces Claude Code

All my AI work goes through Hub.

Done when:
- [ ] A week without Claude Code
- [x] Threads on the PC

## Stage 1: Proof of concept

Delivered when:
- [x] Survives restarts
- [x] Resumes after quota

To test:
- [ ] Restart hubd mid turn
Ideas: Job Objects, strays by process

## Etapp 2 – Daily driver (active)

A normal day in Hub.

Levererat när:
- [ ] Undo per thread

## Parked

- Rubber duck
- AI DevOps
";

    #[test]
    fn reads_direction_stages_and_parked() {
        let f = parse(HUB);
        let d = f.direction.as_ref().unwrap();
        assert_eq!(d.title, "Hub replaces Claude Code");
        assert_eq!(d.about, "All my AI work goes through Hub.");
        assert_eq!(d.done.len(), 2);
        assert_eq!(f.stages.len(), 2);
        let s = &f.stages[0];
        assert_eq!(s.title, "Proof of concept");
        assert!(s.delivered());
        assert_eq!(s.test.len(), 1);
        assert_eq!(s.test[0].text, "Restart hubd mid turn");
        assert_eq!(s.ideas[0].text, "Job Objects, strays by process");
        assert_eq!(f.stages[1].title, "Daily driver");
        assert!(f.stages[1].marked);
        assert_eq!(f.stages[1].about, "A normal day in Hub.");
        assert_eq!(f.active, Some(1));
        assert_eq!(f.parked.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), ["Rubber duck", "AI DevOps"]);
    }

    #[test]
    fn asks_and_reads_the_focus_check() {
        let f = parse(HUB);
        let prompt = check_prompt(&f.stages[1], "Add a 3D world");
        assert!(prompt.contains("<stage>\nDaily driver\nA normal day in Hub.\n- Undo per thread\n"));
        assert!(prompt.ends_with("<message>\nAdd a 3D world\n</message>"));
        assert_eq!(read_check("Sure: {\"fits\": false, \"why\": \"A later idea\"}"), Some((false, "A later idea".into())));
        assert_eq!(read_check("{\"fits\": true}"), Some((true, String::new())));
        assert_eq!(read_check("no idea"), None);
    }

    #[test]
    fn sums_up_the_stage_now() {
        let dir = tempfile::tempdir().unwrap();
        assert!(summary(dir.path()).is_none());
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(dir.path().join(FOCUS), HUB).unwrap();
        let s = summary(dir.path()).unwrap();
        assert_eq!((s.stage.as_deref(), s.n, s.stages, s.done, s.of, s.to_test), (Some("Daily driver"), 2, 2, 0, 1, 0));
    }

    #[test]
    fn reads_hubs_own_focus() {
        let f = parse(include_str!("../../../docs/FOCUS.md"));
        assert!(f.direction.as_ref().is_some_and(|d| !d.done.is_empty()));
        assert!(f.stages.len() >= 2 && f.active.is_some());
        assert!(f.stages.iter().all(|s| !s.title.is_empty() && !s.done.is_empty()));
    }

    #[test]
    fn the_first_stage_not_delivered_is_active_when_none_is_marked() {
        let f = parse(&HUB.replace(" (active)", ""));
        assert_eq!(f.active, Some(1));
        let f = parse("## Stage: A\n- [x] one\n## Stage: B\n- [x] two\n");
        assert_eq!(f.active, None);
    }

    #[test]
    fn edits_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let e = |op: &str| FocusEdit { path: String::new(), op: op.into(), line: None, done: None, list: None, text: None };
        assert!(edit(root, &e("tick")).is_err(), "no file yet");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join(FOCUS), HUB).unwrap();
        let f = read(root);
        let first = f.stages[0].line;

        // A test goes after the list's last item; a stage without that list gets one.
        let f = edit(root, &FocusEdit { line: Some(first), list: Some("test".into()), text: Some("Install  without Node".into()), ..e("add") }).unwrap();
        assert_eq!(f.stages[0].test.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["Restart hubd mid turn", "Install without Node"]);
        let second = f.stages[1].line;
        let f = edit(root, &FocusEdit { line: Some(second), list: Some("test".into()), text: Some("Undo a thread".into()), ..e("add") }).unwrap();
        assert_eq!(f.stages[1].test[0].text, "Undo a thread");
        assert_eq!(f.stages[1].done.len(), 1, "still one delivered item");
        assert_eq!(f.parked.len(), 2, "parked untouched");

        let test = f.stages[0].test[0].line;
        let f = edit(root, &FocusEdit { line: Some(test), done: Some(true), ..e("tick") }).unwrap();
        assert!(f.stages[0].test[0].done);
        assert!(edit(root, &FocusEdit { line: Some(0), done: Some(true), ..e("tick") }).is_err(), "not a checkbox");

        let f = edit(root, &FocusEdit { line: Some(first), ..e("activate") }).unwrap();
        assert_eq!(f.active, Some(0));
        assert!(!f.stages[1].marked);
        assert!(std::fs::read_to_string(root.join(FOCUS)).unwrap().contains("## Stage 1: Proof of concept (active)\n"));

        let f = edit(root, &FocusEdit { text: Some("Voice".into()), ..e("park") }).unwrap();
        assert_eq!(f.parked.last().unwrap().text, "Voice");
    }

    #[test]
    fn adds_and_removes_ideas() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let e = |op: &str| FocusEdit { path: String::new(), op: op.into(), line: None, done: None, list: None, text: None };
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join(FOCUS), HUB).unwrap();
        let f = read(root);
        let first = f.stages[0].line;

        // After an inline Ideas line, and as a new list in a stage without one.
        let f = edit(root, &FocusEdit { line: Some(first), text: Some("Idea inbox".into()), ..e("idea") }).unwrap();
        assert_eq!(f.stages[0].ideas.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["Job Objects, strays by process", "Idea inbox"]);
        let second = f.stages[1].line;
        let f = edit(root, &FocusEdit { line: Some(second), text: Some("Recipes".into()), ..e("idea") }).unwrap();
        assert_eq!(f.stages[1].ideas[0].text, "Recipes");
        assert_eq!(f.stages[1].done.len(), 1);
        assert!(edit(root, &FocusEdit { line: Some(0), text: Some("x".into()), ..e("idea") }).is_err(), "not a stage");

        assert!(remove_idea(root, "Idea inbox").unwrap());
        assert!(remove_idea(root, "Recipes").unwrap());
        assert!(remove_idea(root, "Rubber duck").unwrap());
        assert!(remove_idea(root, "strays by process").unwrap());
        assert!(!remove_idea(root, "Not there").unwrap());
        let f = read(root);
        assert_eq!(f.stages[0].ideas[0].text, "Job Objects");
        assert!(f.stages[1].ideas.is_empty());
        assert_eq!(f.parked.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), ["AI DevOps"]);
        assert_eq!(f.stages[0].test.len(), 1, "tests untouched");
    }

    #[test]
    fn starts_a_focus_and_parks_without_a_parked_section() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let e = |op: &str| FocusEdit { path: String::new(), op: op.into(), line: None, done: None, list: None, text: None };
        let f = edit(root, &e("start")).unwrap();
        assert!(f.exists && f.rule);
        assert_eq!(f.stages.len(), 1);
        assert_eq!(f.active, Some(0));
        std::fs::write(root.join(FOCUS), "## Stage: A\n").unwrap();
        let f = edit(root, &FocusEdit { text: Some("Later".into()), ..e("park") }).unwrap();
        assert_eq!(f.parked[0].text, "Later");
        let f = edit(root, &FocusEdit { line: Some(0), list: Some("done".into()), text: Some("Ships".into()), ..e("add") }).unwrap();
        assert_eq!(f.stages[0].done[0].text, "Ships");
        assert!(std::fs::read_to_string(root.join(FOCUS)).unwrap().starts_with("## Stage: A\n\nDelivered when:\n- [ ] Ships\n"));
    }
}
