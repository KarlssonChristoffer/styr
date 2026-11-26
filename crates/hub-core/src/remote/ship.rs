//! Bringing other machines to this machine's hubd version, top to bottom: where that version
//! comes from (`Ship`), how a host takes versions (`takes`), Install hubd for a host without one,
//! the ten-minute check and Update now, sending the version the way the host takes it (hubd built
//! here, or the source for its own updater), asking its updater to look, and following that update
//! until it is in. Then what Settings > Updates reads of it (the note and track per host), and the
//! shell commands and hub.json edits the sending runs on the host.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use super::bundle::{bundle, version_bundle};
use super::ssh::{Lacks, install_script, read_check, ssh_run};
use super::{BUILD_HERE_KEY, REMOTE_DATA, Remote, Remotes, STOP_INSTALLER, TooOld};
use crate::locked::{LockExt, ResultExt};
use crate::model::{HostSetup, HostState, InstallAnswer, InstallRequest, UpdateStatus, UpdateStep, UpdateTrack, now_ms};

/// Where this machine's hubd comes from, for building it for other machines.
pub struct Ship {
    pub data: std::path::PathBuf,
    /// The updater's clone.
    pub source: std::path::PathBuf,
    pub branch: String,
    /// For the build folder and its limits, as the updater uses them.
    pub update: crate::config::UpdateConfig,
    pub builder: crate::crossbuild::Builder,
}

impl Ship {
    /// The version other machines get: this one's own, or for a development build the branch's
    /// tip in the clone.
    async fn version(&self) -> Result<String> {
        let running = hub_install::running_version();
        if running != "dev" {
            return Ok(running);
        }
        let (source, rev) = (self.source.clone(), format!("origin/{}", self.branch));
        match tokio::task::spawn_blocking(move || crate::git::short_commit(&source, &rev)).await? {
            Some(short) => Ok(short),
            None => bail!("no origin/{} in {}", self.branch, self.source.display()),
        }
    }
}

