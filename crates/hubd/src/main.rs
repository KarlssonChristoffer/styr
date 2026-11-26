//! `hubd` runs the core in the background: threads keep going when the app is closed.
//!
//! Config is read from `hub.json` in the data folder, or from the path in `HUB_CONFIG`.
//!
//! `hubd install [--source DIR]` builds the checkout in DIR (default `<data dir>/src`), installs
//! it, points the launcher at it and starts Hub. After that the core keeps itself up to date.
//!
//! `hubd threads` lists the threads on this machine; `hubd attach <thread> [--now | --if-idle]` follows one
//! live in the terminal and takes it over on Enter (see `hub_core::terminal`).
//!
//! `hubd sign-in <engine>` signs an engine in on this machine in a way that lasts, and tells the
//! running core (see `hub_core::signin`).


use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use hub_core::locked::Ignore;
use hub_core::{Config, Hub, Store, api, update};
use tokio::sync::Notify;
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    cap_malloc_arenas();
    run()
}

/// glibc gives every thread its own malloc arena (up to 8 per core) and keeps what a burst of big
/// short-lived buffers grew them to: on tincan four 64 MB arenas held ~260 MB of a 385 MB idle hubd.
/// Two arenas is plenty for this load. Must run before the runtime starts its threads.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn cap_malloc_arenas() {
    // Respect an operator's own MALLOC_ARENA_MAX.
    if std::env::var_os("MALLOC_ARENA_MAX").is_none() {
        // SAFETY: mallopt only sets a glibc tunable; called once at start, before other threads.
        unsafe { libc::mallopt(libc::M_ARENA_MAX, 2) };
    }
}
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn cap_malloc_arenas() {}

#[tokio::main]
async fn run() -> Result<()> {
    hub_core::supervise::quiet_errors();
    if helper_mode().await? {
        return Ok(());
    }
    let mut args = std::env::args().skip(1);
    let config = load_config()?;
    if terminal_command(&config).await? {
        return Ok(());
    }
    let version = hub_install::running_version();
    init_logging(&config, &version)?;
    // Work from the data folder, not from `bin/<version>/`: everything hubd starts (engines,
    // git, ssh) inherits its working folder, and on Windows a folder some process is in cannot
    // be removed, which kept every old version's folder in `bin` for good.
    if std::fs::create_dir_all(&config.data_dir).is_ok()
        && let Err(err) = std::env::set_current_dir(&config.data_dir)
    {
        tracing::warn!("could not make {} the working folder: {err}", config.data_dir.display());
    }

    if args.next().as_deref() == Some("install") {
        let source = match (args.next().as_deref(), args.next()) {
            (Some("--source"), Some(dir)) => PathBuf::from(dir),
            (None, _) => config.update.source_in(&config.data_dir),
            _ => bail!("usage: hubd install [--source DIR]"),
        };
        return install(&config, &source);
    }

    // The port first: it tells the app (and an older core handing over) that someone is here,
    // also while a long migration runs.
    let bind = config.bind;
    let mut listener = bind_with_retry(bind).await?;
    // After a rollback done by files alone, the database may be ahead of this build.
    hub_core::rollback::settle_database_file(&config.data_dir);
    let store = Store::open(&config.data_dir.join("hub.db"))?;
    let data = config.data_dir.clone();
    let update_cfg = config.update.clone();
    let ended = hub_core::supervise::end_stale_tunnels();
    if ended > 0 {
        tracing::info!("ended {ended} SSH tunnels left by an earlier core");
    }
    let hub = Hub::new(store, config);
    if let Err(err) = hub_core::halt::load(&hub).await {
        tracing::warn!("could not read the pause: {err:#}");
    }
    lift_old_pictures(&hub);
    start_services(&hub);
    if let Err(err) = hub.secrets.load().await {
        tracing::warn!("could not read the secrets: {err:#}");
    }
    hub.load_sign_ins().await;

    tracing::info!("hubd {version} listening on http://{bind}");
    update::took_over(&data, &version)?;
    // A start nobody asked for counts against a version on probation; the takeover counts itself.
    hub_core::rollback::started(&data, &version, std::env::var_os(update::TAKEOVER_VAR).is_some());
    hub_core::rollback::announce(&hub, &version);
    let mut probation = hub_core::rollback::watch(hub.clone(), version.clone());
    // The previous core parked its engines before it started this one, or it crashed.
    tokio::spawn(resume(hub.clone()));

    loop {
        let shutdown = Arc::new(Notify::new());
        let mut server = serve(listener, hub.clone(), shutdown.clone());
        // A new version from the updater, or the same one again after settings changed.
        let updates = update_watch(hub.clone(), update_cfg.clone(), data.clone(), version.clone());
        let next = tokio::select! {
            result = &mut server => {
                hub_core::rollback::clean_exit(&data, &version);
                hub.park("stop").await;
                result??;
                return Ok(());
            }
            next = updates => next,
            _ = hub_core::prefs::restart_requested() => {
                tracing::info!("restarting for new settings");
                hub_core::rollback::clean_exit(&data, &version);
                tokio::time::sleep(SURFACES_HEAR).await;
                shutdown.notify_waiters();
                stop_server(server).await;
                hub.park("restart").await;
                spawn_detached(&std::env::current_exe()?, &[(update::TAKEOVER_VAR, "1")])?;
                std::process::exit(0);
            }
            fell = hub_core::rollback::failed(&mut probation) => {
                listener = roll_back(&hub, &version, fell, server, &shutdown, (&data, bind)).await?;
                continue;
            }
        };
        listener = hand_over(&hub, &next, server, &shutdown, (&data, &version, bind)).await?;
    }
}

