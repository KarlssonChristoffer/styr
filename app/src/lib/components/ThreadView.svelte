<!-- Shown before the core has made the thread (`hub.showStarting`): the message and "Starting", and a box to write the next one in; what needs the real thread waits for it.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet, quietly } from "../quiet";
  import { poll } from "../poll";
  import { api } from "../api";
  import { payload } from "../events";
  import { hubdHas } from "../compat";
  import { left } from "../visits";
  import type { GitInfo } from "../bindings/GitInfo";
  import type { Thread } from "../bindings/Thread";
  import { folderName, isChatThread, isScratchRun, resets, statusLabel, clip, threadProject } from "../format";
  import { contextMenu, menu } from "../menu.svelte";
  import { pending, threadMenu } from "../menus.svelte";
  import { drag } from "../native";
  import { hub, isStandIn } from "../state.svelte";
  import { detailOf, prefs, setPref } from "../prefs.svelte";
  import { foldSession, rememberOptions, withKnownOptions } from "../session";
  import { engineProvider, engines, modelsFor } from "../providers.svelte";
  import type { SettingValue } from "../bindings/SettingValue";
  import { attemptOf, commands, designOn, folded, queued } from "../transcript";
  import AttemptBar from "./AttemptBar.svelte";
  import BranchChip from "./BranchChip.svelte";
  import GitButton from "./GitButton.svelte";
  import { attachCommand, takeBack, takeOver } from "../terminal";
  import { startedAsWeb } from "../web";
  import Composer from "./Composer.svelte";
  import ComposerDock from "./ComposerDock.svelte";
  import OwnerMark from "./OwnerMark.svelte";
  import { GLYPHS } from "../refs";
  import { isGuardChat, isGuardErrand } from "../system";
  import ProjectIcon from "./ProjectIcon.svelte";
  import { looks } from "../looks.svelte";
  import HostChip from "./HostChip.svelte";
  import CloudMark from "./CloudMark.svelte";
  import CloudCard from "./CloudCard.svelte";
  import { cloudOf } from "../cloud.svelte";
  import Icon from "./Icon.svelte";
  import Transcript from "./Transcript.svelte";
  import AsideLine from "./AsideLine.svelte";
  import StatusLine from "./StatusLine.svelte";
  import FindBar from "./FindBar.svelte";
  import { findRanges, mark, reveal } from "../find";
  import { tick } from "svelte";
  import { sim } from "../simview.svelte";

  let { thread }: { thread: Thread } = $props();
  let composer = $state<Composer>();
  /** The thread Ctrl Enter just started in the background, offered as a tab on the box for a while. */
  let started = $state<{ id: string; title: string } | null>(null);
  let startedTimer: ReturnType<typeof setTimeout> | undefined;
  // Shown before the core has made the thread (`hub.showStarting`): the message and "Starting",
  // and a box to write the next one in; what needs the real thread waits for it.
  const standIn = $derived(isStandIn(thread.id));

  // Attachments shared into this thread from elsewhere wait in the message box.
  $effect(() => {
    const waiting = hub.staged[thread.id];
    if (!waiting?.length || !composer) return;
    for (const a of waiting) composer.attach(a);
    const { [thread.id]: _, ...rest } = hub.staged;
    hub.staged = rest;
  });

  // Hub's short notes (a big paste became a file) sit above the box, in the chat, not in the bar.
  let note = $state("");
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  function hubNote(what: string) {
    note = what;
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => (note = ""), 4000);
  }
  const items = $derived(folded(hub.events[thread.id] ?? [], hub.since[thread.id]));
  // Design mode: a design document before any code (`design.rs`).
  const designing = $derived(designOn(hub.events[thread.id] ?? []));
  const queue = $derived(queued(hub.events[thread.id] ?? []));
  // The commands the agent ran, for the terminal drawer.
  const ran = $derived(commands(items));
  const reported = $derived(foldSession(hub.events[thread.id] ?? []));
  const attempt = $derived(attemptOf(hub.events[thread.id] ?? []));
  // Remembered so the home screen, and threads whose engine is still starting, can offer the
  // same settings. Under the engine that offered them: just after a switch the thread can name
  // its new engine while the log still holds the old one's options.
  $effect(() => {
    if (reported.reported && (reported.engine ?? thread.engine) === thread.engine) rememberOptions(thread.engine, reported.options);
  });
  const session = $derived(withKnownOptions(reported, thread.engine));

  // Another provider takes the thread over; the core hands the conversation to it with the next
  // message, and the model picked for it is applied when it starts.
  async function switchEngine(engine: string, model?: SettingValue) {
    await api.patchThread(thread.id, { engine });
    const option = modelsFor(engine);
    if (option && model !== undefined) await hub.setSetting(thread.id, option.id, model);
  }
  // Typing or picking a setting starts a stopped engine, so the message does not wait for it to
  // start and load its session. Asked at most once a minute; a running engine ignores it.
  let warmed = { id: "", at: 0 };
  function warm() {
    const now = Date.now();
    if (standIn || thread.status === "in_terminal" || (warmed.id === thread.id && now - warmed.at < 60_000)) return;
    warmed = { id: thread.id, at: now };
    quiet(api.warm(thread.id), "warm");
  }
  const busy = $derived(thread.status === "running" || thread.status === "waiting_on_you");
  // The judge's latest note (`judge.rs`), shown while the turn it was written in runs. A window
  // opened later asks for it once; an older hubd has none (compat-until: 2026-12-02).
  const aside = $derived.by(() => {
    const a = hub.asides[thread.id];
    return a && Date.now() - Number(a.at) < 10 * 60_000 ? a : undefined;
  });
  $effect(() => {
    const id = thread.id;
    if (!busy || hub.asides[id]) return;
    hubdHas("chat.aside") // compat-until: 2026-12-02
      .then((has) => (has ? api.aside(id) : null))
      .then((a) => {
        if (a && !hub.asides[id]) hub.asides[id] = a;
      })
      .catch(quietly("aside"));
  });
  let scroller: HTMLDivElement;
  // The commands the agent ran show under Running in the side column.
  const termShown = $derived(hub.panelOpen && !prefs.panelShut.includes("run"));
  let stick = true;

  /**
   * Messages on their way to the core, shown at once at the end of the transcript so Enter feels
   * instant; each goes when the core has it (and its real message is in the log), or on an error,
   * when the composer gets the text back. A message that waits behind a turn shows in the dock
   * instead, so it is only drawn here when the thread is idle.
   */
  let outgoing = $state<{ id: number; text: string; files: number; after: number }[]>([]);
  let outgoingId = 0;
  /** The ones whose real message has not reached the transcript yet. */
  const unsent = $derived(
    outgoing.filter((o) => !(hub.events[thread.id] ?? []).slice(o.after).some((e) => e.kind === "user_message" && payload(e, "user_message")?.text === o.text)),
  );

  // Follow new output unless the user scrolled up to read.
  $effect.pre(() => {
    void items.length;
    void outgoing.length;
    void hub.events[thread.id]?.length;
    if (!scroller) return;
    stick = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
  });
  $effect(() => {
    void hub.events[thread.id]?.length;
    void outgoing.length;
    if (stick && scroller) scroller.scrollTop = scroller.scrollHeight;
  });

  // Content that grows after it is drawn (the handover line under a model switch fills in, images
  // load) must not leave the view above the end: while following, stay pinned to the bottom.
  $effect(() => {
    const column = scroller?.querySelector(".column");
    if (!column || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      if (stick && scroller) scroller.scrollTop = scroller.scrollHeight;
    });
    ro.observe(column);
    return () => ro.disconnect();
  });

  // ---- New: what happened while you were away (the break in Transcript.svelte) ----

  /** The "New" break is above the part you are looking at: the jump button shows. */
  let newAbove = $state(false);
  const newReplies = $derived.by(() => {
    const at = items.findIndex((i) => i.kind === "new");
    return at < 0 ? 0 : items.slice(at).filter((i) => i.kind === "assistant").length;
  });
  const newMark = () => scroller?.querySelector<HTMLElement>("[data-new-mark]") ?? null;
  function checkNew() {
    const m = newMark();
    newAbove = !!m && !!scroller && m.getBoundingClientRect().bottom < scroller.getBoundingClientRect().top + 8;
  }
  function jumpNew() {
    const m = newMark();
    if (!m || !scroller) return;
    stick = false;
    scroller.scrollTo({ top: scroller.scrollTop + m.getBoundingClientRect().top - scroller.getBoundingClientRect().top - 60, behavior: "smooth" });
  }
  // Opening a thread with more new than a screen starts at the break, so it reads from there down;
  // with less, it stays at the bottom as always. Once, when the break first shows.
  let arrived = false;
  $effect(() => {
    void items.length;
    if (arrived || !scroller) return;
    tick().then(() => {
      const m = newMark();
      if (!m || arrived || !scroller) return;
      arrived = true;
      const delta = m.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
      if (scroller.scrollHeight - (scroller.scrollTop + delta) > scroller.clientHeight) {
        stick = false;
        scroller.scrollTop += delta - 60;
      }
      checkNew();
    });
  });
  $effect(() => {
    void items.length;
    tick().then(checkNew);
  });
  // Leaving is remembered, so the next visit knows where to start; a window hidden for a while
  // counts too, and the break moves to where you stopped looking.
  $effect(() => {
    const id = thread.id;
    let hiddenAt = 0;
    const visibility = () => {
      if (document.hidden) {
        hiddenAt = Date.now();
        left(id, hiddenAt);
      } else if (hiddenAt && Date.now() - hiddenAt >= 10_000) {
        hub.arrive(id, thread.seen_at);
        arrived = true;
      }
    };
    const gone = () => left(id);
    document.addEventListener("visibilitychange", visibility);
    window.addEventListener("pagehide", gone);
    return () => {
      document.removeEventListener("visibilitychange", visibility);
      window.removeEventListener("pagehide", gone);
      hub.leave(id);
    };
  });

  // ---- Ctrl F: find in this thread ----

  let finding = $state(false);
  let query = $state("");
  let findBar = $state<FindBar>();
  let ranges = $state<Range[]>([]);
  let at = $state(0);
  async function openFind() {
    const picked = window.getSelection()?.toString().trim();
    if (picked && !picked.includes("\n") && picked.length < 80) query = picked;
    finding = true;
    await tick();
    findBar?.focus();
  }
  function closeFind() {
    finding = false;
    query = "";
    mark([], 0);
    composer?.focus();
  }
  // Looked for again as the words change and as the thread grows; the place you are on is kept.
  $effect(() => {
    void hub.events[thread.id]?.length;
    const q = finding ? query : "";
    tick().then(() => {
      if (!scroller) return;
      const was = ranges[at];
      ranges = q.trim() ? findRanges(scroller, q) : [];
      const same = was ? ranges.findIndex((r) => r.compareBoundaryPoints(Range.START_TO_START, was) >= 0) : -1;
      at = same >= 0 ? same : 0;
      mark(ranges, at);
      if (ranges[at] && !was) go(0);
    });
  });
  function go(step: number) {
    if (!ranges.length) return;
    at = (at + step + ranges.length) % ranges.length;
    mark(ranges, at);
    stick = false;
    reveal(scroller, ranges[at]);
  }
  $effect(() => () => mark([], 0));
  function windowKey(e: KeyboardEvent) {
    const ctrl = (e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey;
    if (ctrl && e.code === "KeyF") {
      e.preventDefault();
      openFind();
    } else if (e.key === "F3" && finding) {
      e.preventDefault();
      go(e.shiftKey ? -1 : 1);
    } else if (e.key === "F2" && !e.ctrlKey && !e.altKey && !editing) {
      // F2 renames the open thread, as in a file explorer.
      e.preventDefault();
      startRename();
    }
  }

  // ---- Quote: select words in a reply, and a button puts them in the message box ----

  let quoting = $state<{ x: number; y: number; text: string } | null>(null);
  function selected() {
    const sel = window.getSelection();
    const words = sel?.toString().trim();
    if (!sel || sel.isCollapsed || !words || !sel.rangeCount) return (quoting = null);
    const reply = (n: Node | null) => (n instanceof Element ? n : n?.parentElement)?.closest(".assistant");
    const from = reply(sel.anchorNode);
    if (!from || from !== reply(sel.focusNode) || !scroller.contains(from)) return (quoting = null);
    const r = sel.getRangeAt(0).getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    quoting = { x: r.left / zoom + r.width / zoom / 2, y: r.top / zoom, text: words };
  }
  function quoteIt() {
    if (!quoting) return;
    composer?.quote(quoting.text);
    quoting = null;
    window.getSelection()?.removeAllRanges();
  }
  // What was sent here before, oldest first, for Arrow Up in the message box.
  const sent = $derived(
    items.flatMap((i) =>
      i.kind === "user" && !i.fromHub && !i.review && !i.terminal && i.text.trim() ? [{ text: i.text, attachments: i.attachments }] : [],
    ),
  );

  // Taking over: the core lets go of the session and the engine's own CLI resumes it in Hub's own
  // terminal, under Terminal in the side column; the thread comes back when the CLI exits, or
  // with Take back.
  const inTerminal = $derived(thread.status === "in_terminal");
  /** The thread works in a worktree Hub made for it (`.hub-threads/<id>` in the repository). */
  const ownTree = $derived(/[\\/]\.hub-threads[\\/]/.test(thread.cwd));
  let terminalNote = $state<string | null>(null);
  async function takeOverHere() {
    try {
      terminalNote = await takeOver(thread);
      if (terminalNote) setTimeout(() => (terminalNote = null), 2500);
    } catch (e) {
      hub.fail(e);
    }
  }

  // Branch and changed files, refreshed while the thread is open and after each turn.
  let git = $state<GitInfo | null>(null);
  $effect(() => {
    void thread.status;
    const id = thread.id;
    if (standIn) return;
    const load = () => quiet(api.git(id).then((g) => (git = g)), "git");
    load();
    const timer = poll(load, 10_000);
    return () => timer();
  });

  // Rename in place: click the title, Enter saves, Escape cancels.
  let editing = $state(false);
  let draft = $state("");
  function startRename() {
    if (standIn) return;
    draft = thread.title;
    editing = true;
  }
  // Rename picked in a right-click menu, here or in the sidebar.
  $effect(() => {
    if (pending.rename !== thread.id) return;
    pending.rename = null;
    startRename();
  });
  async function saveRename() {
    const title = draft.trim();
    editing = false;
    if (title && title !== thread.title) await hub.rename(thread.id, title);
  }
  function renameKey(e: KeyboardEvent) {
    if (e.key === "Enter") saveRename();
    else if (e.key === "Escape") editing = false;
  }
  function autofocus(el: HTMLInputElement) {
    el.focus();
    el.select();
  }

  function openMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    contextMenu.show(r.right / zoom - 180, r.bottom / zoom + 4, threadMenu(thread));
  }
  // Carry on in a fresh thread: the core writes a handoff from the log, Home opens with it.
  let handingOff = $state(false);
  let offerDismissed = $state(false);
  const contextFull = $derived(
    session.usage && session.usage.size ? session.usage.used / session.usage.size >= 0.8 : false,
  );
  async function handoff() {
    handingOff = true;
    try {
      const { text } = await api.handoff(thread.id);
      hub.startDraft(thread.cwd, text, thread.host ?? null);
    } catch (e) {
      hub.fail(e);
    } finally {
      handingOff = false;
    }
  }

  // Near a plan limit: offer the guard's cheaper model for this thread.
  const cheaper = $derived.by(() => {
    const want = hub.guard?.fallback_model.toLowerCase() ?? "sonnet";
    const o = session.options.find((o) => o.category === "model");
    const c = o?.choices.find((c) => `${c.value} ${c.name}`.toLowerCase().includes(want));
    return o && c && o.value !== c.value ? { option: o.id, choice: c } : null;
  });
  const quotaKey = $derived(hub.quota ? `${hub.quota.label}:${Math.floor(hub.quota.percent / 10)}` : null);

  // The project the thread belongs to, as the inbox names it, and its colour.
  const projectName = $derived.by(() => {
    const cwd = threadProject(thread);
    return (!thread.host && hub.projects.find((p) => p.path === cwd)?.name) || folderName(cwd);
  });
  const look = $derived(looks.project(threadProject(thread), thread.host, projectName));

  // A chat: no "Waiting on you" and no settle button, an answer is not a request.
  const chatHere = $derived(isChatThread(thread));

  // A Mac with a simulator running gets a phone button: the picture floats over the chat (simview.svelte.ts).
  $effect(() => sim.watch(thread.host ?? null));
  const simHere = $derived(sim.has(thread.host ?? null));
  const simCanStart = $derived(!simHere && sim.canStart(thread.host ?? null));
  const simStarting = $derived(!!sim.starting[thread.host ?? ""]);
  /** No simulator runs on that Mac: the phone button lists the ones it can start. */
  function startMenu(e: MouseEvent) {
    const v = sim.view(thread.host ?? null);
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    contextMenu.show(Math.max(4, r.right / zoom - 230), r.bottom / zoom + 4, [
      { label: "Start a simulator on this machine", heading: true },
      ...(v?.devices ?? []).slice(0, 12).map((d) => ({ label: d.name, hint: d.runtime, icon: "play" as const, run: () => void sim.start(thread.id, thread.host ?? null, d.udid) })),
    ]);
  }
  const cloud = $derived(cloudOf(thread));
  const simShown = $derived(!!sim.shown && sim.shown.thread === thread.id);
  const simLive = $derived(sim.view(thread.host ?? null)?.live.phase === "live");

  // A parent's title, short enough to sit in the header's meta line.
