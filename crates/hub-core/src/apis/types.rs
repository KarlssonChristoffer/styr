//! The shapes of API connections: the catalog of services Hub can add (`ApiKind`, `catalog`) and
//! how each shows its key (`ApiAuth`, `Signing`), a connection with its credit and prefs, its
//! usage and status, the `/api/apis` request and answer bodies, and the decisions a connection
//! helped make (`ApiDecision`). The behaviour is in `apis.rs`.

use super::*;

/// A service Hub knows how to add, test and ask.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ApiKind {
    pub kind: String,
    /// What it is called in Hub.
    pub label: String,
    pub what: String,
    pub url: String,
    /// The vault name of its key, also the environment variable a project's threads get.
    pub secret: String,
    pub model: Option<String>,
    /// A cheap GET that says whether the key works.
    pub test_path: String,
    pub docs: Option<String>,
    /// Where the key is made.
    pub keys_at: Option<String>,
    /// Hub can ask it questions (`Hub::ask_api`), so it can try one and help with Hub's own decisions.
    pub asks: bool,
    /// How a call shows the key. Part of the recipe: you never pick it.
    pub auth: ApiAuth,
    /// What you paste, in the service's own words ("API key", "Client ID").
    pub paste: String,
    /// One line on getting it at `keys_at`.
    pub setup: String,
}

/// How a call shows who it is. Known services bring theirs in the catalog; for one you add
/// yourself, Hub starts with a Bearer key and finds the right way when the test is refused
/// (`gateway::probe`). Every kind but `Bearer` is reached through Hub's gateway, so a thread
/// never holds the key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum ApiAuth {
    /// `Authorization: Bearer <key>`.
    #[default]
    Bearer,
    /// The key alone in a header of this name, like `X-Api-Key`.
    Header { name: String },
    /// The key in the address, like `?appid=`.
    Query { name: String },
    /// `Authorization: Basic`, the key being `user:password`.
    Basic,
    /// You log in on the service's own page once (OAuth with PKCE, so only the app's client ID is
    /// needed). The vault keeps the refresh token; Hub renews the short-lived access token.
    Login { authorize: String, token: String, scopes: String },
}

impl ApiAuth {
    /// Reached through Hub's gateway rather than with the key in the thread's environment.
    pub fn gateway(&self) -> bool {
        *self != ApiAuth::Bearer
    }

    /// How it shows the key, in words.
    pub fn says(&self) -> String {
        match self {
            ApiAuth::Bearer => "as a Bearer token".into(),
            ApiAuth::Header { name } => format!("in the {name} header"),
            ApiAuth::Query { name } => format!("in the address as ?{name}="),
            ApiAuth::Basic => "as a username and password".into(),
            ApiAuth::Login { .. } => "by logging in once on its own page".into(),
        }
    }
}

/// What Hub can add without you describing it.
pub fn catalog() -> Vec<ApiKind> {
    vec![ApiKind {
        kind: "typesafe".into(),
        label: "Jev".into(),
        what: "TypeSafe's decision model: typed questions about a text, answered with a pick, a score or a yes \
               probability, and how sure it is."
            .into(),
        url: "https://api.typesafe.ai".into(),
        secret: "TYPESAFE_API_KEY".into(),
        model: Some("jev-latest".into()),
        test_path: "/v1/models".into(),
        docs: Some("https://docs.typesafe.ai/introduction".into()),
        keys_at: Some("https://console.typesafe.ai/keys".into()),
        asks: true,
        auth: ApiAuth::Bearer,
        paste: "API key".into(),
        setup: "Make a key in the TypeSafe console.".into(),
    },
    ApiKind {
        kind: "spotify".into(),
        label: "Spotify".into(),
        what: "See and control what you play, your playlists and your library.".into(),
        url: "https://api.spotify.com".into(),
        secret: "SPOTIFY_REFRESH_TOKEN".into(),
        model: None,
        test_path: "/v1/me".into(),
        docs: Some("https://developer.spotify.com/documentation/web-api".into()),
        keys_at: Some("https://developer.spotify.com/dashboard".into()),
        asks: false,
        auth: ApiAuth::Login {
            authorize: "https://accounts.spotify.com/authorize".into(),
            token: "https://accounts.spotify.com/api/token".into(),
            scopes: "user-read-private user-read-email user-read-playback-state user-modify-playback-state \
                     user-read-currently-playing user-read-recently-played user-top-read user-library-read \
                     playlist-read-private playlist-modify-private playlist-modify-public"
                .into(),
        },
        paste: "Client ID".into(),
        setup: "Open your app, or create one (any name, tick Web API). Edit settings, paste the address below under \
                Redirect URIs, press Add, then Save at the bottom. Then copy the app's Client ID."
            .into(),
    },
    ApiKind {
        kind: "github".into(),
        label: "GitHub".into(),
        what: "Repositories, issues, pull requests and your account, through GitHub's REST API.".into(),
        url: "https://api.github.com".into(),
        secret: "GITHUB_API_TOKEN".into(),
        model: None,
        test_path: "/user".into(),
        docs: Some("https://docs.github.com/rest".into()),
        keys_at: Some("https://github.com/settings/personal-access-tokens/new".into()),
        asks: false,
        auth: ApiAuth::Bearer,
        paste: "Token".into(),
        setup: "Make a fine-grained token with the repositories and rights it should have.".into(),
    },
    ApiKind {
        kind: "openweather".into(),
        label: "OpenWeather".into(),
        what: "Weather now and forecasts for any place.".into(),
        url: "https://api.openweathermap.org".into(),
        secret: "OPENWEATHER_API_KEY".into(),
        model: None,
        test_path: "/data/2.5/weather?q=Stockholm".into(),
        docs: Some("https://openweathermap.org/api".into()),
        keys_at: Some("https://home.openweathermap.org/api_keys".into()),
        asks: false,
        auth: ApiAuth::Query { name: "appid".into() },
        paste: "API key".into(),
        setup: "Copy the default key, or make one. A new key can take an hour to start working.".into(),
    }]
}

