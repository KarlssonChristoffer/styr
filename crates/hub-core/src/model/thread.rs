//! Model types: thread.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ThreadStatus {
    /// No turn in progress. The engine process may or may not be alive.
    Idle,
    /// The engine is working on a turn.
    Running,
    /// The engine asked for permission and is blocked on the user.
    WaitingOnYou,
    /// The engine process failed. The next message restarts it.
    Error,
    /// Taken over in a terminal (`hubd attach`): the engine's own CLI has the session, Hub's
    /// engine is stopped and messages from Hub are refused until it is handed back.
    InTerminal,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Thread {
    pub id: ThreadId,
    pub title: String,
    /// Working folder on the host. Projects are derived from this.
    pub cwd: String,
    /// Key into the configured engines, e.g. "claude".
    pub engine: String,
    pub status: ThreadStatus,
    /// The engine's own session id, used to resume after a restart.
    pub engine_session_id: Option<String>,
    /// Set once the user names the thread, so automatic titles stop replacing it.
    pub title_locked: bool,
    /// Archived threads are kept but hidden from the main lists.
    pub archived: bool,
    /// The machine the thread runs on, when it is not the one serving the API. Set by the core
    /// on threads it relays from other hosts; never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub host: Option<String>,
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
    /// When the thread last handed the ball back: a turn ended, failed or asked something.
    /// Unix milliseconds. The activity list sorts by it.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub returned_at: Option<i64>,
    /// When the user last opened the thread. A return after it is unread.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub seen_at: Option<i64>,
    /// Set when the user marks the thread done. It leaves the activity list for the settled
    /// shelf, and comes back when the user writes to it or it needs an answer or fails.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub settled_at: Option<i64>,
    /// When the user last sent the thread work. Lists of working threads sort by it, so a
    /// thread's own activity never moves it.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub sent_at: Option<i64>,
    /// Pinned threads stay on top of the activity list, highest first. Dragging one between two
    /// others sets it between theirs.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub pinned_at: Option<i64>,
    /// Hidden from the activity list until then, unless it needs the user or fails first.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub snoozed_until: Option<i64>,
    /// Hub may settle the thread by itself when it sits idle or its pull request merges.
    #[serde(default = "yes")]
    pub auto_settle: bool,
    /// The thread that started this one as a side job (`children.rs`). Its result goes back
    /// there, and lists show it under that thread instead of on its own.
    #[serde(default)]
    #[ts(type = "string | null")]
    pub parent: Option<ThreadId>,
    /// Where the last turn left the work, as a decision model read its reply (`reading.rs`):
    /// "done", "asks" (waits on your answer), "stuck" or "partway". Cleared when you send the
    /// thread work again. None when nothing read it, or the reading was unsure.
    #[serde(default)]
    #[ts(type = "string | null")]
    pub reading: Option<String>,
    /// Who in Hub started the thread on its own, as an agent and not the user (`system.rs`):
    /// Vakten, a routine, the merge queue, another thread. None for a thread the user started.
    #[serde(default)]
    pub owner: Option<Owner>,
    /// Set when the thread runs on Anthropic's or OpenAI's servers instead of a machine
    /// (`cloud_threads.rs`). None for a thread on a machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cloud: Option<CloudRun>,
    /// The folder of the project the thread belongs to, worked out by the core from `cwd` once
    /// (`thread_display.rs`): a thread's own worktree counts as its repository. Never stored;
    /// None from a hubd that does not send it, and clients then work it out from `cwd` as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<String>,
    /// The word for the thread's state, worked out by the core once (`thread_display.rs`):
    /// "waiting", "error", "asks", "running", "unread" or "idle". Never stored. A client reading
    /// a word it does not know shows the thread as idle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display: Option<String>,
}

