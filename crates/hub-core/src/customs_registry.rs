//! Customs' registry: the package registries as agents see them, with only what has cleared.
//! npm (`/registry/npm/`), crates.io's sparse index (`/registry/crates/`) and PyPI's simple
//! index (`/registry/pypi/simple/`) are fetched from upstream and filtered:
//!
//! - A package that has not cleared is not there: the package manager fails before anything is
//!   downloaded or run. It is put on the list as held, and the review looks at it, so a green
//!   one installs on the next try.
//! - What cleared packages depend on is let through, since a package cannot be used without its
//!   dependencies. Their names are learnt from the cleared package's metadata as it is served,
//!   which package managers fetch before its dependencies'.
//! - Versions published less than a week ago are left out: hijacked releases are usually found
//!   and pulled within days.
//!
//! Downloads still come from the registries themselves (the metadata keeps their URLs and
//! checksums), so nothing large passes through Hub. Engines are pointed here by environment
//! variables when they start (`registry_address::env`).

use crate::locked::ResultExt;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::customs_review::iso_ms;
use crate::hub::Hub;
use crate::model::{CustomsStatus, now_ms};
use crate::registry_address::base;

/// Versions younger than this are left out.
const MIN_AGE_MS: i64 = 7 * 86_400_000;
const DEPS: &str = "customs.deps";

pub fn router() -> Router<Arc<Hub>> {
    Router::new()
        .route("/registry/npm/{*name}", get(npm))
        .route("/registry/crates/config.json", get(crates_config))
        .route("/registry/crates/{*path}", get(crates_index))
        .route("/registry/pypi/simple/{name}/", get(pypi))
        .route("/registry/pypi/simple/{name}", get(pypi))
}

/// Names cleared packages depend on, by ecosystem, kept so a restart does not forget them.
static LEARNT: LazyLock<Mutex<Option<BTreeMap<String, BTreeSet<String>>>>> = LazyLock::new(Default::default);

async fn learnt(hub: &Hub, eco: &str, name: &str) -> bool {
    let mut guard = LEARNT.lock().await;
    if guard.is_none() {
        let stored = hub.store.get_value(DEPS).await.ok().flatten();
        *guard = Some(stored.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default());
    }
    guard.as_ref().is_some_and(|m| m.get(eco).is_some_and(|s| s.contains(name)))
}

async fn learn(hub: &Hub, eco: &str, names: impl IntoIterator<Item = String>) {
    let mut guard = LEARNT.lock().await;
    let map = guard.get_or_insert_with(Default::default);
    let set = map.entry(eco.to_owned()).or_default();
    let before = set.len();
    set.extend(names);
    if set.len() != before
        && let Ok(json) = serde_json::to_string(map)
    {
        hub.store.set_value(DEPS, &json).await.warn_on_err("save DEPS");
    }
}

/// Whether a package may be served: it has cleared, or a cleared package depends on it and it
/// was not refused.
async fn allowed(hub: &Arc<Hub>, kinds: &[&str], eco: &str, name: &str) -> Result<bool> {
    let list = crate::customs::list(&hub.ctx()).await?;
    let mut status = None;
    for kind in kinds {
        let n = crate::customs::normalize(kind, name);
        if let Some(m) = list.iter().find(|m| m.kind == *kind && m.name == n) {
            if m.status == CustomsStatus::Cleared {
                return Ok(true);
            }
            status = Some(m.status);
        }
    }
    if status == Some(CustomsStatus::Refused) {
        return Ok(false);
    }
    Ok(learnt(hub, eco, &crate::customs::normalize(kinds[0], name)).await)
}

