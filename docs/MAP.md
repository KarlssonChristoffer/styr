<!-- för: agent -->
# Map

Read this first. Where does X live, which doc section describes it, which test guards it.
`ARCHITECTURE.md` is an index; the sections live in `docs/architecture/*.md` (each under 800 lines). Read only the section (marked §) you are pointed to (or ask `docs_for`).
Short paths resolve under `crates/hub-core/src/` (R), `app/src/lib/` (L) or `app/src/lib/components/` (C);
tests are `crates/hub-core/tests/it/<module>.rs`, run with `cargo test -p hub-core --test it <module>::`.
`cargo test -p hub-core --test it map::` keeps this file honest (paths exist, under 150 lines).
Edit it in the PR that moves the code. Not here: how it works (the doc section), why (git log).

## Layout
| Part | Where |
|---|---|
| Core library | `crates/hub-core/` (R: one file per feature, `x.rs` or `x/`) |
| Daemon, subcommands | `crates/hubd/src/main.rs` (serves the API, `engine`, `pair`, `sign-in`, `attach`) |
| Installed layout, launcher | `crates/hub-install/`, `crates/hub-launcher/` |
| Scripted ACP agent for tests | `crates/mock-agent/` |
| Svelte app, Tauri shell | `app/src/` (shell `app/src/App.svelte`, mount `app/src/main.ts`), `app/src-tauri/`; files map `docs/APP.md` |
| iPhone app | `ios/Hub/` (Swift; `ios/Hub/HubClient.swift` talks to the API, `ios/Hub/Model.swift` mirrors the types, `ios/Hub/EventPayloads.swift` names the fields of the events it reads) |
| Phone web page | `phone.rs`, `phone/index.html`, `api/phone.rs` |
| Design | `design/DESIGN.md`, tokens in `app/src/app.css` |

## Types, storage, API
| Thing | Where | Doc | Guard |
|---|---|---|---|
| API types (`Thread`, `EventKind`, `Project`) | `model.rs`; TS bindings `L/bindings/` regenerate on `cargo test -p hub-core` | `§Pieces` | `bindings` diff |
| Event payload structs (one per kind, registry `event_kinds!`) | `model/event_data.rs`, `model/event_data/`, `L/events.ts` | `§Thread status from events` | `golden_log::`, `golden_reads::` + `frozen/golden_events.jsonl` |
| Settings, engine presets, `EngineTraits` | `config.rs` (`EngineTraits::preset`), `engines.rs`, `prefs.rs`, `engine_home.rs` (where each engine keeps its login and who is signed in), `home_files.rs` (home folder, reading its JSON files) | `§The settings file` | `engine::` |
| SQLite store, append-only event log | `store.rs` (`append_event`, `append_event_once`) | `§Thread lifecycle` | `store.rs` unit tests |
| HTTP + WebSocket routes | `api.rs` (`router`), one file per area in `api/` (threads, projects, engines, keys, machines, github, phone, usage, bench) | `§API` | `api_routes.rs` + `frozen/routes.txt` |
| Add a route | the area file in `api/`, then its path in `frozen/routes.txt` | `§API` | `api_routes.rs` |
| Module layers (what may call what) | `LAYERS` in `layers.rs` test | `§Pieces` | `layers.rs` + `frozen/layers.txt`, `frozen/ring.txt` (the ring only shrinks) |
| Versions talking to each other | `compat.rs` (`FEATURES`), `L/compat.ts` (`hubdHas`), `phone.rs` | `§Versions that talk to each other` | `compat.rs`, `ios_contract.rs` (iPhone calls), `ios/tests/ContractTests.swift` (Mac), `crates/hub-core/tests/compat/frozen/features.txt` |

