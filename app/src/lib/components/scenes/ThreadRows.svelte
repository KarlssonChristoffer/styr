<!-- A list of threads, one line each: its ring, its title, where it lives and how long ago it moved. A click opens the thread; `open` adds a button for it on the right.
     Used by scenes/SceneLive.svelte, scenes/SceneNeeds.svelte, scenes/SceneProjects.svelte. -->
<script lang="ts">
  import { poll } from "../../poll";
  // A list of threads, one line each: its ring, its title, where it lives and how long ago it
  // moved. A click opens the thread; `open` adds a button for it on the right.
  import type { Thread } from "../../bindings/Thread";
  import { ago } from "../../format";
  import { menu } from "../../menu.svelte";
  import { threadMenu } from "../../menus.svelte";
  import { hub, unread } from "../../state.svelte";
  import ThreadMeta from "../ThreadMeta.svelte";

  let { threads, empty = "", open = "", clock = false }: { threads: Thread[]; empty?: string; open?: string; clock?: boolean } = $props();

  let now = $state(Date.now());
  $effect(() => {
    if (!clock) return;
    const t = poll(() => (now = Date.now()), 1000);
    return () => t();
  });
  /** A running thread's clock, from when it was sent work. */
  function running(t: Thread): string {
    const s = Math.max(0, Math.floor((now - (t.sent_at ?? t.created_at)) / 1000));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    return h ? `${h} h ${String(m).padStart(2, "0")}` : s >= 60 ? `${m} min` : `${s} s`;
  }
</script>

{#if !threads.length}
  <p class="none">{empty}</p>
{:else}
  {#each threads as t (t.id)}
    <div class="row" role="presentation" use:menu={() => threadMenu(t)}>
      <button class="main" onclick={() => hub.open(t.id)}>
        <span class="dot {t.status}" class:unread={unread(t)}></span>
        <span class="tt t grow">{t.title}</span>
        <span class="where"><ThreadMeta {t} when={false} /></span>
        <span class="rt" class:gr={clock && t.status === "running"}>{clock && t.status === "running" ? running(t) : ago(t.returned_at ?? t.updated_at)}</span>
      </button>
      {#if open}<button class="b go" onclick={() => hub.open(t.id)}>{open}</button>{/if}
    </div>
  {/each}
{/if}

<style>
  .row { display: flex; align-items: center; gap: 8px; min-height: 28px; }
  .main { display: grid; grid-template-columns: 14px minmax(0, 1fr) auto 46px; align-items: center; column-gap: 8px; flex: 1; min-width: 0; padding: 4px 0; text-align: left; }
  .tt { color: var(--text); font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .main:hover .tt { color: var(--accent); }
  /* A narrow block drops where a thread lives and keeps its title and time. */
  @container blk (max-width: 28em) { .where { display: none; } .main { grid-template-columns: 14px minmax(0, 1fr) 46px; } }
  .rt { color: var(--muted); font-size: 12px; text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .rt.gr { color: var(--green); }
  .none { margin: 0; padding: 6px 0; color: var(--muted); font-size: 12px; }
</style>
