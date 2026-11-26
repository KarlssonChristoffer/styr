<!-- The bag's drop zone, anywhere in Hub: while something is dragged over the window (files, a picture or link from a browser, text, or a thread from the list) a small zone shows near the top, and dropping on it puts the thing in the context bag. The bag does not need to be open.
     Used by App.svelte. -->
<script lang="ts">
  // The bag's drop zone, anywhere in Hub: while something is dragged over the window (files, a
  // picture or link from a browser, text, or a thread from the list) a small zone shows near the
  // top, and dropping on it puts the thing in the context bag. The bag does not need to be open.
  import { bagDrop, bagTakes, here, putDropped } from "../bag.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  // dragenter and dragleave fire for every element crossed; count them to know when the drag
  // has left the window.
  let depth = 0;
  let outside = $state(false);
  let hide: ReturnType<typeof setTimeout> | undefined;

  const from = () => here(hub.view === "thread" ? hub.selected : null);

  function enter(e: DragEvent) {
    if (!bagTakes(e.dataTransfer)) return;
    depth++;
    clearTimeout(hide);
    outside = true;
  }
  function leave() {
    if (!outside) return;
    depth = Math.max(0, depth - 1);
    // A leave is followed at once by the next element's enter; wait a moment before hiding.
    if (depth === 0) hide = setTimeout(() => (outside = false), 60);
  }
  function end() {
    depth = 0;
    outside = false;
    bagDrop.over = false;
  }

  const show = $derived(outside || !!bagDrop.thread);
</script>

<svelte:window ondragenter={enter} ondragleave={leave} ondropcapture={end} ondragend={end} />

{#if show}
  <div role="presentation"
    class="zone"
    class:over={bagDrop.over}
    data-bag-zone
    ondragover={(e) => {
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
      bagDrop.over = true;
    }}
    ondragleave={() => (bagDrop.over = false)}
    ondrop={(e) => {
      e.preventDefault();
      e.stopPropagation();
      const dt = e.dataTransfer;
      end();
      if (dt) putDropped(dt, from());
    }}
  >
    <Icon name="bag" size={15} />
    <span>{bagDrop.thread ? "Drop to put the thread in the bag" : "Drop here to put it in the bag"}</span>
  </div>
{/if}

<style>
  .zone {
    position: fixed; left: 50%; top: 52px; transform: translateX(-50%); z-index: 900;
    display: flex; align-items: center; gap: 8px; padding: 14px 22px; border-radius: var(--r-lg);
    border: 1px dashed var(--line-strong); background: var(--surface-2); color: var(--text-2);
    font-size: 13px; box-shadow: 0 8px 28px rgb(0 0 0 / 0.35);
  }
  .zone.over { border-color: var(--accent); color: var(--text); }
</style>
