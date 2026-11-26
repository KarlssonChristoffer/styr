# Architecture
<!-- för: agent -->

```
 Surfaces    Tauri app (Svelte) · browser · later phone, CLI, voice
                  │  HTTP + WebSocket on 127.0.0.1:7878
 Core        hubd: threads · event log (SQLite) · host inventory
                  │  ACP over stdio, one child process per active thread
 Engines     Claude Code (claude-agent-acp) · Codex (codex-acp) · anything that speaks ACP
```

## Pieces
<!-- covers: crates/hub-core/src/lib.rs, crates/hub-core/src/hub.rs, crates/hub-core/src/hub/, crates/hub-core/src/model.rs, crates/hub-core/src/model/, crates/hubd/src/main.rs, crates/hub-install/src/lib.rs, crates/hub-launcher/src/main.rs, crates/mock-agent/src/lib.rs, crates/mock-agent/src/main.rs, app/src/main.ts, app/src/App.svelte -->

| Path | What it is |
|---|---|
| `crates/hub-core` | The core as a library: model (`model.rs`, the source of the API types and the TypeScript bindings), store, thread runtime (`hub.rs`, one `impl Hub` block per feature file), ACP engine, host scan, API. |
| `crates/hubd` | The daemon binary. Loads config, opens the store, serves the API; also the subcommands (`engine`, `pair`, `sign-in`, `attach`, `sim-agent`). |
| `crates/hub-install` | The layout of an installed Hub (data folder, `bin/<version>/`, `current`), shared by the core, the app and the launcher. |
| `crates/hub-launcher` | What the Start menu points at: starts the version `current` names and exits, so it is never running when the updater replaces it. |
| `crates/mock-agent` | A scripted ACP agent for tests and for trying the UI for free. |
| `app/` | The Svelte UI (`App.svelte` is the shell, `main.ts` mounts it; `docs/APP.md` maps its files). `app/src-tauri` is the desktop shell. |
| `design/` | Design references from Claude Design and the briefs that produced them. |

## Decisions
<!-- covers: crates/hub-core/src/engines.rs, crates/hub-core/src/engines/, crates/hub-core/src/engine_share.rs, crates/hub-core/src/engine_probe.rs, crates/hub-core/src/routes.rs, crates/hub-core/src/harness_settings.rs, crates/hub-core/src/harness_settings/, app/src/lib/components/HarnessSettings.svelte, app/src/lib/components/EngineList.svelte, crates/hub-core/src/live_output.rs, app/src/lib/transcript.ts, app/src/lib/transcriptItems.ts, app/src/lib/transcriptText.ts, app/src/lib/transcriptTools.ts, app/src/lib/activity.ts, app/src/lib/web.ts, app/src/lib/ansi.ts, app/src/lib/components/StatusLine.svelte, app/src/lib/components/Transcript.svelte, app/src/lib/components/UserMessage.svelte, app/src/lib/components/RewoundNote.svelte, app/src/lib/components/ActivityRow.svelte, app/src/lib/components/WebRow.svelte, app/src/lib/transcriptWords.ts, app/src/lib/workGrid.ts, app/src/lib/components/WorkGrid.svelte, app/src/lib/prefs.svelte.ts, app/src/lib/components/ThreadPanel.svelte, app/src/App.svelte, app/src/lib/steps.ts, app/src/lib/components/Steps.svelte -->

