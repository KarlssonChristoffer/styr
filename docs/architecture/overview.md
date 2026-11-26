# Architecture: Overview, home, cloud threads, dev mode and clean-up
<!-- för: agent -->

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## Overview
<!-- covers: app/src/lib/components/Overview.svelte, app/src/lib/components/Board.svelte, app/src/lib/components/CardBody.svelte, app/src/lib/overview.svelte.ts, app/src/lib/board.ts, app/src/lib/components/OverviewShelf.svelte, app/src/lib/components/CardFrame.svelte, app/src/lib/components/CardMenu.svelte, app/src/lib/components/HowCounted.svelte, app/src/lib/components/DayCard.svelte, app/src/lib/components/DevlogCard.svelte, app/src/lib/components/ClockCard.svelte, app/src/lib/components/DaylightCard.svelte, app/src/lib/components/TimeCard.svelte, app/src/lib/pixels.svelte.ts, app/src/lib/daylight.ts, app/src/lib/components/LinesCard.svelte, app/src/lib/components/RecordsCard.svelte, app/src/lib/components/StatsCard.svelte, app/src/lib/components/NowCard.svelte, app/src/lib/components/PulseCard.svelte, app/src/lib/components/LimitCard.svelte, app/src/lib/pulse.svelte.ts -->

The Overview page (`Overview.svelte`) is a search over your history and then a board of cards you
choose and place. The board itself is `Board.svelte`, which Home uses too (see "Home and New
thread"): it takes a `page` ("overview" or "home") that decides which saved layout and which
starting set it uses, and what each card draws is `CardBody.svelte`, one place for both pages. The
board's arithmetic (`board.ts`) is 48 columns of 4px rows; each card has a spot (x, y,
width in columns, and a height in rows or none, meaning as tall as its content, measured by
`measure`). Cards never overlap: `settle` takes them in reading order and drops each as high as the
cards above it in its columns allow, so removing or shrinking a card lets the ones below rise, and
dropping one on another pushes that one down. Customize turns the page into the board: drag a card
by its body to move it, its left, right and bottom edges or corners to size it (double-click the
bottom edge to go back to fitting the content), × to take it off; Esc cancels a drag and Ctrl Z
or Undo steps back. The Add cards panel (`OverviewShelf.svelte`) lists every card with a small
picture, search and groups; click to put one where it first fits or take it off, or drag it onto
the board.

A card is only its content; everything around it is the board's. The board puts one frame
(`CardFrame.svelte`) around every card: no heading unless you switch the title on, no pickers, no
buttons, no "?". What a card used to draw in its own head is declared once in the catalogue
(`CARDS` in `overview.svelte.ts`) as its knobs (`Knob`): a choice (the period of When you work,
Code in and out and Models and cost; the Clock's shape), a switch (the map's sources, When you
work's Colour by daylight) or an action (Check now, Play the last year, Open Settings > Customs).
The menu draws them under Settings and the card reads its values with `knob` (`setKnob` writes
them); an action reaches the card through `onAction`. Values live per card in
`prefs.overviewFrames`, with the title (off by default; on, it says the card's name or whatever
you typed); the Clock's old `clockKind`, `overviewDaylight` and the map's old source switches are
read when a knob is unset and written along with it, so an older Hub on the same window agrees.
How a card's numbers are counted is written in the card (`HowCounted.svelte` registers it in
`about`) and shown under the content when the menu's How it's counted is on. Every card shares the
frame's measure: 14px inside, one line of small grey text under the picture (`.caption`), one
figure size (`.figure`), so the cards read as one page. Right-clicking a card, with or without
Customize on, opens its menu (`CardMenu.svelte`): Title, Settings (the knobs, and How it's
counted), a colour from the palette (`TINTS`, never a status colour) that tints its frame,
background and title; see-through lets the page show through its background; Hide the frame
drops border and background; S, M, L, Full or Fit content set its size; Del takes it off. Two cards can share one frame:
while customizing, drag a card onto the middle of another (the board holds still and a veil says
Drop to combine; near an edge it is placed there as before) and they become one card, named by
its parts ("Clock and Daylight"), with one spot, one look and one menu; the parts sit under each
other, or side by side once the frame is 520px wide, with the gap between them as the dividing
line. Split in the menu gives each part a spot of its own again, and × takes them all off. A
combined card's spot keeps its parts in `with` (`Spot`, `board.ts`), so the parts have no spot
and no look of their own; any card, a widget too, can be combined. Looks are kept per card in
`prefs.overviewLooks` and reach the cards' own `section` through classes and CSS variables on the
slot, so a card needs no code for them. The menu's Linked with list connects cards, and each link
shares Time, Look or both (two small toggles beside a linked card). Time: a day picked in one (a
map square, a bar in Code in and out) or a range (Month, 90 days…) reaches every card linked to it
for time, directly or through others, that can take it (`TAKES`, `pickDay` and `pickRange` in
`overview.svelte.ts`); a range lands on the nearest one the card has (`takeRange`). Only cards in
`TAKES` can share time. Look: colour, see-through and frame set on one card are set on every card
linked to it for looks (`setLook` walks `reach(id, "look")`), and linking looks gives the partner
the look of the card whose menu it is. Any two cards, widgets too, can share looks. Only a range
handed while the card is on the page counts, so re-adding a card or reopening Overview starts it on
its own range. Links go both ways and are kept in `prefs.overviewLinks` as `[a, b, shares]` (a pair
saved without the list shares time); the starting set (`["map", "day", ["time"]]`) links the map
to the Day card and applies only while that pref is unset. Lines between linked cards show only
while customizing: dashed for time, dotted for a look-only link.
Below 760px the cards stack in reading order. The starting board is the map (two thirds)
and everything else a third, three to a row; Day and Devlog keep their text in a reading-width
column. The board is a window setting (`prefs.overviewBoard`, null for the starting one); a page
saved as the old list of S/M/L/Full cards (`prefs.overviewCards`) is laid onto the board the way it
looked. The catalogue lives in `overview.svelte.ts`: a new card is an entry there and one line in
`CardBody.svelte`. Looks, links, titles and knobs are kept per card id and shared by the pages, so
the same card looks alike wherever it is put; only the layout is per page (`prefs.homeBoard` for
Home). Reset on Home starts only Home's layout over.

