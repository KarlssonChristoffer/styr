<!-- Timeline: the last hours as lanes, one per project. Each thread's latest turn is a bar from the moment you sent it work to when it came back (grey), still going (green) or stopped with an error (red); a thread that then waits on you carries an amber tail up to now. It answers "what happened while I was away, and what is going on": a click on a bar opens the thread. Drawn from the threads the window already has, so only the latest turn of each is known.
     Used by CardBody.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  // Timeline: the last hours as lanes, one per project. Each thread's latest turn is a bar from the
  // moment you sent it work to when it came back (grey), still going (green) or stopped with an
  // error (red); a thread that then waits on you carries an amber tail up to now. It answers "what
  // happened while I was away, and what is going on": a click on a bar opens the thread. Drawn from
  // the threads the window already has, so only the latest turn of each is known.
  import type { Thread } from "../bindings/Thread";
  import { byProject, isScratch } from "../format";
  import { knob } from "../overview.svelte";
  import { hub } from "../state.svelte";
  import HowCounted from "./HowCounted.svelte";

  let now = $state(Date.now());
  $effect(() => {
    const t = poll(() => (now = Date.now()), 30_000);
    return () => t();
  });

  const hours = $derived(Number(knob<string>("timeline", "hours")));
  const from = $derived(now - hours * 3_600_000);
  const span = $derived(now - from);
  const pct = (ms: number) => Math.max(0, Math.min(100, ((ms - from) / span) * 100));

  type Bar = { t: Thread; start: number; work: number; tail: number | null; kind: "run" | "done" | "fail"; row: number };
  type Lane = { key: string; name: string; cwd: string; host: string | null; bars: Bar[]; rows: number; at: number };

  /** One thread's latest turn: when it started, where the work ended, and what follows. */
  function turn(t: Thread): Omit<Bar, "row"> | null {
    const start = t.sent_at ?? t.created_at;
    const running = t.status === "running";
    const work = running ? now : Math.min(now, Math.max(start, t.returned_at ?? t.updated_at));
    const end = t.status === "waiting_on_you" ? now : work;
    if (end < from) return null;
    return { t, start: Math.max(start, from), work: Math.max(work, from), tail: t.status === "waiting_on_you" && work < now ? now : null, kind: running ? "run" : t.status === "error" ? "fail" : "done" };
  }

  const lanes = $derived.by<Lane[]>(() => {
    const out: Lane[] = [];
    for (const p of byProject(hub.everything, []).filter((p) => !isScratch(p.cwd))) {
      const bars = p.threads.map(turn).filter((b): b is Omit<Bar, "row"> => !!b).sort((a, b) => a.start - b.start);
      if (!bars.length) continue;
      // Bars that overlap in time go on rows of their own, so none hides another.
      const ends: number[] = [];
      const placed = bars.map((b) => {
        const end = b.tail ?? b.work;
        let row = ends.findIndex((e) => e <= b.start);
        if (row < 0) row = ends.length;
        ends[row] = end + span * 0.004;
        return { ...b, row };
      });
      out.push({ key: p.key, name: p.name, cwd: p.cwd, host: p.host, bars: placed, rows: ends.length, at: Math.max(...placed.map((b) => b.tail ?? b.work)) });
    }
    return out.sort((a, b) => b.at - a.at).slice(0, 7);
  });
  const count = $derived(lanes.reduce((n, l) => n + l.bars.length, 0));
  const working = $derived(lanes.reduce((n, l) => n + l.bars.filter((b) => b.kind === "run").length, 0));

  /** Ticks on the axis: every hour or two or four, or twelve for days, on the clock's whole hours. */
  const ticks = $derived.by(() => {
    const step = hours <= 6 ? 1 : hours <= 12 ? 2 : hours <= 24 ? 4 : 12;
    const out: { at: number; label: string }[] = [];
    const first = new Date(from);
    first.setMinutes(0, 0, 0);
    for (let d = first.getTime() + 3_600_000; d < now - span * 0.04; d += 3_600_000) {
      const h = new Date(d).getHours();
      if (h % step !== 0) continue;
      const label = hours > 24 && h === 0 ? new Date(d).toLocaleDateString([], { weekday: "short" }) : `${String(h).padStart(2, "0")}`;
      if (d > from + span * 0.02) out.push({ at: pct(d), label });
    }
    return out;
  });
  const said = (b: Bar) => `${b.t.title} · ${b.kind === "run" ? "working" : b.kind === "fail" ? "failed" : b.tail ? "waiting on you" : "done"}`;
