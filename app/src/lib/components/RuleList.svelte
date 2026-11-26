<!-- A short list of patterns: each a chip with a cross, and a field to add one with Enter.
     Used by Settings.svelte. -->
<script lang="ts">
  // A short list of patterns: each a chip with a cross, and a field to add one with Enter.
  import Icon from "./Icon.svelte";

  let {
    items,
    placeholder = "",
    onchange,
  }: { items: string[]; placeholder?: string; onchange: (items: string[]) => void | Promise<void> } = $props();

  let draft = $state("");

  function add() {
    const v = draft.trim();
    draft = "";
    if (v && !items.includes(v)) onchange([...items, v]);
  }
</script>

<div class="rules">
  {#each items as item (item)}
    <span class="chip mono">{item}<button data-tip="Remove" aria-label="Remove {item}" onclick={() => onchange(items.filter((i) => i !== item))}><Icon name="x" size={11} /></button></span>
  {/each}
  <input class="add mono" bind:value={draft} placeholder={items.length ? "Add…" : placeholder} spellcheck="false"
    onkeydown={(e) => {
      if (e.key === "Enter") add();
      else if (e.key === "Escape") draft = "";
    }}
    onblur={add} />
</div>

<style>
  .rules { display: flex; flex-wrap: wrap; align-items: center; gap: 4px; margin-top: 8px; }
  .chip { display: inline-flex; align-items: center; gap: 4px; height: 24px; padding: 0 4px 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 12px; color: var(--text-2); }
  .chip button { display: grid; place-items: center; width: 16px; height: 16px; border-radius: var(--r-xs); color: var(--muted); }
  .chip button:hover { color: var(--text); background: var(--line-strong); }
  .add { flex: 1; min-width: 160px; height: 24px; padding: 0 8px; background: transparent; border: 1px dashed var(--line-strong); border-radius: var(--r-md); font-size: 12px; outline: none; }
  .add:focus { border-style: solid; border-color: var(--accent); }
</style>