/// What has not cleared goes through Customs before it is served: it goes on the list and is
/// reviewed while the package manager waits, as long as that one waits (`wait`), and a stamp
/// serves it then and there. What is still held or refused is turned away in words the
/// package manager shows; a held one is still being reviewed, or waits for the user.
async fn through(hub: &Arc<Hub>, kinds: &[&str], eco: &str, name: &str, wait: Duration) -> Result<(), Response> {
    match allowed(hub, kinds, eco, name).await {
        Ok(true) => return Ok(()),
        Ok(false) => {}
        Err(e) => return Err(upstream_failed(e)),
    }
    let kind = kinds[0];
    let known = crate::customs::get(&hub.ctx(), kind, name).await.ok().flatten();
    if known.as_ref().is_some_and(|m| m.status == CustomsStatus::Refused) {
        return Err(turned_away(kind, name, CustomsStatus::Refused));
    }
    if known.is_none() {
        crate::customs::request(&hub.ctx(), kind, name, "", "", "registry", None).await.warn_on_err("record the registry request");
        crate::customs::change(&hub.ctx(), kind, name, |m| {
            m.decided_by = "found".into();
            m.reason = "Asked for through Hub's registry.".into();
        })
        .await.warn_on_err("mark the registry request found");
    }
    let review = {
        let (hub, kind, name) = (hub.clone(), kind.to_owned(), name.to_owned());
        tokio::spawn(async move { crate::customs_review::settle(&hub, &kind, &name).await })
    };
    let status = match tokio::time::timeout(wait, review).await {
        Ok(Ok(Ok(Some(m)))) => m.status,
        _ => CustomsStatus::Held,
    };
    if status == CustomsStatus::Cleared { Ok(()) } else { Err(turned_away(kind, name, status)) }
}

fn turned_away(kind: &str, name: &str, status: CustomsStatus) -> Response {
    let why = if status == CustomsStatus::Refused {
        format!("Hub Customs: {kind} package \"{name}\" is refused, so it is not served. Hub's Settings > Customs says why.")
    } else {
        format!(
            "Hub Customs: {kind} package \"{name}\" has not cleared yet. It is being reviewed, or waits for the user \
             to clear it in Hub's Settings > Customs; try again in a minute."
        )
    };
    (StatusCode::NOT_FOUND, Json(json!({ "error": why }))).into_response()
}

/// How long each package manager waits for an answer, about: npm waits minutes, cargo half a
/// minute, pip a quarter of one.
const NPM_WAIT: Duration = Duration::from_secs(90);
const CRATES_WAIT: Duration = Duration::from_secs(20);
const PYPI_WAIT: Duration = Duration::from_secs(10);

fn http() -> reqwest::Client {
    static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
        reqwest::Client::builder()
            .user_agent(concat!("hub-customs/", env!("CARGO_PKG_VERSION"), " (https://github.com/KarlssonChristoffer/hub)"))
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_default()
    });
    CLIENT.clone()
}

