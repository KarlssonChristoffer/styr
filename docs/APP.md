# The app
<!-- för: agent -->

Map of the Svelte app's files that `ARCHITECTURE.md` has no section for. Each section names its
files in a `covers:` line; the feature sections in `ARCHITECTURE.md` cover the rest of `app/src/lib`.
The app is Svelte 5 in `app/src/lib` (`components/` for views, `*.ts` and `*.svelte.ts` for state and
helpers). It holds no rules of its own: the core decides, the app shows (see Pieces in
`ARCHITECTURE.md`). Dropdowns are `Select.svelte`, switches `Switch.svelte`, never the browser's.

## Talking to the core
<!-- covers: app/src/lib/components/ConnectionList.svelte, app/src/lib/api.ts, app/src/lib/apiCall.ts, app/src/lib/apiThreads.ts, app/src/lib/apiProjects.ts, app/src/lib/apiGithub.ts, app/src/lib/home.ts, app/src/lib/compat.ts, app/src/lib/lastSeen.ts, app/src/lib/connections.ts, app/src/lib/stream.svelte.ts, app/src/lib/logCache.ts -->
- `api.ts` is every call to hubd (`api.*`, one function per route, written in `apiThreads.ts`, `apiProjects.ts` and `apiGithub.ts` over the fetch helpers in `apiCall.ts`) plus `connectStream`, the
  WebSocket of events. A request that carries a client id is carried out once by the core however
  often it is retried. `pageUrl` is where the core serves a live page, sandboxed.
- `home.ts` is which Hub this window uses: this computer's, or another machine's over the tailnet with
  its own token. `BASE`, `TOKEN`, `streamUrl` and `socketUrl` come from it; the token is shown only once
  `/api/health` proves the machine is the paired one. For this computer's own core, `ownToken`
  (called in `main.ts` before the app starts) asks the desktop shell for the connection token
  (`core_token`, the dev core's for dev mode's window) and adds it to every fetch and socket, so
  the core need not trust the app's `Origin` alone (see Security in `architecture/update.md`). A
  shell without the command, or a plain browser, leaves the window going by its origin.
- `compat.ts` is how a window copes with a hubd of another version: `hubdHas(feature)` asks the
  list in `/api/health` (`compat::FEATURES`), `tooOld(err)` is a 403, 404 or 405. A missing call shows
  "needs the next version" instead of an error. Rules: "Versions that talk to each other" in
  `ARCHITECTURE.md`.
- `stream.svelte.ts` is the live connection (`Stream`, behind `hub.start`, `resync`, `apply`, `catchUp`,
  `refreshHost`): it applies each streamed item to `hub`, batches events once a frame, reads a thread's
  log again when a seq is missing, and its watchdog reads the open running thread after 15 s of silence
  and everything when the window is looked at again. The state stays on `hub`.
  `resync` reads only the logs `hub.events` holds; `hub.follow` lets go of the least recently opened beyond
  8 (`logCache.ts`), never the open, working, waiting, desk or viewed thread, so a long session does not
  grow with every thread opened. An evicted thread loads again when opened.
- `lastSeen.ts` keeps the last answer to a slow question in the webview so a view shows it at once.
- `connections.ts` has the words for the MCP servers and connectors an engine reaches;
  `ConnectionList.svelte` lists them (source, whether each answers, which tools were called) with a
  switch to turn one off for Hub's threads only.

## Machines
<!-- covers: app/src/lib/components/HostChip.svelte, app/src/lib/components/HostPanel.svelte, app/src/lib/components/NewMachineCard.svelte, app/src/lib/components/HomePanel.svelte, app/src/lib/components/HealthCard.svelte, app/src/lib/components/AwayCard.svelte, app/src/lib/away.svelte.ts, app/src/lib/machineUpdate.ts -->
- `HostChip` is where something runs: icon and name, grey when offline, never colour for the kind.
- `HostPanel` lists everything listening on this computer; developer processes first, the rest folded.
  `HomePanel` is Home's right panel and only appears when there is something to tidy.
- `NewMachineCard` offers to set up a Linux machine that joined the tailnet; "Not now" hides it.
- `HealthCard` is the weekly health check as one square per check per project (green, amber, red).
- `away.svelte.ts` notes when Hub was last used; after a long enough gap Home shows `AwayCard`:
  what finished or failed, what landed, what Hub did on its own.

