# Architecture: Taking over, shared UI, rail, status bar, terminals, simulator, inspect
<!-- för: agent -->

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## Taking a thread over
<!-- covers: crates/hub-core/src/terminal.rs, crates/hub-core/src/hub/terminal.rs, app/src/lib/terminal.ts -->

Engines speak ACP over stdio, so there is no screen to attach to while Hub drives one. Taking over
is a handover instead (`crates/hub-core/src/terminal.rs`; the steps the core takes are `Hub`'s, in
`hub/terminal.rs`):

1. `hubd attach <thread>` (any terminal, also over SSH; `hubd threads` lists the short ids) follows
   the thread live from the log. Enter, or `--now`, asks the core to let go: it stops the turn and
   the engine, logs `terminal` `{ state: "taken", pid, command, cut? }` (`cut`: the user message
   whose turn it stopped, if one was running) and sets the thread to `in_terminal`, where Hub
   refuses messages and rollbacks. With `--if-idle` it takes over at once only when the thread is
   not working; while a turn runs it just shows it, and the hint line says Enter would stop it.
2. `attach` runs the engine's own CLI on the session, `terminal` in the engine's config with the
   session id for `{session}` (`claude --resume {session}`, `codex resume {session}`), in the
   thread's folder with `HUB_THREAD_ID` and `HUB_PROJECT`, so what it starts still belongs to the
   project.
   A session nobody has prompted yet has no conversation for the CLI to resume (`claude --resume`
   answers "No conversation found"): one started ahead of the first message, or fresh after a move
   or a switch of engine. The core knows it from the log (`session_prompts`, 0 prompts since the
   session started) and from the engine's `session_log` when it has one. Such a thread is taken
   over with `terminal_new` in the engine's traits (`claude --session-id {session}`: a new
   conversation under the id Hub holds, so hand back loads it); `Takeover.fresh` makes `attach` say
   that the CLI starts empty. An engine without `terminal_new` says to send a message first. An
   engine saved without `terminal` falls back to its harness's (`hermes --resume {session}`), and
   `Takeover.env` (the engine's own `env`, never keys) is set on the CLI so it runs on the provider
   and model the thread uses.
3. When the CLI exits, `attach` hands the thread back (retrying while the core restarts), logs
   `back`, and the core starts the engine at once. Loading the session replays it; the replay is
   kept this time, and every prompt after the ones the log already has (counted from the log:
   messages sent to this session, or the previous import's `replay_turns`) is copied in as `update`
   events, followed by `imported` `{ turns, replay_turns, carry_on? }`. The thread shows them as
   messages "in the terminal".
4. A turn that taking over stopped (`cut`) is carried on: when nothing was said in the terminal, or
   the last thing said there was the CLI's own "[Request interrupted…" note, Hub logs `carry_on:
   true` and sends `terminal::CARRY_ON_TERMINAL` as a message marked `terminal: <seq of taken>`
   (folded like a restart's), once per takeover and only when the same session loaded and
   `resume_turns` is on. A request the terminal saw through is left alone
   (`terminal::carry_on_after_terminal`).

The thread's Take over button runs `hubd attach <thread> --if-idle` in a terminal inside Hub
(below) and opens the thread's side column at it, so a running turn goes on and can be watched there, under Terminal (`app/src/lib/terminal.ts`,
`ThreadPanel.svelte`): `POST /api/hosts/{name}/terminals` with `{ what: "attach", thread }` on the
thread's machine. This computer runs its own hubd; a thread on another machine is reached over its
SSH alias, `ssh -t <alias> 'bash -lc … exec <its hubd> attach <thread> --if-idle'`, so the CLI runs
there in the thread's folder (`machine::attach_command`). The terminal carries the thread's id
(`TerminalSession.thread`), so a press while one still runs shows that one again and the side
column finds it after the app reloads. Take back closes it first, which ends the CLI in it. When
the CLI exits by itself, `attach` hands back and goes on following; Close ends it. A machine Hub
reaches with no SSH alias falls back to the old way below, as does "Take over in a window" in the
thread's menu.

Where tmux is installed (and `tmux` is on in `hub.json`, the default), every thread whose engine
starts gets a detached tmux session `hub-<last 8 hex of its id>` running `hubd attach`, so `tmux
attach -t hub-…` reaches any of them from anywhere, and under systemd they live in a scope of their
own so a restart of the core does not end them. On Windows "Take over" opens Windows Terminal (or a
console) running `hubd attach --if-idle`. The window's title is the thread's title with only plain
characters kept (`machine::window_title`): the console falls back to `cmd /C start "<title>"`, and
cmd runs what follows a `"` or an `&` even inside Rust's quoting, while the agent writes the title.
For the same reason a folder with one of cmd's characters is not opened through cmd
(`machine::cmd_safe`). Outside Hub, a thread on another machine is taken
over there; the app copies `ssh -t <alias> '…'`.

## Starting and stopping a project
<!-- covers: crates/hub-core/src/services.rs, crates/hub-core/src/hub/services.rs -->

What a project starts is its `Procfile.dev` or `Procfile`, one `name: command` per line, the format
foreman, honcho and overmind read (`crates/hub-core/src/services.rs`). Without one Hub guesses
from the manifests (`npm run dev` in the folder or a folder under it, `cargo run`, `manage.py`) and
can save the guess as `Procfile.dev`. A compose file is part of it too.

Start all runs `docker compose up -d` and every service that is not running, each through the
shell in the project folder with `HUB_PROJECT` and `HUB_SERVICE` set, outside the core's process
group (and systemd scope) so it outlives updates. Hub finds services again by those variables, not
by remembering pids. Output goes to `<data dir>/services/<folder>-<hash>/<name>.log`, emptied at each
start and when it passes 20 MB. Stop all stops the services with their children, runs `docker
compose stop`, shuts down the engines of the project's threads and stops what they started (that
part needs the running Hub, so it is `hub/services.rs`; the rest takes the folder and data folder).
Threads open in a terminal and processes started by hand are left alone.

## Shared UI pieces
<!-- covers: app/src/lib/components/Select.svelte, app/src/lib/components/ContextMenu.svelte, app/src/lib/components/ResizeEdge.svelte, app/src/lib/components/Break.svelte, app/src/lib/components/Quiet.svelte, app/src/lib/visits.ts, app/src/lib/components/Working.svelte, app/src/lib/components/IconSwap.svelte, app/src/lib/components/Tabs.svelte, app/src/lib/workClock.svelte.ts, app/src/lib/poll.ts, app/src/lib/resource.ts, app/src/lib/quiet.ts, app/src/lib/autofocus.ts, app/src/lib/lastSeen.ts, app/src/app.css -->

Tab strips are `Tabs.svelte` (items with id, label, optional count; the underline slides; see "Tabs" in `design/DESIGN.md`), never a row of hand-made buttons.

A field that should have the cursor when it appears uses `use:autofocus` from `autofocus.ts` (`use:autofocus={cond}` for a condition), never the `autofocus` attribute. A value a component only reads once at the start (`$state(prop)`, `new Tween(prop)`) is wrapped in `untrack(() => prop)` instead of a `svelte-ignore`.

Anything that repeats on a timer (a refresh, a clock tick) is `poll(fn, ms)` from `poll.ts`, not `setInterval`: it rests while the window is hidden and runs once at once when the window is seen again, and returns the function that stops it. A plain `setInterval` is only for work that must run unseen: the live heartbeat, installing an update while you are away (`autoInstall.ts`) and holding the voice-chat mute key while you dictate (`composerVoice.svelte.ts`). A checkbox is `Switch.svelte`; a hidden file input behind a button, the opacity slider and the snooze and routine time fields are still the browser's (no Hub piece yet).

Something read from hubd and kept fresh is `resource(key, fetch, { poll, invalidateOn })` from `resource.ts`, not a page's own `load()` plus `poll()`: callers with the same key share one value and one fetch, it polls while anyone is subscribed, reads again on a stream message `invalidateOn` names (and always on `resync`; `Stream.apply` calls `notifyResources`), and a component reads `$r.data`, `$r.error`, `$r.loading` and calls `r.refresh()` after a change. A page moves onto it when it is touched anyway; `RecipesView.svelte` is the first.

A call that may fail without telling the person (a refresh, a nice-to-have) is `quiet(promise, "what", fallback?)` from `quiet.ts`, not a bare `.catch(() => {})`: it gives the value or the fallback and leaves one `console.debug` line naming what failed. A failure the person must hear about goes into the UI, not through `quiet`. A stream frame that is not JSON is skipped the same way (`connectStream` in `api.ts`).

An icon that changes shape with state (copy to check, play to pause) is `IconSwap.svelte`, not `<Icon name={a ? x : y}>`: old and new overlap with scale and blur, and reduced motion swaps at once.

Hub is an app, not a page, so nothing in the window can be marked by dragging unless it is text you
would copy: `app.css` sets `user-select: none` on `html`, `body` and buttons, and a component opts
back in with `user-select: text` on the box that holds such text. That is chat messages (your
bubbles, replies, the steps' commands and output, a failed turn's message), code, terminals and
logs (`CodeView`, `TerminalPane`, `TerminalOutput`, `LogView`), files and documents (`FilePreview`,
`Viewer`, `DocReader`, `DocsView`), pull request and issue text, and text boxes. A new box that holds
such text adds the same two lines (`user-select: text; -webkit-user-select: text`, the second for
WKWebView); labels, cards, numbers and tooltips stay unmarkable. The iPhone app is native and marks
only what has `.textSelection(.enabled)`. Copying stays at hand in the chat: the Copy button under
your message and on every code block, and Copy in the right-click menu of any message.

A state with nothing to show is `Quiet.svelte`: `kind="loading"` ("Reading…"), `kind="none"`
("None"), `kind="newer"` ("Needs a newer Hub", for a call the other hubd lacks) or `text` naming what
is missing ("No chats"). The words are fixed so every page says them the same way.

A slow read shows its last answer first: `recall`/`remember` in `lastSeen.ts` keep it in the webview, the view draws it at once and swaps in the new one, and a failed read leaves the old one in place instead of an error (Overview's stats, health and devlog cards, the machines' load, readiness and versions).

Hub draws its own controls so every page looks and behaves the same; the tokens are in
`app/src/app.css`. A dropdown is always `Select.svelte`, never the browser's `<select>`, which
shows a white system list on Windows. `Select` takes `options` (`value`, `label`, an optional
`hint`, `group` heading and `disabled`), works with `bind:value` or `onchange`, and comes in three
sizes: normal (28 px, for forms), `small` (chips and rows) and `bare` (no border, inside another
field, like the level in the Problems search). Its list looks like the right-click menu
(`ContextMenu.svelte`), sits outside any scrolling box, opens upward near the bottom and is driven
from the keyboard: Enter, Space or the arrows open it, arrows and Home/End move, Enter picks,
Escape closes without closing the dialog around it, and letters jump to a label. `autoopen` opens
it at once for a "Move to…" that replaces a button. `searchable` puts a search box on top of the list (and `custom` can turn the typed text into one more option, as the font pickers do), and an option's `font` draws its label in that font. Engine lists use `engineOptions()` from
`providers.svelte.ts`.

"Something is going on" is always the working grid, never a turning ring or a moving bar:
`Working.svelte` (a `WorkGrid.svelte` in the loop chosen in Settings > Transcript, Pour by
default) beside a step, a build, an install, a widget being made or a TL;DR being asked for, and the
working sign in the inbox (`StateSign.svelte`). Every grid on screen plays by one clock
(`workClock.svelte.ts`, one frame loop while any is shown). The 8 px thread dot (`.dot.running` in
`app.css`, in lists, panels and cards) is the same move drawn in CSS: three tiles, one gliding
into the gap at a time. Real progress bars (how much of a plan is done) stay bars.

A break in a thread's chat is one `Break.svelte`: a grey-tinted wavy line across the chat with a few
words and a time on it, an optional sentence under it and nothing boxed (a choice Hub needs, like
"Continue in Codex?", is a plain surface card under the line). The line takes one quiet colour per
kind (`engine` violet, `machine` teal, `session` sky, `waiting` sand, `stuck` rose, `time` grey,
the `--p-*` tokens); the colour says what sort of break it is, not a state, and the words stay grey.
`transcript.ts` turns the events into them: another engine took over after a plan limit or by your
pick (the Claude and Codex marks, "Claude ran out → continued on Codex"), a thread moved between
machines (each machine's own glyph from `machineGlyph`), a wait for a plan limit to reset, both
subscriptions out ("Codex ran out too"), and `idle`, a "New · 2 h later" above a message that
comes more than `IDLE_BREAK_MS` (an hour) after the last turn ended. A new kind of break is a new
`kind` here, never a new look.

**New, since you were last here** is the orange `new` break (`--p-orange`, "New · 3 replies · 12 min ago"):
it sits before the first thing the agent did after you last looked at the thread, whatever the pause.
`folded(events, since)` places it (`NewItem`; it stands in for the `idle` break, and your own message
after you came back means you were looking, so it places none). `since` is `hub.since[threadId]`, set by
`open()` before `seen_at` moves up to now: the later of when you left the thread on this machine
(`left()` and `newSince()` in `visits.ts`, kept in localStorage, written when the thread view closes,
the window hides or Hub closes) and its `seen_at` from any device, so a thread read on the phone is
not new on the desktop. It stays while you read and is dropped on leaving; a window hidden for ten
seconds or more moves it to where you stopped looking (a hidden window no longer marks the thread
seen). `ThreadView.svelte` opens at the break when more than a screen is new, and shows an "↑ New"
button over the chat while the break is scrolled out above. No new API: it is all in the app.

A pane's edge resizes with `ResizeEdge.svelte`: a grip wider than the 2px accent line it lights on
hover and while dragging; double-click puts the default width back. It reports the width live and
once on release, and the parent owns it. The sidebar (right edge, 200 to 480), a thread's side
column and the home and desk panel (left edge) use it, remembered per name in `prefs.widths`
(`sidebar`, `column`, `side`); the viewer and pull panel keep `prefs.viewerWidth`. However far you
drag, the chat beside it keeps 420px and a dragged panel never grows past the window.

## Reactions
<!-- covers: crates/hub-core/src/reactions.rs, crates/hub-core/src/hub/reactions.rs, crates/hub-core/tests/it/reactions.rs, app/src/lib/reactions.svelte.ts, app/src/lib/emojiData.ts, app/src/lib/components/Reactions.svelte, app/src/lib/components/EmojiPicker.svelte, app/src/lib/components/Emoji.svelte -->

An emoji on a message, from you or from the agent. It is a `reaction` event
(`{ seq, emoji, by, on }`) on the message it is about: a user message, or the `turn_end` that closes
an agent's reply (an answer is many `update` chunks, so the end of the turn stands for it, as
`proof` does). `by` is "you" or "agent", `on` false takes it away again, and the latest event for one
`seq`, `emoji` and `by` is the state, so the log stays append-only. A reaction never starts a turn,
does not move the thread's time or status, and is no news: `gist::moves` does not count it, and
surfaces skip a kind they do not draw. The emoji is any Unicode emoji, or `:name:` of one of your
own (up to 12 characters, no letters or spaces, so "yes" is refused).

You react with `POST /api/threads/{id}/react` (`{ seq, emoji, on? }`, always `by: "you"`; a phone
may, `phone::may`). The agent has its own quiet tool, `react` (`mcp/talk.rs`, in `permit::QUIET_TOOLS`,
told in Hub's tool instructions): it puts an emoji on the user's latest message of their own (not
one Hub sent, `latest_user_message`), so a 👍 on "merge" is a full answer before the work starts.
It lists your own emojis in its description. Nothing tells the agent about your reactions yet.

Your own emojis are small pictures (png, webp, gif or jpeg, at most 96 KB; the app shrinks them
first) in `<data folder>/emoji/<name>.<ext>`, a name being lower-case letters, digits, `_` and `-`.
`GET /api/emoji` answers `{ custom: [{ name, mime, data }], quick }` with the pictures inline
(base64), so a surface needs no second call to draw one; `PUT /api/emoji/{name}` (`{ mime, data }`)
keeps one, `DELETE /api/emoji/{name}` removes it, and reactions that name it stay in the log. The
file's first bytes must match its type. `quick` is the quick row of the picker: your most used
emojis, counted when you put a reaction on (kept in the `emoji:usage` value), filled up with 👍 ❤️
🎉 👀 🚀 🙏 until there is something to count. It is the same row on every device that asks this core.
The calls are the feature `chat.reactions` (`compat::FEATURES`); a client that does not find it
shows no reaction button.

On the PC (as in Claude Code) the emojis on a message are soft grey chips under it, with the count
when you and the agent both put one: under your bubble on the right, under a reply on the left in
the reply's footer (`Changes.svelte`, on the turn end's seq). Yours is tinted with the accent and a
click puts it on or takes it away; the ghost smiley+ beside the chips, and the one in the hover row
(or the footer), open the picker (`EmojiPicker.svelte`): your most used emojis in a row, and "+" for
all of them with a search, groups and a tab for your own, where a picture is shrunk to 96 px in the
window and sent with `PUT /api/emoji/{name}`. `reactions.svelte.ts` folds the `reaction` events of a
thread into chips (`reactionsOf`, cached per log) and keeps the emojis and the quick row; the full
list of Unicode emojis (`emojiData.ts`, emoji 14 and older, so none shows as an empty box) loads the
first time "+" is pressed. Reaction events are skipped when the log is read for the idle time.

### Reactions on the phone
<!-- covers: ios/Hub/Reactions.swift, ios/Hub/EmojiData.swift -->

`Folded` (`Model.swift`) turns the `reaction` events into chips by message seq (`reactions`), and
says which seq a turn's two messages have (`userSeq`, and `endSeq` for the turn end that closes the
reply). The chips (`ReactionRow`) sit under your bubble on the right and under the turn's last reply
on the left, once the turn is over, as grey capsules with the emoji and its count (yours tinted with
the accent) and a round smiley button after them that opens the sheet. A tap puts yours on or takes it away, a hold on a chip says who
put it there, a double tap on a message puts a 👍 on it, and a hold on a message opens `ReactSheet`:
your most used emojis (`Store.emoji.quick`, from `GET /api/emoji`), "+" for every Unicode emoji
(`EmojiData*.swift`, the same list as the desktop) with a search, and your own, which are added from
a photo shrunk to 96 px; Copy and Quote in reply are in the same sheet. A hubd without
`chat.reactions` shows none of it, and the hold keeps the plain menu.

