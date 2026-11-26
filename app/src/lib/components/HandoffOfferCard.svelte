<!-- The agent offered to hand the thread's work to one of your tools (`offer_handoff`), because you asked for it in words. Pressing the button is the approval: the core drafts the brief, answers what the tool asks and hands it over in the background (`own_tools::hand_off_now`); the `handed_off` that follows ends the wait. "Edit first" opens the thread menu's dialog instead.
     Used by Transcript.svelte. -->
<script lang="ts">
  // The agent offered to hand the thread's work to one of your tools (`offer_handoff`), because you
  // asked for it in words. Pressing the button is the approval: the core drafts the brief, answers
  // what the tool asks and hands it over in the background (`own_tools::hand_off_now`); the
  // `handed_off` that follows ends the wait. "Edit first" opens the thread menu's dialog instead.
  import type { HandoffOfferItem } from "../transcript";
  import { api } from "../api";
  import { handing, takers, type Taker } from "../ownTools.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { item, threadId }: { item: HandoffOfferItem; threadId: string } = $props();

  // When this view pressed it; a `handed_off` after that ends the wait either way.
  let pressed = $state<number | null>(null);
  const waiting = $derived(pressed != null && !item.done && !(item.failedAt != null && item.failedAt >= pressed));

  const taker = $derived<Taker>(
    takers.list.find((t) => t.folder === item.folder) ?? {
      host: null,
      folder: item.folder,
      name: item.tool,
      label: item.label,
      about: item.about,
      canBuild: item.canBuild,
    },
  );

  async function go() {
    pressed = Date.now() - 5_000;
    try {
      await api.handOffNow(threadId, item.folder);
    } catch (e) {
      pressed = null;
      hub.fail(e);
    }
  }
</script>

<div class="offer" class:done={item.done}>
  <span class="ico"><Icon name="share" size={14} /></span>
  <div class="body">
    <div class="t">{item.tool}</div>
    {#if waiting}
      <div class="why">Writing the brief and handing it over…</div>
    {:else if item.failedAt != null && !item.done}
      <div class="why bad">Didn't go over; the reason is below.</div>
    {:else if item.why}
      <div class="why">{item.why}</div>
    {/if}
  </div>
  {#if item.done}
    <span class="sent"><Icon name="check" size={12} />Handed over</span>
  {:else if waiting}
    <span class="sent">Working…</span>
  {:else}
    <button class="link quiet" onclick={() => (handing.to = { thread: threadId, taker })}>Edit first</button>
    <button class="btn primary" data-tip={item.about ?? undefined} onclick={go}>{item.failedAt != null ? "Try again" : item.label}</button>
  {/if}
</div>

<style>
  .offer { display: flex; gap: 11px; align-items: center; margin: 6px 0 10px; border: 1px solid var(--line); background: var(--surface); border-radius: var(--r-lg); padding: 8px 11px; }
  .ico { width: 28px; height: 28px; flex: none; border-radius: var(--r-md); display: grid; place-items: center; background: var(--accent-bg); color: var(--accent); }
  .body { flex: 1; min-width: 0; }
  .t { color: var(--text); font-weight: 500; font-size: 14px; }
  .why { color: var(--text-3); font-size: 13px; margin-top: 1px; line-height: 1.5; }
  .why.bad { color: var(--red); }
  .btn { height: 26px; padding: 0 11px; font-size: 13px; flex: none; }
  .quiet { color: var(--muted); font-size: 12px; flex: none; }
  .sent { display: inline-flex; gap: 4px; align-items: center; color: var(--muted); font-size: 12px; flex: none; }
  .done { opacity: 0.7; }
</style>
