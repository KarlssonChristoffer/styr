# Architecture: Projects, plugins, library and GitHub
<!-- för: agent -->

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## Projects
<!-- covers: crates/hub-core/src/project.rs, crates/hub-core/src/project/, crates/hub-core/src/project_folder.rs, crates/hub-core/src/project_github.rs, crates/hub-core/src/projects_root.rs, app/src/lib/components/ProjectView.svelte, app/src/lib/components/ProjectRunning.svelte, app/src/lib/format.ts, app/src/lib/components/ProjectIcon.svelte, app/src/lib/components/ProjectIconPicker.svelte, app/src/lib/components/NewProject.svelte, app/src/lib/stacks.ts, app/src/lib/stackLogos.ts, app/src/lib/components/StackLogo.svelte, crates/hub-core/src/templates.rs, crates/hub-core/templates/ -->

A project's icon (`ProjectIcon.svelte`) is a coloured line icon Hub picks from its name, or the icon,
colour or picture chosen in `ProjectIconPicker.svelte` (opened by clicking the icon where it is
editable; "Automatic" goes back to the name's pick, see `looks.rs` below).

A project is the folder threads run in, or a folder added with "New project" (stored in the
`projects` table so it shows before it has threads; creating one runs `git init` and starts the
plan and the rule). Its page (`app/src/lib/components/ProjectView.svelte`)
is the overview before diving into threads: branch, last commit, GitHub PRs and CI, the stack read
from manifests (`Cargo.toml`, `package.json`, ...), the plan, the docs, the threads, and the files
and ports on the side. The living docs are markdown in the repository, so every engine can read
and change them: `docs/plan.md` is a checklist (done steps first, then what is next), and the
other files in `docs/` plus README, CLAUDE.md and AGENTS.md are the docs. Agents update them
without asking because of a rule in CLAUDE.md that the page can add. Apart from that list, nothing
about projects is stored in the core; it reads the folder each time (`crates/hub-core/src/project.rs`).
The Threads card has a switch, "Own worktree per thread", for a project in a git repository on
this machine (`GET`/`PUT /api/projects/trees`, feature `project.worktrees`, so an older hubd shows
no switch): with it on, each new thread works in a worktree and branch of its own (see "A worktree
per thread" under Dev mode). A thread working in one carries an "own worktree" mark in its header.

"New project" (`NewProject.svelte`) makes a project from a name. It has two columns. The left one,
"Where it lives", lists the machines as rows (their glyph, what they are and how much disk is free,
read once from `GET /api/host/about`; an offline one is dimmed and cannot be picked) and under them
the folder, with Browse. The right one is the project: the name, what it is, a Starter card
(Blank by default; the card shows the stack, what it needs with a tick for what the chosen machine
has, and its run command) and the extras as switches (GitHub repository with its visibility,
Setup thread, Build and test on GitHub, Own worktree per thread). A line at the bottom says what
will be made and where. Browse and the Starter card do not open a pop-up: they take over the right
column (Escape or Cancel goes back). The folder browser lists the chosen machine's folders one
level at a time (`GET /api/folders`, with `?host=` for another machine), shows which are git
repositories, takes a typed path, and "Use this folder" sets where the project goes. The starter
list has two tabs. Templates is a grid of the stacks in `GRID` (`stacks.ts`); "Not sure what to use?" lists what you
want to build (Web app, Windows app, Game, …) and, for each, three or four hand-written stacks
with what each is good at and what to watch out for, one marked Recommended; Use picks it. Nothing
there asks a model: the advice is the text in `stacks.ts` (`KINDS`), so changing it is editing that
file. Each stack has its logo in its own colour, from the library in `stackLogos.ts` (Simple Icons,
CC0, 24 by 24 paths; `logoColor` swaps a brand colour too dark for the background for the text
colour) drawn by `StackLogo.svelte`; add a logo by adding its path there. "Existing folder" (the
switch top right) is the old single field with recent folders.

The templates the core writes itself are in `templates.rs`, with their files under
`crates/hub-core/templates/<id>/` (API server `fastapi`, `php`, `node`, `python`, `go`, `rust`,
`static`; `blank` writes nothing but the standard docs). Each is a small project that builds, runs
and passes a test, with a README, `.gitignore`, `CLAUDE.md` (how to build, run and test, layout,
rules), and `docs/plan.md` with the first step ticked. `{{name}}` and `{{slug}}` in the files are the
project's name (quotes, `$`, braces and the like removed so it cannot break out of a string) and a
package-safe form of it. `create_project` writes the template first, then adds Hub's rules to
`CLAUDE.md`, then copies `CLAUDE.md` to `AGENTS.md` so every agent finds the same text. With a
template written and nothing said about the project, no setup thread is started; with a
description it is, told not to scaffold again but to adapt the starter. Add a template by adding
its files, an entry in `templates.rs` (`ALL`, with its test command and CI steps) and the same
stack in `stacks.ts`; a test writes every template and checks that no placeholder is left. `public`
makes the GitHub repository public, and `ci` writes `.github/workflows/ci.yml` (checkout, install,
test) before the first commit, or says in the project's notes why not (a static page has no tests).
`GET /api/projects/templates` answers which templates this machine's core writes and which of the
tools the stacks need (python, uv, php, composer, node, npm, go, cargo, dotnet, flutter, swift,
godot, bash) it has, with versions; the dialog marks each as found or missing. The other stacks
(Laravel, SvelteKit, Flutter, ...) are started by the setup thread with the stack's own generator,
and the stack goes into the thread's first message (`stackHint`). A hubd without feature
`project.templates` writes no templates, so every stack then goes to the setup thread by name and
the options that need the core (public repository, build checks) are disabled with "needs the
next version of Hub". The machine the project is made on can be another one (the call carries
`?host=`; its defaults, GitHub login and features are asked from that machine), and "Own worktree
per thread" is set after the project exists, on this machine only.

A quick thread (one in a Scratch folder, started with "No project") has a "Make project" link in
its header (`ThreadView.svelte`, `hub.makeProjectOf`). It opens New project on the thread's machine
with the thread's title as the name and a description saying where the thread's files are; the
setup thread (always on then) brings over what belongs and leaves the rest. No call is new: the
files are moved by the agent, not by the core. After "New project" makes a project, its page shows
a few short first-steps lines (`hub.firstRun`, `ProjectView.svelte`): open the setup thread, say what
to build in the box, one thread per job. They go away with their cross or the first thread sent from
that page, and are not stored.

Threads are grouped into projects by `projectFolder` in `app/src/lib/format.ts`: an attempt's
worktree or a dev-mode thread's folder counts as its repository, and a "No project" thread's own
folder (`<projects root>/scratch/<date-time>`, made by `scratchFolder` in `folderPicker.ts`, re-exported by `FolderPicker.svelte`)
counts as one Scratch project per machine, the `scratch` folder. So quick one-off threads never
become projects named by a time stamp, and Home never offers a scratch folder again as a recent
folder. A thread started from the Scratch page still gets a fresh folder of its own. The iPhone
app's everyday chats (`chat` on `POST /api/phone/threads`, feature `phone.chat`) get
`<projects root>/chats/<date-time>` instead: they are not dev threads and the path alone tells the
two apart (`isChats`, `CHAT_RUN`). They belong to the Chat world and to nothing of Dev's: Inbox,
the project lists (no Chats project), the pickers and Ctrl K in Dev leave them out (`isChatThread`;
`hub.everything` drops them, `hub.chats` lists them for Chat). See "Dev and Chat worlds" for the wall.

### Focus
<!-- covers: crates/hub-core/src/focus.rs, crates/hub-core/src/focus/, crates/hub-core/src/hub/focus.rs, crates/hub-core/src/ideas.rs, app/src/lib/components/FocusBar.svelte, app/src/lib/components/FocusCard.svelte, app/src/lib/components/TestTab.svelte, app/src/lib/components/TestWindow.svelte, app/src/lib/testScripts.ts, app/src/lib/components/IdeaBox.svelte -->

`docs/FOCUS.md` says where the project is going and what counts as done
(`crates/hub-core/src/focus.rs`, `app/src/lib/components/FocusCard.svelte`). It is plain markdown
with three kinds of `##` section: `Direction: …` with a "Done when" checklist; stages, every other
heading in order, each with "Delivered when", "To test" and an optional `Ideas:` line; and
`Parked`, a bullet list of ideas for later. The stage marked `(active)`, or else the first one not
fully ticked, is the stage now. It is shown on top of the project's overview, because the reason
for it is losing focus: many threads at once, half-done work shelved, forgetting what to test. The
card shows the stages as a line, the stage now with its two checklists, and the direction and
what is parked on the side. Ticking, adding, parking and moving to another stage edit the file
(`POST /api/project/focus`), so agents and the user share one record. A rule in CLAUDE.md (added
with the docs rule, or from the card) asks agents to tick off what they deliver and to add what
the user must test by hand, so finished work leaves a line to try instead of a thought to keep.
"It doesn't work" on a test puts a message about it in the composer. Without a file the card
offers a blank start or a thread that drafts it from the roadmap, the docs and git history.

The project page's **Test** tab (`TestTab.svelte`) lists the stages' "To test" lines as scripts to go through in stage order: Start opens what to do and what to look for (the text before and after "Look for:"), ticking writes back to docs/FOCUS.md through the same `editFocus` call as the focus card, and "It does not work" fills the composer. A line may carry `Prompt: "…"` (Start a thread with it opens a draft in the project; Copy) and `Go: Settings > Updates, Machines` (buttons that take the main window there, `hub.goTo`); `testScripts.ts` parses them and lines without them still show. "Own window" pops the list out into a window of its own for a second screen (`TestWindow.svelte`, the same `open_viewer_window` as the viewers, with `kind: "tests"`); it follows the file every 5 seconds, and what it starts or opens happens in the main window over the `hub-main` channel. It only reads the focus the page already has, so it works against any Hub.

Two things keep the focus in view away from the page. The sidebar's project rows (System) are
one line each, and carry a second line only when there is something to say: the stage now, "3/6 ·
2 to test" when something waits to be tested (`POST /api/project/focus/summaries`, this machine's
folders only), and open pull requests. They are not sorted by when they last moved and show no
times: the pinned ones come first in the rail's order, then the rest by name. A conflict-fix
worktree (`<data>/fix/pr-<n>`) and the widgets folder count as temporary (`isTemporary` in
`format.ts`) and fold into one "Temporary" row under the list, which also keeps them off the rail's
recent projects. Quick threads (started with no project, so in a Scratch folder on any machine) are
not projects at all: they are one row, "Quick threads", fixed under the scrolling list so it never
scrolls away, with a count and the needs-you and working dots. It opens Inbox narrowed to them
(`workScope.quick`, which `ActivityList` reads as "every Scratch folder on every machine").
And a thread you start (`POST /api/threads`, not side jobs or routines) is checked against the
stage now: the tidy engine and model get the stage and the first message and answer whether it
belongs, leaning towards yes for fixes, questions and chores (`Hub::focus_check`). Only a no is
logged, as a `focus` event, which the thread shows under that message with Park it (adds the
thread's title under Parked, stops the turn and settles the thread) or Keep going. The thread
never waits for the check. Settings > Safety > "Stay on the stage" (`focus_check` in
`GuardPrefs`) turns it off.

Focus mode narrows Hub to one project while you work on it (`prefs.focusOn`, a window setting,
`FocusBar.svelte`). `hub.active`, which every list reads (the activity list, Home, the window
title's count, System's projects), keeps only that project's threads; `hub.everything` still has
them all, for search. Notifications from other projects are not shown but kept in `prefs.held`,
the latest per thread; leaving shows them once at the top of the sidebar. The sidebar's focus
bar shows the project's stage now from its `docs/FOCUS.md`, and a new thread starts in
the project. It is turned on from the focus card, a project's menu or the command palette.
System's project list says how many projects focus hides ("N more hidden by focus · Show all",
which leaves focus), so a forgotten focus never reads as projects gone. Going in whose desk cannot
be opened (a machine whose hubd has no desks yet) puts focus back to what it was.