/// How a host takes new versions: hubd built for its `target` by this machine (`update.pushed`
/// there), or the source for its own updater to build.
enum Takes {
    Builds(&'static str),
    /// It builds for itself; the target is there when this machine could build for it instead.
    Source(Option<&'static str>),
    Neither,
}

async fn takes(alias: &str) -> Result<Takes> {
    let answer = ssh_run(
        alias,
        &format!(
            "{REMOTE_DATA}; if grep -q '\"pushed\": true' \"$D/hub.json\" 2>/dev/null; then echo builds \"$(uname -m)\"; \
             elif [ -d \"$D/src/.git\" ]; then echo source \"$(uname -s)-$(uname -m)\"; fi"
        ),
        None,
        Duration::from_secs(60),
    )
    .await
    .context("asking how it updates")?;
    let mut words = answer.split_whitespace();
    Ok(match (words.next(), words.next()) {
        (Some("builds"), Some(arch)) => crate::crossbuild::target(arch).map_or(Takes::Neither, Takes::Builds),
        (Some("source"), arch) => Takes::Source(arch.and_then(|a| a.strip_prefix("Linux-")).and_then(crate::crossbuild::target)),
        _ => Takes::Neither,
    })
}

impl Remotes {
    /// Installs hubd on a host that SSH reaches but that has no hubd: sends `install-host.sh`,
    /// checks what the host lacks, and starts it in the background. The host shows its latest
    /// step until hubd answers. Installing again after a failure carries on where it stopped.
    ///
    /// What only a person can fix fails at once, all of it together. Two things are asked
    /// before anything starts, each once: whether this machine may build hubd for the host
    /// (`build_here`; then the host needs no Rust and gets updates from here, see `crossbuild`),
    /// and the sudo password when the host needs root for packages. The password is fed to sudo
    /// and kept nowhere.
    pub async fn install(self: &Arc<Self>, name: &str, request: InstallRequest) -> Result<InstallAnswer> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        let alias = remote.config.ssh.clone().with_context(|| format!("{name} has no SSH alias to install over"))?;
        let quick = Duration::from_secs(120);
        let arch = ssh_run(
            &alias,
            &format!("{REMOTE_DATA}; mkdir -p \"$D\" && cat > \"$D/install-host.sh\" && uname -sm"),
            Some(install_script().into_bytes()),
            quick,
        )
        .await
        .context("sending the installer")?;
        let target = crate::crossbuild::target(arch.lines().last().unwrap_or_default());
        if let Some(yes) = request.build_here {
            self.store.set_value(BUILD_HERE_KEY, if yes { "yes" } else { "no" }).await?;
        }
        let build_here = match (target, self.store.get_value(BUILD_HERE_KEY).await?.as_deref()) {
            (None, _) => false,
            (Some(_), Some(answer)) => answer == "yes",
            (Some(target), None) => {
                return Ok(InstallAnswer { sudo: None, build: Some(format!("{name} is {target}.")) });
            }
        };
        let env = if build_here { "HUB_PREBUILT=1 " } else { "" };
        let lacks = read_check(
            &ssh_run(&alias, &format!("{REMOTE_DATA}; {env}bash \"$D/install-host.sh\" --check"), None, quick)
                .await
                .context("checking the machine")?,
        );
        if !lacks.problems.is_empty() {
            bail!("{name} needs this first: {}", lacks.problems.join("; "));
        }
        let root = if !lacks.needs_root() {
            None
        } else if lacks.sudo {
            Some(None)
        } else if let Some(password) = request.password {
            // Checked here, so a typo says so at once instead of in the log later.
            ssh_run(&alias, "sudo -S -p '' -v", Some(format!("{password}\n").into_bytes()), quick)
                .await
                .map_err(|_| anyhow!("sudo on {name} did not take that password"))?;
            Some(Some(password))
        } else {
            return Ok(InstallAnswer { sudo: Some(lacks.sudo_for(name)), build: None });
        };
        let version = if build_here { Some(self.ship.version().await?) } else { None };
        tracing::info!("installing hubd on {name} over ssh {alias}");
        // A host that is online keeps serving its threads until the installer restarts it (an
        // update of an older hubd); only one without hubd shows the steps.
        if remote.info.locked().state != HostState::Online {
            remote.info.locked().setup = Some(HostSetup::Installing);
            remote.set_state(self, HostState::Connecting, Some("Installing hubd: starting".to_owned()));
        }
        tokio::spawn(self.clone().run_install(remote.clone(), alias, (root, lacks), (version, target)));
        Ok(InstallAnswer { sudo: None, build: None })
    }

    /// The installer on the host, in the background: packages as root, then hubd (built here or
    /// from the source), then the tailnet. The host's probe reads how it went from the host.
    async fn run_install(
        self: Arc<Self>,
        task: Arc<Remote>,
        alias: String,
        (root, lacks): (Option<Option<String>>, Lacks),
        (version, target): (Option<String>, Option<&'static str>),
    ) {
        let quick = Duration::from_secs(120);
        let started = async {
            // The packages first, as root, with the log and pid where the probe looks; apt on
            // a Pi can take many minutes.
            let sudo = match &root {
                None => None,
                // Split into words by bash and by a Mac's zsh alike, which `$R` would not be.
                Some(None) => Some("$([ \"$(id -u)\" = 0 ] || echo sudo -n)"),
                Some(Some(_)) => Some("sudo -S -p ''"),
            };
            // Installing again replaces an installer that still runs (one stuck waiting, say),
            // with everything it started, instead of running a second one beside it.
            let log = format!(
                "{REMOTE_DATA}; mkdir -p \"$D\" && cd \"$D\" || exit 1; {STOP_INSTALLER}; : > install.log; echo $$ > install.pid"
            );
            match sudo {
                Some(sudo) => {
                    let packages = lacks.packages.join(" ");
                    let command = format!("{log}; {sudo} bash \"$D/install-host.sh\" --root \"$(id -un)\" {packages} >> install.log 2>&1");
                    let input = root.clone().flatten().map(|p| format!("{p}\n").into_bytes());
                    // The Command Line Tools on an old Mac can take longer than apt on a Pi.
                    ssh_run(&alias, &command, input, Duration::from_secs(60 * 60)).await.context("installing packages")?;
                }
                None => {
                    ssh_run(&alias, &log, None, quick).await?;
                }
            }
            let env = match (&version, target) {
                (Some(version), Some(target)) => {
                    let hubd = self.build_for(&task, target, version).await?;
                    task.busy(&self, Some(format!("sending hubd {version} for {target}")));
                    ssh_run(&alias, &format!("{REMOTE_DATA}; cat > \"$D/hubd.prebuilt\""), Some(hubd), Duration::from_secs(600))
                        .await
                        .context("sending hubd")?;
                    format!("HUB_PREBUILT=\"$D/hubd.prebuilt\" HUB_PREBUILT_VERSION={version} ")
                }
                _ => match bundle(&self.ship.source, &self.ship.branch).await {
                    Some(bundle) => {
                        ssh_run(&alias, &format!("{REMOTE_DATA}; cat > \"$D/hub.bundle\""), Some(bundle), quick)
                            .await
                            .context("sending the repository")?;
                        "HUB_BUNDLE=\"$D/hub.bundle\" ".to_owned()
                    }
                    None => String::new(),
                },
            };
            task.busy(&self, None);
            // Detached, so it keeps going when this SSH session ends; its pid says whether it
            // still is.
            let branch = &self.ship.branch;
            let start = format!(
                "{REMOTE_DATA}; cd \"$D\" || exit 1; {env}HUB_BRANCH={branch} $(command -v setsid) nohup bash \"$D/install-host.sh\" >> \"$D/install.log\" 2>&1 < /dev/null & echo $! > \"$D/install.pid\""
            );
            ssh_run(&alias, &start, None, quick).await.context("starting the installer")
        };
        let result = started.await;
        // The packages are in, so apt is free: put the machine on the tailnet too, with the
        // same root, when it is not on it yet.
        if result.is_ok() && root.is_some() {
            match self.join_tailnet(&task.config.name, root.clone().flatten()).await {
                Ok(_) => {}
                Err(e) => tracing::info!("not putting {} on the tailnet: {e:#}", task.config.name),
            }
        }
        if let Err(e) = result {
            tracing::warn!("installing hubd on {}: {e:#}", task.config.name);
            task.busy(&self, None);
            // Where the probe looks, so the host's row says what stopped it.
            let line = format!("Installing hubd stopped on the PC: {e:#}\n").replace('\n', " ").trim().to_owned() + "\n";
            ssh_run(&alias, &format!("{REMOTE_DATA}; cat >> \"$D/install.log\""), Some(line.into_bytes()), quick).await.warn_on_err("note the failure in install.log");
        }
        // The probe reads how it went from the host itself, failure or not.
        task.wake.notify_one();
    }

    /// Keeps the other machines on this machine's version, checked when a host comes online and
    /// every ten minutes. A host this machine builds for (`update.pushed` there) gets hubd built
    /// and sent; one that builds for itself gets the source sent, so its updater never needs a
    /// GitHub login there. Either way the host's updater switches once no thread is working there.
    pub(super) async fn push_updates(self: Arc<Self>) {
        loop {
            tokio::select! {
                _ = self.push_wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(600)) => {}
            }
            let Ok(version) = self.ship.version().await else { continue };
            let hosts: Vec<Arc<Remote>> = self.hosts.locked().values().cloned().collect();
            for remote in hosts {
                let (online, theirs) = {
                    let info = remote.info.locked();
                    (info.state == HostState::Online, info.version.clone())
                };
                let name = remote.config.name.clone();
                if theirs.as_deref() == Some(version.as_str()) {
                    self.notes.locked().remove(&name);
                    continue;
                }
                let Some(alias) = remote.config.ssh.clone() else {
                    self.note(&name, "no SSH alias to send hubd over; add one on its page, or update it there by hand");
                    continue;
                };
                let sent = self.sent.locked().get(&name) == Some(&version);
                if !online || sent {
                    continue;
                }
                // A host with a quiet gap (`push_quiet_mins`) gets at most one version per gap; the
                // clock is when the last one was delivered, kept across this core's own restarts.
                let last = self.store.get_value(&pushed_at_key(&name)).await.ok().flatten().and_then(|v| v.parse().ok());
                // The host's info, not its config: a change from its page counts at the next check.
                if let Some(mins) = quiet_left(self.push_gap(&name), last, theirs.is_some(), now_ms()) {
                    self.note(&name, format!("Next version in {mins} min"));
                    continue;
                }
                let holding = self.failed.locked().get(&name).is_some_and(|(v, _, next)| *v == version && now_ms() < *next);
                // This machine's own build and a cross build each take the memory the linker needs
                // (zig's linker died with "out of memory" when they ran together), so it waits.
                if holding || (crate::update_signals::building() && !matches!(takes(&alias).await, Ok(Takes::Source(None)))) {
                    continue;
                }
                match self.push(&remote, &alias, &version).await {
                    Ok(_) => {
                        self.failed.locked().remove(&name);
                    }
                    Err(e) => {
                        let memory = crate::crossbuild::out_of_memory(&format!("{e:#}"));
                        let (times, wait) = {
                            let mut failed = self.failed.locked();
                            let before = failed.get(&name).filter(|(v, _, _)| *v == version).map_or(0, |(_, n, _)| *n);
                            // Out of memory passes: wait the base ten minutes again, no back-off, no failure counted.
                            let times = if memory { before } else { before + 1 };
                            let wait = retry_after(times.max(1));
                            failed.insert(name.clone(), (version.clone(), times, now_ms() + wait.as_millis() as i64));
                            (times, wait)
                        };
                        let mins = wait.as_secs() / 60;
                        let what = if memory { "ran out of memory".to_owned() } else { format!("failed (try {times}") };
                        tracing::warn!("sending {version} to {name} {what}, next try in {mins} min: {e:#}");
                        let what = if memory { what } else { format!("{what})") };
                        self.note(&name, format!("sending hubd {} {what}, next in {mins} min: {e:#}", crate::text::short_version(&version)));
                        // One card for the host, however many tries: Problems groups by this fingerprint.
                        crate::problem_inbox::capture(send_problem(&name, &version, times, mins, &format!("{e:#}")));
                    }
                }
            }
        }
    }

    /// "Update now" for a host: sends it this machine's version the way it takes updates and has
    /// its updater look at once. A host without hubd, or with an install that neither builds from
    /// a clone nor takes builds, gets Install hubd instead.
    pub async fn update(self: &Arc<Self>, name: &str) -> Result<InstallAnswer> {
        let remote = self.find(name).with_context(|| format!("no machine called {name}"))?;
        let alias = remote.config.ssh.clone().with_context(|| format!("{name} has no SSH alias to update over"))?;
        let version = self.ship.version().await?;
        // Asked for by hand: the back-off ends.
        self.failed.locked().remove(&remote.config.name);
        let online = remote.info.locked().state == HostState::Online;
        match (online, takes(&alias).await?) {
            (true, Takes::Source(_)) => {
                // Building it here and switching it over takes minutes; its row says how it goes.
                let (remotes, remote) = (self.clone(), remote.clone());
                tokio::spawn(async move {
                    if let Err(e) = remotes.push(&remote, &alias, &version).await {
                        tracing::warn!("sending {version} to {}: {e:#}", remote.config.name);
                        remotes.note(&remote.config.name, format!("sending hubd {} failed: {e:#}", crate::text::short_version(&version)));
                        remote.set_state(&remotes, HostState::Online, Some(format!("Sending hubd {version} failed: {e:#}")));
                    }
                });
            }
            // Building hubd takes minutes; the host's row says how it goes.
            (true, Takes::Builds(_)) if self.store.get_value(BUILD_HERE_KEY).await?.as_deref() == Some("yes") => {
                let (remotes, remote) = (self.clone(), remote.clone());
                tokio::spawn(async move {
                    if let Err(e) = remotes.push(&remote, &alias, &version).await {
                        tracing::warn!("sending {version} to {}: {e:#}", remote.config.name);
                        remotes.note(&remote.config.name, format!("sending hubd {} failed: {e:#}", crate::text::short_version(&version)));
                        remote.set_state(&remotes, HostState::Online, Some(format!("Sending hubd {version} failed: {e:#}")));
                    }
                });
            }
            _ => return self.install(name, InstallRequest::default()).await,
        }
        Ok(InstallAnswer { sudo: None, build: None })
    }

    /// Puts `version` where the host's updater looks and has it look now. False when the host
    /// takes updates neither way (or wants builds and this machine was told not to build).
    async fn push(&self, remote: &Remote, alias: &str, version: &str) -> Result<bool> {
        // One send per host at a time: the ten-minute check and Update now both send, and one
        // moved the files away while the other still wrote them (problem #7).
        let lock = self.pushing.locked().entry(remote.config.name.clone()).or_default().clone();
        let _one = lock.lock().await;
        let name = remote.config.name.clone();
        self.set_track(&name, crate::update_track::begin(version, format!("{} builds and sends it", self.local_name), now_ms()));
        let sent = self.send_version(remote, alias, version).await;
        match &sent {
            Ok(true) => {
                if let Some(track) = self.track(&name).filter(|t| t.version == version) {
                    self.set_track(&name, crate::update_track::delivered(track, now_ms()));
                }
                // The track's `sent_at` lives in memory, and this core restarts for its own
                // versions; the quiet gap's clock must outlive that.
                self.store.set_value(&pushed_at_key(&name), &now_ms().to_string()).await.warn_on_err("remember when a version was sent");
                // Its updater looks every few minutes; now is sooner, and its answer is the receipt.
                self.ask_to_look(remote, version).await;
            }
            // Nothing was sent (the host takes updates neither way, or waits for a yes): the note says why.
            Ok(false) => {
                self.tracks.locked().remove(&name);
            }
            Err(e) => {
                let track = crate::update_track::begin(version, "", now_ms());
                self.set_track(&name, crate::update_track::failed(track, format!("sending failed: {e:#}"), now_ms()));
            }
        }
        sent
    }

    /// The sending itself, see `push`.
    async fn send_version(&self, remote: &Remote, alias: &str, version: &str) -> Result<bool> {
        match takes(alias).await? {
            Takes::Builds(target) => {
                if self.store.get_value(BUILD_HERE_KEY).await?.as_deref() != Some("yes") {
                    self.note(
                        &remote.config.name,
                        format!("it takes hubd built on another machine, and {} may not build it: press Update to say yes", self.local_name),
                    );
                    return Ok(false);
                }
                let hubd = self.build_for(remote, target, version).await;
                remote.busy(self, None);
                ssh_run(alias, &hubd_put(version), Some(hubd?), Duration::from_secs(600)).await.context("sending hubd")?;
            }
            // Building hubd on the machine itself takes gigabytes and up to an hour on a Raspberry
            // Pi. When this machine can build for it, it does, and the host stops building.
            Takes::Source(Some(target)) if self.store.get_value(BUILD_HERE_KEY).await?.as_deref() != Some("no") => {
                self.switch_to_builds(remote, alias, target, version).await?;
            }
            Takes::Source(_) => {
                remote.sending(self, Some(format!("sending the source of {version} from {}", self.local_name)));
                let sent = async {
                    // This machine tested the version, so the host only builds it: an install
                    // from before that still runs the tests first, which doubles its build.
                    let config = ssh_run(alias, &format!("{REMOTE_DATA}; cat \"$D/hub.json\" 2>/dev/null || true"), None, Duration::from_secs(60)).await?;
                    if let Some(config) = without_test_step(&config) {
                        ssh_run(alias, &format!("{REMOTE_DATA}; cat > \"$D/hub.json.new\" && mv \"$D/hub.json.new\" \"$D/hub.json\""), Some(config.into_bytes()), Duration::from_secs(60))
                            .await
                            .context("writing its hub.json")?;
                    }
                    let (bundle, name) = version_bundle(&self.ship.source, version).await?;
                    let put = source_put(&name, &self.ship.branch);
                    ssh_run(alias, &put, Some(bundle), Duration::from_secs(600)).await.context("sending the source")
                }
                .await;
                remote.sending(self, None);
                sent?;
            }
            Takes::Neither => {
                self.note(&remote.config.name, "its hubd neither builds from a clone nor takes builds: press Update to install it again");
                return Ok(false);
            }
        }
        tracing::info!("sent {version} to {}", remote.config.name);
        self.note(&remote.config.name, format!("sent hubd {} from {}; its updater builds it and switches when no thread works there", crate::text::short_version(version), self.local_name));
        self.sent.locked().insert(remote.config.name.clone(), version.to_owned());
        Ok(true)
    }

    /// Moves a host that builds hubd for itself over to taking it built from this machine: sends
    /// hubd `version` built here, sets `update.pushed` in its hub.json (keeping everything else
    /// there), frees its build folders and restarts its hubd on the new version. Its threads
    /// carry on after the restart.
    async fn switch_to_builds(&self, remote: &Remote, alias: &str, target: &str, version: &str) -> Result<()> {
        let name = remote.config.name.clone();
        let hubd = self.build_for(remote, target, version).await;
        remote.busy(self, None);
        let hubd = hubd?;
        let config = ssh_run(alias, &format!("{REMOTE_DATA}; cat \"$D/hub.json\" 2>/dev/null || true"), None, Duration::from_secs(60))
            .await
            .context("reading its hub.json")?;
        let config = pushed_config(&config)?;
        remote.sending(self, Some(format!("sending hubd {} from {}", crate::text::short_version(version), self.local_name)));
        let sent = async {
            ssh_run(alias, &hubd_put(version), Some(hubd), Duration::from_secs(600)).await.context("sending hubd")?;
            ssh_run(alias, &format!("{REMOTE_DATA}; cat > \"$D/hub.json.new\" && mv \"$D/hub.json.new\" \"$D/hub.json\""), Some(config.into_bytes()), Duration::from_secs(60))
                .await
                .context("writing its hub.json")?;
            // The build folders go (gigabytes), then the service starts the new version.
            let switch = format!(
                "{REMOTE_DATA}; rm -rf \"$D/src/target\" \"$D/bootstrap\"; printf {version} > \"$D/current.tmp\" && mv \"$D/current.tmp\" \"$D/current\" \
                 && (systemctl --user restart hubd.service >/dev/null 2>&1 < /dev/null &)"
            );
            ssh_run(alias, &switch, None, Duration::from_secs(300)).await.context("switching it over")
        }
        .await;
        remote.sending(self, None);
        sent?;
        tracing::info!("{name} now takes hubd built on {}", self.local_name);
        self.note(&name, format!("now takes hubd built on {} instead of building it itself; its build folder is gone", self.local_name));
        Ok(())
    }

    /// hubd `version` built here for `target`, installing the tools first when they are missing
    /// (the user said yes to that). The host's row says what is going on meanwhile.
    async fn build_for(&self, remote: &Remote, target: &str, version: &str) -> Result<Vec<u8>> {
        let ship = &self.ship;
        let missing = ship.builder.missing(&ship.data, target).await;
        if !missing.is_empty() {
            remote.busy(self, Some(format!("installing {} on {} (once)", missing.join(", "), self.local_name)));
            ship.builder.install_tools(&ship.data, target).await?;
        }
        // In the updater's build folder, with its limits: room before, trimmed after.
        let dir = crate::disk::target_dir(&ship.update, &ship.source).join(crate::crossbuild::DIR);
        crate::disk::check_space(&ship.update, &dir)?;
        remote.busy(self, Some(format!("building hubd {version} for {target} on {}", self.local_name)));
        let built = ship.builder.build(&ship.data, &dir, &ship.source, version, target).await;
        // Not while this machine's own update builds: trimming empties `target/debug`, and its
        // `cargo test` then lost crates mid-run ("can't find crate", one wasted run each time).
        // The update trims when it is done.
        if !crate::update_signals::building() {
            let (cfg, src) = (ship.update.clone(), ship.source.clone());
            tokio::task::spawn_blocking(move || crate::disk::trim(&cfg, &src)).await.warn_on_err("trim the build cache");
        }
        built
    }

    /// Tells the host's updater which version was sent and to look now, and takes its answer as
    /// the receipt. A hubd from before `update.look` cannot answer that; its updater looks by
    /// itself every few minutes, which the track says.
    async fn ask_to_look(&self, remote: &Remote, version: &str) {
        let name = remote.config.name.clone();
        let conn = remote.conn.locked().clone();
        let Some(conn) = conn else { return };
        match conn.post_json::<UpdateStatus>(&self.client, "/api/update/look", &serde_json::json!({ "version": version })).await {
            Ok(status) => self.observe(&name, true, Some(&status), None),
            Err(e) if e.downcast_ref::<TooOld>().is_some() => {
                tracing::info!("{name}'s hubd cannot confirm an update; its updater looks by itself");
                // What the old ask did, for a host that has it.
                conn.post(&self.client, "/api/update/retry").await.warn_on_err("ask the remote to retry the update");
                let mut tracks = self.tracks.locked();
                if let Some(track) = tracks.get_mut(&name).filter(|t| t.version == version && t.step == UpdateStep::Sent) {
                    track.detail = Some("delivered; its hubd is from before receipts, so its updater looks by itself every few minutes".into());
                }
            }
            Err(e) => tracing::info!("asking {name} to look for {version}: {e:#}"),
        }
    }

    /// Follows each version sent to a host on the host's own updater every few seconds while it
    /// moves, so its step is right with Settings > Updates closed too, and the step a person sees
    /// there is the one the machine reported, not the last one this machine wrote down.
    pub(super) async fn follow_updates(self: Arc<Self>) {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let names: Vec<String> = self.tracks.locked().iter().filter(|(_, t)| crate::update_track::moving(t)).map(|(n, _)| n.clone()).collect();
            for name in names {
                let Some(remote) = self.find(&name) else { continue };
                let online = remote.conn.locked().is_some();
                let version = remote.info.locked().version.clone();
                let status = if online { self.get_from::<UpdateStatus>(&name, "/api/version").await.ok() } else { None };
                self.observe(&name, online, status.as_ref(), version.as_deref());
            }
        }
    }

