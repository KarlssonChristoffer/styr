//! One round of following the branch: what to do with a ready version, a failed round, and a new commit.

use super::*;

/// What the update loop does next.
pub(super) enum Next {
    /// Start this version.
    Return(Ready),
    /// Look again at once.
    Again,
    /// Wait for the next interval.
    Wait,
}

/// A version is built and tested: with `ask` it waits for the user to say install, and meanwhile
/// the loop keeps checking so a newer commit replaces the one on offer. Else it waits for the
/// threads to stop and hands it over.
pub(super) async fn on_ready(hub: &Arc<Hub>, cfg: &UpdateConfig, data: &Path, version: String, offered: &mut Option<String>) -> Next {
    if cfg.ask && !hub.install_approved(&version) {
        // What was merged while this one built is next in line: look at once, so it
        // starts building behind this one instead of after the next interval.
        if offered.as_deref() != Some(version.as_str()) {
            *offered = Some(version);
            return Next::Again;
        }
        tokio::select! {
            _ = hub.install_requested() => {}
            _ = hub.build_now_requested() => {}
            _ = tokio::time::sleep(nap(hub, cfg)) => {}
        }
        return Next::Again;
    }
    let hubd = data.join("bin").join(&version).join(hub_install::exe("hubd"));
    before_install(hub).await;
    // Replaced by a newer build meanwhile: look again rather than start what is gone.
    if !hubd.is_file() {
        return Next::Again;
    }
    hub.set_update(|s| {
        s.state = UpdateState::Restarting;
        s.target = Some(version.clone());
    });
    Next::Return(Ready { version, hubd })
}

/// What goes before the restart into a ready version: on a machine that runs tests for others
/// (`checks`), the run that is going finishes first (bounded), since the restart would lose it;
/// then working threads get their moment.
async fn before_install(hub: &Arc<Hub>) {
    let told = AtomicBool::new(false);
    checks::finish_before_install(|step| {
        told.store(true, Ordering::Relaxed);
        hub.set_update(|s| s.step = Some(step));
    })
    .await;
    if told.load(Ordering::Relaxed) {
        hub.set_update(|s| s.step = None);
    }
    wait_until_idle(hub).await;
}

/// An update round failed: remembers the commit (unless the drive or the source folder is to
/// blame), says what happened, and installs a version that was ready meanwhile.
pub(super) async fn on_failed(hub: &Arc<Hub>, cfg: &UpdateConfig, data: &Path, err: &anyhow::Error, failed: &mut Option<String>) -> Next {
    let message = format!("{err:#}");
    tracing::warn!("update failed: {message}");
    // A newer version that failed behind a ready one leaves the ready one on offer.
    let status = hub.update_status();
    let behind = status.state == UpdateState::Ready;
    // A full drive or a changed source folder is not the commit's fault: try it again later.
    if err.downcast_ref::<LowDisk>().is_none() && err.downcast_ref::<DirtySource>().is_none()
        && let Some(version) = if behind { status.building.clone() } else { status.target.clone() } {
            mark_failed(data, &version);
            *failed = Some(version);
        }
    hub.set_update(|s| {
        if behind {
            s.message = Some(match s.building.take() {
                Some(newer) => format!("The newer version {newer} did not build: {message}"),
                None => format!("Could not look for a newer version: {message}"),
            });
        } else {
            s.state = UpdateState::Failed;
            s.message = Some(message);
        }
        s.step = None;
        s.waiting_until = None;
        s.waiting = None;
        s.checked_at = Some(now_ms());
    });
    // The ready version can still be installed meanwhile.
    if behind
        && let Some(version) = status.target.filter(|v| installed(data, v)) {
            if !cfg.ask || hub.install_approved(&version) {
                let hubd = data.join("bin").join(&version).join(hub_install::exe("hubd"));
                before_install(hub).await;
                if !hubd.is_file() {
                    return Next::Again;
                }
                hub.set_update(|s| s.state = UpdateState::Restarting);
                return Next::Return(Ready { version, hubd });
            }
            tokio::select! {
                _ = hub.install_requested() => {}
                _ = tokio::time::sleep(Duration::from_secs(cfg.interval_secs.max(10))) => {}
            }
            return Next::Again;
        }
    Next::Wait
}

