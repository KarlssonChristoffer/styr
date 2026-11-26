<!-- One line in a project's desk: another thread of the project finished, needs you or failed. The thread's dot is live, so a line about a question turns grey once it is answered. A question can be answered from here with a message, unless it asks to use a tool: that is answered in the thread.
     Used by Transcript.svelte. -->
<script lang="ts">
  // One line in a project's desk: another thread of the project finished, needs you or failed.
  // The thread's dot is live, so a line about a question turns grey once it is answered. A
  // question can be answered from here with a message, unless it asks to use a tool: that is
  // answered in the thread.
  import { clock } from "../format";
  import { hub, unread } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { item }: { item: { thread: string; title: string; state: "done" | "asks" | "failed"; text: string; permission: boolean; ts: number } } = $props();

  const thread = $derived(hub.threads.find((t) => t.id === item.thread));
  // Still open while the thread waits on you since this note.
  const open = $derived(item.state === "asks" && thread?.status === "waiting_on_you" && (thread.returned_at ?? 0) <= item.ts + 2000);
  const said = { done: "finished", asks: "asks", failed: "failed" } as const;
  let answer = $state("");
  let sending = $state(false);

  async function send(e: Event) {
    e.preventDefault();
    const text = answer.trim();
    if (!text || sending) return;
    sending = true;
    try {
      await hub.send(item.thread, text);
      answer = "";
    } finally {
      sending = false;
    }
  }
</script>

<div class="desk-note {item.state}" class:open>
  <div class="line">
    <span class="dot {thread?.status ?? 'idle'}" class:unread={thread ? unread(thread) : false}></span>
    <button class="chip" onclick={() => hub.open(item.thread)} data-tip="Open the thread">
      <Icon name="threads" size={11} /><span class="ellipsis">{thread?.title ?? item.title}</span>
    </button>
    <span class="said">{said[item.state]}</span>
    {#if item.state !== "asks" && item.text}<span class="text ellipsis" data-tip={item.text}>{item.text}</span>{/if}
    <span class="when">{clock(item.ts)}</span>
  </div>
  {#if item.state === "asks" && item.text}
    <div class="ask">
      <p class="q">{item.text}</p>
      {#if open}
        {#if item.permission}
          <button class="btn small" onclick={() => hub.open(item.thread)}>Answer in the thread</button>
        {:else}
          <form class="reply" onsubmit={send}>
            <input bind:value={answer} placeholder="Answer {thread?.title ?? 'it'}…" disabled={sending} />
            <button class="btn small" disabled={!answer.trim() || sending}>Send</button>
          </form>
        {/if}
      {/if}
    </div>
  {/if}
</div>

<style>
  .desk-note { margin: 6px 0; font-size: 13px; color: var(--text-3); }
  .line { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .chip { display: inline-flex; align-items: center; gap: 4px; max-width: 46%; padding: 1px 8px 1px 6px; border-radius: var(--r-sm); background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--line); color: var(--text); font-size: 13px; flex: none; }
  .chip:hover { background: var(--surface-hover); }
  .said { flex: none; color: var(--muted); }
  .failed .said { color: var(--red); }
  .text { flex: 1; min-width: 0; color: var(--muted); }
  .when { flex: none; margin-left: auto; color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .ask { margin: 6px 0 2px 4px; padding: 2px 0 2px 12px; border-left: 2px solid var(--line-strong); display: flex; flex-direction: column; gap: 8px; }
  .open .ask { border-left-color: var(--amber); }
  .q { margin: 0; color: var(--text-2); font-size: 14px; line-height: 1.5; white-space: pre-wrap; word-break: break-word; }
  .open .q { color: var(--text); }
  .ask > .btn { align-self: flex-start; }
  .reply { display: flex; gap: 6px; max-width: 520px; }
  .reply input { flex: 1; min-width: 0; background: var(--bg); color: var(--text); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 4px 8px; font: inherit; font-size: 13px; }
  .reply input:focus { outline: none; border-color: var(--accent); }
  .ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
