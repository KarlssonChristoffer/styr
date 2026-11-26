<!-- A small model found that this thread's first message does not belong to the project's stage now. Park it puts the idea under Parked in docs/FOCUS.md, stops the thread and settles it; Keep going folds the note away. Either answer is remembered on this computer.
     Used by Transcript.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  // A small model found that this thread's first message does not belong to the project's stage
  // now. Park it puts the idea under Parked in docs/FOCUS.md, stops the thread and settles it;
  // Keep going folds the note away. Either answer is remembered on this computer.
  let { stage, why, seq, threadId, cwd }: { stage: string; why: string; seq: number; threadId: string; cwd: string } = $props();

  const KEY = "hub.focusNotes";
  const read = (): Record<string, string> => {
    try {
      return JSON.parse(localStorage.getItem(KEY) ?? "{}");
    } catch {
      return {};
    }
  };
  const id = $derived(`${threadId}:${seq}`);
  let answer = $state<string | null>(null);
  $effect(() => {
    answer = read()[id] ?? null;
  });
  function remember(a: string) {
    answer = a;
    try {
      localStorage.setItem(KEY, JSON.stringify({ ...read(), [id]: a }));
    } catch {
      // Shown again next time; nothing else depends on it.
    }
  }

  let busy = $state(false);
  async function park() {
    const thread = hub.threads.find((t) => t.id === threadId);
    busy = true;
    try {
      await api.editFocus({ path: cwd, op: "park", text: thread?.title ?? "An idea from a thread" }, thread?.host ?? null);
      if (thread?.status === "running" || thread?.status === "waiting_on_you") await quiet(api.cancel(threadId), "cancel");
      remember("parked");
      hub.settle(threadId, true);
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  }
</script>

{#if answer === "parked"}
  <div class="note"><Icon name="pin" size={12} /> Parked in <span class="mono">docs/FOCUS.md</span>, for after {stage}.</div>
{:else if !answer}
  <div class="focus">
    <Icon name="star" size={13} />
    <span class="text">Not part of the stage now, <b>{stage}</b>{why ? `: ${why.replace(/\.$/, "")}` : ""}.</span>
    <button class="btn small" disabled={busy} onclick={park} data-tip="Put it under Parked in docs/FOCUS.md, stop this thread and settle it">Park it</button>
    <button class="btn small ghost" onclick={() => remember("kept")}>Keep going</button>
  </div>
{/if}

<style>
  .focus { display: flex; align-items: center; gap: 10px; margin: 8px 0; padding: 8px 12px; border-radius: var(--r-md); background: var(--surface); color: var(--text-3); font-size: 13px; }
  .focus .text { flex: 1; min-width: 0; }
  .focus b { color: var(--text-2); font-weight: 500; }
  .small { height: 24px; font-size: 12px; }
  .ghost { background: none; color: var(--muted); }
  .ghost:hover { color: var(--text); }
  .note { color: var(--muted); font-size: 13px; margin: 8px 0; display: flex; align-items: center; gap: 6px; }
  .mono { font-family: var(--mono); font-size: 12px; }
</style>
