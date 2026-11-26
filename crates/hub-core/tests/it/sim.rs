//! The inbox simulator's start and clear (`hub/sim.rs` in hub-core).

use std::sync::Arc;

use hub_core::model::NewThread;
use hub_core::{Config, Hub, Store};
use crate::support::mock_agent;

fn engine() -> hub_core::config::EngineConfig {
    hub_core::config::EngineConfig {
        command: mock_agent().display().to_string(),
        driver: Default::default(),
        secrets: Vec::new(),
        terminal: None,
        traits: hub_core::config::EngineTraits::preset("claude"),
        ..Default::default()
    }
}

fn hub(with_sim: bool) -> Arc<Hub> {
    let mut config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    config.keep_engines = false;
    config.engines.clear();
    config.engines.insert("mock".into(), engine());
    if with_sim {
        config.engines.insert(hub_core::sim::ENGINE.into(), engine());
    }
    config.default_engine = "mock".into();
    Hub::new(Store::in_memory().unwrap(), config)
}

fn thread_on(engine: &str) -> NewThread {
    serde_json::from_value(serde_json::json!({
        "cwd": hub_core::janitor::test_dir().display().to_string(),
        "engine": engine,
        "message": "hello",
    }))
    .unwrap()
}

#[tokio::test]
async fn the_simulator_needs_its_engine() {
    let hub = hub(false);
    let err = hub.start_sim().unwrap_err();
    assert!(err.to_string().contains("no simulator engine"), "{err:#}");
    assert!(hub.store.threads().await.unwrap().is_empty(), "nothing started");
}

#[tokio::test]
async fn clearing_removes_simulated_threads_and_nothing_else() {
    let hub = hub(true);
    let real = hub.create_thread(thread_on("mock")).await.unwrap();
    hub.create_thread(thread_on(hub_core::sim::ENGINE)).await.unwrap();
    hub.create_thread(thread_on(hub_core::sim::ENGINE)).await.unwrap();

    assert_eq!(hub.clear_sim().await.unwrap(), 2);
    let left = hub.store.threads().await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, real.id);
    assert_eq!(hub.clear_sim().await.unwrap(), 0, "nothing more to clear");
}
