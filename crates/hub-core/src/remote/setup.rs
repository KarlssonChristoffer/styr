//! Adding, changing and removing other machines: `add` for one whose hubd answers, `set_up` that
//! gets a machine over SSH running threads without more clicks (key, install, tailnet name),
//! putting a machine on the tailnet (`join_tailnet`), and the per-host settings changed from the
//! app (`change`, `PATCH /api/hosts/{name}`).

use super::*;

impl Remotes {
    pub async fn add(self: &Arc<Self>, new: NewHost) -> Result<HostInfo> {
        let name = new.name.trim().to_owned();
        if name.is_empty() {
            bail!("give the machine a name");
        }
        let key = host_key(&name);
        if name.eq_ignore_ascii_case(&self.local_name)
            || self.find(&name).is_some()
            || self.hosts.locked().values().any(|r| host_key(&r.config.name) == key)
        {
            bail!("there is already a machine called {name}");
        }
        let ssh = new.ssh.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
        let url = new.url.map(|s| s.trim().trim_end_matches('/').to_owned()).filter(|s| !s.is_empty());
        if ssh.is_none() && url.is_none() {
            bail!("give an SSH host, a URL or both");
        }
        if let Some(url) = &url
            && !url.starts_with("http://")
            && !url.starts_with("https://")
        {
            bail!("the URL should start with http:// or https://");
        }
        let code = new.code.map(|c| c.trim().to_owned()).filter(|c| !c.is_empty());
        let token = match (&code, &url) {
            (Some(code), Some(url)) => Some(self.redeem(url, code).await?),
            (Some(_), None) => bail!("a pairing code needs the machine's URL"),
            (None, _) => None,
        };
        let config = HostConfig { name, ssh, url, port: 7878, token, push_quiet_mins: None };
        let mut added = self.added().await?;
        added.push(config.clone());
        self.save_added(added).await?;
        let remote = self.launch(config, true);
        Ok(remote.info.locked().clone())
    }