## Things named in replies
<!-- covers: app/src/lib/rich.ts, app/src/lib/langs.ts, app/src/lib/components/Markdown.svelte, app/src/lib/components/PrCard.svelte, app/src/lib/prCard.svelte.ts -->

Agent replies (and the hover card's TL;DR) go through `enrich` in `rich.ts`, which gives what
they name Hub's look: a grey glyph says what a thing is, colour only its state, and anything Hub
knows opens on click. Threads, projects, machines, pull requests and issues (by GitHub link or a
`#123` Hub knows), branches, commands, ports, secrets by name, folders and files are found in
links, inline code and plain text. A file can show the logo of its language instead of the plain file
glyph when `langs.ts` knows it; this is switched off (`LANG_MARKS = false` in `langs.ts`, the tooltip still names the language) (Rust, TypeScript, JavaScript, Svelte, Python, Markdown, TOML,
JSON, CSS, HTML, Go, shell, YAML, Dockerfile; paths from Simple Icons, CC0), filled in the same
grey. In plain text only names with a code extension count as files, so "Node.js" stays a word.

A pull request reference has a card (`PrCard.svelte`): `#695` (when a thread has that pull request),
`PR #695`, `hub#695` (a project Hub knows), `owner/hub#695` and a github.com pull address all count;
`C#12` stays text. After a short rest the pointer gets a floating card with state (Open, Draft,
Merged, Closed), repo and number, age, title, author, +/− and files, and the checks (passed, failed,
running 2 of 3) with the owning thread's title; it stays while the pointer is over it. Where there is
no hover (`(hover: none)`) a tap opens it as a bottom sheet with Open PR (Hub's pull request view,
when a thread has it), GitHub and a scrim to close. The facts come from `GET /api/github/card?path=&repo=&number=`
(`github::card`, feature `github.card`, through the project's machine and its `gh`) and are kept a minute
in `prCard.svelte.ts`; what Hub knows already fills the card while it loads and stays when GitHub
cannot be reached, and a hubd without the call says it needs the next version.

A code block that is a shell command (a shell language, or no language and a first line that starts
like a command; `runnable` in `rich.ts`) gets **Run in terminal** beside Copy, shown by `Markdown.svelte`
only on a thread's own page. It opens a terminal under the block (see "Run in terminal", under Terminals
inside Hub).

## Rail and rooms
<!-- covers: app/src/lib/components/UserMenu.svelte, app/src/lib/components/Rail.svelte, app/src/lib/components/SystemIcon.svelte, app/src/lib/components/InboxIcon.svelte, app/src/lib/components/Sidebar.svelte, app/src/lib/rooms.ts, app/src/lib/worldSwitch.ts, app/src/lib/components/ChatList.svelte -->

The far left of the window is the rail, a 52px column that stays when the sidebar is hidden
(Ctrl B). From the top: the world button (below), then the two rooms, Inbox (the thread list; `work` in the code) and
System (projects and the system pages; `control` in the code), then your projects (below; empty rail moves the window), and at the bottom Live mode's button (in the desktop app's main window only, see "Live mode"), the idea box (Ctrl I), the pause button and, while the sidebar is shut, your
avatar, whose menu holds the account, Settings and What's new (with the sidebar open, its foot
has them). A room's icon shows what it reads only when it
matters, in a part of the icon instead of a dot. Inbox's icon (`InboxIcon.svelte`) is three rows,
a small square and a line each, and its top square turns red when a thread failed, gets an amber
outline when one waits on you (how many is in its tooltip) and breathes green while one works.
System's icon is a 2 by 2 grid (`SystemIcon.svelte`) whose bottom-right tile is red when
something is broken and amber-outlined when something is left on. Opening a room, and every
later click on it, plays a short move, a different one each time: Inbox's rows take a new row on
top, slide in, tick off the top one, move the bottom one up, light in a wave, write out their
lines or hop; System's tiles slide a step, spin, hop, light in a wave or briefly change shape. A
click in the middle of a move starts the next one. The top square or top-right tile stays filled
while that room is open, hovering spreads the parts a pixel, and reduced motion skips the moves.
Every button on the rail is the same size with a 16px icon (the two room icons are 19px, so
their moves read), and every state dot is the same 6px dot in the same corner. The rail and the
sidebar are drawn 15% larger than the rest of the window (`--side-scale` in `app.css`, CSS
`zoom`), so the lists you scan all day read easily without zooming everything. The sidebar slides
open and shut in 160ms from behind the rail; it keeps its width while its holder grows, so
nothing inside rewraps. The
words for both come from `rooms.ts`, which the sidebar header reads too.