## Threads and engines
| Thing | Where | Doc | Guard |
|---|---|---|---|
| `Hub` struct, one `impl` per feature | `hub.rs`, `hub/` | `§Pieces` | `hub_items.rs` + `frozen/hub_items.txt` |
| Create / send / queue a message | `hub/lifecycle.rs` (`create_thread`), `hub/send.rs` (`send`, `send_once`) | `§Thread lifecycle` | `engine/`, `sim.rs` |
| Emit an event | `hub.rs` (`emit`/`emit_once` redact, log, publish, then run the list in `hub/observers.rs`: `shadow`, first-token clock, after-turn queue; a new "when X is logged" feature adds an observer there) | `§Thread lifecycle` | `status.rs` |
| Permission questions | `hub/permissions.rs`; auto-answers `permit.rs` (`decide`) | `§Where it goes` | `engine/` |
| After a turn: edits, leaks, proof, docs (Problems hooks in with `Hub::on_look`) | `hub/after_turn.rs`, `proof.rs`, `leak.rs`, `clash.rs`, `covers.rs` | `§Proof before done, leak guard, clash warnings` | unit tests in those files |
| Engine drivers (ACP over stdio) | `engine/mod.rs` (`run`, `complete`), `engine/acp.rs` | `§Thread lifecycle` | `engine/` + `mock-agent` |
| Engine-specific knowledge | ONLY the engine files `ENGINE_FILES` lists in `crates/hub-core/tests/it/rules.rs` (e.g. `config.rs`, `engines.rs`, `activity/claude_code.rs`, `connections.rs`, `engine_home.rs`) | `§Pieces` | `live_output.rs` |
| Harness on a provider, per thread | `routes.rs`, `engines.rs`, `api/engines.rs` | `§What the models can do` | `engine/` |
| Thread status from events | `status.rs`, `status_shadow.rs` | `§Thread status from events` | `status.rs` |
| Project folder and display word of a thread (`Thread.project`, `Thread.display`) | `thread_display.rs` (`project_root`, `display_status`; `store.rs` fills them) | `§What a thread shows: project and display word` | unit tests in `thread_display.rs`, `compat::` |
| Undo, checkpoints, second opinion | `checkpoint.rs`, `hub/reports.rs` | `§Undo` | `engine/` |
| Hand-off / take over a thread | `handoff.rs`, `carry.rs`, `terminal.rs` | `§Handoffs` | `handoff.rs` |
| Attempts, side jobs, children | `attempts.rs`, `children.rs` | `§Attempts` | `engine/` |
| Cloud threads | `cloud_threads.rs`, `cloud_usage.rs` | `§Cloud threads` | `cloud.rs` |
| Bench: suite, checks, measurements, runner, results, API and tools | `crates/hub-core/src/bench.rs`, `crates/hub-core/src/bench/`, `crates/hub-core/bench/smoke.toml`, `crates/hub-core/bench/features.toml`, `crates/hub-core/src/permit/bench.rs`, `crates/hub-core/src/api/bench.rs`, `crates/hub-core/src/hub_tools/bench.rs`; the Labs card `app/src/lib/components/BenchCard.svelte`, `app/src/lib/bench.ts` | `§Bench` | `crates/hub-core/tests/it/bench.rs`, `crates/hub-core/tests/it/bench_run.rs`, `crates/hub-core/tests/it/bench_api.rs`, `app/src/lib/bench.test.ts` |

