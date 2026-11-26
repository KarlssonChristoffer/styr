<!-- Where it goes, one place opened wide: its use as bars on a time line you zoom and drag. Scroll zooms in and out around the pointer and the bars change what they stand for as you go (hours, days, weeks, months), dragging or a sideways scroll moves in time, and a click on a bar opens it (a month into its days, a day into its hours). Hub's bars are split by engine or by who asked; Claude's bars (`group`) by its lanes, where the estimate is striped and never sets the height (a bar it lifts over the rest is cut, with its sum on top); hovering a bar names its parts. The hours come from `usage.spend.hours` (the last 31 days); an older hubd gives days only, and the chart then stops at a day.
     Used by WhereItGoes.svelte. -->
<script lang="ts">
  // Where it goes, one place opened wide: its use as bars on a time line you zoom and drag. Scroll
  // zooms in and out around the pointer and the bars change what they stand for as you go (hours,
  // days, weeks, months), dragging or a sideways scroll moves in time, and a click on a bar opens
  // it (a month into its days, a day into its hours). Hub's bars are split by engine or by who
  // asked; Claude's bars (`group`) by its lanes, where the estimate is striped and never sets the
  // height (a bar it lifts over the rest is cut, with its sum on top); hovering a bar names its parts. The hours come from `usage.spend.hours` (the last 31
  // days); an older hubd gives days only, and the chart then stops at a day
  // (compat-until: 2026-12-04).
  import type { Spend } from "../bindings/Spend";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";

  type Part = { key: string; name: string; rgb: string; v: number; estimate?: boolean };
  type Bucket = { t: number; end: number; v: number; parts: Part[]; now: boolean; measured: number };
  type Unit = "hour" | "day" | "week" | "month";

  let {
    data,
    lane,
    group,
    range,
    engines,
    sources,
    amount,
  }: {
    /** Days (and hours, when the hubd has them) of every lane, oldest first. */
    data: Spend;
    lane: string;
    /** Lanes drawn together as one, each a part of the bars (Claude's terminal, cloud and estimate). */
    group?: Record<string, { name: string; rgb: string; estimate?: boolean }>;
    /** The page's range in days (0 = all time); the chart starts there, and again when it changes. */
    range: number;
    engines: Record<string, { name: string; rgb: string }>;
    sources: Record<string, string>;
    /** An amount as the page shows it: dollars, or tokens when the page counts those. */
    amount: (n: number) => string;
  } = $props();

  const HOUR = 3_600_000;
  const DAY = 24 * HOUR;
  /** Who asked, in the order of what they cost: Hub's palette. */
  const PALETTE = ["217, 119, 87", "169, 156, 240", "120, 196, 230", "108, 199, 184", "181, 207, 114", "236, 154, 100", "238, 138, 154", "224, 149, 214", "205, 180, 141", "163, 173, 187"];

  const midnight = (d: string) => new Date(`${d}T00:00:00`).getTime();
  const startOfDay = (t: number) => new Date(new Date(t).setHours(0, 0, 0, 0)).getTime();
  const today = () => startOfDay(Date.now());

  type Lanes = Spend["lanes"];
  /** One step's dollars and parts: a group's lanes are its parts, a lane's engines otherwise. */
  function step(ls: Lanes, i: number) {
    if (group) {
      const parts = ls.filter((l) => group[l.key]).map((l) => ({ key: l.key, v: l.daily[i] ?? 0 }));
      return { v: parts.reduce((n, p) => n + p.v, 0), engines: parts };
    }
    const l = ls.find((x) => x.key === lane);
    return { v: l?.daily[i] ?? 0, engines: (l?.engines ?? []).map((e) => ({ key: e.key, v: e.daily[i] ?? 0 })) };
  }
  /** Every day, oldest first, with this lane's dollars and its parts. */
  const days = $derived.by(() =>
    data.dates.map((d, i) => ({
      t: midnight(d),
      ...step(data.lanes, i),
      sources: lane === "hub" ? data.sources.map((s) => ({ key: s.key, v: s.daily?.[i] ?? 0 })) : [],
    })),
  );
  const hours = $derived.by(() => {
    const h = data.hourly;
    const ls = h?.lanes.filter((x) => (group ? group[x.key] : x.key === lane)) ?? [];
    if (!h || !ls.length) return null;
    return {
      start: h.start,
      list: ls[0].daily.map((_, i) => ({
        t: h.start + i * HOUR,
        ...step(ls, i),
        sources: lane === "hub" ? h.sources.map((s) => ({ key: s.key, v: s.daily?.[i] ?? 0 })) : [],
      })),
    };
  });
  /** Who asked splits Hub's bars only when the hubd says it per day. */
  const canSplit = $derived(lane === "hub" && data.sources.some((s) => s.daily));
  let split = $state<"engine" | "who">("engine");
  const colour = $derived.by(() => {
    const order = [...data.sources].sort((a, b) => b.cost - a.cost).map((s) => s.key);
    return (key: string) => (split === "who" ? PALETTE[order.indexOf(key) % PALETTE.length] : (group?.[key]?.rgb ?? engines[key]?.rgb ?? "189, 184, 176"));
  });

  /** The span the chart can show: the first day there is up to the end of today. */
  const first = $derived(days[0]?.t ?? today());
  const last = $derived(today() + DAY);
  const shortest = $derived(hours ? 6 * HOUR : DAY);

  // The window shown, in ms. It starts at the page's range and moves with the scroll.
  let t0 = $state(0);
  let t1 = $state(0);
  function reset() {
    const end = today() + DAY;
    const span = range === 1 ? DAY : range === 0 ? end - first : range * DAY;
    place(end - span, end);
  }
  $effect(() => {
    void range;
    void lane;
    void first;
    reset();
  });
  function place(a: number, b: number) {
    const span = Math.min(Math.max(b - a, shortest), last - first);
    a = Math.min(Math.max(a, first), last - span);
    [t0, t1] = [a, a + span];
  }

  let width = $state(600);
  /** The finest bars that fit: hours inside the last 31 days, then days, weeks and months. */
  const unit = $derived.by((): Unit => {
    const span = t1 - t0;
    const fits = (n: number) => n <= Math.max(24, width / 9);
    if (hours && t0 >= hours.start && fits(span / HOUR)) return "hour";
    if (fits(span / DAY)) return "day";
    if (fits(span / (7 * DAY))) return "week";
    return "month";
  });

  /** The start of the bucket `t` falls in. Weeks are counted back from today, so the last one
   *  ends today, as the cards and the busiest week beside the chart count them. */
  function floor(t: number, u: Unit) {
    if (u === "hour") return Math.floor(t / HOUR) * HOUR;
    const d = new Date(startOfDay(t));
    if (u === "week") {
      const end = new Date(today() + DAY);
      const back = Math.round((end.getTime() - d.getTime()) / DAY);
      end.setDate(end.getDate() - Math.ceil(back / 7) * 7);
      return end.getTime();
    }
    if (u === "month") d.setDate(1);
    return d.getTime();
  }
  function next(t: number, u: Unit) {
    if (u === "hour") return t + HOUR;
    const d = new Date(t);
    if (u === "day") d.setDate(d.getDate() + 1);
    else if (u === "week") d.setDate(d.getDate() + 7);
    else d.setMonth(d.getMonth() + 1);
    return d.getTime();
  }

  const buckets = $derived.by((): Bucket[] => {
    const u = unit;
    const base = u === "hour" ? (hours?.list ?? []) : days;
    const out: Bucket[] = [];
    const now = Date.now();
    let j = 0;
    for (let t = floor(t0, u); t < t1; t = next(t, u)) {
      const end = next(t, u);
      const sums = new Map<string, number>();
      let v = 0;
      while (j < base.length && base[j].t < t) j++;
      for (let k = j; k < base.length && base[k].t < end; k++) {
        v += base[k].v;
        for (const p of split === "who" && canSplit ? base[k].sources : base[k].engines) sums.set(p.key, (sums.get(p.key) ?? 0) + p.v);
      }
      const parts = [...sums]
        .filter(([, n]) => n > 0)
        .map(([key, n]) => ({ key, v: n, rgb: colour(key), estimate: group?.[key]?.estimate, name: split === "who" && canSplit ? (sources[key] ?? key) : (group?.[key]?.name ?? engines[key]?.name ?? key) }))
        .sort((a, b) => (group ? Object.keys(group).indexOf(a.key) - Object.keys(group).indexOf(b.key) : b.v - a.v));
      const measured = v - parts.filter((p) => p.estimate).reduce((n, p) => n + p.v, 0);
      out.push({ t, end, v, parts, now: now >= t && now < end, measured });
    }
    return out;
  });
  /** The scale: what was counted. An estimate only sets it where nothing was counted at all. */
  const top = $derived.by(() => {
    const counted = Math.max(0, ...buckets.map((b) => b.measured));
    const all = Math.max(0.0001, ...buckets.map((b) => b.v));
    // A quarter more than the most counted, so an estimate on top of a busy bar still fits.
    return counted ? Math.min(all, counted * 1.25) : all;
  });
  /** The biggest of the bars an estimate lifts over the scale: the one that names its sum. */
  const tallest = $derived(Math.max(0, ...buckets.filter((b) => b.v > top * 1.0001).map((b) => b.v)));
  /** A bar's parts from the bottom up, as shares of the bar drawn; a cut bar is full and its
   *  estimate takes what the counted parts leave. */
  function pieces(b: Bucket) {
    const cut = b.v > top * 1.0001;
    const real = b.measured;
    return [...b.parts].reverse().map((p) => ({ ...p, grow: cut ? (p.estimate ? Math.max(100 - (real / top) * 100, 8) : (p.v / top) * 100) : (p.v / b.v) * 100 }));
  }
  const inView = $derived(buckets.reduce((n, b) => n + b.v, 0));
  const word = $derived(unit === "hour" ? "per hour" : `per ${unit}`);

  const fmt = (t: number, o: Intl.DateTimeFormatOptions) => new Date(t).toLocaleString(undefined, o);
  /** "14:00": hours in 24, short enough to stand under a thin bar. */
  const clock = (t: number) => `${String(new Date(t).getHours()).padStart(2, "0")}:00`;
  /** A bar's name in the axis: short, with the day or month where it turns. */
  function label(b: Bucket, i: number) {
    const d = new Date(b.t);
    if (unit === "hour") return d.getHours() === 0 || i === 0 ? fmt(b.t, { weekday: "short", day: "numeric" }) : clock(b.t);
    if (unit === "day") return d.getDate() === 1 || i === 0 ? fmt(b.t, { day: "numeric", month: "short" }) : `${d.getDate()}`;
    if (unit === "week") return fmt(b.t, { day: "numeric", month: "short" });
    return d.getMonth() === 0 || i === 0 ? fmt(b.t, { month: "short", year: "numeric" }) : fmt(b.t, { month: "short" });
  }
  /** What a bar covers, in words for its card. */
  function when(b: Bucket) {
    if (unit === "hour") return `${fmt(b.t, { weekday: "short", day: "numeric", month: "short" })} · ${clock(b.t)}–${clock(b.end)}`;
    if (unit === "day") return fmt(b.t, { weekday: "long", day: "numeric", month: "short" });
    if (unit === "week") return `Week of ${fmt(b.t, { day: "numeric", month: "short" })}`;
    return fmt(b.t, { month: "long", year: "numeric" });
  }
  const span = $derived.by(() => {
    const a = fmt(t0, { day: "numeric", month: "short" });
    const b = fmt(t1 - 1, { day: "numeric", month: "short", year: t1 - t0 > 300 * DAY ? "numeric" : undefined });
    return unit === "hour" && t1 - t0 <= DAY ? fmt(t0, { weekday: "short", day: "numeric", month: "short" }) : `${a} – ${b}`;
  });
  /** The axis: each bar's label where it fits. From the newest back, a label is kept only when it
   *  clears the one kept after it, so "21 mars" and "16 apr." never run together whatever the
   *  language. A label is measured from its length (12 px text, about 7 px a letter). */
  const labels = $derived.by(() => {
    const n = buckets.length;
    const col = width / Math.max(1, n);
    const out: string[] = new Array(n).fill("");
    let left = Infinity;
    for (let i = n - 1; i >= 0; i--) {
      const text = label(buckets[i], i);
      const w = text.length * 7 + 4;
      const x = Math.min(Math.max((i + 0.5) * col - w / 2, 0), width - w);
      if (x + w + 8 <= left) {
        out[i] = text;
        left = x;
      }
    }
    return out;
  });

  // Scroll zooms around the pointer; a sideways scroll, Shift or a drag moves in time.
  let chart = $state<HTMLDivElement | null>(null);
  function zoom(factor: number, at = 0.5) {
    const s = t1 - t0;
    const pivot = t0 + at * s;
    const n = Math.min(Math.max(s * factor, shortest), last - first);
    place(pivot - at * n, pivot - at * n + n);
  }
  function pan(ms: number) {
    place(t0 + ms, t1 + ms);
  }
  function wheel(e: WheelEvent) {
    if (!chart) return;
    e.preventDefault();
    const r = chart.getBoundingClientRect();
    const sideways = e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY);
    if (sideways) pan(((e.shiftKey ? e.deltaY : e.deltaX) / r.width) * (t1 - t0));
    else zoom(Math.exp(e.deltaY * 0.0015), Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)));
  }
  $effect(() => {
    const el = chart;
    if (!el) return;
    el.addEventListener("wheel", wheel, { passive: false });
    return () => el.removeEventListener("wheel", wheel);
  });
  let drag: { x: number; t0: number; moved: boolean } | null = null;
  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    drag = { x: e.clientX, t0, moved: false };
  }
  function move(e: PointerEvent) {
    hoverAt(e);
    if (!drag || !chart) return;
    const dx = e.clientX - drag.x;
    if (!drag.moved && Math.abs(dx) < 4) return;
    if (!drag.moved) chart.setPointerCapture(e.pointerId);
    drag.moved = true;
    const s = t1 - t0;
    place(drag.t0 - (dx / chart.getBoundingClientRect().width) * s, drag.t0 - (dx / chart.getBoundingClientRect().width) * s + s);
  }
  function up() {
    const clicked = drag && !drag.moved;
    drag = null;
    if (clicked && hover !== null) open(buckets[hover]);
  }
  /** A bar opened: a month into its days, a week into its days, a day into its hours. */
  function open(b: Bucket | undefined) {
    if (!b || unit === "hour") return;
    if (unit === "day" && !hours) return;
    place(b.t, b.end);
  }
  function key(e: KeyboardEvent) {
    const s = t1 - t0;
    if (e.key === "+" || e.key === "=") zoom(1 / 1.5);
    else if (e.key === "-") zoom(1.5);
    else if (e.key === "ArrowLeft") pan(-s / 4);
    else if (e.key === "ArrowRight") pan(s / 4);
    else return;
    e.preventDefault();
  }

  let hover = $state<number | null>(null);
  let tipX = $state(0);
  function hoverAt(e: PointerEvent) {
    if (!chart) return;
    const r = chart.getBoundingClientRect();
    const i = Math.floor(((e.clientX - r.left) / r.width) * buckets.length);
    hover = i >= 0 && i < buckets.length ? i : null;
    tipX = e.clientX - r.left;
  }
  const shown = $derived(hover !== null && buckets[hover]?.v > 0 ? buckets[hover] : null);
