//! Going back to the version before when a new one fails its probation. `hub-install`'s
//! `probation` keeps the files and decides ([`hub_install::probation::verdict`]); this is what the
//! core does with the answer: watch its own version, count its starts and clean exits, say when a
//! window has rendered the new pages, and, on a failed probation, put the database right, point
//! `current` at the version before and start it.

use crate::locked::ResultExt;
use crate::locked::Ignore;
use std::path::Path;
use std::process::Child;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use hub_install::probation::{self, Limits, Reason, RolledBack, Verdict};
use tokio::sync::mpsc;

use crate::config::UpdateConfig;
use crate::hub::Hub;
use crate::model::{RolledBackInfo, StreamMsg, now_ms};

/// How often the probation is looked at.
const TICK: Duration = Duration::from_secs(5);
/// Versions tried, best first, before a rollback gives up and stays where it is.
const ATTEMPTS: usize = 3;

pub fn limits(cfg: &UpdateConfig) -> Limits {
    Limits { crashes: cfg.probation_crashes, ..Limits::default() }
}

impl From<RolledBack> for RolledBackInfo {
    fn from(r: RolledBack) -> Self {
        Self { from: r.from, to: r.to, reason: r.reason, at_ms: r.at_ms as i64, db_set_aside: r.db_set_aside }
    }
}

/// What the probation of `version` says on one look.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing to watch (no probation of this version, or `current` is not it).
    Done,
    Wait,
    /// Healthy: recorded as the last good version.
    Confirmed,
    RollBack(Reason),
    /// Somebody else (the launcher, the app's shell) already rolled this version back: `current`
    /// names the older version, which this core has to make room for.
    Left(String),
}

/// One look at the probation of the running `version`. `up` is how long its core has run with the
/// health check answering; once that is `probation_mins` the core counts as healthy.
pub fn judge(data: &Path, version: &str, now_ms: u64, up: Duration, cfg: &UpdateConfig) -> Step {
    if version != "dev"
        && let Some(r) = probation::read_rolled_back(data).filter(|r| r.from == version)
        && hub_install::current_version(data).is_some_and(|c| c == r.to)
    {
        return Step::Left(r.to);
    }
    if version == "dev" || !probation::read(data).is_some_and(|p| p.version == version) {
        return Step::Done;
    }
    if hub_install::current_version(data).is_none_or(|c| c != version) {
        return Step::Done;
    }
    if up >= Duration::from_secs(cfg.probation_mins * 60) {
        probation::note_core_ok(data, version);
    }
    match probation::verdict(data, now_ms, limits(cfg)) {
        Verdict::Healthy => match probation::confirm_healthy(data, version) {
            Ok(()) => Step::Confirmed,
            Err(err) => {
                tracing::warn!("could not record {version} as the last good version: {err}");
                Step::Wait
            }
        },
        Verdict::Wait => Step::Wait,
        Verdict::RollBack(reason) => Step::RollBack(reason),
    }
}

/// Counts this start of the core of `version` as one nobody asked for, unless it is the takeover
/// (which begins the probation itself). Call before anything else runs.
pub fn started(data: &Path, version: &str, takeover: bool) {
    if version != "dev" && !takeover {
        probation::note_start(data, version);
    }
}

/// The core is about to stop because somebody asked it to: the start that follows is no crash.
pub fn clean_exit(data: &Path, version: &str) {
    if version != "dev" {
        probation::note_clean_exit(data, version);
    }
}

/// Watches the probation of `version` until it ends. The first message is the reason to roll back.
/// The channel closes without a message when there is nothing to roll back.
pub fn watch(hub: Arc<Hub>, version: String) -> mpsc::Receiver<Fell> {
    let (tx, rx) = mpsc::channel(1);
    if version != "dev" {
        tokio::spawn(async move { run(hub, version, tx).await });
    }
    rx
}

/// Why the core has to make room for an older version.
#[derive(Debug)]
pub enum Fell {
    /// Its probation failed; it goes back itself ([`go_back`]).
    Probation(Reason),
    /// Somebody else already moved `current` back to this version ([`hand_back`]).
    MovedBack(String),
}