The idea inbox feeds the focus from anywhere (`crates/hub-core/src/ideas.rs`,
`app/src/lib/components/IdeaBox.svelte`). Ctrl I, the bulb next to search, the phone page's box
and the `park_idea` tool all `capture_idea` (the box's mic is the composer's `ComposerVoice`: live words, final text, polish, the project as hint): the idea is kept at once as sorting, then placed.
An idea that names one project as a word goes there; otherwise the tidy engine and model get
every project on this machine (added ones and folders threads work in) with its stages, and the
project you were on as the hint, and answer with a project, a stage and the idea as one short
line. Without an answer it goes to the project you were on, or the only one. It is written under
that stage's `Ideas:` (focus op `idea`, as bullets after the label) or under Parked; a project
without a focus file gets one with only Parked. Where it went is kept in the store (`kv` key
`ideas`, the last 60) and streamed as `idea`, so the box can show it, move it
(`POST /api/ideas/{id}/move`, which takes the line out of the old file with
`focus::remove_idea`) or take it back (`DELETE`). One no project could be guessed for waits as
unsorted until you pick one.

### Desktop notices

<!-- covers: app/src/lib/native.ts, app/src/lib/prefs.svelte.ts, app/src/lib/notices.ts, app/src/lib/components/NoticeSettings.svelte, app/src/lib/presence.ts, app/src-tauri/src/toast.rs -->