/// A thread that runs in the cloud, as Hub last read it (`cloud_threads.rs`). The conversation
/// itself lives on the service's own page; Hub keeps where it stands and where the work lands.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CloudRun {
    /// "claude" (a Claude Code cloud session) or "codex" (a Codex cloud task).
    pub provider: String,
    /// The service's id for the session; empty until it has one.
    #[serde(default)]
    pub session_id: String,
    /// The session's page on claude.ai or chatgpt.com.
    #[serde(default)]
    #[ts(optional)]
    pub url: Option<String>,
    /// `owner/name` on GitHub.
    #[serde(default)]
    #[ts(optional)]
    pub repo: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub branch: Option<String>,
    /// "starting", "working", "waiting", "done", "failed" or "stopped". A client reading one it
    /// does not know shows it as working.
    pub state: String,
    /// The last thing the session said it was doing, when the service lets Hub read it.
    #[serde(default)]
    #[ts(optional)]
    pub last: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub pr: Option<CloudPr>,
    /// Why it could not start or be read, in words for the thread.
    #[serde(default)]
    #[ts(optional)]
    pub note: Option<String>,
    /// The last session event Hub has shown in the thread, so the next look only adds what is new.
    #[serde(default)]
    #[ts(optional)]
    pub seen: Option<String>,
    /// A message was told to the session and its answer has not been read yet; until it is, an
    /// idle session is not taken for a finished one.
    #[serde(default)]
    pub awaiting: bool,
}

/// The pull request a cloud thread ended in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CloudPr {
    #[ts(type = "number")]
    pub number: i64,
    #[serde(default)]
    #[ts(optional)]
    pub title: Option<String>,
    pub url: String,
}

