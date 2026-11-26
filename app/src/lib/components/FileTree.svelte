<!-- One folder level, loaded when it is first opened.
     Used by MachineView.svelte, ProjectView.svelte. -->
<script lang="ts">
  import { api } from "../api";
  import type { TreeEntry } from "../bindings/TreeEntry";
  import { menu } from "../menu.svelte";
  import { fileMenu, join } from "../menus.svelte";
  import { hub } from "../state.svelte";
  import FileTree from "./FileTree.svelte";
  import Icon from "./Icon.svelte";

  // One folder level, loaded when it is first opened.
  let {
    path,
    rel = "",
    depth = 0,
    selected = null,
    host = null,
    onopen,
  }: {
    path: string;
    rel?: string;
    depth?: number;
    selected?: string | null;
    /** The machine the folder is on, when it is not this one. */
    host?: string | null;
    onopen: (rel: string) => void;
  } = $props();

  let entries = $state<TreeEntry[] | null>(null);
  let open = $state<Record<string, boolean>>({});

  $effect(() => {
    api.projectTree(path, rel, host).then((e) => (entries = e)).catch((e) => hub.fail(e));
  });
</script>

{#if entries === null}
  <div class="row muted" style:padding-left="{8 + depth * 14}px">…</div>
{:else}
  {#each entries as e (e.rel)}
    {#if e.dir}
      <button class="row" style:padding-left="{8 + depth * 14}px" onclick={() => (open[e.rel] = !open[e.rel])} use:menu={() => fileMenu(join(path, e.rel), { rel: e.rel })}>
        <span class="chev" class:open={open[e.rel]}><Icon name="chevron" size={12} /></span>
        <Icon name="folder" size={13} /><span class="name">{e.name}</span>
      </button>
      {#if open[e.rel]}<FileTree {path} rel={e.rel} depth={depth + 1} {selected} {host} {onopen} />{/if}
    {:else}
      <button class="row file" class:active={selected === e.rel} style:padding-left="{8 + depth * 14 + 16}px" onclick={() => onopen(e.rel)} use:menu={() => fileMenu(join(path, e.rel), { rel: e.rel, open: () => onopen(e.rel) })}>
        <Icon name="file" size={13} /><span class="name">{e.name}</span>
      </button>
    {/if}
  {:else}
    {#if depth === 0}<div class="row muted">Empty folder</div>{/if}
  {/each}
{/if}

<style>
  .row {
    display: flex; align-items: center; gap: 6px; width: 100%; height: 26px;
    padding-right: 8px; border-radius: var(--r-sm); color: var(--text-3); font-size: 13px; text-align: left;
  }
  .row:hover { background: var(--surface); color: var(--text-2); }
  .row.active { background: var(--surface-hover); color: var(--text); }
  .row :global(svg) { flex: none; color: var(--muted); }
  .muted { color: var(--muted); }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chev { display: grid; transition: transform 0.12s var(--ease); }
  .chev.open { transform: rotate(90deg); }
</style>
