<!-- För: agent. The chip by Send that offers to summarise a long, cold thread first (Composer). -->
<script lang="ts">
  import { tokens } from "../session";

  let { used, keepFull = $bindable(false) }: { used: number; keepFull?: boolean } = $props();
</script>

<button
  class="compact-chip"
  class:off={keepFull}
  data-tip={keepFull
    ? "Sends with the whole conversation, read again from scratch. Click to summarise it first."
    : "This thread has been quiet over an hour, so the engine reads it all again. Sending summarises it first (/compact). Click to keep it all."}
  onclick={() => (keepFull = !keepFull)}
>{keepFull ? "Full" : "Compact"} · {tokens(used)}</button>

<style>
  .compact-chip { font-size: 12px; color: var(--accent); background: var(--accent-bg); border-radius: var(--r-sm); padding: 3px 8px; margin-right: 4px; white-space: nowrap; }
  .compact-chip.off { color: var(--muted); background: var(--surface-hover); }
</style>
