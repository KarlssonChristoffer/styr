<!-- The To test scripts in a window of their own, for a second screen. It follows the file, and what it starts or opens (a thread's prompt, Settings, a page) happens in the main window.
     Used by boot.ts. -->
<script lang="ts">
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  import { applyTheme } from "../themes";
  // The To test scripts in a window of their own, for a second screen. It follows the file, and
  // what it starts or opens (a thread's prompt, Settings, a page) happens in the main window.
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { Focus } from "../bindings/Focus";
  import { prefs } from "../prefs.svelte";
  import { hub } from "../state.svelte";
  import { installZoom } from "../zoom";
  import ContextMenu from "./ContextMenu.svelte";
  import TestTab from "./TestTab.svelte";

  let { path, host = null }: { path: string; host?: string | null } = $props();
  hub.popped = true;
  let focus = $state<Focus | null>(null);
  let failed = $state("");

  async function load() {
    try {
      focus = (await api.project(path, host)).focus;
      failed = "";
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    }
  }
  onMount(() => {
    const stopHub = hub.start();
    const stopZoom = installZoom();
    document.title = "Hub · To test";
    void load();
    // A tick made in the main window, or a line an agent added, shows here within seconds.
    const timer = poll(load, 5000);
    return () => {
      timer();
      stopHub();
      stopZoom();
    };
  });
  $effect(() => {
    applyTheme(prefs.theme);
  });
</script>

<div class="window">
  {#if focus}
    <TestTab {path} {host} {focus} window onfocus={(f) => (focus = f)} onfill={(t) => hub.startDraft(path, t, host)} onprompt={(t) => hub.startDraft(path, t, host)} />
  {:else}
    {#if failed}<p class="none">{failed}</p>{:else}<Quiet kind="loading" />{/if}
  {/if}
  {#if !hub.connected}<div class="banner">The core is not running. Hub reconnects on its own.</div>{/if}
</div>
<ContextMenu />

<style>
  .window { height: 100%; overflow-y: auto; background: var(--bg); position: relative; }
  .none { padding: 18px; color: var(--muted); font-size: 13px; }
  .banner { position: absolute; left: 12px; right: 12px; bottom: 12px; padding: 8px 12px; border-radius: var(--r-md); background: var(--amber-bg); color: var(--amber); font-size: 13px; }
</style>
