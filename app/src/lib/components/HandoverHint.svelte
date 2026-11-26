<!-- The line under a model switch: Hub handed the new engine the conversation so far, and what that came to. The counts are the thread's own; the size comes from the slip Hub wrote when the next message went, on a hubd that keeps slips (feature `package.ledger`).
     Used by Transcript.svelte. -->
<script lang="ts">
  import { quietly } from "../quiet";
  // The line under a model switch: Hub handed the new engine the conversation so far, and what that
  // came to. The counts are the thread's own; the size comes from the slip Hub wrote when the next
  // message went, on a hubd that keeps slips (feature `package.ledger`).
  import { api } from "../api";
  import { hubdHas } from "../compat";
  import type { Slip } from "../bindings/Slip";
  import type { Carried } from "../transcript";
  import SlipView from "./SlipView.svelte";

  let { threadId, ts, carried, ticks = 0 }: { threadId: string; ts: number; carried?: Carried; ticks?: number } = $props();
  let slip = $state<Slip | null>(null);

  // The slip is written when the next message goes, so look again as the thread grows until it is there.
  $effect(() => {
    void ticks;
    if (slip || !threadId) return;
    hubdHas("package.ledger") // compat-until: 2026-12-03
      .then((has) => (has ? api.packageSlips(threadId, "switch") : []))
      .then((all) => {
        slip = all.filter((s) => s.at >= ts).at(-1) ?? null;
      })
      .catch(quietly("handover slip"));
  });
</script>

Hub handed it the conversation so far.{#if slip}{" "}<span class="dot">·</span>{" "}{/if}
<SlipView {slip} {carried} />

<style>
  .dot { color: var(--muted); }
</style>
