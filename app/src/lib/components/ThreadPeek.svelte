<!-- The hover card on a thread in the inbox: where it stands in a few lines (the TL;DR the agent wrote itself with `write_tldr`, shown with when; asked for here when missing or outdated, `gist.rs`), what it needs from you with its quick replies and a reply box, live facts (pull request, branch, files, tests) and the everyday actions. The agent's own last reply is one key away (R), so the summary can be checked against it.
     Used by ActivityList.svelte. -->
<script lang="ts">
  // The hover card on a thread in the inbox: where it stands in a few lines (the TL;DR the agent
  // wrote itself with `write_tldr`, shown with when; asked for here when missing or outdated, `gist.rs`), what it needs from you with its quick replies and a
  // reply box, live facts (pull request, branch, files, tests) and the everyday actions. The
  // agent's own last reply is one key away (R), so the summary can be checked against it.
  import type { Thread } from "../bindings/Thread";
  import type { ThreadPeek } from "../bindings/ThreadPeek";
  import { api } from "../api";
  import { duration } from "../activity";
  import { ago } from "../format";
  import { contextMenu } from "../menu.svelte";
  import { snoozeMenu, threadMenu } from "../menus.svelte";
  import { portal } from "../portal";
  import { providerOf } from "../providers.svelte";
  import { asksYou, hub, needsYou, unread } from "../state.svelte";
  import { takeOver } from "../terminal";
  import Icon from "./Icon.svelte";
  import Markdown from "./Markdown.svelte";

  let {
    thread: t,
    anchor,
    onkeep,
    onleave,
    onclose,
  }: {
    thread: Thread;
    /** The row it belongs to; the card sits to its right. */
    anchor: HTMLElement;
    onkeep: () => void;
    onleave: () => void;
    onclose: () => void;
  } = $props();

  // Peeks seen this session, so hovering a row again shows at once while a fresh one loads.
  const seen = cache();
  let peek = $state<ThreadPeek | null>(null);
  // The card is made anew for each thread (`{#key}` in the list), so the first value is the one.
  $effect.pre(() => {
    if (!peek) peek = seen.get(t.id) ?? null;
  });
  let failed = $state(false);
  let showReply = $state(false);
  let text = $state("");
  let sent = $state(false);
  let card = $state<HTMLElement>();
  let box = $state<HTMLTextAreaElement>();
  let pos = $state({ left: 0, top: 0 });

  $effect(() => {
    const id = t.id;
    let gone = false;
    api
      .peek(id)
      .then((p) => {
        if (gone) return;
        peek = p;
        seen.set(id, p);
      })
      .catch(() => !gone && (failed = true));
    return () => (gone = true);
  });

  // Scrolling anything but the card itself moves the row away from it.
  $effect(() => {
    const scrolled = (e: Event) => !(e.target instanceof Node && card?.contains(e.target)) && onclose();
    document.addEventListener("scroll", scrolled, true);
    return () => document.removeEventListener("scroll", scrolled, true);
  });

  // Beside the row, inside the window. The sidebar is drawn larger (`--side-scale`), the card is
  // not, so it is placed from what the row measures on screen.
  $effect(() => {
    void peek;
    void showReply;
    if (!card) return;
    // The row is gone (settled, snoozed, moved away): there is nothing to sit beside.
    if (!anchor.isConnected) {
      onclose();
      return;
    }
    const zoom = "__TAURI_INTERNALS__" in window ? 1 : Number(document.documentElement.style.zoom) || 1;
    const r = anchor.getBoundingClientRect();
    const vw = innerWidth / zoom, vh = innerHeight / zoom;
    const h = card.offsetHeight, w = card.offsetWidth;
    const left = Math.min(r.right / zoom + 10, vw - w - 8);
    const top = Math.max(8, Math.min(r.top / zoom - 6, vh - h - 8));
    pos = { left, top };
  });

  // The TL;DR's ask counts while it is current: written after the last turn and nothing since.
  const asks = $derived(
    !!peek?.replies || !!peek?.permission || t.status === "error" || (!!peek?.ask && (needsYou(t) || (!peek.stale && t.status === "idle"))),
  );
  const question = $derived(
    t.status === "error"
      ? peek?.error ?? "It failed."
      : peek?.permission
        ? `Wants to: ${peek.permission}`
        : peek?.replies?.question || peek?.ask || "",
  );
  const pr = $derived(hub.prs[t.id]);
  const branch = $derived(hub.branches[t.id]);
  const provider = $derived(providerOf(t.engine));
  const fallback = $derived(peek && !peek.points.length ? firstLines(peek.last_reply) : "");

  function status(): { words: string; tone: string } {
    if (t.status === "waiting_on_you") return { words: "Waiting for you", tone: "amber" };
    if (t.status === "error") return { words: "Failed", tone: "red" };
    if (asksYou(t)) return t.reading === "stuck" ? { words: "Stuck, needs you", tone: "red" } : { words: "Asks you", tone: "amber" };
    if (t.status === "running") return { words: `Working · ${duration(hub.now - (t.sent_at ?? t.updated_at))}`, tone: "green" };
    if (t.status === "in_terminal") return { words: "In the terminal", tone: "" };
    if (unread(t)) return { words: t.reading === "partway" ? "Stopped half way" : "Done", tone: "accent" };
    return { words: "Idle", tone: "" };
  }

  /** The reply's opening lines, for when no TL;DR is written yet. */
  function firstLines(reply: string) {
    const s = reply.replace(/^\[…\]\s*/, "").trim();
    return s.length > 320 ? `${s.slice(0, 320).replace(/\s+\S*$/, "")} …` : s;
  }

  const ASK_TLDR = "Write a TL;DR of this thread with the write_tldr tool.";

  /** The clock time it was written, with the day when it is not today. */
  function when(ms: number) {
    const d = new Date(ms);
    const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    return d.toDateString() === new Date().toDateString() ? time : `${d.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
  }

  async function send(message: string) {
    const body = message.trim();
    if (!body || sent) return;
    sent = true;
    try {
      await hub.send(t.id, body);
      setTimeout(onclose, 700);
    } catch {
      sent = false;
    }
  }

  function open() {
    hub.open(t.id);
    onclose();
  }

  async function takeOverHere() {
    hub.open(t.id);
    onclose();
    try {
      const note = await takeOver(t);
      if (note) hub.tell(note);
    } catch (e) {
      hub.fail(e);
    }
  }

  function at(e: MouseEvent, entries: ReturnType<typeof snoozeMenu>) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenu.show(r.left, r.bottom + 4, entries);
  }

  // Keys while the card is up and you are not typing: 1-3 quick replies, Enter opens, E settles,
  // S snoozes, R shows the last reply, Esc closes.
  function keydown(e: KeyboardEvent) {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const typing = e.target instanceof HTMLElement && e.target.closest("input, textarea, [contenteditable]");
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
      return;
    }
    if (typing) return;
    const n = Number(e.key);
    if (n >= 1 && n <= 3 && peek?.replies?.options[n - 1]) {
      e.preventDefault();
      send(peek.replies.options[n - 1].message);
    } else if (e.key === "Enter") {
      e.preventDefault();
      open();
    } else if (e.key === "e" || e.key === "E") {
      e.preventDefault();
      hub.settle(t.id, !t.settled_at);
      onclose();
    } else if (e.key === "r" || e.key === "R") {
      e.preventDefault();
      showReply = !showReply;
    }
  }

  function typed(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send(text);
    }
  }

  function grow() {
    if (!box) return;
    box.style.height = "auto";
    box.style.height = `${Math.min(box.scrollHeight, 120)}px`;
  }

  function cache(): Map<string, ThreadPeek> {
    const w = window as unknown as { __hubPeeks?: Map<string, ThreadPeek> };
    return (w.__hubPeeks ??= new Map());
  }

  const s = $derived(status());
</script>

<svelte:window onkeydown={keydown} />

<div
  class="peek"
  use:portal
  bind:this={card}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
  role="dialog"
  tabindex="-1"
  aria-label="About {t.title}"
  onpointerenter={onkeep}
  onpointerleave={onleave}
>
  <header>
    <div class="title">{t.title}</div>
    <div class="meta">
      <span class="dot {t.status}" class:unread={unread(t)}></span>
      <span class="state {s.tone}">{s.words}</span>
      <span class="sep">·</span><span>{ago(t.returned_at ?? t.updated_at, hub.now)} ago</span>
      {#if t.host}<span class="sep">·</span><span>{t.host}</span>{/if}
      <span class="sep">·</span><span>{provider.name}</span>
      {#if peek}<span class="sep">·</span><span>{peek.turns} {peek.turns === 1 ? "message" : "messages"}</span>{/if}
    </div>
  </header>

  <section class="tldr">
    <div class="label">
      TL;DR
      {#if peek?.written_at}
        <span class="note" class:old={peek.stale} data-tip={new Date(peek.written_at).toLocaleString()}>
          · {peek.by === "agent" ? "written by the agent" : "written"} {when(peek.written_at)}{peek.stale && peek.since ? ` · ${peek.since} ${peek.since === 1 ? "message" : "messages"} since` : peek.stale ? " · outdated" : ""}
        </span>
      {:else if peek}<span class="note">· none written yet</span>{/if}
    </div>
    {#if peek?.points.length}
      <Markdown rich thread={t} text={peek.points.map((p) => `- ${p}`).join("\n")} />
    {:else if fallback}
      <div class="fallback"><Markdown rich thread={t} text={fallback} /></div>
    {:else if !peek && !failed}
      <div class="shimmer"></div><div class="shimmer short"></div>
    {:else}
      <p class="none">{failed ? "Could not read this thread just now." : "Nothing said yet."}</p>
    {/if}
    {#if peek && peek.turns > 0 && peek.stale && t.status !== "running"}
      <button class="b pri ask-tldr" disabled={sent} onclick={() => send(ASK_TLDR)}>
        <Icon name="spark" size={11} />{peek.written_at ? "Ask for a new TL;DR" : "Ask for a TL;DR"}
      </button>
    {/if}
  </section>

  {#if peek?.last_reply && peek.points.length}
    <button class="more" onclick={() => (showReply = !showReply)}>
      <span class="chev" class:open={showReply}><Icon name="chevron" size={11} /></span>{showReply ? "Hide" : "Show"} last reply <kbd>R</kbd>
    </button>
    {#if showReply}
      <div class="reply"><Markdown rich thread={t} text={peek.last_reply} /></div>
    {/if}
  {/if}

  <div class="ask" class:asks class:red={t.status === "error"}>
    {#if asks && question}
      <div class="label">{t.status === "error" ? "Failed" : "Needs you"}</div>
      <div class="q"><Markdown rich thread={t} text={question} /></div>
    {/if}
    {#if peek?.replies}
      <div class="btns">
        {#each peek.replies.options.slice(0, 3) as o, i (i)}
          <button class="b" class:pri={i === 0} disabled={sent} onclick={() => send(o.message)} data-tip={o.message !== o.label ? o.message : undefined}><kbd>{i + 1}</kbd>{o.label}</button>
        {/each}
      </div>
    {/if}
    {#if peek?.permission}
      <div class="btns"><button class="b pri" onclick={open}>Open to answer</button></div>
    {:else}
      <div class="compose" class:sent>
        <textarea
          bind:this={box}
          bind:value={text}
          rows="1"
          disabled={sent}
          placeholder={sent ? "Sent" : asks ? "…or write a reply" : t.status === "running" ? "Queue a message for when it is done" : "Write to this thread"}
          onkeydown={typed}
          oninput={grow}
        ></textarea>
        <button class="go" disabled={!text.trim() || sent} onclick={() => send(text)} data-tip="Send (Enter)"><Icon name="play" size={11} /></button>
      </div>
    {/if}
  </div>

  {#if pr || branch || peek?.files != null || peek?.tests}
    <div class="facts">
      {#if pr}
        <button class="chip pr is-{pr.state}" onclick={() => { hub.openProject(t.cwd, t.host ?? null); hub.github = { mode: "pulls", pull: pr.number, issue: null }; hub.projectTab = "github"; onclose(); }} data-tip={pr.title}>
          <Icon name="pr" size={11} />#{pr.number} {pr.state}
        </button>
      {/if}
      {#if branch && branch !== "main" && branch !== "master"}<span class="chip mono"><Icon name="branch" size={11} />{branch}</span>{/if}
      {#if peek?.files != null}<span class="chip">{peek.files} {peek.files === 1 ? "file" : "files"} changed</span>{/if}
      {#if peek?.tests}<span class="chip {peek.tests === 'passed' ? 'ok' : 'bad'}">tests {peek.tests}</span>{/if}
    </div>
  {/if}

  <footer>
    <button class="act main" onclick={open}><Icon name="external" size={12} />Open thread <kbd>↵</kbd></button>
    <button class="act" onclick={() => { hub.settle(t.id, !t.settled_at); onclose(); }}><Icon name="check" size={12} />{t.settled_at ? "Unsettle" : "Settle"} <kbd>E</kbd></button>
    <button class="act" onclick={(e) => at(e, snoozeMenu([t.id]))}><Icon name="clock" size={12} />Snooze</button>
    <span class="grow"></span>
    <button class="act" data-tip={t.pinned_at ? "Unpin" : "Pin on top"} onclick={() => hub.pin([t.id], !t.pinned_at)}><Icon name="pin" size={12} /></button>
    {#if t.engine_session_id && t.status !== "in_terminal"}
      <button class="act" data-tip="Take over in Hub's terminal" onclick={takeOverHere}><Icon name="terminal" size={12} /></button>
    {/if}
    <button class="act" data-tip="More" onclick={(e) => at(e, threadMenu(t))}><Icon name="more" size={12} /></button>
  </footer>
</div>

<style>
  .peek {
    position: fixed; z-index: 60; width: calc(440px * var(--text-grow)); max-height: calc(100vh - 16px); overflow-y: auto;
    background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    box-shadow: 0 14px 40px rgba(0, 0, 0, 0.45); font-size: 13px; color: var(--text-2);
    animation: rise var(--dur-fast) var(--ease);
  }
  @keyframes rise { from { opacity: 0; transform: translateY(3px); } }
  @media (prefers-reduced-motion: reduce) { .peek { animation: none; } }
  header { padding: 12px 14px 10px; border-bottom: 1px solid var(--line); }
  .title { font-size: 14px; font-weight: 600; line-height: 1.35; color: var(--text); display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; overflow-wrap: anywhere; }
  .meta { margin-top: 4px; display: flex; flex-wrap: wrap; align-items: center; gap: 4px; font-size: 12px; color: var(--muted); }
  .meta .dot { margin-right: 2px; }
  .state.amber { color: var(--amber); }
  .state.red { color: var(--red); }
  .state.green { color: var(--green); }
  .state.accent { color: var(--accent); }
  .sep { color: var(--line-strong); }
  section, .more, .reply { padding: 0 14px; }
  .tldr { padding-top: 10px; padding-bottom: 8px; }
  .label { font-size: 12px; color: var(--muted); margin-bottom: 6px; display: flex; align-items: center; gap: 6px; }
  .note { text-transform: none; letter-spacing: 0; }
  .note.old { color: var(--amber); }
  .ask-tldr { margin-top: 8px; }
  .tldr :global(ul) { margin: 0; padding-left: 1.1em; display: flex; flex-direction: column; gap: 3px; }
  .tldr :global(li) { line-height: 1.45; }
  .fallback { color: var(--text-3); line-height: 1.45; }
  .none { color: var(--muted); margin: 0; }
  .shimmer { height: 10px; border-radius: var(--r-xs); background: var(--surface-2); margin: 6px 0; }
  .shimmer.short { width: 60%; }
  .more { display: flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; padding-bottom: 8px; }
  .more:hover { color: var(--text-2); }
  .chev { display: inline-flex; transition: transform 0.12s var(--ease); }
  .chev.open { transform: rotate(90deg); }
  .reply { margin: 0 14px 10px; padding: 2px 0 2px 10px; border-left: 2px solid var(--line-strong); max-height: 220px; overflow-y: auto; line-height: 1.5; }
  .ask { margin: 2px 14px 10px; }
  .ask.asks { border-radius: var(--r-md); background: var(--amber-bg); padding: 10px 12px; }
  .ask.asks.red { background: var(--red-bg); }
  .ask.asks .label { color: var(--amber); }
  .ask.asks.red .label { color: var(--red); }
  .q { color: var(--text); line-height: 1.45; }
  /* The box already says it asks; no second marker for the closing question. */
  .q :global(.ask) { margin: 0; padding: 0; }
  .q :global(.ask)::before { display: none; }
  .btns { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
  .b { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; padding: 4px 10px; border-radius: var(--r-md); border: 1px solid var(--line-strong); background: var(--surface-2); color: var(--text-2); }
  .b:hover { background: var(--surface-hover); color: var(--text); }
  .b.pri { background: var(--accent-bg); border-color: transparent; color: var(--accent); }
  kbd { font-family: var(--mono); font-size: 11px; color: var(--muted); border: 1px solid var(--line-strong); border-radius: var(--r-xs); padding: 0 4px; line-height: 1.4; }
  .compose { display: flex; align-items: flex-end; gap: 6px; margin-top: 8px; border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 6px 6px 6px 10px; background: var(--bg); }
  .ask.asks .compose { background: rgba(0, 0, 0, 0.18); }
  .compose:focus-within { border-color: var(--accent); }
  .compose.sent { opacity: 0.6; }
  textarea { flex: 1; resize: none; border: 0; outline: 0; background: none; color: var(--text); font: inherit; line-height: 1.4; min-height: 19px; padding: 1px 0; }
  textarea::placeholder { color: var(--muted); }
  .go { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 22px; border-radius: var(--r-sm); color: var(--text-3); }
  .go:not(:disabled):hover { background: var(--surface-hover); color: var(--text); }
  .go:disabled { opacity: 0.4; }
  .facts { display: flex; flex-wrap: wrap; gap: 6px; padding: 0 14px 10px; }
  .chip { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; padding: 2px 8px; border-radius: var(--r-sm); background: var(--surface-2); border: 1px solid var(--line); color: var(--text-3); white-space: nowrap; }
  .chip.mono { font-family: var(--mono); font-size: 11px; max-width: 220px; overflow: hidden; text-overflow: ellipsis; }
  .chip.pr { color: var(--text-2); }
  .chip.pr:hover { background: var(--surface-hover); text-decoration: underline; text-underline-offset: 3px; }
  .chip.pr.is-open, .chip.pr.is-draft, .chip.ok { color: var(--green); }
  .chip.pr.is-merged { color: var(--accent); }
  .chip.pr.is-closed, .chip.bad { color: var(--red); }
  footer { display: flex; align-items: center; gap: 2px; padding: 6px 8px; border-top: 1px solid var(--line); flex-wrap: wrap; }
  .act:has(:global(svg):only-child) { padding: 4px 8px; }
  .act { display: inline-flex; align-items: center; gap: 6px; padding: 4px 8px; border-radius: var(--r-sm); color: var(--text-3); font-size: 12px; white-space: nowrap; }
  .act:hover, .act.main { background: var(--surface-hover); color: var(--text); }
  .grow { flex: 1; }
</style>
