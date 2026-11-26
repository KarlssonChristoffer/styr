//! The connection to one host: finding a route that answers (the URL, then an SSH tunnel), taking
//! its threads and relaying its stream until it ends, retrying while it is unreachable, and `Conn`,
//! the handle the rest of `remote` asks the host through. A route counts once hubd answers on it
//! (`wait_healthy`) and takes this machine's token (`token_accepted`).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use axum::http::{HeaderValue, StatusCode, header};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite;

use super::ssh::{Probe, free_port, open_tunnel, probe, read_token};
use super::{Remote, Remotes, TooOld, old_answer};
use crate::config::HostConfig;
use crate::locked::LockExt;
use crate::locked::Ignore;
use crate::model::{HostSetup, HostState, StreamMsg, Thread, ThreadId, UpdateStatus};

/// Where an online host answers.
#[derive(Clone)]
pub(super) struct Conn {
    pub(super) base: String,
    pub(super) token: Option<String>,
}

/// Why a connection ended without an error.
enum Ended {
    Closed,
    /// The URL answers again while the host was reached through the tunnel.
    Better,
}

impl Remote {
    /// Keeps a connection to the host, retrying with a growing pause while it is unreachable.
    pub(super) async fn run(self: Arc<Self>, remotes: Arc<Remotes>) {
        let mut pause = 2;
        loop {
            // Retries of an offline host stay offline, with the reason, until one works.
            if self.info.locked().state != HostState::Offline {
                self.set_state(&remotes, HostState::Connecting, None);
            }
            let error = match self.session(&remotes).await {
                // A better route answers again: straight over to it.
                Ok(Ended::Better) => {
                    pause = 2;
                    continue;
                }
                Ok(Ended::Closed) => {
                    pause = 2;
                    "the connection closed".to_owned()
                }
                Err(e) => format!("{e:#}"),
            };
            tracing::info!("host {} offline: {error}", self.config.name);
            *self.conn.locked() = None;
            self.set_state(&remotes, HostState::Offline, Some(error));
            // A host that refuses this machine's key stays refusing until a key is put there, and
            // every try is a failed login in its log (fail2ban and sshd's limits count them, and
            // then lock out this machine's other SSH too). So wait for a nudge (Set up, Reconnect),
            // with only a slow try now and then.
            let wait = if self.info.locked().setup == Some(HostSetup::NoKey) { 30 * 60 } else { pause };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(wait)) => {}
                _ = self.wake.notified() => {}
            }
            pause = (pause * 2).min(60);
        }
    }

    /// The first route that answers: the URL, then the SSH tunnel. The tunnel lives as long as
    /// the returned child.
    async fn connect(&self, remotes: &Remotes) -> Result<(Conn, Option<tokio::process::Child>)> {
        let mut failed = Vec::new();
        if let Some(url) = &self.config.url {
            let conn = Conn { base: url.trim_end_matches('/').to_owned(), token: self.config.token.clone() };
            match wait_healthy(&remotes.client, &conn, None, &self.config).await {
                // The health check is open to anyone, so a token the host has since changed shows
                // only now. With SSH to read the right one, go on to the tunnel.
                Ok(()) if self.config.ssh.is_some() && !token_accepted(&remotes.client, &conn).await => {
                    failed.push("the URL refused this machine's token".to_owned());
                }
                Ok(()) => {
                    self.info.locked().setup = None;
                    return Ok((conn, None));
                }
                Err(e) => failed.push(format!("{e:#}")),
            }
        }
        if let Some(alias) = &self.config.ssh {
            let port = free_port()?;
            let mut tunnel = open_tunnel(alias, port, self.config.port)?;
            let token = match &self.config.token {
                Some(token) => Some(token.clone()),
                None => read_token(alias).await,
            };
            let mut conn = Conn { base: format!("http://127.0.0.1:{port}"), token };
            match wait_healthy(&remotes.client, &conn, Some(&mut tunnel), &self.config).await {
                Ok(()) => {
                    // A token kept here from before the host's hubd got a new one is refused
                    // forever, and Reconnect only reused it. The host's own is one SSH away.
                    if !token_accepted(&remotes.client, &conn).await {
                        match read_token(alias).await {
                            Some(fresh) if conn.token.as_ref() != Some(&fresh) => {
                                tracing::info!("host {}: its token changed; using the one read over SSH", self.config.name);
                                conn.token = Some(fresh);
                            }
                            Some(_) => tracing::info!(
                                "host {}: refuses the token in its own data folder; the hubd answering may run with another data folder than the one read over SSH",
                                self.config.name
                            ),
                            None => {}
                        }
                    }
                    self.info.locked().setup = None;
                    return Ok((conn, Some(tunnel)));
                }
                // SSH got in, so hubd is the problem: say whether it is there at all.
                Err(e) if matches!(tunnel.try_wait(), Ok(None)) => {
                    drop(tunnel);
                    // This machine may be building hubd for it, with nothing on the host to show.
                    let busy = self.busy.locked().clone();
                    let probed = match busy {
                        Some(doing) => Probe::Installing(doing),
                        None => probe(alias).await,
                    };
                    let (setup, why) = match probed {
                        Probe::Missing => (Some(HostSetup::Missing), format!("hubd is not installed on {alias}")),
                        Probe::Installing(step) => (Some(HostSetup::Installing), format!("Installing hubd: {step}")),
                        Probe::Failed(step) => (Some(HostSetup::Failed), format!("Installing hubd stopped: {step}")),
                        Probe::Installed | Probe::Unknown => {
                            (None, format!("{e:#}; on {alias}: journalctl --user -u hubd -n 50 (on a Mac: ~/.local/share/hub/hubd.log)"))
                        }
                    };
                    self.info.locked().setup = setup;
                    failed.push(why);
                }
                // SSH itself did not get in, so whatever the row said about an install there is
                // old news: say what stops it now.
                Err(e) => {
                    let why = format!("{e:#}");
                    let refused = crate::sshkey::wants_password(&why);
                    self.info.locked().setup = refused.then_some(HostSetup::NoKey);
                    failed.push(if refused {
                        format!("{alias} does not let this machine in over SSH ({why}). Its login password, once, puts this machine's key there")
                    } else {
                        why
                    });
                }
            }
        }
        if failed.is_empty() {
            bail!("set ssh or url for {} in hub.json", self.config.name);
        }
        bail!("{}", failed.join("; "))
    }

    /// One connection: find a route, take the host's threads and relay its stream until it
    /// closes, or until the URL answers again while on the tunnel.
    async fn session(&self, remotes: &Remotes) -> Result<Ended> {
        let (conn, tunnel) = self.connect(remotes).await?;
        let fallback = tunnel.is_some() && self.config.url.is_some();
        self.info.locked().address = match &tunnel {
            Some(_) => format!("ssh {}", self.config.ssh.as_deref().unwrap_or_default()),
            None => conn.base.clone(),
        };

        let status: UpdateStatus = conn.get(&remotes.client, "/api/version").await?;
        self.info.locked().version = Some(status.version);

        // Subscribe before listing, so nothing that happens in between is missed.
        let mut ws = conn.stream().await?;
        self.sync_threads(remotes, &conn).await?;
        *self.conn.locked() = Some(conn.clone());
        self.set_state(remotes, HostState::Online, None);
        remotes.push_wake.notify_one();

        let mut ping = tokio::time::interval(Duration::from_secs(30));
        let mut better = tokio::time::interval(Duration::from_secs(60));
        better.tick().await;
        loop {
            tokio::select! {
                _ = better.tick(), if fallback => {
                    if let Some(url) = &self.config.url
                        && answers(&remotes.client, url).await
                    {
                        tracing::info!("host {}: {url} answers again; leaving the SSH tunnel", self.config.name);
                        return Ok(Ended::Better);
                    }
                }
                msg = ws.next() => match msg {
                    Some(Ok(tungstenite::Message::Text(text))) => {
                        // A message type or a thread from a newer hubd that this build cannot read
                        // is skipped (or shown as far as it is understood), never the stream.
                        match serde_json::from_str::<StreamMsg>(&text).ok().or_else(|| crate::compat::stream_from_wire(&text)) {
                            Some(msg) => self.relay(remotes, &conn, msg).await?,
                            None => tracing::debug!("{}: stream message from a newer hubd skipped", self.config.name),
                        }
                    }
                    Some(Ok(tungstenite::Message::Close(_))) | None => return Ok(Ended::Closed),
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                },
                _ = ping.tick() => ws.send(tungstenite::Message::Ping(Default::default())).await?,
            }
        }
    }

    async fn relay(&self, remotes: &Remotes, conn: &Conn, msg: StreamMsg) -> Result<()> {
        match msg {
            StreamMsg::Thread { mut thread } => {
                // Only the host's own threads; a host that relays others would loop.
                if thread.host.is_some() {
                    return Ok(());
                }
                thread.host = Some(self.config.name.clone());
                self.threads.locked().insert(thread.id.clone(), thread.clone());
                remotes.stream.send(StreamMsg::Thread { thread }).ignore("no window may be listening");
            }
            StreamMsg::ThreadDeleted { id } => {
                if self.threads.locked().remove(&id).is_some() {
                    remotes.stream.send(StreamMsg::ThreadDeleted { id }).ignore("no window may be listening");
                }
            }
            StreamMsg::Event { event } => {
                if self.threads.locked().contains_key(&event.thread_id) {
                    remotes.stream.send(StreamMsg::Event { event }).ignore("no window may be listening");
                }
            }
            StreamMsg::Update { status } => {
                self.info.locked().version = Some(status.version);
                self.set_state(remotes, HostState::Online, None);
            }
            StreamMsg::Resync => {
                self.sync_threads(remotes, conn).await?;
                remotes.stream.send(StreamMsg::Resync).ignore("no window may be listening");
            }
            // Projects, other hosts and the like are the other machine's own business.
            _ => {}
        }
        Ok(())
    }

    /// Replaces the cached threads with the host's list and tells surfaces what changed.
    pub(super) async fn sync_threads(&self, remotes: &Remotes, conn: &Conn) -> Result<()> {
        // One thread this build cannot read costs that thread, not the machine's whole list.
        let list = crate::compat::threads_from_wire(conn.get::<Vec<serde_json::Value>>(&remotes.client, "/api/threads").await?);
        let fresh: HashMap<ThreadId, Thread> = list
            .into_iter()
            .filter(|t| t.host.is_none())
            .map(|mut t| {
                t.host = Some(self.config.name.clone());
                (t.id.clone(), t)
            })
            .collect();
        let old = std::mem::replace(&mut *self.threads.locked(), fresh.clone());
        for id in old.keys().filter(|id| !fresh.contains_key(*id)) {
            remotes.stream.send(StreamMsg::ThreadDeleted { id: id.clone() }).ignore("no window may be listening");
        }
        for thread in fresh.into_values() {
            remotes.stream.send(StreamMsg::Thread { thread }).ignore("no window may be listening");
        }
        Ok(())
    }
}

