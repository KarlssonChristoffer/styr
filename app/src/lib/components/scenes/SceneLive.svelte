<!-- Live: how many threads worked over the last hours, every thread as a lane, what waits on you, what works now, what finished lately and what it cost today. The chart and the lanes read the simulation instead of the real threads while Simulating is on (`sim.svelte.ts`).
     Used by Scenes.svelte. -->
<script lang="ts">
  import { poll } from "../../poll";
  import Quiet from "../Quiet.svelte";
  // Live: how many threads worked over the last hours, every thread as a lane, what waits on you,
  // what works now, what finished lately and what it cost today. The chart and the lanes read the
  // simulation instead of the real threads while Simulating is on (`sim.svelte.ts`).
  import { api } from "../../api";
  import type { Stats } from "../../bindings/Stats";
  import { sim } from "../../sim.svelte";
  import { hub, waitsOnYou } from "../../state.svelte";
  import { turnsOf } from "../../workmap";
  import ThreadLanes from "./ThreadLanes.svelte";
  import ThreadRows from "./ThreadRows.svelte";
  import WorkChart from "./WorkChart.svelte";

  const HOURS = 6;
  let clock = $state(Date.now());
  $effect(() => {
    const t = poll(() => (clock = Date.now()), 30_000);
    return () => t();
  });
  const now = $derived(sim.on ? sim.now : clock);
  const turns = $derived(sim.on ? sim.turns : turnsOf(hub.everything, now, now - HOURS * 3_600_000));
  const asking = $derived(hub.active.filter(waitsOnYou));

  const finished = $derived(
    hub.active
      .filter((t) => t.status === "idle" && !t.settled_at && t.returned_at != null && Date.now() - t.returned_at < 24 * 3_600_000)
      .sort((a, b) => (b.returned_at ?? 0) - (a.returned_at ?? 0))
      .slice(0, 4),
  );

  const tz = -new Date().getTimezoneOffset();
  let stats = $state<Stats | null>(null);
  $effect(() => {
    api.stats(1, tz).then((s) => (stats = s), () => {});
  });
  const tokens = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)} M` : n >= 1e3 ? `${Math.round(n / 1e3)} k` : String(n));
  const top = $derived(stats?.models.slice(0, 4) ?? []);
  const most = $derived(Math.max(1, ...top.map((m) => m.tokens)));
</script>

<section class="s12"><div class="lbl">Threads working, last {HOURS} hours <span class="r">{#if sim.on}simulated{:else}amber dot = it asked you{/if}</span></div>
  <div class="blk p"><WorkChart {turns} {now} hours={HOURS} /></div></section>
<section class="s12"><div class="lbl">Every thread, last {HOURS} hours
  <span class="r key"><i class="g"></i>working <i class="a"></i>waits on you <i class="r2"></i>failed <i class="d"></i>done{#if sim.on}<span class="sm">simulated</span>{/if}</span></div>
  <div class="blk p"><ThreadLanes {turns} {now} hours={HOURS} /></div></section>
<section class={asking.length ? "s12" : "s6"} style:order={asking.length ? -1 : 0}><div class="lbl">Needs you <span class="n">{asking.length}</span></div>
  <div class="blk"><ThreadRows threads={asking} empty="Nothing waits on you." /></div></section>
<section class="s6 c"><div class="lbl">Running now <span class="n">{hub.running.length}</span></div>
  <div class="blk"><ThreadRows threads={hub.running} empty="Nothing is running." clock /></div></section>
<section class="s6 c"><div class="lbl">Just finished <span class="n">{finished.length}</span></div>
  <div class="blk"><ThreadRows threads={finished} empty="Nothing finished in the last day." /></div></section>
<section class="s6"><div class="lbl">Used today{#if stats}<span class="r">{tokens(stats.tokens)} tokens{stats.cost ? ` · about $${stats.cost.toFixed(2)} at API prices` : ""}</span>{/if}</div>
  <div class="blk p">
    {#if !stats}<Quiet kind="loading" />
    {:else if !top.length}<Quiet text="No answers yet today" />
    {:else}
      <div class="hb">
        {#each top as m (m.model)}
          <span>{m.model}</span><span class="bar"><i style:width="{(m.tokens / most) * 100}%"></i></span><span class="v">{tokens(m.tokens)}</span>
        {/each}
      </div>
    {/if}
  </div></section>

<style>
  .key { display: inline-flex; align-items: center; gap: 4px; }
  .key i { width: 14px; height: 8px; border-radius: var(--r-xs); margin-left: 8px; }
  .key i:first-child { margin-left: 0; }
  .key .sm { margin-left: 10px; }
  .key .g { background: var(--green); }
  .key .a { background: var(--amber); }
  .key .r2 { background: var(--red); }
  .key .d { background: color-mix(in srgb, var(--text) 22%, var(--bg)); }
</style>
