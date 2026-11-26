<!-- Pulse: how busy the agents are, as a line that moves. Tool calls a minute over the last ten minutes, counted from the stream this window already has, with a dot where now is.
     Used by CardBody.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  // Pulse: how busy the agents are, as a line that moves. Tool calls a minute over the last ten
  // minutes, counted from the stream this window already has, with a dot where now is.
  import { rate } from "../pulse.svelte";
  import { isChatThread } from "../format";
  import { hub } from "../state.svelte";
  import HowCounted from "./HowCounted.svelte";

  const SLICE = 10_000;
  const N = 60;
  let now = $state(Date.now());
  $effect(() => {
    const t = poll(() => (now = Date.now()), 2000);
    return () => t();
  });
  const buckets = $derived(rate(now, N, SLICE));
  const perMin = $derived(buckets.slice(-6).reduce((a, b) => a + b, 0));
  const top = $derived(Math.max(3, ...buckets));
  const W = 600, H = 110;
  const pts = $derived(buckets.map((v, i) => [(i / (N - 1)) * W, H - 6 - (v / top) * (H - 18)] as const));
  const line = $derived(pts.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(" "));
  const last = $derived(pts[pts.length - 1]);
  const working = $derived(hub.threads.filter((t) => t.status === "running" && !t.archived && !isChatThread(t)).length);
</script>

<HowCounted card="pulse">
  <p>Agent tool calls per ten seconds, last ten minutes. Not stored.</p>
</HowCounted>

<svg viewBox="0 0 {W} {H}" width="100%" preserveAspectRatio="none" class="pic" aria-hidden="true">
  <line x1="0" x2={W} y1={H - 6} y2={H - 6} stroke="var(--line-strong)" />
  <polygon points="0,{H - 6} {line} {W},{H - 6}" fill="var(--green)" opacity="0.14" />
  <polyline points={line} fill="none" stroke="var(--green)" stroke-width="1.6" stroke-linejoin="round" vector-effect="non-scaling-stroke" />
</svg>
<div class="axis"><span>10 min ago</span><span>now</span></div>
<p class="caption"><span class="figure">{perMin}</span> tool calls a minute{working ? ` · ${working} ${working === 1 ? "thread" : "threads"} working` : " · nothing is working"}</p>

<style>
  .pic { display: block; height: clamp(90px, 16cqw, 150px); }
  .axis { display: flex; justify-content: space-between; color: var(--muted); font-size: 11px; margin-top: 4px; }
  .caption { display: flex; align-items: baseline; gap: 8px; }
</style>
