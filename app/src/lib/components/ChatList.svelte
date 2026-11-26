<!-- The Chat world's list: every everyday chat from the iPhone or New chat, newest first, in Today and Earlier, with nothing of the work side (no project, branch or pull request). A chat that waits on you or failed says so in state colour; the rest is just its title and when it moved.
     Used by Sidebar.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // The Chat world's list: every everyday chat from the iPhone or New chat, newest first, in Today
  // and Earlier, with nothing of the work side (no project, branch or pull request). A chat that
  // waits on you or failed says so in state colour; the rest is just its title and when it moved.
  import type { Thread } from "../bindings/Thread";
  import { listTime } from "../format";
  import { menu } from "../menu.svelte";
  import { threadMenu } from "../menus.svelte";
  import { prefs } from "../prefs.svelte";
  import { chatMark, hub } from "../state.svelte";

  const at = (t: Thread) => t.returned_at ?? t.updated_at;
  const startOfDay = $derived.by(() => {
    const d = new Date(hub.now);
    d.setHours(0, 0, 0, 0);
    return d.getTime();
  });
  const today = $derived(hub.chats.filter((t) => at(t) >= startOfDay));
  const earlier = $derived(hub.chats.filter((t) => at(t) < startOfDay));

  const dotTip = { failed: "Didn't finish", writing: "Writing", new: "A new answer" };
</script>

{#snippet group(label: string, list: Thread[])}
  {#if list.length}
    <div class="label">{label}</div>
    {#each list as t (t.id)}
      {@const m = chatMark(t)}
      <button class="chat" class:active={hub.view === "thread" && hub.selected === t.id} class:fresh={m === "new"} onclick={() => hub.open(t.id)} use:menu={() => threadMenu(t)}>
        <!-- The dot hangs in the row's left padding, so the titles line up whether or not a chat has one. -->
        {#if m}<span class="mark {m}" data-tip={dotTip[m]}></span>{/if}
        <span class="title ellipsis">{t.title}</span>
        <span class="when">{listTime(at(t), hub.now, prefs.listTime)}</span>
      </button>
    {/each}
  {/if}
{/snippet}

<div class="list">
  {@render group("Today", today)}
  {@render group("Earlier", earlier)}
  {#if !hub.chats.length}
    <Quiet text="No chats" />
  {/if}
</div>

<style>
  .list { display: flex; flex-direction: column; gap: 1px; padding: 4px 8px 12px; }
  .label { padding: 14px 10px 4px; font-size: 12px; color: var(--muted); letter-spacing: 0.02em; }
  .chat {
    display: flex; align-items: baseline; gap: 10px; padding: 8px 10px; border-radius: var(--r-md); text-align: left; width: 100%;
    color: var(--text-2);
  }
  .chat:hover { background: var(--surface-hover); }
  .chat.active { background: var(--surface); box-shadow: inset 0 0 0 1px var(--line-strong); color: var(--text); }
  .title { flex: 1; min-width: 0; font-size: 14px; }
  .ellipsis { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .when { flex: none; font-size: 12px; color: var(--muted); }
  .chat { position: relative; }
  .chat.fresh .title { font-weight: 600; color: var(--text); }
  .mark { position: absolute; left: 0; top: 50%; width: 6px; height: 6px; margin-top: -3px; border-radius: 50%; }
  .mark.new { background: var(--accent); }
  .mark.failed { background: var(--red); }
  .mark.writing { background: var(--green); animation: pulse 2s var(--ease-move) infinite; }
  .empty { margin: 12px 10px; font-size: 13px; color: var(--muted); }
</style>