/// Whether hubd answers its health check at `url` within a few seconds.
async fn answers(client: &reqwest::Client, url: &str) -> bool {
    let request = client.get(format!("{}/api/health", url.trim_end_matches('/'))).timeout(Duration::from_secs(3));
    request.send().await.is_ok_and(|a| a.status().is_success())
}

/// Whether the host takes this connection's token. Only a refusal says no: a host that is slow
/// or does not know the call is left to the real requests to complain about.
pub(super) async fn token_accepted(client: &reqwest::Client, conn: &Conn) -> bool {
    let mut request = client.get(format!("{}/api/host", conn.base)).timeout(Duration::from_secs(5));
    if let Some(token) = &conn.token {
        request = request.bearer_auth(token);
    }
    !matches!(request.send().await, Ok(a) if a.status() == StatusCode::UNAUTHORIZED)
}

/// Waits until hubd answers through the tunnel, or says why it does not.
async fn wait_healthy(
    client: &reqwest::Client,
    conn: &Conn,
    mut tunnel: Option<&mut tokio::process::Child>,
    config: &HostConfig,
) -> Result<()> {
    // A tunnel takes a few seconds to come up; a URL answers or it does not.
    let patience = Duration::from_secs(if tunnel.is_some() { 15 } else { 3 });
    let deadline = tokio::time::Instant::now() + patience;
    while tokio::time::Instant::now() < deadline {
        if let Some(child) = tunnel.as_deref_mut()
            && let Some(status) = child.try_wait()?
        {
            let mut text = String::new();
            if let Some(mut err) = child.stderr.take() {
                use tokio::io::AsyncReadExt;
                err.read_to_string(&mut text).await.ignore("without ssh's last words the exit status is the reason");
            }
            let reason = text.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("").to_owned();
            if reason.is_empty() {
                bail!("ssh exited ({status})");
            }
            // ssh says "ssh: connect to host …" itself.
            if reason.starts_with("ssh:") {
                bail!("{reason}");
            }
            bail!("ssh: {reason}");
        }
        let mut request = client.get(format!("{}/api/health", conn.base)).timeout(Duration::from_secs(3));
        if let Some(token) = &conn.token {
            request = request.bearer_auth(token);
        }
        match request.send().await {
            Ok(answer) if answer.status().is_success() => return Ok(()),
            Ok(answer) if answer.status() == StatusCode::UNAUTHORIZED => {
                bail!("hubd refused the connection token; set the token for {} in hub.json", config.name)
            }
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
    match &config.ssh {
        Some(alias) => bail!("hubd does not answer on {alias} port {}; is it installed and running?", config.port),
        None => bail!("hubd does not answer at {}", conn.base),
    }
}

impl Conn {
    pub(super) async fn get<T: serde::de::DeserializeOwned>(&self, client: &reqwest::Client, path: &str) -> Result<T> {
        let mut request = client.get(format!("{}{path}", self.base)).timeout(Duration::from_secs(20));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let answer = request.send().await?;
        let status = answer.status();
        if status == StatusCode::UNAUTHORIZED {
            bail!("hubd refused the connection token; pair again with a code from `hubd pair` there");
        }
        if old_answer(status) {
            return Err(TooOld(path.to_owned()).into());
        }
        if !status.is_success() {
            bail!("{path} answered {status}");
        }
        Ok(answer.json().await?)
    }

    pub(super) async fn post_json<T: serde::de::DeserializeOwned>(&self, client: &reqwest::Client, path: &str, body: &serde_json::Value) -> Result<T> {
        let mut request = client.post(format!("{}{path}", self.base)).json(body).timeout(Duration::from_secs(30));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let answer = request.send().await?;
        let status = answer.status();
        if old_answer(status) {
            return Err(TooOld(path.to_owned()).into());
        }
        if !status.is_success() {
            bail!("{path} answered {status}");
        }
        Ok(answer.json().await?)
    }

    pub(super) async fn post(&self, client: &reqwest::Client, path: &str) -> Result<()> {
        let mut request = client.post(format!("{}{path}", self.base)).timeout(Duration::from_secs(20));
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let status = request.send().await?.status();
        if !status.is_success() {
            bail!("{path} answered {status}");
        }
        Ok(())
    }

    async fn stream(
        &self,
    ) -> Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>> {
        use tungstenite::client::IntoClientRequest;
        let base = self.base.replacen("http", "ws", 1);
        let url = match &self.token {
            // Browsers cannot set headers on a WebSocket, so the core also takes it in the query.
            Some(token) => format!("{base}/api/stream?token={token}"),
            None => format!("{base}/api/stream"),
        };
        let mut request = url.into_client_request()?;
        if let Some(token) = &self.token {
            request.headers_mut().insert(header::AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {token}"))?);
        }
        let (ws, _) = tokio::time::timeout(Duration::from_secs(20), tokio_tungstenite::connect_async(request))
            .await
            .map_err(|_| anyhow!("the stream did not open"))??;
        Ok(ws)
    }
}