</script>

<svelte:window onkeydown={windowKey} />

<header use:menu={() => (standIn ? [] : threadMenu(thread))} use:drag>
  <div class="titles">
    <!-- Where you are, in colour, above the title: the project, the machine, how it stands. The
         folder's path is the project's tooltip, no longer a line of its own. -->
    <span class="meta">
      {#if isGuardChat(thread)}
        <button class="where" style:color="var(--muted)" data-tip="Hub's own guard of Problems, not a project. Open Problems" onclick={() => ((hub.view = "problems"), (hub.selected = null))}>
          <svg viewBox="0 0 24 24" aria-hidden="true" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"><path d={GLYPHS.shield} /></svg>Hub
        </button>
      {:else if chatHere}
        <!-- A chat is no project: its label opens nothing in Dev. -->
        <span class="where" style:color="var(--p-{look.color})"><ProjectIcon path={threadProject(thread)} host={thread.host} name={projectName} size={15} />{projectName}</span>
      {:else if thread.cwd}
        <button class="where" style:color="var(--p-{look.color})" data-tip="{thread.cwd}. Open the project page" onclick={() => hub.openProject(thread.cwd, thread.host ?? null)}>
          <ProjectIcon path={threadProject(thread)} host={thread.host} name={projectName} size={15} />{projectName}
        </button>
      {/if}
      {#if thread.cwd && isScratchRun(thread.cwd) && !chatHere && !standIn}
        <button class="crumb" data-tip="Make this a project" onclick={() => hub.makeProjectOf(thread)}>Make project</button>
      {/if}
      {#if cloud}<CloudMark {cloud} size={13} />{:else if hub.hosts.length > 1}<HostChip name={hub.hostOf(thread)} />{/if}
      {#if !chatHere || thread.status === "error"}<span class="state"><span class="dot {thread.status}"></span>{statusLabel[thread.status]}</span>{/if}
      {#if thread.owner && !isGuardChat(thread)}<OwnerMark owner={thread.owner} />{/if}
      {#if isGuardErrand(thread)}
        <button class="crumb" data-tip="The Guard's list of problems and its threads" onclick={() => ((hub.view = "problems"), (hub.selected = null))}>Open in Problems</button>
      {/if}
      {#if thread.parent}
        {@const parent = hub.threads.find((t) => t.id === thread.parent)}
        <button class="crumb" data-tip="Open the thread that started this side job" onclick={() => hub.open(thread.parent!)}>Side job of “{clip(parent?.title ?? "another thread", 32)}”</button>
      {/if}
      {#if git?.branch}
        <BranchChip {thread} branch={git.branch} onswitched={() => api.git(thread.id).then((g) => (git = g))} />
        {#if git.changed}<span class="changed" data-tip="Files with changes">{git.changed} changed</span>{/if}
        {#if ownTree}<span class="own" data-tip="Own worktree: {thread.cwd}">own worktree</span>{/if}
      {/if}
    </span>
    {#if editing}
      <input class="title-input" bind:value={draft} onkeydown={renameKey} onblur={saveRename} use:autofocus />
    {:else}
      <button class="title" data-tip="Rename" onclick={startRename}>{thread.title}</button>
    {/if}
  </div>
  {#if git?.branch && !inTerminal}<GitButton {thread} />{/if}
  {#if inTerminal}
    {#if hub.takeovers[thread.id]}
      <button class="ghost" data-tip="Show the terminal that has the thread" onclick={() => hub.showTakeover(thread.id, hub.takeovers[thread.id])}>
        <Icon name="terminal" />
      </button>
    {/if}
    <button
      class="btn"
      onclick={() => takeBack(thread).catch((e) => hub.fail(e))}
      data-tip={hub.takeovers[thread.id] ? "Bring the thread back to Hub; this closes the terminal that has it" : "Bring the thread back to Hub. Close the CLI in the terminal first, or it keeps writing to the same session."}
    >
      <Icon name="terminal" size={14} />Take back
    </button>
  {:else if thread.engine_session_id}
    <button class="btn" onclick={takeOverHere} data-tip="Open this session in the terminal. Or run: {attachCommand(thread)}">
      <Icon name="terminal" size={14} />{terminalNote ?? "Take over"}
    </button>
  {/if}
  {#if !thread.archived && !standIn && !chatHere}
    <button
      class="ghost"
      class:settled={!!thread.settled_at}
      data-tip={thread.settled_at ? "Settled: click to put it back in the list" : "Settle: done with it, on to the next"}
      aria-pressed={!!thread.settled_at}
      onclick={() => hub.settle(thread.id, !thread.settled_at)}
    >
      <Icon name="check" />
    </button>
  {/if}
  {#if simHere || simShown || simCanStart}
    <button
      class="ghost sim"
      class:on={simShown || simStarting}
      data-tip={simStarting ? "Starting the simulator…" : simShown ? "Hide the simulator" : simCanStart ? "Start an iPhone simulator on this machine" : "Show the iPhone simulator on this machine, live"}
      aria-pressed={simShown}
      disabled={simStarting}
      onclick={(e) => (simShown ? void sim.close() : simCanStart ? startMenu(e) : void sim.open(thread.id, thread.host ?? null))}
    >
      <Icon name="phone" />{#if simLive}<i class="live-dot"></i>{/if}
    </button>
  {/if}
  {#if ran.length}
    <button
      class="ghost"
      class:on={termShown}
      data-tip="Terminal (Ctrl §)"
      aria-pressed={termShown}
      onclick={() => hub.toggleTerminal()}
    >
      <Icon name="console" />
    </button>
  {/if}
  {#if !standIn}
    <button class="ghost" data-tip="Thread actions" aria-haspopup="menu" onclick={openMenu}>
      <Icon name="more" />
    </button>
  {/if}
  <button class="ghost" data-tip="Toggle panel (Ctrl .)" onclick={() => (hub.panelOpen = !hub.panelOpen)}>
    <Icon name="panel" />
  </button>
</header>

{#if finding}
  <div class="find-anchor">
    <FindBar bind:this={findBar} bind:query count={ranges.length} {at} onnext={() => go(1)} onprev={() => go(-1)} onclose={closeFind} />
  </div>
{/if}
{#if quoting}
  <button class="quote-btn" style:left="{quoting.x}px" style:top="{quoting.y}px" onpointerdown={(e) => e.preventDefault()} onclick={quoteIt} data-tip="Quote it in your reply">
    <Icon name="pen" size={12} />Quote
  </button>
{/if}
<div role="presentation" class="scroll" bind:this={scroller} onpointerup={() => setTimeout(selected)} onkeyup={(e) => e.shiftKey && selected()} onscroll={() => { quoting = null; stick = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80; checkNew(); }}>
  {#if newAbove}
    <div class="new-jump"><button onclick={jumpNew} data-tip="Back to where what you missed starts"><span class="dot"></span><Icon name="arrowUp" size={12} />New{newReplies ? ` · ${newReplies} ${newReplies === 1 ? "reply" : "replies"}` : ""}</button></div>
  {/if}
  <div class="column">
    {#if cloud}<CloudCard {thread} {cloud} />{/if}
    {#if attempt && !attempt.done}<AttemptBar {thread} start={attempt} />{/if}
    <Transcript {items} threadId={thread.id} cwd={thread.cwd} host={thread.host} live={thread.status === "running"} find={finding ? query : ""} />
    {#if !busy}
      {#each unsent as o (o.id)}
        <div class="outgoing" data-tip="Sending…">
          {#if o.files}<span class="files"><Icon name="clip" size={11} />{o.files}</span>{/if}
          {#if o.text}<div class="bubble">{o.text}</div>{/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<div class="dock">
  {#if busy && aside}
    <AsideLine {aside} />
  {/if}
  {#if busy}
    <StatusLine
      {items}
      engine={thread.engine ? (engines.labels[thread.engine] ?? engineProvider(thread.engine).name) : undefined}
      waiting={thread.status === "waiting_on_you"}
      word={detailOf(thread.id) === "minimal"}
      detail={detailOf(thread.id)}
      seed={thread.id}
      sentAt={thread.sent_at}
      onterminal={(id) => hub.showCommand(id)}
      onstop={standIn ? undefined : () => api.cancel(thread.id).catch((e) => hub.fail(e))}
    />
  {/if}
  {#if note}
    <div class="offer">{note}</div>
  {:else if thread.status === "waiting_on_you"}
    <div class="offer">Answer with the number keys</div>
  {:else if handingOff || pending.handoff === thread.id}
    <div class="offer">Writing a handoff for a fresh thread…</div>
  {:else if hub.quota && quotaKey !== hub.quotaAcked}
    <div class="offer" class:hot={hub.quota.percent >= 95}>
      <Icon name="gauge" size={13} />
      {hub.quota.label} at {Math.round(hub.quota.percent)}%{hub.quota.resets ? `, resets ${resets(hub.quota.resets)}` : ""}.
      {#if cheaper}
        <button class="link" onclick={() => hub.setSetting(thread.id, cheaper.option, cheaper.choice.value)}>Switch this thread to {cheaper.choice.name}</button>
      {/if}
      {#if hub.guard?.switch_at != null && hub.quota.percent < hub.guard.switch_at}
        <span>Hub switches at {hub.guard.switch_at}%.</span>
      {/if}
      <button class="link quiet" onclick={() => (quotaKey && hub.ackQuota(quotaKey))}>OK</button>
    </div>
  {:else if inTerminal}
    <div class="offer">
      <Icon name="terminal" size={13} />
      Open in a terminal. It comes back here when you exit the CLI there.
      <button class="link quiet" onclick={() => navigator.clipboard.writeText(attachCommand(thread))} data-tip={attachCommand(thread)}>Copy attach command</button>
    </div>
  {:else if contextFull && !offerDismissed && !busy}
    <div class="offer">
      Context is {Math.round(((session.usage?.used ?? 0) / (session.usage?.size ?? 1)) * 100)}% full.
      <button class="link" onclick={handoff}>Continue in a new thread</button>
      <button class="link quiet" onclick={() => (offerDismissed = true)}>Not now</button>
    </div>
  {/if}
  <Composer
    bind:this={composer}
    placeholder={busy ? (prefs.steer ? "Steer: stops the turn, this goes next…" : "Queue a follow-up…") : hub.isDesk(thread) ? "Tell the desk what to do, or ask how it's going…" : "Reply…"}
    {busy}
    steer={prefs.steer}
    engine={thread.engine}
    onnote={hubNote}
    {session}
    inThread
    threadId={thread.id}
    webStart={startedAsWeb(items)}
    cwd={thread.cwd}
    host={thread.host}
    history={sent}
    design={designing}
    ondesign={standIn ? undefined : (on) => api.design(thread.id, on).catch((e) => hub.fail(e))}
    ondesignreview={async () => {
      await api.reviewDesign(thread.id);
    }}
    idleSince={hub.events[thread.id]?.findLast((e) => e.kind !== "reaction")?.ts}
    {started}
    onopenstarted={() => started && hub.open(started.id)}
    onsubmit={async (text, attachments, original, how) => {
      // Ctrl Enter: a new thread in this folder, started in the background; this one stays as it is.
      if (how?.next) {
        const o = session.options.filter((o) => o.category !== "mode" && o.id !== "mode");
        const t = await hub.newThread(thread.cwd, text, attachments, Object.fromEntries(o.map((o) => [o.id, o.value])), original, thread.host ?? undefined, undefined, thread.engine, false);
        started = { id: t.id, title: t.title };
        clearTimeout(startedTimer);
        startedTimer = setTimeout(() => (started = null), 8000);
        return;
      }
      const id = ++outgoingId;
      outgoing = [...outgoing, { id, text, files: attachments.length, after: hub.events[thread.id]?.length ?? 0 }];
      const drop = () => (outgoing = outgoing.filter((o) => o.id !== id));
      try {
        await hub.send(thread.id, text, attachments, original, busy && prefs.steer !== !!how?.other);
        // The core has it; its own message shows as soon as the event arrives.
        setTimeout(drop, 3000);
      } catch (e) {
        drop();
        throw e;
      }
    }}
    oncancel={() => (standIn ? undefined : api.cancel(thread.id).catch((e) => hub.fail(e)))}
    onsetting={(id, value) => {
      hub.setSetting(thread.id, id, value);
      warm();
    }}
    onengine={switchEngine}
    ontyping={warm}
    findFiles={(q) => api.files(thread.id, q)}
    onerror={(e) => hub.fail(e)}
  >
    <ComposerDock threadId={thread.id} {items} {queue} idle={thread.status === "idle"} />
  </Composer>
</div>


<style>
  .outgoing { display: flex; flex-direction: column; align-items: flex-end; gap: 3px; margin: 18px 0 10px; opacity: 0.6; }
  .outgoing .bubble {
    background: var(--surface-hover); color: var(--text); padding: 8px 14px;
    border-radius: var(--r-lg); max-width: 80%; white-space: pre-wrap; font-size: calc(14.5px * var(--chat-scale));
  }
  .outgoing .files { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; }
  header { position: relative; container: thread-head / inline-size; display: flex; align-items: center; gap: 10px; padding: 14px calc(20px + var(--caption-room, 0px)) 12px 28px; border-bottom: 1px solid var(--line); }
  .titles { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .title { color: var(--text); font-weight: 600; font-size: 16px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; text-align: left; max-width: 100%; border-radius: var(--r-sm); padding: 0 4px; margin-left: -4px; }
  .title:hover { background: var(--surface); }
  .title-input { background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-sm); padding: 1px 6px; margin-left: -8px; font-size: 16px; font-weight: 600; outline: none; }
  .meta { display: flex; align-items: center; gap: 10px; color: var(--muted); font-size: 13px; min-width: 0; white-space: nowrap; }
  .where { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; font-weight: 600; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .where:hover { text-decoration: underline; text-underline-offset: 2px; }
  .state { display: inline-flex; align-items: center; gap: 6px; }
  .changed { color: var(--amber); white-space: nowrap; }
  .own { color: var(--muted); white-space: nowrap; }
  .ghost { color: var(--muted); display: grid; place-items: center; padding: 4px; border-radius: var(--r-sm); }
  .ghost:hover { color: var(--text); background: var(--surface-hover); }
  .ghost.settled, .ghost.on { color: var(--accent); }
  .ghost.sim { position: relative; }
  .live-dot { position: absolute; top: 3px; right: 3px; width: 6px; height: 6px; border-radius: 50%; background: var(--green); }
  .find-anchor { position: relative; height: 0; }
  .quote-btn {
    position: fixed; z-index: 30; transform: translate(-50%, calc(-100% - 6px)); display: inline-flex; align-items: center; gap: 4px;
    padding: 4px 8px; font-size: 12px; color: var(--text-2); background: var(--surface-2);
    border: 1px solid var(--line-strong); border-radius: var(--r-md); box-shadow: 0 6px 18px rgba(0, 0, 0, 0.3);
  }
  .quote-btn:hover { color: var(--text); background: var(--surface-hover); }
  .scroll { flex: 1; overflow-y: auto; min-height: 0; }
  /* Zero height and sticky: the button floats over the top of the chat without moving it. */
  .new-jump { position: sticky; top: 10px; height: 0; z-index: 3; display: flex; justify-content: center; pointer-events: none; }
  .new-jump button { pointer-events: auto; display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 12px; border-radius: 99px; background: var(--surface-hover); border: 1px solid var(--line-strong); color: var(--text-3); font-size: 12px; box-shadow: 0 4px 14px rgba(0, 0, 0, 0.35); }
  .new-jump button:hover { color: var(--text); }
  .new-jump .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--p-orange); }
  .column { max-width: var(--chat); margin: 0 auto; padding: 12px 28px 24px; }
  /* A notice is a tab on top of the composer, attached to it. */
  .offer {
    position: relative; z-index: 1; display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
    margin: 0 14px -1px; padding: 4px 12px; color: var(--muted); font-size: 13px;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-bottom: 0; border-radius: var(--r-md) var(--r-md) 0 0;
  }
  .offer .link.quiet:last-child { margin-left: auto; }
  .offer.hot { color: var(--amber); }
  .link { color: var(--accent); font-size: 13px; padding: 1px 6px; border-radius: var(--r-sm); }
  .link:hover { background: var(--accent-bg); }
  .link.quiet { color: var(--muted); }
  .link.quiet:hover { color: var(--text); background: var(--surface-hover); }
  .dock { max-width: var(--chat); width: 100%; margin: 0 auto; padding: 8px 28px 16px; }
  .crumb { color: inherit; font-size: inherit; }
  .crumb:hover { color: var(--text); text-decoration: underline; text-underline-offset: 2px; }
</style>
