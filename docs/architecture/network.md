# Architecture: Other machines, status, versions, API, settings and logs
<!-- för: agent -->

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## Other machines
<!-- covers: crates/hub-core/src/remote.rs, crates/hub-core/src/remote/, crates/hub-core/src/carry.rs, crates/hub-core/src/moving.rs, crates/hub-core/src/folders.rs, app/src/lib/components/FolderPicker.svelte, app/src/lib/components/FolderConnect.svelte, app/src/lib/components/FolderOld.svelte, app/src/lib/components/FolderRows.svelte, app/src/lib/components/FolderRail.svelte, app/src/lib/folderPicker.ts, app/src/lib/github.svelte.ts, crates/hub-core/src/ready.rs, crates/hub-core/src/git_identity.rs, scripts/install-host.sh -->

Threads can run on other machines (tincan, Plåtnicklas, a cloud VM). Set up once, on the PC: each
minute the PC sends every machine its engines with their keys, its lasting sign-ins
(`signin.rs`), and who git commits as (set only where git knows no one). A machine that gets an
engine whose harness it lacks installs that harness once by itself (`engines::install_once`). Each runs the same `hubd`,
bound to its own loopback, with its own log and engines, so its threads keep going while the PC
sleeps. The PC's core keeps a connection to each (`crates/hub-core/src/remote.rs`, the connection itself in `remote/link.rs`):

- Reached at a URL, usually the one Tailscale serves it at (`https://tincan.<tailnet>.ts.net`), or
  through `ssh -N -L <free port>:127.0.0.1:7878 <alias>` with the user's own keys and `BatchMode`
  (never a password prompt). A host can have both: the URL is tried first and the tunnel is the
  fallback, and once a minute a host on the tunnel checks whether the URL answers again and moves
  back to it. Which one is in use shows as the host's address. The route says nothing about who
  is calling; the token does (see Pairing).
- The token sent is the one the other core gave this machine for a pairing code, or one set in
  `hub.json`, or the other core's own, read over SSH from its data folder.
  A token from a pairing code is kept in the key vault as `HUB_HOST_<NAME>` (the machine's name in
  capitals, other characters `_`); the `hosts` value in the database holds none. At start, tokens an
  older version left in `hosts` are moved to the vault, cleared from `hosts` and from the copies in
  `backups/` (`backup::scrub_host_tokens`, every start; the copy taken before an update is left
  alone, since a failed update goes back to it). Names that make the same key are refused. When the vault is memory-only (lent keys) the token stays
  in `hosts`. Removing a machine removes its key. An older hubd started afterwards finds no token
  and falls back to reading the other core's over SSH, or needs a new pairing code.
  Health is open to anyone, so a token the other hubd has since replaced (it was reinstalled or its data
  reset) only shows on the first real call. A connection therefore tries its token once on connect
  (`token_accepted`, `GET /api/host`); refused, it takes the other hubd's own token over SSH (tried three times, since a Mac's sshd turns some connections away at once; why a read failed is in hubd's log), and a URL
  that refuses goes on to the tunnel, instead of showing "refused the connection token" until a person re-pairs.
- It lists the host's threads with `host` set, relays the host's stream into its own, and keeps
  the list while the host is offline (the chip goes grey). Threads relayed from a host that relays
  others are skipped, so hosts never loop.
- Requests about another machine are forwarded by one middleware: any `?host=<name>` (starting a
  thread there, its files, its project pages, stopping its processes), and every
  `/api/threads/{id}/...` of a thread that lives there. Surfaces talk to one core only.
