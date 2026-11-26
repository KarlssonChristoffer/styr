# Architecture: Guards, tools and side jobs
<!-- för: agent -->

Part of [ARCHITECTURE.md](../ARCHITECTURE.md); the sections keep their names.

## Where it goes
<!-- covers: app/src/lib/components/WhereItGoes.svelte, app/src/lib/components/WhereZoom.svelte, app/src/lib/apiPrices.ts, crates/hub-core/src/spend.rs, crates/hub-core/src/spend/engines.rs, crates/hub-core/src/cloud_usage.rs, crates/hub-core/src/codex_live.rs, crates/hub-core/src/plan_meter.rs -->

Settings > Where it goes (`WhereItGoes.svelte`) shows all AI use of today, 7 or 30 days or all
time, in Hub and outside it, so Hub's own use is never out of sight. All time (feature
`usage.spend.all`, `all=true`) counts from the first day this Hub has anything for and says that
date ("since …"); a range over two months is drawn in weeks, over about a year in 30-day blocks,
counted back from today. An older hubd greys the button out. One total, then a card per place it
happened, each with its week of bars on one shared scale and its share of the total: **Hub**
(everything run through Hub, its bars split per engine), **Claude** and **Codex**. The page draws
Claude's three lanes as one card (`together` in `WhereItGoes.svelte`), each a part of its bars:
Claude Code on its own (Terminal: the terminal and the Claude app), Claude in the cloud (In the
cloud: project threads, claude.ai/code and Cowork; only when the hubd counts it) and the estimate
for Elsewhere, striped. The estimate never sets a chart's scale: the scale is the most counted plus
a quarter, and a bar the estimate lifts above it is drawn full and cut, its sum on the tallest.
The cards are tinted flat in their company's colour (Claude clay, Codex blue). Under them, **Inside
Hub** is two lines (You asked, Hub on its own) and one slim split strip by engine with a chip per
engine, and **What Hub sent** is a strip and four lines (name, calls, tokens); neither has a percent
column, the share is in the tooltip. The tab or row names the page, so its own heading and subtitle are hidden.
Beside the ranges, **$ / Tokens** switches what every amount on the page counts (remembered in
`localStorage`, `hub.whereItGoes.unit`): with `usage.spend.tokens` the page asks with `unit=tokens`
and draws the same cards, bars, rows and zoom in tokens (fresh input plus output, without cache
reads); there is no Elsewhere then, since the estimate is dollars only, and money paid by the token
(DeepSeek, Jev) stays in dollars. Against an older hubd Tokens is greyed with "Comes with the next
version of Hub". Claude's opened view lists the three parts in place of answers and tokens, and its busiest day counts
what was measured. Hub's card is twice the width of the others, so the main chart leads, and an opened card grows to twice the width too. Then there is
**Codex** on its own (the CLI and ChatGPT's Codex; with Codex's own
count of the whole account in tokens in place of its line, when the hubd has it). With
`usage.spend.machines` the page asks for every machine added together and names each machine's
part under the total (an offline one says so). The hubd's `notes` stand in the footnote. The cards
carry the plan as quiet muted text (no chip and no colour blobs behind them); Hub's engine key stands beside its bars, so every card's bars
have the same height. Bars are Edge bars, flat in the place's own colours with a solid top line
(Claude clay to sand, the cloud clay to blue, Codex sky to iris, Hub's engines each in theirs). A
card with days opens on a click into a wide view (`WhereZoom.svelte`): one time line you zoom with
the scroll wheel (or + and −) around the pointer and move by dragging, a sideways scroll or the
arrows, its bars turning from hours (the last 31 days, from a hubd with `usage.spend.hours`) to
days, weeks and months as the window widens. A click on a bar opens it (a month or week into its
days, a day into its hours), hovering names the bar's parts, and Hub's bars split by engine or by
who asked. The range buttons set where it starts; it reads every day there is (`all=true&hours=true`)
when a card opens; from the newest back, a label shows only where it clears the next one, and
weeks are counted back from today as the cards count them.
Beside it the range's facts (dollars, answers, tokens, the average and busiest day, and for Hub its
four biggest sources by name, the rest folded into one "N more" row that opens them).
Under them, Hub's part by who asked (you: your threads, chats,
side jobs; Hub on its own: Guard, fix threads, routines, widgets, Customs,
threads another thread started, helper calls) and by engine, each with its percentage of Hub, and
the money paid by the token: DeepSeek's balance and spend, and the API connection Hub's own
decisions ask (Jev), priced by kind in `apiPrices.ts`. While a thread runs the page reads the
numbers again every 15 seconds and the total's cents take the accent colour. Last, **What Hub
sent**: the packing slips' totals (`/api/package/summary`, feature `package.ledger`) in tokens and
calls, grouped as what you wrote, handovers (switch, rewind, move), Hub's own words in a thread
(`hub:` purposes) and quick helper calls (every one-shot, the three biggest named). A hubd without
the ledger leaves the section out.

`GET /api/usage/spend?days=&tz=` (feature `usage.spend`, `spend.rs`) counts it on this machine.
`spend.rs` is engine-neutral (the tally, the lanes, adding machines up); what reads each engine's
own records lives in `spend/engines.rs`: the lanes a count starts with (`OWN_LANES`), the first
day for all time (`first_use`), the gathering of answers (`count`: Claude, then Codex, then the
cloud), the account counts and the cloud note (`finish`), and the lanes that are one account's
whichever machine counts them (`ONE_ACCOUNT_LANES`). With `everywhere=true` (feature `usage.spend.machines`, `spend::everywhere`) it also asks every
online host for its own count (without `everywhere`, so none asks the others again), waits up to
8 seconds for each, adds them up by date and lists each machine's part in `machines`, with why one
is missing (offline, too old to count, no answer). Another machine's DeepSeek and cloud lane are
left out, since the balance is one account and the cloud sessions one login, and this machine
already counts them. With `hours=true` (feature `usage.spend.hours`) the answer also carries
`hourly`: the same lanes hour by hour over the range's last 31 days at most (`Tally::hourly`),
Hub's per engine and by who asked (each source's `daily`, which the per-day answer now carries too),
for the page's zoom into a day. DeepSeek's balance and the plan-meter estimate are
per day only and stay out of it; cloud sessions fall on the hours their totals were read. Other
machines are asked with `hours` too and added hour by hour (`add_hours`).

