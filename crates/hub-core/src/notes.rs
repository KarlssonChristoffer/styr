//! Patch notes. When the updater has built a new version, a small model reads every commit
//! since the running one and writes what is new and what to try. They are kept in the database
//! per version, so the app can show them after it restarts. The model writes at most once an
//! hour (`spaced`): versions built closer together keep their commits only, and the next written
//! notes cover them too.
//!
//! The model is asked by the caller (`engine::ask_tidy`, handed to `record`), and the notes are
//! kept in the store it passes, so this file needs neither the engines nor the Hub.

use std::future::Future;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::model::{Change, ReleaseNotes, now_ms};
use crate::store::Store;

/// How long the model may take to write them.
pub const WAIT: Duration = Duration::from_secs(180);

/// How far apart the model writes notes, ms. Every version asked a whole agent session once
/// (314 in a week of many small merges), and each update waited for it.
pub const APART_MS: i64 = 3_600_000;

const INDEX: &str = "notes:index";
const KEEP: usize = 30;

const PROMPT: &str = "You write the patch notes for Hub, a desktop app for running coding agents, \
built and used by one person. He reads them on the Updates page before or after installing, to know \
what is new and to try it. Below are all commits since his last version, newest first.

Write markdown, nothing before or after it:
- `## New` and `## Changed and fixed`: one bullet per feature or fix, grouped (several commits \
about one feature are one bullet). Plain words about what he will see or can do, not file names. \
Leave out a section with nothing in it.
- At the end of a bullet, `{show: PLACE}` when the change is on one page he can be taken to. PLACE \
is one of: PLACES. Leave it out when no page fits (something in every thread, the message box or \
the sidebar).
- After that, `{test: step; step; step}`: 1 to 4 short steps to try it by hand and what should \
happen, when it can be tried. Leave it out for fixes he cannot provoke.
Keep it short. Skip pure refactors, tests and docs unless they change what he sees.";

/// The pages a note can take you to with `{show: …}`: the app's views and Settings sections, as
/// `UpdateCenter.svelte` opens them.
pub const PLACES: &[&str] = &[
    "home", "overview", "workers", "machines", "services", "routines", "recipes", "library", "plugins",
    "problems", "labs", "settings/account", "settings/appearance", "settings/notifications",
    "settings/dictation", "settings/chat", "settings/engines", "settings/safety", "settings/customs",
    "settings/projects", "settings/memory", "settings/extensions", "settings/apis", "settings/vault",
    "settings/machines", "settings/access", "settings/updates", "settings/whatsnew", "settings/privacy",
    "settings/about",
];

/// Commits in `from..to`, or the latest ones when `from` is unknown.
pub fn commits(src: &Path, from: Option<&str>, to: &str) -> Vec<(Change, String)> {
    let range = match from {
        Some(f) if git(src, &["cat-file", "-e", &format!("{f}^{{commit}}")]).is_some() => format!("{f}..{to}"),
        _ => to.to_owned(),
    };
    let Some(out) = git(src, &["log", "-n", "80", "--format=%h%x1f%ct%x1f%s%x1f%b%x1e", &range]) else {
        return Vec::new();
    };
    out.split('\u{1e}')
        .filter_map(|record| {
            let mut parts = record.trim().splitn(4, '\u{1f}');
            let hash = parts.next()?.to_owned();
            let ts = parts.next()?.parse::<i64>().ok()? * 1000;
            let subject = parts.next()?.to_owned();
            let body = parts.next().unwrap_or_default().trim().to_owned();
            Some((Change { hash, ts, subject }, body))
        })
        .collect()
}

/// The prompt for a range: subjects with their bodies, minus attribution lines.
pub fn prompt(commits: &[(Change, String)]) -> String {
    let mut text = PROMPT.replace("PLACES", &PLACES.join(", "));
    text.push_str("\n\n<commits>\n");
    for (c, body) in commits {
        text.push_str(&format!("- {}\n", c.subject));
        let body: Vec<&str> = body
            .lines()
            .filter(|l| !l.starts_with("Co-Authored-By") && !l.starts_with("Claude-Session") && !l.trim().is_empty())
            .collect();
        if !body.is_empty() {
            let body = body.join(" ");
            let short: String = body.chars().take(600).collect();
            text.push_str(&format!("  {short}\n"));
        }
    }
    text.push_str("</commits>");
    text
}

/// The last commit from before the past day, to start a range from when there is no earlier
/// version to compare with.
pub fn start_of_recent(src: &Path, head: &str) -> Option<String> {
    git(src, &["rev-list", "-n", "1", "--before=24.hours", head]).map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
}

/// Writes and stores the notes for `version`, built from commit `to`. A model that cannot be
/// asked leaves the text empty; the commits are stored all the same. `write` asks the model
/// (`engine::ask_tidy` with purpose `notes` and `WAIT`).
pub async fn record<W, F>(store: &Store, src: &Path, from: Option<&str>, to: &str, version: &str, write: W) -> Result<ReleaseNotes>
where
    W: FnOnce(String) -> F,
    F: Future<Output = Result<String>>,
{
    let (src2, from2, to2) = (src.to_owned(), from.map(str::to_owned), to.to_owned());
    let list = tokio::task::spawn_blocking(move || commits(&src2, from2.as_deref(), &to2)).await?;
    let text = if list.is_empty() {
        String::new()
    } else {
        match write(prompt(&list)).await {
            Ok(text) => text,
            Err(err) => {
                tracing::warn!("could not write patch notes: {err:#}");
                String::new()
            }
        }
    };
    let notes = ReleaseNotes {
        version: version.to_owned(),
        from: from.map(str::to_owned),
        text,
        commits: list.into_iter().map(|(c, _)| c).collect(),
        created_at: now_ms(),
    };
    save(store, &notes).await?;
    Ok(notes)
}