/// Runs one of the helper modes `hubd` is started in by something else (ssh, Claude Code, the
/// supervisor, a keeper, the simulator). `true` when it ran one, so the core is not started.
async fn helper_mode() -> Result<bool> {
    // Started by ssh as its password prompt, for putting this machine's key on a new one.
    if let Ok(secret) = std::env::var(hub_core::sshkey::SECRET_ENV) {
        println!("{secret}");
        return Ok(true);
    }
    // Started by Claude Code as its shell prefix, to run one command (`live_output.rs`).
    if hub_core::live_output::is_wrapper() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        std::process::exit(hub_core::live_output::run_wrapper(&args));
    }
    if std::env::args().nth(1).as_deref() == Some(hub_core::supervise::ENGINE_ARG) {
        let command: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(hub_core::supervise::run_contained(&command)?);
    }
    // The elevated helper of a terminal as administrator (`pty.rs`), started through Windows' UAC.
    if std::env::args().nth(1).as_deref() == Some(hub_core::pty::HOST_ARG) {
        let rest: Vec<String> = std::env::args().skip(2).collect();
        std::process::exit(hub_core::pty::host(&rest, hub_core::auth::token).await?);
    }
    // Keeps one engine running apart from the core, so a restart of the core does not end it.
    if std::env::args().nth(1).as_deref() == Some(hub_core::keeper::KEEP_ARG) {
        std::process::exit(hub_core::keeper::serve().await?);
    }
    // The simulator's fake agent (Labs), on stdio like any engine.
    if std::env::args().nth(1).as_deref() == Some(SIM_AGENT_ARG) {
        mock_agent::serve(true).await.map_err(|e| anyhow::anyhow!("{e}"))?;
        return Ok(true);
    }
    Ok(false)
}

/// Commands for a terminal, talking to the running core; no logging setup of their own. `true`
/// when it ran one.
async fn terminal_command(config: &Config) -> Result<bool> {
    let rest: Vec<String> = std::env::args().skip(2).collect();
    match std::env::args().nth(1).as_deref() {
        Some(hub_core::terminal::ATTACH_ARG) => hub_core::terminal::attach(config, &rest).await?,
        Some(hub_core::terminal::THREADS_ARG) => hub_core::terminal::list(config).await?,
        Some(hub_core::signin::SIGN_IN_ARG) => hub_core::signin::run(config, &rest).await?,
        Some(hub_core::signin::CLI_ARG) => hub_core::signin::cli(config, &rest).await?,
        Some(hub_core::signin::ROOT_ARG) => hub_core::signin::root(config).await?,
        Some(hub_core::terminal::PAIR_ARG) => hub_core::terminal::pair(config, &rest).await?,
        _ => return Ok(false),
    }
    Ok(true)
}

