//! The fun numbers: which models answered, how many tokens they used and what that would have
//! cost at API prices, per model and per project. Read from Claude Code's answers that Hub copies
//! for the activity map. Most work runs on a subscription, so the cost is what the same tokens
//! would cost through the API, not what was paid.

use std::collections::HashMap;
use std::path::Path;

use crate::activity::claude_code::{CcMark, CcSession};
use crate::config::Price;
use crate::model::{ModelStat, Project, ProjectStat, Stats, Thread, ThreadStat};
pub use crate::model::rows::{Handed, Outcome};

/// What one answer would have cost through the API at the engines' `prices`, or None for a
/// model without a price. The first price whose id the model's starts with counts.
pub fn cost(m: &CcMark, prices: &[Price]) -> Option<f64> {
    let model = m.model.as_deref()?;
    let p = prices.iter().find(|p| model.starts_with(p.model.as_str()))?;
    let dollars = m.input as f64 * p.input
        + m.cache_write as f64 * p.input * 1.25
        + m.cache_read as f64 * p.cache_read
        + m.output as f64 * p.output;
    Some(dollars / 1_000_000.0)
}

/// `claude-opus-4-8-20260101` reads "Opus 4.8"; ids that do not look like Claude's stay as they are.
pub fn model_name(id: &str) -> String {
    let Some(rest) = id.strip_prefix("claude-") else { return id.to_owned() };
    let mut words: Vec<&str> = rest.split('-').filter(|w| w.len() < 8 || !w.chars().all(|c| c.is_ascii_digit())).collect();
    // Older ids put the version first: claude-3-5-sonnet.
    let numeric = |w: &&str| w.chars().all(|c| c.is_ascii_digit());
    if let (Some(true), Some(i)) = (words.first().map(numeric), words.iter().position(|w| !numeric(w))) {
        let family = words.remove(i);
        words.insert(0, family);
    }
    let Some((family, version)) = words.split_first() else { return id.to_owned() };
    let mut name: String = family.chars().take(1).flat_map(char::to_uppercase).chain(family.chars().skip(1)).collect();
    if !version.is_empty() {
        name.push(' ');
        name.push_str(&version.join("."));
    }
    name
}

/// Per model and per project, biggest first. A session counts towards the outermost project
/// whose folder holds it, so work in `hub/app` is `hub`'s, or else towards its own folder.
pub fn stats(marks: &[CcMark], sessions: &[CcSession], projects: &[Project], days: u32, prices: &[Price]) -> Stats {
    let cwd: HashMap<&str, &str> = sessions.iter().filter_map(|s| Some((s.id.as_str(), s.cwd.as_deref()?))).collect();
    let mut models: HashMap<String, ModelStat> = HashMap::new();
    let mut by_project: HashMap<String, ProjectStat> = HashMap::new();
    let mut out = Stats { days, ..Default::default() };
    for m in marks.iter().filter(|m| m.prompt.is_none()) {
        let Some(id) = m.model.as_deref() else { continue };
        let price = cost(m, prices);
        out.answers += 1;
        out.tokens += m.tokens;
        out.cost += price.unwrap_or(0.0);
        let name = model_name(id);
        let s = models.entry(name.clone()).or_insert_with(|| ModelStat { model: name, priced: true, ..Default::default() });
        s.answers += 1;
        s.tokens += m.tokens;
        s.cost += price.unwrap_or(0.0);
        s.priced &= price.is_some();
        let (name, path) = project_of(cwd.get(m.session_id.as_str()).copied(), projects);
        let p = by_project.entry(name.clone()).or_insert_with(|| ProjectStat { name, path, ..Default::default() });
        p.tokens += m.tokens;
        p.cost += price.unwrap_or(0.0);
    }
    out.models = models.into_values().collect();
    out.models.sort_by_key(|m| std::cmp::Reverse(m.tokens));
    out.projects = by_project.into_values().collect();
    out.projects.sort_by(|a, b| b.cost.total_cmp(&a.cost).then(b.tokens.cmp(&a.tokens)));
    out
}



