//! Versions of Hub that talk to each other (ARCHITECTURE.md, "Versions that talk to each other").
//!
//! A phone, a window on another machine's Hub and a core relaying another host's threads run a
//! different build from the hubd they reach. These tests keep that working:
//! - `v1/` holds what hubds in the field send today. This build must read all of it, and must
//!   still read it with unknown fields added anywhere (what a newer hubd sends).
//! - `frozen/` holds what a client in the field relies on: the keys, statuses, kinds and calls
//!   below are only ever added to. Removing or retyping one fails here, on purpose.

use std::collections::BTreeSet;

use hub_core::compat::{CORE_CALLS, FEATURES, feature_names, stream_from_wire, thread_from_wire, threads_from_wire};
use hub_core::model::{Event, EventKind, StreamMsg, Thread, ThreadStatus};
use serde_json::{Value, json};

fn file(path: &str) -> String {
    let full = hub_core::source_tree::hub_core().join("tests/compat").join(path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

fn json_file(path: &str) -> Value {
    serde_json::from_str(&file(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn lines(path: &str) -> Vec<String> {
    file(path).lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect()
}

/// The same JSON with an unknown field in every object, the way a newer hubd sends more.
fn with_extras(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out: serde_json::Map<String, Value> = map.iter().map(|(k, v)| (k.clone(), with_extras(v))).collect();
            out.insert("added_in_a_later_version".into(), json!({ "anything": [1, 2, 3] }));
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(with_extras).collect()),
        other => other.clone(),
    }
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Null => "null",
    }
}

fn assert_frozen_keys(what: &str, sent: &Value, keys: &Value) {
    for (key, kind) in keys[what].as_object().unwrap() {
        let got = &sent[key];
        assert!(!got.is_null() || sent.get(key).is_some(), "{what}: `{key}` is no longer sent; clients in the field read it");
        if kind != "any" {
            assert_eq!(type_of(got), kind, "{what}: `{key}` changed type; clients in the field read it");
        }
    }
}

#[test]
fn what_hubds_in_the_field_send_is_still_read() {
    for name in ["v1/thread.json", "v1/thread-full.json"] {
        let thread: Thread = serde_json::from_value(json_file(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!thread.id.is_empty());
    }
    let events: Vec<Event> = serde_json::from_value(json_file("v1/event.json")).unwrap();
    assert_eq!(events.len(), 6);
    assert!(events.iter().all(|e| e.kind != EventKind::Unknown), "a kind of today is read as unknown");
    let stream: Vec<StreamMsg> = serde_json::from_value(json_file("v1/stream.json")).unwrap();
    assert_eq!(stream.len(), 5);
}

#[test]
fn what_a_newer_hubd_adds_does_not_break_reading() {
    for name in ["v1/thread.json", "v1/thread-full.json"] {
        let more = with_extras(&json_file(name));
        serde_json::from_value::<Thread>(more).unwrap_or_else(|e| panic!("{name} with more fields: {e}"));
    }
    serde_json::from_value::<Vec<Event>>(with_extras(&json_file("v1/event.json"))).expect("events with more fields");
    serde_json::from_value::<Vec<StreamMsg>>(with_extras(&json_file("v1/stream.json"))).expect("stream with more fields");
}

#[test]
fn a_value_from_a_newer_hubd_gives_the_nearest_thing_not_an_error() {
    // An event kind this build lacks is kept as unknown.
    let event: Event = serde_json::from_value(json!({ "thread_id": "t", "seq": 1, "ts": 1, "kind": "from_the_future", "data": {} })).unwrap();
    assert_eq!(event.kind, EventKind::Unknown);
    // A thread with a status or owner kind it lacks is still a thread, in a list with others.
    let mut odd = json_file("v1/thread.json");
    odd["status"] = json!("napping");
    odd["owner"] = json!({ "kind": "from_the_future", "name": "x" });
    let list = threads_from_wire(vec![json_file("v1/thread.json"), odd.clone(), json!({ "broken": true })]);
    assert_eq!(list.len(), 2, "one unreadable thread must not cost the list");
    assert_eq!(thread_from_wire(odd).unwrap().status, ThreadStatus::Idle);
    // The stream: a thread in it is read the same way; a message type it lacks is skipped.
    let msg = json!({ "type": "thread", "thread": { "id": "t", "title": "t", "cwd": "/", "engine": "e", "status": "napping",
        "engine_session_id": null, "title_locked": false, "archived": false, "created_at": 1, "updated_at": 2 } });
    assert!(matches!(stream_from_wire(&msg.to_string()), Some(StreamMsg::Thread { .. })));
    assert!(stream_from_wire(r#"{"type":"something_new","x":1}"#).is_none());
    assert!(serde_json::from_str::<StreamMsg>(r#"{"type":"something_new"}"#).is_err(), "the caller skips what it cannot read");
}

#[test]
fn what_this_build_sends_still_has_what_clients_in_the_field_read() {
    let keys = json_file("frozen/keys.json");
    let thread: Thread = serde_json::from_value(json_file("v1/thread.json")).unwrap();
    assert_frozen_keys("thread", &serde_json::to_value(&thread).unwrap(), &keys);
    let events: Vec<Event> = serde_json::from_value(json_file("v1/event.json")).unwrap();
    assert_frozen_keys("event", &serde_json::to_value(&events[0]).unwrap(), &keys);
}

#[test]
fn statuses_kinds_and_message_types_of_today_stay() {
    for status in lines("frozen/thread-statuses.txt") {
        let read: ThreadStatus = serde_json::from_value(json!(status)).unwrap_or_else(|_| panic!("status `{status}` is gone"));
        assert_eq!(serde_json::to_value(read).unwrap(), json!(status), "status `{status}` is spelled differently now");
    }
    for kind in lines("frozen/event-kinds.txt") {
        let read: EventKind = serde_json::from_value(json!(kind)).unwrap();
        assert_ne!(read, EventKind::Unknown, "event kind `{kind}` is gone");
        assert_eq!(serde_json::to_value(read).unwrap(), json!(kind));
    }
    let stream: Vec<Value> = serde_json::from_value(json_file("v1/stream.json")).unwrap();
    let types: BTreeSet<&str> = stream.iter().filter_map(|m| m["type"].as_str()).collect();
    for kind in lines("frozen/stream-types.txt") {
        assert!(types.contains(kind.as_str()), "v1/stream.json lacks an example of `{kind}`");
    }
}

#[test]
fn status_events_and_error_leaves_are_additive() {
    // A `status` marker is a kind this build knows; an older build reads it as unknown, which surfaces skip.
    let marker: Event = serde_json::from_value(json!({ "thread_id": "t", "seq": 2, "ts": 1, "kind": "status", "data": { "status": "in_terminal", "why": "taken" } })).unwrap();
    assert_eq!(marker.kind, EventKind::Status);
    // An error from before `leaves` existed, and one with it, are both plain errors.
    for data in [json!({ "message": "x", "prompt": 1 }), json!({ "message": "x", "prompt": 1, "leaves": "idle" })] {
        let event: Event = serde_json::from_value(json!({ "thread_id": "t", "seq": 3, "ts": 1, "kind": "error", "data": data })).unwrap();
        assert_eq!(event.kind, EventKind::Error);
    }
}

fn call_line((method, path): (&str, &str)) -> String {
    format!("{method} {path}")
}

fn allowed_for_a_phone(method: &str, path: &str) -> bool {
    let path = path.replace("{id}", "abc").replace("{request}", "7").replace("{problem}", "7");
    hub_core::phone::may(&method.parse().unwrap(), &path)
}

#[test]
fn the_core_calls_stay_and_a_phone_may_make_them() {
    let now: BTreeSet<String> = CORE_CALLS.iter().copied().map(call_line).collect();
    for line in lines("frozen/core-calls.txt") {
        assert!(now.contains(&line), "core call `{line}` was removed; clients in the field make it");
    }
    for (method, path) in CORE_CALLS {
        assert!(allowed_for_a_phone(method, path), "a phone may not make core call {method} {path}");
    }
}

#[test]
fn features_are_only_added_and_a_phone_feature_is_one_a_phone_may_use() {
    let now: BTreeSet<&str> = feature_names().into_iter().collect();
    for name in lines("frozen/features.txt") {
        assert!(now.contains(name.as_str()), "feature `{name}` was removed or renamed; clients in the field ask for it");
    }
    for feature in FEATURES.iter().filter(|f| f.name.starts_with("phone.")) {
        assert!(!feature.calls.is_empty(), "{} lists no calls", feature.name);
        for (method, path) in feature.calls {
            assert!(allowed_for_a_phone(method, path), "{} is advertised, but a phone may not make {method} {path}", feature.name);
        }
    }
}

#[tokio::test]
async fn health_tells_a_client_what_this_hubd_can_do() {
    let config = hub_core::Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = hub_core::Hub::new(hub_core::Store::in_memory().unwrap(), config);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, hub_core::api::router(hub)).await.unwrap() });
    // Open: no token, so any client can ask before it knows what it may use.
    let health: Value = reqwest::Client::builder().no_proxy().build().unwrap().get(format!("{base}/api/health")).send().await.unwrap().json().await.unwrap();
    assert_frozen_keys("health", &health, &json_file("frozen/keys.json"));
    assert_eq!(health["core"], hub_core::compat::CORE);
    let listed: BTreeSet<&str> = health["features"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    for name in feature_names() {
        assert!(listed.contains(name), "/api/health does not list `{name}`");
    }
    task.abort();
}

/// Asks the real router, with this machine's token, whether it has a route for the call. A
/// request with a method no route uses never reaches a handler: a path nobody registered is a 404
/// and a path with other methods only is a 405 naming the ones it has, so nothing is run.
pub(crate) async fn router_has(hub: &std::sync::Arc<hub_core::Hub>, method: &str, path: &str) -> Result<(), String> {
    use tower::ServiceExt;
    let path = path.split('/').map(|p| if p.starts_with('{') { "x" } else { p }).collect::<Vec<_>>().join("/");
    let token = hub_core::auth::token(&hub.config.data_dir).unwrap();
    let req = axum::http::Request::builder()
        .method("PROBE")
        .uri(&path)
        .header("authorization", format!("Bearer {token}"))
        .body(axum::body::Body::empty())
        .unwrap();
    let res = hub_core::api::router(hub.clone()).oneshot(req).await.unwrap();
    match res.status() {
        axum::http::StatusCode::NOT_FOUND => Err(format!("{method} {path}: the router has no such path")),
        axum::http::StatusCode::METHOD_NOT_ALLOWED => {
            let allow = res.headers().get("allow").and_then(|a| a.to_str().ok()).unwrap_or_default().to_owned();
            if allow.split(',').any(|m| m.trim().eq_ignore_ascii_case(method)) {
                Ok(())
            } else {
                Err(format!("{method} {path}: the router has the path, but only for [{allow}]"))
            }
        }
        other => Err(format!("{method} {path}: expected 404 or 405 from a probe, got {other}")),
    }
}

#[tokio::test]
async fn every_core_and_feature_call_is_a_route_in_the_router() {
    let config = hub_core::Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = hub_core::Hub::new(hub_core::Store::in_memory().unwrap(), config);
    let mut missing = Vec::new();
    for (method, path) in CORE_CALLS.iter().chain(FEATURES.iter().flat_map(|f| f.calls.iter())) {
        if let Err(e) = router_has(&hub, method, path).await {
            missing.push(e);
        }
    }
    assert!(missing.is_empty(), "compat lists calls the router does not serve (a typo gives a phone a 404):\n{}", missing.join("\n"));
}