The window raises Windows notifications itself: `announce` in `state.svelte.ts` turns a change of status into a
sort (`sortOf`: needs you, failed, done) and hands it to `runNotices` in `notices.ts`, which shows it through
`notify` in `native.ts` (`tauri-plugin-notification`). Nothing is on until chosen: the switches
(`notifyWaiting`, `notifyFailed`, `notifyDone`, `flashTaskbar`) are put back to off once for everyone
(`prefs.quietStart`), so a test chat cannot fill the screen.

Settings > Notifications (`NoticeSettings.svelte`) is a card per device. This PC holds the switches for the three
sorts, each with a scope (`noticeScope`: all projects or one), the taskbar flash and the sound. The iPhone card is
read only (`api.phoneNotify`): the phone app owns those settings and syncs them to every machine, so editing them
here would be overwritten; its "Send test" is `phoneNotifyTest`. "Your own lines" is the sentence builder
(`noticeLines`: "tell me when X in Y as Z, at once or after N minutes"); a line with a wait only speaks if the
thread is still in that status, and timers are cleared per thread on each new status.

Quiet rules (`noticeQuiet`, `quietWhy` in `notices.ts`) are checked right before anything is shown: paused
(1 h, 4 h, until 08:00), quiet hours (done and failed wait; "needs you" passes unless `strict`), and while you are
at the PC (Hub in front and input within 3 minutes, `idleMs` in `presence.ts`). Held back means not shown; the
thread is still in the inbox. "Send test notice" shows one at once, ignoring these rules (`notify(…, force)`),
and says whether Windows allowed it. Windows draws a toast only for an app it knows by an AppUserModelID, so the
shell names Hub to it at start (`toast::register`: `HKCU\Software\Classes\AppUserModelId\se.karlsson.hub`); without
that the sound plays and nothing is drawn. `toast_check` (called by the test through `toastCheck`) registers again
and reads why a toast would still stay hidden (notifications off for Windows or for Hub, Do not disturb/Focus,
full screen, presenting), and "Send test" says so instead of claiming it showed. The next steps (one rule brain in the core shared with the phone's ntfy, a
Windows shell with tray and toast buttons, a gateway for widgets and servers) are under Parked in
`docs/FOCUS.md`.

### Notices from outside

<!-- covers: crates/hub-core/src/notice.rs, crates/hub-core/tests/it/notices.rs -->

The gateway in for widgets, scripts, servers and Windows tools: `POST /api/notices` (feature `notices.post`, with
the connection token like every call) takes `{ source, title, body?, sort?, link?, phone? }`, `GET /api/notices`
gives the last hundred (the core keeps 300 in memory only). A notice goes out on the ordinary stream as
`StreamMsg::Notice`, so every window and client already following the stream gets it, and nothing in the core shows
it: the window decides under Settings > Notifications. `sort` is `info` (the default), `waiting`, `failed` or `done`;
`link` must start with `hub://`, `https://` or `http://`. A source (free text, `outside` when left out) may send 20
a minute; more gets `429`, so a loop in a script cannot flood anyone. `phone: true` also asks for the phone, and it
goes through the phone's own choices in `ntfy.rs` (topic, switches, quiet hours; `info` counts as done); what
happened comes back in `phone` ("sent" or why it was held). It is off unless asked for. The types are `Notice` and
`NewNotice` in `model.rs`. A narrower token for outside clients (`Grant::Widget`) and the window's own rules for
these notices come in later steps (see Parked in `docs/FOCUS.md`).

### What a project made
<!-- covers: crates/hub-core/src/outputs.rs -->

