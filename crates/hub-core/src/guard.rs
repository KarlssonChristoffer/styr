//! The API's door (`check`, an axum middleware over the whole router): which caller gets in,
//! with which grant. Browsers are told apart by their `Origin` header, which a web page cannot
//! forge; everything else shows the connection token (`auth.rs`, which keeps the token and the
//! plain helpers the rest of Hub reads without reaching this door).

use crate::locked::{LockExt, ResultExt};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::auth::{AGENTS, GATEWAY, NARROW_HEADER, Narrow, OAUTH_CALLBACK, TOKEN_PARAM, fresh, same, token, tool_token};
use crate::hub::Hub;
use crate::pairing::Grant;

/// Paths anyone may call: health says only that this is Hub and which version, and redeeming a
/// pairing code is how a device gets a token, so it cannot need one. A service that logged you in
/// sends the browser back with a one-time `state` only Hub made (`OAUTH_CALLBACK`).
const OPEN: &[&str] = &["/api/health", "/api/pair/redeem", crate::promptlink::PATH, OAUTH_CALLBACK];

/// An agent's page (`pages.rs`), which the app loads in a frame and the browser opens, neither
/// of which can show a token. Its two random ids are what lets one in, and a page is all it gives.
fn is_page(request: &Request) -> bool {
    let rest = request.uri().path().strip_prefix("/api/threads/");
    request.method() == axum::http::Method::GET
        && rest.and_then(|r| r.split_once("/pages/")).is_some_and(|(t, p)| !t.contains('/') && !p.is_empty() && !p.contains('/'))
}

#[derive(Clone)]
pub struct Guard {
    hub: Arc<Hub>,
    token: Arc<str>,
    /// Threads already told that their engine was refused Hub's tools.
    told: Arc<Mutex<HashSet<String>>>,
    /// The app's windows show the token (`core_token` in the desktop shell), so its origin alone
    /// no longer lets anyone in (`WINDOW_SHOWS_TOKEN`).
    strict: Arc<AtomicBool>,
}

/// Noted in the data folder the first time a window of the app shows the connection token with
/// its origin. From then on the app's `Origin` alone is refused: any program on this machine can
/// send that header (`curl -H "Origin: tauri://localhost"`), so only a shell that could not read
/// the token yet (one from before `core_token`) was let in by it. Removing the file opens that
/// way again, which takes the same step as reading the token file itself.
pub const WINDOW_SHOWS_TOKEN: &str = "token.window";

impl Guard {
    pub fn new(hub: Arc<Hub>) -> Self {
        let token = token(&hub.config.data_dir).unwrap_or_else(|err| {
            // Browsers still work; other clients cannot get in until the file can be written.
            tracing::error!("no connection token: {err:#}");
            fresh()
        });
        let strict = hub.config.data_dir.join(WINDOW_SHOWS_TOKEN).exists();
        Self { hub, token: token.into(), told: Default::default(), strict: Arc::new(AtomicBool::new(strict)) }
    }

    /// A window showed the token with the app's origin: its origin alone is not enough any more.
    fn window_showed_token(&self) {
        if self.strict.swap(true, Ordering::SeqCst) {
            return;
        }
        let note = "The app's windows show the connection token; its origin alone no longer lets a program in.\n";
        std::fs::write(self.hub.config.data_dir.join(WINDOW_SHOWS_TOKEN), note).warn_on_err("write token.window");
        tracing::info!("the app's windows show the connection token; its origin alone is refused from now on");
    }
}

