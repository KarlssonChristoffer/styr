//! The dev core: a `hubd` built from the clone (or the thread's worktree the dev window shows) and
//! run on a copy of this core's database, rebuilt and restarted whenever its Rust changes
//! (`keep_core`), so the dev window can try core changes without touching the real threads.

use crate::locked::ResultExt;
use super::*;

impl Hub {
    /// Points the dev window at a core built from the clone, or back at this one. The dev core
    /// runs on a copy of this core's database, made the first time, so what is done in the dev
    /// window leaves the threads here alone.
    pub async fn dev_core(self: &Arc<Self>, on: bool) -> Result<DevStatus> {
        if on {
            {
                let mut inner = self.dev.lock();
                if inner.core_building {
                    bail!("the dev core is already being built");
                }
                inner.core_wanted = true;
                inner.core_building = true;
                inner.core_failed = false;
                inner.core_message = Some("Building the core".into());
            }
            self.store.set_value(CORE_KEY, "on").await?;
            let hub = self.clone();
            tokio::spawn(async move { hub.keep_core().await });
        } else {
            {
                let mut inner = self.dev.lock();
                inner.core_wanted = false;
                inner.core_failed = false;
                inner.core_message = None;
            }
            self.store.take_value(CORE_KEY).await?;
            self.stop_core().await?;
            self.dev_restart_ui().await?;
        }
        self.dev_status().await
    }

    /// Builds and starts the dev core, then rebuilds and restarts it whenever Rust code in the
    /// clone changes, until it is switched off. A build that fails leaves the running one.
    pub(super) async fn keep_core(self: Arc<Self>) {
        let mut built = None;
        let mut built_dir = None;
        let mut first = true;
        loop {
            if !self.dev.lock().core_wanted {
                return;
            }
            let Ok(clone) = self.dev_dir().await else { return };
            // The thread's worktree the dev window shows, or the clone.
            let Ok((dir, _)) = self.dev_shown().await else { return };
            let newest = {
                let dir = dir.clone();
                tokio::task::spawn_blocking(move || newest_source(&dir)).await.ok().flatten()
            };
            if first || newest > built || built_dir.as_ref() != Some(&dir) {
                built = newest;
                built_dir = Some(dir.clone());
                {
                    let mut inner = self.dev.lock();
                    inner.core_building = true;
                    inner.core_message = Some(if first { "Building the core" } else { "Rebuilding the core" }.into());
                }
                let result = self.run_core(&dir, &clone).await;
                {
                    let mut inner = self.dev.lock();
                    inner.core_building = false;
                    match &result {
                        Ok(()) => {
                            inner.core_failed = false;
                            inner.core_message = None;
                        }
                        Err(e) => {
                            inner.core_failed = true;
                            inner.core_message = Some(if first || !core_serving() {
                                format!("{e:#}")
                            } else {
                                format!("The new build failed; the previous one is still running.\n{e:#}")
                            });
                            if first {
                                inner.core_wanted = false;
                            }
                        }
                    }
                }
                if first {
                    if result.is_err() {
                        self.store.take_value(CORE_KEY).await.warn_on_err("forget the core key");
                        return;
                    }
                    if let Err(e) = self.dev_restart_ui().await {
                        self.dev.fail(format!("{e:#}"));
                    }
                }
                first = false;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Builds the core in `dir` (the clone or a thread's worktree, which share the clone's build
    /// cache), then swaps the running dev core for it.
    pub(super) async fn run_core(self: &Arc<Self>, dir: &Path, clone: &Path) -> Result<()> {
        let data = core_dir(self);
        std::fs::create_dir_all(&data).with_context(|| format!("creating {}", data.display()))?;
        let db = data.join("hub.db");
        if !db.exists() {
            self.store.backup(&db).await.context("copying the database for the dev core")?;
        }
        let mut config = self.config.clone();
        config.data_dir = data.clone();
        config.bind = SocketAddr::from(([127, 0, 0, 1], CORE_PORT));
        config.update.enabled = Some(false);
        // Other machines and terminals stay with the real core; turns the copy cut off stay cut off.
        config.hosts.clear();
        config.tmux = false;
        config.resume_turns = false;
        config.name = Some("Dev".into());
        config.engines.remove(crate::sim::ENGINE);
        std::fs::write(data.join("hub.json"), serde_json::to_string_pretty(&config)?)?;

        let (src, log) = (dir.to_owned(), core_log(self));
        let target = clone.join("target");
        let cap = self.dev_cache_cap().await;
        let cache = {
            let target = target.clone();
            tokio::task::spawn_blocking(move || {
                let cache = super::build::trim_cache(&target, cap);
                std::fs::write(&log, "").warn_on_err("write file");
                run(&src, "cargo", &["build", "-p", "hubd", "--target-dir", &target.display().to_string()], &log).map(|()| cache)
            })
            .await??
        };
        self.dev.lock().cache_bytes = Some(cache);
        if !self.dev.lock().core_wanted {
            return Ok(());
        }

        self.stop_core().await?;
        let name = if cfg!(windows) { "hubd.exe" } else { "hubd" };
        // A copy runs, so the next build can replace the one cargo made while it does.
        let (built, exe) = (target.join("debug").join(name), data.join(name));
        let mut copied = std::fs::copy(&built, &exe);
        for _ in 0..20 {
            if copied.is_ok() {
                break;
            }
            // Windows lets go of a stopped program's file a moment later.
            tokio::time::sleep(Duration::from_millis(250)).await;
            copied = std::fs::copy(&built, &exe);
        }
        copied.with_context(|| format!("copying {} to {}", built.display(), exe.display()))?;

        let out_log = data.join("core.log");
        let out = File::create(&out_log)?;
        let mut cmd = command(&exe.display().to_string(), &data);
        cmd.env("HUB_CONFIG", data.join("hub.json"))
            .env("HUB_PROJECT", dir)
            .stdin(Stdio::null())
            .stdout(out.try_clone()?)
            .stderr(out);
        let child = cmd.spawn().with_context(|| format!("starting {}", exe.display()))?;
        self.dev.lock().core = Some(child);
        for _ in 0..240 {
            if core_serving() {
                return Ok(());
            }
            let exited = self.dev.lock().core.as_mut().and_then(|c| c.try_wait().ok().flatten());
            if let Some(code) = exited {
                self.dev.lock().core = None;
                bail!("the dev core stopped ({code}):\n{}", tail(&out_log));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        bail!("the dev core did not answer within a minute:\n{}", tail(&out_log))
    }

    /// Stops the dev core, also one an earlier core started.
    pub(super) async fn stop_core(&self) -> Result<()> {
        let child = self.dev.lock().core.take();
        tokio::task::spawn_blocking(move || stop(child, CORE_PORT, core_serving)).await?
    }
}
