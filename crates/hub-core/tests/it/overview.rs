//! Overview: a project's devlog reads its commits and threads per day, and "While you were away"
//! counts the commits, what Hub did on its own, and nothing from before you left.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use hub_core::model::{EventKind, Thread, ThreadStatus, now_ms};
use hub_core::{Config, Hub, Store};
use serde_json::json;

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=Claude", "-c", "user.email=noreply@anthropic.com"])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

async fn thread(hub: &Hub, cwd: &str, title: &str) -> Thread {
    let now = now_ms();
    hub.store
        .insert_thread(Thread {
            id: uuid::Uuid::now_v7().to_string(),
            title: title.into(),
            cwd: cwd.into(),
            engine: "claude".into(),
            status: ThreadStatus::Idle,
            engine_session_id: None,
            title_locked: true,
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
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn a_projects_days_and_what_happened_while_away() {
    let hub = hub();
    // Not the real projects root (D:\Dev), whose repositories would all be read.
    let empty = tempfile::tempdir().unwrap();
    hub.set_projects_root(&empty.path().display().to_string()).await.unwrap();
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path().display().to_string();
    git(repo.path(), &["init", "-q"]);
    for subject in ["First step", "Second step"] {
        std::fs::write(repo.path().join("a.txt"), subject).unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-qm", subject]);
    }
    let elsewhere = thread(&hub, "/somewhere/else", "Not this project").await;
    let here = thread(&hub, &format!("{root}/app"), "Build the page").await;
    hub.store.append_event(&elsewhere.id, EventKind::UserMessage, json!({ "text": "hi" })).await.unwrap();
    hub.store.append_event(&here.id, EventKind::UserMessage, json!({ "text": "go" })).await.unwrap();

    let days = hub_core::devlog::project_days(&hub.ctx(), &root, 14, 0).await.unwrap();
    assert_eq!(days.len(), 1, "{days:?}");
    assert_eq!(days[0].commits, 2);
    assert_eq!(days[0].subjects, ["Second step", "First step"]);
    assert_eq!(days[0].threads.len(), 1);
    assert_eq!(days[0].threads[0].title, "Build the page");

    // What Hub did by itself counts; unfinished reviews do not.
    let since = now_ms() - 1;
    hub.store.append_event(&here.id, EventKind::PermissionAuto, json!({ "allowed": true })).await.unwrap();
    hub.store.append_event(&here.id, EventKind::Review, json!({ "state": "running" })).await.unwrap();
    hub.store.append_event(&here.id, EventKind::Review, json!({ "state": "done" })).await.unwrap();
    hub.store.append_event(&here.id, EventKind::QuotaWait, json!({ "state": "resumed" })).await.unwrap();
    hub.store.insert_project(hub_core::model::Project { name: "repo".into(), path: root.clone(), created_at: 0 }).await.unwrap();
    let away = hub.away(since - 3_600_000, 0).await.unwrap();
    assert_eq!((away.permissions, away.reviews, away.resumed, away.side_jobs), (1, 1, 1, 0));
    // Found by its commits: git names the folder its own way (forward slashes on Windows, no
    // `\\?\` prefix), and Hub's projects root may hold other repositories with commits too.
    let mine = away.repos.iter().find(|r| r.subjects.iter().any(|s| s == "First step")).expect("the repo");
    assert_eq!(mine.commits, 2);
    assert_eq!(mine.subjects[0], "Second step");

    let later = hub.away(now_ms() + 60_000, 0).await.unwrap();
    assert_eq!((later.permissions, later.reviews), (0, 0));
    assert!(later.repos.is_empty());
}

#[tokio::test]
async fn a_written_day_opens_without_working_anything_out() {
    let hub = hub();
    assert!(hub_core::devlog::day(&hub.ctx(), "2026-10-30").await.unwrap().is_none());
    let entry = "Hub 0.9 went out.\n\n### Shipped\n- Routines no longer wait (#175)";
    hub.store.set_value(&format!("{}2026-10-30", hub_core::devlog::SUMMARY), entry).await.unwrap();
    let day = hub_core::devlog::day(&hub.ctx(), "2026-10-30").await.unwrap().unwrap();
    assert_eq!(day.summary, entry);
    assert_eq!(hub_core::devlog::days(&hub.ctx(), 10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn the_usage_log_keeps_names_only_and_can_be_switched_off_and_cleared() {
    use hub_core::model::UseHit;
    let hub = hub();
    let hit = |f: &str, prev: &str| UseHit { ts: now_ms(), feature: f.into(), area: "thread".into(), prev: prev.into(), took_ms: Some(40) };
    // A sentence or a path is not a name: it is dropped, the rest are kept.
    let kept = hub_core::uselog::record(&hub, vec![hit("thread.composer", ""), hit("thread.composer", ""), hit("Fix my login bug", ""), hit("thread.model", "thread.composer"), hit("a/b", "")]).await.unwrap();
    assert_eq!(kept, 3);
    let r = hub_core::uselog::report(&hub, 30).await.unwrap();
    assert_eq!(r.features[0].feature, "thread.composer");
    assert_eq!(r.features[0].count, 2);
    assert_eq!((r.flows[0].from.as_str(), r.flows[0].to.as_str()), ("thread.composer", "thread.model"));
    // Off: nothing more is kept. Clear: nothing is left.
    hub_core::uselog::set_off(&hub, true).await.unwrap();
    assert_eq!(hub_core::uselog::record(&hub, vec![hit("home.inbox", "")]).await.unwrap(), 0);
    assert!(hub_core::uselog::report(&hub, 30).await.unwrap().off);
    assert_eq!(hub_core::uselog::clear(&hub).await.unwrap(), 3);
    assert_eq!(hub_core::uselog::report(&hub, 30).await.unwrap().total, 0);
}
