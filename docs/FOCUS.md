# Focus
<!-- för: Christoffer -->

Where Hub is going and what counts as done. The project page shows this file on top of the
overview; agents tick off what they deliver and add what Christoffer needs to test by hand.
First drafted 2026-10-30 from the roadmap and what had been merged; change anything.

## Direction: Hub replaces Claude Code as my daily driver

All my AI coding work, on all my machines, goes through Hub, and nothing gets lost on the way.

Done when:
- [ ] I have not opened Claude Code directly for a week (only through Take over in Hub)
- [ ] I can start, follow and answer threads from the PC, tincan and the phone
- [ ] Everything waiting on me is in one place, and nothing lies half done without me knowing

## Stage 1: Proof of concept, real threads on my PC (active)

Hub runs real agents on the PC all the way from question to pull request, and survives everyday use.

Delivered when:
- [x] Hub installs and updates itself on the PC (install.ps1, safe updates)
- [x] Threads survive a restart of the core
- [x] A thread stopped by a plan limit carries on when it resets
- [ ] A real Claude thread reads, edits, commits and opens a PR on the PC without me touching a terminal (done in the cloud on Linux, not yet on Windows)
- [ ] Permission cards, plan mode and model picking work against the real agent on Windows (recorded and replayed: Hub reads the real adapter's tool turn and permission question on Claude and Codex; not yet the card in the UI, plan mode or model picking)
- [ ] Take over in a terminal and handing back works on Windows

To test:
- [ ] Bench card (UI, needs the Hub with the bench): open Labs > Benchmark Hub and press Start smoke run. Look for: "Case 1 of 5 (…) · 0 of 15 repeats done" counting up with a Stop button, then the run in the list; opening it shows a row per case with ✓/✗ per repeat, flag, tokens, time and cost, and the report under it. Starting while a run goes, or with the plan at 60 % or more, shows Hub's reason in red.
- [ ] Bench card suites (UI, spends the plan): in Labs > Benchmark Hub pick "features (4 cases)" in Suite. Look for: Engine and Repeats reading "claude (suite default)" and "3 repeats (suite default)", the note naming the cost cap and an estimate (or "No earlier run to estimate from"), a "Start features run" button; start it with 1 repeat and open the finished run: a flag's tooltip lists its reasons, the line under the table says which Hub version it compares with (or "new series"), and on a machine without Knegarn the handoff repeat's "–" tooltip says it needs one of your tools that takes handoffs.
- [ ] Bench (Rust, the next Hub): in any thread say 'run the smoke bench with bench_run and show me the report'. Look for: 15 Bench threads folded under From Hub, a report with k/3, times, tokens and cost per case, and after a second run a 'vs before' column.
- [ ] Features bench (Rust, the next Hub; spends the plan): in any thread say 'run the features bench with bench_run, repeats 1, and show me the report'. Look for: four Bench threads (files-in-chat, side-job, handoff, branch-commit) folded under From Hub, the side job's child archived with its parent, no Knegarn run started (only a handoff card in the archived thread), nothing new under your rules, and a report with a row per case.
- [ ] Proof stamp for a page (UI, after "Proof for a page"): ask a thread for a small web page with a script error in it. Look for: while Hub has it fix the page the Changed files line reads "fixing page errors…", and if it stays broken "page has errors" in red, not "tests failed".
- [x] Recipes on the shared reader (UI): open Recipes, keep a starter, wait 15 s. Look for: the list as before, a kept recipe showing at once, and no flicker while it re-reads.
- [x] Project status from its threads (UI): open a project page where a thread asks you something or failed. Look for: the Threads head saying "1 waiting on you" or "1 failed" like Home does, not only a permission question counting.
- [x] Rule from chat (UI, needs the next Hub): in a thread say "From now on always run the tests", press Keep as a rule?, Save, then open the project page > Rules. Look for: the rule on top under "Changed this week" with the thread's name and "now", before anything is committed.
- [x] Rules tab (UI, needs the next Hub for the data): open a project page > Rules. Look for: one line per rule from CLAUDE.md with file and how long ago it changed, "Changed this week" on top, the theme chips and the search narrowing the list. Without the new Hub it says "Needs a newer Hub".
- [x] Project and status from the core (UI, with the next Hub): open the inbox with threads in a project, in its own worktree, a quick thread and a chat. Look for: threads grouped under the same projects as before, and the same threads marked as needing you.
- [x] Settings with less text (UI): open Settings and scroll Account, Appearance, Chat, Safety, Machines and Updates. Look for: most rows with just a name and their control, hints only a few words long, nothing that explains what the name already says.
- [x] Long threads (UI): open the thread "Set up Rearange anything" (over 8000 events). Look for: its end showing (the 21:04 turn and what came after) and the side job "Granska Claude CLI-bryggan" saying Done, not "reviewing…".
- [x] ask_jev (needs the next Hub): in a thread of a project with Jev on, prompt "Use ask_jev to check whether this text shows a failed test: 'FAILED tests/x.rs: 2 failed', then say what it answered." Look for: an answer with pick/yes-no and confidence, a row in Jev's list under the agent feature, calls counted in the Jev card. Switch Agents can ask Jev off (card menu or this chat) and ask again: it says Decide yourself.
- [x] Quick thread to project and first steps (UI): start a thread with No project, send an idea, then press Make project in its header. Look for: New project open with the thread's title as the name and the setup text naming the scratch folder, and after Create the project page showing three short lines (with Open it for the setup thread) that go away with the cross or when you send a first thread. Go: New thread, No project.
- [x] Model switch warning (UI): in a thread with messages, open the provider menu in the composer and the model picker. Look for: an amber "New session." line saying what the new model gets and what stays behind.
- [x] Welcome guide (UI): Ctrl K > Välkommen, or Extend > Välkommen in the sidebar. Look for: five steps and four places, the links jump to Nytt projekt, Home and Customs, and it reads well in Cappuccino.
- [x] Windows notices: Settings > Notifications > This PC > Send test. Look for: a notice in the corner and in the Action Center. If Windows holds it back (Do not disturb, Focus, full screen), the line under the button says why instead of claiming it showed.
- [x] Same words everywhere (UI): open Overview, Settings > Machines and a project page while they load or are empty. Look for: only "Reading…", "None" or a two-word "No keys" style line, and "Needs a newer Hub" where a call is missing, never a paragraph.
- [x] Thread cards (UI): Settings > Sidebar > Thread cards, try Compact and Dense in dark and Cappuccino, and open a project so the Inbox is narrowed to it. Look for: Compact two lines with no project name once narrowed, Dense one line with a coloured dot, the divider line with project, count, machine glyphs and a cross, and with five or more machines three glyphs and a +N that opens the list.
- [x] iPhone, with a project set as Working: tap the locked folder button in the pill from the Inbox. Look for: the project opens at once, no half-slid pages over each other, the iOS clock and icons on top as before.
- [x] Themes: Settings > Appearance, click each of the ten theme tiles (Dark, Coal, Graphite, Ink, Forest, Aubergine, Smoke, Cappuccino, Paper, Sage) and look at Home, a thread and Settings. Look for: unreadable grey text, a status colour that looks wrong, the picker tile matching the real look, and the theme staying after a restart. Go: Settings > Appearance.
- [x] iPhone, with a project set as Working: look at the top band. Look for: a chip with a lock and the project name left of the island, Hub's own clock and battery on the right (no iOS signal icons), no "Hubtest · Leave" row in the Inbox. Tap the chip from the Inbox: the project opens at once, no half-slid pages, and the chip now reads "‹ Inbox"; tap it to go back. The folder button in the pill opens the whole project list. Hold the chip: All projects and Leave. Go: Inbox, Projects.
- [x] Look at Machines, Engines in Settings and a project page: the small capital labels above cards (Memory in use, Auto-compact, group heads in the right panel) are all the same size and spacing, and no text looks half a pixel off
- [x] Thread view, one sitting on the PC (Stage 1 and chat world): ask for a 4-step plan and a file read, then switch the model on a long thread, hide Hub for a few minutes and come back, and press Take over and /exit. Look for: the Steps card and one line per detail level (Quiet, Minimal, Normal), the same left edge and grey dot or cube on working rows, the 'Hub handed it the conversation so far' line, the thread catching up at once, emoji reactions on both sides; then in the Chat world send a chat, get a picture and a live-data answer, delete a chat and go back to Dev: it stays in Chat, nothing leaks into Dev lists, no Web button in Chat. Prompt: "Make a plan of 4 steps with update_steps, read docs/FOCUS.md and the README, then do the steps and name the files you read." Go: Settings > Chat.
- [x] Menus and pickers grow from what you pressed (UI): right-click a thread, open a Select in Settings, the model/effort menus in the composer, the emoji picker, Ctrl K and the user menu. Look for: each one scaling up from about 95% and fading in from where you clicked (about a quarter second at most), leaving a little faster, and nothing animating with reduced motion on. Go: Settings, any thread.
- [x] Looks, in dark and in Cappuccino (Home, a thread, Settings, Machines, Overview): click through them at Large text and 125% zoom. Look for: four corner sizes, one icon weight, no thin lines in the composer, readable grey text and darker status words in Cappuccino, the Home scenes and Custom cards (drag, size, combine, Reset), three-line inbox rows with hover Snooze/Settle and the hover card, Ctrl K sections, the rail (Alt 0/1/2/3, pin, drag), tooltips and the usage strip; nothing cut off or overlapping. Go: Home, Machines, Overview, Settings > Appearance.
- [x] What agents show in the chat: ask one thread for a CSV, an SVG, an Excel file, an HTML chart page, a .md file and one or three pictures (also from a side job), and name them in the answer. Look for: a file card under the reply for each (up to four) that opens in Hub, the right viewer for each kind (table with sheet tabs, drawn SVG, live page card, rendered markdown, picture lightbox), 'no viewer yet' for a .blend, the Library tab listing them, and a side job that edits files showing its step line and diff. Prompt: "Make ten orders as a CSV file, a flowchart as an SVG, an Excel file, an HTML page with a chart and a markdown file with a heading, a list, a table and code. Write them with a script and name every file in your answer." Go: Library.
- [x] Updates and builds: merge one UI-only and one Rust PR and watch Settings > Updates (and a MacBook page with an older hubd). Look for: the three 'Only the pages changed' steps ready in about a minute, the Rust build waiting its quiet time with Build now, 'New version ready' in the sidebar foot with Install and What's new, the install card replacing the window and reloading once, running threads surviving, the five-dot step on a machine's Update button, and hub/update statuses on the merge commit on GitHub. Prompt: "Change one sentence in docs/FOCUS.md for the better and open a pull request. Docs only." Go: Settings > Updates, Machines.
- [x] Patch notes once an hour: merge two PRs within the hour and install both. Look for: the second update ready without the notes wait and listing its commits; the first build an hour later has notes that cover all of them.
- [x] Engine start without npx: start a second Claude thread. Look for: the first answer about 2 s sooner than before, and the thread working as usual.
- [x] Quiet pictures in a chat: in an everyday chat ask for a picture. Look for: the picture shows, and the agent does not write a second answer saying it is done.
- [x] PowerShell row: open the PC's machine page. Look for: an amber PowerShell row with the reason while scripts are refused, the fix opening a terminal with the command, and the row gone after running it and Check again.
- [x] Settings, engines and small things: open Settings > Engines (Hermes harness, local models, Add harness), Notifications (per-device cards, Send test, a quiet rule), Dictation (Ctrl D) and Plugins, press Ctrl I for an idea, drag a file onto Hub and use the three Labs dev-window switches. Look for: each setting saving and applying without restart, the notice showing on this PC and the phone, words appearing within a second while dictating, plugin cards loading in seconds, the idea landing in docs/FOCUS.md, and the dev window following a dev thread. Go: Settings > Engines, Settings > Notifications, Settings > Dictation, Plugins, Labs.
- [x] Files in the chat sit in it: ask a thread to make an HTML page, a CSV and a .md file in one answer, then again with one file only. Look for: no frame and no scrollbar, a faded preview straight on the chat, a click anywhere on it (or Enter) opens it big, Open shows on hover, a quiet name line with ⋮ under it; with three files, a row each with a miniature; big view: the page fits the window and the panel itself scrolls when it is longer. Prompt: "Write a one-page HTML architecture sketch, a CSV with ten orders and a short report.md with a list, using a script, and name all three." Go: a thread.
- [x] Inspect mode (UI): press Ctrl Shift I in Hub, hover and click a few elements, write a note on one, Shift-click two sidebar rows to group them, then press Open thread. Look for: a frame and a label on what you point at, the note box at the element, the element tree on the right following your pointer (and a click on a tree row marking it), the group note, a card with a screenshot that shows your marks and numbers, and a new thread starting in the background while the page stays; open it and check that it names the component files and has the picture. Also press Z (or the glass in the panel) and rest the pointer on a small icon: a lens beside it should show it larger and crisp. Go: Ctrl Shift I anywhere, or Ctrl K > Inspect Hub.
- [x] Usage map (Rust and UI): Overview > Customize > add Usage map; click around for a few minutes (Home, a thread, send a message, Snooze, Settings pages) and wait 10 seconds. Look for: squares brightening with use and a dashed frame on what you never touched, hover showing uses, last use and time, "Stop counting" pausing it and "Clear the log" emptying it, and no text you typed anywhere. Go: Overview.
- [x] Hover `#695`, `PR #688` or `KarlssonChristoffer/hub#685` in an agent reply (Rust and UI): after a moment a card shows Merged/Open, repo, age, title, author, +/− and files, and the checks; on the phone a tap opens it as a sheet.
- [x] Where it goes bars (UI): open Settings > Where it goes, click each card on Today, 7 days, 30 days and All time. Look for: bars flat in the place's own colours (Claude clay, the cloud blue, Codex sky, Hub's engines each in theirs) with a solid top line and no glow, no white blocks, one day a single bar instead of a full-width block, day numbers on 30 days spaced out and readable, and no date or $ label running into the facts column on All time
- [x] Where it goes, one Claude card (UI): open Settings > Where it goes, All time, and click Claude. Look for: one Claude card in place of Claude Code, In the cloud and Elsewhere, growing to twice the width when opened; its bars in parts (terminal clay, cloud blue, Elsewhere striped); the counted weeks readable, with the Elsewhere spike cut and its ≈ sum on top; Terminal, In the cloud and Elsewhere as three rows beside the chart
- [x] Where it goes in tokens (UI + Rust): open Settings > Where it goes and press Tokens at the right of the ranges. Look for: the total as "N M tokens", every card, bar, row and the opened chart in tokens, Claude without Elsewhere, DeepSeek's box still in dollars; close Settings and open it again and it is still in tokens; press $ and everything is dollars again
- [x] Where it goes zoom (UI + Rust): open Settings > Where it goes, Today, and click the Hub card. Look for: today's hours as bars, the current one growing while a thread runs; scroll out over the chart and it turns into days, weeks, then months, scroll in and back to hours; drag sideways to move in time; click a day to see its hours; hover a bar for its parts; switch Hub's bars to By who asked
- [x] Where it goes rework (UI): open Settings > Where it goes. Look for: no heading or subtitle above the ranges, no text beside the big number, Claude and Codex cards in a light clay and blue tint, and Inside Hub and What Hub sent with one slim strip and plain numbers instead of percent bars (hover a line for its percent).
- [x] Unslop polish (UI): click through Home, the inbox, Settings and Where it goes. Look for: small section labels in sentence case instead of capitals, flat chart bars with no glow, and rows and chips that sit a pixel tighter or looser but nothing cut off or overlapping.
- [x] Unslop keepers (UI): Settings > Appearance, pick Neutral, then click through Home, a thread, Settings and Where it goes; switch back to Dark. Look for: Neutral is a plain grey with a blue accent, Where it goes has no colour clouds behind the cards and Hub's card is the wide one, Max/Plus are plain grey words, an amber "waiting" dot holds still and the Working word in a thread no longer shimmers.
- [x] Hermes goes on by itself (Rust): on a DeepSeek thread ask for a change and "review and merge" so it starts a review helper (`delegate_task`) and says it is running. Look for: the thread staying "Working" while the helper runs and after it comes back and the agent carries on by itself, a finished mark and the notification when it is quiet, "Worked for" on that part, and Stop working on it. A DeepSeek outage now reads "the engine's turn failed inside the engine … model provider" instead of a bare Internal error.
- [x] Settings in ten rows (UI): open Settings and look at the left list, then click Account, Chat, Engines, Safety, Connections, Machines and Hub. Look for: no scrolling in the left list, no group headings, tabs over the page for the parts that were separate rows (Usage / Where it goes, Chat / Voice, Engines / Handoffs / Personality, Rules / Customs, APIs / Keys / MCP and plugins, Machines / Devices, Updates / What's new / Data / About), thread list settings under Chat, no "Soon" rows, and old links (Ctrl K, the name menu, Customs links in threads) landing on the right tab. Go: Settings.
- [ ] Proof for a page (Rust, the next Hub): in a project folder ask "bygg en enkel html sida hello world tack". Look for: if the agent shows no proof, Hub's question names page_check with `file`: index.html (not tests), the agent looks at the page with it, and the Changed files line ends "screenshot", not "no proof" or a failed turn.
Ideas: strays by process tree instead of folder

## Stage 2: Daily driver, a week without Claude Code

Hub is better than the terminal for a normal working day, not just as good.

Delivered when:
- [ ] I have worked a whole day only in Hub
- [x] Live tool output in the thread: command output, exit code, elapsed time
- [x] A real message queue and an Esc that always stops the turn
- [ ] Permissions docked in the composer, and always-allow sticks
- [ ] Undo per thread (a git checkpoint before the agent starts)
- [ ] The project page shows the direction, the stage now and what to test

To test:
- [ ] A normal working day in a thread (Stage 2): queue three messages and edit one, press Esc during a stubborn tool call, answer a permission question with the key 1, send a code block's 'Run in terminal', stash a draft with Ctrl S and start a background thread with Ctrl Enter, press Ctrl . for the side column, and turn on Own worktree per thread for a project and start two threads. Look for: messages running in order, the turn cancelled within ten seconds, the permission becoming a transcript line, the terminal opening under the code block, the stashed draft in tabs, and each thread working in its own .hub-threads folder and branch. Prompt: "Run `sleep 60` in the terminal, then run `cargo test --workspace`, and say done. I will queue messages and press Esc meanwhile." Go: Settings > Safety.

- [ ] Open Settings > Where it goes: one total for the week, then cards for Hub, Claude Code on its own and Codex on its own in their colours, each with its week of bars and share; Hub's bars are split Claude/Codex/DeepSeek, Inside Hub gives You asked and Hub on its own with % and $, and DeepSeek and Jev show what was really paid. Start a thread and, while it runs, the total says live and grows (built on request, outside this stage)

- [ ] Click through the tab bars in Settings, a project, a machine, Customs and a problem: all look the same (13 px, accent underline) and the underline slides to the tab you pick; counts sit after the label
- [ ] In a thread, click a file card or page card: it unfolds in the chat (✕ in its caption folds it); the caption's panel button opens it in the right panel, and ⋮ > Open in a window in a modal, once. Settings > Chat > Files open switches what a plain click does (In the chat, Panel, Modal). The chat never moves aside
- [ ] Expand a file or page card with the expand button in its caption: it grows in place (as wide as the thread's room, at most 1280 px, as tall as the window), the chat scrolls out of sight above, Ctrl + wheel zooms around the pointer, holding Ctrl or Space lets you drag to pan, Esc or ✕ folds it back to where you were

Ideas: quota guard (warn and switch model), screenshot to thread, focus nudge when a new thread is off the stage, focus mode (built, see To test)

## Stage 3: Everywhere, other machines and the phone

I can leave the PC and still work.

Delivered when:
- [ ] tincan runs threads the PC starts, and the app reaches it directly even when the PC sleeps
- [ ] Answer from the phone: a queue of questions and permission cards, push, one-tap answers
- [ ] Run on: PC / tincan / Cloud, with the project's machine as the default (the Cloud entry, icons and thread card are in the window; the core starts Claude and Codex cloud threads, the phone is not built yet)
- [ ] Panic button: pause every agent on every machine

To test:
- [ ] Machines, cloud and GitHub (Stage 3): open each machine page (PC, MacBook, clanker, hermes-vps, tincan), move a thread with uncommitted files to another machine with Move to, start a cloud thread in a project with a GitHub remote, and press New thread > Connect GitHub. Look for: Ready-for-work dots and CPU/memory/disk rings, the thread arriving with its files and one 'Hub checkpoint' commit, the cloud card going Starting, Working with the claude.ai link, GitHub repos with Clone to machine, Take over over SSH on tincan, and delete of a thread killing its engine and any sleep within seconds. Prompt: "Write a file called hub-move-test.txt with the word hello in the project folder. Do not commit it." Go: Machines, Settings > Machines, Settings > Access.
- [ ] The iPhone app (Stage 3), after the next phone build: pair with the QR code, then in one sitting start a Dev thread and a Chat, swipe-settle and snooze an inbox row, double-tap a message for a thumbs-up, open a thread with changes (Commit/Push, diff comments), browse a project's Files, and open the model sheet. Look for: the floating tab pill and gestures (swipe back, swipe to quote, hold menus) feeling right, the orange 'New' line after a reply, picture side jobs under Working then Done, the Cloud row under Run it on, and no stutter in a long thread.

- [ ] On the MacBook, in the repo root, run the command in the header of `ios/tests/ContractTests.swift`: it should print "Contract tests passed" (if it fails to compile, tell the thread which Swift file it needs added)
- [ ] On the MacBook's page press Terminal, then Claude: a Terminal tab opens on the right with two terminals inside Hub (no Windows Terminal window), you can type, Ctrl+C with text selected copies and Ctrl+V pastes; go to Home and back and both are still there with what they showed; × closes one, Open in a window opens the same in Windows Terminal
- [ ] On the MacBook's page Tailscale says "Running · 100.x…" (or the real reason the tailscale command failed), not "Not installed"
- [ ] Start a thread from the phone (needs the next Hub and the next app build): its mode in the composer is the engine's auto one, not manual; a model and effort picked from the built-in lists (say Terra, Medium) show in the thread's composer as what it runs on; the Desk project is not offered as default and a thread sent to it is refused with a clear line
- [ ] In the iPhone app (next build, with the next Hub): write a message in a thread on the PC while the phone has the same thread open; it shows on the phone within a second or two, and never stays missing while later words are there
- [ ] On the iPhone (next build), open a chat where the agent showed a live page (a chart): the card is as tall as the page with no empty space under it, the agent's page_check screenshot of it is not in the chat, and an older version of the page is one grey line "v1 · v2 is further down" that opens it
- [ ] On the iPhone (next build), in a long thread, a chat and a project page, swipe from the left edge to go back, also while the list is moving and with a slightly slanted swipe: it goes back on the first try, and a swipe up or down from the edge still scrolls
- [ ] On the iPhone (next build), send a photo with a message in a chat: a small square of it sits above your words (a tap opens it big); ask for a picture so a side job draws it: no "Hub sent the agent a note" line under the side job
- [ ] Once Hub has the `phone.threadpage` call (a Rust build) and the iPhone has its next build, ask an agent to save a chart as an .html file: under its answer on the phone the chart shows as a live page card (tap to use, expand to open big), not only the file name
- [ ] On the iPhone (next build), open a chat and New chat: there is no globe or "Web" in the box and no Search chip; ask something that needs the internet and the agent looks it up anyway; an old web-search chat still shows its globe and source cards
- [ ] On the iPhone (next build), switch a chat to another engine after a turn that made a page and send a message: the "switched to …" line sits just above your message, below the earlier page and its "Worked" chip
- [ ] On the iPhone (next build), open a thread: it slides in as one solid page with the answer already at the bottom, nothing of the inbox showing through and no keyboard; tap the title: a card drops out of it with the name, engine, machine and project, and the keyboard only comes when you tap the pencil; hold the title to rename straight away; ⋯ no longer has Rename, machine or Engine
- [ ] In a thread on the iPhone, scroll to the end and pull up a short way: the next thread's card peeks and arms, and on letting go it opens with its conversation (not an empty page)
- [ ] Open a long thread and a chat in the iPhone app, also one with new replies: each opens at its latest message, with no scrolling down
- [ ] With a thread open in the iPhone app, send a message and wait for the reply: no ntfy notice comes for it; lock the phone and the next reply does send one
- [ ] In the iPhone app, ask an agent for a task with steps: its plan shows as a card above the box with one moving cube (none on the working line); swipe it down to a pill, tap the pill for the sheet with times; after the turn it is a small "3/3 steps" pill
- [ ] In the iPhone app, ask an agent to write a small CSV and an SVG: after the turn each shows as a card like on the desktop (a table, the drawing); a tap opens it big
- [ ] On the iPhone (next build): hold a project on Projects and choose Set as working. Look for: the project button in the pill in the project's colour with a lock, a tap on it from the Inbox or Machines going straight into the project, the Projects page with the project as a big card on top and the others as quiet rows, the Inbox showing only that project with "N held" and Leave, another project opening with a "Peeking · Back to <project>" bar, and a project's Home tab with its tiles. Unlock (or Leave) puts everything back.
- [ ] Once Hub has installed this update, start a chat in the iPhone app with Sonnet picked and ask "which model are you?": it says Sonnet, and a chat started with Opus picked says Opus
- [ ] In a Hermes (DeepSeek) thread, change the permission (Default, Accept Edits, Don't Ask) while it is working: the pick stays on screen and is applied the moment the turn ends (no snap back to Default); a new Hermes thread starts in Don't Ask, not Default (after the next Hub build)
- [ ] Let a thread wait on an Approve edit card, restart Hub (an update or Ctrl C), and open the thread: the old Approve cards are closed, and only the one the agent asks again is left; pressing Allow once on it works (after the next Hub build)
- [ ] Once Hub has the next Rust build, open a project's Git, Docs, Backlog and Board parts on the phone's project page (needs #720's pages): Git shows the main branch with its CI, open pull requests and recent branches; Docs lists FOCUS, ARCHITECTURE and DESIGN first; add an idea to the Backlog, put it on the Board and move it across; the repo then has docs/BACKLOG.md and docs/BOARD.md with the change
- [ ] Ready-made Hub for a new PC (Stage 3, after the next Rust build): on a second Windows PC with only git, run `scripts\install.ps1 -From http://<your-pc>:7878 -Code <code>` with a code from `hubd pair` on this PC. Look for: Hub installed and started in about a minute, no cargo or npm needed; with a wrong code or an unreachable address it says so and falls back to building from source.
Ideas: pick the machine with the most free CPU and memory, workers from other machines, narrower grants per device

## Stage 4: Hub works for me

Hub does work while I sleep, and my own tools are part of it.

Delivered when:
- [ ] Night shift: queue jobs in the evening, Plåtnicklas works through them
- [ ] Side jobs hand off a task and the answer comes back by itself
- [ ] Knegarn and Forman show up and are controlled in Workers (Knegarn: each run is a thread with its chain, pause, stop and more room; Forman still to come)
- [x] Agents ask without waiting: `ask_user` returns at once with the agent's assumption, the question stays in the chat, a late answer reaches the agent as a message
- [x] Test keys and sign-ins per machine: one button on the machine page tries every engine and API connection for real
- [x] Review button on a plugin's page (UI): puts a GitHub repository on Customs' list, shows the review running and its result (the review of repositories itself comes with the next Hub version)
- [x] Key vault: create, replace and remove keys in Windows Credential Manager, and see what uses each one
- [x] Any API in API connections: pick or name a service, paste what it gives you, and it works (Spotify logs in with Allow; threads call through Hub's gateway and never see a key)

To test:
- [ ] Problems, Vakten, Customs and Knegarn (Stage 4): cause a real error and an 'Authentication required' on an engine, open Control > Problems, press Start a thread on a problem and Let Vakten do it, let a dev thread idle 20 minutes, open Settings > Handoffs and Customs, then hand a small task to Knegarn from a thread. Look for: the problem row with count and stack, Vakten's Fix thread opening in the right folder and ending at a PR, the Aftermath run on tincan, the handoff and Customs cards saying Cleared, Held or Refused with a reason, and the Knegarn run card with Pause, Resume, Stop and a PR button when done. Prompt: "Run the command `definitely-not-a-command`, then read the file `/nope/missing.txt`, so Problems gets entries." Go: Problems, Settings > Customs, Settings > Handoffs, Workers.
- [ ] Jev, the key vault and API connections (Stage 4): with Jev connected ask 'hur går det?' mid-work and run a failing cargo test, open the Jev card (gear, Tune), then Settings > Key vault and API connections > Spotify and a typed unknown service. Look for: the proof stamp 'checking test output…', Jev's cost and credit left in the status bar, the vault listing keys by last four with Used by, a new key appearing in Windows Credential Manager, Spotify answering through Hub's gateway without a key, and each connection page's header matching its service. Prompt: "hur går det?" Go: Settings > Key vault, Settings > API connections.
- [ ] Test keys and sign-ins (Stage 4): open a machine's page (Machines), press Test keys and sign-ins under Ready for work. Look for: a row per engine and API connection saying Works, Refused or Missing (with the date a kept sign-in ends), no key shown anywhere, and the Sign in button on a row that is Refused. On an older hubd it says the machine needs the next version.
- [ ] Review button on the plugin page (Stage 4), once Hub has installed the Rust part: open System > Plugins, a repository, and press Review in its Customs card. Look for: "Reviewing…" with the button disabled, then the stamp changing to Cleared, Held or Refused with the reason, and the row appearing in Settings > Customs under github-repo. On the Hub installed before, the button says "Needs the next version".
- [ ] Keys in memory only on the VPS (Stage 4): add `"keys_in_memory": true` to hub.json on the VPS, restart hubd there, wait a minute. Look for: Settings > Key vault (open with the VPS as machine) saying "memory only", a Hermes/DeepSeek thread on the VPS working, no `secrets.json` rewritten there, and after restarting hubd on the VPS the key being back within a minute.
- [ ] Tincan relays keys (Stage 4): add `"relay_keys": true` to hub.json on tincan and the VPS as one of tincan's machines, turn the PC off, restart hubd on the VPS. Look for: the VPS getting its DeepSeek key and sign-in back within a minute, and the PC's keys unchanged when it is on again.

- [ ] Customs checks a GitHub repository (Stage 4, after the next Rust build): ask a thread to declare a well-known repository with `customs_declare` (kind `github-repo`, name `owner/name`) and then a repository made last week with almost no stars. Look for: the well-known one Cleared with a one-line reason and category within a minute, the new one Held with every reason spelled out (age, stars, licence, install script) in Settings > Customs > Needs you, and a made-up `nobody/does-not-exist` coming back as an error that leaves nothing on the list. Prompt: "Declare github-repo `modelcontextprotocol/servers` through Customs, then `somebody/brand-new-thing`, and tell me what each came back as." Go: Settings > Customs.

- [ ] GitHub connection per machine (Stage 4): open Settings > Account. Look for: one GitHub line per online machine with the connected login; Reconnect starts the device login with its code and Open button. After the next Rust build also: your name and picture, the scopes, Sign out (the line then says Not connected, Connect brings it back) and "Sign-in lost" if GitHub refuses the saved login. On the Hub installed before, only the login shows.

- [ ] Agent questions that do not wait (Stage 4, after the next Rust build): tell a thread "Use ask_user to ask me which of two names to use for a new file, then carry on without waiting". Look for: the thread keeps working and never says Needs you, a quiet question with "assumed X" over the message box and in the chat, and after you press an option the agent saying it saw your answer. Go: any thread.

Ideas: Arsenal, AI proxy on subscriptions, approved tech for new projects (Customs)

## Parked

- Code analysis in Problems (requested separately, 2026-11-05): step 1 built: tool-neutral run
  and finding storage/API, commit/profile/scope evidence, stable occurrence identities and
  verified disappearance. To test: post a successful run with a finding, find it with source
  `code_analysis`, then post a partial empty run (stays open) and a comparable successful empty
  run (resolves). Next: scanner adapters and Analyze button, then Guard and quality gates.

- Notice system for Windows (plan, sketches and the questions: https://claude.ai/artifact/6H72xM8W5v83dmyC824cEw, 2026-11-03; step 1 built, the test notice): the core owns the rules (nothing passes until chosen, set per device: PC and iPhone), toast, Hub's own card, tray and badge are receivers, the phone's ntfy and #595 move to the same rules, `/api/notices` with a stream (built: the door in, `notice.rs`), webhook out and a `Grant::Widget` token for widgets and servers. Settings page to be redrawn first (sketches, one column per device)
- Faster update build (measured 2026-11-03 on the PC: Rust steps ~12-15 min per version, test compile 2.5-4.5 min, tests 4-5 min, release 3 min; 3 of the last 5 test steps were stopped for low memory and rerun from scratch): hub-core's integration tests are now one program; next, in order: why the memory guard trips (and resume instead of rerun), lld as linker on Windows (to measure); Defender and a Dev Drive are off the list (Christoffer has Defender off and does not want it touched, 2026-11-03), cargo-nextest, and only then splitting hub-core into crates (hand-written code, so a big refactor for an unproven gain)
- Aftermath and Hub, next steps (thread facts are built, 2026-11-03): a per-thread view of the stretches from Aftermath, an "AI-granskad" stamp on a thread in Hub (UI), Aftermath independently re-running a stretch's tests so Hub's proof can become a real pass (Knegarn's verifier, or contract 89), the files changed, commit and PR number per stretch, and closing a stretch when a thread is settled or archived instead of after 20 quiet minutes
- A chat's library on the iPhone: + in a chat opens what was made and sent in it (pictures, pages, files you attached), kept in one bigger library for every thread; Christoffer wants the chat fixed first (2026-11-03)
- Viewkit later stages (plan: https://claude.ai/artifact/HFxd2vTqpwL97CC42DZXzv): a registry of kinds the chat, libraries and phone all ask, tables for .xlsx/.csv, documents and slides, Mermaid/SVG/3D as generated pages, phone parity, question and plan cards (2026-11-03; stages 1 and 2 built)

- Charts and other agent output designed in Hub's own look, standardised across agents (a ```chart block drawn natively, or a chart kit for pages): Christoffer wants to talk it through before anything is built (2026-11-03, thread "Show agent output in chat via HubKit"). Only the six series colours (`--chart-1` to `--chart-6`) and the layout rules are in the page guide so far (2026-11-04)
- Home's "Tokens per hour" columns (sketched 2026-11-02, G1): the core only gives totals per day, so it needs an hourly tokens call in Rust first, as its own pull request
- Warn when the model that answered is not the one the composer shows: claude-agent-acp puts the real one on every `usage_update` (`_meta["_claude/model"]`, like `claude-opus-5-5`), so the thread can compare it with its picked model and say so (2026-11-02, after a Sonnet chat ran on Opus)
- Bench, two features cases left out of the features suite (2026-11-11, docs/architecture/bench.md "Features suite"): a rule saved from the chat (needs a runner step that presses Keep as a rule and starts a new session) and a push (needs a guard that can tell a push stays inside the run folder; the bypasses are listed there)
- Rubber duck
- AI-driven DevOps (has its own thread)
- More in Settings > Personality besides writing style (tone, language it answers in, how long its replies are); only Claude Code can be told today, through its output styles
- Run threads on local models: Settings > Engines finds Ollama and LM Studio per machine (2026-11-01); next is offering them as a provider to OpenCode or Hermes
- Every session in tmux (Windows only for now)
- Voice and Discord as surfaces
- A second Claude engine without ACP (`claude -p`)
- 3D agent world
- Overview cards, step 2: what the agents did, who waited on whom, hot files, idea to merge, your words, and alarms you set yourself (step 1, the customizable page, is built)
- Plugins steps 2 to 4 (Customs' review of a repository is done, `POST /api/customs/repo`; the Plugins page does not ask for it yet): install MCP servers, skills and tools behind Customs, Hub's MCP as the gateway agents see them through, switches per thread and project, `@` in the composer, Try in a scratch thread, updates as a diff, a weekly "new this week" routine (plan in the project files, plugins/pluginsida-forslag.md)
- Recipes from the repository: read `.claude/commands/*.md` and a `.hub/recipes/` folder as recipes for that project, and run recipes on other machines
- Problems, next steps: OTLP over protobuf (JSON is built), linters as problems, a rule against swallowing errors silently, revert detection that moves an area toward ask first, and Vakten learning rules by itself instead of only suggesting them (v2 plan in the project files, felinsamling/v2/inspiration-och-egna-funktioner.md)
- More file kinds in the reader: Excel and Word files, pictures inside .md files, SQLite databases, Parquet, OpenAPI specs as endpoints, editing a CSV cell (Markdown, JSON, CSV, PDF, notebooks, logs and the rest are built)

### Settings rows that were promised ("Soon") and removed from the UI (Settings restructure)

The Settings pages no longer show unbuilt rows. Each is an idea to pick up when it is built for real:

- Picture: A photo or initials next to your name, and on the threads you start.
- Usage over time: Tokens and cost per day, per project and per engine, from the threads' own counts.
- Spending cap: A monthly limit for API keys. Threads ask before going over it.
- More providers: Gemini, OpenRouter and local models, through the AI proxy.
- Accent colour: The colour of selection, links and the active tab. Status colours stay as they are.
- Density: Compact or roomy rows in the lists and the chat.
- Code colours: Syntax colours for code and diffs, one for the dark theme and one for the light, with a preview.
- Contrast: Stronger lines and text for bright rooms or tired eyes.
- Start on: Open Hub on Home, the last thread, or the activity list.
- Push to talk: Hold a key anywhere in Windows to dictate into the thread you are looking at.
- Microphone: Pick which microphone Hub listens to.
- Read replies aloud: A voice reads a reply when you ask, or every short one.
- Send with: Enter sends and Shift Enter makes a new line, or the other way round with Ctrl Enter.
- Skills in the slash menu: Show your skills next to the commands when you type /.
- Vim keys: Vim editing in the message box.
- Transcript view: What a thread shows by default: the replies, also the thinking, or every tool call.
- Chat width: A narrow column that is easy to read, or the whole width.
- Time format: 24-hour or 12-hour clock on messages.
- Diffs: Side by side or stacked, and whether whitespace changes show.
- Snooze options: The times Snooze offers, and snoozing until a plan limit resets.
- Thread titles: Your own instructions for how new threads are named.
- Confirmations: Ask before archiving or deleting a thread, or not.
- Cost cap per thread: A thread stops and asks when it has spent this much.
- Project template: The files every new project starts with: CLAUDE.md, docs/plan.md and a .gitignore.
- Commits: The name and attribution line the agents' commits get.
- Merging: How pull requests are merged (merge, squash or rebase) and whether Hub pulls main afterwards.
- Branch names: A prefix and a pattern for the branches threads make.
- Commit and PR writing: How the agents write commit messages and pull requests, and whether they follow the repository's templates.
- Worktrees: Where a thread's own checkout goes, and when one that is merged or unused is cleaned up.
- Other Git hosts: GitLab or Gitea next to GitHub.
- Your instructions: Your own CLAUDE.md, which every Claude thread reads, edited here.
- Project instructions: Each project's CLAUDE.md and AGENTS.md, with a jump to the project page.
- Hub's memory: Facts the threads save as they work: what they are, where they came from, and forget one.
- Remember across threads: Let a new thread start with what the last ones in the project learned.
- Skills: Saved ways of doing a task. See them, switch them off, and add new ones.
- Add a connection: Add an MCP server from here, for Hub's threads or everywhere, and log in to one that needs it.
- Install plugins: Add plugins from a marketplace or a repository; the installed ones are listed under Connections.
- Hooks: Commands Hub runs before or after a tool, such as formatting a file the agent wrote.
- Commands: Your own slash commands for the message box.
- Subagents: Helpers a thread can hand part of its work to, each with its own instructions and model.
- Browser: Let threads drive a browser to test what they built.
- Create an SSH key: An ed25519 key, added to the SSH agent and to GitHub in one go.
- Edit hosts: Add and change hosts in the SSH config from here, and test that they answer.
- Wake a machine: Wake on LAN for a sleeping machine before a thread starts on it.
- Access log: Every call a device or an agent made, and what it changed.
- Expiry: Device tokens that run out unless the device is used.
- Roll back: Go back to the previous version when a new one misbehaves.
- Release channel: Only the batched releases, or every commit on main.
- Keep threads for: Archive and then delete threads after a while, or keep them for ever.
- Export: A thread or a whole project as Markdown, with its attachments.
- Archived threads: Every archived thread in one list, to bring back or delete.
- Tidy the database: Delete old logs and give the freed space back to the disk.
- Delete everything: Remove all threads and settings from this computer, with one confirmation.
- Back up settings: hub.json, these preferences and the agent keys' names in one file, to move to a new computer.
- Own shortcuts: Change any of the shortcuts above.
- Report a problem: The logs and the last error, packed into a GitHub issue.