async fn save(store: &Store, notes: &ReleaseNotes) -> Result<()> {
    store.set_value(&format!("notes:{}", notes.version), &serde_json::to_string(notes)?).await?;
    let mut index: Vec<String> =
        store.get_value(INDEX).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    index.retain(|v| v != &notes.version);
    index.insert(0, notes.version.clone());
    index.truncate(KEEP);
    store.set_value(INDEX, &serde_json::to_string(&index)?).await
}

/// Stored notes, newest first.
pub async fn list(store: &Store) -> Result<Vec<ReleaseNotes>> {
    let index: Vec<String> =
        store.get_value(INDEX).await?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default();
    let mut out = Vec::new();
    for version in index {
        if let Some(text) = store.get_value(&format!("notes:{version}")).await?
            && let Ok(notes) = serde_json::from_str(&text)
        {
            out.push(notes);
        }
    }
    Ok(out)
}

/// Where the notes of a version built at `now` start, and whether the model writes them. It
/// writes when the last notes it wrote are `APART_MS` old, and then from the version those were
/// for, so what was built in between is told too. Else the range is `running..` and only the
/// commits are kept.
pub async fn spaced(store: &Store, running: Option<&str>, now: i64) -> (Option<String>, bool) {
    match last_written(store, None).await {
        // A clock set back does not keep the model unasked.
        Some(n) if (0..APART_MS).contains(&(now - n.created_at)) => (running.map(str::to_owned), false),
        Some(n) => (Some(n.version), true),
        None => (running.map(str::to_owned), true),
    }
}

/// The newest notes the model wrote, for a version other than `except`. Only for a version that
/// is a commit: notes written by hand on a dev build ("dev") start no range.
pub async fn last_written(store: &Store, except: Option<&str>) -> Option<ReleaseNotes> {
    let commit = |v: &str| v.len() >= 7 && v.chars().all(|c| c.is_ascii_hexdigit());
    list(store).await.unwrap_or_default().into_iter().filter(|n| !n.text.trim().is_empty() && commit(&n.version) && Some(n.version.as_str()) != except).max_by_key(|n| n.created_at)
}

/// The version stored before `version`, to start a range from.
pub async fn previous(store: &Store, version: &str) -> Option<String> {
    let index: Vec<String> = serde_json::from_str(&store.get_value(INDEX).await.ok()??).ok()?;
    index.into_iter().find(|v| v != version)
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    crate::git::run(dir, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn prompt_lists_commits_without_attribution() {
        let c = |s: &str| Change { hash: "abc1234".into(), ts: 0, subject: s.into() };
        let text = prompt(&[
            (c("Settings: tools"), "Detects git and gh.\n\nCo-Authored-By: someone\nClaude-Session: x".into()),
            (c("Fix the menu"), String::new()),
        ]);
        assert!(text.contains("- Settings: tools\n  Detects git and gh.\n"));
        assert!(text.contains("- Fix the menu\n"));
        assert!(!text.contains("Co-Authored-By"));
    }

    #[tokio::test]
    async fn the_model_writes_at_most_once_an_hour_and_then_from_where_it_last_wrote() {
        let store = Store::in_memory().unwrap();
        assert_eq!(spaced(&store, Some("a"), 0).await, (Some("a".into()), true), "nothing written yet");
        let notes = |version: &str, text: &str, at: i64| ReleaseNotes { version: version.into(), from: None, text: text.into(), commits: Vec::new(), created_at: at };
        save(&store, &notes("dev", "## By hand", 1_500)).await.unwrap();
        save(&store, &notes("b0b0b0b", "## New", 1_000)).await.unwrap();
        save(&store, &notes("c0c0c0c", "", 2_000)).await.unwrap();
        assert_eq!(spaced(&store, Some("c"), 1_000 + APART_MS - 1).await, (Some("c".into()), false), "commits only inside the hour");
        assert_eq!(spaced(&store, Some("c"), 1_000 + APART_MS).await, (Some("b0b0b0b".into()), true), "then from the last written version that is a commit");
        assert_eq!(spaced(&store, Some("c"), 0).await, (Some("b0b0b0b".into()), true), "a clock set back still writes");
        assert!(last_written(&store, Some("b0b0b0b")).await.is_none());
    }

    #[test]
    fn reads_a_range_from_git() {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let ok = Command::new("git").current_dir(dir.path()).args(args).output().unwrap().status.success();
            assert!(ok, "git {args:?}");
        };
        run(&["init", "-q"]);
        for (i, msg) in ["first", "second\n\nwith a body", "third"].iter().enumerate() {
            std::fs::write(dir.path().join("f"), i.to_string()).unwrap();
            run(&["add", "."]);
            run(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", msg]);
        }
        let first = git(dir.path(), &["rev-parse", "--short", "HEAD~2"]).unwrap();
        let list = commits(dir.path(), Some(first.trim()), "HEAD");
        let subjects: Vec<&str> = list.iter().map(|(c, _)| c.subject.as_str()).collect();
        assert_eq!(subjects, ["third", "second"]);
        assert_eq!(list[1].1, "with a body");
        // An unknown start gives the latest commits instead of nothing.
        assert_eq!(commits(dir.path(), Some("dev"), "HEAD").len(), 3);
    }
}
