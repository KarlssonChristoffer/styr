<!-- Inspect mode's panel, in the place of the thread's side column: the page's element tree, what is marked with a note each, a note for each group, and the button that starts a thread from it.
     Used by App.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // Inspect mode's panel, in the place of the thread's side column: the page's element tree, what
  // is marked with a note each, a note for each group, and the button that starts a thread from it.
  import { inspect } from "../inspect.svelte";
  import { shortOf } from "../inspectInfo";
  import Icon from "./Icon.svelte";
  import InspectTree from "./InspectTree.svelte";

  const opened = new Set<Element>([document.body]);
  const letters = $derived([...new Set(inspect.marks.map((m) => m.group).filter((g): g is string => !!g))]);
  const count = $derived(inspect.marks.length);
</script>

<aside class="inspector" data-inspect-ui aria-label="Inspect">
  <header>
    <b>Inspect</b>
    <span class="hint">Click an element to comment · Shift-click to group</span>
    <button class="x" class:on={inspect.magnify} aria-label="Magnify (Z)" title="Magnify (Z)" onclick={() => (inspect.magnify = !inspect.magnify)}><Icon name="search" size={14} /></button>
    <button class="x" aria-label="Leave Inspect" onclick={() => inspect.stop()}><Icon name="x" size={14} /></button>
  </header>
  <div class="tree" role="tree">
    <InspectTree el={document.body} {opened} />
  </div>
  <section class="marked">
    <h5>Marked {count ? `· ${count}` : ""}</h5>
    {#if !count}
      <Quiet text="Nothing marked" />
    {/if}
    {#each inspect.marks as m (m.id)}
      <div class="item" class:open={inspect.editing === m.id}>
        <button class="num" onclick={() => (inspect.editing = m.id)}>{inspect.tag(m)}</button>
        <div class="body">
          <div class="what">{m.info.name || m.info.short}<span>{shortOf(m.el)}</span></div>
          <input value={m.note} placeholder="Note" onfocus={() => (inspect.editing = m.id)} oninput={(e) => inspect.setNote(m.id, e.currentTarget.value)} />
        </div>
        <button class="rm" aria-label="Remove" onclick={() => inspect.remove(m.id)}><Icon name="x" size={12} /></button>
      </div>
    {/each}
    {#each letters as g (g)}
      <div class="item group">
        <span class="num">{g}</span>
        <div class="body">
          <div class="what">Group {g}<span>{inspect.marks.filter((m) => m.group === g).length} elements</span></div>
          <input bind:value={inspect.groups[g]} placeholder="What is wrong with these?" />
        </div>
      </div>
    {/each}
  </section>
  <footer>
    <button class="ghost" disabled={!count} onclick={() => inspect.clear()}>Clear</button>
    <span class="sp"></span>
    <button class="go" disabled={!count} onclick={() => (inspect.sending = true)}>Open thread</button>
  </footer>
</aside>

<style>
  .inspector { width: var(--col-w, calc(clamp(340px, 30vw, 460px) * var(--text-grow, 1))); flex: none; min-height: 0; display: flex; flex-direction: column; background: var(--bg-sidebar); border-left: 1px solid var(--line); }
  header { display: flex; align-items: center; gap: 10px; padding: 10px 12px; padding-top: calc(10px + var(--controls-h, 0px)); }
  header b { font-size: 14px; font-weight: 600; }
  .hint { flex: 1; min-width: 0; color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .x { color: var(--muted); padding: 3px; border-radius: var(--r-sm); }
  .x:hover { color: var(--text); background: var(--surface-hover); }
  .x.on { color: var(--accent); background: var(--accent-bg); }
  .tree { flex: 1 1 55%; min-height: 120px; overflow: auto; padding: 4px 6px; border-top: 1px solid var(--line); }
  .marked { flex: 0 1 auto; max-height: 45%; overflow: auto; padding: 10px 12px; border-top: 1px solid var(--line); }
  h5 { margin: 0 0 6px; font-size: 12px; font-weight: 500; color: var(--muted); }
  .item { display: flex; gap: 8px; align-items: flex-start; padding: 6px 0; }
  .item + .item { border-top: 1px solid var(--line); }
  .num { flex: none; min-width: 18px; height: 18px; padding: 0 4px; box-sizing: border-box; border-radius: 999px; background: var(--accent); color: var(--bg); font: 700 11px/18px var(--font); text-align: center; }
  .body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .what { font-size: 13px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .what span { margin-left: 8px; color: var(--muted); font: 12px var(--mono); }
  input { background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 4px 8px; color: var(--text); font: 13px var(--font); outline: none; min-width: 0; }
  input:focus { border-color: var(--accent); }
  .rm { color: var(--muted); padding: 3px; border-radius: var(--r-sm); }
  .rm:hover { color: var(--text); background: var(--surface-hover); }
  footer { display: flex; align-items: center; gap: 6px; padding: 10px 12px; border-top: 1px solid var(--line); }
  .sp { flex: 1; }
  .ghost { color: var(--text-3); padding: 4px 10px; border-radius: var(--r-md); }
  .ghost:hover:not(:disabled) { background: var(--surface-hover); color: var(--text); }
  .go { background: var(--accent); color: var(--bg); border-radius: var(--r-md); padding: 4px 12px; font-weight: 600; }
  button:disabled { opacity: 0.4; cursor: default; }
</style>