    /// Looks at once whether a host needs this machine's version, instead of within ten minutes.
    /// For a host that turned out too old for something.
    pub fn wake_push(&self) {
        self.push_wake.notify_one();
    }

    /// The hubd version this machine brings the others to.
    pub async fn ship_version(&self) -> Result<String> {
        self.ship.version().await
    }

    /// What happened when this machine last tried to bring `name` to its version.
    pub fn hubd_note(&self, name: &str) -> Option<String> {
        self.notes.locked().get(name).cloned()
    }

    fn note(&self, name: &str, note: impl Into<String>) {
        self.notes.locked().insert(name.to_owned(), note.into());
    }

    /// Says on the machine's update cell why an update did not start, for the page to show.
    pub(crate) fn say(&self, name: &str, note: impl Into<String>) {
        self.note(name, note);
    }

    /// How far the version sent to `name` has got there, if one was sent.
    pub fn track(&self, name: &str) -> Option<UpdateTrack> {
        self.tracks.locked().get(name).cloned()
    }

    fn set_track(&self, name: &str, track: UpdateTrack) {
        self.tracks.locked().insert(name.to_owned(), track);
    }

    /// Takes what a look at `name` showed (its updater's state, if it answered) into the track of
    /// the version sent there.
    pub fn observe(&self, name: &str, online: bool, status: Option<&UpdateStatus>, version: Option<&str>) {
        let mut tracks = self.tracks.locked();
        let Some(track) = tracks.get(name) else { return };
        let next = crate::update_track::advance(track, &crate::update_track::Seen { online, status, version }, now_ms());
        if next != *track {
            tracing::info!("update {} on {name}: {:?}{}", next.version, next.step, next.detail.as_deref().map(|d| format!(" ({d})")).unwrap_or_default());
            tracks.insert(name.to_owned(), next);
        }
    }
}