System's list is three groups, not eleven rows: Watch (Overview, Problems, Machines), Automate
(Routines, Recipes) and Extend (Library, Plugins, Workers, Services, Updates, Labs, Welcome). Watch is
always open and has no heading of its own; the other two start folded. A folded group shows its pages in grey on its own row (and a dot when
servers are left on or an update failed); the page you are on keeps its group open; which
groups you opened is `prefs.systemOpen`, kept in this window. No page was merged or removed.

What wants you now is not on the rail: the sidebar header has a bell next to search, drawn only
while something does (`nowItems()` in `rooms.ts`: threads waiting on you or failed, machines gone
offline, problems waiting on you; a dot in the worst state colour). A click folds a list out under
the header and pushes the rest down, so it covers nothing: at most three lines (dot and title), then
"+N more in Inbox"; a click on a line opens it and folds the list in, as do the bell again and
Escape. With the sidebar shut the bell is not there. The rail lists projects: the pinned ones on top in the order you set, a thin
line, then the five you worked in last, dimmed. A dot on a project says amber needs you, red
failed, green working. Clicking one narrows Inbox (and Home's list) to its threads, or opens its
page in System; right-click pins or unpins it above the usual project menu, and dragging a
project onto a pinned one, or onto the empty rail, pins it there. Pins are kept in Hub's store
with the icons (`Looks.pinned`, `PUT /api/looks`), so they are the same in every window. The
projects scroll when there are many; the rooms and the bottom stay put.

Alt 0, 1 and 2 go to Home, Inbox and System (Home is the house button next to New thread in the sidebar, no longer on the rail); Alt 3 changes world; Ctrl N opens New thread; Ctrl E still switches between the two rooms. Each room
remembers the page it had open, and Home, New thread and chats are not among them (a chat belongs to the Chat world, so Inbox never takes you into it; `hub.threadHere`), so going Home and back into Inbox
opens the thread you left (`hub.enterRoom`), else the first that needs you. Clicking the room you
are already in opens or closes the sidebar.

The sidebar shows the room you are in: its name, what it reads, search, then New thread or New
project and the room's list. A ready version of Hub is a row just above its foot
(`UpdateBadge.svelte`). The foot takes a `footer` snippet; without one it holds the usage strip
and two buttons. The strip shows one subscription: the one the open thread runs on (its engine's
provider, `engineProvider`), else the one last picked on the name menu's card. It reads as the
provider's mark (`ProviderMark.svelte`), its name and the limit closest to running out (grey
until 80%, amber then, red from 95%), or the balance left for a pay-per-token provider, then the
other subscriptions as small dimmed marks. The wheel on it steps through them until another
thread opens. It has no hover (Christoffer found it ugly, 2026-11-02): the card that opens on click shows every limit. Clicking it opens the name menu
(`UserMenu.svelte`) above it, spanning the sidebar, on that subscription's card; the card's head
carries the mark too, and with three or more subscriptions its switch shows the next one's mark.
The head is one line that never clips its right edge or cuts a word: the name, the plan badge, the
switch and the refresh icon keep their place, and when the plan badge, the text size or a narrow
sidebar leaves too little room, what goes is dropped whole, never cut to "10…": first the age
("21 min", its tooltip still says it), then the other subscription's name (its mark stays). The
steps are container queries in em of the head's own text (`@container head`), so they come at the
same place at every text size. The limit rows share three columns (label, bar, percentage) through
a subgrid: the label column is as wide as the longest label ("Weekly · all models"), never under
6em, and the bar takes what is left. The sidebar's default width is 256px grown by `--text-grow`
(a width dragged at its edge stays as set).
After the strip, Account and usage and Settings are buttons of their own, so it is plain where
they are; a dot on Settings says What's new is waiting (a click opens it) or an update failed.
`hub.meMenu` says whether the menu is open from the foot or from the avatar on the rail, which
shows only while the sidebar is shut.

