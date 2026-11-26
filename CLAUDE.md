# Hub

Rust workspace plus a Svelte app. Read `docs/MAP.md` first: it says where each thing lives, which doc
section describes it and which test guards it. Read `docs/ARCHITECTURE.md` (an index) and then only the section the map or
`docs_for` points you to (the sections are in `docs/architecture/*.md`). Every doc says on its first lines who it is for
(`<!-- för: agent -->` or `<!-- för: Christoffer -->`); agents write the agent ones in English, dense.

- `AGENTS.md` is a copy of this file for Codex and the other engines. After editing `CLAUDE.md`, run
  `cp CLAUDE.md AGENTS.md`; `cargo test -p hub-core --test it agents_mirror::` fails when they differ.

- Run `cargo test --workspace` and `cd app && npm run check && npm test` before calling work done. App tests are vitest
  (`app/vitest.config.ts`), named `*.test.ts` beside the module they test; start with pure modules in `app/src/lib/`.
- hub-core's integration tests are ONE test program, `crates/hub-core/tests/it/main.rs`. Add a test
  file there as a module, never as a new `tests/*.rs` (each of those is another link of the whole
  core, which the update build pays on every version). One module: `cargo test -p hub-core --test it engine::`.
- The Rust model in `crates/hub-core/src/model.rs` is the source of truth for API types. After
  changing it, run `cargo test -p hub-core` to regenerate `app/src/lib/bindings` and commit both.
- Engines are ACP agents started by command. Engine knowledge (file formats, command names, env vars)
  lives only in the engine files, and `ENGINE_FILES` in `tests/it/rules.rs` is the list of them
  (for example the engine presets `config.rs`, `engines.rs`, and `activity/claude_code.rs`,
  `connections.rs`, `engine_home.rs`, `spend/engines.rs`); elsewhere ask the engine's traits (for
  example `session_log`) or a neutral function or constant in an engine file. Need it in a new
  place? Add the file to `ENGINE_FILES`.
- The event log is append-only. Add new `EventKind`s rather than changing what old events mean.
- UI tokens live in `app/src/app.css` and follow `design/home-v1.html`. Keep it dark, calm and
  dense; status colours only for state.
- New Rust function code that only needs the database and settings takes `&Ctx` (`hub.ctx()`, `ctx.rs`), not `&Hub`; move an old function over when you touch it. `tests/it/layers.rs` ranks modules and its frozen upward edges only shrink.
- Use Hub's own controls, never the browser's (a dropdown is `Select.svelte`, not `<select>`; see
  "Shared UI pieces" in `docs/ARCHITECTURE.md`).
- `cargo test -p hub-core --test it rules::` guards these rules with frozen lists
  (`tests/it/frozen/rules.txt`; per-file lines show where the debt is, but the ratchet is the total per rule, and per lint
  for clippy, so moving code between files never needs a bless or a hand-edited line; a total that grows fails): engine names outside the files above, browser controls in `.svelte`,
  files without a header (`//!` in Rust, a `<!-- -->` comment in Svelte), and eight habits that have a helper (`.lock().unwrap()` → `locked()`, `catch(() => {})` → `quiet()`, `setInterval` → `poll()`, `store.events(.., 1_000_000)` over the whole log, a new `fn clip`, `git` run outside `git.rs`, a bare `.unwrap()` → `?`, `unwrap_or*` or `.expect("why")`, a bare `let _ =` → `.warn_on_err(what)` on a write or event, or `.ignore("why")` with the reason when the failure is expected). Clean
  up, then `RULES_BLESS=1` on that test rewrites the lines (refused while a total grew); never add a line to it to get green. Clippy has the same kind of list (`frozen/clippy.txt`): before you call Rust work done run `HUB_CLIPPY=1 cargo test -p hub-core --test it clippy::`; a lint whose total grows fails, fix it.
- New feature or behaviour change (not a fix or clean-up): before building, write a short plan in the PR
  description or thread: what it does for Christoffer, which files and `docs/` sections it touches
  (`docs_for`), how old and new versions talk (additive API, `compat::FEATURES`), what the test is
  (written first where it can be), and what "To test" line he gets. A reviewing agent, not the author,
  reads the diff against that plan and the debt rules and sends it back if the plan is missing, the
  diff does more than the plan, or a test or doc is missing. No cap on features; the check is on how
  they are planned and reviewed.
- Debt rules: prefer short, concise files and functions that each do one well-defined thing, so the flow
  is easy to follow. Split along a real seam (a type and its impl, one stage of a flow, one screen
  section), never to a `_helpers`/`_more`/`_2` file or a `_first`/`_rest` function just to make it
  shorter; a long file or function is fine when it reads top to bottom as one thing. There is no line
  limit on files or functions. Before adding a poll, lock or clip, grep for the existing helper (for
  example `app/src/lib/poll.ts`) and reuse it; a second copy is debt. A reviewing agent checks both on
  every diff: is each touched file and function one well-defined thing whose flow can be followed, and
  is no helper copied; it sends the change back if either is broken.
- `docs/FOCUS.md` holds Hub's direction and stages. When you finish work that belongs to the
  active stage, tick off what it delivered under "Delivered when" and add what Christoffer must
  test by hand under "To test" (one line: what to do and what should happen). Ideas outside the
  active stage go under "Parked" instead of being built on the side.