/// What a host runs to take hubd `version` on stdin into `bin/<version>`. Each send writes into
/// a folder of its own (`.partial.<pid>`) and moves it in whole, so a send that overlaps another
/// (from a second hubd here, say) never has its folder moved away mid-write.
pub(super) fn hubd_put(version: &str) -> String {
    format!(
        "{REMOTE_DATA}; B=\"$D/bin/{version}\"; P=\"$B.partial.$$\"; {{ mkdir -p \"$P\" && cat > \"$P/hubd\" && [ -s \"$P/hubd\" ] \
         && chmod 755 \"$P/hubd\" && printf {version} > \"$P/VERSION\" && printf hubd > \"$P/PROGRAMS\" && rm -rf \"$B\" && mv \"$P\" \"$B\"; }} \
         && printf {version} > \"$D/pushed\" || {{ rm -rf \"$P\"; exit 1; }}"
    )
}

/// The shell command on the host that takes a bundle from `version_bundle` on its stdin: into a
/// bare copy next to the clone, which becomes the clone's origin, so the updater fetches from it as
/// it would from GitHub.
pub(super) fn source_put(name: &str, branch: &str) -> String {
    format!(
        "{REMOTE_DATA}; M=\"$D/source.git\"; {{ [ -d \"$M\" ] || git init --quiet --bare \"$M\"; }} \
         && cat > \"$D/source.bundle\" && git -C \"$M\" fetch --quiet \"$D/source.bundle\" \"+{name}:refs/heads/{branch}\" \
         && git -C \"$D/src\" remote set-url origin \"$M\" && rm -f \"$D/source.bundle\""
    )
}