- **Claude** from Claude Code's transcripts (`cc_marks`), subagents' answers included (their
  side-chain lines and their own files under `<session>/subagents/`). Each machine reads its
  own transcripts; what the same Claude plan spends where no Hub runs (claude.ai and the Claude
  app's chats) is not in the numbers. An answer in a Hub thread's session
  (its current one or an older one from `session_started`) is Hub's and goes to who asked
  (`spend::source_of`); a thread since removed is "deleted". A session started in the system temp
  folder is a Hub helper call (`engine::complete`), source "helper". The rest is Claude Code on
  its own. Answers of a thread on a pay-per-token engine (Claude Code on DeepSeek's key) are left
  out, since DeepSeek's balance counts them.
- **Claude in the cloud** (lane `claude_cloud`, feature `usage.spend.cloud`, `cloud_usage.rs`):
  Claude Code sessions in Anthropic's cloud on the same login (claude.ai/code, the Claude app's
  project threads, Cowork). Every ten minutes, in the background, Hub lists them through the
  Sessions API Claude Code uses for `--teleport` (`GET /v1/sessions`, also by the tags
  `hearth-thread`, `cowork-local`, `cowork-remote`) and reads each changed session's running total
  (`external_metadata.usage`: tokens and `cost_usd`) with Claude Code's own sign-in (its
  organization from `.claude.json`, or else from the account's profile, `GET /api/oauth/profile`,
  as Claude Code itself does). What grew
  since the last look goes to `cloud_marks` at the session's last activity; a first look spreads
  what it had over the days it ran. Remote Control sessions (`environment_kind` "bridge") are
  left out, since their transcripts are on a machine of yours. The API is not documented: when it
  cannot be read (Claude Code's sign-in is out of date, Hub cannot read it, the API changed) the
  lane is empty and `notes` says why. Hub never renews the sign-in itself.
- **Claude elsewhere, estimated** (lane `claude_elsewhere`, feature `usage.spend.estimate`,
  `plan_meter.rs`): what the plan spent where Hub cannot see, such as claude.ai's chat. Each read
  of Claude's limits keeps the weekly meter for all models (`plan_samples`, at most every ten
  minutes unless it moved). A day's climb of the meter is set against the Claude dollars counted
  that day (on its own, in the cloud, Hub's Claude engine); the upper quarter of those days gives
  dollars per point (kept in `kv` as `plan.rate`, so a short range still has one), and what each
  day climbed beyond what was counted, at that rate, is the lane, drawn as approximate. A new week
  is one whose meter fell: a reset worded another way or another weekly limit read inside the same
  week is not one (taking it for one counted the whole meter again, a $57 607 week), and a day
  climbs at most the whole meter. It is
  added after every machine is in (in the route, `plan_meter::add_estimate`), and another
  machine's own estimate is left out. Until the meter has a few days of readings `notes` says so.
  The page draws it as the striped part "Elsewhere" of the Claude card, with a ≈, shown only when
  the hubd sends the lane.
- **Tokens** (`unit=tokens`, feature `usage.spend.tokens`, `Tally::in_tokens`): the same answer
  with every answer's tokens (fresh input plus output, without cache reads) counted where its
  dollars would be, in lanes, engines, sources, days and hours; `unit` says "tokens". The balance
  (dollars only) and the plan-meter estimate are left out, and a host too old to count tokens is
  named in `machines` ("too old to count tokens") rather than added in dollars.
- **All time** (`all=true`, feature `usage.spend.all`, `spend::all_days`): the range runs from the
  first day anything was counted on this machine (the earliest of Claude Code's marks, the cloud
  marks and Codex's oldest session log) to today, at most ten years (`MAX_DAYS`, which also bounds
  `days` and the packing slips' summary). With every machine each host is asked with `all=true`
  and counts from its own first day; one that began earlier reaches the range back to its first
  day (`widen`), with nothing on the days the others gain.
- **Codex's own count** (`accounts`, feature `usage.spend.accounts`): `codex_live` asks
  `codex app-server` for `account/usage/read` beside its limits, which gives the whole account's
  tokens per day wherever it ran (the CLI anywhere, ChatGPT's Codex, Codex in the cloud). Tokens
  only, counted Codex's way, so it stands beside the dollars rather than in them. Asked in the
  background; the page shows the last answer.
- **Codex** from its session logs (`<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-*.jsonl` and
  `archived_sessions/`): each `token_count` event whose running total grew, the model from the
  last `turn_context`. A session a Hub thread holds, or one whose `originator` is the ACP adapter
  Hub runs Codex through, is Hub's; the rest is Codex on its own. Logs are kept parsed by size,
  so reading again while a thread runs only reads what grew. Prices are OpenAI's API prices in
  `CODEX_PRICES`; a model not listed is priced as GPT-5 and the lane is marked an estimate (≈).
- **DeepSeek** from its balance ledger (`balance::spent_by_day`, `balance.spend.<engine>` with
  `days`): what the balance fell by each day, in Hub's lane as engine "deepseek" with `paid`.

Plan use is what the same tokens would cost through the API (`stats::cost`). An older hubd is
read through `/api/stats`: Hub's threads against Claude's total, without days, Codex or who asked,
and the page says those come with the next version.

## Quota guard
<!-- covers: crates/hub-core/src/resume.rs, crates/hub-core/src/hub/resume.rs, crates/hub-core/src/usage.rs, crates/hub-core/src/balance.rs, crates/hub-core/src/codex_live.rs, app/src/lib/plan.ts -->

Plan limits are fetched after every turn (at most every two minutes, see `usage.rs`) and pushed
to surfaces as a `usage` stream message. The guard's settings (`GuardPrefs`, in the store under
`guard`, edited in Settings > Subscriptions) say when threads warn (80% by default), whether to
move a thread to a cheaper model before its next message once a limit passes a line (off by
default), which model that is, and which model gives second opinions. A switch is a `setting`
event followed by a `quota_switch` event; once the user picks a model after it, the guard leaves
that thread alone.

A turn that stops because a limit ran out is carried on when it resets (`resume.rs` reads the limit and its reset, `hub/resume.rs` waits and carries on; `resume` in
`GuardPrefs`, on by default). The Claude adapter fails such a turn with its "You've hit your limit"
text; Hub then reads the plan limits again and waits for the full one that resets last (the
engine's own "resets 3am (Europe/Stockholm)" names a zone Hub cannot place, while `/usage` gives a
UTC offset). The wait is a `quota_wait` event as `waiting`, kept in memory and picked up from the
log when the core starts, and checked against the wall clock every 20 seconds, so a PC that slept
catches up when it wakes. Two minutes after the reset Hub logs `resumed` and sends "carry on" as a
user message marked `quota: <wait>`. A message the person writes first, or Don't in the thread,
logs `cancelled`; three carry-ons without a finished turn log `gave_up`. When the reset time is not
known Hub tries again after 30 minutes.

Waiting is not the only way on. When another subscription is free, the thread goes on there
instead (`GuardPrefs.limit_switch`: `auto`, the default, moves it by itself; `ask` leaves the offer
for a click; `off` only waits). A subscription engine is one with plan limits (`limit_phrases`,
`Config::is_subscription`: Claude and Codex), so a pay-per-token engine such as DeepSeek is never
where a thread goes when a limit runs out. The partner is the first other subscription engine
that is not out of its own limit (`Waits` remembers which are, until their reset); with both out,
or only one subscription, the thread waits for the reset as before. The `waiting` event carries
`offer: <engine>` when there is a partner. The thread moves with `Hub::continue_elsewhere`
(`POST /api/threads/{id}/quota-wait { action: "switch", engine? }`, feature `limit.switch`):
`switch_engine`, a `quota_wait` event as `switched` (`to`, `from`, `by`), and a message marked
`quota: <wait>` that carries the conversation to the new engine like any engine switch. Settings
does not show `limit_switch` yet.

Next to Claude's limits, `/api/usage` lists the other subscriptions (`others`): Codex's limits from
its own logs, and for a pay-per-token provider, which has no limits, the balance left on the
account instead (`balance.rs`, `AccountUsage.balance`, e.g. "$4.20"). Only DeepSeek so far: Hub
asks `GET https://api.deepseek.com/user/balance` with the engine's key from the vault, which costs
nothing, and keeps the answer five minutes. A key that is missing or refused shows as a note.
The provider has no call for what was spent, so each fresh reading also goes into a small ledger
in the store (`balance.spend.<engine>`: the day in the app's time zone, the balance last seen, the
sum of its falls since the day began; a rise is a top-up and counts for nothing). That sum is
`AccountUsage.spent_today`, e.g. "$0.42". It is what the balance fell by while Hub was asking, and a
gap while Hub was shut counts toward the day it is next seen on. The ledger also keeps each day's sum
for 90 days (`days`), which Where it goes reads as DeepSeek's spend per day.

Codex's plan and limits come from Codex itself (`codex_live.rs`): Hub starts `codex app-server`
for a moment, asks `account/read` (plan) and `account/rateLimits/read` (the 5-hour and weekly
windows) and `account/usage/read` (the account's tokens per day, for Where it goes), and keeps
the answer two minutes. When Codex is missing, signed out or silent the
session logs are read as before (`codex_live::usage`), and a failed ask waits ten minutes before the
next. What it reported last also stands in for the plan in Settings, ahead of the login file: a
read notes it with `engine_home::note_plan` and a failed one clears it, and Settings > Accounts
reads it through `engine_home::engine_accounts`. The usage report asks for these other plans with
`plan_meter::other_plans`.

The plan's name (the tag next to Claude or Codex in the account card, Settings and the usage panel)
is never listed in Hub. It is what the provider's own data says: `subscription_type` in Claude's
`/usage`, `subscriptionType` in its login file, `plan_type` in Codex's logs and ID token. The app only
tidies the raw value for reading (`plan.ts`: `max` becomes `Max`, `default_claude_max_20x` becomes
`Max 20x`, a new name such as `ultra` shows as `Ultra`), so a provider's rename needs no change here.

Until the provider says it, a plan can be given a name of your own (Settings > Account and usage >
Plan name, `prefs.planLabels`, `shownPlan` in `plan.ts`). It is saved with the value the provider
gave at the time (Claude says `max` for what is sold as Ultra) and is ignored as soon as that value
changes, so the provider's own name takes over without anyone removing the label.

## Proof before done, leak guard, clash warnings
<!-- covers: crates/hub-core/src/proof.rs, crates/hub-core/src/leak.rs, crates/hub-core/src/clash.rs, crates/hub-core/src/covers.rs, app/src/lib/components/ProofStamp.svelte, app/src/lib/proofWords.ts, app/src/lib/components/DocsStamp.svelte -->

Three checks run on their own, each switched in Settings > Subscriptions > Safe threads (the
`proof`, `leak_guard` and `clash_warn` fields of `GuardPrefs`; the Customs check, a fourth, is
described under Customs). Hub looks at events after they are
logged, in a task of its own (`Hub::look_at`), so the engine never waits for them. What sits above Hub,
Problems, hooks in with `Hub::on_look` (`problems::install`, done by `problems::start` in hubd and by a test that
wants failed threads kept as problems), so `hub` names nothing in `problems`.

- **Proof before done** (`proof.rs`). After a turn that changed files other than docs (the diff
  from the checkpoint before its message to now), Hub looks through the turn's tool calls for a
  test or check run (`cargo test`, `npm run check`, `pytest`, `go test`, ...) or a screenshot. The
  last test run decides. A call to Hub's own `page_check` (known by its tool name however the
  engine namespaces it, `permit::hub_tool_call`) is a screenshot that decides by itself: passed
  when the page rendered with no script error, console error or blocked load, failed when it
  reported one, and nothing when it could not look (no browser, no such file). Other screenshots
  count only when they passed. Without a passing one Hub asks the agent once, with a user message
  marked `proof: <turn_end seq>`, and logs a `proof` event as `asked`; after the agent's answer
  the same reply gets `shown`, `failed` or `missing`. The question (`proof::ask_missing`) asks
  for the project's tests, except when every changed file that needs proof is a web page
  (`.html`/`.htm`, with `.css`/`.js` beside one): then it asks the agent to look at the page with
  `page_check`, `file` set to its path from the thread's folder, where `page_check` looks for it
  (the file runs as the viewer runs it, `pages::save_file`); a page outside the thread's folder
  cannot be looked at so and gets the question for tests.
  The reply's "Changed files" line is stamped
  tested, tests failed or no proof; a failed look at a page (`page_check`, kind `screenshot`) reads
  page has errors, and an unknown kind keeps the test words (`proofWords.ts`). Files that are the thing made for the person (a table, a
  drawing, a picture, a spreadsheet, a PDF; `proof::needs_proof`) count as docs, and so does
  everything in a top-level `output/` or `outputs/` folder (the things made and the script that
  made them). A turn in a scratch folder or an everyday chat's folder
  (`projects_root::is_made_for_the_person`, `<projects root>/scratch/<time>` and `chats/<time>`)
  is noted as `skipped` with `why: scratch` and never asked: nothing there ships.
- **Leak guard** (`leak.rs`). The same diff's added lines are scanned for keys (Anthropic, OpenAI,
  GitHub, AWS, Slack, Google, Stripe, Hugging Face, private keys), random-looking values assigned
  to names like `api_key` or `password`, and the values in Hub's vault. Ignored files are never in
  that diff, so a key in an ignored `.env` is fine. New findings are logged as a `leak` event with
  only a hint of each value, and sent to the agent (marked `leak: <seq>`) to move out of the code;
  that turn is judged for proof after the agent's answer instead. The same patterns keep keys out
  of the log: every event's strings are redacted before they are stored, so the engine gets what
  you wrote but the database keeps `[Anthropic API key sk-a…1Gs5]`.
- **Docs in step** (`covers.rs`, `Hub::docs_check`, the `docs_check` field). After a turn that
  changed code a doc section covers (`covers:`, see Docs as linked notes), Hub looks at whether
  the turn changed that doc, opened it, or asked `docs_for` about the file. If it did none of
  those, Hub asks once, in the same message as a proof request when there is one (marked
  `proof:`, else `docs: <turn_end seq>`), naming each section with its lines. A `docs` event
  records `updated`, `checked`, `asked`, then the answer's `updated`, `checked` or `ignored`.
  A turn that puts covered files back as an earlier checkpoint had them is `skipped`
  (`reverted`), and a section already asked about for the same files is not asked again. The
  reply's "Changed files" line gets a quiet docs stamp.
- **Clash warnings** (`clash.rs`). Every file an agent edits (from its `tool_call` updates) is
  noted with the thread and time. When another thread edited the same file within three hours,
  both threads get a `clash` event, once per file and pair, and each agent's next message starts
  with a note naming the other thread and the files. What was touched is kept in memory, so it
  covers work since the core started.

## Customs
<!-- covers: crates/hub-core/src/customs.rs, crates/hub-core/src/hub/customs_share.rs, crates/hub-core/src/customs_registry.rs, crates/hub-core/src/registry_address.rs, crates/hub-core/src/customs_review.rs, crates/hub-core/src/hub/customs_review.rs, crates/hub-core/src/customs_known.rs, crates/hub-core/src/customs_review/repo.rs, crates/hub-core/src/github_name.rs, crates/hub-core/src/customs_log.rs, app/src/lib/components/Customs.svelte, app/src/lib/customsLog.ts, app/src/lib/components/CustomsCard.svelte -->

Everything from outside that agents build with passes Customs (`customs.rs`): packages (npm,
crates, PyPI), CSS libraries from a CDN, CLI tools, MCP servers, Docker images. Services and API calls, free-text names, and well known tools (git, node, cargo, Hub's own engine programs; `customs_known.rs`) are stamped without a review. Whatever can run is never stamped unseen: a repository written as a link or `github:owner/name` is reviewed as a repository whatever kind it was filed under, and a runnable item (cli, mcp, docker, script...) whose name Customs cannot look up is held, not cleared. Only a refusal is sent to the agent; a held item never wakes it and is reviewed once.
Each item is a `CustomsItem`: a `kind` (an open list of short words, so a new kind needs no change
in the core), a name compared the way its ecosystem compares names, a category, a one-line summary,
the cleared versions, a status and who decided it. The list is one JSON value in the `kv` table
(key `customs`).

- **Cleared** items have the customs stamp: every agent on every machine may use them. **Held**
  items could not be verified and wait for the user. **Refused** items are not used, and the
  reason says why. Customs is a gate, not a short list: something missing from the list is not
  wrong, it is new, and goes through Customs when an agent first needs it.
- Agents look things up with `customs_search` (words in name, category or summary; cleared first)
  and take what is missing through Customs with `customs_declare`, which puts it on the list and
  reviews it before it answers (`customs_review::settle`), so a green one is used in the same turn.
  Both are among Hub's tools for every access level.
- Settings > Customs is where it is watched and run. At the top: how many are cleared, held,
  refused and have advisories, how long a review takes on average, and a bar per day for what came
  through in the last week (amber on a day something was held). Under it one line says what
  Customs does right now ("Now: reviewing x", from the reviews `customs_log::reviewing` marks as
  running) or, when quiet, what happened last and when the daily check ran. Tabs: What happened
  (the log by day), Needs you (held, with Review again, Clear and Refuse), Cleared, Refused,
  Advisories, How it runs (the `CustomsPrefs` switches) and Add. A row opens its item on the
  right: where it stands, why, its advisories, category and summary (edited there, so agents find
  it), the buttons, and its own story from the log. The page asks for the log again every few
  seconds while it is open. Read a project's packages (under Add) clears the direct dependencies in
  its `package.json`, `Cargo.toml` and `requirements.txt` files (three levels deep, ignored folders
  skipped) as `existing`.
- **The log** (`customs_log.rs`, table `customs_log`, schema step 15) keeps what happens:
  `declared` by an agent (with its thread), `found` after a turn, `asked` by a package manager
  through the registry, `cleared`/`held`/`refused` by the review (with how long it took) or by
  you, `reviewed` when a review ran but your decision stood, `advisory` from the daily check,
  `added`, `removed`, `imported`, and one line per daily check and monthly round. Append-only,
  kept 180 days (pruned by the daily check), and each machine keeps its own: the list is shared,
  the log is not. `GET /api/customs/activity` answers the last 30 days (500 lines at most), or with
  `kind` and `name` all of one item's, and the reviews running now. The Overview's Customs card
  (`CustomsCard.svelte`) shows today's numbers, a running review and the last few lines, and opens
  Settings > Customs.
- A **GitHub repository** (kind `github-repo`, name `owner/repo`) is put on the list and reviewed
  from its Plugins page (the Review button, see "Plugins"). The hubd that reads repositories
  lists `customs.github` among its features; an older one has no such review, and the page says
  so.

- **The review** (`customs_review.rs`) gathers facts from the package's registry (npm, crates.io,
  PyPI: age, downloads, maintainers, install scripts, repository, licence, deprecation) and asks
  OSV for known vulnerabilities in its newest version. `triage` decides what is clear-cut without a
  model: red (refused) for a name no registry has or known malware, yellow (held) for anything it
  could not look up, a known vulnerability, deprecation, a package under 30 days old, few
  downloads, or install scripts on a little-used package. The rest goes to a model through the
  tidy engine (or the default one, or the engine and model picked for plugin vetting in Settings >
  Handoffs, see Handoffs), on the user's plan, in a throwaway session that may not use
  tools: green clears, yellow holds, red refuses, and it fills in category and summary. A model
  that does not answer leaves it held. The review asks Hub for the model and for new threads
  through `customs_review::Officer`, which `hub/customs_review.rs` answers, so it names neither Hub
  nor the engines. A person's decision (`you`, `existing`) stands unless the
  verdict is red. `settle` reviews only what is held, not what was reviewed in the last day, and
  waits for a review of the same item that is already running.
- **A GitHub repository** (kind `github-repo`, the one Plugins stamps; `customs_review/repo.rs`) has no
  registry, so Hub asks GitHub through the `gh` the machine is signed in with: age, stars, the
  owner and how old the account is, licence, last push, archived or a fork, npm lifecycle scripts in
  `package.json` and files at the top that run when set up (`install.sh`, `setup.py`, a Makefile and
  the like). `customs_review::repo::triage` decides the clear cases: a repository GitHub does not know is
  refused; held for under 30 days old, unworked for a year, archived, under 50 stars, no licence,
  or (below 1000 stars) a young owner account, a fork or code that runs when installed, with
  every reason in the text. What passes goes to a model with the README and the installing files,
  each cut short and marked as untrusted data (an attempt to steer the verdict is a reason for
  red), and it answers green, yellow or red as for a package. Nothing is OSV-checked; the monthly
  round reviews what the review cleared again, as for packages. `POST /api/customs/repo`
  `{ repo }` (owner/name or a link; feature `customs.github`, which `POST /api/customs/review` with kind `github-repo` and name `owner/repo` also takes) puts the repository on the list when it
  is new, reviews it now and answers its item; a repository GitHub does not know is an error and
  leaves nothing on the list. `customs_declare` with that kind takes the same road.
- **The cadence** (`customs_review::cadence`, checked hourly): once a day every package is looked
  up in OSV again (malware refuses it, vulnerabilities go in `advisories`); once a month what the
  review cleared over a month ago is reviewed again, and a thread is started in each project with
  a manifest to update its dependencies to versions at least a week old.
- **The Customs check** (the `customs` field of `GuardPrefs`, on by default) runs after each turn
  next to the leak guard, on the same diff from the turn's checkpoint (`customs::in_turn`). For
  `package.json`, `Cargo.toml` and `requirements.txt` it compares the dependencies the file names
  now with those it named at the checkpoint (`git show`), so only what the turn added counts. In
  pages, components, scripts and stylesheets it reads the added lines for links to the common CDNs
  (jsDelivr, unpkg, esm.sh, Skypack, JSPM, cdnjs, Google's, jQuery's, Tailwind's and Bootstrap's)
  and takes the package name. A CDN link passes when the npm package of that name is cleared.
  Anything unknown goes on the list (`decided_by: "found"`) and through the review right away.
  Everything looked at is logged as a `customs` event; the agent is sent a message only about what
  stayed held or was refused (marked `customs: <turn_end seq>`), never to take out what cleared.
  Hub sends one message a turn, so after a leak the next turn tells it.
- **The registry** (`customs_registry.rs`, open at `/registry/` without a token, like the
  registries it stands for) serves npm's metadata, crates.io's sparse index and PyPI's simple
  index, filtered: versions younger than a week are left out, and a package that has not cleared
  is reviewed while the package manager waits (about as long as each one waits) and served if it
  clears. What cleared packages depend on is let through, learnt from their metadata as it is
  served. npm tarballs are redirected to npm unless refused, since a lockfile already pinned them.
  With `registry` on in `CustomsPrefs` (off by default), engines start with `npm_config_registry`,
  `PIP_INDEX_URL` and the like pointing there; cargo needs the config snippet Settings shows.
  Those values and the address (`registry_address.rs`) sit apart from the registry, so the auth
  layer and the engine start read them without reaching the registry's handlers.
  `customs_url` in `hub.json` points a machine's engines at another machine's registry.
- **One list for all machines**: every minute the machine that has hosts swaps lists with each one
  online (`Hub::share_customs` in `hub/customs_share.rs`, `POST /api/customs/share`,
  `customs::merge`). Of the same item the one changed last wins, and what was taken off stays off (tombstones, kept 90 days), so a stamp given anywhere holds
  everywhere within a minute.

## Attempts
<!-- covers: crates/hub-core/src/attempts.rs -->

The same task can run as two to four attempts at once (`attempts.rs`), each a thread in its own git
worktree, usually on different models. `POST /api/threads` with `attempts` (one settings map per
attempt, on top of `settings`) snapshots the folder as it is, uncommitted changes included
(`refs/hub/attempts/<group>`), adds worktrees under `.git/hub-attempts/<group>-<n>` on branches
`hub/attempt-<group>-<n>` with those files in place, creates the threads and sends each the
message. Each thread's first event is an `attempt` event naming the group and its threads, which
the thread view shows as a bar with every attempt's model, state and diff size
(`GET /api/threads/{id}/attempts`).

Picking one (`POST /api/threads/{id}/attempts/pick`) stops every attempt and what it started and
waits (up to 5 s, `workers::stop_thread`) until those processes have ended, snapshots the folder,
applies the picked attempt's changes to it as uncommitted changes (a 3-way merge when the folder
changed meanwhile), and removes all the worktrees and branches. A worktree folder that still cannot
be deleted after a few tries (Windows keeps a folder a process works in) is logged and left with
a note (`.hub-leftover`: the ref its work is under, and its thread); the pick goes on. The clean-up
removes such a folder an hour after the note, only when git lists the worktrees, the folder is not
linked as one, the ref exists and no other thread (nor its own while busy) works in it
(`janitor::sweep_attempt_leftovers`). Every
attempt's last state stays under `refs/hub/<thread>/attempt`. The picked thread moves to the
folder and carries on there; the others are archived. The picked `attempt` event holds the
snapshot from before, so picking is undone like a rollback. The sidebar counts worktrees as their
repository's project.

