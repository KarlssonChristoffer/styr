<!-- Threads working: one thin line for how many threads worked at each moment of the last hours, with a scale, a hover that names who was working, and a row below for the times a thread asked you. The only colours are states: green for working, amber for an ask. Drawn from the latest turn of each thread (`workmap.ts`), or from the simulation.
     Used by scenes/SceneLive.svelte. -->
<script lang="ts">
  // Threads working: one thin line for how many threads worked at each moment of the last hours,
  // with a scale, a hover that names who was working, and a row below for the times a thread asked
  // you. The only colours are states: green for working, amber for an ask. Drawn from the latest
  // turn of each thread (`workmap.ts`), or from the simulation.
  import { hub } from "../../state.svelte";
  import { concurrency, hhmm, hourTicks, type Turn } from "../../workmap";

  let { turns, now, hours = 6 }: { turns: Turn[]; now: number; hours?: number } = $props();

  const N = $derived(hours * 6);
  const W = 1000;
  const H = 130;
  const from = $derived(now - hours * 3_600_000);
  const pts = $derived(concurrency(turns, from, now, N));
  const peak = $derived(Math.max(0, ...pts.map((p) => p.titles.length)));
  const max = $derived(Math.max(2, peak % 2 ? peak + 1 : peak));
  const yPct = (v: number) => 100 - (v / max) * 100;
  const path = $derived.by(() => {
    let d = "";
    pts.forEach((p, i) => {
      const y = H - (p.titles.length / max) * H;
      d += `${i ? " L" : "M"}${((i / N) * W).toFixed(1)},${y.toFixed(1)} L${(((i + 1) / N) * W).toFixed(1)},${y.toFixed(1)}`;
    });
    return d;
  });
  const asks = $derived(turns.filter((t) => t.waiting && t.work >= from));
  const ticks = $derived(hourTicks(from, now, 2));
  const current = $derived(pts.at(-1)?.titles.length ?? 0);

  let at = $state<number | null>(null);
  function move(e: PointerEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    at = Math.max(0, Math.min(N - 1, Math.floor(((e.clientX - r.left) / r.width) * N)));
  }
  const hit = $derived(at == null ? null : pts[at]);
  const flip = $derived(at != null && at > N / 2);
</script>

{#if !turns.length}
  <p class="quiet">No thread has worked in the last {hours} hours.</p>
{:else}
  <div class="chart">
    {#each [0, max / 2, max] as v (v)}
      <div class="gl" style:top="{yPct(v)}%"></div>
      <span class="yt" style:top="{yPct(v)}%">{v}</span>
    {/each}
    <div class="plot" role="img" aria-label="Threads working in the last {hours} hours, {current} now, at most {peak}" onpointermove={move} onpointerleave={() => (at = null)}>
      <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" aria-hidden="true">
        <path d="{path} L{W},{H} L0,{H} Z" fill="var(--green)" opacity="0.1" />
        <path d={path} fill="none" stroke="var(--green)" stroke-width="2" stroke-linejoin="round" vector-effect="non-scaling-stroke" />
      </svg>
      {#if hit && at != null}
        {@const x = ((at + 0.5) / N) * 100}
        <div class="cross" style:left="{x}%"></div>
        <i class="dot8" style:left="{x}%" style:top="{yPct(hit.titles.length)}%"></i>
        <div class="tip" class:flip style:left={flip ? undefined : `calc(${x}% + 14px)`} style:right={flip ? `calc(${100 - x}% + 14px)` : undefined}>
          <b>{hit.titles.length} working</b>
          <span class="when">{hhmm(hit.at)}</span>
          {#each hit.titles.slice(0, 3) as t, i (i)}<span class="who">{t}</span>{/each}
          {#if hit.titles.length > 3}<span class="who">+{hit.titles.length - 3} more</span>{/if}
        </div>
      {:else}
        <i class="dot8" style:right="0" style:left="auto" style:top="{yPct(current)}%"></i>
        <span class="now-n" class:below={yPct(current) < 30} style:top="{yPct(current)}%">{current} now</span>
      {/if}
    </div>
  </div>
  <div class="rail">
    {#each asks as t (t.id)}
      <button class="ask" style:left="{((t.work - from) / (now - from)) * 100}%" data-tip="{t.title} · asked you {hhmm(t.work)}" aria-label="{t.title} asked you at {hhmm(t.work)}" disabled={t.fake} onclick={() => hub.open(t.id)}></button>
    {/each}
  </div>
  <div class="axis">
    {#each ticks as k (k.at)}<span style:left="{k.at}%">{k.label}</span>{/each}
    <span class="now" style:left="100%">now</span>
  </div>
{/if}

<style>
  .chart { position: relative; height: 130px; margin-top: 4px; }
  .gl { position: absolute; left: 28px; right: 0; height: 1px; background: var(--line); }
  .yt { position: absolute; left: 0; width: 20px; text-align: right; color: var(--muted); font-size: 11px; transform: translateY(-50%); font-variant-numeric: tabular-nums; }
  .plot { position: absolute; left: 28px; right: 0; top: 0; bottom: 0; }
  svg { width: 100%; height: 100%; display: block; overflow: visible; }
  .cross { position: absolute; top: 0; bottom: 0; width: 1px; background: var(--line-strong); pointer-events: none; }
  .dot8 { position: absolute; width: 8px; height: 8px; margin: -4px 0 0 -4px; border-radius: 50%; background: var(--green); box-shadow: 0 0 0 2px var(--bg); pointer-events: none; }
  .now-n { position: absolute; right: 12px; transform: translateY(-170%); color: var(--text); font-size: 12px; pointer-events: none; }
  .now-n.below { transform: translateY(45%); }
  .tip { position: absolute; top: -6px; z-index: 3; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 8px 10px; display: flex; flex-direction: column; font-size: 12px; color: var(--muted); box-shadow: 0 8px 22px rgba(0, 0, 0, 0.4); white-space: nowrap; line-height: 1.5; pointer-events: none; }
  .tip b { color: var(--text); font-size: 13px; font-weight: 500; }
  .tip .who { color: var(--text-2); max-width: 260px; overflow: hidden; text-overflow: ellipsis; }
  .rail { position: relative; height: 22px; margin: 6px 0 0 28px; background: linear-gradient(var(--line), var(--line)) 0 50% / 100% 1px no-repeat; }
  .ask { position: absolute; top: 50%; width: 24px; height: 24px; margin: -12px 0 0 -12px; padding: 0; border-radius: 50%; background: transparent; }
  .ask::after { content: ""; position: absolute; left: 8px; top: 8px; width: 8px; height: 8px; border-radius: 50%; background: var(--amber); box-shadow: 0 0 0 2px var(--bg); }
  .ask:hover::after { box-shadow: 0 0 0 2px var(--bg), 0 0 0 4px color-mix(in srgb, var(--amber) 35%, transparent); }
  .axis { position: relative; height: 14px; margin: 4px 0 0 28px; }
  .axis span { position: absolute; top: 0; transform: translateX(-50%); font-size: 11px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .axis .now { transform: translateX(-100%); color: var(--accent); }
  .quiet { margin: 0; color: var(--muted); font-size: 12px; }
</style>
