<!-- The working indicator: System's 2 by 2 grid playing a little loop (`workGrid.ts`). Grey is the grid, green is what works. With Reduce motion (or the system's setting) it stands still. A tile made of parts (Pour, Crumble) draws each part at rest clipped to its tile's rounded shape, so it reads as one tile with round corners and no seams.
     Used by InstallCard.svelte, StatusLine.svelte, Steps.svelte, Transcript.svelte and 1 more. -->
<script lang="ts">
  // The working indicator: System's 2 by 2 grid playing a little loop (`workGrid.ts`). Grey is the
  // grid, green is what works. With Reduce motion (or the system's setting) it stands still.
  // A tile made of parts (Pour, Crumble) draws each part at rest clipped to its tile's rounded
  // shape, so it reads as one tile with round corners and no seams.
  import { SHAPES, type WorkShape } from "../workGrid";
  import { prefs } from "../prefs.svelte";
  import { watchWorkClock, workClock } from "../workClock.svelte";

  let { shape, size = 16, hold = false }: { shape: WorkShape; size?: number; /** Stands still: a second grid near a moving one (one moving grid per thread view). */ hold?: boolean } = $props();

  const def = $derived(SHAPES[shape] as (typeof SHAPES)[WorkShape] & { turn?: (t: number) => number });
  const still = $derived(hold || prefs.reduceMotion || (typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches));
  // One clock for every grid on screen (`workClock.svelte.ts`), so a list of them costs one frame loop.
  $effect(() => (still ? undefined : watchWorkClock()));
  const t = $derived(still ? 0 : workClock.t);

  const tiles = $derived(def.tiles(t));
  const turn = $derived(def.turn ? def.turn(t) : 0);
  const id = `wg${Math.random().toString(36).slice(2, 8)}`;
  const SLOTS: [number, number][] = [[4, 4], [13, 4], [4, 13], [13, 13]];
  const h = $derived(def.sw / 2);
  const clipped = $derived(tiles.some((b) => b.clip != null));
  // A seated part on its tile's edge reaches out over the outline, so the tile keeps its size.
  const out = (b: (typeof tiles)[number]) => {
    const [tx, ty] = SLOTS[b.clip!];
    const x = b.x <= tx + 0.01 ? b.x - h : b.x;
    const y = b.y <= ty + 0.01 ? b.y - h : b.y;
    return { x, y, w: b.w + (b.x - x) + (b.x + b.w >= tx + 7 - 0.01 ? h : 0), h: b.h + (b.y - y) + (b.y + b.h >= ty + 7 - 0.01 ? h : 0) };
  };
</script>

<svg class="work-grid" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
  {#if clipped}
    <defs>
      {#each SLOTS as [x, y], k (k)}
        <clipPath id="{id}-{k}"><rect x={x - h} y={y - h} width={7 + def.sw} height={7 + def.sw} rx={1.6 + h} /></clipPath>
      {/each}
    </defs>
  {/if}
  <g style:transform="rotate({turn}deg)">
    {#each tiles as b, i (i)}
      {#if b.clip != null}
        {@const r = out(b)}
        <rect x={r.x} y={r.y} width={r.w} height={r.h} clip-path="url(#{id}-{b.clip})" opacity={b.o} class="seated" />
      {:else if b.ns}
        <rect x={b.x} y={b.y} width={Math.max(0, b.w)} height={Math.max(0, b.h)} rx={Math.max(0, b.r)} opacity={b.o} class="seated" />
      {:else}
        <rect x={b.x} y={b.y} width={Math.max(0, b.w)} height={Math.max(0, b.h)} rx={Math.max(0, b.r)} opacity={b.o}
          stroke-width={def.sw} fill-opacity={b.f} style:stroke="color-mix(in srgb, var(--green) {Math.round(b.g * 100)}%, var(--muted))" />
      {/if}
    {/each}
  </g>
</svg>

<style>
  .work-grid { display: block; overflow: visible; flex: none; }
  g { transform-origin: 12px 12px; }
  rect { fill: var(--green); }
  /* Parts (seated or on their way) are opaque, so where two meet there is no darker seam; the colour is what a whole
     tile shows inside its outline (green at its fill over the page). */
  rect.seated { fill: color-mix(in srgb, var(--green) 85%, var(--bg)); }
</style>