### Right-click menus
<!-- covers: app/src/lib/menu.svelte.ts, app/src/lib/menus.svelte.ts, app/src/lib/components/ContextMenu.svelte, app/src/lib/components/Icon.svelte -->

Right-click menus come from one place: `use:menu={() => items}` (`app/src/lib/menu.svelte.ts`) on
an element, with the lists for threads, projects, files, servers and messages in
`app/src/lib/menus.svelte.ts`, drawn by `ContextMenu.svelte`. Links and selected text get Open and
Copy everywhere. The WebView's own menu is kept only in text fields, or with Shift held.

An item can carry an `icon` (a name from `Icon.svelte`, a small grey glyph so the kind of action
reads at a glance), `children` (a submenu that hangs off the row: it opens on hover after a beat,
on click or with the right arrow, closes with the left arrow or Esc, and flips to the other side
or up when it would run off the screen), `checked` (a setting, drawn as a tick) and `heading` (a
small label above a group that cannot be picked). Empty submenus are left out. The thread menu is
kept to about fifteen rows: Open, New thread here, Rename, Pin, Snooze, Mark unread; Chat detail (a submenu: Follow the default and the four levels); Continue in a
new thread, Move to (one row per machine that is on), Take over, your own tools; Open in, Copy,
Show only this project, More (Regenerate title, Settle by itself as a tick, the bag, restart,
second opinion, roll back); Settle, Archive, Delete. A snoozed thread shows Wake thread instead of
Snooze. The snooze choices (`snoozeMenu`) are one list for the thread menu and the buttons on rows.

