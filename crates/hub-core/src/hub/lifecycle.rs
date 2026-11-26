//! Starting and ending the things Hub owns: projects, threads and attempts, their settings, warming and shutdown.

use super::*;

impl Hub {
    /// Gives threads from before owners existed theirs, once (`system::backfill`), with Vakten's
    /// thread and the routines read here so `system.rs` names nothing above it.
    pub async fn backfill_owners(self: Arc<Self>) {
        let run = async {
            if crate::system::backfilled(&self.store).await? {
                return Ok(());
            }
            let vakten = crate::vakten_thread::thread_id(&self.store).await;
            let routines = crate::routines::load(&self.ctx()).await.unwrap_or_default();
            crate::system::backfill(&self.store, vakten, &routines, |thread| self.publish(StreamMsg::Thread { thread })).await
        };
        if let Err(e) = run.await {
            tracing::warn!(target: "hub_core::system", "could not give old threads their owners: {e:#}");
        }
    }

    /// Adds a folder as a project. With `create` the folder is made first, as a git repository
    /// with a plan and living docs, and optionally a private GitHub repository and a thread
    /// that sets up the stack.
    pub async fn create_project(self: &Arc<Self>, new: NewProject) -> Result<CreatedProject> {
        let path = new.path.trim().trim_end_matches(['/', '\\']).to_owned();
        if path.is_empty() {
            bail!("pick a folder for the project");
        }
        let create = new.create.unwrap_or(false);
        let folder = PathBuf::from(&path);
        let blocking_folder = folder.clone();
        let name = new
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| folder_name(Path::new(&path)));
        let template = new.template.clone().filter(|_| create);
        let project_name = name.clone();
        let written = tokio::task::spawn_blocking(move || -> Result<bool> {
            let folder = blocking_folder;
            let mut written = false;
            if create {
                if folder.exists() && std::fs::read_dir(&folder)?.next().is_some() {
                    bail!("{} already exists and is not empty; add it as an existing folder", folder.display());
                }
                std::fs::create_dir_all(&folder).with_context(|| format!("creating {}", folder.display()))?;
                // A project is a repository from the start. Without git it is still a folder.
                crate::git::run(&folder, &["init", "-b", "main"]).or_else(|| crate::git::run(&folder, &["init"])).ignore("without git it is still a folder");
                // A template goes in first, so the plan and CLAUDE.md the docs step adds to are its own.
                if let Some(id) = template.as_deref() {
                    written = crate::templates::apply(&folder, id, &project_name)?;
                }
                crate::project::set_up_docs(&folder)?;
                if written {
                    crate::templates::mirror_agents(&folder)?;
                }
            } else if !folder.is_dir() {
                bail!("folder {} does not exist", folder.display());
            }
            Ok(written)
        })
        .await??;
        let project = self.store.insert_project(Project { path: path.clone(), name: name.clone(), created_at: now_ms() }).await?;
        self.publish(StreamMsg::Project { project: project.clone() });

