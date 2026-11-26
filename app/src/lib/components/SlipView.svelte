<!-- What Hub handed an AI as one quiet line (`package/`, "What Hub sends to an AI"): how big the package was, and, where the thread says it, what went along and what did not. Shown under a model switch in the chat (`HandoverHint.svelte`) and for a row of Settings > Where it goes, which has the slip but not the thread. There is no "show text": the line says what went along and what did not, and that is all it needs to.
     Used by HandoverHint.svelte. -->
<script lang="ts">
  // What Hub handed an AI as one quiet line (`package/`, "What Hub sends to an AI"): how big the
  // package was, and, where the thread says it, what went along and what did not. Shown under a
  // model switch in the chat (`HandoverHint.svelte`) and for a row of Settings > Where it goes,
  // which has the slip but not the thread. There is no "show text": the line says what went along and
  // what did not, and that is all it needs to.
  import type { Slip } from "../bindings/Slip";
  import type { Carried } from "../transcript";

  let { slip = null, carried }: { slip?: Slip | null; carried?: Carried } = $props();

  const size = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)} k` : String(n));
  const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;
  const parts: Record<string, string> = { you: "your message", hub: "Hub's message", handover: "the conversation", rolled_back: "rolled-back work", clash: "a clash", design: "design", lost_session: "the lost session", prompt: "the question", other: "the rest" };
</script>

{#if slip}<span class="size">{size(slip.tokens)} tokens</span>{/if}
{#if carried && carried.messages > 0}
  <div class="went">
    <span><b>{plural(carried.messages, "message")}</b>{#if carried.tools}{" "}and the name of <b>{plural(carried.tools, "tool")}</b>{/if}{" "}went along.</span>
    <span>Left out: what the tools printed{carried.files ? `, ${plural(carried.files, "attached file")}` : ""}.</span>
  </div>
{:else if slip}
  <div class="went">
    {#each slip.sections as s (s.id)}<span>{parts[s.id] ?? s.id} <b>{size(s.tokens)}</b></span>{/each}
  </div>
{/if}

<style>
  .size { color: var(--text-2); font-variant-numeric: tabular-nums; }
  .went { flex-basis: 100%; display: flex; justify-content: center; column-gap: 6px; row-gap: 0; flex-wrap: wrap; margin-top: 1px; font-size: 12px; }
  .went b { font-weight: 400; color: var(--text-2); font-variant-numeric: tabular-nums; }
</style>