## Hub's tools
<!-- covers: crates/hub-core/src/hub_tools.rs, crates/hub-core/src/hub_tools/, crates/hub-core/src/mcp.rs, crates/hub-core/src/mcp/, crates/hub-core/src/tool_instructions.rs, crates/hub-core/src/engine_models.rs, crates/hub-core/src/permit.rs, crates/hub-core/src/thread_wait.rs -->

**One registry.** A tool is one `Tool` entry (`hub_tools/tool.rs`): its MCP definition, whether it changes
anything (a read-only agent key may not call it) and the function that runs it. `hub_tools::registry(access)`
holds the tools every caller may get, `mcp::thread_tools` the ones only a thread's own engine gets (steps, pages,
docs, library, side jobs, Guard's). `tools/list` and `tools/call` both read that list, so a new tool is one
entry in one place and a tool cannot be listed without being callable. Still beside it: a read-only tool must
also be in `permit::QUIET_TOOLS` (a test checks). A logged tool call is read back as Hub's tool under any engine's
naming (`mcp__hub__x`, `hub.x`, `hub/x`, `hub: x`, or the engine's `_meta` tool name) by one matcher in `permit.rs`:
`hub_tool_call(call, among)` picks from a list, `hub_tool_name(call)` names any (the bench counts calls with it).
`wait_for_thread` waits in `thread_wait.rs` (`wait`, `answered`), which the bench's runner shares.

Hub's threads are tools for agents (`hub_tools.rs`, after T3 Code's orchestrator MCP server).
They are served by the same MCP endpoint as `second_opinion` (`mcp.rs`, its tools by group in `mcp/`): `hub_brief` ("what's
going on?" in one call: what needs you and what it asks, what is working and on what, what finished
in the last hours with the end of its reply and whether it is unread, what failed and why; it asks the log for the newest matching event or the last turn's reply chunks per thread, never a whole log: `Store::last_event`, `last_tool_call`, `last_reply_chunks`),
`where_am_i` (this machine's name, OS, CPU, home and free disk, the programs on PATH, the other machines Hub knows, and the caller's own thread, folder and project; an outside agent passes `thread_id`), `hub_status` (thread ids by state, projects, engines and their models), `list_threads`, `read_thread` (the log
folded into messages and replies, or with tool calls too; the latest part within a budget, or
paged forward from `after_seq`), `wait_for_thread`, `start_thread`, `send_to_thread` and
`stop_thread`, Customs' `customs_search` and `customs_declare` (see Customs), `api_connections` (see API connections), `plugins_search` (see Plugins), `problems`, `problem` and `problem_dossier`, and for supervised callers `stamp_problem`, `change_problem`, `problem_thread` and `tell_user` (see Problems), and `delegate`: `start_thread` that returns at once, while a task in the core
watches the new thread (up to 24 hours, through questions to the user) and, when its turn ends,
sends its reply to the caller's thread as a message marked `from` the new thread, which gives the
caller a new turn. Only threads can delegate. The watch is kept in the store (`delegations`, by
the delegated thread) until the report is sent, so a core restart picks it up again
(`resume_delegations`, which hubd starts with its other loops). A turn that a plan limit cut off is not reported as failed: the watch
waits for Hub to carry it on (see Quota guard) and follows the message Hub sends for that, or for
a restart, as the same job. A message an agent sends is a `user_message` marked `from: { thread, title }` or
`from: { agent }`, which the thread shows as "from ..." rather than as yours. Waiting matches a
turn to its message by the `prompt` seq that `turn_end` and turn errors carry.

Who may do what:

- **An engine Hub runs** reaches them as its own thread at `/mcp/<thread id>` with `supervised`
  access, showing the token made for that thread (`auth::tool_token`, see Security), which opens
  that one path and nothing else. A thread it starts gets its own thread's settings (permission mode included), and it may
  only change the model or effort, so it can never start something with more rights than itself.
  It cannot message or stop its own thread.
- **An agent outside Hub** (Claude Code in a terminal, a bot) uses `/mcp` with an agent key made in
  Settings > Agents, which also gives the `claude mcp add` command. A key is `read` (look only),
  `supervised` (also start, message and stop; new threads run with the engine's defaults) or
  `full` (also any setting). Keys last 30 days, can be revoked, and only their SHA-256 is kept
  (`kv` key `agent_keys`). The connection token works there too, with full access.

Ask about everything in the new-thread box (on New thread or Home) starts a thread in `<data dir>/desk`, whose `CLAUDE.md` and
`AGENTS.md` (written by the core) tell the engine it is there to look across all of Hub, so "what's
going on?" and "fix X in Atlas" work from the box. There is no named persona. Threads started before
in `<data dir>/platschef` get the same brief.
Threads on other machines are listed but not read or messaged yet.

Agents also manage threads the way the user's own buttons do (`manage` in `hub_tools.rs`, through
`Hub::patch_thread`): `settle_thread` (what the user means by "close this thread"; `back: true`
undoes it), `rename_thread`, `pin_thread` and `snooze_thread` (`hours`, 0 wakes it). Each acts on
the caller's own thread unless it names another `thread_id`; a desk only on its own project's.
They are quiet tools (`permit::QUIET_TOOLS`), since each is undone with one click. A thread that
settles itself mid-turn stays settled when the turn ends; it comes back as any settled thread does.
`start_thread` and `delegate` take `context` (`brief`, `handover` or `full`, as side jobs do: a
summary of the caller's thread or its whole conversation after the brief) and `files`, paths in
the caller's folder sent as links (`Attachment.path`), so "start a new thread with the review
attached" carries the review. The handover is written before the thread is made, so a failed
one leaves no empty thread.

A thread's own engine also gets `offer_replies` (`mcp/talk.rs`): 2 to 4 one-click answers (label,
and the message a click sends) to the question it ends its turn on, logged as a `replies` event
and docked over the message box (see the composer above). It also gets `ask_user`, a question that
never waits: the call returns at once with `{ answered: false, assuming }` (the agent's own
assumption), and an `ask` event `{ id, question, options, assumed }` keeps the question in the chat
as a quiet line (`AskLine.svelte`) and docked over the box (`openAsk`) until it is answered. The
thread does not show "Needs you" for it. An answer, now or changed later, is a normal message that
starts with `askLead`: `You asked "X" and assumed "Y"; the person answered: Z`, so the agent can
change course. Hard-to-undo steps stay permission prompts, which wait and are never approved on a
timeout. It also gets `update_steps`: the whole
plan (3 to 8 short steps, each pending, in_progress or completed) every time a step starts or
finishes, logged as an ordinary `plan` update, so the steps card (see the composer above) fills
whatever the engine; `STEPS_RULE` in `tool_instructions` tells a top-level thread to use it for
jobs of three or more steps, or its own to-do tool if it has one. It also gets `write_tldr` (see the
thread hover card), which the same rule tells it to call last after more than a small change.

### A project's desk
<!-- covers: crates/hub-core/src/desk.rs, crates/hub-core/src/hub/desk.rs, app/src/lib/components/DeskNote.svelte -->

Going into a project (Go in on its folder in the Work list, its menu, its focus card or Ctrl K
"Go into ...") turns on focus mode for it and opens its desk (`desk.rs`, `DeskPanel.svelte`): one
long-lived thread per project, in the project's folder, that you steer the project's threads from,
like a project chat in Claude. Which thread is a folder's desk is kept in `kv` as
`desk:<folder>` (normalised like `same_folder`); `POST /api/desks` makes or finds it (`Hub::open_desk`
in `hub/desk.rs`) and
`GET /api/desks` lists their ids, so lists can leave desks out (the focus bar's Desk row leads to
it). The desk's agent gets Hub's tools plus desk instructions with its MCP `instructions`: hand
work to threads, steer them, read `docs/FOCUS.md`, never write code itself. `start_thread`,
`delegate` and `run_recipe` from a desk refuse other folders. When another thread of the project
finishes a turn, starts waiting on you or fails, `set_status` leaves a `desk_note` in the desk (`Hub::note_desk`)
(the end of the reply, the question or the error) and marks the desk unread; it does not start a
turn. The desk shows it as a line with the thread's live dot, and a question that is still open
can be answered from there with a message (a permission is answered in the thread). One line is
`DeskNote.svelte`; its dot follows the thread, so a question answered elsewhere turns grey. The right
panel beside a desk lists the project's threads by Needs you, Working and Idle under its stage
now; hovering one peeks at its last words and plan, and lets you steer or stop it.

## Live pages
<!-- covers: crates/hub-core/src/pages.rs, crates/hub-core/src/pages_kit.css, crates/hub-core/src/page_check.rs, app/src/lib/components/PageCard.svelte, app/src/lib/components/PageFrame.svelte, app/src/lib/components/ImageCarousel.svelte, ios/Hub/Rich.swift -->

An agent can answer with a live HTML page (a chart, mockup, animation, small tool) by calling
`show_page` on its MCP server. `pages.rs` keeps the document under `<data dir>/pages/<thread>/`
(local images inlined), a `page` event records it, and the transcript shows it as a card, the way
Claude Code shows a visual (`PageCard.svelte`): the live page sits inside a Viewkit card in the
chat column (see "Viewkit" below), with its name and expand above it and no toolbar. Expand opens
it the way "Files open" says (see "Viewkit" below): unfolded in the chat, in the viewer panel or big.
The page is served from `/api/threads/{id}/pages/{page}`; an older version is one grey line.
Pages go with their thread.

Every page is served with Hub's kit before the agent's own document (`pages_kit.css` and a few
lines of script in `pages.rs`): `hk-` classes for a picture with its source chip, a gallery, a
title and meta line, a hairline table, bars, notes in Hub's voice, metric tiles, a checklist,
tabs, a segmented switch and buttons, in the quiet look of the thread (no frames, one grey label
above a block, colour only for state). The tool's description tells the agent to use them, so an
answer built by Claude or Codex looks like the rest of Hub on the desktop and the phone alike, and
says that plain text, lists, tables and pictures are better as Markdown than as a page. The agent's
own CSS comes after the kit and wins.

An HTML file an agent wrote to disk (a chart saved as `.html`) is a page too: the viewer asks
`POST /api/project/page` (`pages::save_file`, feature `pages.file`) and frames the answer from the same
page route, so its scripts draw. The core reads the file from its folder, inlines the images, scripts and
styles that sit next to it (nothing outside the folder), keeps the result under the pseudo-thread `files`
with a random id and prunes it after a day. A core without the call leaves the viewer on its still
picture (`sandbox=""`, no scripts). Every agent is told what the chat shows as what it is, and that
`show_page` is the way to show the rest (`pages::SHOWS`, part of the instructions in `mcp.rs`). The same
text says a diagram, flowchart or chart is not a picture (draw it as SVG or a page, never with the image
model or a side job) and that no server is started to show a page. The tool's guide (`pages::GUIDE`) adds
the layout rules (no background or card on the outermost element, fixed pixel heights for charts, no
`vh`, 390 px wide must work) and the six series colours `--chart-1` to `--chart-6`, which the kit
defines on `:root` for every page so desktop and iPhone get them alike. A test keeps the guide and
`pages_kit.css` naming the same classes.

Claude Code puts an MCP server's `instructions` in front of the model; the others may not (Codex and
Hermes were seen drawing a picture and sending the user to a browser, 2026-11-04). So the engine's
`mcp_instructions` trait (`EngineTraits`, from the harness catalog; true only for Claude Code) decides:
an engine without it gets the same instructions (`tool_instructions::instructions`) after its first message of
each session, inside `<hub_instructions>` (`engine/acp.rs`, `turns`). The user's words go first, since an
engine names the session after how its first message begins: with the rules in front, threads were named
after them ("Hub coding agent workflow"). A title such an engine suggests that names Hub while nothing the
user wrote does is not taken either (`turns::names_hubs_rules`). The log keeps the user's own
words, and the packing slip shows the text as its own section `hub_instructions`.

