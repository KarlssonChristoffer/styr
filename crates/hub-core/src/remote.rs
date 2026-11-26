//! Other machines running hubd. Their threads are relayed into this core's list and stream, and
//! requests about them are forwarded, so a surface talks to one core whatever runs where.
//!
//! A host is reached at a URL (Tailscale serves hubd at `https://<machine>.<tailnet>.ts.net`), or
//! through an SSH tunnel to the other hubd's loopback port (`ssh -N -L`, with the user's own keys).
//! With both, the URL is tried first and the tunnel is the fallback, and a host on the tunnel moves
//! back to the URL once it answers again. The other hubd is the same program: it owns its threads,
//! its log and its engines, and keeps running when this machine sleeps.
//!
//! This file owns the list of hosts (`Remotes`, each a `Remote`): starting, keeping and saving
//! them, asking one or forwarding a request to it. The parts: `link` keeps the connection to a
//! host and relays its stream, `ship` brings a host to this machine's hubd version (install,
//! send, follow the update there, and what Settings > Updates reads of it), `bundle` makes git
//! bundles of this machine's clone, `ssh` runs commands on a host, `setup` adds, sets up and
//! removes hosts, `views` reads across them (threads, identities, the phone's machines).

mod bundle;
mod link;
mod setup;
mod ship;
mod ssh;
mod views;

pub(crate) use bundle::version_bundle_since;
use link::Conn;
pub use ship::Ship;
use ssh::*;

use crate::locked::LockExt;
use crate::locked::Ignore;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use tokio::sync::{Notify, broadcast};
use tokio::task::JoinHandle;

use crate::config::HostConfig;
use crate::model::{HostInfo, HostSettings, HostSetup, HostState, TailnetJoin, InstallAnswer, InstallRequest, NewHost, SetUpAnswer, StreamMsg, Thread, ThreadId, UpdateTrack};
use crate::store::Store;

/// Hosts added from the app, as JSON in the store.
const HOSTS_KEY: &str = "hosts";

/// The vault key a machine's token is kept under: `HUB_HOST_<NAME>`.
fn host_key(name: &str) -> String {
    let name: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    format!("HUB_HOST_{name}")
}
/// The installer, sent over SSH to a machine that has no hubd yet.
const INSTALL_SCRIPT: &str = include_str!("../../../scripts/install-host.sh");
/// Hub's folder on a Linux host, in a shell there.
/// Whether the user let this machine build hubd for the others (`crossbuild`): "yes" or "no".
const BUILD_HERE_KEY: &str = "build_for_hosts";
const REMOTE_DATA: &str = "D=\"${XDG_DATA_HOME:-$HOME/.local/share}/hub\"";
/// Stops the installer named in `install.pid` and every process under it, in a shell in `$D`.
/// By walking the tree, since a Mac has no `setsid` to make it a group of its own.
const STOP_INSTALLER: &str = "stop_tree() { for c in $(pgrep -P \"$1\"); do stop_tree \"$c\"; done; kill \"$1\" 2>/dev/null; }; \
     if [ -f install.pid ] && kill -0 \"$(cat install.pid)\" 2>/dev/null; then stop_tree \"$(cat install.pid)\"; sleep 1; fi";

pub struct Remotes {
    local_name: String,
    configured: Vec<HostConfig>,
    store: Store,
    vault: Arc<crate::secrets::Vault>,
    stream: broadcast::Sender<StreamMsg>,
    client: reqwest::Client,
    hosts: Mutex<BTreeMap<String, Arc<Remote>>>,
    ship: Ship,
    /// A host came online: maybe it wants this machine's version.
    push_wake: Notify,
    /// The version last sent to each host, so it is sent once.
    sent: Mutex<HashMap<String, String>>,
    /// Hosts whose send failed: the version, how many times in a row, and when to try again.
    failed: Mutex<HashMap<String, (String, u32, i64)>>,
    /// Held while a version is sent to a host, so two sends there do not overlap.
    pushing: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// What happened the last time this machine tried to bring each host to its version, for
    /// Settings > Updates: sent, or why not.
    notes: Mutex<HashMap<String, String>>,
    /// How far the version sent to each host has got there (the handshake), by host.
    tracks: Mutex<HashMap<String, UpdateTrack>>,
    /// One Vakten for every machine (`problems::vakten`): which machine's Vakten reads this one.
    pub(crate) guarding: Guarding,
}

