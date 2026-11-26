//! The core: owns threads, writes every change to the store and fans it out to surfaces.

use crate::locked::{Ignore, LockExt, send_or_log};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use serde_json::json;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::config::Config;
use crate::engine::{self, Command, Prompt};
use crate::model::{
    Attachment, CreatedProject, Event, EventKind, NewProject, NewThread, Project, StreamMsg, Thread, ThreadId, ThreadPatch,
    AttemptData, CheckpointData, ClashData, CustomsData, CustomsFound, DocsData, DocsSection, LeakData, LeakFinding, ProofData, EngineSwitchData, ReviewChunk, ReviewData, MovedData, RestoredData, RewindData, RuleSavedData, written,
    AttemptGroup, AttemptInfo, PermissionPrefs, ReviewRequest, ReviewStarted, ThreadChange, ThreadStatus, UndoPreview, UpdateState, UpdateStatus, now_ms,
};
use crate::store::Store;
use crate::usage::GuardPrefs;

/// Key in the store under which a core that hands over says why: "update", "restart", "stop"
/// (`Hub::park`). Without it the next core takes the restart for a crash (`restart::resume`).
pub const RESTART_PLANNED: &str = "restart.planned";

// Child modules the hub.rs split moves code into (see the plan for the H1-H6 slices). Empty until then.
mod after_turn;
pub use after_turn::LookHook;
mod customs_review;
mod customs_share;
mod cloud_threads;
mod design;
mod desk;
mod devlog;
mod focus;
mod lifecycle;
mod observers;
mod permissions;
mod reach;
mod reactions;
mod recipes;
mod reports;
mod resume;
mod retry;
mod runtime;
mod send;
mod services;
mod sim;
mod terminal;
mod tool_handoff;
mod triage;

// What the rest of the crate reaches through `crate::hub::`.
pub(crate) use crate::project_folder::add_rule;
pub(crate) use send::{conversation_log, handover_log, moved_note};