A changed page is a new version, not a new page: `show_page` takes `replaces`, the id of a page
the thread showed, and the `page` event carries it. The transcript folds a family of versions
(`family`, `version`, `of` on `PageItem`): an older version shrinks to one grey line where it was
shown ("v1 · v3 is further down") that opens that version, the viewer steps between them with
‹ v2 of 3 ›, and the Shown list lists the page once.

`page_check` lets the agent look before it shows (`page_check.rs`). It keeps a draft as
`check-<id>` (never shown, removed after), takes an `.html` file in the thread's folder (`file`,
relative or absolute inside it; run like the viewer runs it, `pages::save_file` with the styles,
scripts and pictures beside it, and removed after), or takes a shown page's id, and opens it from the core's
own page route in a headless Chrome, Edge or Chromium already on the machine (`HUB_BROWSER` names
one), driven over the DevTools protocol with a throwaway profile. The answer is a screenshot at
720 px, the page's own height, and its script errors, `console.error`s and what the sandbox
blocked, with line numbers as in the agent's document.

On the iPhone (`PageCard` and `PageWeb` in `ios/Hub/Rich.swift`) a page sits in the chat as a live
web view, folded the same way: an older version is one grey line that opens it in a sheet. The
card takes the height of what is in the page's body, measured by a script of the app's own, not
the height the page reports: that one measures the document, which is never shorter than the web
view, so a card that started taller than its page kept the empty space under it. The phone's
transcript leaves `page_check`'s screenshot out as the desktop does (`Model.swift`).
An HTML file the agent wrote, named in its answer or written in the turn, shows the same way under the
turn (`FilePages` in `RichPictures.swift`): the phone asks `POST /api/phone/threads/{id}/page` with the path as
the agent wrote it (feature `phone.threadpage`, only a file in the thread's own folder) and frames the
page from `/api/threads/files/pages/{page}`. A Hub without the call leaves the name in the text. A CSV,
TSV or SVG the reply wrote whole is a card after the turn as on the desktop (`MadeFiles.swift`, sketch
40): the fold keeps the turn's file edits from the tool calls' `diff` content and, at the turn's end,
the files whose first edit wrote them, applied as `fromEdits` does (`Folded.made`, at most two), so
nothing is fetched and no hubd call is needed. A table shows its first six rows (header row grey,
numbers to the right, sideways for more columns), a drawing is drawn as it is with scripts off; a tap
or the expand opens it big in a sheet (every row with the header on top, or the drawing to pinch).

## Viewkit
<!-- covers: app/src/lib/viewkit/, app/src/lib/components/PageCard.svelte, app/src/lib/components/ThreadPanel.svelte, app/src/lib/components/Changes.svelte, app/src/lib/components/NamedFile.svelte -->

**Three ways to open a file or page card, chat never moves** (`viewkit/openfile.svelte.ts`). A click
on a card follows `prefs.fileOpen` (Settings > Chat > Files open, default `inline`): *inline* unfolds
the card in the chat (`Card` `unfolded` + its `full` snippet, scrolls inside 70vh, ✕ folds it back,
state in `unfolded` by thread and path), *panel* sets `hub.viewing` (the right-panel viewer) and
*modal* opens `Big`. The caption has a panel button, and ⋮ holds "Open in a window" (the modal), which open that way once.
The caption's expand button **grows the card in place** (`Card` `grown` + `ongrow`, state in `grown` by
thread and path): out of the chat column to the thread scroller's width (at most 1280 px) and height,
the height animated from the card's own, the thread scrolled to it, Esc/✕ folding it back to the
saved scroll. Zoom is CSS `zoom` on the `full` snippet (Ctrl + wheel around the pointer, 25 to 400 %);
holding Ctrl or Space lays a catcher over the content so wheel and drag reach Card, not a live page's iframe.
A caller with no card to unfold (a chip in the text, a list, a compact row) passes `card: false` and
inline falls back to the panel. There is no layout where the chat gives way to a file (tried in #704,
removed: the chat is never squeezed or moved).

