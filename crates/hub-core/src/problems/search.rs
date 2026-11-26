//! The search box on the Problems page, and Vakten's way to ask for a list: words plus terms in
//! the style of Sentry's, `is:new level:error project:Hub times_seen:>10 lastSeen:-24h "could not"`.
//! A term with `-` in front is left out (`-source:app`). Unknown terms are read as words, so a
//! typo finds nothing instead of everything.

use super::{Lane, Level, Prio, Problem, ProblemStatus};
use crate::model::now_ms;

#[derive(Debug, Clone, Default)]
pub struct Query {
    /// Words that must all be in the title, where it happened, the project or the category.
    words: Vec<(bool, String)>,
    terms: Vec<Term>,
    /// Only problems with a hit from this release: read by the database, not by [`Query::matches`].
    pub release: Option<String>,
}

#[derive(Debug, Clone)]
struct Term {
    not: bool,
    key: String,
    op: Op,
    value: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Is,
    More,
    Less,
    AtLeast,
    AtMost,
}

const KEYS: &[&str] = &[
    "is", "level", "project", "source", "machine", "release", "prio", "lane", "category", "stamp", "thread", "times_seen",
    "firstseen", "lastseen", "fixable", "confidence", "by", "id", "has",
];

impl Query {
    pub fn parse(text: &str) -> Query {
        let mut q = Query::default();
        for raw in split(text) {
            let (not, body) = match raw.strip_prefix('-') {
                Some(rest) if !rest.is_empty() => (true, rest.to_owned()),
                _ => (false, raw.clone()),
            };
            let Some((key, value)) = body.split_once(':') else {
                q.words.push((not, body.to_lowercase()));
                continue;
            };
            let key = key.to_ascii_lowercase().replace('_', "");
            let key = if key == "timesseen" { "times_seen".to_owned() } else { key };
            if !KEYS.contains(&key.as_str()) || value.is_empty() {
                q.words.push((not, body.to_lowercase()));
                continue;
            }
            let (op, value) = if let Some(v) = value.strip_prefix(">=") {
                (Op::AtLeast, v)
            } else if let Some(v) = value.strip_prefix("<=") {
                (Op::AtMost, v)
            } else if let Some(v) = value.strip_prefix('>') {
                (Op::More, v)
            } else if let Some(v) = value.strip_prefix('<') {
                (Op::Less, v)
            } else {
                (Op::Is, value)
            };
            let value = value.trim_matches('"').to_owned();
            if key == "release" && !not && op == Op::Is {
                q.release = Some(value);
                continue;
            }
            q.terms.push(Term { not, key, op, value });
        }
        q
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.terms.is_empty() && self.release.is_none()
    }

    pub fn matches(&self, p: &Problem) -> bool {
        let hay = format!(
            "{} {} {} {} {} {}",
            p.title,
            p.culprit.as_deref().unwrap_or_default(),
            p.project,
            p.category.as_deref().unwrap_or_default(),
            p.machine,
            p.id
        )
        .to_lowercase();
        self.words.iter().all(|(not, w)| hay.contains(w.as_str()) != *not) && self.terms.iter().all(|t| t.holds(p) != t.not)
    }
}