pub struct Hub {
    pub store: Store,
    pub config: Config,
    stream: broadcast::Sender<StreamMsg>,
    runtimes: Mutex<HashMap<ThreadId, Runtime>>,
    /// Threads whose engine runs under a keeper, which lives through a restart (`keeper.rs`).
    kept: Mutex<std::collections::HashSet<ThreadId>>,
    /// The vault keys each running engine started with (`renew_engines`).
    pub(crate) engine_keys: Mutex<HashMap<ThreadId, std::collections::BTreeMap<String, String>>>,
    /// Threads whose reach (`reach.rs`) changed while their engine was running.
    pub(crate) reach_stale: Mutex<std::collections::HashSet<ThreadId>>,
    permissions: Mutex<HashMap<String, PendingPermission>>,
    /// User messages each engine has taken from its queue, as (thread, seq); a message in here
    /// can no longer be edited or removed. Held while the log is checked, so an edit and the
    /// engine taking the message never cross.
    taken: tokio::sync::Mutex<std::collections::HashSet<(ThreadId, i64)>>,
    update: Mutex<UpdateStatus>,
    pub voice: crate::voice::Voice,
    pub usage: crate::usage::Usage,
    /// The last answer from GitHub for the activity map, with when it came.
    github: tokio::sync::Mutex<Option<(i64, Arc<crate::activity::github::Github>)>>,
    /// Lines added and removed per commit hash, read from `kv` on first use (`Hub::lines`).
    lines_cache: tokio::sync::Mutex<Option<std::collections::HashMap<String, (u32, u32)>>>,
    pub secrets: Arc<crate::secrets::Vault>,
    /// The built version the user said to install, and the signal that they did.
    install: Mutex<Option<String>>,
    install_signal: tokio::sync::Notify,
    retry_signal: tokio::sync::Notify,
    /// "Build now": the version waiting for a quiet branch builds without waiting.
    build_now: std::sync::atomic::AtomicBool,
    build_now_signal: tokio::sync::Notify,
    /// Other machines whose threads show here.
    pub remotes: Arc<crate::remote::Remotes>,
    /// Devices let in with a pairing code.
    pub pairing: crate::pairing::Pairing,
    /// Terminals inside Hub, opened from a machine's page (`pty.rs`).
    pub terminals: crate::pty::Terminals,
    /// The look at this Mac's iPhone simulators (`simview.rs`).
    pub simview: crate::simview::Simview,
    /// Files each thread edited lately, for clash warnings.
    clashes: Mutex<crate::clash::Clashes>,
    /// Events that need a look once logged: edits (clashes), turn ends (leaks, proof) and errors
    /// (plan limits).
    after: mpsc::UnboundedSender<Event>,
    /// What a feature above Hub does with each of those events (`Hub::on_look`; Problems installs its own).
    look_hooks: Mutex<Vec<after_turn::LookHook>>,
    /// Where the slips of what is sent to an AI go (`package/`); none outside an async runtime.
    pub(crate) slips: Option<mpsc::UnboundedSender<crate::model::Slip>>,
    /// Working on Hub from inside Hub.
    pub dev: crate::dev_state::Dev,
    /// Threads stopped by a plan limit, waiting for it to reset.
    pub(crate) waits: crate::resume::Waits,
    /// The pull request of each thread's branch (`triage`).
    pub(crate) prs: crate::triage::Prs,
    /// Set while the core hands over to another one: no engine starts any more.
    parked: std::sync::atomic::AtomicBool,
    /// The panic button: while set, no engine starts and messages wait (`crate::halt`).
    pub(crate) halt: Mutex<Option<crate::model::Halt>>,
    /// Shadow mode of the thread status (`status_shadow.rs`).
    pub(crate) shadow: crate::status_shadow::Shadow,
    /// How long an engine's own turn waits in silence before it counts as over (`engine::unprompted`).
    pub(crate) unprompted_quiet: crate::engine::unprompted::Quiet,
}

struct Runtime {
    generation: u64,
    commands: mpsc::UnboundedSender<Command>,
    /// The task running the engine; aborting it kills the engine without logging an end.
    task: tokio::task::AbortHandle,
    /// Closes when the engine has exited.
    exited: tokio::sync::watch::Receiver<()>,
}

struct PendingPermission {
    thread_id: ThreadId,
    /// When it was asked, for putting a session to sleep that waits too long (`sleep_waiting`).
    asked: std::time::Instant,
    answer: oneshot::Sender<Option<String>>,
}

