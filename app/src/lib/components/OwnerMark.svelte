<!-- The robot on a thread Hub or an agent started, not you, with who did: "Guard", "Daily review", "Customs". Grey like every kind glyph; colour stays for state (`system.ts`).
     Used by ActivityList.svelte, ProjectView.svelte, ThreadView.svelte. -->
<script lang="ts">
  // The robot on a thread Hub or an agent started, not you, with who did: "Guard", "Daily
  // review", "Customs". Grey like every kind glyph; colour stays for state (`system.ts`).
  import type { Owner } from "../bindings/Owner";
  import { GLYPHS } from "../refs";
  import { ownerName, ownerTip } from "../system";

  let { owner, name = true }: { owner: Owner; name?: boolean } = $props();
</script>

<span class="owner" data-tip={ownerTip(owner)}>
  <svg viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.robot} /></svg>{#if name}<span class="nm">{ownerName(owner)}</span>{/if}
</span>

<style>
  .owner { display: inline-flex; align-items: center; gap: 3px; min-width: 0; max-width: 14ch; flex-shrink: 1; color: var(--muted); font-size: 0.92em; white-space: nowrap; }
  .nm { overflow: hidden; text-overflow: ellipsis; }
  svg { width: 12px; height: 12px; flex: none; fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
</style>
