# Architecture: Problems, routines, models, keys, APIs, viewer and labs
<!-- för: agent -->

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## Problems
<!-- covers: crates/hub-core/src/problems/, crates/hub-core/src/problem_inbox.rs, crates/hub-core/src/metric_samples.rs, crates/hub-core/src/vakten_thread.rs, app/src/lib/components/ProblemsView.svelte, app/src/lib/problemsView.ts, app/src/lib/components/ProblemDetail.svelte, app/src/lib/components/ProblemsSetup.svelte, app/src/lib/components/GuardPanel.svelte, app/src/lib/guard.ts, app/src/lib/problems.ts, app/src/lib/components/ErrorCard.svelte, app/src/lib/fixes.svelte.ts -->

Code analysis (`problems/analysis.rs`, feature `problems.analysis`) is an additive, tool-neutral
input alongside runtime reports. `POST /api/problems/analysis/runs` starts a run with project,
canonical project folder, branch, commit, tool/version, effective rules/configuration digest and
scope (the exact analysis targets). `POST /api/problems/analysis/runs/{id}/finish` supplies a
terminal state (`succeeded`, `failed`, `partial`), findings and a diagnostic (required for failed
or partial runs). `GET` on the run returns metadata and observations; the collection lists the
latest 100, optionally by `?project=`. Running and incomplete runs remain visible through this
API; no scanner is launched and there is no run-history UI yet. Adapters must attest that a
successful snapshot is complete and actually analyzes the declared commit/configuration.

Successful findings appear in the existing Problems list, source/filter `code_analysis`.
Their hit contains run id, commit, tool and rule metadata; the frame links the relative file and
line. Identity is project/folder/branch/tool/scope plus rule/path/adapter occurrence key, never
the line number. The adapter must supply a stable key that distinguishes occurrences in one
file. Versions do not duplicate findings, but automatic disappearance requires exactly the same
tool version and rules/configuration digest as the finding's latest observation. Different
branches/scopes stay separate. Failed/partial observations are retained in the run only and
never update or close Problems. Out-of-order success is rejected when a newer run in the same
stream already succeeded. Finalization is transactional and terminal runs cannot be rewritten.

Only a successful comparable complete snapshot can resolve a code finding; ordinary resolve,
merge-into and PR-merge resolution cannot bypass verification. Archive remains available to
dismiss a finding (archived findings stay archived). A resolved finding seen again reopens with
the same problem id and a regression activity. Analysis starts in ask-first and is excluded from
Guard's automatic queue until the later agent integration. Existing source filters already work
against this input; scanner adapters, a manual Analyze button and merge gates are later steps.
Analysis output uses the same secret redaction as Problems before storage.

Errors and warnings from Hub and from the user's own programs land in one list (`problems/`,
System > Problems), the way GlitchTip and Sentry do it: the same error is one problem with a
count, first and last seen, a sparkline of the last 7 days, and the latest hits (30 kept per
problem) with their stack, what happened before (breadcrumbs or log lines), release and anything
else the sender said. Secrets are masked before anything is kept, as in the event log. Everything is
in Hub's own tables (`problems`, `problem_hits`, `problem_days`, `problem_activity`,
`problem_aliases`, `problem_metrics`); Sentry's and OpenTelemetry's formats are only ways in.

Where problems come from:

- **Hub's core**: hubd's log layer (`ToProblems` in `crates/hubd/src/main.rs`) hands every
  `warn` and `error` to `problems::capture`, with the 20 log lines before it. The problems module
  logs only under its own target, which the layer skips, so it cannot loop. `capture` and its
  queue live in `problem_inbox.rs` (rank 1), so code that reports a problem (host updates) does
  not reach the module that records it.
- **The app window** (`app/src/lib/problems.ts`): anything thrown that nothing caught and
  promises that failed with nobody waiting, with the last clicks, once a minute per message, to
  `POST /api/problems/report`. Browser noise (`ResizeObserver loop`) is left out.
- **Files without a viewer**: when the chat meets a file an agent made that Hub has no way to show
  as what it is (`.blend`, `.glb`, `.docx`…, see `filePreview.ts`), the app window reports a warning
  "No viewer for .blend files" (source `app`, one per extension and a day, fingerprint `no-viewer` and
  the extension, with the file name and thread as extra; no file content). A built-in rule stamps it
  P2, category `missing-viewer`, and leaves the lane empty so Vakten reads it: its brief says to hand
  it to a fix thread that adds the kind to `filePreview.ts` and draws it in `FilePreview.svelte`.
- **Threads**: an `error` event in a thread (`Hub::look_at`), grouped by engine and message, not
  per thread, with what came before in the thread (your message, its tool calls) as breadcrumbs.
  Plan limits are left out, since Hub carries those threads on by itself. A thread outside every
  known project (a dated folder from Home) counts under project Hub with no folder, so the same
  engine failure is one problem and a fix goes to Hub's code.