/// Waits for the reason from [`watch`]; never ends when there will be none.
pub async fn failed(rx: &mut mpsc::Receiver<Fell>) -> Fell {
    match rx.recv().await {
        Some(reason) => reason,
        None => std::future::pending().await,
    }
}

async fn run(hub: Arc<Hub>, version: String, tx: mpsc::Sender<Fell>) {
    let data = hub.config.data_dir.clone();
    let cfg = hub.config.update.clone();
    let port = hub.config.bind.port();
    let mut up = Duration::ZERO;
    let mut last = now_ms();
    loop {
        let now = now_ms();
        // Run time, not wall clock: a computer asleep between two looks adds nothing.
        let gap = now.saturating_sub(last).max(0) as u64;
        last = now;
        if gap <= 3 * TICK.as_millis() as u64 {
            if health_ok(port).await {
                up += Duration::from_millis(gap);
            }
        } else {
            // Asleep for a while: a window had no chance to answer either.
            probation::restart_window_clock(&data, &version, now.max(0) as u64);
        }
        match judge(&data, &version, now.max(0) as u64, up, &cfg) {
            Step::Done => {
                hub.set_update(|s| s.probation = None);
                return;
            }
            Step::Confirmed => {
                hub.set_update(|s| s.probation = None);
                tracing::info!("{version} got through its probation");
                return;
            }
            Step::RollBack(reason) => {
                tracing::error!("{version} failed its probation: {reason}");
                tx.send(Fell::Probation(reason)).await.ignore("the core stopping drops the watcher; the error above says why");
                return;
            }
            Step::Left(to) => {
                tracing::error!("{version} was rolled back to {to} while it ran");
                tx.send(Fell::MovedBack(to)).await.ignore("the core stopping drops the watcher; the error above says why");
                return;
            }
            Step::Wait => {}
        }
        tokio::time::sleep(TICK).await;
    }
}

/// Whether the core on `port` answers its health check.
async fn health_ok(port: u16) -> bool {
    let Ok(client) = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(3)).build() else { return false };
    client.get(format!("http://127.0.0.1:{port}/api/health")).send().await.is_ok_and(|r| r.status().is_success())
}

/// A window says the pages of `version` rendered. Counts only for the version that runs.
pub fn alive(data: &Path, running: &str, version: &str) -> bool {
    version == running && probation::note_alive(data, version)
}

/// Shows an earlier rollback (`rolled-back.json`) in the update status until it is dismissed.
pub fn announce(hub: &Hub, version: &str) {
    if version != "dev"
        && let Some(p) = probation::read(&hub.config.data_dir).filter(|p| p.version == version)
    {
        let ends = now_ms() + hub.config.update.probation_mins as i64 * 60_000;
        hub.set_update(|s| s.probation = Some(crate::model::ProbationInfo { previous: p.previous, ends_at: Some(ends) }));
    }
    if let Some(r) = probation::read_rolled_back(&hub.config.data_dir) {
        hub.set_update(|s| s.rolled_back = Some(r.into()));
    }
}

/// The person has seen the rollback.
pub fn dismiss(hub: &Hub) {
    probation::dismiss_rolled_back(&hub.config.data_dir).warn_on_err("removing the rollback notice");
    hub.set_update(|s| s.rolled_back = None);
}

