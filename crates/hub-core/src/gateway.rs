//! How API connections sign their calls, whatever way a service wants its key, and Hub's gateway
//! that lets threads call them without ever holding one.
//!
//! Each connection has an `ApiAuth`: a known service brings it in the catalog, and one you add
//! yourself starts as a Bearer key and is tried the other usual ways when the test is refused
//! (`Hub::probe`). So adding any API is pick (or name) it and paste what it gave you.
//!
//! A service that logs you in (`ApiAuth::Login`, OAuth with PKCE, like Spotify) needs only the
//! app's client ID: `Hub::api_login` makes the address of its Allow page, the service sends the
//! browser back to `CALLBACK` on this machine, and Hub keeps the refresh token in the vault and
//! renews the short-lived access token when a call needs it.
//!
//! Threads in a project a connection is on get `<NAME>_BASE_URL`, pointing at `/gw/<key>` on this
//! core. The key in the path stands for one connection in one project (kept in the store as
//! `apis.gateway`) and stops working when the project is turned off. The gateway puts the
//! service's address in front, signs the call, and counts it.

use crate::locked::LockExt;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use axum::extract::{Query, Request, State};
use axum::http::{HeaderName, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Digest;

use crate::apis::{ApiAuth, ApiConnection, ApiTest, CLIENT, Signing};
use crate::folders::same_folder;
use crate::hub::Hub;

/// Where threads call their connections, and where a service that logs you in sends the browser
/// back. They live in `auth.rs`; `guard.rs` lets both through.
pub use crate::auth::{GATEWAY as PREFIX, OAUTH_CALLBACK as CALLBACK};
const KEYS: &str = "apis.gateway";
/// How long an Allow page may stay open.
const LOGIN_WAIT: Duration = Duration::from_secs(15 * 60);
/// What a call through the gateway may take.
const CALL_WAIT: Duration = Duration::from_secs(120);
/// An access token this close to running out is renewed first.
const RENEW_BEFORE: Duration = Duration::from_secs(60);
const MAX_BODY: usize = 32 * 1024 * 1024;

/// The other usual ways a key is sent, tried in turn when a Bearer key is refused.
fn ways() -> Vec<ApiAuth> {
    let header = |n: &str| ApiAuth::Header { name: n.into() };
    let query = |n: &str| ApiAuth::Query { name: n.into() };
    vec![
        header("X-Api-Key"),
        header("Authorization"),
        header("api-key"),
        query("api_key"),
        query("apikey"),
        query("key"),
        query("appid"),
        query("token"),
        query("access_token"),
        ApiAuth::Basic,
    ]
}

/// Access tokens by connection, with when they run out. Only ever in memory.
static TOKENS: LazyLock<Mutex<BTreeMap<String, (String, Instant)>>> = LazyLock::new(Default::default);
/// One renewal at a time, so two calls don't both spend a refresh token.
static RENEW: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);
/// An Allow page open now: connection, PKCE verifier, opened.
type Pending = (String, String, Instant);
/// Allow pages open now, by their `state`.
static PENDING: LazyLock<Mutex<BTreeMap<String, Pending>>> = LazyLock::new(Default::default);
/// Read-change-write of the gateway keys, one at a time.
static KEYS_WRITE: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(Default::default);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GatewayKey {
    name: String,
    folder: String,
}

fn random() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sha2::Sha256::digest(verifier.as_bytes()))
}

fn forget_token(name: &str) {
    TOKENS.locked().remove(name);
}

/// What a token endpoint answered, or why not, in words.
async fn token_answer(sent: reqwest::Result<reqwest::Response>, label: &str) -> Result<Value> {
    let r = sent.with_context(|| format!("no answer from {label}'s login"))?;
    let status = r.status();
    let body: Value = r.json().await.unwrap_or_default();
    if !status.is_success() {
        let error = body["error"].as_str().unwrap_or_default();
        let said = body["error_description"].as_str().unwrap_or(error);
        if error == "invalid_grant" {
            bail!("{label} dropped the login ({said}). Press Allow again on its page in Settings > API connections.");
        }
        bail!("{label}'s login answered {status}: {said}");
    }
    Ok(body)
}

