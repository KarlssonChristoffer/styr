//! The running side of the hub: the guard's and permission preferences, the update handshake,
//! parking and stopping engines, and starting a thread's engine.

use crate::locked::ResultExt;
use super::*;

impl Hub {
    pub async fn guard_prefs(&self) -> Result<GuardPrefs> {
        Ok(match self.store.get_value(GUARD).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => GuardPrefs::default(),
        })
    }

    pub async fn set_guard_prefs(&self, prefs: GuardPrefs) -> Result<GuardPrefs> {
        if !(1.0..=100.0).contains(&prefs.warn_at) || prefs.switch_at.is_some_and(|p| !(1.0..=100.0).contains(&p)) {
            bail!("limits are percentages from 1 to 100");
        }
        if prefs.fallback_model.trim().is_empty() {
            bail!("name a model to switch to");
        }
        self.store.set_value(GUARD, &serde_json::to_string(&prefs)?).await?;
        Ok(prefs)
    }

    pub async fn permission_prefs(&self) -> Result<PermissionPrefs> {
        Ok(match self.store.get_value(PERMISSIONS).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => PermissionPrefs::default(),
        })
    }

    pub async fn set_permission_prefs(&self, mut prefs: PermissionPrefs) -> Result<PermissionPrefs> {
        for list in [&mut prefs.allow, &mut prefs.deny, &mut prefs.protected] {
            let mut seen = std::collections::HashSet::new();
            list.retain_mut(|r| {
                *r = r.trim().to_owned();
                !r.is_empty() && seen.insert(r.clone())
            });
        }
        self.store.set_value(PERMISSIONS, &serde_json::to_string(&prefs)?).await?;
        Ok(prefs)
    }

    /// The model to move a thread to before its next message, when a plan limit is past the
    /// guard's line. Not when the user picked a model after the guard last switched this thread.
    /// Stale limits are refreshed for next time.
    /// Only threads on the engine the limits were read from: another engine has its own plan.
    pub(crate) async fn quota_switch(self: &Arc<Self>, thread_id: &str, engine: &str) -> Option<(String, crate::usage::PlanLimit)> {
        let prefs = self.guard_prefs().await.ok()?;
        let at = prefs.switch_at?;
        if !self.reads_plan_of(engine) {
            return None;
        }
        let (plan, stale) = self.usage.plan().unzip();
        if stale.unwrap_or(true) {
            self.usage_report(true).await.warn_on_err("usage report");
        }
        let limit = plan?.highest()?.clone();
        if limit.percent < at {
            return None;
        }
        let events = self.store.events_of(thread_id, 0, &[EventKind::QuotaSwitch, EventKind::Setting]).await.ok()?;
        let last = |f: &dyn Fn(&Event) -> bool| events.iter().rev().find(|e| f(e)).map(|e| e.seq);
        let switched = last(&|e| e.kind == EventKind::QuotaSwitch);
        let chosen = last(&|e| e.data_as::<crate::model::SettingData>().is_some_and(|s| s.config_id == "model" && s.by.as_deref() != Some("auto")));
        if switched.is_some() && chosen > switched {
            return None;
        }
        Some((prefs.fallback_model, limit))
    }

    /// Whether the plan limits Hub reads are `engine`'s.
    pub(crate) fn reads_plan_of(&self, engine: &str) -> bool {
        self.config.plan_engine().is_some_and(|(name, ..)| name == engine)
    }

    /// Keeps the plan limits current for the guard and the surfaces after a turn on `engine`;
    /// fetched at most every couple of minutes, and never for an engine Hub does not read them
    /// from.
    pub(crate) async fn after_turn_usage(self: &Arc<Self>, engine: &str) {
        if self.reads_plan_of(engine) {
            self.usage_report(true).await.warn_on_err("usage report");
        }
    }

    pub fn update_status(&self) -> UpdateStatus {
        self.update.locked().clone()
    }

    pub(crate) fn set_update(&self, f: impl FnOnce(&mut UpdateStatus)) {
        let status = {
            let mut status = self.update.locked();
            f(&mut status);
            status.clone()
        };
        crate::update_report::tell(&self.config, &status);
        self.publish(StreamMsg::Update { status });
    }

    /// The user said to install the version that is ready. It switches over once no thread is
    /// working.
    pub fn approve_install(&self) -> Result<()> {
        let status = self.update_status();
        let (UpdateState::Ready, Some(target)) = (status.state, status.target) else {
            bail!("no new version is ready to install");
        };
        *self.install.locked() = Some(target);
        self.install_signal.notify_one();
        Ok(())
    }

    /// Carries the user's "install" over to `version`, which replaced the one they said it for.
    pub(crate) fn approve_version(&self, version: &str) {
        *self.install.locked() = Some(version.to_owned());
    }

    pub(crate) fn install_approved(&self, version: &str) -> bool {
        self.install.locked().as_deref() == Some(version)
    }

    pub(crate) async fn install_requested(&self) {
        self.install_signal.notified().await
    }

    /// Forgets the version that failed, so the updater tries it again now instead of waiting for
    /// a newer commit.
    pub fn retry_update(&self) -> Result<()> {
        if self.update_status().state != UpdateState::Failed {
            bail!("the last update did not fail");
        }
        self.retry_signal.notify_one();
        Ok(())
    }

    pub(crate) async fn retry_requested(&self) {
        self.retry_signal.notified().await
    }

    /// Another machine says it sent this one `expecting` (a version) and asks the updater to look
    /// now, instead of at its next round. The answer is the receipt: the updater's state, which
    /// names the version as its target when it was idle and now looks at it. A failed one is
    /// tried again, as for Try again. Used by `POST /api/update/look`.
    pub fn update_look(&self, expecting: Option<&str>) -> UpdateStatus {
        match self.update_status().state {
            UpdateState::Idle => self.set_update(|s| {
                if s.state == UpdateState::Idle {
                    s.state = UpdateState::Checking;
                    s.target = expecting.map(str::to_owned);
                    s.message = None;
                    s.checked_at = Some(now_ms());
                }
            }),
            UpdateState::Failed => {
                self.retry_update().warn_on_err("retry the update");
            }
            _ => {}
        }
        crate::update_signals::look_now();
        self.update_status()
    }

    /// Builds the version the build mode holds back (`update.build_mode`) now.
    pub fn build_update_now(&self) -> Result<()> {
        let status = self.update_status();
        if status.waiting_until.is_none() && status.waiting.is_none() {
            bail!("no version is waiting to be built");
        }
        self.build_now.store(true, std::sync::atomic::Ordering::SeqCst);
        self.build_now_signal.notify_one();
        Ok(())
    }

    /// Whether "build now" was asked for since the last build started; asking again needs a new click.
    pub(crate) fn take_build_now(&self) -> bool {
        self.build_now.swap(false, std::sync::atomic::Ordering::SeqCst)
    }

    pub(crate) async fn build_now_requested(&self) {
        self.build_now_signal.notified().await
    }

    /// Shows that `version` failed to take over and this one carries on.
    pub fn fail_update(&self, version: &str, message: String) {
        self.set_update(|s| {
            s.state = UpdateState::Failed;
            s.target = Some(version.to_owned());
            s.message = Some(message);
            s.checked_at = Some(now_ms());
        });
    }

    /// True while any thread is working or waiting on the user; restarts wait for this.
    pub async fn busy(&self) -> Result<bool> {
        Ok(self
            .store
            .threads()
            .await?
            .iter()
            .any(|t| t.cloud.is_none() && matches!(t.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou)))
    }

    /// Stops every engine without ending its turn in the log, before this core hands over to
    /// another (`by` is "update", "restart" or "stop"). Threads that were working keep their
    /// status, so the next core finds their turns cut off and carries them on
    /// (`crate::restart::resume`). No engine starts after this until [`Hub::unpark`].
    pub async fn park(&self, by: &str) {
        self.parked.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Err(err) = self.store.set_value(super::RESTART_PLANNED, by).await {
            tracing::warn!("could not note the restart: {err:#}");
        }
        // Engines under a keeper hold on for the next core, which takes them over mid-turn.
        let kept: Vec<ThreadId> = {
            let runtimes = self.runtimes.locked();
            self.kept.locked().iter().filter(|t| runtimes.contains_key(*t)).cloned().collect()
        };
        crate::keeper::detach(&self.config.data_dir, &kept).await;
        let runtimes: Vec<Runtime> = self.runtimes.locked().drain().map(|(_, r)| r).collect();
        // Their questions die with the engines; the agent asks again when it carries on.
        self.permissions.locked().clear();
        for r in &runtimes {
            r.task.abort();
        }
        for mut r in runtimes {
            tokio::time::timeout(std::time::Duration::from_secs(5), r.exited.changed()).await.ignore("an engine that does not end in 5 s is left to the abort");
        }
    }

    /// Whether the panic button holds every agent here.
    pub fn halted(&self) -> bool {
        self.halt.locked().is_some()
    }

    /// Stops every engine without ending its turn in the log, as [`Hub::park`] does, and rejects
    /// their open questions. Returns the threads that were in a turn, which are idle afterwards.
    pub(crate) async fn stop_engines(&self) -> Vec<ThreadId> {
        let runtimes: Vec<(ThreadId, Runtime)> = self.runtimes.locked().drain().collect();
        for (_, r) in &runtimes {
            r.task.abort();
        }
        let mut cut = Vec::new();
        for (id, mut r) in runtimes {
            tokio::time::timeout(std::time::Duration::from_secs(5), r.exited.changed()).await.ignore("an engine that does not end in 5 s is left to the abort");
            self.cancel_permissions(&id).await;
            let working = self
                .store
                .thread(&id)
                .await
                .ok()
                .flatten()
                .is_some_and(|t| matches!(t.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou));
            if working {
                // Not through `set_status`: a paused turn has not come back to the user.
                self.emit_typed(&id, &crate::model::StatusData::new(ThreadStatus::Idle, "engine_stopped")).await.warn_on_err("emit Status");
                self.update_thread(&id, |t| t.status = ThreadStatus::Idle).await.warn_on_err("set status Idle");
                cut.push(id);
            }
        }
        cut
    }

    /// Lets engines start again after [`Hub::park`], when the handover failed.
    pub fn unpark(&self) {
        self.parked.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether a thread is working with an engine that a restart would cut off (none under a
    /// keeper), so an update has reason to wait for it.
    pub async fn busy_unkept(&self) -> Result<bool> {
        let kept = self.kept.locked().clone();
        Ok(self
            .store
            .threads()
            .await?
            .iter()
            .any(|t| t.cloud.is_none() && matches!(t.status, ThreadStatus::Running) && !kept.contains(&t.id)))
    }

    /// Returns the command channel of the thread's engine, starting the engine if needed.
    pub(crate) fn runtime(self: &Arc<Self>, thread: Thread) -> Result<mpsc::UnboundedSender<Command>> {
        self.runtime_with(thread, None)
    }

    /// Takes over an engine a keeper held through the restart (`keeper.rs`).
    pub(crate) fn adopt_engine(self: &Arc<Self>, thread: Thread, link: crate::keeper::Link) -> Result<mpsc::UnboundedSender<Command>> {
        self.runtime_with(thread, Some(link))
    }

    pub(super) fn runtime_with(self: &Arc<Self>, thread: Thread, link: Option<crate::keeper::Link>) -> Result<mpsc::UnboundedSender<Command>> {
        let mut runtimes = self.runtimes.locked();
        if let Some(r) = runtimes.get(&thread.id)
            && !r.commands.is_closed() {
                return Ok(r.commands.clone());
            }
        if self.parked.load(std::sync::atomic::Ordering::SeqCst) {
            bail!("Hub is restarting; the message goes to the engine when it is back");
        }
        if self.halted() {
            bail!("every agent is paused; resume them first");
        }
        let engine = self
            .config
            .engines
            .get(&thread.engine)
            .with_context(|| format!("engine {:?} is not configured", thread.engine))?
            .clone();
        if self.config.tmux {
            crate::terminal::watch_in_tmux(&thread);
        }
        let (tx, mut rx) = mpsc::unbounded_channel();
        let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (exit, exited) = tokio::sync::watch::channel(());

        let hub = self.clone();
        let id = thread.id.clone();
        let task = tokio::spawn(async move {
            let _exit = exit;
            let thread_id = thread.id.clone();
            let mut leftover = Vec::new();
            // Whoever starts the engine (a message, a restart, a warm engine), the clean-up may
            // have removed the folder since: bring it back, or say so plainly. The agent would
            // only answer "`cwd` does not exist".
            hub.bring_back_folder(&thread).await;
            let result = if thread.host.is_none() && !Path::new(&thread.cwd).exists() {
                Err(anyhow::anyhow!(
                    "this thread's folder is gone ({}) and could not be brought back; its work is on its branch or pull request. Start a new thread in the project folder to go on.",
                    thread.cwd
                ))
            } else {
                engine::run(hub.clone(), thread, engine, &mut rx, &mut leftover, link).await
            };
            hub.engine_ended(thread_id, generation, rx, leftover, result).await;
        });
        runtimes.insert(id, Runtime { generation, commands: tx.clone(), task: task.abort_handle(), exited });
        Ok(tx)
    }

    /// What follows the end of an engine: forget it, hand back what it held, say how it ended and
    /// start a fresh engine for the messages left over.
    async fn engine_ended(self: &Arc<Self>, thread_id: String, generation: u64, mut rx: mpsc::UnboundedReceiver<Command>, mut leftover: Vec<Prompt>, result: Result<()>) {
        self.note_kept(&thread_id, false);
        {
            let mut runtimes = self.runtimes.locked();
            if runtimes.get(&thread_id).is_some_and(|r| r.generation == generation) {
                runtimes.remove(&thread_id);
            }
        }
        // Messages the engine held behind its last turn, and ones that arrived while it was
        // shutting down, go to a fresh engine.
        rx.close();
        while let Ok(cmd) = rx.try_recv() {
            if let Command::Prompt(prompt) = cmd {
                leftover.push(prompt);
            }
        }
        self.cancel_permissions(&thread_id).await;
        // An engine started ahead of a thread that never came (`warm.rs`), or one whose thread
        // was removed under it (deleted mid-turn, or replaced by a move back from another
        // machine), has nowhere to say how it ended.
        let warm = crate::warm::pending(&thread_id).is_some();
        let gone = matches!(self.store.thread(&thread_id).await, Ok(None)) || result.as_ref().is_err_and(crate::store::gone);
        if gone {
            crate::warm::forget(&thread_id);
            match result {
                Err(err) if warm => tracing::info!(thread = %thread_id, "a warm engine stopped: {err:#}"),
                Err(err) => tracing::debug!(thread = %thread_id, "the engine of a removed thread stopped: {err:#}"),
                Ok(()) => {}
            }
            return;
        }
        if result.is_ok() {
            retry::ended_well(&thread_id);
        }
        if let Err(err) = result {
            tracing::warn!(thread = %thread_id, "engine failed: {err:#}");
            self
                .emit_typed(&thread_id, &crate::model::ErrorData::new(format!("{err:#}")))
                .await.warn_on_err("emit Error");
            self.set_status(&thread_id, ThreadStatus::Error).await.warn_on_err("set status Error");
            // An engine that fails before it gets anywhere (a broken command) would fail every
            // fresh one too; after a few in a row the messages wait for the next one you send.
            let streak = retry::failed(&thread_id);
            if streak >= retry::GIVE_UP {
                for prompt in leftover.drain(..) {
                    self.emit_typed(&thread_id, &crate::model::QueueData::new(prompt.seq, "dropped")).await.warn_on_err("emit Queue");
                }
            } else if !leftover.is_empty() {
                tokio::time::sleep(retry::wait(streak)).await;
            }
        } else if leftover.is_empty()
            // An engine that went away mid-turn said so and left the thread needing you.
            && !matches!(self.store.thread(&thread_id).await, Ok(Some(t)) if t.status == ThreadStatus::Error)
        {
            self.set_status_because(&thread_id, ThreadStatus::Idle, "engine_ended").await.warn_on_err("set status Idle");
        }
        if !leftover.is_empty()
            && let Ok(Some(thread)) = self.store.thread(&thread_id).await
                && let Ok(commands) = self.runtime(thread) {
                    for prompt in leftover {
                        let seq = prompt.seq;
                        if !send_or_log(&commands, Command::Prompt(prompt), "queued message") {
                            self.emit_typed(&thread_id, &crate::model::QueueData::new(seq, "dropped")).await.warn_on_err("emit Queue");
                        }
                    }
                }
    }
}