## Writing a message
<!-- covers: app/src/lib/attachments.ts, app/src/lib/composerFiles.ts, app/src/lib/composerConfig.ts, app/src/lib/composerCommands.ts, app/src/lib/components/ConfigControls.svelte, app/src/lib/components/WhoControl.svelte, app/src/lib/components/DesignControls.svelte, app/src/lib/components/UsageControl.svelte, app/src/lib/components/ComposerTabs.svelte, app/src/lib/components/ReachRow.svelte, app/src/lib/components/CompactChip.svelte, app/src/lib/components/kits/, app/src/lib/components/AttachmentRow.svelte, app/src/lib/drafts.svelte.ts, app/src/lib/recent.ts, app/src/lib/typing.svelte.ts, app/src/lib/components/TidyBar.svelte, app/src/lib/components/RuleChip.svelte, app/src/lib/components/Receipt.svelte, app/src/lib/components/FocusNote.svelte, app/src/lib/components/TopicNote.svelte, app/src/lib/components/AskLine.svelte, app/src/lib/voice.ts -->
- `attachments.ts` turns picked, dropped or pasted files into attachments (size cap `MAX_BYTES`; a big
  paste becomes a named file; an `@` thread goes along as a short note read on demand).
- `composerFiles.ts` holds what the message box does with files without touching its state (read files, tell a
  big paste from text, put an attached text back in the box); `AttachmentRow.svelte` is the chips above the
  box and the look at one, with its own CSS. `Composer.svelte` keeps the list and passes it in. Hub's own short notes (a big paste became a file, "Answer with the number keys") are not drawn in the box's bar: in a thread `Composer` hands them to `onnote` and `ThreadView` shows them as the tab above the box, like the other notices; only the new-thread box keeps them in its bar. The rest of the box is split by control:
  `ConfigControls` (effort, mode), `WhoControl` (provider, model), `DesignControls`, `UsageControl`,
  `ComposerTabs` (started and stash tabs), `ReachRow` (extra folders), `CompactChip`; the config reading
  is `composerConfig.ts` and the hub commands are `composerCommands.ts`.
- `drafts.svelte.ts` keeps unsent text and files per thread (the activity row shows a pen). `recent.ts`
  is Arrow Up history for boxes outside a thread.
- `typing.svelte.ts`: a question that appears while you type is held until you stop
  (`typingNow`), so a key meant for the message does not answer it.
