//! Locks the HTTP routes of the core. `api.rs` is being split into `api/<area>.rs`; a move that
//! drops or doubles a route fails here. Adding a route means adding its path to
//! `frozen/routes.txt` (sorted, one line per `.route(` call); removing one breaks clients in the
//! field (see `compat::`).

use std::path::PathBuf;

fn root() -> PathBuf {
    hub_core::source_tree::hub_core()
}

/// The first argument of every `.route(` call in `api.rs` and `api/*.rs`: a string literal's
/// text, or a constant's path (`crate::gateway::CALLBACK`). One entry per call, sorted.
fn routes_in_source() -> Vec<String> {
    let mut files = vec![root().join("src/api.rs")];
    let mut dir: Vec<_> = std::fs::read_dir(root().join("src/api")).expect("src/api").map(|e| e.unwrap().path()).collect();
    dir.sort();
    files.extend(dir.into_iter().filter(|p| p.extension().is_some_and(|e| e == "rs")));
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap().replace("\r\n", "\n");
        let mut rest = text.as_str();
        while let Some(at) = rest.find(".route(") {
            rest = rest[at + ".route(".len()..].trim_start();
            let end = rest.find([',', ')']).unwrap_or(rest.len());
            let path = rest[..end].trim().trim_matches('"');
            if rest.starts_with('"') || rest.starts_with("crate::") {
                found.push(path.to_owned());
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_route_is_registered_and_none_is_lost() {
    let frozen = std::fs::read_to_string(root().join("tests/it/frozen/routes.txt")).unwrap().replace("\r\n", "\n");
    let frozen: Vec<String> = frozen.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned).collect();
    let mut sorted = frozen.clone();
    sorted.sort();
    assert_eq!(frozen, sorted, "frozen/routes.txt must be sorted");

    // A path may be registered by two calls with different methods (`/api/threads/{id}/reach`);
    // axum merges those, so each call is one line.
    let found = routes_in_source();
    let missing: Vec<_> = frozen.iter().filter(|p| !found.contains(p)).collect();
    let extra: Vec<_> = found.iter().filter(|p| !frozen.contains(p)).collect();
    assert!(missing.is_empty(), "routes gone from the source: {missing:?}");
    assert!(extra.is_empty(), "routes not in frozen/routes.txt (add them, sorted): {extra:?}");
    assert_eq!(found, frozen, "a route is registered a different number of times than frozen/routes.txt says");
    assert_eq!(found.len(), 343, "343 `.route(` calls today");
}