- **Engine warnings are notices, not reply text.** Hub advertises the ACP `session.notices`
  capability, so an engine such as codex-acp sends its warnings ("Exceeded skills context
  budget…") as a `notice` update instead of text in the agent's reply. The log keeps it as an
  `update`; the chat shows it once as a quiet line, with a link to Settings > Extensions for the
  skills budget.
- **Engines never inherit a pay-per-token key.** An `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` and the
  like in hubd's own environment would silently move Claude Code or Codex off the subscription.
  When an engine starts, those names are blanked unless Hub sets one on purpose (the engine's
  `env`, a vault key it names, a project's API connection); the core logs which were kept out
  (`engines::inherited_paid_keys`, used in `engine/acp.rs`). Hub's own sign-in token stays.
- **The core is a separate process.** Threads keep running when the window closes, and the
  same core can later move to Plåtnicklas without changing the app.
- **Engines are a harness on a provider.** Settings > Engines (`EngineList.svelte`) adds one from a catalog
  (`engines.rs`, data only): a harness is the ACP agent (Claude Code, Codex, Hermes, OpenCode,
  Gemini CLI, or any command), a provider is where its model comes from (its own login, or a key
  such as DeepSeek's or OpenRouter's). Adding writes one plain entry to `hub.json`: `command`,
  the key's name in `secrets` (value in the vault), the start `model` (picked by name when a
  session starts, unless the thread picked one), non-secret `env` the harness reads its provider
  and model from, and `label`/`harness`/`provider` for the page. The core runs it like any other
  engine. Trying one starts the agent and asks for a session without a prompt, so it spends
  nothing. Provider keys never include `apis::ENGINE_KEYS`, so Claude and Codex stay on
  subscriptions. An engine is added once, on the machine whose app you use: every minute (and at
  once after Add or Remove) that hubd posts its engines and their keys to every other hubd
  (`engine_share::share`, `POST /api/engines/receive`). The receiver stores the keys in its vault,
  writes the engines with `shared_from` set, starts the harness by its own path, removes the
  ones the sender dropped, and restarts its core when anything changed. Received engines are
  never sent on, and an engine added on that machine by hand under the same name stays. Each
  machine answers whether the harness is installed there, and the page offers Install via
  `?host=`. A machine also tries each engine it was sent, as a new thread there would start it
  (the provider it would pick, with its own copy of the keys: `engine_share::check_shared`, at most
  every six hours or when the engine changes), and the page says "ready" only after that try
  worked; an older hubd that cannot try says so. An engine added there by hand under the same
  name is tried too. Each try ends within three minutes, ok or not, and a try that fails or
  panics never leaves the engine "being tried" (`EngineHere.checking`); the page says "trying it
  there…" only while a try really runs, and "waits to be tried there" otherwise. Each machine is
  a lamp under its short name, with its answer in the lamp's tip; only a machine that is not
  ready gets words under the row (with Install where that fits). A provider
  whose key works shows only its Works stamp (its models in the tip); only a failure gets words.
  The rack and the knobs above it are described under "New threads and how agents work".
- **A harness's threads pick their provider** (`routes.rs`). A harness that runs on many
  providers (Hermes, OpenCode) is added once, and a provider is just a key in the vault and the
  catalog's models. When such an engine starts, Hub offers one model setting of its own,
  `hub_route`, grouped by provider (each provider it runs on that has a key; the harness's own
  setup only when there is none, since Hub cannot see what that setup is and Hermes's often sends
  its default model to a provider that refuses it), in place of the agent's model setting. A provider's models are what its API lists
  with the key (asked at most hourly, `live_models`; catalog names when it does not answer), so
  an old name such as `deepseek-chat` gives way to one it takes now. The pick is kept like any
  setting; the agent starts with that provider's `env` and key, and is then sent ACP's
  `session/set_model` with `provider:model`, which is how Hermes takes a model (it ignores
  `HERMES_MODEL` over ACP and would send its own default to the provider). Picking another stops the agent and starts it
  again on the new one, loading the same session. When an agent cannot load its session, the
  next message carries the conversation so far, as when another agent takes over. Like T3 Code,
  models come from the agent when it lists them; unlike T3 Code, one harness is one entry in the
  picker rather than one per provider. The pick stays in place of the agent's model setting
  however often the agent lists its settings again (`SessionState::keep_route`, and the same in
  `session.ts` for `config_option_update`), and an agent's modes still show beside it: Hermes
  lists its edit modes (Default, Accept Edits, Don't Ask) apart from its settings, which the
  composer used to drop once Hub's pick was among them.
  **Brand colour in the model picker** (`ModelPicker.svelte`, `prefs.pickerTheme`, colours in
  `cardSwitch.ts` `BRAND_RGB`): the picker takes the colours of the provider tab you are on, the
  same clay/sand for Claude and sky/iris for Codex as the limits card. It is only a layer behind
  the list, so the stars and the tick stay in their columns. Favorites and search keep the colour
  of the thread's own provider. Settings > Appearance picks Edge (default), Glow, Quiet or Off.
  **One tab per provider, a head per harness** (`tabsOf` in `providers.svelte.ts`, `modelRows` in
  `modelRows.ts`): engines are grouped by the provider they stand for, so DeepSeek in Hermes and
  in Claude Code is one DeepSeek tab with a "Hermes" head and a "Claude Code" head over their models,
  and one row picks model and harness together (an engine switch, as before). Nothing is shown where
  there is no choice: Claude and OpenAI have one harness, so no head, tag or lock. Favorites and
  search tag a row with its harness only on such shared tabs, and the model pill reads
  `deepseek-v4-pro · Hermes`. The Provider menu lists one row per tab. In a thread, the Provider menu and the picker's footer warn before a switch (amber "New session."): the new model gets the start and the latest part of the thread, not what tools printed or attached files. An engine on a provider's
  key that has not run here yet lists that provider's models from the catalog (`modelsFor`), so its
  head is there before its first start. Settings > Engines > Add harness offers a harness paired
  with a provider you have a key for (`pairable`, `engines.pairs`).
  The list leads with the newest model of each current family, strongest first (`splitModels` in
  `session.ts`): Fable, Opus, Sonnet, Haiku for Claude, Astra, Sol, Terra, Luna for OpenAI. Older
  models stay under More models.
- **A harness paired with a provider** (`Harness.pairs`, `routes::harness`). The catalog is a matrix
  of which harness runs which provider: `providers` are the ones a harness's threads pick among
  (Hermes, OpenCode), `pairs` are the ones it runs on only as an engine of its own, added by hand
  next to its login. Today that is Claude Code on DeepSeek's key (`claude-deepseek`, "Claude Code ·
  DeepSeek"): Claude Code speaks Anthropic's API, which DeepSeek offers at `anthropic_url`, so the
  engine's environment points `ANTHROPIC_BASE_URL` there, names the model for every size Claude
  Code asks for (`ANTHROPIC_MODEL`, the Opus, Sonnet and Haiku defaults, the subagent model), and the
  key goes in `ANTHROPIC_AUTH_TOKEN` by its vault name (`Harness.key_env`; the vault itself
  never holds that name). It is a route engine too, so the picker lists DeepSeek's models for it
  and another one restarts it with another `ANTHROPIC_MODEL`; Hub does not also send
  `session/set_model` (`Harness.set_model`, Hermes only), since Claude Code does not list those
  names. It has no "own setup" (that would be the subscription under DeepSeek's name), no
  limit phrases (so `Config::is_subscription` is false and a thread never moves to it when a plan
  limit runs out; failover stays between subscriptions) and no terminal command (the CLI alone
  would lack the address and key, so Take over is off for it). Claude, OpenAI and Gemini stay in
  their own harness and are never in `pairs`. A client asks for it as the `engines.pairs` feature.
- **Thinking follows the provider.** A provider in the catalog may name the thinking levels its
  models really have (`Provider.efforts`; DeepSeek: Off, Low, High, Max, default High). When a
  thread runs on it through Hub's pick and the harness offers a thinking setting (newer Hermes's
  `reasoning_effort`, nine levels), the composer shows only those levels under the provider's
  names. A harness that offers none per thread (Hermes 0.22) still gets the control: it changes
  the harness's own default for all its threads (the setting marked `thinking` below), and says
  so, pointing to Settings › Updates for a Hermes that lets each thread pick.
- **A harness's own settings are changed through its own program** (`harness_settings.rs`).
  Settings › Engines › Settings on a harness's row lists what the catalog there names (Hermes:
  thinking default, tools as `agent.disabled_toolsets`, memory, profile, asking before memory or
  skills are written, compression and its threshold, most steps per message), read with the
  harness's `config get <section> --json` and written with `config set`/`unset`, so Hub never
  parses or rewrites its files. A value changed from Hub is kept in store kv `harness_settings`
  and sent to every other hubd with the engines (`Shipment.settings`, ignored by an older hubd),
  where `harness_settings::take_in` sets it when it differs there.
- **Engines are ACP agents.** ACP (Agent Client Protocol) is an open standard for driving
  coding agents. Claude Code and Codex have adapters, so Hub drives them the same way and uses
  the subscription the adapter is logged in with. An engine is a command in the config, plus
  traits for what ACP does not say (`EngineTraits` in `config.rs`): the prompt that reads its
  plan limits (`usage_probe`, Claude's `/usage`; unset, Hub never asks), how its limit message
  reads (`limit_phrases`), the rules file it reads (`rules_file`, `CLAUDE.md` or `AGENTS.md`),
  its models for second opinions (`review_models`), its API prices (`prices`) and whether it shows MCP instructions to the model (`mcp_instructions`; one that does not gets Hub's in its first message, see "Live pages") and whether to hand it Hub's tools although it does not say it takes an MCP server over HTTP (`mcp_http`; Hermes takes one and never says so, so without it `show_page`, the picture side job and `page_check` never reached DeepSeek) and whether the thread's tool token also goes in the address (`mcp_token_in_url`; Hermes drops the headers of an MCP server, so it was refused Hub's tools and Guard's thread ran raw shell calls instead; `guard::check` reads `?token=` and still only opens that thread's tools). The core reads
  these and never the engine's name; Claude's and Codex's are built-in presets by name, so a
  config written before traits existed works as it did. Plan limits, the quota guard and carrying
  on after a limit only apply to threads on the engine the limits are read from (the default
  engine), and a failed read waits longer each time before the next.
- **One append-only event log.** Every user message, streamed update, permission question and
  answer, turn end and error is an `Event { thread_id, seq, ts, kind, data }`. ACP updates are
  stored as they arrive, so nothing is lost by translating them. Surfaces fold events into what
  they show (`app/src/lib/transcript.ts`, with its types in `transcriptItems.ts`, text readers in `transcriptText.ts`, tool-call readers in `transcriptTools.ts`). This is the record Aftermath-style views read later.
  Image blocks an engine sends, in a reply or in a tool call's content, become pictures in the
  reply that stay out of the folded work (`ImageCarousel.svelte`), bare as in Claude Code: one is
  shown big, two or three sit in a row at one height, more than three share one place with arrows
  and a strip of the others. A click opens a picture over the whole window (`Lightbox.svelte`,
  `openPicture` in `lightbox.svelte.ts`). `page_check`'s screenshot is left out: it is the agent
  looking at its own page, which the reply already holds. That is how Codex's image generation shows up: codex-acp
  reports it as an "Image generation" tool call carrying the PNG and the path Codex saved it to,
  and it costs the ChatGPT plan, not an API key.
  Command output comes the same way. Hub declares `_meta.terminal_output_delta` (the Zed
  convention both adapters read), so a command's output arrives as chunks under its tool call's
  `_meta`, then `terminal_exit` with the exit code: Codex streams them while the command runs,
  Claude's adapter sends the output in one chunk when it ends, so for Claude Hub reads it itself
  (`live_output.rs`): engines get `CLAUDE_CODE_SHELL_PREFIX` pointing at `hub-shell`, a link to
  hubd in `<data dir>/bin`. Claude Code runs each Bash command as `hub-shell '<script>'` with its
  output going to a file; the wrapper notes that file and runs the script in the same shell, its
  streams and exit code untouched. The core reads the file while the tool call runs and logs
  `output` events on it, which the engine's own output replaces at the end. Hooks and MCP servers
  also pass through the prefix and run as they are. `"live_output": false` in `hub.json` turns it off. The fold keeps the last 256 KB of each command
  (`ToolItem.output`); a running command shows its last lines under the activity line, and the
  Running section of the thread's side column (`ThreadPanel.svelte`, Ctrl §) shows every command:
  running and failed ones as rows that unfold to their output, finished ones behind Show N finished.
  The column is wider than other pages' panels (`.right.column` in `App.svelte`: 340 to 560 px, times
  the text size) but never takes more than the window less 720 px, so the sidebar and chat keep room
  near the 1100 px breakpoint. Opening the same command twice (`showCommand`) unfolds it again. `ansi.ts` turns colours into the theme's state colours and lets `\r` redraw a line.
  Each tool row in an opened activity line has a grey glyph for its kind (`glyph()` in
  `activity.ts`), its time, `exit N` for a command (red when not 0) and, under it, the last line
  it printed (or a failed tool's error). While a thread works, each live signal is said once: `StatusLine.svelte` above the
  composer is the turn's one clock ("Working 1m 35s", counted from the later of the running turn's first message and `thread.sent_at`, the same start the activity list counts from, so a message that waits behind an older turn does not leave the two clocks 17 hours apart; with the working grid, `WorkGrid.svelte`, in
  place of a ring; an amber ring when it waits on you), with Output (the terminal drawer) while a
  command runs and Esc to stop; the transcript's live activity line says what it does now, with the
  working grid in front, standing still, and a running command's last lines under it (a running
  tool's row has a small still grid too): one grid moves per thread view, the status line's; the fold row says what is done so far ("So far · 1 note,
  read 1 file") without a clock or pulse, and "Worked for … · …" once the turn ends. Receipts on
  your message are ticks only (words for Stopped and Failed): one tick as soon as Hub has the
  message and it is next (`got`), two once the agent starts on it; while the engine is still
  starting, the status line says "Starting <engine>" in place of "Working" and its clock counts
  from the send. A side job's line pulses green
  like anything working. The status line is built from the fold alone (`now()` in `activity.ts`),
  so it needs nothing from the core.
  **The steps card** (`Steps.svelte`, `steps.ts`) shows the plan the agent keeps (an ACP `plan` update.
  Nothing in the logs of 2026-10-29 to 11-04 held one: Claude Code does not offer its to-do tools on
  the newer models and Codex and Hermes never called theirs, so every thread has Hub's own
  `update_steps` tool, below, that writes the same update; `transcript.ts` keeps one plan item per turn, updated in
  place, with when each step started and ended). While a turn runs it sits above the live step: a grey
  check for a step done (with its time when it took a second or more), the working grid on the step
  now, a hollow ring for the next; a click on its head folds it to the step now, and a plan of
  more than six steps folds the finished ones away but the last. It holds the thread's one moving
  grid: while it is up the live activity line, a running tool's row and the status line show no grid
  (`stepsUp` in `Transcript.svelte`, `stepsShown` in `steps.ts`), and with no step now the grid sits
  in the card's head. In Full there is no card (it shows the plan where it was made); Minimal shows the plan as one
  line in place of the word ("Step 2 of 4 · Draw the flow", `stepsLine()` in `steps.ts`), and Quiet
  drops the "So far" fold line above the card, since the card says how far it is. When the turn ends the plan goes into the fold ("Worked for 24s · 3 steps")
  and shows there as the same list without the grid.
  **Chat detail** says how much of that work a thread shows, in four levels (`WorkDetail` in
  `prefs.svelte.ts`): Minimal (one playful word, `playfulWord()`, with the working grid while it works: System's 2 by 2 icon playing one of its loops, Pour by default (three filled tiles and a gap, one tile pouring into the gap cell by cell over a finer 4 by 4 grid inside each tile and whole again where it lands, a step every 1.3 s), or the one chosen in Settings > Transcript, or Random for a new one each turn, `workGrid.ts` and `WorkGrid.svelte`;
  it changes with every new step, not on a timer, so a word that stands still means the turn does;
  the status line then keeps only the clock and Esc; a finished turn is "Worked for 1m 12s · 3
  steps · 3 files changed" (`workedQuietly()`, Quiet says the same) and the answer, with no file chips under it), Quiet (the default: one line of what
  it does now, no command output), Normal (also its latest note and a running command's last
  lines) and Full (`all`: nothing folds, activity lines are open and each step shows its diff or
  output). Needs-you cards and failures show at every level. The default is the window pref
  `chatDetail` (Settings > Chat and threads); a thread gets its own in `threadDetail` through
  "Chat detail" in its ··· menu (a submenu like Snooze, `detailMenu()` in `menus.svelte.ts`: the
  four levels with a tick, and "Follow the default" to take it back), and `detailOf(threadId)` is what `Transcript.svelte` asks. Both are
  window prefs, like the other chat prefs, so nothing in the core or the log changes.
- **A web search shows as one.** The globe in the composer (`Composer.svelte`, in any Dev thread; not in the Chat world, where a chat may always use the internet and has no button) sends
  your message with Hub's instruction to search the web for it (`webAsk()` in `web.ts`); the same
  words as the phone's Search (`Web.prefix` in `ios/Hub/Web.swift`), so each shows the other's. The
  instruction asks the agent to open and read the pages it relies on, not to answer from search
  snippets (Haiku otherwise stops at the snippets); older wordings are still read as searches
  (`EARLIER`). A thread whose first message was a web search starts its box with the globe on
  (`startedAsWeb()`, `webStart`), so follow-ups search too until you turn it off. The
  bubble shows only your words with a globe and "Web". The agent's web steps (ACP kind `fetch`,
  or a title that says web search) group into their own activity row (`group()` marks it `web`):
  a turning globe and "Searching the web…" while it looks, "Searched the web · 3 pages · 6s" after,
  and opened, each query in quotes and the pages it found or read (Claude's search results list
  them as `{"title", "url"}`). In a turn you asked as a web search that row stays out of the fold
  and the answer's links show as source cards under it (`webAnswer()`), the list of links it ends
  on taken out of the text; in other turns the row folds like any work and the answer is left as is.
- **SQLite in WAL mode**, accessed on the blocking pool. One file to back up.
- **Live by default.** The WebSocket pushes every thread change and event. A client that falls
  behind gets `resync` and refetches; a client that reconnects catches up from the log by `seq`.
  A thread that a request answers with (marking it seen, a refetch) is taken only when it is at
  least as new (`updated_at`) as what the stream has brought: a turn can start and end while an
  answer is on its way, and the older snapshot would leave the thread shown as Running for good
  (`upsert` in `threadActions.svelte.ts`, `newest` in `state.svelte.ts`).
  A stream can go quiet without closing, and a covered window pauses animation frames, so the
  window guards itself (`start` and `flush` in `state.svelte.ts`): the events of a burst are shown
  by a 250 ms timer as well as by the next frame, an open thread that runs with nothing heard for
  15 s reads its log, and a window looked at again after 5 s away refetches everything.
- **Shared types.** The Rust model generates TypeScript (`ts-rs`), so the API has one
  definition.

## Where each section lives

The rest of this document is split by area (each file stays under 800 lines). Section names are unchanged; find the file here.

| Section | File |
|---|---|
| Thread lifecycle, Restarts, Panic button, Activity list, Back and forward, Small helpers, Undo, Second opinion, Design mode, What a thread's tokens went to, What Hub sends to an AI | `docs/architecture/threads.md` |
| Where it goes, Quota guard, Proof before done, leak guard, clash warnings, Customs, Attempts, Hub's tools, Live pages, Viewkit, Handoffs, Side jobs | `docs/architecture/guards.md` |
| Problems, Routines, System threads, Recipes, What the models can do, New threads and how agents work, Auto (taken out), Key vault, API connections, The viewer, The context bag, Welcome, Labs | `docs/architecture/problems.md` |
| Overview, Home and New thread, Cloud threads, Dev mode, Starting programs without a console window, Clean-up after agent work | `docs/architecture/overview.md` |
| Projects, Plugins, Library, GitHub | `docs/architecture/projects.md` |
| Taking a thread over, Starting and stopping a project, Shared UI pieces, Reactions, Things named in replies, Rail and rooms, Search (Ctrl K), Status bar and panel, Zoom and text size, Fonts and the chat's look, The Machines page, Terminals inside Hub, The simulator view, Inspect mode | `docs/architecture/ui.md` |
| Other machines, Thread status from events, What a thread shows: project and display word, Versions that talk to each other, API, Staying up, The settings file, Logs and backups | `docs/architecture/network.md` |
| Self-update, Security | `docs/architecture/update.md` |
| The phone, Smaller pieces, Known limits | `docs/architecture/phone.md` |
| Bench | `docs/architecture/bench.md` |
