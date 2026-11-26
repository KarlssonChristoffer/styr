# Architecture: Self-update and security
<!-- för: agent -->

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## Self-update
<!-- covers: crates/hub-core/src/update.rs, crates/hub-core/src/update/, crates/hub-core/src/rollback.rs, crates/hub-install/src/probation.rs, crates/hub-launcher/src/main.rs, app/src-tauri/src/main.rs, app/src/lib/rollback.ts, app/src/lib/components/BuildWaiting.svelte, crates/hub-core/src/notes.rs, crates/hub-core/src/update_report.rs, crates/hub-core/src/update_signals.rs, crates/hub-core/src/prci.rs, app/src/lib/pushGap.ts, app/src/lib/components/PushGap.svelte, crates/hub-core/tests/it/main.rs, crates/hub-core/tests/it/wire.rs, scripts/pins.py, scripts/monday.py, scripts/mutanter.py, scripts/adapters.py -->

**CI on pull requests** (`prci.rs`). With `update.tests_on` set, the
PC's hubd looks every 20 seconds at the open pull requests against main (not drafts, not forks), fetches the head
of each one that has no `hub/ci · <machine>` verdict yet, sends it to the testing machine through
`checks::start`/`wait` (a commit already in the machine's latest history is not sent again, only started) and posts pending, success or failure on the head commit. It is report only:
nothing waits for it. It skips a round while the update itself builds or waits for its own tests after an install (the machine runs one test run
at a time and the update's comes first), and a run that could not finish is posted as `pending` "Waiting, tries again: <why>" and tried again after a minute, up to three times, then as `error` and tried every 10 minutes (`prci::posted`, from `checks::outcome_of`). Why: the update's run took the machine's single slot (`tincan is testing another version`, or `stopped for the tests of another version` when the PC asks in the moment the old run ends); hubd there restarted (`tincan no longer has the run`); the machine installed an update (`stopped to install the update`); memory stayed short. On the machine a step that is stopped for memory or killed by the system (`MemoryShort`, `out_of_memory`) is run again, one job at a time, up to three times; still short, the job ends `stopped` (`JobState::Stopped`) instead of `failed`, so low memory never paints a commit red. Any run the machine stops (for another version, for an install) also ends `stopped`, its tail saying why (`checks::STOPPED_FOR_*`, a tail that starts with "stopped "; an older PC reads it as "ran out of memory", still pending). Before the testing machine installs a ready version (`update::round::before_install`, ahead of waiting for the threads) it lets the run that is going finish, the PC's update run as well as a pull request's, at most until the run is `checks::RUN_LIMIT` (40 min, as long as the PC waits for it) old or the install has waited as long (`checks::finish_before_install`; Settings > Updates there shows the wait as the step); one still running then is stopped as `stopped to install the update`. An older machine ended a run it stopped for another version as `failed` with the tail "stopped to install the version that was ready"; the PC reads that as lost (`compat-until: 2026-12-11`). A failure names the tests that fell: the machine's step error puts their `test <name> ... FAILED` lines first (`update::failed_tests` over the whole step output, the `... FAILED` lines and libtest's closing `failures:` list, at most 20), before the last 12 lines, so the 4000-character tail keeps them and an older PC's parser finds them too; the status says as many as fit in its 140 characters and "+N more". The polling PC waits three minutes for a machine that does not answer. Threads that merge seconds after opening the PR (most do) never see a verdict: the verdict needs the thread to wait for it. It uses no GitHub Actions.
When the tests passed it also posts `hub/pins · <machine>` from `scripts/pins.py` (Rust and app test counts, weeds from `scripts/garden.py`, `hub_lines` = lines in `crates/hub-core/src/hub.rs` + `hub/`, each against main): `success` or `failure` with the numbers. `hub_lines` is a ceiling that only shrinks: breaking the ring by moving `impl Hub` parts into `hub/` that grow it is red, and the PR says what else shrinks. Red means the pull request says why in its description; it does not block a merge by itself. Idle CPU/RSS, phone wire size and build time are too slow or noisy per PR: the Monday routine on tincan runs `python3 -I scripts/monday.py <repo> --out-dir <halsorutin> --hubd <hubd release binary>` instead. It measures idle (`scripts/idle-matning.py`), wire bytes (the `wire::` test of hub-core's `it` program, `tests/it/wire.rs`, with `HUB_WIRE_OUT`: the thread list, the Now card and one long thread's events for a fixed set of threads, so only a change in what the core sends moves it) and build seconds (`code-health.py --build-time`, test build + app build), writes `pins-<date>.json` and compares it with the previous week's file through `pins.py` (`--extra-base/--extra-head`), so the thresholds stay in one place: idle CPU +20 %, RSS +10 %, wire +10 %, build +15 %, merges without green tincan +10 %. That last pin (`merge_no_green_pct`, with `merges_7d`) is counted with `gh` from last week's merged pull requests: the share whose head never had `hub/ci · tincan` = success. It says how often the merge rule (wait for green) held; `--repo owner/name` when `gh repo view` cannot name it; first measured 10 nov: 69 % of 97. The same routine runs `scripts/mutanter.py` (`cargo mutants` on one hub-core file a week, single job; survivors are listed in `mutanter-<date>.md` and go to Christoffer, nothing is deleted on them). Nothing is posted when Python or the script is missing.

An installed Hub lives in `%LOCALAPPDATA%\hub` (layout in `crates/hub-install`). `hubd` follows
`origin/main` in its own clone under `src/`, every `update.interval_secs` (180):

1. Fetch. If the branch points at the running version, or at a commit that failed before
   (remembered in `update-failed`), do nothing. Try again in Settings (`POST /api/update/retry`)
   forgets that commit, for failures that were not its fault.
2. Reset the clone to the branch and run the steps: `npm
   install` (skipped when `package.json` and `package-lock.json` hash the same as when `node_modules`
   was last installed; the stamp is `node_modules/.hub-lock-stamp`), then the tests (`npm run check`, `npm test`, `cargo test --workspace --exclude hub-app`), then
   the build (`npm run tauri build -- --no-bundle`, `cargo build --release -p hubd -p
   hub-launcher`). The default clone (`<data>/src`) belongs to the updater, so whatever a
   build left in it (a rewritten lockfile, generated bindings) is discarded first; ignored
   folders such as `target` stay. A clone set in `update.source_dir` is never touched when it
   has changes. The tests are Hub's CI, run where Hub runs; a commit that fails them is never
   built. Hub-core's integration tests are one test program (`crates/hub-core/tests/it/`, a
   module per former file), so the test step links one binary, not twenty, and the tests of
   every module share the test threads. Output goes to `update.log`, cut to its last 2 MB once it passes 8. A step that
   runs past `update.step_timeout_mins` is stopped with everything it started (its own process
   group on Linux, `taskkill /T` on Windows). Builds go to `update.target_dir` (default `src/target`;
   Settings can move it to another drive) without incremental caches or debug info, and wait
   with a warning while less than `update.min_free_gb` (8) is free there. After each update, and
   when the core starts, the build folder is trimmed below `update.max_build_gb` (10).
   The steps run with `TEMP`/`TMP`/`TMPDIR` pointed at `<data>/test-temp`, emptied when
   the run starts and when it ends, so the thousands of test folders a run makes (some held
   open on Windows until it ends) never pile up in the system's temp folder. It is not in the
   build folder because `src/target` is inside the source clone, where the tests that need a
   folder outside any repository (`janitor::tempdir_outside_git`) would be inside one.
   Dictation goes first: a step does not start while someone has dictated in the last 90
   seconds (`voice::dictating`), and a running step is stopped when someone dictates and less
   than 3 GB of memory is left, then run again once it is quiet (a build took the memory
   whisper-server needed to start). The shared check runner (`checks::checks_in`, used by the update and the test machine) does the same: a
   check stopped for dictation runs again once it is quiet, and is never reported as the pull
   request's failure ("stepped aside for dictation").
   Hub and your threads win over the build in memory too: a step is stopped when free memory
   plus free page file stays under 1 GB for 20 seconds (`memory_short`; a shorter dip is
   ridden out, since rustc and the linker swing by gigabytes and a stop loses the minutes
   already compiled), and the log first names the five biggest programs by memory
   (`biggest_processes`), so the next stop says who had it. The step then runs once more, after
   memory has come back, with one job at a time.
3. Copy the binaries to `bin/<version>/` with a `VERSION` file. Versions sit side by side, so
   nothing that is running is overwritten; the running version's folder is never replaced.
   Older folders are removed, except the last good version and what a version on probation
   replaced (see "On probation" below); one that cannot go yet (Windows keeps a folder some program is
   in) is left for the next time and named in the log. For that reason nothing Hub starts runs
   from a `bin` folder: `hubd`, the launcher and the app all work from the data folder, and the
   engines, git and ssh they start inherit that.
4. Offer it: the state is `ready` and the sidebar shows a small "New version ready" row with
   Install, What's new (Settings > Updates) and × (hide until the next version). With
   `update.ask` off it goes on right away. After Install (`POST /api/update/install`), give
   working threads whose engine has no keeper up to two minutes to finish their turn, park the rest (see Restarts), copy the database to
   `backups/pre-<version>.db` (always a fresh copy), broadcast `restarting`, wait 150 ms for it to leave, let go of the
   port and start the new `hubd` with `HUB_TAKEOVER=1`.
5. The new `hubd` opens and migrates the database, binds the port and only then points `current`
   at itself. The old one waits for that and exits. If the new one exits or is not up within 90
   seconds, the old one stops it, restores the copy if the database was already migrated past
   what it knows, takes the port back and marks the commit as failed. The app hears `restarting`
   and, once `current` has moved, follows it: in place when only the pages changed (see "The
   window follows without restarting" below), else by starting the new app. Engines resume
   their sessions.

**On probation.** (Automatic rollback, in parts; `hubd`, the launcher and the app's shell act on the verdict, see below.) When a
`hubd` started with `HUB_TAKEOVER=1` points `current` at itself (`update::took_over`), the
version is on probation until it has shown it is healthy. The files sit beside `current` and are
kept by `crates/hub-install/src/probation.rs`, which has no dependencies so the launcher, the
app's shell and `hubd` can all ask the same question: `probation.json` (version, `previous`,
the `backups/pre-<version>.db` copy, starts, clean exits, `core_ok`, `window_needed`, `window_ok`),
`last-good` (the newest version that got through) and `rolled-back.json` (the last rollback,
shown until dismissed). `probation::verdict(data, now, Limits)` answers `Healthy`, `Wait` or
`RollBack(reason)` from those files alone: healthy once the core has been up long enough
(`note_core_ok`) and, when a window needs it, the new pages said they are alive (`note_alive`);
rolled back when the core restarted unasked `crashes` (2) times (`starts - 1 - clean_exits`; a
stop somebody asked for, `note_clean_exit`, never counts), when a window loaded the pages and
stayed silent for `alive_secs` (90), or when the version's programs are gone. A probation that
`current` no longer points at, and a `dev` build, are never judged. `roll_back_target` picks
`previous`, else `last-good`, else the newest other complete version. `update::prune` keeps
`last-good` and the version a probation replaced, and never the one `current` names. It removes
folders only: a file in `bin` is the shell wrapper `hub-shell`, which engines that outlive the update
still run every command through (removing it made each of their commands exit 127). A new hubd renews it
by making the new one beside it and renaming it into place (`live_output::renew`), so there is no moment without one.
`confirm_healthy` records only the version `current` names, and a version taking over from one
still on probation goes back past it to `last-good`. Writers replace the file whole under a name
of their own; two writers at the same instant can still lose one count (the core is the only
frequent writer, so this is accepted).

The launcher and the app's shell take the same decision by calling `probation::act` (it reads the
verdict and, on `RollBack`, runs `probation::roll_back`: `current` goes to `roll_back_target`, the
failing commit goes into `update-failed` so it is not built again, `rolled-back.json` is written,
the probation ends). Once `current` has moved, a failure of the bookkeeping after it is printed
to stderr (the crate has no logger), not dropped; `end`, `end_of` and `dismiss_rolled_back` return
the error (a file already gone is fine) and the core logs it. They hold no logic of their own. The launcher (`hub-launcher`) calls it before
it starts anything, and a version whose `hub-app` cannot be started is rolled back the same way
(three tries at most). The shell (`app/src-tauri/src/main.rs`) tells the probation a window is
needed when it starts (`note_window_needed`, only with a local core and not for `dev`), starts the
alive clock when a window loads the pages (`note_window_loaded`, in `Pages`), and every 3 seconds
calls `act`; when `current` has moved back off the version it shows, whoever moved it (itself, the
launcher or the core), it runs `relaunch`, which reloads the windows in place if the older version
has the same shell, else starts the older app. `roll_back` only moves files: putting the database
right for the older `hubd` is the core's job when it starts. The limits are `Limits::default()`
(2 crashes, 90 s) until the core's `update.probation_*` settings are read here too. The shell marks a window as needed (`WINDOW_PING`) because the core now confirms health and takes the alive ping. Stopping a failing core that still runs is the core's job (it is not stopped by the files moving).

**Probation in the window.** `app/src/lib/rollback.ts` reads two optional fields the core adds to the
update status: `probation` (the running version is on trial) and `rolled_back` (`from`, `to`,
`reason`, `at`, `db_set_aside`). Settings > Updates shows the local Hub as "on trial" and, after
a rollback, a red notice "Went back to X" with the reason and a Dismiss button
(`DELETE /api/update/rolled-back`). Once the pages of a version on trial have rendered, the
window sends `POST /api/update/alive { version }` (once per version, feature `update.alive`;
a core without it is never asked, and a status without the fields shows nothing).

**Going back.** `hubd` (`crates/hub-core/src/rollback.rs`) watches its own version while it is on
probation. At every start nobody asked for it calls `rollback::started` (`note_start`; the
takeover counts itself), and before a stop somebody asked for (settings restart, Ctrl-C) it calls
`clean_exit`. A task looks every 5 seconds (`rollback::judge`): it adds run time while its own
`/api/health` answers (a gap longer than three looks, such as sleep, adds nothing) and, after
`update.probation_mins` (5), calls `note_core_ok`; `Healthy` writes `last-good` and ends the
probation, `RollBack(reason)` goes to the main loop. `update.probation_crashes` (2) is the crash
limit. A page that rendered posts its version to `POST /api/update/alive` (feature `update.alive`);
it counts only for the version that runs. On a failed probation the core tells the surfaces
(`StreamMsg::RolledBack`), stops serving, parks its threads and calls `rollback::go_back`: it
puts the database right, then for up to three candidates (`roll_back_candidates`: what the
version replaced, `last-good`, then the newest complete folder) points `current` at it, starts its
`hubd` with `HUB_TAKEOVER=1` and waits for its health check. The first that answers wins: the
failing commit is marked failed (after the start, since a taking-over core clears the marker),
`rolled-back.json` is written and the probation ends. If none starts, `current` stays and the
probation ends, so nothing loops; the core takes its port back and the update status says
`Failed` with the reason.

The database rule: the copy `backups/pre-<version>.db` has the schema the version before knew
(`user_version`). A live database at that schema or lower stays as it is. A higher one is saved
as `backups/rolledback-<version>.db` (kept 14 days, never touched by the pre-update prune) and the
copy replaces it; what was written since the handover, at most the probation, is in the saved
file and `rolled_back.db_set_aside` says so. With no copy nothing can be done and the version
before may refuse the database. Going back past `previous` to `last-good` can land on a version
older than that copy's schema; accepted for now. The last rollback shows in `UpdateStatus.rolled_back`
(feature `update.rollback`) until `DELETE /api/update/rolled-back`. When the launcher or the app's shell rolled the version back first (files only), the running
core sees `rolled-back.json` naming it with `current` moved (`Step::Left`), puts the database
right through the same rule and starts the older `hubd` (`hand_back`); and a `hubd` starting after
such a rollback sets aside a database migrated past its schema before opening it
(`settle_database_file`; only builds with this code do it). Not yet built: the test button
`HUB_FAULT`.

**Pages only.** Most commits change only what the window shows. Each installed version has a
`NATIVE` file: git's ids of what its programs were built from (`crates/`, `Cargo.toml`,
`Cargo.lock`, `app/src-tauri/` and the toolchain files, hashed by `update::native_stamp_at`).
When the commit to build has the same stamp, and the same `SHELL` stamp, as an installed version
that has all its programs (`update::pages_only_donor`, `update.fast_pages`, on by default, and
only with the default steps), the build is four steps: `npm install`, `npm run check`, `npm test` and `npm
run build`, then the docs' `covers:` lines are checked in-process (`covers::stale`, the one Rust test
a change to pages can break). The version is made of that donor's `hubd`, app and launcher plus the
new pages, so no Rust is tested or built, the build folder stays warm and the window reloads in
place (same `SHELL`). Other Rust code or shell, a donor without a stamp (the first build after this
arrived) or a lane that cannot be put together (`PagesUnavailable`) builds everything as before.