static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Hub {
    /// The database and settings, for code that needs nothing else of the Hub (`ctx.rs`).
    pub fn ctx(&self) -> crate::ctx::Ctx<'_> {
        crate::ctx::Ctx::new(&self.store, &self.config)
    }

    pub fn new(store: Store, config: Config) -> Arc<Self> {
        let (stream, _) = broadcast::channel(1024);
        let voice = crate::voice::Voice::new(&config.data_dir, config.voice.clone());
        let ship = crate::remote::Ship {
            data: config.data_dir.clone(),
            source: config.update.source_in(&config.data_dir),
            branch: config.update.branch.clone(),
            update: config.update.clone(),
            builder: Default::default(),
        };
        let secrets = Arc::new(crate::secrets::Vault::new(store.clone(), &config.data_dir).in_memory(config.keys_in_memory));
        let remotes = crate::remote::Remotes::new(
            local_host_name(&config),
            config.hosts.clone(),
            store.clone(),
            secrets.clone(),
            stream.clone(),
            ship,
        );
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let remotes = remotes.clone();
            runtime.spawn(async move { remotes.start().await });
        }
        // Every package sent to an AI gets a slip, written in the background.
        let slips = crate::package::ledger::writer(store.clone());
        if let Some(tx) = &slips {
            crate::package::ledger::share(tx.clone());
        }
        let (after, mut logged) = mpsc::unbounded_channel::<Event>();
        let hub = Arc::new(Self {
            clashes: Mutex::default(),
            after,
            look_hooks: Mutex::default(),
            slips,
            voice,
            remotes,
            pairing: Default::default(),
            terminals: Default::default(),
            simview: Default::default(),
            secrets,
            install: Mutex::default(),
            dev: Default::default(),
            waits: Default::default(),
            prs: Default::default(),
            parked: Default::default(),
            halt: Mutex::default(),
            shadow: Default::default(),
            unprompted_quiet: Default::default(),
            install_signal: tokio::sync::Notify::new(),
            retry_signal: tokio::sync::Notify::new(),
            build_now: Default::default(),
            build_now_signal: tokio::sync::Notify::new(),
            usage: Default::default(),
            github: Default::default(),
            lines_cache: Default::default(),
            store,
            config,
            stream,
            runtimes: Mutex::default(),
            kept: Mutex::default(),
            engine_keys: Mutex::default(),
            reach_stale: Mutex::default(),
            permissions: Mutex::default(),
            taken: Default::default(),
            update: Mutex::new(UpdateStatus {
                version: hub_install::running_version(),
                state: UpdateState::Disabled,
                target: None,
                message: None,
                building: None,
                step: None,
                checked_at: None,
                waiting_until: None,
                waiting: None,
                rolled_back: None,
            probation: None,
            }),
        });
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let me = Arc::downgrade(&hub);
            runtime.spawn(async move {
                while let Some(event) = logged.recv().await {
                    let Some(hub) = me.upgrade() else { return };
                    tokio::spawn(async move { hub.look_at(event).await });
                }
            });
            runtime.spawn(hub.clone().watch_waits());
            runtime.spawn(hub.clone().tidy_threads());
            runtime.spawn(hub.clone().watch_prs());
            runtime.spawn(crate::cloud_threads::watch(hub.clone()));
            runtime.spawn(hub.clone().backfill_owners());
        }
        hub
    }

    /// What thread `id`'s tokens went to, from its engine's session log.
    pub async fn context_detail(&self, id: &str) -> Result<crate::usage::ContextDetail> {
        let thread = self.store.thread(id).await?.context("no such thread")?;
        let none = |note: &str| crate::usage::ContextDetail { note: Some(note.into()), ..Default::default() };
        let Some(pattern) = self.config.traits(&thread.engine).session_log else {
            return Ok(none("This engine keeps no log Hub can read, so only the total is known."));
        };
        let Some(session) = thread.engine_session_id.clone() else {
            return Ok(none("The breakdown shows after the first reply."));
        };
        tokio::task::spawn_blocking(move || match crate::usage_detail::find(&pattern, &session) {
            Some(path) => crate::usage_detail::read(&path).map_err(Into::into),
            None => Ok(none("The engine's log for this thread is not on this machine.")),
        })
        .await?
    }

    /// Plan limits and per-thread usage. `refresh` fetches the plan limits again when they are
    /// stale, in the background, from the engine `Config::plan_engine` names; the report says
    /// when that is still going on.
    pub async fn usage_report(self: &Arc<Self>, refresh: bool) -> Result<crate::usage::UsageReport> {
        // Asking the engine takes a while, so the last limits from before a restart show meanwhile.
        if self.usage.plan().is_none()
            && let Some(kept) = self.store.get_value(LAST_PLAN).await?.and_then(|j| serde_json::from_str(&j).ok())
        {
            self.usage.seed(kept);
        }
        let probe = self.config.plan_engine().map(|(_, engine, probe)| (engine.clone(), probe));
        if probe.is_none() {
            self.usage.none("The default engine does not report plan limits");
        }
        if let Some((engine, probe)) = probe.filter(|_| refresh && self.usage.start_refresh()) {
            let hub = self.clone();
            tokio::spawn(async move {
                let cwd = hub.config.data_dir.display().to_string();
                let ask = engine::complete("usage_probe", &engine, &cwd, None, probe);
                let result = match tokio::time::timeout(std::time::Duration::from_secs(90), ask).await {
                    Ok(result) => result,
                    Err(_) => Err(anyhow::anyhow!("the engine took too long to report usage")),
                };
                hub.usage.finish_refresh(result);
                if let Some((plan, _)) = hub.usage.plan() {
                    crate::plan_meter::record(&hub.store, &plan).await;
                    if let Ok(json) = serde_json::to_string(&plan) {
                        hub.store.set_value(LAST_PLAN, &json).await.ok();
                    }
                    hub.publish(StreamMsg::Usage { plan });
                }
            });
        }
        let (plan, plan_error, refreshing) = self.usage.snapshot();
        let threads = self.store.latest_usage().await?;
        let mut others = crate::plan_meter::other_plans().await;
        // Pay-per-token providers have no limits; what is left on the account stands in for them.
        others.extend(crate::balance::all(self).await);
        Ok(crate::usage::UsageReport { plan, plan_error, refreshing, threads, others })
    }

    /// Threads here and on every other host, newest activity first.
    pub async fn all_threads(&self) -> Result<Vec<Thread>> {
        let mut threads = self.store.threads().await?;
        threads.extend(self.remotes.threads());
        threads.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
        Ok(threads)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StreamMsg> {
        self.stream.subscribe()
    }

    /// The folder new projects go in: the one the user picked, or D:\Dev, or Dev in the home folder.
    /// The engine and model for Hub's own small helper questions (`handoff::tidy_target`).
    pub async fn tidy_target(&self, cwd: Option<&str>) -> (String, Option<String>) {
        crate::handoff::tidy_target(&self.ctx(), cwd).await
    }

    pub async fn projects_root(&self) -> Result<String> {
        crate::projects_root::get(&self.store).await
    }

    pub async fn set_projects_root(&self, root: &str) -> Result<()> {
        let root = root.trim().trim_end_matches(['/', '\\']);
        if root.is_empty() {
            bail!("pick a folder");
        }
        self.store.set_value(crate::projects_root::KEY, root).await
    }

    /// The library's folder: the one the user picked, or `library` in the projects root.
    pub async fn library_root(&self) -> Result<std::path::PathBuf> {
        if let Some(root) = self.store.get_value(crate::library::ROOT_KEY).await? {
            return Ok(root.into());
        }
        Ok(std::path::Path::new(&self.projects_root().await?).join("library"))
    }

    /// The library, made if needed, with Hub's facts brought up to date.
    pub async fn library(&self) -> Result<std::path::PathBuf> {
        let root = self.library_root().await?;
        let projects = self
            .store
            .projects()
            .await?
            .into_iter()
            .map(|p| (p.name, std::path::PathBuf::from(p.path)))
            .filter(|(_, path)| path.is_dir())
            .collect();
        let source = crate::library::Source { projects, machines: self.remotes.list() };
        let at = root.clone();
        tokio::task::spawn_blocking(move || crate::library::refresh(&at, &source)).await??;
        Ok(root)
    }

    /// The model for a second opinion from the engine's `review_models`: the one set in the
    /// guard when it is one of them, or one that is not doing the work, cheaper when the plan is
    /// running low. An engine that names none cannot give one, rather than reviewing its own work
    /// with the same model.
    pub(crate) async fn reviewer(&self, engine: &str, events: &[Event]) -> Result<String> {
        let prefs = self.guard_prefs().await.unwrap_or_default();
        let models = self.config.traits(engine).review_models.unwrap_or_default();
        let models: Vec<String> = models.into_iter().map(|m| m.trim().to_lowercase()).filter(|m| !m.is_empty()).collect();
        let Some(strongest) = models.first() else {
            bail!("the {engine} engine names no models for a second opinion; set review_models for it in the config, or ask for a model")
        };
        // The guard's model is one of a single engine's, so it only counts on that engine.
        if let Some(model) = prefs.review_model.map(|m| m.trim().to_owned()).filter(|m| {
            let m = m.to_lowercase();
            !m.is_empty() && models.iter().any(|o| m.contains(o.as_str()) || o.contains(&m))
        }) {
            return Ok(model);
        }
        let low = self.reads_plan_of(engine)
            && self.usage.plan().and_then(|(p, _)| p.highest().map(|l| l.percent >= prefs.warn_at)).unwrap_or(false);
        let current = current_model(events).unwrap_or_default().to_lowercase();
        let on_strongest = current.contains(strongest.as_str()) || current.is_empty() || current == "default";
        let pick = match (on_strongest, low) {
            (true, _) => models.get(1).unwrap_or(strongest),
            (false, true) => models.last().unwrap_or(strongest),
            (false, false) => strongest,
        };
        Ok(pick.clone())
    }

    /// Records a session setting and applies it to the running engine. A stopped engine picks it
    /// up from the log when it next starts.
    pub async fn set_setting(
        self: &Arc<Self>,
        thread_id: &str,
        config_id: String,
        value: serde_json::Value,
    ) -> Result<()> {
        if !(value.is_string() || value.is_boolean()) {
            bail!("a setting is a string or a boolean");
        }
        let thread = self.store.thread(thread_id).await?.context("no such thread")?;
        // Another provider or model of a harness starts the agent anew on it (`routes.rs`).
        let reroute = config_id == crate::routes::ROUTE;
        if reroute && matches!(thread.status, ThreadStatus::Running | ThreadStatus::WaitingOnYou) {
            bail!("Let the turn finish before switching provider.");
        }
        self.emit_typed(thread_id, &crate::model::SettingData::new(config_id.as_str(), value.clone())).await?;
        if reroute {
            let running = self.runtimes.locked().contains_key(thread_id);
            if running {
                self.shutdown(thread_id, false).await?;
                self.warm(thread_id).await?;
            }
            return Ok(());
        }
        let commands = self.runtimes.locked().get(thread_id).map(|r| r.commands.clone());
        if let Some(commands) = commands {
            send_or_log(&commands, Command::SetConfig { config_id, value }, "set config");
        }
        Ok(())
    }

    /// The conversation before message `seq`, with a note, for an agent that lost its session.
    /// None when nothing was said before it.
    pub(crate) async fn lost_session_context(&self, thread_id: &str, seq: i64) -> Result<Option<(String, &'static str)>> {
        let events = crate::replies::conversation(&self.store, thread_id, &[EventKind::TurnEnd]).await?;
        let before: Vec<Event> = events.into_iter().filter(|e| e.seq < seq).collect();
        if !before.iter().any(|e| e.kind == EventKind::TurnEnd) {
            return Ok(None);
        }
        let (log, recipe) = handover_log(thread_id, &before);
        Ok(Some((format!("{}\n\n<conversation>{log}\n</conversation>", lost_note()), recipe)))
    }

    /// Adopts a title the engine suggests, unless the user named the thread.
    pub(crate) async fn suggest_title(&self, thread_id: &str, title: String) -> Result<()> {
        let title = title.trim().to_owned();
        if title.is_empty() {
            return Ok(());
        }
        let thread = self.store.thread(thread_id).await?.ok_or_else(|| crate::store::ThreadGone(thread_id.to_owned()))?;
        if !thread.title_locked && thread.title != title {
            self.update_thread(thread_id, move |t| t.title = title).await?;
        }
        Ok(())
    }

    /// Threads whose engine is running.
    pub fn engines(&self) -> Vec<ThreadId> {
        self.runtimes.locked().iter().filter(|(_, r)| !r.commands.is_closed()).map(|(id, _)| id.clone()).collect()
    }

    pub(crate) async fn ask_permission(
        &self,
        thread_id: &str,
        data: serde_json::Value,
    ) -> Result<oneshot::Receiver<Option<String>>> {
        let (tx, rx) = oneshot::channel();
        if let Some(verdict) = self.auto_permission(thread_id, &data).await {
            let option_id = crate::permit::option_for(&verdict, &data["options"]);
            let (allowed, why) = match verdict {
                crate::permit::Verdict::Allow(why) => (true, why),
                crate::permit::Verdict::Refuse(why) => (false, why),
            };
            let auto = crate::model::PermissionAutoData { tool_call: data["tool_call"].clone(), option_id: Some(option_id.clone()), allowed, why };
            self.emit_typed(thread_id, &auto).await?;
            tx.send(option_id).ignore("rx is held here and returned just below");
            return Ok(rx);
        }
        let id = uuid::Uuid::now_v7().to_string();
        self.permissions
            .locked()
            .insert(id.clone(), PendingPermission { thread_id: thread_id.to_owned(), asked: std::time::Instant::now(), answer: tx });
        let typed = crate::model::PermissionRequestData { id: id.clone(), tool_call: data["tool_call"].clone(), options: data["options"].clone() };
        self.emit_typed(thread_id, &typed).await?;
        if let Err(err) = self.set_status(thread_id, ThreadStatus::WaitingOnYou).await {
            // Removed between the question and here: it has nowhere to go, so the agent's
            // request ends as cancelled (the dropped answer) instead of failing in the engine.
            if self.store.thread(thread_id).await?.is_some() {
                return Err(err);
            }
            self.permissions.locked().remove(&id);
        }
        Ok(rx)
    }

    pub(crate) async fn emit(
        &self,
        thread_id: &str,
        kind: EventKind,
        data: serde_json::Value,
    ) -> Result<Event> {
        let mut data = data;
        self.secrets.redact(&mut data);
        let event = self.store.append_event(thread_id, kind, data).await?;
        self.publish(StreamMsg::Event { event: event.clone() });
        self.observe(&event);
        Ok(event)
    }

    /// [`Hub::emit`] for a kind that has a payload struct (`model/event_data.rs`).
    pub(crate) async fn emit_typed<T: crate::model::EventData>(&self, thread_id: &str, data: &T) -> Result<Event> {
        self.emit(thread_id, T::KIND, data.to_json()).await
    }

    /// Runs the observers (`hub/observers.rs`) over a logged event.
    fn observe(&self, event: &Event) {
        for observer in observers::ALL {
            observer(self, event);
        }
    }

    /// Puts an event on the queue `look_at` works through.
    pub(crate) fn queue_after(&self, event: Event) {
        self.after.send(event).ignore("look_at's loop ends only once the hub is gone");
    }

    /// [`Hub::emit`], once per `client_id`; `None` when that request was logged before.
    pub(crate) async fn emit_once(
        &self,
        thread_id: &str,
        kind: EventKind,
        data: serde_json::Value,
        client_id: &str,
    ) -> Result<Option<Event>> {
        let mut data = data;
        self.secrets.redact(&mut data);
        let Some(event) = self.store.append_event_once(thread_id, kind, data, client_id).await? else {
            return Ok(None);
        };
        self.publish(StreamMsg::Event { event: event.clone() });
        self.observe(&event);
        Ok(Some(event))
    }

    pub(crate) async fn update_thread(
        &self,
        thread_id: &str,
        f: impl FnOnce(&mut Thread) + Send + 'static,
    ) -> Result<Thread> {
        let thread = self.store.update_thread(thread_id, f).await?;
        self.publish(StreamMsg::Thread { thread: thread.clone() });
        Ok(thread)
    }

    pub(crate) async fn set_status(&self, thread_id: &str, status: ThreadStatus) -> Result<Thread> {
        let now = now_ms();
        let before = self.store.thread(thread_id).await?.map(|t| t.status);
        let thread = self.update_thread(thread_id, move |t| {
            let needs_you = matches!(status, ThreadStatus::WaitingOnYou | ThreadStatus::Error);
            let finished = t.status == ThreadStatus::Running && status == ThreadStatus::Idle;
            if t.status != status && (needs_you || finished) {
                t.returned_at = Some(now);
            }
            // Something done that needs you again is back in play, snoozed or not.
            if needs_you {
                t.settled_at = None;
                t.snoozed_until = None;
            }
            t.status = status;
        })
        .await?;
        if let Some(before) = before {
            self.note_desk(before, &thread).await;
            crate::ntfy::note(&self.ctx(), before, &thread).await;
        }
        Ok(thread)
    }

    /// [`Hub::set_status`] for a change no engine event carries (a terminal taken or handed back,
    /// a cloud session, a queued message taken back, an engine stopped, the start-up repair):
    /// a `Status` marker goes in the log first, so the log says it too (D3 step 4). Nothing is
    /// logged when the status is the one it has.
    pub(crate) async fn set_status_because(&self, thread_id: &str, status: ThreadStatus, why: &str) -> Result<Thread> {
        if self.store.thread(thread_id).await?.is_some_and(|t| t.status != status) {
            self.emit_typed(thread_id, &crate::model::StatusData::new(status, why)).await?;
        }
        self.set_status(thread_id, status).await
    }

    /// How many surfaces (and listeners in the core) follow the stream.
    pub(crate) fn subscribers(&self) -> usize {
        self.stream.receiver_count()
    }

    pub(crate) fn publish(&self, msg: StreamMsg) {
        // No subscribers is fine; surfaces catch up from the store when they connect.
        self.stream.send(msg).ignore("no window may be listening");
    }

    /// Notes whether a thread's engine runs under a keeper.
    pub(crate) fn note_kept(&self, thread_id: &str, kept: bool) {
        let mut set = self.kept.locked();
        if kept {
            set.insert(thread_id.to_owned());
        } else {
            set.remove(thread_id);
        }
    }
}