/// What this machine knows of the other machines' Vaktens, in memory and per Hub (not per
/// process, so two cores in one test program do not see each other's).
#[derive(Default)]
pub(crate) struct Guarding {
    /// The machine whose Vakten reads this one's problems, and until when (it asks every few minutes).
    pub by: Mutex<Option<(String, i64)>>,
    /// How many wait in each other machine's queue, as last asked.
    pub waiting: Mutex<HashMap<String, i64>>,
}

/// A host whose hubd is from before the endpoint asked for: it answered 404 or 405.
#[derive(Debug)]
pub struct TooOld(pub String);

impl std::fmt::Display for TooOld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the hubd there is too old for {}", self.0)
    }
}

impl std::error::Error for TooOld {}

/// Whether an error from another machine means its hubd lacks the endpoint, so updating it fixes it.
pub fn too_old(e: &anyhow::Error) -> bool {
    e.chain().any(|c| c.downcast_ref::<TooOld>().is_some())
}

/// 404 and 405 on a path every current hubd has: an older hubd. 405 is when an older route with
/// the same shape takes another method (`DELETE /api/engines/{name}` before `/receive`).
fn old_answer(status: StatusCode) -> bool {
    status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED
}

pub struct Remote {
    config: HostConfig,
    removable: bool,
    info: Mutex<HostInfo>,
    conn: Mutex<Option<Conn>>,
    threads: Mutex<HashMap<ThreadId, Thread>>,
    task: Mutex<Option<JoinHandle<()>>>,
    wake: Notify,
    /// What this machine is doing for the host's install (building hubd for it, sending it), for
    /// its row while the host itself has nothing to show.
    busy: Mutex<Option<String>>,
}

impl Remotes {
    pub fn new(
        local_name: String,
        configured: Vec<HostConfig>,
        store: Store,
        vault: Arc<crate::secrets::Vault>,
        stream: broadcast::Sender<StreamMsg>,
        ship: Ship,
    ) -> Arc<Self> {
        // Tunnels end on loopback and tailnets are direct; a system proxy would only get in the way.
        let client = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("building the HTTP client");
        Arc::new(Self { local_name, configured, store, vault, stream, client, hosts: Mutex::default(), ship, push_wake: Notify::new(), sent: Mutex::default(), failed: Mutex::default(), pushing: Mutex::default(), notes: Mutex::default(), tracks: Mutex::default(), guarding: Guarding::default() })
    }