/// What Hub threads cost and what they showed for it. A result is a turn that changed files,
/// settled once its last proof verdict is anything but "asked"; it costs every answer in the
/// thread since the result before it settled, so the turns that asked for and gave the proof
/// count, and so do the failures. `sessions` are each thread's engine sessions over its life.
/// Work handed to a tool (Knegarn) is a result of the thread too, at the cost and with the
/// verdict the tool reported, under the model it names.
pub fn results(out: &mut Stats, marks: &[CcMark], outcomes: &[Outcome], handed: &[Handed], threads: &[Thread], sessions: &[(String, String)], prices: &[Price]) {
    let owner: HashMap<&str, &str> = sessions
        .iter()
        .map(|(t, s)| (s.as_str(), t.as_str()))
        .chain(threads.iter().filter_map(|t| Some((t.engine_session_id.as_deref()?, t.id.as_str()))))
        .collect();
    let mut answers: HashMap<&str, Vec<&CcMark>> = HashMap::new();
    for m in marks.iter().filter(|m| m.prompt.is_none() && m.model.is_some()) {
        if let Some(t) = owner.get(m.session_id.as_str()) {
            answers.entry(t).or_default().push(m);
        }
    }
    let mut by_model: HashMap<String, (u32, u32, f64)> = HashMap::new();
    let mut stats: Vec<ThreadStat> = Vec::new();
    for (thread, mut said) in answers {
        said.sort_by_key(|m| m.ts);
        let mut row = ThreadStat {
            id: thread.to_owned(),
            title: threads.iter().find(|t| t.id == thread).map(|t| t.title.clone()).unwrap_or_else(|| "Deleted thread".into()),
            ..Default::default()
        };
        for m in &said {
            row.tokens += m.tokens;
            row.cost += cost(m, prices).unwrap_or(0.0);
        }
        // Each changed turn's last verdict, and when it came, in the order the turns ended.
        let mut verdicts: Vec<(i64, i64, &str)> = Vec::new();
        for o in outcomes.iter().filter(|o| o.thread_id == thread) {
            let (Some(reply), Some(state)) = (o.reply, o.state.as_deref()) else { continue };
            match verdicts.iter_mut().find(|v| v.0 == reply) {
                Some(v) => (v.1, v.2) = (o.ts, state),
                None => verdicts.push((reply, o.ts, state)),
            }
        }
        verdicts.sort_by_key(|v| v.0);
        let mut since = i64::MIN;
        for (_, settled, state) in verdicts.into_iter().filter(|v| v.2 != "asked") {
            // Per model: dollars, and tokens to choose between models without a price.
            let mut spent: HashMap<&str, (f64, i64)> = HashMap::new();
            for m in said.iter().filter(|m| m.ts > since && m.ts <= settled) {
                let s = spent.entry(m.model.as_deref().unwrap_or_default()).or_default();
                s.0 += cost(m, prices).unwrap_or(0.0);
                s.1 += m.tokens;
            }
            since = settled;
            let proven = state == "shown";
            row.results += 1;
            row.proven += proven as u32;
            let Some((model, _)) = spent.iter().max_by(|a, b| a.1 .0.total_cmp(&b.1 .0).then(a.1 .1.cmp(&b.1 .1))) else { continue };
            let total: f64 = spent.values().map(|s| s.0).sum();
            let entry = by_model.entry(model_name(model)).or_default();
            entry.0 += 1;
            entry.1 += proven as u32;
            entry.2 += total;
        }
        stats.push(row);
    }
    let title = |id: &str| threads.iter().find(|t| t.id == id).map(|t| t.title.clone()).unwrap_or_else(|| "Deleted thread".into());
    for h in handed {
        let row = match stats.iter().position(|r| r.id == h.thread_id) {
            Some(i) => &mut stats[i],
            None => {
                stats.push(ThreadStat { id: h.thread_id.clone(), title: title(&h.thread_id), ..Default::default() });
                stats.last_mut().expect("just pushed")
            }
        };
        row.results += 1;
        row.proven += h.proven as u32;
        row.cost += h.cost.unwrap_or(0.0);
        let Some(model) = h.model.as_deref() else { continue };
        let name = model_name(model);
        if !out.models.iter().any(|m| m.model == name) {
            out.models.push(ModelStat { model: name.clone(), priced: h.cost.is_some(), ..Default::default() });
        }
        let entry = by_model.entry(name).or_default();
        entry.0 += 1;
        entry.1 += h.proven as u32;
        entry.2 += h.cost.unwrap_or(0.0);
    }
    for m in out.models.iter_mut() {
        if let Some((results, proven, spent)) = by_model.get(&m.model) {
            (m.results, m.proven, m.result_cost) = (*results, *proven, *spent);
        }
    }
    stats.sort_by(|a, b| b.cost.total_cmp(&a.cost).then(b.tokens.cmp(&a.tokens)));
    stats.truncate(12);
    out.threads = stats;
}