**When Rust builds: the build mode.** A version that needs a full build (no installed version has
its `NATIVE` stamp) is held back by `update.build_mode` (`update::hold`), set under Settings >
Updates and read again from `hub.json` on every look (`update::live`), so a change counts at once.
A pages-only version never waits in any mode, and neither does a machine the PC pushes to. While
held the state is `building`, `waiting_until` says when it goes at the latest (none for Only when I
say), the step says why, and `UpdateStatus.waiting` (feature `update.modes`) says how many merged
pull requests change Rust (`rust`), how many in all (`total`) and the count that starts the build
(`goal`). `GET /api/update/waiting` lists them (`update::changes_since`: the first-parent commits
since the running version, each marked Rust or pages, the title taken from the pull request).
Build now (`POST /api/update/build-now`, feature `update.quiet`) skips every mode.

- `right_away` (default): the commit is `update.rust_quiet_mins` (2) old, so Rust changes merged a
  minute apart become one build. The wait has a ceiling of three times that after the oldest Rust
  change the running version lacks (`wait_until`), because merges arriving every few minutes
  would otherwise move the build off for ever.
- `merges`: builds once `update.build_merges` (8) Rust changes wait, or
  `update.build_max_wait_mins` (60) after the oldest of them. With many threads merging nonstop
  the branch is never quiet, so a count says more than a quiet time. `update.build_notice_at`
  (3 and 5) are the counts at which the window says how many wait.
