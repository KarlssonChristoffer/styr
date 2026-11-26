<!-- Minimise, maximise and close, drawn by Hub since the window has no Windows frame.
     Used by App.svelte. -->
<script lang="ts">
  // Minimise, maximise and close, drawn by Hub since the window has no Windows frame.
  import { onMount } from "svelte";
  import type { Window } from "@tauri-apps/api/window";

  let win = $state<Window>();
  let maximized = $state(false);

  onMount(() => {
    let stop: (() => void) | undefined;
    import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      const w = getCurrentWindow();
      win = w;
      maximized = await w.isMaximized();
      stop = await w.onResized(async () => (maximized = await w.isMaximized()));
    });
    return () => stop?.();
  });
</script>

<div class="controls">
  <button aria-label="Minimise" data-tip="Minimise" onclick={() => win?.minimize()}>
    <svg viewBox="0 0 10 10"><path d="M0 5h10" /></svg>
  </button>
  <button aria-label={maximized ? "Restore" : "Maximise"} data-tip={maximized ? "Restore" : "Maximise"} onclick={() => win?.toggleMaximize()}>
    {#if maximized}
      <svg viewBox="0 0 10 10"><path d="M.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" /></svg>
    {:else}
      <svg viewBox="0 0 10 10"><path d="M.5 .5h9v9h-9z" /></svg>
    {/if}
  </button>
  <button class="close" aria-label="Close" data-tip="Close" onclick={() => win?.close()}>
    <svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0 0 10" /></svg>
  </button>
</div>

<style>
  /* Sized in screen pixels (see --controls-w in app.css), so zooming Hub leaves them alone. */
  .controls { position: fixed; top: 0; right: 0; z-index: 60; display: flex; height: var(--controls-h); width: var(--controls-w); }
  button { flex: 1; display: grid; place-items: center; color: var(--text-3); }
  button:hover { background: var(--surface-hover); color: var(--text); }
  button.close:hover { background: #c42b1c; color: #fff; }
  svg { width: calc(10px / var(--zoom, 1)); height: calc(10px / var(--zoom, 1)); fill: none; stroke: currentColor; stroke-width: 1; shape-rendering: crispEdges; }
</style>