The cards: the activity map and the Day card (`overview.day`, which only `pickDay` sets, through the
links: the map does not write it itself, so unlinked the Day card stays on its own day), the devlog, the health
check, models and cost, three playful ones, and two about the sun. The sun ones (and the daylight
colours in When you work) work out where the sun is at your place (`prefs.place`, Stockholm until
set under Settings > Appearance) from the date alone (`daylight.ts`: the NOAA approximation, no
weather, no library): Daylight (`DaylightCard.svelte`) says today's hours of sun and sunrise and
sunset, with yesterday's as a small diff in grey and gold (never red and green: a shorter day is
not an error), and Clock (`TimeCard.svelte`) shows the time in a ring that is the day as light, as big as the card lets it,
midnight at the bottom and noon at the top, violet through the night, orange at dawn and dusk and
gold in the day, with notches for sunrise and sunset, a flat sun that turns into a moon through
the twilight, a trail, stars in the dark, and the seconds as a thin arc; the Shape knob in its
menu picks Ring, or the day as 24 squares with the hour ringed and the minutes filling in as
Boxes, Grids (hours, minutes and seconds as three grids) or Rails (three rows). Daylight draws the
day as a picture: a horizon coloured by each hour's light, the sun's arc from sunrise to sunset
with the times at its feet, yesterday's arc dotted behind it, the night's path under the horizon,
and the sun (by day) or a crescent moon (by night) where the sun stands now, with the hours of sun
and the difference from yesterday in one line. When you work colours each hour by how light it is
today (`daySlice`, every weekday row alike, so the map changes with the season), with the count as
how solid the colour is; Colour by daylight in its menu switches that off and on, and Play the
last year runs the window over the last year (click the map to stop). Its line says how much of
your writing was done in daylight, by the sun as it stood when each message was written (the
twilight counts for how light it was); its cells, like the map's days, are whole pixels worked out
from the card's width, so they line up at any zoom, and this hour today is ringed with a dot in
it. The map draws as many weeks as fit the card, up to two years, with days that fill the width;
its totals are always the last twelve months and follow the source switches in its menu. Models
and cost is a band of each model's share of the tokens in its maker's colour (Anthropic violet,
deeper for the bigger model; OpenAI teal; Google sky; DeepSeek blue; open models lime), then a
row per model, the projects and threads that cost the most, and the totals in one line. Health
check is one square per check across every project in its status colour, a row for each project
with something wrong, and the count in one line; click a square or a row for that project's
checks.
The playful cards read `GET /api/pulse` (`Hub::pulse`, database only, so it answers at once) and
`GET /api/pulse/lines` (`Hub::lines`, git, slow only the first time); both are asked once per visit
and shown from last time at once:

- When you work (`ClockCard.svelte`): your messages in Hub and prompts in Claude Code as hour by
  weekday in this computer's clock, and the busiest three hours in a row.
- Code in and out (`LinesCard.svelte`): lines your commits added and removed per day, on a
  square-root scale so one huge day does not flatten the rest.
- Records and badges (`RecordsCard.svelte`): biggest days, longest streak and longest thread, and
  badges with how far along each is. All of it is counted in the page from the Pulse, so a new
  badge needs no change in the core.