    /// Changes a machine added from the app (`PATCH /api/hosts/{name}`): saved with the other added
    /// machines and put on its info, which the ten-minute check reads, so it counts at the next
    /// check without a restart. A machine from `hub.json` is changed there.
    pub async fn change(&self, name: &str, settings: HostSettings) -> Result<HostInfo> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        if !remote.removable {
            bail!("{name} is set in hub.json; change it there");
        }
        let mut added = self.added().await?;
        let entry = added.iter_mut().find(|h| h.name == remote.config.name).with_context(|| format!("{name} is not among the machines added here"))?;
        if let Some(mins) = settings.push_quiet_mins {
            entry.push_quiet_mins = (mins > 0).then_some(mins);
        }
        let gap = entry.push_quiet_mins;
        self.save_added(added).await?;
        let info = {
            let mut info = remote.info.locked();
            info.push_quiet_mins = gap;
            info.clone()
        };
        // No window listening is not a failure: the next list has it.
        self.stream.send(StreamMsg::Host { host: info.clone() }).ok();
        Ok(info)
    }

    /// The gap between versions sent to `name` on its own, as its info has it now (`ship::quiet_left`).
    pub fn push_gap(&self, name: &str) -> Option<u64> {
        self.find(name).and_then(|r| r.info.locked().push_quiet_mins)
    }

    /// Adds a machine and gets it running threads without more clicks, as T3 Code does over SSH
    /// ("launch or reuse"): a machine whose hubd answers is just added; one without hubd gets it
    /// installed, built on this machine unless the user said otherwise before. What only a person
    /// can answer (a sudo password, SSH that lets nobody in) stays on the machine's row.
    ///
    /// SSH has to get in first. A machine that does not know this one's key yet asks for its
    /// password once, which puts the key there. The machine is named after its host name when
    /// no name is given, and reached by its tailnet name when it is on the tailnet, so it answers
    /// from anywhere and not only at home.
    pub async fn set_up(self: &Arc<Self>, mut new: NewHost) -> Result<SetUpAnswer> {
        let target = new.ssh.clone().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).context("give an SSH target")?;
        let quick = Duration::from_secs(20);
        let hostname = match ssh_run(&target, "hostname", None, quick).await {
            Ok(name) => name,
            Err(e) if crate::sshkey::wants_password(&format!("{e:#}")) => match new.password.take() {
                Some(password) => {
                    crate::sshkey::give_key(&target, &password).await?;
                    ssh_run(&target, "hostname", None, quick).await.context("logging in with the key just put there")?
                }
                None => {
                    return Ok(SetUpAnswer {
                        host: None,
                        password: Some(format!("{target} does not know this machine's SSH key yet. Its password, once, puts the key there.")),
                    });
                }
            },
            Err(e) => return Err(e.context(format!("reaching {target} over SSH"))),
        };
        let hostname = hostname.trim().split('.').next().unwrap_or_default().to_lowercase();
        if new.name.trim().is_empty() {
            // Its host name, unless that is taken (a second Pi still called raspberrypi).
            let taken = |n: &str| n.eq_ignore_ascii_case(&self.local_name) || self.find(n).is_some();
            new.name = if hostname.is_empty() || taken(&hostname) { target.rsplit('@').next().unwrap_or(&target).to_owned() } else { hostname.clone() };
        }
        let (user, host) = match target.rsplit_once('@') {
            Some((user, host)) => (Some(user.to_owned()), host.to_owned()),
            None => (None, target.clone()),
        };
        if !hostname.is_empty() && !host.to_lowercase().starts_with(&hostname) {
            let net = self.tailnet(&self.store).await;
            if let Some(peer) = net.peers.iter().find(|p| p.name.eq_ignore_ascii_case(&hostname) && p.machine.is_none() && !p.dns.is_empty()) {
                let there = match user {
                    Some(user) => format!("{user}@{}", peer.dns),
                    None => peer.dns.clone(),
                };
                // Only when the key gets in that way too.
                if ssh_run(&there, "true", None, quick).await.is_ok() {
                    new.ssh = Some(there);
                }
            }
        }
        let info = self.add(new).await?;
        let (remotes, name) = (self.clone(), info.name.clone());
        tokio::spawn(async move {
            let Some(remote) = remotes.find(&name) else { return };
            // The first connection says whether hubd is there: online, or SSH in without it.
            let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
            let setup = loop {
                let (state, setup) = {
                    let info = remote.info.locked();
                    (info.state, info.setup)
                };
                if state == HostState::Online {
                    remotes.join_quietly(&name).await;
                    return;
                }
                if setup.is_some() || state == HostState::Offline || tokio::time::Instant::now() > deadline {
                    break setup;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            };
            if !matches!(setup, Some(HostSetup::Missing | HostSetup::Failed)) {
                return;
            }
            let build_here = match remotes.store.get_value(BUILD_HERE_KEY).await {
                Ok(None) => Some(true),
                _ => None,
            };
            match remotes.install(&name, InstallRequest { password: None, build_here }).await {
                Ok(InstallAnswer { sudo: Some(why), .. }) => {
                    remote.set_state(&remotes, HostState::Offline, Some(format!("{why} Install hubd asks for the sudo password.")));
                }
                Ok(_) => remotes.join_quietly(&name).await,
                Err(e) => {
                    tracing::warn!("setting up {name}: {e:#}");
                    remote.set_state(&remotes, HostState::Offline, Some(format!("Setting up stopped: {e:#}")));
                }
            }
        });
        Ok(SetUpAnswer { host: Some(info), password: None })
    }

    /// Puts a machine that SSH reaches on the tailnet: installs Tailscale there when it is
    /// missing and starts it, and the link Tailscale gives for signing it in shows on the row
    /// until the person has approved it. Needs root there: sudo without a password, or the
    /// password given (fed to sudo, kept nowhere). Without either, says so and starts nothing.
    pub async fn join_tailnet(self: &Arc<Self>, name: &str, password: Option<String>) -> Result<Option<String>> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        let alias = remote.config.ssh.clone().with_context(|| format!("{name} has no SSH alias to reach it by"))?;
        if remote.info.locked().join.as_ref().is_some_and(|j| !j.failed) {
            return Ok(None);
        }
        let quick = Duration::from_secs(60);
        let check = ssh_run(
            &alias,
            &format!("{REMOTE_DATA}; mkdir -p \"$D\" && cat > \"$D/join-tailnet.sh\"; {JOIN_CHECK}"),
            Some(JOIN_SCRIPT.as_bytes().to_vec()),
            quick,
        )
        .await
        .context("reaching it over SSH")?;
        if check.lines().any(|l| l == "on" || l == "mac") {
            remote.joining(self, None);
            return Ok(None);
        }
        let sudo = if check.lines().any(|l| l == "root") {
            "$([ \"$(id -u)\" = 0 ] || echo sudo -n)"
        } else if let Some(password) = &password {
            ssh_run(&alias, "sudo -S -p '' -v", Some(format!("{password}\n").into_bytes()), quick)
                .await
                .map_err(|_| anyhow!("sudo on {name} did not take that password"))?;
            "sudo -S -p ''"
        } else {
            return Ok(Some(format!("Hub needs sudo on {name} once to install and start Tailscale.")));
        };
        let step = if check.lines().any(|l| l == "missing") { "installing Tailscale" } else { "starting Tailscale" };
        remote.joining(self, Some(TailnetJoin { step: step.to_owned(), login_url: None, failed: false }));
        let (remotes, task) = (self.clone(), remote.clone());
        tokio::spawn(async move {
            let run = format!(
                "{REMOTE_DATA}; cd \"$D\" || exit 1; : > tailscale.log; {sudo} sh \"$D/join-tailnet.sh\" \"$(id -un)\" >> tailscale.log 2>&1"
            );
            let input = password.map(|p| format!("{p}\n").into_bytes());
            let joined = ssh_run(&alias, &run, input, Duration::from_secs(30 * 60));
            tokio::pin!(joined);
            let mut look = tokio::time::interval(Duration::from_secs(3));
            let result = loop {
                tokio::select! {
                    done = &mut joined => break done,
                    _ = look.tick() => {
                        let log = ssh_run(&alias, &format!("{REMOTE_DATA}; tail -n 40 \"$D/tailscale.log\""), None, quick).await.unwrap_or_default();
                        if let Some(url) = login_url(&log) {
                            task.joining(&remotes, Some(TailnetJoin {
                                step: "waiting for you to sign it in".to_owned(),
                                login_url: Some(url),
                                failed: false,
                            }));
                        }
                    }
                }
            };
            match result {
                Ok(_) => {
                    tracing::info!("{} is on the tailnet", task.config.name);
                    task.joining(&remotes, None);
                }
                Err(e) => {
                    let log = ssh_run(&alias, &format!("{REMOTE_DATA}; tail -n 5 \"$D/tailscale.log\""), None, quick).await.unwrap_or_default();
                    let why = log.lines().map(str::trim).rfind(|l| !l.is_empty()).map(str::to_owned).unwrap_or_else(|| format!("{e:#}"));
                    tracing::warn!("putting {} on the tailnet: {why}", task.config.name);
                    task.joining(&remotes, Some(TailnetJoin { step: why, login_url: None, failed: true }));
                }
            }
        });
        Ok(None)
    }

    /// Puts a machine being set up on the tailnet too, when that needs no question; otherwise
    /// its row offers it.
    pub(super) async fn join_quietly(self: &Arc<Self>, name: &str) {
        match self.join_tailnet(name, None).await {
            Ok(None) => {}
            Ok(Some(why)) => {
                if let Some(remote) = self.find(name) {
                    remote.joining(self, Some(TailnetJoin { step: why, login_url: None, failed: true }));
                }
            }
            Err(e) => tracing::info!("not putting {name} on the tailnet: {e:#}"),
        }
    }

    pub async fn remove(&self, name: &str) -> Result<()> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        if !remote.removable {
            bail!("{name} is set in hub.json; remove it there");
        }
        let mut added = self.added().await?;
        added.retain(|h| h.name != remote.config.name);
        self.save_added(added).await?;
        if let Err(e) = self.vault.delete(&host_key(&remote.config.name)).await {
            tracing::warn!("removing the token of {}: {e:#}", remote.config.name);
        }
        self.hosts.locked().remove(&remote.config.name);
        if let Some(task) = remote.task.locked().take() {
            // Dropping the task drops the tunnel, which kills ssh.
            task.abort();
        }
        let gone: Vec<ThreadId> = remote.threads.locked().drain().map(|(id, _)| id).collect();
        for id in gone {
            self.stream.send(StreamMsg::ThreadDeleted { id }).ignore("no window may be listening");
        }
        self.stream.send(StreamMsg::HostRemoved { name: remote.config.name.clone() }).ignore("no window may be listening");
        Ok(())
    }
}