/// What `user_version` the database copy at `path` has, which is the schema its writer knew.
pub fn file_version(path: &Path) -> Result<i64> {
    let conn = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {}", path.display()))?;
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

/// Makes the database fit the version before. The copy taken before the failing version first
/// started (`backups/pre-<version>.db`) has the schema the version before understood. A database
/// at the same schema or lower stays as it is, so nothing is lost. One a newer schema has migrated
/// is set aside as `backups/rolledback-<version>.db`, and the copy takes its place. Returns the
/// name of the set-aside file. Without a copy nothing can be done, and the version before may then
/// refuse the database; the rollback says so.
pub async fn settle_database(store: &crate::store::Store, data: &Path, failing: &str, copy: Option<&str>) -> Result<Option<String>> {
    let Some(copy) = copy else { return Ok(None) };
    let copy = crate::backup::dir(data).join(copy);
    if !copy.is_file() {
        tracing::warn!("no database copy to go back to at {}", copy.display());
        return Ok(None);
    }
    let (now, then) = (store.version().await?, file_version(&copy)?);
    if now <= then {
        return Ok(None);
    }
    let aside = crate::backup::rolled_back_name(failing);
    // The newest changes are kept before they are replaced; if that fails nothing is replaced.
    store.backup(&crate::backup::dir(data).join(&aside)).await.context("setting the current database aside")?;
    store.restore(&copy).await.context("putting the database copy back")?;
    tracing::warn!("the database of {failing} was set aside as {aside}; {} is in use again", copy.display());
    Ok(Some(aside))
}

/// Goes back from `failing`: sets the database right, points `current` at the best earlier version
/// that starts (up to three tried), and records it. `start` runs the `hubd` of a version; the
/// caller has already stopped serving and parked the threads, so the port is free. When no version
/// starts, `current` is left as it was and the probation ends, so nothing loops.
pub async fn go_back(hub: &Arc<Hub>, failing: &str, reason: &Reason, mut start: impl FnMut(&Path) -> Result<Child>) -> Result<RolledBackInfo> {
    let data = hub.config.data_dir.clone();
    let candidates = probation::roll_back_candidates(&data, failing);
    if candidates.is_empty() {
        probation::end_of(&data, failing).warn_on_err("ending the probation of the failing version");
        bail!("there is no earlier version to go back to");
    }
    let copy = probation::read(&data).and_then(|p| p.db_backup);
    let db_set_aside = match settle_database(&hub.store, &data, failing, copy.as_deref()).await {
        Ok(aside) => aside,
        Err(err) => {
            probation::end_of(&data, failing).warn_on_err("ending the probation of the failing version");
            return Err(err.context("the database could not be put right for the earlier version"));
        }
    };
    let mut last: Option<anyhow::Error> = None;
    for to in candidates.iter().take(ATTEMPTS) {
        if let Err(err) = hub_install::set_current(&data, to) {
            last = Some(anyhow::Error::new(err).context("pointing the launcher at the earlier version"));
            break;
        }
        let hubd = hub_install::bin_dir(&data).join(to).join(hub_install::exe("hubd"));
        let started = match start(&hubd) {
            Ok(mut child) => await_up(&mut child, hub.config.bind.port(), crate::update::TAKEOVER_TIMEOUT).await,
            Err(err) => Err(err),
        };
        match started {
            Ok(()) => {
                // Written once the earlier version is up: it clears this marker as it takes over.
                crate::update::mark_failed(&data, failing);
                let record = RolledBack {
                    from: failing.to_owned(),
                    to: to.clone(),
                    reason: reason.to_string(),
                    at_ms: now_ms().max(0) as u64,
                    db_set_aside,
                };
                if let Err(err) = probation::write_rolled_back(&data, &record) {
                    tracing::warn!("could not note the rollback: {err}");
                }
                probation::end_of(&data, failing).warn_on_err("ending the probation of the failing version");
                return Ok(record.into());
            }
            Err(err) => {
                tracing::error!("{to} did not start when going back from {failing}: {err:#}");
                last = Some(err.context(format!("{to} did not start")));
            }
        }
    }
    hub_install::set_current(&data, failing).warn_on_err("set the current version");
    probation::end_of(&data, failing).warn_on_err("ending the probation of the failing version");
    // This core goes on, so it needs the database it migrated.
    if let Some(aside) = &db_set_aside
        && let Err(err) = hub.store.restore(&crate::backup::dir(&data).join(aside)).await
    {
        tracing::error!("could not put the database back after a failed rollback: {err:#}");
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("no earlier version started")))
}

