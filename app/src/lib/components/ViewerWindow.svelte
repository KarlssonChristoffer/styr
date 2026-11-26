<!-- A viewer popped out of the main window into its own: just the viewer, following its thread live. What it starts (a thread, a draft for Home) happens in the main window.
     Used by boot.ts. -->
<script lang="ts">
  import { applyTheme } from "../themes";
  // A viewer popped out of the main window into its own: just the viewer, following its thread
  // live. What it starts (a thread, a draft for Home) happens in the main window.
  import { onMount, untrack } from "svelte";
  import { prefs } from "../prefs.svelte";
  import { hub, type Viewing } from "../state.svelte";
  import { installZoom } from "../zoom";
  import ContextMenu from "./ContextMenu.svelte";
  import Viewer from "./Viewer.svelte";

  let { viewing }: { viewing: Viewing } = $props();
  hub.popped = true;
  hub.viewing = untrack(() => viewing);

  onMount(() => {
    const stopHub = hub.start();
    hub.follow(viewing.thread);
    const stopZoom = installZoom();
    return () => {
      stopHub();
      stopZoom();
    };
  });

  $effect(() => {
    applyTheme(prefs.theme);
  });
</script>

<div class="window">
  {#if hub.viewing}<Viewer viewing={hub.viewing} popped />{/if}
  {#if !hub.connected}<div class="banner">The core is not running. Hub reconnects on its own.</div>{/if}
  {#if hub.error}
      <div class="toast">
        {hub.error}
        {#if hub.errorAction}
          {@const action = hub.errorAction}
          <button class="toast-action" onclick={() => { hub.error = null; hub.errorAction = null; action.run(); }}>{action.label}</button>
        {/if}
      </div>
    {/if}
</div>
<ContextMenu />

<style>
  .window { height: 100%; display: flex; flex-direction: column; position: relative; background: var(--bg); }
  .banner { position: absolute; left: 12px; right: 12px; bottom: 12px; padding: 8px 12px; border-radius: var(--r-md); background: var(--amber-bg); color: var(--amber); font-size: 13px; }
  .toast { position: absolute; left: 50%; bottom: 16px; transform: translateX(-50%); padding: 8px 14px; border-radius: var(--r-md); background: var(--red-bg); color: var(--red); font-size: 13px; }
  .toast-action { margin-left: 10px; padding: 2px 8px; border-radius: var(--r-sm); background: var(--surface-hover); color: var(--text); font-size: 13px; }
  .toast-action:hover { background: var(--line-strong); }
</style>
