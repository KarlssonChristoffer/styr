<!-- Every thread of the last hours as one thin lane, grouped by project: green while it worked, grey when it came back, red when it stopped with an error and an amber tail while it waited on you. A click opens the thread. Drawn from the latest turn of each thread (`workmap.ts`), or from the simulation.
     Used by scenes/SceneLive.svelte. -->
<script lang="ts">
  // Every thread of the last hours as one thin lane, grouped by project: green while it worked,
  // grey when it came back, red when it stopped with an error and an amber tail while it waited on
  // you. A click opens the thread. Drawn from the latest turn of each thread (`workmap.ts`), or from
  // the simulation.
  import { hub } from "../../state.svelte";
  import { endOf, hhmm, hourTicks, lanesOf, type Turn } from "../../workmap";
  import Ref from "../Ref.svelte";

  let { turns, now, hours = 6 }: { turns: Turn[]; now: number; hours?: number } = $props();

  const MAX = 14;
  const from = $derived(now - hours * 3_600_000);
  const span = $derived(now - from);
  const pct = (ms: number) => Math.max(0, Math.min(100, ((ms - from) / span) * 100));
  const ticks = $derived(hourTicks(from, now, 1));
  const lanes = $derived.by(() => {
    let left = MAX;
    const out = [];
    for (const l of lanesOf(turns, now)) {
      if (left <= 0) break;
      out.push({ ...l, turns: l.turns.slice(0, left), more: Math.max(0, l.turns.length - left) });
      left -= l.turns.length;
    }
    return out;
  });
  const total = $derived(turns.length);
  const shown = $derived(lanes.reduce((n, l) => n + l.turns.length, 0));
  const said = (t: Turn) => {
    const mins = Math.max(1, Math.round((endOf(t, now) - t.start) / 60_000));
    const what = t.kind === "run" ? `working for ${mins} min` : t.kind === "fail" ? `failed at ${hhmm(t.work)}` : t.waiting ? `waiting on you since ${hhmm(t.work)}` : `done at ${hhmm(t.work)}`;
    return `${t.title} · ${what}`;
  };
  const status = (t: Turn) => (t.kind === "run" ? "running" : t.kind === "fail" ? "error" : t.waiting ? "waiting_on_you" : "idle");
</script>

{#if !turns.length}
  <p class="quiet">No thread has worked in the last {hours} hours.</p>
{:else}
  <div class="lanes">
    <div class="row axis"><span></span><div class="tk">{#each ticks as k (k.at)}<span style:left="{k.at}%">{k.label}</span>{/each}<span class="now" style:left="100%">now</span></div></div>
    <div class="body">
      {#each lanes as l (l.key)}
        <div class="row grp">
          <div class="nm">
            {#if l.turns[0]?.fake}<span class="fk">{l.project}</span>{:else}<Ref kind="project" project={{ path: l.cwd, host: l.host }} label={l.project} />{/if}
            <span class="c">{l.turns.length + l.more} {l.turns.length + l.more === 1 ? "thread" : "threads"}</span>
          </div>
          <div class="tk"></div>
        </div>
        {#each l.turns as t (t.id)}
          <div class="row">
            <button class="nm th" disabled={t.fake} data-tip={said(t)} onclick={() => hub.open(t.id)}><span class="dot {status(t)}"></span><span class="t grow">{t.title}</span></button>
            <div class="tk">
              {#each ticks as k (k.at)}<i class="vg" style:left="{k.at}%"></i>{/each}
              <i class="bar {t.kind}" style:left="{pct(t.start)}%" style:width="max(6px, {pct(t.work) - pct(t.start)}%)" data-tip={said(t)}></i>
              {#if t.waiting}<i class="bar wait" style:left="{pct(t.work)}%" style:width="{100 - pct(t.work)}%" data-tip={said(t)}></i>{/if}
            </div>
          </div>
        {/each}
      {/each}
      <i class="nowline"></i>
    </div>
  </div>
  {#if total > shown}<p class="caption">{total - shown} more {total - shown === 1 ? "thread" : "threads"} not shown</p>{/if}
{/if}

<style>
  .lanes { display: flex; flex-direction: column; }
  .row { display: grid; grid-template-columns: minmax(120px, 28%) minmax(0, 1fr); align-items: center; }
  .nm { display: flex; align-items: center; gap: 8px; min-width: 0; height: 26px; padding-right: 12px; font-size: 13px; color: var(--text-2); text-align: left; }
  .th:not(:disabled):hover { color: var(--text); }
  .t { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tk { position: relative; height: 26px; }
  .axis .tk { height: 14px; }
  .axis .tk span { position: absolute; top: 0; transform: translateX(-50%); font-size: 11px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .axis .tk .now { transform: translateX(-100%); color: var(--accent); }
  .body { position: relative; }
  .grp .nm { height: 30px; margin-top: 8px; color: var(--text); }
  .grp .tk { height: 30px; margin-top: 8px; border-bottom: 1px solid var(--line); }
  .grp:first-child .nm, .grp:first-child .tk { margin-top: 0; }
  .c { margin-left: auto; color: var(--muted); font-size: 11px; }
  .fk { color: var(--text); }
  .vg { position: absolute; top: 0; bottom: 0; width: 1px; background: var(--line); opacity: 0.6; }
  .bar { position: absolute; top: 9px; height: 8px; border-radius: var(--r-xs); background: color-mix(in srgb, var(--text) 22%, var(--bg)); }
  .bar.run { background: var(--green); }
  .bar.fail { background: var(--red); }
  .bar.wait { background: var(--amber); border-radius: 0 var(--r-xs) var(--r-xs) 0; }
  .nowline { position: absolute; top: 0; bottom: 0; right: 0; width: 1px; background: var(--accent); opacity: 0.6; pointer-events: none; }
  .quiet { margin: 0; color: var(--muted); font-size: 12px; }
</style>