/// `current` already names the older version `to` (somebody else rolled this one back): starts its
/// `hubd` and waits until it answers, so this core can go. Errs when it does not start.
pub async fn hand_back(hub: &Arc<Hub>, failing: &str, to: &str, mut start: impl FnMut(&Path) -> Result<Child>) -> Result<()> {
    let data = &hub.config.data_dir;
    // The files moved without the database, which only this core has open.
    let copy = format!("pre-{failing}.db");
    match settle_database(&hub.store, data, failing, Some(&copy)).await {
        Ok(Some(aside)) => {
            if let Some(mut r) = probation::read_rolled_back(data) {
                r.db_set_aside = Some(aside);
                probation::write_rolled_back(data, &r).warn_on_err("write the rollback note");
            }
        }
        Ok(None) => {}
        Err(err) => bail!("the database could not be put right for {to}: {err:#}"),
    }
    let hubd = hub_install::bin_dir(&hub.config.data_dir).join(to).join(hub_install::exe("hubd"));
    let mut child = start(&hubd)?;
    await_up(&mut child, hub.config.bind.port(), crate::update::TAKEOVER_TIMEOUT).await
}

/// For the older version that starts after a rollback done by files alone (the launcher or the
/// app's shell cannot open the database): when `hub.db` was migrated past what this build knows,
/// it is set aside as `backups/rolledback-<version>.db` and the copy taken before that version
/// first started takes its place. Call before the database is opened.
pub fn settle_database_file(data: &Path) {
    let Some(mut r) = probation::read_rolled_back(data).filter(|r| r.db_set_aside.is_none()) else { return };
    let db = data.join("hub.db");
    let copy = crate::backup::before_update(data, &r.from);
    let ours = crate::store::schema_version() as i64;
    let (Ok(now), true) = (file_version(&db), copy.is_file()) else { return };
    if now <= ours || file_version(&copy).is_ok_and(|v| v > ours) || file_version(&copy).is_err() {
        return;
    }
    let aside = crate::backup::rolled_back_name(&r.from);
    // A copy through SQLite, which reads what is still in the write-ahead log.
    let aside_path = crate::backup::dir(data).join(&aside);
    let kept = (|| -> Result<()> {
        crate::locked::clean_file(&aside_path);
        rusqlite::Connection::open(&db)?.execute("VACUUM INTO ?1", [aside_path.display().to_string()])?;
        Ok(())
    })();
    if let Err(err) = kept {
        tracing::error!("could not set the database aside, so it stays: {err:#}");
        return;
    }
    let result = Ok::<(), std::io::Error>(()).and_then(|_| {
        for extra in ["hub.db-wal", "hub.db-shm"] {
            crate::locked::clean_file(data.join(extra));
        }
        std::fs::copy(&copy, &db)
    });
    match result {
        Ok(_) => {
            tracing::warn!("the database of {} was set aside as {aside}; the copy from before it is back", r.from);
            r.db_set_aside = Some(aside);
            probation::write_rolled_back(data, &r).warn_on_err("write the rollback note");
        }
        Err(err) => tracing::error!("could not put the database right after a rollback: {err}"),
    }
}

/// Tells the surfaces the core is going back, just before it hands over.
pub fn tell(hub: &Hub, failing: &str, reason: &Reason) {
    hub.publish(StreamMsg::RolledBack {
        info: RolledBackInfo {
            from: failing.to_owned(),
            to: probation::roll_back_candidates(&hub.config.data_dir, failing).into_iter().next().unwrap_or_default(),
            reason: reason.to_string(),
            at_ms: now_ms(),
            db_set_aside: None,
        },
    });
}

