<!-- A load as a smooth wave: values are shares of capacity, 0 to 1, oldest first, null where there is no reading yet (a graph fills from the right as readings come). The line is the accent while it is calm, amber past `warn` and red past `bad`, so a full machine reads at a glance; the dashed line marks where amber begins. `mini` is the small one inside a card.
     Used by MachineCard.svelte, MachineView.svelte, MachinesView.svelte. -->
<script lang="ts" module>
  let seq = 0;
</script>

<script lang="ts">
  // A load as a smooth wave: values are shares of capacity, 0 to 1, oldest first, null where there
  // is no reading yet (a graph fills from the right as readings come). The line is the accent while
  // it is calm, amber past `warn` and red past `bad`, so a full machine reads at a glance; the
  // dashed line marks where amber begins. `mini` is the small one inside a card.
  let {
    values,
    mini = false,
    warn = 0.8,
    bad = 0.92,
    tone = "accent",
    marks = true,
  }: {
    values: (number | null)[];
    mini?: boolean;
    warn?: number;
    bad?: number;
    tone?: "accent" | "blue" | "grey";
    marks?: boolean;
  } = $props();

  const id = `wv${++seq}`;
  const W = 600;
  const H = 120;
  const n = $derived(Math.max(values.length - 1, 1));
  const X = (i: number) => (i / n) * W;
  const Y = (v: number) => H - Math.min(1, Math.max(0, v)) * H;

  /** Runs of readings with no gap, as a smooth path each. */
  const runs = $derived.by(() => {
    const out: { line: string; area: string }[] = [];
    let cur: number[] = [];
    const flush = () => {
      if (cur.length === 1) {
        // The first reading: a short stub until the next one comes.
        const d = `M${X(cur[0]) - 10},${Y(values[cur[0]]!)} L${X(cur[0])},${Y(values[cur[0]]!)}`;
        out.push({ line: d, area: "" });
      } else if (cur.length > 1) {
        let d = `M${X(cur[0])},${Y(values[cur[0]]!)}`;
        for (let k = 1; k < cur.length; k++) {
          const a = cur[k - 1];
          const b = cur[k];
          const cx = (X(a) + X(b)) / 2;
          d += ` C${cx},${Y(values[a]!)} ${cx},${Y(values[b]!)} ${X(b)},${Y(values[b]!)}`;
        }
        out.push({ line: d, area: `${d} L${X(cur[cur.length - 1])},${H} L${X(cur[0])},${H}Z` });
      }
      cur = [];
    };
    // A gap of a few buckets (a window in the background reads less often) is drawn across.
    let gap = 0;
    values.forEach((v, i) => {
      if (v != null) {
        gap = 0;
        cur.push(i);
      } else if (cur.length && ++gap > 3) flush();
    });
    flush();
    return out;
  });
  const redAt = $derived(Math.min(1, Math.max(0, 1 - bad)));
  const amberAt = $derived(Math.min(1, Math.max(0, 1 - warn)));
</script>

<svg class="wv {tone}" class:mini viewBox="0 0 {W} {H}" preserveAspectRatio="none" aria-hidden="true">
  <defs>
    {#each [["l", 1], ["f", 0.9]] as [k, o] (k)}
      <linearGradient id="{id}{k}" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2={H}>
        <stop offset="0" class="red" stop-opacity={o} />
        <stop offset={redAt} class="red" stop-opacity={o} />
        <stop offset={redAt + 0.005} class="amber" stop-opacity={o} />
        <stop offset={amberAt} class="amber" stop-opacity={o} />
        <stop offset={amberAt + 0.005} class="base" stop-opacity={o} />
        <stop offset="1" class="base" stop-opacity={o} />
      </linearGradient>
    {/each}
    <linearGradient id="{id}m" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#fff" stop-opacity="0.55" />
      <stop offset="1" stop-color="#fff" stop-opacity="0" />
    </linearGradient>
    <mask id="{id}k"><rect width={W} height={H} fill="url(#{id}m)" /></mask>
  </defs>
  {#if marks && !mini}
    {#if bad < 1}<rect x="0" y="0" width={W} height={H * amberAt} class="band" />{/if}
    <path d="M0,{H * amberAt}H{W}" class="thr" vector-effect="non-scaling-stroke" />
  {/if}
  {#each runs as r, i (i)}
    <path d={r.area} fill="url(#{id}f)" mask="url(#{id}k)" />
    <path d={r.line} fill="none" stroke="url(#{id}l)" stroke-width={mini ? 1.5 : 2} vector-effect="non-scaling-stroke" stroke-linecap="round" />
  {/each}
</svg>

<style>
  .wv { display: block; width: 100%; height: 100%; overflow: visible; }
  .base { stop-color: var(--accent); }
  .blue .base { stop-color: #7fa6ee; }
  .grey .base { stop-color: var(--text-3); }
  .red { stop-color: var(--red); }
  .amber { stop-color: var(--amber); }
  .band { fill: var(--red); opacity: 0.05; }
  .thr { stroke: var(--line-strong); stroke-dasharray: 3 5; fill: none; }
</style>
