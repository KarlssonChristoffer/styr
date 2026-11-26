//! Customs: a project's packages clear as already in use, an agent finds them with its tools, and
//! what it declares is held until the user clears it.

use std::sync::Arc;

use hub_core::hub_tools::{Caller, call};
use hub_core::model::{AgentAccess, CustomsEdit, CustomsPrefs, CustomsStatus};
use hub_core::{Config, Hub, Store};
use serde_json::{Value, json};

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}

async fn tool(hub: &Arc<Hub>, name: &str, args: Value) -> Value {
    let caller = Caller::Agent { name: "terminal".into(), access: AgentAccess::Read };
    let answer = call(hub, &caller, name, &args).await.expect("one of Hub's tools").expect("the tool works");
    serde_json::from_str(&answer).unwrap()
}

#[tokio::test]
async fn what_is_used_clears_and_what_is_declared_is_held() {
    let hub = hub();
    // The review looks things up online; it has tests of its own.
    let quiet = CustomsPrefs { auto_review: false, ..CustomsPrefs::default() };
    hub_core::customs::set_prefs(&hub.ctx(), &quiet).await.unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("package.json"), r#"{"dependencies":{"svelte":"^5"},"devDependencies":{"vite":"^7"}}"#).unwrap();
    std::fs::create_dir(project.path().join("core")).unwrap();
    std::fs::write(project.path().join("core/Cargo.toml"), "[dependencies]\nSerde = \"1\"\nown = { path = \"../own\" }\n").unwrap();

    let first = hub_core::customs::import(&hub.ctx(), project.path()).await.unwrap();
    assert_eq!((first.added, first.known), (3, 0));
    let again = hub_core::customs::import(&hub.ctx(), project.path()).await.unwrap();
    assert_eq!((again.added, again.known), (0, 3));

    // The user sorts one; an agent finds it by its words.
    hub_core::customs::edit(
        &hub.ctx(),
        CustomsEdit {
            kind: "npm".into(),
            name: "svelte".into(),
            status: None,
            category: Some("UI".into()),
            summary: Some("Compiled UI components".into()),
            versions: None,
            reason: None,
        },
    )
    .await
    .unwrap();
    let found = tool(&hub, "customs_search", json!({ "query": "ui components" })).await;
    assert_eq!(found["total"], 1, "{found}");
    assert_eq!(found["items"][0]["name"], "svelte");
    assert_eq!(found["items"][0]["status"], "cleared");
    let crates = tool(&hub, "customs_search", json!({ "kind": "crates" })).await;
    assert_eq!(crates["items"][0]["name"], "serde", "crate names compare without case");

    // Even a read-only agent may declare; what it declares is held, and asking again changes nothing.
    let declared = tool(&hub, "customs_declare", json!({ "kind": "cdn", "name": "bootstrap", "why": "grid layout" })).await;
    assert_eq!(declared["status"], "held");
    assert_eq!(declared["reason"], "Declared: grid layout");
    let again = tool(&hub, "customs_declare", json!({ "kind": "cdn", "name": "bootstrap", "why": "other" })).await;
    assert_eq!(again["reason"], "Declared: grid layout");
    let known = tool(&hub, "customs_declare", json!({ "kind": "npm", "name": "vite", "why": "build" })).await;
    assert_eq!(known["status"], "cleared");

    // Held comes after cleared in a search, and the user's stamp clears it.
    let all = tool(&hub, "customs_search", json!({})).await;
    assert_eq!(all["items"][3]["name"], "bootstrap", "{all}");
    let stamped = hub_core::customs::edit(
        &hub.ctx(),
        CustomsEdit {
            kind: "cdn".into(),
            name: "bootstrap".into(),
            status: Some(CustomsStatus::Cleared),
            category: None,
            summary: None,
            versions: Some("^5".into()),
            reason: None,
        },
    )
    .await
    .unwrap();
    assert_eq!((stamped.status, stamped.decided_by.as_str()), (CustomsStatus::Cleared, "you"));

    hub_core::customs::remove(&hub.ctx(), "cdn", "bootstrap").await.unwrap();
    assert_eq!(hub_core::customs::list(&hub.ctx()).await.unwrap().len(), 3);
}

