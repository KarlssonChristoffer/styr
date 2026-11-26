<!-- A thread's unsent text is kept, and files dropped on its row in the sidebar come along. The composer is made again for each thread, so the draft is read once.
     Used by App.svelte, Home.svelte, ProjectView.svelte, ThreadView.svelte. -->
<script lang="ts">
  import { ownsDictationKey } from "../dictationScope";
  import { poll } from "../poll";
  import { onDestroy, untrack } from "svelte";
  import { pastedFile, THREAD_MIME } from "../attachments";
  import { isBigPaste, putInText, readFiles } from "../composerFiles";
  import { recentSent, rememberSent, type Sent } from "../recent";
  import type { Attachment } from "../bindings/Attachment";
  import type { FileMatch } from "../bindings/FileMatch";
  import type { SettingValue } from "../bindings/SettingValue";
  import { emptySession, type ConfigOption, type Session } from "../session";
  import { ComposerVoice } from "../composerVoice.svelte";
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import { drafts, dropped, filesOf, setDraft, setDraftFiles, takeDropped } from "../drafts.svelte";
  import { stash } from "../stash.svelte";
  import { Recall, StashList, type Hooks } from "../composerHistory.svelte";
  import ComposerStash from "./ComposerStash.svelte";
  import ComposerPlus from "./ComposerPlus.svelte";
  import MicMenu from "./MicMenu.svelte";
  import SuggestionList from "./SuggestionList.svelte";
  import { Suggest } from "../suggest.svelte";
  import type { ThreadReach } from "../bindings/ThreadReach";
  import { typed, typing } from "../typing.svelte";
  import {
    engineProvider,
    engines,
    favoriteKey,
    favorites,
    modelsFor,
    harnessLabel,
    tabsOf,
    hostEngines,
    loadEngines,
    loadHostEngines,
    loadReady,
    notReady,
    fixEngine,
    missingProviders,
    providerOf,
    toggleFavorite,
    harnessFacts,
    loadHarnessFacts,
    routeEfforts,
  } from "../providers.svelte";
  import AttachmentRow from "./AttachmentRow.svelte";
  import ReachRow from "./ReachRow.svelte";
  import CompactChip from "./CompactChip.svelte";
  import { attachDiff, hubCommandsFor, type CommandBox } from "../composerCommands";
  import ConfigControls from "./ConfigControls.svelte";
  import DesignControls from "./DesignControls.svelte";
  import ComposerTabs from "./ComposerTabs.svelte";
  import UsageControl from "./UsageControl.svelte";
  import WhoControl from "./WhoControl.svelte";
  import { configOf } from "../composerConfig";
  import Icon from "./Icon.svelte";
  import TidyBar from "./TidyBar.svelte";
  import { webAsk } from "../web";

  let {
    placeholder,
    busy = false,
    steer = false,
    onnote,
    engine = "claude",
    session = emptySession(),
    inThread = false,
    threadId,
    cwd,
    host,
    started = null,
    onopenstarted,
    onsubmit,
    oncancel,
    onsetting,
    onengine,
    ontyping,
    findFiles,
    onerror,
    history = [],
    idleSince,
    children,
    design = false,
    ondesign,
    ondesignreview,
    webStart = false,
    reach = $bindable({}),
  }: {
    placeholder: string;
    busy?: boolean;
    /** Sending while busy steers (stops the turn, the message goes next) rather than queues. */
    steer?: boolean;
    /** Where Hub's short notes go, such as a big paste turned into a file. Without it they show in the bar. */
    onnote?: (what: string) => void;
    engine?: string;
    session?: Session;
    /** Usage and slash commands need a thread to run in. */
    inThread?: boolean;
    threadId?: string;
    /** The thread began as a web search: the box starts with web search on, so follow-ups search too. */
    webStart?: boolean;
    /** Extra folders and connections off for this thread (`thread.reach`); a new thread's box hands it to the thread it starts, a thread's own box keeps it in the core. */
    reach?: ThreadReach;
    /** Folder the engine works in; also where a long dump is tidied. */
    cwd?: string;
    /** The machine `cwd` is on, when it is not this one. */
    host?: string | null;
    /** A thread Ctrl Enter just started in the background: a tab on the box, "Open" goes there. */
    started?: { id: string; title: string } | null;
    onopenstarted?: () => void;
    /** `original` is set when the text is a tidied version of what the user wrote; `how.next` when
     *  Ctrl Enter sent it, to start a thread of its own in the background; `how.other` asks for the
     *  other of queue and steer (Alt Enter). */
    onsubmit: (text: string, attachments: Attachment[], original?: string, how?: { next?: boolean; other?: boolean }) => Promise<void> | void;
    oncancel?: () => void;
    onsetting?: (configId: string, value: SettingValue) => void;
    /** Moves to another engine, with `model` picked on it when known. */
    onengine?: (engine: string, model?: SettingValue) => Promise<void> | void;
    /** Text was typed; a message is likely on its way. */
    ontyping?: () => void;
    findFiles?: (query: string) => Promise<FileMatch[]>;
    onerror?: (e: unknown) => void;
    /** What was sent in this thread before, oldest first, for Arrow Up. Outside a thread, Arrow Up
     *  goes through the prompts you started threads with. */
    history?: Sent[];
    /** When the thread last did anything, for the compact offer on coming back to it. */
    idleSince?: number;
    children?: import("svelte").Snippet;
    /** Design mode (`design.rs`): the agent writes a design document and no code. */
    design?: boolean;
    /** Shows the Design switch; turns design mode on or off. */
    ondesign?: (on: boolean) => void;
    /** In design mode, has fresh readers review the design document. */
    ondesignreview?: () => Promise<void> | void;
  } = $props();

  // A thread's unsent text is kept, and files dropped on its row in the sidebar come along.
  // The composer is made again for each thread, so the draft is read once.
  let text = $state(untrack(() => (inThread && threadId ? (drafts[threadId] ?? "") : "")));
  let attachments = $state<Attachment[]>(untrack(() => (inThread && threadId ? [...filesOf(threadId)] : [])));
  $effect(() => {
    if (inThread && threadId) setDraft(threadId, text);
  });
  $effect(() => {
    if (inThread && threadId) setDraftFiles(threadId, $state.snapshot(attachments) as Attachment[]);
  });
  $effect(() => {
    if (inThread && threadId && dropped[threadId]?.length) attachments.push(...takeDropped(threadId));
  });
  // Files picked from the @ menu. Only those still named in the text are sent.
  let mentions = $state<FileMatch[]>([]);
  let sending = $state(false);
  // What the user wrote before tidying it, sent along so the log keeps it.
  let original = $state<string | null>(null);
  $effect(() => {
    if (!text.trim()) original = null;
  });
  let box: HTMLTextAreaElement;
  let picker: HTMLInputElement;
  let root: HTMLDivElement;

  export function focus() {
    box?.focus();
  }

  /** Adds an attachment shared from elsewhere, such as a picture an agent made. */
  export function attach(a: Attachment) {
    if (a.mime.startsWith("image/") && !session.images) return onerror?.(new Error("This engine does not take images"));
    if (!attachments.some((b) => b.name === a.name && b.data === a.data)) attachments.push(a);
    queueMicrotask(() => box?.focus());
  }

  /** Quotes `words` from a reply as a Markdown quote, with room under it to say something about it. */
  export function quote(words: string) {
    const quoted = words.trim().split(/\r?\n/).map((l) => (l.trim() ? "> " + l : ">")).join("\n");
    const before = text.trimEnd();
    text = (before ? before + "\n\n" : "") + quoted + "\n\n";
    queueMicrotask(() => {
      box?.focus();
      box.selectionStart = box.selectionEnd = text.length;
      box.scrollTop = box.scrollHeight;
    });
  }

  /** Puts text in the box to edit before sending. */
  export function fill(value: string, end = false) {
    text = value;
    queueMicrotask(() => {
      box?.focus();
      box.selectionStart = box.selectionEnd = end ? value.length : 0;
      if (!end) box.scrollTop = 0;
    });
  }

  const config = $derived(configOf(session));
  const options = $derived(config.options);
  const mode = $derived(config.mode);
  const model = $derived(config.model);
  const effort = $derived(config.effort);
  const toggles = $derived(config.toggles);
  const others = $derived(config.others);


  // ---- Compact on return ----
  // Back at a long thread after the engine's prompt cache has gone cold (about an hour on a Claude
  // plan), the whole conversation is read again from scratch, which eats the plan. A chip by the
  // send button offers to summarise first, as in T3 Code; on by default, a click keeps it all.
  const COLD_MS = 60 * 60 * 1000;
  const BIG = 100_000;
  let now = $state(Date.now());
  $effect(() => {
    const timer = poll(() => (now = Date.now()), 60_000);
    return () => timer();
  });
  const offerCompact = $derived(
    inThread && !busy && !!idleSince && now - idleSince > COLD_MS && (session.usage?.used ?? 0) >= BIG && session.commands.some((c) => c.name === "compact"),
  );
  let keepFull = $state(false);

  const canSend = $derived((!!text.trim() || attachments.length > 0) && !sending);

  // ---- Web search ----
  // On, a message goes with Hub's instruction to search the web for it (`web.ts`); the thread
  // shows your words with a globe. It stays on until you turn it off, as a chat goes on, and a
  // thread begun as a web search starts with it on (`webStart`).
  let webPick = $state<{ id?: string; on: boolean } | null>(null);
  // Not in the Chat world: a chat may always use the internet (its instructions say so, `hub_tools::CHAT_STYLE`),
  // so it has no button for it and never sends the instruction along.
  const inChat = $derived(hub.world === "chat");
  const web = $derived(!inChat && (webPick && webPick.id === threadId ? webPick.on : webStart));

  async function submit(how: { next?: boolean; other?: boolean } = {}) {
    if (sending) return;
    // Enter while dictating: the words just said go with the message.
    if (voice.state !== "idle") {
      sending = true;
      try {
        await voice.settle();
      } finally {
        sending = false;
      }
    }
    if (!canSend) return;
    const value = text.trim();
    const linked = mentions
      .filter((m) => text.includes("@" + m.rel))
      .map((m): Attachment => ({ name: m.rel, mime: m.dir ? "inode/directory" : "text/plain", path: m.path }));
    // A thread named with @ goes as a short note; the agent reads the thread itself if it needs to.
    // Picked from the menu, or pasted (Ctrl Shift C copies a thread as its @ name).
    const named = [...threadRefs, ...hub.threads.filter((t) => hub.inWorld(t) && t.id !== threadId && !threadRefs.some((r) => r.id === t.id))];
    const threads = named
      .filter((t) => t.title.trim().length > 2 && text.includes("@" + t.title))
      .map((t): Attachment => ({
        name: t.title,
        mime: THREAD_MIME,
        text: `Hub thread “${t.title}” (id ${t.id}). Read it with Hub's read_thread tool (thread_id "${t.id}") when you need what was said or done there.`,
      }));
    sending = true;
    // The box empties at once, so Enter feels instant however long the core takes to answer
    // (it may be starting up after an update); an error puts everything back.
    const kept = { text, original, attachments, mentions, threadRefs, keepFull };
    const files = [...attachments, ...linked, ...threads];
    const compact = offerCompact && !keepFull;
    // What was said is sent; the next message starts with the mic off.
    voice.drop();
    text = "";
    original = null;
    attachments = [];
    peek = null;
    mentions = [];
    threadRefs = [];
    recall.at = -1;
    keepFull = false;
    try {
      if (compact) await onsubmit("/compact", []);
      await onsubmit(web && value && !value.startsWith("/") ? webAsk(value) : value, files, kept.original ?? undefined, how);
      if (!inThread) rememberSent(value, $state.snapshot([...kept.attachments, ...linked]) as Attachment[]);
    } catch {
      // The error is shown by the caller; give the text back so nothing is lost, unless
      // something new was written meanwhile.
      if (!text.trim() && !attachments.length) {
        ({ text, original, attachments, mentions, threadRefs, keepFull } = kept);
      }
    } finally {
      sending = false;
      queueMicrotask(() => box?.focus());
    }
  }

  // ---- Attachments: picker, paste and drop ----

  const addFiles = (files: Iterable<File>) => readFiles(files, session.images, (a) => attachments.push(a), onerror);

  // A paste of 32 KB or 300 lines or more (a log, a stack trace) goes along as a text file instead
  // of filling the box, as in T3 Code. Ctrl Shift V keeps it as text, and so does "Put in the box"
  // on its chip.
  let plainPaste = false;
  let pastes = 0;
  function paste(e: ClipboardEvent) {
    const files = [...(e.clipboardData?.files ?? [])];
    if (files.length) {
      e.preventDefault();
      addFiles(files);
      return;
    }
    const pasted = e.clipboardData?.getData("text/plain") ?? "";
    const keep = plainPaste;
    plainPaste = false;
    if (keep || !isBigPaste(pasted)) return;
    e.preventDefault();
    pastes++;
    attachments.push(pastedFile(pasted, pastes));
    flash("Big paste attached as a file · click it to look, Ctrl Shift V pastes as text");
  }

  /** A short note in the bar, such as where a put-away prompt went. */
  let note = $state("");
  let noteTimer: ReturnType<typeof setTimeout> | undefined;
  function flash(what: string) {
    if (onnote) return onnote(what);
    note = what;
    clearTimeout(noteTimer);
    noteTimer = setTimeout(() => (note = ""), 4000);
  }

  // ---- A look at an attachment before it goes ----

  /** The attachment opened from its chip: a picture big, a text file's text. */
  let peek = $state<number | null>(null);
  function unattach(i: number) {
    attachments.splice(i, 1);
    peek = null;
  }
  /** A text file goes back into the box as text, where the cursor is. */
  function inline(i: number) {
    const a = attachments[i];
    if (!a?.text) return;
    const at = box?.selectionStart ?? text.length;
    const put = putInText(text, at, box?.selectionEnd ?? at, a.text);
    text = put.text;
    unattach(i);
    queueMicrotask(() => {
      box?.focus();
      box.selectionStart = box.selectionEnd = put.caret;
    });
  }

  // ---- Arrow Up brings back what you sent, Ctrl S puts a prompt away ----

  const historyHooks: Hooks = {
    text: () => text,
    setText: (t) => (text = t),
    attachments: () => attachments,
    setAttachments: (a) => (attachments = a),
    addAttachment: (a) => attachments.push(a),
    box: () => box,
    past: () => (inThread ? history : recentSent()),
    clearPeek: () => (peek = null),
    clearOriginal: () => (original = null),
    clearSuggestions: () => suggest.clear(),
    flash: (what) => flash(what),
  };
  const recall = new Recall(historyHooks);
  const stashList = new StashList(historyHooks);

  let dragging = $state(false);
  function drop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    if (e.dataTransfer?.files.length) addFiles([...e.dataTransfer.files]);
  }

  // ---- Suggestions: / commands and @ files ----

  /** Threads picked from the @ menu; only those still named in the text are sent. */
  let threadRefs = $state<{ id: string; title: string }[]>([]);

  // `hubCommands` is declared further down; the host only reads it once the box is in use.
  const suggest = new Suggest({
    text: () => text,
    setText: (t) => (text = t),
    caret: () => box?.selectionStart ?? null,
    focusAt: (at) => {
      box.focus();
      box.selectionStart = box.selectionEnd = at;
    },
    threadId: () => threadId,
    inThread: () => inThread,
    hubCommands: () => hubCommands,
    engineCommands: () => session.commands,
    findFiles: () => findFiles,
    mention: (m) => {
      if (!mentions.some((x) => x.path === m.path)) mentions.push(m);
    },
    refer: (t) => {
      if (!threadRefs.some((x) => x.id === t.id)) threadRefs.push(t);
    },
  });

  /** Names a thread in the text, as @ does, so the agent is told where to read it. */
  function addThread(t: { id: string; title: string }) {
    if (!threadRefs.some((x) => x.id === t.id)) threadRefs.push({ id: t.id, title: t.title });
    const at = box?.selectionStart ?? text.length;
    const before = text.slice(0, at);
    const gap = before && !/\s$/.test(before) ? " " : "";
    const inserted = `${gap}@${t.title} `;
    text = before + inserted + text.slice(at);
    menu = null;
    queueMicrotask(() => {
      box?.focus();
      box.selectionStart = box.selectionEnd = before.length + inserted.length;
    });
  }

  function keydown(e: KeyboardEvent) {
    const ctrl = (e.ctrlKey || e.metaKey) && !e.altKey;
    if (e.key === "Escape" && peek !== null) {
      e.preventDefault();
      peek = null;
      return;
    }
    if (ctrl && !e.shiftKey && e.code === "KeyS") {
      e.preventDefault();
      stashList.key();
      return;
    }
    if (stashList.open && stashList.keys(e)) return;
    if (ctrl && e.shiftKey && e.code === "KeyV") plainPaste = true;
    if (!suggest.list.length && (e.key === "ArrowUp" || e.key === "ArrowDown") && recall.step(e)) {
      e.preventDefault();
      return;
    }
    if (suggest.key(e)) return;
    if (e.key === "Tab" && e.shiftKey) {
      // Shift Tab cycles the permission mode, as in Claude Code.
      const mode = options.find((o) => o.category === "mode" || o.id === "mode");
      if (mode?.choices.length) {
        e.preventDefault();
        const i = mode.choices.findIndex((c) => c.value === mode.value);
        choose(mode, mode.choices[(i + 1) % mode.choices.length].value);
      }
    } else if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      // Ctrl Enter starts a new thread in the background, as T3 Code's "start in the background".
      e.preventDefault();
      submit({ next: ctrl, other: e.altKey });
    } else if (e.key === "Escape" && busy && oncancel) {
      oncancel();
    } else if (e.key === "Backspace" && !text && attachments.length) {
      unattach(attachments.length - 1);
    }
  }

  // Grow with the text up to a limit.
  $effect(() => {
    void text;
    if (!box) return;
    box.style.height = "auto";
    box.style.height = Math.min(box.scrollHeight, 260) + "px";
  });

  // ---- Settings menus ----

  let menu = $state<string | null>(null);
  let who = $state<WhoControl>();
  let reachRow = $state<ReachRow>();
  function choose(option: ConfigOption, value: SettingValue, close = true) {
    if (close) menu = null;
    onsetting?.(option.id, value);
  }
  function closeMenus(e: PointerEvent) {
    if (menu && !(e.target as Element).closest?.(".menu-wrap")) menu = null;
    if (peek !== null && !(e.target as Element).closest?.(".chips")) peek = null;
    if (stashList.open && !(e.target as Element).closest?.(".stash")) stashList.open = false;
  }

  // ---- Usage ----

  const usage = $derived(session.usage);
  const has = (name: string) => session.commands.some((c) => c.name === name);
  async function run(command: string) {
    menu = null;
    try {
      await onsubmit(command, []);
    } catch {
      // Shown by the caller.
    }
  }

  // ---- Hub's own commands ----
  // Typed like the engine's slash commands; they run here instead of going to the engine.

  let tidyBar = $state<TidyBar>();

  const commandBox = (): CommandBox => ({
    cwd,
    host,
    threadId,
    inThread,
    attach: (a) => attachments.push(a),
    text: () => text,
    tidy: () => tidyBar?.tidyNow(),
    onerror,
  });
  const hubCommands = $derived(hubCommandsFor(commandBox()));

  // ---- Voice ----
  // Dictation is `ComposerVoice` (composerVoice.svelte.ts); the box is what it writes into.
  const voice = new ComposerVoice({
    get text() {
      return text;
    },
    set text(v: string) {
      text = v;
    },
    get caret() {
      return caretAt();
    },
    element: () => box,
    cwd: () => cwd,
    onerror: (e) => onerror?.(e),
  });
  const pending = $derived.by(() => voice.pending(text));
  let mirror = $state<HTMLDivElement>();
  $effect(() => {
    void pending;
    if (mirror && box) {
      mirror.style.width = box.clientWidth + "px";
      mirror.scrollTop = box.scrollTop;
    }
  });

  function caretAt() {
    return box && document.activeElement === box ? box.selectionStart : text.length;
  }

  function globalKey(e: KeyboardEvent) {
    // Esc closes a look at an attachment wherever the focus is, such as on its chip.
    if (e.key === "Escape" && peek !== null && !e.defaultPrevented) {
      e.preventDefault();
      peek = null;
      return;
    }
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "u") {
      e.preventDefault();
      picker.click();
      return;
    }
    const ctrl = (e.ctrlKey || e.metaKey) && !e.altKey;
    if (ctrl && e.shiftKey && e.code === "KeyM") {
      e.preventDefault();
      who?.openModels();
      return;
    }
    if (ctrl && e.shiftKey && e.code === "KeyE" && (effort || toggles.length || others.length)) {
      e.preventDefault();
      menu = menu === "effort" ? null : "effort";
      return;
    }
    if (ctrl && e.shiftKey && e.code === "KeyA" && mode) {
      e.preventDefault();
      menu = menu === "mode" ? null : "mode";
      return;
    }
    if (menu === "model" && ctrl && !e.shiftKey && /^Digit[1-9]$/.test(e.code)) {
      if (who?.pickNth(Number(e.code.slice(5)))) {
        e.preventDefault();
        return;
      }
    }
    // A menu that handled Escape itself (closing the picker's search) stays open.
    if (e.key === "Escape" && menu && !e.defaultPrevented) {
      menu = null;
      return;
    }
    if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === "d") {
      const active = document.activeElement;
      const owns = ownsDictationKey({
        handled: e.defaultPrevented,
        recording: voice.state === "recording",
        focusInside: !!active && root.contains(active),
        focusElsewhere: !!active && active !== document.body && active !== document.documentElement,
        shown: root.getClientRects().length > 0,
      });
      if (!owns) return;
      e.preventDefault();
      voice.toggle();
    }
  }

  onDestroy(() => voice.destroy());