pub async fn check(State(guard): State<Guard>, mut request: Request, next: Next) -> Response {
    let tailnet = crate::phone::via_tailnet(&request);
    // The phone page holds nothing secret, but only the tailnet and this machine are shown it.
    if crate::phone::is_page(request.uri().path()) {
        return if tailnet { next.run(request).await } else { StatusCode::NOT_FOUND.into_response() };
    }
    // Redeeming a phone's code needs to know (`api::redeem`).
    request.extensions_mut().insert(crate::phone::Tailnet(tailnet));
    let sender = crate::phone::Sender(crate::phone::sender(&request));
    request.extensions_mut().insert(sender);
    // Agents outside Hub show their own key there, checked by the handler (`api::agent_mcp`). No
    // web page may, so a browser is still told apart by its origin.
    let agents = request.uri().path() == AGENTS
        && request.headers().get(header::ORIGIN).is_none_or(|o| o.to_str().is_ok_and(|o| origin_allowed(&guard.hub.config, o)));
    // Customs' registry is as open as the registries it stands for: package managers bring no token.
    let registry = request.uri().path().starts_with(crate::registry_address::PREFIX);
    // A Sentry SDK brings its DSN's key, checked by the handler (`problems::ingest`).
    let ingest = crate::problems::is_ingest(request.uri().path());
    // The gateway is called by the code a thread runs, which shows the key in its address: one
    // connection for one project, checked by the handler (`gateway::forward`). No web page may.
    let gateway = request.uri().path().starts_with(GATEWAY) && request.headers().get(header::ORIGIN).is_none();
    let open = OPEN.contains(&request.uri().path()) || agents || registry || ingest || gateway || is_page(&request);
    // Read what the request shows first: a request cannot be held across an await.
    let shown = shown(&guard, &request);
    let grant = if open { Some(Grant::Full) } else { allowed(&guard, shown, request.uri().path()).await };
    match grant {
        Some(Grant::Full) => {
            if request.headers().contains_key(NARROW_HEADER) {
                request.extensions_mut().insert(Narrow);
            }
            return next.run(request).await;
        }
        Some(Grant::Phone) if tailnet && crate::phone::may(request.method(), request.uri().path()) => {
            request.extensions_mut().insert(Narrow);
            return next.run(request).await;
        }
        Some(Grant::Phone) => {
            return (StatusCode::FORBIDDEN, "a phone may only follow and answer threads, from the tailnet").into_response();
        }
        None => {}
    }
    if let Some(thread) = request.uri().path().strip_prefix("/mcp/") {
        refused_tools(&guard, thread).await;
    }
    (StatusCode::UNAUTHORIZED, "a connection token is needed: pair with `hubd pair` on this machine, or use the token file in Hub's data folder")
        .into_response()
}

/// An engine was turned away from Hub's tools. It only says "failed to start" and points at a
/// command the user cannot run from Hub, so the thread says why, once.
async fn refused_tools(guard: &Guard, thread: &str) {
    if !guard.told.locked().insert(thread.to_owned()) {
        return;
    }
    tracing::warn!(thread, "an engine was refused Hub's tools: it showed no or the wrong connection token");
    let message = "Hub's tools (MCP server \"hub\") refused this engine: it showed no or the wrong connection token. \
                   Restart the thread's engine; if it keeps happening, the token file in Hub's data folder may have changed.";
    guard.hub.emit_typed(thread, &crate::model::ErrorData::new(message)).await.warn_on_err("emit Error");
}

/// The app's own origins, and the dev window's, which is served from the dev mode clone (`dev.rs`).
pub fn origin_allowed(config: &crate::Config, origin: &str) -> bool {
    config.allowed_origins.iter().any(|a| a == origin) || crate::dev::ORIGINS.contains(&origin)
}

async fn allowed(guard: &Guard, shown: Shown, path: &str) -> Option<Grant> {
    let (given, app) = match shown {
        // The app's origin without a token: only while no window has shown one.
        Shown::Origin(ok) => return (ok && !guard.strict.load(Ordering::SeqCst)).then_some(Grant::Full),
        Shown::Token(token, app) => (token, app),
        Shown::Nothing => return None,
    };
    // This machine's own token, or one a paired device got for itself.
    if same(&given, &guard.token) {
        if app {
            guard.window_showed_token();
        }
        return Some(Grant::Full);
    }
    // A thread's engine at its own tools, and nowhere else.
    if let Some(thread) = path.strip_prefix("/mcp/")
        && !thread.is_empty()
        && !thread.contains('/')
        && same(&given, &tool_token(&guard.token, thread))
    {
        return Some(Grant::Full);
    }
    let grant = guard.hub.pairing.grant(&guard.hub.store, &given).await;
    // A window with a stale token still gets in by its origin, while that is enough.
    grant.or((app && !guard.strict.load(Ordering::SeqCst)).then_some(Grant::Full))
}