        let mut notes = Vec::new();
        let mut pushed_to = None;
        if create && new.github.unwrap_or(false) {
            if new.ci.unwrap_or(false) {
                match new.template.as_deref() {
                    Some(id) if written => {
                        if let Err(why) = crate::templates::write_ci(&folder, id) {
                            notes.push(format!("No build and test workflow: {why}"));
                        }
                    }
                    _ => notes.push("No build and test workflow: the project did not start from a template Hub wrote.".into()),
                }
            }
            match crate::project::create_github_repo(&folder, &name, new.public.unwrap_or(false)).await {
                Ok(url) => pushed_to = Some(url),
                Err(e) => notes.push(format!("No GitHub repository: {e:#}")),
            }
        }
        let mut thread_id = None;
        // A template that was written needs no scaffolding thread unless there is something to adapt it to.
        let described = new.about.as_deref().is_some_and(|a| !a.trim().is_empty());
        if create && new.setup.unwrap_or(false) && (!written || described) {
            let from = written.then(|| new.template.as_deref().and_then(crate::templates::get)).flatten().map(|t| t.title);
            let message = crate::project::setup_prompt(&name, new.about.as_deref(), pushed_to.as_deref(), from);
            match self
                .create_thread(NewThread {
                    cwd: path,
                    title: Some(format!("Set up {name}")),
                    engine: None,
                    message: Some(message),
                    attachments: Vec::new(),
                    settings: Default::default(),
                    original: None,
                    client_id: None,
                    attempts: Vec::new(),
                    warm: None,
                    design: false,
                    unattended: None,
                    reach: None,
                    cloud: false,
                })
                .await
            {
                Ok(thread) => thread_id = Some(thread.id),
                Err(e) => notes.push(format!("No setup thread: {e:#}")),
            }
        }
        Ok(CreatedProject { project, thread_id, notes })
    }

    /// Takes a project off the list. The folder and its threads stay.
    pub async fn remove_project(&self, path: &str) -> Result<()> {
        self.store.delete_project(path).await?;
        self.publish(StreamMsg::ProjectRemoved { path: path.to_owned() });
        Ok(())
    }

    pub async fn create_thread(self: &Arc<Self>, new: NewThread) -> Result<Thread> {
        let client_id = new.client_id.clone();
        if let Some(client_id) = &client_id
            && let Some(id) = self.store.recall(client_id).await?
            && let Some(thread) = self.store.thread(&id).await?
        {
            return Ok(thread);
        }
        let thread = self.create_thread_now(new).await?;
        if let Some(client_id) = &client_id {
            self.store.remember(client_id, &thread.id).await?;
        }
        Ok(thread)
    }

    async fn create_thread_now(self: &Arc<Self>, new: NewThread) -> Result<Thread> {
        self.create_thread_under(new, None).await
    }

    /// Creates a thread, as a side job of `parent` when given (`children.rs`).
    pub(crate) async fn create_thread_under(self: &Arc<Self>, new: NewThread, parent: Option<ThreadId>) -> Result<Thread> {
        self.create_thread_as(new, parent, None).await
    }

    /// Creates a thread Hub starts on its own, owned by `owner` (`system.rs`), as a side job of
    /// `parent` when given.
    pub async fn create_thread_as(
        self: &Arc<Self>,
        new: NewThread,
        parent: Option<ThreadId>,
        owner: Option<crate::model::Owner>,
    ) -> Result<Thread> {
        let mut new = new;
        // "auto" is what an app from before Auto was taken out may still remember: the default.
        if new.engine.as_deref() == Some("auto") {
            new.engine = None;
        }
        if new.attempts.len() > 1 {
            return Box::pin(self.create_attempts(new)).await;
        }
        let engine = new.engine.unwrap_or_else(|| self.config.default_engine.clone());
        if !self.config.engines.contains_key(&engine) {
            bail!("unknown engine {engine:?}");
        }
        // In the cloud (`cloud_threads.rs`): checked before anything is made, so a refusal leaves no thread.
        let cloud = if new.cloud {
            Some(crate::cloud_threads::begin(&engine, new.message.as_deref().is_some_and(|m| !m.trim().is_empty()))?)
        } else {
            None
        };
        let cwd = Path::new(&new.cwd);
        if !cwd.is_dir() {
            bail!("folder {:?} does not exist", new.cwd);
        }
        let title_locked = new.title.as_deref().is_some_and(|t| !t.trim().is_empty());
        let title = new
            .title
            .filter(|t| !t.trim().is_empty())
            .or_else(|| new.message.as_deref().map(title_from_message))
            .unwrap_or_else(|| folder_name(cwd));
        let now = now_ms();
        // An engine started for it while the message was typed takes the thread (`warm.rs`).
        // Not one that started without the reach the thread asks for (`reach.rs`).
        let reach = new.reach.take().filter(|r| !r.folders.is_empty() || !r.connections_off.is_empty());
        let id = match new.warm.take() {
            Some(warm) if parent.is_none() && reach.is_none() && crate::warm::claim(&warm, &engine) => warm,
            _ => uuid::Uuid::now_v7().to_string(),
        };
        // A thread on Hub itself in dev mode works in a worktree of its own (`dev_threads`).
        let cwd = match parent {
            None => match self.dev_thread_folder(&new.cwd, &id, &title).await {
                Some(folder) => folder,
                // Or, in a project with worktrees on, a worktree of its own (`project_trees`).
                None => self.project_thread_folder(&new.cwd, &id, &title).await.unwrap_or(new.cwd),
            },
            Some(_) => new.cwd,
        };
        let thread = self
            .store
            .insert_thread(Thread {
                id,
                title,
                cwd,
                engine,
                status: ThreadStatus::Idle,
                engine_session_id: None,
                title_locked,
                archived: false,
                returned_at: None,
                seen_at: None,
                settled_at: None,
                sent_at: None,
                pinned_at: None,
                snoozed_until: None,
                auto_settle: true,
                parent,
                reading: None,
                owner,
                cloud,
                host: None, project: None, display: None,
                created_at: now,
                updated_at: now,
            })
            .await?;
        self.publish(StreamMsg::Thread { thread: thread.clone() });
        // Before the first message, so the engine reads it when it starts.
        if let Some(reach) = reach {
            crate::reach::start(&self.ctx(), &thread.id, &thread.cwd, reach).await?;
        }
        if let Some(unattended) = &new.unattended {
            self.emit_typed(&thread.id, unattended).await?;
        }
        for (config_id, value) in new.settings {
            self.emit_typed(&thread.id, &crate::model::SettingData::new(config_id, value)).await?;
        }
        if new.design {
            self.emit_typed(&thread.id, &crate::model::DesignData::switched(true, "start")).await?;
        }
        if new.message.is_some() || !new.attachments.is_empty() {
            self.send(&thread.id, new.message.unwrap_or_default(), new.attachments, new.original)
                .await?;
        }
        Ok(thread)
    }

    /// Starts the same task as two to four attempts, each in its own worktree and thread.
    async fn create_attempts(self: &Arc<Self>, mut new: NewThread) -> Result<Thread> {
        let variants = std::mem::take(&mut new.attempts);
        if variants.len() > 4 {
            bail!("at most four attempts at once");
        }
        let message = new.message.take().filter(|m| !m.trim().is_empty()).context("attempts need a message to work on")?;
        let group = uuid::Uuid::now_v7().to_string();
        let cwd = PathBuf::from(&new.cwd);
        let (at, g) = (cwd.clone(), group.clone());
        let n = variants.len();
        let (base, worktrees) = tokio::task::spawn_blocking(move || -> Result<_> {
            let base = crate::attempts::base(&at, &g)?;
            let worktrees = (1..=n).map(|i| crate::attempts::add(&at, &base, &g, i)).collect::<Result<Vec<_>>>()?;
            Ok((base, worktrees))
        })
        .await??;
        let title = new.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| title_from_message(&message));
        let mut threads = Vec::new();
        for (i, (variant, wt)) in variants.into_iter().zip(&worktrees).enumerate() {
            let mut settings = new.settings.clone();
            settings.extend(variant);
            let one = NewThread {
                cwd: wt.cwd.display().to_string(),
                title: Some(format!("{title} · {}/{n}", i + 1)),
                engine: new.engine.clone(),
                message: None,
                attachments: Vec::new(),
                settings,
                original: None,
                client_id: None,
                attempts: Vec::new(),
                warm: None,
                design: false,
                unattended: new.unattended.clone(),
                reach: new.reach.clone(),
                cloud: false,
            };
            threads.push(self.create_thread_now(one).await?);
        }
        let ids: Vec<&str> = threads.iter().map(|t| t.id.as_str()).collect();
        for (i, (thread, wt)) in threads.iter().zip(&worktrees).enumerate() {
            let data = AttemptData {
                group: group.clone(),
                n: Some(i as u64 + 1),
                of: Some(n as u64),
                threads: Some(ids.iter().map(|id| (*id).to_owned()).collect()),
                base: Some(base.commit.clone()),
                root: Some(base.root.display().to_string()),
                cwd: Some(new.cwd.clone()),
                worktree: Some(wt.path.display().to_string()),
                branch: Some(wt.branch.clone()),
                ..Default::default()
            };
            self.emit_typed(&thread.id, &data).await?;
        }
        for thread in &threads {
            self.send(&thread.id, message.clone(), new.attachments.clone(), new.original.clone()).await?;
        }
        Ok(threads.into_iter().next().expect("two or more attempts"))
    }

    /// The group of attempts a thread belongs to, with what each has changed so far.
    pub async fn attempts(&self, thread_id: &str) -> Result<AttemptGroup> {
        let start = self.attempt_start(thread_id).await?;
        let base = start.base.clone().unwrap_or_default();
        let mut out = AttemptGroup { group: start.group.clone(), cwd: start.cwd.clone().unwrap_or_default(), done: false, attempts: Vec::new() };
        for (i, id) in start.threads.iter().flatten().enumerate() {
            let Some(thread) = self.store.thread(id).await? else { continue };
            let events = self.store.events_of(id, 0, &[EventKind::Attempt, EventKind::SessionState, EventKind::Setting]).await?;
            let attempts: Vec<AttemptData> = events.iter().filter_map(Event::data_as).collect();
            let state = attempts.iter().rev().find_map(|a| a.state.clone());
            out.done |= state.is_some();
            let worktree = PathBuf::from(attempts.first().and_then(|a| a.worktree.clone()).unwrap_or_default());
            let (files, added, removed) = if state.is_none() && worktree.is_dir() {
                let base = base.clone();
                tokio::task::spawn_blocking(move || crate::attempts::stat(&worktree, &base)).await?.unwrap_or_default()
            } else {
                Default::default()
            };
            out.attempts.push(AttemptInfo {
                n: i as u32 + 1,
                thread_id: thread.id.clone(),
                title: thread.title.clone(),
                status: thread.status,
                model: current_model(&events),
                files,
                added,
                removed,
                state,
            });
        }
        Ok(out)
    }

    /// Picks this thread's attempt: its work lands in the folder the group started from, as
    /// uncommitted changes, and it carries on there. The other attempts are stopped and archived,
    /// and every worktree is removed; all their work stays under `refs/hub/`. Undoable.
    pub async fn pick_attempt(&self, thread_id: &str) -> Result<AttemptGroup> {
        let start = self.attempt_start(thread_id).await?;
        let ids: Vec<String> = start.threads.clone().unwrap_or_default();
        for id in &ids {
            let events = self.store.events_of(id, 0, &[EventKind::Attempt]).await?;
            if events.iter().filter_map(Event::data_as::<AttemptData>).any(|a| a.state.is_some()) {
                bail!("an attempt of this group was already picked");
            }
        }
        let picked = self.store.thread(thread_id).await?.context("no such thread")?;
        if matches!(picked.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou) {
            bail!("let this attempt finish its turn, or stop it, before picking it");
        }
        for id in &ids {
            self.shutdown(id, true).await?;
        }
        let root = PathBuf::from(start.root.clone().context("bad attempt")?);
        let cwd = start.cwd.clone().context("bad attempt")?;
        let base = start.base.clone().context("bad attempt")?;
        let mut worktrees = Vec::new();
        for id in &ids {
            let events = self.store.events_of(id, 0, &[EventKind::Attempt]).await?;
            if let Some(e) = events.iter().find(|e| e.kind == EventKind::Attempt) {
                let first = e.data_as::<AttemptData>().unwrap_or_default();
                let wt = crate::attempts::Worktree {
                    path: PathBuf::from(first.worktree.unwrap_or_default()),
                    cwd: PathBuf::new(),
                    branch: first.branch.unwrap_or_default(),
                };
                worktrees.push((id.clone(), wt));
            }
        }
        let picked_wt = worktrees.iter().find(|(id, _)| id == thread_id).map(|(_, w)| w.path.clone()).context("no worktree for this attempt")?;
        let (at, keep, safety_name) = (root.clone(), format!("{thread_id}/attempt"), format!("{thread_id}/pick-{}", now_ms()));
        let all = worktrees.clone();
        let (safety, files) = tokio::task::spawn_blocking(move || -> Result<_> {
            let safety = crate::checkpoint::take(&at, &safety_name, "Hub: before picking an attempt")?.context("not a git repository")?;
            let files = crate::attempts::apply(&at, &picked_wt, &base, &keep)?;
            for (id, wt) in &all {
                crate::attempts::remove(&at, wt, &format!("{id}/attempt"));
            }
            Ok((safety, files))
        })
        .await??;

        for id in &ids {
            let Some(thread) = self.store.thread(id).await? else { continue };
            let title = thread.title.rsplit_once(" · ").map(|(t, _)| t.to_owned()).unwrap_or(thread.title.clone());
            if id == thread_id {
                let data = AttemptData {
                    group: start.group.clone(),
                    state: Some("picked".into()),
                    files: Some(files as u64),
                    cwd: Some(cwd.clone()),
                    root: Some(root.display().to_string()),
                    safety: Some(safety.commit.clone()),
                    safety_head: Some(safety.head.clone()),
                    safety_branch: Some(safety.branch.clone()),
                    ..Default::default()
                };
                self.emit_typed(id, &data).await?;
                let cwd = cwd.clone();
                self.update_thread(id, move |t| {
                    t.cwd = cwd;
                    t.title = title;
                })
                .await?;
            } else {
                let dropped = AttemptData { group: start.group.clone(), state: Some("dropped".into()), by: Some(thread_id.to_owned()), ..Default::default() };
                self.emit_typed(id, &dropped).await?;
                self.update_thread(id, |t| t.archived = true).await?;
            }
            self.clashes.locked().forget(id);
        }
        self.attempts(thread_id).await
    }

    /// The event that started this thread as an attempt.
    async fn attempt_start(&self, thread_id: &str) -> Result<AttemptData> {
        let events = self.store.events_of(thread_id, 0, &[EventKind::Attempt]).await?;
        events.iter().filter_map(Event::data_as::<AttemptData>).find(|a| a.n.is_some()).context("this thread is not one of several attempts")
    }

    pub async fn patch_thread(self: &Arc<Self>, thread_id: &str, mut patch: ThreadPatch) -> Result<Thread> {
        if let Some(title) = &patch.title
            && title.trim().is_empty()
        {
            bail!("a thread needs a title");
        }
        if let Some(engine) = patch.engine.take() {
            self.switch_engine(thread_id, engine).await?;
        }
        let marks = ThreadPatch {
            settled: patch.settled,
            seen: patch.seen,
            pinned: patch.pinned,
            pin_order: patch.pin_order,
            snooze_until: patch.snooze_until,
            auto_settle: patch.auto_settle,
            ..Default::default()
        };
        let mut thread = None;
        if patch.title.is_some() || patch.archived.is_some() {
            thread = Some(
                self.update_thread(thread_id, move |t| {
                    if let Some(title) = patch.title {
                        t.title = title.trim().to_owned();
                        t.title_locked = true;
                    }
                    if let Some(archived) = patch.archived {
                        t.archived = archived;
                    }
                })
                .await?,
            );
        }
        let marking = marks.settled.is_some()
            || marks.seen.is_some()
            || marks.pinned.is_some()
            || marks.pin_order.is_some()
            || marks.snooze_until.is_some()
            || marks.auto_settle.is_some();
        if marking {
            let marked = self.store.mark_thread(thread_id, move |t| apply_marks(t, &marks, now_ms())).await?;
            self.publish(StreamMsg::Thread { thread: marked.clone() });
            thread = Some(marked);
        }
        // Put away means put away: an engine that is only waiting for the next message is stopped
        // now instead of at the idle timeout. A thread in a turn or asking something is left alone;
        // its engine goes when the turn ends and the idle timeout runs out.
        if patch.settled == Some(true) || patch.archived == Some(true) {
            let idle = self.store.thread(thread_id).await?.is_some_and(|t| matches!(t.status, ThreadStatus::Idle | ThreadStatus::Error));
            if idle && self.runtimes.locked().contains_key(thread_id) {
                self.shutdown(thread_id, false).await?;
            }
        }
        match thread {
            Some(thread) => Ok(thread),
            None => self.store.thread(thread_id).await?.context("no such thread"),
        }
    }

    /// Moves a thread to another engine between turns. The old engine stops and its session is
    /// left behind; the next message hands the conversation over (see `send_from`). The new
    /// engine starts right away, so it is up by the time that message is written.
    pub async fn switch_engine(self: &Arc<Self>, thread_id: &str, engine: String) -> Result<Thread> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        if thread.engine == engine {
            return Ok(thread);
        }
        if !self.config.engines.contains_key(&engine) {
            bail!("engine {engine:?} is not configured");
        }
        if thread.host.is_some() {
            bail!("this thread runs on another machine");
        }
        if matches!(thread.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou | ThreadStatus::InTerminal) {
            bail!("the thread is busy; stop it or let it finish before switching");
        }
        self.shutdown(thread_id, false).await?;
        self.emit_typed(thread_id, &crate::model::EngineSwitchData { from: thread.engine.clone(), to: engine.clone() }).await?;
        let thread = self
            .update_thread(thread_id, move |t| {
                t.engine = engine;
                t.engine_session_id = None;
            })
            .await?;
        self.warm(thread_id).await?;
        Ok(thread)
    }

    /// Starts the thread's engine ahead of a message, so starting it and loading its session do
    /// not hold the message up. Does nothing when the engine runs, the thread is in a terminal or
    /// Hub is restarting.
    pub async fn warm(self: &Arc<Self>, thread_id: &str) -> Result<()> {
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        if thread.host.is_some()
            || thread.status == ThreadStatus::InTerminal
            || self.parked.load(std::sync::atomic::Ordering::SeqCst)
            || self.halted()
        {
            return Ok(());
        }
        self.runtime(thread)?;
        Ok(())
    }

    /// Starts an engine for a thread that does not exist yet, while its first message is typed,
    /// and returns the id to create the thread with (`warm.rs`). None when nothing started: "auto"
    /// (no one engine, as for attempts on several), an unknown engine or folder, or Hub pausing.
    pub async fn warm_new(self: &Arc<Self>, ask: crate::model::WarmAsk) -> Result<Option<String>> {
        let engine = ask.engine.unwrap_or_else(|| self.config.default_engine.clone());
        if engine == "auto"
            || !self.config.engines.contains_key(&engine)
            || !Path::new(&ask.cwd).is_dir()
            || self.parked.load(std::sync::atomic::Ordering::SeqCst)
            || self.halted()
        {
            return Ok(None);
        }
        let settings: Vec<(String, serde_json::Value)> = ask.settings.into_iter().collect();
        let route = crate::routes::picked(&settings);
        if let Some(id) = crate::warm::reusable(&engine, &ask.cwd, &route)
            && self.runtimes.locked().get(&id).is_some_and(|r| !r.commands.is_closed())
        {
            return Ok(Some(id));
        }
        let id = uuid::Uuid::now_v7().to_string();
        let warm = crate::warm::Warm {
            id: id.clone(),
            engine: engine.clone(),
            cwd: ask.cwd.clone(),
            settings,
            at: std::time::Instant::now(),
            claimed: false,
        };
        let thread = warm.stand_in();
        for old in crate::warm::add(warm) {
            // Its command channel closes and the engine stops.
            self.runtimes.locked().remove(&old);
        }
        if let Err(e) = self.runtime(thread) {
            crate::warm::forget(&id);
            return Err(e);
        }
        Ok(Some(id))
    }

    /// Stops the thread's engine and everything it started, then removes the thread with its log.
    /// The stop does not wait for the engine to agree (a tool that hangs, an engine that ignores
    /// Stop): a deleted thread is not left running with nobody to stop it.
    pub async fn delete_thread(&self, thread_id: &str) -> Result<()> {
        self.store.thread(thread_id).await?.context("no such thread")?;
        self.stop_thread(thread_id, true, DELETE_GRACE).await?;
        self.cancel_permissions(thread_id).await;
        crate::terminal::close_tmux(thread_id);
        self.store.delete_thread(thread_id).await?;
        crate::pages::remove(&self.config.data_dir, thread_id);
        crate::locked::clean_file(crate::engine::acp::engine_log(self, thread_id));
        crate::locked::clean_file(crate::engine::acp::engine_trace(self, thread_id));
        self.publish(StreamMsg::ThreadDeleted { id: thread_id.to_owned() });
        Ok(())
    }

    pub async fn cancel(self: &Arc<Self>, thread_id: &str) -> Result<()> {
        crate::cloud_threads::stop(self, thread_id).await?;
        let commands = self.runtimes.locked().get(thread_id).map(|r| r.commands.clone());
        if let Some(commands) = commands {
            send_or_log(&commands, Command::Cancel, "cancel");
        }
        // Stop also ends watching a pull request, as in T3 Code.
        self.unwatch_pr(thread_id).await?;
        Ok(())
    }

    /// Stops the thread's turn and its engine, as the idle timeout would; the next message starts
    /// it again. With `everything`, also stops every process the thread started, once the
    /// engine is gone so it does not see its tools die and fail the thread.
    pub async fn shutdown(&self, thread_id: &str, everything: bool) -> Result<()> {
        self.stop_thread(thread_id, everything, SHUTDOWN_GRACE).await
    }

    /// Stops the turn, then the engine: it gets `grace` to end by itself, and what has not ended
    /// by then is ended from outside (its task is dropped, which closes the connection to its
    /// keeper, and the keeper is told to stop in case that was not enough). With `everything`,
    /// every process carrying the thread's id goes last, so a tool that hangs, or one that left
    /// its process group, does not outlive the thread.
    async fn stop_thread(&self, thread_id: &str, everything: bool, grace: std::time::Duration) -> Result<()> {
        let runtime = self.runtimes.locked().remove(thread_id);
        let mut stuck = false;
        if let Some(Runtime { commands, mut exited, task, .. }) = runtime {
            send_or_log(&commands, Command::Cancel, "cancel");
            // Dropping the last sender closes the channel; the engine finishes and exits.
            drop(commands);
            if tokio::time::timeout(grace, exited.changed()).await.is_err() {
                stuck = true;
                task.abort();
                self.note_kept(thread_id, false);
            }
        }
        if stuck || everything {
            crate::keeper::stop(&self.config.data_dir, thread_id).await;
        }
        if everything {
            // Waited out (5 s at most), so its folder can go next (an attempt's worktree, `pick_attempt`).
            let id = thread_id.to_owned();
            tokio::task::spawn_blocking(move || crate::workers::stop_thread(&id, std::time::Duration::from_secs(5))).await?;
        }
        Ok(())
    }
}