- **GitHub in the picker** (`github.svelte.ts`, feature `github.repos`, see "GitHub"): the rail ends
  with a GitHub entry ("All repos" and the account's login) once any online machine is signed in;
  it lists every repository of the account, newest push first. A repository cloned on some machine
  (matched by the origin key `github.com/owner/name`) shows that machine and its branch and opens the
  clone, on the thread's own machine when it has one; one that is not cloned anywhere says "only on
  GitHub" and has "Clone to <machine>" (`/api/folders/clone` into the projects folder there, then it
  is picked). Searching adds the same two kinds under the local matches ("On GitHub, not cloned
  yet"), and a repository cloned on several machines is one row with its machines. Signed out, a note
  under the list and the rail's "Not connected" open the Connect GitHub box: it starts `gh auth login
  --web` on this machine (`POST /api/github/connect`), shows the one-time code and an Open button for
  github.com/login/device, lists each online machine with its state (a Connect button, "waiting",
  the login), polls every two seconds and closes itself once signed in. The list is asked from the
  first signed-in machine, this one first; a machine without the feature says "arrives with the next
  version of Hub", never an error.
- Picking a folder on another machine works the same way (`folders.rs`, `FolderPicker.svelte`):
  Home's folder field is a bordered button showing the folder, its place and a chevron; it opens a
  list with a rail on the left. The rail has Recent (the folders threads ran in, newest first, a
  worktree counting as its repository), Repositories and Folders (the ones threads ran in that
  are not repositories) across every machine, then one entry per machine that is online (grey and
  disabled when it is not). The list opens on the side of the field with more room (above, as
  long as 460px fit) and is never taller than that room, measured against the nearest box that
  clips it: in a small window or with the composer high up it shrinks, the rail and the rows
  scroll inside it, and the buttons and key hints under them stay in view. Repositories come from every online machine at once: every repository
  up to three levels below the usual code folders of a machine (the projects folder, `~/Dev`,
  `~/code`, `X:\Dev` on each drive and the like; `GET /api/folders/projects?host=`, read from
  `.git` without running git). A row shows the project's icon, name, place, which machine it is on
  (not when one machine is picked) and its branch: grey on the branch origin calls its own
  (`main` read from `origin/HEAD`, else main or master), amber on any other, with `↓N` for the
  commits on its upstream it lacks (counted with one `git rev-list` for each of the 30 newest,
  from the last fetch; nothing is fetched). Linked worktrees are not listed: the repository
  says "N worktrees" (`worktree_of` and `worktrees` on `ProjectFolder`). Picking a row starts the
  thread on the machine the folder is on (`onhost`); a row on another machine than the thread's
  has "bring it here" on hover (the carry below). The buttons under the list never scroll away:
  New project, No project (a fresh scratch folder), Browse and, in the app, System dialog; they
  act on the machine picked in the rail. A fuzzy search over name and folder covers every
  machine. Typing a path browses
  folders in a single flat list (`GET /api/folders?path=&host=`), leaving out hidden, system and build folders (AppData,
  Cookies, node_modules) unless asked. Pasting a git URL clones it into the projects folder, and a
  name that matches nothing makes a new project folder there with `git init`. As in T3 Code, a repository is known by
  its remote (`host/owner/repo`), so switching machine, or "Continue on <machine>" on a thread
  (the handoff carries the context), finds the same repository's clone there, or offers to bring it
  into that machine's projects folder. Bringing (`carry.rs`, `POST /api/folders/bring`) works as in
  T3 Code: the machine the project is on packs its branches and tags into a `git bundle` with the
  ignored `.env`, `.env.*`, `.envrc` and `.dev.vars` files (1 MiB each, 4 MiB in all, none from
  `node_modules` and the like; `POST /api/folders/pack`), the core the app talks to carries the pack
  over, and the other machine clones from the bundle, checks out the same branch and points origin
  at the same remote (`POST /api/folders/unpack`). It needs no GitHub login there. Work that is too big to
  send as files or as a patch (a folder with no commits and a lot in it, or uncommitted changes over 20 MB) is never
  handed back as "commit it first": Hub saves it as one commit "Hub checkpoint: where we left off" on the branch it
  is on (`carry::checkpoint`; the user's own git name if set, Hub's otherwise, nothing pushed, `node_modules`, `target`
  and the like kept out through `.git/info/exclude`, never the project's `.gitignore`) and carries that as history. A bundle over
  40 MiB is not carried, and the other machine clones the remote instead. Uncommitted changes stay
  where they were, and the composer says how many. A clone that is already there only gets the env
  files it lacks ("Bring .env files"), and a file already there is never overwritten.
  A repository with no remote (a project just made with New project) has no key, so the picker
  knows it by where it is, and Continue on <machine> offers to bring it rather than finding a
  clone. With commits it comes as a bundle with no origin; with none yet,
  its files that git does not ignore come along as they are (same limits) into a fresh `git init`
  on the same branch (`via: "files"`).

  **Move to <machine>** (thread menu, `moving.rs`) carries on the same thread over there, with its
  uncommitted work. The core the app talks to orchestrates (`POST /api/threads/move`, either end may
  be itself): the machine the thread is on **exports** it (`POST /api/threads/export`: a running turn
  is stopped when the person chose to, since the menu says "stops this turn"; the project is packed
  as above plus `git diff --binary HEAD` and the files git does not track, 20 MiB at most, and the
  whole event log), the other machine **imports** it (`POST /api/threads/import`: finds its clone of
  the repository by remote or clones one under the projects folder, fast-forwards the branch to the
  commit and applies the changes; a clone with uncommitted changes of its own is refused, never
  mixed), and then the first **steps aside** (`POST /api/threads/stepped-aside`): a `moved_away`
  event, archived, and `kv moved:<id>` which `Store::threads` leaves out, so lists show the thread
  once, where it is. The new copy keeps the id and log and starts with a `moved` event. The engine's
  own session cannot travel, so the thread's first message there hands the conversation to a fresh
  session as after an engine switch (`moved_note`); a turn the move cut off carries on by itself like
  after a restart (a `restart` with `by: "move"`), and questions open at the time are closed.
  A folder that is not a repository (a Scratch thread's) moves too, as its plain files (20 MiB at
  most, no `node_modules` and the like, `plain` in the pack): it lands in `scratch` under the
  projects folder there and is never made a repository. Moving back is the same move the other
  way, into the folder the thread had there before (`resume`): that folder is not "other files"
  any more, the moved files are laid over it, and changes it still held from when the thread left
  are put in a stash ("Hub: what was here before the thread came back"), not lost. While it moves
  the app changes nothing on screen (`hub.moveThread`): the thread's row and chat are held as they
  were (`holding`: its `thread` and event messages and the old machine's "gone" are not applied,
  and a resync keeps its row), so it does not drop out of the inbox, the open chat is not
  re-drawn and you are not sent Home. When the move is done the core's list is read once and
  only the machine on the row changes; the chat keeps its log and reads what the move added. **Leaving** (`POST /api/threads/leaving`, command
  palette: "Leaving: move my running threads to <machine>") moves every thread that is working or
  waiting here, stopping their turns. Not carried: side jobs (they stay and report to nowhere), a
  project too big to bundle that is not pushed, a repository with no commits, messages written to the
  thread while it is being packed.

  Each machine's page says whether it is ready for work (`ready.rs`, `GET /api/machine/ready`):
  every configured engine's command found and its login (from `prefs::accounts`), git with a name
  and email (read and set in `git_identity.rs`, which an engine's share step uses too), room on the disk, and on Windows a PowerShell that runs scripts (a row only when its execution policy refuses them, since npm and npx then fail for an agent that works in PowerShell). A check that is not fine carries its fix: sign in opens the
  engine's CLI there, git gets the PC's name and email in one click
  (`POST /api/machine/git-identity`), and anything else opens a terminal there with what to do.

Hosts come from `hosts` in `hub.json` (`{ name, ssh?, url?, port?, token? }`) and from Settings >
Machines, which stores them in the database and offers every `Host` block in `~/.ssh/config` once
(`Host tincan 192.168.0.174` is one machine with two names), leaving out blocks that go to the same
HostName as a machine already added, and marks two added machines that do.

When SSH gets in but hubd does not answer, Hub asks the host over SSH what it has: no hubd, an
install running (the host shows the installer's latest line), an install that stopped (its last
line), or hubd installed but down (then `journalctl` says why). With no hubd, Settings > Machines
offers Install hubd (`POST /api/hosts/{name}/install`). The core sends `install-host.sh` (built
into hubd) over SSH and asks for `uname -m`. For a host it can build for (x86_64, aarch64,
armv7l) it asks once, for all hosts, whether it may (`InstallAnswer.build`, remembered in `kv`
as `build_for_hosts`); see "Building hubd for other machines" below. Then it runs
`install-host.sh --check`, which lists everything the machine lacks
at once: apt packages (`package git`), lingering (`linger`), whether root is at hand without a
password (`sudo`), and what a person has to fix (`problem …`, such as no systemd or too little
disk), which fails the install at once with all of it. When packages or lingering need root and
sudo wants a password, nothing starts: the answer (`InstallAnswer.sudo`) says what for, Settings
asks for the password once, and Install again checks it with `sudo -v` and feeds it to
`install-host.sh --root USER PKG…` on stdin. It is never stored or logged. Then the core builds
hubd for the host and sends it (`HUB_PREBUILT`), or, when the host builds for itself, sends a git
bundle of its own clone's branch, so a private repository needs no login there. It starts the
script detached with its pid in `install.pid` and its output in `install.log` in Hub's folder
there. While the core builds, the host's row says so; when that fails, the reason goes to the end
of the host's `install.log`, so the row shows it like any other stop.

The script runs in seven steps, each printed as `Step N/7: …` so the host's row shows where it is:
check, Rust (rustup-init from static.rust-lang.org, checksummed, in the home folder), Node.js 22
(checksummed, from nodejs.org, in Hub's folder, when there is no npx from Node 20 or newer), the
source (from the bundle, or the repository when the bundle does not take), the build (one job per
1.2 GB of memory and swap, so a Raspberry Pi does not run out), the service, and pairing. With a
hubd from the core, Rust and the source are skipped, the build is a copy, and the check asks for
neither git nor a compiler. Every
step skips what is done, so running it again after a failure carries on where it stopped (the
host's row then offers Continue install), and downloads try five times with a growing pause.
Each step is timed: a step over 5 s says how long it took, and the script ends (also on failure)
with the time per step. The engines' npm installs print each download (`--loglevel=http`), so with no
terminal the machine's latest line keeps moving instead of looking hung.
`scripts/test-install-host.sh` runs it from nothing in fresh Debian and Ubuntu containers through
every path above; run it by hand after changing the installer (Hub has no GitHub CI: its Actions minutes run out). This machine
is called `name`, or "PC" on Windows and the hostname elsewhere. `scripts/install-host.sh` installs
`hubd` on a Linux machine as a systemd user service: it follows `main` like the PC, and after an
update's handover systemd restarts the service into the version `current` names.

On a Mac (a macOS peer on the tailnet gets Set up too) the same script, in its one sudo step,
installs the Xcode Command Line Tools through `softwareupdate`, turns off sleep on the charger and
runs hubd as the launchd daemon `/Library/LaunchDaemons/se.karlsson.hubd.plist` with `UserName`,
so it starts at boot without a login and KeepAlive starts each new version; without sudo it falls
back to a launchd agent that runs while you are logged in. A Mac always builds its own hubd
(`crossbuild::target` gives no build for `Darwin`, which the PC learns from `uname -sm`), and Hub
never runs Tailscale there: its command line cannot reach the Mac app from SSH (error -600), so the
PC reaches a Mac's hubd through the SSH tunnel. Remote commands avoid bash-only word splitting,
since a Mac logs in with zsh. Every machine also gets the engines' command lines (Claude Code,
Codex) in `<data>/tools`, for signing in there.

### Pairing
<!-- covers: crates/hub-core/src/pairing.rs, crates/hub-core/src/identity.rs -->

The idea is T3 Code's (github.com/pingdotgg/t3code, MIT): every machine runs the same server, the
route to it is only a route, and a device gets in with a one-time code instead of a copied token
(`crates/hub-core/src/pairing.rs`).

1. `hubd pair` on the machine (or Settings > Machines > Make a code) asks the running core for a
   code like `5J9C-XZNU`: it works once, for ten minutes; at most five wait at once (wrong codes:
   see step 3).
2. `hubd pair --tailscale` first runs `tailscale serve --bg --https=443 http://127.0.0.1:<port>`,
   so Tailscale serves hubd over HTTPS with a real certificate to the tailnet, and prints that
   address. It stays across restarts. hubd itself still binds to loopback only: Tailscale's
   requests arrive there and need a token like any other. On Linux, changing what Tailscale serves
   needs `sudo tailscale set --operator=$USER` once; `install-host.sh` does both.
3. The device that wants in posts the code to `/api/pair/redeem` (open, like `/api/health`) and
   gets a token of its own. Adding a machine by URL with a code does this for the PC's core.
   Wrong codes are counted per sender (`phone::Sender`: the peer, or the device `tailscale serve`
   names last in `X-Forwarded-For`): ten in ten minutes and that sender is refused, right code or
   not, while everyone else's codes keep working. A hundred from everyone together drop every
   waiting code.
4. Only a SHA-256 of each device's token is stored (`devices` in the kv table). Settings >
   Machines lists the devices, when they last called and a Revoke that stops the token at once.

A device's token can do what the machine's own token can; narrower grants (read only, no
terminal) are for when the phone arrives.

Every machine also has an id of its own (`crates/hub-core/src/identity.rs`, `<data>/machine-id`),
made once and kept, so a new name, a new address or a second SSH alias is still the same machine.
`/api/health` names it (`machine`, `name`) without a token, so a client checks who answers at an
address before it shows that address a token, and `GET /api/machine/routes` says where Tailscale
serves the machine. This is the start of clients that reach every machine themselves, as in T3
Code, instead of through the PC's core: see "The phone" for the first of them.

A computer that should show the same Hub as another machine, such as a laptop or a work
computer, can use that machine's Hub instead of running its own (Settings > About > Use the Hub on
another machine; `app/src/lib/home.ts`). It takes the machine's address (`pc.<tailnet>.ts.net`)
and a code made there (Settings > Machines > Make a code, which now also turns on Tailscale serve),
redeems the code for a device token and keeps both in the window's storage. Every request to that
address then carries the token, and the stream carries it as `?token=`; the token is shown only
once `/api/health` there has named the machine id it was paired with, so a different machine that
answers at that address later gets nothing. The desktop shell then starts no core of its own
(`<data>/app-uses-another-hub`, `set_local_core` in `app/src-tauri`); Use this computer's own Hub
goes back. Everything the window does happens on that machine, so its threads, projects, ideas and
settings are the same from every computer. The machine to pick is one that stays on: while it
sleeps, the window waits for it.

In the app every thread on another machine has a host chip (icon and name, grey when offline,
never colour), the composer on Home has "Run on", project pages are per folder and machine, and
the sidebar can be filtered by machine. All of it only shows once there is a second machine.

## Thread status from events
<!-- covers: crates/hub-core/src/model.rs, crates/hub-core/src/model/event_data.rs, crates/hub-core/src/model/event_data/, crates/hub-core/tests/it/golden_log.rs, crates/hub-core/tests/it/golden_reads.rs, crates/hub-core/src/status.rs, crates/hub-core/src/status_shadow.rs, app/src/lib/components/ProblemsView.svelte, crates/hub-core/tests/compat/frozen/event-kinds.txt, app/src/lib/events.ts, ios/Hub/EventPayloads.swift -->

Plan: `/mnt/project-files/teknisk-skuld/d3-status-fran-handelser-plan.md` (D3). The goal is that the log is the truth and `threads.status` a cache written in the same transaction as the event. The model part is in place (additive, nothing reads it yet):

