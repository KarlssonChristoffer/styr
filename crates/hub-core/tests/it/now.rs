//! `GET /api/phone/now`: what works and what waits on you, small enough for a widget.

use std::sync::Arc;

use hub_core::model::{EventKind, Thread, ThreadStatus, now_ms};
use hub_core::{Config, Hub, Store};
use serde_json::{Value, json};

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}

async fn thread(hub: &Hub, title: &str, status: ThreadStatus, patch: impl FnOnce(&mut Thread)) -> Thread {
    let now = now_ms();
    let mut t = Thread {
        id: uuid::Uuid::now_v7().to_string(),
        title: title.into(),
        cwd: "/home/a/hub/.hub-threads/fix-1".into(),
        engine: "claude".into(),
        status,
        engine_session_id: None,
        title_locked: true,
        archived: false,
        returned_at: None,
        seen_at: None,
        settled_at: None,
        sent_at: Some(now - 5_000),
        pinned_at: None,
        snoozed_until: None,
        auto_settle: true,
        parent: None,
        reading: None,
        owner: None,
        cloud: None,
        host: None, project: None, display: None,
        created_at: now - 10_000,
        updated_at: now,
    };
    patch(&mut t);
    hub.store.insert_thread(t).await.unwrap()
}

#[tokio::test]
async fn the_snapshot_lists_what_works_and_what_waits_with_its_question() {
    let hub = hub();
    let working = thread(&hub, "Build the widgets", ThreadStatus::Running, |_| {}).await;
    let asking = thread(&hub, "Push the branch", ThreadStatus::WaitingOnYou, |_| {}).await;
    let _idle = thread(&hub, "Done already", ThreadStatus::Idle, |_| {}).await;
    let _archived = thread(&hub, "Gone", ThreadStatus::Running, |t| t.archived = true).await;
    let _settled = thread(&hub, "Put away", ThreadStatus::Running, |t| t.settled_at = Some(1)).await;
    let _side_job = thread(&hub, "A side job", ThreadStatus::Running, |t| t.parent = Some("other".into())).await;
    hub.store
        .append_event(
            &asking.id,
            EventKind::PermissionRequest,
            json!({
                "id": "req-1",
                "tool_call": { "title": "Run `git push`", "rawInput": { "command": "git push origin main" } },
                "options": [
                    { "optionId": "allow", "name": "Allow", "kind": "allow_once" },
                    { "optionId": "deny", "name": "Deny", "kind": "reject_once" }
                ]
            }),
        )
        .await
        .unwrap();

    let now = hub_core::now::read(&hub).await.unwrap();
    let json: Value = serde_json::to_value(&now).unwrap();

    assert_eq!(json["working"].as_array().unwrap().len(), 1, "only the thread that works and is in front of you");
    assert_eq!(json["working"][0]["id"], working.id);
    assert_eq!(json["working"][0]["project"], "hub", "a dev thread's worktree is its repository");
    assert!(json["working"][0]["since"].as_i64().unwrap() < now_ms(), "the clock runs from when you sent it work");

    assert_eq!(json["waiting"].as_array().unwrap().len(), 1);
    let ask = &json["waiting"][0]["ask"];
    assert_eq!(json["waiting"][0]["id"], asking.id);
    assert_eq!(ask["request"], "req-1");
    assert_eq!(ask["what"], "Run `git push`");
    assert_eq!(ask["command"], "git push origin main");
    assert_eq!(ask["options"][0]["id"], "allow");
    assert_eq!(ask["options"][1]["kind"], "reject_once");
    assert!(json["limits"].is_array());
}

#[tokio::test]
async fn an_answered_question_is_not_offered_again() {
    let hub = hub();
    let asking = thread(&hub, "Push the branch", ThreadStatus::WaitingOnYou, |_| {}).await;
    hub.store.append_event(&asking.id, EventKind::PermissionRequest, json!({ "id": "req-1", "tool_call": { "title": "Run `ls`" }, "options": [] })).await.unwrap();
    hub.store.append_event(&asking.id, EventKind::PermissionResolved, json!({ "id": "req-1", "option_id": "allow" })).await.unwrap();
    let now = serde_json::to_value(hub_core::now::read(&hub).await.unwrap()).unwrap();
    assert!(now["waiting"][0].get("ask").is_none(), "it waits on something else now; the client opens it");
}
