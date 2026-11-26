<!-- What the window shows while Hub installs an update, when the shell cannot hide it: the card (`InstallCard.svelte`) on a plain dark page, not the app dimmed behind it, so it reads as Hub restarting, which it is. A newer shell hides Hub's windows and shows the card as a small window of its own (`InstallWindow.svelte`), so this one is never seen there. The window reloads by itself once the new version is up (`relaunch` in the shell), and this goes with the old page.
     Used by App.svelte. -->
<script lang="ts">
  // What the window shows while Hub installs an update, when the shell cannot hide it: the card
  // (`InstallCard.svelte`) on a plain dark page, not the app dimmed behind it, so it reads as Hub
  // restarting, which it is. A newer shell hides Hub's windows and shows the card as a small window
  // of its own (`InstallWindow.svelte`), so this one is never seen there. The window reloads by
  // itself once the new version is up (`relaunch` in the shell), and this goes with the old page.
  import { ownTitleBar } from "../native";
  import { hub } from "../state.svelte";
  import InstallCard from "./InstallCard.svelte";
</script>

<div class="install" class:below={ownTitleBar}>
  <InstallCard target={hub.update?.target ?? ""} />
</div>

<style>
  .install { position: fixed; inset: 0; z-index: 10000; display: grid; place-items: center; background: var(--bg); }
  .install.below { top: var(--caption-h); }
</style>