async fn await_up(child: &mut Child, port: u16, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        if health_ok(port).await {
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            bail!("exited at start ({status})");
        }
        if start.elapsed() > timeout {
            child.kill().warn_on_err("kill the timed-out version");
            child.wait().warn_on_err("wait for the killed version");
            bail!("did not answer within {} seconds", timeout.as_secs());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hub_install::probation::Probation;

    fn version(data: &Path, name: &str) {
        let dir = hub_install::bin_dir(data).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for p in probation::PROGRAMS {
            std::fs::write(dir.join(hub_install::exe(p)), "").unwrap();
        }
    }

    fn on_probation(data: &Path, new: &str, old: &str) {
        version(data, new);
        version(data, old);
        hub_install::set_current(data, new).unwrap();
        probation::begin(data, new, Some(old), None, 0).unwrap();
    }

    fn cfg() -> UpdateConfig {
        UpdateConfig::default()
    }

    #[test]
    fn a_core_that_stays_up_long_enough_becomes_the_last_good_version() {
        let d = tempfile::tempdir().unwrap();
        on_probation(d.path(), "new", "old");
        assert_eq!(judge(d.path(), "new", 1000, Duration::from_secs(60), &cfg()), Step::Wait);
        assert_eq!(judge(d.path(), "new", 1000, Duration::from_secs(300), &cfg()), Step::Confirmed);
        assert_eq!(probation::last_good(d.path()).as_deref(), Some("new"));
        // Nothing left to watch once it is through.
        assert_eq!(judge(d.path(), "new", 1000, Duration::from_secs(400), &cfg()), Step::Done);
    }

    #[test]
    fn a_window_that_is_needed_has_to_say_it_is_alive_first() {
        let d = tempfile::tempdir().unwrap();
        on_probation(d.path(), "new", "old");
        probation::note_window_needed(d.path(), "new", 0);
        let up = Duration::from_secs(301);
        assert_eq!(judge(d.path(), "new", 10_000, up, &cfg()), Step::Wait);
        assert!(!alive(d.path(), "new", "old"), "pages of another version do not count");
        assert!(!alive(d.path(), "new", "other"));
        assert!(alive(d.path(), "new", "new"));
        assert_eq!(judge(d.path(), "new", 10_000, up, &cfg()), Step::Confirmed);
    }

    #[test]
    fn a_window_that_stays_silent_rolls_the_version_back() {
        let d = tempfile::tempdir().unwrap();
        on_probation(d.path(), "new", "old");
        probation::note_window_needed(d.path(), "new", 0);
        assert_eq!(judge(d.path(), "new", 91_000, Duration::ZERO, &cfg()), Step::RollBack(Reason::NoWindow));
    }

    #[test]
    fn unasked_restarts_fail_the_probation_but_asked_ones_do_not() {
        let d = tempfile::tempdir().unwrap();
        on_probation(d.path(), "new", "old");
        // A settings restart: a clean exit, then the takeover begins the same version again.
        clean_exit(d.path(), "new");
        probation::begin(d.path(), "new", Some("old"), None, 5).unwrap();
        assert_eq!(judge(d.path(), "new", 1000, Duration::ZERO, &cfg()), Step::Wait);
        // Two starts nobody asked for.
        started(d.path(), "new", false);
        assert_eq!(judge(d.path(), "new", 1000, Duration::ZERO, &cfg()), Step::Wait);
        started(d.path(), "new", false);
        assert_eq!(judge(d.path(), "new", 1000, Duration::ZERO, &cfg()), Step::RollBack(Reason::Crashed(2)));
    }

    #[test]
    fn the_limits_come_from_the_config() {
        let d = tempfile::tempdir().unwrap();
        on_probation(d.path(), "new", "old");
        started(d.path(), "new", false);
        let mut one = cfg();
        one.probation_crashes = 1;
        assert_eq!(judge(d.path(), "new", 1000, Duration::ZERO, &one), Step::RollBack(Reason::Crashed(1)));
        let mut short = cfg();
        short.probation_mins = 1;
        let d = tempfile::tempdir().unwrap();
        on_probation(d.path(), "new", "old");
        assert_eq!(judge(d.path(), "new", 1000, Duration::from_secs(60), &short), Step::Confirmed);
    }

    #[test]
    fn a_dev_build_and_a_version_left_behind_are_never_judged() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(judge(d.path(), "dev", 0, Duration::ZERO, &cfg()), Step::Done);
        on_probation(d.path(), "new", "old");
        hub_install::set_current(d.path(), "other").unwrap();
        assert_eq!(judge(d.path(), "new", 0, Duration::from_secs(999), &cfg()), Step::Done);
        // The dev build counts nothing either.
        started(d.path(), "dev", false);
        clean_exit(d.path(), "dev");
        assert!(probation::read(d.path()).is_some_and(|p: Probation| p.starts == 1 && p.clean_exits == 0));
    }

    #[test]
    fn a_rollback_done_by_the_launcher_makes_the_running_core_make_room() {
        let d = tempfile::tempdir().unwrap();
        on_probation(d.path(), "new", "old");
        std::fs::remove_file(hub_install::bin_dir(d.path()).join("new").join(hub_install::exe("hubd"))).unwrap();
        let r = probation::roll_back(d.path(), "new", "gone", 5).unwrap();
        assert_eq!(judge(d.path(), "new", 0, Duration::ZERO, &cfg()), Step::Left(r.to));
        // The older version itself has nothing to make room for.
        assert_eq!(judge(d.path(), "old", 0, Duration::ZERO, &cfg()), Step::Done);
    }

    #[test]
    fn a_database_migrated_past_this_build_is_set_aside_by_file() {
        let d = tempfile::tempdir().unwrap();
        let schema = crate::store::schema_version() as i64;
        let db = d.path().join("hub.db");
        drop(crate::store::Store::open(&db).unwrap());
        let copy = crate::backup::before_update(d.path(), "new");
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        std::fs::copy(&db, &copy).unwrap();
        rusqlite::Connection::open(&db).unwrap().pragma_update(None, "user_version", schema + 1).unwrap();
        let record = RolledBack { from: "new".into(), to: "old".into(), reason: "x".into(), at_ms: 1, db_set_aside: None };
        probation::write_rolled_back(d.path(), &record).unwrap();
        settle_database_file(d.path());
        assert_eq!(file_version(&db).unwrap(), schema);
        let aside = probation::read_rolled_back(d.path()).unwrap().db_set_aside;
        assert_eq!(aside.as_deref(), Some("rolledback-new.db"));
        assert_eq!(file_version(&crate::backup::dir(d.path()).join("rolledback-new.db")).unwrap(), schema + 1);
        // A second look has nothing to do.
        settle_database_file(d.path());
        assert_eq!(file_version(&db).unwrap(), schema);
    }

    #[test]
    fn a_takeover_start_is_not_counted_twice() {
        let d = tempfile::tempdir().unwrap();
        on_probation(d.path(), "new", "old");
        started(d.path(), "new", true);
        assert_eq!(probation::read(d.path()).unwrap().starts, 1);
    }

    #[tokio::test]
    async fn a_database_a_newer_schema_migrated_is_set_aside_and_the_copy_comes_back() {
        let d = tempfile::tempdir().unwrap();
        let schema = crate::store::schema_version() as i64;
        // The copy from before the update, at the schema the version before knew.
        let copy = crate::backup::before_update(d.path(), "new");
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        let store = crate::store::Store::open(&d.path().join("hub.db")).unwrap();
        store.backup(&copy).await.unwrap();
        // The live database has been migrated further and has taken a change since.
        store.set_value("after", "update").await.unwrap();
        store.with(move |c| Ok(c.pragma_update(None, "user_version", schema + 1)?)).await.unwrap();

        let aside = settle_database(&store, d.path(), "new", Some("pre-new.db")).await.unwrap();
        assert_eq!(aside.as_deref(), Some("rolledback-new.db"));
        assert_eq!(store.version().await.unwrap(), schema);
        assert!(store.get_value("after").await.unwrap().is_none(), "the copy replaced the database");
        let kept = crate::backup::dir(d.path()).join("rolledback-new.db");
        assert_eq!(file_version(&kept).unwrap(), schema + 1);
    }

    #[tokio::test]
    async fn a_database_at_the_same_schema_is_left_alone() {
        let d = tempfile::tempdir().unwrap();
        let copy = crate::backup::before_update(d.path(), "new");
        std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
        let store = crate::store::Store::open(&d.path().join("hub.db")).unwrap();
        store.backup(&copy).await.unwrap();
        store.set_value("after", "update").await.unwrap();
        assert_eq!(settle_database(&store, d.path(), "new", Some("pre-new.db")).await.unwrap(), None);
        assert_eq!(store.get_value("after").await.unwrap().as_deref(), Some("update"));
        // No copy at all is no reason to touch it either.
        assert_eq!(settle_database(&store, d.path(), "new", None).await.unwrap(), None);
        assert_eq!(settle_database(&store, d.path(), "new", Some("pre-gone.db")).await.unwrap(), None);
    }
}