impl Term {
    fn holds(&self, p: &Problem) -> bool {
        let v = self.value.to_lowercase();
        let same = |s: &str| v.split(',').any(|v| s.eq_ignore_ascii_case(v.trim()));
        match self.key.as_str() {
            "is" => v.split(',').any(|v| match v.trim() {
                "new" => p.new,
                "open" | "unresolved" => p.status == ProblemStatus::Open,
                "resolved" => p.status == ProblemStatus::Resolved,
                "archived" | "ignored" | "muted" => p.status == ProblemStatus::Archived,
                "regressed" | "back" => p.regressed,
                "escalating" => p.escalating,
                "unstamped" | "untriaged" => p.prio.is_none() && p.category.is_none(),
                "stamped" | "triaged" => p.prio.is_some() || p.category.is_some(),
                "needs_you" | "needsyou" | "mine" => p.needs_you,
                "merged" => p.merged_into.is_some(),
                "assigned" | "working" => p.thread.is_some(),
                "unassigned" => p.thread.is_none(),
                _ => false,
            }),
            "level" => {
                let want = Level::parse(&v);
                match self.op {
                    Op::Is => p.level == want,
                    Op::More => p.level > want,
                    Op::AtLeast => p.level >= want,
                    Op::Less => p.level < want,
                    Op::AtMost => p.level <= want,
                }
            }
            "project" => same(&p.project),
            "source" => same(&p.source),
            "machine" => same(&p.machine),
            // Only "not this release" gets here.
            "release" => false,
            "prio" => {
                let Some(prio) = p.prio else { return v == "none" };
                let want = match v.trim_start_matches('p') {
                    "0" => Prio::P0,
                    "1" => Prio::P1,
                    "2" => Prio::P2,
                    _ => Prio::P3,
                };
                // P0 is the highest, so "prio:<=p1" means P0 and P1.
                match self.op {
                    Op::Is => v.split(',').any(|v| format!("{prio:?}").eq_ignore_ascii_case(v.trim())),
                    Op::AtMost => prio <= want,
                    Op::Less => prio < want,
                    Op::AtLeast => prio >= want,
                    Op::More => prio > want,
                }
            }
            "lane" => match p.lane {
                Some(lane) => v.split(',').any(|v| {
                    matches!((lane, v.trim()), (Lane::Quiet, "quiet") | (Lane::Tell, "tell" | "do_and_tell") | (Lane::Ask, "ask" | "ask_first"))
                }),
                None => v == "none",
            },
            "category" | "stamp" => {
                let stamps = [
                    p.category.clone(),
                    p.prio.map(|p| format!("{p:?}")),
                    p.lane.map(|l| format!("{l:?}")),
                    p.fixable.as_ref().map(|f| format!("fixable-{f}")),
                ];
                v.split(',').any(|v| stamps.iter().flatten().any(|s| s.eq_ignore_ascii_case(v.trim())))
                    || (v == "none" && p.category.is_none())
            }
            "fixable" => p.fixable.as_deref().is_some_and(same),
            "confidence" => p.confidence.as_deref().is_some_and(same),
            "by" => p.stamped_by.as_deref().is_some_and(same) || p.resolved_by.as_deref().is_some_and(same),
            "thread" | "has" if v == "thread" || v == "yes" || v == "true" => p.thread.is_some(),
            "thread" if v == "no" || v == "none" || v == "false" => p.thread.is_none(),
            "has" if v == "pr" => p.pr.is_some(),
            "has" if v == "note" => p.note.is_some(),
            "thread" => p.thread.as_deref().is_some_and(|t| t.starts_with(&self.value)),
            "id" => self.value.split(',').any(|v| v.trim().trim_start_matches('#').parse::<i64>().ok() == Some(p.id)),
            "times_seen" => compare(self.op, p.count as f64, v.parse::<f64>().unwrap_or(0.0)),
            "firstseen" | "lastseen" => {
                let at = if self.key == "firstseen" { p.first_seen } else { p.last_seen };
                // `-24h` means within the last 24 hours, `+7d` longer ago than that.
                let (older, span) = match v.strip_prefix('+') {
                    Some(rest) => (true, rest),
                    None => (false, v.trim_start_matches('-')),
                };
                let Some(ms) = span_ms(span) else { return false };
                let since = now_ms() - ms;
                match self.op {
                    Op::Is => if older { at < since } else { at >= since },
                    // `lastSeen:>-1h` reads as "after an hour ago".
                    Op::More | Op::AtLeast => at >= since,
                    Op::Less | Op::AtMost => at < since,
                }
            }
            _ => false,
        }
    }
}

fn compare(op: Op, have: f64, want: f64) -> bool {
    match op {
        Op::Is => (have - want).abs() < f64::EPSILON,
        Op::More => have > want,
        Op::Less => have < want,
        Op::AtLeast => have >= want,
        Op::AtMost => have <= want,
    }
}

