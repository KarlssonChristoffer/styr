<!-- The tabs on the top edge of the box: a thread Ctrl Enter started in the background, and the stash of
     prompts put away with Ctrl S. Used by Composer.svelte. -->
<script lang="ts">
  import { stash } from "../stash.svelte";
  import type { StashList } from "../composerHistory.svelte";
  import Icon from "./Icon.svelte";

  let {
    started = null,
    onopenstarted,
    stashList,
  }: {
    /** A thread Ctrl Enter just started in the background: "Open" goes there. */
    started?: { id: string; title: string } | null;
    onopenstarted?: () => void;
    stashList: StashList;
  } = $props();
</script>

{#if started || (stash.list.length && !stashList.open)}
  <div class="tabs">
    {#if started}
      <span class="tab started" role="status">
        <span class="dot"></span><span>Started in the background</span>
        <button class="open-it" data-tip={started.title} onpointerdown={(e) => e.preventDefault()} onclick={onopenstarted}>Open</button>
      </span>
    {/if}
    {#if stash.list.length && !stashList.open}
      <button class="tab" data-tip="What you put away with Ctrl S" onpointerdown={(e) => e.preventDefault()} onclick={() => stashList.show()}>
        <Icon name="bookmark" size={13} /><span>Stash</span><span class="n">{stash.list.length}</span>
      </button>
    {/if}
  </div>
{/if}

<style>
  .tabs { position: absolute; right: 16px; bottom: 100%; margin-bottom: -1px; display: flex; gap: 6px; z-index: 4; }
  .tab {
    display: flex; align-items: center; gap: 8px; padding: 4px 11px 6px; font-size: 13px; color: var(--muted);
    background: var(--surface); border: 1px solid var(--line-strong); border-bottom: none; border-radius: var(--r-md) var(--r-md) 0 0; white-space: nowrap;
  }
  button.tab { cursor: pointer; }
  button.tab:hover { color: var(--text); }
  .tab .n { color: var(--text-3); font-variant-numeric: tabular-nums; }
  .tab .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--green); }
  .tab .open-it { color: var(--accent); }
  .tab .open-it:hover { text-decoration: underline; }
</style>