#[tokio::test]
async fn machines_share_one_list() {
    let (pc, server) = (hub(), hub());
    let quiet = CustomsPrefs { auto_review: false, ..CustomsPrefs::default() };
    for h in [&pc, &server] {
        hub_core::customs::set_prefs(&h.ctx(), &quiet).await.unwrap();
    }
    let clear = |kind: &str, name: &str| CustomsEdit {
        kind: kind.into(),
        name: name.into(),
        status: Some(CustomsStatus::Cleared),
        category: None,
        summary: None,
        versions: None,
        reason: None,
    };
    hub_core::customs::edit(&pc.ctx(), clear("npm", "svelte")).await.unwrap();
    hub_core::customs::edit(&pc.ctx(), clear("npm", "left-pad")).await.unwrap();
    hub_core::customs::request(&server.ctx(), "crates", "serde", "parsing", "", "agent", None).await.unwrap();

    // The PC swaps lists with the server: both end up with all three.
    let answer = hub_core::customs::merge(&server.ctx(), hub_core::customs::shared(&pc.ctx()).await.unwrap()).await.unwrap();
    hub_core::customs::merge(&pc.ctx(), answer).await.unwrap();
    let names = |l: Vec<hub_core::model::CustomsItem>| l.into_iter().map(|m| m.name).collect::<Vec<_>>();
    assert_eq!(names(hub_core::customs::list(&pc.ctx()).await.unwrap()), ["serde", "left-pad", "svelte"].map(String::from));
    assert_eq!(names(hub_core::customs::list(&server.ctx()).await.unwrap()).len(), 3);

    // A stamp on the server reaches the PC, and what the PC took off stays off on the server.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    hub_core::customs::edit(&server.ctx(), clear("crates", "serde")).await.unwrap();
    hub_core::customs::remove(&pc.ctx(), "npm", "left-pad").await.unwrap();
    let answer = hub_core::customs::merge(&server.ctx(), hub_core::customs::shared(&pc.ctx()).await.unwrap()).await.unwrap();
    hub_core::customs::merge(&pc.ctx(), answer).await.unwrap();
    for h in [&pc, &server] {
        let list = hub_core::customs::list(&h.ctx()).await.unwrap();
        assert_eq!(names(list.clone()), ["serde", "svelte"].map(String::from));
        assert_eq!(list[0].status, CustomsStatus::Cleared);
    }
}

#[tokio::test]
async fn what_happens_is_logged() {
    let hub = hub();
    let quiet = CustomsPrefs { auto_review: false, ..CustomsPrefs::default() };
    hub_core::customs::set_prefs(&hub.ctx(), &quiet).await.unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("package.json"), r#"{"dependencies":{"svelte":"^5"}}"#).unwrap();
    hub_core::customs::import(&hub.ctx(), project.path()).await.unwrap();
    hub_core::customs::request(&hub.ctx(), "npm", "left-pad", "pads", "", "agent", None).await.unwrap();
    // Known already: nothing new happened.
    hub_core::customs::request(&hub.ctx(), "npm", "left-pad", "pads", "", "agent", None).await.unwrap();
    let decide = |status| CustomsEdit {
        kind: "npm".into(),
        name: "left-pad".into(),
        status: Some(status),
        category: None,
        summary: None,
        versions: None,
        reason: None,
    };
    hub_core::customs::edit(&hub.ctx(), decide(CustomsStatus::Cleared)).await.unwrap();
    // Sorting it is not something that happened at Customs.
    hub_core::customs::edit(&hub.ctx(), CustomsEdit { status: None, category: Some("Text".into()), ..decide(CustomsStatus::Cleared) }).await.unwrap();
    hub_core::customs::remove(&hub.ctx(), "npm", "left-pad").await.unwrap();

    let log = hub_core::customs_log::activity(&hub.store, None).await.unwrap();
    let said: Vec<(&str, &str, &str)> = log.entries.iter().map(|e| (e.what.as_str(), e.by.as_str(), e.name.as_str())).collect();
    assert_eq!(
        said,
        vec![("removed", "you", "left-pad"), ("cleared", "you", "left-pad"), ("declared", "agent", "left-pad"), ("imported", "you", "")],
        "newest first"
    );
    assert_eq!(log.entries[1].detail, "Was held");
    assert!(log.reviewing.is_empty());
    let one = hub_core::customs_log::activity(&hub.store, Some(("npm".into(), "left-pad".into()))).await.unwrap();
    assert_eq!(one.entries.len(), 3, "one item's own story");
}
