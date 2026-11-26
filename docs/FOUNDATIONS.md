# Foundations
<!-- för: agent -->

What tools like Hub are built on underneath, how Hub compares, and what to pour before the
features pile up. Written 2026-10-30 from a read of the code at `a31cfea`; the status below says
what has been built since.

## Status (2026-10-30)

| | Built | Borrowed from |
|---|---|---|
| 1 Safe updates | Tests run on the PC before a build (GitHub Actions minutes are too few for CI there); database copied before a new version; the new core takes over only once it is up, otherwise the old one rolls back and remembers the commit | the takeover pattern of Chrome/VS Code updaters |
| 2 Versioned log | `PRAGMA user_version` steps, newer databases refused, unknown kinds kept, `session_started` | `rusqlite_migration` |
| 3 Supervision | Job Object per engine on Windows, process groups on Unix, engine stderr to a file, the app restarts a dead core | `win32job`; the ACP crate's own process groups and stderr capture |
| 4 Token, protocol | Origin for the app, token for everything else, `{ ok, version, protocol, schema }` on `/api/health` | VS Code server and Jupyter connection tokens |
| 5 Engine door | `engine::run`/`complete` dispatch on a `driver`, the contract in `engine/mod.rs` | ACP as the internal language |
| 6 Sync | `client_id` on new threads and messages | |
| 7 Logs, backups | Daily log files kept two weeks, daily database copies kept a week | `tracing-appender`, SQLite `VACUUM INTO` |
| 8 Secrets | Windows Credential Manager, handed to engines by name, redacted from the log | `keyring` |

Still open: a global cursor for sync (goes with remote hosts), policy in the core and sandboxing,
the runner process, contract tests against the real adapter.

Worth borrowing next rather than writing:

- **Litestream** to stream `hub.db` continuously to Server, so a dead disk loses seconds, not a
  day.
- **Tailscale `tsnet` / WhoIs** for identity between hosts, on top of the token.
- **OpenTelemetry** (`tracing-opentelemetry`) once more than one host logs, to follow one thread
  across machines.
- **Codex's sandbox approach** (Landlock and seccomp on Linux, Seatbelt on macOS) and
  **bubblewrap** for unattended engines; on Windows, AppContainer.
- **Recorded sessions** from `claude-agent-acp` replayed by `mock-agent` as contract tests.

## What comparable tools rest on

| Tool | What its core is |
|---|---|
| Zed agent panel | The editor is an ACP client; agents are child processes speaking JSON-RPC over stdio. Same model as Hub. |
| Codex (app-server) | One long-lived server owns threads and turns; every surface (CLI, IDE, desktop) is a client of a versioned JSON-RPC protocol with generated schemas. |
| Claude Code | A single agent loop with a typed permission layer (allow/ask/deny rules per tool and path), hooks, an OS sandbox for shell commands, and sessions as append-only JSONL. |
| OpenHands | An event stream (actions and observations) is the only source of truth; the agent never touches the host directly but a sandboxed runtime behind an API. |
| Conductor and similar | One git worktree per agent, so parallel agents never share a working copy. |
| VS Code remote server | A small server per host, reached through a tunnel, authenticated with a connection token; the UI is the same wherever the server runs. |
| Tailscale | Machine identity from the tailnet (WhoIs), not passwords; a tiny agent per host that is supervised by the OS. |
| Chrome, VS Code, Tailscale updates | Staged versions side by side, a health check after switching, and automatic rollback when the new version does not come up. |

The common pattern: **a durable log as the truth, a versioned protocol between every pair of
processes, a supervisor that keeps processes alive and contained, identity on every connection,
and updates that can undo themselves.** Hub already has the first and part of the second.

## Where Hub stands

Solid already: the core is its own process; engines are ACP children; one append-only log in
SQLite WAL; types generated from Rust; WebSocket with catch-up by `seq`; side-by-side versions
with a launcher that is never locked; localhost only with an Origin check.

The gaps, roughly from most to least dangerous:

### 1. Updates cannot undo themselves