impl Hub {
    /// The address services that log you in send you back to. A service checks it letter for
    /// letter, so it is always the loopback address and this core's port.
    pub fn redirect_uri(&self) -> String {
        format!("http://127.0.0.1:{}{CALLBACK}", self.config.bind.port())
    }

    /// How a call to `c` shows `key` (for a login, the refresh token).
    pub(crate) async fn signing(&self, c: &ApiConnection, key: &str) -> Result<Signing> {
        let auth = "Authorization".to_owned();
        Ok(match &c.auth {
            ApiAuth::Bearer => Signing::Header(auth, format!("Bearer {key}")),
            ApiAuth::Header { name } => Signing::Header(name.clone(), key.to_owned()),
            ApiAuth::Query { name } => Signing::Query(name.clone(), key.to_owned()),
            ApiAuth::Basic => {
                let pair = if key.contains(':') { key.to_owned() } else { format!("{key}:") };
                Signing::Header(auth, format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(pair)))
            }
            ApiAuth::Login { token, .. } => Signing::Header(auth, format!("Bearer {}", self.access_token(c, key, token).await?)),
        })
    }

    /// A fresh access token for a login, renewed with the refresh token when it is about to run out.
    async fn access_token(&self, c: &ApiConnection, refresh: &str, token_url: &str) -> Result<String> {
        let fresh = |name: &str| {
            TOKENS.locked().get(name).filter(|(_, until)| *until > Instant::now() + RENEW_BEFORE).map(|(t, _)| t.clone())
        };
        if let Some(token) = fresh(&c.name) {
            return Ok(token);
        }
        let _one = RENEW.lock().await;
        if let Some(token) = fresh(&c.name) {
            return Ok(token);
        }
        let client_id = c.client_id.as_deref().map(str::trim).filter(|s| !s.is_empty()).with_context(|| format!("{} has no client ID", c.label))?;
        let sent = CLIENT
            .post(token_url)
            .form(&[("grant_type", "refresh_token"), ("refresh_token", refresh), ("client_id", client_id)])
            .timeout(Duration::from_secs(20))
            .send()
            .await;
        let body = token_answer(sent, &c.label).await?;
        self.keep_tokens(c, &body).await
    }

    /// Keeps what a token endpoint gave: the access token in memory, a new refresh token in the vault.
    async fn keep_tokens(&self, c: &ApiConnection, body: &Value) -> Result<String> {
        let access = body["access_token"].as_str().with_context(|| format!("{}'s login sent no access token", c.label))?;
        let lasts = Duration::from_secs(body["expires_in"].as_u64().unwrap_or(3600));
        if let Some(refresh) = body["refresh_token"].as_str() {
            let had = self.secrets.env(std::slice::from_ref(&c.secret)).await?.pop().map(|(_, v)| v);
            if had.as_deref() != Some(refresh) {
                self.secrets.set(&c.secret, refresh).await?;
            }
        }
        TOKENS.locked().insert(c.name.clone(), (access.to_owned(), Instant::now() + lasts));
        Ok(access.to_owned())
    }

    /// The address of the service's Allow page for a connection that logs you in.
    pub async fn api_login(&self, name: &str) -> Result<String> {
        let prefs = self.api_prefs().await?;
        let c = prefs.connections.iter().find(|c| c.name == name).with_context(|| format!("no connection called {name}"))?;
        let ApiAuth::Login { authorize, scopes, .. } = &c.auth else {
            bail!("{} takes a key, not a login", c.label);
        };
        let client_id = c.client_id.as_deref().map(str::trim).filter(|s| !s.is_empty()).with_context(|| format!("paste {}'s client ID first", c.label))?;
        let verifier = random();
        let state = random();
        {
            let mut pending = PENDING.locked();
            pending.retain(|_, (_, _, opened)| opened.elapsed() < LOGIN_WAIT);
            pending.insert(state.clone(), (c.name.clone(), verifier.clone(), Instant::now()));
        }
        let redirect = self.redirect_uri();
        let mut params = vec![
            ("response_type", "code"),
            ("client_id", client_id),
            ("redirect_uri", redirect.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state.as_str()),
        ];
        let challenge = challenge(&verifier);
        params.push(("code_challenge", challenge.as_str()));
        if !scopes.trim().is_empty() {
            params.push(("scope", scopes.as_str()));
        }
        Ok(reqwest::Url::parse_with_params(authorize, &params).context("the login address")?.to_string())
    }

    /// The service sent the browser back: trades the code for tokens and keeps them. Says what
    /// to show on the page the browser lands on.
    pub async fn api_logged_in(&self, state: &str, code: &str) -> Result<String> {
        let (name, verifier, opened) =
            PENDING.locked().remove(state).context("this login wasn't started here, or was already used. Press Allow again in Hub.")?;
        if opened.elapsed() > LOGIN_WAIT {
            bail!("this login waited too long. Press Allow again in Hub.");
        }
        let prefs = self.api_prefs().await?;
        let c = prefs.connections.into_iter().find(|c| c.name == name).with_context(|| format!("{name} was removed"))?;
        let ApiAuth::Login { token, .. } = &c.auth else { bail!("{} takes a key, not a login", c.label) };
        let client_id = c.client_id.as_deref().unwrap_or_default().trim().to_owned();
        let redirect = self.redirect_uri();
        let sent = CLIENT
            .post(token)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect.as_str()),
                ("client_id", client_id.as_str()),
                ("code_verifier", verifier.as_str()),
            ])
            .timeout(Duration::from_secs(20))
            .send()
            .await;
        let body = token_answer(sent, &c.label).await?;
        if body["refresh_token"].as_str().is_none() {
            bail!("{} logged you in but gave no refresh token, so Hub couldn't keep the login", c.label);
        }
        forget_token(&c.name);
        self.keep_tokens(&c, &body).await?;
        let test = self.test_api(&c.name).await?;
        Ok(if test.ok { format!("{} is connected. {}", c.label, test.message) } else { format!("{} let Hub in, but {}", c.label, test.message) })
    }

    /// Tries the other usual ways to send a key a service refused as a Bearer token. The first
    /// that works, and what its test said.
    pub(crate) async fn probe(&self, c: &ApiConnection, key: &str) -> Option<(ApiAuth, ApiTest)> {
        for auth in ways() {
            let mut tried = c.clone();
            tried.auth = auth.clone();
            let Ok(signing) = self.signing(&tried, key).await else { continue };
            let test = self.try_key(&tried, &signing).await;
            if test.ok {
                return Some((auth, test));
            }
        }
        None
    }

    async fn gateway_keys(&self) -> BTreeMap<String, GatewayKey> {
        match self.store.get_value(KEYS).await {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            _ => BTreeMap::new(),
        }
    }

    /// The gateway address of connection `name` for the threads in `folder`.
    pub async fn gateway_url(&self, name: &str, folder: &str) -> Result<String> {
        let _one = KEYS_WRITE.lock().await;
        let mut keys = self.gateway_keys().await;
        let key = match keys.iter().find(|(_, k)| k.name == name && same_folder(&k.folder, folder)) {
            Some((key, _)) => key.clone(),
            None => {
                let key = random();
                keys.insert(key.clone(), GatewayKey { name: name.into(), folder: folder.into() });
                self.store.set_value(KEYS, &serde_json::to_string(&keys)?).await?;
                key
            }
        };
        Ok(format!("http://127.0.0.1:{}{PREFIX}{key}", self.config.bind.port()))
    }

    /// The connection a gateway key stands for, while it is still on for that key's project.
    async fn gateway_connection(&self, key: &str) -> Option<ApiConnection> {
        let k = self.gateway_keys().await.remove(key)?;
        let prefs = self.api_prefs().await.ok()?;
        prefs.connections.into_iter().find(|c| c.name == k.name && c.auth.gateway() && c.projects.iter().any(|p| same_folder(p, &k.folder)))
    }
}