</script>

<div class="zoom">
  <div class="bar-head">
    <span class="what">{word}, {span} · <b>{amount(inView)}</b> in view</span>
    {#if canSplit}
      <Select size="small" label="Split by" bind:value={split} options={[{ value: "engine", label: "By engine" }, { value: "who", label: "By who asked" }]} />
    {/if}
    <span class="tools">
      <button class="btn" aria-label="Zoom out" data-tip="Zoom out (or scroll)" onclick={() => zoom(1.5)}><Icon name="minus" size={13} /></button>
      <button class="btn" aria-label="Zoom in" data-tip="Zoom in (or scroll)" onclick={() => zoom(1 / 1.5)}><Icon name="plus" size={13} /></button>
    </span>
  </div>
  <!-- A zoomable, keyboard-driven chart is a custom widget (role application); the lint does not know that role as interactive. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div class="plot" bind:this={chart} bind:clientWidth={width} role="application" tabindex="0"
    aria-label="{word}, {span}: {amount(inView)}. Scroll or plus and minus zoom, arrows move in time."
    onpointerdown={down} onpointermove={move} onpointerup={up} onpointerleave={() => (hover = null)} onkeydown={key}
    class:can={unit !== "hour" && (unit !== "day" || !!hours)}>
    {#each buckets as b, i (b.t)}
      {@const cut = b.v > top * 1.0001}
      {@const h = b.v > 0 ? (cut ? 82 : Math.max((b.v / top) * 82, 1.5)) : 0}
      <div class="col" class:now={b.now} class:lit={hover === i}>
        <em class:est={cut}>{b.v > 0 && ((cut && b.v === tallest) || buckets.length <= 10 || b.v === top) ? `${cut ? "≈" : ""}${amount(b.v)}` : ""}</em>
        {#if (lane === "hub" || group) && b.parts.length}
          <span class="bar stack" class:cut style:height="{h}%">
            {#each pieces(b) as p (p.key)}<i class:est={p.estimate} style:flex-grow={p.grow} style:--c={p.rgb}></i>{/each}
          </span>
        {:else}
          <span class="bar" class:empty={!b.v} style:height={b.v > 0 ? `${h}%` : "2px"}></span>
        {/if}
        <small>{labels[i]}</small>
      </div>
    {/each}
    {#if shown}
      <div class="tip" style:left="{Math.min(Math.max(tipX, 90), width - 90)}px">
        <b>{when(shown)} · {shown.parts.some((p) => p.estimate) ? "≈" : ""}{amount(shown.v)}</b>
        {#each shown.parts.slice(0, 6) as p (p.key)}
          <span><i class:est={p.estimate} style:--c={p.rgb}></i>{p.name}{p.estimate ? ", estimated" : ""}<em>{p.estimate ? "≈" : ""}{amount(p.v)}</em></span>
        {/each}
        {#if unit !== "hour" && (unit !== "day" || hours) && shown.v > 0}<small>Click to open {unit === "day" ? "its hours" : "its days"}</small>{/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .zoom { min-width: 0; }
  .bar-head { display: flex; align-items: center; gap: 8px; min-height: 26px; color: var(--muted); font-size: 12px; }
  .what { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .what b { color: var(--text-2); font-weight: 500; font-variant-numeric: tabular-nums; }
  .tools { margin-left: auto; display: flex; gap: 4px; }
  .tools .btn { padding: 3px 6px; }
  .plot { position: relative; display: flex; align-items: flex-end; gap: 3px; height: 210px; margin-top: 10px; touch-action: none; user-select: none; outline: none; cursor: grab; }
  .plot:focus-visible { box-shadow: 0 0 0 1px var(--line-strong); border-radius: var(--r-sm); }
  .plot:active { cursor: grabbing; }
  .col { flex: 1; min-width: 0; height: 100%; display: flex; flex-direction: column; justify-content: flex-end; gap: 4px; }
  .col em, .col small { display: flex; justify-content: center; white-space: nowrap; min-height: 15px; line-height: 15px; font-variant-numeric: tabular-nums; }
  .col:first-child em, .col:first-child small { justify-content: flex-start; }
  .col:last-child em, .col:last-child small { justify-content: flex-end; }
  .col:only-child em, .col:only-child small { justify-content: center; }
  .col em { font-style: normal; color: var(--text-2); font-size: 12px; }
  .col small { color: var(--muted); font-size: 12px; }
  /* Edge bars: flat, in the place's colour (--a), a solid line along the top. */
  .bar { flex: none; align-self: center; width: 100%; max-width: 72px; position: relative; overflow: hidden; border-radius: 3px 3px 2px 2px; background: rgba(var(--a), 0.3); box-shadow: inset 0 0 0 1px rgba(var(--a), 0.35); }
  .bar::before { content: ""; position: absolute; left: 0; right: 0; top: 0; height: 3px; background: rgb(var(--a)); }
  .bar.empty { opacity: 0.35; }
  .bar.empty::before { height: 1px; box-shadow: none; }
  .now .bar:not(.stack), .lit .bar:not(.stack) { background: rgba(var(--a), 0.5); }
  .bar.stack { display: flex; flex-direction: column; gap: 1px; background: none; box-shadow: none; opacity: 0.85; }
  .bar.stack::before { content: none; }
  .now .bar.stack, .lit .bar.stack { opacity: 1; }
  /* Grow in parts of a hundred: shares under 1 would leave the bar part empty. */
  .bar.stack i { display: block; flex: 0 0 0; min-height: 1.5px; background: rgba(var(--c), 0.3); box-shadow: inset 0 2px 0 rgb(var(--c)), inset 0 0 0 1px rgba(var(--c), 0.3); }
  /* Claude's estimate: striped, never lit, because it is a guess and not a count. A bar it lifts
     over the counted scale is drawn full and cut near its top, with its sum above it. */
  .bar.stack i.est { background: repeating-linear-gradient(135deg, rgba(var(--c), 0.38) 0 3px, rgba(var(--c), 0.08) 3px 7px); box-shadow: inset 0 0 0 1px rgba(var(--c), 0.5); }
  .bar.stack.cut::after { content: ""; position: absolute; left: -2px; right: -2px; top: 8px; height: 3px; background: var(--surface); transform: skewY(-14deg); }
  .col em.est { color: rgb(204, 160, 112); }
  .tip span i.est { background: repeating-linear-gradient(135deg, rgba(var(--c), 0.9) 0 1.5px, transparent 1.5px 3px); box-shadow: inset 0 0 0 1px rgba(var(--c), 0.7); }
  .plot.can .col { cursor: zoom-in; }
  .tip { position: absolute; top: 0; transform: translateX(-50%); z-index: 2; pointer-events: none; display: flex; flex-direction: column; gap: 3px; min-width: 150px; padding: 8px 10px; border-radius: var(--r-sm); background: var(--surface-2); box-shadow: 0 0 0 1px var(--line-strong), 0 6px 18px rgba(0, 0, 0, 0.35); font-size: 12px; color: var(--text-2); }
  .tip b { color: var(--text); font-weight: 500; white-space: nowrap; }
  .tip span { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
  .tip span i { width: 7px; height: 7px; border-radius: 2px; background: rgb(var(--c)); flex: none; }
  .tip span em { margin-left: auto; padding-left: 12px; font-style: normal; font-variant-numeric: tabular-nums; color: var(--text); }
  .tip small { color: var(--muted); }
</style>