pub use crate::thread_owner::{Owner, OwnerKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EventKind {
    /// `{ text }` sent by the user.
    UserMessage,
    /// `UpdateData`: a raw ACP `SessionUpdate` (agent_message_chunk, tool_call, plan, ...), kept as it came;
    /// `update_kind` says which.
    Update,
    /// `{ id, tool_call, options }` waiting for an answer.
    PermissionRequest,
    /// A permission question was answered or closed (`PermissionResolvedData`): `{ id, option_id }`,
    /// `option_id` null when cancelled, plus one of `stale`, `woke`, `restart`, `moved` when Hub closed it.
    PermissionResolved,
    /// `{ ids }` (`SleptData`): the engine was stopped while these permission questions waited for the
    /// user, so it takes no memory meanwhile. The answer wakes it (`Hub::sleep_waiting`).
    Slept,
    /// `{ stop_reason, prompt? }` when a turn ends; `prompt` is the seq of the user message the
    /// turn answered (missing in logs from before it was added).
    TurnEnd,
    /// `{ message, prompt?, detail? }` when the engine fails; `prompt` when it failed a turn,
    /// `detail` with what the engine added to its message (the error's `data`). `leaves` (optional,
    /// "idle" or "error") says what state the thread is left in: a turn that failed but the engine
    /// lives is "idle", an engine that died is "error". Missing in older logs: with a `prompt` it
    /// means "idle", without one "error", except for `ENGINE_GONE` (see `docs/ARCHITECTURE.md`,
    /// "Thread status from events").
    Error,
    /// `SessionStateData`: what can be set on the session (model, effort, permission mode, ...)
    /// and what a prompt may contain. Surfaces keep the latest of each field.
    SessionState,
    /// `SettingData` (`{ config_id, value, by? }`): a setting was chosen, by the user unless `by`
    /// says who. It is applied to the running session, or when the engine next starts.
    Setting,
    /// `RuleSavedData` (`{ text, path, seq }`): a rule from the user's message `seq` was added to `path`, the
    /// rules file of the thread's engine in its folder.
    RuleSaved,
    /// `CheckpointData` (`{ seq, commit, root, head, branch }`): the repository as it was before user message
    /// `seq` reached the engine, kept under `refs/hub/<thread>/<seq>`.
    Checkpoint,
    /// `RestoredData` (`{ to, seq, root, files, commits, kept_commits, safety, safety_head, safety_branch }`): the
    /// repository was rolled back to snapshot `to`, the checkpoint before message `seq` or an
    /// earlier rollback's `safety` (then `seq` is null). `safety` is the repository just before,
    /// with where its branch pointed, so the rollback can be undone.
    Restored,
    /// `ReviewData` (`{ id, state, model, files, base, head, seq, by, reason, chunks, text?, sent?, error? }`):
    /// a second opinion by another model on what changed from snapshot `base` to `head` (kept, so
    /// the next review starts there). `by` is "you" or "agent", with the agent's `reason`.
    /// `chunks` are the parts it was cut into, `{ title, files, review, why, state, verdict?,
    /// text?, error? }`. Logged again under the same id as it progresses, then as "done" with the
    /// whole review in `text`, or "failed". `sent` says the findings went back to the agent as a
    /// user message marked `review: id`.
    Review,
    /// `QuotaSwitchData`: the quota guard moved the thread to a cheaper model because a plan
    /// limit was nearly used. A `setting` event with the model comes first.
    QuotaSwitch,
    /// `SessionStartedData`: an engine started or resumed the thread's session, so the `update`s
    /// after it can be read knowing which protocol and agent wrote them.
    SessionStarted,
    /// `ClashData` (`{ path, other, other_title }`): another thread changed `path` (absolute) too, within the
    /// last few hours. Logged in both threads, once per file and pair.
    Clash,
    /// `LeakData` (`{ seq, findings, sent }`) after a turn: lines it added hold what looks like a secret.
    /// `findings` are `{ path, line, kind, hint }` with only a hint of the value. `sent` says they
    /// went to the agent as a user message marked `leak: seq`.
    Leak,
    /// `ProofData` (`{ reply, files, kind, command, hidden?, state, why? }`) after a turn that changed `files` files (docs
    /// aside): `reply` is that turn's `turn_end` seq. `state` is "shown" (a test run or screenshot
    /// that passed, `kind` "tests" or "screenshot"), "failed" (the last test run failed), "asked"
    /// (Hub asked the agent for proof with a user message marked `proof: reply`), "missing" (it
    /// was asked and still showed none) or "skipped" (nothing new to show; `why` says what).
    Proof,
    /// `DocsData` (`{ reply, state, why?, sections }`) after a turn that changed code a doc section covers
    /// (`covers:`, see `covers.rs`). `sections` are `{ doc, heading, first, last, files }`.
    /// `state` is "updated" (the turn changed the doc), "checked" (it opened the doc or asked
    /// `docs_for`), "asked" (it did neither, and Hub asked it to check them in a user message
    /// marked `docs: reply` or, with a proof request, `proof: reply`), "skipped" (`why`
    /// "reverted": it put earlier work back), and after an "asked" the answer's "updated",
    /// "checked" or "ignored".
    Docs,
    /// `AttemptData`: `{ group, n, of, threads, base, root, cwd, worktree, branch }` first in each thread of a
    /// group of attempts at the same task, each in its own worktree. `threads` are the group's
    /// thread ids in order. Later `{ group, state: "picked" | "dropped", by?, files?, ... }` when the
    /// person picks one: the picked work was applied to `cwd` and the others' worktrees removed.
    Attempt,
    /// `TerminalData` (`{ state, pid?, command?, cut?, turns?, replay_turns?, carry_on? }`): the thread went to a
    /// terminal (`state: "taken"`, with the `hubd attach` process and the command it runs, and
    /// `cut`, the user message whose turn taking over stopped, if one was running), came back
    /// (`"back"`), and what was said there was copied into the log (`"imported"`: `turns` new
    /// prompts, `replay_turns` prompts in the session as the engine replayed it, and `carry_on`
    /// true when Hub then carries the stopped turn on with a message marked `terminal: <seq of
    /// the taken event>`). The imported prompts and replies are ordinary `update` events between
    /// `back` and `imported`.
    Terminal,
    /// `QuotaWaitData`: a turn stopped on a plan limit and Hub carries the thread on when it
    /// resets. `state` is "waiting", then "resumed", "switched", "cancelled" or "gave_up".
    QuotaWait,
    /// `{ prompt }` as user message `prompt` goes to the engine. A turn start without a
    /// `turn_end` or `error` after it is a turn that was cut off.
    TurnStart,
    /// `{ by, prompt, queued, state, why? }` (`RestartData`) when the core starts and finds the turn for
    /// user message `prompt` cut off: `by` is "update", "restart" (new settings), "crash", "pause" or
    /// "move" (the thread arrived from another machine mid-turn). `queued` are the messages sent
    /// meanwhile, which go to the engine after it. `state` is "continued" (Hub sent a message marked
    /// `restart: prompt` asking the agent to carry on) or "stopped" (it did not: `why` is "off" when
    /// `resume_turns` is off, "loop" after several restarts in a row cut it off).
    Restart,
    /// `{ by }` (`PausedData`): the panic button stopped this thread's turn (`crate::halt`), pressed on
    /// machine `by`. The turn is carried on as a `restart` with `by: "pause"` when agents are resumed.
    Paused,
    /// `EngineSwitchData` (`{ from, to }`): the thread moved to another engine. The next message
    /// carries the conversation so far to it, marked `handoff: seq` with this event's seq.
    EngineSwitch,
    /// `PageData` (`{ id, title, height, bytes, replaces? }`): the agent showed a live HTML page in the thread
    /// (`show_page`). The page is kept as a file (`pages.rs`) and served at
    /// `/api/threads/<thread>/pages/<id>`; `height` is the agent's guess in CSS pixels.
    Page,
    /// `ChildData` (`{ id, title, kind, context, engine, model, state, by, reply?, images?, error? }`) in the parent
    /// of a side job: thread `id` was started to do one thing (`kind`: "image", "research",
    /// "review", "subtask", "quick" or "other") with `context` ("brief", "handover" or "full")
    /// on `engine`. Logged as "working", then again under the same id as "done" with the child's
    /// `reply` and the `images` it made (paths), or "failed" with `error`. The result goes to the
    /// parent's agent as a user message marked `child: id`.
    Child,
    /// `{ tool_call, option_id, allowed, why }` (`PermissionAutoData`): Hub answered a permission
    /// question itself, by the user's permission settings (`permit.rs`), so it never reached them. `why`
    /// says which setting, in words the user sees. `option_id` is null when there was no option to pick.
    PermissionAuto,
    /// `CustomsData`: `{ seq, found, sent }` after a turn that brought in something that has not cleared
    /// Customs: a new dependency in a manifest or a library linked from a CDN. `found` are
    /// `{ kind, name, path, status }`, status as it stood on Customs' list (anything unknown was
    /// put there as held). `sent` says they went to the agent as a user message marked
    /// `customs: seq`.
    /// Also `{ declared: true, found }` when the agent declared something mid-turn with
    /// `customs_declare` and it is held: `found` items then have an empty `path` and a `why`.
    Customs,
    /// `AutoData`. Auto, which picked who answered each message. Taken out on 2026-11-01 (pictures still go
    /// to a side job, `children.rs`); old threads keep these lines. `{ on, by }` turned it on or off:
    /// `by` is "start" (the thread was started on Auto), "you" (picked in the model picker) or
    /// "pick" (you picked a model yourself, which locks it). A pick is `{ ask, engine, why, from?,
    /// to?, from_name?, to_name? }`: what kind of ask it took the message for ("image", "quick" or
    /// "work") and the model it moved to, `to`, from `from`. A `setting` event marked `by: "auto"`
    /// comes first. At the start of a thread a pick names only the `engine` it went to.
    Auto,
    /// `{ seq, action, text? }` (`QueueData`): user message `seq` waited in the queue behind a running
    /// turn and was changed before the engine took it. `action` is "edited" (with the new `text`),
    /// "removed" (it never goes to the engine), "dropped" (the engine kept failing, so it was let go; it
    /// shows as sent but unanswered) or "steered" (the running turn was stopped so it goes next). The
    /// message's own event stays as it was.
    Queue,
    /// `HandedOffData` (`{ tool, title, ok, output, folder?, work? }`): the thread's work was handed to one of your own tools
    /// (Knegarn) with the brief the user approved; `output` is what the tool said. Also `folder`
    /// (the tool's) and `work` (`hub:thread:<id>`), to follow it.
    HandedOff,
    /// `HandoffResultData` (`{ tool, title, state, proven, model, cost, said, link }`): the work handed off ended, as
    /// the tool's `follow` command tells it. `state` is "done" or "stopped"; `proven` is whether
    /// its verification passed (null: not known); `cost` is dollars it reported, `model` the one
    /// that wrote the change, `link` a pull request or branch.
    HandoffResult,
    /// `HandoffOfferData` (`{ tool, folder, label, about, can_build, why }`): the agent offered to hand the thread's work
    /// to one of your own tools (`offer_handoff`), because the user asked for it in words ("run
    /// Knegarn on this"). Nothing is handed over until the user presses the card, which is the
    /// approval: Hub drafts the brief, answers what the tool asks and hands it over
    /// (`Hub::hand_off_now`). The outcome is the `handed_off` that follows.
    HandoffOffer,
    /// `FocusData` (`{ fits: false, stage, line, why }`): a decision model or a small model read the thread's first message and
    /// found it does not belong to `stage`, the project's stage now in `docs/FOCUS.md` (its
    /// heading on `line`). Only logged when it does not fit (`focus.rs`).
    Focus,
    /// `OutputData` (`{ tool_call_id, data }`): a piece of what a command prints while it runs, read by Hub's
    /// shell wrapper (`live_output.rs`) for engines that only report the output when the command
    /// ends. The engine's own output, when it comes, takes its place.
    Output,
    /// `RewindData` (`{ from, files, by }`): the person went back to user message `from` and sent an edited
    /// message in its place ("Edit from here"). Everything from `from` up to this event is left
    /// out of the conversation from now on, but stays in the log. The old session is left behind;
    /// the next message carries the conversation before `from` to a fresh one, marked
    /// `rewind: seq` with this event's seq. `files` says the repository was rolled back to the
    /// checkpoint before `from` too (a `restored` event comes first).
    Rewind,
    /// `ReadingData` (`{ feature, connection, confidence, ms, acted, ... }`): an API connection read something for one
    /// of Hub's own decisions (`reading.rs`). `feature` "turn": `{ seq, state }`, where the turn
    /// ending at `seq` left the work. `feature` "focus": `{ stage, line, text }`, the stage's
    /// "Delivered when" line or idea the thread works towards (`line` is `null` for an idea).
    /// `feature` "topic": `{ seq, state: "new" }`, the message at `seq` starts another task.
    Reading,
    /// `DeskNoteData` (`{ thread, title, state, text, permission }`) in a project's desk (`desk.rs`): another
    /// thread of the project finished a turn (`state` "done", `text` the end of its reply), needs
    /// the user ("asks", the question; `permission` when it asks to use a tool) or failed
    /// ("failed", the error). A note for the user; it does not
    /// start a turn.
    DeskNote,
    /// `SignInLostData`: the turn failed because the engine's sign-in on a machine is gone. The
    /// thread offers a new sign-in there.
    SignInLost,
    /// `PluginSuggestedData` (`{ repo, why }`): the agent found a GitHub repository that could help and suggested it as a
    /// plugin (`suggest_plugin`); `repo` is a `PluginRepo` as it was then, `why` the agent's reason.
    PluginSuggested,
    /// `RepliesData` (`{ question, options: [{ label, message }] }`): the agent offered one-click replies to what
    /// it asked at the end of its turn (`offer_replies`). A click sends `message`, or the label
    /// when there is none. Only the offer of the latest turn counts, until the user says something.
    Replies,
    /// `ReportedData` (`{ about, problem, state, fix?, fix_title?, text?, pr? }`): the thread's failure, the `error`
    /// event `about`, went to Problems as problem `problem` (`problems/fixnow.rs`). Logged again
    /// under the same `about` as it moves: `state` "sent", then "fixing" when the user pressed Fix
    /// now (`fix` is the fix thread), then "fixed" with the end of the fix thread's reply as `text`
    /// and its pull request `{ number, url, state }` as `pr` when it has one, "stopped" or
    /// "failed". A note for the user; it does not start a turn.
    Reported,
    /// `MovedData` (`{ from, folder, via, files, dirty, stopped }`) first in the part of a thread's log that
    /// continues on this machine: the thread was moved here from machine `from` (`moving.rs`).
    /// `folder` is the project here, `via` how the code came ("bundle", "clone", "existing" or "files"),
    /// `dirty` how many uncommitted files came along as a patch, `stopped` whether a running turn
    /// was cut off to move (it carries on here, as after a restart). The engine's own session
    /// stays behind, so the next message hands the conversation to a fresh one, as after an
    /// engine switch.
    Moved,
    /// `MovedAwayData` (`{ to, folder }`) last in the log of the copy a thread left behind on the machine it was
    /// moved from. That copy is archived and no longer listed; the thread goes on under the same
    /// id on machine `to`.
    MovedAway,
    /// `WorkerRunData` (the `WorkerRun`, `{ tool, folder, run, title, state, paused, said, steps, round,
    /// agents, link, cost, controls, cursor }`, and `lines`): where a run of one of your Workers stands (`worker_runs.rs`), written
    /// whenever it moved. `lines` are only the run's events since the previous `worker_run` in
    /// this thread (`cursor` is the last one's seq), so the thread holds each line once and the
    /// newest `worker_run` holds the state. `state` is "running", "done" or "stopped".
    WorkerRun,
    /// `DesignData`. Design mode (`design.rs`): the agent plans a feature in a design document under
    /// `docs/design/` and changes no code. Three shapes:
    /// `{ on, by }` turned it on or off; `by` is "you" or "start" (the thread was started in it).
    /// `{ review: id, state, doc, checks, verdict?, ready?, sent?, by, error? }`: fresh sessions
    /// that never saw the conversation read the design document (`doc`, relative to the folder).
    /// Logged as "running" and again under the same id as each check ends and as "done" or
    /// "failed". `checks` are `{ name, title, model, state, verdict?, text?, error? }` with `name`
    /// "explain", "gaps", "fit" or "ready". `ready` is the last check's go; `sent` says the findings
    /// went to the agent as a user message marked `design: id`. `by` is "you" or "agent".
    /// `{ strayed: [paths], seq }`: a turn in design mode changed files outside `docs/design/`
    /// (an engine that edits without asking never comes past Hub's permission answers).
    Design,
    /// `Unattended` (`{ by, prefer }`) first in a thread that runs without the user
    /// (`NewThread::unattended`). Its permission questions never reach the user; Hub answers them
    /// (`permit::unattended`). `prefer` maps a setting category to words: when the engine first
    /// starts, the first offered choice matching a word is set (a `setting` event marked
    /// `by: "prefer"`), unless the thread chose that setting itself.
    Unattended,
    /// `GistData` (`{ upto, points, ask, by? }`): a TL;DR of the thread for its hover card in the lists
    /// (`gist.rs`). Since the agent writes it itself (`write_tldr`) `by` is "agent"; the older ones
    /// have no `by`: the small model tidying uses wrote those when the card opened.
    /// It covers the log up to seq `upto`; `points` are one to three short sentences and `ask` is
    /// what the thread needs from the person, or null. A note for the card; it does not start a turn.
    Gist,
    /// `ReactionData` (`{ seq, emoji, by, on }`): a reaction to the message at `seq`, a `user_message` or the
    /// `turn_end` that closes an agent's reply. `emoji` is a Unicode emoji or `:name:` of one of
    /// the user's own (`reactions.rs`). `by` is "you" or "agent"; `on` is false when it was taken
    /// away again. The latest event for one `seq`, `emoji` and `by` is the state. A note on the
    /// message; it never starts a turn and does not count as the thread having changed.
    Reaction,
    /// `TaskData` (`{ id, task, name, kind, engine, model, state, by, verdict?, reason?, ms? }`): Hub itself ran one
    /// task for the thread on the engine the user picked for that kind of task (`handoff.rs`),
    /// say Customs reviewing something the agent declared. `task` is a `handoff::TASKS` id, `id`
    /// pairs the lines (the same `id` logged as "working", then "done" with `verdict`
    /// ("cleared", "held" or "refused") and `reason`, or "failed" with `reason`), `ms` is how long it
    /// took. A note in the chat; it never starts a turn.
    Task,
    /// `AskData` (`{ question, options: [{ label, message }], id, assumed }`): the agent asked the person
    /// something without waiting (`ask_user`). It carried on with `assumed`; the question stays in
    /// the chat until answered, and an answer arrives as a normal message. It never makes the
    /// thread wait on the person.
    Ask,
    /// `{ status, why }` (`StatusData`): the thread's status changed for a reason no engine event carries
    /// (the thread went to a terminal or came back, a cloud session changed, a queued message was taken
    /// back, the engine was stopped, the start-up repair). `status` is a `ThreadStatus` word and `why`
    /// says what happened in a word. A marker for the log: surfaces skip it, and a client that does not
    /// know the kind reads it as `unknown`.
    Status,
    /// A kind written by a newer version of Hub. Kept in the log as it is; surfaces skip it.
    #[serde(other)]
    Unknown,
}

/// What the hover card on a thread in the lists shows (`gist.rs`, `GET /api/threads/{id}/peek`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ThreadPeek {
    /// The latest TL;DR, one to three short points. Empty until one is written.
    pub points: Vec<String>,
    /// What the thread needs from the person, as the TL;DR put it.
    pub ask: Option<String>,
    /// The last seq the TL;DR covered.
    #[ts(type = "number")]
    pub upto: i64,
    /// When the TL;DR was written (ms), None until there is one.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub written_at: Option<i64>,
    /// Who wrote it: "agent" (`write_tldr`), or None for one Hub's model wrote in older versions.
    #[serde(default)]
    pub by: Option<String>,
    /// Messages and turns that came after the TL;DR, 0 while it is current.
    #[serde(default)]
    #[ts(type = "number")]
    pub since: i64,
    /// Something happened since the TL;DR (or there is none yet): `POST /api/threads/{id}/gist`
    /// writes a new one.
    pub stale: bool,
    /// The end of the agent's reply in the last turn that ended.
    pub last_reply: String,
    /// One-click replies the agent offered at the end of its turn, until the person writes.
    pub replies: Option<PeekReplies>,
    /// The tool the agent waits for permission to use, while it waits.
    pub permission: Option<String>,
    /// Why the thread failed, while it is failed.
    pub error: Option<String>,
    /// Files the last proven turn changed.
    #[ts(type = "number | null")]
    pub files: Option<i64>,
    /// "passed" or "failed" when the latest proof was a test run.
    pub tests: Option<String>,
    /// Messages the person sent.
    #[ts(type = "number")]
    pub turns: i64,
}