The project page's Library tab (`outputs.rs`, `ProjectLibrary.svelte`, `GET /api/project/outputs`)
lists what the project's threads made, read from their logs each time, so nothing new is kept:
pages agents showed (one per page, with its versions), pictures (image blocks with a file, side
jobs' pictures, and ones only in the log), docs the threads wrote or changed, new files that are
not code, and servers the project's threads run now (`runtime.rs`). Events of all the project's
threads are read in time order, so the thread that wrote a file first made it and later ones
changed it. For each thing it says where it is now: kept in Hub (pages), on this machine or gone,
and for files what git says on the checked out branch (committed, changed, not tracked), with
the first commit added to its life. Desks are left out. The desk's panel shows the latest four.
It does not reach other machines yet, and it does not follow a file to another branch or to a
merged pull request.

Clicking a thing opens it big inside the tab (`projectItem` in the route, so Esc and the mouse's
back button return to the list, and ← → step through it): pages in a frame, docs rendered,
other text as is, pictures as they are. The core only reads a thing by its library id
(`GET /api/project/outputs/text` and `/raw`), never any path it is handed. The file manager opens
only from "Show in folder".

### Docs as linked notes
<!-- covers: crates/hub-core/src/vault.rs, crates/hub-core/src/vault/, crates/hub-core/src/covers.rs, app/src/lib/components/DocsView.svelte, app/src/lib/components/CodeView.svelte -->

The docs tab reads every `.md` file in the project (honouring `.gitignore`, skipping
`node_modules`, `target` and the like) the way Obsidian reads a vault
(`crates/hub-core/src/vault.rs`, `app/src/lib/components/DocsView.svelte`). `[[Doc]]`,
`[[Doc#Heading]]`, `[[Doc|text]]` and relative `[text](other.md)` links resolve by path, file
name (the nearest one wins) or title; `#tags` and YAML front matter (`title`, `summary`, `tags`,
anything else shown as properties) describe a doc. The tab shows a tree, the docs changed lately
and the tags; the doc with live links and a preview on hover; and on the side its outline, what
links to it with the line around each link, docs that name it without linking, and where it
links, with links to docs not written yet dashed. Ctrl O finds a doc by title, heading, text or
`#tag`. The doc is part of the route, so back and forward step through docs. The tab fills the height and each
column scrolls on its own; the finder and the link preview are moved to the end of the page
(`portal.ts`) so the page's scroll mask and header cannot clip them. Code blocks, and files on the
Files tab (`CodeView.svelte`), are coloured with highlight.js (BSD-3, `code.ts`) in Hub's own
tokens (`.hljs-*` in `app.css`), with the language, Copy, and line numbers and Wrap for files.
Edit on a doc swaps the reader for the file in a text box; typing `[[` offers the docs to link,
Ctrl S saves (`PUT /api/project/vault/doc`, markdown inside the folder only).

The format is plain Obsidian-flavoured markdown on purpose: the folder opens in Obsidian or Foam
as well, and agents read and write it with ordinary file tools. They also get `docs_map` (the
whole tree with summaries, tags, link counts and missing links), `docs_search` and `docs_read`
(a doc with its links and backlinks) on the thread's MCP server, and the living docs rule asks
for a `summary:` and `[[links]]`. The index is kept in memory per project and rereads only files
whose time or size changed, so asking often is cheap.

Docs say which code they describe (`covers.rs`): `covers: path, folder/, app/**/*.svelte` in the
front matter for a whole doc, or a `<!-- covers: ... -->` line right under a heading for that
heading's section (up to the next heading at its level). A plain path takes in itself and, as a
folder, everything under it; `*` and `**` are globs. Agents get `docs_for` with a list of paths:
the sections that cover each, their line range and text (60 lines at most), the paths no doc
covers, and a reminder to keep what it says true. Every section of this file carries such a line,
and `cargo test -p hub-core covers` fails when one matches no file any more, so the map does not
rot when code moves.

### Files read as what they are
<!-- covers: crates/hub-core/src/sheets.rs, crates/hub-core/src/dbview.rs, app/src/lib/components/Changes.svelte, app/src/lib/components/FilePreview.svelte, app/src/lib/components/DocReader.svelte, app/src/lib/components/JsonTree.svelte, app/src/lib/components/DataTable.svelte, app/src/lib/components/LogView.svelte, app/src/lib/components/NotebookView.svelte, app/src/lib/filePreview.ts, app/src/lib/docs.ts -->

One reader shows any file as what it is (`FilePreview.svelte`; the kind comes from the name in
`filePreview.ts`), with a small bar to switch between the reading and the file's text:

| Kind | Shown as |
| --- | --- |
| `.md` | rendered by the docs tab's renderer (`DocReader.svelte`, `renderDoc` with links unchecked): front matter as labels, highlighted code with Copy, tables |
| `.json` (comments and trailing commas allowed) | a folding tree, clicking a key copies its path (`JsonTree.svelte`) |
| `.jsonl` / `.ndjson` | a table of the objects' keys, or a tree per line; lines that do not parse are counted |
| `.xlsx` / `.xlsm` / `.xls` / `.ods` | one table per sheet, the sheets as tabs (first row as the header, at most 1000 rows and 60 columns a sheet), read by the core (`sheets.rs`, the `calamine` crate: formulas show the value last saved, nothing runs, 20 MB cap) from `GET /api/project/sheet` when the machine has `view.sheet`; an older Hub says it needs the next version |
| `.db` / `.sqlite` / `.sqlite3` | the tables as tabs (name and row count), the chosen table's columns and first 100 rows as a table; read by the core (`dbview.rs`, `rusqlite`, opened read-only, only `SELECT`) from `GET /api/project/database` when the machine has `view.database`; a file that does not start with `SQLite format 3` says it is not a SQLite database; an empty file has no tables; blobs show as their size |
| `.csv` / `.tsv` | a table that sorts, filters and keeps its header (`DataTable.svelte`); the CSV separator is guessed (comma, semicolon, tab) |
| pictures, `.svg`, `.pdf`, sound, video | themselves, from `GET /api/project/raw` as a blob address; SVG only ever as a picture, so its scripts never run |
| `.html` | a still page in a frame with `sandbox=""` (no scripts, no forms) |
| `.ipynb` | cells: Markdown rendered, code coloured with run numbers, text, picture and error output; HTML output stays text |
| `.log` | lines coloured by level, terminal colours kept, errors and warnings to filter (`LogView.svelte`) |
| `.env` | keys with their values hidden until Shown |
| code, anything else | coloured with line numbers (`CodeView.svelte`) |

Text comes from `GET /api/project/file` (512 KB at most, said when cut), bytes from `/raw` (64 MB
at most); both work for other machines. An absolute path outside the thread's folder is read from
its own folder (`placeOf`).

It is used in four places. Under each reply the files it changed are chips with their +/−
(`Changes.svelte`); a chip opens below the line, Markdown, JSON, tables and the rest as they read,
code as its diff, with File, Diff, Wide (the viewer) and, for Markdown in the project, Docs. The
text there comes from the reply's own edits when it wrote the whole file, so an old reply shows
what it wrote then. A file path in a reply opens in the viewer (`kind: "file"`, the id the path)
when Hub can read it; Ctrl-click shows it in its folder. And the project's Files tab reads with
it, so pictures and PDFs open there too.

### What belongs to a project
<!-- covers: crates/hub-core/src/runtime.rs, crates/hub-core/src/process_marks.rs -->

Nothing is tagged by hand (`crates/hub-core/src/runtime.rs`). Every engine is started with
`HUB_THREAD_ID` and `HUB_PROJECT` in its environment (the names live in `process_marks.rs`, below
`runtime`, `services`, `workers` and `host`), and everything it starts inherits them, so a
dev server a thread started belongs to that thread's project wherever it runs. A process without
them belongs to the project whose folder it runs in. Docker containers belong to the project
compose started them from (`com.docker.compose.project.working_dir`). A server the project's
processes are connected to but did not start, such as a shared Aftermath, is listed under "Uses"
and is never stopped from the project.

### Workers
<!-- covers: crates/hub-core/src/workers.rs -->

The Workers view (`crates/hub-core/src/workers.rs`) shows everything Hub has running in the
background on one host, found the same way: each thread's engine (Hub's child carrying
`HUB_THREAD_ID`) and every process carrying the thread's id, as trees with CPU and memory;
processes left behind by archived or deleted threads; all running containers; and Hub's own
helpers. Stop, pause and resume act on a process and everything under it (pause is
`NtSuspendProcess` on Windows, `SIGSTOP` elsewhere). Engines are not paused: their turn is stopped,
or the engine shut down as the idle timeout would. Hub, the processes it runs under and the
operating system are refused. Shutting a thread down with everything (`stop_thread`) stops every
process carrying its id and waits, bounded, until they have ended (on Windows asked of the process
itself, since an ended one stays listed while a handle to it is open), so its folder can go next. Each host will report its own workers once remote hosts exist.

### Sessions
<!-- covers: app/src/lib/sessions.svelte.ts, app/src/lib/components/SessionRow.svelte, app/src/lib/components/StatusBar.svelte -->

Live agent sessions show under their machine (the Machines page, below) and as the status bar's
item, so a settled thread that still has something running stands out. They add nothing to the
core: `sessions.svelte.ts` asks each machine for `/api/workers` and `/api/host` (`?host=` for
another one) and makes one session per thread whose engine runs, plus threads that are settled,
archived or gone and still have processes (those are "should be gone", in red; an engine with no
thread row younger than 5 minutes is a warm one from `warm.rs`, waiting for the thread being typed,
and is left out), plus threads that
wait on you with no engine (asleep, see Sleeping sessions, with the moon sign). `SessionRow.svelte`
draws one: only icon, colour and time, the inbox's state sign (working, asks you, idle, should be
gone, asleep), the provider mark and the memory taken; on a machine's page (`wide`) also a memory
bar against the machine's real total, CPU and a stop button. An idle row counts down to the engine
stopping (`IDLE_TIMEOUT` in `engine/acp.rs`, repeated as `IDLE_STOP_MS`; it counts from the
thread's last activity, so it is close, not exact). Stop shuts the engine down (and everything the
thread started when it should be gone); a row in a turn asks twice. The bar item shows the same
icons with counts and opens a card with the rows; a machine that is about to run out of memory or
disk gets a red chip beside it. The bar asks every 15 s, the Machines pages every 3 s.

