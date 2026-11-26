<!-- Whether a reply showed that it works: a passing test run or a screenshot, Hub asking for one, a failing run or a page with errors, none after asking, or none needed (code only taken out, a revert, or waived).
     Used by Changes.svelte, Transcript.svelte. -->
<script lang="ts">
  // Whether a reply showed that it works: a passing test run or a screenshot, Hub asking for one,
  // a failing run or a page with errors, none after asking, or none needed (code only taken out, a revert, or waived).
  import type { ProofState } from "../transcript";
  import Icon from "./Icon.svelte";
  import { proofWords } from "../proofWords";

  let { proof }: { proof: ProofState } = $props();

  const shown = $derived(proofWords(proof));
</script>

<span class="proof {shown.tone}" data-tip={shown.title}>
  {#if shown.tone === "busy"}<span class="pulse"></span>{:else if proof.state === "skipped"}<Icon name="slash" size={11} />{:else if shown.tone === "done"}<Icon name="check" size={11} />{:else}<Icon name="x" size={11} />{/if}{shown.text}
</span>

<style>
  .proof { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; white-space: nowrap; }
  .done { color: var(--green); }
  .busy { color: var(--amber); }
  .bad { color: var(--red); }
  .none { color: var(--muted); }
  .pulse { width: 6px; height: 6px; border-radius: 50%; background: var(--amber); }
</style>
