<!-- Hub's one tab strip (design book, "Tabs"): 13 px underline tabs, the underline slides to the picked tab. Each page keeps its own state and passes the ids; `count` is the small number after the label, `end` pushes that tab and the ones after it to the right edge.
     Used by Customs.svelte, MachineView.svelte, ProblemDetail.svelte, ProjectView.svelte and 1 more. -->
<script lang="ts" generics="T extends string">
  // Hub's one tab strip (design book, "Tabs"): 13 px underline tabs, the underline slides to the
  // picked tab. Each page keeps its own state and passes the ids; `count` is the small number after
  // the label, `end` pushes that tab and the ones after it to the right edge.
  import { tick } from "svelte";

  type Item = { id: T; label: string; count?: number | string | null; amber?: boolean; tip?: string; end?: boolean };
  let { items, value, onchange, label = "Tabs", style = "" }: {
    items: Item[];
    value: T;
    onchange: (id: T) => void;
    label?: string;
    style?: string;
  } = $props();

  let bar: HTMLElement | undefined = $state();
  let ind = $state({ x: 0, w: 0, ready: false });

  async function place() {
    await tick();
    const b = bar?.querySelector<HTMLElement>('[aria-selected="true"]');
    if (!b) { ind.w = 0; return; }
    ind.x = b.offsetLeft;
    ind.w = b.offsetWidth;
    // The first placement jumps; later ones slide.
    requestAnimationFrame(() => (ind.ready = true));
  }
  $effect(() => {
    void value; void items.map((i) => i.label + i.count);
    place();
  });
  $effect(() => {
    if (!bar) return;
    const ro = new ResizeObserver(() => place());
    ro.observe(bar);
    return () => ro.disconnect();
  });

  const firstEnd = $derived(items.findIndex((i) => i.end));
</script>

<div class="tabs" role="tablist" aria-label={label} bind:this={bar} {style}>
  {#each items as it, i (it.id)}
    <button role="tab" class="no-press" class:end={i === firstEnd} aria-selected={value === it.id} data-tip={it.tip}
      onclick={() => onchange(it.id)}>
      {it.label}{#if it.count != null && it.count !== ""}<span class="n" class:amber={it.amber}>{it.count}</span>{/if}
    </button>
  {/each}
  <span class="ind" class:ready={ind.ready} style="width: {ind.w}px; transform: translateX({ind.x}px); opacity: {ind.w ? 1 : 0}"></span>
</div>

<style>
  .tabs { position: relative; display: flex; gap: 2px; margin: 0 0 14px; border-bottom: 1px solid var(--line); overflow-x: auto; scrollbar-width: none; flex: none; }
  .tabs::-webkit-scrollbar { display: none; }
  button { padding: 8px 12px; font-size: 13px; color: var(--muted); background: none; border-radius: 0; white-space: nowrap; display: inline-flex; gap: 6px; align-items: baseline; }
  button:hover { color: var(--text-2); }
  button[aria-selected="true"] { color: var(--text); }
  button.end { margin-left: auto; }
  .n { color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .n.amber { color: var(--amber); }
  .ind { position: absolute; left: 0; bottom: 0; height: 2px; background: var(--accent); pointer-events: none; }
  @media (prefers-reduced-motion: no-preference) {
    .ind.ready { transition: transform 0.2s var(--ease), width 0.2s var(--ease); }
  }
</style>