- Offers under the box: `TidyBar` (long, dictated text to a clear prompt, original goes to the log),
  `RuleChip` ("always…" to CLAUDE.md), `FocusNote` (off the project's stage: park it) and `TopicNote`
  (a different task: start a new thread). Answers are remembered on this computer.
- `Receipt` is one tick when Hub has the message, two when the agent started on it.
- `AskLine` is a question the agent asked without waiting (`ask_user`), with its answer.
- `voice.ts` records 16 kHz mono WAV for the core's whisper.cpp and plays the start and end chirps.

## Permissions and Customs
<!-- covers: app/src/lib/components/Permission.svelte, app/src/lib/describe.ts, app/src/lib/components/CustomsAsk.svelte, app/src/lib/components/RuleList.svelte -->
- `Permission` is one line in the transcript, and the open one docks in the composer (1, 2, 3 answer).
  Work-destroying actions are red and need a second press. `describe.ts` says in plain words what a
  shell command does; `risk.ts` decides what is dangerous.
- `CustomsAsk` is something the agent declared at Customs and waits for; clearing it tells the agent.
- `RuleList` is a list of patterns as chips with a field to add one.

## Reading a thread
<!-- covers: app/src/lib/session.ts, app/src/lib/find.ts, app/src/lib/components/FindBar.svelte, app/src/lib/figures.ts, app/src/lib/components/StreamingText.svelte, app/src/lib/components/TerminalOutput.svelte, app/src/lib/components/ThreadMeta.svelte, app/src/lib/components/TaskLine.svelte, app/src/lib/components/ReviewCard.svelte, app/src/lib/components/Checkpoint.svelte, app/src/lib/components/AttemptBar.svelte, app/src/lib/refs.ts, app/src/lib/components/Ref.svelte, app/src/lib/viewed.ts, app/src/lib/viewerGaps.svelte.ts, app/src/lib/refs.ts -->
- `session.ts` folds a thread's log into what the composer offers: settings, slash commands, usage.
- Ctrl F: `find.ts` marks matches with the CSS Custom Highlight API (the page is not changed) and the
  transcript opens folded turns that hold one; `FindBar` is the box.
- `figures.ts` loads the math and diagram libraries the first time a reply has a formula or diagram.
  Only `$$…$$`, `\[…\]`, `\(…\)` and fenced `math` blocks count; a lone `$` is money.
- `StreamingText` reveals a reply at an even pace; `TerminalOutput` shows a command's last lines with
  colours; `ThreadMeta` is project, machine, engine and age under a title.
- `TaskLine` is a task Hub ran on the engine picked in Settings > Handoffs; `ReviewCard` is a second
  opinion as one line; `Checkpoint` is the hairline with an undo mark (second click rolls back);
  `AttemptBar` shows attempts side by side and picks one.
- `refs.ts` and `Ref.svelte` are Hub's visual vocabulary: a grey glyph says what a thing is, colour
  only says state; `shortPath` shows the home folder as `~`.
- `viewed.ts` keeps which files of a pull request you ticked as read, with a fingerprint so a file
  pushed to after comes back as changed. `viewerGaps.svelte.ts` tells Problems once per kind and day
  about a file with no viewer (only extension, name and thread go along).

## Providers, engines, handoffs
<!-- covers: app/src/lib/providers.svelte.ts, app/src/lib/components/ProviderList.svelte, app/src/lib/components/ProviderMark.svelte, app/src/lib/cardSwitch.ts, app/src/lib/handoffs.svelte.ts, app/src/lib/components/Handoffs.svelte, app/src/lib/components/HandoffOfferCard.svelte, app/src/lib/ownTools.svelte.ts, app/src/lib/components/WorkersView.svelte -->
- `providers.svelte.ts` says who is behind an engine and model (read from the model name first, then
  the engine); `ProviderMark` draws the mark. Brand colour is the one exception to status-only colour.
  `ProviderList` is Settings > Engines > Providers.
- `cardSwitch.ts` holds the styles for how the limits card changes between subscriptions and the
  backdrops per company.
- `handoffs.svelte.ts` and `Handoffs.svelte` are Settings > Handoffs (`handoff.rs`); a hubd without
  `handoff.tasks` shows "needs the next version". `HandoffOfferCard` is the agent's offer to hand work
  to one of your tools; pressing it is the approval. `ownTools.svelte.ts` keeps which tools take handoffs.
- `WorkersView` is everything running in the background: engines, leftover processes, containers.

## Projects and files
<!-- covers: app/src/lib/components/ProjectLibrary.svelte, app/src/lib/components/ProjectApis.svelte, app/src/lib/components/ProjectRecipes.svelte, app/src/lib/components/RecipesView.svelte, app/src/lib/components/RoutinesView.svelte, app/src/lib/components/DeskPanel.svelte, app/src/lib/components/FileTree.svelte, app/src/lib/components/BranchChip.svelte, app/src/lib/components/IssueView.svelte, app/src/lib/github.ts, app/src/lib/components/DocSwitcher.svelte, app/src/lib/docs.ts, app/src/lib/code.ts, app/src/lib/components/CodeView.svelte, app/src/lib/components/NamedFile.svelte, app/src/lib/components/ShareSheet.svelte -->
- Project page tabs: `ProjectLibrary` (what threads made, from `outputs.rs`), `ProjectApis` (which APIs'
  keys its threads get; adding happens in Settings), `ProjectRecipes` (saved jobs, one click to run;
  writing happens in `RecipesView`). `RoutinesView` is the schedule list.
- `DeskPanel` is the desk's right panel: the project's stage and its threads by need; hover peeks.
- `FileTree` loads one folder level when opened, on the machine the folder is on. `BranchChip` switches
  or creates a branch (git refuses if changes would be lost). `IssueView` is one issue with its comments.
- `github.ts` is pure: groups pulls, splits `gh pr diff` per file, picks the next Git step. No requests.
- Docs: `DocSwitcher` finds by name, heading or text (`#tag` filters, empty lists the latest);
  `code.ts` colours code with Hub's own `.hljs-*` tokens, `CodeView` shows it with line numbers.
- `ShareSheet` is where a picture can go: Hub's places first, the system share sheet as one icon.

## Settings and small controls
<!-- covers: app/src/lib/components/Settings.svelte, app/src/lib/components/settings/, app/src/lib/components/AgentKeys.svelte, app/src/lib/components/Field.svelte, app/src/lib/components/Switch.svelte, app/src/lib/components/CountUp.svelte, app/src/lib/components/WindowControls.svelte, app/src/lib/components/SimInbox.svelte -->
- `Settings.svelte` is the shell: the window, the row list with its tabs, Find a setting (`index` lists
  every setting by name) and the page that `hub.settingsSection` names. Each page is a component in
  `components/settings/` (`Account`, `Appearance`, `Chat`, `Dictation`, `Engines`, `Safety`, `Projects`,
  `Machines`, `Devices`, `Updates`, `WhatsNew`, `Privacy`, `AboutHub`); the small ones (Where it goes,
  Notifications, Handoffs, Personality, Customs, APIs, Keys, Extensions) are one line in the shell that
  mounts a component of their own. A page loads what it shows when it opens and keeps its own state; what
  more than one page needs (the core's settings, `edit`, `reloadCore`, `restart`, `openFolder`) lives in
  `settings/core.svelte.ts`. A new page is a file in `settings/`, a row in the list at the top of the
  shell and a line in `index`. The look of rows, headings and lists is `kits/settings.css`: a page puts a
  comment `/* @kit settings */` in its `<style>` and `svelte.config.js` pastes the file there, so each page gets the
  rules scoped to itself and they cannot leak into the components inside it (the unused-selector warning
  is off for these pages, since a page keeps only what it uses). The same works for any file in
  `components/kits/` (`menus.css`: the composer's pills, menus and option rows; `chips.css`: attachment and
  reach chips; `transcript.css`: the chat's rows; `projectView.css`: `ProjectView` and `ProjectRunning`). A kit is
  only for CSS that several components share; a component's own look stays in its own `<style>` (a kit with one user
  hides its unused selectors from svelte-check).
- `AgentKeys` (Settings > Access): keys for agents outside Hub, shown once with the command that adds it.
- `Field` is a text setting saved with Enter, the Save button or by leaving; `Switch` is the on/off
  control; `CountUp` counts a percentage to its new value; `WindowControls` are Hub's own minimise,
  maximise and close since the window has no frame; `SimInbox` is fake threads on the `sim` engine.

## Feel
<!-- covers: app/src/lib/pop.ts, app/src/lib/popover.ts, app/src/lib/portal.ts, app/src/lib/tooltip.ts, app/src/lib/sound.ts, app/src/lib/themes.ts, app/src/lib/closed.svelte.ts, app/src/lib/terminal-links.ts, scripts/garden.py, docs/GARDEN.md -->
- `popover.ts` (`pop`) is the one entrance and exit for floating layers, growing from where the user
  pressed; `pop.ts` has the transitions for modals, lightbox, find bar, bottom panel and toast.
  Timings live in `design/ANIMATION.md`.
- `portal.ts` moves an element to the end of the page so no clipping parent hides it.
- `tooltip.ts`: anything with `data-tip` (or a plain `title`, moved on first hover) gets Hub's own tip.
- `sound.ts` makes notification sounds on the spot (no audio files). `themes.ts` sets the theme on
  `<html>`; `app.css` swaps the tokens.
- `closed.svelte.ts`: Ctrl Shift T reopens what was closed in the right panel.
- `terminal-links.ts` finds http(s) links in terminal text, including ones a terminal wrapped.
- `scripts/garden.py` writes `docs/GARDEN.md`: weeds (pub items, exports, components nothing uses, unused crate dependencies, expired `compat-until`, `hubdHas` calls with no `compat-until` marker, tests without assertion; "not run" instead of 0 when `cargo machete` is missing). A hit is a candidate, not a verdict; false positives go in `docs/garden-keep.txt` with a reason.