## Tools for agents
| Thing | Where | Doc | Guard |
|---|---|---|---|
| Hub's tools (one registry) | `hub_tools/tool.rs` (one entry per tool), `hub_tools.rs`, `mcp.rs` (`/mcp/<thread id>`, the protocol), `mcp/` (a thread's own tools by group, each with declarations and bodies: `mcp/talk.rs`, `mcp/pages.rs`, `mcp/side.rs`, `mcp/docs.rs`, `mcp/guard.rs`), `tool_instructions.rs` (what an agent is told as it connects) | `§Hub's tools` | `hub_tools.rs`, `tools.rs` |
| Docs for the code you change | `covers.rs`, `vault.rs`; tool `docs_for` | `§Key vault` | `covers` unit test |
| Hub's own tools list for the user | `own_tools.rs` | `§Hub's tools` | `tools.rs` |

## Projects, machines, updates
| Thing | Where | Doc | Guard |
|---|---|---|---|
| Rules in force (rules files, dated by git blame) | `standing.rs`, `git.rs` (`blame`), `api/projects.rs` (`/api/project/rules`) | `§Rules in force` | `standing.rs` unit tests |
| Project page, focus, library | `project.rs`, `project_folder.rs` (plan path, `inside`, stack: low, outside the ring), `focus.rs`, `library.rs`, `project_parts.rs`, `api/projects.rs` | `§Projects` | `project_parts.rs` |
| Other machines, tailnet, SSH | `remote.rs`, `machine.rs`, `host.rs`, `lan.rs`, `tailnet.rs`, `api/machines.rs` | `§Other machines` | `hosts.rs` |
| Self-update, versions of agent CLIs | `update.rs`, `updates.rs`, `update_track.rs`, `L/components/UpdateCenter.svelte` | `§Self-update` | `compat.rs` |
| Clean-up of worktrees, temp folders | `janitor.rs`, `tempdirs.rs` (tests use `janitor::test_dir()`) | `§Clean-up after agent work` | unit tests |
| Windows without a console window | `proc.rs` | `§Starting programs without a console window` | `no_window.rs` |
| Git, GitHub, PR watch | `git.rs`, `gitops.rs`, `github.rs`, `prwatch.rs`, `api/github.rs` | `§GitHub` | unit tests |
| Secrets, key vault, API connections | `secrets.rs`, `apis.rs`, `apis/vault.rs`, `engine_renew.rs`, `api/keys.rs`, `L/components/KeyVault.svelte` | `§API connections` | `customs.rs` |
| Customs (allow / deny review) | `customs.rs`, `customs_*.rs` | `§Customs` | `customs.rs` |
| Problems, Vakten | `problems/`, `L/problems.ts` | `§Problems` | `problems.rs`, `vakten_guard.rs` |
| Usage, spend, plan limits | `usage.rs`, `spend.rs`, `stats.rs`, `api/usage.rs` | `§Quota guard` | unit tests |
| Routines, recipes | `routines.rs`, `routine_runs.rs`, `recipes.rs` | `§Routines` | unit tests |
| Notices, ntfy, Discord | `notice.rs`, `ntfy.rs`, `gateway.rs` | `§Smaller pieces` | `notices.rs` |
| Phone pairing and its narrow API | `phone.rs`, `pairing.rs`, `api/phone.rs` | `§The phone` | `compat.rs` |
| Security, tokens, auth | `auth.rs`, `guard.rs`, `identity.rs`, `login.rs` | `§Security` | unit tests |

## The app (Svelte 5)
| Thing | Where | Doc | Guard |
|---|---|---|---|
| Shell, rooms, routing | `app/src/App.svelte`, `C/Rail.svelte`, `C/Sidebar.svelte` | `§Rail and rooms` | `npm run check` |
| Core calls, WebSocket | `L/api.ts`, `L/stream.svelte.ts`, `L/poll.ts` | `§Talking to the core` | `npm run check` |
| App-wide state | `L/state.svelte.ts`, `L/prefs.svelte.ts` | `§Talking to the core` | `npm run check` |
| Thread view, transcript, composer | `C/ThreadView.svelte`, `C/Transcript.svelte`, `C/Composer.svelte` | `§Reading a thread` | `npm run check`, `npm test` (`L/transcript.test.ts`) |
| Settings | `C/Settings.svelte` and its sub-components | `§Settings and small controls` | `npm run check` |
| Home, overview | `C/Home.svelte`, `C/Overview.svelte` | `§Home and New thread` | `npm run check` |
| Shared controls (never the browser's) | `C/Select.svelte`, `C/Switch.svelte`, `C/Field.svelte` | `§Shared UI pieces` | review |
| Right-click menus | `L/menus.svelte.ts` (add actions there) | `§Shared UI pieces` | `npm run check` |
| Screenshots, layout, design audit | `app/scripts/shots.cjs`, `app/scripts/check-layout.cjs`, `app/scripts/design-audit.cjs`, `app/scripts/test-coding-destination.mjs`, `app/scripts/test-terminal-links.mjs` (run by hand with node) | `§Feel` | report only |
| Code health (size, habits, `--coverage`, `--audit`, `--build-time`, `--pr-labels`, `--table` A-E; always a `structure` block: dependency cycle, ranked modules, `Arc<Hub>`, untyped events) | `scripts/code-health.py` | `§Feel` | report only |
| Idle measurement of hubd (CPU %, RSS, threads, wakeups/s over 60 s; starts its own hubd on a free port, or `--pid` for a running one; Linux only) | `scripts/idle-matning.py` | `§Feel` | report only |
| Weeds (unused items, dependencies, expired `compat-until`) | `scripts/garden.py`, `docs/GARDEN.md`, `docs/garden-keep.txt` | `§Feel` | report only |
| PR pins (tests, weeds; idle/wire/build from JSON) posted as `hub/pins · tincan`, thresholds in the script | `scripts/pins.py`, `prci.rs` | `§Self-update` | `prci.rs` unit test |
| Weekly routine on tincan (garden, pins, mutants, adapter pins vs npm via `scripts/adapters.py`; reports to branch halsorutin; systemd timer) | `scripts/veckorutin.sh`, `scripts/adapters.py`, `scripts/systemd/hub-veckorutin.*` | `§Self-update` | report only |
| Monday pins (idle, wire bytes, build seconds against last week, via `pins.py`) and weekly mutation tests (one file, survivors listed) | `scripts/monday.py`, `scripts/mutanter.py`, `crates/hub-core/tests/it/wire.rs` | `§Self-update` | `cargo test -p hub-core --test it wire::` |
| Clippy ratchet (warnings per lint and file; the total per lint may not grow) | `crates/hub-core/tests/it/clippy.rs`, `frozen/clippy.txt` | `§API` | `HUB_CLIPPY=1 cargo test -p hub-core --test it clippy::` |
| Rule ratchets (CLAUDE.md rules a grep checks; lines per file, the total per rule may not grow) | `crates/hub-core/tests/it/rules.rs`, `crates/hub-core/tests/it/ratchet.rs` (shared check), `crates/hub-core/tests/it/test_code.rs` (which lines are test code), `frozen/rules.txt` | `§API` | `rules::`, `ratchet::`, `test_code::` |
| Which checkout a test reads (at run time, not `env!`; a shared `CARGO_TARGET_DIR` reuses test binaries across worktrees) | `crates/hub-core/src/source_tree.rs` | `§API` | `source_tree` unit test |

## Docs and rules
| Thing | Where |
|---|---|
| Rules for agents | `CLAUDE.md`; `AGENTS.md` is a copy (`cp CLAUDE.md AGENTS.md`, guard `agents_mirror.rs`) |
| Direction, stages, "To test" | `docs/FOCUS.md` (for Christoffer; agents tick off "Delivered when") |
| Weeds to delete (generated, for agents) | `docs/GARDEN.md` |
| Roadmap, foundations | `docs/ROADMAP.md`, `docs/FOUNDATIONS.md` |
| Who each doc is for | first lines: `<!-- för: agent -->` or `<!-- för: Christoffer -->`; guard `map.rs` |
