# Architecture: The phone and smaller pieces
<!-- för: agent -->

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## The phone
<!-- covers: crates/hub-core/src/phone.rs, crates/hub-core/src/phone/, crates/hub-core/src/phone_files.rs, crates/hub-core/src/project_parts.rs, crates/hub-core/src/ntfy.rs, crates/hub-core/src/now.rs, app/src/lib/presence.ts -->

Settings > Access > Connect your phone shows a QR code for
`https://<machine>.<tailnet>.ts.net/phone#pair=<code>` (`crates/hub-core/src/phone.rs`). Pressing
it runs `tailscale serve` when this machine has no tailnet address yet, and makes a one-time phone
code (`Pairing::phone_code`). The code sits after `#`, so it never reaches a server log; the page
trades it for a token, keeps that in the phone's storage and drops the code from the address.

The page is one plain HTML file with its script (`phone/index.html`), embedded in hubd, so every
machine serves it with no build step. It lists threads (needs you, working, recent), shows a
thread folded down to messages, replies, the work between them as one line, questions and
failures, writes to it, answers permission questions and stops it, live over `/api/stream`.

It is laid out for iPhone Safari and its home-screen mode: the top bar and the composer are
sticky (the composer's wrapper `#bottom` is the sticky box, so it stays on screen in a long
thread), the notch and home-bar areas and landscape sides are padded with `env(safe-area-inset-*)`,
text boxes are 16 px so Safari does not zoom in, and `/phone/icon-180.png` is the
`apple-touch-icon` (iOS ignores the SVG icon). Checked at iPhone SE, 15 and Pro Max sizes in an
emulated browser; not yet on a real phone.

