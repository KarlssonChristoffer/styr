//! Threads Hub starts on its own (`system.rs`, `Hub::create_thread_as`, `Hub::backfill_owners`): they carry their owner, it survives the store, and
//! threads from before owners existed get theirs from what Hub already knew.

use std::sync::Arc;

use hub_core::model::{NewThread, Owner, OwnerKind};
use hub_core::{Config, Hub, Store};

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: tempfile::tempdir().unwrap().keep(), ..Default::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}

fn new(hub: &Hub, title: &str) -> NewThread {
    NewThread {
        cwd: hub.config.data_dir.to_string_lossy().into_owned(),
        title: Some(title.into()),
        engine: None,
        message: None,
        attachments: Vec::new(),
        settings: Default::default(),
        original: None,
        client_id: None,
        attempts: Vec::new(),
        warm: None,
        design: false,
        unattended: None,
        reach: None,
        cloud: false,
    }
}

#[tokio::test]
async fn a_thread_hub_starts_keeps_its_owner() {
    let hub = hub();
    let owner = Owner::new(OwnerKind::Routine, "Daily review", Some("daily-review".into()));
    let made = hub.create_thread_as(new(&hub, "Daily review · 1 Nov"), None, Some(owner.clone())).await.unwrap();
    assert_eq!(made.owner.as_ref(), Some(&owner));
    assert!(made.owner.as_ref().unwrap().system());
    let stored = hub.store.thread(&made.id).await.unwrap().unwrap();
    assert_eq!(stored.owner, Some(owner.clone()));
    // A later change to the thread leaves the owner alone.
    hub.store.update_thread(&made.id, |t| t.title = "renamed".into()).await.unwrap();
    let all = hub.store.threads().await.unwrap();
    assert_eq!(all.iter().find(|t| t.id == made.id).unwrap().owner, Some(owner));

    let yours = hub.create_thread(new(&hub, "mine")).await.unwrap();
    assert_eq!(yours.owner, None);
}

#[tokio::test]
async fn old_threads_get_their_owners_once() {
    let hub = hub();
    let vakten = hub.create_thread(new(&hub, "Vakten")).await.unwrap();
    hub.store.mark_thread(&vakten.id, |t| t.pinned_at = Some(1)).await.unwrap();
    hub.store.set_value("problems.vakten.state", &format!(r#"{{"thread":"{}"}}"#, vakten.id)).await.unwrap();
    let widget = hub.create_thread(new(&hub, "Widget · clock")).await.unwrap();
    let mine = hub.create_thread(new(&hub, "Fix the login")).await.unwrap();

    // As if from a database before owners: let it run again.
    hub.store.take_value("system.owners").await.unwrap();
    hub.clone().backfill_owners().await;

    let get = |id: String| {
        let hub = hub.clone();
        async move { hub.store.thread(&id).await.unwrap().unwrap() }
    };
    let v = get(vakten.id.clone()).await;
    assert_eq!(v.owner.map(|o| o.kind), Some(OwnerKind::Vakten));
    assert_eq!(v.pinned_at, None, "Vakten's chat is no longer pinned to the top of the inbox");
    assert_eq!(get(widget.id).await.owner.map(|o| o.kind), Some(OwnerKind::Widget));
    assert_eq!(get(mine.id.clone()).await.owner, None);

    // Once: a thread named like one later is the user's to keep.
    let later = hub.create_thread(new(&hub, "Widget · mine")).await.unwrap();
    hub.clone().backfill_owners().await;
    assert_eq!(get(later.id).await.owner, None);
}