    /// Connects to every host in the config and every host added from the app.
    pub async fn start(self: &Arc<Self>) {
        let added = self.added().await.unwrap_or_else(|e| {
            tracing::warn!("reading hosts: {e:#}");
            Vec::new()
        });
        for config in self.configured.clone() {
            self.launch(config, false);
        }
        for config in added {
            self.launch(config, true);
        }
        tokio::spawn(self.clone().push_updates());
        tokio::spawn(self.clone().follow_updates());
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    pub fn local_name(&self) -> &str {
        &self.local_name
    }

    /// This machine first, then the others by name.
    pub fn list(&self) -> Vec<HostInfo> {
        let mut list = vec![HostInfo {
            name: self.local_name.clone(),
            local: true,
            address: String::new(),
            state: HostState::Online,
            version: Some(hub_install::running_version()),
            error: None,
            removable: false,
            setup: None,
            join: None,
            sending: None,
            push_quiet_mins: None,
        }];
        list.extend(self.hosts.locked().values().map(|r| r.info.locked().clone()));
        list
    }

    /// Posts the same JSON to every host online now, and answers each one's reply by name. For
    /// state the machines keep together, such as Customs' list.
    pub async fn exchange<T: serde::de::DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Vec<(String, Result<T>)> {
        let hosts: Vec<Arc<Remote>> = self.hosts.locked().values().cloned().collect();
        let mut out = Vec::new();
        for remote in hosts {
            let Some(conn) = remote.conn.locked().clone() else { continue };
            out.push((remote.config.name.clone(), conn.post_json(&self.client, path, body).await));
        }
        out
    }

    /// Posts JSON to one machine and reads its answer, waiting up to `wait`. The machine's own
    /// error is the error, so it reads as if asked there.
    pub async fn post_to<T: serde::de::DeserializeOwned>(&self, name: &str, path: &str, body: &impl serde::Serialize, wait: Duration) -> Result<T> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        let Some(conn) = remote.conn.locked().clone() else { bail!("{} is offline", remote.config.name) };
        let mut request = self.client.post(format!("{}{path}", conn.base)).json(body).timeout(wait);
        if let Some(token) = &conn.token {
            request = request.bearer_auth(token);
        }
        let answer = request.send().await.with_context(|| format!("{} does not answer", remote.config.name))?;
        let status = answer.status();
        if old_answer(status) {
            self.wake_push();
            return Err(anyhow::Error::new(TooOld(path.to_owned()))
                .context(format!("the hubd on {} is too old for this; Hub is updating it (Settings > Updates)", remote.config.name)));
        }
        if !status.is_success() {
            let body: serde_json::Value = answer.json().await.unwrap_or_default();
            bail!("{}: {}", remote.config.name, body["error"].as_str().unwrap_or(status.as_str()));
        }
        Ok(answer.json().await?)
    }

