<!-- A project's icon: a coloured line icon Hub picks from its name, or the icon, colour or picture the person chose (`looks.svelte.ts`). With `editable`, clicking it opens the picker.
     Used by ActivityList.svelte, CommandPalette.svelte, Exceptions.svelte, FolderPicker.svelte and 8 more. -->
<script lang="ts">
  // A project's icon: a coloured line icon Hub picks from its name, or the icon, colour or
  // picture the person chose (`looks.svelte.ts`). With `editable`, clicking it opens the picker.
  import { looks, PROJECT_ICONS, projectKey } from "../looks.svelte";

  let {
    path,
    host = null,
    name,
    size = 18,
    editable = false,
  }: { path: string; host?: string | null; name: string; size?: number; editable?: boolean } = $props();

  const look = $derived(looks.project(path, host, name));

  function pick(e: MouseEvent) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    looks.picking = { key: projectKey(path, host), name, x: r.left, y: r.bottom + 6 };
  }
</script>

{#snippet face()}
  {#if look.image}
    <img src={look.image} alt="" width={size} height={size} />
  {:else}
    <svg viewBox="0 0 24 24" width={size} height={size} style:color="var(--p-{look.color})" aria-hidden="true"><path d={PROJECT_ICONS[look.icon]} /></svg>
  {/if}
{/snippet}

{#if editable}
  <button class="project-icon edit" style:width="{size}px" style:height="{size}px" data-tip="Change icon" onclick={pick}>{@render face()}</button>
{:else}
  <span class="project-icon" style:width="{size}px" style:height="{size}px" aria-hidden="true">{@render face()}</span>
{/if}

<style>
  .project-icon { flex: none; display: inline-grid; place-items: center; }
  svg { fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  img { border-radius: 22%; object-fit: cover; display: block; }
  .edit { padding: 0; border-radius: var(--r-sm); cursor: pointer; }
  .edit:hover { outline: 1px solid var(--line-strong); outline-offset: 3px; }
</style>
