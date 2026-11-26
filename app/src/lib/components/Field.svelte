<!-- A text setting: edited in place, saved with Enter, the Save button or by leaving the field.
     Used by EngineList.svelte, Settings.svelte. -->
<script lang="ts">
  // A text setting: edited in place, saved with Enter, the Save button or by leaving the field.
  let {
    value,
    placeholder = "",
    width = 240,
    onsave,
  }: { value: string; placeholder?: string; width?: number; onsave: (value: string) => void | Promise<void> } = $props();

  let draft = $state("");
  $effect(() => {
    draft = value;
  });
  const changed = $derived(draft.trim() !== value.trim());

  function save() {
    if (changed) onsave(draft.trim());
  }
</script>

<input class="field mono" style:width="{width}px" bind:value={draft} {placeholder} spellcheck="false"
  onkeydown={(e) => {
    if (e.key === "Enter") save();
    else if (e.key === "Escape") draft = value;
  }}
  onblur={save} />
{#if changed}<button class="btn primary" onmousedown={(e) => e.preventDefault()} onclick={save}>Save</button>{/if}

<style>
  .field { height: 28px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; min-width: 0; }
  .field:focus { border-color: var(--accent); }
</style>