**Typed payloads** (`model/event_data.rs`, one file per group of kinds in `model/event_data/`). The log stores `data` as JSON; every kind has a struct that describes what is in it (`turns.rs`: the five kinds every turn writes and `update`, which is the engine's own JSON kept whole as `UpdateData`, with `update_kind(data)` to say which session update it is without copying it, `asks.rs`: `permission_resolved`, `permission_auto`, `slept`, `status`, `queue`, `paused`, `restart`, and `session.rs`: `session_started`, `session_state`, `setting`, `quota_switch`, `quota_wait`, `engine_switch`, `sign_in_lost`, with `unattended` as the `Unattended` it is also asked with (read strictly), and `history.rs`: `checkpoint`, `restored`, `rewind`, `rule_saved`, `attempt` (which writes itself, because its two shapes order `root` and `cwd` differently), `terminal`, `moved`, `moved_away`, and `checks.rs`: `clash`, `leak`, `customs`, `proof`, `docs`, and `judged.rs`: `review`, `design` (three shapes in one struct, every field optional), `task` (which writes itself, because its start and end order `state` and `by` differently), `focus`, `reading`, and `shown.rs`: `page`, `replies`, `ask`, `plugin_suggested` (its `repo` stays the JSON of the `PluginRepo` of that day, so an older card still reads), `reaction`, `gist`, `output`, `desk_note`, `reported`, and `delegated.rs`: `child`, `auto` (no Hub writes it any more), `handoff_offer`, `handed_off`, `handoff_result`, `worker_run` (the `WorkerRun` flattened, read strictly as it always was, plus `lines`); `devlog.rs` still counts finished reviews and side jobs from the raw `state`; `capabilities.rs` and `engine_models.rs` still walk the engine's own option JSON inside the latest `session_state` rows (`store.latest_session_states`), which the struct keeps as JSON). Writers build the struct and call `Hub::emit_typed`; readers call `Event::data_as::<T>()`, which is `None` for another kind or a payload that is not an object. Code that looks at one key of many events of the five turn kinds (the queue, the status, who answered what) does not copy each payload: `turns.rs` gives `Event` borrowing reads (`said()`, `mark(key)`, `has(key)`, `prompt()`, `stop_reason()`, `ended_well()`, `cancelled()`, `handed_over()`, `failure()`, `leaves()`, `detail()`, `request_id()`, `tool_call()`, `options()`), so code with an `Event` in hand does not write those key names itself (SQL still does: `json_extract`, and the rows `problems/tools.rs` and `store/events.rs` read as kind and JSON). What is still indexed as JSON outside the model is the engine's own: the fields of an `update`, and what is inside `tool_call` and `options`. The rules: every field an older log may lack is `Option` or `#[serde(default)]`; every field that can fail to read uses `deserialize_with = "lenient"`, so a value of the wrong type is that field's default and the rest still reads (what `data["x"].as_str()` gave, so a reader can move from JSON to the struct without getting stricter); unknown fields are ignored, `None` is not written (a field the old code wrote as an explicit `null`, like `permission_resolved.option_id`, is an `Option<Option<T>>` read with `nullable`, so missing, null and a value each come back as written), and a field is only added, never renamed or retyped. Key order is kept too: `serde_json` runs with `preserve_order` (set in the workspace `Cargo.toml`; the ACP crate needs it too), so a struct writes its fields in declaration order, and they are declared in the order the old `json!` wrote them. The registry at the end of `event_data.rs` (`event_kinds!`, one row per kind) writes the `EventData` impl, the TypeScript map `EventPayloads` (payload type by kind name, for the app) and `rewritten`, which the golden log uses. `tests/it/golden_log.rs` reads `tests/it/frozen/golden_events.jsonl` (payloads as older versions wrote them) into the structs and back and compares the text; add a line when a new shape appears, never edit a line a released Hub wrote (builds between #973 and #981 wrote `turn_end` with `prompt` before `forced` and `permission_request` with `id` first: those read, but do not come back as the same text; the same goes for `review` events from before 2026-10-30, which had no `head`, `seq`, `by` or `chunks`). A new kind needs a struct, a registry row and a golden line: `golden_log::every_kind_has_a_struct_and_a_golden_line` fails without them (`scripts/code-health.py` reports `event_kinds_without_struct`, now 0). What a struct reads from an odd payload (a field missing, `null` or mistyped, an older shape) is pinned in `tests/it/golden_reads.rs`. The app reads the log through the same types: `app/src/lib/events.ts` has `typed(e)`, which gives an event's kind and payload as one pair so a `switch` on the kind knows the payload (`transcript.ts` folds with it), and `payload(e, "kind")` for a reader of one kind. The log is not checked in the app, so a reader keeps its fallback where it had one; the fields of an `update`, a `user_message`'s marks and the engine's `tool_call` / `options` are still read as JSON there. The iPhone app has no generated types: `ios/Hub/EventPayloads.swift` has one view per kind the app reads (`ChildPayload`, `QuotaWaitPayload`, ...), each naming the fields of that kind's Rust struct that the app uses, so the log's field names are written in that one Swift file (a `user_message`'s mark names are still given to `mark(_:)` where they are read); `ios/tests/ModelTests.swift` (`payloads`) reads lines shaped like the golden log through them.

- `EventKind::Status { status, why }` marks a status change that no engine event carries: a terminal taken or handed back, a cloud session changed, a queued message taken back, an engine stopped, the start-up repair. Older clients read it as `unknown` and skip it.
- `Error` events may carry `leaves` (`"idle"` or `"error"`): the status the failure leaves the thread in. Without it (every log written before) the old rule applies: an error with a `prompt` leaves the thread idle, one without leaves it in error, except `ENGINE_GONE`.

**Shadow mode (step 3, no change of behaviour).** `status::Tracker` follows each thread's events as `Hub::emit` logs them (`status_shadow.rs`). Four seconds after the last event that moved it, the status it implies is compared with `threads.status`. A difference is logged (`status shadow disagreed`), counted and kept (the latest 20, in `kv` under `status.shadow`); the tracker then follows the real status so one miss counts once. A thread starts being followed at its first event after the core starts, and is compared from its next change. `GET /api/status-shadow` answers the counts and misses, `DELETE` clears them (feature `status.shadow`); Problems shows a small card (`ProblemsView.svelte`, only when there are misses, with Clear) when there are misses. 

**Markers beside `set_status` (step 4).** `Hub::set_status_because(thread, status, why)` logs a `Status { status, why }` marker (only when the status really changes) and then calls `set_status`, for every change no engine event carries: `terminal` / `terminal_back`, `move_stopped` / `move_carried_on`, `restart` (carry-on resets), `engine_taken_over`, `message_taken_back`, `engine_ended`, `engine_stopped` (`stop_engines`, which emits the marker and keeps its direct write), `cloud_prompt` / `cloud_done` / `cloud_waiting` / `cloud_working` / `cloud_stopped` / `cloud_following_stopped`, and `core_started` (the start-up repair in `Store::init`, written in the same transaction as the update). `next_status` and `Tracker` read a marker as "the status is now `status`", in every state (also out of `in_terminal`). Still nothing reads the markers to decide the status: the field stays the truth until step 5. Step 5 (event and status in one transaction) starts only when the shadow card has stayed empty for some days of real use.

## What a thread shows: project and display word
<!-- covers: crates/hub-core/src/thread_display.rs, crates/hub-core/src/thread_owner.rs, crates/hub-core/src/model.rs, app/src/lib/threadFlags.ts -->

AI-friendly step 4 (`/mnt/project-files/teknisk-skuld/ai-vanlig-kod-plan.md`): a thing derived from a thread is worked out once, on the server, and clients read the field.

- `Thread.project`: the folder of the project the thread belongs to, from `thread_display::project_root(cwd)`. A thread's own worktree (`<repo>/.hub-threads/<id>`, `<repo>/.git/hub-attempts/<n>`) is its repository, a quick thread's `scratch/<date-time>` and a chat's `chats/<date-time>` folder are the `scratch` and `chats` folders, an Overview widget's folder is `widgets`. Pure text, either slash. Registered projects and the Claude app's folders are not known to it: `stats.rs` (engine knowledge) and `project.rs` start from its answer.
- `Thread.display`: one word for the state: `waiting`, `error`, `asks` (idle, the last reply asks or got stuck), `running` (also in a terminal), `unread`, `idle`; from `thread_display::display_status`. A client reading a word it does not know shows the thread as idle. Snooze and settle stay separate fields, they depend on the clock and the user.
- Both are never stored. `store.rs` fills them as it reads a thread (`thread_from_row`), and `Remotes::threads` fills them for a thread a hubd without them relayed. Feature `threads.display` in `compat::FEATURES` says a hubd sends them; a client that finds them missing derives them from `cwd` and `status` as before (`app/src/lib/format.ts` `threadProject`, `threadFlags.ts` `needsYou`/`asksYou`). The app reads `Thread.project` through `threadProject(t)`; iOS and the phone page still derive their own (next steps).
- `now.rs`, `problems/tools.rs` and `stats.rs` call `project_root` instead of their own copies. Do not write a fourth: add the case to `project_root`.

## Versions that talk to each other
<!-- covers: crates/hub-core/src/compat.rs, crates/hub-core/tests/it/compat.rs, crates/hub-core/tests/compat/, crates/hub-core/tests/it/ios_contract.rs, ios/tests/ContractTests.swift, app/src/lib/compat.ts -->

Hub is never one version. The PC updates when Christoffer releases, the other machines follow
later, and the iPhone app, a window on another machine's Hub and a core relaying another host's
threads each run a build of their own. The rule: **a thread list, the live stream, a thread's
events, sending, stopping and answering a permission work between any two versions.** Only a
brand-new feature may be missing on the older side, and then it is hidden or disabled where it
would be used. Nothing says "needs updating" for the whole app, and nothing breaks because the
other side is newer or older.

For whoever changes the API (the rules, in order of how often they matter):

1. **Add, never change.** A new field is optional (`#[serde(default)]`, `Option`) and a client
   copes without it. A field is never removed, renamed or given another type, and a value is never
   given another meaning (the same rule as the event log). If the shape has to change, add a new
   field or a new call next to the old one and keep sending the old one.
2. **New calls are features.** Add a line to `compat::FEATURES` with the calls it brings, in the
   same change. `/api/health` (open, no token) lists the names, next to `core` and the old
   `protocol`. `protocol` is fixed at 1 and nothing reads it; `core` does not move either.
   A phone feature's calls must also be in `phone::may`, and `phone::may` only grows.
3. **Clients ask, they do not try.** A client reads `/api/health` once per machine. No `features`
   key is a hubd from before the list: it has the core and nothing else is assumed. It shows a
   feature's entry points only when the name is listed, and where it is not it says once, beside
   that one feature, that the machine needs a newer Hub. A 403 is not "too old": the hubd knows
   the call and refuses this client (a phone asking for more than it may), so it is a real error;
   ask `hubdHas(feature)` first instead of reading the feature's absence from a failure. A 404 or 405 on an optional call is
   the same answer (`tooOld`), never an error page and never a reason to stop showing threads.
4. **Read what you do not know.** Unknown fields are ignored (no `deny_unknown_fields` on anything
   that crosses machines). An unknown status, event kind or owner becomes the nearest known thing
   (`EventKind::Unknown`, idle, no owner) and the rest of the payload is still used. A list is read
   one element at a time, so one unreadable thread never empties the inbox, and a stream message
   of a type the client lacks is skipped. In Rust that is `compat::thread_from_wire`,
   `threads_from_wire` and `stream_from_wire`, used wherever one core reads another's threads
   (`remote/link.rs`); in Swift the same by `decodeIfPresent`, `unknown` cases and per-element decoding;
   in the app `ApiError.tooOld` and `hubd.has("feature")` (`app/src/lib/compat.ts`).
5. **The core is frozen.** `compat::CORE_CALLS` is the list a client may rely on for good (health,
   threads, one thread, events, stream, messages, cancel, permission answers). Calls and the keys
   they return are only added to.

6. **Thirty days behind.** A client or machine is supported until it is 30 days older than the
   build that replaced what it uses (the iPhone app must therefore be updated at least monthly).
   A fallback kept only for older clients carries a `compat-until: YYYY-MM-DD` comment (change date
   plus 30; every `hubdHas(...)` call in `app/src` carries one on its line, and `scripts/garden.py` lists the ones that do not); once the date has passed, whoever next touches the file deletes the fallback. This
   removes fallbacks, never API: calls and keys stay (rules 1 and 5), and removal of a call still goes
   through a deprecation that clients can see in `/api/health` first.

`crates/hub-core/tests/it/compat.rs` keeps it so. `tests/compat/v1/` is what hubds in the field send:
the build must read all of it, also with an unknown field added in every object.
The same test file asks the real router (a request with a method no route uses, so no handler runs)
whether every `CORE_CALLS` and `FEATURES` call is a route, so a misspelt path fails there and not as a
404 on the phone.
`tests/compat/frozen/` lists the keys (with types), thread statuses, event kinds, stream message
types, core calls and feature names that clients in the field rely on; the test fails if one is
removed or retyped. Adding to a frozen file is fine and expected: add the new key, status or call
there when you add it. When the test fails because you changed one, the change is the problem, not
the test.

The iPhone app has a contract test of its own. `tests/it/ios_contract.rs` scans `ios/Hub/*.swift` for every call
the app makes (`call("GET", "api/…")`, the few URLs built by hand) and checks each is a route in the real router
and one a phone's token may make (`phone::may`; pairing and web-view pages have their own rules and are left out
by name). A renamed route or a dropped allowance fails there, from the cloud, without a Mac. The decoding half is
`ios/tests/ContractTests.swift`: it reads `tests/compat/v1/*.json` with `HubThread`, `Lossy` and `HubEvent`, plain
and with an unknown field in every object. It needs a Mac (built as Mac Catalyst with the whole app target; run command in its header; passed on the Intel MacBook 2026-11-07) and is not in any CI yet.