/// Starts the loops that run for as long as the core does, each restarted if it fails.
fn start_services(hub: &Arc<Hub>) {
    let hub = hub.clone();
    // The copy is a VACUUM of the whole database; it waits until the core has been up a while.
    keep("backup::daily", &hub, |hub| hub_core::backup::daily(hub.store.clone(), hub.config.data_dir.clone()));
    keep("devlog::daily", &hub, Hub::devlog_daily);
    hub_core::problems::start(hub.clone());
    keep("health::weekly", &hub, |hub| hub_core::health::weekly(hub.store.clone(), hub.config.clone()));
    keep("routines::scheduler", &hub, hub_core::routine_runs::scheduler);
    keep("customs_review::cadence", &hub, hub_core::customs_review::cadence);
    keep("customs::share", &hub, Hub::share_customs);
    keep("engines::share", &hub, hub_core::engine_share::share);
    keep("autologin::watch", &hub, hub_core::autologin::watch);
    // Services a harness needs before a thread does, such as Hermes's gateway (`standby.rs`).
    keep("standby::keep", &hub, hub_core::standby::keep);
    keep("engines::renew_engines", &hub, hub_core::engine_renew::renew_engines);
    keep("updates::follow", &hub, hub_core::updates::follow);
    keep("awake::hold", &hub, hub_core::awake::hold);
    keep("autopull::keep_current", &hub, hub_core::autopull::keep_current);
    keep("own_tools::follower", &hub, Hub::follow_handoffs);
    keep("facts::sweeper", &hub, hub_core::facts::sweeper);
    keep("worker_runs::follower", &hub, hub_core::worker_runs::follower);
    // A start-up routine, not a loop: running it again would restart the UI server it started.
    tokio::spawn(hub_core::dev::follow(hub.clone()));
    // The delegations the core was waiting on when it last stopped report back again.
    tokio::spawn(hub_core::hub_tools::resume_delegations(hub.clone()));
    // A bench run cut short by a restart carries on, or ends when the restart was an update.
    tokio::spawn(hub_core::bench::run::resume(hub.clone()));
    keep("janitor::often", &hub, hub_core::janitor::often);
    keep("sleep::watch", &hub, hub_core::sleep::watch);
}

/// One of the core's loops, started with its own handle on the hub and restarted if it fails.
fn keep<F, Fut>(name: &'static str, hub: &Arc<Hub>, run: F)
where
    F: Fn(Arc<Hub>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let hub = hub.clone();
    hub_core::supervise::forever(name, move || run(hub.clone()));
}

/// Serves the API on the listener until Ctrl-C or `shutdown`.
fn serve(listener: tokio::net::TcpListener, hub: Arc<Hub>, shutdown: Arc<Notify>) -> tokio::task::JoinHandle<std::io::Result<()>> {
    let router = api::router(hub);
    tokio::spawn(async move {
        axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .with_graceful_shutdown(async move {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = shutdown.notified() => {}
                }
            })
            .await
    })
}

/// A new version from the updater, or the same one again after settings changed; never ready
/// when updates are off for this build.
async fn update_watch(hub: Arc<Hub>, update_cfg: hub_core::config::UpdateConfig, data: PathBuf, version: String) -> update::Ready {
    if update_cfg.is_enabled(&version) {
        hub_core::prci::start(&hub);
        update::run(hub, update_cfg, data).await
    } else {
        std::future::pending().await
    }
}

/// This version is broken (or the launcher moved back): hands the port back to the one before it
/// and exits. When that fails, stays on this version and returns the listener bound again.
async fn roll_back(
    hub: &Arc<Hub>,
    version: &str,
    fell: hub_core::rollback::Fell,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
    shutdown: &Notify,
    (data, bind): (&std::path::Path, std::net::SocketAddr),
) -> Result<tokio::net::TcpListener> {
    let start = |hubd: &std::path::Path| spawn_detached(hubd, &[(update::TAKEOVER_VAR, "1")]);
    let (why, went) = match &fell {
        hub_core::rollback::Fell::Probation(reason) => {
            hub_core::rollback::tell(hub, version, reason);
            (reason.to_string(), None)
        }
        hub_core::rollback::Fell::MovedBack(to) => (format!("{to} was made current by the launcher or the app"), Some(to.clone())),
    };
    tokio::time::sleep(Duration::from_millis(800)).await;
    shutdown.notify_waiters();
    stop_server(server).await;
    hub.park("update").await;
    let outcome = match (&fell, &went) {
        (hub_core::rollback::Fell::Probation(reason), _) => hub_core::rollback::go_back(hub, version, reason, start)
            .await
            .map(|info| tracing::info!("went back from {} to {}: {}", info.from, info.to, info.reason)),
        (_, Some(to)) => hub_core::rollback::hand_back(hub, version, to, start).await,
        _ => Ok(()),
    };
    match outcome {
        Ok(()) => std::process::exit(0),
        Err(err) => {
            let message = format!("{version} failed ({why}) and Hub could not go back: {err:#}; staying on it");
            tracing::error!("{message}");
            update::mark_failed(data, version);
            let listener = bind_with_retry_for(bind, 60).await?;
            hub.fail_update(version, message);
            hub.unpark();
            tokio::spawn(resume(hub.clone()));
            Ok(listener)
        }
    }
}

