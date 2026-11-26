<!-- Usage map: a square per part of Hub, brighter the more it was used over the period picked in the card's menu, a dashed frame for what you never touched. Counted by the core from names and times only (docs/ARCHITECTURE.md, "The usage log"); the card only draws what it answers.
     Used by CardBody.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import Quiet from "./Quiet.svelte";
  // Usage map: a square per part of Hub, brighter the more it was used over the period picked in
  // the card's menu, a dashed frame for what you never touched. Counted by the core from names
  // and times only (docs/ARCHITECTURE.md, "The usage log"); the card only draws what it answers.
  import { api } from "../api";
  import { knob, onAction, setKnob } from "../overview.svelte";
  import { prefs } from "../prefs.svelte";
  import { USE_FEATURES, loadUse, useOff, uselog } from "../uselog.svelte";
  import HowCounted from "./HowCounted.svelte";

  const days = $derived.by(() => {
    void prefs.overviewFrames;
    return Number(knob<string>("uselog", "range"));
  });
  const off = $derived.by(() => {
    void prefs.overviewFrames;
    return knob<boolean>("uselog", "off");
  });

  $effect(() => {
    void loadUse(days);
  });
  // The switch in the menu reaches the core and this window.
  let sent: boolean | null = null;
  $effect(() => {
    const v = off;
    if (sent !== null && v !== sent) quiet(api.useOff(v), "use off");
    sent = v;
    useOff(v);
  });
  onAction("uselog", "clear", () => {
    api.useClear().then(() => loadUse(days), () => {});
  });
  // The core's own switch wins if it is off already (set from another window).
  $effect(() => {
    if (uselog.data && uselog.data.off !== off && sent === off) setKnob("uselog", "off", uselog.data.off);
  });

  type Tile = { id: string; label: string; area: string; count: number; last: number | null; median: number | null };
  const areas = $derived.by(() => {
    const counted = new Map((uselog.data?.features ?? []).map((f) => [f.feature, f]));
    const all = new Map<string, Tile>();
    for (const [id, f] of Object.entries(USE_FEATURES)) {
      const c = counted.get(id);
      all.set(id, { id, label: f.label, area: f.area, count: c?.count ?? 0, last: c?.last_ts ?? null, median: c?.median_ms ?? null });
    }
    for (const c of counted.values()) if (!all.has(c.feature)) all.set(c.feature, { id: c.feature, label: c.feature, area: c.area, count: c.count, last: c.last_ts, median: c.median_ms });
    const by = new Map<string, Tile[]>();
    for (const t of all.values()) by.set(t.area, [...(by.get(t.area) ?? []), t]);
    return [...by].map(([area, tiles]) => ({ area, tiles: tiles.sort((a, b) => b.count - a.count) }));
  });
  const top = $derived(Math.max(1, ...areas.flatMap((a) => a.tiles.map((t) => t.count))));
  // Brightness runs on a log scale so a few heavy features do not wash out the rest.
  const level = (n: number) => (n === 0 ? 0 : Math.min(4, 1 + Math.floor((Math.log(n) / Math.log(top + 1)) * 4)));
  // Deliberately not format.ts `ago`: whole days worded for a tooltip ("yesterday", "3 days ago").
  const ago = (ts: number) => {
    const d = Math.floor((Date.now() - ts) / 86_400_000);
    return d <= 0 ? "today" : d === 1 ? "yesterday" : `${d} days ago`;
  };
  const tip = (t: Tile) =>
    t.count ? `${t.label}: ${t.count} ${t.count === 1 ? "use" : "uses"}, last ${ago(t.last!)}${t.median ? `, usually ${t.median < 1000 ? `${t.median} ms` : `${(t.median / 1000).toFixed(1)} s`}` : ""}` : `${t.label}: not used in this period`;
  const name = (a: string) => a.charAt(0).toUpperCase() + a.slice(1);
  const unused = $derived(areas.reduce((n, a) => n + a.tiles.filter((t) => t.count === 0).length, 0));
</script>

{#if uselog.old}
  <Quiet kind="newer" />
{:else if uselog.error}
  <p class="quiet">The usage log could not be read.</p>
{:else if !uselog.data}
  <Quiet kind="loading" />
{:else}
  {#if uselog.data.off}<p class="quiet">Counting is off. Nothing new is kept.</p>{/if}
  {#each areas as a (a.area)}
    <div class="area">
      <span class="aname">{name(a.area)}</span>
      <span class="tiles">
        {#each a.tiles as t (t.id)}
          <span class="tile l{level(t.count)}" data-tip={tip(t)}>{t.label}{#if t.count}<i>{t.count.toLocaleString()}</i>{/if}</span>
        {/each}
      </span>
    </div>
  {/each}
  <p class="caption">{unused ? `${unused} not used in this period. ` : ""}Only counts, kept on this computer.</p>
{/if}

<HowCounted card="uselog">
  <p>Pages opened and buttons pressed, never anything you type. Kept 400 days on this computer.</p>
</HowCounted>

<style>
  .area { display: grid; grid-template-columns: 64px 1fr; gap: 8px; margin-bottom: 8px; align-items: start; }
  .aname { color: var(--text-3); font-size: 12px; padding-top: 4px; }
  .tiles { display: flex; flex-wrap: wrap; gap: 4px; }
  .tile { display: inline-flex; align-items: center; gap: 6px; height: 24px; padding: 0 8px; border-radius: var(--r-xs); font-size: 12px; color: var(--text-2); border: 1px solid transparent; }
  .tile i { font-style: normal; color: var(--muted); font-size: 11px; }
  .l0 { border: 1px dashed var(--line-strong); color: var(--muted); }
  .l1 { background: color-mix(in srgb, var(--accent) 10%, transparent); }
  .l2 { background: color-mix(in srgb, var(--accent) 22%, transparent); }
  .l3 { background: color-mix(in srgb, var(--accent) 38%, transparent); }
  .l4 { background: color-mix(in srgb, var(--accent) 58%, transparent); color: var(--text); }
  .quiet { color: var(--muted); font-size: 12px; margin: 0 0 6px; }
</style>
