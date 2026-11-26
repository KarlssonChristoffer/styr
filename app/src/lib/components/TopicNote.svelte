<!-- A decision model read this message as a different task than the thread has been on. Start it in a thread of its own opens New thread with the message ready (nothing is sent twice and nothing is taken back); Keep it here folds the note away. Either answer is remembered on this computer.
     Used by Transcript.svelte. -->
<script lang="ts">
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  // A decision model read this message as a different task than the thread has been on. Start
  // it in a thread of its own opens New thread with the message ready (nothing is sent twice
  // and nothing is taken back); Keep it here folds the note away. Either answer is remembered
  // on this computer.
  let { text, seq, by, threadId, cwd }: { text: string; seq: number; by: string; threadId: string; cwd: string } = $props();

  const KEY = "hub.topicNotes";
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
  function move() {
    const thread = hub.threads.find((t) => t.id === threadId);
    remember("moved");
    hub.startDraft(cwd, text, thread?.host ?? null);
  }
</script>

{#if !answer}
  <div class="topic" data-tip="Read by {by}">
    <Icon name="branch" size={13} />
    <span class="text">This looks like a different task than this thread has been on.</span>
    <button class="btn small" onclick={move}>Start it in a thread of its own</button>
    <button class="btn small ghost" onclick={() => remember("kept")}>Keep it here</button>
  </div>
{/if}

<style>
  .topic { display: flex; align-items: center; gap: 10px; margin: 8px 0; padding: 8px 12px; border-radius: var(--r-md); background: var(--surface); color: var(--text-3); font-size: 13px; }
  .topic .text { flex: 1; min-width: 0; }
  .small { height: 24px; font-size: 12px; }
  .ghost { background: none; color: var(--muted); }
  .ghost:hover { color: var(--text); }
</style>