</script>

<HowCounted card="timeline">
  <p>One bar per thread's latest turn, a lane per project.</p>
</HowCounted>

{#if !lanes.length}
  <p class="quiet">No thread has worked in the last {hours} hours.</p>
{:else}
  <div class="lanes">
    {#each lanes as l (l.key)}
      <div class="lane">
        <span class="name" data-tip={l.host ? `${l.host}: ${l.cwd}` : l.cwd}>{l.name}</span>
        <div class="track" style:height="{l.rows * 24 - 4}px">
          {#each l.bars as b (b.t.id)}
            {@const w = pct(b.work) - pct(b.start)}
            <button class="bar {b.kind}" style:left="{pct(b.start)}%" style:width="max(6px, {w}%)" style:top="{b.row * 24}px" data-tip={said(b)} aria-label={said(b)} onclick={() => hub.open(b.t.id)}></button>
            {#if b.tail}<button class="bar wait" style:left="{pct(b.work)}%" style:width="{100 - pct(b.work)}%" style:top="{b.row * 24}px" data-tip={said(b)} aria-label={said(b)} onclick={() => hub.open(b.t.id)}></button>{/if}
          {/each}
        </div>
      </div>
    {/each}
    <div class="lane axis">
      <span></span>
      <div class="track ticks">
        {#each ticks as k (k.at)}<span style:left="{k.at}%">{k.label}</span>{/each}
        <span class="now" style:left="100%">now</span>
      </div>
    </div>
  </div>
  <p class="caption"><em>{count}</em> {count === 1 ? "thread" : "threads"} in the last {hours === 72 ? "3 days" : `${hours} hours`}{working ? ` · ${working} working` : ""}</p>
{/if}

<style>
  .lanes { display: flex; flex-direction: column; gap: 8px; }
  .lane { display: grid; grid-template-columns: minmax(70px, 22%) 1fr; align-items: start; gap: 12px; }
  .name { font-size: 13px; color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; line-height: 20px; }
  .track { position: relative; min-width: 0; background-image: linear-gradient(var(--line), var(--line)); background-size: 100% 1px; background-position: 0 10px; background-repeat: no-repeat; }
  .bar { position: absolute; height: 20px; border-radius: var(--r-sm); padding: 0; min-width: 6px; background: var(--surface-hover); border: 1px solid var(--line-strong); }
  .bar.run { background: color-mix(in srgb, var(--green) 26%, transparent); border-color: color-mix(in srgb, var(--green) 60%, transparent); animation: pulse 2s var(--ease-move) infinite; }
  .bar.fail { background: color-mix(in srgb, var(--red) 26%, transparent); border-color: color-mix(in srgb, var(--red) 60%, transparent); }
  .bar.wait { background: var(--amber-bg); border-color: color-mix(in srgb, var(--amber) 55%, transparent); border-left: 0; border-radius: 0 var(--r-sm) var(--r-sm) 0; }
  .bar:hover { filter: brightness(1.3); }
  .axis { margin-top: 2px; }
  .ticks { height: 14px; background: none; }
  .ticks span { position: absolute; top: 0; transform: translateX(-50%); font-size: 11px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .ticks .now { transform: translateX(-100%); color: var(--accent); }
</style>
