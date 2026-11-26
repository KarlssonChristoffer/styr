<!-- A JSON value as a tree that folds: objects and arrays show their size when closed, values are coloured by type, and clicking a key copies its path (`$.items[2].name`). Children are only drawn once opened, so big files stay quick.
     Used by FilePreview.svelte. -->
<script lang="ts">
  import { untrack } from "svelte";
  // A JSON value as a tree that folds: objects and arrays show their size when closed, values are
  // coloured by type, and clicking a key copies its path (`$.items[2].name`). Children are only
  // drawn once opened, so big files stay quick.
  import { copy } from "../menu.svelte";
  import JsonTree from "./JsonTree.svelte";

  let {
    value,
    name = null,
    path = "$",
    depth = 0,
    open: openDepth = 2,
    last = true,
  }: {
    value: unknown;
    /** The key or index this value sits under; null at the top. */
    name?: string | number | null;
    path?: string;
    depth?: number;
    /** Levels open from the start. */
    open?: number;
    last?: boolean;
  } = $props();

  const LIMIT = 200;
  const branch = $derived(value !== null && typeof value === "object");
  const entries = $derived(
    !branch ? [] : Array.isArray(value) ? value.map((v, i) => [i, v] as const) : Object.entries(value as Record<string, unknown>),
  );
  let opened = $state(untrack(() => depth < openDepth));
  let all = $state(false);
  const shown = $derived(all ? entries : entries.slice(0, LIMIT));
  const isArray = $derived(Array.isArray(value));
  const childPath = (k: string | number) => (typeof k === "number" ? `${path}[${k}]` : /^[A-Za-z_$][\w$]*$/.test(k) ? `${path}.${k}` : `${path}[${JSON.stringify(k)}]`);

  let copied = $state(false);
  function copyPath() {
    copy(path).then(() => {
      copied = true;
      setTimeout(() => (copied = false), 1200);
    });
  }
  const kind = (v: unknown) => (v === null ? "null" : typeof v);
  const show = (v: unknown) => (typeof v === "string" ? JSON.stringify(v) : String(v));
</script>

<div class="row" class:top={depth === 0}>
  {#if branch}
    <button class="fold" aria-label={opened ? "Fold" : "Open"} onclick={() => (opened = !opened)}><span class="arrow" class:opened>›</span></button>
  {:else}
    <span class="fold"></span>
  {/if}
  {#if name !== null}
    <button class="key" class:idx={typeof name === "number"} data-tip={copied ? "Copied" : `Copy ${path}`} onclick={copyPath}>{typeof name === "number" ? name : JSON.stringify(name)}</button><span class="colon">:</span>
  {/if}
  {#if branch}
    <button class="open" onclick={() => (opened = !opened)}>
      {isArray ? "[" : "{"}{#if !opened}<span class="size">{entries.length} {isArray ? (entries.length === 1 ? "item" : "items") : entries.length === 1 ? "key" : "keys"}</span>{isArray ? "]" : "}"}{#if !last},{/if}{/if}
    </button>
  {:else}
    <span class="v {kind(value)}">{show(value)}</span>{#if !last}<span class="colon">,</span>{/if}
  {/if}
</div>
{#if branch && opened}
  <div class="kids">
    {#each shown as [k, v], i (k)}
      <JsonTree value={v} name={k} path={childPath(k)} depth={depth + 1} open={openDepth} last={i === entries.length - 1} />
    {/each}
    {#if entries.length > shown.length}
      <button class="more" onclick={() => (all = true)}>Show {entries.length - shown.length} more</button>
    {/if}
  </div>
  <div class="row"><span class="fold"></span><span class="close">{isArray ? "]" : "}"}{#if !last},{/if}</span></div>
{/if}

<style>
  .row { display: flex; align-items: baseline; gap: 0; font: 13px/1.65 var(--mono); white-space: nowrap; }
  .kids { padding-left: 16px; border-left: 1px solid var(--line); margin-left: 6px; }
  .fold { width: 14px; flex: none; color: var(--muted); display: inline-grid; place-items: center; }
  .arrow { display: inline-block; transition: transform 0.1s var(--ease); }
  .arrow.opened { transform: rotate(90deg); }
  .key { color: var(--text); padding: 0; }
  .key.idx { color: var(--muted); }
  .key:hover { text-decoration: underline; text-decoration-color: var(--line-strong); text-underline-offset: 3px; }
  .colon, .close { color: var(--muted); margin-right: 6px; }
  .close { margin: 0; }
  .open { color: var(--muted); padding: 0; }
  .size { color: var(--text-3); font-family: var(--font); font-size: 12px; margin: 0 6px; }
  .v { white-space: pre-wrap; overflow-wrap: anywhere; }
  .v.string { color: var(--green); }
  .v.number { color: var(--accent); }
  .v.boolean { color: var(--amber); }
  .v.null { color: var(--muted); font-style: italic; }
  .more { margin: 2px 0 2px 14px; color: var(--muted); font-size: 12px; }
  .more:hover { color: var(--text); }
</style>
