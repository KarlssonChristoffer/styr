# Roadmap
<!-- för: Christoffer -->

MVP (this repo now): threads on this PC, Claude Code via ACP, event log, ports and processes.

Ideas to keep (from Christoffer, 2026-10-29):

- **Every engine session in tmux, so you can take over.** Built as a handover (see
  ARCHITECTURE.md, "Taking a thread over"): `hubd attach` follows a thread and on Enter runs the
  engine's own CLI on its session; on Linux every thread with an engine has a tmux session
  `hub-<id>`, on Windows "Take over" opens Windows Terminal. Still open: watching the live TUI
  while Hub drives (needs a terminal engine driver instead of ACP).
- **Start and stop everything per project.** Built: `Procfile.dev` (or a guess from the
  manifests) plus docker compose, Start all / Stop all on the project page. Still open: start a
  project's services when a thread there needs them, and health checks per service.
- **A second Claude engine without ACP:** run `claude -p --output-format stream-json` directly
  and translate its stream into the event log. Using a subscription through the Agent SDK or
  ACP is a grey zone; the plain CLI is the documented path. Fits the engine abstraction as one
  more engine kind next to `acp`.

- **Seamless updates while developing Hub in Hub.** Today an update reloads the window and
  waits for idle threads. Next: the core serves the UI, so a UI update is a page reload in the
  same window and only a change to the app shell restarts the app. Then engines move out of the
  core into a process that never restarts (or tmux, see above), so the core can be swapped in
  under a second even while threads are working.
  Dev mode (Labs > Work on Hub) is built: a clone with a live UI in its own window, a dev core on
  port 7879 with a copy of the database that is rebuilt on save, and Ship, which opens a pull
  request. The preview makes the clone follow the open pull requests merged on top of main (or
  one of them), refreshed within 30 s of a push, so threads working elsewhere show in the dev
  window long before a release. Step 2 is built too: a git worktree per thread in Hub (dev),
  sharing the clone's packages and build cache, the dev window and dev core can show any one
  thread's worktree, the dev core comes back after a restart, and the build cache has a cap.
  Next there: the shell (`app/src-tauri`) in dev mode.

Next, in order:

1. **Hub's tools.** Built (see ARCHITECTURE.md): Hub's threads as MCP tools for every engine and,
   with an agent key, for agents outside Hub. Next: auto-allow its read-only tools so they never
   ask, reach threads on other machines, a budgeted context handoff on `read_thread` instead of
   the model-written one, and delegated tasks whose answer comes back to the caller by itself.
2. **Engine picker** in the composer, with a clear mark for subscription vs API. (Model, effort
   and permission mode are already picked per thread from what the engine offers.)
3. **Windows Job Objects** so an engine's whole process tree stops with it. Built
   (ARCHITECTURE.md, "Thread lifecycle", step 6): on Windows `hubd engine <command>` runs the
   engine in a Job Object, on Unix in its own process group.