- Docs and code stay in step. Before you change a file, ask `docs_for` (Hub's tools) which doc
  sections describe it, or grep `docs/` for `covers:` with its path; after the change, update what
  those sections say if it is no longer true. A section names its code in a
  `<!-- covers: path, folder/ -->` line under its heading; add one when you write about new code.
  `cargo test -p hub-core covers` fails when a `covers:` points at a file that is gone.
- Leave no trace on disk. Scratch files and folders go in the system temp folder named `hub-…`
  (the clean-up removes those after a day) and you remove them before you finish; never make
  worktrees, clones or folders of your own beside the repo or in the home folder. Tests that
  need a folder beyond a `TempDir` use `hub_core::janitor::test_dir()`. Hub removes worktrees and
  branches once they are pushed (see "Clean-up after agent work" in `docs/ARCHITECTURE.md`).
- Labels: a pull request that fixes a bug gets the label `bug` plus one `area:<x>` from the layout in
  `docs/MAP.md`: `area:core` (`crates/hub-core/`), `area:hubd` (`crates/hubd/`), `area:app` (`app/`),
  `area:ios` (`ios/`), `area:install` (`hub-install`, `hub-launcher`), `area:docs`, `area:tooling`
  (scripts, CI, build). Set them when you open the PR (`.github/pull_request_template.md` reminds you);
  a feature or clean-up PR gets no `bug`. This is how bug counts per area are measured later.
- Weeds: `docs/GARDEN.md` lists what looks unused (generated by `scripts/garden.py`). If a file you change is listed, delete what is listed, or add it to `docs/garden-keep.txt` with the reason. When you remove or switch off a feature, remove its leftovers too (guards, flags, stubs), or mark a stub for old clients `compat-until: YYYY-MM-DD`.
- Root cause: when weeds turn up, find out why an agent added them (the thread, `git blame`) and fix the rule or doc that was missing, in the same PR as the deletion.
- The user prefers fluid, low-ceremony tools and does not use Trello.
- Support window: a client or machine is supported 30 days behind. A fallback for an old client or
  hubd (the app counting itself what the server now counts, `hubdHas` branches, `serde(alias)` and old
  wire shapes) is marked in the code with `compat-until: YYYY-MM-DD`, the date of the change plus 30 days,
  and the next change that touches the file after that date removes it. The API itself is still never
  removed without a deprecation first ("Versions that talk to each other" in `docs/ARCHITECTURE.md`).
- Versions of Hub talk to each other (phone, other machines, windows on another Hub), so API
  changes are additive: optional fields, new calls registered in `compat::FEATURES`, nothing
  removed or retyped, and a client reading an unknown value keeps working. Read "Versions that talk
  to each other" in `docs/ARCHITECTURE.md`; `cargo test -p hub-core --test it compat::` guards it.
- Merging: the merge queue is OFF. An agent, whether a Claude thread or one of Hub's own, opens the
  pull request, has it reviewed (a review can be its own agent), fixes what the review finds, then
  merges it itself once the checks are green, waits for any Hub build already running, and lets Hub
  build the update from the merge. "Green" includes tincan: Hub's own PR check posts the commit status
  `hub/ci · tincan` on the head of every open PR (`prci.rs`; `gh api repos/{owner}/{repo}/commits/<head>/statuses`).
  `success` on the head: merge. `failure`: read the tail, fix, push. Tincan is memory-starved at times
  and often posts no status or `error: Not tested` (10 nov: 0 of 25 merges had a green one; the Monday row
  counts `merge_no_green_pct`). So a missing verdict is the normal case, not a reason to wait: run the full
  tests yourself (`cargo test --workspace`, `cd app && npm run check && npm test`; for a change with no Rust
  or app code, what covers it), write in the PR that you merged on your own local tests, then merge. Never
  merge on `failure` or on a test you did not run. The rule tightens again when merges without green fall under 10 %. A PR that changes only docs/`.github` needs no tincan verdict. Never tell Christoffer to press anything; say it is merged and
  that he tries it once Hub has installed the update. There is no release thread; do not write that
  one merges.
- A pull request that changes nothing under `crates/`, `app/src-tauri/`, `Cargo.toml` or
  `Cargo.lock` takes the fast lane: the update only checks and builds the pages and is on
  Christoffer's screen about a minute after the merge. Anything else is a full build. So keep a UI
  change free of Rust, and split a change that needs both into a UI pull request and a Rust one.
  Open the UI one first, so Christoffer sees the new look within a minute, and make it work
  against the Hub he has installed now: a call the running hubd does not have yet (`hubdHas` in
  `app/src/lib/compat.ts`) shows that it needs the next version instead of failing. After a merge, say that Hub is building it (Settings > Updates shows the step), not that it waits
  for Christoffer to release it.
- A version that changes Rust waits until the branch has been quiet for 10 minutes before it builds
  (Build now skips that), so Rust changes merged close together become one build. To know where
  a merged change stands, read the commit statuses on its merge commit (`gh api
  repos/{owner}/{repo}/commits/<sha>/statuses`): each Hub posts `hub/update · <machine>` as
  pending (waiting or building, with the step), success (built, then installed) or failure. Say
  what that says instead of guessing.
