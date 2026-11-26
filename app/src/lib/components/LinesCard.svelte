<!-- Code in and out: lines your commits added (up) and removed (down) each day, over the period picked in the card's menu. Bars use a square-root scale so one huge day does not flatten the rest; the tip has the real numbers, and one line under the bars has the totals.
     Used by CardBody.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // Code in and out: lines your commits added (up) and removed (down) each day, over the period
  // picked in the card's menu. Bars use a square-root scale so one huge day does not flatten the
  // rest; the tip has the real numbers, and one line under the bars has the totals.
  import { onDestroy } from "svelte";
  import { dateKey, knob, lines, loadLines, overview, pickDay, pickRange, setKnob, takeRange } from "../overview.svelte";
  import { prefs } from "../prefs.svelte";
  import HowCounted from "./HowCounted.svelte";

  const RANGES = [14, 30, 90];
  const days = $derived.by(() => {
    void prefs.overviewFrames;
    return Number(knob<string>("lines", "range"));
  });
  loadLines();

  // A range picked in a linked card moves this one to its nearest range (overview.svelte.ts);
  // one picked in the menu reaches the linked cards, and does not go back.
  let handed = false;
  takeRange("lines", RANGES, (d) => {
    handed = true;
    setKnob("lines", "range", String(d));
  });
  let last: number | null = null;
  $effect(() => {
    if (last !== null && days !== last && !handed) pickRange("lines", days);
    handed = false;
    last = days;
  });

  // The wait, in seconds, while git is read.
  let now = $state(Date.now());
  const tick = poll(() => (now = Date.now()), 1000);
  onDestroy(() => tick());
  const waited = $derived(lines.since ? Math.max(0, Math.round((now - lines.since) / 1000)) : 0);

  const bars = $derived.by(() => {
    const by = new Map((lines.data?.days ?? []).map((l) => [l.date, l]));
    const now = new Date();
    const out = [];
    for (let i = days - 1; i >= 0; i--) {
      const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i);
      const key = dateKey(d);
      const l = by.get(key);
      out.push({ key, d, added: l?.added ?? 0, removed: l?.removed ?? 0, commits: l?.commits ?? 0 });
    }
    return out;
  });
  const top = $derived(Math.sqrt(Math.max(1, ...bars.map((b) => Math.max(b.added, b.removed)))));
  const height = (n: number) => `${(Math.sqrt(n) / top) * 100}%`;
  const added = $derived(bars.reduce((n, b) => n + b.added, 0));
  const removed = $derived(bars.reduce((n, b) => n + b.removed, 0));
  const num = (n: number) => n.toLocaleString();
  const tip = (b: (typeof bars)[number]) =>
    `${b.d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })}: +${num(b.added)} −${num(b.removed)} in ${b.commits} ${b.commits === 1 ? "commit" : "commits"}`;
  const label = (d: Date) => d.toLocaleDateString([], { day: "numeric", month: "short" });
</script>

<HowCounted card="lines">
  <p>Lines your commits added and removed, from git. Lock files and binary files are left out.</p>
</HowCounted>

{#if !lines.data}
  {#if lines.error}
    <p class="quiet">Could not read git: {lines.error}</p>
  {:else}
    <p class="quiet">Reading git history{waited ? `… ${waited} s` : "…"}</p>
    <p class="quiet">First read takes a few minutes.</p>
  {/if}
{:else if !added && !removed}
  <Quiet text="No commits with lines in this range" />
{:else}
  <div class="wave">
    {#each bars as b (b.key)}
      <button class="col" class:picked={overview.picked.lines === b.key} data-tip={tip(b)} onclick={() => pickDay("lines", b.key)} aria-label={tip(b)}>
        <div class="plus-half"><div class="in" style:height={height(b.added)}></div></div>
        <div class="minus-half"><div class="out" style:height={height(b.removed)}></div></div>
      </button>
    {/each}
  </div>
  <div class="axis"><span>{label(bars[0].d)}</span><span>today</span></div>
  <p class="caption"><em class="plus">+{num(added)}</em> added · <em class="minus">−{num(removed)}</em> removed · net <em>{added >= removed ? "+" : "−"}{num(Math.abs(added - removed))}</em></p>
{/if}

<style>
  /* Taller as the card gets wider, so a wide card does not show a flat strip. */
  .wave { display: flex; gap: 2px; height: clamp(120px, 24cqw, 280px); }
  .col { flex: 1; min-width: 0; height: 100%; padding: 0; display: flex; flex-direction: column; border-radius: 2px; }
  .col.picked { background: var(--accent-bg); outline: 1px solid var(--accent); }
  .col:hover .in, .col:hover .out { opacity: 1; }
  .plus-half, .minus-half { position: relative; height: 50%; }
  .plus-half { border-bottom: 1px solid var(--line-strong); }
  .in, .out { position: absolute; left: 0; right: 0; opacity: 0.8; }
  .in { bottom: 0; background: var(--green); border-radius: 2px 2px 0 0; }
  .out { top: 0; background: var(--red); border-radius: 0 0 2px 2px; }
  .axis { display: flex; justify-content: space-between; color: var(--muted); font-size: 11px; margin-top: 4px; }
</style>
