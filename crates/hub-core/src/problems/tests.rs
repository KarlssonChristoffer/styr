//! Guards how hits become problems and how Sentry reports get in: numbers and names in a message
//! and moved lines do not split a problem, envelopes read with and without lengths, gzip bodies
//! unpack, the key is found wherever an SDK puts it, and only Sentry's paths count as ingest.

use super::*;

#[test]
fn numbers_and_names_do_not_split_a_problem() {
    assert_eq!(normalize("could not open \"C:\\x\\a.json\": error 5"), normalize("could not open \"D:\\y.json\": error 32"));
    assert_eq!(normalize("thread 0198f3a2b4c5 stopped after 12 s"), normalize("thread 0198f3a2b4c6 stopped after 9 s"));
    assert_ne!(normalize("could not read the secrets"), normalize("could not read the pause"));
}

#[test]
fn stack_groups_by_own_frames_not_lines() {
    let hit = |line, value: &str| Hit {
        message: value.into(),
        kind: Some("KeyError".into()),
        frames: vec![
            Frame { file: Some("lib/x.py".into()), function: Some("get".into()), line: Some(3), own: false, code: None },
            Frame { file: Some("app/a.py".into()), function: Some("parse".into()), line: Some(line), own: true, code: None },
        ],
        ..Default::default()
    };
    let a = fingerprint("Aftermath", "sentry", None, &hit(88, "'session_id'"));
    let b = fingerprint("Aftermath", "sentry", None, &hit(91, "'model'"));
    assert_eq!(a, b);
    assert_ne!(a, fingerprint("Knegarn", "sentry", None, &hit(88, "'session_id'")));
    assert_eq!(culprit(&hit(88, "").frames).as_deref(), Some("app/a.py in parse, line 88"));
}

#[test]
fn reads_an_envelope_with_and_without_lengths() {
    let event = r#"{"level":"error","exception":{"values":[{"type":"KeyError","value":"'session_id'","stacktrace":{"frames":[{"filename":"a.py","function":"run","lineno":4,"in_app":true}]}}]},"breadcrumbs":{"values":[{"timestamp":1761925262.5,"message":"reading files","level":"info"}]},"timestamp":"2026-10-31T15:41:02.500Z","release":"0.9.2"}"#;
    let body = format!(
        "{{\"event_id\":\"x\",\"dsn\":\"http://k@127.0.0.1:7878/1\"}}\n{{\"type\":\"session\"}}\n{{\"status\":\"ok\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{}\n",
        event.len(),
        event
    );
    let events: Vec<Value> = envelope_events(body.as_bytes()).unwrap().into_iter().filter(|(k, _)| k == "event").map(|(_, v)| v).collect();
    assert_eq!(events.len(), 1);
    let source = Source { id: 1, name: "Aftermath".into(), key: "k".into(), folder: None, dsn: String::new(), quiet_after_hours: None, last_heard: None, otlp: String::new() };
    let incoming = from_sentry(&source, &events[0]);
    assert_eq!(incoming.level, Level::Error);
    assert_eq!(incoming.hit.kind.as_deref(), Some("KeyError"));
    assert_eq!(title(&incoming.hit), "KeyError: 'session_id'");
    assert_eq!(incoming.hit.crumbs[0].message, "reading files");
    assert_eq!(incoming.hit.ts, 1_793_461_262_500);
    assert_eq!(incoming.hit.release.as_deref(), Some("0.9.2"));
    // Unbounded items end at the newline.
    let loose = format!("{{}}\n{{\"type\":\"event\"}}\n{event}\n");
    assert_eq!(envelope_events(loose.as_bytes()).unwrap().len(), 1);
}

#[test]
fn finds_the_key_where_sdks_put_it() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("x-sentry-auth", "Sentry sentry_version=7, sentry_key=abc, sentry_client=x".parse().unwrap());
    assert_eq!(shown_key(&headers, None, b"").as_deref(), Some("abc"));
    let none = axum::http::HeaderMap::new();
    assert_eq!(shown_key(&none, Some("sentry_version=7&sentry_key=def"), b"").as_deref(), Some("def"));
    assert_eq!(shown_key(&none, None, b"{\"dsn\":\"http://ghi@127.0.0.1:7878/2\"}\n").as_deref(), Some("ghi"));
}

#[test]
fn only_sentry_paths_are_ingest() {
    assert!(is_ingest("/api/3/envelope/"));
    assert!(is_ingest("/api/12/store"));
    assert!(!is_ingest("/api/problems/report"));
    assert!(!is_ingest("/api/3/envelope/x"));
    assert!(!is_ingest("/api/threads/envelope/"));
    assert!(is_ingest("/api/4/otlp/v1/logs"));
    assert!(!is_ingest("/api/x/otlp/v1/logs"));
}

#[test]
fn gzip_bodies_are_unpacked() {
    use std::io::Write;
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(b"{\"a\":1}").unwrap();
    assert_eq!(decode(Some("gzip"), &gz.finish().unwrap()).unwrap(), b"{\"a\":1}");
}
