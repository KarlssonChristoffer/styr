<!-- Labs > Bench: pick a suite (`GET /api/bench/suites`: its defaults, cost cap and an estimate from earlier runs), start a run (`POST /api/bench/runs`), watch it repeat by repeat with a Stop, and read a finished run: one row per case with each repeat's verdict (and why one was not counted), the flag with its reasons against the run before, median tokens and time, cost, and the markdown report under it. Pure parts in `bench.ts`; the API and the run are in docs/architecture/bench.md.
     Used by Labs.svelte. -->
<script lang="ts">
  // Labs > Bench: pick a suite, start a run, watch it with a Stop, read a finished run per case.
  // The numbers per case come from the server's summary (`caseRows` decides, with a fallback for
  // an older hubd); pure parts in `bench.ts`; the API and the run are in docs/architecture/bench.md.
  import { onMount } from "svelte";
  import { api } from "../api";
  import {
    caseRows,
    comparedWith,
    costNote,
    engineChoices,
    estimateFor,
    estimateText,
    flagClass,
    progress,
    refusal,
    repeatChoices,
    repeatTip,
    runLine,
    stateWords,
    totalCost,
    type Verdict,
  } from "../bench";
  import type { BenchRun } from "../bindings/BenchRun";
  import type { BenchSuiteInfo } from "../bindings/BenchSuiteInfo";
  import { hubdHas } from "../compat";
  import { duration } from "../activity";
  import { ago, dollars } from "../format";
  import { poll } from "../poll";
  import { engines, loadEngines } from "../providers.svelte";
  import { quiet } from "../quiet";
  import { tokens } from "../session";
  import Markdown from "./Markdown.svelte";
  import Select from "./Select.svelte";

  type Opened = { run: BenchRun; report: string | null };

  /** null while asking the hubd; false: it is older than the bench. */
  let has = $state<boolean | null>(null);
  let runs = $state<BenchRun[]>([]);
  /** The run that is going, with its rows. */
  let live = $state<Opened | null>(null);
  /** The finished run opened from the list. */
  let open = $state<Opened | null>(null);
  /** The built-in suites; null: an older hubd without `bench.suites`, which starts smoke only. */
  let suites = $state<BenchSuiteInfo[] | null>(null);
  let suiteName = $state("smoke");
  let engine = $state<string | undefined>(undefined);
  let repeats = $state<number | undefined>(undefined);
  let acting = $state(false);
  let failed = $state<string | null>(null);
  let note = $state<string | null>(null);

  const going = $derived(live?.run.state === "running" ? live : null);
  /** A string, so a refresh of the same run does not restart the poll. */
  const goingId = $derived(going?.run.id ?? null);
  const finished = $derived(runs.filter((r) => r.state !== "running").slice(0, 5));
  const step = $derived(going ? progress(going.run) : null);
  const suite = $derived(suites?.find((s) => s.name === suiteName) ?? null);
  const suiteChoices = $derived((suites ?? []).map((s) => ({ value: s.name, label: `${s.name} (${s.cases.length} cases)` })));
  const engineOptions = $derived(engineChoices(engines.names, suite));
  const repeatOptions = $derived(repeatChoices(suite));
  const estimate = $derived(estimateText(estimateFor(suite, engine, repeats)));
  const marks: Record<Verdict, string> = { pass: "✓", fail: "✗", invalid: "–", pending: "·" };

  async function load() {
    runs = await api.benchRuns();
    const now = runs.find((r) => r.state === "running");
    live = now ? await api.benchRun(now.id) : null;
  }

  /** While a run goes: its rows; once it ends, the list again and the run opened. */
  async function refresh(id: string) {
    const r = await api.benchRun(id);
    live = r;
    if (r.run.state !== "running") {
      note = null;
      open = r;
      await load();
    }
  }

  /** The suite list, or null on an older hubd (compat-until: 2026-12-12). */
  async function loadSuites() {
    if (!(await hubdHas("bench.suites"))) return;
    const list = await quiet(api.benchSuites(), "bench suites", null);
    suites = list?.length ? list : null;
    if (suites && !suites.some((s) => s.name === suiteName)) suiteName = suites[0].name;
  }

  onMount(() => {
    quiet(
      // compat-until: 2026-12-11
      hubdHas("bench.runs").then((v) => {
        has = v;
        if (v) return Promise.all([load(), loadEngines(), loadSuites()]);
      }),
      "bench runs",
    );
  });

  $effect(() => {
    const id = goingId;
    if (!id) return;
    return poll(() => refresh(id), 3000);
  });

  async function act(f: () => Promise<unknown>) {
    acting = true;
    try {
      await f();
      failed = null;
    } catch (e) {
      failed = refusal(e);
    } finally {
      acting = false;
    }
  }

  const start = () =>
    act(async () => {
      note = null;
      const run = await api.benchStart({ suite: suiteName, engine, repeats });
      live = { run, report: null };
      runs = [run, ...runs];
    });

  const stop = (id: string) =>
    act(async () => {
      note = (await api.benchStop(id)).note;
    });

  const toggle = (id: string) =>
    act(async () => {
      open = open?.run.id === id ? null : await api.benchRun(id);
    });
</script>

