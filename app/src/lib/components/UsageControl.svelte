<!-- The ring in the bar that shows how full the context is, and the plan limits it opens. Used by
     Composer.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  import { tokens, type Usage } from "../session";
  import UsagePanel from "./UsagePanel.svelte";

  let {
    usage,
    threadId,
    menu = $bindable(),
    oncompact,
  }: {
    usage?: Usage;
    threadId?: string;
    /** Which menu of the bar is open. */
    menu: string | null;
    /** Summarises the thread, when the engine can. */
    oncompact?: () => void;
  } = $props();

  const usedPct = $derived(usage && usage.size ? Math.min(100, (usage.used / usage.size) * 100) : 0);
</script>

<div class="menu-wrap">
  <button
    class="tool usage"
    class:warn={usedPct >= 80}
    class:full={usedPct >= 95}
    data-tip={usage ? `Context ${tokens(usage.used)} of ${tokens(usage.size)} tokens · plan limits` : "Usage and plan limits"}
    onclick={() => (menu = menu === "usage" ? null : "usage")}
  >
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="8" cy="8" r="6" fill="none" stroke="var(--line-strong)" stroke-width="2" />
      <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"
        stroke-dasharray={`${(usedPct / 100) * 37.7} 37.7`} transform="rotate(-90 8 8)" />
    </svg>
    {#if usage}<span>{Math.round(usedPct)}%</span>{#if usage.size}<span class="of">of {tokens(usage.size)}</span>{/if}{/if}
  </button>
  {#if menu === "usage"}
    <div class="menu right" transition:pop|global>
      <UsagePanel
        {threadId}
        current={usage}
        {oncompact}
      />
    </div>
  {/if}
</div>

<style>
  /* @kit menus */
  .usage span { font-variant-numeric: tabular-nums; font-size: 12px; }
  .usage.warn { color: var(--amber); }
  .usage.full { color: var(--red); }
  .usage .of { color: var(--muted); opacity: 0.75; }
</style>