/// A host's hub.json with its updater taking builds from another machine (`update.pushed`)
/// instead of building: the build settings go, everything else (engines, hosts) stays.
pub(super) fn pushed_config(text: &str) -> Result<String> {
    let mut file: serde_json::Value = if text.trim().is_empty() { serde_json::json!({}) } else { serde_json::from_str(text).context("its hub.json is not JSON")? };
    let root = file.as_object_mut().context("its hub.json is not a JSON object")?;
    let update = root.entry("update").or_insert_with(|| serde_json::json!({}));
    let update = update.as_object_mut().context("its hub.json has an update that is not an object")?;
    for key in ["steps", "artifacts", "min_free_gb", "max_build_gb", "source_dir", "target_dir"] {
        update.remove(key);
    }
    update.insert("enabled".into(), true.into());
    update.insert("ask".into(), false.into());
    update.insert("pushed".into(), true.into());
    Ok(serde_json::to_string_pretty(&file)? + "\n")
}

/// A host's hub.json without the `cargo test` step its installer once wrote, or None when there
/// is none to drop (or the file is not JSON, which stays as it is).
pub(super) fn without_test_step(text: &str) -> Option<String> {
    let mut file: serde_json::Value = serde_json::from_str(text).ok()?;
    let steps = file.get_mut("update")?.get_mut("steps")?.as_array_mut()?;
    let before = steps.len();
    steps.retain(|s| s["command"].get(1).and_then(|c| c.as_str()) != Some("test"));
    if steps.len() == before || steps.is_empty() {
        return None;
    }
    Some(serde_json::to_string_pretty(&file).ok()? + "\n")
}

