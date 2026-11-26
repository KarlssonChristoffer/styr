//! The handover to a new version: the new `hubd` says it took over (`took_over`, which starts its
//! probation), the old core waits for that or stops it (`await_takeover`), lets nearly done turns
//! finish first, and remembers a version that failed so it is not tried again.

use crate::locked::ResultExt;
use super::*;

/// Remembers a version that failed to build, test or start, so it is not tried again.
pub fn mark_failed(data: &Path, version: &str) {
    if let Err(err) = std::fs::write(failed_file(data), version) {
        tracing::warn!("could not remember the failed version: {err}");
    }
}

/// On a machine the PC pushes to (`update.pushed`), the version the PC sent last.
pub fn pushed_file(data: &Path) -> PathBuf {
    data.join("pushed")
}

pub(super) fn failed_file(data: &Path) -> PathBuf {
    data.join("update-failed")
}

/// Called by a `hubd` started to take over, once it has opened the database and bound the port:
/// from now on the launcher starts this version, and the old core knows it can go.
pub fn took_over(data: &Path, version: &str) -> Result<()> {
    if std::env::var_os(TAKEOVER_VAR).is_none() || version == "dev" {
        return Ok(());
    }
    let previous = hub_install::current_version(data);
    hub_install::set_current(data, version).context("pointing the launcher at this version")?;
    crate::locked::clean_file(failed_file(data));
    begin_probation(data, version, previous.as_deref());
    Ok(())
}

pub(super) fn begin_probation(data: &Path, version: &str, previous: Option<&str>) {
    // From here the version is on probation until it has shown it is healthy (`hub_install::probation`).
    let copy = format!("pre-{version}.db");
    let copy = crate::backup::dir(data).join(&copy).is_file().then_some(copy);
    if let Err(err) = hub_install::probation::begin(data, version, previous, copy.as_deref(), now_ms().max(0) as u64) {
        tracing::warn!("could not start the probation of {version}: {err}");
    }
}

/// Waits for the new `hubd` to take over. On failure it is stopped and the reason returned.
pub async fn await_takeover(data: &Path, version: &str, child: &mut std::process::Child, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    loop {
        if hub_install::current_version(data).as_deref() == Some(version) {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            bail!("version {version} exited at start ({status}); see logs/hubd.*.log");
        }
        if start.elapsed() > timeout {
            child.kill().warn_on_err("kill the new version");
            child.wait().warn_on_err("wait for the killed version");
            bail!("version {version} did not come up within {} seconds", timeout.as_secs());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Gives turns that are nearly done a moment to finish. Those still going after it are cut off
/// and carried on by the new core (`crate::restart`). Turns whose engine runs under a keeper
/// (`crate::keeper`) are not waited for: the new core takes them over where they are.
pub(super) async fn wait_until_idle(hub: &Hub) {
    let until = std::time::Instant::now() + IDLE_WAIT;
    while std::time::Instant::now() < until && hub.busy_unkept().await.unwrap_or(false) {
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
