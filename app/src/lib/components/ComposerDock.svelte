<!-- What waits on you or on the agent, docked at the top of the composer: the open permission question or the question the reply ended on (with the replies the agent offered, or "Go ahead" under a proposal), answered with 1, 2, 3 while the message box is empty, and the messages queued behind the running turn, which can be edited or removed until the engine takes them.
     Used by ThreadView.svelte. -->
<script lang="ts">
  // What waits on you or on the agent, docked at the top of the composer: the open permission
  // question or the question the reply ended on (with the replies the agent offered, or "Go
  // ahead" under a proposal), answered with 1, 2, 3 while the message box is empty, and the messages queued behind the running turn, which can be edited or removed until
  // the engine takes them.
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import { askMessage, openAsk, openQuestion, type Item, type PermissionItem, type QueuedItem } from "../transcript";
  import Icon from "./Icon.svelte";
  import Permission from "./Permission.svelte";

  let { threadId, items, queue, idle }: { threadId: string; items: Item[]; queue: QueuedItem[]; idle: boolean } = $props();

  const open = $derived(items.filter((i): i is PermissionItem => i.kind === "permission" && !i.resolved));
  let permission = $state<Permission>();

  // The question the reply ended on, until something is said or it is put away.
  let dismissed = $state<Record<string, number>>({});
  const question = $derived.by(() => {
    if (!idle || open.length || queue.length) return null;
    const q = openQuestion(items);
    return q && dismissed[threadId] !== q.seq ? q : null;
  });
  // A question the agent asked without waiting: docked until answered, but never a "needs you".
  const waiting = $derived(open.length ? null : openAsk(items));
  let askDismissed = $state<Record<string, number>>({});
  const asked = $derived(waiting && askDismissed[threadId] !== waiting.seq ? waiting : null);
  let answering = $state(false);
  async function answerAsk(n: number) {
    const a = asked;
    const option = a?.options[n - 1];
    if (!a || !option || answering) return;
    answering = true;
    try {
      await hub.send(threadId, askMessage(a, option));
    } catch {
      // Shown by hub.send.
    } finally {
      answering = false;
    }
  }
  async function answer(n: number) {
    const q = question;
    const option = q?.options[n - 1];
    if (!q || !option || answering) return;
    answering = true;
    try {
      await hub.send(threadId, q.messages?.[n - 1] ?? `${n}. ${option}`);
    } catch {
      // Shown by hub.send.
    } finally {
      answering = false;
    }
  }

  // Number keys answer while nothing is being written: no field has focus, or the message box is
  // empty. Ctrl with a number stays the model picker's.
  function key(e: KeyboardEvent) {
    if (!/^[1-9]$/.test(e.key) || e.ctrlKey || e.metaKey || e.altKey || e.isComposing) return;
    if (!open.length && !question && !asked) return;
    const t = e.target as HTMLElement | null;
    const typing = t?.closest("input, textarea, select, [contenteditable=true], [role=dialog], [role=menu]");
    const emptyBox = t instanceof HTMLTextAreaElement && !!t.closest(".composer") && t.value === "";
    if (typing && !emptyBox) return;
    e.preventDefault();
    const n = Number(e.key);
    if (open.length) permission?.pick(n);
    else if (question) answer(n);
    else answerAsk(n);
  }

  // ---- The queue ----
  let editing = $state<number | null>(null);
  let draft = $state("");
  function edit(q: QueuedItem) {
    editing = q.seq;
    draft = q.text;
  }
  async function save(q: QueuedItem) {
    const text = draft.trim();
    editing = null;
    if (!text || text === q.text) return;
    try {
      await api.editQueued(threadId, q.seq, text);
    } catch (e) {
      hub.fail(e);
    }
  }
  async function steer(q: QueuedItem) {
    try {
      await api.steer(threadId, q.seq);
    } catch (e) {
      hub.fail(e);
    }
  }
  async function remove(q: QueuedItem) {
    try {
      await api.removeQueued(threadId, q.seq);
    } catch (e) {
      hub.fail(e);
    }
  }
  function editKey(e: KeyboardEvent, q: QueuedItem) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      save(q);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      editing = null;
    }
  }
  function grow(el: HTMLTextAreaElement) {
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
    const fit = () => {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 160) + "px";
    };
    fit();
    el.addEventListener("input", fit);
  }
</script>

<svelte:window onkeydown={key} />

