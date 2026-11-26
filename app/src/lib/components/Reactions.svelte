<!-- The emojis on one message, as Claude Code draws them: soft grey chips with the count, under your bubble on the right and under a reply on the left. Yours is tinted with the accent; a click puts it on or takes it away. The ghost button beside them adds another.
     Used by Changes.svelte, Transcript.svelte. -->
<script lang="ts">
  // The emojis on one message, as Claude Code draws them: soft grey chips with the count, under
  // your bubble on the right and under a reply on the left. Yours is tinted with the accent; a click
  // puts it on or takes it away. The ghost button beside them adds another.
  import { hub } from "../state.svelte";
  import { emojis, picker, reactionsOf, react, togglePicker } from "../reactions.svelte";
  import Emoji from "./Emoji.svelte";
  import Icon from "./Icon.svelte";

  let { threadId, seq, align = "end" }: { threadId: string; seq: number; align?: "start" | "end" } = $props();

  const chips = $derived(emojis.on ? (reactionsOf(hub.events[threadId] ?? []).get(seq) ?? []) : []);
  const adding = $derived(picker.at?.thread === threadId && picker.at.seq === seq);
  const who = (c: { you: boolean; agent: boolean }) => (c.you && c.agent ? "You and the agent" : c.you ? "You" : "The agent");
</script>

{#if chips.length}
  <div class="chips" class:start={align === "start"} role="group" aria-label="Reactions">
    {#each chips as c (c.emoji)}
      <button class="chip" class:mine={c.you} data-tip="{who(c)} · {c.you ? 'click to take it away' : 'click to add yours'}" onclick={() => react(threadId, seq, c.emoji, !c.you)}>
        <Emoji emoji={c.emoji} size={14} />{#if c.count > 1}<span class="n">{c.count}</span>{/if}
      </button>
    {/each}
    <button class="ghost" data-react-btn class:on={adding} data-tip="Add a reaction" aria-label="Add a reaction" onclick={(e) => togglePicker(e.currentTarget, threadId, seq)}><Icon name="smilePlus" size={14} /></button>
  </div>
{/if}

<style>
  .chips { display: flex; flex-wrap: wrap; justify-content: flex-end; align-items: center; gap: 4px; max-width: 80%; margin-top: 1px; }
  .chips.start { justify-content: flex-start; max-width: 100%; margin: 6px 0 4px; }
  .chip {
    display: inline-flex; align-items: center; gap: 4px; height: 24px; padding: 0 8px; border-radius: 999px;
    background: var(--surface-hover); color: var(--text-2); font-size: 12px; font-variant-numeric: tabular-nums;
    transition: background 0.12s var(--ease);
  }
  .chip:hover { background: var(--line-strong); }
  .chip.mine { background: var(--accent-bg); color: var(--accent); }
  .chip.mine:hover { background: color-mix(in srgb, var(--accent) 26%, transparent); }
  .ghost { display: grid; place-items: center; width: 26px; height: 24px; border-radius: 999px; color: var(--muted); opacity: 0.55; transition: opacity 0.12s var(--ease), background 0.12s var(--ease); }
  .ghost:hover, .ghost.on { opacity: 1; color: var(--text-2); background: var(--surface-hover); }
</style>