The live cards (group "Right now") change by themselves, from the stream the window already has,
and fetch nothing on a timer. Now (`NowCard.svelte`) lists the threads that work, each with a
clock that runs from `sent_at`, and the ones waiting on you first; a plain permission question
shows in the card with Approve and Deny (the log of a waiting thread is followed so the question
is there), a risky one only offers Open. Pulse (`PulseCard.svelte`) draws tool calls a minute over
the last ten minutes: `notePulse` in `pulse.svelte.ts` counts each `tool_call` update as it
arrives in `HubState.apply`, so it counts only what this window saw. Limit (`LimitCard.svelte`)
is a ring of what is left of the plan window that is fullest, with the time to its reset
counting down and, once a tenth of the window has passed, where the pace leads by the reset; the
numbers are `hub.plan`, only the clock is new.

### Widgets
<!-- covers: crates/hub-core/src/widgets.rs, crates/hub-core/src/widget_builds.rs, app/src/lib/components/WidgetCard.svelte -->

A widget is a card an agent writes on request, with no rebuild of the app. Make a widget in the
Add cards panel (typed or dictated) calls `POST /api/widgets` (`widget_builds::create`): Hub starts an
ordinary thread on the default engine in a folder of its own, `<data>/widgets/<id>` (all of them
show as one "widgets" project in the sidebar and in Models and cost), lists the widget as building
in `kv` (`widgets`) and sends `widget_builds::brief` plus the ask, marked `widget` so the transcript
shows the ask as the user's words. The agent writes one live page and shows it with `show_page`;
`widgets::page_shown` (called from the tool) makes that page the card and its title the card's
title. When a turn ends with no page (`widgets::finished`, from `look_at`), the widget is failed
with the reply as the reason. The list and those two take `&Ctx` (`widgets.rs`); starting the
build and asking for a change need the running Hub (`widget_builds.rs`). The panel lists widgets under Yours, greyed while building, and the
page polls the list every few seconds until none is.

`WidgetCard.svelte` frames the page like any agent page (`pages.rs`: sandboxed, scripts but no
network or storage), gives it Hub's colours with a transparent background so the card's tint and
see-through show behind it, and follows its height. The page asks for numbers with
`postMessage({hub: "data", want})` and the card answers with what the other cards already read:
`pulse`, `lines`, `stats` (30 days), `threads` and `now`. A right-click inside the page stays in
the page, so the card has a ⋯ button that opens the same menu, which for a widget also has Ask AI
to change this card (`POST /api/widgets/{id}/change`, a message to the same thread asking it to
replace the page) and Open the thread. The card follows the newest page the thread showed.


### The usage log
<!-- covers: crates/hub-core/src/uselog.rs, app/src/lib/uselog.svelte.ts, app/src/lib/components/UseCard.svelte -->

Hub keeps a log of which parts of it you use, so the Overview can show it as a map (a square per
feature, brighter the more it is used, a dashed frame for what you never touch) and an agent can
say what is in the way. `uselog.rs` holds it: one row per use in `use_log` (schema step 20) with
the feature's name (`thread.composer`), the area it sits in, the feature used just before it
(`prev`, for the flows) and sometimes how long it took. It never holds anything you typed, opened
or named: `clean` lets only names of 1 to 48 of `a-z 0-9 . _ -` through, so a sentence, a path or
a thread title is dropped on arrival. It is append-only, kept 400 days and local to this machine.
`GET /api/uselog?days=` answers counts per feature (with last use and the median time), the flows
between features and whether the log is off; `POST /api/uselog` takes up to 200 lines the page
collected; `PUT /api/uselog/off` switches it off (kv `usage.off`: nothing is kept while it is);
`DELETE /api/uselog` clears it. All four are the feature `usage.log` in `compat::FEATURES`, so a
page asks `hubdHas` before it reports or reads.

On the page, `uselog.svelte.ts` counts: `track("area.feature")` for a page view (`App.svelte` tracks
`page.<view>` and `settings.<section>`) and a delegated click listener for any element marked
`data-use="area.feature"` (the composer's send, Snooze, Settle, the Steps header, the update
badge). Hits wait 8 seconds and go in one call; nothing is sent to a hubd without `usage.log`.
`USE_FEATURES` names the parts the map knows, so one you never use shows as a dashed square;
mark a new feature with `data-use` and add it there. The Usage map card (`UseCard.svelte`, Overview
and Home) draws squares per area, brighter on a log scale, with last use and median time on hover;
its menu has the period, Stop counting and Clear the log.

## Home and New thread
<!-- covers: app/src/lib/components/Home.svelte, app/src/lib/components/Scenes.svelte, app/src/lib/components/scenes/, app/src/lib/scenes.ts, app/src/lib/workmap.ts, app/src/lib/sim.svelte.ts, app/src/lib/components/Board.svelte, app/src/lib/components/ProjectsCard.svelte, app/src/lib/components/TimelineCard.svelte, app/src/lib/components/MachinesCard.svelte, app/src/lib/components/RecentCard.svelte -->

