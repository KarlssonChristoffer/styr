<!-- A section of a machine's page that folds: threads, services, builds, containers. Folded, it still says what it holds and what that takes of the machine's memory. Hub remembers which sections are open for each machine (folds.svelte.ts).
     Used by MachineView.svelte. -->
<script lang="ts">
  // A section of a machine's page that folds: threads, services, builds, containers. Folded, it
  // still says what it holds and what that takes of the machine's memory. Hub remembers which
  // sections are open for each machine (folds.svelte.ts).
  import { bytes } from "../format";
  import { folds } from "../folds.svelte";
  import type { Snippet } from "svelte";

  let {
    machine,
    id,
    title,
    count,
    mem = 0,
    total = 0,
    open = true,
    tone = "var(--text-3)",
    icon,
    children,
  }: {
    machine: string;
    id: string;
    title: string;
    count: number;
    /** What the section takes of memory, in bytes. */
    mem?: number;
    /** The machine's memory, for the share. */
    total?: number;
    /** Open until the person folds it. */
    open?: boolean;
    tone?: string;
    icon?: Snippet;
    children: Snippet;
  } = $props();

  const isOpen = $derived(folds.isOpen(machine, id, open));
</script>

<section class="grp" class:shut={!isOpen}>
  <button class="gh" aria-expanded={isOpen} onclick={() => folds.toggle(machine, id, open)}>
    <svg class="cv" class:shut={!isOpen} width="11" height="11" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" /></svg>
    {#if icon}<span class="gi" style:color={tone}>{@render icon()}</span>{/if}
    <b>{title}</b>
    {#if count > 0}<small>{count}</small>{/if}
    {#if mem > 0}
      <span class="gs"><span class="mono">{bytes(mem)}</span><span class="pc mono">{total ? Math.round((mem / total) * 100) : 0} %</span></span>
    {/if}
  </button>
  {#if isOpen}<div class="body">{@render children()}</div>{/if}
</section>

<style>
  /* Each group is a card of its own: the header row is the handle (an arrow that turns, a lighter
     row on hover), the list sits inside the card, so where one list ends and the next begins is
     plain even when the groups are folded. */
  .grp { min-width: 0; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); overflow: hidden; }
  .body { max-height: clamp(180px, 36vh, 400px); overflow-y: auto; scrollbar-gutter: stable; border-top: 1px solid var(--line); padding: 4px 6px 6px; }
  .gh { display: flex; align-items: center; gap: 8px; width: 100%; padding: 8px 12px; color: var(--text); text-align: left; font-size: 13px; }
  .gh:hover { background: var(--surface-hover); }
  .gh b { font-weight: 500; }
  .gh small { color: var(--muted); font-size: 12px; }
  .gh:hover .cv { color: var(--text); }
  .cv { color: var(--text-3); flex: none; transition: transform var(--dur-fast) var(--ease); }
  .cv.shut { transform: rotate(-90deg); }
  .gi { display: grid; place-items: center; }
  .gs { margin-left: auto; display: flex; gap: 12px; color: var(--muted); font-size: 12px; }
  .gs .pc { width: 38px; text-align: right; }
  .mono { font-family: var(--mono); }
</style>
