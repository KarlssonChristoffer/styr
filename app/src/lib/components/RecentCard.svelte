<!-- Recent threads: the ones you touched last, a ring for their state and where each lives. The same list Home used to show as its third tile, now a card you can place or leave out.
     Used by CardBody.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // Recent threads: the ones you touched last, a ring for their state and where each lives. The
  // same list Home used to show as its third tile, now a card you can place or leave out.
  import { hub, unread } from "../state.svelte";
  import { menu } from "../menu.svelte";
  import { threadMenu } from "../menus.svelte";
  import HowCounted from "./HowCounted.svelte";
  import ThreadMeta from "./ThreadMeta.svelte";

  const list = $derived(hub.active.slice(0, 7));
</script>

<HowCounted card="recent">
  <p>Your threads, latest first.</p>
</HowCounted>

{#if !list.length}
  <Quiet text="No threads" />
{:else}
  <div class="list">
    {#each list as t (t.id)}
      <button class="line" onclick={() => hub.open(t.id)} use:menu={() => threadMenu(t)}>
        <span class="dot {t.status}" class:unread={unread(t)}></span>
        <span class="t">{t.title}</span>
        <ThreadMeta {t} />
      </button>
    {/each}
  </div>
{/if}

<style>
  .list { display: flex; flex-direction: column; }
  .line { display: flex; align-items: center; gap: 10px; width: 100%; min-width: 0; height: 34px; text-align: left; padding: 0; border-bottom: 1px solid var(--line); }
  .line:last-child { border-bottom: 0; }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-size: 13px; color: var(--text-2); }
  .line:hover .t { color: var(--text); }
</style>