### Dev and Chat worlds
<!-- covers: app/src/lib/worldSwitch.ts, app/src/lib/components/ChatList.svelte, app/src/lib/components/Rail.svelte, app/src/lib/components/StatusBar.svelte, app/src/lib/state.svelte.ts, app/src/lib/threadFlags.ts, app/src/lib/viewTypes.ts, app/src/lib/format.ts, app/src/lib/menus.svelte.ts, app/src/app.css -->

The window has two worlds, as the iPhone has (`Theme.world`). **Dev** is everything above. **Chat** is
for everyday chats (threads in `<projects root>/chats/`, `isChatThread`): a list of only them
(`ChatList.svelte`: Today and Earlier; a row has its title and its time, and a dot hanging in the left margin so titles never shift: accent and a bold title for an answer you have not read, a pulsing green one while it writes, red when it failed, none when read; `chatMark` in `threadFlags.ts`, passed on by `state.svelte.ts`). Opening a chat reads it (`markSeen` in `threadActions.svelte.ts`, also while you look at it and an answer comes), and the dot goes with it: a question at the end of an answer (`reading` asks/stuck) does not keep it, only a live permission question does, **New chat** (Home's
box in chat mode, "Ask anything", which starts the thread in a fresh `chats/<time>` folder, so it starts
lean; the folder is never the last project, and the "Starting" stand-in already counts as a chat, `STARTING_CHAT`, so Chat stays on screen), no right panel, and a trimmed status bar (`StatusBar.svelte` leaves out the code-side
chips). The rail's first button is the world button (it replaced Home): a click, or Alt 3, changes
world with a circle that grows from the button (View Transitions, `worldSwitch.ts`, plain swap
under reduced motion). It carries a dot when the other world has something: amber for a thread in Dev that waits on you, the accent colour for a chat with a new answer. A chat never "needs you": no "Waiting on you" in its header, no settle button, the bell stays quiet in Chat, and the world button's dot is the accent colour, not amber. Inbox has no Chats fold: a chat is never listed in Dev.

**The wall (2026-11-03).** Chat threads and dev threads are two separate sets and neither shows in, is
started from, or is reached through the other's views:
- Lists: Dev's lists (Inbox, project lists, project pickers, Ctrl K, mentions, share) never hold a chat
  (`hub.everything`, `hub.withChats` and `byProject` leave the `chats/` folder out; `hub.inWorld(t)` is what
  mentions, share and the + menu offer); Chat holds only chats (`ChatList`, and Ctrl K, which in Chat has
  no projects and only New, sidebar and Settings).
- Starting: a dev draft never inherits a chat's folder (`codingFolderDefault`, `recentFolders`,
  `rememberFolder`, `startDraft` and `draftFolder` drop it), and `hub.newThread`, which every new thread
  passes, refuses a chat folder from Dev and a project folder from Chat.
- Leaving: Chat has no Home. Deleting, archiving or settling a chat lands on the next chat or New chat
  (`goHome` is New chat in Chat); Alt 0, 1, 2, Ctrl J and `switchRoom` do nothing in Chat.
- A chat's menu (`chatMenu` in `menus.svelte.ts`) has nothing of the code side, and its header label
  opens no project page.
The world changes only when you press the world button (Alt 3), or when a thread is opened by id from outside both lists (a notification, a link,
back and forward): the thread's own world comes with it (`hub.syncWorld()`). A page of Dev's that ends up on screen in Chat is sent to
New chat, never silently into Dev. Each world remembers the page it had (`hub.worldRoute`, `hub.switchWorld()`).
The Chat world sets `data-world="chat"` on `<html>`, and `app.css` then overrides the same tokens
with warm browns and a serif for titles (Newsreader, bundled in `boot.ts`). Only the default Dark
theme gets it; every named theme (Cappuccino and the others in `themes.ts`) keeps its own colours. Spaces, Artifacts and
Images are not built here; the Chat rail has Chats only.

## Search (Ctrl K)
<!-- covers: app/src/lib/components/CommandPalette.svelte -->

