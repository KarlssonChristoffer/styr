<!-- One output as a line in a library: its kind's glyph (or a small picture), its name, a quiet detail and when. Clicking opens it big; the owner decides what "big" is for its kind.
     Used by ThreadPanel.svelte. -->
<script lang="ts">
  // One output as a line in a library: its kind's glyph (or a small picture), its name, a quiet
  // detail and when. Clicking opens it big; the owner decides what "big" is for its kind.
  import { KINDS, type ViewKind } from "./kinds";
  import Icon from "../components/Icon.svelte";

  let {
    kind,
    name,
    detail = "",
    thumb,
    tip = "Open big",
    onopen,
    onmenu,
  }: {
    kind: ViewKind;
    name: string;
    detail?: string;
    /** A small picture instead of the glyph. */
    thumb?: string;
    tip?: string;
    onopen: () => void;
    onmenu?: (e: MouseEvent) => void;
  } = $props();
</script>

<button class="row" data-tip={tip} onclick={onopen} oncontextmenu={onmenu}>
  {#if thumb}<img class="thumb" src={thumb} alt="" />{:else}<span class="glyph"><Icon name={KINDS[kind].icon} size={12} /></span>{/if}
  <span class="name">{name}</span>
  {#if detail}<span class="detail">{detail}</span>{/if}
</button>

<style>
  .row { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 6px; border-radius: var(--r-sm); color: var(--muted); text-align: left; }
  .row:hover { background: var(--surface); color: var(--text-2); }
  .glyph { display: grid; flex: none; width: 18px; place-items: center; }
  .thumb { width: 18px; height: 14px; object-fit: cover; border-radius: 3px; flex: none; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); }
  .detail { flex: none; margin-left: auto; color: var(--text-3); font-size: 12px; font-variant-numeric: tabular-nums; }
</style>