</script>

<svelte:window onpointerdown={closeMenus} onkeydown={globalKey} />

<div role="presentation"
  class="composer"
  class:dragging
  bind:this={root}
  ondragover={(e) => {
    if (e.dataTransfer?.types.includes("Files")) {
      e.preventDefault();
      dragging = true;
    }
  }}
  ondragleave={(e) => {
    if (!root.contains(e.relatedTarget as Node)) dragging = false;
  }}
  ondrop={drop}
>
  {@render children?.()}

  <SuggestionList {suggest} />

  <ComposerStash list={stashList} />

  <ComposerTabs {started} {onopenstarted} {stashList} />

  <ReachRow bind:this={reachRow} bind:reach {threadId} {onerror} />

  <AttachmentRow {attachments} bind:peek {threadId} {cwd} onremove={unattach} oninline={inline} {onerror} />

  <TidyBar
    bind:this={tidyBar}
    {text}
    {cwd}
    {threadId}
    {onerror}
    onuse={(tidy, from) => {
      original ??= from;
      text = tidy;
      queueMicrotask(() => box?.focus());
    }}
  />

  <div class="field" class:dictating={!!pending}>
    {#if pending}
      <div class="mirror" bind:this={mirror} aria-hidden="true">{#each pending as part, i (i)}{#if part.grey}<span class="heard">{part.text}</span>{:else}{part.text}{/if}{/each}</div>
    {/if}
    <textarea
      bind:this={box}
      bind:value={text}
      {placeholder}
      rows="1"
      onkeydown={keydown}
      onfocus={() => {
        typing.focused = true;
        voice.warm();
      }}
      oninput={() => {
        typed();
        recall.edited(text);
        suggest.refresh();
        if (text.trim()) ontyping?.();
      }}
      onclick={() => suggest.refresh()}
      onblur={() => {
        typing.focused = false;
        setTimeout(() => suggest.clear(), 120);
      }}
      onpaste={paste}
      onscroll={() => {
        if (mirror && box) mirror.scrollTop = box.scrollTop;
      }}
    ></textarea>
  </div>

  <div class="bar">
    <ComposerPlus
      bind:menu
      {threadId}
      {engine}
      images={session.images}
      {web}
      {cwd}
      {host}
      {reach}
      onreach={(next) => reachRow!.change(next)}
      onfiles={() => picker.click()}
      onbag={(items) => {
        for (const a of items) if (!attachments.some((b) => b.name === a.name && b.data === a.data && b.url === a.url)) attachments.push(a);
      }}
      onthread={addThread}
      ondiff={() => attachDiff(commandBox())}
      onweb={(on) => (webPick = { id: threadId, on })}
      onfocus={() => box?.focus()}
    />
    <WhoControl bind:this={who} {session} {engine} {host} {inThread} {busy} bind:menu {onengine} {onerror} onchoose={choose} onfocus={() => box?.focus()} />

    <ConfigControls {session} {engine} {host} bind:menu onchoose={choose} onflash={flash} />

    {#if ondesign}
      <span class="sep"></span>
      <DesignControls {design} {ondesign} {ondesignreview} {onerror} />
    {/if}

    {#if note}
      <span class="status">{note}</span>
    {/if}
    <span class="grow"></span>

    {#if !inChat}<button
      class="tool web"
      class:on={web}
      aria-pressed={web}
      data-tip={web ? "Web search is on: the agent searches the web for what you send. Click to turn it off" : "Search the web for what you send"}
      onclick={() => (webPick = { id: threadId, on: !web })}
    ><Icon name="globe" size={15} />{#if web}<span>Web</span>{/if}</button>{/if}
    <input bind:this={picker} type="file" multiple hidden onchange={() => {
      // Copied first: clearing the input empties its live FileList.
      addFiles([...(picker.files ?? [])]);
      picker.value = "";
    }} />

    <MicMenu {voice} open={menu === "mic"} ontoggle={() => (menu = menu === "mic" ? null : "mic")} onclose={() => (menu = null)} />

    {#if cwd}<button class="tool" data-tip="Turn what you wrote or said into a structured prompt, with the project as context" disabled={!text.trim()} onclick={() => tidyBar?.tidyNow()}><Icon name="wand" size={15} /></button>{/if}

    <UsageControl {usage} {threadId} bind:menu oncompact={inThread && has("compact") ? () => run("/compact") : undefined} />

    {#if offerCompact}<CompactChip used={session.usage?.used ?? 0} bind:keepFull />{/if}
    {#if busy && oncancel && !canSend}
      <button class="send stop" data-tip="Stop (Esc)" onclick={oncancel}><Icon name="stop" size={13} /></button>
    {:else}
      <button class="send" data-use="thread.send" data-tip={busy ? (steer ? "Steer: stop the turn, this goes next (Enter). Alt Enter queues" : "Queue (Enter). Alt Enter steers") : "Send (Enter)"} disabled={!canSend} onclick={() => submit()}>
        <Icon name="arrowUp" size={16} />
      </button>
    {/if}
  </div>
</div>


<style>
  .composer {
    position: relative;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-lg);
    padding: 12px 8px 8px 16px;
  }
  .composer:focus-within { border-color: color-mix(in srgb, var(--accent) 55%, transparent); }
  .composer.dragging { border-color: var(--accent); border-style: dashed; }
  textarea {
    width: 100%;
    resize: none;
    background: none;
    border: 0;
    outline: none;
    font-size: 15px;
    line-height: 1.5;
    padding: 2px 8px 6px 0;
  }
  textarea::placeholder { color: var(--muted); }
  /* One card, as in T3 Code and Claude Desktop: attachments, text and controls together. */
  .field { min-width: 0; position: relative; }
  /* Dictated words still being heard: grey until their final text is in. The copy under the
     textarea draws the letters; it must lay them out exactly as the textarea does. */
  .mirror {
    position: absolute; top: 0; left: 0; bottom: 0; overflow: hidden; pointer-events: none;
    font: inherit; font-size: 15px; line-height: 1.5; padding: 2px 8px 6px 0;
    white-space: pre-wrap; overflow-wrap: break-word; color: var(--text);
  }
  .mirror .heard { color: var(--muted); }
  .field.dictating textarea { color: transparent; caret-color: var(--text); position: relative; }
  .bar { display: flex; align-items: center; gap: 2px; padding: 4px 0 0; margin-left: -8px; flex-wrap: wrap; position: relative; }
  .status { color: var(--muted); font-size: 12px; margin-left: 6px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  /* @kit menus */


  .send {
    width: 30px; height: 30px; border-radius: 50%; flex: none; margin-left: 6px;
    display: grid; place-items: center;
    background: var(--accent); color: var(--bg);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 14%, transparent);
    transition: box-shadow var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
  }
  .send:not(:disabled):hover { box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 20%, transparent); }
  .send:disabled { background: var(--surface-hover); color: var(--muted); cursor: default; box-shadow: none; }
  .send.stop { background: var(--surface-hover); color: var(--text); }

</style>