### Sleeping sessions
<!-- covers: crates/hub-core/src/sleep.rs, crates/hub-core/src/hub.rs, crates/hub-core/src/hub/permissions.rs -->

An engine that waits on a permission question keeps its process and memory until someone answers,
which can be a night. `sleep::watch` (started by hubd, every 30 s) calls `Hub::sleep_waiting` with
`sleep::AFTER` (5 minutes): a thread whose open questions are all older than that gets its engine
stopped (`shutdown`) while the questions stay as they are. It logs a `slept` event first (naming the
questions), takes the questions out of the open map without answering (so the stop is not logged as
an answer), and leaves the thread `waiting_on_you`: a waiting thread with no engine is asleep. The
cancelled `turn_end` that follows is the engine going, not the user cancelling. While the panic
button has halted Hub nothing sleeps.

The answer wakes it. `answer_permission` finds no open question, and `wake_from_sleep` accepts it
when the log shows that question asked, slept and not answered: it logs `permission_resolved` with
`woke: true`, adds a user message telling the agent what it asked and what the user chose, starts
the engine (`load_session` resumes the same session, as after a restart) and sends that message as
a prompt. Nothing is answered for the user and nothing is carried over from before the sleep except
the session itself. Old clients read `slept` as an unknown event and keep working.

### Your tools
<!-- covers: crates/hub-core/src/own_tools.rs, crates/hub-core/src/own_tools/, crates/hub-core/src/hub/tool_handoff.rs, crates/hub-core/src/facts.rs, app/src/lib/components/OwnTools.svelte -->

Programs of your own that run beside Hub (Forman, Knegarn, Aftermath) show in the Workers view
when their folder holds a `hub.toml` (`crates/hub-core/src/own_tools.rs`, `OwnTools.svelte`). Hub
has no code for any one tool: the file says which commands print one JSON document, where in it
the headline, state and lists are, and which commands Hub may run. Each core looks in its projects
and in every folder right inside the home folder and the projects folder (so `~/Forman` on
Plåtnicklas is found by Plåtnicklas' core), and the app asks every online machine with `?host=`.

A `hub.toml` names commands that Hub and its agents run, so a cloned repository must not get them
just by sitting in the home folder. Only the folders of projects the user added run on their own;
any other folder runs once it is approved (kv `own_tools.allowed`, `POST /api/tools/allow`, the
"Not allowed yet" row in the Workers view, and installing a Worker). The first time a core runs
this it approves the tools it finds, so earlier setups keep working. `GET /api/tools/unapproved`
lists the ones waiting; feature `tools.allow`.

```toml
name = "Forman"
about = "One line on what it is."
env = { PYTHONPATH = "src" }          # optional, on top of the core's environment

[status]                              # optional
run = ["python3", "-m", "forman", "now", "--json"]
headline = "/headline"                # a JSON pointer, or text with pointers: "Capture is {/overall}"
state = "/status_word"                # one word; coloured when it plainly means ok, warn or bad
detail = "/status_reason"

[[list]]                              # any number
title = "Waiting on you"
run = ["…"]                           # optional: the status document when left out
items = "/questions"                  # where the array is
where = { status = "open", blocking = true }   # equal to the value, or one of a list of values
id = "/id"                            # pointers below are into each item
text = "/question"
state = "/status"
detail = "{/title} · default: {/default}"
link = "/delivery/pr_url"             # opened in the browser
limit = 8                             # how many are shown; `total` counts them all
reverse = false                       # true when the array is oldest first
needs_you = true                      # its items count as waiting on you

[[action]]                            # any number
id = "answer"
label = "Answer"
on = "Waiting on you"                 # optional: run on one item of that list, its id as {id}
ask = ["text"]                        # typed first, each filling {name}
danger = false                        # true: the app asks twice and agents need full access
panic = "pause"                       # optional, "pause" or "resume": run by the panic button
about = "Shown on hover."
run = ["python3", "-m", "forman", "answer", "{id}", "{text}"]
```

Commands run in the tool's folder without a shell, so an input fills an argument (or part of one)
and is never parsed; `${VAR}`, `${VAR:-default}` and a leading `~` are expanded, and `python3` is
`python` on Windows. A command may exit non-zero and still answer: the JSON it prints counts, and
when there is none its last line is the error. A command gets 30 seconds, and a reading is reused
for 10 so the view can poll. Lists sharing a command run it once. An action runs only for a tool
found on that machine (`POST /api/tools/action`), and the reading is taken again after it.

Agents get the same as Hub's tools: `your_tools` (read) and `run_tool_action` (supervised; a
`danger` action needs full access, so an engine Hub runs cannot halt Forman). As with Desk before
it, Hub never gets a button the tool's own CLI lacks: every action is a command a person could
type in that folder.