Every push to `main` ships to the PC within minutes. `hubd` builds the new version, starts it and
exits (`crates/hubd/src/main.rs`). If the new `hubd` panics at start, fails a migration, or cannot
bind, nothing notices: the old process is gone, `current` already points at the broken build, and
nothing restarts anything. There is also no CI, so nothing tests a commit before it ships, and a
new version migrates the database before anyone knows it works.

What the others do: stage, switch, health check, roll back.

- The new `hubd` must report healthy (store opened, migrations done, port bound, `/api/health`
  answers) within N seconds, or the old one keeps running and marks the commit as failed. Only
  then does `current` move.
- The launcher (or a scheduled task) keeps the previous version and falls back to it if
  `current` crashes twice in a row at start.
- A copy of `hub.db` is taken before the first start of a new version (see 9).
- CI on GitHub Actions, on Windows and Linux: `cargo test --workspace`, `npm run check`, and a
  release build. The updater only takes commits whose checks passed (`gh api` or the commit
  status), so a red commit never reaches the PC.

### 2. The log has no versions

The log is the most valuable thing Hub owns, and its format is not versioned.

- Migrations are "add the column if it is missing" (`store.rs`, `migrate`). Use
  `PRAGMA user_version` and a numbered list of migrations, each run once in a transaction, with a
  test that opens a database from every earlier version.
- A database written by a newer version should be refused by an older one (rollback in 1 makes
  this real). Today an older binary would map every event kind it does not know to `Update`
  (`parse_kind`), which silently misreads the log. Unknown kinds should stay unknown and be
  skipped by surfaces.
- `Update` events are raw ACP `SessionUpdate`s. That was the right call, but the record does not
  say which protocol or schema version produced them. Store the engine kind and ACP protocol
  version with each session start (a `session_started` event), so a future fold can tell old
  shapes from new.
- Streaming chunks are one row each, and `send` reads up to 10 000 events per message to decide
  a title. Logs will grow to hundreds of thousands of rows per thread. Keep the raw log, and add
  projections (a `messages` table or a snapshot per turn) that surfaces and the core read
  instead of folding everything.
- `delete_thread` removes the log. Fine for a single user, but archive should be the normal way
  out, and deletion a separate, deliberate act.

### 3. Processes are not supervised

- Engines: Job Objects on Windows and process groups on Unix, so stopping a thread stops its
  whole tree (already in the roadmap; it belongs to the foundation, not to features).
- Engine stderr is not captured anywhere. When `claude-agent-acp` fails at start, the reason is
  lost. Capture it per thread to a file and put the tail in the `error` event.
- `hubd` itself: if it crashes, the app restarts it only when the window is opened. Run it as a
  per-user scheduled task or service with restart on failure, as Tailscale does.
- Engines live inside the core today, so every update waits for all threads to be idle. The
  roadmap's split is the foundation for "Hub develops Hub": a small **runner** process (or tmux /
  ConPTY session) owns engine processes and their pipes and never restarts; the core reconnects
  to it after an update. Decide the runner's protocol now, even if the first version is in-process.

### 4. One engine interface, several drivers

The engine module is ACP only (`engine/mod.rs` has one `Command` enum and one `acp::run`). Before
`claude -p` and tmux arrive, make the boundary explicit:

- An `Engine` driver trait: start or resume, prompt, cancel, set config, answer permission,
  stop; and a stream of updates out.
- ACP's `SessionUpdate` as Hub's internal language. A `claude -p --output-format stream-json`
  driver translates into it, so the log, the fold and the UI stay one shape.
- A driver says what it can do (resume, config options, permissions, attach), and the UI asks
  the capability, not the engine's name. This keeps engine-specific code out of the core, as
  `CLAUDE.md` requires.