{#if open.length}
  <Permission bind:this={permission} item={open[0]} {threadId} docked more={open.length - 1} />
{:else if question}
  <div class="question">
    <div class="ask">
      <Icon name="help" size={13} />
      <span class="dot waiting_on_you"></span>
      <span class="text" data-tip={question.question}>{question.question}</span>
      <button class="ghost" data-tip="Put away; answer in your own words" onclick={() => (dismissed = { ...dismissed, [threadId]: question.seq })}>
        <Icon name="x" size={12} />
      </button>
    </div>
    <div class="options">
      {#each question.options as o, n (n)}
        <button class="option" disabled={answering} onclick={() => answer(n + 1)} data-tip={o}>
          <span class="key">{n + 1}</span><span class="label">{o}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

{#if asked && !open.length}
  <div class="question quiet">
    <div class="ask">
      <Icon name="help" size={13} />
      <span class="text" data-tip={asked.question}>{asked.question}</span>
      {#if asked.assumed}<span class="assumed">assumed {asked.assumed}</span>{/if}
      <button class="ghost" data-tip="Put away; it stays in the chat" onclick={() => (askDismissed = { ...askDismissed, [threadId]: asked.seq })}>
        <Icon name="x" size={12} />
      </button>
    </div>
    <div class="options">
      {#each asked.options as o, n (n)}
        <button class="option" disabled={answering} onclick={() => answerAsk(n + 1)} data-tip={o.label}>
          <span class="key">{n + 1}</span><span class="label">{o.label}</span>
        </button>
      {/each}
    </div>
  </div>
{/if}

{#if queue.length}
  <div class="queue" aria-label="Queued messages">
    {#each queue as q (q.seq)}
      <div class="row" class:editing={editing === q.seq}>
        <span class="wait" data-tip="Queued {new Date(q.ts).toLocaleTimeString([], { timeStyle: 'short' })}."><Icon name="clock" size={13} /></span>
        {#if editing === q.seq}
          <textarea rows="1" bind:value={draft} onkeydown={(e) => editKey(e, q)} onblur={() => save(q)} use:grow></textarea>
        {:else}
          <button class="text" data-tip="Edit before it is sent" onclick={() => edit(q)}>
            {q.text || (q.attachments.length ? `${q.attachments.length} attached` : "")}
          </button>
          {#if q.attachments.length && q.text}<span class="tag"><Icon name="clip" size={11} />{q.attachments.length}</span>{/if}
          {#if q.edited}<span class="tag">edited</span>{/if}
          {#if q.steered}
            <span class="tag steering"><span class="dot running"></span>Steering, goes next</span>
          {:else}
            <button class="steer" data-tip="Stop the turn and send this next" onclick={() => steer(q)}><Icon name="arrowUp" size={12} />Steer</button>
          {/if}
          <span class="actions">
            <button class="ghost" data-tip="Edit" onclick={() => edit(q)}><Icon name="pen" size={12} /></button>
            <button class="ghost" data-tip="Remove from the queue" onclick={() => remove(q)}><Icon name="x" size={12} /></button>
          </span>
        {/if}
      </div>
    {/each}
    <div class="note">{queue.length === 1 ? "Goes" : `${queue.length} go`} when the turn ends.</div>
  </div>
{/if}

<style>
  .question, .queue { padding: 0 0 8px; margin: -2px 0 8px; border-bottom: 1px solid var(--line); }
  .ask { display: flex; align-items: center; gap: 8px; color: var(--muted); min-width: 0; padding-right: 4px; }
  .ask .text { color: var(--text); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1; }
  .assumed { color: var(--muted); font-size: 12px; flex: none; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .options { display: flex; flex-direction: column; gap: 1px; margin: 6px 8px 0 12px; }
  .option { display: flex; align-items: center; gap: 10px; text-align: left; padding: 4px 8px; border-radius: var(--r-md); color: var(--text-3); font-size: 13px; min-width: 0; }
  .option .label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .option:hover { background: var(--surface-hover); color: var(--text); }
  .key { display: inline-grid; place-items: center; width: 18px; height: 18px; border-radius: var(--r-sm); border: 1px solid var(--line-strong); color: var(--muted); font-size: 11px; font-family: var(--mono); flex: none; }

  .row { display: flex; align-items: center; gap: 8px; color: var(--muted); min-width: 0; padding: 1px 4px 1px 0; border-radius: var(--r-md); }
  .row .text { flex: 1; min-width: 0; text-align: left; color: var(--text-3); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 3px 0; }
  .row:hover .text { color: var(--text); }
  .row textarea { flex: 1; min-width: 0; resize: none; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 4px 8px; font-size: 13px; line-height: 1.45; outline: none; color: var(--text); }
  .tag { display: inline-flex; align-items: center; gap: 3px; font-size: 11px; color: var(--muted); flex: none; }
  .wait { display: grid; place-items: center; flex: none; }
  .tag.steering { color: var(--text-3); gap: 6px; }
  .steer { display: inline-flex; align-items: center; gap: 4px; flex: none; font-size: 12px; color: var(--muted); padding: 2px 8px; border-radius: var(--r-sm); opacity: 0; }
  .row:hover .steer, .row:focus-within .steer { opacity: 1; }
  .steer:hover { color: var(--text); background: var(--surface-hover); }
  .actions { display: flex; gap: 2px; opacity: 0; flex: none; }
  .row:hover .actions, .row:focus-within .actions { opacity: 1; }
  .ghost { color: var(--muted); display: grid; place-items: center; padding: 4px; border-radius: var(--r-sm); flex: none; }
  .ghost:hover { color: var(--text); background: var(--surface-hover); }
  .note { color: var(--muted); font-size: 12px; margin: 2px 0 0 21px; }
</style>