**Facts about threads** (`crates/hub-core/src/facts.rs`). One record of work done on every machine
without a copy of Aftermath on each: a tool that keeps a `hub-facts.toml` beside its `hub.toml`
(`kind = "hub-thread-facts-v1"`, `dir` for its spool on that machine, and an optional `ingest`
command) takes files Hub writes about finished stretches of development threads. It is a separate
file so a hubd from before this still reads `hub.toml`, whose parser refuses unknown sections.
A *stretch* is the work since the last one, closed when a thread has had no event for 20 minutes
and its log holds a finished turn (`turn_end` or `error`). Nothing is written for an everyday chat
(a folder under `chats/`), for a thread relayed from another machine, or while no machine has a tool
that takes the files. The file is a pure function of the log (`facts::envelope`; the contract is
Aftermath's `docs/contracts/118`): the reference `hub:stretch:<thread>:<seq of the last event>`
names it, the file name is the SHA-256 of `hub`, a NUL and the reference, so a stretch written twice
is one file. It holds the engine and model names, how it ended, the turns, and Hub's own proof and
second opinion as *labelled facts, never a verdict* (the agent's side of the work); no prompt, reply,
path, branch or repository. A value that is not a plain name is left out, since the tool refuses a
file with a value outside its vocabulary and stops on it. The thread's last closed `seq` is kept in
the store (`facts.closed.<thread>`), a thread idle for over a week when first looked at is skipped,
and files wait in `<data>/facts/outbox/` until every machine that takes them has them (for 14 days).
Delivery is `POST /api/tools/facts` to each online machine whose `GET /api/tools/facts` names the
kind, and straight into the folder when the tool is here; the receiver checks the name and that the
text is JSON, writes `pending/<name>` unless it is already there or in `done/`, and runs `ingest`.
Feature `tools.facts`; an older hubd answers 404 and counts as taking nothing.

### Rules in force
<!-- covers: crates/hub-core/src/standing.rs, crates/hub-core/src/git.rs, crates/hub-core/src/api/projects.rs, app/src/lib/components/RulesTab.svelte, app/src/lib/rulesView.ts -->

`GET /api/project/rules?path=` (`standing::read`, compat feature `project.rules`) lists the rules the
project's rules files give its agents: `CLAUDE.md`, `AGENTS.md` and `GEMINI.md`, one rule per
top-level `- ` bullet with its indented continuation, code fences skipped, a rule already seen in an
earlier file (an `AGENTS.md` mirror) dropped. Each rule has a theme from its words (`merge`, `test`,
`ui`, `docs`, `code`, `other`), a source (`hub` under `## Rules from Hub`, else `repo`), file and
line, and when it last changed: the newest `git blame` time over its lines (`git::blame`), with the
commit and its subject. A rule saved from chat (`RuleSaved` event, `Store::rules_said`) takes the day it
was said instead, and carries the thread's id and title (`standing::with_threads`), so one not
committed yet is dated too. Newest first. Read only; no memory files, settings or routines yet.

The project page's Rules tab (`RulesTab.svelte`, pure parts in `rulesView.ts`) shows them one line each with the
file (a chat rule: its thread) and age, "Changed this week" on top, theme chips and a search; a hubd without the call shows "Needs a newer Hub".

## Plugins
<!-- covers: crates/hub-core/src/plugins.rs, crates/hub-core/src/plugins/, app/src/lib/components/PluginsView.svelte, app/src/lib/components/PluginPage.svelte, app/src/lib/components/PluginCard.svelte, app/src/lib/components/PluginChatCard.svelte, app/src/lib/plugins.ts -->

GitHub repositories made for agents, to find and read before bringing them into Hub
(`crates/hub-core/src/plugins.rs`, System > Plugins). Everything is read through the `gh` the
machine is logged in with; no keys. A repository's kind (`mcp`, `skill` or `tool`) is guessed from
its topics, name and description (`kind_of`). A `hub-worker` topic, or a `hub.toml` with a
`[worker]` section found when its page is read, makes it a **Worker** (see "Workers"), and a Worker
can be installed: Install clones it with `gh repo clone` into the projects folder under its name
(`POST /api/plugins/install`), where Hub finds its `hub.toml` like every other tool of yours, then
runs its ready check (`[worker] ready`, one JSON document `{ ok, checks: [{ section, status, label,
detail }] }`) and lists what is missing; Check again runs it again (`POST /api/plugins/ready`). Your
own repositories install straight away; anyone else's only once Customs has cleared them
(`PluginWorker.why_not`).

A repository's page has a **Review** button in its Customs card. It puts the repository on Customs'
list (kind `github-repo`, name `owner/repo`, held with "Waiting for review") when it is not there
(`PUT /api/customs`), then asks for the review (`POST /api/customs/review`, see "Customs"), shows
"Reviewing…" while it runs (a review already running, say an agent's, shows the same: the page
looks at `GET /api/customs/activity` every few seconds), and then the stamp and the reason it
gives (Cleared, Held or Refused). Until the hubd has `customs.github` in `compat::FEATURES` (it
reads repositories, not only packages) the button is disabled and says it needs the next version.

- **Discover** searches six agent topics (`plugins::TOPICS`: mcp-server, model-context-protocol,
  claude-code, claude-skills, agent-skills, codex; the engine ones, with the description words and
  top-level folders that mark a repository as made for an engine, are `connections::MADE_FOR`)
  twice each: worked on in the last month with
  over 50 stars, and made in the last 30 days. It shows Rising this week, New this month and Most
  stars. The lists show at once: the last ones counted are kept in memory and in
  `plugins-discover.json` in the data folder, and when they are over an hour old (or on Look again)
  Hub counts again in the background while the page shows the old ones (`updating`, the page asks
  again every few seconds). With nothing counted yet, New and Most stars come after the searches
  and Rising follows when its stars are counted. Star pages are read in parallel, at most 16 at once
  (`STAR_SLOTS`), and only as many as the rate says reach two weeks back (`pages_to_reach`). A Show switch filters every list by
  kind, with a count each and a line on what that kind is (`KIND_SAYS`); quick searches sit under
  the search box (`QUICK`). Every
  list is cards (`PluginCard.svelte`): Rising as big ranked cards with the week's gain and star line
  (first six, Show all for the rest), the others as compact cards with name, maker, stars and Customs.
  A repository opens on its own page, or, when the window is at least 1500 px wide, in a panel beside
  the lists (`PluginPage` with `side`; Esc or clicking the card again closes it).
- **Rising** is counted from the stars themselves, since GitHub has no trending list: the
  stargazers list, newest last, carries when each star was given (`recent_stars`, at most page
  400, so not over 40 000 stars). Hub reads the last pages back two weeks (5 pages, 15 for the
  fastest twelve) and counts the last 7 days (`week_of`); when the pages do not reach back a week
  it projects from their rate and the app writes `~`. The sparkline is the running total over
  14 days, or over less for a busy repository (`curve`, `spark_span`).
- **Search** is GitHub's own repository search, best match first; `owner/name` or a GitHub link
  in the box opens that repository's page.
- **A plugin's page** (`plugins::detail`, kept half an hour) has the repository, its README with
  pictures taken out, its own social picture when it has one (else the app draws its name as a
  hex dump), commits this month, people, a 30-step star line, what the files at the top say it is
  (`inside_of`: SKILL.md, `.claude-plugin`, the MCP SDK in package.json or pyproject, programs),
  Hub's first look (`checks_of`: age, licence, last work, owner's age, npm install scripts) and
  three similar ones by topic. Pictures come from GitHub's picture hosts only and travel as data
  URLs, since the app shows no outside pictures.
- **In threads**: agents find plugins with `plugins_search` (one of Hub's tools; GitHub search, or
  what is rising when the query is empty) and show one with `suggest_plugin` (threads only), which
  writes a `plugin_suggested` event `{ repo, why }` with the repository as it is then. The thread
  shows it as a "Found a plugin" card (`PluginChatCard.svelte`) with the agent's reason, stars,
  this week and Customs' word, and Open. Both tools never ask (`permit::QUIET_TOOLS`), and Hub's
  instructions tell agents to use them when the user wants a tool for something. In any reply a
  link to a GitHub repository itself (not a pull request or a file) becomes a plugin reference
  (`rich.ts`) that opens it in Plugins.
- **Customs** stamps a repository as kind `github-repo` with its `owner/name`; cards and the page
  show that status, or Not checked. Customs reviews a repository itself (see Customs, "A GitHub
  repository", `POST /api/customs/repo`); the page does not call it yet. Installing, Hub's MCP as a
  gateway to installed plugins, switching one on per thread or project, Try it here and lines
  for a plugin in use are not built yet.

## Library
<!-- covers: crates/hub-core/src/library.rs, app/src/lib/components/LibraryView.svelte -->

One folder of linked notes about everything the user runs and builds, shared by every thread
(`crates/hub-core/src/library.rs`, `LibraryView.svelte`). It lives in `<projects root>/library`
(or the `library.root` setting) and is the same markdown as the docs tab, so it opens in Obsidian.
It is written mainly for agents, after Karpathy's "LLM wiki": every thread's engine gets
`library_map`, `library_search`, `library_read`, `library_write` and `library_understood`, and
`library::RULE` in the MCP instructions tells it to look there first and write back what will
still be true next week. `index.md` is the front page and `log.md` records every change, newest first.

Hub writes what it knows for a fact into each note, between `<!-- hub:facts -->` and
`<!-- /hub:facts -->`: a note per project (folder, stack, remote), per technology in a stack (which
projects use it) and per machine. That block is rewritten each time the library is opened or a
tool is called, so it never goes stale; `library_write` keeps it whatever the agent sends, and Hub
adds a note the first time it sees something new. The rest belongs to agents and the user.

The page has no tree: you reach a note from a link, Ctrl O, the index, or a stack chip on a
project page (click opens it, right click offers Learn). Asking in its box starts a thread in the
library that answers from it and writes back. Learn this starts a thread that explains the note
the way the user uses it, then tests them (explain it back, predict a case from their setup, one
tricky question) and only on a pass calls `library_understood`, which sets `understood: <date>`
in the note's front matter. A later rewrite keeps that mark.
**Handing a thread's work to a tool.** A tool whose `hub.toml` has a `[handoff]` section is offered
in the thread menu ("Hand to Knegarn…") on that tool's machine:

```toml
[handoff]
label = "Hand to Knegarn"
about = "Shown under the brief."
guide = """How to write the brief: the sections, what to leave out."""
run = ["python3", "hub_handoff.py", "--repo", "{repo}", "--title", "{title}",
       "--work", "{work}", "--brief-file", "{brief_file}"]
follow = ["python3", "hub_handoff.py", "--follow", "{work}"]   # optional
```

The thread's own engine drafts a title and a brief from the conversation with the guide
(`POST /api/threads/{id}/handoff/draft`, answered as a title line, then the brief). The person reads
and edits both in `HandoffDialog.svelte`, and pressing Hand over is the approval: only then is the
brief written to `handoffs/<id>.md` in the data folder and the command run in the tool's folder
(`POST /api/threads/{id}/handoff/tool`) with `{repo}` (the thread's folder), `{title}`, `{work}`
(`hub:thread:<id>`) and `{brief_file}`. What it printed is a `handed_off` event, shown in the
thread as one folded line. What the tool does with the brief is its own business; Knegarn writes a
mandate into its inbox and starts a pass. Examples of each tool's file are in `docs/examples/`.

With `follow`, the core asks every two minutes (`Hub::follow_handoffs`, for a week) how a thread's
latest handoff stands. The command prints `{ state, proven, model, cost, said, link }`: `state` is
waiting, running, done or stopped; `proven` whether its verification passed (null when unknown);
`cost` reported dollars. Once it says done or stopped, a `handoff_result` event goes in the thread,
with the pull request or branch as a link.

**What a result costs.** The Overview's Models and cost card (`stats::results`) counts, per
model, the changes Hub threads made that Hub checked for proof (`proof` events) and how many
showed it, and what one proven change cost: every Claude Code answer in the thread since the
change before it settled, the proof turns and the failures included, divided by the proven ones.
A change goes to the model that spent most on it; a thread's sessions are all its
`session_started` ones. Handed-off work counts as a result of its thread at the cost and with the
verdict the tool reported. The same card lists the most expensive threads.

### Workers
<!-- covers: crates/hub-core/src/worker_runs.rs, app/src/lib/components/WorkerRun.svelte, app/src/lib/components/WorkerChain.svelte, app/src/lib/components/ActivityList.svelte, app/src/lib/components/HandoffDialog.svelte -->

A Worker is one of your tools that does whole jobs by itself, with several agents (Knegarn). Its
`hub.toml` has a `[worker]` section, and each job, a run, shows as a thread
(`crates/hub-core/src/worker_runs.rs`). Hub has no code for any one Worker:

```toml
[worker]
label = "Knegarn run"                   # what one run is called
runs = ["python3", "knegarn.py", "inspect", "runs", "--limit", "30"]
items = "/runs"                         # where the runs are (default /runs)
id = "/run_id"
title = "/title"
work = "/work_ref"                      # hub:thread:<id> ties a handed-over run to its thread
started = "/started_at"                 # RFC 3339
ended = "/terminal_status"              # empty or null while it goes on
state = "/outcome"
said = "/escalation/reason"
link = "/delivery/pr_url"
cost = "/budget/cost_usd"
events = ["python3", "knegarn.py", "inspect", "events", "{run}", "--after", "{seq}"]
# In the events document (defaults): event_items = "/events", seq = "/seq", phase = "/phase",
# round = "/round", kind = "/kind", text = "/detail".
agent = "/data/role"                    # which agent an event is about
model = "/data/model"
pause = "/data/action"                  # "pause" / "resume"
ready = ["python3", "doctor.py", "--json", "--skip-claude-probe"]   # after installing

[[worker.step]]                         # the chain in the inbox, in order
id = "build"
label = "Build"
phases = ["implementing"]               # the tool's phases that are this step

[[worker.control]]                      # what the user may do to a running run
id = "pause"
label = "Pause"
run = ["python3", "control.py", "pause", "{run}"]
when = "running"                        # or "paused"; always while it runs when left out
ask = []                                # typed first, each filling {name}
danger = false                          # asked twice
```

While a run goes the core reads each Worker's runs every 15 seconds, backing off to every 2 minutes
while none does (`worker_runs::follower`), with 60 s for the list: a read that is late keeps the
list as it was and is only warned about after three in a row. A run handed over from a thread shows in that thread; one started elsewhere gets a thread of its own in the tool's
folder, made once (client id `worker:<folder>:<run>`), and only while it runs or if it started in
the last six hours. For each run still going, Hub replays its events after the last one it read,
with its own cursor (Knegarn's read contract says to), and folds them into a `WorkerRun`: the steps
(`done`, `now`, `todo`, and how often each was entered, since a repair goes back to testing), the
round, the agents with their calls and model, paused or not. When anything moved it writes a
`worker_run` event carrying that state and only the new lines (`WorkerLine`), so the thread holds
each line once. When the run ends (`ended` is set) the state is `done` for words like `completed`,
else `stopped`, with `said`; the thread comes back to the inbox, and an ended run is not read again.
After a restart the newest `worker_run` of each thread says where each run stood.

The inbox row shows the newest run of its thread as a third line, the chain (`WorkerChain.svelte`)
with the step going on lit (amber when paused, red where it stopped, green when done) and the
round; the app reads them from `GET /api/worker-runs` and keeps them current from the stream. In
the thread every `worker_run` of a run folds into one card (`WorkerRun.svelte`) where the run first
showed: the chain, the agents, the cost, how it ended, the pull request, the lines by round under
Show, and the run's controls. A control runs the tool's command in its folder
(`POST /api/worker-runs/control`) with `{run}` and `{authorization}`
(`hub:approved:<time>:thread:<id>`); one whose command takes `{authorization}` widens something, and
the card says that pressing it authorizes it. A control the run does not offer now is refused.

**Starting a run.** There is one way to start a Knegarn run from Hub, and it is not a command in
the tool's folder: open a thread, choose Hand to Knegarn (the tool's `[handoff]`), approve the
drafted brief, with Let it build on (the default; off, Knegarn only tries and builds nothing). Hub hands the brief over with the tool's
own `hub_handoff.py`, which stamps a contract and writes a mandate into the inbox, and the run then
shows in that thread. The dialog opens from the thread menu. When the person asks for it in words
("run Knegarn on this"), the thread's agent puts a card in the thread instead with `offer_handoff`
(`mcp/talk.rs`, threads only): a `handoff_offer` event with the tool, its folder and the agent's reason
(`HandoffOfferCard.svelte`). Whether any tool here takes handoffs at all, from the same folders and
files only, is `own_tools::takes_handoffs` (the bench's `handoff_tool` need). One press on its button is the approval, with no dialog:
`POST /api/threads/{id}/handoff/now` runs `Hub::hand_off_now` (`hub/tool_handoff.rs`) in the background, which
drafts the brief, hands it over with Let it build on, and when the tool asks something first
(Knegarn's test command and Docker image for a repository it does not know) has the thread's
engine answer from the folder's top files, falling back to the tool's own example, and hands over
again. The `handed_off` that follows ends the card's wait: "Handed over", or the reason it did not
go and Try again. "Edit first" on the card opens the dialog instead. Any tool with a `[handoff]`
works this way, Knegarn is just the first. Knegarn is found through the `hub.toml` in its folder (Settings > Plugins
installs it from GitHub); a copy with no `hub.toml` is not Hub's Knegarn, and its own README may
describe an older way (Trello cards, `orchestrator.py`) that no longer exists. Do not start runs by
running the tool's scripts by hand. Approving a handoff with Let it build on sets that project's
`execution_mode` to `local` in Knegarn's `projects.toml`; that approval is the person's ask, so
nothing else should move it past `dry_run`.

## GitHub
<!-- covers: crates/hub-core/src/github.rs, crates/hub-core/src/github_repos.rs, crates/hub-core/src/gitops.rs, crates/hub-core/src/prwatch.rs, app/src/lib/components/GithubTab.svelte, app/src/lib/components/PullView.svelte, app/src/lib/components/GitButton.svelte, app/src/lib/components/GithubAccount.svelte -->

After T3 Code's source control features, everything on GitHub can be done from Hub. It all goes
through the GitHub CLI (`gh`) of the machine the project is on, run in the project folder so `gh`
picks the repository from its remote; nothing new to log in to.

- **Every repository, and signing in** (`crates/hub-core/src/github_repos.rs`, feature
  `github.repos`): the folder pickers only know clones on the machines, so Hub also asks GitHub.
  `GET /api/github/repos` is one `gh api graphql --paginate` question for everything the account
  owns, collaborates on or reaches through an organization (name, owner, description, private,
  archived, last push), newest first and kept five minutes (`?fresh=true` asks again). A client
  matches it to local clones by the same key the picker uses (`github.com/owner/name`, lower
  case) and offers "Clone to <machine>" (`/api/folders/clone`) for the rest. Signing in is
  `gh auth login --web` run by the core of that machine (`GET/POST/DELETE /api/github/connect`):
  `state` is `ready`, `waiting` (with the one-time `code` and its `url`), `none`, `missing` or
  `failed`. Hub shows the code and never sees a password or token; when it succeeds it runs
  `gh auth setup-git` so `git clone` and `git fetch` there use the same sign-in (which also stops
  Git Credential Manager's own window). The list comes from the machine the window talks to; each
  machine signs in on its own, through `?host=`. No GitHub App and no tokens of Hub's own.
  Feature `github.account` makes the connection readable: a `ready` answer also carries the
  account's `name`, `avatar` and the `scopes` of the sign-in (from `gh api -i user`, the
  `X-OAuth-Scopes` header; empty for a fine-grained token), `state: lost` is a sign-in gh still
  holds but GitHub refuses (401, revoked or expired; an older client reads it as `none`), and
  `POST /api/github/disconnect` runs `gh auth logout` for that account and answers with the state
  after. Settings > Account shows this per machine (`GithubAccount.svelte`) with Connect,
  Reconnect and Sign out; a hubd without the feature shows the login only.
- **The GitHub tab** on the project page (`GithubTab.svelte`, `crates/hub-core/src/github.rs`):
  pull requests grouped by how close they are to merging (ready, needs work, waiting, drafts,
  merged today), each naming the thread that made it; issues (read, comment, close, create, start
  a thread from one); branches here and on GitHub, with the merged ones deleted in one go.
- **A pull request** (`PullView.svelte`): description, checks, merge (the repository's methods,
  default first, a second click to merge, delete the branch, auto-merge), the conversation with
  line comments, Comment / Approve / Request changes, the diff per file, and the commits. "Fix in
  a thread" starts a thread with the comment and the branch. Deleting the branch after a merge
  deletes only the one on GitHub; `gh`'s own `--delete-branch` would also switch the folder away
  from it under any thread working there.
- **The thread's Git button** (`GitButton.svelte`, `crates/hub-core/src/gitops.rs`): the next
  step for where the folder stands (Commit, Commit & push, Commit, push & PR, Push & create PR,
  Pull), the commit message and pull request written by the tidy model from the changes and the
  thread (`git_draft`), a second ask before pushing to the default branch with a way to make a
  branch instead. The branch in the header switches branch. The pull request chip opens the pull
  request in the right panel, where "Ask the agent to fix" sends a comment to the thread.
- **Watching a pull request** (`crates/hub-core/src/prwatch.rs`, after T3 Code's
  `watch_pull_request`): the `watch_pull_request` and `stop_watching_pull_request` tools of Hub's
  MCP, or the Git button's menu. Every two minutes Hub reads the pull request and messages the
  thread (a `user_message` marked `watch`, folded in the transcript) when a check fails, all checks
  pass, someone comments or reviews, or the branch starts to conflict. Comments with a Claude Code
  footer do not wake it. Watching ends on merge or close, after 8 failed reads, after 10 wakes in a
  row that only brought comments, on Stop, or when the thread is settled or archived; a watched
  thread does not settle by itself. Watches are kept in the store under `github.watches`. Hub also
  starts one by itself (`auto_watch`, from the tidy pass every few minutes, `ThreadPrefs.watch_prs`,
  on by default) for a thread that was active in the last six hours and whose branch has an open
  pull request, once for each pull request (`github.watch_auto`), so a Stop stays. Such a watch does
  not wake the thread when the checks pass, only for a failure, a conflict or a comment.