/// Key in the store for the quota guard's settings.
const GUARD: &str = "guard";
/// The user's permission settings (`PermissionPrefs`).
const PERMISSIONS: &str = "permissions";

/// Lines added and removed per commit hash, as JSON (`Hub::lines`).
const LINES_CACHE: &str = "activity.lines";

/// Key in the store for the last plan limits Claude reported, shown at once after a restart.
const LAST_PLAN: &str = "usage.last_plan";

fn lost_note() -> String {
    "(Note from Hub: this thread carries on in a fresh session, because the earlier one could not \
be loaded, for example after switching to another provider or model. Below is the conversation so \
far, for context only; the work it describes is in the folder, and where the two differ the folder \
is right. Answer the message after it.)"
        .into()
}

/// The model the thread works with: the engine's current choice, else the user's last pick.
pub(crate) fn current_model(events: &[Event]) -> Option<String> {
    for e in events.iter().rev() {
        match e.kind {
            EventKind::SessionState => return e.data_as::<crate::model::SessionStateData>()?.model().map(str::to_owned),
            EventKind::Setting => {
                if let Some(chose) = e.data_as::<crate::model::SettingData>().filter(|s| s.config_id == "model") {
                    return chose.model().map(str::to_owned);
                }
            }
            _ => {}
        }
    }
    None
}

/// The first `max` characters of `text`, saying when the rest was left out.
use crate::text::clip_counted as clip;

pub(crate) fn title_from_message(message: &str) -> String {
    let line = message.lines().next().unwrap_or_default().trim();
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    title
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn folder_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "New thread".into())
}

/// What this machine is called on host chips: the config's name, "PC" on Windows, or the hostname.
fn local_host_name(config: &Config) -> String {
    if let Some(name) = config.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        return name.to_owned();
    }
    if cfg!(windows) {
        return "PC".into();
    }
    sysinfo::System::host_name().filter(|n| !n.is_empty()).unwrap_or_else(|| "this machine".into())
}