/// Quick replies on the hover card: the agent's question and its offered answers.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PeekReplies {
    pub question: String,
    pub options: Vec<PeekReply>,
}

/// One offered answer: `label` on the button, `message` what a click sends.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PeekReply {
    pub label: String,
    pub message: String,
}

/// One entry in the append-only event log.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Event {
    pub thread_id: ThreadId,
    /// Monotonic per thread, starting at 1.
    #[ts(type = "number")]
    pub seq: i64,
    #[ts(type = "number")]
    pub ts: i64,
    pub kind: EventKind,
    pub data: serde_json::Value,
}

/// What the core pushes to every connected surface over `/api/stream`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum StreamMsg {
    Thread { thread: Thread },
    ThreadDeleted { id: ThreadId },
    Event { event: Event },
    Update { status: UpdateStatus },
    /// The core is going back to the version before it, which failed its probation. Sent just
    /// before it hands over, so a window can follow `current` at once.
    RolledBack { info: RolledBackInfo },
    Project { project: Project },
    ProjectRemoved { path: String },
    /// Fresh plan limits, after any engine run fetched them.
    Usage { plan: PlanUsage },
    /// A machine came online, went offline or changed.
    Host { host: HostInfo },
    HostRemoved { name: String },
    /// A prompt link was opened (`promptlink.rs`): put this in a new message box.
    Draft { draft: PromptLink },
    /// Every agent was paused (`Some`) or resumed (`None`).
    Halt { halt: Option<Halt> },
    /// An idea was captured, placed or moved (`ideas.rs`).
    Idea { idea: InboxIdea },
    IdeaRemoved { id: String },
    /// A problem was seen, or changed (`problems/`).
    Problem { problem: problems::Problem },
    /// Vakten tells the user something it did (`problems/vakten.rs`).
    ProblemNews {
        #[ts(type = "number | null")]
        id: Option<i64>,
        text: String,
        /// Only a critical line is also a desktop notification; the rest only shows on the Problems page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        critical: Option<bool>,
    },
    /// A note from Hub's judge about what you wrote to a thread while it worked (`judge.rs`).
    /// Not part of the thread's log: windows show it as one quiet line that the next replaces.
    Aside {
        thread_id: ThreadId,
        aside: Aside,
    },
    /// Something outside Hub (a widget, a script, a server) sent a notice (`notice.rs`).
    Notice { notice: Notice },
    /// The client may have missed messages and should refetch.
    Resync,
}