    /// Sends raw bytes to one machine and reads its JSON answer, as `post_to` does.
    pub async fn post_bytes_to<T: serde::de::DeserializeOwned>(&self, name: &str, path: &str, body: Vec<u8>, wait: Duration) -> Result<T> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        let Some(conn) = remote.conn.locked().clone() else { bail!("{} is offline", remote.config.name) };
        let mut request = self.client.post(format!("{}{path}", conn.base)).header(header::CONTENT_TYPE, "application/octet-stream").body(body).timeout(wait);
        if let Some(token) = &conn.token {
            request = request.bearer_auth(token);
        }
        let answer = request.send().await.with_context(|| format!("{} does not answer", remote.config.name))?;
        let status = answer.status();
        if old_answer(status) {
            self.wake_push();
            return Err(anyhow::Error::new(TooOld(path.to_owned()))
                .context(format!("the hubd on {} is too old for this; Hub is updating it (Settings > Updates)", remote.config.name)));
        }
        if !status.is_success() {
            let body: serde_json::Value = answer.json().await.unwrap_or_default();
            bail!("{}: {}", remote.config.name, body["error"].as_str().unwrap_or(status.as_str()));
        }
        Ok(answer.json().await?)
    }

    /// Reads JSON from one machine that is online now.
    /// Asks the machine for its threads again, so surfaces see one that just moved to or from it.
    pub async fn refresh_threads(self: &Arc<Self>, name: &str) {
        let Some(remote) = self.find(name) else { return };
        let Some(conn) = remote.conn.locked().clone() else { return };
        if let Err(e) = remote.sync_threads(self, &conn).await {
            tracing::warn!("could not refresh the threads on {name}: {e:#}");
        }
    }

    pub async fn get_from<T: serde::de::DeserializeOwned>(&self, name: &str, path: &str) -> Result<T> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        let Some(conn) = remote.conn.locked().clone() else { bail!("{name} is offline") };
        conn.get(&self.client, path).await
    }

    /// Has the host's updater install the version it has ready, as its own Install button does.
    pub async fn approve_install(&self, name: &str) -> Result<()> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        let Some(conn) = remote.conn.locked().clone() else { bail!("{name} is offline") };
        conn.post(&self.client, "/api/update/install").await
    }

    /// The other machines that are online now, by name.
    pub fn online(&self) -> Vec<String> {
        self.hosts.locked().values().filter(|r| r.conn.locked().is_some()).map(|r| r.config.name.clone()).collect()
    }

    /// Puts this machine's SSH key on a host that refuses it, with the host's login password
    /// (used once, kept nowhere), then tries the host again.
    pub async fn let_in(&self, name: &str, password: &str) -> Result<()> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        let target = remote.config.ssh.clone().with_context(|| format!("{name} has no SSH target"))?;
        crate::sshkey::give_key(&target, password).await?;
        remote.info.locked().setup = None;
        self.reconnect(name)
    }

    /// Tries an offline host again now instead of waiting for the next attempt.
    pub fn reconnect(&self, name: &str) -> Result<()> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        if remote.info.locked().state == HostState::Offline {
            remote.set_state(self, HostState::Connecting, None);
            remote.wake.notify_one();
        }
        Ok(())
    }

    /// The host a request is about, when it is not this machine: `?host=<name>`, or a thread
    /// that lives on another host. An unknown host name is an error.
    pub fn route(&self, uri: &Uri) -> Option<Result<Arc<Remote>, String>> {
        if let Some(name) = host_param(uri) {
            if name.eq_ignore_ascii_case(&self.local_name) {
                return None;
            }
            return Some(self.find(&name).ok_or_else(|| format!("no machine called {name}")));
        }
        let id = uri.path().strip_prefix("/api/threads/")?.split('/').next()?;
        let hosts = self.hosts.locked();
        hosts.values().find(|r| r.threads.locked().contains_key(id)).cloned().map(Ok)
    }

    /// Shows the other machine its pairing code and takes the token it gives this one.
    async fn redeem(&self, url: &str, code: &str) -> Result<String> {
        let answer = self
            .client
            .post(format!("{url}/api/pair/redeem"))
            .timeout(Duration::from_secs(20))
            .json(&crate::model::PairRequest { code: code.to_owned(), name: self.local_name.clone() })
            .send()
            .await
            .with_context(|| format!("{url} does not answer"))?;
        if !answer.status().is_success() {
            let body: serde_json::Value = answer.json().await.unwrap_or_default();
            bail!("{}", body["error"].as_str().unwrap_or("the machine refused the code"));
        }
        let paired: crate::model::Paired = answer.json().await?;
        Ok(paired.token)
    }

    /// The SSH alias a machine is reached by: `Some(None)` when it has none, `None` when there
    /// is no such machine.
    pub fn ssh_alias(&self, name: &str) -> Option<Option<String>> {
        self.find(name).map(|r| r.config.ssh.clone())
    }

    fn find(&self, name: &str) -> Option<Arc<Remote>> {
        let hosts = self.hosts.locked();
        hosts.values().find(|r| r.config.name.eq_ignore_ascii_case(name)).cloned()
    }

    /// The machines added from the app, each with its token from the vault. Tokens an older
    /// version kept in the `hosts` value are moved to the vault here, once, and cleared from it
    /// and from the database copies in `backups/`.
    async fn added(&self) -> Result<Vec<HostConfig>> {
        let mut hosts: Vec<HostConfig> = match self.store.get_value(HOSTS_KEY).await? {
            Some(json) => serde_json::from_str(&json)?,
            None => Vec::new(),
        };
        let mut moved = false;
        let mut keys = std::collections::HashSet::new();
        for host in &mut hosts {
            // Two names that make the same key keep the second one's token where it is.
            if let Some(token) = host.token.clone().filter(|_| keys.insert(host_key(&host.name)))
                && self.keep_token(&host.name, &token).await {
                    moved = true;
                    host.token = None;
                }
        }
        if moved {
            self.store.set_value(HOSTS_KEY, &serde_json::to_string(&hosts)?).await?;
        }
        if !self.vault.is_in_memory() {
            for host in &mut hosts {
                if host.token.is_none() {
                    host.token = self.vault.value(&host_key(&host.name)).await;
                }
            }
        }
        // Every start, since a copy can be made while the move runs.
        let data = self.ship.data.clone();
        tokio::task::spawn_blocking(move || crate::backup::scrub_host_tokens(&data)).await.ok();
        Ok(hosts)
    }

    /// Puts a host's token in the vault. False when the vault cannot keep it (it lives in memory
    /// only, or the write failed), so the caller leaves the token where it is.
    async fn keep_token(&self, name: &str, token: &str) -> bool {
        if self.vault.is_in_memory() {
            return false;
        }
        match self.vault.set(&host_key(name), token).await {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("moving the token of {name} to the vault: {e:#}");
                false
            }
        }
    }

    /// Saves the added machines; their tokens go to the vault and the `hosts` value holds none.
    async fn save_added(&self, mut hosts: Vec<HostConfig>) -> Result<()> {
        for host in &mut hosts {
            if let Some(token) = host.token.clone()
                && self.keep_token(&host.name, &token).await {
                    host.token = None;
                }
        }
        self.store.set_value(HOSTS_KEY, &serde_json::to_string(&hosts)?).await
    }

    fn launch(self: &Arc<Self>, config: HostConfig, removable: bool) -> Arc<Remote> {
        let address = routes(&config).join(" · ");
        let remote = Arc::new(Remote {
            info: Mutex::new(HostInfo {
                name: config.name.clone(),
                local: false,
                address,
                state: HostState::Connecting,
                version: None,
                error: None,
                removable,
                setup: None,
                join: None,
                sending: None,
                push_quiet_mins: config.push_quiet_mins.filter(|m| *m > 0),
            }),
            config,
            removable,
            conn: Mutex::default(),
            threads: Mutex::default(),
            task: Mutex::default(),
            wake: Notify::new(),
            busy: Mutex::default(),
        });
        self.hosts.locked().insert(remote.config.name.clone(), remote.clone());
        self.stream.send(StreamMsg::Host { host: remote.info.locked().clone() }).ignore("no window may be listening");
        let task = tokio::spawn(remote.clone().run(self.clone()));
        *remote.task.locked() = Some(task);
        remote
    }
}