fn upstream_failed(e: impl std::fmt::Display) -> Response {
    (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Hub Customs could not reach the registry: {e}") }))).into_response()
}

// npm ----------------------------------------------------------------------------------------

async fn npm(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> Response {
    // Scoped names come as `@scope%2fname` or `@scope/name`; the rest of npm's API is not served.
    let name = name.replace("%2f", "/").replace("%2F", "/");
    if name.starts_with("-/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    // A tarball: npm rewrites the registry's own download links to here. What it fetches was
    // resolved already (from metadata served here, or a lockfile's pinned version and checksum),
    // so only a refused package is turned away; the rest goes to the registry itself.
    if let Some((package, file)) = name.split_once("/-/") {
        let refused = crate::customs::get(&hub.ctx(), "npm", package).await.ok().flatten().is_some_and(|m| m.status == CustomsStatus::Refused);
        if refused {
            return turned_away("npm", package, CustomsStatus::Refused);
        }
        let to = format!("https://registry.npmjs.org/{package}/-/{file}");
        return (StatusCode::FOUND, [(header::LOCATION, to)]).into_response();
    }
    if !crate::customs::prefs(&hub.ctx()).await.is_ok_and(|p| p.registry) {
        return pass_npm(&name).await;
    }
    if let Err(away) = through(&hub, &["npm", "cdn"], "npm", &name, NPM_WAIT).await {
        return away;
    }
    let url = format!("https://registry.npmjs.org/{}", name.replace('/', "%2f"));
    let doc: Value = match http().get(&url).send().await.and_then(|r| r.error_for_status()) {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => return upstream_failed(e),
        },
        Err(e) if e.status() == Some(reqwest::StatusCode::NOT_FOUND) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return upstream_failed(e),
    };
    let (doc, deps) = filter_npm(doc, now_ms());
    learn(&hub, "npm", deps).await;
    Json(doc).into_response()
}

/// With the registry switched off, npm gets npm's answer as it is.
async fn pass_npm(name: &str) -> Response {
    let url = format!("https://registry.npmjs.org/{}", name.replace('/', "%2f"));
    match http().get(&url).send().await {
        Ok(r) => {
            let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            match r.bytes().await {
                Ok(body) => (status, [(header::CONTENT_TYPE, "application/json")], body).into_response(),
                Err(e) => upstream_failed(e),
            }
        }
        Err(e) => upstream_failed(e),
    }
}

/// A packument without versions younger than a week, `latest` moved back when it was one of
/// them, and the names its versions depend on.
pub fn filter_npm(mut doc: Value, now: i64) -> (Value, BTreeSet<String>) {
    let times = doc["time"].clone();
    let young = |v: &str| times[v].as_str().and_then(iso_ms).is_some_and(|t| now - t < MIN_AGE_MS);
    let mut deps = BTreeSet::new();
    if let Some(versions) = doc["versions"].as_object_mut() {
        versions.retain(|v, _| !young(v));
        for meta in versions.values() {
            for table in ["dependencies", "optionalDependencies", "peerDependencies"] {
                deps.extend(meta[table].as_object().into_iter().flatten().map(|(n, _)| n.clone()));
            }
        }
    }
    let kept: Vec<String> = doc["versions"].as_object().map(|v| v.keys().cloned().collect()).unwrap_or_default();
    let newest = kept.iter().max_by_key(|v| times[v.as_str()].as_str().and_then(iso_ms).unwrap_or(0)).cloned();
    if let Some(tags) = doc["dist-tags"].as_object_mut() {
        tags.retain(|_, v| v.as_str().is_some_and(|v| kept.iter().any(|k| k == v)));
        if !tags.contains_key("latest")
            && let Some(newest) = newest
        {
            tags.insert("latest".into(), json!(newest));
        }
    }
    (doc, deps)
}

// crates.io ----------------------------------------------------------------------------------

async fn crates_config(_headers: HeaderMap, State(hub): State<Arc<Hub>>) -> Response {
    Json(json!({ "dl": "https://static.crates.io/crates", "api": "https://crates.io", "auth-required": false, "hub": base(&hub.config) }))
        .into_response()
}

async fn crates_index(State(hub): State<Arc<Hub>>, Path(path): Path<String>) -> Response {
    let Some(name) = path.rsplit('/').next().map(str::to_lowercase).filter(|n| !n.is_empty()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let on = crate::customs::prefs(&hub.ctx()).await.is_ok_and(|p| p.registry);
    if on
        && let Err(away) = through(&hub, &["crates"], "crates", &name, CRATES_WAIT).await {
            return away;
        }
    let text = match http().get(format!("https://index.crates.io/{path}")).send().await.and_then(|r| r.error_for_status()) {
        Ok(r) => match r.text().await {
            Ok(t) => t,
            Err(e) => return upstream_failed(e),
        },
        Err(e) if e.status() == Some(reqwest::StatusCode::NOT_FOUND) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return upstream_failed(e),
    };
    if !on {
        return text.into_response();
    }
    let (text, deps) = filter_crates(&text, now_ms());
    learn(&hub, "crates", deps).await;
    text.into_response()
}

/// An index file without versions younger than a week (where the index says when they were
/// published), and the crates its versions depend on.
pub fn filter_crates(text: &str, now: i64) -> (String, BTreeSet<String>) {
    let mut out = String::new();
    let mut deps = BTreeSet::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v["pubtime"].as_str().and_then(iso_ms).is_some_and(|t| now - t < MIN_AGE_MS) {
            continue;
        }
        for d in v["deps"].as_array().into_iter().flatten() {
            if let Some(n) = d["package"].as_str().or(d["name"].as_str()) {
                deps.insert(n.to_lowercase());
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    (out, deps)
}

// PyPI ---------------------------------------------------------------------------------------

async fn pypi(State(hub): State<Arc<Hub>>, Path(name): Path<String>) -> Response {
    let name = crate::customs::normalize("pypi", &name);
    let on = crate::customs::prefs(&hub.ctx()).await.is_ok_and(|p| p.registry);
    if on
        && let Err(away) = through(&hub, &["pypi"], "pypi", &name, PYPI_WAIT).await {
            return away;
        }
    let simple = http()
        .get(format!("https://pypi.org/simple/{name}/"))
        .header(header::ACCEPT, "application/vnd.pypi.simple.v1+json")
        .send()
        .await
        .and_then(|r| r.error_for_status());
    let doc: Value = match simple {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => return upstream_failed(e),
        },
        Err(e) if e.status() == Some(reqwest::StatusCode::NOT_FOUND) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return upstream_failed(e),
    };
    let doc = if on { filter_pypi(doc, now_ms()) } else { doc };
    if on {
        // The simple index does not say what a project depends on; its JSON page does.
        if let Ok(r) = http().get(format!("https://pypi.org/pypi/{name}/json")).send().await
            && let Ok(meta) = r.json::<Value>().await
        {
            learn(&hub, "pypi", requires(&meta)).await;
        }
    }
    ([(header::CONTENT_TYPE, "application/vnd.pypi.simple.v1+json")], Json(doc)).into_response()
}

/// A simple-index page without files uploaded less than a week ago.
pub fn filter_pypi(mut doc: Value, now: i64) -> Value {
    if let Some(files) = doc["files"].as_array_mut() {
        files.retain(|f| f["upload-time"].as_str().and_then(iso_ms).is_none_or(|t| now - t >= MIN_AGE_MS));
    }
    doc
}

/// The project names in a PyPI JSON page's `requires_dist`.
pub fn requires(meta: &Value) -> BTreeSet<String> {
    meta["info"]["requires_dist"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r.as_str())
        .filter_map(|r| {
            let end = r.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))).unwrap_or(r.len());
            (end > 0).then(|| crate::customs::normalize("pypi", &r[..end]))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_000_000_000;

    fn at(days_ago: i64) -> String {
        // Midnight-aligned is enough here: iso_ms only needs the shape.
        let ms = NOW - days_ago * 86_400_000;
        let days = ms.div_euclid(86_400_000);
        // Back from days to a civil date (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!("{y:04}-{m:02}-{d:02}T00:00:00.000Z")
    }

    #[test]
    fn npm_leaves_out_young_versions_and_learns_dependencies() {
        let doc = json!({
            "name": "x",
            "dist-tags": { "latest": "2.0.0", "next": "3.0.0-beta" },
            "time": { "1.0.0": at(400), "2.0.0": at(2), "3.0.0-beta": at(1) },
            "versions": {
                "1.0.0": { "dependencies": { "a": "^1" }, "peerDependencies": { "react": "*" } },
                "2.0.0": { "dependencies": { "evil": "^1" } },
                "3.0.0-beta": {}
            }
        });
        let (doc, deps) = filter_npm(doc, NOW);
        assert_eq!(doc["versions"].as_object().unwrap().keys().collect::<Vec<_>>(), vec!["1.0.0"]);
        assert_eq!(doc["dist-tags"], json!({ "latest": "1.0.0" }));
        assert_eq!(deps.into_iter().collect::<Vec<_>>(), vec!["a", "react"]);
    }

    #[test]
    fn crates_index_drops_young_versions() {
        let text = format!(
            "{}\n{}\n",
            json!({ "name": "x", "vers": "1.0.0", "deps": [{ "name": "serde" }, { "name": "rq", "package": "Reqwest" }], "pubtime": at(100) }),
            json!({ "name": "x", "vers": "1.1.0", "deps": [{ "name": "evil" }], "pubtime": at(3) }),
        );
        let (out, deps) = filter_crates(&text, NOW);
        assert_eq!(out.lines().count(), 1);
        assert!(out.contains("\"1.0.0\""));
        assert_eq!(deps.into_iter().collect::<Vec<_>>(), vec!["reqwest", "serde"]);
        // Old index lines say nothing about when; they stay.
        let (old, _) = filter_crates(&json!({ "name": "x", "vers": "0.1.0", "deps": [] }).to_string(), NOW);
        assert_eq!(old.lines().count(), 1);
    }

    #[test]
    fn pypi_drops_young_files_and_reads_requirements() {
        let doc = json!({ "files": [{ "filename": "a-1.whl", "upload-time": at(300) }, { "filename": "a-2.whl", "upload-time": at(1) }] });
        assert_eq!(filter_pypi(doc, NOW)["files"].as_array().unwrap().len(), 1);
        let meta = json!({ "info": { "requires_dist": ["urllib3<3,>=1.21.1", "charset_normalizer<4,>=2", "PySocks!=1.5.7; extra == \"socks\""] } });
        assert_eq!(requires(&meta).into_iter().collect::<Vec<_>>(), vec!["charset-normalizer", "pysocks", "urllib3"]);
    }
}