/// Hands the port to the new version and exits once it took over. When it did not, stays on this
/// version, puts the database back if the new one migrated it, and returns the listener bound again.
async fn hand_over(
    hub: &Arc<Hub>,
    next: &update::Ready,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
    shutdown: &Notify,
    (data, version, bind): (&std::path::Path, &str, std::net::SocketAddr),
) -> Result<tokio::net::TcpListener> {
    let backup = hub_core::backup::before_update(data, &next.version);
    if let Err(err) = hub.store.backup(&backup).await {
        tracing::warn!("no copy of the database before the update: {err:#}");
    }
    hub_core::backup::prune_before_update(data);

    tracing::info!("handing over to {}", next.version);
    // Let surfaces hear that we are restarting before the socket goes away.
    tokio::time::sleep(SURFACES_HEAR).await;
    shutdown.notify_waiters();
    // Open WebSockets would hold a graceful shutdown forever; the port has to be free.
    stop_server(server).await;
    // Threads still working stop here and carry on in the new core.
    hub.park("update").await;

    let outcome = match spawn_detached(&next.hubd, &[(update::TAKEOVER_VAR, "1")]) {
        Ok(mut child) => update::await_takeover(data, &next.version, &mut child, update::TAKEOVER_TIMEOUT).await,
        Err(err) => Err(err),
    };
    match outcome {
        Ok(()) => {
            tracing::info!("{} took over", next.version);
            std::process::exit(0);
        }
        Err(err) => {
            let message = format!("{err:#}; staying on {version}");
            tracing::error!("update failed: {message}");
            update::mark_failed(data, &next.version);
            // The new version may have migrated the database before it failed.
            if hub.store.version().await.unwrap_or(0) > hub_core::store::schema_version() as i64 {
                match hub.store.restore(&backup).await {
                    Ok(()) => tracing::warn!("restored the database from {}", backup.display()),
                    Err(err) => tracing::error!("could not restore the database: {err:#}"),
                }
            }
            let listener = bind_with_retry_for(bind, 60).await?;
            hub.fail_update(&next.version, message);
            hub.unpark();
            tokio::spawn(resume(hub.clone()));
            Ok(listener)
        }
    }
}

/// Pictures in old rows move out to files once, after a copy of the database that was checked.
fn lift_old_pictures(hub: &Arc<Hub>) {
    let hub = hub.clone();
    tokio::spawn(async move {
        let copy = hub_core::backup::dir(&hub.config.data_dir).join("before-pictures.db");
        match hub.store.lift_old_pictures(&copy).await {
            Ok(0) => {}
            Ok(n) => tracing::info!("moved the pictures of {n} log rows out to files (copy of the database: {})", copy.display()),
            Err(err) => tracing::warn!("moving old pictures out of the log failed, nothing was changed: {err:#}"),
        }
        // Old streamed replies fold to one row each (`compact.rs`), after that copy.
        hub_core::compact::at_start(&hub.store, &hub.config.data_dir).await;
    });
}

/// How long surfaces get to hear that the core is restarting before the socket goes away. They
/// reconnect on their own, so this only has to cover the notice leaving the process.
const SURFACES_HEAR: Duration = Duration::from_millis(150);

/// Carries on the turns the restart cut off.
async fn resume(hub: Arc<Hub>) {
    if let Err(err) = hub_core::restart::resume(&hub).await {
        tracing::warn!("could not carry threads on after the restart: {err:#}");
    }
}

/// Stops serving and lets go of the port, cutting open WebSockets after a moment.
async fn stop_server(mut server: tokio::task::JoinHandle<std::io::Result<()>>) {
    if tokio::time::timeout(Duration::from_secs(2), &mut server).await.is_err() {
        server.abort();
        server.await.ignore("the server task was just aborted; its result is the cancel, and the port is let go either way");
    }
}

/// A new version starts while the old one is still letting go of the port.
async fn bind_with_retry(bind: std::net::SocketAddr) -> Result<tokio::net::TcpListener> {
    let attempts = if std::env::var_os(update::TAKEOVER_VAR).is_some() { 60 } else { 1 };
    bind_with_retry_for(bind, attempts).await
}