Where it applies today: the iPhone app and the phone page against the machine they are paired
with; the PC core against the hubd on each remote machine (`remote/link.rs` and `remote.rs`: the thread list, the
stream and `TooOld` for a call an older hubd lacks); a window using another machine's Hub
(`home.ts`). A change to a type in `model.rs` that a client reads should come with a look at which
of those it reaches.

## API
<!-- covers: crates/hub-core/src/api.rs, crates/hub-core/src/error_code.rs, crates/hub-core/src/api/, crates/hub-core/tests/it/api_routes.rs -->
The router in `api.rs` is grouped by area (threads, projects, phone, machines, github, engines, keys, usage, bench: `api/bench.rs`, the runs of `bench/run.rs`); the area files in `api/` hold the routes moved out so far (`github.rs`: pull requests, issues, the updater, the dev setup and the retired merge queue route). `tests/it/api_routes.rs` locks every `.route(` call against `tests/it/frozen/routes.txt`: a new route needs a line there.

<!-- covers: crates/hub-core/tests/it/rules.rs, crates/hub-core/tests/it/clippy.rs, crates/hub-core/tests/it/ratchet.rs, crates/hub-core/tests/it/test_code.rs -->
**Rule ratchets.** `tests/it/rules.rs` turns the CLAUDE.md rules a grep can check into tests with a frozen list (`tests/it/frozen/rules.txt`, one `rule path size` line per file, so a reader sees where the debt is): `engine` (lines naming claude, codex, hermes or `CLAUDE_` in non-test Rust outside `ENGINE_FILES`), `controls` (`<select`, checkbox, radio, file, date, range inputs in `.svelte` outside `Select.svelte`), `header` (a Rust file with no `//!` line in its first 30 lines, a `.svelte` file that does not open with a `<!--` comment) and the habits in `HABITS`, each with a helper that owns it (non-test code only, the owner file is exempt): `lock_unwrap` (`.lock().unwrap()`, use `.locked()`), `empty_catch` (`catch(() => {})`, use `quiet()`), `set_interval` (use `poll()` in `poll.ts`), `whole_log` (`.events(.., 1_000_000)` over the whole log), `own_clip` (a new `fn clip*`, use `text.rs`) `raw_git` (`Command::new("git")` or `command("git"` outside `git.rs`), `unwrap` (a bare `.unwrap()`; use `?`, `unwrap_or*` or `.expect("why it cannot fail")`) and `let_discard` (a bare `let _ =`, or an `.ignore("")` with no reason; use `.warn_on_err(what)` for writes and events whose failure somebody should see, `.ignore("why")` (`Ignore` in `locked.rs`) where the failure is expected, with a reason specific to the site, and `clean_file` / `clean_dir` for clean-up of a temp file). The ratchet is the total per rule across the workspace, not the line per file (`tests/it/ratchet.rs`, shared with the clippy guard and unit-tested there): it fails when a rule's total grew (sum of sizes; a `header` line counts one; a rule missing from the list has total 0) and names the files that rose or are new. Moving a case between files, with the total the same or lower, passes without a bless, so a function with a `let _ =` or a raw `git` call can move to the file it belongs in. `RULES_BLESS=1 cargo test -p hub-core --test it rules::` rewrites the lines to what is true now after a clean-up or a move, and is refused (writes nothing) while any total grew; an exception is a line added by hand, which raises its rule's total. Test code is skipped item by item, not from a marker to the end of the file: `tests/it/test_code.rs` (shared with `layers::` and `no_window::`, unit-tested there) hides the item under a `#[cfg(test)]` or `#[cfg(all(test, ..))]` that opens a line (a `mod tests;` or `use ..;` to its `;`, an inline module, `fn` or `impl` to its matching `}`, a field or arm to its `,`; braces in strings, chars and comments do not count) and reads the code after it again, so `#[cfg(test)] mod tests;` may sit anywhere in a file; `#[cfg(not(test))]` is code. Whole test files (`tests.rs`, `*_tests.rs`) are skipped. `let_discard` also counts a `let _ =` that follows `{` or `;` on a line (`if x { let _ = f(); }`).