- `times`: at the next of `update.build_times` (`HH:MM` on the clock of
  `update.build_utc_offset_min`, which Settings sends from the window) after the oldest waiting
  Rust change (`update::next_slot`).
- `manual`: only Build now.

Settings > Updates has the four modes as one row (a mode the hubd lacks shows "Needs the next
version", `hubdHas("update.modes")`) with the count or the times under it, and "Waiting to build"
(`BuildWaiting.svelte`): the merged changes since the running version, Rust or pages, the count
against the goal, and Build now. The status bar says "3 of 8 waiting", and `StatusBar.svelte`
tells once when the count reaches one of `notice_at` (3 and 5).

Merges do not stack up behind the wait but behind the build. While a build runs, the commits that
land are the next build's batch: when it ends, a commit that was merged before the end
(`LAST_BUILD_END`) starts its build with no quiet time, and with Ask before installing on the
updater looks at the branch right after a build instead of after the next interval, so the next
build starts behind the ready version at once, with everything merged meanwhile. The ready
version stays on offer and Install still works at any point (it stops the build behind it).

**Where a version stands, on GitHub.** Each change of the update's status is posted as a commit
status on the version's commit (`update_report.rs`), context `hub/update · <machine>`: pending
while it waits or builds (with the step), success once it is built and once it is installed,
failure when it did not build. It goes through `gh` in the updater's clone, one at a time, and a
failure to post is only logged; `update.report_status` turns it off. A thread that opened a pull
request reads this on the merge commit instead of guessing where its change is.

**Tests on another machine.** The tests (`npm run check`, `cargo test`) took most of the memory of
an update on the PC. With `update.tests_on` set to a machine's name (a Linux server such as tincan),
the full build sends that machine the commit and builds only the release itself meanwhile
(`update/checks.rs`, `update::build_and_install_with`); the version is installed once the machine says the
tests passed. The machine keeps its own repository in `<data>/checks/src` and takes the commit as a
git bundle (a host has no GitHub login), only what it lacks after the first one; it asks for it with
`GET /api/checks` (what it has, what is missing, how the last run went), `POST /api/checks/source`
(the bundle), `POST /api/checks/run` (the commit, the same version asked twice is the same run, a
newer one stops the older) and `POST /api/checks/prepare` (installs Rust with rustup, no sudo, when
it is the one tool missing). It runs the shared check runner (`checks::checks_in`) into
`<data>/checks.log`, with its build folder in `update.target_dir/checks` (tincan's `/` has 12 GB
free, so set `target_dir` to a big drive there). The tests run on Linux, so Windows-only code is
compiled by the PC's release build but its tests do not run anywhere. A machine that cannot (offline,
hubd too old for the calls, no room) or that loses or stops the run (it restarted, ran another version's tests, ran short of memory) leaves the tests to the PC,
which runs them after the build as before; only a failed test fails the commit. With
`update.tests_after` the version is installed as soon as it is built and the verdict comes later:
a failing run (or one that could not finish) becomes a problem in Hub's Problems (source `update
tests`, grouped, a thread can fix it), the installed version stays. Where `tests_on` cannot run
them, they do not run, and a warning problem says so. Not used with
`update.steps` of its own, and the pages-only lane runs no Rust tests either way. Feature
`update.checks`.

The database has a schema version (`PRAGMA user_version`, steps in `store.rs`). Steps are only
added, never changed, and a build refuses a database a newer build has moved past rather than
misread it. Event kinds a build does not know are read as `unknown` and skipped.

Before step 4 the core writes patch notes for the new version (`crates/hub-core/src/notes.rs`): every
commit since the running version goes to the tidy model, which answers with what is new and what to
try. The model is asked at most once an hour (`notes::spaced`): a version built sooner after the last written
notes keeps only its commits (the pages list those), and the next written notes start at the version the
last ones were for (up to 80 commits), so nothing built in between is left out; "Write notes for this version"
under Settings > What's new writes them at once, from the same start. The day's devblog reads only written notes. A bullet may end in `{show: place}`, a page from `notes::PLACES`, and `{test: step; step}`,
how to try it by hand; the Updates page turns those into Show me and How to test
(`app/src/lib/patchNotes.ts`), and Settings > What's new shows the text without them. They are kept
in the `kv` table per version; the app shows "What's new" in the sidebar footer until you open them.

The Start menu shortcut points at `Hub.exe`, a tiny launcher that starts whatever `current`
names and exits, so it is never locked when an update replaces it. Development builds report
version `dev` and never update. Steps, artifacts, branch and interval can be overridden in
`hub.json` under `update`.

### The window follows without restarting
<!-- covers: crates/hub-core/src/shell.rs, app/src-tauri/src/main.rs, app/src-tauri/src/card.rs, app/src/lib/components/InstallBox.svelte, app/src/lib/components/InstallCard.svelte, app/src/lib/components/InstallWindow.svelte -->

The app is a small native shell (`app/src-tauri`) around pages (`app/src`), and nearly every
version changes only the pages. So step 3 also copies the built pages (`app/dist`, emptied before
the build so an old one never passes for new) to `bin/<version>/ui` and writes `bin/<version>/SHELL`,
a stamp of what the shell is built from (`crates/hub-core/src/shell.rs`): the files under
`app/src-tauri` and `crates/hub-install` (without build output, line endings evened out), the
crates `hub-app` pulls in as `Cargo.lock` pins them, and the `@tauri-apps/*` versions in the app's
npm lockfile. A core-only crate, a Svelte file or another npm package leaves it as it was.

The shell serves its windows' pages through its own asset provider (`Pages` in `main.rs`): each
time a window loads `index.html`, the version `current` names is used when its `SHELL` is the
shell's own and it has `ui/`; otherwise the pages built into the program. When the core says
`restarting`, `relaunch` waits for `current` to move past the version on screen, then:

- Same stamp: every window but dev mode's reloads. The window keeps its size and place, the page
  (a reload keeps the window's history, `history.svelte.ts`), drafts, popped-out viewers and the
  dev window. One switch serves all windows that asked.
- New stamp (or a version from before this): the new app starts where the main window was
  (`HUB_WINDOW`: position, size, maximized) and this one exits. The main window noted its page
  (`keepPlace`, `hub.place` in local storage) and the new one opens there when it starts within
  five minutes.

A development build has no stamp and always shows its own pages.

While the window waits for the new version, it shows the install card (`InstallCard.svelte`:
"Installing a new version of Hub" with the working grid, `WorkGrid.svelte`, the same cube an agent
shows while it works). A shell that has the card window (`card.rs`) hides Hub's windows and opens
the card as a small window of its own on the desktop, on top, without a taskbar entry, so Hub looks
closed until it is back; the page there is `InstallWindow.svelte`, picked in `boot.ts` by
`__HUB_INSTALL_CARD__`, and talks to no core. Older shells leave the windows, and `InstallBox.svelte`
covers the page with the card on a plain dark page instead. `hub.installing` is set when the core
says `restarting` in the desktop app's windows (not a browser tab, not the dev window, which reload
nothing) and cleared when `relaunch` finds no newer version or the core says `failed`; a reload of
the page ends it. That box leaves the caption bar alone, so the window still moves and closes.

### Live mode
<!-- covers: app/src-tauri/src/live.rs, app/src-tauri/capabilities/, app/src-tauri/build.rs, app/src/lib/live.svelte.ts, app/src/lib/carry.ts, app/src/lib/components/LiveNote.svelte, app/src/main.ts, app/src/boot.ts -->

Christoffer sees what the open pull requests draw in his own window, without merging or building
anything. A button on the rail (and Ctrl Shift L) switches the main window between the installed
pages and the live ones: dev mode's clone on the preview's `live` setting (see "Dev mode"), served
by Vite on port 1421, so a push shows within seconds. Left click switches; right click lists every
open pull request with a tick on the ones that show and, for the rest, why not; below them the
ones merged but not installed yet, ticked too (they are in the base the pages are built on), with
where the update stands. "Look for new pushes now" answers in a notice: "Found 2 new pushes: #5,
#9", "Checked, nothing new" or why the look failed (`DevPreview.check`). The live pages
talk to this core like the installed ones, so what is clicked there really happens. A thin frame
in the accent runs round the window while it is live.

Going live (`live.svelte.ts`) turns the preview on, waits for Vite and the first preview (the
first time also for the clone and its packages; the button breathes meanwhile) and asks the shell
(`live_go` in `app/src-tauri/src/live.rs`) to move the window there. Going back is `live_leave`.
The two kinds of pages come from different origins, so each has its own storage: the page that
leaves hands its storage to the shell and the page that comes takes it (`live_take`) before the
app reads any (`carry.ts`; `main.ts` waits for it, then loads `boot.ts`). Drafts, zoom and where
you were come along both ways.
If `boot.ts` fails to load (Vite serves files that dev mode's refresh is rewriting, for instance right
after a merge), the page reloads itself up to four times, two seconds apart, before it sends the
window back with "The live pages did not start".

The installed pages are never at risk. While live, the page sends a sign of life every three
seconds as long as the app is drawn (`live_beat`); after 15 seconds without one (90 while it
first loads) the shell sends the window back by itself. An error that leaves the page empty sends
it back from the page, with the error and, when the error names files only one of the shown pull
requests changes, that pull request (`LiveNote.svelte`, with Try again). An update with the same
shell reloads the live pages in place; one with a new shell starts on the installed pages. The reload of the live window waits first (`live::wait_for_pages`): the new
core starts Vite again as it comes up, and a reload that met the gap showed the browser's "refused
the connection" page, so it waits for two answers of 200 in a row, at most a minute (the install box
covers the page meanwhile).

Tauri lets a page from another origin call none of the app's commands unless a capability names
them, so the app's commands have permissions of their own (`build.rs`): `capabilities/default.json`
grants all of them to the installed pages, `capabilities/live.json` only what the live pages need
(not `live_go` or `open_dev_window`), and `capabilities/dev.json` only `core_token`, which every page
needs to show its core the token (see Security). The dev window ("Hub · dev") is a window of its own and stays
one; in the main window the same pages count as the main window (`isDevWindow` in `native.ts`).

The dialog plugin's `dialog:allow-message` is granted in the same three files. It is what
`window.alert` and `window.confirm` call, so both show a native box (the plugin's init script
replaces them). Without it the call fails with "not allowed by ACL" and nothing shows.

### A ready-made Hub for a new PC
<!-- covers: crates/hub-core/src/dist.rs, scripts/install.ps1, crates/hub-core/src/api/machines.rs -->

A new Windows PC used to compile all of Hub (10-25 minutes, git, cargo and npm needed first).
Now a PC that already has Hub hands over its installed `bin/<version>`: `GET /api/dist?platform=<os>-<arch>`
(`machine.dist` in `compat::FEATURES`) sends it as a `.tar.gz`, made once per version by the
system `tar` into `<data>/dist/` (`crates/hub-core/src/dist.rs`). Only the same operating system
and CPU can run it, so any other `platform` is answered `404` with what this machine is, and a
Hub that is not an installed version (a `cargo run`) has nothing to hand over. The call takes a
connection token like the rest of the API, and a phone's token is not allowed it.
`scripts/install.ps1 -From http://pc:7878 -Code ABCD-EFGH` redeems a pairing code (Settings >
Machines, or `hubd pair`) for a token, downloads and unpacks the package into `bin/<version>`,
sets `current`, copies the launcher to `Hub.exe` and starts Hub; it also clones the source for the
updater when git can. If the download fails for any reason the script says so and builds from
source as before. Not done: a Mac (no Windows PC can build one; the Mac path is `install-host.sh`),
a button in Settings that shows the command with a fresh code, and sending later updates to such a
PC (its updater still builds from the source clone).

### Building hubd for other machines
<!-- covers: crates/hub-core/src/crossbuild.rs -->

A Raspberry Pi takes up to an hour to compile hubd and can run out of memory doing it, and
updating from the private repository would need a GitHub login on it. So the core builds hubd for
the Linux machines it reaches and sends it (`crates/hub-core/src/crossbuild.rs`).
`cargo zigbuild` cross-compiles with zig as compiler and linker, for `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu` or `armv7-unknown-linux-gnueabihf`, against glibc 2.31 (Debian 11,
Raspberry Pi OS, Ubuntu 20.04 and newer). It builds in a worktree of the updater's clone at the
version wanted, one build at a time, with the output in `cross.log`. That happens in `cross` in
the updater's build folder, so it follows `update.target_dir` to another drive and the updater's
limits apply: no build with less than `min_free_gb` free (it is tried again ten minutes later), and
`max_build_gb` covers both. Trimming drops `cross` after the tests' debug build, unless a build for
another machine is running, and Settings > Updates > Disk lists it on its own. A build for another
machine does not trim while this machine's own update builds (`update::building`): emptying
`target/debug` under its `cargo test` broke the run ("can't find crate") and cost a second one. A host that is not up to date is tried every ten
minutes, but a send that failed waits 10, 20, 40 minutes and so on up to six hours before the next
try (`remote/ship.rs`, `retry_after`; "Update now" ends the wait), is not tried while this machine's own
update builds (zig's linker died with "LLVM ERROR: out of memory" when both ran), and shows as one
problem per host in Problems with the linker's own error line. A cross build gets
`CARGO_BUILD_JOBS` from free memory (3 GB a job, 1 to 4, `crossbuild::jobs`). A build that dies of
memory ("out of memory", "memory allocation of", "Allocation failed") is not counted as a failed
send: it waits the base ten minutes again without backing off, and the problem is a warning that
says it ran out of memory. The first build for
a target takes a few minutes, later ones less.

The tools are installed once, after the user said yes: `rustup target add`, `cargo install
cargo-zigbuild`, and zig 0.13.0 in `<data>/zig`, downloaded from ziglang.org and checked against
the checksum in its index.

Such a host has `update.pushed` in its `hub.json` (the installer writes it). It fetches and builds
nothing. Instead the core keeps it on its own version: when the host comes online, and every ten
minutes, a host on another version gets this one built for it. The core puts it in the host's
`bin/<version>/` and names it in `pushed`. One send per host runs at a time, and each writes into
a folder of its own (`bin/<version>.partial.<pid>`) that it moves in whole, so overlapping sends
never move each other's files away. The host's updater sees `pushed`, waits until no
thread is working and hands over as usual. A development build (`dev`) pushes nothing.
A host with `push_quiet_mins` (optional, unset or 0 = no gap; in its `hub.json` `hosts` entry, or for
a machine added from the app chosen on its page: the "New versions" row, `PushGap.svelte`, choices
in `pushGap.ts`, set with `PATCH /api/hosts/{name}`, feature `hosts.settings`; a hubd without it
shows "Needs the next version") gets at most one version per that many minutes from the ten-minute check (`ship::quiet_left`):
meanwhile its update cell says "Next version in N min". The clock is when the last version was
delivered, kept in the database (`remote_pushed_at:<host>`) because this core restarts for its own
versions and the track's `sent_at` is in memory. Update now, a host never sent to and a host with no
version still get one at once. At 30 min a day of merges restarts the testing machine (and the pull
request runs in it) at most twice an hour.

A Linux host that still builds for itself (installed before this, or with a hub.json of its own)
is moved over the first time it would be sent the source, unless the user said no to building
here: the core builds hubd for it, reads its `hub.json` over SSH and writes it back with
`update.pushed` and without the build settings (`remote::pushed_config`, engines and the rest
stay), deletes `src/target` and `bootstrap` there, points `current` at the new version and
restarts its service. Building on the host needed several GB for `target` (a debug build for the
tests and a release build) plus 8 GB free by default, which a 32 GB Raspberry Pi card does not
have; the binary it gets instead is tens of MB.

Where a machine still compiles hubd itself (a Mac, which the PC cannot build for), it builds only
hubd in release, without running the tests first: the version comes from the PC, which tested
it. The release profile has no LTO and the default 16 codegen units, so the compiler uses every
core on `hub-core`, one crate of about 50,000 lines; with one codegen unit and LTO that crate
compiled on a single core. `hub-core` itself is built with `opt-level = 1` (its dependencies keep
full optimisation). Measured on 4 cores: a clean build took 14 minutes with LTO and one codegen
unit and 7 without; rebuilding after a change to `hub-core` took about 6 minutes, and under 4
with `opt-level = 1`.

Installing again while an installer still runs on the host (Start over on its row in Settings >
Machines) stops that installer and everything under it first (`STOP_INSTALLER`), so one stuck
waiting is replaced instead of joined by a second build. The installer also rewrites a
`hub.json` that is exactly the old build-with-tests config to build only.

When SSH itself does not get in, the row says so instead of keeping an old install step. A
host that refuses this machine's key gets `setup: no_key` and a password field on its row: the
login password, once, puts the key there (`Remotes::let_in`, `POST /api/hosts/{name}/key`, the
same `sshkey::give_key` that Set up uses) and the host is tried again.
While a host is `no_key` Hub stops retrying it on its own (one slow try per 30 minutes, or Reconnect):
every try is a failed login in the host's log, which fail2ban and sshd's limits count. `give_key` adds a
newline to `authorized_keys` first when the file lacks one, so the new key never joins the last line.

### Updates on every machine
<!-- covers: ios/Hub/Updates.swift, ios/Hub/ReleaseNotes.swift, ios/Hub/ReleaseNotesView.swift, ios/scripts/deploy.sh, ios/Hub/SettingsPages.swift, ios/Hub/HubClient.swift, ios/Hub/SettingsView.swift, crates/hub-core/src/updates.rs, crates/hub-core/src/update_track.rs, app/src/lib/autoInstall.ts, app/src/lib/components/UpdateCenter.svelte, app/src/lib/components/UpdateBadge.svelte, app/src/lib/components/TourCard.svelte, app/src/lib/patchNotes.ts -->

The native iPhone Updates page (`ios/Hub/SettingsPages.swift`, `Updates.swift`) reads
`GET /api/updates?fresh=true` from the paired Hub, the same fleet and shipping target as that
Hub's desktop. Git hashes are identities: never sort them to choose a newest version. A remote
on another revision differs from the sending Hub; that alone does not prove which commit is
newer. Updater state (building, ready, restarting, failed) is shown separately. The updater's
reported version takes precedence over the older host inventory version. Phone rows put long
build messages below the name/version with natural height. The phone refreshes every five
seconds while visible and on foreground return; desktop reads fresh on opening/return and
polls every three seconds when working, otherwise fifteen. Refresh failures keep the last
snapshot with a stale-data warning. The phone's existing permission scope can send updates,
but ready builds are installed from desktop. Clients paired to different Hubs can have different
shipping targets; compare against the desktop of the paired Hub.

What's new on the phone (`ios/Hub/ReleaseNotes.swift`, `ReleaseNotesView.swift`): `ios/scripts/deploy.sh` lists the
pull requests merged under `ios/` since the last build installed from that Mac (`git log --first-parent`; a
`(#n)` subject or a "Merge pull request #n" body gives the title and number, a leading "iPhone app:" is dropped),
puts the history, newest first, in `ios/Hub/ReleaseNotes.json` for the build, and writes it back to
`~/.hub-ios/builds.json` only after the install worked. The committed file is an empty list and the script
restores it on exit. The app reads it from its bundle: once per build, a second after start, a half sheet shows
the three newest changes (drag up for all of them and the builds before; × or a drag down closes it, "Turn off"
ends it). It never shows on a first install, for a build with nothing recorded, with the notes off
(`releaseNotes.enabled`, Settings > Updates > Tell me what's new) or twice for the same build
(`releaseNotes.seen`). Settings > Updates lists every build as a card that opens in place, and the build line at
the foot of Settings opens that page.

Settings > Updates is one place for everything that can be updated, on every machine
(`UpdateCenter.svelte`). On the left a row per program: Hub itself and each agent CLI Hub knows
(the harness catalog's `cli`: Claude Code, Codex, Hermes, OpenCode, Gemini CLI), split into
Waiting and Up to date, with a few words on how it stands per machine. On the right the one
picked: a row per machine in fixed columns (name, from → to, a short state pill, Install or
Update), Install everywhere on top. The pill is one short word (ready, building, waiting, updating,
different version, update sent, failed, up to date, offline). For another machine that was sent a version the pill is its step (sending, sent, received, building, ready, restarting, updated, failed), followed by five dots that fill as the machine reports each step (red where it failed) and one line of what it is doing or why it failed (`hubd_track`, see "The handshake of a sent update"); a failed one has Try again. The longer detail (why it failed, that Hub sent
the version and the machine's updater switches when no thread works there) sits in the pill's
tooltip so rows never grow. Then what is new. For Hub that is the patch notes of the version on offer (else the one running),
each item with Show me (opens the page in its `{show: …}` and a small card naming the change,
`TourCard.svelte`) and How to test (its steps, Works / Doesn't work; Doesn't work drafts a thread
in Hub's own folder). For a CLI it is its makers' release notes for the versions between the
oldest any machine runs and the newest (`GET /api/updates/news`): the harness catalog's
`changelog` is a Markdown file with one `## <version>` per release (`md:<url>`) or a GitHub
repository's releases (`github:<owner>/<repo>`), read once an hour. The settings of how Hub
updates (branch, interval, disk) are folded under the page.

A ready version of Hub shows as one row at the foot of the sidebar (`UpdateBadge.svelte`,
`hub.updateReady`, just above the foot); the avatar on the rail carries an accent dot then too. Install or × takes
it away; how far the install got is on the Updates page. With "Install when I am away" on (the
window's own setting `autoInstall`, on by default, under How updates work) the app's main window
installs it by itself after three minutes without a key, click, scroll or mouse move in it
(`app/src/lib/autoInstall.ts`); × (not now) still holds that version back. Running threads carry on
through it (see "Engines that outlive the core") and the window reloads in place (see "The window
follows without restarting").

Each hubd answers for itself: `GET /api/clis` finds each CLI,
runs `--version` and says how it is updated there, by what installed it (an npm install with
`npm install -g <package>@latest`, a Homebrew one with `brew upgrade`, otherwise its own
`self_update` such as `claude update` or `hermes update`). `POST /api/clis/update` runs that in
the background with the installer's runner (`engines::run_shell`). When npm fails, `npm_failure`
reads its error code and path from the output: a cache root owns (`~/.npm`, left by an old `sudo
npm`) is retried with Hub's own cache, a global folder root owns is retried with `sudo -n` when sudo
needs no password, and otherwise the row says the real reason and the one command to run there
(never npm's log path). The machine you use puts them
together (`GET /api/updates`, cached; `?fresh=true` asks again) and reads the newest version of
each npm package once an hour. Install everywhere (`POST /api/updates/run`) updates one CLI on
every machine online that has it, or on one machine when the request names it (`machine`); for
hubd it approves a ready version here and on the hosts, and sends this machine's version to
hosts on another one (`Remotes::update`), all the machines at the same time so a slow one holds no other back; one that cannot be started (SSH fails, or it needs its sudo password or your answer to "build hubd for it on this PC", which only Settings > Machines asks) says so in its cell instead of staying "behind" with no reason. With follow on (`updates.follow`, default on) the
manager looks every fifteen minutes and right after an update here finishes, and asks a machine
whose CLI is older than here to update it, once per version. hubd follows this machine anyway
(see above). A failed ask shows red on its machine's row with why.

A host whose hubd is too old for an endpoint answers 404, or 405 when an older route of the same
shape takes another method (`DELETE /api/engines/{name}` caught `POST /api/engines/receive`).
`remote::too_old` reads both as such, and the core then sends that host its version at once
(`Remotes::wake_push`) instead of telling you to update it by hand. Why a host did not get its
version (no SSH alias, may not build for it, neither clone nor builds) is kept per host and shown
under the table.

**The handshake of a sent update** (`update_track.rs`). Sending a version to another machine is not the
end of it: this machine follows it there. `Remotes::push` starts a track for the host (`UpdateTrack`,
`MachineUpdates::hubd_track`) at `sending`, moves it to `sent` once the files are on the host, and then
asks the host's updater to look at once (`POST /api/update/look` with `{version}`, feature
`update.look`; `Hub::update_look`). The answer is the receipt: the updater's own state, which names the
version as its target when it was idle (`checking`), and still says `disabled` when no updater runs
there. From then on `Remotes::follow_updates` reads the host's `GET /api/version` every five seconds
while the track moves (and `fleet` reads it on each look at the page), and `update_track::advance`
turns what it sees into the next step: received, building (the build step, or "waits for a quiet
branch"), ready, restarting (the host is offline then, which is no failure for ten minutes), done when
the host reports the version. A step only moves forward, so a row never falls back to "sent" while the
machine works; a host that shows nothing of the version five minutes after it was delivered fails with
what it said (idle, off, does not answer), a switched-off updater fails at once, and a failure of that
version in the host's updater shows with its own reason. A stalled one that starts after all carries on
from there. A hubd from before `update.look` answers 404/405: the track stays at `sent` with a line
saying its updater looks by itself every few minutes, and the old "retry" ask is still made.
(The old ask was only `POST /api/update/retry`, which a host that had not failed answers with an
error that was never read: the host waited out its three-minute round and the row showed the first
"sent" note for ever.)

## Security
<!-- covers: crates/hub-core/src/auth.rs, crates/hub-core/src/guard.rs -->

The core binds to `127.0.0.1` only. Every request goes through `guard::check` (`crates/hub-core/src/guard.rs`; the token and its
helpers are in `auth.rs`):

- A browser sends `Origin`, which a web page cannot forge. Only the app's own origins
  (`allowed_origins`) get in; any other page is refused, even with a token. CORS tells browsers
  the same. The app's origin is only enough from this machine itself: a request from another
  address or through a proxy here (`tailscale serve` names the caller in `X-Forwarded-For`) can
  carry any Origin it likes, so it shows a token like every other client (`phone::from_elsewhere`).
- Any program on this machine can send the app's `Origin` too (`curl -H "Origin: tauri://localhost"`),
  so the app's windows show the token as well: the desktop shell reads it from the data folder of
  the core the page uses (`core_token` in `app/src-tauri/src/main.rs`, the dev core's for dev mode's
  port) and `ownToken` in `app/src/lib/home.ts` adds it to every fetch and socket. The first time a
  window shows it with the app's origin, the core notes `<data dir>/token.window`
  (`guard::WINDOW_SHOWS_TOKEN`) and from then on refuses the origin alone, and a wrong token with
  it. Until then (a shell from before `core_token`) the origin alone still lets a window in, so an
  update never locks the window out. `permit::hub_files` keeps agents off that file too. What has
  no `core_token` gets 401 from then on: a plain browser tab on Vite (`npm run dev`, port 1420),
  live or dev pages from a branch before `ownToken`, and a window whose token file changed until
  it reloads.
- Anything else (another program, a core on another host, a phone) shows a token as
  `Authorization: Bearer <token>`, or as `?token=` where no header can be sent (`guard::token_in_address`:
  `/api/stream`, a path ending `/ws`, `/mcp/…` for `mcp_token_in_url`; elsewhere it is ignored, so a
  token does not end up in addresses and histories): the connection token from
  `<data dir>/token` (created on first start, readable only by the user), or one a device got for
  a pairing code (see Pairing).
- `POST /mcp` takes an agent key instead (see Hub's tools); a browser still needs an allowed origin.
- A thread's engine never holds the connection token. `auth::tools_endpoint` hands it a token for its own
  tools only (`auth::tool_token`, a SHA-256 of the connection token and the thread id), good for
  `POST /mcp/{thread}` and nothing else, so an agent talked into calling Hub's API with what it has
  gets 401. For the same reason `permit::hub_files` refuses, whatever Settings > Safety says, any
  tool call that reads or changes Hub's own files by name (`token`, `secrets.json`, `hub.json`,
  `hub.db`) under a `hub` folder or under this core's data folder.
- `GET /api/health` is open and says `{ ok, version, protocol, schema }`. `protocol` is a fixed 1, kept for
  older clients and read by nothing: a client asks for `features` (`compat::FEATURES`), and a core
  tells an older core apart by the endpoints it lacks (`remote::TooOld`, a 404 or 405 on a path
  every current hubd has). The app always comes from the same build as its core.
- Problem ingest paths (Sentry's `/api/<n>/envelope/` and `/api/<n>/store/`, OpenTelemetry's
  `/api/<n>/otlp/v1/logs` and `/v1/traces`, `problems::is_ingest`) take no token: a program shows
  its source's key instead (in its DSN, or as a bearer token), which only lets it add problems and
  timings to its own source. Removing the source shuts it out.

Secrets (`crates/hub-core/src/secrets.rs`) live in Windows Credential Manager, or in a file only
the user can read on systems without one; never in `hub.json` or the log. An engine gets the
ones its config lists under `secrets` as environment variables, and any secret value an engine
echoes is logged as `[secret NAME]`. Anything else that looks like a key is logged as a hint of
it (see the leak guard above).

Other machines reach the core over an SSH tunnel or through `tailscale serve`, which forwards to
loopback; the port is never opened. The core talks HTTPS and WSS with rustls and the web's root
certificates, so a `https://…ts.net` address works.

A sudo password given for Install hubd goes from the app to the core in the request body and on
to sudo over SSH on stdin; it is checked, used once and kept nowhere. Only plain package names
from the host's own check reach the root shell.

Settings > Safety's "everything but deletes" reads a command line by what each part really runs
(`permit::risky`): split at `&&`, `||`, `;`, `|`, `&`, new lines and the start of `$(…)` or
`` `…` ``, with wrappers such as `sudo -u pi`, `env`, `xargs`, `nohup`, `timeout 10` and the
program's folder taken off, and the quoted command of `sh -c`, `pwsh -Command` or `cmd /c` read as
the command. It is a guard against the agent deleting by habit, not against one set on getting
round it: an agent that can run commands at all can write a script and run that.

### Prompt links
<!-- covers: crates/hub-core/src/promptlink.rs -->

`http://127.0.0.1:7878/new?prompt=…` (`promptlink.rs`, after T3 Code) puts a draft in the message
box of the open window; nothing is sent, so the path needs no token. Invisible characters are taken
out of the prompt and the draft says so. A browser says how it came to load the address in
`Sec-Fetch-Mode` and `Sec-Fetch-Dest`: only a link opened as a page (`navigate`, `document`) is
taken, so a web page cannot plant a draft that reads as the user's own through an `<img>`, a fetch
or a frame. A script or `curl` on this machine sends neither header and is let through.
`promptlink.rs` reads the request and words the page; the handler that publishes the draft is
`prompt_link` in `api/projects.rs`, so the link module needs nothing from the core.

**Adapter pins.** The model list of Claude Code and Codex is what the pinned adapter (`claude-agent-acp@0.107`, `codex-acp@2.7` in `config.rs`) offers in `session/new`; patch releases float, a new minor does not. The weekly routine runs `scripts/adapters.py`, which asks npm for the latest and writes `adapters-<date>.md` next to the other reports; BEHIND means bump the pin (config.rs, connections/tests.rs, README.md) and run the replay tests. Providers with an API key list their models live (`routes::live_models`, hourly cache, catalog as fallback).