{#snippet table(o: Opened)}
  {@const rows = caseRows(o.run, o.report)}
  {@const cost = totalCost(rows)}
  <div class="scroll">
    <table>
      <thead>
        <tr><th>Case</th><th>Repeats</th><th>Passed</th><th>Flag</th><th class="num">Tokens</th><th class="num">Time</th><th class="num">Cost</th></tr>
      </thead>
      <tbody>
        {#each rows as r (r.case)}
          <tr>
            <td class="case">{r.case}</td>
            <td class="marks">{#each r.repeats as v, i (i)}<span class={v.verdict} data-tip={repeatTip(i, v)}>{marks[v.verdict]}</span>{/each}</td>
            <td data-tip={r.before ? `Before: ${r.before}` : undefined}>{r.passed}</td>
            <td>{#if r.flag}<span class="flag {flagClass(r.flag.level)}" data-tip={r.flag.why.join("\n") || undefined}>{r.flag.level}</span>{:else}<span class="dim">new</span>{/if}</td>
            <td class="num">{r.tokens === null ? "–" : tokens(r.tokens)}</td>
            <td class="num">{r.wallMs === null ? "–" : duration(r.wallMs)}</td>
            <td class="num">{r.cost === null ? "–" : dollars(r.cost)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <p class="hint">
    {stateWords(o.run)} · cost {cost === null ? "not reported by the engine" : dollars(cost)} · Hub {o.run.hub_version} · {o.run.machine}.
    Medians over the repeats that count. {comparedWith(o.run)}
  </p>
  {#if o.report}
    <details>
      <summary>Report</summary>
      <div class="report"><Markdown text={o.report} /></div>
    </details>
  {/if}
{/snippet}

<div class="bench">
  {#if has === false}
    <p class="hint">The bench needs the next version of Hub on this machine.</p>
  {:else if has}
    {#if suite}
      <p class="hint">
        Runs each of the {suite.cases.length} cases of the <b>{suite.name}</b> suite as real threads, a few times each, on
        {suite.model === "fast" ? "the engine's cheapest model" : suite.model}, and checks and times them. It uses your plan: it does
        not start with the plan at 60 % or more. {costNote(suite)} {estimate}
      </p>
    {:else}
      <!-- compat-until: 2026-12-12 (an older hubd has no suite list: smoke only) -->
      <p class="hint">
        Runs each case of the <b>smoke</b> suite as real threads, a few times each, on the engine's cheapest model, and checks
        and times them. It uses your plan (Hub has no estimate of how much): it does not start with the plan at 60 % or more, and
        stops at 80 % or at the suite's cost cap.
      </p>
    {/if}
    {#if going && step}
      <div class="row">
        <span class="dot running"></span>
        <span class="state">{step.text}</span>
        <button class="btn" disabled={acting || !!note} onclick={() => stop(going.run.id)}>Stop</button>
      </div>
      <div class="bar"><span style:width="{step.total ? (100 * step.done) / step.total : 0}%"></span></div>
      {#if note}<p class="hint">{note[0].toUpperCase() + note.slice(1)}.</p>{/if}
      {#if going.run.rows.length}{@render table(going)}{/if}
    {:else}
      <div class="actions">
        {#if suites}<Select size="small" label="Suite" bind:value={suiteName} options={suiteChoices} disabled={acting} />{/if}
        <Select size="small" label="Engine" bind:value={engine} options={engineOptions} disabled={acting} />
        <Select size="small" label="Repeats" bind:value={repeats} options={repeatOptions} disabled={acting} />
        <button class="btn primary" disabled={acting} onclick={start}>Start {suiteName} run</button>
      </div>
    {/if}
    {#if failed}<p class="error">{failed}</p>{/if}
    {#if finished.length}
      <ul class="list">
        {#each finished as r (r.id)}
          <li class:shown={open?.run.id === r.id}>
            <button class="title" onclick={() => toggle(r.id)}>{runLine(r)}</button>
            <span class="meta">{ago(r.started)}</span>
          </li>
        {/each}
      </ul>
    {/if}
    {#if open && open.run.id !== going?.run.id}{@render table(open)}{/if}
  {/if}
</div>

<style>
  .bench { border: 1px solid var(--line); border-radius: var(--r-md); padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .hint { color: var(--text-3); margin: 0; max-width: 640px; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
  .row { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .state { color: var(--text); flex: 1; min-width: 0; }
  .bar { height: 3px; border-radius: var(--r-xs); background: var(--line); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--muted); transition: width var(--dur-move) var(--ease); }
  .list { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 2px; border-top: 1px solid var(--line); padding-top: 8px; }
  .list li { display: flex; align-items: center; gap: 8px; min-width: 0; padding: 2px 6px; border-radius: var(--r-md); }
  .list li.shown { background: var(--surface); }
  .list .title { background: none; border: 0; padding: 0; color: var(--text); font: inherit; cursor: pointer; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1; }
  .list .title:hover { text-decoration: underline; }
  .meta, .dim { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .scroll { overflow-x: auto; border: 1px solid var(--line); border-radius: var(--r-md); }
  table { border-collapse: collapse; font-size: 12px; width: 100%; }
  th { color: var(--muted); font-weight: 500; text-align: left; padding: 4px 8px; border-bottom: 1px solid var(--line); white-space: nowrap; }
  td { color: var(--text-2); padding: 3px 8px; white-space: nowrap; }
  tr + tr td { border-top: 1px solid var(--line); }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  .case { color: var(--text); font-family: var(--mono); }
  .marks { font-family: var(--mono); letter-spacing: 2px; }
  .marks .pass { color: var(--green); }
  .marks .fail { color: var(--red); }
  .marks .invalid, .marks .pending { color: var(--muted); }
  .flag { padding: 0 6px; border-radius: var(--r-xs); }
  .flag.green { color: var(--green); background: var(--green-bg); }
  .flag.amber { color: var(--amber); background: var(--amber-bg); }
  .flag.red { color: var(--red); background: var(--red-bg); }
  details summary { color: var(--muted); cursor: pointer; font-size: 12px; }
  .report { margin-top: 6px; color: var(--text-2); font-size: 12px; overflow-x: auto; }
  .error { color: var(--red); background: var(--red-bg); border-radius: var(--r-md); padding: 8px 10px; margin: 0; white-space: pre-wrap; font-size: 12px; }
</style>