async fn bind_with_retry_for(bind: std::net::SocketAddr, attempts: u32) -> Result<tokio::net::TcpListener> {
    for attempt in 1..=attempts {
        match tokio::net::TcpListener::bind(bind).await {
            Ok(listener) => return Ok(listener),
            Err(_) if attempt < attempts => tokio::time::sleep(Duration::from_millis(250)).await,
            Err(err) => return Err(err).with_context(|| format!("binding {bind}; is hubd already running?")),
        }
    }
    unreachable!()
}

fn install(config: &Config, source: &std::path::Path) -> Result<()> {
    let version = git_short_head(source)?;
    println!("Building Hub {version} from {} (this takes a few minutes the first time)…", source.display());
    update::build_and_install(&config.update, &config.data_dir, source, &version, &|step| println!("{step}"), &Default::default())?;
    hub_install::set_current(&config.data_dir, &version)?;
    let dir = hub_install::current_dir(&config.data_dir).context("installed version is missing")?;
    let app = dir.join(hub_install::exe("hub-app"));
    if app.exists() {
        println!("Installed {version}. Starting Hub.");
        spawn_detached(&app, &[]).map(drop)
    } else {
        println!("Installed {version}.");
        Ok(())
    }
}

fn git_short_head(dir: &std::path::Path) -> Result<String> {
    match hub_core::git::short_commit(dir, "HEAD") {
        Some(short) => Ok(short),
        None => bail!("{} is not a git checkout (or git is missing)", dir.display()),
    }
}

/// Starts `exe` on its own, from the data folder (see `main`), never from its own `bin` folder.
fn spawn_detached(exe: &std::path::Path, env: &[(&str, &str)]) -> Result<std::process::Child> {
    let mut cmd = hub_install::proc::command(exe);
    cmd.current_dir(hub_install::data_dir());
    cmd.envs(env.iter().copied());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
    }
    cmd.spawn().with_context(|| format!("starting {}", exe.display()))
}

/// Installed builds run without a console, so they log to `<data dir>/logs/hubd.<date>.log`, a
/// new file each day, keeping two weeks.
fn init_logging(config: &Config, version: &str) -> Result<()> {
    TRAIL.set_release(version);
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    if version == "dev" {
        tracing_subscriber::registry().with(filter).with(tracing_subscriber::fmt::layer()).with(ToProblems).init();
    } else {
        let logs = tracing_appender::rolling::Builder::new()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("hubd")
            .filename_suffix("log")
            .max_log_files(14)
            .build(config.data_dir.join("logs"))
            .context("opening the log folder")?;
        let file = tracing_subscriber::fmt::layer().with_ansi(false).with_writer(logs);
        tracing_subscriber::registry().with(filter).with(file).with(ToProblems).init();
    }
    Ok(())
}

/// Warnings and errors in the core's log also become problems on the Problems page, with the
/// log lines before them (`hub_core::problems::LogTrail`).
struct ToProblems;

static TRAIL: hub_core::problems::LogTrail = hub_core::problems::LogTrail::new();

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ToProblems {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Text(String, String);
        impl tracing::field::Visit for Text {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                } else {
                    self.1.push_str(&format!(" {}={value:?}", field.name()));
                }
            }
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "message" {
                    self.0 = value.to_owned();
                } else {
                    self.1.push_str(&format!(" {}={value}", field.name()));
                }
            }
        }
        let meta = event.metadata();
        let mut text = Text(String::new(), String::new());
        event.record(&mut text);
        TRAIL.line(meta.level().as_str(), meta.target(), meta.file(), format!("{}{}", text.0, text.1));
    }
}

/// Runs the simulator's fake agent instead of the core.
const SIM_AGENT_ARG: &str = "sim-agent";

fn load_config() -> Result<Config> {
    let mut config = hub_core::config::load();
    // The simulator's engine is this program, so it comes with every build.
    if let Ok(exe) = std::env::current_exe() {
        config.engines.entry(hub_core::sim::ENGINE.into()).or_insert_with(|| hub_core::config::EngineConfig {
            command: format!("\"{}\" {SIM_AGENT_ARG}", exe.display()),
            driver: Default::default(),
            secrets: Vec::new(),
            terminal: None,
            traits: Default::default(), ..Default::default()
        });
    }
    Ok(config)
}