Home and New thread are two pages (two `View`s, `home` and `new`; `hub.goHome()`, `hub.goNew()`) that
share one component, `Home.svelte`, with a `mode`, so the folder, machine and half-written message
stay when you go from one to the other. Both show **scenes** (`Scenes.svelte`): nobody has to lay
anything out. Alt 0 and the sidebar's house button go to Home. Ctrl N, the sidebar's New thread, the palette and
every "new thread here" (`hub.startDraft`) go to New thread, so a draft or a handoff always lands in
the box. Home's desk/platschef folders are excluded from implicit coding defaults. Entering New
thread or applying a project draft resets the previous Home scope. Neither page is remembered as the room's page: going Inbox and back opens the thread you left.

A scene is one of Needs you, Live, While you were away, Projects and Today, and Hub picks the one
that fits (Auto, `pickScene` in `scenes.ts`, a pure function of four counts so it only changes when
the state does): something waits on you (Live, which lists those first under the graphs; the Needs you
scene is a tab), else you came back after a gap with something changed, else something runs, else after 17:00 the day if anything moved, else your projects. The tabs on top are
text links; one picks another scene and keeps it (`prefs.homeScene`), Auto goes back to Hub's pick,
and a tab for a scene with nothing to show is dimmed, not hidden (`dimmed`). The layout never
changes: one line about now (counts only, empty when quiet) and the tabs, a wide column holding the scene's blocks
(a 12-column grid, each block `s5`, `s6`, `s7` or `s12`) and a column that is the same in every scene
(`SceneSide.svelte`: plan limits as meters with their reset, machines and the update). There are no
strokes, only a soft tint behind each block, and nothing is bigger than 18 px. Every scene reads what
the window already has (threads, hosts, plan, the update) plus calls that exist in every core:
`/api/away`, `/api/stats`,
`/api/activity/day` and `/api/project/focus/summaries`, so Home works against any Hub. Needs you
(`SceneNeeds`) lists the threads that wait on you with an Open button, what runs, and the projects' "To test" counts
(`ToTest.svelte`, the unticked lines of each project's `docs/FOCUS.md`). Live (`SceneLive`) is
two graphs, then lists. **Threads working** (`WorkChart.svelte`) is one thin step line of how many
threads worked at each ten minutes of the last six hours, with a scale, a hover that names who was
working, and a row below with an amber dot for each time a thread asked you. **Every thread**
(`ThreadLanes.svelte`) is one lane per thread grouped by project: green while it worked, grey when
it came back, red when it failed, an amber tail while it waits on you; a click opens the thread.
Both are drawn from `workmap.ts`: the latest turn of each thread the window has (`turnsOf`), so a
thread shows once, and the same shape of turn comes from the simulation. Under them: what waits on
you, the running threads with a clock, what finished in the last day and today's models by tokens.
The **Simulate** chip next to the tabs (not on New thread) turns on `sim.svelte.ts`: made-up threads
in five projects that start, finish, ask you and fail on a clock that runs ahead (a tick of 1.5 s is
a minute and a half), and made-up limits that creep up. It is memory of this window only, off at
start, ended when you leave Home, and it writes nothing to the core and touches no real thread, pull
request or setting; while it is on, Auto shows Live and the graphs, lanes and limit meters read it
instead of the real data. The Timeline card is still the Custom board's. While you were away (`SceneAway`) is the old away card as a scene: threads that
finished, failed or asked since you left in time order, what Hub did by itself, commits per
repository, pull requests opened and what the health check found; without a gap to look back on it
shows the last twelve hours, and "Got it" ends the gap (`away.dismiss`). Projects (`SceneProjects`)
is the Projects card's list (same choice, `prefs.homeProjects`) with the last finished threads.
Today (`SceneToday`) is the day's commits, pull requests, lines, threads and tokens, the models and
projects by tokens, and the devlog entry.

The chip beside the folder on New thread also offers **Cloud** (see "Cloud threads").

The seventh tab, **Custom**, is the board that Home was in the first version: an away card, then
`Board.svelte` (page "home"), edited with Customize exactly like Overview (drag, size, right-click
for title, knobs, colour, links; the Add cards panel offers every card, Make a widget included).
Its starting cards are Now, Projects, Timeline, Limit, Machines and Pulse (`HOME_LIST`
in `overview.svelte.ts`). **Projects** (`ProjectsCard.svelte`) lists projects with their running,
needs-you and failed threads (counted by `projectCounts` in `threadFlags.ts`, which the project page's Threads head and `SceneProjects` use too), "Choose projects…" in its menu picks which; **Timeline**
(`TimelineCard.svelte`) draws the last 6, 12, 24 or 72 hours as one lane per project, each thread's
latest turn a bar (green while working, grey done, red failed, an amber tail while it waits on you);
**Machines** and **Recent threads** are cards too. The Devlog card opens the Overview
with that day picked. A new machine's card and any sign-in stay above the scenes in every case.

**On New thread** the scene sits above the heading as two blocks of it (the ones marked `c`) with no
side column (`compact`). It is out of the layout (`position: absolute` above `.mid`), so the box is
exactly where it was without it, and it is hidden, not cut, when the window is too low for it
(`Home.svelte` measures the room above the heading). Nothing lives under the box.

## Cloud threads
<!-- covers: app/src/lib/cloud.svelte.ts, app/src/lib/components/CloudCard.svelte, app/src/lib/components/CloudMark.svelte, crates/hub-core/src/cloud_threads.rs, crates/hub-core/src/cloud_threads/, crates/hub-core/src/hub/cloud_threads.rs, ios/Hub/NewThreadSheets.swift -->

A thread can run on Anthropic's or OpenAI's servers instead of a machine of yours: a Claude Code
cloud session or a Codex cloud task, always on the subscription's own login (never an API key, and
never a switch to one when the plan is full: the thread stops). It is one entry, **Cloud**, in the
same "Run on" menu as the machines (the chip next to the folder on New thread); the model picked
beside it decides whose cloud it is (`cloudProviderFor`: the Claude harness on Anthropic's models,
Codex on OpenAI's; Hermes and Claude Code on DeepSeek have none, so the entry is not offered).
Picking a machine ends it, and it is for one thread: the next starts where it always does.

The core sets `cloud` on such a thread (`CloudRun` in `cloud.svelte.ts`: provider, session id,
page link, repository, branch, state, last line, pull request, a note). The window only reads it:
`cloudOf(thread)` is null for any thread on a machine and for every thread of a core that has no
cloud threads. Where a machine's icon would stand (the inbox row, the thread's header, the meta line
under a title) a thread in the cloud shows the cloud icon with an arrow (`cloudrun`, drawn by
`CloudMark.svelte`; `cloud` alone is the icon of a VPS machine). The model's own mark stands next
to it, so the two together say "Codex, in the cloud". The thread page opens with `CloudCard.svelte`:
how far it has come (started, working, branch pushed, pull request open), the subscription, the
repository and branch, the last line when the service lets Hub read it, "Open on claude.ai" (the
conversation itself lives there) and Stop, and the pull request when there is one. A follow-up is the
box below, as in any thread.

**The core (`cloud_threads.rs`).** `NewThread.cloud` starts the thread in the cloud; `Thread.cloud` (a
`CloudRun`, one JSON column) holds where it stands. Only Claude is started so far: the first message
runs `claude --cloud "<task>"` in the thread's folder (which must have a GitHub remote) inside a
pseudo-terminal (without a terminal the command only accepts an existing session id), reads the
session id and address from what it prints, and the thread is then followed by reading the session
from the same sessions interface `cloud_usage.rs` uses, every 30 seconds, with Claude Code's own
login. The state words, the branch and the pull request are looked for defensively because that
interface is undocumented; one that is not found just stays unknown. Later messages go to the session
with `claude -p "<msg>" --cloud <id>`. Both `claude` and `codex` are found with `engines::locate` (npm global folder and `.cmd` shims on Windows), not by bare name. There is no engine behind such a thread, no checkpoint and no
`turn_end` (those read the local repository), and it does not count as work for `busy`, an update's
wait or keeping the machine awake. Stop only marks the thread stopped and says so in its note: the
session itself is stopped on claude.ai. A failure (no GitHub remote, the command missing, no answer in
three minutes) puts the thread in error with the reason on its card. "Bring home" is not built, and the
card has no button or call for it. The module reaches the thread's
store, card, status and events through `cloud_threads::Threads`, which `hub/cloud_threads.rs`
answers for `Hub`, so it never names Hub. 

The pseudo-terminal answers the cursor-position query (`ESC[6n`) Windows' ConPTY sends before it prints
anything, runs an npm `.cmd` shim's real `.exe` directly (task text never goes through `cmd.exe`), and
a failed start shows the last lines it printed (or that it ended/printed nothing) on the card.

**Following a Claude session.** An `idle` session (it answered and waits for you) is "waiting" (shown
"Your turn"), never "done"; only completed/archived/ended sessions are done. Each pass also reads
`GET /v1/sessions/{id}/events` (undocumented; assistant text blocks only, sub-agents left out) and
adds the new replies to the thread as agent messages; `CloudRun.seen` is the cursor and
`awaiting` holds an idle reading back as "working" until the first answer to the start or a
follow-up has been read, so a session is not taken for finished before it began.

A Codex cloud task is started with `codex cloud exec --env <environment> "<task>"` (the ChatGPT login;
`codex cloud` takes an environment id or label, not always `owner/name`: Hub first reads
ChatGPT's environment calls with Codex's own login (`wham/environments/by-repo/github/<repo>`, then
the whole list matched by label, or the only environment there is), then `codex cloud list --json` for the
environment id earlier tasks of the repository ran in, then tries the
repository's name and its bare name, and when none exists the note says to create the environment in
ChatGPT, Codex, Settings, Environments), its address `chatgpt.com/codex/tasks/<id>` is the link, and
all of them are read together with `codex cloud list --json`. Codex takes no follow-up from the
command line, so a second message to such a thread is refused with a sentence saying so; the pull
request is opened on chatgpt.com.

The iPhone app starts one the same way: `cloud: true` in `POST /api/phone/threads` (feature
`phone.cloud`, refused by the app itself on a hubd without it). In New thread, the "on PC" line opens
"Run it on", where **Cloud** stands below the machines when the model is Claude or Codex and the
project is a real one; picking a machine ends it. A cloud thread shows the cloud icon alone where a
machine's name stood in the inbox, and the thread's About card shows "Runs in", the state and the
branch instead of the machine row.

Starting needs the core to list `threads.cloud` in `/api/health`; until it does the entry is
there but greyed with "next version" (`cloudSupport`), and a hubd that cannot be asked counts as not
having it, so a thread never starts on the PC when the cloud was asked for.

## Dev mode
<!-- covers: crates/hub-core/src/dev.rs, crates/hub-core/src/dev_state.rs, crates/hub-core/src/dev/, crates/hub-core/src/dev_threads.rs, crates/hub-core/src/project_trees.rs, app/src/lib/components/DevMode.svelte -->

Working on Hub from inside Hub (`crates/hub-core/src/dev.rs`, `DevMode.svelte` in Labs). Set up
clones Hub from the updater's clone (which stays untouched) to `<projects root>/hub`, points it at
the same remote and branch, installs its packages and adds it as the project "Hub (dev)". The core
then runs `npm run dev` there on port 1421 with `HUB_PROJECT` set, so the server shows under the
project and in Workers. "Open dev window" opens that in a second app window (`open_dev_window` in
the shell, `capabilities/dev.json`), titled "Hub · dev", which talks to this core like the main
window: its origins are let in by `guard::origin_allowed`. A thread in Hub (dev) changes the clone
and the dev window shows each change as it is saved; undo in the thread takes it back the same
way. A change that breaks the UI breaks only the dev window. A server an earlier core started is
found by its port.

The preview (`crates/hub-core/src/dev/preview.rs`, top to bottom: on and off, the loop, what
GitHub says, the git steps that rebuild the clone, the check of what Vite serves) asks GitHub for
new pushes every 30 seconds only while someone looks: a `GET
/api/dev` or `POST /api/dev/preview` in the last two minutes (`Hub::dev_looked`). Otherwise it
waits five minutes, and the next look wakes it at once. The live window asks for the status once a
minute while it is in front and when it comes back to the front (`live.svelte.ts`), which keeps it
fast there and slow when the window sits in the background.

Packages are kept in step with the clone's `package.json` and lock: after an install Hub writes a
stamp of both into `node_modules/.hub-packages`, and each preview refresh installs again when the
files no longer match it (a merge added a package) or there is no stamp. The install happens
before the preview counts as ready, so Live mode never moves the window to pages whose imports
are not installed yet.

Ship never pushes to the branch the updater follows; only the thread that opened the pull request merges it. It commits
everything to a branch of its own (`hub-dev/<title>-<stamp>`), rebases it on that branch, pushes it
and opens a pull request with `gh`. The words come from the Git button's draft (`git_draft`), whose
commit body becomes the release note, or from what was typed. While that pull request is open, the
next Ship adds to it; once it was merged or closed, the next Ship starts a new branch. The clone's
own copy of the base branch is put back where the remote has it.

"Use a dev core" builds `hubd` from the clone (`cargo build -p hubd`, into the clone's own
`target`), copies the binary to `<data dir>/dev-core` and runs it there on port 7879 with a config
of its own: a copy of this core's database made the first time, no updater, no other machines, no
tmux, and turns it finds cut off are not resumed. The live UI is restarted with `VITE_HUB_URL`
pointing at it, so the dev window talks to the dev core and the main window keeps this one, and the
thread editing Hub is never restarted. While it is on, a change to a Rust file or manifest in
`crates/` (or `Cargo.lock`) rebuilds it and swaps the running one; a build that fails leaves the
previous one running and says why. "Back to this core" stops it and points the live UI back here.
Secrets are not copied, so engines that need one fail in the dev core. Whether it is on is kept
(`dev.core`), so it is built and started again when this core restarts.

A worktree per thread (`crates/hub-core/src/dev_threads.rs`). A new thread whose folder is the
clone (or a folder in it) gets a git worktree of its own at `<clone>/.hub-threads/<id>`, on a new
branch `hub-dev/<title>-<stamp>` from the updater's branch as the remote has it (fetched first, 15 s
at most), and works there; the thread's folder is the worktree, and the app counts it as the
clone's project (`projectFolder`). Two threads on Hub then never edit the same files, and the
preview never parks anyone's work. Git is told to ignore `.hub-threads/` in `info/exclude`, not in
`.gitignore`; it is not under `.git` because Vite neither serves nor watches anything there. The
worktree's `app/node_modules` is a link (a junction on Windows) to the clone's, unless its
`package-lock.json` differs, and `.hub-threads/.cargo/config.toml` points cargo in every worktree at
the clone's `target/`, so a new thread costs no install and no build from scratch. Labs lists the
threads with their branch, changed files and commits ahead (`git_state`). The page asks every two
seconds, so the branch is read from HEAD and the two counts come from `git::run_cached`: kept 30 s
unless the worktree's index or HEAD moved. Running git for each worktree on every ask started about
17 processes a second with ten worktrees, and on Windows each process start leaves kernel memory
behind in some drivers. Every git Hub itself starts goes through `git::output` (`git::run` for stdout): no prompt of
any kind (`GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never`, empty askpass, no stdin) and killed after 60 s (10 minutes for clone, pull, push and moving a thread), so a fetch or a credential helper can never hold a thread. "Show" restarts the live UI in that worktree (Vite may serve the linked packages through
`HUB_DEV_MODULES`) and the dev core, when on, rebuilds from it; Ship then ships that worktree's
branch. "Remove" keeps uncommitted work under `refs/hub/dev-threads/<id>`, keeps the branch when it
has commits and refuses while the thread runs. The clean-up after agent work (below) removes a
worktree by itself once everything in it is pushed; written to again, a thread whose worktree is
gone gets it back on its branch. Off (`dev.worktrees` = `off`) threads work in the
clone as before. A dev core makes no worktrees.