enum Shown {
    Origin(bool),
    /// A token, and whether it came with the app's own origin from this machine.
    Token(String, bool),
    Nothing,
}

fn shown(guard: &Guard, request: &Request) -> Shown {
    let headers = request.headers();
    if let Some(origin) = headers.get(header::ORIGIN) {
        // A browser. Its own origin decides, never a token it might have been handed; only the
        // phone page this hubd serves shows the token it was given.
        let app = origin.to_str().is_ok_and(|o| origin_allowed(&guard.hub.config, o));
        // The app's origin only proves something on this machine: anything `tailscale serve` (or
        // another proxy) brings in can set any Origin it likes, so from there the app shows a
        // token like every other client.
        if app && crate::phone::from_elsewhere(request) {
            // Falls through to the token below.
        } else if app {
            // A window that shows the token is checked by it (`window_showed_token`).
            return given(request).map_or(Shown::Origin(true), |token| Shown::Token(token, true));
        } else if !(crate::phone::own_page(headers) || crate::phone::sibling_page(headers)) {
            return Shown::Origin(false);
        }
    }
    given(request).map_or(Shown::Nothing, |token| Shown::Token(token, false))
}

/// The token a request shows, as a bearer token or as `?token=`. The address form is only taken
/// where a header cannot be sent: a WebSocket (a browser cannot set one) and Hub's tools for an
/// engine that drops MCP headers (`mcp_token_in_url`). Anywhere else a token in an address only
/// ends up in histories and screenshots.
fn given(request: &Request) -> Option<String> {
    let bearer = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let query = request.uri().query().filter(|_| token_in_address(request.uri().path())).unwrap_or_default().split('&').find_map(|pair| {
        pair.strip_prefix(TOKEN_PARAM).and_then(|rest| rest.strip_prefix('='))
    });
    bearer.or(query).map(|token| token.trim().to_owned())
}