/// `30m`, `24h`, `7d`, `2w` in milliseconds.
fn span_ms(s: &str) -> Option<i64> {
    let (n, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit())?);
    let n: i64 = n.parse().ok()?;
    let unit = match unit {
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        "w" => 7 * 86_400_000,
        _ => return None,
    };
    Some(n * unit)
}

/// Words, with "quoted words" and key:"quoted value" kept whole.
fn split(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn problem() -> Problem {
        serde_json::from_value(serde_json::json!({
            "id": 7, "project": "Hub", "source": "hubd", "level": "error", "title": "could not reach tincan over ssh",
            "culprit": "crates/hub-core/src/remote.rs in connect, line 40", "folder": null, "machine": "PC",
            "first_seen": now_ms() - 3 * 86_400_000, "last_seen": now_ms() - 3_600_000, "count": 14, "status": "open",
            "regressed": false, "thread": null, "days": [], "prio": "P1", "lane": "ask", "category": "environment",
            "confidence": "high", "fixable": "no", "note": null, "stamped_by": "Vakten", "archive": null, "resolved_at": null,
            "resolved_by": null, "next_release": false, "pr": null, "merged_into": null, "escalating": false, "new": true,
            "needs_you": true
        }))
        .unwrap()
    }

    #[test]
    fn terms_and_words() {
        let p = problem();
        let yes = |q: &str| Query::parse(q).matches(&p);
        assert!(yes("is:new level:error project:hub"));
        assert!(yes("\"over ssh\" times_seen:>10 lastSeen:-24h firstSeen:+2d"));
        assert!(yes("prio:<=p1 lane:ask stamp:environment -source:app thread:no"));
        assert!(!yes("is:resolved"));
        assert!(!yes("times_seen:<10"));
        assert!(!yes("-project:Hub"));
        assert!(!yes("lastSeen:-30m"));
        assert!(!yes("nosuch:thing"));
        let q = Query::parse("release:0.52 is:new");
        assert_eq!(q.release.as_deref(), Some("0.52"));
    }

    /// A fix thread runs on a small model unless the problem is big, never the largest, on Auto.
    #[test]
    fn fix_threads_pick_a_fitting_model() {
        let mut p = problem();
        let model = |p: &Problem, size: Option<&str>| super::super::fix_unattended(p, "you", size).prefer["model"][0].clone();
        assert_eq!(model(&p, None), "sonnet", "P1 and ask-first is big");
        assert_eq!(model(&p, Some("small")), "haiku", "Vakten's judgement wins");
        (p.prio, p.lane) = (Some(Prio::P3), Some(Lane::Tell));
        assert_eq!(model(&p, None), "haiku");
        assert_eq!(model(&p, Some("big")), "sonnet");
        let all = super::super::fix_unattended(&p, "Vakten", None);
        assert_eq!(all.by, "Vakten");
        assert_eq!(all.prefer["mode"], vec!["auto".to_string()]);
        assert!(all.prefer.values().flatten().all(|w| !w.contains("opus")));
    }

    /// A fix thread's card says where it stands: the pull request first, then the thread.
    #[test]
    fn a_fix_card_says_where_it_stands() {
        let mut p = problem();
        let mut t: crate::model::Thread = serde_json::from_value(serde_json::json!({
            "id": "t", "title": "Fix #7", "cwd": "/tmp", "engine": "claude", "status": "running",
            "engine_session_id": null, "title_locked": true, "archived": false, "created_at": 0, "updated_at": 0
        }))
        .unwrap();
        let state = |p: &Problem, t: &crate::model::Thread| super::super::fix_state(p, t);
        assert_eq!(state(&p, &t).0, "working");
        t.status = crate::model::ThreadStatus::Idle;
        assert_eq!(state(&p, &t).0, "ended");
        t.reading = Some("stuck".into());
        assert_eq!(state(&p, &t).0, "stuck");
        p.pr = Some(super::super::ProblemPr { number: 329, url: String::new(), state: "open".into() });
        let (s, says) = state(&p, &t);
        assert_eq!(s, "review");
        assert!(says.contains("#329") && says.contains("merges it"), "{says}");
        p.pr.as_mut().unwrap().state = "merged".into();
        assert_eq!(state(&p, &t).0, "merged");
    }
}