/// Only the answers given in sessions inside `root`.
pub fn within(marks: Vec<CcMark>, sessions: &[CcSession], root: &str) -> Vec<CcMark> {
    let cwd: HashMap<&str, &str> = sessions.iter().filter_map(|s| Some((s.id.as_str(), s.cwd.as_deref()?))).collect();
    marks.into_iter().filter(|m| cwd.get(m.session_id.as_str()).is_some_and(|c| inside(c, root))).collect()
}

/// `path` is `root` or a folder in it, on Windows or anywhere else.
pub(crate) fn inside(path: &str, root: &str) -> bool {
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let (at, root) = (norm(path), norm(root));
    at == root || at.starts_with(&format!("{root}/"))
}

fn project_of(cwd: Option<&str>, projects: &[Project]) -> (String, Option<String>) {
    let Some(cwd) = cwd else { return ("Unknown folder".into(), None) };
    // The Claude app makes a worktree of a repository in `<repo>/.claude/worktrees/<name>`: its
    // work is the repository's.
    let flat = cwd.replace('\\', "/");
    if let Some((repo, _)) = flat.split_once("/.claude/worktrees/").filter(|(repo, _)| !repo.is_empty()) {
        return project_of(Some(repo), projects);
    }
    // And a chat without a project gets a throwaway folder of its own in `scratch-workspaces/`.
    if let Some((root, _)) = flat.split_once("/scratch-workspaces/") {
        return ("Claude app chats".into(), Some(format!("{root}/scratch-workspaces")));
    }
    // A thread's own worktree counts as its repository, a quick thread's folder as `scratch`.
    let root = crate::thread_display::project_root(cwd);
    let cwd = root.as_str();
    if let Some(p) = projects.iter().filter(|p| inside(cwd, &p.path)).min_by_key(|p| p.path.len()) {
        return (p.name.clone(), Some(p.path.clone()));
    }
    let path = Path::new(&cwd.replace('\\', "/")).to_path_buf();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    (name.unwrap_or_else(|| cwd.to_owned()), Some(cwd.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude() -> Vec<Price> {
        crate::config::EngineTraits::preset("claude").prices.unwrap()
    }

    fn answer(session: &str, model: &str, input: i64, output: i64) -> CcMark {
        CcMark {
            key: format!("{session}{model}{input}"),
            session_id: session.into(),
            tokens: input + output,
            model: Some(model.into()),
            input,
            output,
            ..Default::default()
        }
    }

    #[test]
    fn names_read_like_the_product() {
        assert_eq!(model_name("claude-opus-4-8"), "Opus 4.8");
        assert_eq!(model_name("claude-sonnet-4-5-20250929"), "Sonnet 4.5");
        assert_eq!(model_name("claude-3-5-haiku-20241022"), "Haiku 3.5");
        assert_eq!(model_name("claude-opus-5-5"), "Opus 5.5");
        assert_eq!(model_name("gpt-5"), "gpt-5");
    }

    #[test]
    fn the_more_specific_price_wins() {
        let m = answer("s", "claude-opus-5-5", 1_000_000, 0);
        assert_eq!(cost(&m, &claude()), Some(4.0));
        let m = answer("s", "claude-opus-5", 1_000_000, 0);
        assert_eq!(cost(&m, &claude()), Some(5.0));
        let cached = CcMark { cache_write: 1_000_000, cache_read: 1_000_000, ..answer("s", "claude-sonnet-5-5", 0, 0) };
        assert_eq!(cost(&cached, &claude()), Some(2.0 * 1.25 + 0.2));
        assert_eq!(cost(&answer("s", "some-local-model", 5, 5), &claude()), None);
    }

    #[test]
    fn answers_count_per_model_and_per_project() {
        let sessions = [
            CcSession { id: "a".into(), cwd: Some("D:\\Dev\\hub\\app".into()), title: None },
            CcSession { id: "b".into(), cwd: Some("/home/me/scratch".into()), title: None },
        ];
        let projects = [
            Project { path: "D:/Dev/hub/app".into(), name: "app".into(), created_at: 0 },
            Project { path: "D:/Dev/hub".into(), name: "hub".into(), created_at: 0 },
        ];
        let marks = [
            answer("a", "claude-opus-4-8", 1_000_000, 0),
            answer("a", "claude-haiku-4-5", 0, 1_000_000),
            answer("b", "claude-opus-4-8-20260101", 0, 10),
            answer("b", "local", 3, 0),
            CcMark { prompt: Some("hi".into()), session_id: "a".into(), ..Default::default() },
        ];
        let s = stats(&marks, &sessions, &projects, 30, &claude());
        assert_eq!((s.answers, s.tokens), (4, 2_000_013));
        assert_eq!(s.models[0].model, "Opus 4.8");
        assert_eq!(s.models[0].answers, 2);
        assert!(s.models[0].priced);
        assert!(!s.models.iter().find(|m| m.model == "local").unwrap().priced);
        assert_eq!((s.projects[0].name.as_str(), s.projects[0].cost), ("hub", 10.0));
        assert_eq!(s.projects[1].name, "scratch");
        let hub_only = within(marks.to_vec(), &sessions, "d:/dev/HUB/");
        assert_eq!(hub_only.len(), 3);
    }

    #[test]
    fn claude_app_worktrees_and_scratch_chats_are_not_projects_of_their_own() {
        let projects = [Project { path: "D:/Dev/hub".into(), name: "Hub (dev)".into(), created_at: 0 }];
        let of = |cwd: &str| project_of(Some(cwd), &projects).0;
        assert_eq!(of("D:\\Game\\.claude\\worktrees\\vigorous-tu-7a99d0"), "Game");
        assert_eq!(of("F:\\Dev\\REA\\.claude\\worktrees\\zealous-easley-eaefc3\\src"), "REA");
        assert_eq!(of("D:\\Dev\\hub\\.claude\\worktrees\\x-y-123456"), "Hub (dev)");
        let scratch = "C:\\Users\\me\\AppData\\Roaming\\Claude\\scratch-workspaces\\a\\b\\scratch-2026-10-21-c3acb4";
        assert_eq!(of(scratch), "Claude app chats");
        assert_eq!(of("/home/me/Claude/scratch-workspaces/a/b/scratch-2026-10-21-c3acb4"), "Claude app chats");
        assert_eq!(of("D:\\Game"), "Game");
    }

    #[test]
    fn a_proven_change_costs_everything_since_the_last_one_settled() {
        let at = |session: &str, model: &str, input: i64, ts: i64| CcMark { ts, ..answer(session, model, input, 0) };
        let thread = Thread {
            id: "t".into(),
            title: "Fix it".into(),
            cwd: "/tmp".into(),
            engine: "claude".into(),
            status: crate::model::ThreadStatus::Idle,
            engine_session_id: Some("new".into()),
            title_locked: false,
            archived: false,
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
            host: None, project: None, display: None,
            created_at: 1,
            updated_at: 1,
        };
        // The thread restarted once: "old" was its first session.
        let sessions = [("t".to_owned(), "old".to_owned())];
        let marks = [
            at("old", "claude-opus-5-5", 1_000_000, 10), // $4, the first change
            at("new", "claude-opus-5-5", 1_000_000, 30), // $4, the proof it was asked for
            at("new", "claude-sonnet-5", 1_000_000, 50), // $2, the second change, which failed
            at("new", "claude-opus-5-5", 1_000_000, 70), // $4, talk after it: the thread's only
            at("elsewhere", "claude-opus-5-5", 1_000_000, 20),
        ];
        let o = |seq, ts, reply: Option<i64>, state: Option<&str>| Outcome { thread_id: "t".into(), seq, ts, reply, state: state.map(Into::into) };
        let outcomes = [
            o(1, 11, None, None),
            o(2, 12, Some(1), Some("asked")),
            o(3, 31, None, None),
            o(4, 32, Some(1), Some("shown")),
            o(5, 51, None, None),
            o(6, 52, Some(5), Some("failed")),
            o(7, 71, None, None),
        ];
        let mut s = stats(&marks, &[], &[], 30, &claude());
        let handed = [Handed { thread_id: "t".into(), model: Some("claude-sonnet-5".into()), cost: Some(3.0), proven: true }];
        results(&mut s, &marks, &outcomes, &handed, &[thread], &sessions, &claude());
        let opus = s.models.iter().find(|m| m.model == "Opus 5.5").unwrap();
        assert_eq!((opus.results, opus.proven, opus.result_cost), (1, 1, 8.0));
        let sonnet = s.models.iter().find(|m| m.model == "Sonnet 5").unwrap();
        assert_eq!((sonnet.results, sonnet.proven, sonnet.result_cost), (2, 1, 5.0));
        assert_eq!(s.threads.len(), 1);
        let t = &s.threads[0];
        assert_eq!((t.title.as_str(), t.cost, t.results, t.proven), ("Fix it", 17.0, 3, 2));
    }
}
