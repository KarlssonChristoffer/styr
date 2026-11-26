<!-- One element in Inspect's tree, and its children when it is open. The tree reads the page itself (it is the page's own DOM), so `inspect.rev` is what makes it look again.
     Used by Inspector.svelte. -->
<script lang="ts">
  // One element in Inspect's tree, and its children when it is open. The tree reads the page
  // itself (it is the page's own DOM), so `inspect.rev` is what makes it look again.
  import { inspect } from "../inspect.svelte";
  import { inUI, nameOf, shortOf } from "../inspectInfo";
  import Self from "./InspectTree.svelte";

  let { el, depth = 0, opened }: { el: Element; depth?: number; opened: Set<Element> } = $props();

  const LEAF = new Set(["svg", "path", "script", "style", "canvas", "img", "input", "textarea", "br", "hr", "iframe", "video"]);
  const MORE = 80;

  const kids = $derived.by(() => {
    void inspect.rev;
    if (LEAF.has(el.tagName.toLowerCase())) return [] as Element[];
    return [...el.children].filter((c) => !inUI(c) && !["SCRIPT", "STYLE", "LINK"].includes(c.tagName));
  });
  const focus = $derived(inspect.focus);
  const ancestorOfFocus = $derived.by(() => {
    void inspect.rev;
    return !!focus && el !== focus && el.contains(focus);
  });
  const open = $derived((void inspect.rev, opened.has(el)) || ancestorOfFocus);
  const marked = $derived(inspect.markOf(el));
  const name = $derived(el.getAttribute("data-c") ? nameOf(el.getAttribute("data-c")!) : "");
  const text = $derived(kids.length === 0 ? (el.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 40) : "");
  let row = $state<HTMLElement>();

  $effect(() => {
    // The row of the element being looked at scrolls into view.
    if (focus === el && inspect.hover === null) row?.scrollIntoView({ block: "nearest", inline: "start" });
  });

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    if (open && !ancestorOfFocus) opened.delete(el);
    else opened.add(el);
    inspect.rev++;
  }
</script>

<div class="node">
  <div bind:this={row} class="row" class:focus={focus === el} class:marked={!!marked} style:padding-left="{6 + depth * 12}px"
    role="treeitem" aria-selected={focus === el} tabindex="-1"
    onmouseenter={() => (inspect.hover = el)} onmouseleave={() => (inspect.hover = null)}
    onclick={() => inspect.pick(el)} onkeydown={(e) => e.key === "Enter" && inspect.pick(el)}>
    {#if kids.length}
      <button class="caret" class:open aria-label={open ? "Close" : "Open"} onclick={toggle}>▸</button>
    {:else}<span class="caret"></span>{/if}
    <span class="tag">&lt;{shortOf(el)}&gt;</span>
    {#if name}<span class="comp">{name}</span>{/if}
    {#if text}<span class="text">{text}</span>{/if}
    {#if marked}<span class="num">{inspect.tag(marked)}</span>{/if}
  </div>
  {#if open}
    {#each kids.slice(0, MORE) as k (k)}
      <Self el={k} depth={depth + 1} {opened} />
    {/each}
    {#if kids.length > MORE}<div class="more" style:padding-left="{18 + (depth + 1) * 12}px">{kids.length - MORE} more…</div>{/if}
  {/if}
</div>

<style>
  .row { display: flex; align-items: center; gap: 4px; height: 22px; padding-right: 8px; font: 12px/1 var(--mono); color: var(--text-3); white-space: nowrap; cursor: default; border-radius: var(--r-xs); }
  .row:hover, .row.focus { background: var(--accent-bg); color: var(--text); }
  .row.marked { box-shadow: inset 2px 0 0 var(--accent); }
  .caret { flex: none; width: 14px; height: 14px; color: var(--muted); font-size: 10px; line-height: 14px; text-align: center; transition: transform 0.1s var(--ease); }
  .caret.open { transform: rotate(90deg); }
  button.caret:hover { color: var(--text); }
  .tag { overflow: hidden; text-overflow: ellipsis; }
  .comp { color: var(--accent); font-family: var(--font); font-size: 12px; }
  .text { color: var(--muted); overflow: hidden; text-overflow: ellipsis; font-family: var(--font); }
  .num { margin-left: auto; flex: none; min-width: 16px; height: 16px; padding: 0 4px; box-sizing: border-box; border-radius: 999px; background: var(--accent); color: var(--bg); font: 700 11px/16px var(--font); text-align: center; }
  .more { color: var(--muted); font: 12px/22px var(--font); }
</style>