/// One connection you added.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiConnection {
    /// Short id, letters, digits and -, like "jev".
    pub name: String,
    /// A catalog kind, or "custom".
    pub kind: String,
    pub label: String,
    /// Base address, without a trailing /.
    pub url: String,
    /// The vault name of its key.
    pub secret: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub test_path: String,
    #[serde(default)]
    pub docs: Option<String>,
    /// Hub stops asking for the day after this many tokens. Off at 0.
    #[serde(default)]
    #[ts(type = "number")]
    pub daily_tokens: u64,
    /// Project folders whose threads get it.
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub auth: ApiAuth,
    /// The app's client ID, for `ApiAuth::Login`. Not a secret: the service shows it on every login.
    #[serde(default)]
    pub client_id: Option<String>,
    /// What you have on the service's account, which Hub counts down from (`ApiStatus.total`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub credit: Option<ApiCredit>,
}

/// Money you put on a service, entered by hand because services like TypeSafe have no way to ask.
/// What is left is `usd` less what the input tokens since `tokens_at` cost.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiCredit {
    pub usd: f64,
    /// `ApiStatus.total.input_tokens` when you entered it. Hub sets it whenever `usd` changes.
    #[serde(default)]
    #[ts(type = "number")]
    pub tokens_at: u64,
    /// When you entered it, ms. Hub sets it too.
    #[serde(default)]
    #[ts(type = "number")]
    pub at: i64,
}

/// Hub's own decisions a connection can make, by name, with what each one decides.
pub const FEATURES: [(&str, &str); 6] = [
    ("turns", "Reads where each finished turn left the work: done, asks you, stuck or stopped half way"),
    ("focus", "Says which line of the project's stage a new thread works towards, or that it is off the stage"),
    ("proof", "Reads the end of a test run that exited cleanly, to catch failures it printed anyway"),
    ("topic", "Notices a message that starts a different task than the thread has been on, and offers a thread of its own"),
    ("judge", "Reads what you write to a thread that is working: a question is answered at the side, a stop stops, a correction steers, the rest waits its turn"),
    ("agent", "Lets an agent ask it a choice or a yes/no question about a text (`ask_jev`), when a thread has it on"),
];

/// The connections, and which of Hub's own features use one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ApiPrefs {
    pub connections: Vec<ApiConnection>,
    /// The connection Hub's own decisions ask (`FEATURES`). Null: the first one that answers
    /// questions and has its key.
    pub helper: Option<String>,
    /// Features in `FEATURES` turned off.
    pub off: Vec<String>,
    /// How sure the connection must be before Hub goes by it, per feature in `FEATURES`, where you
    /// changed it (0.5 to 0.99). A feature not listed keeps its own threshold. Stopping a working
    /// thread and offering a new one keep their own higher bars (`judge.rs`).
    #[ts(type = "Record<string, number>")]
    pub sure: BTreeMap<String, f64>,
}