The phone follows every machine, not only the one it paired with, so it keeps working while the PC
sleeps. After pairing, the page asks the paired machine for the others (`POST /api/phone/machines`
with the ids it already has): for each online machine with an HTTPS address on the tailnet, that
core asks the machine for a one-time phone code (`POST /api/pair/phone-code`, which needs the
machine's own token) and hands the codes over. The page redeems each at the machine itself, keeps
the tokens in its storage, opens one stream per machine and makes one list of them. A machine's
own threads win over the copies the paired machine relays; when it cannot be reached, those
copies stay and calls about its threads go through the paired machine, which passes them on.
The panic button is sent to the paired machine and to each machine reached directly. A machine on
a hubd from before machine ids is left out and seen through the paired machine as before.

What keeps it safe:

- Tailnet only. hubd still listens on loopback; `tailscale serve` connects from there and adds
  the phone's address last in `X-Forwarded-For`. The page, a phone code and a phone token work
  only when that address is a Tailscale one (100.64.0.0/10, fd7a:115c:a1e0::/48) or the request
  comes from this machine itself. Funnel requests (`Tailscale-Funnel-Request`) are refused.
- A phone's token is narrower than a machine's (`Grant::Phone`, `phone::may`). It follows and
  answers threads (read threads, their logs, cards and changes; send, answer a permission, stop,
  steer and edit the queue), changes a thread from its menu (the whole `PATCH`: title, engine, pin,
  snooze, settle, archive; its agent's settings; moving it to another machine), starts an ordinary
  thread (`POST /api/phone/threads`: a folder, a message, an engine, never attempts or unattended
  runs; `scratch` instead of a folder makes `<projects root>/scratch/<time>` there, the desktop's
  "No project"; `chat` makes `<projects root>/chats/<time>`, an everyday chat the phone keeps
  apart from dev threads; `POST /api/phone/warm` takes `chat` too and makes that folder at once, so
  the engine is up before the first message and a changed pick reuses the same empty folder), keeps ideas, pauses everything, reads the machines (`/api/hosts`, `/api/host`,
  `/api/host/about`, `/api/machine/ready`, `/api/updates`) and updates or reconnects a machine's
  hubd, reads plan limits and projects and their focus and ticks a box in FOCUS.md
  (`POST /api/phone/focus`), reads a project's library (`/api/project/outputs`, pictures raw,
  docs as text), adds a project, reads Safety and makes it stricter, never looser (a higher level,
  a new allow rule, or a deny rule or protected path taken away is refused, `auth::loosens`), and
  changes when threads settle (`/api/prefs/permissions`, `/api/prefs/threads`), never switches a
  thread to a mode that asks nothing, or turns on a switch named like one (`engines::asks_nothing` in
  `api::set_setting`; answering a question with an engine's own "always allow" still works); a phone's
  request is marked `auth::Narrow`, and a Hub that forwards it to another machine sends
  `x-hub-narrow` so the limits hold there too (a hubd from before ignores it), reads a thread's pull request and merges,
  approves or comments on it (`/api/github/pull`, `/api/github/action`, its diff `/api/github/diff`),
  reads a thread's git line, has a commit message drafted and commits and pushes it
  (`/api/threads/{id}/gitstate`, `branches`, `gitdraft`, `ship`; feature `phone.git`, not `switch`),
  shows the parts of a project's page (`/api/phone/project/git|docs|backlog|board`, features
  `phone.projectgit`, `projectdocs`, `projectbacklog`, `projectboard`, `project_parts.rs`; only a
  folder that is one of Hub's projects). Git is read only: the default branch with its newest
  commit and CI (`gh run list`), open pull requests and recent branches. Docs lists the repo's
  markdown with FOCUS, ARCHITECTURE and DESIGN pinned. The backlog (`docs/BACKLOG.md`, `- Title #tag`)
  and the board (`docs/BOARD.md`, `## To do|Doing|Done`, cards may end in `#724` or `thread:<id>`)
  are markdown in the repo, no outside service; a POST names an item by line and title and is
  refused when the file changed meanwhile (`add`, `drop`, `board`, `move`),
  browses and reads a project's files (`/api/phone/files/tree|file|raw|search`, feature
  `phone.files`, `phone_files.rs`: only a folder that is one of Hub's projects, never a path that
  leaves it even through a link, never a project that is the filesystem root, the home folder or above it, or holds Hub's data folder, or lies in a folder of logins on the way down from home (so on Windows `%TEMP%`, which is in `AppData`) or in Hub's data folder other than the dev checkout and the Home desk, judged by the part below that folder since on Windows it is itself in `AppData` (`phone_files::too_wide`, since a phone may add any folder as a project), nothing that looks like a secret such as `.env`, keys, `.git`, CLI logins (`.config`, `.claude`, `.codex`, `AppData`, `*credentials*`) and Hub's `hub.db`/`hub.json`,
  pictures and PDFs up to 20 MB as they are; the desktop's `/api/project/file` stays closed),
  shows a picture the thread's agent named by path in its answer, one the agent saved itself and
  so has no image event (`GET /api/phone/threads/{id}/picture?path=`, feature `phone.threadpicture`,
  `phone_files::thread_picture`: png, jpg, gif, webp, avif or bmp only, and only a file that lies
  inside that thread's own folder once links and `..` are resolved; the path may be absolute or
  relative to the folder), runs an HTML file it named the same way as a live page
  (`POST /api/phone/threads/{id}/page` with `{path}`, feature `phone.threadpage`,
  `phone_files::thread_html` then `pages::save_file`: `.html` and `.htm` only, inside the thread's
  folder; answers `{thread: "files", page}` for the page route),
  reads the problems Hub found and starts a fix thread for one or archives it
  (`/api/problems`, `/api/problems/{n}`, `.../thread`; only a numeric id, so `sources`, `rules`
  and `vakten` stay closed), reads the routines, runs one now and switches it on or off
  (`/api/routines`, `.../run`, `POST .../enabled`; feature `phone.watch`; editing a routine's
  prompt or folder stays on the machine), reads usage over time (`/api/stats`, `phone.stats`) and the
  devices let in (`/api/devices`, `phone.devices`; revoking stays on the machine), picks provider, model and effort before a thread starts (`GET /api/phone/engines`: the engines this
  machine can start with their providers, models and thinking levels, and the model and effort
  options each offered in its latest thread, so they show before a new thread has run, and `POST /api/phone/warm`
  which starts the engine so it can say what it offers; `POST /api/phone/threads` takes `settings`
  and `warm`, and only `hub_route`, `model` and the effort settings get through, never the
  permission mode; feature `phone.engines`; a value that starts with `~` is a name the engine
  resolves when it starts, `phone.pickbyname`; a thread whose folder is Hub's own desk or
  `platschef` folder is refused, since its brief only hands work on), carries a project to a machine that lacks it
  (`POST /api/phone/bring {from?, path, to?}`, feature `phone.bring`: the path must be a project the source
  machine lists, or lie inside one (a thread's working folder in it: the listed project is carried), and a
  refusal names the path; it goes to `<that machine's projects folder>/<name>` through the same carry as the
  folder picker's `/api/folders/bring`, so no GitHub login is needed there; the phone never names the
  target path or a clone URL), and sets its own notices
  (`/api/phone/notify`), and reads `GET /api/phone/now` (`now.rs`, feature `phone.now`): the
  threads that work (with `since`, so a clock runs without asking again), the ones that wait on
  you with the permission question they ask (request id, what, command, options) and the plan
  limits, in one small answer for a widget or a lock screen. It states facts only; whether a
  question is safe to answer from a lock screen is the client's to judge, and one it cannot judge
  it opens. Never: secrets, the config by hand, engines and their keys, files,
  terminals or commands, adding machines, pairing. It is revoked in Settings > Access.
- The page's own requests carry its origin; they are let in only when the origin is the address
  asked for and that is this machine or its tailnet name (`phone::own_page`), or the phone page of
  another machine on the same tailnet (`https://pc.<tailnet>.ts.net` asking
  `clanker.<tailnet>.ts.net`, `phone::sibling_page`, also allowed by CORS), and only with a
  token. A page elsewhere that points its name at this machine gets nowhere.
- The page writes everything as text, never as markup, and is served with a strict
  Content-Security-Policy, no referrer and no framing.

### The iPhone app (proof of concept)
<!-- covers: ios/ -->

Tests (`ios/scripts/test.sh`, on a Mac with Xcode): `ios/tests/ReleaseNotesTests.swift` and `UpdatesTests.swift` are plain Swift programs; `ModelTests.swift` is built for the iOS simulator together with every file in `ios/Hub/` except `HubApp.swift` (the `@main`; `Nav` lives in `Nav.swift` so the rest compiles without it) and run with `simctl spawn`. It checks what a hubd of any version sends (`HubThread`, `Lossy`, `HubEvent`), the inbox sections and dots, `Folded` (the event log folded into items: replies, tool groups, asks, quick replies, reactions, side jobs, pages, plan timings, the New line), `Store.tail`/`projectRoot`, `ThreadIndex` and pairing links. Add a case there when a model or fold rule changes. No cloud runner has Xcode, so the script runs on the Mac.

Focus on the phone (`ios/Hub/FocusMode.swift`, `Nav.focus`): the desktop's focus mode, kept per phone in `UserDefaults` (`focusProject`: path, name, machine). Set it with a hold on a project row, or Set as working in a project's ··· menu; Unlock and Leave let go. While it is on, the project button in the pill carries the project's colour and a lock and goes straight in from any tab (`Nav.enterFocus`, `FloatingBar.item`); the Projects page keeps the project as a hero card on top (`FocusHero`) and the rest as quiet rows (`ProjectRow`, which also takes the hold); another project opens as a peek with a bar, Back to <project> (`PeekBar`); and the inbox shows only that project, with a bar that counts what the others hold (`FocusBar`, `InboxView.matches`). Held means only counted: the ntfy notices of other projects are not stopped yet. A project's first mode is Home, a grid of its parts with one fact each (`PartTile`); Git, Docs, Backlog and Board are parts that need new calls in hubd and come in a later Rust build.

Chat detail (feature `phone.chatdetail`, `GET/PUT /api/prefs/chatdetail`): how much of the agent's work a
thread shows, with the desktop's levels and wording (`Store.detail(of:)`, `TurnView.levelRows`). A thread's own
level (its menu, Chat detail) wins, then the default (Settings > Chat detail), then Minimal in Chat and Quiet in
the work world. Minimal shows one playful word while it works and the answer plus the "Worked" chip after; Quiet
adds the step it is on, in one line, and folds the rest to "Worked for …"; Normal also keeps the latest note and
a running command's last lines; Full leaves every group and command open. Questions, failures, files and pages
are never folded. A hubd without the feature keeps the older drawing.

`ios/` holds a native SwiftUI app, a second client for the same phone token as the page above. It
follows the sketches in the project files (`iphone-app/putsad/`): the inbox as the home screen
(needs you, working, done; allow, deny, quick replies and try again on the row; New thread and the
mic by the thumb) and a thread with its steps as grey rows and an ask docked above the reply box.
The first start follows sketch 5: scan the Your phone QR code with the app's own camera (the
camera app would open it in Safari and use it up there), or type the link, then a check that
Tailscale and the machines answer, and the inbox. The made-up evening from the sketches is only
there when asked for (a link on that screen, or `-demo 1` at launch). The code is redeemed at
`/api/pair/redeem` and the token kept in the Keychain. The frame is drawn by the app
(`Frame.swift`, sketch "Ramen"): on the four root pages one floating pill of four icons (inbox with an
amber dot while something waits, projects, machines, settings), a round search button beside it on
the inbox only, and the violet pen, the only coloured button. A tap on the icon you are on scrolls
that page to its top (`Nav.topTick`). A finger that stays on the pill is its own gesture: slide across
the icons and the tab changes under it with a tick for each (live, you land where you let go); hold
on an icon (0.45 s, no movement) and `RecentPlaces` (`Places.swift`) lists the last five places
opened in that tab (`Nav.remember`, filled by `PlaceWatcher` from the four stacks, in memory only),
where one move to a row and a let-go jumps there (`Nav.open`), as the pen's menu does. On a pushed page (a project, a machine, a settings page, a
thread) the pill steps aside and `HubHeader` draws the top: a round back button, a capsule with the
title and a status line, and the page's round buttons (⋯ menus). Edge swipe from the left goes back
on every page (`Gestures.swift`; Hub takes over the swipe's delegate, so it also lets the edge swipe win as UIKit's own does: a scroll view waits until it has failed and the pages' own drags run beside it instead of cancelling it), and so does a swipe to the right anywhere on a page, except one that starts in a `swipeZone`: a code block, table, row of chips or the reply box, anything that scrolls or swipes sideways itself, and the machine line of a new thread, which changes machine. `ThreadTriage` (settle, snooze) leaves the same zones alone, and the page's back swipe is off while a sheet stands over it. A page of a thread that is gone (`ThreadGone`) keeps Hub's own top. The bar's widest form (Dev's inbox: pill, search and pen) is 388 pt A page with modes (a project's Focus, Threads, Ideas, Files and Library; a machine's Overview, Threads, Ports and Run; Changes; a pull request; a Markdown file's Read and Source) is a `ModePager` (`ModePager.swift`): each mode is its own pane with its own scroll, the pane follows the finger, a long or quick enough swipe goes on to the next mode, at the last it gives a little and springs back, and at the first a swipe to the right is the page's back swipe (so the page has `swipeBack(enabled: mode == 0)`). Only the pane you are on is built; a neighbour is added while a finger drags, and from the first move of a swipe until the page has settled the panes take no touches, so a row under the finger is not opened by the swipe. Taps are told from other touches as iOS's own lists do (`TouchGuard.swift`): a watcher on the window (`TouchWatch`) sees every touch without taking it, claims a touch that lands on a gliding list so it only stops the list, and notes a touch that drifts more than 10 pt or comes within 0.3 s of one of Hub's own swipes (`TouchGuard.swiped()`); rows act through `safeTap` or check `TouchGuard.ok` (`PanelRow`, the model and project pickers), so none of those opens. The pills (`Tabs`, `ModeSwitch`) glide to the mode and `Tabs` scrolls the chosen one into view; a mode chosen on a pill slides in from its side like a swipe (the pager draws its own `current` and follows `selection` with a slide), and panes never fade, so two modes never show through each other.
(14 pt at the sides, 8 pt between the pill and the round buttons), so it fits a 390 pt phone: a bar
wider than the screen makes the whole root wider and slides the pill and the pages out on both
sides, which is how Dev's inbox once sat 6.5 pt further out than Chat.

It then works as the phone page does (`Store.swift`): one `/api/stream` socket per machine, the
other machines found through `POST /api/phone/machines` and followed directly, one merged list,
calls about a thread sent to its own machine or through the paired one. A thread's log is kept by `seq`: an event from either stream is put in its place when it is not there yet, and a hole in the numbers (a lost stream message) is fetched again from the first missing `seq`, so a message written on the PC reaches the phone. A thread's log is folded
for the phone (`Folded` in `Model.swift`, the page's `fold`): messages, replies, tool calls as grey
rows, permission asks with the agent's own options, quick replies, failures. The inbox loads the
log of each thread that needs you, for its ask and options on the row, and reads the card
(`GET /api/threads/{id}/peek`) of recent finished ones for their last line; an older hubd that
refuses a phone the card just shows less. A native client sends no `Origin`, so hubd checks its
bearer token like any other client, under the same `phone::may` limits.

The inbox follows sketch 1: a swipe right settles, a swipe left snoozes (an hour, tomorrow 08:00),
a tap opens the thread. Each change shows at once and can be undone for 5 s; it goes to `/tidy`,
and a hubd too old for that call puts the row back and says so. Opening a thread marks it seen.
The line under the title is the state, "2 need you · 1 working · 3 up" with the rows' dots (amber,
green), or "All clear" when nothing waits (`InboxView.statusLine`; "up" is the machines that
answer). The machine and project chips (and Quick) filter the rows; they show when a filter is on or
when "Filters" in the title's menu is chosen. Pulling the list down or the pill's search button opens
search (`onScrollPhaseChange`; it also asks the machines for the latest a second later, which the pull
used to do alone), and the pill's round button turns into a close button. Search takes the title's own
place (`InboxView.searchRow`, as tall as the title and its line, the filter menu at its right), so
nothing under it moves when it opens and folded groups stay folded; once something is typed only the
groups with a hit remain, open, and while typing or filtering settled threads count as hits and come
in a Settled group. A hold on a
row (`Nav.picking`) picks many: the title becomes "2 selected" with Done, and the pill is replaced
by an action bar with Settle, Snooze (an hour, 08:00, when the limits reset), Pin and Archive. The
filter button by the title opens the shelf of settled, snoozed and archived threads and also picks
many. The shelf (`ShelfView`, `Tabs.swift`) uses Hub's own `Tabs` strip for its three lists and its
rows swipe (`StripSwipe`, `Swipe.swift`: the row follows the finger, a strip behind it names the act, a
line with a tick arms it): Settled right brings it back to the inbox (`Tidy.reopen`) and left archives,
Snoozed right wakes it (`Tidy.wake`) and left snoozes it until tomorrow, Archived right restores it
(`Tidy.unarchive`). The rows are a swipe zone, so the shelf's sideways swipe between lists is the
strip's tap only. The Chat list's rows swipe the same way: right archives, left opens `SpacePickSheet`.

A tap on the pen is a new thread in the project last worked in; a hold (`PenMenu` in
`Capture.swift`) offers New thread in that project (and its machine), Talk an idea, New thread in…
(the project picker first) and Thread without a project. The hold goes on as one move: `PenButton`
(`Frame.swift`) reports the finger in `Nav.penFinger` while it slides, the row under it lights with
a tick, and a let-go (`Nav.penLetGo`) on a row chooses it, outside the menu closes it, and lifting
without moving leaves it open to tap. The menu can also be dragged down or toward the corner: it
folds back into the pen. Talk an idea is a screen of its own, and a swipe down closes it (words
already said are kept as a draft, none just closes); it
listens at once with `Dictation`, the mic held speaks and let go stops (a tap starts and stops
too), the words show as you say them, and Save draft keeps them on the phone (`IdeaDraft`, in
`UserDefaults`) with the project Hub guessed (the last one). Drafts sit at the bottom of the inbox
as their own group; a tap opens a new thread with the words in the box and takes the draft away, a
swipe deletes it. Nothing runs until you send.

Mobile coding starts validate the selected folder on the destination machine before creating a
thread: missing folders, canonical aliases of Home, and inherited Home briefs in moved folders
are rejected with a clear error. Explicit No project still creates a scratch folder. Archived
threads do not choose the default coding project.

A new thread is a chat (sketch 2): an empty page with the keyboard up and a line saying where it
goes, by default the project last worked in, its machine and its last engine; tapping the line
opens a sheet of chips to change them. It is sent to the project's machine directly when the phone
follows it, else through the paired one with `?host=`. The mic is the iPhone's own speech
recognition (`Dictation.swift`, on the phone when it can, Swedish or English): tap the mic in any
reply box. Swedish goes to Apple's recognizer
on its servers, which hears it far better than the phone's own model. Projects (sketch 5) groups
the projects as going on, this week and quiet, each a card with its threads as dots, what is left
to test and its stage from the focus summaries; a project's page has FOCUS.md with boxes to tick,
its threads and its ideas. Machines (sketch 3) puts the machines that need you first, each a card
with CPU, RAM, disk and temperature rings coloured like the desktop's limits; a machine's page has
Ready for work, its projects and what runs there. Under them the plan limits and the panic button.
Review changes (thread menu, or the Changes chip) lists the thread's files with plus and minus and
opens each diff; Looks good or Ask for changes (typed or said) goes back to the thread as a message.
Settings (sketch 4) shows the three groups with what holds now; the phone changes only its own
things there (dictation language, forgetting the machines), the rest says it is set on the PC. The thread menu pins, snoozes, settles, archives, stops and copies
the last reply.

The thread and the new thread were rebuilt on T3 Code's mobile app as the model
(`ThreadView.swift`, `Composer.swift`, `NewThreadView.swift`). A thread is folded into turns, one
per message from you (`Folded` keeps each item's time and turn): an earlier turn shows your
message, one "Worked 3m 05s · 12 steps" line that opens, and the agent's last words; the latest
shows everything, its tool calls in a grey group whose steps open to their command and output. The
feed follows the end until you scroll up, then offers a button back down (in the middle, clear of the suggested replies; it also stops a feed still gliding from a flick); a message you send shows at once at the end and the feed goes there, before the machine's own event for it comes back (`Store.sentLocal`); while the agent works one line sits at the end of the feed, where the answer grows
(`WorkingLine.swift`, sketches 21 and 25 A): the desktop's working grid playing Pour (`WorkGrid.swift`, the same loops as `workGrid.ts`; Reduce Motion holds it still; `Working` is the grid wherever the phone shows that something works: the inbox's working dot, the tool group, web search, and busy Send, Merge, Run and Fix buttons, in place of system spinners, while plain loading of a list or image keeps the system one), what it does in plain words from the latest live
event (`Folded.activity`: Thinking…, Reading…, Searching…, Searching the web…, Editing…, Running code…,
Generating image…, Building the page…, Writing…, Waiting for you while an ask is open, else Working…) and
the time in grey; there is no bar above the box and Stop is only the box's red button. In Chat the thread
shows no tool rows at all: while it works only that line, and after the turn one small grey chip under the
answer, "Worked 38s · 4 steps", that opens a "What it did" panel with the turn's tool groups as Dev shows them;
Dev keeps its tool groups in the feed; an open ask is docked above the box with big buttons. The box is
shared by both screens: it grows to five lines, keeps a draft per thread across launches, sends
only on the arrow (Return is a new line), offers Stop when empty while the agent works, and turns
into a dictation bar on a tap of the mic (cancel, level and time, done; the words show as you
speak and land in the box). A new thread asks "What should we build in <project>?" with the
project as an underlined word (`NewThreadSheets.swift`, sheet "Ny tråd" screens 1 to 4): a tap opens a
short sheet of one line per project with the machines it lives on at the right, a search, and No
project last (More… opens the full searchable picker); the machine is its own quiet line under the
question with a state dot, a tap opens "Run it on" (every machine with online or the reason it is
grey; an online machine that lacks the project says "Bring <project> here", which copies the project
there with `POST /api/phone/bring` (`phone.bring`: `from`, `path`, `to`, answers the `folder` on the target) and picks that machine and folder, with a spinner while it works and the server's message in the row on a failure) and a swipe sideways cycles the online ones. The model is the logo and name in the box and
opens the model sheet, with Safety as a button there; there is no row of chips above the box any more. All the sheets are one-line rows of 38 pt. dated scratch folders are not offered as projects. When the machine's hubd cannot start threads
from the phone it says so before you write. A thread shows the pictures it made (from the
project's library, over the tailnet, full screen on a tap in `ImageViewer`, `ImageViewer.swift`), and a project has a
Library tab. The viewer shows the pictures of that thread (or project) as one row: swipe sideways or tap a thumbnail to change
picture, pinch up to 8x or double tap (fit and 2.5x on the spot tapped) to zoom, one finger pans while zoomed (the page then
neither turns nor closes), a pull down at fit size shrinks the picture and fades the black into the page and past a line
(or a flick) closes it, one tap hides the buttons; the top has a close, a count and Share.

In a thread the top is three pills, after the Claude app: a round back button, the title in a
capsule with the inbox row's words under it (where it stands, then project and machine: "Needs you ·
Hub · PC"), and a round ⋯ for the thread menu. The title is what the thread is, ⋯ is what you do with
it (sketch 33b): a tap or a pull down on the title drops a card out of it (`ThreadAbout.swift`) with the
name and a pencil, the engine, the machine (moving the thread there, `POST /api/threads/move`), the
project and when it started; a hold opens it ready to rename, and only the pencil brings the keyboard.
⋯ keeps Settle, Pin, Snooze, Review changes, Pull request, Copy the last reply and Archive. The page
slides in solid: its background reaches behind the status bar (`containerBackground` for the
navigation), and the feed always starts at its latest message: `defaultScrollAnchor` for the first
frame, and since the log arrives after the page and the lazy rows settle their heights for a moment,
every change of the feed's height puts it back at the end until the first touch or about a second
after the conversation came in (`landing`). It no longer jumps up to the "New" break; the New pill
goes there. The turns already there come
without the spring a newly sent message has. The agent's plan for the turn (ACP `plan`
updates) sits above the box (`PlanDock.swift`, sketch steg-ruta 7): a card ticked off while it works,
where a swipe down folds it to a pill with the step now and "2 of 3" (kept, `plan.folded`), a swipe up
opens it again and a tap on the pill slides every step up as a sheet with how long each took (the
fold keeps when each step started and finished, `PlanStep.started`/`finished`, and when the plan came,
`Folded.planAt`). When the turn is over it is a small "3/3 steps" pill, ticked when all are done, that
opens the same sheet, until you write again. One moving cube per thread: while the plan shows, the step
at work has it and the working line goes without. The box has + for
context (camera, photos, files on the phone, another thread), sent as `attachments` with the
message (and with a new thread: `POST /api/phone/threads` takes them too). What you sent shows small above your words in the feed (`SentFiles` in `RichPictures.swift`): a photo as a 44 pt square that opens big, a file or link as a chip with its name. A message Hub wrote to the agent itself is one faint line that says what it was (`Folded.hubNote`); a side job's result has none, since its side job line already says it. The engine's model and
effort as a short label that opens a panel: providers first, then the engine's own settings as it
reported them (`session_state` and `config_option_update`), set with `POST /api/threads/{id}/settings`,
effort as rows with a filling ring; and @ to mention the files the thread changed, threads and
projects. Panels are Hub's own (`Panel.swift`), not Apple sheets, and kept compact: rows are 40 pt, the title bar and the corners small, at most 440 pt tall.

Sheet "Tråden" and "Git och granskning" (`ThreadExtras.swift`, `Review.swift`): typing `/` at the start
of the box raises the commands the phone can do above it, narrowing as you type (`/diff` opens the
changes, `/plan` and `/compact` write in the box; Return never sends, only the arrow). When the agent
planned first and nothing has begun (`Store.planToApprove`, the latest `plan` with every step
pending) the plan is a card above the box with Approve and run (answers the agent's own allow when
it asked, else sends a go-ahead) and Change (the plan quoted in the box), and the header says
"Needs you · plan". In the engine panel an engine that is signed out or missing on the thread's
machine stays listed, greyed, with the reason in amber, read from that machine's `engine:<name>`
readiness check. Holding a message gives Copy and Quote in reply. Inside a thread you go
through the inbox like TikTok (`ThreadTriage.swift`): a swipe to the right settles the thread and opens the next
one (a green strip grows from the left edge), a swipe to the left snoozes it for the first of the snooze
choices, an hour, and opens the next (the night strip with the moon), and pulling up past the end of the
feed, when the pull began with the feed at its end (24 pt of slack), shows the next thread's card peeking up and opens it
without doing anything to this one (about 42 pt of rubber band, a short pull, arms it; 30 pt keeps it armed). All three
swap the top of the stack in place, so `RouteView` gives each `ThreadView` its thread id as identity:
otherwise SwiftUI kept the old page with the new id and never opened the new thread's log. Each has a line (about 110 pt of finger for the swipes, with 20 pt of
hysteresis): a tick and a solid strip past it, letting go there does it, before it springs back; Undo shows
as usual. The left 24 pt edge stays the system's back, and with no next thread the swipes go back to the
inbox and the pull does nothing. A chat (`Store.isChat`) is not in the inbox, so `ThreadTriage` is off there: a swipe to the
right anywhere is plain back (`ChatBack`) and a swipe to the left on a message pulls out a reply mark and
quotes it in the box (`MessageActions`, `Environment.swipeToQuote`, set only in chats). A thread draws its
latest twelve turns; a line at the top says how many are folded, and pulling the feed down past its top (when the
pull began there, with a tick at the line) or tapping that line opens twelve more above, the first turn
shown staying where it is. "Next" follows the inbox's order (`Store.inboxGroups`: Needs you, Done,
Working, From Hub, Snoozed with Inbox grouping on, else Needs you, Done, Working). A git line under
the header ("3 files changed +42 −7", or the pull request the thread watches) opens a panel with
Review changes, Commit, Push and Open pull request; the phone has no call for those three, so they ask
the agent in the thread, and a line says Hub doing it itself needs the next hubd (`gitstate`,
`ship`, `branches` and `github/diff` are not in `phone::may`; the branch name and "pushed or not"
need `gitstate`). Changes is one mode switch, Files | Diff | Comments: a tap on a line number in the
diff opens a box under the line, the comments gather in Comments and Send puts them in the thread as
one message naming each file and line. The pull request page is Overview | Files | Discussion with
Approve and Merge at the bottom (Merge as before).

More of the sweep (`Files.swift`, `Watch.swift`, `Screens.swift`): the thread's git line and panel use
`phone.git` when the machine has it: the branch and "not pushed" under the header (`GET
/api/threads/{id}/gitstate`), Commit, Push and Open pull request done by Hub itself with the words
from `POST …/gitdraft` and `POST …/ship`, refreshed afterwards; the pull request page's Files opens
each file's diff from `GET /api/github/diff`. Plan limits show a pace marker (`LimitRow`): the bar is
what is used, the white line how much of the window (5 hours or a week, read from the label and the
reset time) has passed, amber when ahead of it. A project has a Files mode (`phone.files`: `GET
/api/phone/files/tree`, `/file`, `/raw` and `/search`, read-only): folders open in place with the
path as tappable parts, a file opens on its own page (code with line numbers, markdown read or as
Source, pictures large), Ask about this starts a thread naming the file, and a hold offers Copy path
and Mention in a new thread. A swipe to the right inside a folder goes up one folder (`FilesTab` keeps
the listings it has read so the folder above slides in; `PageSwipe.rightGoesUp` makes the mode
pager leave that swipe alone), and only at the top of the project does the pager's swipe go to the
mode before. Settings gets a Watch group (`phone.watch`, only where the machine has it)
with Problems (one row per kind of error with a seven-day spark, a problem's page with Fix in a
thread and Archive) and Routines (the PC's list with a switch, `POST /api/routines/{id}/enabled`, the last runs, Run now, which opens
the run's thread). Where a machine lacks a feature the view says it needs the next version of hubd.

Settings also gets Usage (`Usage.swift`, `phone.stats`, `GET /api/stats`): answers and tokens for 7, 14
or 30 days, by model and by project, as Claude Code's own logs give them; there is no per-day series
in `Stats`, so no daily chart, and nothing about money. The Devices page lists, under the phone's
own routes, the devices paired with the Hub (`phone.devices`, `GET /api/devices`, read only: it is
cut off on the PC).

The model picker (`Engines.swift`, sheet "Ny tråd" screen 6, feature `phone.engines`): the box shows the
engine's logo (Claude's spark, OpenAI's knot) and the model picked, and a tap opens one sheet with
the company as a tab at the top (every engine the machine can start, from `GET /api/phone/engines`,
Claude, Codex, a harness on DeepSeek and so on, not only the ones seen on old threads), its models as
one-line rows, the thinking effort as a compact row and Safety as a button. A new thread starts the
engine while the page is open (`POST /api/phone/warm`) and reads the options the agent reports as
events of the warmed thread (Claude's models, Codex's); a harness that runs on several providers
(Hermes, OpenCode) offers its provider-and-model route and each provider's efforts from the engine
list. What is picked goes with the thread (`settings`, and `warm`, to `POST /api/phone/threads`). A
thread's engine panel is the same picker: the engines as tabs, the model and effort of the open one
at the top, and a button to carry on with another engine, where one that is signed out or missing
stays listed with the reason. A tap on a model of another engine carries on with that engine on that model. A pick shows from the thread's own `setting` event, so it stays chosen while the engine is stopped and only takes it at its next start. Only the newest models show (the latest of each family and anything from Claude 5.5 or GPT 6 up, plus the favourites and the chosen one; a hold on a model favourites it, "Show all" lists the rest, as the desktop favourites do). The sheet is never empty: it reads the agent's options (the warmed thread's), else `options` of the engine entry (the model and thought_level options of its latest thread), else a built-in list (Claude: Fable, Opus, Sonnet, Haiku with Low to Max; Codex: Astra, Sol, Terra, Luna with Low to High; any other engine just Default). A pick from a built-in list is a hint (`hint:` ids): it shows as the real choice of the same name once the agent's own list is known, and it is sent as a name (`model: ~terra`, `thought_level: ~medium`, `PickerData.sendable`) when the machine has `phone.pickbyname`, which the engine looks up when it starts, so a name it does not know cannot fail a start and a pick is never silently dropped. The project lists leave out Hub's own desk folder (any folder named `desk` or `platschef`, since a machine can keep Hub's data elsewhere) as an internal folder, so it is never the default. Without `phone.engines` the tabs are the engines seen on threads.

Quick threads (sheet "Snabbtrådar"): a thread in a scratch folder (a dated folder, on any machine) is a quick
thread (`HubThread.isQuick`). It stays out of Projects, which shows one fixed row "Quick threads · N" with
the states as dots above the pill; a tap opens the inbox with the Quick chip on (`Nav.quick`), which
filters the rows and makes the title line count only those. A quick thread's row and header say
"Quick · <machine>" instead of the project.

Two worlds, Dev and Chat (`Chat.swift`, `NewChat.swift`, `Modes.swift`, `Theme.swift`; sheets 8 to 11): one app, one
skeleton, two themes. `Theme.world` picks the palette every `Hub.*` colour reads (Dev as before, Chat
"warm": bg #201b16, accent #e2b883, a serif title); the world button, a dashed circle first in the pill (a
speech bubble in Dev, a terminal in Chat), or a swipe down from the top edge changes world, and each
world remembers what it had open (`Nav.saveWorld`). Chat's pill is Chats, Spaces, Artifacts, Settings. A chat
is a thread in a folder named for its time under `chats` (`HubThread.inChatsFolder`, made by `POST
/api/phone/threads` with `chat: true` when the machine has `phone.chat`; without it a scratch thread the
phone remembers by id, `Store.chatIds`). Dev's inbox, counts, quick threads (only `scratch` folders) and
Projects leave chats out, and Chat lists only them. Chats is only the list (groups New answers and Earlier,
chips for space and order); the pen opens a New chat page (`NewChatView`): the box with attach and the mic,
Search (asks the engine to search the web and give links, `Web.ask`; the thread is named by your words) and
Image (asks it to make a picture), and the
engine and model in the box opening the model sheet (without Safety: a chat asks before it acts). Claude on
Sonnet is the default, GPT (Codex) on Terra the other, the last pick is remembered; the machine is chosen
for you, the PC, else tincan, else a VPS, else any other that is up (the MacBook last), and said in one
quiet line, or the page says that none is up. Spaces are a grouping kept on the phone (`ChatSpace`,
UserDefaults), Organize files loose chats into them. Artifacts (`ArtifactsView`) is what the chats made,
read from the libraries of their folders (`GET /api/project/outputs`, one read per folder at once,
only what that folder's chats made) and listed by kind: Pages (what the agent showed with
`show_page`, built with Hub's kit, as small sleeping thumbnails, `PageWeb` at half zoom, that open
big in `PageSheet`), Pictures (a grid, full screen on a tap), Docs and Files (rows that open in the
file reader when the machine has `phone.files` and the file is in the chat's folder, else the chat).
Chips filter by kind and by space; a hold opens the chat that made it; pull down reads again past
the minute the libraries are kept. Servers stay in Dev.
Subscriptions only. The change between worlds is one of four animations (Settings > Appearance, "Switching
between Dev and Chat", with Try buttons; default Split: a swipe slides, a tap grows a circle): A slide
sideways, B circle reveal from the button, C soft lift, D cascade of bands. Each works from a picture of the
old world (`Snap`, `drawHierarchy`) with the new world drawn live in its colours (`WorldTransition`, animated
by one progress value). Cut for now: Chat's thread is Dev's thread view in warm colours (its tool and diff
chrome is not hidden), the pill's icons do not roll one by one in C and the pill moves with the page in A,
and Organize is a list to file by hand rather than a suggestion.

The short sheets of New thread and New chat (project, Run it on, model) stand over a dimmed page and close with a tap outside them or a pull down on them; they have no Done button. The pull works like drawing a bow (`Bow.swift`, also on `HubPanel`): a drag that scrolls a sheet's list stays a scroll, also when it reaches the top; a fresh pull down with the list at its top draws the sheet down with growing resistance (the list itself does not stretch, `NoBounce`, so only the sheet moves), a tick marks the line, and letting go past it (or a flick down) closes the sheet, before it the sheet springs back. The page's default project is the last real one (never a chat, a dated scratch or chats folder, or one of Hub's own), else No project. New chat starts the engine while the page is open (`POST /api/phone/warm`, `chat: true`) and sends the `warm` id with the thread, so the first answer does not wait for the engine to start. A tap on the world button while a change between worlds runs is ignored.

The two worlds share one layout (`Layout.swift`, `Metrics`): the pill, its dashed world button, the search button and the pen are one `FloatingBar` at the same frames in Dev and Chat (the pill is sized so Dev's inbox row, with the search button, fits without shifting it), and every page's title block is a `PageHead` of a fixed 42 pt row and 18 pt status line, so Chat's serif title and Dev's bold one leave the chips row, the section headers and the first list row at the same Y. Only colours, the title's typeface and the content differ. Engine marks: Claude's spark, the OpenAI knot for Codex (`OpenAIMark`, the Simple Icons path as a shape) and a wing for Hermes (`HermesWing`).

Sheet "Valark" (sheet 19): the model sheet has a star tab first once you have favourites (a star on a row keeps a model; the tab lists them across engines and a tap picks engine and model; a thread's Engine panel is the same picker with the same star tab, where a favourite on another engine carries the thread on with that engine on that model); picking a model closes the sheet or panel a moment after the check shows (a reasoning level leaves it open), the models as one-line rows with a star and a check, the older ones folded into "Older models · N" at the foot, and Reasoning as one row that lists the levels. Only the newest models show (Claude 5.5 and GPT-6 up). The project sheet has the layout of the full picker, smaller: a search, Recent first with what waits on each, the rest grouped by machine (Hub's own and hidden folders left out), a hold pins or hides a row, and No project and Add sit at the foot. "Run it on" has no Done button: a tap on the dimmed page, a drag down or a tap on the row in use closes it. Choosing a model for a model several harnesses can run (harness groups) waits for the server side.

A failed call shows the server's own sentence (`{"error": "…"}` is read in `HubClient.Failure`, never shown as JSON). "Bring <project> here" sends the project's folder as its machine lists it (`GET /api/projects` on the source: the listed project whose folder holds the thread's), and a model or reasoning pick in a thread's engine panel is drawn at once and kept in front until the machine reports the same value, so an older value arriving first never flashes back.

The new inbox (`InboxCards.swift`, sketches 23 and 24, switched off and on under Settings > Appearance > Inbox grouping): the desktop list's rows, two lines each (the project in its colour with its folder and the engine mark, the machine in a small chip and the state at the right: how long it has waited in amber, the timer in green while it works, "Done" in violet; then the dot and the title), in groups that fold (Needs you, Done, Working, From Hub and Snoozed, in that order, Working below Done as on the desktop because it needs nothing from you, the last two shut by default, a tap on a heading folds it and the choice is kept), each group one rounded card with a hairline between its rows and a colour rail per project on every row (a quick thread says "Quick"). What waits for you stays in Needs you wherever it came from; From Hub holds Hub's own folders (`isInternal`). Search opens by a pull down and the filters (Quick, machine, project) by the filter menu; a row no longer carries Allow and Deny, the thread has them. A swipe to the right settles; a swipe to the left is the night swipe (Settings > Appearance > Snooze swipe: moon): a star sky that twinkles and a moon that grows from a crescent while you pull, three nights from `Store.snoozeChoices` (an hour, tomorrow 08:00, when the limits reset) to tap (they are uncovered from the right as the row moves, and the moon grows in the sky to their left), a long pull past a line (with slack both ways and a tick when crossed) makes the first night grow over the whole strip and letting go takes it, with no words swapped on the way, and if you hold the swipe still for about a second and a half a shooting star crosses the sky with a light tap, no night is chosen and the row springs back. With the moon off the sky is plain and the choices the same.

Notices go through the free ntfy app (`ntfy.rs`), since Apple lets only paid accounts push: the
phone sets a topic on every machine it follows (`/api/phone/notify`), and each machine posts to it
as its own threads need you, fail or finish (`Hub::set_status`, next to the desk note), with
`Click: hub://thread/<id>` so a tap opens the thread in the app. Quiet hours, on the phone's clock
(it sends its UTC offset), hold back what is done, never what waits for you. The phone keeps the
settings and the topic itself (`Notices.swift`): the topic is made once and kept, and each refresh
gives every machine that is up the phone's settings where they differ, so a machine that was
updated, asleep or new sends to the same ntfy subscription without anything set up again; Send a
test asks each machine to send one. A notice ntfy refuses is logged as a warning. While the app is
in front it tells each machine what it shows (`POST /api/phone/looking` {active, thread}, feature
`phone.looking`, `Store.look`: on coming to the front, every minute, when another thread opens, and
`active: false` on going to the background), and the machine holds back every notice about that
thread and "Done" for the others, which the inbox shows anyway; it keeps the word for 90 seconds
(`ntfy::held_back`), so a phone that went away without saying so hears about everything again soon.
The desktop says the same about itself (`POST /api/presence` {client, active, thread, idle_ms},
feature `presence.desktop`, `presence.ts`): the main window tells the machine that runs the open thread (`?host=`
forwards it) every 30 seconds while it is in front, when you switch thread or window, and `active: false`
when it leaves. A window counts as at the computer while it is in front and the last key or mouse movement
(`idle_ms` plus the age of the word) is under 3 minutes (`ntfy::at_pc`); then nothing is sent about that
thread. A question or approval is not dropped but looked at again after 45 seconds and goes out if it still
waits and you have left. The word runs out after 90 seconds, and the phone's own switch is `quiet_at_pc`
(default on) in `/api/phone/notify`, a row under Notifications > Also in the iPhone app (`Quiet while I'm at the PC`).
The thread menu (⋯) is a Hub panel: Settle and Pin first, the snooze times (an hour, 08:00, when the
limits reset), review changes, the pull request (checks, what it does, Approve, Merge), copy the last
reply, and archive last; rename, the machine and the engine are in the card the title opens.
The app has no Apple dropdowns or confirmation sheets left (the long-press menus and the New space name box are still Apple's):
`HubMenu` (`HubMenu.swift`) is the drop-in for `Menu { Button… } label:`, a small dark card that grows out of the control touched
(below it, or above when it is low on the screen) and closes when a row is lifted, drawn once at the top of the app
(`HubMenuHost`, `Nav.menu`); `.hubConfirm` (`HubConfirm.swift`) is the drop-in for `.confirmationDialog`, a dark card rising from the
bottom (`Nav.confirm`) with a line on what happens, a plain button, or for what stops things (Pause every agent, Forget the
machines) a button held for 0.9 s until it fills; a swipe down or a tap outside cancels. Settings pages change Safety, notices, when threads settle, and update a machine's
hubd; Projects' + adds a project folder.

**Where the app stutters** (`Perf.swift`, `Haptics` in `Haptics.swift`): a quiet meter keeps `perf.log` in the
app's Documents folder (read it with `xcrun devicectl device copy from --domain-type appDataContainer
--source Documents/perf.log`, 400 KB at most, one old copy). It has no screen. A line says "hitch 58 ms ·
ThreadView.body×3 Prose.body×12" when a frame came late (a display link) with what ran in the last second
(how many times a page was rebuilt or a text laid out), "slow fold 14 ms (3200 events)" for work over 6 ms
(folding a thread's log, merging the thread list), and notes where a panel opened or closed, a sheet was
pulled, a swipe began or ended and a thread was opened. A page is rebuilt whenever any of its state changes,
so the texts are parsed once and kept (`MarkdownBlock.cached`, `inline`, `NSCache`), a panel's animation sits
on the panel and not on the page beneath it (`hubPanel`), and the haptics come from generators kept and
prepared (`Haptics.tick`, `firm`, `ready`) instead of a new one at every tick. The app runs its animations at 120 Hz on ProMotion iPhones (`CADisableMinimumFrameDurationOnPhone` in `ios/Info.plist`); the meter's own display link asks for 60 Hz so it never holds the screen at 120 by itself. Each act has its own feel instead of one light tap for all: a settle, an answer, a ticked test or a kept idea is a success (`Haptics.done`), a snooze a soft tap and a softer one (`snooze`), a send one crisp tap the moment you press it (`sent`), a failure an error (`fail`), an agent asking for permission a warning (`asks`), a finished turn a soft tap (`arrived`), and a mode, pill or undo a selection tick. What you tap answers the press (sketch "Känsla"): rows and cards lighten and sink a little while held (`PressStyle`, `.press`, `tapRow`), dense lists (files, settings, pickers, panels) only lighten (`.pressFlat`), an inbox card does the same through its long-press's pressing state; letting go of a settle holds the strip while a check is drawn on it (`CheckMark`) before the row flies out, a snooze puts the chosen night's moon to sleep (`moon.zzz`) for a moment before the row glides out, and the strip behind a leaving row fades with it (`leaving`) so nothing is cut off, the send button dips (`DipStyle`) and the message you send springs up from the box. The inbox does not resize with the keyboard (`ignoresSafeArea(.keyboard)`, with room left at the end of the list while you search), the machines are asked for the latest a second after the search opened, and a merge that finds the same threads changes nothing (`Store.merged`), so a stream's repeat of what is known does not rebuild the pages. A list of projects (the Projects page, the project sheet and the picker in New thread) takes the threads grouped by project once per redraw (`Store.threadIndex()`, `ThreadIndex`) and a thread's project folder is remembered per path (`Store.projectRoot`), because asking `threads(of:)` per row went through every thread several times per row at every key typed in the search. The list of projects (`Store.projects`) is worked out from every thread, so it is kept until the threads, the stored projects or the machine change (`threadsVersion`), and `isInternal` keeps its answer per project; before that the inbox asked for it once per thread at every redraw (a slow `inboxGroups` shows in perf.log). Still open: the log is folded
from the first event at every new event, and the four root pages are all alive under a pushed one.

New versions go to the phone over Wi-Fi (`ios/scripts/deploy.sh`): the Mac finds the paired iPhone
with `devicectl`, builds a Release numbered by the time (yyyyMMddHHmm, shown at the bottom of
Settings) and installs it, no taps on the phone. The phone needs the cable once, Developer Mode,
and the same Wi-Fi. An install link over the tailnet (`itms-services`) was tried: the phone
downloads the app, but iOS does not install a build signed by a free Apple ID that way, and says
nothing. A free Apple ID signs for 7 days, so deploy at least weekly.

The Xcode project is hand-written (`ios/Hub.xcodeproj/project.pbxproj`, object version 77) with
the `Hub/` folder as a synchronized group, so new Swift files need no project edit. Build for the
simulator with `xcodebuild -project ios/Hub.xcodeproj -scheme Hub -sdk iphonesimulator
CODE_SIGN_IDENTITY=- build`. It must be signed, even ad hoc with no team: an unsigned build cannot
use the Keychain and forgets its pairing on every launch. At launch `-open <thread id>` opens that
thread and `-pair <link>` pairs without typing. Paths from Windows and a thread's own worktree
(`<repo>/.hub-threads/<id>`) both name the repo as the project.

**What a thread holds besides words** (`Rich.swift`, `RichProse.swift`, `RichPictures.swift`, sketch "K1, utan ram": content straight on the
chat, one small grey label above it, nothing boxed). The thread folds the same events as the
desktop and draws them alike. The wavy line where something changed (`Brk`, `BreakView`, colour by
kind) marks an engine switch, a plan-limit wait ("ran out, continued on Codex", "Stopped by a plan
limit", both subscriptions out), a thread moved between machines, and "New" after an hour of quiet. An engine switch and "New" belong to the message after them and sit above it, not among the turn before (whose pages and "Worked" chip are drawn after its items). The orange `new` break is
what happened while you were away, as on the desktop: `Folded(events, since:)` places it before the
first thing the agent did after `since`, which `Store.open` takes (the later of `leftAt`, kept in
UserDefaults when you leave a thread or the app goes to the background, and `seen_at`) before it
marks the thread seen. It reads "New · 3 replies · 12 min ago", the feed opens at it when more than
a few things are new, and an "↑ New" button at the top of the feed jumps back to it while it is out of
sight (`onScrollVisibilityChange` on the break sets `Store.newInView`).
A page the agent showed with `show_page` (`page` event, versions by `replaces`) is a `PageCard`: a
web view of `/api/threads/{id}/pages/{page}` that sleeps until tapped, passes the page's height,
ready and open messages on through a small script (the page is the top window there, so its
`parent.postMessage` lands on itself), answers with Hub's colours like `PageFrame.svelte`, opens big
in a sheet and has Copy link, Open in Safari on a hold. The sources line under an answer is read
from the web addresses in the turn's tool calls, not from the agent. A turn you asked as a web search
(`Web.swift`, the same rules as the desktop's `web.ts`) shows your words with a globe, "Searching the
web…" with a turning globe and the sites it finds in place of the working line until the answer starts
(`WebSearching`), and the answer's links as cards under it with the list it ends on taken out of the text
(`SourceCards`; the pages it read when the answer has none). A chat may always use the internet, so its box
and New chat have no web switch and send no instruction with your words; chats searched with the instruction
earlier still show as searches. A picture the agent made (an ACP `image` block in a tool call, which is how Codex's image generation answers, with the data inside the event) stands on its own below the work group as a `Picture`: tap opens it big with pinch to zoom, a hold has Copy and Share (the share sheet saves it). A picture the agent saved itself and only named in its answer ("saved as D:\Dev\chats\…\gw.png") is fetched by that path from the thread's machine (`ReplyPictures`, `GET /api/phone/threads/{id}/picture`, which gives only pictures inside the thread's folder, feature `phone.threadpicture`) and shown under the answer the same way; without the call the path stays plain text. `Prose` sets the Markdown as
blocks: headings, lists, quotes, hairline tables (a column of numbers lines up right), pictures
at web addresses, code.

## Smaller pieces
<!-- covers: crates/hub-core/src/autologin.rs, app/src/lib/components/SignInCard.svelte, app/src/lib/dictation.ts, crates/hub-core/src/login.rs, crates/hub-core/src/autopull.rs, crates/hub-core/src/looks.rs, crates/hub-core/src/locked.rs, crates/hub-install/src/ignore.rs, app/src/lib/looks.svelte.ts, crates/hub-core/src/connections.rs, crates/hub-core/src/connections/, crates/hub-core/src/devlog.rs, crates/hub-core/src/hub/devlog.rs, crates/hub-core/src/disk.rs, crates/hub-core/src/files.rs, crates/hub-core/src/git.rs, crates/hub-core/src/health.rs, crates/hub-core/src/host.rs, crates/hub-core/src/lan.rs, crates/hub-core/src/machine.rs, crates/hub-core/src/hubd_args.rs, crates/hub-core/src/mute.rs, crates/hub-core/src/reach.rs, crates/hub-core/src/signin.rs, crates/hub-core/src/sshkey.rs, crates/hub-core/src/stats.rs, crates/hub-core/src/tailnet.rs, crates/hub-core/src/triage.rs, crates/hub-core/src/hub/triage.rs, crates/hub-core/src/voice.rs, crates/hub-core/src/voice/, crates/hub-core/src/voice_config.rs -->

Core modules with one job each, described at the top of their file; this is where they show up.

| Module | What it does | Where |
|---|---|---|
| `locked.rs` | `locked()` on a std `Mutex` takes the guard even if a thread panicked holding it, so one panic does not cascade; use it instead of `lock().unwrap()`. `send_or_log(&tx, msg, "what")` replaces `let _ = tx.send(..)` where the message matters and logs a `warn!` when the channel is closed. `read_locked()` / `write_locked()` do the same for an `RwLock`; `.warn_on_err("what")` on a `Result` logs the failure of a write or event instead of `let _ =`; `.ignore("why")` on a `Result` (any error type) or an `Option` is the deliberate discard (the trait lives in `hub_install::ignore`, re-exported here, so the app's shell and `hubd` use the same one; `mock-agent`, which links no workspace crate, has a private copy): it does nothing at run time and says at the call site why the failure is expected (a receiver that may be gone, a process already reaped); an empty reason fails a debug build. `clean_file` / `clean_dir` (and `_async`) are best-effort clean-up: a file Windows says is in use or access denied (os error 5 or 32) is logged at debug (`pages::prune` tries it again on the next new page); other failures are a `warn!`. | Everywhere |
| `autopull.rs` | Every few minutes pulls a project's default branch when the folder is clean and on it, after T3 Code's "Automatically pull". | Project page |
| `connections.rs` | Which MCP servers and claude.ai connectors each engine's agents can reach (asked with `claude mcp list`, `codex mcp list`; their files when the CLI is missing), each engine's plugins (`claude plugin list`; Codex's from `~/.codex/plugins/cache`, with the skills each brings) and which tools the threads called. A plugin switched off reaches Claude as `enabledPlugins` and Codex as `-c plugins.<id>.enabled=false` plus a `skills.config` rule per skill, since Codex lists every skill to the model within 2% of its context and warns when they do not fit. A thread in `<projects root>/chats/` (an everyday chat) starts Claude without any of it (`lean`: no connectors, `strictMcpConfig`, no setting sources; see "A chat starts lean"). | Settings > Extensions, the thread panel |
| `devlog.rs` | A devblog entry per day worked (gist, Shipped / Worked / Blocked / Next), written at night, and the searchable history; the writer, "While you were away" and the search are `Hub` methods in `hub/devlog.rs`. | Overview, Labs |
| `disk.rs` | How much room Hub takes (the build folder most of all) and how much is left; where the build folder is and whether there is room to build. | Settings > Updates, a machine's page |
| `files.rs` | Files in a folder for @-mentions, skipping what git ignores. | The composer |
| `git.rs` | The one way Hub runs git (`rules::raw_git` flags any other): `output` (no prompt, no stdin, killed after `TIMEOUT` 60 s or `LONG` 600 s for fetch, push, bundles, checkouts), `output_with` (the caller's env and stdin, for checkpoints), `output_async` (on a blocking thread, for async code), `run` (stdout or None), `checked` (trimmed stdout or `git <args> failed: <stderr>`, which `dev::git` and `update::git` are), `short_commit` (a 12-character version name). Also just enough git for the thread header: the branch and how many files changed. The branch is read from HEAD without starting git; `run_cached` keeps an answer for 30 s unless the index or HEAD moved, for pages that ask every few seconds. | Thread header, Labs (dev mode), every git Hub runs |
| `health.rs` | The weekly look over every project without running anything: CI, uncommitted or unpushed work, stray branches, gone quiet. | Overview |
| `host.rs` | What is listening on this machine and which thread, Hub or something else started it; what kind of computer it is (`machine_kind`: a Raspberry Pi by its board, a VPS by its hypervisor or firmware vendor, a laptop by its chassis or battery, a Linux box without a desktop as a server). | Workers, a machine's page, every machine icon |
| `looks.rs` | Icons the person picked over the automatic ones: a machine's kind, a project's icon, colour or picture (at most 96 KB, PNG/JPEG/WebP/GIF). The automatic project icon is picked in the app from the name (`app/src/lib/looks.svelte.ts`): a word in the name hints at one (bot, voice, game, docs...), else a hash of the name picks icon and colour. The Work list writes the project's name in that colour too. The app also remembers which machines said they run macOS (`looks.isMac`): their rows in the inbox show an apple as the machine icon (no card of their own: every row is the same, the selected one a grey fill). | The sidebar, Work, project pages, machines |
| `lan.rs` | Machines on the home network that SSH answers on, for setting up one not on the tailnet yet. | Settings > Machines |
| `machine.rs` | Opens a terminal on a machine over SSH: a shell, a root shell or an engine's own CLI (through `hubd cli`, signed in as threads are). On this computer when it is Windows, `admin` opens the shell or CLI in a Windows Terminal window as administrator (`Start-Process -Verb RunAs`; Windows asks once, the window is outside Hub). | A machine's page |
| `dictation.ts` (app) | Keeps dictated words in the message box while they are still heard: each phrase owns a stretch of the text, its live guess is redone as the speaker goes on and replaced in place by the final text. Typing elsewhere moves the stretches along; typing inside one hands it to you, so a late result never overwrites your words. | Dictation (Ctrl D) |
| `mute.rs` | Holds a push-to-mute key down while dictating so a voice chat stays quiet. | Dictation |
| `reach.rs` | What one thread reaches beyond its own folder: extra folders the agent may look in and connections switched off for that thread only; applied when its engine starts, and a change restarts an idle engine. | The composer's + |
| `signin.rs` | Keeps engines signed in on every machine with a year-long setup token (`hubd sign-in`) instead of the short login, and says when one is lost. A token made on one machine goes to the others with the engines (`Shipment.sign_ins`, `take_sign_ins`): only ones made there travel (`Kept.from`), and a newer one made on the receiving machine stays. `hubd root` opens a root shell (sudo) with that sign-in in its environment, so `claude` in a Root shell is not asked to log in again. `hubd cli <engine>` runs the engine's own CLI with that sign-in and without provider keys; a machine page's Claude and Codex buttons open it there (`machine::command_for`, falling back to the bare CLI on a hubd too old for it), so the CLI in a window is signed in the way threads are, also on a Mac over SSH where the CLI's own login is locked in the Keychain. On a Mac the CLI's own login is in the Keychain, which hubd (a launchd daemon) cannot read, so there the card and Ready for work ask for Hub's sign-in. The card's button opens the machine by the name this machine knows it under (the thread's host), not its hostname. | Ready for work, the sign-in card |
| `autologin.rs` | Starts the link-and-code sign-in by itself on a machine that comes online with an engine not signed in (a ready check whose fix is `SignInHere`, Codex today): the PC asks each online machine for `/api/machine/ready`, runs `POST /api/engines/{name}/login` there, waits for the link and code, and keeps them in `GET /api/sign-ins/pending` (feature `sign_ins.pending`) and shows each on Home as one row (`SignInCard.svelte`: the code to copy, Open link, Later hides that code only, New code when it ran out; polled every ten seconds, none from a hubd too old to know) and sends a phone notice through ntfy (the same switch as permission questions, not held back by quiet hours). A sign-in nobody finished is offered again after six hours, never sooner; one that is signed in drops off the list. Engines with a lasting token (Claude Code) are not handled here: the token travels with the engines. `hubd cli <engine>` signs the engine in first when it is not signed in on that machine, so a machine page's Codex or Claude button no longer opens a CLI that only says so. | Ready for work, a notice on the phone |
| `login.rs` | Signs an engine in on a machine from the app when its sign-in is a link and a code (`codex login --device-auth`): `POST /api/engines/{name}/login` runs the engine's `sign_in` there in the background, `GET` gives the link and code it printed; Ready for work shows them (`ReadyFix::SignInHere`). When the CLI that signs in is missing there (only the engine's ACP adapter was set up), it is installed first (`engines::install_command`, `engines/install.rs`, which `hubd sign-in` and the ready check use too): the harness's installer, else its npm package, installed with `--prefix <data>/hub/tools` (no sudo; `install -g` into a system prefix failed for a person without root), and Node.js 22 first into `<data>/hub/node` where the machine has no npm (the same download as `install-host.sh`, so a Raspberry Pi on Debian works). A machine whose CLI is missing gets the sign-in as its fix in Ready for work, so `autologin.rs` installs and signs in by itself the first time the machine is seen. `hubd sign-in` does the same in its window. | Ready for work on a machine's page |
| `sshkey.rs` | Gets this machine's SSH key onto a fresh machine with its password once. | Set up a machine |
| `stats.rs` | Which models answered, tokens used and what that would have cost at API prices, per model and project. A Claude app worktree (`<repo>/.claude/worktrees/<name>`) counts to its repository, and the app's projectless chat folders (`scratch-workspaces/`) to one "Claude app chats" row. | Overview, Library |
| `tailnet.rs` | New machines found on the tailnet, set up from there with one click. | Settings > Machines |
| `triage.rs`, `hub/triage.rs` | Keeps the activity list short by itself: each thread's pull request, settling threads idle for days or merged. The settings and types are in `triage.rs`, the loop on `Hub` in `hub/triage.rs`. | Activity list |
| `voice.rs` | Speech to text with whisper.cpp locally (KBLab's Swedish models); downloads the binary and models on first use. Keeps one whisper-server warm per model: the live preview runs on small (processor) or medium (graphics card), and on a graphics card the final text of each phrase runs on kb-whisper-large, fetched in the background (falls back to medium for the session if it will not run). whisper-server gets Silero voice activity detection (`ggml-silero-v5.1.2.bin`, under 1 MB) so silence is cut before whisper hears it. Whisper's prompt starts with the thread folder's name, branch and changed files (read at most every 30 s). A start that fails is tried again on a fresh port, says why from `voice/whisper-server.log`, and the recording falls back to whisper-cli. The graphics card build is given up for good (`gpu-failed.txt`) only if it never worked here (`gpu-worked.txt`); after that a failure moves just that recording to the processor. With `voice.polish` (Settings > Dictation), `POST /api/voice/polish` has the tidy engine's small model correct the dictated text when dictation stops (`Hub::polish_dictation`), keeping the wording; an answer far off in length is dropped. In the app, dictation is `ComposerVoice` (`app/src/lib/composerVoice.svelte.ts`: recording, phrases, mute, microphone, tidy) and the mic button with its menu is `MicMenu.svelte`; `Composer.svelte` only gives it the box, and acts on Ctrl D only when it owns the key (`dictationScope.ts`: the focused box, one already recording, or the visible one when nothing is focused; a key the idea box already took is left alone). The composer keeps the start of a live guess still (`Steady` in `app/src/lib/dictation.ts`: words two guesses agree on are locked). | Dictation (Ctrl D) |

## Known limits

- Tested end to end against `mock-agent`, and smoke-tested by hand against the real
  `claude-agent-acp` 0.99.1 on Linux (2026-10-30). The replay is built
  (`mock-agent replay <trace.jsonl>` plays the agent side of a `trace_engines` recording back to
  Hub; `tests/it/replay.rs` runs a thread on it). `crates/hub-core/tests/fixtures/` holds one
  recording against `mock-agent` and three from the PC (2026-11-10): `acp-claude-tool-turn`
  (claude-agent-acp 0.99.1), `acp-claude-permission` (a Write that asks, Manual mode) and
  `acp-codex-tool-turn` (codex-acp 2.5.0). Their scratch folder, e-mail and token are rewritten and
  the command lists cut to three. Each has a test in `replay.rs`, so a change in an adapter's wire
  shape fails a test instead of a thread. Not recorded yet: a Codex permission question, plan mode,
  a model switch, a stopped turn. A new adapter version is a new recording next to the old.
  The updater runs the suite as its gate on the machine Hub runs on, so a test that reads that
  machine (a real login, an installed CLI, a browser) or that waits on a tight clock fails
  updates there; such tests take what they need as arguments or stay behind an environment
  variable (`HUB_TEST_BROWSER`). Smoke-tested against the adapter: a coding task with permission questions,
  Customs, tests and a commit; Esc mid-turn with a queued follow-up; and a core restart mid-turn,
  which the thread carried on from. Not yet on Windows, and not recorded as contract tests.
