<!-- The sidebar's update row: a new version of Hub is built and tested, Install or read what is new. It says only that; the patch notes, the other machines and how far an install got are on the Updates page. Install or × takes it away (× until the next version). It sits at the foot of the list column; the avatar's dot (hub.updateReady) stands in for it when the sidebar is folded.
     Used by Sidebar.svelte. -->
<script lang="ts">
  // The sidebar's update row: a new version of Hub is built and tested, Install or read what is new.
  // It says only that; the patch notes, the other machines and how far an install got are on the
  // Updates page. Install or × takes it away (× until the next version). It sits at the foot of the
  // list column; the avatar's dot (hub.updateReady) stands in for it when the sidebar is folded.
  import { setPref } from "../prefs.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  const target = $derived(hub.updateReady);
</script>

{#if target}
  <div class="badge" role="status" aria-label="New version of Hub ready">
    <div class="text">
      <div class="title">New version ready</div>
      <button class="news" data-use="update.news" onclick={() => hub.openSettings("updates")}>What's new</button>
    </div>
    <button class="install" data-use="update.install" onclick={() => hub.installUpdate()}>Install</button>
    <button class="close" aria-label="Not now" data-tip="Not now: hide until the next version" onclick={() => setPref("laterUpdate", target)}>
      <Icon name="x" size={12} />
    </button>
  </div>
{/if}

<style>
  .badge {
    display: flex; align-items: center; gap: 8px; margin: auto 4px 4px 0; padding: 8px 6px 8px 10px;
    border: 1px solid var(--line-strong); border-radius: var(--r-md); background: var(--surface);
  }
  .text { flex: 1; min-width: 0; line-height: 1.3; }
  .title { color: var(--text); font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .news { color: var(--muted); font-size: 12px; padding: 0; text-decoration: underline; text-decoration-color: var(--line-strong); text-underline-offset: 3px; }
  .news:hover { color: var(--text-2); }
  .install { flex: none; height: 26px; padding: 0 10px; border-radius: var(--r-md); background: var(--accent-bg); color: var(--accent); font-size: 13px; }
  .install:hover { filter: brightness(1.15); }
  .close { flex: none; width: 22px; height: 22px; display: grid; place-items: center; border-radius: var(--r-sm); color: var(--muted); }
  .close:hover { color: var(--text); background: var(--surface-hover); }
</style>