Viewkit is the name for everything that shows what an agent made. (HubKit is the other thing: the
`hk-` styles an agent uses inside its own HTML page.) An output is shown in three ways: a **card**
in the chat (`viewkit/Card.svelte`: the preview itself on the chat, a still with no frame and no
scrollbar, cut and faded after 236 px, the whole of it one click that opens it big; under it one quiet
line with the glyph, name, kind, version and ⋮; an answer with more than two outputs draws a row with
a miniature each, `compact`), **big** over the window
(`viewkit/Big.svelte` for a page, `Lightbox.svelte` for pictures; a page there is as tall as it says and the panel scrolls, never a box inside it) and a **row** in a library
(`viewkit/Row.svelte`). `viewkit/kinds.ts` lists the kinds (page, picture, doc, file) with their
word and glyph; a new kind is an entry there and the component that draws it. A page is the first
to sit in the chat as a card with the live page inside. A file a reply wrote whole, of a kind that is
the point of a file (`showsInChat` in `filePreview.ts`: CSV and TSV as a table, SVG, HTML, Markdown as a document; a Markdown file is shown only when the reply wrote it whole, never because the answer names it), shows the
same way above the reply's footer (`Changes.svelte`: a Card around `FilePreview`, the reply's own
text, a scrolling box), and its expand opens it big (`Big.svelte` draws pages and files, `openBig`
and `openBigFile` in `bigview.svelte.ts`). A file the reply only names (a script wrote it, so no edit shows it) gets the same card, and a
spreadsheet too, when its path is in the answer's words and the folder has it (`NamedFile.svelte`
probes the file first; a name that is no file shows nothing). A markdown link to a file in a reply (Codex writes `[orders.csv](orders.csv)`, or a `file:` address or Windows path) becomes the same file chip as a path in code and opens the file in Hub (`linkedFile` in `rich.ts`; `Markdown.svelte` lets those addresses through the sanitizer). A bare name (`order.csv`, where a script wrote it to `outputs/visning-test/` and the reply says only the name) is looked for in the folders the reply mentions, then a few levels down the folder tree (`findFile`, `dirHints` in `filePreview.ts`), by the chip and by `NamedFile`. Pictures keep their bare carousel and
the other files their chips. Name a `.svelte.ts` file differently from a component by more than case: Windows reads
`big.svelte` as `Big.svelte`.