- **Agents' tool calls** (`tools.rs`, source `tool`): every engine's calls are read from the event
  log as they are logged, whichever engine made them, and counted per day, engine and tool in
  `tool_calls` (the first start counts the last 30 days already in the log). A call that ends
  `failed` is a warning grouped by engine, tool, the program a command ran and the first line that
  says what went wrong (not "Exit code 1"), with the call's input and the thread's breadcrumbs. For a
  command the engine streams as a terminal (the call itself carries only `_meta.terminal_exit`), what it
  said is the end of the `terminal_output` chunks, else the exit code (`terminal_said`). A
  call that said nothing at all (no output, a bare exit code, a fence) is kept with the command it
  ran instead, so the page shows what was tried rather than that the call said nothing.
  Calls refused at a permission question (by you or by Settings > Safety) and calls cut off by a
  stop are not failures. Tools are named the same across engines: Claude Code's own name (`Bash`),
  `server · tool` for an MCP tool, else the ACP kind (`Command` for "execute"). What a failed
  call said (that first telling line, never a bare ``` fence) is counted per day too, in
  `tool_errors` (the first start after step 13 counts the last 30 days already in the log). The
  page's **Tool calls** tab (`GET /api/problems/tools?days=`) lists each tool per engine and
  machine with its calls, failures, failure rate, failures per day and what its failed calls said
  most often (`errors`, linked to the problem they are kept as when there is one), sortable by
  each column and filtered by machine, engine and words in the tool or its errors.
  Hub's first pass archives these problems until they get worse, so Vakten does not chase them
  one by one. Instead, once a day `vakten::look` hands it the tools that failed at least 5 times,
  or 3 times and 10% of their calls, in the last week on any machine (`tools::review_candidates`,
  with the week before as the trend), and Vakten reviews them together: agent misuse (a rule in
  the project's CLAUDE.md or AGENTS.md), machine setup, a Hub bug (a fix thread with a contract),
  an outside service or noise, and what is worth the effort. It saves the review with
  `tool_review`; the Tool calls tab shows the newest one (`GET /api/problems/tools/review`) and
  each tool's finding on its row.
- **The user's programs**: any Sentry SDK. "Connect a program" makes a source with a DSN
  (`http://<key>@127.0.0.1:<port>/<n>`, kept in `kv` as `problems.sources`); the SDK posts
  envelopes to `/api/<n>/envelope/` (or old-style events to `/api/<n>/store/`), gzip, deflate or
  brotli. `event` items become problems, `check_in` items feed the cron watch, `transaction` items
  become timings; the rest is skipped. A program with OpenTelemetry posts OTLP JSON to
  `/api/<n>/otlp/v1/logs` and `/v1/traces` with the key as `Authorization: Bearer` (`otlp.rs`):
  log records at warning and above become problems, root spans become timings, error spans
  problems. These paths need no Hub token: the handler checks the source's key. All of Problems'
  HTTP (the `/api/problems/...` router, its handlers and this intake, `is_ingest` for the guard)
  is `problems/routes.rs`; parsing an envelope is `ingest.rs`.
- **Quiet sources** (`quiet.rs`, source `monitor`): a cron job that sent an `error` check-in or
  missed its time (its schedule, or the gap between two good runs), a program silent for longer
  than its "quiet after" setting, a machine offline for half an hour, a routine run that failed.
- **Slower releases** (`metrics.rs` sweeps what `metric_samples.rs` gathered, source `metrics`): Send to first word (`TurnStart` to the
  first update), the API's time per GET (`api::to_host`), hubd's memory, and the programs'
  transactions gather in memory and are kept as one median per metric and release on each sweep.
  A release half as slow again as the one before it (and by over 100 ms, or 50 MB) is a problem
  stamped `performance`. It gets one hit when it first shows, and another only when the median
  grows by a tenth over the one the last hit recorded for that release (found through
  `problem_aliases` too, so a problem merged into another still counts once) (or a new release
  starts its own problem);
  each hit's `extra.sweeps` counts the sweeps it stayed slow since the first hit, so a steady
  difference never looks like a storm to the escalation rule. `POST /api/problems/metric` takes a
  timing from anywhere.

Grouping (`fingerprint`): a rule's `group`, else the sender's own `fingerprint` (`{{ default }}`
stands for the rest), else the error's type and the project's own stack frames without line
numbers, else the message with numbers, ids, paths and quoted names taken out. Project and source
are always part of it. A merged problem's fingerprint is an alias of the one it went into
(`problem_aliases`), so later hits count there; "split off" removes the alias.

States: open, resolved, archived (was "muted"; step 11 moved those to archived for good).

- **Resolve** now (any hit brings it back as "came back") or **in the next release** (the
  releases it was seen in are kept; only a hit from another release brings it back).
- **Archive** until it gets worse (back when a day has over twice the week's daily rate and at
  least 10), until a count, for a time (`watch.rs` brings it back when the time is up) or for good.
- **Escalating**: a problem older than a day with 10 or more hits today and over three times its
  daily rate of the week before. Its prio goes up a step; `watch.rs` takes the mark off once a
  day passes without hits.
- **Linked to its fix**: the thread started on it, that thread's pull request (followed by
  `watch::follow_prs` from `Hub::thread_prs`), and a merge resolves it for the next release with
  "PR #n" as who resolved it. When the thread is archived or its worktree gone (no branch to find the
  PR by), `follow_prs` asks GitHub for the PR number kept on the problem, and a merge noted but never
  acted on is caught up once. If it comes back after a fix, its lane becomes ask first.