/// A connection's count since Hub started counting. Before `TOTALS` existed it is the days still kept.
pub(super) fn total_of(usage: &BTreeMap<String, BTreeMap<String, ApiUsage>>, totals: &BTreeMap<String, ApiUsage>, name: &str) -> ApiUsage {
    if let Some(t) = totals.get(name) {
        return t.clone();
    }
    let mut t = ApiUsage::default();
    for u in usage.get(name).into_iter().flat_map(|d| d.values()) {
        t.calls += u.calls;
        t.input_tokens += u.input_tokens;
        t.output_tokens += u.output_tokens;
        t.errors += u.errors;
    }
    t
}

/// Calls and tokens in one day or month.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiUsage {
    pub calls: u32,
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
    pub errors: u32,
}

/// The last time a connection was tried, by a test or a real call.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiCheck {
    pub ok: bool,
    pub message: String,
    #[ts(type = "number")]
    pub at: i64,
}

/// How a connection is doing.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ApiStatus {
    pub name: String,
    /// The end of the key, when the vault has it.
    pub key: Option<String>,
    pub today: ApiUsage,
    pub month: ApiUsage,
    /// Everything since Hub started counting, for what is left of `ApiConnection.credit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub total: Option<ApiUsage>,
    /// The last thing that went wrong, with when.
    pub last_error: Option<String>,
    #[ts(type = "number | null")]
    pub last_error_at: Option<i64>,
    /// The last test or call, kept across restarts.
    pub check: Option<ApiCheck>,
}

/// `GET /api/apis`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ApiState {
    pub prefs: ApiPrefs,
    pub catalog: Vec<ApiKind>,
    /// Hub's own decisions a connection can make (`FEATURES`).
    pub features: Vec<ApiFeature>,
    pub status: Vec<ApiStatus>,
    /// The address a service that logs you in sends you back to (`ApiAuth::Login`).
    pub redirect: String,
}

/// One of Hub's own decisions a connection can make.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ApiFeature {
    pub name: String,
    pub what: String,
}

/// `POST /api/apis/{name}/test`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ApiTest {
    pub ok: bool,
    pub message: String,
    pub ms: u32,
    /// The models the key can use, when the service lists them.
    pub models: Vec<String>,
}

/// `POST /api/apis/{name}/try`: one yes/no question about a text.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiTry {
    pub state: String,
    pub statement: String,
}

/// What a connection answered.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ApiAnswer {
    pub model: String,
    /// The service's answers by question name, as it sent them.
    #[ts(type = "Record<string, any>")]
    pub answers: Value,
    pub ms: u32,
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
}

/// What `Hub::choose` got back.
#[derive(Debug, Clone)]
pub struct Choice {
    pub connection: String,
    pub pick: String,
    pub confidence: f64,
    /// Sure enough to go by.
    pub acted: bool,
    pub ms: u32,
}

/// One time a connection helped decide something in Hub, kept to look back at.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ApiDecision {
    #[ts(type = "number")]
    pub at: i64,
    pub feature: String,
    pub connection: String,
    pub thread: Option<String>,
    /// The start of what was sorted.
    pub text: String,
    /// What Hub decided without the connection.
    pub hub: String,
    /// What the connection said.
    pub said: String,
    pub confidence: Option<f64>,
    pub agree: bool,
    pub ms: u32,
    /// Hub went by what the connection said, because it was sure enough.
    #[serde(default)]
    pub acted: bool,
    /// The words it read, as short as they go: the end of the agent's reply, what you wrote, the
    /// command. Older decisions have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub quote: Option<String>,
    /// The tokens the call took, in and out. Older decisions have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub tokens: Option<u64>,
}

/// Everything a decision was made from: the question, its options and what the connection read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Seen {
    pub at: i64,
    pub feature: String,
    pub thread: Option<String>,
    pub instructions: String,
    pub options: Vec<(String, String)>,
    pub state: String,
    pub sure: f64,
}

/// How one call shows the key.
pub enum Signing {
    Header(String, String),
    Query(String, String),
}

impl Signing {
    pub fn apply(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            Signing::Header(name, value) => request.header(name.as_str(), value.as_str()),
            Signing::Query(name, value) => request.query(&[(name, value)]),
        }
    }
}

/// The variable a thread finds a gateway connection's address in: spotify → SPOTIFY_BASE_URL.
pub fn gateway_url_var(name: &str) -> String {
    format!("{}_BASE_URL", name.to_ascii_uppercase().replace('-', "_"))
}