<!-- covers: crates/hub-core/src/source_tree.rs -->
**Which checkout a test reads.** Tests that read Hub's own files (these ratchets, `layers`, `map`, `covers`, `agents_mirror`, `api_routes`, fixtures) find the checkout with `source_tree::root()` / `source_tree::hub_core()` at run time, from the working folder cargo gives a test (its package folder), never `env!("CARGO_MANIFEST_DIR")`: where checkouts share one build folder (`CARGO_TARGET_DIR`, as on Christoffer's PC), cargo reuses a test binary built in one worktree for another of the same commit, and the baked-in path made `rules::` scan, and a bless write, the other worktree. Only `prefs::source_dir` keeps `env!` (a dev build's own checkout). No test may `set_current_dir`.

**Clippy ratchet.** `tests/it/clippy.rs` does the same for `cargo clippy` (workspace without `hub-app`, all targets): `tests/it/frozen/clippy.txt` holds the warnings as `lint path count` lines, and the total per lint across files may only get fewer (`ratchet.rs`): a lint whose total grew fails and names the files that rose or are new; a warning that moves with its code passes. It runs clippy, so it only runs when asked: `HUB_CLIPPY=1 cargo test -p hub-core --test it clippy::` (Linux only; code behind `cfg(windows)` is not seen there). `HUB_CLIPPY=1 CLIPPY_BLESS=1` after a clean-up rewrites the lines and is refused while any lint's total grew; the list is only written on Linux.

<!-- covers: crates/hub-core/tests/it/layers.rs, crates/hub-core/src/ctx.rs -->
**Layers in hub-core.** `tests/it/layers.rs` reads every `crate::x` in `src/` (test code left out: each item under a `#[cfg(test)]`, wherever it sits (`tests/it/test_code.rs`), and the `tests.rs` files a module takes in with `#[cfg(test)] mod tests;`) and ranks the modules in `LAYERS`, lowest first: helpers (`proc`, `locked`, `text`, `git`, `home_files`: the home folder and reading a small JSON file there, `project_folder`: plain facts about a project folder, `http`: the one shared HTTP client with Hub's user agent), `model` and `config`, `store` and `ctx`, infrastructure (`secrets`, `auth`, `engine_home`) and the data half of a feature whose `impl Hub` half lives under `hub/` (`reach`, `reactions`, `recipes`, `focus`: `hub/reach.rs` and so on) and the settings Hub reads when it starts work (`projects_root`, `handoff`), the platform (`engine`, `mcp`, `hub`); every other module is above them. A lower module naming a higher one is an upward edge and must be listed in `tests/it/frozen/layers.txt`. The list holds the 110 edges that existed when the two top layers were added (most are the platform and infrastructure naming function modules); it may only shrink: a new upward edge fails the test, and taking one away leaves a stale line that `LAYERS_BLESS=1 cargo test -p hub-core --test it layers::` tidies. There is no `http` module yet (the 19 own HTTP clients are open work). It is the first step towards splitting the crate (`hub-util`, `hub-model`, `hub-store`).

**Ctx.** `ctx.rs` has `Ctx { store, config }`, borrowed from the Hub with `hub.ctx()`. New function code that only needs the database and the settings takes `&Ctx`, not `&Hub`, so it stays callable without a running core and can sit low in the layers; code that is touched anyway moves over (`health.rs` is the first). The rule is in CLAUDE.md.

**The ring.** The same test file finds the modules that depend on each other in a circle (strongly connected components of the `crate::x` edges with more than one module) and holds them against `tests/it/frozen/ring.txt`, one module per line: 92 when the list was written, now `engine` and `hub`, which belong together. A module not on the list that ends up in a ring fails `no_module_joins_the_ring`; one that leaves is dropped from the list by the same `LAYERS_BLESS=1`, which never adds a line once the list exists. A module leaves by no longer naming a ring module: take the one value it needs as an argument (the store, a path) in place of the whole Hub, or move the shared helper to a lower module. Hiding the import (a re-export through `lib.rs`, `super::`) is not removing the edge; the test only reads `crate::<module>` and cannot see that, so the reviewer looks for it. A ring module that is renamed gets its line changed by hand, in the same change. The plan and order: `teknisk-skuld/vecka4-delsystem-och-ring/plan.md` in the project folder.

A handler's `anyhow` error becomes an `ApiError` through `status_of`: a database (`rusqlite`) error in the chain is 500 and logged with `tracing::error!`, everything else stays 400. The body is `{ "error": text, "code": kind }`; `code` (`error_code.rs`) is one of `not_found`, `conflict`, `refused`, `upstream`, `internal` and is new: a body without it comes from an older hubd, so a client treats a missing or unknown code as "unknown". The kind comes from the error's chain (`ErrorCode::of_error`: database → internal, `ThreadGone` → not_found, keeper busy → conflict, another machine or HTTP failure → upstream, anything else refused) or from the status of a hand-made `ApiError::new`. The status itself is untouched (clients branch on 404/405), so older apps and phones are unaffected. New code that fails in a way a client should tell apart adds a typed error with a case in `of_error`, not another text to compare.

| Method | Path | |
|---|---|---|
| GET | `/api/health` | `{ ok, version, protocol, core, features, schema, machine, name }`, without a token; `features` names what this hubd has beyond the core (`compat::FEATURES`) |
| GET | `/api/package/slips` | What was sent to an AI, newest first (`thread`, `purpose`, `before`, `limit`); feature `package.ledger` |
| GET | `/api/package/summary` | `{ days: [{ day, purposes: [{ purpose, calls, tokens }] }] }` for the last `days`; feature `package.ledger` |
| POST | `/api/threads/{id}/react` | `{ seq, emoji, on? }`: put your reaction on a message, or take it away (`on: false`); feature `chat.reactions` |
| GET | `/api/emoji` | `{ custom: [{ name, mime, data }], quick }`: your own emojis with their pictures, and your most used; feature `chat.reactions` |
| PUT / DELETE | `/api/emoji/{name}` | `{ mime, data }`: keep one of your own emojis (base64 picture); or remove it; feature `chat.reactions` |
| GET | `/api/version` | The updater's state: the running version, its state and the version being built or ready to start |
| POST | `/api/update/install` | Install the version that is ready; it switches over once no thread is working |
| POST | `/api/update/retry` | Forget the version that failed, so the updater tries it again now |
| GET | `/api/secrets` | Secrets Hub keeps: name and last four characters, never the value |
| PUT | `/api/secrets/{name}` | `{ value }`: keep a secret (Windows Credential Manager) |
| DELETE | `/api/secrets/{name}` | |
| GET / POST | `/api/keys` | The key vault: each key by its last four characters and what uses it; POST `{ name, value }` adds one |
| PUT / DELETE | `/api/keys/{name}` | `{ value }`: rotate a key, keeping its name; or remove it |
| GET | `/api/apis` | API connections, the catalog, and per connection the key's hint, calls and tokens today and this month, the last error |
| PUT | `/api/apis` | `ApiPrefs`: the connections, their projects and daily caps, the `helper` for Hub's own decisions and the features `off` |
| POST | `/api/apis/{name}/test` | Calls the connection's test path with its key: ok, message, ms, models |
| POST | `/api/apis/{name}/try` | `{ state, statement }`: one yes/no question, answered |
| GET | `/api/apis/decisions` | What a connection said next to what Hub decided, newest first |
| GET | `/api/threads` | All threads, newest activity first |
| POST | `/api/threads` | `{ cwd, title?, engine?, message?, attachments?, settings?, attempts? }`; with `attempts`, one thread per attempt in its own worktree |
| GET | `/api/github/repos` | `?fresh=`: `GithubRepos`, every repository of the account `gh` is signed in as on this machine (feature `github.repos`) |
| GET / POST / DELETE | `/api/github/connect` | `GithubConnect`: where this machine stands with GitHub; POST starts `gh auth login --web` and answers with the code to type at github.com/login/device, DELETE gives it up |
| GET | `/api/github/pulls?path=&state=` | Pull requests: open, merged or closed |
| GET | `/api/github/pull?path=&number=` | One pull request with checks, conversation, commits and files |
| GET | `/api/github/diff?path=&number=` | Its diff, as `{ text }` |
| POST | `/api/github/action` | `{ path, number, action, method?, body?, delete_branch? }`: merge, auto_merge, cancel_auto_merge, comment, approve, request_changes, ready, draft, close, reopen, update_branch, checkout, delete_branch |
| GET | `/api/github/issues?path=&state=` · `/api/github/issue?path=&number=` | Issues, and one issue |
| POST | `/api/github/issue` | `{ path, number, action, title?, body? }`: create (answers the new number), comment, close, reopen |
| GET / DELETE | `/api/github/branches` | Branches here and on GitHub; DELETE `{ path, names, local, remote }` answers what could not go |
| GET | `/api/github/card?path=&repo=&number=` | The hover card of a pull request: state, title, author, +/−, files, checks (`repo` is `owner/name` when not the project's own) |
| GET | `/api/github/watches` | The pull request each thread watches |
| POST / DELETE | `/api/threads/{id}/watch` | `{ number? }`: watch it for the thread (the branch's pull request when left out), or stop |
| GET | `/api/threads/{id}/gitstate` · `/api/threads/{id}/branches` | Where the folder stands for the Git button; its local branches |
| POST | `/api/threads/{id}/switch` | `{ branch, create? }` |
| POST | `/api/threads/{id}/ship` | `{ commit?, push?, pr?, pull?, message?, title?, body?, draft? }`, in that order |
| POST | `/api/threads/{id}/gitdraft` | `{ message, title, body }` written from the changes and the thread |
| GET | `/api/threads/{id}` | |
| PATCH | `/api/threads/{id}` | `ThreadPatch`: title, engine, archived, settled, seen, pinned, snoozed and the like; fields left out stay |
| DELETE | `/api/threads/{id}` | Stop the engine and everything the thread started, without waiting for the engine to agree (three seconds, then its task is dropped, its keeper is told to stop and every process carrying the thread's id is stopped), then remove the thread with its log |
| GET | `/api/threads/{id}/git` | Branch and remote of the thread's folder |
| GET | `/api/threads/{id}/changes` | Every file that differs from the thread's first checkpoint; null without git |
| POST | `/api/threads/{id}/retitle` | Name the thread again from what it turned into |
| GET | `/api/threads/{id}/peek` | `ThreadPeek` for the hover card: the latest TL;DR and whether it is stale, the last reply, quick replies, a waiting permission, the error, proof |
| POST | `/api/threads/{id}/gist` | Kept for older apps: answers `ThreadPeek` (Hub no longer writes a TL;DR itself; the agent does with `write_tldr`) |
| POST | `/api/threads/{id}/warm` | Start the engine ahead of a message, so the message is not held up |
| POST | `/api/threads/warm` | `{ cwd, engine?, settings? }`: start an engine for a thread not created yet; answers `{ id }` to create it with (`warm`) |
| POST | `/api/threads/{id}/quota-wait` | `{ action, engine? }`: a thread waiting for a plan limit carries on `now`, `cancel` stops it waiting, or `switch` moves it to `engine` (else the one offered) and carries on there |
| PATCH / DELETE | `/api/threads/{id}/queue/{seq}` | `{ text }`: change a message queued behind the running turn, or take it out |
| POST | `/api/threads/{id}/queue/{seq}/steer` | Stop the running turn and send queued message `seq` next |
| GET | `/api/threads/{id}/context` | What the thread's tokens went to, from its engine's session log (`ContextDetail`) |
| GET | `/api/threads/{id}/connections?refresh=` | The MCP servers and connectors the thread's engine has, and which of their tools it called |
| POST | `/api/threads/{id}/handoff/draft` | `{ folder }`: a brief for handing the thread's work to one of your tools, to read and edit |
| POST | `/api/threads/{id}/handoff/tool` | `HandoffBrief`: hand the work over with the brief you approved, and note it in the thread |
| GET | `/api/branches` | The branch each thread's folder is on, by thread |
| GET | `/api/prs` | The pull request each thread is linked to, by thread |
| GET | `/api/waits` | When each thread waiting for a plan limit carries on (Unix ms), by thread |
| GET | `/api/threads/{id}/events?after=seq` | The log |
| POST | `/api/threads/{id}/messages` | `{ text, attachments?, client_id? }`: images and files as base64 or text, host files as paths, links as `url`; a repeated `client_id` is not sent again |
| GET, POST, DELETE | `/api/bag` | the context bag without content, newest first; put an item in (`{ item, thumb?, from? }`); empty it |
| POST | `/api/bag/picture` | `{ url }` → the picture at a web address as an attachment, for the bag; pictures only |
| GET, DELETE | `/api/bag/{id}` | one item with its content; take it out |
| POST | `/api/threads/{id}/messages/{seq}/edit` | `{ text, attachments?, files? }`: "Edit from here", sends `text` in place of user message `seq` on a fresh session, rolling the files back to its checkpoint with `files` |
| POST | `/api/threads/{id}/settings` | `{ config_id, value }` |
| GET | `/api/threads/{id}/files?q=` | Files in the thread's folder for @-mentions, honouring `.gitignore` |
| GET | `/api/files?cwd=&q=` | The same before a thread exists |
| GET | `/api/diff?cwd=` | `{ text }`: the folder's uncommitted changes, for attaching to a message |
| GET | `/api/usage?refresh=` | Plan limits (from `/usage` in a throwaway session, at most every 2 minutes) and each thread's context |
| GET | `/api/models?refresh=` | What each model the engines offer can do, from the public model list |
| GET | `/api/connections?refresh=` | Every engine's MCP servers and connectors, and which of their tools threads called |
| GET / PUT | `/api/connections/prefs` | `ConnectionPrefs`: what Hub switches off in the engines it starts |
| POST | `/api/connections/doctor` | `{ engine }`: the engine's own check of its installation, such as `claude doctor` |
| GET | `/api/voice` | Whether speech to text is set up |
| POST | `/api/voice/prepare` | Sent as a recording starts: downloads what is missing, else loads the model into whisper-server so the first live words are quick |
| POST | `/api/voice/mute` | Query `on`: holds `voice.mute_key` (F13 to F24) down while dictating, as a voice chat's Push to Mute keybind; resent every 10 s, let go 30 s after the last |
| POST | `/api/voice/mute/teach` | Query `key`: presses it once after 3 s, for the chat app's keybind recorder |
| POST | `/api/voice/transcribe` | Body: 16 kHz mono WAV; query `prompt` (text just before it), `final` and `cwd` (the thread's folder, for name hints). Runs whisper.cpp locally with KBLab's Swedish model; the first call downloads it and the model into `<data_dir>/voice`. Previews on the processor encode only what was recorded (`audio_ctx`); with an Nvidia card, `POST /api/voice/prepare` also fetches the CUDA build and kb-whisper-medium into `bin-gpu` |
| POST | `/api/voice/polish` | Body: `{ text, cwd }`. The dictated text corrected by the tidy engine's small model (punctuation, misheard tech words, filler words), same wording; sent when dictation stops with Settings > Dictation > Tidy what you said on |
| POST | `/api/voice/prepare` | Fetch whisper.cpp and the model in the background, then the CUDA build when there is an Nvidia card |
| POST | `/api/threads/{id}/reach` | `{ targets }`: what paths named by a destructive command hold |
| POST | `/api/threads/{id}/rules` | `{ text, seq }`: add a rule said in message `seq` to the folder's CLAUDE.md |
| POST | `/api/threads/{id}/handoff` | A first message for a fresh thread, written from the log |
| POST | `/api/threads/{id}/undo/preview` | `{ to }`: what rolling back to a checkpoint (or an earlier rollback's snapshot) would change |
| POST | `/api/threads/{id}/undo` | `{ to }`: roll back; refused while the thread is working |
| POST | `/api/threads/{id}/review` | `{ model?, from?, send_back?, reason? }`: a second opinion on what changed since the last review |
| POST | `/api/threads/{id}/design` | `{ on }`: design mode on or off (`design.rs`) |
| POST | `/api/threads/{id}/design/review` | `{ doc? }`: fresh sessions review the design document; answers `{ id, doc }` |
| GET | `/api/threads/{id}/attempts` | The group of attempts the thread is one of, with each one's model, state and changes |
| POST | `/api/threads/{id}/attempts/pick` | Bring this attempt's work into the folder, archive the others, remove the worktrees |
| GET | `/api/threads/{id}/pages/{page}` | An agent's live page, sandboxed (no token: the ids are the key) |
| POST | `/mcp/{thread id}` | Hub's MCP tools for that thread's agent |
| POST | `/mcp` | Hub's tools for an agent outside Hub, with an agent key |
| GET/PUT/DELETE | `/api/customs` | Customs' list; PUT a `CustomsEdit` adds, sorts or stamps one; DELETE `?kind=&name=` takes one off |
| POST | `/api/customs/import` | `{ folder }`: clear the direct dependencies in its manifests; answers `CustomsImport` |
| POST | `/api/customs/review` | `{ kind, name }`: review it now; answers the `CustomsItem` after |
| GET/PUT | `/api/customs/prefs` | `CustomsPrefs`: review by itself, daily check, monthly update, registry |
| GET | `/api/customs/registry` | `CustomsRegistry`: where the registry answers, the engines' variables, the cargo snippet |
| POST | `/api/customs/share` | Another machine's list (`items`, `removed`): merged, and the list after is answered |
| GET | `/registry/npm/…`, `/registry/crates/…`, `/registry/pypi/simple/…` | Customs' registry (no token) |
| GET/POST | `/api/agent-keys` | Agent keys (a new one is shown once, with its command) |
| DELETE | `/api/agent-keys/{id}` | Revoke a key |
| GET | `/api/desk` | The desk folder for Ask about everything, made ready |
| POST | `/api/tidy` | `{ text, cwd?, context?, thread_id? }`: a long, messy message rewritten as a clear prompt. With `context: true` (the composer's wand, autoprompt) the writing model is also told the machine, project, folder, branch, top-level files, the rules file and the thread's latest conversation (`autoprompt.rs`) |
| GET | `/api/activity?days=&tz=` | Messages, agent turns, threads and your commits per day (`tz` in minutes east of UTC) |
| GET | `/api/activity/day?date=&tz=` | One day: its threads, commits and saved summary |
| POST | `/api/activity/day?date=&tz=` | Summarise that day with the tidy model and keep it |
| GET | `/api/pulse?days=&tz=` | When you wrote, pull requests per day and the longest thread, for the Overview's playful cards (database only) |
| GET | `/api/pulse/lines?days=&tz=` | Lines your commits added and removed per day; each commit is diffed once and kept |
| GET | `/api/stats?days=&tz=&project=` | Claude Code answers per model and per project: tokens and the cost at API prices |
| GET | `/api/devlog/day?date=` | One day's stored devblog entry, or null; nothing is worked out |
| GET | `/api/devlog?days=` | The written days of the devlog, newest first (written by `Hub::devlog_daily`, `hub/devlog.rs` the morning after) |
| GET | `/api/devlog/project?path=&days=&tz=` | One project's last days (14 by default), newest first: its commits and the threads written in |
| GET | `/api/away?since=&tz=` | What happened since `since` (ms) besides the threads, for "While you were away" |
| GET | `/api/history?q=&tz=` | Search your messages, Claude Code prompts, commits and the devlog |
| GET | `/api/checkup` | The last weekly health check of every project |
| POST | `/api/checkup` | Run the health check now |
| GET | `/api/problems?view=&level=&project=&source=&query=` | Problems, newest first, each with 7 days of counts and its stamps; `query` takes the search terms |
| GET | `/api/problems/fixes` | `FixCard` by thread id: where each fix thread of this machine's problems stands, for its inbox card |
| GET/PATCH | `/api/problems/{id}` | One problem with its latest hits, activity and tags; a `ProblemPatch` stamps, resolves, archives, merges or opens it |
| POST | `/api/problems/{id}/thread` | Starts a thread on it, `{ plan? }` to carry out Vakten's plan: `{ thread, folder, prompt }`, no thread when no folder is known |
| GET | `/api/problems/{id}/dossier` | Suspect commits, similar problems, what else went wrong around it, earlier attempts |
| GET/PUT | `/api/problems/vakten` | Vakten's settings and state; `POST /api/problems/vakten/look` hands it what is new now |
| GET/PUT | `/api/problems/rules` | Global rules (TOML), the projects' own files, suggested rules; `POST …/rules/add` adds one |
| GET/PUT | `/api/problems/views` | Saved searches |
| GET | `/api/problems/metrics`, `/api/problems/monitors` | Timings per release; cron jobs' check-ins |
| POST | `/api/problems/metric` | A timing `{ name, value, release?, project? }` |
| POST | `/api/<n>/otlp/v1/logs`, `/api/<n>/otlp/v1/traces` | OpenTelemetry JSON from a source, its key as a bearer token |
| POST | `/api/problems/report` | A problem in Hub's own format (the app window) |
| GET/POST | `/api/problems/sources` | Programs that report with a Sentry SDK, with their DSN; `{ name, folder? }` adds one |
| DELETE | `/api/problems/sources/{id}` | The program can no longer report |
| POST | `/api/{n}/envelope/`, `/api/{n}/store/` | Sentry's ingest, with the DSN's key instead of a token |
| GET / POST | `/api/routines` | Routines with when each runs next, and the starter routines; POST `{ name, folder, engine?, prompt, schedule, enabled, template? }` |
| PUT / DELETE | `/api/routines/{id}` | Change or remove one |
| POST | `/api/routines/{id}/run` | Run it now; answers `{ thread }` |
| GET / POST | `/api/recipes` | Saved recipes; POST a `RecipeDraft` |
| PUT / DELETE | `/api/recipes/{id}` | Change or remove one |
| POST | `/api/recipes/{id}/keep` | Keep an agent's suggested recipe as it is |
| POST | `/api/recipes/{id}/run` | `{ folder?, note? }`: start it; answers `{ thread }` |
| POST | `/api/threads/{id}/cancel` | Stop the current turn |
| POST | `/api/threads/{id}/shutdown` | `{ everything }`: stop the turn and the engine; with `everything`, every process the thread started too |
| POST | `/api/threads/{id}/takeover` | `{ pid? }`: let go of the thread for `hubd attach`; returns the CLI command, folder and title |
| POST | `/api/threads/{id}/handback` | Take it back from the terminal and copy in what was said there |
| POST | `/api/threads/{id}/terminal` | Open the thread in a terminal on its machine (Windows Terminal, or a tmux session) and say how to attach |
| GET | `/api/project/services?path=` | The project's services (Procfile or guess), which run, their ports and logs, and its compose file |
| POST | `/api/project/services/start` | `{ path, name? }`: start one service, or everything with compose |
| POST | `/api/project/services/stop` | `{ path, name? }`: stop one service, or everything the project runs (Stop all) |
| GET | `/api/project/services/log?path=&name=` | `{ text }`: the end of a service's log |
| POST | `/api/project/services/save` | `{ path }`: write the guessed services as `Procfile.dev` |
| GET | `/api/plugins/discover` | `PluginDiscover`: rising, new and most starred agent repositories; `?fresh=true` asks GitHub now |
| GET | `/api/plugins/search` | `?q=`: GitHub's repository search, as `PluginSearch` |
| GET | `/api/plugins/repo` | `?name=owner/name`: one plugin's page, as `PluginDetail` |
| POST | `/api/plugins/install` | `{ repo }`: install a Worker into the projects folder and run its ready check, as `WorkerReady` |
| POST | `/api/plugins/ready` | `{ folder }`: a tool's ready check again |
| GET | `/api/worker-runs` | The newest Worker run of each thread that shows one, as `WorkerRun` by thread id |
| POST | `/api/worker-runs/control` | `RunWorkerControl`: press one of a run's controls |
| GET | `/api/workers` | Engines, their process trees with CPU and memory, Hub's helpers and containers |
| GET | `/api/tools?fresh=` | Your own tools on this machine (`hub.toml`), with their questions and commands |
| POST | `/api/tools/action` | `RunOwnToolAction`: run one of a tool's commands; the app has already asked twice for a dangerous one |
| GET | `/api/tools/facts` | The tools here that take Hub's thread facts, `[{ tool, kind }]` (`hub-facts.toml`, feature `tools.facts`) |
| POST | `/api/tools/facts` | `{ kind, files: [{ name, text }] }`: put thread-fact files into the tools here that take them and let them read them; answers `{ tools, written }` |
| GET | `/api/halt` | The pause in force, or null |
| POST | `/api/halt/pause` | Pause every agent here and on every other machine (`{ here: true }`: only here) |
| POST | `/api/halt/resume` | Resume them and carry the cut-off turns on |
| POST | `/api/host/processes/{pid}/stop` | Stop a process and everything it started |
| POST | `/api/host/processes/{pid}/pause` | Pause a process tree; `/resume` continues it |
| POST | `/api/threads/{id}/permissions/{request}` | `{ option_id }`, null rejects |
| GET | `/api/dev` | Dev mode: the clone, whether its live UI is up, what is not shipped yet |
| POST | `/api/dev/setup` | `{ dir? }`: clone Hub, install packages, add the project, start the live UI (in the background) |
| POST | `/api/dev/start`, `/api/dev/stop` | The live UI server on port 1421 |
| POST | `/api/dev/ship` | `{ message? }`: commit to a branch, rebase on the updater's branch, push and open (or add to) a pull request |
| POST | `/api/dev/core` | `{ on }`: build and run the dev core on 7879 and point the dev window at it, or back at this core |
| POST | `/api/dev/preview` | `{ on, only?, refresh? }`: the clone follows the open pull requests merged on top of main (or one of them), or goes back to its own branch |
| POST | `/api/dev/show` | `{ thread? }`: the dev window (and dev core) shows that thread's worktree, or the clone |
| GET, PUT | `/api/projects/trees` | `?path=` / `{ path, on, base? }`: whether new threads in the project get a worktree of their own, and what they are cut from (`ProjectTrees`, feature `project.worktrees`) |
| POST | `/api/dev/worktrees` | `{ on }`: whether new threads in Hub (dev) get a worktree of their own |
| POST | `/api/dev/worktrees/remove` | `{ thread }`: remove a thread's worktree, keeping its work in git |
| POST / DELETE | `/api/sim` | Start simulated threads in `<data dir>/Simulation` (answers how many), or remove them all |
| GET | `/api/projects` | Folders added as projects |
| POST | `/api/projects` | `{ path, create?, name?, github?, setup?, about?, template?, public?, ci? }`: add a folder, or create it with git and living docs, optionally from a template (`template`), a GitHub repo (`gh`, private unless `public`, with a workflow when `ci`) and a setup thread (feature `project.templates` for the last three) |
| GET | `/api/projects/templates` | `{ templates, tools }`: the templates this core writes and which of the tools the stacks need it has, with versions (`ProjectTemplates`, feature `project.templates`) |
| GET | `/api/projects/defaults` | `{ root, github }`: where new projects go (D:\Dev by default) and whether `gh` is logged in |
| PUT | `/api/projects/defaults` | `{ root }`: change where new projects go |
| DELETE | `/api/projects?path=` | Take a project off the list; the folder stays |
| GET | `/api/folders?path=&hidden=` | The folders in a folder, for the folder picker |
| POST | `/api/folders` | `{ path, git? }`: make a folder, with `git init` when asked |
| GET | `/api/folders/repo?path=` or `?key=` | A folder's repository key and remote, or where the repository with that key is on this machine |
| GET | `/api/folders/projects` | Folders under the projects root and the ones threads and projects use |
| POST | `/api/folders/clone` | `{ url, path }`: clone a repository there |
| GET | `/api/project?path=` | The project page: branch, commits, GitHub remote, stack, docs and plan |
| GET | `/api/project/github?path=` | Open PRs and the latest CI run, through the `gh` CLI when it is logged in |
| GET | `/api/project/runtime?path=` | What the project has running (servers, containers) and what it uses |
| POST | `/api/project/open` | `{ path, with }`: open the folder in the editor, a terminal or the file manager |
| GET | `/api/project/tree?path=&rel=` | One folder level of the file tree, honouring `.gitignore` |
| GET | `/api/project/file?path=&rel=` | A text file in the project, for the file reader |
| GET | `/api/project/raw?path=&rel=` | A picture, PDF, sound or video file as its bytes (64 MB at most, sandboxed), for the file readers |
| GET | `/api/project/vault?path=` | Every markdown doc with title, summary, tags and link counts, and the tags |
| GET | `/api/project/vault/doc?path=&rel=` | One doc by path or name: text, front matter, headings, links, backlinks, unlinked mentions |
| PUT | `/api/project/vault/doc` | Writes a doc the user edited: `{ path, rel, body }` |
| GET | `/api/library` | The library's folder, after Hub brought its facts up to date |
| GET | `/api/library/prompt?ask=` or `?learn=` | The first message of an ask or learn thread |
| GET | `/api/project/vault/search?path=&q=` | Docs matching every word (last one as a prefix) and `#tags`, with the section and line |
| POST | `/api/project/plan` | `{ path, line, done }` ticks a step, `{ path, text }` adds one |
| POST | `/api/project/focus` | Edits `docs/FOCUS.md`: `op` tick (`line`, `done`), add (`line` of the stage, `list` done or test, `text`), park (`text`), activate (`line`), start, rule; answers the focus |
| POST | `/api/project/focus/summaries` | `{ paths }` → each folder's stage now in a line, `{ stage, n, stages, done, of, to_test }`, by path |
| GET | `/api/project/focus/draft` | `{ message }`, the first message of a thread that drafts the focus |
| POST | `/api/project/docs` | `{ path }`: start `docs/plan.md` and add the living-docs rule to CLAUDE.md |
| GET / POST | `/api/ideas` | The idea inbox; POST `{ text, here? }` keeps an idea and answers at once, the stream says where it went |
| GET | `/api/ideas/places` | Projects an idea can move to, with their stages |
| DELETE | `/api/ideas/{id}` | Drop an idea |
| POST | `/api/ideas/{id}/move` | `{ project, stage? }`: move it to another project or stage |
| POST | `/api/open` | `{ url }`: open a web link in the browser of the machine the core runs on |
| POST | `/api/reveal` | `{ path }`: show a file selected in its folder, or a folder, in the file manager |
| POST | `/api/downloads?name=` | The body's bytes saved in Downloads on the core's machine, under a free name; `{ path }` |
| POST | `/api/project/images?path=&name=` | The body's bytes saved in that project's `docs/images` under a free name; `{ path }` |
| GET | `/api/prefs/about` | Version, update settings, where data and config live, recent commits |
| GET | `/api/prefs/accounts` | Whether each engine is logged in (subscription or key) and API keys in the environment, never the secrets |
| GET | `/api/prefs/guard` | The quota guard's settings |
| PUT | `/api/prefs/guard` | `GuardPrefs`: change them |
| GET / PUT | `/api/prefs/permissions` | `PermissionPrefs`: Settings > Safety, the permission rules and protected files |
| GET / PUT | `/api/prefs/threads` | `ThreadPrefs`: when Hub settles threads by itself |
| GET / PUT | `/api/prefs/chatdetail` | `ChatDetail`: how much of the agent's work a thread shows on the phone (a default and a level by thread id); levels minimal, quiet, normal, all |
| GET / PUT | `/api/looks` | `Looks`: the icons picked for machines (a `MachineKind`) and projects (icon, colour or a small picture) |
| GET | `/api/prefs/ssh` | Keys in `~/.ssh` and the hosts in its config |
| GET, PUT | `/api/prefs/config` | The settings the app can change in `hub.json` (running and saved); PUT `{ key, value }`, null resets |
| POST | `/api/prefs/restart` | Start a fresh core so saved settings apply; working threads carry on in it |
| GET / POST | `/api/prefs/disk` | What Hub's data and build folders take; POST empties what the build folder can do without, refused while an update builds |
| GET | `/api/prefs/tools` | Programs Hub leans on (git, gh, node, claude, ...) and their versions |
| GET, POST | `/api/prefs/notes` | Patch notes per version; POST writes them for the running version now |
| GET | `/api/host` | CPU, memory, listening ports and their processes |
| GET | `/api/host/about` | What this machine is, for its page: host name, system, CPU, disk, uptime, Tailscale, and its `kind` (desktop, laptop, board, VPS, server) for its icon |
| GET | `/api/hosts` | This machine first, then the others with their state |
| POST | `/api/hosts` | `{ name, ssh?, url?, code? }`: add a machine; `code` is redeemed at `url` for a token |
| DELETE | `/api/hosts/{name}` | Remove a machine added from the app; its threads stay on it |
| PATCH | `/api/hosts/{name}` | `HostSettings` (every field optional, one left out stays): change a machine added from the app, kept with the others (`Remotes::change`); one from `hub.json` is refused (change it there). Now `push_quiet_mins` (0 = none, see "Self-update"): put on the host's info (`HostInfo.push_quiet_mins`), which the ten-minute check reads (`Remotes::push_gap`), so it counts at the next check without a restart. Answers the `HostInfo`. Feature `hosts.settings` |
| POST | `/api/hosts/{name}/reconnect` | Try an offline machine again now |
| POST | `/api/hosts/{name}/install` | Install hubd over SSH on a machine that has none; runs in the background. Body `InstallRequest` (the sudo password, when asked); answers `InstallAnswer`, whose `sudo` says what root is needed for when nothing started |
| POST | `/api/hosts/{name}/update` | Update now: send the machine this version the way it takes updates; one without hubd gets Install instead. Answers `InstallAnswer` |
| POST | `/api/hosts/{name}/tailnet` | `InstallRequest`: install Tailscale over SSH and start it; the sign-in link shows on its row |
| POST | `/api/hosts/{name}/open` | `{ what, engine? }`: open a terminal on this computer logged in there: a shell, a root shell or an engine's CLI |
| POST | `/api/hosts/{name}/terminals` | `{ what, engine?, thread?, cols, rows, admin? }`: the same as a terminal inside Hub; `attach` with `thread` takes that thread over (`pty.rs`); `admin` (Windows, this computer) runs it elevated through `hubd admin-host`; answers its `TerminalSession` |
| GET | `/api/terminals/{id}/host` | WebSocket for `hubd admin-host` only, with the terminal's one-time `secret` (no web page: an Origin is refused) |
| GET | `/api/terminals` | The terminals inside Hub, ended ones with their exit code |
| DELETE | `/api/terminals/{id}` | Close one, ending what runs in it |
| GET | `/api/terminals/{id}/ws` | WebSocket: its output so far, then live; keys and `{"resize": [cols, rows]}` back |
| GET | `/api/tailnet` | The tailnet as this machine sees it, each machine matched to the one it is in Hub |
| POST | `/api/tailnet/setup` | `NewHost`: add a machine found on the tailnet and install hubd there when it has none |
| POST | `/api/tailnet/{name}/dismiss` | Hide a machine from the nudge; it stays in the list in Settings |
| GET | `/api/lan` | Machines on the home network that answer SSH |
| GET | `/api/engines/clis` | The engines that have a CLI to open in a terminal |
| POST | `/api/pair` | A one-time pairing code, with the address Tailscale serves this machine at |
| POST | `/api/pair/redeem` | `{ code, name }` → `{ token, machine }`; open, the code is the credential |
| POST | `/api/phone` | A pairing code for the phone, with its link and QR code when Tailscale serves this machine, or the problem |
| GET | `/phone`, `/phone/{file}` | The phone page (no token: it pairs with the code in the link) |
| GET | `/api/devices` | Devices let in with a code |
| DELETE | `/api/devices/{id}` | Revoke a device's token |
| GET | `/api/stream` | WebSocket of `StreamMsg` |

Any request with `?host=<name>` for another machine is answered by that machine's core.

## Staying up

The app checks the core every five seconds and starts it again when it has been gone for 25
seconds, longer than an update's handover takes. A new core binds the port before it opens the
database, so a long migration does not look like a dead core.

## The settings file
<!-- covers: crates/hub-core/src/config.rs, crates/hub-core/src/prefs.rs, crates/hub-core/src/engine_home.rs, crates/hub-core/src/home_files.rs -->

`hub.json` in the data folder (or the path in `HUB_CONFIG`) holds the settings; a missing file
means defaults. Several places change it while Hub runs: Settings (`prefs::edit_config`), Add and
remove engine and the engines shared from other machines (`engines.rs`). They all go through
`config::edit_file`: one lock around read, change and write, so two changes at once cannot lose
each other's part, and a check that the result still reads as a `Config` before anything is
written. `config::write_file` writes to a file beside it and moves it into place, so a crash or a
power cut mid-write leaves the old file, never a cut-off one.

At start `config::load` reads the file. One that cannot be read (not JSON, or a value of the
wrong kind) is moved aside as `hub.json.broken-<time>` and Hub starts on defaults, with the
reason in the log and under Settings > About (`About.config_problem`), instead of not starting at
all. Hosts added from the app, pairings and the like live in the store, not in `hub.json`, so they
survive this.

## Logs and backups
<!-- covers: crates/hub-core/src/store.rs, crates/hub-core/src/store/, crates/hub-core/src/backup.rs, crates/hub-core/src/blobs.rs, crates/hub-core/src/compact.rs -->

Installed builds log to `<data dir>/logs/hubd.<date>.log`, one file a day, two weeks kept.
Engines' stderr is in `<data dir>/engines/<thread>.log`. Warnings and errors in the log also go to
Problems (see there). With `"trace_engines": true` in
`hub.json`, every JSON-RPC line between Hub and an engine is kept too, in
`<data dir>/engines/<thread>.trace.jsonl` as `{ from: "hub" | "agent", ts, msg }`, keys redacted
like the logs; that is how a real session is recorded as a fixture for a contract test. Both go
when the thread is deleted. The database is copied into
`<data dir>/backups` once a day (three kept) and before every new version starts (two kept);
to go back, stop Hub and put a copy in place of `hub.db`.

The store (`store.rs`) is one SQLite connection behind a mutex, WAL journal, every call its own
transaction on the blocking pool. The schema is a list of steps (`steps()`), each applied once
and never changed after it shipped; a database from a newer build is refused rather than opened.
Deleting a thread (and only that, after "Delete for good?") removes rows, and a problem keeps only
its latest 30 hits and 90 days of counts; everything else only adds. An event for a thread that is
no longer there (its engine was still talking when it was deleted) fails with `ThreadGone`, not
SQLite's bare foreign-key error: the engine drops such updates with a debug line and stops
without logging a failure. A panic inside one call leaves the connection usable for the next.

**Generated pictures are kept once, as files.** An engine sends a picture as base64 in an ACP image
block, and Codex repeats it in `rawOutput.result` of the same call, which made 39 events 60 % of a
424 MB database. The store lifts a picture of 4 KB or more out when it writes an `update`
(`blobs::lift`): the bytes go to `<data dir>/blobs/<sha256>.<ext>` (the same picture is one file),
the row keeps `blob: "<name>"` in the image block instead of `data`, and the repeat is dropped. The
reads that return events (`events`, `events_of`, `events_except`, `last_events`, `reply_chunks`,
`events_since`) put `data` back, so the app, the phone, side jobs and a thread moved to another
machine (`import_thread` lifts again there) see the same event as before; no API changed. A file
that is gone leaves the block without `data`, which the app shows as nothing. At start hubd runs
`Store::lift_old_pictures` once for rows from before: it takes a copy to
`backups/before-pictures.db`, opens it to check it is intact and has the events, then moves each
row's picture after its file is written, and notes `blobs.lifted` in the store. Files are not
removed when a thread is deleted yet.

Old replies fold (`compact.rs`, Christoffer's decision, the one exception to "append-only" and only
for streamed text): a run of `agent_message_chunk` or `agent_thought_chunk` rows that follow each
other, are plain text of the same shape, have an ended turn (`turn_end`/`error`) after them and
whose last chunk is over two days old becomes one row, with the first chunk's `seq` and `ts` and all
the text in order. The other rows are deleted, so `seq` has gaps in old history. At start, after
the picture job, hubd runs `Store::compact_old_chunks` at most once a day: a first read-only pass
counts, and only if there is work it takes a checked copy (`backups/before-chunks.db`, `VACUUM INTO`),
then folds one thread per transaction and vacuums. Running it again finds nothing.

A log is mostly `update` payloads (a busy thread holds tens of MB of JSON), and `events` parses every
row it returns into memory. Code that runs often (each permission request, each turn end, each
engine start) therefore reads with `Store::events_of(thread, after, kinds)`, which asks SQLite for
only the kinds it needs (design mode, checkpoints, user messages, settings, session state), and
`events` only where it really needs the conversation. Sending a message reads the tail without updates (`events_except`, `last_seq`), the failure check `last_events`, and the topic check the reply text only (`reply_chunks`). A conversation (a handover, a side job's or a brief's context, `read_thread`, the judge) reads `events_with_updates(thread, after, kinds, updates)`: the kinds asked for plus only the `update`s whose `sessionUpdate` is named (`replies::conversation` asks for messages, rewinds, `agent_message_chunk` and `tool_call`), so tool output, most of a log, stays in SQLite; on a synthetic 10 000-event thread (debug build) 53 ms against 157 ms for the whole log, and `wait` polls only turn ends, errors and chunks. No production code reads a whole log with `events(.., 1_000_000)` any more (`whole_log` in `rules.txt` is empty). `events_after_last(thread, kind, skip)` gives what came after the last turn end or user message. Undo, edit, review, attempts, permission answers, queue edits, restart, side jobs and routines read this way too. Measured with a 40 MB thread: one full read held the single connection for 240 ms (any other call waited that long), a kind-filtered read 14 ms, so a separate read connection waits until a narrow read shows up as slow. This is what keeps hubd near 100 MB on a
100 MB store: hubd on tincan held 300 MB because `design::is_on` parsed a whole log per permission request.

`Store::last_turn` lists the messages that never reached the engine, which a restart or the carry-on sends again. One the user removed, or Hub dropped because the engine kept failing (a `queue` event `removed` or `dropped`), is not in it: a thread whose engine cannot start is not started again for the same prompts.