- Parallel agents in one repository need one git worktree each (Conductor's lesson). Make
  "where a thread runs" a `place = host + folder (+ worktree)`, not just `cwd`.

### 5. Hosts and identity

- Any local process can call the API today, including `stop process` and starting engines that
  run commands. Browsers are blocked by Origin, other programs are not. Add a connection token
  now (written to the data folder, sent by the app as a header and on the WebSocket), as VS Code
  server and Jupyter do. It costs little and every later surface needs it anyway.
- Remote hosts: the same API on each host (a `hub-agent` built from the same crates), reached
  over Tailscale, with the tailnet identity checked on every request (`tsnet` or the local API's
  WhoIs) plus the token. One protocol, not one per host type.
- Version the protocol: `/api/v1`, a `protocol` number in `/api/health`, and a rule that a
  surface and a core with different major versions say so instead of half working. The phone and
  remote hosts will be on different versions than the PC.
- Thread ids are already UUID v7, which is right for merging logs from several hosts. Add a
  `host` to the thread now, even if it is always the local one.

### 6. State sync

- `seq` is per thread, so a client that reconnects must refetch everything (`resync`). Add a
  global, monotonic `cursor` (an autoincrement over all events and thread changes) so any
  surface, including a phone or a remote core, resumes with one number.
- Writes have no idempotency key. A phone on a bad connection that retries will send a message
  twice. Accept a `client_id` on every POST that creates something, and ignore repeats.
- Permission questions live only in memory and are lost on restart; the thread falls back to
  `idle` with a question in the log that can no longer be answered. Log their expiry as an event
  so the UI shows them as closed.

### 7. Permissions and sandboxing

Today the engine's own permission mode is the only policy. As soon as several engines and hosts
exist, policy has to live in the core:

- Rules per project (allow, ask, deny per tool kind and path), applied to every engine's
  permission requests before the user sees them, and logged (`permission_resolved` with
  `by: rule`).
- A destructive-command check in the core, not only in the app (`risk.ts`), so a phone surface
  gets it too.
- Sandboxing later: worktrees first, then OS sandboxes (Windows AppContainer, Linux bubblewrap or
  containers) for engines that run unattended (night shift).

### 8. Secrets

- Never in `hub.json`, the log, or `hubd.log`. Store them in the OS credential store (Windows
  Credential Manager through the `keyring` crate) and hand them to engines as environment
  variables at start, per engine, by name in the config.
- Redact known secret values from events before they are stored; agents echo environment
  variables more often than one would think.

### 9. Logs, diagnostics and backup

- `hubd.log` grows forever. Use `tracing-appender` with daily rotation and keep 14 days; log as
  JSON with `thread_id` and `host` as fields so one thread can be followed across processes.
- A "diagnostics" button that zips the recent logs, versions and config (secrets removed).
- Backups: SQLite's online backup (`VACUUM INTO`) daily and before every new version starts, into
  `<data_dir>/backups`, keeping the last 7 daily and 3 pre-update copies. Later, copy them to
  Server.

### 10. Extensions

Do not invent a plugin API. The two open protocols already cover it:

- **ACP** for anything that does work in a thread (engines, Knegarn).
- **MCP** for anything that gives tools to engines (Hub's tools, Arsenal modules, Forman).
- Hub's own surfaces use the HTTP/WS API. A module that needs to show something in the UI
  registers a view described as data, later.

### 11. Testing

- CI (see 1). Run the end-to-end engine tests on Windows too; Windows is where it runs.
- Contract tests against the real adapter: record a session with `claude-agent-acp` once and
  replay it in tests, so a breaking change in the adapter shows up as a failed test, not as a
  broken PC.
- Migration tests from a database of every released schema version (see 2).
- An update test: install version A, update to B, check that threads and the log survived and
  that a broken B rolls back.

## Proposed order

1. **Safe updates**: CI, health check and rollback, backup before a new version.
2. **Versioned log**: `user_version` migrations, unknown kinds kept, `session_started` with
   protocol versions.
3. **Process supervision**: Job Objects, engine stderr, `hubd` restarted on crash.
4. **Connection token and `/api/v1`** with a protocol number.
5. **Engine driver trait**, with `claude -p` as its second driver; `place` with host and worktree.
6. **Global cursor and idempotent writes.**
7. **Log rotation and daily backups.**
8. **Policy in the core**, then **secrets in the credential store**.
9. **Runner process** so updates no longer wait for idle threads.

1 and 2 protect against losing work. 3 to 5 decide shapes that are expensive to change once
remote hosts and the phone exist. The rest can follow the features that need them.