impl Remote {
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Says on the host's row what this machine is doing for its install; None when done.
    fn busy(&self, remotes: &Remotes, doing: Option<String>) {
        *self.busy.locked() = doing.clone();
        if let Some(doing) = &doing
            && self.info.locked().state != HostState::Online
        {
            self.info.locked().setup = Some(HostSetup::Installing);
            self.set_state(remotes, HostState::Connecting, Some(format!("Installing hubd: {doing}")));
        }
        self.sending(remotes, doing);
    }

    /// Says on the host's row what is being sent to it, so its page can show it while it is
    /// online and its own updater has nothing to say yet.
    fn sending(&self, remotes: &Remotes, doing: Option<String>) {
        let info = {
            let mut info = self.info.locked();
            if info.sending == doing {
                return;
            }
            info.sending = doing;
            info.clone()
        };
        remotes.stream.send(StreamMsg::Host { host: info }).ignore("no window may be listening");
    }

    /// Says on the host's row how putting it on the tailnet goes; None when done or not going on.
    fn joining(&self, remotes: &Remotes, join: Option<TailnetJoin>) {
        let info = {
            let mut info = self.info.locked();
            info.join = join;
            info.clone()
        };
        remotes.stream.send(StreamMsg::Host { host: info }).ignore("no window may be listening");
    }

