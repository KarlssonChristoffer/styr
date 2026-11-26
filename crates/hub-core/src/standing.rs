//! The rules in force for a project: what its rules files (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`)
//! tell every agent, one rule per top-level bullet, each with where it sits and when it last
//! changed (`git blame`, so the day it was said, not the day the file was copied). Read only: it
//! changes nothing and starts no agent. Rules said in chat land under `## Rules from Hub`
//! (`hub/send.rs`) and are marked `hub` here, the rest `repo`.

use std::path::Path;

use serde::Serialize;
use ts_rs::TS;

use crate::git;

/// The rules files agents read, in the order they are listed. An `AGENTS.md` that only mirrors
/// `CLAUDE.md` adds no rule a second time.
const FILES: [&str; 3] = ["CLAUDE.md", "AGENTS.md", "GEMINI.md"];

/// The heading Hub files chat rules under.
const FROM_HUB: &str = "Rules from Hub";

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct StandingRule {
    /// Stable for the same file and text, so the app can keep a choice about it.
    pub id: String,
    /// The rule in plain text: markdown marks and line breaks removed.
    pub text: String,
    /// `merge`, `test`, `ui`, `docs`, `code` or `other`: a theme to group by, from the words.
    pub theme: String,
    /// `repo` (the file's own text) or `hub` (said in a thread and saved by Hub).
    pub source: String,
    /// The file, relative to the project folder.
    pub file: String,
    /// First line of the rule in the file, from 1.
    #[ts(type = "number")]
    pub line: usize,
    /// Unix seconds of its latest change; None when git cannot say.
    #[ts(type = "number | null")]
    pub since: Option<i64>,
    /// The commit that last changed it, short; empty when not committed.
    pub commit: String,
    /// That commit's subject.
    pub why: String,
    /// A rule saved from chat: the thread it was said in and its title. Empty for the rest.
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub thread: String,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct Standing {
    /// Newest change first.
    pub rules: Vec<StandingRule>,
    /// The rules files found, relative to the project folder.
    pub files: Vec<String>,
}

pub fn read(root: &Path) -> Standing {
    let mut out = Standing::default();
    for name in FILES {
        let Ok(body) = std::fs::read_to_string(root.join(name)) else { continue };
        out.files.push(name.to_owned());
        let blame = git::blame(root, name).unwrap_or_default();
        for mut rule in parse(&body, name) {
            if out.rules.iter().any(|r| r.text == rule.text && r.file != name) {
                continue;
            }
            // A rule is as new as its newest line.
            let span = (rule.line - 1)..(rule.line - 1 + rule.id.parse::<usize>().unwrap_or(1));
            let newest = blame.get(span).into_iter().flatten().max_by_key(|b| b.time);
            if let Some(b) = newest {
                rule.since = Some(b.time);
                rule.commit = b.commit.clone();
                rule.why = b.summary.clone();
            }
            rule.id = format!("{name}:{}", rule.line);
            out.rules.push(rule);
        }
    }
    out.rules.sort_by(|a, b| b.since.cmp(&a.since).then(a.file.cmp(&b.file)).then(a.line.cmp(&b.line)));
    out
}

/// The rules in `body`: each top-level `- ` bullet with its continuation lines. While parsing,
/// `id` holds the number of lines the rule spans; [`read`] replaces it.
fn parse(body: &str, file: &str) -> Vec<StandingRule> {
    let mut rules: Vec<StandingRule> = Vec::new();
    let mut heading = String::new();
    let mut open = false;
    let mut fenced = false;
    for (i, raw) in body.lines().enumerate() {
        if raw.trim_start().starts_with("```") {
            fenced = !fenced;
            open = false;
            continue;
        }
        if fenced {
            continue;
        }
        if raw.starts_with('#') && raw.trim_start_matches('#').starts_with(' ') {
            heading = raw.trim_start_matches('#').trim().to_owned();
            open = false;
        } else if let Some(t) = raw.strip_prefix("- ").or_else(|| raw.strip_prefix("* ")) {
            rules.push(StandingRule {
                id: "1".into(),
                text: t.trim().to_owned(),
                source: if heading == FROM_HUB { "hub" } else { "repo" }.into(),
                file: file.to_owned(),
                line: i + 1,
                ..Default::default()
            });
            open = true;
        } else if raw.trim().is_empty() {
            open = false;
        } else if open && raw.starts_with(' ')
            && let Some(last) = rules.last_mut()
        {
            last.text.push(' ');
            last.text.push_str(raw.trim());
            last.id = (last.id.parse::<usize>().unwrap_or(1) + 1).to_string();
        } else {
            open = false;
        }
    }
    for r in &mut rules {
        r.text = plain(&r.text);
        r.theme = theme(&r.text).into();
    }
    rules.retain(|r| !r.text.is_empty());
    rules
}

/// Dates the rules saved from chat by when they were said, and names the thread. `said` is
/// `Store::rules_said`: `(thread id, title, unix seconds, text)`. The day it was said wins over
/// the commit's, and a rule not committed yet gets a date at all.
pub fn with_threads(st: &mut Standing, said: &[(String, String, i64, String)]) {
    for rule in st.rules.iter_mut().filter(|r| r.source == "hub") {
        if let Some((id, title, ts, _)) = said.iter().rev().find(|s| plain(&s.3) == rule.text) {
            rule.since = Some(*ts / 1000);
            rule.thread_id = id.clone();
            rule.thread = title.clone();
        }
    }
    st.rules.sort_by(|a, b| b.since.cmp(&a.since).then(a.file.cmp(&b.file)).then(a.line.cmp(&b.line)));
}

fn plain(text: &str) -> String {
    text.replace("**", "").replace('`', "").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A theme from the words, the first that matches: people look for "the merge rules", not for a
/// line in a file.
fn theme(text: &str) -> &'static str {
    let t = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if has(&["merge", "pull request", "label"]) {
        "merge"
    } else if has(&["cargo test", "npm test", "vitest", " tests", " test "]) {
        "test"
    } else if has(&["svelte", "design", "app/", " ui"]) {
        "ui"
    } else if has(&["docs/", "doc ", "docs ", "documentation"]) {
        "docs"
    } else if has(&["rust", "crates/", "source file", "engine", "unwrap"]) {
        "code"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "# Hub\n\n- Merging: open the PR,\n  then merge it itself.\n- Run `cargo test`.\n\n```\n- not a rule\n```\n\n## Rules from Hub\n- Never ask twice\n";

    #[test]
    fn splits_bullets_with_their_continuation() {
        let rules = parse(BODY, "CLAUDE.md");
        let texts: Vec<_> = rules.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["Merging: open the PR, then merge it itself.", "Run cargo test.", "Never ask twice"]);
        assert_eq!(rules[0].id, "2", "spans two lines");
        assert_eq!((rules[0].theme.as_str(), rules[1].theme.as_str()), ("merge", "test"));
        assert_eq!(rules[2].source, "hub");
        assert_eq!(rules[1].line, 5);
    }

    #[test]
    fn dates_a_rule_from_git_and_skips_a_mirror() {
        let dir = crate::janitor::test_dir();
        let git = |args: &[&str]| assert!(git::run(&dir, args).is_some(), "git {args:?}");
        git(&["init", "-q"]);
        std::fs::write(dir.join("CLAUDE.md"), BODY).unwrap();
        std::fs::write(dir.join("AGENTS.md"), BODY).unwrap();
        git(&["add", "."]);
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "say the rules"]);
        let st = read(&dir);
        assert_eq!(st.files, ["CLAUDE.md", "AGENTS.md"]);
        assert_eq!(st.rules.len(), 3, "the mirror adds nothing");
        assert!(st.rules.iter().all(|r| r.since.is_some() && r.why == "say the rules" && r.commit.len() == 8));
        assert_eq!(st.rules[0].id.split(':').next(), Some("CLAUDE.md"));
    }

    #[test]
    fn a_rule_said_in_chat_gets_its_day_and_thread() {
        let dir = crate::janitor::test_dir();
        std::fs::write(dir.join("CLAUDE.md"), BODY).unwrap();
        let mut st = read(&dir);
        assert!(st.rules.iter().all(|r| r.since.is_none()), "nothing committed");
        let said = vec![("t1".to_owned(), "Fix tabs".to_owned(), 1_700_000_000_000, "Never ask twice".to_owned())];
        with_threads(&mut st, &said);
        let r = &st.rules[0];
        assert_eq!((r.text.as_str(), r.since, r.thread.as_str(), r.thread_id.as_str()), ("Never ask twice", Some(1_700_000_000), "Fix tabs", "t1"));
        assert!(st.rules[1..].iter().all(|r| r.thread.is_empty()));
    }

    #[test]
    fn no_rules_file_is_an_empty_answer() {
        let dir = crate::janitor::test_dir();
        let st = read(&dir);
        assert!(st.rules.is_empty() && st.files.is_empty());
    }
}