/// Whether `path` may carry its token as `?token=`: the event stream, a terminal's socket, Hub's tools.
fn token_in_address(path: &str) -> bool {
    path.ends_with("/api/stream") || path.ends_with("/ws") || path.starts_with("/mcp/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::get;
    use tower::ServiceExt;

    async fn status(request: axum::http::Request<Body>) -> StatusCode {
        let data = tempfile::tempdir().unwrap();
        let config = crate::Config { data_dir: data.path().to_owned(), ..crate::Config::default() };
        let hub = Hub::new(crate::Store::in_memory().unwrap(), config);
        let guard = Guard { hub, token: "secret".into(), told: Default::default(), strict: Default::default() };
        let app = axum::Router::new()
            .route("/api/health", get(|| async { "ok" }))
            .route("/api/threads", get(|| async { "[]" }))
            .layer(axum::middleware::from_fn_with_state(guard, check));
        app.oneshot(request).await.unwrap().status()
    }

    fn get_req(uri: &str) -> axum::http::request::Builder {
        axum::http::Request::get(uri)
    }

    #[tokio::test]
    async fn the_app_gets_in_by_its_origin() {
        let req = get_req("/api/threads").header("origin", "tauri://localhost").body(Body::empty()).unwrap();
        assert_eq!(status(req).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn the_apps_origin_is_not_enough_through_tailscale_serve() {
        // `tailscale serve` connects from loopback and names the caller in X-Forwarded-For.
        let forwarded = |token: Option<&str>| {
            let mut req = get_req("/api/threads").header("origin", "tauri://localhost").header("x-forwarded-for", "100.100.1.1");
            if let Some(token) = token {
                req = req.header("authorization", format!("Bearer {token}"));
            }
            let mut req = req.body(Body::empty()).unwrap();
            req.extensions_mut().insert(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 50000))));
            req
        };
        assert_eq!(status(forwarded(None)).await, StatusCode::UNAUTHORIZED);
        assert_eq!(status(forwarded(Some("wrong"))).await, StatusCode::UNAUTHORIZED);
        assert_eq!(status(forwarded(Some("secret"))).await, StatusCode::OK);
        // A caller that is not on this machine at all.
        let mut far = get_req("/api/threads").header("origin", "tauri://localhost").body(Body::empty()).unwrap();
        far.extensions_mut().insert(axum::extract::ConnectInfo(std::net::SocketAddr::from(([100, 100, 1, 1], 50000))));
        assert_eq!(status(far).await, StatusCode::UNAUTHORIZED);
        // The app on this machine still gets in by its origin alone.
        let mut local = get_req("/api/threads").header("origin", "tauri://localhost").body(Body::empty()).unwrap();
        local.extensions_mut().insert(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 50000))));
        assert_eq!(status(local).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn other_web_pages_do_not_even_with_the_token() {
        let req = get_req("/api/threads")
            .header("origin", "https://evil.example")
            .header("authorization", "Bearer secret")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status(req).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn programs_need_the_token() {
        assert_eq!(status(get_req("/api/threads").body(Body::empty()).unwrap()).await, StatusCode::UNAUTHORIZED);
        let bearer = get_req("/api/threads").header("authorization", "Bearer secret").body(Body::empty()).unwrap();
        assert_eq!(status(bearer).await, StatusCode::OK);
        // An address carries the token only where no header can: sockets and Hub's tools.
        let query = get_req("/api/threads?token=secret").body(Body::empty()).unwrap();
        assert_eq!(status(query).await, StatusCode::UNAUTHORIZED);
        assert!(token_in_address("/api/stream") && token_in_address("/api/terminals/t1/ws") && token_in_address("/mcp/t1"));
        assert!(!token_in_address("/api/threads") && !token_in_address("/api/prefs/config"));
        let wrong = get_req("/api/threads?token=secreT").body(Body::empty()).unwrap();
        assert_eq!(status(wrong).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn once_a_window_shows_the_token_the_apps_origin_alone_is_not_enough() {
        let data = tempfile::tempdir().unwrap();
        let config = crate::Config { data_dir: data.path().to_owned(), ..crate::Config::default() };
        let app = |config: crate::Config| {
            let hub = Hub::new(crate::Store::in_memory().unwrap(), config);
            let guard = Guard::new(hub);
            axum::Router::new()
                .route("/api/threads", get(|| async { "[]" }))
                .route("/api/stream", get(|| async { "[]" }))
                .layer(axum::middleware::from_fn_with_state(guard, check))
        };
        let token = token(data.path()).unwrap();
        let req = |token: Option<&str>| {
            let mut req = get_req("/api/threads").header("origin", "tauri://localhost");
            if let Some(token) = token {
                req = req.header("authorization", format!("Bearer {token}"));
            }
            req.body(Body::empty()).unwrap()
        };
        let first = app(config.clone());
        // A shell from before `core_token`: its origin, or a stale token, still gets in.
        assert_eq!(first.clone().oneshot(req(None)).await.unwrap().status(), StatusCode::OK);
        assert_eq!(first.clone().oneshot(req(Some("stale"))).await.unwrap().status(), StatusCode::OK);
        // A window shows the token: from now on, and after a restart, the origin alone (what
        // `curl -H "Origin: tauri://localhost"` sends) is refused, and so is a wrong token.
        assert_eq!(first.clone().oneshot(req(Some(&token))).await.unwrap().status(), StatusCode::OK);
        assert_eq!(first.clone().oneshot(req(None)).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        assert_eq!(first.clone().oneshot(req(Some("stale"))).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        // A WebSocket shows it as `?token=`; through `tailscale serve` the origin never counts.
        let socket = get_req(&format!("/api/stream?{TOKEN_PARAM}={token}")).header("origin", "tauri://localhost").body(Body::empty()).unwrap();
        assert_eq!(first.clone().oneshot(socket).await.unwrap().status(), StatusCode::OK);
        let proxied = get_req("/api/threads").header("origin", "tauri://localhost").header("x-forwarded-for", "100.100.1.1").body(Body::empty()).unwrap();
        assert_eq!(first.clone().oneshot(proxied).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        let restarted = app(config);
        assert_eq!(restarted.clone().oneshot(req(None)).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        assert_eq!(restarted.oneshot(req(Some(&token))).await.unwrap().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_phone_and_what_a_hub_forwards_for_one_are_narrow() {
        let data = tempfile::tempdir().unwrap();
        let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
        let hub = Hub::new(crate::Store::in_memory().unwrap(), config);
        let code = hub.pairing.phone_code().code;
        let phone = hub.pairing.redeem(&hub.store, &code, "phone").await.unwrap();
        let guard = Guard { hub, token: "secret".into(), told: Default::default(), strict: Default::default() };
        let narrow = |n: Option<axum::Extension<Narrow>>| async move { if n.is_some() { "narrow" } else { "full" } };
        let app = axum::Router::new().route("/api/threads", get(narrow)).layer(axum::middleware::from_fn_with_state(guard, check));
        let send = |token: &str, header: bool| {
            let mut req = get_req("/api/threads").header("authorization", format!("Bearer {token}"));
            if header {
                req = req.header(NARROW_HEADER, "phone");
            }
            let mut req = req.body(Body::empty()).unwrap();
            req.extensions_mut().insert(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 50000))));
            req
        };
        let body = |r: axum::response::Response| async move { String::from_utf8(axum::body::to_bytes(r.into_body(), 64).await.unwrap().to_vec()).unwrap() };
        assert_eq!(body(app.clone().oneshot(send("secret", false)).await.unwrap()).await, "full");
        assert_eq!(body(app.clone().oneshot(send("secret", true)).await.unwrap()).await, "narrow");
        assert_eq!(body(app.oneshot(send(&phone, false)).await.unwrap()).await, "narrow");
    }

    #[tokio::test]
    async fn a_paired_device_gets_in_with_its_own_token() {
        let data = tempfile::tempdir().unwrap();
        let config = crate::Config { data_dir: data.path().to_owned(), ..crate::Config::default() };
        let hub = Hub::new(crate::Store::in_memory().unwrap(), config);
        let code = hub.pairing.code(None).code;
        let device = hub.pairing.redeem(&hub.store, &code, "phone").await.unwrap();
        let guard = Guard { hub, token: "secret".into(), told: Default::default(), strict: Default::default() };
        let app = axum::Router::new()
            .route("/api/threads", get(|| async { "[]" }))
            .layer(axum::middleware::from_fn_with_state(guard, check));
        let req = |token: &str| get_req("/api/threads").header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap();
        assert_eq!(app.clone().oneshot(req(&device)).await.unwrap().status(), StatusCode::OK);
        assert_eq!(app.oneshot(req("not-paired")).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_phone_gets_its_few_calls_from_its_own_page_on_the_tailnet() {
        let data = tempfile::tempdir().unwrap();
        let config = crate::Config { data_dir: data.path().to_owned(), ..crate::Config::default() };
        let hub = Hub::new(crate::Store::in_memory().unwrap(), config);
        let code = hub.pairing.phone_code().code;
        let phone = hub.pairing.redeem(&hub.store, &code, "phone").await.unwrap();
        let guard = Guard { hub, token: "secret".into(), told: Default::default(), strict: Default::default() };
        let app = axum::Router::new()
            .route("/api/threads", get(|| async { "[]" }))
            .route("/api/secrets", get(|| async { "[]" }))
            .route("/phone", get(crate::phone::serve))
            .layer(axum::middleware::from_fn_with_state(guard, check));
        let peer = |ip: &str| axum::extract::ConnectInfo(std::net::SocketAddr::new(ip.parse().unwrap(), 50000));
        let send = |uri: &str, token: &str, ip: &str, forwarded: &str| {
            let mut req = get_req(uri)
                .header("origin", "https://pc.tail1.ts.net")
                .header("host", "pc.tail1.ts.net")
                .header("authorization", format!("Bearer {token}"));
            if !forwarded.is_empty() {
                req = req.header("x-forwarded-for", forwarded);
            }
            let mut req = req.body(Body::empty()).unwrap();
            req.extensions_mut().insert(peer(ip));
            app.clone().oneshot(req)
        };
        let ok = |r: Result<axum::response::Response, std::convert::Infallible>| r.unwrap().status();
        assert_eq!(ok(send("/api/threads", &phone, "127.0.0.1", "100.100.1.1").await), StatusCode::OK);
        assert_eq!(ok(send("/api/secrets", &phone, "127.0.0.1", "100.100.1.1").await), StatusCode::FORBIDDEN);
        assert_eq!(ok(send("/api/threads", &phone, "127.0.0.1", "203.0.113.4").await), StatusCode::FORBIDDEN);
        assert_eq!(ok(send("/api/threads", "made-up", "127.0.0.1", "100.100.1.1").await), StatusCode::UNAUTHORIZED);
        assert_eq!(ok(send("/phone", "", "127.0.0.1", "100.100.1.1").await), StatusCode::OK);
        assert_eq!(ok(send("/phone", "", "127.0.0.1", "203.0.113.4").await), StatusCode::NOT_FOUND);
        assert_eq!(ok(send("/phone", "", "192.168.1.9", "").await), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_engines_tool_token_opens_its_own_tools_only() {
        let data = tempfile::tempdir().unwrap();
        let config = crate::Config { data_dir: data.path().to_owned(), ..crate::Config::default() };
        let hub = Hub::new(crate::Store::in_memory().unwrap(), config);
        let guard = Guard { hub, token: "secret".into(), told: Default::default(), strict: Default::default() };
        let app = axum::Router::new()
            .route("/api/threads", get(|| async { "[]" }))
            .route("/api/secrets", get(|| async { "[]" }))
            .route("/mcp/{id}", axum::routing::post(|| async { "{}" }))
            .layer(axum::middleware::from_fn_with_state(guard, check));
        let tool = tool_token("secret", "t1");
        assert_ne!(tool, "secret");
        let post = |uri: &str, token: &str| {
            axum::http::Request::post(uri).header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap()
        };
        assert_eq!(app.clone().oneshot(post("/mcp/t1", &tool)).await.unwrap().status(), StatusCode::OK);
        assert_eq!(app.clone().oneshot(post("/mcp/t2", &tool)).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        let read = |uri: &str| get_req(uri).header("authorization", format!("Bearer {tool}")).body(Body::empty()).unwrap();
        assert_eq!(app.clone().oneshot(read("/api/threads")).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        assert_eq!(app.clone().oneshot(read("/api/secrets")).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        // An engine that drops headers (Hermes) shows the same token in the address.
        let bare = |uri: &str| axum::http::Request::post(uri).body(Body::empty()).unwrap();
        assert_eq!(app.clone().oneshot(bare(&format!("/mcp/t1?token={tool}"))).await.unwrap().status(), StatusCode::OK);
        assert_eq!(app.clone().oneshot(bare(&format!("/mcp/t2?token={tool}"))).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        // The connection token still works there, for scripts on this machine.
        assert_eq!(app.oneshot(post("/mcp/t1", "secret")).await.unwrap().status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_is_open() {
        assert_eq!(status(get_req("/api/health").body(Body::empty()).unwrap()).await, StatusCode::OK);
    }

}
