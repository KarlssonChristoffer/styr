<!-- Read receipts on a message you sent, as on a phone: one tick as soon as Hub has it (the engine may still be starting; the status line says so), two when the agent started on it. The tooltip has the times. Colour only for state.
     Used by Transcript.svelte. -->
<script lang="ts">
  // Read receipts on a message you sent, as on a phone: one tick as soon as Hub has it (the
  // engine may still be starting; the status line says so), two when the agent started on it.
  // The tooltip has the times. Colour only for state.
  import type { QueuedItem } from "../transcript";

  let { sent, receipt }: { sent: number; receipt: NonNullable<QueuedItem["receipt"]> } = $props();

  const full = (ms: number) => new Date(ms).toLocaleString([], { dateStyle: "medium", timeStyle: "medium" });
  const time = (ms: number) => new Date(ms).toLocaleTimeString([], { timeStyle: "medium" });
  const title = $derived(
    [
      `Sent ${full(sent)}`,
      `Received ${time(receipt.got)}`,
      receipt.taken && `Picked up by the engine ${time(receipt.taken)}`,
      receipt.seen && `Read ${time(receipt.seen)}`,
      receipt.done && `${receipt.outcome === "failed" ? "Failed" : receipt.outcome === "stopped" ? "Stopped" : "Answered"} ${time(receipt.done)}`,
    ]
      .filter(Boolean)
      .join("\n"),
  );
  const state = $derived(receipt.outcome ?? (receipt.seen ? "read" : receipt.taken ? "taken" : "got"));
  const word: Record<string, string> = { stopped: "Stopped", failed: "Failed" };
</script>

<span class="receipt {state}" data-tip={title} aria-label={receipt.seen ? "Read" : receipt.taken ? "Picked up" : "Received"}>
  <svg width="16" height="11" viewBox="0 0 16 11" aria-hidden="true">
    <path d="M1 6l3 3 6-7" />
    {#if receipt.seen}<path d="M7.5 8l1 1 6-7" />{/if}
  </svg>
  <!-- Ticks say received and read; words only when it did not go to plan. The tooltip has the times. -->
  {#if receipt.outcome === "stopped" || receipt.outcome === "failed"}<span>{word[state]}</span>{/if}
</span>

<style>
  .receipt { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); }
  svg { fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .read { color: var(--accent); }
  .failed { color: var(--red); }
</style>
