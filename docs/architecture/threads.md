# Architecture: Threads, restarts, the log and what Hub sends
<!-- för: agent -->

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## Thread lifecycle
<!-- covers: crates/hub-core/src/hub/observers.rs, crates/hub-core/src/engine/, crates/hub-core/src/warm.rs, crates/hub-core/src/npx.rs, crates/hub-core/src/timings.rs, crates/hub-core/src/standby.rs, app/src/lib/warm.ts, app/src/lib/state.svelte.ts, app/src/lib/focus.ts, app/src/lib/worlds.ts, app/src/lib/launch.ts, app/src/lib/projectNav.ts, app/src/lib/threadActions.svelte.ts, app/src/lib/threadLists.svelte.ts, app/src/lib/standIns.svelte.ts, crates/hub-core/src/awake.rs, crates/hub-core/src/supervise.rs, app/src/lib/components/ComposerDock.svelte, app/src/lib/stash.svelte.ts, app/src/lib/composerHistory.svelte.ts, app/src/lib/components/ComposerStash.svelte, app/src/lib/components/Composer.svelte, app/src/lib/components/SuggestionList.svelte, app/src/lib/suggest.svelte.ts, app/src/lib/components/ModelPicker.svelte, app/src/lib/modelRows.ts, app/src/lib/components/PlusMenu.svelte, app/src/lib/components/ComposerPlus.svelte, app/src/lib/components/MicMenu.svelte, app/src/lib/composerVoice.svelte.ts, crates/hub-core/src/hub_tools.rs, crates/hub-core/src/mcp.rs, crates/hub-core/src/mcp/, crates/hub-core/src/reach.rs, crates/hub-core/src/hub/reach.rs -->
1. `POST /api/threads` stores the thread. Its first message starts the engine process, and the
   thread is `running` from the moment a message is in, not once the engine has taken it.

   **After an event is logged** (`hub/observers.rs`). `Hub::emit` and `Hub::emit_once` both redact, append, publish and then run the same list `observers::ALL`: the status shadow, the first-token clock and the after-turn queue (`look_at`). A new "do something when X is logged" feature adds a function to that list; it does not edit `hub.rs`. Observers are quick and synchronous and never log events themselves.

   **Fast first answers** (`crates/hub-core/src/warm.rs`, `crates/hub-core/src/npx.rs`). An engine whose command
   is `npx -y <package>@<version>` starts through npx only until npx has the package: after that
   `npx::direct` finds its program in npm's cache (`_npx/*/node_modules/<package>`, the installed
   version being the asked one) and the engine starts as `node <program>` (the Node beside npx when npx is named by its path), which saves the 1 to 2 s npm
   spends finding what it has. Anything unexpected (no version asked, no cache, a program that is not
   JavaScript) leaves the line to npx. Starting an engine is most of the wait
   before a new thread answers (measured on Christoffer's PC, 2026-10-31: Claude 3.3 s, Codex
   4.5 s, Hermes 14.7 s, of which about 10 s is Hermes bringing up its own gateway). So the
   new-thread composers (New thread and a project page, `app/src/lib/warm.ts`; the same box sits under Home's board) ask
   `POST /api/threads/warm` on the first key, with the engine, folder (a fresh scratch folder for
   "No project") and settings they would start with. The core reserves the thread's id and runs
   the engine as that thread's runtime through `initialize`, `session/new` and the provider's
   model, then waits up to three minutes for the thread. `POST /api/threads` with `warm: <id>`
   creates the thread under that id, and its first message goes to the engine that is already
   up. Nothing reaches the agent before that, so a warm engine spends nothing; nothing is logged
   for a thread that never came. The engine connects to Hub's tools (`/mcp/<id>`) as it starts,
   before the thread exists, so `mcp::handle` answers an id a warm engine waits for as that thread
   (`Warm::stand_in`); turned away, Claude Code marks the server "hub" as failed for the whole
   session. A thread that came out different (another folder, another
   provider pick) gets a fresh engine, as without warming. Auto, attempts and other machines are
   not warmed. Existing threads already start their stopped engine on the first key
   (`POST /api/threads/{id}/warm`).

   **A chat starts lean.** Warming hides the engine's own start, but Claude Code also connects
   every MCP server and loads every plugin and skill of the user before it asks the model
   anything. On the PC (2026-11-02, a Haiku chat from the phone, from `hub.db`) the first model
   call came 7.8 s after the turn began, with 44 000 tokens of context (92 slash commands, a few
   hundred deferred tools) before the first word, and the model then spent a round trip fetching
   the web search tool (ToolSearch). A thread whose folder is `<projects root>/chats/<time>` (the
   phone's Chat world) therefore starts Claude with `connections::lean`:
   `ENABLE_CLAUDEAI_MCP_SERVERS=false`, `strictMcpConfig` and an empty `settingSources`, so no
   plugin, skill, hook, settings file or MCP server of the user's, only Claude Code's own tools
   and Hub's. Dev threads keep everything; other engines are untouched. claude-agent-acp still
   reads the user's settings.json itself and reports its `model` as the session's, and trusts
   Claude Code to be on it, so the `model` named there goes along as `options.model`
   (`connections::settings_model`); without it a chat said Sonnet and ran Opus. For the
   same reason a new session is sent every setting the thread picked, even one it reports as
   current (`engine/acp.rs`); only a resumed session skips what it already has.

   **A turn the engine starts on its own** (`engine/unprompted.rs`). ACP knows only the answer to a
   `session/prompt`, but Hermes runs `delegate_task` in the background and, when the helper is done,
   starts a new turn in the same session with no prompt behind it (2026-11-06: the thread looked
   finished at "Review is running", then idle while the agent patched and ran `cargo test`).
   An update that arrives while no prompt of Hub's is in flight now starts an unprompted turn:
   the thread shows as working, `turn_start` and `turn_end` are logged with `unprompted: true` and
   no `prompt`, and it ends once no tool call is open and the engine has been quiet for 30 s (per
   Hub, `unprompted::set_quiet`, so a test's shorter quiet reaches no other test's Hub). A
   prompt of Hub's (`hand_over`) ends it first; Stop cancels it; the idle stop does not take an
   engine that is in one. Its start is logged after the end of the prompt before it (`prompt_over`
   holds the turn until that end is written, and `idle_unless_own_turn` does not set the thread idle
   over an own turn that began meanwhile), and an engine taken over mid-turn from the core
   before (`keeper.rs`, `Attached::busy`) counts as in that prompt from the first update, since the
   keeper replays what it said meanwhile before `turns` waits for the prompt's end. A failed turn whose engine code is `admitted_turn_failed` says it is
   usually the model provider not answering (`plain_detail`).

   **A chat talks like a chat.** A chat used to get the coding threads' writing rules (headings,
   Note quotes, checklists) and Claude Code's own coding prompt, so it answered like a status report:
   a Haiku chat (2026-11-03) stacked "not verified" caveats, added a Sources list beside the source
   cards and read out the brief it had given a side job. A chat's MCP `instructions` (`tool_instructions::instructions`)
   now hold Hub's tools, `tool_instructions::CHAT_STYLE` and the library rule, not the "Writing for Hub" part
   of `hub_tools::INSTRUCTIONS` (cut at `WRITING_MARK`); and `connections::lean` adds the same text to
   Claude's system prompt (`_meta.systemPrompt.append`), which models weigh more than tool
   instructions. The style asks for the answer first, plain prose, no narrating of tools or side jobs
   and at most one caveat, and still lets the model use a list, table, code or headings when the user asks or
   the content is one. One text for every model; a chat already open keeps the instructions it started with.

   **Where the time goes** (`crates/hub-core/src/timings.rs`). Every message, engine start, model
   call, tool call and turn end is in the event log with its time, so how long an answer took in
   each part is read from the log, not measured by a second instrument. `GET /api/threads/{id}/timings`
   cuts each turn into `engine_ms` (message to an open session; only when an engine came up for
   it, and near zero when a warm one was waiting), `model_ms` (turn start to the first model call
   answering, where Claude Code's own start-up and the first token wait), `tool_ms`, `text_ms`
   and `done_ms`, with each tool's duration, the model, the tokens of context at the first call
   and how many slash commands the engine listed, and `usage` (`{ input, output, cache_read,
   cache_write }`, each optional): the tokens the engine said the turn used. That comes from ACP's
   prompt response (`usage`: `inputTokens`, `outputTokens`, `cachedReadTokens`, `cachedWriteTokens`,
   behind the crate's `unstable_end_turn_token_usage` feature), which `engine/acp/turns.rs` writes
   into `turn_end` as `usage` when the engine sent it; an older `turn_end` has none and the turn
   shows none. `GET /api/timings` sums up the first answers of
   the latest 40 threads by engine and kind (`chat` or `dev`) as median and 90th percentile. The
   engine's own start is the one thing measured where it happens: `session_started` carries
   `start_ms: { initialize, session, total }` since the core started it. Listed in
   `compat::FEATURES` as `speed.timings`. No screen yet; CPU and memory of the engine's processes are
   not measured.

   Warming on the first key hides a second or two; it cannot hide a harness that brings up a
   service of its own. Hermes's ACP agent is a viewer of the Hermes gateway and starts one when
   none runs, and that gateway ends itself after ten idle minutes. On the PC (2026-11-01, from
   `hub.db` and Hermes's logs) a Hermes thread took 15 to 24 s to its first turn when it had to
   start the gateway and 3 to 6 s when one was up. So the core keeps such services up itself
   (`crates/hub-core/src/standby.rs`): a harness names a command in its catalog entry
   (`Harness::standby`, for Hermes `hermes gateway ensure`, the way Hermes's own desktop app keeps
   its gateway), and the core runs it five seconds after it starts and every five minutes, for
   each harness an engine here uses, with those engines' keys, outside the core's job so the
   service outlives a core update. A gateway started this way has no idle exit. When one of those
   keys changes in the vault it runs `Harness::standby_again` instead (`gateway restart --detach`,
   which lets running turns end), since the service keeps the keys it started with. Nothing runs
   while every agent is paused.

   **The thread opens on send** (`showStarting` in `app/src/lib/state.svelte.ts`, which passes on to `app/src/lib/standIns.svelte.ts`). Creating a
   thread can itself take seconds before `POST /api/threads` answers: a dev worktree (a fetch of
   up to 15 s and `git worktree add`, `dev_threads.rs`), a scratch folder, another machine. So
   New thread (and Home) and a project page open a stand-in thread the moment the message goes, with the message
   and "Starting" as the real one shows them, and swap it for the real thread (its log loaded
   first, the box's draft moved along) when the core answers. A message written in the stand-in
   goes once the real thread is there. If the thread does not start, the stand-in goes and the
   message is back in the box. Stand-in ids start with `starting:` and never reach the core.
2. The engine is initialised and a session created, or the previous session loaded
   (`session/load`) when the thread has one. History the agent replays on load is ignored
   because the log already has it. A `session/update` never gets an error back (a notification
   has no reply, so ACP would only log a bare "Internal error"): one for a thread deleted while
   its engine winds down is dropped, and any other failure to log it is a hubd warning that
   names the thread and the cause.
3. Each message is a `session/prompt`. Messages sent during a turn are queued. Stop sends
   `session/cancel` and rejects any open permission question, as ACP requires. An engine that
   has not ended the turn ten seconds after that is stopped by Hub: the turn is logged as
   `turn_end` with `stop_reason: cancelled` and `forced: true`, the engine process goes with its
   connection, and the messages it held in its queue go to a fresh engine, so Esc always stops
   the turn and loses nothing that was queued. The tool the engine was waiting on goes too:
   what sits under the engine (found through parent links, `workers::engine_tools`, so a
   command in a process group of its own, as the Claude adapter starts them, is found as well)
   is stopped first. A dev server the agent left on is not under the engine any more and stays.

   The queue is the log: a queued message is a `user_message` like any other, and the engine
   takes it with `Hub::take_prompt`, which applies `queue` events (`edited` with new text,
   `removed`) logged since and marks it taken. `PATCH`/`DELETE /api/threads/{id}/queue/{seq}`
   log those events and are refused once the engine has the message (its `turn_start`, or the
   taken set, checked under one lock). So a queue survives a core restart as the restart's
   `queued` messages do, edits included. Messages the engine let go after failing in a row are
   logged as `queue` `dropped`. Steering (`POST .../queue/{seq}/steer`, or a message sent with
   `steer: true`) logs `queue` `steered` and sends `Command::Steer`: ACP has no way into a running
   turn, so the driver cancels it and moves that prompt to the front of its queue. Whether Enter
   queues or steers while a turn runs is the app pref `steer`; Alt Enter does the other. The app's fold (`transcript.ts`) holds a message sent while a turn
   runs out of the transcript until its own `turn_start`, so the conversation reads in the order
   it ran; `queued()` lists the held ones, shown with edit and remove at the top of the composer
   (`ComposerDock.svelte`). The open permission question docks there too, and so does a reply
   that ended on a question with choices (`openQuestion`): first the replies the agent offered with
   `offer_replies` (a `replies` event, folded onto the turn it was made in; a click sends each
   option's `message`), else numbered or lettered choices in the text (`questionIn`), else a lone
   go-ahead question such as "Should I start a new thread?" (`proposalIn`), answered by Go ahead,
   Go ahead then close this thread, or No. 1 to 9 answer while the message box is empty. In the transcript a permission question is one line. A shell
   command is put in plain words first (`describe.ts`, e.g. "Searches the code and runs tests"),
   with the raw command behind a "Show the command" fold.

   Ctrl S in the message box puts the draft away (`stash.svelte.ts`: one list for all of Hub, newest
   first, 30 at most, kept in the browser's storage with its files). Something put away shows as a
   "Stash N" tab on the box's top edge; Ctrl S in an empty box, or a click on the tab, opens the list
   over the box (the list's open state is `StashList` in `composerHistory.svelte.ts`, drawn by `ComposerStash.svelte`; Arrow Up recall is `Recall` in the same file). Enter or a click puts a row back in the box, Delete or the cross removes
   it, Esc or typing closes the list. It is copy and paste in and out, never a thread of its own.
   Ctrl Enter in the box starts a thread of its own with the text, in the same folder, on the same
   engine and with the same settings except the permission mode (`hub.newThread(…, open = false)`),
   and leaves the open thread as it is; a "Started in the background · Open" tab shows on the box for
   eight seconds (`started` in `ThreadView.svelte` and `Home.svelte`). Enter still replies in the thread.

   The + at the left of the box's tool row (`PlusMenu.svelte`, in a reply box and a new thread's box)
   is where everything that goes along is added, instead of a clip and a bag button: Add files or
   photos (Ctrl U), the Context bag as a list beside the menu, Folder or repo, Connections, Another
   thread (puts `@title` in the text at the caret and a `threadRefs` entry, as the @ menu does),
   Uncommitted changes (`/diff`) and a switch for web search. Arrow keys walk the rows, → opens a
   list, ← or Esc closes it. Connections lists what the thread's engine reaches (`/api/threads/{id}/connections`,
   or the engine's entry of `/api/connections` in a new thread's box) with each one's state.

   Folder or repo and the per-thread connection switches are a thread's *reach* (`reach.rs`; restarting an engine on a new reach is `hub/reach.rs`,
   `ThreadReach`: up to eight `folders` the agent may also look in, and `connections_off`, names
   switched off for this thread only). It is kept under `reach:<thread id>` and read through
   `GET`/`PUT /api/threads/{id}/reach`; with GitHub connected (`github.svelte.ts`) typing in the Folder or
  repo search also lists the account's repositories that are not cloned on the thread's machine
  ("Clone and add" clones next to the other projects, then adds the folder); the box shows the folders as chips above the text (× removes one); a new thread's box keeps them and sends them as `NewThread.reach`, and a
   thread that has one never takes a warm engine. It applies when the engine starts: the folders
   go in the ACP session request as `additionalDirectories` when the agent advertises it, and into
   Claude Code's options of that name (`connections::add_dirs`); the names join what Settings >
   Extensions switches off for the engine. A change stops an idle engine (one in a turn is stopped
   when the turn is over, by the same loop that renews engines whose key changed), and the next
   message starts it on the new reach and loads the same session. An older Hub lacks `thread.reach`
   (`compat::FEATURES`); the menu then lists folders and connections and says the switches need the
   next version, so nothing fails.
4. Permission requests are answered asynchronously, so the agent keeps streaming while it waits.
   A question still open when the turn ends (an agent whose own approval timed out, as Hermes
   does) is rejected with the turn, and an answer only puts a thread back to running from
   waiting on you, so a stale card can never leave an idle thread marked as running.
5. Session settings (model, effort, permission mode and whatever else the engine offers as ACP
   config options) are `setting` events. A running engine applies them right away; a starting
   engine re-applies the latest of each from the log, so they survive restarts. A setting the
   session does not list, or whose value it does not offer (one chosen while another engine ran,
   or a thread created with another engine's settings), is not sent: it is logged at debug, not
   as a warning, and the rest still apply. A refusal of a setting the session does list is still
   a warning. **A new thread starts in the engine's auto mode** (`default_mode`, `DEFAULT_MODE` in `engine/acp.rs`): when
   the engine first starts and the thread has not chosen a mode the engine offers, the first choice
   called or containing "auto" (else "dont_ask", Hermes's mode that does not ask about edits) is set and logged as a `setting` marked `by: "default"`, so a later
   start and the composer see it, and a mode the user picks afterwards (manual included) wins. It
   is left alone for a resumed session, for threads whose questions Hub answers by its own rules
   (design mode, Vakten, a routine's run, a fix thread that picks by words) and for an engine that
   offers no auto mode. **A setting the engine refuses while a turn runs is kept, not dropped**:
   Hermes changes its mode only when the session is idle, so `SessionState::set_during_turn`
   keeps the pick in `deferred`, publishes nothing (the composer keeps showing the pick instead of
   snapping back to the old mode) and `apply_deferred` sends it when the turn ends, then publishes.
   The pick is in the log either way, so an engine that restarts first applies it from there. A pick by name (`~terra`, `~medium` as a setting value, what the phone sends
   when it only knows an engine by its family) is looked up among what the session offers (`by_name`)
   and logged as the real choice, `by: "name"`; a name the session does not offer is dropped. The
   engine's answer is a `session_state` event, which is what the composer shows. In a thread that runs
   on another machine the picker lists that machine's engines (`loadHostEngines`, its hubd's
   `/api/prefs/config?host=`), and a switch goes to that hubd like any other thread request.
   Engines that machine's "Ready for work" marks missing (not installed, not signed in there;
   `loadReady`, `/api/machine/ready?host=`) stay in the list dimmed with a short why ("Sign-in
   expired", "Not installed") and the fix as a button right there (`fixEngine`: a terminal on
   that machine that signs it in, as its page's button does); picking one says why on a toast
   that carries the same button (`hub.fail(e, action)`). An engine whose program is missing fails to start with that said ("hermes is not
   installed on …") rather than ACP's bare "Internal error", and every ACP error keeps the
   detail the agent gave. An engine whose connection closes mid-turn (ACP's "Incoming transport
   closed") says so in plain words ("The engine stopped in the middle of this turn…", with the
   last line of its log as the detail) and is ended there, so the next message starts a fresh
   one on the same session instead of failing at once on the dead connection. Slash commands and
   context usage arrive as ordinary ACP updates (`available_commands_update`, `usage_update`);
   `/usage` is answered by the Claude adapter itself with the plan limits.
6. Each engine's process tree stops with it (`crates/hub-core/src/supervise.rs`): on Unix the
   engine leads its own process group, on Windows `hubd engine <command>` runs it inside a Job
   Object that kills the whole tree when the wrapper goes. What the engine prints on stderr goes
   to `<data dir>/engines/<thread>.log`, and the tail of it into the `error` event when it dies.
   The `ssh -N -L` tunnels to other machines are not engines, but the same file ends the ones a
   core left behind (`end_stale_tunnels`, called when `hubd` starts): `kill_on_drop` does not run
   when the core exits, and each orphaned `ssh` keeps a login open on the other machine until
   its sshd refuses new ones. Only tunnels with Hub's exact options and no living parent are ended.
   The core's own background loops (backup, routines, updates, awake …) start through
   `supervise::forever`: a panic is logged as an error (so it reaches the log and Problems) and the
   loop starts again after 1 s, doubling to 5 min; a loop that returns normally is not restarted.
7. An engine that has been idle for 15 minutes is stopped to free memory; the next message
   starts it again and resumes the session. A crashed engine marks the thread as failed and is
   restarted the same way.

While any thread here is `running`, the core keeps the computer from sleeping
(`crates/hub-core/src/awake.rs`: `SetThreadExecutionState` on Windows, `caffeinate` or
`systemd-inhibit` elsewhere); the screen still turns off. `keep_awake: false` in `hub.json` turns
it off.

Status of a thread: `idle`, `running`, `waiting_on_you`, `error`, `in_terminal`. A restart of the core resets
`running` and `waiting_on_you` to `idle`; the turns it cut off carry on (below).

### A thread's status from its log (`status.rs`)

<!-- covers: crates/hub-core/src/status.rs -->

`status::next_status(prev, &event, open_questions, asleep)` is a pure state machine: it says what a
thread's status is after one event, or `None` when the event leaves it alone. `status::replay`
folds a whole log with it. It is step 1 of D3 (plan: the project file `teknisk-skuld/d3-status-fran-handelser-plan.md`, not in the repo)
and only the shadow mode calls it (`status_shadow.rs`): `Hub::set_status` still writes `threads.status`. The
last steps write event and status in one transaction (see "Thread status from events").

The rules: `user_message` to an idle or errored thread and `turn_start` give running;
`permission_request` and `slept` give waiting on you; an answer (`permission_resolved`,
`permission_auto`) gives running only from waiting and only when no other question is open, except
a waking answer (`woke`), which always does; answers closed by a restart or a move, and a
`turn_end` that hands a turn over to a message, change nothing;
`turn_end` gives idle; `error` gives idle for a failed turn and error for an engine that failed or
went away (`ENGINE_GONE`), unless the event carries `leaves`. A thread in a terminal is never moved
by engine events. A `turn_end` after a `slept` (the engine is stopped, its turn cancelled) leaves a
thread waiting while a slept question is unanswered. States no engine event describes (terminal, cloud sessions, a queued message
taken back, an engine parked, the start-up repair) are logged as a `Status` marker event by `Hub::set_status_because`; the marker names the status.

## Restarts
<!-- covers: crates/hub-core/src/restart.rs -->

An engine without a keeper (see "Engines that outlive the core" below; on by default) is a child
of the core, so an update, new settings or a crash ends the turn it is in. The thread carries on anyway (`crates/hub-core/src/restart.rs`, after T3 Code's
`RestartContinuation` and command receipts):

- Each prompt is logged as a `turn_start { prompt }` just before it goes to the engine, and its
  `turn_end` and `error` name the same `prompt`. A start with no end after it is a turn that was
  cut off; user messages after the last started one never reached the engine.
- A core that hands over (`Hub::park`: update, restart for settings, Ctrl C) first notes why
  under `restart.planned` and aborts every engine task, which kills the engine and logs nothing,
  so statuses and the log stay as they were. Without that note the next core calls it a crash.
- When the core starts (`restart::resume`), each cut-off turn gets a `restart` event (`by`,
  `prompt`, the `queued` messages) and a message marked `restart: <prompt>` asking the agent to
  carry on and to check before re-running anything that must not run twice. The engine resumes
  its session (`session/load`), takes that message, then the queued ones in order. The thread
  shows one line: "Hub updated at 14:02, in the middle of this turn. Asked the agent to carry on".
- Nothing is sent twice. The marked message is the receipt: a core that dies again before the
  engine got it sends the logged one instead of asking again. The app gives each message and new
  thread a client id, and the core logs the message together with its receipt in one transaction
  (`Store::append_event_once`, the `requests` table); the app retries a send for up to a minute
  while the core is away, so a message written during an update is neither lost nor doubled.
- After three requests to carry on with no turn ending in between, Hub lets the turn go
  (`state: "stopped"`, `why: "loop"`); `resume_turns: false` in `hub.json` does the same for every
  restart (`why: "off"`). Messages written meanwhile are still sent.
- The permission questions the last core left open are closed first thing (`close_orphaned_questions`:
  a `permission_resolved` with `restart: true`), so no card from before the restart stays on screen;
  an agent that still needs the answer asks again with a new id. A card still answered from before
  (a window that has not reloaded) closes itself in `answer_permission` with a `stale: true`
  resolution and a message saying why, not "no open permission request".
- What a restart does not keep: an open permission question (the agent asks again), a second
  opinion or other small helper that was running, and a message sent in the moment between a
  parked core's last write and the new core reading the log.

### Engines that outlive the core
<!-- covers: crates/hub-core/src/keeper.rs -->

Most restarts no longer cut a turn off at all (`crates/hub-core/src/keeper.rs`). Each engine runs
under a keeper, `hubd keep`: a small process the core starts apart from itself (its own process
group on Unix; on Windows detached and, where the job allows, outside the core's job), which reads
what to start from its stdin, starts the engine (in a job of its own on Windows, its own process
group on Unix), writes its stderr to the engine log and listens on a loopback port. It writes a card
(`<data>/keepers/<thread>.json`: port, a token of its own, pid, protocol version) and the core talks
to it with that token, line for line as over the engine's stdio.

- `Hub::park` tells each kept engine's keeper to hold on (`detach`) before it lets go. The keeper
  keeps the engine working, keeps what the agent says (up to 50,000 lines) and waits up to ten
  minutes for a core. A core that goes away without `detach` (Stop, an idle engine, a forced stop,
  the panic button, a crash) ends the engine and its tree at once, as when it was a child. Deleting
  a thread (and `shutdown`) also tells the keeper to stop by its card (`keeper::stop`, op `stop`),
  whether or not a core is attached, so an engine whose core connection is stuck does not live on.
- The next core, before it carries anything on (`restart::resume`), takes every keeper over
  (`attach`, `Hub::adopt_engine`): no `initialize` and no `session/load`, since the keeper noted the
  agent's answer and the session id; the settings come from the last `session_state` in the log.
  What the agent said meanwhile comes first, questions it asked meanwhile are asked again, and a
  turn the old core started ends in the new one: the core asks `_hub/wait_turn`, which the keeper
  answers with that prompt's answer. Messages written while no core had the engine follow. Nothing
  is logged as a restart and the agent is not asked to carry on.
- The keeper speaks just enough ACP for that: requests from the core go to the agent under ids of
  its own (every core counts from zero, and the old core's prompt may still be open) and their
  answers come back under the core's id, or are dropped when that core is gone (kept, for a prompt).
- A keeper with a core attached that has not handed over refuses every other `attach` ("held by
  another core"), and `keeper::take_over` leaves such a keeper and its card alone. Without this a
  second core on the same data folder (Hub's own tests on Christoffer's PC, 2026-11-01) took over
  every working engine and ended them all when it let go: the threads died at the same moment
  with "Incoming transport closed". Tests build their cores on a temporary data folder.
- When a keeper ends the engine it says why in the engine's log (`[hub, unix time …]`): the engine
  exited by itself (with its exit code), the core let go without handing over, or no core came
  after a handover. A thread whose engine went away mid-turn shows that line (below).
- An update waits only for working threads whose engine has no keeper (`Hub::busy_unkept`).
- A warm engine (see "Fast first answers") has a keeper like any other. One whose thread was never
  created is let go by the next core; a keeper removes its card only while the card is still its
  own, since a fresh engine for the same thread may already have written one.

Off with `keep_engines: false` in `hub.json`, and while `trace_engines` is on. Where a keeper cannot
start, or has another protocol version, the engine runs as before and the turn is carried on as
below. A running engine keeps the settings it started with until it next starts (after 15 idle
minutes, or Stop).

## Panic button
<!-- covers: crates/hub-core/src/halt.rs -->

One click pauses every agent on every machine (`crates/hub-core/src/halt.rs`): the pause button at
the bottom of the sidebar, Ctrl K "Pause every agent", or the phone page. It reuses the restart
machinery rather than freezing processes, since an engine frozen for an hour loses its connection
to the model anyway:

- The core holds first (`Hub::halt`, kept in the store under `halt`, so a restart stays paused),
  then stops every engine as `Hub::park` does: the task is aborted, the engine's process tree goes
  with it and no `turn_end` is logged, so the log still says which turn was cut off. Open
  permission questions are rejected. Each thread that was in a turn gets a `paused` event and goes
  idle without coming back to the inbox.
- While paused no engine starts (`Hub::runtime` refuses). Messages, whether from the user, a
  routine, a side job, a plan-limit resume or another agent, are logged and wait. Scheduled
  routines hold back and "Run now" is refused.
- The machine where it was pressed posts `POST /api/halt/pause { here: true, by }` to every other
  online machine, which does the same for itself. Machines that are offline are named in the
  answer and in the app's strip as not reached.
- Your tools join in by marking an action in their `hub.toml` with `panic = "pause"` or
  `panic = "resume"` (no `on` or `ask`); each machine runs its own tools' actions.
- Resuming (the strip's Resume all, the same sidebar button) clears the hold everywhere and runs
  `restart::carry_on(hub, "pause")`: each cut-off turn gets a `restart` event with `by: "pause"` and
  a message asking the agent to carry on, then the messages that waited go out in order. A pause is
  never counted towards the three-in-a-row loop guard, and `resume_turns: false` does not apply.

Processes a thread left running outside its engine (a dev server started with `nohup`) and docker
containers are not touched; the Workers view stops those.

## Activity list
<!-- covers: app/src/lib/components/ScopeBar.svelte, app/src/lib/motion.ts, app/src/lib/fold.ts, app/src/lib/components/ActivityList.svelte, app/src/lib/components/StateSign.svelte, app/src/lib/inputMode.ts, app/src/lib/components/ThreadView.svelte, crates/mock-agent/src/sim.rs -->

The sidebar can show threads as an inbox instead of the menu (Ctrl E, `prefs.sidebarMode`). Three
marks on the thread drive it, kept as columns like `archived` rather than as events, since they say
how the user files the thread and nothing about its work: `returned_at` is set by the core when a
turn ends, fails, asks something or is cut off by a restart; `seen_at` when the user opens the
thread; `settled_at` when they mark it done. Marks never bump `updated_at`. A settled thread comes
back when the user writes to it or it needs them. Settling or archiving a thread that is idle also stops its engine at once (`Hub::patch_thread`), instead of leaving it to the 15-minute idle timeout; a thread in a turn or asking something keeps its engine until it is done. The list shows threads that came back, newest
first, with "Done" while `returned_at` is after `seen_at`; working and settled threads fold at
the bottom.

How much room a thread takes is `prefs.cards` (Settings > Sidebar, Thread cards): comfortable is the
three-line row, compact two lines (project, title, PR and who runs it on top, the sign, word and time
below with the side jobs at the end; the project name is left out while the list is narrowed to one
project), dense one line (the pin's row with a project-coloured dot instead of the pin). What the
Inbox is narrowed to sits in `ScopeBar.svelte`, a divider line: project icon, name and count, then the
machines as glyphs (none with one machine, all up to four, else the three with most threads and a
"+N" menu; a chosen hidden machine takes the third place), then the cross that clears the project.

Which thread you are in is told by a calm grey fill on its row (one step lighter than the hover)
and a heavier title; the project's colour is in the project's name on every row and in the thread's
header, whose first line, above the title, has the project's icon and name in its colour (the
folder's path is its tooltip, a click opens the project page), the machine, the state and the
branch. No bar, wash or ring on a row says "needs you": that is the sign and the word.

A row has three lines so nothing is mixed up (`ActivityList.svelte`). First what and where: the
project in its colour with its branch, who started it (the robot), a pin or an unsent draft, and
at the end the machine it runs on, always and as its icon alone (`.machine`, an apple for a Mac,
dimmed when it is offline; the hover has its name) with the mark of the agent after it (the hover
says "Claude Code on Claude", from `harnessLabel` and the provider).
Second the title on its own, with its pull request (number and open, draft, merged or closed) and
"N side jobs" at the end. Third how it stands: a sign (`StateSign.svelte`) and the status word with
the time; the end of that line is empty, so the hover's Snooze and Settle buttons land there and
hide nothing. The sign is a
ring in the state's colour with a mark in it: ? asks you, ! stuck, ✕ failed, a clock for the
subscription limit, >_ in a terminal, ✓ done and unread, a half-full ring stopped half way, the
working grid (`Working.svelte`) while it works, an empty grey ring when it is read and quiet. A Worker run's
chain and a fix thread's line follow as a fourth line. The rows are set small on purpose (project
line, state line 10 px, title 11.5 px, drawn 1.15 times larger with the sidebar), so a row reads
as a label and the list stays dense. The focus frame around a clicked control is handled in
`app.css` and `lib/inputMode.ts`: it is drawn only after Tab or an arrow key, not after a click.

A thread's own activity never moves it. The inbox sorts by `returned_at`, the working list by
`sent_at` (set when the user sends a message), the settled shelf by `settled_at`, with the id
breaking ties. When rows do move, `use:motion` (`app/src/lib/motion.ts`, adapted from T3 Code)
slides them: it watches its list's direct children and animates each change from where the rows
were, 150 ms, fading rows in and out. Lists put rows, headings and dividers as siblings in one
element so a row changing section slides too.

The list's sections, top to bottom: Pinned (`pinned_at`, also the order key a drag sets; a pin is
one line, the pin, title, state sign, time, machine and agent, and the inbox under it gets its own
"Inbox" heading so it is plain where the pins end), the inbox, Working (`prefs.workingSection`), Snoozed (`snoozed_until`, woken early by needing the user,
failing or a message; a snooze that ran out sorts as a return at its wake time) and Settled. The
groups from Working down (with From Hub) need nothing from the user, so a spacer in the list
docks them at the bottom of the sidebar, above the usage bar, and the empty space on top says
nothing waits. Each opens downward under its heading; when its rows run past the bottom, the
heading scrolls to the top so they show (`openGroup`). How a group opens is
`prefs.groupMotion` (Settings > Sidebar > Opening a group). A group's rows sit in a box of their
own (`drawer` items), which `fold.ts` grows from nothing under the heading and shrinks back into
it as Svelte transitions with its overflow clipped, so no row ever shows over the heading or its
neighbours; the spacer above and the groups below give way as it grows. `unfold` (the default)
and `cascade` fold the rows out one after the other (`.stagger`, a short and a longer step),
`drawer` slides them out from under the heading as one, `now` does the same box in 160 ms. A long
group grows only as far as the sidebar is tall and takes the rest at the end, below the edge.
While it moves, `data-fold` keeps the list's slide motion out of the way and `REMEASURE` has it
take the new places after. When the rows run past the bottom, `follow` keeps the sidebar scrolled
so the heading rises to the top in step; folding needs nothing, the scroll comes down as the
content shrinks. Reduce motion opens and folds at once. All
are marks (`mark_thread`, no `updated_at` bump). Rows drag between sections to pin, unpin, settle,
un-settle or wake, but only after the row has been held for about 0.3 s (`HOLD_MS`, the row lifts
with a shadow): moving before that is a click, a scroll or a selection, so nothing is pinned by
accident; the settle button drags across rows to settle a run; Ctrl or Shift click picks
several. `hub.mark` does each of these with one Undo. `triage` runs in the core every five
minutes: it looks up each thread's pull request with `gh` (one call per repository) and settles
idle threads after `ThreadPrefs.settle_after_days` or when their PR merged after the user's last
message, unless the thread is pinned, working or has `auto_settle` off. Unsent text per thread
lives in `drafts.svelte.ts`, which also carries files dropped on a row to that thread's composer.

Labs > Simulate the inbox tries the list without an AI (`hub_core::sim` names the threads, `Hub::start_sim`/`clear_sim` in `hub/sim.rs` run them): seven threads in a
`Simulation` project on the `sim` engine, which is hubd itself (`hubd sim-agent`, the scripted agent
in `crates/mock-agent/src/sim.rs`), so every build has it and it is never offered as an engine. Each
thread's `scenario` setting picks a script: work with made-up tool calls for a while, then finish,
ask for permission, ask a question in the reply, or fail. Anything written to it gets a short
follow-up turn. Remove deletes every thread on the `sim` engine.

### Thread hover card
<!-- covers: crates/hub-core/src/gist.rs, crates/hub-core/src/replies.rs, app/src/lib/components/ThreadPeek.svelte -->

Resting on an inbox row for half a second opens a card beside it (`ThreadPeek.svelte`, replacing
the title tooltip); moving down the list swaps it at once, and it closes a moment after the pointer
leaves both, on a scroll, a click on a row or Esc, and at once when its row is settled, snoozed or
otherwise gone from the list (a row that leaves sends no pointer-leave, so the list checks). From the top: the whole title, the state in
words with when, machine, engine and how many messages; a TL;DR of one to three points; "Show last
reply" (R) with the agent's own words, to check the summary against; a "Needs you" box (red when
failed) with the question, the agent's quick replies on keys 1 to 3 and a reply box (Enter sends,
as the chat does, and queues behind a running turn); fact chips for the pull request (opens it in
the project's GitHub tab), branch, files changed and tests; and Open thread (Enter), Settle (E),
Snooze, Pin, Take over and the thread menu. A thread waiting on a permission offers "Open to
answer" instead of the reply box.

The card asks the core for `GET /api/threads/{id}/peek` (`gist.rs`), read from the log without a
model. Hub writes no TL;DR of its own: the agent writes it with its `write_tldr` tool (one to three
points and an optional `needs`, quiet, no permission question), logged as a `gist` event
`{ upto, points, ask, by: "agent" }`. Hub's instructions remind every top-level thread to call it
last when the work was more than a small change. It covers the turn it is written in, so that
turn's end does not make it stale; a message, a later turn end, an error or a permission question
does (`since` counts them). The card shows when it was written ("written by the agent · 17:42"),
dims an outdated one with "3 messages since", and offers "Ask for a TL;DR" when there is none or it
is outdated: that sends the thread an ordinary message asking the agent to write one, and the card
fills in when the agent does. Without one the card shows the opening of the last reply. Older
`gist` events (no `by`, written by the tidy model) still read as a TL;DR. `POST
/api/threads/{id}/gist` stays for older apps and now only answers the peek. Peeks seen this session
are kept in memory so a row opens at once on a second hover. Points are Markdown through `rich.ts`,
so pull requests, files (with their language's logo) and branches look and open as in the chat.

## Back and forward
<!-- covers: app/src/lib/history.svelte.ts, app/src/lib/modalHistory.svelte.ts -->

The mouse's side buttons and Alt ← / Alt → move through the places you have been, as in a browser
(`app/src/lib/history.svelte.ts`). A place is `hub.route`: the view plus what tells two of it apart
(the thread, the project and its tab, the settings section). Every change of it is pushed to the
window's own history, so the webview's own steps (over a live page's frame) land there too; going
to the place you are on adds nothing, and dialogs, menus and the big page panel are not places.
The app takes the side buttons itself and cancels the webview's, so one press is one step.
Deleted threads are skipped. In a text field Alt ← / Alt → stay in the field.

A window over the page with places of its own (Settings' pages) keeps its own small history
(`modalHistory.svelte.ts`, `useModalHistory`): while it is open the side buttons and Alt ← / Alt →
step through its pages first and leave the page behind it alone. A window with no places just
keeps the keys from reaching the page.

## Small helpers
<!-- covers: app/src/lib/risk.ts, crates/hub-core/src/autoprompt.rs, app/src/lib/components/TidyBar.svelte -->

Tidying a dump and writing a handoff each need one short answer from a model. The core starts the
engine for a throwaway session (`engine::acp::complete`), rejects every permission request so it
can only answer in text, and stops it again. Nothing of it goes in the log, but it gets a slip
under the purpose its caller names (see "What Hub sends to an AI"). Tidying picks the
model whose name contains `tidy.model` (default `haiku`) when the engine offers models.
Autoprompt (the wand in the composer, `autoprompt.rs`) is tidying with context: the same one answer, but
the prompt also carries the machine, project, folder, branch, the folder's top level, the rules file
(cut at 6 000 characters) and, in a thread, its latest 5 000 characters of conversation. The written
prompt is shown in an editable box (`TidyBar.svelte`); nothing is sent until the person uses it and sends.

Destructive commands are spotted in the app (`app/src/lib/risk.ts`), which asks the core what
their targets hold. Rules said in chat go under `## Rules from Hub` in the folder's CLAUDE.md and
are logged as `rule_saved`.

## Undo
<!-- covers: crates/hub-core/src/checkpoint.rs -->

Before each message reaches the engine, the core snapshots the repository the thread works in
(`crates/hub-core/src/checkpoint.rs`): `git add -A` into a throwaway copy of the index, a commit
of that tree, and a ref `refs/hub/<thread>/<seq>` that keeps it. Tracked and untracked files are in
it, ignored files are not, and the user's index, branch and stash are not touched. The log gets a
`checkpoint` event. Rolling back to one deletes files made since, restores changed and deleted
ones, and moves the branch back over commits made since, unless they are pushed or the branch
changed (then only the files go back). The state just before is snapshotted too and logged with
the `restored` event, so a rollback can be undone the same way. The engine's next message starts
with a note that files were rolled back. Folders that are not git repositories have no undo (one that stops being a repository after its
checkpoint, such as a removed worktree, makes the after-turn checks warn and skip);
`checkpoints: false` in `hub.json` turns it off. In the thread a checkpoint is a faint undo mark
between messages; clicking it says what would go back in one line, and a second click rolls back.

**Edit from here.** Hovering one of your own messages shows Copy and Edit. Edit opens the message
in place; sending it (`POST /api/threads/{id}/messages/{seq}/edit`) stops a running turn, removes
later messages still in the queue, rolls the files back to the message's checkpoint when "Files
too" is ticked (an ordinary rollback, so it can be undone), and logs a `rewind` event
`{ from, files }`. ACP has no way to cut a session short or fork it, so the engine's session is
left behind and the edited message carries the conversation before `from` to a fresh session, as
an engine switch does (`conversation_log` leaves out everything a rewind cut). The cut part stays
in the log; the transcript folds it into one line that opens to show what was there.

## Second opinion

`POST /api/threads/{id}/review` diffs the repository now (untracked files included) against where
the last review ended (each review keeps its snapshot under `refs/hub/<thread>/review-<id>`), or
the thread's first checkpoint, or `from`. A pile of three files or more, or 300 changed lines, is
first cut into parts by the small `tidy` model, which also marks parts that need no review (docs,
lock files, formatting). Each part that does goes, with what the person asked since then, to
another model in a
throwaway session (`engine::acp::complete`, every permission rejected, so it can read but not
change anything). The model is the guard's `review_model` (Sonnet unless changed). The answer starts with a verdict
line and is logged as a `review` event, once as `running` and again with the same id as `done` or
`failed`. Unless the verdict is "Looks good", the findings go straight back to the agent as a user
message marked `review: <id>`, which the thread folds into one collapsible "Reviewed by" line.
The eye on a reply's changes asks for one; replies a review saw are stamped.

The agent can ask for one too: each engine gets Hub's own MCP server at `/mcp/<thread id>`
(`mcp.rs`, plain JSON-RPC over HTTP, offered when the agent supports HTTP MCP servers;
`agent_tools: false` turns it off) with a `second_opinion` tool that takes a `reason`. Such a
review is logged with `by: "agent"` and the reason, and its findings arrive after the turn.

## Design mode
<!-- covers: crates/hub-core/src/design.rs, crates/hub-core/src/hub/design.rs, app/src/lib/components/DesignCard.svelte -->

For something big, a thread plans before it builds (`design.rs`, after Google's Elephant-Goldfish
Model). The Design switch in the composer, on Home or in a thread, turns it on
(`POST /api/threads/{id}/design`, or `design: true` in `POST /api/threads`); each switch is a
`design` event `{ on, by }`, and the last one says whether the mode is on. The first message after
a switch carries Hub's note to the agent (`design::ON_NOTE` or `OFF_NOTE`), the way clashes are
told: talk the feature through with the user, then write one design document under
`docs/design/` with the problem, the plan, rejected alternatives, every file to change with its
types and signatures, tests and open questions, and change nothing else.

While the mode is on, Hub's permission answers (`permit::design`, before Vakten's and the user's
settings) allow edits under `docs/design/` and refuse every other edit and any command
`permit::report_only` would call a write (commits, pushes, installs, redirects). Only questions the
engine asks come past Hub, so an engine whose own mode edits without asking is not stopped; after
such a turn `Hub::design_after_turn` logs `{ strayed: [paths] }` and the thread shows a warning.
The switch, that check and the review are `Hub` methods in `hub/design.rs`; `design.rs` keeps the
notes, the checks and the readers of the log.

Review design in the composer (or Hub's quiet `review_design` tool, which the note tells the agent
to call when the document is ready) starts four fresh sessions in parallel on the thread's engine,
each read-only (`engine::complete`, as a second opinion) and given only the document, never the
conversation: one explains it back and lists where it had to guess (told not to read other
files), one looks for gaps and marks them Blocking or Minor, one checks it against the code, and one
says "Ready to build" or "Not ready". The gap and readiness checks use the second opinion's
reviewer model, the other two the engine's default. Each answers with a verdict line first. The
review is one `design` event id, logged as each check ends; the thread shows it as one line that
opens to the four (`DesignCard.svelte`). Unless every check found nothing, the findings go to the
agent as a message marked `design: id`, telling it to settle them with the user and not to write
code. The document is the one named, else the last the thread wrote under `docs/design/`, else
the newest there. Turning the mode off lets the agent build from the document.

## What a thread's tokens went to
<!-- covers: crates/hub-core/src/usage_detail.rs, app/src/lib/components/UsagePanel.svelte -->

ACP's `usage_update` gives only a total, so What used it in a thread's context box
(`UsagePanel.svelte`, `GET /api/threads/{id}/context`) reads the engine's own session log
(`usage_detail.rs`). Where that log is comes from the engine trait `session_log`, a pattern like
`~/.claude/projects/*/{session}.jsonl` (set in the claude preset, under `CLAUDE_CONFIG_DIR` when
that is set), so this file has no engine-specific code (engine knowledge lives only in the presets and the files CLAUDE.md names); an engine without one says only the total
is known. The log is read as Messages-style JSONL: the context of the first answer is the setup
(system prompt, tool definitions, rules), the latest is the context now, answers' usage sums to
what was spent (fresh, cache, out), and `tool_use`/`tool_result` blocks are counted per tool and
per MCP server (`mcp__<server>__<tool>`), in tokens estimated at about four characters each.

## What Hub sends to an AI
<!-- covers: crates/hub-core/src/package/, crates/hub-core/src/engine/mod.rs, crates/hub-core/src/runtime.rs, crates/hub-core/src/moving.rs, app/src/lib/components/SlipView.svelte, app/src/lib/components/HandoverHint.svelte -->

Hub writes much of what models read: the notes it puts before your message, the conversation it
hands a fresh session after a model switch, a rewind or a move, the messages it sends in its own
name (`review`, `quota`, `proof`...), the one-shot questions behind titles, commit messages and
reviews, and the decisions it asks Jev for. `crates/hub-core/src/package/` is where that is
counted and, in steps, built. The plan (sources that deliver one fact each, recipes per purpose
with a token budget per source, a renderer per target model, a slip for everything) is in the
Claude Doc "Paketsystemet" and goes in as it is built. What exists now is the slip.

- **A slip per package** (`Slip` in `model.rs`, `package/ledger.rs`; "packing slip" on screen, since "receipt" is
  what Aftermath's hash-checked records are called and the chat has its own `Receipt` view): what it was for
  (`purpose`: `message`, `switch`, `rewind`, `moved`, `resend`, `hub:<mark>`, or a one-shot's name
  such as `gist`, `tidy`, `judge_answer`), the engine and model that got it, the tokens of the
  whole and of each part (`you`, `handover`, `rolled_back`, `clash`, `design`, `lost_session`,
  `hub`, `prompt` for a one-shot not yet split, `other` for what the parts leave), the thread and
  the seq of the user message it answers, the engine `session` it went into (the key that joins it
  to the engine's own token count and to Aftermath), and the `recipe` that built it (`legacy` until recipes
  exist). Tokens are estimated at about four characters each and marked `estimated`.
- **No text is kept.** A slip points at its sources: the thread's log rebuilds a conversation
  that was handed over (`conversation_log` below the seq), and a one-shot keeps only its first 160
  characters. Nothing is stored twice, and a model's own session history is not copied.
- **Written before it goes.** `send_inner` (`hub.rs`) records what it added in front of the words
  as the message is built; the turn loop (`engine/acp.rs`) writes the slip just before the
  prompt reaches the engine, so one exists when the turn fails. A message that waited behind a
  turn and was edited is measured as it finally went. Every `engine::complete` and
  `engine::acp::complete` call names its purpose as its first argument, so nothing is sent
  unlabelled (the compiler checks it).
- **A work name for Aftermath.** A development thread starts its engine with
  `AFTERMATH_WORK_REF=work://hub/<thread id>` (`runtime::work_ref`), so Aftermath's own hooks can say
  which work a session belonged to. An everyday chat gets none: it is not followed up, and its
  lean start does not run those hooks anyway. Nothing about the work is stored in Hub.
- **Never in the way.** Recording only puts the slip on a channel; a task started in
  `Hub::new` writes them to the `slips` table (schema step 16), a few at a time, and clears
  those older than 90 days. A thread's message uses its own hub's channel; the one-shots, which
  have no hub in reach, use one shared slot (`ledger::share`), which is the single hub of a
  running core. Without an async runtime a slip is dropped.
- **First recipe: `handover-v1`.** The conversation a fresh session is handed after a model switch, a
  rewind, a move or a lost session (`handover_log` in `hub.rs`) is no longer up to 60 000 characters of everything: it
  is the start (a quarter of the budget, which says what the work is) and the latest part (the rest,
  which says where it stands), about 24 000 characters or 6 000 tokens in all. The turns between are
  named by place, not carried: "N turns in between are left out; `read_thread` with thread_id and
  after_seq reads them", so the new agent fetches them when the message needs them. Under that line Hub lists what the person asked in the left-out turns, one line of at most 160 characters each and 2 400 in all ("and N more" past that), so the new agent knows where the gap went without reading it (`left_out_asks`). A thread that
  fits is handed over whole and its slip says `legacy`; a cut one says `handover-v1`
  (`Prompt.recipe` to `Slip.recipe`). Tool calls still carry only their title, never what they
  printed. Gists and judges still use `conversation_log`.
- **Calls** (feature `package.ledger`): `GET /api/package/slips?thread=&purpose=&session=&before=&limit=`
  (newest first, at most 500) and `GET /api/package/summary?days=` (per day and purpose: calls and
  tokens, in the time zone of the day views).
- **On screen: the line under a model switch.** `HandoverHint.svelte` (used by `Transcript.svelte`)
  says "Hub handed it the conversation so far" and opens in the row into `SlipView.svelte`: one
  line, "N messages and the name of M tools went along; left out: what the tools printed, N attached
  files". The counts come from the thread's own events (`carriedBy` in `transcript.ts`), so
  they show on any Hub; the size and the part list come from the slip once the running core has
  `package.ledger` (`api.packageSlips`, the last `switch` slip at or after the switch). There is no "show text": the line says what went
  along and what was left out, which is all it is for.
  `SlipView` is pure display (`{slip?, carried?}`), so Usage can show the same row for any slip.
