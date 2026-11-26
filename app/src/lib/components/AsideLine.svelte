<!-- What Hub did with a message you wrote while the thread worked (`judge.rs`), or its answer to a question for the side: one quiet grey line above the status line, with when. The next one replaces it, it is not part of the thread's log, and a click copies what it says.
     Used by ThreadView.svelte. -->
<script lang="ts">
  // What Hub did with a message you wrote while the thread worked (`judge.rs`), or its answer to a
  // question for the side: one quiet grey line above the status line, with when. The next one
  // replaces it, it is not part of the thread's log, and a click copies what it says.
  import type { Aside } from "../bindings/Aside";
  import Icon, { type IconName } from "./Icon.svelte";

  let { aside }: { aside: Aside } = $props();

  const glyph = $derived<IconName>(
    ({ answer: "help", stopped: "stop", steered: "swap", queued: "clock", new: "external" } as Record<string, IconName>)[aside.kind] ?? "help",
  );
  const said = $derived(
    aside.text ||
      ({ stopped: "Stopped", steered: "Steered it", queued: "Queued for after", new: "Looks like a task of its own" } as Record<string, string>)[aside.kind] ||
      "",
  );
  const at = $derived(new Date(Number(aside.at)).toLocaleTimeString([], { timeStyle: "short" }));
  let copied = $state(false);

  async function copy() {
    if (aside.kind !== "answer") return;
    try {
      await navigator.clipboard.writeText(aside.text);
      copied = true;
      setTimeout(() => (copied = false), 1200);
    } catch {
      // Copying is a convenience; the line is already readable.
    }
  }
</script>

{#if said}
  <div class="aside" class:answer={aside.kind === "answer"}>
    <Icon name={glyph} size={12} />
    {#if aside.question}<span class="q">{aside.question} →</span>{/if}
    <button class="text" onclick={copy} data-tip={aside.kind === "answer" ? (copied ? "Copied" : "Click to copy") : undefined} disabled={aside.kind !== "answer"}>{said}</button>
    <span class="at">{at}</span>
  </div>
{/if}

<style>
  .aside { display: flex; align-items: baseline; gap: 8px; min-width: 0; padding: 0 6px 6px; color: var(--muted); font-size: 13px; line-height: 1.4; }
  .aside :global(svg) { flex: none; align-self: center; }
  .q { color: var(--muted); white-space: nowrap; max-width: 40%; overflow: hidden; text-overflow: ellipsis; }
  .text { text-align: left; color: var(--text-3); font: inherit; padding: 0; min-width: 0; }
  .answer .text { color: var(--text-2); cursor: copy; }
  .text:disabled { cursor: default; }
  .at { margin-left: auto; font-variant-numeric: tabular-nums; white-space: nowrap; }
</style>