/// Where the time a version was last delivered to `host` is kept (ms).
fn pushed_at_key(host: &str) -> String {
    format!("remote_pushed_at:{host}")
}

/// Minutes until the ten-minute check may send `host` a new version, `None` when it may now.
/// With `quiet_mins` set, at most one version per that many minutes after `last_sent` (ms); a host
/// never sent to, or with no version at all, gets one at once. Update now does not ask.
pub(super) fn quiet_left(quiet_mins: Option<u64>, last_sent: Option<i64>, has_version: bool, now: i64) -> Option<u64> {
    let gap = quiet_mins.filter(|m| *m > 0)? as i64 * 60_000;
    let since = now - last_sent.filter(|_| has_version)?;
    let left = gap - since.max(0);
    (left > 0).then(|| (left as u64).div_ceil(60_000))
}

/// A version as the app shows it: a commit's first seven characters.
/// How long to wait after the `times`-th failed send in a row: ten minutes, doubling, at most six hours.
pub(super) fn retry_after(times: u32) -> Duration {
    Duration::from_secs(600 << times.saturating_sub(1).min(5)).min(Duration::from_secs(6 * 3600))
}

/// The problem a failed send to a host is, one per host.
fn send_problem(host: &str, version: &str, times: u32, mins: u64, why: &str) -> crate::problem_inbox::Incoming {
    use crate::problem_inbox::Incoming;
    use crate::model::problems::Hit;
    let memory = crate::crossbuild::out_of_memory(why);
    let message = if memory {
        format!("Building hubd {} for {host} ran out of memory on this machine; waiting {mins} min, then trying again", crate::text::short_version(version))
    } else {
        format!("Sending hubd {} to {host} failed {times} time(s); next try in {mins} min", crate::text::short_version(version))
    };
    Incoming {
        project: "Hub".into(),
        source: "host updates".into(),
        level: if memory { crate::model::problems::Level::Warning } else { crate::model::problems::Level::Error },
        folder: None,
        fingerprint: Some(vec!["host-update-send".into(), host.to_owned()]),
        hit: Hit {
            ts: now_ms(),
            message,
            kind: Some("host updates".into()),
            frames: Vec::new(),
            trace: Some(why.to_owned()),
            crumbs: Vec::new(),
            release: Some(version.to_owned()),
            environment: None,
            extra: serde_json::json!({ "machine": host, "version": version, "tries": times }),
        },
    }
}