Any project can have the same, without the shared packages and build cache
(`crates/hub-core/src/project_trees.rs`, a switch on the project page, feature `project.worktrees`,
`GET`/`PUT /api/projects/trees`). With it on, a new thread in the project's repository (or a folder
in it, which maps to the same place in the worktree) works in `<repo>/.hub-threads/<id>` on a new
branch `hub/<title>-<stamp>`, cut from the project's base (`remote/branch`, fetched first, 15 s at
most) or, when none is set, from the remote's default branch as the repository last saw it, or
else from where the folder is now. The setting is kept per repository in `trees.projects`; off is
the default, and the folder of a repository that is itself a linked worktree never gets one. For
Hub's own repository the switch is the dev setting above (`dev.worktrees`). The clean-up (below)
treats these worktrees like the others in `.hub-threads` and `bring_back_folder` brings one back on
its branch for a thread written to again; unlike Hub (dev) there is no packages link, so a project
that needs an install runs it in the worktree.

The preview has a third setting beside all and one: `live` (`DevPreviewSet.live`, feature
`dev.live`), which the main window's Live mode turns on. It takes every open pull request that
changes only the pages: one that touches `crates/`, `Cargo.toml`, `Cargo.lock`, `app/src-tauri/`,
`.cargo` or the toolchain file (the paths the updater's fast lane looks at) is skipped "changes
Rust", since its pages would talk to a core that does not have it yet. Each one it shows carries
the files it changes (`DevPreviewPr.files`), so a page that breaks can name it. Turning it on sets
dev mode up first when there is no clone yet. A pull request GitHub lists as open but has no
`refs/pull/<n>/head` for (it happens, as with #422) is skipped "GitHub has no head for it yet":
`fetch_preview` falls back from one fetch to the base alone and each head on its own, so one lost head does not stop the preview.

A pull request that is merged is not open any more, but the preview is built on the updater's
branch as the remote has it, so the live pages already show what it changed. The preview also lists
the ones merged lately that the installed version does not have yet (`DevPreview.merged`, from
`gh pr list --state merged`, kept when the version's commit is an ancestor of the merge commit;
nothing for a development build or a version this clone does not know), saying where the update
stands ("merged, being built", "merged, built, installs next"). Each look at GitHub leaves what it
found in `DevPreview.check` (`DevPreviewCheck`: when, which open pull requests are new or have a
new head since the build, whether the branch moved, or why the look failed), which the live menu
reads after "Look for new pushes now". Whenever a rebuild changes anything under `app/` (or sets
files aside), Vite is restarted, not only when the packages change: its file watcher is not to be
trusted across a branch switch (a push once showed the old page for good), and the page reloads
itself when the server comes back. A new core
also restarts it when it picks the preview back up, since a server an earlier core left running
may already be serving an old file. After each rebuild, and on
every look that finds nothing to rebuild, `check_served` asks Vite for up to three of the pages
the rebuild changed and looks for plain phrases from the changed lines in what it serves
(`phrases`, `serves_old`); a page that has none of them is an old copy, so Vite is restarted
once and the live menu says so, also when it still serves the old one after that.

The build cache (the clone's `target/`, shared by the dev core and the worktrees) has a cap,
10 GB unless `dev.cache_gb` says otherwise: before each dev-core build, past the cap the
incremental cache goes, then the whole debug build. Labs shows its size next to the dev core.

## Starting programs without a console window
<!-- covers: crates/hub-install/src/proc.rs, crates/hub-core/src/proc.rs, crates/hub-core/tests/it/no_window.rs -->

On Windows a program started by a process with no console (`hubd` and the window both run that
way) gets a console of its own, which flashes on screen and vanishes. Every background spawn
(git, ssh, scp, cargo, gh, powershell, tar and the rest) therefore goes through
`hub_core::proc::command` (std) or `proc::tokio_command` (tokio), or `hub_install::proc::command`
in the launcher, the app and `hubd`, which set `CREATE_NO_WINDOW`. A raw `Command::new` fails
`cargo test -p hub-core --test it no_window::`, which scans every source file outside test-only items (`#[cfg(test)]`, read by `tests/it/test_code.rs`).
`command_with_console` (and `tokio_command_with_console`) is the explicit exception for a window
the person asked for or a console they already have: the terminal opened by Take over, `hubd
attach`, `hubd sign-in` and `hub-shell`; `CREATE_NO_WINDOW` there would take the terminal away.

A `git` started this way also never asks for a password: `command` sets `GIT_TERMINAL_PROMPT=0`,
`GCM_INTERACTIVE=never` and empty askpass variables (`no_credential_prompt`). Without that, a
repository whose saved GitHub sign-in is missing or rejected made Git Credential Manager open its
"Connect to GitHub" window every time the update check ran (once a minute), and git waited for it.
Now git fails and the caller shows the error. The update check (`update::fetch`) tries once more
with the GitHub CLI's sign-in (`gh auth git-credential`) before it tells the person to run
`gh auth login`.

`proc::gh_within(root, args, limit)` is how the core asks the GitHub CLI a question with a time limit (signing in to GitHub in `github_repos.rs` starts `gh` itself): it runs `gh` in `root`, waits at most `limit` and answers with what was printed or a `GhError` (`Missing` when `gh` is not installed, `Failed` with its first line of error or that it took too long). It sits in `proc` and not in `project` so that low modules such as `activity` can ask GitHub without naming a module above them (`tests/it/layers.rs`).

## Clean-up after agent work
<!-- covers: crates/hub-core/src/janitor.rs, crates/hub-core/src/tempdirs.rs -->

Threads leave worktrees, branches, temporary folders and logs behind. `janitor::often` (started by
`hubd`, not by a dev core) runs `Hub::clean_up` three minutes after the core starts and then every
half hour; Settings > Updates > After agent work shows the last run (`GET /api/cleanup`, a
`Cleanup`) and has Clean up now (`POST /api/cleanup`). The rule is that nothing goes that exists
only on this machine:

- Worktrees Hub made (`<repo>/.hub-threads/<id>` for Hub (dev) threads and for projects with
  worktrees on, `.git/hub-attempts/<n>` for attempts) in every project's repository and the dev clone go when `git status` is empty,
  `git rev-list HEAD --not --remotes` is empty, and no thread in them is working, waiting on you,
  taken over, shown in the dev window or active in the last hour. Their branch goes with them
  when it too is all on a remote; the remote branch and the pull request stay. An attempt's
  worktree with a thread stays (picking or archiving removes it). A folder in `.git/hub-attempts`
  that picking could not delete goes an hour after Hub left its `.hub-leftover` note there, when
  git lists the worktrees, it is not linked as one, its work is under the ref the note names and
  no thread is on it; otherwise it is listed as kept. Worktrees Hub did not make are listed as kept, never touched.
- Local branches no worktree has out, other than the default (`update.branch`, `main`, `master`,
  `origin/HEAD`, `hub-preview`), go when every commit on them is on a remote. Hub's and agents'
  branches (`hub-dev/`, `hub/`, and the agents' own from `engine_home::AGENT_BRANCHES`, today
  `claude/`) with unpushed commits are listed as kept.
- Hub's own temporary files in the system's temp folder (`hub-test-`, `hub-page-check-`,
  `hub-carry-`, `hub-*.bundle` and the like) older than a day, and engine logs in
  `<data>/engines` that stayed empty for a day.

A thread in Hub (dev), or in a project with worktrees on, whose worktree was removed gets it back before its next message is sent
(`bring_back_folder`, called from `send_inner` and again when any engine starts, `runtime_with`, since a restart or a warm engine does not pass through a message): on its branch (remembered in `dev.branch.<id>`)
when that is still local or on the remote, otherwise on a new branch with that name from the
updater's branch, since the work was merged. A folder that is already back (a second restore, or the thread's own start got there first) is left as it is, and a branch made between the check and the add is taken as it is, so neither is a warning. Tests that need a folder to outlive a `TempDir` use
`janitor::test_dir()` (`hub-test-…`), never `tempdir().keep()`. When the folder cannot be brought back (a project's own folder, or git refuses), the engine does not start: the thread gets an Error saying its folder is gone and its work is on the branch or pull request, never the agent's `cwd` error.