Stamps (`change`, `PATCH /api/problems/{id}` with a `ProblemPatch`): prio P0 to P3, lane (quiet,
tell, ask), category, confidence, fixable, and a note (what, likely cause, plan, next, and the fix
contract's repro and done when) that the page shows first. A note adds to the one there: a field
left empty keeps what it said. `problem_thread` takes the contract as arguments or reads it from the
note. Every change goes in the problem's activity with who did it ("you", "Vakten",
"rule", a thread's title, "Hub", "PR #n") and why. A problem **needs you** when it is in the ask
lane, came back, is P0 or P1 with no thread, or is an unsorted error while Vakten is off. Every
"need you" count is that one set, counted per problem on this machine and every online one
(`hub.problemsNeedYou`, keyed `machine:id`, read again each minute and whenever the page loads):
the sidebar, the shelf on Home, the summary line and the Needs you tab. Fold alike only changes
how many rows they take, which the line under the list says ("6 of 6 problems, in 4 rows"); the
tab and summary also follow the page's level and search.

The list (`ProblemsView.svelte`, its row types, tabs, sorting and formatters in `problemsView.ts`) filters by machine, project, source and prio beside the search
(each choice with how many rows have it), sorts by any column, and folds problems with the same
title, source and level in several places (threads, machines) into one row that opens to show
them. A line under the summary says what stands out: new today, where most is open, where most
comes from. A row that needs you says what it asks in words, with the button that answers it:
**Sign in** for a lost engine sign-in (a terminal on that machine, as in the chat's sign-in card),
**Let Vakten do it** for a plan, **Open thread** when a thread is on it, else **Start thread**.
Filters and sorting are kept in `localStorage` between visits. Rows speak Hub's visual language (`refs.ts`): the ring in front of
each row is on the threads' scale (amber ring waits for you, pulsing green is being fixed, red
fill went wrong again or is urgent with nobody on it, lavender fill is fixed, grey ring is left as
it is), and grey glyphs say what the rest is: the project with its own icon, the source, the
machine with its online dot, the engine's mark on Tool calls. The thread on a problem and its pull
request are links in the Vakten column. Only P0 and P1 get a box. With a problem open beside it, the list drops the
7 days and the ask's sentence before it cuts a title (a container query), and drops Vakten's
column when narrower still.

A problem's page (`ProblemDetail.svelte`) shows the latest hit on its Overview, under Vakten's
reading and the buttons: when and on which machine, the release, the thread it happened in (a
link), the message, what the agent sent (a tool call's input), the stack newest call first with
the project's own frames bright and runs of library frames folded, and the breadcrumbs as a
timeline ending in "this error", each with a ring for its level. The Hits, Code (whole stack and
suspect commits), Similar and Activity tabs hold the rest.

**First pass without AI** (`rules::built_in`): before any rule of the user's, Hub stamps what the
words alone tell. A lost sign-in ("Authentication required", "not logged in", …) is P1, config,
ask first, grouped as one problem; busy services (rate limit, overloaded) P3 external; network
failures (refused, reset, timed out, unresolved host) P3 environment; an engine's "Internal error"
in a thread P2 external; an agent's failed tool call (source `tool`) P3 noise, archived until it
gets worse. Only the sign-in rule sets a lane; the others are a head start ("Hub's
guess") and still go to Vakten. The user's rules come after and win.

**Vakten** (`vakten.rs`) is the agent that sorts and fixes. Off until turned on (Problems > Vakten,
kept as `problems.vakten`). There is **one Vakten for every machine**: the one on the machine that
has the others (the PC). Problems stay where they happened, but on each look it asks every online
machine for its queue (`GET /api/problems/guard/queue`) and hands those over too, as "#12 on
tincan". Its tools take `machine` and act there through the guard routes
(`/api/problems/guard/{id}/change|fix|tell`), so a fix thread for a problem on tincan starts on
tincan, where the code is. A machine asked for its queue is guarded for 15 minutes
(`vakten::guarded`, kept in memory per Hub on `Remotes::guarding`, with the other machines'
queue counts): its own Vakten stays still even if on, its page says who guards it, and its
unsorted errors no longer count as needing the user. Its **queue** (`vakten::queue`) is every open problem at the level it reads with no lane yet,
unless Vakten or the user stamped it already. A minute after something new or changed
(`vakten::nudge`), and every five minutes for the clock's work (`watch::sweep`), it hands its one
thread (in Hub's data folder, a system thread folded under From Hub in the inbox, see System threads) the queue and what came back, 15 at a time, with Hub's guess
for each; the first message carries its brief. While the queue is not empty it checks back every
20 seconds, so the next 15 go as soon as its turn ends. A problem it was handed and left as it was
(`vakten::pick`, kept by "machine:id" in its state) is not handed again until its count goes up,
something not Vakten's own happens to it, or six hours pass; otherwise one it cannot settle would
come back every look. When the brief or its tools change (the version is a hash of the brief text and `VAKTEN_TOOLS`, so nobody has to raise a number), its thread is archived
and a new one starts, since an engine keeps the tools its session began with. It skips while its thread is busy, the panic
button holds, or a plan limit is spent. Its thread is answered by `permit::vakten`: like a routine
(reads, commands that only look) plus `VAKTEN_TOOLS` (`stamp_problem`, `change_problem`,
`problem_thread`, `tell_user`, `tool_review`, `answer_worker`). `problem_thread` takes a **contract**: repro, cause, done when
(checkable), and plan; Hub keeps it in the problem's note and refuses Vakten without repro and
done. It holds Vakten to the lanes (`vakten::may_fix`): no ask-first problem, none that came back
after its fix, none with a thread on it unless it starts it over (see below). There is no cap on fix threads. Each fix thread is an
ordinary thread in the problem's project, one per problem, running alongside the others while
Vakten goes on sorting; it gets the brief plus the contract (`problems::contract`) and is told to
end at an open pull request that the thread has reviewed and merges itself (Hub has no merge queue). Vakten also gives `size` ("small" or "big").
A failure never feeds itself (`vakten/backoff.rs`, `hub/retry.rs`). After two failed turns of
Vakten's own thread in a row (engine or network down) it is left alone for 5, then 15, then 60
minutes; a turn that gets through resets that. A failed turn of Vakten's thread is not recorded as
a problem, and the same failure of any thread is recorded at most once a minute (`skip_failure`).
When an engine fails to start, the messages queued behind it are tried again after 2, 4, 8 s (up to
a minute) and dropped after three failures in a row, counted in memory (`retry::failed`) rather than
from the log, where other events between two failures broke the count.

**Fix threads report back to Guard** (`report.rs`, feature `guard.reports`), never to the user:
Guard and its threads decide together, and the user is needed only when auto mode refuses
something. A thread Guard starts is kept as its fix (`problems.vakten.fix:<thread>`, the problem
and the machine it is on; `start_thread` and `fix_here` note it). Its brief tells it never to ask
the user: it asks Guard with Hub's `ask_guard` and ends its turn, and says how it ended with
`report` ("fixed" or "blocked") before its last turn ends (both MCP tools only a Guard fix thread
gets, `mcp::guard_tools`). When its turn ends or fails (`report::turn_ended`, from `look_at`) Hub
writes a **report**: asked, blocked, failed, stopped, or ended without a pull request, with its last
words. A turn that ends at a pull request is only noted in the problem's activity; the thread itself reviews and merges it. Reports wait on the machine the thread runs on (`problems.vakten.reports`, at
most 50) and wake Guard; another machine hands them over with its queue to a Guard that asks for
them (`&reports=true`, so an older Guard never makes them vanish). Guard's turn starts with "Your fix
threads reported back" and it decides: `answer_worker` sends its answer into the thread, here or on
its machine (`POST /api/problems/guard/answer`), and the thread carries on; `problem_thread` with
`again` starts the problem over with a new plan (not while its thread works, at most
`vakten::TRIES` = 3 threads per problem, `may_try_again`), which puts the thread before away and
stops its reports (`report::retire`); or it leaves the problem and says why. While Guard's own turn
runs, waiting reports make it check back every 20 seconds.

**Fix threads run in the background**, whoever starts them (Vakten, the page's buttons, Fix now):
`start_thread` creates them with `NewThread::unattended` (`problems::fix_unattended`), logged
first in the thread as an `unattended` event. It holds words to pick settings by: model "haiku"
(then "mini", "flash") for most problems, "sonnet" first for a big one (`big_fix`: Vakten said
"big", or P0/P1, came back after a fix, escalating, or ask-first), never the largest model; and
mode "auto". When the engine first starts (`engine/acp.rs`, `preferred`) the first offered choice
that is a word, else contains one, is set and logged as a `setting` marked `by: "prefer"`, unless
the thread chose that setting itself; an engine that offers no match keeps its default. Its
permission questions never reach the user: `permit::unattended` answers by Settings > Safety
first, then allows what the "everything" level would (nothing that deletes or destroys work) and
refuses the rest with a note to work around it or say so in the pull request. The page never
opens the thread: the button answers at once with a dot that flies to Inbox's icon, which plays a
row landing on top (`fixes.svelte.ts`, `InboxIcon`'s `land`), and the row says "Sending", then
"In Inbox, working", while the core makes the thread (a dev worktree can take seconds). The
brief asks the thread to rename itself first, in plain words without the error code. Its card in
the inbox gets a third line (`ActivityList.svelte`, `hub.fixCards` from `GET
/api/problems/fixes` on every online machine, `problems::fix_cards`): where it stands in words,
the pull request first ("Fix ready in PR #n · its thread reviews and merges it", "merged · comes with
your next update", "closed without merging"), else the thread ("Looking into it", "Waits for
your answer", "Stuck", "Stopped on an error", "Finished without a pull request"), coloured only
by state (`fix_state`). A problem on a machine that knows no folder for its project (Hub's own
code is often only on the PC) has its fix thread started on the asking machine instead, in the
project of the same name (`problems::fix_here`, `POST /api/problems/fix_here`; Vakten's
`problem_thread` does the same), and the problem there is told which thread is on it (`POST
/api/problems/guard/{id}/linked`). Only when no machine knows the folder does the brief open in
a message box to pick one. `tell_user` adds a line on the Problems page (`StreamMsg::ProblemNews`; a desktop notification only with `critical: true`, which Guard is told to use almost never) and a line in the
activity. Once a week it also writes a summary.

**Rules** (`rules.rs`): TOML `[[rule]]` tables, global ones in Settings (`problems.rules`) and a
project's own in `.hub/problems.toml`, matched on words, path, project, source, machine and level,
setting group, prio, lane, category or archive. They apply as hits come in; a new problem a rule
stamps is stamped by "rule". When the user stamps the same way, or archives, three times for one
project and source, the rules panel suggests it as a rule.

**Search** (`search.rs`): words plus Sentry-style terms, `is:new|unstamped|regressed|escalating|needs_you`,
`level:`, `project:`, `source:`, `machine:`, `release:`, `prio:<=p1`, `lane:`, `category:` or
`stamp:`, `thread:no`, `times_seen:>10`, `lastSeen:-24h`, `firstSeen:+7d`, `-term` to leave out.
The page's tabs are views (`View`): needs you, Vakten is on it, all open, fixed this week, came
back, quiet sources, slower, archived; searches can be saved as tabs of their own (`problems.views`).

**Dossier** (`dossier.rs`, `GET /api/problems/{id}/dossier`, tool `problem_dossier`): the commits
that last touched the files in the stack (`git log` in the problem's folder), problems in the same
place or with mostly the same words, every other hit within five minutes from any source, and the
earlier attempts. A thread started on a problem gets a brief with its stack, what came before,
Vakten's reading and how to read the dossier; without a folder the brief goes in a new message box.

**Failure cards** (`fixnow.rs`, `ErrorCard.svelte`): when a thread's engine fails a turn and the
failure becomes a problem, the thread logs a `reported` event and its error shows as a card: sent
to Problems #n and where that stands, with **Fix now** (`POST /api/problems/fix`). The same failure
again right after (an engine that cannot start) is one card, counted; the cards do not count as
"something else logged" when Hub decides a broken engine has failed enough times in a row. Fix now
starts a fix thread on the problem through `start_thread`, the path Vakten's fixes take, with the
failed thread's title, folder, engine, machine and error added to the brief; it is an ordinary
thread that runs in the background and ends at an open pull request like Vakten's. When its first
turn ends, the card in the failed thread turns "fixed" (or "stopped", "failed") with the end of
its reply and its pull request, and offers **Try again**, which sends the message that failed once
more. A fix in a pull request reaches the machine only when it is released.

Other machines: the page asks every online machine through `?host=` and shows their problems in
the same list, each acted on where it happened.

**Guard's panel.** The app calls Vakten *Guard* (the core still sends and stores "Vakten"; the app
translates it for display, `guardName` in `system.ts`). Guard lives here: with no problem picked, the
right column is `GuardPanel.svelte`: its state, the fix threads it has started (`hub.guardThreads`,
what each does, a click opens it), what it last said, a line to tell it something (sent to its
chat) and a link to its chat. Picking a problem puts that problem's detail there instead.

## Routines
<!-- covers: crates/hub-core/src/routines.rs, crates/hub-core/src/routine_runs.rs -->

Work Hub starts by itself on a schedule, such as a daily review of what was committed or a nightly
hunt for weak spots (`routines.rs` keeps the list and ends a run; `routine_runs.rs` schedules and
starts runs; System > Routines). A routine is a name, a folder on this
machine, an engine (the default when left out), a prompt and a schedule: every day, weekdays or
one weekday at a time, or every few hours, in the time zone the app last reported. They are kept as
one list under `routines` in the key-value table, each with its last 20 runs.

- `routine_runs::scheduler` looks every minute. A routine is due at the first slot after its last run
  or after its schedule was set, so one missed while the core was down runs once when it is back,
  not once per missed slot, and a new or re-enabled routine waits for its next slot. A run still
  busy or asking something holds the next one back.
- A scheduled run that fails (could not start, engine error) is retried twice, 10 and then 30
  minutes after it ended (`retry_due`, `by: "retry"`), then waits for its next slot. Runs stopped
  by the user and runs started with Run now are never retried; a run that works ends the streak.
- A run is an ordinary thread titled "<name> · <date>". Its first message is marked
  `routine: { id, name }` and carries Hub's note (report only; numbered findings with file and line
  and a suggested fix; nothing repeated from last time), the routine's prompt and the last report.
  The transcript folds it into one line.
- While that first turn runs, Hub answers every permission question itself and never waits
  (`permit::routine`): `permit::report_only` refuses edits and any command that commits, pushes,
  opens or merges a pull request, installs packages or writes files, and Hub's tools that start
  other work, whatever Settings > Safety allows; reading, and command lines where every part only
  looks (`permit::only_looks`: git log/fetch/diff, rg, cat, tests, linters, PowerShell readers),
  are allowed; the user's own Safety rules count for the rest; anything left is refused with a
  note to put it in the report. It only sees what the engine asks about,
  so an engine set to bypass permissions is held by the note alone. When the turn ends, the reply
  becomes the run's report and the thread settles itself (`routines::settle_run`): the report is on
  the Routines page, and the run, a system thread, is folded under From Hub in the inbox. Answering in it
  ("fix 2") is ordinary work again, which stops at an open pull request like every thread.
- The starter routines (`routines::templates`) are added per folder with one click: Daily review,
  Weakness hunt, Dependencies (checked against Customs), Dead code and TODOs, Test gaps. Nothing
  runs until one is added. Runs use the subscription the engine is logged in with; GitHub Actions
  is not used.

## System threads
<!-- covers: crates/hub-core/src/system.rs, app/src/lib/system.ts, app/src/lib/components/OwnerMark.svelte -->

A thread Hub or an agent started, not the user, carries an **owner** (`Thread.owner`, `{ kind,
name, id? }`, column `owner` since schema step 14). Every place Hub starts a thread on its own goes
through `Hub::create_thread_as` with the owner (and a parent for a side job), so the owner is set when the thread
is made and never later:

| kind | who | name |
| :--- | :--- | :--- |
| `vakten` | Vakten's one chat (the app calls it **Guard**) | Vakten |
| `routine` | a routine's run (`id` = routine id) | the routine's name |
| `merge` | the retired merge queue (old threads only; nothing sets it now; compat-until: 2026-12-09) | Merge queue |
| `widget` | an Overview widget build | Widget |
| `bench` | one repeat of a bench case (`bench/run.rs`, `id` = the run; archived once checked; a system thread like the four above) | Bench |
| `fixer` | a fix thread from Vakten or Fix now (`id` = problem number) | Vakten / Fix now |
| `customs` | Customs' monthly dependency update | Customs |
| `agent` | a thread another thread started through Hub's tools, or a side job (`id` = that thread) | its title |

The first four are **system threads** (`OwnerKind::system`; the app's list is `SYSTEM` in `system.ts`, plain strings, which already holds `bench` for the bench's threads before a core sends that kind): Hub's own work across projects and
machines. They stay in the inbox, out of the way: `hub.inbox` leaves them out, and the activity
list folds them under **From Hub** near the bottom (`hub.fromHub`, open or shut with
`prefs.showHubThreads`). One that waits on you goes to **Hub needs you** at the top instead
(`hub.hubAsks`, heading in amber), and counts like any thread that needs you. System threads are
not projects: `hub.withChats` leaves them out, so Hub's data folder is not a project "hub" in the
lists, and `hub.pinned` leaves them out too (a pinned one that asked was listed twice).

**Guard is the exception.** Its chat and its fix threads (`fixer` named Vakten, `isGuard` in
`system.ts`) never sit in the inbox, not even folded: Guard looks after them itself and they are
listed in Problems and the status bar's Guard card. Only when Guard or one of its threads waits on
you does a **Guard needs you** section (shield) show on top (`hub.guardAsks`). Its chat shows a
shield and a grey "Hub" instead of a project; its fix threads show their project with "via Guard".
A fix thread that gets stuck goes back to Guard, not to you: with a core that has `guard.reports`
(`hub.guardReports`), `waitsOnYou` leaves out a Guard fix thread that asks, is stuck or failed
(`hub.guardHandles`), so no count, list or the Rail shows it; only a live permission question of one
still waits on you. The status bar card and Guard's panel say "back with Guard". An older core keeps
the old way (it waits on you). Their finished turns
do not notify; failing or asking does. The rest are **errands**: done in a project and listed
there, but one still on its first run stays out of the inbox until it hands back.

Every owned thread shows a grey robot and its owner's name (`OwnerMark.svelte`) in the inbox row, the
project page's list, the thread's header and Ctrl K. Colour stays for state.

`system::backfill` runs once per database (kv `system.owners`) and gives threads from before owners
theirs: Vakten's thread from its state (and unpins it; it used to be pinned to stay out of the way),
routine runs from the routines' run lists, fix threads from `problems.thread_id` when titled
"Fix #N", and merge, widget and Customs threads by the titles those give theirs.

## Recipes
<!-- covers: crates/hub-core/src/recipes.rs, crates/hub-core/src/hub/recipes.rs -->

Saved jobs the user starts by hand on any project, like "Update dependencies" or "Fix failing tests"
(`recipes.rs` keeps the list; `Hub::start_recipe` in `hub/recipes.rs` starts a run; System > Recipes, and a Recipes card on the project page). Where a routine runs on a
schedule and only reports, a recipe runs only when started and does the work. A recipe is a name, a
line on what it does, a prompt, an engine (the default when left out) and, if the user ties it, one
folder; otherwise it runs on the project picked under "Run on". They are kept as one list under
`recipes` in the key-value table, each with its last 20 runs (thread, folder, time). How a run went
is its thread's status, so nothing follows the run.

- A run is an ordinary thread named after the recipe. Its first message is marked
  `recipe: { id, name, by? }` and carries Hub's note (do the whole job without asking, new branch,
  the project's checks, stop at an open pull request, Customs for packages, one line if there is
  nothing to do), the prompt and the note typed for this run. The transcript folds it into one line.
- Agents use three of Hub's tools: `list_recipes` and `suggest_recipe` never ask, and
  `run_recipe` (supervised) asks like `start_thread` and gives the run the caller's settings, so it
  never runs with more than its starter. A suggestion is a recipe with `suggested: { by, thread,
  why }`: it waits at the top of the Recipes page until the user keeps it (as it is, or changed) or
  dismisses it, and cannot run before. A suggestion with the name of one the user keeps is refused,
  so the agent runs that one instead. At most 20 wait; the oldest goes.
- The starter recipes (`recipes::templates`) are kept with one click: Fix failing tests, Update
  dependencies (within major versions, checked against Customs), Fix warnings, Add missing tests,
  Bring docs up to date. A routine's run refuses `run_recipe` like the other tools that start work.

## What the models can do
<!-- covers: crates/hub-core/src/capabilities.rs, app/src/lib/components/ModelTable.svelte -->

`capabilities.rs` keeps a table of what each model the engines offer can do. The source is LiteLLM's public model list
(`model_prices_and_context_window.json` on GitHub), which tracks every provider's models with
their context size and whether they see images, read PDFs, use tools, think and search the web.
Hub fetches it at most once a day, keeps the chat models of Anthropic, OpenAI, DeepSeek and
Gemini in `<data dir>/models/catalog.json`, and keeps the last list when a fetch fails.

Each model an engine offered in its latest session is matched by its words: every word of its
value and name must be in the list's name, and the newest such model wins, then the one with
fewest other words ("opus" is the newest Claude Opus, "Luna 6" is `gpt-6-luna`, not
`gpt-5.6-luna`). Dated snapshots are skipped. A model that matches wrong or not at all (Claude's
"Default") is corrected in `<data dir>/models/overrides.json` by engine and value:
`{ "claude/default": { "catalog": "claude-opus-5-5", "web_search": false } }`. `GET /api/models`
(`?refresh=true` to fetch now) serves the table; Settings > Engines shows it folded shut
(`ModelTable.svelte`), with a column per ability, a link to the list on GitHub and, in the
link's tip, how to correct a match. The list's price
per output token is kept as `cost`, a sign of how heavy a model is (on a subscription it is not
what you pay).

## New threads and how agents work
<!-- covers: crates/hub-core/src/agent_prefs.rs, app/src/lib/components/EngineKnobs.svelte, app/src/lib/components/LocalModels.svelte, app/src/lib/components/Exceptions.svelte, app/src/lib/components/Personality.svelte, app/src/lib/agentPrefs.svelte.ts -->

The top of Settings > Engines is two knobs (`EngineKnobs.svelte`): what new threads start on
(the engine by its mark, a model, and effort as a segmented row) and Auto-compact (a dial and
Auto / 60–90 / Off). Each knob is the default, with the projects that differ folded under it
("Except in", `Exceptions.svelte`): shut it names them, open it lists each with what it uses
there, and a click on one edits it in place with the same controls. There is no separate
per-project list. Settings are controls, never words in a sentence (he tried that and did not
like it). How agents write is not an engine thing: it is Settings > Personality
(`Personality.svelte`), with the same "Except in" under it. The page's lead links to Routines
for threads on a schedule. Below them the harnesses are a rack (`EngineList.svelte`):
each engine one row with its mark, a lamp per machine (red only where something is wrong, and
then the reason in words under the row), and a meter of how much of its plan it has used, from
the limits Hub already reads (`GET /api/usage`); a keyed one says it pays per use. A click opens its command, Settings,
Try and Remove.

- **New threads.** For the default engine, the model and effort every new thread starts on ("Last
  used" keeps what its last thread had), and per project its exception: another engine and/or
  its own model and effort (`ProjectStart`). Both live
  in the window's local storage (`startDefaults` in `providers.svelte.ts`); Home lays the pinned
  model and effort over the last-used ones and switches to a project's engine when its folder is
  picked; leaving for a folder with none hands back the engine picked before, and a pick in the
  message box still wins for that thread. A project's model and effort apply to the engine it
  starts on.
- **How agents work.** When a thread compacts by itself (the agent's own point, at 60 to 90%,
  or never) and its output style (short, explanatory, learning), everywhere and per project
  (`AgentPrefs.projects` by folder, `ProjectPrefs`; `agent_prefs::for_folder` lays the nearest
  project that holds the thread's folder over the rest). The core keeps them in the store
  (kv `agent_prefs`, `GET/PUT /api/agent-prefs`) and `connections::apply_agent_prefs` adds them to how each
  agent starts, beside what `connections::launch` switches off: Claude Code gets
  `DISABLE_AUTO_COMPACT=1` or `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`, and `outputStyle` in its
  session settings. Codex has no such switch and keeps its own; Hermes has compression under
  Settings on its row. Agents already running keep what they started with. They travel with
  the engines to every other machine (`Shipment.agent_prefs`, with when they were set; the newest
  wins, kv `agent_prefs_at`), so a thread run on another machine starts the same way.

Under Providers, **Local models** (`LocalModels.svelte`) asks every machine's hubd
(`GET /api/local-models`, `?host=` for the others) whether Ollama (`:11434`) or LM Studio
(`:1234`) answers there and which models it has. It only finds them so far; threads cannot run
on them yet.

## Auto (taken out)

The model picker had an Auto entry that picked the model per message (a quick question to the
fast model, then back). It was taken out on 2026-11-01: it could only step down for small talk and
back, which felt random, and T3 Code and Claude Code have nothing like it either. You pick the
model. What stayed is the hand-off: a picture asked for in any thread goes to the engine that
draws as a side job (see Side jobs), so the conversation carries on. Old threads keep their `auto`
lines, and a new thread asked for with engine `"auto"` (an app that still remembers it) starts on
the default engine.

## Key vault
<!-- covers: crates/hub-core/src/secrets.rs, crates/hub-core/src/apis/vault.rs, crates/hub-core/src/engine_renew.rs, app/src/lib/components/KeyVault.svelte, crates/hub-core/src/credcheck.rs -->

Every secret Hub keeps is in the vault (`secrets.rs`): values in Windows Credential Manager
(`keyring`, service `hub`), elsewhere in `<data dir>/secrets.json` readable only by the user. Only
the real Hub, on the default data folder (`hub_install::data_dir`), uses Credential Manager: its
entries are one per name for the whole Windows user, so the test suite the updater runs before
each build and the dev core on its copy of the data wrote over the real keys (a test's DeepSeek
key ending in 7890, a test removing TYPESAFE_API_KEY). Any other data folder, and every unit test,
keeps its keys in its own `secrets.json`; the dev core therefore starts without the real keys. An engine keeps the environment it started with, so `engine_renew::renew_engines`
(started by hubd) stops, once its turn is over, every engine whose vault keys (`Hub::engine_keys`,
noted at start in `acp.rs`) changed since; the next message starts it on the new key in the same
session. Without it a replaced DeepSeek key showed Works in Settings while open threads got 401. A
harness may also read keys from a file of its own over its environment (`Harness::key_file`:
Hermes reads `~/.hermes/.env` first), so before it starts `engines::clear_stale_keys` comments out
any variable there that Hub hands over with another value; else an old key in that file won over
the vault for new threads too. The
store keeps only the names (`secret_names`) and the dates (`secret_meta`: added, last replaced,
last handed out, the last noted at most every ten minutes). Settings > Key vault (`KeyVault.svelte`,
`GET /api/keys`) lists each key by its last four characters and what uses it (`Hub::key_vault`, `apis/vault.rs`):
engines whose `secrets` name it, API connections that hold it, the projects those are on and
Hub's own decisions the connection makes (`apis::FEATURES`). A key opens on its own page to replace or remove it.
`POST /api/keys` adds a key and refuses a name already there or a model provider's own key
(`config::ENGINE_KEYS`); `PUT /api/keys/{name}` replaces one that is there (rotation keeps the name,
so everything using it gets the new one from its next start) and the page tests the connections
that use it straight away; `DELETE /api/secrets/{name}` removes it from Credential Manager too.
No value ever leaves the core through this API. Well-known keys in the core's environment that
are not in the vault are listed read only. The refusal of a provider's own key sits in
`Vault::set`, so it holds for every way in (Settings, Add engine, `PUT /api/secrets/{name}`);
changes to the vault go one at a time. With `"keys_in_memory": true` in `hub.json` (for an exposed
machine such as the VPS) the vault never touches the disk (`Vault::in_memory`): keys other
machines lend it through `engine_share::share` and the sign-ins they send live only in memory, no
`secrets.json` is written or read (an old one is ignored, not removed), not even the names are
kept, and Settings > Key vault says "memory only". After hubd restarts the keys are gone until
the sender's next push (within a minute), so a thread started in that minute has no key. A key
or sign-in made by hand on that machine is lost the same way. With `"relay_keys": true` on an
always-on machine (tincan) it hands on, besides its own, the engines, keys and sign-ins other
machines lent it (`engines::outgoing`, `Hub::outgoing_sign_ins`), so a memory-only machine gets
them again after a restart while the PC is off. The machine that made an engine does not take its
keys back from a relay (`engines::comes_back`), and a sign-in is only taken when it is newer than
the machine's own. The relay must have the memory-only machine among its hosts. An engine's stderr log (`engines/<thread>.log`) is
redacted like the thread's log, and goes with the thread when it is deleted.

**Testing keys and sign-ins per machine** (`credcheck.rs`, `POST /api/machine/credentials/check`,
feature `machine.credentials`, `?host=` like every machine call). On a machine's page, under Ready
for work, "Test keys and sign-ins" tries each engine for real as a new thread would start it
(`engine_probe::test`: the provider is asked about the key, then the agent is started with the machine's
keys and any sign-in Hub keeps and asked for a session, which spends nothing) and each API
connection with its cheap GET (`test_api`). A row says Works, Refused (the service or agent
said no), Missing (no key, not signed in, not installed) or No answer, plus when a kept sign-in
ends; a row that is not fine carries the same fix as Ready for work (Sign in). No key value is in the
answer. A session opening does not prove a Claude login is still accepted by the server, so a
login that only fails when a turn runs is still caught by `sign_in_lost`. The page of an older hubd
says it needs the next version.

## API connections
<!-- covers: crates/hub-core/src/apis.rs, crates/hub-core/src/http.rs, crates/hub-core/src/apis/, crates/hub-core/src/gateway.rs, crates/hub-core/src/reading.rs, crates/hub-core/src/judge.rs, crates/hub-core/src/jev_tune.rs, app/src/lib/components/AsideLine.svelte, app/src/lib/components/ApiConnections.svelte, app/src/lib/components/ApiUse.svelte, app/src/lib/apiConnections.ts, app/src/lib/apis.svelte.ts, app/src/lib/components/ProviderLogo.svelte, app/src/lib/providerLogos.ts -->

API connections are outside services Hub reaches with a key you add by hand (`apis.rs`), the one
place Hub uses API keys: engines stay on subscriptions. Jev, TypeSafe's decision model, is the
first and the template. The page also lists the model providers (`ProviderList.svelte`, the same
list as Settings > Engines > Providers), since a provider like DeepSeek is a keyed API too. Adding one is the same for every service: you type or pick a name
in Settings > API connections and paste what the service gives you. A known service comes from the
catalog (`apis::catalog`: address, key name, a cheap test path, docs, where the key is made, what
to paste, and how it signs in); any other name asks for its address and key, and its key name is
made from the name. The key goes to the vault under its name (for Jev `TYPESAFE_API_KEY`) and
Test calls the test path. Settings shows each connection with its logo and opens its page in
the service's own colour (`BRANDS` and the logos in `providerLogos.ts`, drawn by `ProviderLogo`).
A known service's page header is a small copy of its own site (`LOOKS` in
`ApiConnections.svelte` (the use Hub makes of a connection is `ApiUse.svelte`; labels and looks are in `apiConnections.ts`): TypeSafe's dithered clouds and big black type, Spotify's green-to-black,
GitHub's dark canvas and contribution grid, OpenWeather's orange); others get a tinted header.
Add connection opens the same card grid as Add a provider, right under your connections.

How a call shows the key is the connection's `ApiAuth`, which you never pick (`gateway.rs`):
Bearer, a header, a query parameter, Basic, or a login. A custom connection starts as Bearer;
when the test is refused, Hub tries the usual other ways (`X-Api-Key`, `?api_key=`, `?appid=` and
so on), keeps the first that works and says so. A login (`ApiAuth::Login`, OAuth with PKCE, as
Spotify) needs only the app's client ID: Open copies `ApiState::redirect`
(`http://127.0.0.1:<port>/api/oauth/callback`, which the service checks letter for letter) and
opens the service's dashboard; Connect saves the connection and opens the Allow page
(`POST /api/apis/{name}/login`); the service sends the browser back to the callback, which is open
without a token but takes only a one-time `state` Hub made in the last 15 minutes. Hub keeps the
refresh token in the vault under the connection's key name (`SPOTIFY_REFRESH_TOKEN`), access
tokens only in memory, renewed when one is a minute from running out or refused. Logging in works
from the machine the core runs on. The connections,
the projects they are on and the counts live in the store (`apis.prefs`, `apis.usage`, which keeps
62 days, and `apis.totals`, everything since Hub started counting, sent as `ApiStatus.total`); keys
only in the vault. A connection can carry the credit you put on its account (`ApiConnection.credit`),
since services like TypeSafe have no way to ask for a balance: when its amount changes Hub stamps
the total input tokens and the time, and what is left is the amount less what the tokens since
cost, which the Jev card counts down. Beside each kept decision Hub keeps the whole of what the connection was
asked and read (`apis.seen`: instructions, options, state, threshold), apart from the decisions so
the status bar's list stays small. `jev_tune` writes those, with each decision's thread, what the
person wrote there in the half hour after, the features' settings and what Hub does without Jev,
to `<data>/jev-tune/brief-<ms>.md` (the last five are kept); `POST /api/apis/tune` (`apis.tune`)
returns its path and an ask for a thread. `POST /api/apis/tune/run` (`apis.tune.run`) does the
same in the background instead: one `engine::complete` on the tidy engine with Sonnet, the brief
inline and cut to about 80,000 characters (unsure decisions and ones unlike Hub's own rule go in
full first, the rest one line each), asking for at most six suggestions as JSON: more context for a
feature, better wording, or a new threshold, each naming the decisions that back it. They are kept
in `apis.tune` (`TuneState`, `GET /api/apis/tune/state`, `running` while one reads; one at a time).
`POST /api/apis/tune/answer` uses or skips one: using a threshold sets `ApiPrefs.sure`, using the
others only marks it, for a thread to build. The last test or call of each is kept (`apis.checks`), so the list in Settings shows
each connection as working, failing, not checked or without a key, and checks again one not checked
for six hours when it opens; a connection opens on its own page with the rest. A connection can never hold an engine's key (`ENGINE_KEYS`, such as
`ANTHROPIC_API_KEY`), since that would move an engine onto paying per token.

They are used two ways:

- **Projects.** A connection turned on for a project (Settings, or the project page's APIs card)
  gives that folder's threads its key and its address as environment variables
  (`TYPESAFE_API_KEY`, `TYPESAFE_BASE_URL`) from their next start, for the code they write and
  run. A connection signed any other way than Bearer stays with Hub: the threads get only
  `<NAME>_BASE_URL` (`SPOTIFY_BASE_URL`), an address on Hub's gateway (`/gw/<key>/…`). The key in
  that address stands for one connection in one project (`apis.gateway` in the store) and stops
  working when the project is turned off; the gateway puts the service's address in front, signs
  the call, renews a login, passes the answer back and counts the call. A browser can't call it. Hub's tool `api_connections` tells the agent what it has, by variable name, never the value.
  For a quick decision an agent asks Hub's tool `ask_jev` (`apis::Hub::ask_for_agent`) instead of
  writing the call: a `question` and a `text`, optionally `options` (2 to 8, it picks one) or
  `items` (up to 20, the same question each); without options it answers yes or no. It goes through
  `Hub::choose` as the feature `agent` (`FEATURES`), so it is counted, logged in `apis.decisions`
  with the thread, and `sure` means 0.6. It is a quiet tool and never an error the agent is stuck on:
  Jev off, off in this chat, slow (4 s), unsure or the allowance spent each answer `answered: false`
  with "Decide yourself." The allowance is 200,000 tokens a day across agents and 50 calls a thread,
  kept in memory (a restart gives a fresh one; the connection's own `daily_tokens` still applies).
  Two switches, both in the Jev card: all agents is `agent` in `ApiPrefs.off`, and one thread is
  `GET`/`PUT /api/threads/{id}/jev` (`{ "agents": bool }`, kept as a list of thread ids in
  `apis.agent_off`; `agents` reads false when either switch is off). Compat feature `apis.agent`.
- **Hub itself.** `Hub::ask_api` asks a connection that answers questions (`asks` in the catalog)
  typed questions about a text, with a short wait, one retry on 429/529, and a daily token cap
  per connection. Every call is counted per day. `Hub::choose` asks one choice question for one
  of Hub's own decisions (`apis::FEATURES`) and Hub goes by the pick only at 0.6 confidence or
  more; unsure, slow or missing, it does what it did before. Every answer is kept next to what
  Hub would have done (`apis.decisions`, last 300), with the words it read in short (`quote`: the
  end of the agent's reply, what you wrote, the command) and the tokens the call took, and listed
  on the connection's page and in the status bar's card. The connection asked is `helper` (null:
  the first that answers and has its key); `off` lists features turned off; `sure` holds a
  threshold you changed per feature (50% to 99%), while stopping a working thread and offering a
  new one keep their own higher bars. These use it:
  - **Turn ends** (`reading.rs`): after each turn that ended normally, the ask and the end of the
    reply are read as done, asks (waits on your answer), stuck or partway. A sure reading is
    `Thread::reading` (cleared when the thread is sent work), logged as a `reading` event; asks and
    stuck count as needs you in the lists and bring the thread back from settled or snoozed, and a
    thread read as done that you have seen settles after a day.
  - **Focus check** (`focus.rs`): the new thread's message against the active stage's open
    "Delivered when" lines, its ideas and "none". A sure "none" is the off-stage note; a sure line
    is a `reading` event shown under the first message. Unsure, the small model decides as before.
  - **Test output** (`proof.rs`, `Hub::hides_failure`): a test run that exited cleanly (as through
    `| tail`, where the pipe's exit code wins) is read for failures it printed anyway. Only runs
    whose output holds a failure word (`proof::may_hide_failure`, not "0 failed") are asked about,
    at 0.75 confidence, so most runs cost nothing. A sure "failed" makes the proof state `failed`
    with `hidden`, and the agent is asked to look at the whole output.
  - **Same task?** (`Hub::check_topic`): a message you write to a thread that has finished a turn
    is read against how the thread began and its last reply. A sure "new" (0.75) logs a `reading`
    event with `feature: "topic"` and the transcript offers a thread of its own for it
    (`TopicNote.svelte`, opens a draft; nothing is sent twice).

### What you write to a working thread (`judge.rs`)

A message sent while a turn runs is logged and queued as always. When Jev is connected, the `judge`
feature (`apis::FEATURES`) then reads it (`Hub::judge`, started from `send_once`) with the
thread's first message, the agent's latest steps and words, and whether it waits for you, and
picks one of `ask`, `stop`, `steer`, `later` or `new`. `ask` is answered by the small model tidying
uses from the thread's log (the agent never hears of it) and the question leaves the queue; `stop`
(needs 0.8) cancels the turn and drops the message; `steer` steers; `later` stays queued; `new`
logs the same `reading` the "different task" note uses. A missing, slow (4 s) or unsure Jev leaves
it to your own queue or steer setting, so nothing is lost, and a message with files skips it. The
result is one `aside` stream message (`StreamMsg::Aside`, `GET /api/threads/{id}/aside`, feature
`chat.aside`), kept in memory and never in the log; `AsideLine.svelte` shows it as one grey line
with the time above the status line while the turn runs, and a click copies an answer.

## The viewer
<!-- covers: app/src/lib/components/Viewer.svelte, app/src/lib/components/ViewerWindow.svelte, app/src/lib/components/BrowserFrame.svelte, app/src/lib/preview.ts, app/src/lib/components/Lightbox.svelte, app/src/lib/lightbox.svelte.ts -->

What an agent shows opens big in one viewer in the right panel (`Viewer.svelte`,
`hub.viewing = { thread, kind, id }`), and the thread panel's Shown list lists it all. The viewer
is one frame for every kind: the name, "2 of 3" with the arrows (and ← →) through the others of
that kind, full screen, and close (Esc leaves full screen first). Each kind brings its own tools to
the bar and fills the body. A page keeps its bar to the name, ‹ v2 of 3 › through its versions, ⋮
and close, as Claude Code's artifact panel does: Reload, Open in the browser, Copy the link, its own
window and full screen are in ⋮, around `PageFrame full`.

A picture opens over the whole window instead (`Lightbox.svelte`), not in the panel: the picture
on a dark veil with its name, "2 / 6", Download, Copy, ⋮ (Share where the system has a share sheet,
Show in folder, the context bag) and close; arrows, ← → and a strip at the bottom step through the
thread's other pictures, − Fit + (or Ctrl and the wheel, double-click) zoom, and a drag moves a
zoomed picture. Show in folder is left out for a picture of a thread on another machine (its path means nothing
on this computer's Explorer; Copy image and Copy path still work). Esc or a click beside the picture closes it. The desktop app has no download bar,
so Download asks the core to save the picture in Downloads, and the bar offers to show it there. A
picture popped out into a window of its own still shows in the viewer. A new kind of shown thing is a kind here, not
a viewer of its own; a file a reply named or changed is one (`kind: "file"`, see Files read as what
they are). When the window is too narrow for a panel, the viewer covers it. Its edge
drags to resize it (remembered in `prefs.viewerWidth`). Pop out opens it in a window of its own
(`open_viewer_window`, `ViewerWindow.svelte`), told what to show by an initialization script since
an app URL keeps no query; what it starts there (a thread to send to, a draft) happens in the main
window, over the `hub-main` BroadcastChannel.

The viewer's other kind is a dev server beside the thread (`kind: "browser"`, the id its URL;
`BrowserFrame.svelte`, `preview.ts`). A port in the thread panel, a `:5173` in a reply and the
server menu open it there, Ctrl-click in the real browser. It has an address bar, back and
forward through the addresses gone to, Reload and Open in your browser. Only this machine's
localhost opens there, never Hub's own port, and not for threads on other machines; the frame
keeps the server's own origin (`allow-same-origin`, which is safe only because that origin is
not Hub's), so logins and hot reload work. The Tauri CSP's `frame-src` allows localhost ports.

A page runs the agent's scripts, so it is served with `Content-Security-Policy: sandbox
allow-scripts` and never `allow-same-origin`: an opaque origin with no cookies, storage or Tauri
bridge, which the API refuses like any other foreign origin. `connect-src 'none'` stops it from
fetching anything, so it cannot send data out or reach local services; scripts load inline or
from cdnjs, jsdelivr or unpkg. The frame and the browser cannot show a token, so that one GET path
is open, and the two random ids in it are what let one in. The page talks to the app only through
`postMessage` from its own frame: its height, a request for Hub's colour tokens, and an http(s)
link to open in the browser.

## The context bag
<!-- covers: crates/hub-core/src/bag.rs, app/src/lib/bag.svelte.ts, app/src/lib/components/BagMenu.svelte, app/src/lib/components/BagDrop.svelte -->

Files, pictures, links and notes you carry between threads, in any project (`bag.rs`,
`bag.svelte.ts`, `BagMenu.svelte`). The core keeps them, one `bag:<id>` value each with its
content, so every window and the phone see the same bag; `GET /api/bag` lists them without
content (pictures bring a small thumbnail the app made) and `GET /api/bag/{id}` has it all. The
same thing put in twice is one item; the bag holds 100 things of up to 20 MB each.

The + menu in the composer opens it (Context bag): click an item to add it to the message, Add all, the eye
shows it in the viewer (`kind: "bag"`), and files dropped on it, or a link or note pasted into its
box, go in. An item dragged onto a thread's row in the sidebar goes to that thread's composer, the
same way as files dropped there (`drafts.svelte.ts`, `dropped`). Things go in from where they are:
the bag on an attachment chip, Put in the context bag on a sent attachment, and the bag in the
viewer's bar for a picture or a dev server's address. A link goes along as an attachment with a
`url`, which an ACP engine gets as a `resource_link`.

The bag does not have to be open: while anything is dragged over Hub, `BagDrop.svelte` shows a
drop zone near the top, and `putDropped` (`bag.svelte.ts`) takes files as they are, a picture
dragged out of a web page as the picture (the browser hands over only its address, so the core
fetches it: `POST /api/bag/picture`, `image/*` only, 20 MB, 20 s), a link as a link and other text
as a note. A thread goes in by dragging it out of the list onto the zone or with Put in the bag in
its menu, as a note with its title, folder, engine and conversation (the latest ~40k characters).
Ctrl V in the open bag takes screenshots and copied files.

## Welcome
<!-- covers: app/src/lib/components/Welcome.svelte -->

A short guide (`Welcome.svelte`, view `welcome`, Extend in the sidebar and "Welcome" in Ctrl K), titles only: five
steps for after a project is created, each with a jump where one fits, and four places to know
(Overview, Machines, Library, Problems). Plain text and buttons, no state of its own, and it changes
nothing. A new step or place is a line in the two lists at the top of the file.

## Labs
<!-- covers: app/src/lib/components/Labs.svelte, app/src/lib/components/ActivityMap.svelte, crates/hub-core/src/activity/, crates/hub-core/src/sim.rs, crates/hub-core/src/hub/sim.rs -->

Experiments get a page of their own before they get a place (`Labs.svelte`, "Labs" in the
sidebar). Each one is a self-contained component, so moving it to Home or the project page is one
line. Today it holds Work on Hub (`DevMode.svelte`), the inbox simulator (`SimInbox.svelte`) and
the bench card (`BenchCard.svelte`, see `architecture/bench.md`). The first was the activity map (`ActivityMap.svelte`, `crates/hub-core/src/activity/`), now a
card on the Overview. It lights a year of days from four sources, each of which can be switched
off on the map:

- Hub's log: your messages and the agents' finished turns.
- Claude Code outside Hub (`activity/claude_code.rs`): prompts and tokens from the session files
  in `~/.claude/projects`, with subagents' files under `<session>/subagents/` (tokens only). They are copied into `cc_marks` and `cc_sessions` as the files grow
  (the read offset per file is in `kv`), because Claude Code deletes sessions after 30 days by
  default. Sessions that are Hub threads (`engine_session_id`) count only their tokens.
- Your commits (by `user.email`) in every repository a thread or project lives in.
- GitHub (`activity/github.rs`): the contribution calendar, commits per repository and pull
  requests, through `gh api graphql`, cached for ten minutes. A day counts the larger of local and
  GitHub commits, since they overlap.

A day's devblog entry is written in the night after it (`Hub::devlog_daily` in `hub/devlog.rs`,
`Hub::summarize_day`) by the default engine with its own model, from the day's activity plus the versions released that day
(`notes.rs`) and threads that ended failed or waiting: a one-line gist, then Shipped, Worked,
Blocked and Next. It is kept in `kv` as `activity.summary.<date>`, with `devlog.blog.<date>` marking
the new form (days in the last week with only the older few lines are written again). Clicking a
day (in the Day card on the Overview) reads the stored entry (`GET /api/devlog/day`) and shows it at once; the threads, sessions and
commits behind it (`GET /api/activity/day`, which imports and reads git) load only on Show what
happened.
Reading the log does not diff: the lines each commit added and removed (lock files and binaries
left out) are counted only for the Code in and out card, by `count_lines` with `git show
--numstat`, once per commit ever. The counts are kept by hash in `kv` (`activity.lines`), since
diffing a few thousand commits takes minutes on Windows. A day's commits are the larger of local
commits and GitHub's commits (`github_commits`), not GitHub's other contributions.