/// Headers that stay between the thread and Hub.
const KEEP_IN: [HeaderName; 9] = [
    header::HOST,
    header::AUTHORIZATION,
    header::PROXY_AUTHORIZATION,
    header::COOKIE,
    header::CONTENT_LENGTH,
    header::CONNECTION,
    header::TRANSFER_ENCODING,
    header::ORIGIN,
    header::REFERER,
];
/// Headers that stay between the service and Hub.
const KEEP_OUT: [HeaderName; 4] = [header::SET_COOKIE, header::CONTENT_LENGTH, header::CONNECTION, header::TRANSFER_ENCODING];

fn refuse(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(serde_json::json!({ "error": message, "from": "Hub's gateway" }))).into_response()
}

/// `ANY /gw/<key>/<path>`: one call from a thread to a connection, signed by Hub.
pub async fn forward(State(hub): State<Arc<Hub>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().strip_prefix(PREFIX).unwrap_or_default();
    let (key, rest) = path.split_once('/').unwrap_or((path, ""));
    let Some(c) = hub.gateway_connection(key).await else {
        return refuse(StatusCode::FORBIDDEN, "this address is not on for this project any more; ask the user to turn the connection on in Hub");
    };
    let secret = match hub.connection(&c.name).await {
        Ok((_, secret)) => secret,
        Err(err) => return refuse(StatusCode::UNAUTHORIZED, &format!("{err:#}")),
    };
    let Ok(body) = axum::body::to_bytes(body, MAX_BODY).await else {
        return refuse(StatusCode::PAYLOAD_TOO_LARGE, "the body is too big for the gateway");
    };
    let mut url = format!("{}/{rest}", c.url);
    if let Some(query) = parts.uri.query() {
        url = format!("{url}?{query}");
    }
    for attempt in 0..2 {
        let signing = match hub.signing(&c, &secret).await {
            Ok(s) => s,
            Err(err) => {
                hub.count(&c.name, None, Some(&format!("{err:#}"))).await;
                return refuse(StatusCode::UNAUTHORIZED, &format!("{err:#}"));
            }
        };
        let mut call = CLIENT.request(parts.method.clone(), &url);
        for (name, value) in &parts.headers {
            if !KEEP_IN.contains(name) && !name.as_str().starts_with("x-forwarded") {
                call = call.header(name, value);
            }
        }
        let sent = signing.apply(call).body(body.clone()).timeout(CALL_WAIT).send().await;
        match sent {
            // A login's token can be refused before its time; once more with a new one.
            Ok(r) if r.status() == StatusCode::UNAUTHORIZED && attempt == 0 && matches!(c.auth, ApiAuth::Login { .. }) => {
                forget_token(&c.name);
            }
            Ok(r) => {
                let status = r.status();
                let failed = (status.is_server_error() || status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN)
                    .then(|| format!("{} answered {status}", c.label));
                hub.count(&c.name, None, failed.as_deref()).await;
                let mut out = Response::builder().status(status);
                for (name, value) in r.headers() {
                    if !KEEP_OUT.contains(name) {
                        out = out.header(name, value);
                    }
                }
                let bytes = r.bytes().await.unwrap_or_default();
                return out.body(axum::body::Body::from(bytes)).unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
            }
            Err(err) => {
                let message = format!("no answer from {}: {err}", c.label);
                hub.count(&c.name, None, Some(&message)).await;
                return refuse(StatusCode::BAD_GATEWAY, &message);
            }
        }
    }
    refuse(StatusCode::UNAUTHORIZED, &format!("{} refused the login twice; press Allow again in Hub", c.label))
}

