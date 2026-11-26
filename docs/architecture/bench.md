<!-- för: agent -->
# Architecture: Bench

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## Bench
<!-- covers: crates/hub-core/src/bench.rs, crates/hub-core/src/bench/, crates/hub-core/bench/, crates/hub-core/src/permit/bench.rs, crates/hub-core/src/api/bench.rs, crates/hub-core/src/hub_tools/bench.rs, crates/hub-core/src/model/bench.rs, app/src/lib/bench.ts, app/src/lib/components/BenchCard.svelte -->

A fixed suite of prompts run as real threads, checked with hard checks read from the event log and
the run folder, timed, and compared across Hub versions. Runs only when asked: the tool `bench_run`
(full access only), `POST /api/bench/runs` or the card in Labs (below); the results come back in the asking thread through
`bench_report`. Defaults agreed 2026-11-11: the smoke suite, three repeats, the engine's cheapest
model. Two suites are built in (`BUILTIN`, `bench::builtin_names()`, smoke first as the default):
`smoke` (small coding tasks) and `features` (Hub's own features end to end, below); any is picked by
name (`BenchAsk.suite`, `bench_run {"suite": "features"}`).

**Suite** (`crates/hub-core/bench/<name>.toml`, baked in with `include_str!`, `bench::builtin(name)`).
Top keys, all required: `suite` (name), `engine` (an engine name from the settings; data, the code
names no engine), `model` (`"fast"` = the engine's cheapest, `EngineModels.fast`; else a model name
picked like a thread's), `repeats`, `timeout_s` (per prompt, until the thread settles),
`cost_cap_usd` (whole run). Each `[[case]]`: `id` (unique), `prompts` (sent in order), optional
`mode` (words for the engine's mode), `[case.files]` (path = content; laid down by `lay_down` in a
new empty folder, then `git init` and one commit by Hub, through `git.rs`/`checkpoint::run`),
optional `[case.attach]` (file name = text; sent with the first prompt as text attachments, as the
app sends a dropped text file, by `suite::attachments`; never laid down, so the agent can only
answer from the message), optional `needs` (a list of words for what the machine must have, so a
case that cannot run here is not read as Hub failing it; one word so far, `handoff_tool`: one of the
person's tools takes handoffs, `own_tools::takes_handoffs`, the folders `offer` looks in, files read
only; an unknown word is refused at parse like a check word), and one or more `[[case.check]]`, each with exactly one check word plus that word's own keys. Parsing
is strict: an unknown key or check word is an error naming the case and the key (a typo must not
become a check that always passes); fixture and checked paths must be relative and stay inside
(no root, drive, `.` or `..`; attachment names too); `unchanged` paths must be fixtures; regexes
must compile; an `event` must name a known `EventKind`.
**Version**: `suite_hash` = first 12 hex of SHA-256 of the file's text with CRLF made LF
(`.gitattributes` keeps `bench/*.toml` LF too). Any edit, a comment included, starts a new result series.

**Checks** (`bench::check(case, events, folder, sent) -> Vec<CheckResult { check, ok, why }>`;
`sent` = the `user_message` seq of each prompt the runner sent; `check` is the label as written):

| Word | Reads |
|---|---|
| `ended = "end_turn"` | the `turn_end` whose `prompt` is each sent seq has that `stop_reason` (fails with no prompt sent) |
| `no_error = true` | no `error` event |
| `settled = true` | every `permission_request` has a `permission_resolved` with its `id`; every `ask` (`ask_user`, which never blocks) has a later `user_message` quoting its question, as the app's answer does. The thread's WaitingOnYou status is not in the log: the runner adds that |
| `refused_max = N` | `permission_auto` with `allowed = false`, at most N |
| `file = "x"`, `matches = "re"`, `lines = N` | the run folder's file exists, matches, has N non-blank lines |
| `unchanged = [..]` | each file's bytes equal the case's fixture |
| `reply = "re"` | `replies::last_reply` (the agent's message chunks of the last turn) matches |
| `names = [..]` | the last reply names each file on its own (after a space, `/`, tick or quote; not part of a longer name; any case), and a file of that name is somewhere in the folder (not `.git`) |
| `hub_tool = "x"`, `min = N` (default 1) | distinct `toolCallId`s of `tool_call`/`tool_call_update` updates that `permit::hub_tool_name` reads as Hub's tool `x` under any engine's naming |
| `proof = "shown" \| "failed" \| "none"` | the last `proof` event: "shown"; "failed" = asked, missing or failed; "none" = no proof event or only "skipped" |
| `page = true` | a `page` event |
| `steps = N` | the last `plan` update (`update_steps` writes ACP's shape) has N entries, all `completed` |
| `commits = N` | `git rev-list --count HEAD` minus the fixture commit is exactly N |
| `event = "kind"`, `matches = "re"` | an event of that `EventKind` (its snake_case name) is in the log; with `matches`, one whose payload as compact JSON text matches (added for the features suite: a side job's `child` "done", a `handoff_offer` and its note, which no other word reads) |

**Measurements** (`bench::measure(events, sent, asked_model) -> Measured`, log only, reusing
`timings::turns`): `wall_ms` (first sent prompt to the last `turn_end`), `first_word_ms` (prompt to
the first message chunk of the first turn), `engine_ms` and `model_ms` of the first turn, `turns`
(Hub's follow-ups included), `tool_calls`, `tool_failed`, `proof_rounds` and `docs_rounds` (user
messages Hub sent marked `proof`/`docs` by `hub/after_turn.rs`; one message asking both is marked
`proof`), `perm_allowed`/`perm_refused` (`permission_auto`), `tokens` (sum of `TurnEnd.usage` over
the turns, `None` when none carried any), `cost_usd` (from `usage_update.cost.amount`, a running
total per agent session: the last amount plus the peak before each drop). The repeat's tokens and
cost include its side jobs: each child's log (ids from the `child` events) is read by the same code
(`measure::spent`) and added by `Measured::with_side_jobs`, which counts them in `side_jobs`, so the
summed `cost_usd` the cost cap reads (`results::cost`, `over_cap`) covers them; times, turns, tools
and validity stay the parent's own. `model` (the last
`usage_update` model) of the parent. Per turn vs cumulative was settled by reading claude-agent-acp 0.99.1 and its
SDK: usage per turn, cost cumulative per session; codex-acp ships as a binary and sends no cost, its
per-turn usage is unverified (note in `measure.rs`).
**Validity**, apart from pass/fail: `invalid` is set when a `quota_switch` is in the log or a
model other than `asked_model` answered (same name, or it with a `-dated` or `[variant]` suffix).

**Summary** (`bench::summarize(case, &[(passed, &Measured)])`): valid runs only (invalid counted
apart), pass count k of n, median (`timings::spread`) and min-max of `wall_ms`, `first_word_ms` and
total tokens (all four counts). `bench::flag(before, now)`: pass count down by 2+ red, by 1 amber;
a median above the previous by more than max(previous min-max spread, 20 %) amber. Amber twice in
a row turning red needs the run history and belongs to the runner.

**Runner** (`bench/run.rs`, free functions on `&Arc<Hub>`). One run per Hub at a time; cases in
order, each case's repeats in order, one thread at a time. A repeat: a fresh folder
`<temp>/hub-bench-<run>/<case>-<n>-<word>` (a random word, so a repeat run again after a restart starts clean; prefix in the janitor's list, so leftovers go after a day; not
under the projects root, where proof is skipped), `lay_down`, a thread from `create_thread_as` with
owner `OwnerKind::Bench` (a system kind: folded under "From Hub"), title `Bench · <suite>/<case> ·
<n>/<N>`, the resolved model as its setting and `Unattended { by: "Bench", prefer: {} }` (no mode
words: the engine keeps its default mode, in which it asks before it acts). Each prompt is sent
(the first with the case's attachments) and
waited for with `thread_wait::wait`, then until the thread has logged nothing for 5 s, is not working and has no newer
message from Hub (a proof or docs follow-up from `after_turn` is waited for the same way; a side job
whose last `child` event still says "working" counts as working, and its result comes back as such a
message), up to
`timeout_s` per prompt (then the turn is cancelled: `timed_out`). Then `check` (the `settled` check
also fails when the thread's status is WaitingOnYou) and `measure` (asked model = the resolved
one), each turn's usage kept apart (`turn_usage`), its side jobs (the `child` events) stopped if
still working and archived, the thread archived, the folder removed on a pass
and kept on a fail. "fast" resolves once per run through `model_table` (`EngineModels.fast`), a name
through `engine_models::pick`; an engine that has never run here is refused with words. At the
end kv forgets the run, then the ended file is written and the run unregistered under the one lock
`going` takes, so a run whose file says ended is never still going (no `progress`, `stop` answers
no, a new run may start; `bench_api::a_run_that_reads_as_ended_is_no_longer_going`).

**Safety.** Hub answers a bench thread's permission questions with `permit::bench`
(`unattended_by("Bench", ..)`, routed in `hub/permissions.rs`): refused when a path the call names,
or a path-like word of its command line (also what follows an `=` or a redirection inside a word:
`--out=/x`, `of=../x`, `x>/y`), is outside the run folder, any word the shell expands into
something that could be a place (`$x`, `${x}`, `$(..)`, `$env:x`, backticks, `%x%`, a `~` anywhere;
`$?`, `$$`, `$!`, `$#`, `$0`-`$9`, `$@`, `$*` pass), and any `git push` (paths are read as text,
the same on every system: `\` separates too and a drive letter is absolute, so `..\x` and `C:\x`
are places on Linux as well; case and Git Bash's `/c/...` count only on Windows); the rest is
`unattended`'s answer (Hub's own files, the user's rules and protected files, no delete, no
command `risky` names). Plain `unattended` (fix threads) looks at no paths at all: a write anywhere
and a plain push are allowed (pinned in `permit/tests.rs`). Not fenced: the network, and what a
program does on its own (a script writing elsewhere). A Hub whose temp folder lies inside its own
data folder (as the update build's tests run, `update/steps.rs`) would see every write of a case
refused, since `hub_files` refuses Hub's data folder for every thread; the permit tests therefore
use a folder of their own, not the temp folder. A run refuses to start unless the guard
answers a write outside the folder with a refusal, and refuses a case that sets `mode` (a mode can
stop the engine asking, and then Hub's answer is never asked). What the engine does without asking,
by the user's own engine settings (an allow list in the engine's config), never comes past Hub.
A side job a case's agent starts (`start_side_job`) gets the parent's `Unattended { by: "Bench" }`
(`children.rs`, only for a Bench parent), so it runs under the same answer in the same folder and
never waits on the person. The guard stays shut to every `git push` (decided 2026-11-11 for the
features suite): a push to a bare repository inside the run folder was weighed and refused, because
the call text cannot settle where a push goes. `git push <url or path>`, `--repo=`, a `pushurl`,
an `insteadOf`/`pushInsteadOf` rewrite in the user's global config, `git -c remote.x.url=..`,
`--receive-pack`/`--exec` (runs a program), and a remote re-pointed earlier in the same command
line (`git remote set-url .. && git push`, read before the line runs) each need their own reading,
and a hook in the bare repository runs code anyway; one miss pushes to a real remote with nobody
watching. So the features suite judges a commit on a named branch, not a push.

**Features suite** (`bench/features.toml`). A bench leaves the person's own Hub as it found it:
a case may change only its run folder and its own threads (archived after the repeat); a tool
that would change real user state (rules, projects, inbox, a tool's work) is either aimed at
something the bench owns or left out, with the reason here. The cases, and what each may touch:

| Case | Judged by | Outside the run folder |
|---|---|---|
| `files-in-chat` | `[case.attach]` receipt, `reply` with the taxi amount, `unchanged` | nothing |
| `side-job` | `hub_tool = "start_side_job"`, `event = "child"` "done", `file` sum.txt = 137 (written from the job's answer), `unchanged` numbers.txt, `settled` | one child thread in the same folder, under the bench guard, stopped and archived with the repeat. Its engine and model are the person's pick for "quick" (Settings > Handoffs); its tokens and cost are in the repeat's and under `cost_cap_usd` (Measurements) |
| `handoff` | `hub_tool = "offer_handoff"`, `event = "handoff_offer"` whose `why` names `--color`, `unchanged` cli.js | reads the person's own tools (`your_tools` runs their status commands, as the Workers view does; `offer` reads a `hub.toml`) and logs a card in the bench thread. Nothing is handed over: that takes the person's press (`hand_off`), which the bench never makes. `needs = ["handoff_tool"]`: on a machine without a tool of the person's that takes handoffs (Knegarn), every repeat is `invalid` ("needs one of your tools that takes handoffs", `run::NO_HANDOFF_TOOL`), nothing is laid down or sent and no tokens are spent; the report lists it under "Not counted", not as a failure |
| `branch-commit` | `file = ".git/HEAD"` on `refs/heads/bench/greeting`, `commits = 1`, hello.txt changed, `refused_max = 0` | nothing (push is not benchmarked, see Safety) |

Left out: **a rule saved from the chat**. No agent tool saves a rule: saving is the person's
button (`POST /api/threads/{id}/rules`, `Hub::save_rule`, a `rule_saved` event), which writes the
engine's rules file in the thread's folder, and the rule is read only when the next engine session
starts. A bench case sends prompts only and has no step to press it or start a new session, so it
would test the agent's memory within one session, not Hub. Parked in `docs/FOCUS.md`.

**Guards and validity.** Refused to start: every agent paused, another run going (or one still in
kv), an unknown suite/engine/case, the plan engine's fullest limit at 60 % or more or within 10 of
the quota guard's `switch_at` (`usage.plan()`, `guard_prefs()`: the figures `quota_switch` reads).
Between repeats a run ends as `stopped` (a stop call, or a pause), `cost_cap` (summed
`cost_usd` at the suite's cap), `plan_limit` (80 %) or `hub_updated` (`update_status().version`
changed). A repeat is `invalid`, never failed, with a `quota_switch`, a spent plan (an `error`
that `resume::limit_hit` reads as a plan limit by the engine's `limit_phrases`, or a `quota_wait`:
`measure::PLAN_LIMIT`, "the plan limit was reached"), another answering model, a version change,
or a case `needs` this machine does not meet (checked before the repeat starts anything).
A spent plan also ends the run as `plan_limit` right there (its thread's wait for the reset is
stopped), and the report says how many repeats were not run. A refusal by Hub (`perm_refused`,
with `permission_auto.why` kept in `Measured.refused`) gets a line of its own in the report, so a
false refusal by the stricter guard is visible.

**Restart.** kv `bench.current` holds the run id, the suite's text and hash, case, repeat, thread
and the version at start. At start-up (`bench::run::resume`, spawned by hubd) the cut repeat gets an
`interrupted` row and its thread is archived; the same version repeats it and goes on (a run that cannot, say because its model is unknown, ends as `failed`), another ends
the run as `hub_updated` keeping what finished. Installs are never held back.

**Storage** (`bench/results.rs`, `&Ctx`): `<data>/bench/runs/<started>-<suite>.json` (`BenchRun`
in `model/bench.rs`: version, suite and hash, engine, asked and answering model, machine, times,
state; rows with case, repeat, thread, state, pass, checks, measurements, turn usage, invalid,
kept folder; `summary`) and a markdown report beside it, rewritten after every repeat: per case k/n now and
before, median and min-max of wall time, first word and tokens, the flag, the cost, what three
repeats can and cannot show, failed repeats with thread ids, and a note when every turn's count rose
over the one before (an engine reporting session totals). "Before" is the newest earlier finished
run with the same suite hash, engine and answering model; none is "new series". Amber in the run
before too turns amber red. The case table is data first: `results::summary(run, before, earlier)
-> BenchSummary {before: {id, hub_version}?, cases: [BenchCaseSummary]}` (per case `passed`,
`runs` (valid), `invalid`, `wall_ms`/`first_word_ms`/`tokens` as `BenchRange {median, min, max}`
or null, `cost_usd` summed over the case's repeats; tokens and cost read each row's stored
`Measured`, so they include its side jobs as the report and estimates do, `before: {passed, runs}?`, `flag: {level:
"green"|"amber"|"red", why: [..]}?`; not run when `runs + invalid` is 0; flag null when not run
or new); `save` stores it in `BenchRun.summary` and `report` renders the table from it alone, so
the two never disagree (`the_summary_and_the_report_table_agree`). `bench::{Range, Level, Flag}`
are these model types. A run stored before the field (no `summary` key) reads as before and
gets it worked out on read (`load`, `list`) when it has rows.

**API and tools.** `POST /api/bench/runs` (`BenchAsk {suite, engine?, model?, repeats?,
cases?}`), `GET /api/bench/runs` (newest first, without rows, with `summary`), `GET /api/bench/runs/{id}` (`{run,
report}`, `run.summary` the table as data), `POST /api/bench/runs/{id}/stop` (ends after the current repeat; ends at once a run only marked in kv that Hub could not carry on; 409 otherwise); feature `bench.runs` in
`compat::FEATURES`, `bench.summary` for `BenchRun.summary`. `GET /api/bench/suites` (feature
`bench.suites`; `bench/catalog.rs`, `&Ctx`) lists every built-in suite that parses (whatever
`BUILTIN` holds) as `BenchSuiteInfo {name, hash, cases, engine, model, repeats, timeout_s,
cost_cap_usd, estimates}`. `estimates` is history only, never a formula: one `BenchEstimate
{engine, model, repeats, runs, tokens?, cost_usd?}` per engine, resolved `model_asked` and repeats
among the `done` runs of that suite hash over all its cases, newest group first; `tokens` and
`cost_usd` are medians (`timings::spread`) of each run's totals, null when no run said; no such
run, no entry. Tools `bench_run` (starts, answers at once with the id and what it will do) and
`bench_report` (the stored markdown, rendered from the summary, latest or by id, "Still running: case X, repeat n/N" while it goes), both
registered for full access only (`hub_tools/bench.rs`, `registry.rs`). An app or hubd that does
not know the owner kind `bench` reads the thread without an owner (`compat::thread_from_wire`).

**Card** (Labs > Benchmark Hub, `BenchCard.svelte`; pure parts in `app/src/lib/bench.ts` with
`bench.test.ts`). Gated on `hubdHas("bench.runs")` (older hubd: "needs the next version"). With
`bench.suites` it lists `GET /api/bench/suites` in a `Select` (default `smoke`); the intro names the
suite, its case count and model, the 60 %/80 % guards and the suite's `cost_cap_usd` (`costNote`),
and an estimate: the first `estimates` entry whose `engine` and `repeats` equal the choice with the
suite's defaults filled in (`estimateFor`, `estimateText`: "Earlier runs like this took about N
tokens, $X (median of k runs)"; none: "No earlier run to estimate from"). Engine (`engines.names`)
and repeats (1, 2, 3, 5) are optional, left out = the suite's, and the default option names it
("claude (suite default)", `engineChoices`/`repeatChoices`); Start says which suite it starts.
Without `bench.suites` (compat-until 2026-12-12): smoke only, "Suite's …" labels, no estimate.
A refused start or stop shows the server's `error` text as a sentence (`refusal`). While a
run is `running` it polls `GET /api/bench/runs/{id}` every 3 s (`poll`), shows "Case i of n (id)
· k of N repeats done" (`progress`; `interrupted` rows not counted), a Stop (the answer's `note`),
and the table so far. The latest five finished runs list one line each (`runLine`, from the
row-less list); opening one shows per case: each repeat's verdict (pass, fail, invalid, not run)
from `run.rows`, its tooltip with the row's `invalid` or `note` (why it was not counted or not run),
k/n over valid repeats (the run before's k/n in its tooltip), the flag with its `why` lines in the
tooltip, median tokens (all four counts) and wall time, summed `cost_usd`, then the state, total
cost, what the flag compares with (`comparedWith`: `summary.before`'s Hub version, or "new series")
and the markdown report folded. `caseRows(run, report)` alone decides the source: `run.summary` when
present; else (an older hubd or a run without it, compat-until 2026-12-12) it counts k/n and
medians from the rows itself and reads the flag with `flagsFromReport` from the report's Flag column.
