<!-- Guard's panel on the Problems page: its state, the fix threads it has started, what it last said, and a line to tell it something. It sits where a problem's detail opens, so it shows when no problem is picked. Guard works from this list; its threads are listed here (and in the status bar), not in the inbox, unless one of them waits on you.
     Used by ProblemsView.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // Guard's panel on the Problems page: its state, the fix threads it has started, what it last
  // said, and a line to tell it something. It sits where a problem's detail opens, so it shows when
  // no problem is picked. Guard works from this list; its threads are listed here (and in the
  // status bar), not in the inbox, unless one of them waits on you.
  import type { VaktenStatus } from "../bindings/VaktenStatus";
  import { ago } from "../format";
  import { threadSays } from "../guard";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { vakten, onsettings }: { vakten: VaktenStatus | null; onsettings: () => void } = $props();

  const chatId = $derived(hub.guardChat?.id ?? vakten?.thread ?? null);
  const threads = $derived(hub.guardThreads);
  const news = $derived(hub.problemNews.slice(0, 3));
  const stateText = $derived.by(() => {
    switch (vakten?.state) {
      case "working": return "working";
      case "waiting": return "waits for you";
      case "paused": return "paused";
      case "idle": return "idle, nothing to do";
      default: return "off";
    }
  });
  let text = $state("");
  let sending = $state(false);

  async function tell() {
    const t = text.trim();
    if (!t || !chatId || sending) return;
    sending = true;
    try {
      await hub.send(chatId, t);
      text = "";
      hub.tell("Sent to Guard");
    } catch {
      // `hub.send` has already said what went wrong; the text stays.
    } finally {
      sending = false;
    }
  }
</script>

<aside class="guard">
  <div class="head">
    <span class="shield"><Icon name="shield" size={14} /></span>
    <b>Guard</b>
    <span class="state {vakten?.state ?? 'off'}">{stateText}</span>
    <button class="icon" aria-label="Guard's settings" data-tip="Guard's settings" onclick={onsettings}><Icon name="gear" size={13} /></button>
  </div>

  <div class="label">Its threads · {threads.length}</div>
  {#each threads as t (t.id)}
    {@const [tone, says] = threadSays(t)}
    <button class="trow" onclick={() => hub.open(t.id)} data-tip="Open the thread">
      <span class="dot {t.status}"></span>
      <span class="main">
        <span class="title">{t.title}</span>
        <span class="meta">{hub.projectName(t.cwd, t.host ?? null)}</span>
      </span>
      <span class="when {tone}">{says}</span>
    </button>
  {:else}
    <Quiet text="No fix threads" />
  {/each}

  {#if news.length}
    <div class="label">Last said</div>
    {#each news as n (n.ts)}
      <p class="news"><span class="dim">{ago(n.ts)}</span> {n.text}</p>
    {/each}
  {/if}

  {#if chatId}
    <div class="label">Tell Guard</div>
    <form class="tell" onsubmit={(e) => (e.preventDefault(), tell())}>
      <input class="field" placeholder="Tell Guard…" bind:value={text} disabled={sending} spellcheck="false" />
      <button class="btn small" disabled={!text.trim() || sending}>Send</button>
    </form>
    <button class="link" onclick={() => hub.open(chatId)}>Open Guard's chat</button>
  {/if}
</aside>

<style>
  .guard { width: var(--panel-w); flex: none; border-left: 1px solid var(--line); background: var(--bg-sidebar); overflow-y: auto; padding: calc(var(--controls-h) + 10px) 20px 40px; font-size: 13px; }
  .head { display: flex; align-items: center; gap: 8px; }
  .head b { color: var(--text); font-size: 15px; font-weight: 600; }
  .shield { display: grid; color: var(--muted); }
  .state { color: var(--muted); font-size: 12px; }
  .state.working, .state.idle { color: var(--green); }
  .state.waiting, .state.paused { color: var(--amber); }
  .icon { margin-left: auto; display: grid; place-items: center; width: 26px; height: 26px; border-radius: var(--r-sm); color: var(--muted); }
  .icon:hover { color: var(--text); background: var(--surface-hover); }
  .say { color: var(--text-3); font-size: 13px; line-height: 1.45; margin: 8px 0 0; }
  .label { color: var(--muted); font-size: 12px; margin: 18px 0 6px; }
  .trow { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 6px; margin: 0 -6px; border-radius: var(--r-sm); text-align: left; }
  .trow:hover { background: var(--surface-hover); }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .title { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .when { flex: none; font-size: 12px; color: var(--muted); }
  .when.work { color: var(--green); }
  .when.ask { color: var(--amber); }
  .when.bad { color: var(--red); }
  .quiet { color: var(--muted); margin: 4px 0; }
  .news { color: var(--text-2); font-size: 13px; line-height: 1.4; margin: 4px 0; }
  .dim { color: var(--muted); margin-right: 4px; }
  .tell { display: flex; gap: 6px; }
  .field { flex: 1; min-width: 0; height: 28px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; color: var(--text-2); }
  .field:focus { border-color: var(--accent); }
  .btn.small { height: 28px; padding: 0 10px; font-size: 12px; }
  .link { margin-top: 10px; font-size: 12px; color: var(--muted); padding: 0; }
  .link:hover { color: var(--text-2); }
</style>
