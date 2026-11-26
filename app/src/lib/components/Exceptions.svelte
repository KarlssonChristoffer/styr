<!-- "Except in": projects that differ from a setting, folded under it (Settings > Engines and Personality). Shut it names the projects; open it lists each with what it uses there, and a click on one shows `editor` for it in place. The setting itself stays the default above.
     Used by EngineKnobs.svelte, Personality.svelte. -->
<script lang="ts">
  // "Except in": projects that differ from a setting, folded under it (Settings > Engines and
  // Personality). Shut it names the projects; open it lists each with what it uses there, and a
  // click on one shows `editor` for it in place. The setting itself stays the default above.
  import type { Snippet } from "svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import Select from "./Select.svelte";

  let {
    items,
    onadd,
    onremove,
    editor,
  }: {
    /** The projects that differ, by folder, and what they use there in a few words. */
    items: { path: string; summary: string }[];
    onadd: (path: string) => void;
    onremove: (path: string) => void;
    editor: Snippet<[string]>;
  } = $props();

  let open = $state(false);
  let editing = $state<string | null>(null);
  let adding = $state(false);

  // Real projects only: not the scratch folders "No project" threads run in.
  const real = $derived(hub.projects.filter((p) => !/[\\/]\.hub-threads[\\/]|scratch/i.test(p.path)));
  const nameOf = (path: string) => real.find((p) => p.path === path)?.name ?? path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  const addable = $derived(real.filter((p) => !items.some((i) => i.path === p.path)).map((p) => ({ value: p.path, label: p.name, hint: p.path })));

  function add(path: string | undefined) {
    adding = false;
    if (!path) return;
    onadd(path);
    open = true;
    editing = path;
  }
</script>

<div class="except">
  {#if items.length}
    <button class="fold" aria-expanded={open} onclick={() => ((open = !open), (editing = null))}>
      <span class="l">Except in</span>
      {#if !open}<span class="names">{items.map((i) => nameOf(i.path)).join(", ")}</span>{/if}
      <span class="chev" class:open><Icon name="chevron" size={11} /></span>
    </button>
  {/if}
  {#if open && items.length}
    {#each items as i (i.path)}
      <div class="row" class:on={editing === i.path}>
        <button class="pick" data-tip={i.path} onclick={() => (editing = editing === i.path ? null : i.path)}>
          <ProjectIcon path={i.path} name={nameOf(i.path)} size={15} />
          <span class="name">{nameOf(i.path)}</span>
          <span class="sum">{i.summary}</span>
        </button>
        <button class="x" data-tip="Use the default there again" aria-label={`Drop ${nameOf(i.path)}'s exception`} onclick={() => onremove(i.path)}><Icon name="x" size={11} /></button>
      </div>
      {#if editing === i.path}<div class="edit">{@render editor(i.path)}</div>{/if}
    {/each}
  {/if}
  {#if !items.length || open}
    {#if adding}
      <Select size="small" label="Pick a project" placeholder="Pick a project" autoopen options={addable} onchange={(v) => add(v)} onclose={() => (adding = false)} />
    {:else if addable.length}
      <button class="add" onclick={() => (adding = true)}><Icon name="plus" size={11} />{items.length ? "A project" : "Except in a project"}</button>
    {/if}
  {/if}
</div>

<style>
  .except { display: flex; flex-direction: column; gap: 2px; border-top: 1px solid var(--line); padding-top: 8px; margin-top: auto; }
  .fold { display: flex; align-items: center; gap: 8px; padding: 2px 0; text-align: left; min-width: 0; }
  .l { font-size: 12px; color: var(--muted); flex: none; }
  .names { font-size: 13px; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .chev { display: inline-flex; color: var(--muted); margin-left: auto; transition: transform var(--dur-fast) var(--ease); }
  .chev.open { transform: rotate(90deg); }
  .fold:hover .names, .fold:hover .chev { color: var(--text); }
  .row { display: flex; align-items: center; border-radius: var(--r-sm); }
  .row:hover, .row.on { background: var(--surface-hover); }
  .pick { flex: 1; display: flex; align-items: center; gap: 8px; padding: 4px 6px; min-width: 0; text-align: left; font-size: 13px; }
  .name { color: var(--text-2); white-space: nowrap; }
  .sum { margin-left: auto; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .x { display: inline-flex; color: var(--muted); padding: 4px; border-radius: var(--r-xs); }
  .x:hover { color: var(--text); }
  .edit { border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 8px 10px; margin: 2px 0 4px; display: flex; flex-direction: column; gap: 8px; }
  .add { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); font-size: 13px; padding: 4px 6px; align-self: flex-start; }
  .add:hover { color: var(--text); }
</style>
