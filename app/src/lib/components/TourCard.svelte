<!-- Show me from the Updates page took you to a page: this card names the change you came to see, in the corner, until you close it or go back to the updates.
     Used by App.svelte. -->
<script lang="ts">
  import { shortVersion } from "../format";
  // Show me from the Updates page took you to a page: this card names the change you came to see,
  // in the corner, until you close it or go back to the updates.
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Markdown from "./Markdown.svelte";
</script>

{#if hub.tour}
  {@const t = hub.tour}
  <div class="tour" role="status">
    <div class="head">
      <span class="label">New in <span class="mono">{shortVersion(t.version)}</span></span>
      <button class="close" aria-label="Close" onclick={() => (hub.tour = null)}><Icon name="x" size={12} /></button>
    </div>
    <div class="text"><Markdown text={t.text} /></div>
    <button class="back" onclick={() => { hub.tour = null; hub.openSettings("updates"); }}>Back to Updates</button>
  </div>
{/if}

<style>
  .tour {
    position: absolute; right: 20px; top: calc(var(--controls-h) + 12px); z-index: 30; width: min(320px, calc(100% - 40px));
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md);
    padding: 10px 12px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35); font-size: 13px;
  }
  .head { display: flex; align-items: center; justify-content: space-between; }
  .label { color: var(--muted); font-size: 12px; }
  .text { color: var(--text); margin: 4px 0 8px; }
  .text :global(p) { margin: 0; }
  .close { width: 22px; height: 22px; display: grid; place-items: center; border-radius: var(--r-sm); color: var(--muted); }
  .close:hover { color: var(--text); background: var(--surface-hover); }
  .back { color: var(--accent); font-size: 13px; padding: 0; }
  .back:hover { text-decoration: underline; }
</style>