/// The version to follow: the one the PC pushed (named in a file), or the branch's head after a
/// fetch. `None` when a pushed machine has been given none yet.
pub(super) async fn latest_remote(hub: &Hub, cfg: &UpdateConfig, data: &Path, src: &Path) -> Result<Option<String>> {
    Ok(Some(if cfg.pushed {
        // The PC put the version in `bin` before naming it here.
        match std::fs::read_to_string(pushed_file(data)).map(|v| v.trim().to_owned()) {
            Ok(version) if !version.is_empty() => version,
            _ => {
                hub.set_update(|s| {
                    s.state = UpdateState::Idle;
                    s.checked_at = Some(now_ms());
                });
                return Ok(None);
            }
        }
    } else {
        let (cfg2, src2) = (cfg.clone(), src.to_owned());
        blocking(move || {
            fetch(&src2, &cfg2.remote, &cfg2.branch)?;
            git(&src2, &["rev-parse", "--short=12", &format!("{}/{}", cfg2.remote, cfg2.branch)])
        })
        .await?
    }))
}


/// Nothing new, or the commit that failed last time: keep showing what happened.
pub(super) fn show_nothing_new(hub: &Hub, is_running: bool) {
    if is_running {
        hub.set_update(|s| {
            s.state = UpdateState::Idle;
            s.target = None;
            s.message = None;
            s.step = None;
            s.waiting_until = None;
            s.waiting = None;
            s.checked_at = Some(now_ms());
        });
    } else {
        hub.set_update(|s| s.state = UpdateState::Failed);
    }
}

/// How a build ended, for the round that asked for it.
pub(super) enum Built {
    /// Built and installed; go on.
    Done,
    /// Stopped for the version that was ready: that is the round's answer.
    Stop(Option<String>),
}

/// Builds and installs `remote`, with the step showing under Updates. While a version is ready,
/// saying install stops this build and installs the ready one.
pub(super) async fn build_remote(hub: &Arc<Hub>, cfg: &UpdateConfig, data: &Path, src: &Path, remote: &str, ready: Option<&String>) -> Result<Built> {
    let remote = remote.to_owned();
    let (cfg2, src2, data2, remote2) = (cfg.clone(), src.to_owned(), data.to_owned(), remote.clone());
    let (hub2, stop) = (hub.clone(), Arc::new(AtomicBool::new(false)));
    let stop2 = stop.clone();
    let build = blocking(move || {
        clean(&cfg2, &src2)?;
        git(&src2, &["reset", "--quiet", "--hard", &format!("{}/{}", cfg2.remote, cfg2.branch)])?;
        // Which step runs shows under Updates, so a slow build reads as slow, not stuck.
        let built = build_and_install_with(&cfg2, &data2, &src2, &remote2, &|step| hub2.set_update(|s| s.step = Some(step)), &stop2, Some(&hub2));
        LAST_BUILD_END.store(now_ms(), Ordering::Relaxed);
        built
    });
    tokio::pin!(build);
    match ready {
        None => build.await?,
        // Saying install while a newer version builds installs the ready one now: the build
        // behind it stops, and the new core builds it again.
        Some(ready) => tokio::select! {
            done = &mut build => done?,
            _ = hub.install_requested() => {
                stop.store(true, Ordering::Relaxed);
                let finished = build.await.is_ok() && installed(data, &remote);
                hub.set_update(|s| {
                    s.building = None;
                    s.step = None;
                });
                if !finished {
                    return Ok(Built::Stop(Some(ready.clone())));
                }
                // The newer build was already installing when the stop came, and that
                // removed the ready one's files: the newer one is installed instead.
                hub.approve_version(&remote);
            }
        },
    }
    Ok(Built::Done)
}