One box for everything you can jump to or do (`CommandPalette.svelte`, opened by Ctrl K or the
sidebar's search button). Empty, it lists what needs you (up to three), then what you can do with
the thread you are in, then with its project, then recent threads and the everywhere actions.
Typing searches threads, projects, actions and Settings pages together, with the letters you typed
bold and underlined, and adds what was said in threads (`api.history`, after three letters).

A first sign picks one kind, shown as a chip you clear with Backspace on an empty box: `>` actions
(with the thread's and the project's first), `@` projects, `#` pull requests and branches (from
`hub.prs` and `hub.branches`), `"` what was said, `/` files in the project you are in
(`api.filesIn`), `!` what waits on you. "New thread in…" opens the project list in the same box.
Actions show their shortcut as key caps, and Alt Enter on a thread or project row opens the same
menu as its right-click (`threadMenu`, `projectMenu`). Shift Enter starts a thread in the project
you are in with the words you typed. It is all in the app and uses calls Hub already has, so an
older hubd works with it.

## Status bar and panel
<!-- covers: app/src/lib/statusbar.svelte.ts, app/src/lib/components/StatusBar.svelte, app/src/lib/components/BottomPanel.svelte, app/src/lib/components/PortList.svelte, app/src/lib/components/JevBar.svelte, app/src/lib/guard.ts -->

The main window has a status bar at its foot (`--bar-h`, 24px), as in VS Code, with Hub's own
things in it. What it shows follows the page (`bar.place`): in a thread, that thread's machine,
folder and branch; in a project, the project's; on a machine's page, that machine; elsewhere this
machine. From the left:

- The machine, in the accent (opens its page).
- The branch and how many files changed (`GET /api/threads/{id}/git`, or the project's info).
- Problems that need you, from Guard (opens Problems; shown only when there are some).
- Guard: a shield, "N threads" for the fix threads it has started (`hub.guardThreads`; green with
  the working grid while one runs) and an amber "· N need you" when Guard or one of them waits on you.
  Shown when Guard is on or has threads. Its card lists the threads with what each is doing
  (`guard.ts`), and links to Guard's chat and to Problems.
- Terminal: opens the panel with a shell on the page's machine (`POST
  /api/hosts/{name}/terminals`, what `shell`). In a folder, the new shell is sent `cd "<folder>"`
  when it first connects (`TerminalPane`'s `input`). The bar keeps its terminals' ids in local
  storage and forgets those the core no longer has. Ctrl Shift § toggles it. A click on a link in
  the terminal opens the system browser (`TerminalPane` link provider, `terminal-links.ts`); a link
  wrapped over several rows, like the Claude sign-in URL, counts as one.
- Ports: a small card of what listens on the page's machine (`GET /api/host`: process, who
  started it, how long ago), with Open in the browser and Stop, which takes a second click.
- The update: the version, the step while it builds, "Builds at HH:MM" while a Rust version waits
  for a quiet branch, Ready or failed. Its card shows the way there (Merged › Quiet › Test › Build ›
  Ready), the step, Build now, Install or Try again, and the last versions with their pull
  requests (from the patch notes).

On the right is the decision model, when one has its key (`JevBar.svelte`, see API connections):
its logo, its name and what is left of the credit you entered (else how many decisions it made in
the open thread). Its card has a faint copy of the service's clouds in its head and two tabs (this
thread, today). Its big number is what is left of `ApiConnection.credit` (the credit less what the
input tokens counted since in `ApiStatus.total` cost at TypeSafe's price), or today's cost until a
credit is entered, drawn to six decimals with the last four set apart; it counts to each new value
over a second and a half (a `Tween`) and the card reloads every five seconds while open. Under it
are the tab's spend, about how many calls and how long the credit lasts at this month's pace, then
a strip with one bar per decision (latest forty, colour by what Hub did, height by how sure; a new
one fades in and empty slots wait on the right) and the latest four as one line each: a mark, what
happened and the words it read, a ring for how sure it was against the feature's threshold, and
the time. A new line opens in gently. "Full log" opens the connection's page in Settings, whose
"What it decided" shows the whole log a hundred at a time. The list keeps room for four rows and the strip
its forty slots on both tabs, and switching tabs shows the other list at once (no fade or slide),
so the card keeps one height and does not jump. With a core that has `apis.agent`, the gear view has "Agents can ask Jev" (the `agent` entry in the connection's off-list) and the This thread tab a row for the open chat (`GET`/`PUT /api/threads/{id}/jev`, `{agents}`, which is false too while the global switch is off, so the row is then disabled); on an older core both say "comes with the next version". The card and its gear view are both laid out at
once, one hidden, so flipping to the gear and back keeps that height too. With a core that tunes in the background
(`apis.tune.run`), "Tune" starts one and the foot breathes "Tuning…" while it reads; when it has
suggestions the foot shows "N suggestions ›", which opens the connection's page. There
"Suggestions" lists them above the log: Use sets a threshold, Build opens a thread in Hub's own
project (else the usual draft folder) that makes that change, Skip drops it. The gear turns the card into what it may decide: the `apis::FEATURES`
switches (`ApiPrefs.off`, the same as Settings), their thresholds, the connection's model, cap and
price, and what Hub does without it. Then Live mode. A card opens above its item and closes on Escape or a click elsewhere. `onScreen` measures it and slides it back inside the window, because a larger text size makes a card wider than the width it was placed by.
The panel above the bar (`BottomPanel.svelte`) has Terminal, Ports and Problems; its height is
dragged from its top edge and kept. Everything comes from calls the core already had, so the bar
works against older cores (a call that fails leaves its item out).

## Zoom and text size
<!-- covers: app/src/lib/zoom.ts, app/src/lib/textSize.ts, app/src/lib/screens.svelte.ts, app/vite.config.ts, app/scripts/check-layout.cjs, app/scripts/shots.cjs, app/scripts/shots/ -->

Two ways to make Hub bigger, and they combine. **Zoom** (Ctrl + / −, Ctrl and the wheel, `app/src/lib/zoom.ts`)
is the webview's own: everything grows by the same amount. **Text size** (Settings > Appearance,
`app/src/lib/textSize.ts`) grows small text most and headings hardly at all, for reading from a
distance without giving up the screen.

Stylesheets are written in plain px; a PostCSS step in `app/vite.config.ts` makes them follow the
text size. Each px font size becomes `max(own size, --text-min less half its distance to 15px)`.
Boxes that hold text grow by `--text-grow` (how much body text grew): widths from 100px (40px in a
rule that sets a font size), px grid columns, heights from 16px and px line heights. A rule with a
fixed px width and height (an avatar, a dot) keeps its size, and so does a rule with
`--keep-size: 1;`: text drawn inside an SVG's viewBox (the clock's digits, the daylight curve's
hours) sizes with the drawing, and growing the text alone made the seconds run into the time; the
drawing grows with `--text-grow` instead. Small text grows faster than `--text-grow` (11px becomes
about 14.5px at Large, boxes grow 1.17 times), so a row that only just fits at Default will not fit
by growing its box alone: give its parts room that follows their own text (em, `ch`, container
queries in em, a subgrid) and let whole parts drop out instead of being cut by an ellipsis. Media and container queries get a copy
per step with grown thresholds under `:root[data-text=step]`, so layouts fold sooner when the text is
bigger. At Default nothing changes.

**Sizes per screen** (`app/src/lib/screens.svelte.ts`). How big things are belongs to the screen the
window is on, not to the person: zoom, Text size, the chat's size, line spacing and width, the code
size and every dragged width (sidebar, panels, viewer). Hub knows a screen by its size in device
pixels and its scale (`currentMonitor()` in the app, `screen` in a browser), and `watchScreens()`
checks again when the window moves, resizes, gets focus or changes scale, then `enterScreen()` puts
that screen's sizes on the page. `prefs.screens[key]` holds only what was set while the window was
there (`setPref` and the zoom's `hub-zoom` event write to it through `rememberHere`; Hub's own switch
is not taken for a pick). A screen that has set nothing inherits what was last set, with the zoom
lowered to the largest step that keeps the window 1200px wide (`fitZoom`), and takes every panel at its
default width; the first screen Hub ever sees keeps what is set now. Fonts, the theme and the rest
of the settings are the same everywhere. Settings > Appearance shows the screen and "Use Hub's sizes
here", which forgets what was set on it. The sidebar can not be narrower than 240px (grown with the
text), so "New thread" never wraps; its Ctrl N hint drops out below 226px, and the simulator panel's
machine name gets an ellipsis. Nothing in the core or the log changes.

## Fonts and the chat's look
<!-- covers: app/src/lib/fonts.ts, app/src/lib/components/TypographySettings.svelte, app/src/lib/prefs.svelte.ts, app/src/lib/components/Transcript.svelte -->

Settings > Appearance has three fonts, as in Claude's own settings, each picked by name
(`TypographySettings.svelte`, a `Select` with a search box, each option drawn in its own face):
**Interface font** (`prefs.uiFont`, the CSS variable `--font`) is everything that is Hub's own:
lists, menus, buttons, your messages, the settings. **Chat font** (`chatFont`, `--chat-font`, which
follows `--font` until another is picked) is what the agent writes, including the ask card's text;
Hub's own lines in the chat (Worked for, breaks, tool rows) stay in the interface font. **Code font**
(`codeFont`, `--mono`) is code, command chips, diffs and the terminal (`TerminalPane` reads it, and
`--code-scale`, when it opens). Beside the chat and code fonts is a size in px; they are stored as
multiples of the designed sizes (`CHAT_SIZE` 14.5, `CODE_SIZE` 12.5) in `--chat-scale` and
`--code-scale`, and a font size written `calc(Npx * var(--chat-scale))` still grows with Text size
(the PostCSS step in `vite.config.ts` knows that form). Line spacing is `--chat-lh`; Chat width
(narrow, medium, wide: 720, 820 or 920 px times `--text-grow`) overrides `--chat`.

`fonts.ts` holds the fonts Hub carries (Geist, Inter, Source Sans 3, IBM Plex Sans, Figtree,
Newsreader, Literata; Geist Mono and JetBrains Mono) and loads one only when it is used, as its
Latin file (about 50 KB, with the italic) through the `FontFace` API; Geist, Geist Mono and
Newsreader's upright face come with `boot.ts`. Any other name is taken as a font installed on this
computer. Listing them (`queryLocalFonts`) makes the webview ask "use the fonts on your computer?", so Hub
never asks by itself: the lists end with "Show installed fonts…", which asks once, and the installed
fonts show without asking only after the person has said yes. What is typed in the search box can
always be used ("Use “Charter”"), with no prompt. A missing font falls back on the
generic family. The settings live in this window's own preferences (`prefs.svelte.ts`), so nothing
in the core or the log changes and an older core works.

**Chat look** presets (Hub, Claude Code, Compact: `LOOKS` in `fonts.ts`) set only the chat's font,
size, line spacing and width; the interface and code fonts are yours. Custom lights up when the
four match no preset, and Reset to Hub puts every font and size back. The sample under Typography
is drawn with the same variables as the chat.

`app/scripts/check-layout.cjs` checks the result: with the dev server and a core running it opens
every view, the name menu and each settings page at every text size and at 100, 125 and 175% zoom,
and reports text that overlaps other text, spills out of its box, is cut off, has a letter or two
outside a box that hides overflow, or is short (a label, a time, a plan name) and cut by an ellipsis.
The name menu is opened from the sidebar's foot and from the rail, for both subscriptions. Long
text with an ellipsis (a thread title) is fine. It adds a little
letter spacing first, so layouts that only just fit fail there and not on a machine whose fonts
render a little wider. Run it after changing layout.

`app/scripts/shots.cjs` is the check for a change that must not alter what the app looks like (moving
code between files): it takes a picture of each main view, every settings page, the message box in a
thread and in a new thread, the model picker, `@` and `/` suggestions, the mic menu and the stash list, and
compares two runs pixel by pixel. The app has no JS tests, so this is the before and after. It needs
only the dev server: `scripts/shots/mock-core.cjs` answers every call with the same made-up threads
and settings, the stream says nothing and the page's clock stands still. `node scripts/shots.cjs
--save before`, change the app, then `--save after --against before` lists the places that differ and
writes a diff picture for each (not when the size changed), and counts places it could not compare (no picture before or now) as differing. Needs Playwright (not in `package.json`; in the cloud container `NODE_PATH=/opt/node22/lib/node_modules`). Report only: it exits 0 unless `--strict`. Pictures live in the
system temp folder (`hub-shots/`); `--clean` removes them. A place the mock answers badly shows as a
page error in the output; give the call an answer in `mock-core.cjs` rather than ignoring it.

Grids of small squares (the activity map, "When you work") take their cell, gap and label column
from `app/src/lib/pixels.svelte.ts`: whole **device** pixels, so at 125% zoom (a 2px gap is 2.5
device pixels) every gap rounds the same way instead of some looking wider and darker.

## The Machines page
<!-- covers: app/src/lib/middleClose.ts, app/src/lib/components/MachinesView.svelte, app/src/lib/components/MachineView.svelte, app/src/lib/components/ReadyGroup.svelte, app/src/lib/components/RunningGroup.svelte, app/src/lib/components/MachineCard.svelte, app/src/lib/components/MachineGroup.svelte, app/src/lib/components/MachineRow.svelte, app/src/lib/components/Wave.svelte, app/src/lib/machineStats.svelte.ts, app/src/lib/folds.svelte.ts, app/src/lib/machineHealth.ts, app/src/lib/machineUpdate.ts, app/src/lib/machines.svelte.ts -->

System > Machines opens with two waves for the whole network, memory in use and CPU busy (1 h, 6 h
or 24 h), one line on how many more agents fit ("Room for about 10 more agents, most of it on
tincan": each machine's free memory less a tenth, over what an agent takes now), and a card per
machine (`MachineCard.svelte`): its kind icon, name and OS, a line on how it is doing when it is
not fine, what is free of memory, CPU and disk (small waves for the first two), and the agent
sessions running there. A card that is red inside is about to run out. Buttons for a new thread in
its home folder, a terminal and the first engine's CLI sit under it; ⋯ (or a right-click) holds the
rest. A click on the card opens the machine's page.

The state behind the machines lives in `machines.svelte.ts`: the tailnet and the machines found on it
(`newMachines`, `setUpMachine`), the version updater (`update`, `updateReady`, `installUpdate`), patch
notes and the subscriptions' limits (`plan`, `others`). `hub` passes its fields of the same names on
to it, so `hub.update` and the rest read as before.

The waves (`Wave.svelte`) are shares of capacity: the line is the accent while calm, amber past 80 %
and red past 92 % (CPU only amber, from 90 %), with a dashed line where amber begins. They are
drawn from `machineStats.svelte.ts`, which asks each online machine for `/api/host` every 3 s while
the Machines page or a machine's page is open (only that machine on its page) and every 15 s from
the status bar, and keeps averages in buckets (1 h in 30 s, 6 h in 3 min, 24 h in 12 min) for as
long as the window lives. A graph fills from the right; the core does not keep history yet.

The colours come from `machineHealth.ts`, so a ring, a number and the card's line always agree:
grey is fine, amber wants a look, red is about to go wrong. Memory goes amber from 80 % and red
from 92 %; the disk amber under 15 % free and red under 5 % or 5 GB; the temperature (the chip's, averaged over its CPU
sensors) amber from 85 °C and red from 95 °C, from 90 and 100 on a Mac and from 75 and 85 on a
board like a Raspberry Pi; CPU only amber, from 90 %, since it spikes. A ready check that is not
fine, a thread that needs you and a hubd older than this computer's are amber too, and offline or
a missing engine or sign-in is red. The card's line is the worst of these with how many more
there are ("Claude Code: sign-in ran out · 2 more"); with nothing to say it is green: ready, or
how many threads are working.

A machine's page has the same look. The left column is the card (free capacity as rows with small
waves, counts, temperature), the buttons that act there (New thread here, Terminal, Root shell,
each engine's CLI with its own mark and colour (Claude's burst and OpenAI's knot, in the picker's
colours), on this computer when it is Windows an Admin button (it lights, pulses once, and puts a
small shield on the other buttons; the next Terminal, Claude or Codex press then opens elevated
in the Terminal tab, and Windows asks first), Add to tailnet) and what the machine is.
The update is the control at the top right of the card (`machineUpdate.ts`): an amber Update button when the machine runs another hubd than this computer; once pressed the step it is at (sending, sent, received, building, ready, restarting, updated) with five dots that fill and never go back within one update, the build step and times in the hover; failed is red with Try again and the reason in a line under the card. A built version that waits for the machine's threads says so there with Switch now. The Machines list keeps the plain word "update" on a card that is behind. The right side
has tabs: Overview, Threads, Files, Ports and Projects, and Terminal once a terminal is open there.
Overview shows a red bar when the machine is in trouble (with what takes the most memory), the two
big waves, and then what runs there as cards in one, two or three columns by width (an
ultrawide gets three). Each card is a group with a header row you press to fold it (an arrow that
turns) and at most about a third of the window high, scrolling inside itself. The columns are
separate stacks and each group has a fixed column, so folding one moves nothing in another column:
Sessions, Everything else (memory Hub does not account for, folded) and Recent threads; Services and
ports (stop where Hub may; a green dot says a settled thread left it) and Containers (with Docker's
mark); Builds and Hub (read-only), Ready for work (the checks with their fix) and Projects here.
Hub remembers what is open per machine (`folds.svelte.ts`). A folded section still shows its count and what it takes of
memory. These come from `/api/workers` and the listeners in `/api/host`. A terminal tab (here and in
the bottom panel) closes with a middle click, as in a browser; `middleClose.ts` cancels the press so
nothing autoscrolls or pastes.

Its Terminal, Root shell and engine buttons (and Sign in there, the Machines page's Terminal, a
thread's "Sign in on …" and the model picker's Sign in) open a terminal inside Hub, below.

## Terminals inside Hub
<!-- covers: crates/hub-core/src/pty.rs, app/src/lib/components/TerminalPane.svelte, app/src/lib/components/RunTerminal.svelte, app/src/lib/runInTerminal.ts -->

A terminal opened from a machine's page runs in this computer's core, in a pseudo-terminal
(`pty.rs`, portable-pty: ConPTY on Windows, a Unix pty elsewhere). What runs in it is what the
window used to run (`machine::command_for`): `ssh <alias>`, `ssh -t <alias>` running `hubd root` there (`signin::root`: `sudo --preserve-env=<Hub's kept sign-in variables> -i env PATH=<the login user's tool folders>:… bash`; root is another user with its own home, so the login user's `~/.local/bin` and Hub's `tools` and `node` folders are put in front, and Hub's lasting sign-in goes into root's environment, never a command line, so `claude` there is signed in like the machine's threads; nothing is written under `/root`, provider keys are left out, and a hubd too old for `root`, found by `hubd-root-signed-in-v1` in its binary, gets the plain `sudo -i` with the same PATH). Codex has no lasting token, so it still signs in once as root, the engine's CLI
in a login shell there, or for this computer its own shell (PowerShell on Windows, `$SHELL -l`
elsewhere). A thread's Take over runs `hubd attach` in one (`what: "attach"`, above). `POST /api/hosts/{name}/terminals` starts one; the app draws it with xterm.js
(`TerminalPane.svelte`, coloured from the theme's tokens) over `GET /api/terminals/{id}/ws`: output
as binary frames, keys back as binary frames, `{"resize": [cols, rows]}` as text, and
`{"exit": code}` when what ran in it ends.

A terminal keeps the last 512 KB of its output and keeps running while no page shows it, so going
to Home and back shows it as it was. It ends when what runs in it exits (it stays listed, greyed,
with its exit code) or when × closes it; a core restart ends them all. The machine's page lists
its own under a Terminal tab, one chip each; "Open in a window" still opens the same in Windows
Terminal (`machine::open`, `POST /api/hosts/{name}/open`). Ctrl+C copies when text is selected and
interrupts otherwise; Ctrl+V pastes. xterm's fit addon measures its parent's computed height, which
includes padding under `box-sizing: border-box`, so the terminal is fitted inside a padding-free
box (`.fit`) within the padded one, and fitted again when fonts have loaded; otherwise it gets a
row too many and the last line is cut off. Phones may not open or type in one (`phone::may`), and the
socket checks the origin as the stream does.

**As administrator** (Windows, this computer only; the Admin button sends `admin: true` with
`POST /api/hosts/{name}/terminals`, feature `machine.admin.terminal`). Windows lets no process that
is not elevated start an elevated child or reach one, so the pseudo-terminal cannot be this core's.
`Terminals::start_hosted` instead makes a listed terminal with no process (`TerminalSession.admin`,
title "… · administrator", a shield on its chip) and `machine::start_admin_host` asks Windows to
start `hubd admin-host <bind> <data folder> <terminal> <secret> <cols> <rows> [-- program]` as
administrator (PowerShell's `Start-Process -Verb RunAs`, no window; a "No" is the error the page
shows and the terminal is dropped). That helper reads the connection token from the data folder
(the same user), opens ConPTY and the shell itself and connects to
`GET /api/terminals/{id}/host?secret=…` (`pty::run_host` / `serve_host`): what the shell prints is
binary frames into the terminal's own backlog and out to the pages, keys come back as binary
frames, `{"resize": [cols, rows]}` and `{"kill": true}` as text, and it ends with `{"exit": code}`.
The secret is made for that one launch and lets one helper in; if none comes within 30 seconds the
terminal ends saying so. Closing the terminal, or this core going away, ends the shell. A page
opens and types in it like any terminal, so anyone who may type in a terminal on this computer may
type in this elevated one (a phone's token may not, `phone::may`); the shield on its chip says
which one it is. An older hubd has no `machine.admin.terminal`, and the Admin button then opens
the window outside Hub as before.

### Run in terminal
A command in a reply opens its own terminal under the block (`RunTerminal.svelte`, mounted by
`Markdown.svelte`), on the thread's machine and in its folder, with the command **typed and not
run**: the person reads it and presses Enter. Hub never sends the Enter. Pressing it again on the
same block replaces the card. The arrow on the button lists the machines that are online (the
thread's own first; only it starts in the thread's folder) and "as administrator", which is never
remembered. The command is the block's text with prompt marks dropped and `\`-continued lines joined
(`commandText`); a command of several lines is typed as a bracketed paste so the shell does not run
each line at its newline (`typedCommand`). The terminal is the same pty as everywhere (`what: shell`),
typed through `TerminalPane`'s `input` once it connects, after a `cd` into the folder; `type()` types
later. Administrator opens a `root` shell on Linux and Mac (`sudo -i` asks for its password, so the
command is typed with a button after it, never ahead into the prompt); on this computer when it is
Windows it opens an elevated terminal inside Hub (`admin: true`, feature `machine.admin.terminal`; Windows
asks once, and there is no password prompt, so the command is typed ahead like in any terminal), or on an older
hubd the window outside Hub of "As administrator" on the machine page (`machine.admin`) with the command copied
for Ctrl V; a Windows machine over SSH is already elevated, so no switch. The
card can move to the bottom panel (`bar.mine`), and when the thread is left the terminal moves there
by itself; only × ends it. The phone cannot open terminals yet (`phone::may`).

## The simulator view
<!-- covers: crates/hub-core/src/simview.rs, app/src/lib/simview.svelte.ts, app/src/lib/components/SimFloat.svelte, app/src/lib/components/SimPanel.svelte, app/src/lib/components/SimScreen.svelte, ios/Hub/Simulator.swift -->

A Mac with Xcode can show its iPhone simulator, so what a thread builds is seen in Hub and not
on a screen you have to go and find (`simview.rs`, feature `sim.view`). It runs where the simulator
is, on that Mac's hubd; the PC, the phone and a window on another machine ask it with `?host=`
like any other call, and the forwarding middleware carries the answer (which is why the live
picture is single JPEGs asked for a few times a second, not a relayed stream).

- `GET /api/simview`: `SimView` with `available` (a Mac with `xcrun`; elsewhere `reason` says so),
  `live_possible` (Node and npm are there), the iPhones and iPads that can run (`xcrun simctl list
  devices available -j`, booted first) and `live`, the phase of the live picture: `off`,
  `installing`, `booting`, `starting`, `live` (with `fps`) or `error` (with `detail`).
- `GET /api/simview/frame?udid=`: a JPEG. While that simulator is watched live it is the newest
  picture of the stream (204 until the first arrives); otherwise it is a screenshot
  (`xcrun simctl io <udid> screenshot --type=jpeg`), which needs no tool and no stream. That
  screenshot is also what a thread can attach when it has built the app.
- `POST /api/simview/live {udid, on}` starts or stops the live picture and answers with the state
  at once; the phase says how far it got. `POST /api/simview/boot {udid}` boots one and waits.
- The live picture uses `serve-sim` (Evan Bacon, Apache-2.0), the tool T3 Code's device panel is
  built on. Hub installs the pinned version once into `<data>/tools/serve-sim` with
  `npm install --ignore-scripts`, runs it on loopback only (never `--host 0.0.0.0`: its preview has
  a shell-exec route), reads its motion-JPEG stream and keeps the last picture. Nobody asking for a
  picture for a minute stops it, and a stopped or crashed tool is killed with `serve-sim --kill`.
  It is not an engine and not part of an agent's tools, so it does not go through Customs, but the
  version is pinned and a newer one is a decision. Tried by hand on the 2018 Intel MacBook (macOS 15,
  Xcode 26): about 4.5 pictures a second at full size, a quarter to a third of a core.
- `POST /api/simview/input {udid, touches: [{phase, x, y}], button}` (feature `sim.touch`) touches the
  simulator that is watched live: `begin`, `move` and `end` with the place as 0..1 of the screen
  from its top left corner, and a button (`home`, `swipe_home`, `app_switcher`). Hub keeps the
  tool's control socket open beside the picture stream (`/helper/<udid>/ws`) and sends each touch as
  one binary message, a code byte and JSON; 400 while the simulator is not live. A client that
  gets 404 (an older hubd) keeps the picture and hides touch.
- `POST /api/simview/shutdown {udid}` (feature `sim.shutdown`) shuts a simulator down
  (`xcrun simctl shutdown`; already off is fine) after stopping the live picture if it is that one,
  and answers with the new state. Hub never shuts one down by itself: the live picture stops after a
  minute without a viewer or when the phone is closed, the simulator keeps running until you power
  it off (a power button in the panel's bar). With no simulator running the phone button in the
  thread's header lists the ones the Mac can start (`POST /api/simview/boot`, then it opens).
- `POST /api/simview/app {udid, action, thread?, tab?}` (feature `sim.app`) runs Hub's own iPhone app
  (`se.karlsson.hub`) in the simulator, answers 204. `start` launches it with `-pair <link>`: Hub
  makes a one-time code itself (an ordinary device code: a phone code only redeems over the tailnet) and the link is `http://127.0.0.1:<port>/#pair=<code>`, since
  the simulator shares the Mac's network, so nobody types a code or scans a QR code. `tab` (0 to 3:
  Inbox, Chats, Machines, Settings) does the same and adds `-tab <n>`. `thread` opens
  `hub://thread/<id>` in the running app. An app that is not installed in that simulator answers
  502 "not installed yet". The panel's "Hub app" menu (`SimPanel.svelte`) offers these; a hubd
  without the feature answers 404 once and the menu says it needs the next version (`sim.noApp`).
- A UDID is hex and dashes; nothing else reaches a command line.
- On the computer a thread on a Mac that has a simulator running gets a phone button in its header
  (a green dot while live), not a phone in the chat. It opens the phone floating over the chat:
  bare, no frame, two see-through buttons in the corner (expand, close), draggable. Expanding opens
  it in the right panel as a viewer of kind `sim` (`SimPanel.svelte`: device in Hub's `Select`, a
  Live pill with the frame rate); the other button floats it again. `simview.svelte.ts` asks each
  machine every four seconds while a thread of it is open, and `SimScreen.svelte` asks for the
  picture about five times a second. Closing stops the live picture; the simulator keeps running.
  The screen takes touches (`SimScreen.svelte`): pressing is a finger down, dragging moves it,
  letting go lifts it, so a click is a tap and a drag is a swipe; moves are sent in batches of what
  came in 33 ms, in order, one request at a time (`api.simInput`, `sim.touch`). A home button sits in
  the floating phone's corner and in the panel's bar (with the app switcher). The floating phone is
  moved by a thin grip along its top edge, which shows on hover, since the screen itself is for
  touch. A hubd without `sim.touch` answers 404 once and the screen stays a picture (`sim.canTouch`).
- On the iPhone the same round phone button sits in the thread's top and opens the simulator full
  screen on near-black (`Simulator.swift`). The screen takes touches like a phone (`TouchPump`: a
  finger down, moves, up); home and close are two quiet round buttons in the top corner.

## Inspect mode
<!-- covers: app/src/lib/inspect.svelte.ts, app/src/lib/inspectInfo.ts, app/src/lib/inspectSend.ts, app/src/lib/components/Inspector.svelte, app/src/lib/components/InspectTree.svelte, app/src/lib/components/InspectOverlay.svelte, app/src/lib/components/InspectCard.svelte, app/svelte.config.js -->

Ctrl Shift I (or "Inspect Hub" in Ctrl K) turns Hub's own pages into something like the browser's
F12 with a way back into a thread: mark elements, write what is wrong, group some, and start a thread
that gets it all. Pure UI, no core call beyond the ordinary `POST /api/threads`, so it works on any hubd.

- **The panel** (`Inspector.svelte`) takes the place of a thread's side column (beside a viewer or a
  pull request it is added at the edge): the page's element tree (`InspectTree.svelte`, the real DOM,
  looked at again each second), what is marked with a note each, a note per group, Clear and Open
  thread. Hovering a tree row frames the element on the page and the other way round; a click marks it.
