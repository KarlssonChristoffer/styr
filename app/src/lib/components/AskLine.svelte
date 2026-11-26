<!-- A question the agent asked without waiting (`ask_user`): one quiet line in the chat with what it assumed and, once answered, what you said. The options stay under it so the answer can be changed later; the agent gets each answer as a normal message (`askMessage`).
     Used by Transcript.svelte. -->
<script lang="ts">
  // A question the agent asked without waiting (`ask_user`): one quiet line in the chat with what it
  // assumed and, once answered, what you said. The options stay under it so the answer can be
  // changed later; the agent gets each answer as a normal message (`askMessage`).
  import { askMessage, type AskItem } from "../transcript";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { item, threadId }: { item: AskItem; threadId: string } = $props();
  let open = $state(false);
  let sending = $state(false);

  async function pick(n: number) {
    const option = item.options[n];
    if (!option || sending) return;
    sending = true;
    try {
      await hub.send(threadId, askMessage(item, option));
      open = false;
    } catch {
      // Shown by hub.send.
    } finally {
      sending = false;
    }
  }
</script>

<div class="ask">
  <button class="line" onclick={() => (open = !open)} data-tip={open ? "Hide the options" : item.answer == null ? "Answer, or leave it: the agent carries on with its assumption" : "Change your answer"}>
    <Icon name="help" size={13} />
    <span class="q">{item.question}</span>
    <span class="note">{item.answer != null ? `you said ${item.answer}` : item.assumed ? `assumed ${item.assumed}` : ""}</span>
  </button>
  {#if open || item.answer == null}
    <div class="options">
      {#each item.options as o, n (n)}
        <button class="option" class:picked={item.answer === o.message} disabled={sending} onclick={() => pick(n)}>{o.label}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .ask { margin: 4px 0 8px; min-width: 0; }
  .line { display: flex; align-items: center; gap: 8px; max-width: 100%; color: var(--muted); font-size: 13px; text-align: left; min-width: 0; }
  .line:hover { color: var(--text-3); }
  .q { color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .note { font-size: 12px; flex: none; }
  .options { display: flex; flex-wrap: wrap; gap: 6px; margin: 6px 0 0 21px; }
  .option { height: 24px; padding: 0 10px; border-radius: var(--r-sm); border: 1px solid var(--line-strong); color: var(--text-3); font-size: 12px; }
  .option:hover { background: var(--surface-hover); color: var(--text); }
  .option.picked { color: var(--text); background: var(--surface-2); }
</style>