    fn set_state(&self, remotes: &Remotes, state: HostState, error: Option<String>) {
        let info = {
            let mut info = self.info.locked();
            info.state = state;
            info.error = error;
            info.clone()
        };
        remotes.stream.send(StreamMsg::Host { host: info }).ignore("no window may be listening");
    }

    /// Sends a request on to the host and hands back its answer. Threads in the answer get
    /// their host set, as in the list.
    pub async fn forward(&self, client: &reqwest::Client, req: Request) -> Response {
        let Some(conn) = self.conn.locked().clone() else {
            return error(StatusCode::BAD_GATEWAY, format!("{} is offline", self.config.name));
        };
        let (parts, body) = req.into_parts();
        let body = match axum::body::to_bytes(body, 64 * 1024 * 1024).await {
            Ok(body) => body,
            Err(e) => return error(StatusCode::BAD_REQUEST, e.to_string()),
        };
        let path = without_host_param(&parts.uri);
        let mut request = client.request(parts.method.clone(), format!("{}{path}", conn.base)).body(body);
        if let Some(kind) = parts.headers.get(header::CONTENT_TYPE) {
            request = request.header(header::CONTENT_TYPE, kind);
        }
        if let Some(token) = &conn.token {
            request = request.bearer_auth(token);
        }
        // A phone's request stays a phone's there too (`auth::Narrow`).
        if parts.extensions.get::<crate::auth::Narrow>().is_some() {
            request = request.header(crate::auth::NARROW_HEADER, "phone");
        }
        let answer = match request.send().await {
            Ok(answer) => answer,
            Err(e) => return error(StatusCode::BAD_GATEWAY, format!("{}: {e}", self.config.name)),
        };
        let status = answer.status();
        let kind = answer.headers().get(header::CONTENT_TYPE).cloned();
        let mut bytes = match answer.bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(e) => return error(StatusCode::BAD_GATEWAY, format!("{}: {e}", self.config.name)),
        };
        if status.is_success() && answers_with_thread(&parts.method, parts.uri.path())
            && let Ok(mut thread) = serde_json::from_slice::<Thread>(&bytes)
        {
            thread.host = Some(self.config.name.clone());
            self.threads.locked().insert(thread.id.clone(), thread.clone());
            bytes = serde_json::to_vec(&thread).unwrap_or(bytes);
        }
        let mut response = Response::new(Body::from(bytes));
        *response.status_mut() = status;
        if let Some(kind) = kind {
            response.headers_mut().insert(header::CONTENT_TYPE, kind);
        }
        response
    }
}

/// How a host is reached, in the order tried.
fn routes(config: &HostConfig) -> Vec<String> {
    config.url.iter().cloned().chain(config.ssh.iter().map(|alias| format!("ssh {alias}"))).collect()
}

/// `POST /api/threads` and `GET`/`PATCH /api/threads/{id}` answer with one thread.
fn answers_with_thread(method: &Method, path: &str) -> bool {
    match path.strip_prefix("/api/threads") {
        Some("") => method == Method::POST,
        Some(rest) => {
            let id = rest.trim_start_matches('/');
            !id.is_empty() && !id.contains('/') && (method == Method::GET || method == Method::PATCH)
        }
        None => false,
    }
}

fn error(status: StatusCode, message: String) -> Response {
    (status, axum::Json(serde_json::json!({ "error": message }))).into_response()
}

fn host_param(uri: &Uri) -> Option<String> {
    uri.query()?.split('&').find_map(|pair| {
        let value = pair.strip_prefix("host=")?;
        let value = percent_decode(value);
        (!value.is_empty()).then_some(value)
    })
}

/// The path and query of `uri` without its `host` parameter, which only means something here.
fn without_host_param(uri: &Uri) -> String {
    let rest: Vec<&str> =
        uri.query().unwrap_or("").split('&').filter(|p| !p.is_empty() && !p.starts_with("host=")).collect();
    if rest.is_empty() { uri.path().to_owned() } else { format!("{}?{}", uri.path(), rest.join("&")) }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests;