- **The overlay** (`InspectOverlay.svelte`) frames the element under the pointer with a label (component and
  size), keeps the marks with their numbers, and opens a note box right at the element after a click.
  While Inspect is on every pointer event is held back from the page (capture listeners), so a click
  picks and never presses; Esc closes the note box, then the thread card, then the mode. Shift-click
  puts the element in a group with the one clicked before (letters A, B…; a group of one is dissolved).
  Everything of Inspect's own carries `data-inspect-ui`, which it never picks and the screenshot leaves out.
- **Which component** is known because `svelte.config.js` has a markup preprocessor that puts
  `data-c="lib/components/Rail.svelte"` on the root elements each component draws (not on child components),
  in the installed build too. `inspectInfo.ts` reads it: component file and name, tag and classes
  (without Svelte's hash classes), the path up to the component's root, the nearest four components,
  the text (100 characters), the rectangle and a few computed styles.
- **The thread** (`InspectCard.svelte`, a floating card, the page stays) holds a screenshot, the marks as chips,
  a line of your own and a project (Hub's own, or the folder of the thread you are in). It starts the
  thread through `hub.newThread(..., open = false)`, in the background. The screenshot (`inspectSend.ts`)
  is the page drawn by `html-to-image` (dynamic import, loaded on first use) with the frames and numbers
  painted on it, cut where the panel starts, at most 1400 px wide, one PNG; if drawing fails the thread
  starts without it and the card says so. The message says what the picture is, the view, window size,
  theme and text size, then per group and per mark: note, element, component file, size and place,
  text, styles and the components around it. No DOM dump, no stylesheet: the agent has the code.
- **Magnifier** (Z, or the glass in the panel's head): a lens beside the pointer shows the element under it three times larger
  (more for small ones, at most 440 × 320 on screen), drawn by `html-to-image` a moment after the pointer rests and kept per element
  (`magnified` in `inspectSend.ts`).
- Hover cards (a thread's peek) can still show in the picture: they open on `mouseenter`, which cannot be held back.
- Not built yet: the `hub:inspect` purpose and `inspect-v1` recipe in the packing slip (the message counts as
  `you` meanwhile), the thread stamped with where it came from and closing after the answer, a native
  screenshot from the shell, and Inspect inside agent pages (iframes).