/// `GET /api/oauth/callback`: where the service sends the browser after Allow.
pub async fn callback(State(hub): State<Arc<Hub>>, Query(q): Query<BTreeMap<String, String>>) -> Response {
    let (ok, said) = match (q.get("state"), q.get("code"), q.get("error")) {
        (state, _, Some(error)) => {
            if let Some(state) = state {
                PENDING.locked().remove(state);
            }
            (false, format!("The service said no: {error}. You can close this tab and press Allow again in Hub."))
        }
        (Some(state), Some(code), None) => match hub.api_logged_in(state, code).await {
            Ok(said) => (true, format!("{said} You can close this tab.")),
            Err(err) => (false, format!("{err:#}")),
        },
        _ => (false, "This page is for a service sending you back to Hub after you allowed it.".into()),
    };
    let colour = if ok { "#84c29a" } else { "#e5806f" };
    let said = said.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    Html(format!(
        "<!doctype html><meta charset=utf-8><title>Hub</title>\
         <body style=\"margin:0;height:100vh;display:grid;place-items:center;background:#1a1918;color:#ece8e2;\
         font:15px/1.5 system-ui,sans-serif\"><div style=\"max-width:520px;padding:24px\">\
         <div style=\"color:{colour};font-size:13px;margin-bottom:6px\">{}</div>{said}</div>",
        if ok { "Connected" } else { "Not connected" }
    ))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apis::gateway_url_var as url_var;

    #[test]
    fn pkce_challenge_is_rfc_7636() {
        // The example in RFC 7636, appendix B.
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn variable_names() {
        assert_eq!(url_var("spotify"), "SPOTIFY_BASE_URL");
        assert_eq!(url_var("my-weather"), "MY_WEATHER_BASE_URL");
    }

    /// A pretend service: `/me` wants the key in X-Api-Key, `/api/me` wants a login's access
    /// token, and `/token` is its login.
    async fn service(challenge_seen: Arc<Mutex<Option<String>>>) -> String {
        use axum::routing::{get, post};
        use axum::http::HeaderMap;
        let me = |headers: HeaderMap| async move {
            if headers.get("x-api-key").is_some_and(|v| v == "k-123") {
                (StatusCode::OK, axum::Json(serde_json::json!({ "login": "tester" }))).into_response()
            } else {
                StatusCode::UNAUTHORIZED.into_response()
            }
        };
        let api_me = |headers: HeaderMap| async move {
            let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default().to_owned();
            if auth == "Bearer a1" || auth == "Bearer a2" {
                (StatusCode::OK, axum::Json(serde_json::json!({ "display_name": "christoffer", "token": auth }))).into_response()
            } else {
                StatusCode::UNAUTHORIZED.into_response()
            }
        };
        let token = move |axum::Form(form): axum::Form<BTreeMap<String, String>>| {
            let seen = challenge_seen.clone();
            async move {
                let answer = match form.get("grant_type").map(String::as_str) {
                    Some("authorization_code")
                        if form.get("code").is_some_and(|c| c == "c1")
                            && form.get("client_id").is_some_and(|c| c == "cid")
                            && seen.locked().as_deref() == form.get("code_verifier").map(|v| challenge(v)).as_deref() =>
                    {
                        serde_json::json!({ "access_token": "a1", "refresh_token": "r1", "expires_in": 3600 })
                    }
                    Some("refresh_token") if form.get("refresh_token").is_some_and(|r| r == "r1") => {
                        serde_json::json!({ "access_token": "a2", "expires_in": 3600 })
                    }
                    _ => return (StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({ "error": "invalid_grant" }))).into_response(),
                };
                axum::Json(answer).into_response()
            }
        };
        let app = axum::Router::new().route("/me", get(me)).route("/api/me", get(api_me)).route("/token", post(token));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        address
    }

    fn connection(name: &str, url: &str, test_path: &str, secret: &str, auth: ApiAuth) -> ApiConnection {
        ApiConnection {
            name: name.into(),
            kind: "custom".into(),
            label: name.into(),
            url: url.into(),
            secret: secret.into(),
            model: None,
            test_path: test_path.into(),
            docs: None,
            daily_tokens: 0,
            projects: vec![r"D:\Dev\site".into()],
            auth,
            client_id: Some("cid".into()),
            credit: None,
        }
    }

    #[tokio::test]
    async fn any_key_any_way_and_a_login_through_the_gateway() -> Result<()> {
        let seen = Arc::new(Mutex::new(None));
        let url = service(seen.clone()).await;
        let data = tempfile::tempdir()?;
        let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
        let hub = Hub::new(crate::Store::in_memory()?, config);
        let login = ApiAuth::Login { authorize: format!("{url}/authorize"), token: format!("{url}/token"), scopes: "a b".into() };
        hub.set_api_prefs(crate::apis::ApiPrefs {
            connections: vec![
                connection("acme", &url, "/me", "ACME_API_KEY", ApiAuth::Bearer),
                connection("spot", &url, "/api/me", "SPOT_REFRESH_TOKEN", login),
            ],
            helper: None,
            off: Vec::new(),
            sure: Default::default(),
        })
        .await?;
        hub.secrets.set("ACME_API_KEY", "k-123").await?;

        // Refused as Bearer, so Hub finds the header it wants and keeps it.
        let test = hub.test_api("acme").await?;
        assert!(test.ok, "{}", test.message);
        assert!(test.message.contains("as tester") && test.message.contains("X-Api-Key"), "{}", test.message);
        assert_eq!(hub.api_prefs().await?.connections[0].auth, ApiAuth::Header { name: "X-Api-Key".into() });

        // The thread gets the gateway's address, never the key.
        let env = hub.api_env("d:/dev/site").await;
        assert!(!env.iter().any(|(_, v)| v.contains("k-123")), "{env:?}");
        let (_, base) = env.iter().find(|(n, _)| n == "ACME_BASE_URL").expect("ACME_BASE_URL");
        let path = base.split_once("7878").unwrap().1.to_owned();
        let call = |path: String| {
            let hub = hub.clone();
            async move {
                let r = forward(State(hub), Request::builder().uri(path).body(axum::body::Body::empty()).unwrap()).await;
                let status = r.status();
                (status, String::from_utf8_lossy(&axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap()).into_owned())
            }
        };
        assert_eq!(call(format!("{path}/me")).await, (StatusCode::OK, r#"{"login":"tester"}"#.into()));
        assert_eq!(call(format!("{PREFIX}nope/me")).await.0, StatusCode::FORBIDDEN);

        // Log in: the Allow page's address, then the service sends back a code.
        let allow = reqwest::Url::parse(&hub.api_login("spot").await?)?;
        let q: BTreeMap<String, String> = allow.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "cid");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:7878/api/oauth/callback");
        assert_eq!(q["code_challenge_method"], "S256");
        *seen.locked() = Some(q["code_challenge"].clone());
        assert!(hub.api_logged_in("not-mine", "c1").await.is_err());
        let said = hub.api_logged_in(&q["state"], "c1").await?;
        assert!(said.contains("connected") && said.contains("christoffer"), "{said}");
        assert!(hub.api_logged_in(&q["state"], "c1").await.is_err(), "a state is used once");
        assert_eq!(hub.secrets.env(&["SPOT_REFRESH_TOKEN".into()]).await?[0].1, "r1");

        // The access token ran out: the gateway renews it with the refresh token.
        forget_token("spot");
        let env = hub.api_env(r"D:\Dev\site").await;
        let (_, base) = env.iter().find(|(n, _)| n == "SPOT_BASE_URL").expect("SPOT_BASE_URL");
        let path = base.split_once("7878").unwrap().1.to_owned();
        let (status, body) = call(format!("{path}/api/me")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("Bearer a2"), "{body}");

        // Turned off for the project: its address stops working.
        let mut prefs = hub.api_prefs().await?;
        prefs.connections[1].projects.clear();
        hub.set_api_prefs(prefs).await?;
        assert_eq!(call(format!("{path}/api/me")).await.0, StatusCode::FORBIDDEN);
        Ok(())
    }

    /// A fake Spotify token endpoint that rotates the refresh token and later revokes it.
    async fn rotating_service() -> String {
        use axum::routing::post;
        let calls = Arc::new(Mutex::new(0u32));
        let token = move |axum::Form(form): axum::Form<BTreeMap<String, String>>| {
            let calls = calls.clone();
            async move {
                // Spotify's PKCE login takes the client ID, never a secret, and a form body.
                assert!(!form.contains_key("client_secret"));
                assert_eq!(form["client_id"], "cid", "client ID is trimmed");
                let n = {
                    let mut c = calls.locked();
                    *c += 1;
                    *c
                };
                match (form["grant_type"].as_str(), form.get("refresh_token").map(String::as_str), n) {
                    ("refresh_token", Some("r1"), 1) => {
                        axum::Json(serde_json::json!({ "access_token": "a1", "refresh_token": "r2", "expires_in": 3600 })).into_response()
                    }
                    _ => (
                        StatusCode::BAD_REQUEST,
                        axum::Json(serde_json::json!({ "error": "invalid_grant", "error_description": "Refresh token revoked" })),
                    )
                        .into_response(),
                }
            }
        };
        let app = axum::Router::new().route("/token", post(token));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await });
        address
    }

    #[tokio::test]
    async fn spotify_login_address_and_token_handling() -> Result<()> {
        let data = tempfile::tempdir()?;
        let config = crate::Config { data_dir: data.path().to_owned(), ..Default::default() };
        let hub = Hub::new(crate::Store::in_memory()?, config);

        // The real catalog entry: what Spotify's Allow page is sent.
        let kind = crate::apis::catalog().into_iter().find(|k| k.kind == "spotify").expect("spotify in the catalog");
        let mut c = connection("spotify", &kind.url, &kind.test_path, &kind.secret, kind.auth.clone());
        c.client_id = Some(" cid\n".into());
        hub.set_api_prefs(crate::apis::ApiPrefs { connections: vec![c.clone()], helper: None, off: Vec::new(), ..Default::default() }).await?;
        let allow = reqwest::Url::parse(&hub.api_login("spotify").await?)?;
        assert_eq!(allow.host_str(), Some("accounts.spotify.com"));
        assert_eq!(allow.path(), "/authorize");
        let q: BTreeMap<String, String> = allow.query_pairs().into_owned().collect();
        assert_eq!(q["response_type"], "code");
        assert_eq!(q["client_id"], "cid");
        // Spotify matches this letter for letter against the app's Redirect URIs, and no longer accepts "localhost".
        assert_eq!(q["redirect_uri"], hub.redirect_uri());
        assert!(q["redirect_uri"].starts_with("http://127.0.0.1:") && q["redirect_uri"].ends_with("/api/oauth/callback"));
        assert_eq!(q["code_challenge_method"], "S256");
        assert!(!q.contains_key("client_secret"));
        for scope in ["user-read-private", "user-modify-playback-state", "user-read-playback-state"] {
            assert!(q["scope"].split(' ').any(|s| s == scope), "{scope} missing in {}", q["scope"]);
        }
        // Spotify wants a verifier of 43-128 characters from the unreserved set; the challenge is 43 base64url characters.
        let (_, verifier, _) = PENDING.locked().get(&q["state"]).cloned().expect("pending login");
        assert!((43..=128).contains(&verifier.len()) && verifier.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)));
        assert_eq!(challenge(&verifier), q["code_challenge"]);
        assert_eq!(q["code_challenge"].len(), 43);

        // The user pressed Cancel on Spotify: the page says so and the login is forgotten.
        let page = callback(
            State(hub.clone()),
            Query([("error", "access_denied"), ("state", q["state"].as_str())].map(|(a, b)| (a.to_owned(), b.to_owned())).into()),
        )
        .await;
        let html = String::from_utf8_lossy(&axum::body::to_bytes(page.into_body(), 1 << 20).await?).into_owned();
        assert!(html.contains("Not connected") && html.contains("access_denied"), "{html}");
        assert!(PENDING.locked().get(&q["state"]).is_none());

        // Refresh: a rotated refresh token is kept, and a revoked one says what to do.
        let url = rotating_service().await;
        let login = ApiAuth::Login { authorize: format!("{url}/authorize"), token: format!("{url}/token"), scopes: String::new() };
        let mut c = connection("rot", &url, "/v1/me", "ROT_REFRESH_TOKEN", login);
        c.client_id = Some("  cid ".into());
        hub.set_api_prefs(crate::apis::ApiPrefs { connections: vec![c.clone()], helper: None, off: Vec::new(), ..Default::default() }).await?;
        hub.secrets.set("ROT_REFRESH_TOKEN", "r1").await?;
        let (_, refresh) = hub.connection("rot").await?;
        assert_eq!(hub.access_token(&c, &refresh, &format!("{url}/token")).await?, "a1");
        assert_eq!(hub.secrets.env(&["ROT_REFRESH_TOKEN".into()]).await?[0].1, "r2", "rotated token kept");
        forget_token("rot");
        let err = hub.access_token(&c, "r1", &format!("{url}/token")).await.unwrap_err().to_string();
        assert!(err.contains("Press Allow again") && err.contains("revoked"), "{err}");

        // Never logged in: the message points at Allow, not at pasting a key.
        hub.set_api_prefs(crate::apis::ApiPrefs { connections: vec![connection("fresh", &url, "/v1/me", "FRESH_REFRESH_TOKEN", c.auth.clone())], helper: None, off: Vec::new(), ..Default::default() }).await?;
        let err = hub.connection("fresh").await.unwrap_err().to_string();
        assert!(err.contains("Press Allow"), "{err}");
        Ok(())
    }
}