/// How long an engine gets to end by itself when its thread is shut down, and when it is deleted
/// (a deleted thread has nobody to wait for, so it is shorter).
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(10);
const DELETE_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// Applies the marks a person put on a thread: settled, seen, pinned, snoozed.
fn apply_marks(t: &mut Thread, marks: &ThreadPatch, now: i64) {
    if let Some(settled) = marks.settled {
        t.settled_at = settled.then_some(now);
        // Settled work is put away whole: no pin, no snooze. Back in the list it
        // starts again from the top.
        t.pinned_at = None;
        t.snoozed_until = None;
        if !settled {
            t.returned_at = Some(now);
            t.seen_at = Some(now);
        }
    }
    match marks.seen {
        Some(true) => t.seen_at = Some(now),
        // Unread again: as if it had just come back.
        Some(false) => {
            t.seen_at = None;
            t.returned_at.get_or_insert(now);
        }
        None => {}
    }
    if let Some(order) = marks.pin_order {
        t.pinned_at = Some(order);
        t.settled_at = None;
    } else if let Some(pinned) = marks.pinned {
        t.pinned_at = pinned.then_some(now);
        if pinned {
            t.settled_at = None;
        }
    }
    if let Some(until) = marks.snooze_until {
        t.snoozed_until = (until > now).then_some(until);
        if until > now {
            t.settled_at = None;
        } else {
            // Woken: back on top of the list.
            t.returned_at = Some(now);
        }
    }
    if let Some(auto) = marks.auto_settle {
        t.auto_settle = auto;
    }
}