/// A notice that came in through `POST /api/notices` (`notice.rs`). Kept in memory only, the last
/// few hundred; it is not part of any thread.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Notice {
    pub id: String,
    #[ts(type = "number")]
    pub at: i64,
    /// Who sent it: "obs-widget", "backup-server". Free text, chosen by the sender.
    pub source: String,
    /// "info", "waiting", "failed" or "done". A client reading another word treats it as "info".
    pub sort: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// What a tap opens: `hub://…` or an `https://` address.
    #[serde(default)]
    pub link: Option<String>,
    /// What happened with the phone, when the sender asked for it: "sent", or why it was held.
    #[serde(default)]
    pub phone: Option<String>,
}

/// What a sender posts to `POST /api/notices`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(default)]
pub struct NewNotice {
    pub source: String,
    pub sort: String,
    pub title: String,
    pub body: String,
    pub link: Option<String>,
    /// Also send it to the phone, if the phone's own choices allow it. Off unless asked for.
    pub phone: bool,
}

/// What Hub did with a message you wrote to a thread that was working, or its answer when the
/// message was a question for the side (`judge.rs`). Kept in memory only, one per thread.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Aside {
    /// "answer" (a question answered, `text` is the answer), "stopped", "steered", "queued" or
    /// "new" (the message looks like a task of its own). A client reading another word shows
    /// `text` as it is.
    pub kind: String,
    /// What you wrote, for "answer".
    #[serde(default)]
    pub question: Option<String>,
    /// The answer, or empty.
    #[serde(default)]
    pub text: String,
    #[ts(type = "number")]
    pub at: i64,
}

