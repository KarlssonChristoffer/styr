//! The iPhone app against this server, checked from the server side (no Mac needed).
//!
//! Every call the Swift sources make (`call("GET", "api/…")` and the few `appendingPathComponent`
//! URLs) must be a route in the real router AND a call a phone's token may make (`phone::may`),
//! so a renamed route or a dropped allowance fails here and not as a 404 or 403 on the phone.
//! The Swift side of the same contract (decoding `tests/compat/v1/*.json` with `Model.swift`) is
//! `ios/tests/ContractTests.swift`, which needs a Mac; see ARCHITECTURE.md, "Versions that talk
//! to each other".

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo() -> PathBuf {
    hub_core::source_tree::root()
}

/// `\(expr)` in a Swift string becomes `{x}`; the expression may hold one level of parentheses.
fn placeholders(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'(') {
            chars.next();
            let mut depth = 1;
            for d in chars.by_ref() {
                match d {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    break;
                }
            }
            out.push_str("{x}");
        } else {
            out.push(c);
        }
    }
    out
}

/// A quoted string starting at `from` (just after the opening quote); returns it and what follows.
fn quoted(text: &str) -> Option<(&str, &str)> {
    let mut prev = '\0';
    for (i, c) in text.char_indices() {
        if c == '"' && prev != '\\' {
            return Some((&text[..i], &text[i + 1..]));
        }
        prev = if prev == '\\' && c == '\\' { '\0' } else { c };
    }
    None
}

/// (method, path) of every call in the Swift sources, with the file for the message.
fn ios_calls() -> Vec<(String, String, String)> {
    let dir = repo().join("ios/Hub");
    let mut files: Vec<_> = std::fs::read_dir(&dir).expect("ios/Hub").map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "swift")).collect();
    files.sort();
    let mut found = Vec::new();
    for file in files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        let text = std::fs::read_to_string(&file).unwrap();
        // call("METHOD", "path"
        let mut rest = text.as_str();
        while let Some(at) = rest.find("call(\"") {
            rest = &rest[at + "call(\"".len()..];
            let Some((method, after)) = quoted(rest) else { break };
            let after = after.trim_start_matches([',', ' ']);
            if ["GET", "POST", "PUT", "PATCH", "DELETE"].contains(&method)
                && after.starts_with('"')
                && let Some((path, _)) = quoted(&after[1..])
            {
                found.push((method.to_owned(), format!("/{}", placeholders(path)), name.clone()));
            }
        }
        // base.appendingPathComponent("api/…"): a plain GET (a page in a web view)
        let mut rest = text.as_str();
        while let Some(at) = rest.find("appendingPathComponent(\"api/") {
            rest = &rest[at + "appendingPathComponent(\"".len()..];
            if let Some((path, _)) = quoted(rest) {
                // The pairing call is a POST built by hand, and the stream is a socket: both are
                // listed below. Everything else this finds is a page loaded by URL.
                if path.starts_with("api/pair") || path == "api/stream" {
                    continue;
                }
                found.push(("GET".into(), format!("/{}", placeholders(path)), name.clone()));
            }
        }
    }
    // Built by hand in HubClient.swift (`redeem`): an open POST; a phone has no token yet.
    found.push(("POST".into(), "/api/pair/redeem".into(), "HubClient.swift".into()));
    found.sort();
    found.dedup();
    found
}

/// Open calls and web-view pages are not checked against `phone::may`: the pairing code is
/// redeemed before there is a token, and a page is let in by its own rule in `guard.rs`.
fn needs_phone_token(path: &str) -> bool {
    !(path == "/api/pair/redeem" || path.contains("/pages/"))
}

#[test]
fn the_scan_finds_the_calls_the_app_makes() {
    let calls = ios_calls();
    let has = |m: &str, p: &str| calls.iter().any(|(cm, cp, _)| cm == m && cp == p);
    assert!(has("GET", "/api/threads"), "the scan lost the inbox call");
    assert!(has("POST", "/api/threads/{x}/messages"), "the scan lost send");
    assert!(has("POST", "/api/threads/{x}/permissions/{x}"), "the scan lost permission answers");
    assert!(calls.len() > 60, "only {} calls found; did the Swift call style change?", calls.len());
}

#[tokio::test]
async fn every_call_the_iphone_app_makes_is_a_route() {
    let config = hub_core::Config { data_dir: hub_core::janitor::test_dir(), ..Default::default() };
    let hub = hub_core::Hub::new(hub_core::Store::in_memory().unwrap(), config);
    let mut bad = Vec::new();
    for (method, path, file) in ios_calls() {
        if let Err(e) = super::compat::router_has(&hub, &method, &path).await {
            bad.push(format!("{file}: {e}"));
        }
    }
    assert!(bad.is_empty(), "the iPhone app makes calls the router does not serve:\n{}", bad.join("\n"));
}

#[test]
fn every_call_the_iphone_app_makes_is_one_a_phone_may_make() {
    let refused: BTreeSet<String> = ios_calls()
        .into_iter()
        .filter(|(method, path, _)| needs_phone_token(path) && !hub_core::phone::may(&method.parse().unwrap(), &path.replace("{x}", "7")))
        .map(|(method, path, file)| format!("{file}: {method} {path}"))
        .collect();
    assert!(refused.is_empty(), "the phone's token is refused (403) these calls the app makes; add them to phone::may:\n{}", refused.into_iter().collect::<Vec<_>>().join("\n"));
}