The thread's library is the "Library" group in the thread panel (`ThreadPanel.svelte`): the pages
and pictures from the thread's log, and the docs and files it wrote from the project's outputs
(`outputs.rs`, filtered to the thread), each a row that opens big. The plan for the rest (a
registry the chat, the libraries and the phone all ask, converters in the core for Excel and
documents, kinds shown as generated pages for Mermaid and 3D) is the stages in
`docs/FOCUS.md`.

## Handoffs
<!-- covers: crates/hub-core/src/handoff.rs -->

Who does which kind of task is a table (`handoff.rs`, kv `handoff_prefs`, `GET/PUT /api/handoff-prefs`,
compat feature `handoff.tasks`): per task type (`handoff::TASKS`: pictures, plugin vetting, research,
code review, quick questions, titles and summaries) one pick for everyday chats and one for dev
threads, each an engine and a model (`HandoffPick`). A task with no pick is done by the thread
itself, as before. Picking something means only that may do it. The table travels with the engines
to every machine (`Shipment.handoff`, the newest set wins, as `agent_prefs` does), and a pick for an
engine a machine does not have is not used there.

Two roads read it. Hub runs some tasks itself, so they are hard: its own helper questions
(`engine::complete`: titles, tidying, commit text, focus checks) ask `Hub::tidy_target`
for the "housekeeping" pick before the `tidy` engine and model in `hub.json`, and Customs asks for
"vetting" (`customs_review::judge`, through `settle_for`/`review_for`, which `customs_declare` calls
with the thread's folder so the right column applies). The others are soft: a side job the agent
starts takes its engine, and unless the agent names one its model, from the pick for its `kind`
(`handoff::task_of_kind`) and refuses another engine with the one that was chosen; Hub tells a
top-level agent which tasks to hand over (`handoff::instructions`, and for pictures the `PICTURES`
text in `mcp.rs`), but the agent has to do it. Nothing falls over to another engine when a pick
fails: the task fails and the user decides (subscriptions and keys added by hand only).

When Customs vets something an agent declared in a thread with a pick for vetting, the thread logs a
`task` event: `{ id, task, name, kind, engine, model, state, by: "hub", verdict?, reason?, ms? }`,
"working" and then, under the same `id`, "done" (verdict cleared, held or refused) or "failed". It
is a note in the chat and never starts a turn.

## Side jobs
<!-- covers: crates/hub-core/src/children.rs, app/src/lib/components/ChildLine.svelte, ios/Hub/Rich.swift, ios/Hub/Store.swift -->

A thread's agent can hand one piece of work to a child thread and carry on (`children.rs`, Hub's
`start_side_job` tool). The main use is what the agent cannot do itself: Claude has no image model,
so a picture goes to Codex, which draws with the ChatGPT plan's image tool. Only subscriptions; no
API keys. Which engine does which kind of job is `side_jobs` in `hub.json` (`"image": "codex"` by
default, unless Settings > Handoffs picked an engine for the kind, see Handoffs); other kinds run on the parent's engine. The agent can also name an `engine` and a `model`
for a job (as `start_thread` and `delegate` do), say Haiku for a quick lookup; an engine Hub does
not have is refused.