4. **Remote hosts.** Built: the same `hubd` on each machine, reached over Tailscale serve or an
   SSH tunnel as fallback, with pairing codes and a token per device (ARCHITECTURE.md, "Other
   machines"). Next, as in T3 Code: the app connects to every machine directly, so the phone
   reaches tincan while the PC sleeps; narrower grants per device; picking the machine with the
   most free CPU and memory for a new thread.
5. **Workers.** The view is built (engines, their processes, containers, Hub's helpers). Your own
   tools with a `hub.toml` (Forman, Knegarn, Aftermath) show there too, with their questions and
   commands. Next: "Send to Knegarn" from a thread, and what waits on you from them on Home.
6. **Keys.** Inventory of API and SSH keys (where, what uses them, last checked); secrets stay in
   a vault.
7. **Arsenal.** Your own tools register themselves as modules the hub can call.
8. **New project and approved tech.** Templates that create the repo, and a vetted, scanned list
   of technologies the agents may use.
9. **AI proxy** for API models (LiteLLM), and a phone surface.

Also on the list (brainstorm 2026-10-29, all agreed, not yet ordered):

- **First real run** against `claude-agent-acp` on the PC. Done on Linux in the cloud
  (2026-10-30, see Known limits in ARCHITECTURE.md); still to do on Windows, and to record the
  session as a contract test for `mock-agent`.
- **Strays by process, not folder.** Home finds stray dev servers by the engine's process tree,
  not only by working directory.
- **Project page extras:** artifacts in the side panel, token and cost stats per model, projects
  made of several repos or a whole host.
- **Key vault in Hub:** create, rotate and delete keys and see what each one is used by (Windows
  Credential Manager, never plaintext in SQLite).
- **AI DevOps (parked, do not build yet).** A shared pipeline base, customised per project by
  tech stack, each project builds its own tests, pipelines run from the project page.
- **More surfaces:** voice and Discord next to the phone.
- **Which machine a thread runs on.** Every thread shows a host chip (icon + name, e.g.
  "tincan · ~/srv/game") in the list and header; icon and shape, not colour, since colour is
  for status; grey when offline. The composer gets "Run on: PC / tincan / Cloud", defaulted by
  the project. Cloud is just another entry in that list and shows a repo instead of a folder.
  The sidebar can filter by host. Where you are connected from is only a quiet footer note.

Favourites from the same brainstorm (Christoffer wants all of them; the first three most):

- **Undo per thread.** Hub takes a git checkpoint before the agent starts work, so everything the
  thread changed can be rolled back with one click. Built (ARCHITECTURE.md, "Undo"): a
  checkpoint per message, rollback and "Edit from here".
- **Second opinion.** Another model reviews a thread's diff before you merge. Built
  (ARCHITECTURE.md, "Second opinion"): `POST /api/threads/{id}/review` and the agents'
  `second_opinion` tool.
- **Quota guard.** Hub sees when a subscription is close to its limit, warns, or switches model.
  A thread stopped by a limit carries on by itself when it resets (built; from T3 Code). That is
  the first step towards Night shift.
- **While you were away** (built, card on Home). On open: what finished, what waits for you, what broke.
- **Answer from the phone.** One queue of questions and permission cards across threads, with
  push and one-tap answers. First version built (ARCHITECTURE.md, "The phone"): a page on the
  tailnet, paired by a QR code in Settings > Access, that follows threads, writes to them and
  answers their questions. Parked 2026-10-30, not a priority; come back to it for: a first run
  on a real phone over real Tailscale, push notifications, one queue of questions across
  threads, starting new threads, marking threads seen, and the phone reaching other machines
  while the PC sleeps.
- **Night shift.** Queue work in the evening, Plåtnicklas works through it overnight.
- **Recipes.** Saved recurring jobs ("fix CI", "update dependencies") you start on any project.
- **Rubber duck.** A mode where the AI helps you think instead of answering: it asks short
  questions, mirrors back what you said and points out gaps, but never hands over the solution
  unless you ask. Voice first (talk while walking). Three levels: silent (just listens, then
  summarises), asking (Socratic), and a "give me a hint" button that escalates hint, then answer.
  It can read the project and thread for sharper questions. When you say "aha", it writes up
  problem and solution and offers to start a thread with it. Any stuck thread gets a "duck this
  with me" button.
- **Several attempts at once.** Same task in 2–3 worktrees, ideally different models; pick the best.
  Built: "Attempts" on Home, the attempt bar in each thread, Pick this one (see ARCHITECTURE).
- **Clash warning, leak guard, proof before done.** Built (Settings > Subscriptions > Safe threads).
- **Ask the history** (built, search on Overview). Search every thread ever run ("when did we fix that port bug?").
- **Automatic devlog** (built, on Overview; a devblog entry written in the night after each day). Each evening Hub writes what you actually did across projects.
- **Idea inbox** (built: Ctrl I, the bulb by search, the phone page and the park_idea tool). Say an idea any time; Hub files it into the right project's focus, under the stage it fits or Parked.
- **Screenshot to thread.** Paste a picture of a bug; Hub finds the project and starts a thread.
- **Panic button** (built: the pause button at the bottom of the sidebar, on the phone page and in Ctrl K). Pause every agent on every host with one click.
- **Focus mode.** Only one project visible; notifications are held until you are done.
- **Weekly health check** (built, on Overview: git and CI without running anything, plus a
  build-and-test thread per project on demand). An agent checks every project: does it build, are tests green,
  outdated dependencies, dead branches. A short report comes back.
- **Context bag.** Drag files, links and screenshots into a bag that can follow into any thread.
- **Activity map** (on Overview). A year of small dots, lit by how much you worked each day (threads, commits,
  tokens), with streaks. Click a day for an AI summary of what you did. Fed by the automatic
  devlog.
