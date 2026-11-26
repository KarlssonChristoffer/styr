//! Notices from outside (`notice.rs`): a post goes on the stream and into the list, a bad one is
//! refused with a reason, a source that talks too much is turned away, and the phone only gets
//! what it was asked to.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use hub_core::model::StreamMsg;
use hub_core::{Config, Hub, Store};
use serde_json::{Value, json};
use tower::ServiceExt;

fn hub() -> Arc<Hub> {
    let config = Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    Hub::new(Store::in_memory().unwrap(), config)
}

async fn post(hub: &Arc<Hub>, body: Value) -> (StatusCode, Value) {
    let token = hub_core::auth::token(&hub.config.data_dir).unwrap();
    let req = Request::post("/api/notices")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = hub_core::api::router(hub.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn a_notice_goes_on_the_stream_and_into_the_list() {
    let hub = hub();
    let mut stream = hub.subscribe();
    let (status, notice) = post(&hub, json!({ "source": "it-widget", "title": "Backup done", "sort": "done", "link": "hub://inbox" })).await;
    assert_eq!(status, StatusCode::OK, "{notice}");
    assert_eq!(notice["source"], "it-widget");
    assert_eq!(notice["phone"], Value::Null, "the phone was not asked for");
    match stream.recv().await.unwrap() {
        StreamMsg::Notice { notice: n } => assert_eq!((n.title.as_str(), n.sort.as_str()), ("Backup done", "done")),
        other => panic!("expected a notice on the stream, got {other:?}"),
    }
    assert!(hub_core::notice::recent().iter().any(|n| n.source == "it-widget" && n.title == "Backup done"));
}

#[tokio::test]
async fn a_bad_notice_is_refused_with_the_reason() {
    let hub = hub();
    let (status, _) = post(&hub, json!({ "source": "it-bad", "title": "  " })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = post(&hub, json!({ "source": "it-bad", "title": "x", "link": "file:///etc/passwd" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_source_that_talks_too_much_gets_429() {
    let hub = hub();
    for _ in 0..20 {
        assert_eq!(post(&hub, json!({ "source": "it-chatty", "title": "again" })).await.0, StatusCode::OK);
    }
    assert_eq!(post(&hub, json!({ "source": "it-chatty", "title": "again" })).await.0, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(post(&hub, json!({ "source": "it-quiet", "title": "hello" })).await.0, StatusCode::OK);
}

#[tokio::test]
async fn the_phone_is_only_told_when_asked_and_set_up() {
    let hub = hub();
    // Asked for, but this machine has no phone topic: held, with the reason, and still on the stream.
    let (status, notice) = post(&hub, json!({ "source": "it-phone", "title": "Disk full", "sort": "failed", "phone": true })).await;
    assert_eq!(status, StatusCode::OK);
    assert!(notice["phone"].as_str().unwrap().starts_with("held"), "{notice}");
}

#[tokio::test]
async fn nobody_posts_without_the_connection_token() {
    let hub = hub();
    let req = Request::post("/api/notices").header("content-type", "application/json").body(Body::from(json!({ "title": "hi" }).to_string())).unwrap();
    assert_eq!(hub_core::api::router(hub.clone()).oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);
}