/// An idea from the idea inbox (`ideas.rs`): said or typed at any time, then placed in the
/// focus of the project Hub guessed, under a stage's ideas or under Parked.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InboxIdea {
    pub id: String,
    /// As it went into the focus file, one short line.
    pub text: String,
    /// As it was typed or said.
    pub said: String,
    #[ts(type = "number")]
    pub at: i64,
    pub state: IdeaState,
    /// The project folder it is in, and the project's name.
    pub project: Option<String>,
    pub name: Option<String>,
    /// The stage whose ideas it is under; None is Parked.
    pub stage: Option<String>,
    /// Why it went there, in a few words, when a model placed it.
    pub why: Option<String>,
    /// Who chose the project: "model", "name" (the idea names it), "here" (the page you were on),
    /// "only" (the only project) or "you" (moved by hand).
    pub by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum IdeaState {
    /// Being placed.
    Sorting,
    Placed,
    /// No project could be guessed: it waits in the inbox for you to pick one.
    Unsorted,
}

/// `POST /api/ideas`.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct NewIdea {
    pub text: String,
    /// The project folder you were looking at, the best guess when the idea does not say.
    #[serde(default)]
    #[ts(optional)]
    pub here: Option<String>,
}

/// A project an idea can be moved to, with its stages in order (`GET /api/ideas/places`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IdeaPlace {
    pub path: String,
    pub name: String,
    pub stages: Vec<String>,
    /// Index of the stage now.
    #[ts(type = "number | null")]
    pub active: Option<usize>,
}

/// `POST /api/ideas/{id}/move`: put the idea in this project, under this stage's ideas, or
/// under Parked when `stage` is None.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export)]
pub struct MoveIdea {
    pub project: String,
    #[serde(default)]
    #[ts(optional)]
    pub stage: Option<String>,
}

/// A draft from a prompt link: text to read and send, never run by itself.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PromptLink {
    pub prompt: String,
    /// A project folder on this machine.
    pub folder: Option<String>,
    /// A GitHub repository, `owner/name`.
    pub repo: Option<String>,
    /// Characters that cannot be seen were taken out: the link may be trying to hide something.
    pub stripped: bool,
}
