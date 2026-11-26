<!-- The card that says Hub is installing a new version: the working grid (`WorkGrid.svelte`, the same one an agent shows while it works) beside the title, and a line on what happens. The window shows it on a plain dark page (`InstallBox.svelte`); a shell that knows how shows it as a small window of its own on the desktop instead, with Hub's windows hidden (`InstallWindow`).
     Used by InstallBox.svelte, InstallWindow.svelte. -->
<script lang="ts">
  // The card that says Hub is installing a new version: the working grid (`WorkGrid.svelte`, the
  // same one an agent shows while it works) beside the title, and a line on what happens. The
  // window shows it on a plain dark page (`InstallBox.svelte`); a shell that knows how shows it as
  // a small window of its own on the desktop instead, with Hub's windows hidden (`InstallWindow`).
  import { prefs } from "../prefs.svelte";
  import { workShape } from "../workGrid";
  import WorkGrid from "./WorkGrid.svelte";
  import { poll } from "../poll";

  let { target = "", alone = false }: { target?: string; alone?: boolean } = $props();

  let seconds = $state(0);
  // Counted from the start, so a stretch with the window hidden (when `poll` rests) still counts.
  const since = Date.now();
  $effect(() => poll(() => (seconds = Math.floor((Date.now() - since) / 1000)), 1000));
</script>

<div class="card" class:alone role="status" aria-live="polite">
  <div class="row"><WorkGrid shape={workShape("install", prefs.workIndicator)} size={26} /><span class="title">Installing a new version of Hub</span></div>
  <div class="sub">
    {#if seconds < 60}Threads keep working. This takes a few seconds, and Hub comes back by itself.{:else}This takes longer than usual. Hub comes back by itself the moment the new version is up{#if target}: {target.slice(0, 7)}{/if}.{/if}
  </div>
</div>

<style>
  .card {
    width: 340px; padding: 20px 22px; background: var(--surface-2); border: 1px solid var(--line-strong);
    border-radius: var(--r-lg); box-shadow: 0 18px 48px rgba(0, 0, 0, 0.5); color: var(--text);
  }
  /* Its own window: the window is the card, so no frame or shadow of its own. */
  .card.alone { width: auto; height: 100vh; border: 0; border-radius: 0; box-shadow: none; box-sizing: border-box; }
  .row { display: flex; align-items: center; gap: 12px; }
  .title { font-size: 15px; font-weight: 500; }
  .sub { margin-top: 10px; font-size: 13px; line-height: 1.5; color: var(--muted); }
</style>