- The child is an ordinary thread with `parent` set, in the same folder. It starts with the task the
  agent wrote and, by `context`, nothing more ("brief"), a handover written from the parent
  ("handover"), or the parent's conversation ("full"). A side job cannot start side jobs. It asks
  for permission as any thread does, except under a bench case: then it takes the parent's
  `Unattended { by: "Bench" }` and runs under the bench's answer (see Bench).
- On the iPhone a thread whose own turn is over but whose side job still runs counts as working:
  `Store.merge` sets `HubThread.sideBusy` (since when a running side job began, never sent by
  hubd), so the thread sits in the inbox's Working group, its sign is the working cube and its row
  says "Side job" with the timer, until the job ends. It needs nothing from hubd, so no compat entry.
- The parent logs a `child` event when the job starts and again as "done" (the child's last reply
  and the paths of the pictures it made) or "failed". The first result also goes to the parent's
  agent as a message marked `child: <id>`, queued behind its turn if it is busy. One case does not wake the agent: pictures drawn for an everyday chat (`connections::is_chat_folder`). They show in the chat and the agent has nothing to add, so the "done" event carries `quiet: true`, no message is sent, and the agent hears of it in a note in front of the next message (`tool_instructions::quiet_side_jobs`, added by `hub/send/notes.rs`). The chat is marked as having something new (`returned_at`), as the turn used to do. The agent cannot carry on with that picture by itself, so the answer of `start_side_job` tells it then that it is not called back and to do the rest of the ask now. In a coding thread that message names the paths of the pictures for the agent's own work, but says the card already shows where they are saved and asks for at most two short lines (no recital of paths, copies, style or "verified"); a picture job's own agent is told to end with just the path, and when pictures exist its answer is not handed on at all; in an everyday chat (`connections::is_chat_folder`) a side job's answer comes without paths, since a path in the answer ("saved as C:\\...\\Temp\\...png") is noise (`chat_done_message`, and a line in `tool_instructions::CHAT_STYLE`). `CHAT_STYLE` also tells a chat's agent the internet is always allowed (no toggle, no permission to ask), so it searches instead of asking for data. Later turns in the
  child, when you carry on there, stay in the child.
- With an engine set for pictures, Hub tells each top-level thread's agent as it connects (the MCP
  server's instructions) to hand pictures to a side job rather than draw with a tool of its own, such
  as a claude.ai connector (ElevenLabs), which holds the turn while it draws. `"image": "claude"`
  sends pictures to a Claude child that draws with its connector; `"side_jobs": {}` lets the agent
  pick.
- A side job is like a subagent: it never has a row of its own in a list, in Inbox or among the
  chats, on the desktop (`everything` and `chats` in `state.svelte.ts` leave out a thread with a
  `parent`) or the iPhone (`Store.sideThreads`, kept apart so its line can still open it; `Store.thread(id)` finds either). It shows
  only in its parent, which says "Needs you" on the line when the job waits on you. In the parent it is a bracket in the left margin, a branch that goes out from the agent's
  words and comes back (never the wavy break line: that one says the thread itself changed, a side
  job leaves the thread on its own engine). The head says "Side job · Claude asked Codex", with the
  state at its right ("drawing…", then a green "Done · 41s", or "Failed"), Open, and the job's
  title; a finished job closes the bracket on a dot and labels what it brought back "Result": its
  pictures, read from the child's log, or its answer, folded.
  The iPhone draws the same bracket, more compact (`SideJobView` in `Rich.swift`): a head line, the
  job, and once it is done a "Result · Open" row; a tap opens the job's thread. A job that drew
  shows its pictures under the line, read from the job's own log as the desktop does (the phone
  loads that log once the job is done), and a tap on one opens it big.
  On the desktop a job that made pictures also has one quiet line under them, "Saved on <machine> · <file>" and a Folder button that shows the file in Explorer (only for a job on this machine), so
  neither the card nor the agent has to spell out a path (`ChildLine.svelte`).
- A job that can change files (not pictures, research, reviews or quick questions) shows it on its
  line, live: the step it is on now ("Editing src/db/migrate.rs…") and a chip with "3 files +212
  −18". The chip unfolds the files, a file unfolds its diff. All of it is read from the child's own
  log, which the parent's window follows (`jobWork` in `transcript.ts`, counting the turn still
  running), so nothing asks the folder or the core.
