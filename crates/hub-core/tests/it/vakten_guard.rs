//! One Vakten for every machine: a machine asked for its queue by another machine's Vakten hands
//! it over, keeps its own Vakten still, and stops counting its unsorted errors as the user's.
//! Being guarded is kept per Hub (`Remotes::guarding`), so the other tests' hubs in this program
//! never see it.

use std::sync::Arc;

use hub_core::problems::{self, ProblemFilter, View, vakten};
use hub_core::{Config, Hub, Store};

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}

#[tokio::test]
async fn another_machines_vakten_takes_the_queue() {
    let hub = hub();
    let report = serde_json::json!({ "project": "Knegarn", "level": "error", "message": "index out of range in parse_rows" });
    let p = problems::record(&hub, problems::Incoming::from_report(serde_json::from_value(report).unwrap(), "app")).await.unwrap();
    // Nobody reads this machine yet: an unsorted error waits for the user.
    assert!(problems::get(&hub, p.id).await.unwrap().unwrap().needs_you);
    assert_eq!(vakten::guarded(&hub), None);

    // A fix thread here reported back: only a Guard that asks for reports takes them, once.
    let said = problems::report::Report {
        problem: p.id,
        machine: None,
        thread: "t1".into(),
        thread_machine: None,
        title: "Fix the parser".into(),
        outcome: "blocked".into(),
        text: "No test data".into(),
        pr: None,
        ts: 0,
    };
    problems::report::push(&hub, said.clone()).await.unwrap();
    let q = vakten::guard_queue(&hub, "PC", 0, 1, 15, false).await.unwrap();
    assert!(q.reports.is_empty() && problems::report::waiting(&hub).await);
    let q = vakten::guard_queue(&hub, "PC", 0, 1, 15, true).await.unwrap();
    assert_eq!(q.reports, vec![said]);
    assert!(!problems::report::waiting(&hub).await);
    assert_eq!(q.waiting, 1);
    assert_eq!(q.fresh.len(), 1);
    assert_eq!(vakten::guarded(&hub).as_deref(), Some("PC"));
    // The PC's Vakten reads it now, so it no longer needs the user, and this machine's Vakten stays still.
    assert!(!problems::get(&hub, p.id).await.unwrap().unwrap().needs_you);
    let needs = problems::list(&hub, ProblemFilter { view: Some(View::NeedsYou), ..Default::default() }).await.unwrap();
    assert!(needs.is_empty());
    let status = vakten::status(&hub).await.unwrap();
    assert_eq!((status.state.as_str(), status.guarded_by.as_deref()), ("guarded", Some("PC")));
    vakten::set_prefs(&hub, vakten::VaktenPrefs { on: true, ..Default::default() }).await.unwrap();
    assert!(!vakten::look(&hub).await.unwrap());
    assert_eq!(vakten::queue(&hub).await.unwrap(), 0);
}

/// Two cores in one process: the one another machine's Vakten reads is guarded, the other is not.
/// Guarding used to be a process-wide static, so a guarded test hub made other tests' unsorted
/// errors stop needing the user and emptied their Vakten queue.
#[tokio::test]
async fn being_guarded_stays_with_its_own_hub() {
    let (guarded, other) = (hub(), hub());
    vakten::guard_queue(&guarded, "PC", 0, 1, 15, false).await.unwrap();
    assert_eq!(vakten::guarded(&guarded).as_deref(), Some("PC"));
    assert_eq!(vakten::guarded(&other), None);
    let report = serde_json::json!({ "project": "Knegarn", "level": "error", "message": "index out of range in parse_rows" });
    let p = problems::record(&other, problems::Incoming::from_report(serde_json::from_value(report).unwrap(), "app")).await.unwrap();
    assert!(problems::get(&other, p.id).await.unwrap().unwrap().needs_you);
    vakten::set_prefs(&other, vakten::VaktenPrefs { on: true, ..Default::default() }).await.unwrap();
    assert_eq!(vakten::queue(&other).await.unwrap(), 1);
    assert_eq!(vakten::status(&other).await.unwrap().guarded_by, None);
}
