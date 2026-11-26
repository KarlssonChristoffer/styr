<!-- The small window of its own the shell opens while it installs a new version of Hub (`card` in `app/src-tauri/src/main.rs`), with Hub's windows hidden: just the card, on the desktop. It talks to no core, since the core is the thing restarting; the shell closes it when Hub is back. It can be moved by any part of it, and nothing else: no maximize on a double click.
     Used by boot.ts. -->
<script lang="ts">
  import { quiet } from "../quiet";
  // The small window of its own the shell opens while it installs a new version of Hub (`card` in
  // `app/src-tauri/src/main.rs`), with Hub's windows hidden: just the card, on the desktop. It
  // talks to no core, since the core is the thing restarting; the shell closes it when Hub is back.
  // It can be moved by any part of it, and nothing else: no maximize on a double click.
  import InstallCard from "./InstallCard.svelte";

  async function move(e: MouseEvent) {
    if (e.button !== 0) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await quiet(getCurrentWindow().startDragging(), "get current window");
  }
</script>

<div role="presentation" class="window" onmousedown={move}><InstallCard alone /></div>

<style>
  :global(html), :global(body) { background: var(--surface-2); overflow: hidden; }
  .window { height: 100vh; cursor: default; user-select: none; }
</style>
