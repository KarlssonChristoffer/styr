<!-- The sidebar as an inbox: threads that handed the ball back on top, latest first, and the ones still working, snoozed or settled folded below. Opening a thread is to get rid of it. A thread only moves when its state changes, and then it slides (`use:motion`). Rows are dragged between sections to pin, settle or wake them; the settle button drags across rows.
     Used by Sidebar.svelte. -->
<script lang="ts">
  import { autofocus } from "../autofocus";
  import { pop } from "../popover";
  // The sidebar as an inbox: threads that handed the ball back on top, latest first, and the
  // ones still working, snoozed or settled folded below. Opening a thread is to get rid of it.
  // A thread only moves when its state changes, and then it slides (`use:motion`). Rows are
  // dragged between sections to pin, settle or wake them; the settle button drags across rows.
  import Working from "./Working.svelte";
  import type { Thread } from "../bindings/Thread";
  import { fromFile } from "../attachments";
  import { BAG_DRAG, bagDrop, intoThread, threadInBag } from "../bag.svelte";
  import { duration } from "../activity";
  import { drafts, dropped } from "../drafts.svelte";
  import { folderName, fullTime, isScratch, listTime, threadProject } from "../format";
  import { contextMenu, menu } from "../menu.svelte";
  import { pending, snoozeMenu, snoozeTimes, threadMenu } from "../menus.svelte";
  import { motion, REMEASURE } from "../motion";
  import { clipBox, follow, groupMotion, openTime, shift } from "../fold";
  import { openUrl } from "../native";
  import { prefs, setPref } from "../prefs.svelte";
  import { asksYou, chatMark, hub, isSnoozed, needsYou, unread, wakeLabel } from "../state.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import CloudMark from "./CloudMark.svelte";
  import { cloudOf } from "../cloud.svelte";
  import StateSign, { type Sign } from "./StateSign.svelte";
  import { harnessLabel, providerOf } from "../providers.svelte";
  import Icon from "./Icon.svelte";
  import OwnerMark from "./OwnerMark.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import WorkerChain from "./WorkerChain.svelte";
  import ThreadPeek from "./ThreadPeek.svelte";
  import { looks } from "../looks.svelte";
  import { changeWorld } from "../worldSwitch";
  import { machineGlyph, GLYPHS } from "../refs";
  import { isGuardChat } from "../system";

  type Section = "hubasks" | "pinned" | "inbox" | "working" | "hub" | "chats" | "snoozed" | "settled";

  // An apple, for threads that run on a Mac.
  const APPLE =
    "M15.6 2.2c-1.3.1-2.8.9-3.6 2-.7.9-1.2 2.2-1 3.4 1.4.1 2.8-.8 3.6-1.9.8-1 1.2-2.2 1-3.5zM20 17.2c-.5 1.2-.8 1.7-1.5 2.7-1 1.4-2.3 3.1-4 3.1-1.5 0-1.9-1-3.9-1s-2.5 1-4 1c-1.7 0-2.9-1.6-3.9-3-2.7-3.8-3-8.3-1.4-10.7 1.1-1.7 2.8-2.7 4.4-2.7 1.7 0 2.7 1 4.1 1 1.3 0 2.1-1 4.1-1 1.5 0 3 .8 4.1 2.2-3.6 2-3 7.1 1.1 8.5z";
  let showWorking = $state(true);
  let showSnoozed = $state(false);
  let showSettled = $state(false);
  let closedFolders = $state<Record<string, boolean>>({});
  const now = $derived(hub.now);

  function project(t: Thread): string {
    const cwd = threadProject(t);
    return (!t.host && hub.projects.find((p) => p.path === cwd)?.name) || folderName(cwd);
  }

  type Mark = { label: string; tone: string; sign: Sign };
  function mark(t: Thread): Mark | null {
    if (t.status === "waiting_on_you") return { label: "Needs you", tone: "amber", sign: "asks" };
    if (hub.waits[t.id]) return { label: `Limited · ${wakeLabel(hub.waits[t.id], now)}`, tone: "limited", sign: "limited" };
    if (t.status === "error") return { label: "Failed", tone: "red", sign: "failed" };
    if (asksYou(t)) return t.reading === "stuck" ? { label: "Stuck", tone: "red", sign: "stuck" } : { label: "Asks you", tone: "amber", sign: "asks" };
    if (t.status === "in_terminal") return { label: "In terminal", tone: "ring", sign: "terminal" };
    if (t.status === "running") return { label: "Working", tone: "green", sign: "working" };
    if (unread(t)) return t.reading === "partway" ? { label: "Stopped half way", tone: "accent", sign: "partway" } : { label: "Done", tone: "accent", sign: "done" };
    return null;
  }

  // ---- The list, flat, so a row that changes section slides there ----

  type Item =
    | { kind: "head"; key: string; section: Section; label: string; n: number; open?: boolean; toggle?: () => void; robot?: boolean; shield?: boolean; ask?: boolean; tip?: string }
    | { kind: "folder"; key: string; cwd: string; host: string | null; name: string; threads: Thread[] }
    | { kind: "row"; key: string; t: Thread; section: Section }
    | { kind: "line"; key: string; t: Thread; section: "snoozed" | "settled" }
    | { kind: "empty"; key: string }
    /** A line of text at the foot of a group that does something (a group's foot). */
    | { kind: "link"; key: string; label: string }
    | { kind: "gap"; key: string }
    /** A folding group's rows in a box of their own, so it can open and fold clipped (`fold.ts`).
     *  Its rows are there while it is shut too, but not drawn. */
    | { kind: "drawer"; key: string; section: Section; open: boolean; rows: Item[] };

  /** The inbox in a folder per project, in the order each project's latest thread came back. */
  function folders(threads: Thread[]): Item[] {
    const groups = new Map<string, Thread[]>();
    for (const t of threads) {
      const k = `${t.host ?? ""}\u0000${threadProject(t)}`;
      groups.set(k, [...(groups.get(k) ?? []), t]);
    }
    const out: Item[] = [];
    for (const [k, list] of groups) {
      out.push({ kind: "folder", key: `f:${k}`, cwd: threadProject(list[0]), host: list[0].host ?? null, name: project(list[0]), threads: list });
      if (!closedFolders[k]) out.push(...list.map((t): Item => ({ kind: "row", key: t.id, t, section: "inbox" })));
    }
    return out;
  }

  /** Work narrowed to one project (switched over from it in Control) or one machine. */
  function inScope(t: Thread) {
    const s = hub.workScope;
    if (s?.quick) {
      if (!isScratch(threadProject(t))) return false;
    } else if (s && (threadProject(t) !== s.cwd || (t.host ?? null) !== s.host)) return false;
    return !hub.hostFilter || hub.hostOf(t) === hub.hostFilter;
  }
  const only = (list: Thread[]) => (hub.workScope || hub.hostFilter ? list.filter(inScope) : list);

  const items = $derived.by((): Item[] => {
    const out: Item[] = [];
    const [pinned, inbox, working, snoozed, settled, asks, guardAsks, fromHub] = [hub.pinned, hub.inbox, hub.working, hub.snoozed, hub.settled, hub.hubAsks, hub.guardAsks, hub.fromHub].map(only);
    // The Guard handles Problems itself: its chat and its fix threads come in only when one of
    // them needs you, with its own heading. Hub's other threads (routines, ...) are out of the
    // way: on top only when one needs you, else folded under "From Hub" near the bottom.
    if (guardAsks.length) {
      out.push({ kind: "head", key: "h:guardasks", section: "hubasks", label: "Guard needs you", n: guardAsks.length, shield: true, tip: "The Guard, or a fix thread it started, got stuck or wants your go: it tried what it could first" });
      out.push(...guardAsks.map((t): Item => ({ kind: "row", key: t.id, t, section: "hubasks" })));
    }
    if (asks.length) {
      out.push({ kind: "head", key: "h:hubasks", section: "hubasks", label: "Hub needs you", n: asks.length, robot: true, tip: "Threads Hub's own agents started (routines, the merge queue, ...) that wait on you" });
      out.push(...asks.map((t): Item => ({ kind: "row", key: t.id, t, section: "hubasks" })));
    }
    if (pinned.length || drag?.active) {
      out.push({ kind: "head", key: "h:pinned", section: "pinned", label: "Pinned", n: pinned.length });
      out.push(...pinned.map((t): Item => ({ kind: "row", key: t.id, t, section: "pinned" })));
    }
    // Under the pins the inbox gets a heading of its own, so it is plain where the pins end.
    if (pinned.length && inbox.length) out.push({ kind: "head", key: "h:inbox", section: "inbox", label: "Inbox", n: inbox.length });
    if (prefs.groupByProject) out.push(...folders(inbox));
    else out.push(...inbox.map((t): Item => ({ kind: "row", key: t.id, t, section: "inbox" })));
    if (!inbox.length) out.push({ kind: "empty", key: "empty" });
    // Everything below needs nothing from you: it sits at the bottom, above the usage bar, so the
    // empty space on top says nothing waits. A group opens downward under its heading.
    out.push({ kind: "gap", key: "gap" });
    const below: Item[][] = [];
    const group = (head: Extract<Item, { kind: "head" }>, rows: Item[]) =>
      below.push([head, { kind: "drawer", key: `d:${head.section}`, section: head.section, open: !!head.open, rows }]);
    if (working.length) group(
      { kind: "head", key: "h:working", section: "working", label: "Working", n: working.length, open: showWorking, toggle: () => (showWorking = !showWorking) },
      working.map((t): Item => ({ kind: "row", key: t.id, t, section: "working" })));
    if (fromHub.length) group(
      { kind: "head", key: "h:hub", section: "hub", label: "From Hub", n: fromHub.length, robot: true, tip: "Threads Hub's own agents started: routines, the merge queue, widget builds", open: prefs.showHubThreads, toggle: () => setPref("showHubThreads", !prefs.showHubThreads) },
      fromHub.map((t): Item => ({ kind: "row", key: t.id, t, section: "hub" })));
    // Chats are not here at all: they live in the Chat world, behind the rail's world button.
    if (snoozed.length) group(
      { kind: "head", key: "h:snoozed", section: "snoozed", label: "Snoozed", n: snoozed.length, open: showSnoozed, toggle: () => (showSnoozed = !showSnoozed) },
      snoozed.map((t): Item => ({ kind: "line", key: t.id, t, section: "snoozed" })));
    if (settled.length || drag?.active) group(
      { kind: "head", key: "h:settled", section: "settled", label: "Settled", n: settled.length, open: showSettled, toggle: () => (showSettled = !showSettled) },
      settled.map((t): Item => ({ kind: "line", key: t.id, t, section: "settled" })));
    out.push(...below.flat());
    return out;
  });

  /** The group that is opening, whose rows fold out one by one (`.stagger`). */
  let unfolding = $state<Section | null>(null);
  let foldTimer: ReturnType<typeof setTimeout>;

  /** Opens or folds a group at the bottom. Its box grows under the heading or shrinks into it
   *  (`fold.ts`), while the list's own slide motion stands aside (`data-fold`) and takes the new
   *  places afterwards. When the rows run past the bottom, the sidebar follows so the heading
   *  rises to the top. */
  function openGroup(head: HTMLElement, item: Extract<Item, { kind: "head" }>) {
    const opening = !item.open;
    const m = groupMotion();
    const view = head.closest<HTMLElement>(".scroll");
    const ms = m ? openTime(m) : 0;
    clearTimeout(foldTimer);
    if (m && list) list.dataset.fold = "true";
    unfolding = opening && m && m !== "drawer" && m !== "now" ? item.section : null;
    item.toggle?.();
    if (opening && view) {
      if (m) follow(view, head, ms);
      else queueMicrotask(() => head.scrollIntoView({ block: "nearest" }));
    }
    foldTimer = setTimeout(() => {
      if (list) delete list.dataset.fold;
      unfolding = null;
      list?.dispatchEvent(new Event(REMEASURE));
    }, ms + 80);
  }

  /** The rows as they are drawn, open groups' rows in their place. */
  const flat = $derived(items.flatMap((i) => (i.kind === "drawer" ? (i.open ? i.rows : []) : [i])));

  /** Threads in the order they are on screen, for Shift click, sweeps and Ctrl 1–9. */
  const visible = $derived(flat.filter((i) => i.kind === "row" || i.kind === "line").map((i) => (i as { t: Thread }).t));

  // ---- Ctrl 1–9 open the first nine; holding Ctrl shows which is which ----

  let hints = $state(false);
  function keydown(e: KeyboardEvent) {
    if (e.key === "Control" || e.key === "Meta") {
      hints = !e.shiftKey && !e.altKey;
      return;
    }
    if (e.key === "Escape") {
      if (drag?.active || sweep) {
        drag = null;
        sweep = null;
        e.preventDefault();
      } else if (hub.picked.length) hub.picked = [];
      return;
    }
    const ctrl = (e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey;
    // The model picker uses the same keys while it is open.
    if (!ctrl || !/^Digit[1-9]$/.test(e.code) || e.defaultPrevented || document.querySelector('[role="dialog"][aria-label="Models"]')) return;
    const t = visible[Number(e.code.slice(5)) - 1];
    if (t) {
      e.preventDefault();
      hints = false;
      hub.open(t.id);
    }
  }

  // ---- Picking several: Ctrl click adds one, Shift click a run ----

  let anchor: string | null = null;
  function click(e: MouseEvent, t: Thread) {
    if (dragged) return;
    if (e.ctrlKey || e.metaKey) {
      hub.picked = hub.picked.includes(t.id) ? hub.picked.filter((id) => id !== t.id) : [...hub.picked, t.id];
      anchor = t.id;
    } else if (e.shiftKey) {
      const ids = visible.map((x) => x.id);
      const from = ids.indexOf(anchor ?? hub.selected ?? t.id);
      const to = ids.indexOf(t.id);
      const [a, b] = from < 0 ? [to, to] : [Math.min(from, to), Math.max(from, to)];
      hub.picked = [...new Set([...hub.picked, ...ids.slice(a, b + 1)])];
    } else {
      hub.picked = [];
      anchor = t.id;
      hub.open(t.id);
    }
  }

  // ---- Dragging a row to another section ----

  type Drop = { section: Section; above?: number | null; below?: number | null; y?: number } | null;
  // Svelte sets this back to null when the list unmounts (a section switch, a maximise), while a
  // pointer move, a sweep or a queued microtask may still run after that.
  let list: HTMLDivElement | null = null;
  let drag = $state<{ id: string; from: Section; x0: number; y0: number; dy: number; armed: boolean; active: boolean; drop: Drop } | null>(null);
  // A row is only picked up after it has been held for a moment, so a click that wobbles, a scroll
  // or a text selection that starts on a row never moves or pins anything.
  const HOLD_MS = 280;
  let holdTimer: ReturnType<typeof setTimeout> | undefined;
  // A drag ends with a click on the row it started on; that click opens nothing.
  let dragged = false;

  function verb(from: Section, to: Section): string | null {
    if (from === to) return to === "pinned" ? "Move" : null;
    if (to === "pinned") return "Pin";
    if (to === "settled") return "Settle";
    if (to === "snoozed") return null;
    if (from === "pinned") return "Unpin";
    if (from === "settled") return "Un-settle";
    if (from === "snoozed") return "Wake";
    return null;
  }

  function down(e: PointerEvent, t: Thread, section: Section) {
    if (e.button !== 0 || e.ctrlKey || e.metaKey || e.shiftKey) return;
    drag = { id: t.id, from: section, x0: e.clientX, y0: e.clientY, dy: 0, armed: false, active: false, drop: null };
    clearTimeout(holdTimer);
    holdTimer = setTimeout(() => {
      if (drag && !drag.active && drag.id === t.id) drag.armed = true;
    }, HOLD_MS);
  }

  function dropAt(y: number): Drop {
    const nodes = [...(list?.querySelectorAll<HTMLElement>("[data-section]") ?? [])].filter((n) => n.dataset.key !== drag?.id && !n.inert);
    let over: HTMLElement | undefined;
    for (const n of nodes) {
      const r = n.getBoundingClientRect();
      if (y < r.top + r.height / 2) {
        over = n;
        break;
      }
    }
    // Past the end is the last section; above everything is the top of the pins.
    const section = (over?.dataset.section ?? nodes.at(-1)?.dataset.section ?? "inbox") as Section;
    const head = over?.dataset.head === "true";
    if (section === "pinned") {
      const pins = nodes.filter((n) => n.dataset.section === "pinned" && n.dataset.head !== "true");
      const at = over && !head ? pins.indexOf(over) : pins.length;
      const order = (n?: HTMLElement) => (n ? Number(n.dataset.pin) : null);
      const before = pins[at - 1];
      const box = (before ?? nodes.find((n) => n.dataset.head === "true" && n.dataset.section === "pinned"))?.getBoundingClientRect();
      return { section: "pinned", above: order(before), below: order(pins[at]), y: box ? box.bottom - (list?.getBoundingClientRect().top ?? 0) : 0 };
    }
    // A section's heading counts as the section below it.
    if (head && over) return { section: over.dataset.section as Section };
    // A row is in the section of the heading above it.
    const above = nodes.filter((n) => n.getBoundingClientRect().top < y).at(-1);
    return { section: ((above ?? over)?.dataset.section ?? section) as Section };
  }

  function move(e: PointerEvent) {
    if (sweep) return sweepMove(e);
    if (!drag) return;
    const away = Math.hypot(e.clientX - drag.x0, e.clientY - drag.y0);
    if (!drag.active && !drag.armed) {
      // Moved before the hold was over: this is a click, a scroll or a selection, not a pick-up.
      if (away >= 6) {
        clearTimeout(holdTimer);
        drag = null;
      }
      return;
    }
    if (!drag.active && away < 6) return;
    drag.active = true;
    drag.dy = e.clientY - drag.y0;
    drag.drop = dropAt(e.clientY);
    // Out of the list sideways, the thread can go in the context bag.
    const out = !!list && e.clientX > list.getBoundingClientRect().right;
    bagDrop.thread = out ? drag.id : null;
    bagDrop.over = out && !!document.elementFromPoint(e.clientX, e.clientY)?.closest("[data-bag-zone]");
  }

  function up() {
    if (sweep) return sweepUp();
    clearTimeout(holdTimer);
    const d = drag;
    drag = null;
    const toBag = bagDrop.thread && bagDrop.over;
    bagDrop.thread = null;
    bagDrop.over = false;
    if (!d?.active) return;
    dragged = true;
    setTimeout(() => (dragged = false), 0);
    if (toBag) return void threadInBag(d.id, "thread list");
    const to = d.drop?.section;
    if (!to || !verb(d.from, to)) return;
    if (to === "pinned") hub.pinAt(d.id, d.drop!.above ?? null, d.drop!.below ?? null);
    else if (to === "settled") hub.settleMany([d.id], true);
    else if (d.from === "pinned") hub.pin([d.id], false);
    else if (d.from === "settled") hub.settleMany([d.id], false);
    else if (d.from === "snoozed") hub.snooze([d.id], 0);
  }

  // ---- Sweeping: press a row's button and drag across rows to do the same to all of them ----

  let sweep = $state<{ section: Section; from: number; to: number } | null>(null);
  const swept = $derived.by(() => {
    if (!sweep) return new Set<string>();
    const ids = flat.filter((i): i is Extract<Item, { t: Thread }> => "t" in i && i.section === sweep!.section).map((i) => i.t.id);
    const [a, b] = [Math.min(sweep.from, sweep.to), Math.max(sweep.from, sweep.to)];
    return new Set(ids.slice(a, b + 1));
  });

  function sectionIds(section: Section) {
    return flat.filter((i): i is Extract<Item, { t: Thread }> => "t" in i && i.section === section).map((i) => i.t.id);
  }

  function sweepDown(e: PointerEvent, t: Thread, section: Section) {
    if (e.button !== 0) return;
    // The row never sees this press (it stops here), so the card is closed here.
    peekClose();
    e.stopPropagation();
    const at = sectionIds(section).indexOf(t.id);
    sweep = { section, from: at, to: at };
  }

  function sweepMove(e: PointerEvent) {
    if (!sweep) return;
    const ids = sectionIds(sweep.section);
    const rows = ids.map((id) => list?.querySelector<HTMLElement>(`[data-key="${CSS.escape(id)}"]`));
    const at = rows.findIndex((r) => {
      const box = r?.getBoundingClientRect();
      return box && e.clientY >= box.top && e.clientY < box.bottom;
    });
    if (at >= 0) sweep.to = at;
  }

  function sweepUp() {
    const s = sweep;
    sweep = null;
    if (!s) return;
    const all = sectionIds(s.section);
    const run = all.slice(Math.min(s.from, s.to), Math.max(s.from, s.to) + 1);
    if (!run.length) return;
    if (s.section === "settled") hub.settleMany(run, false);
    else if (s.section === "snoozed") hub.snooze(run, 0);
    else hub.settleMany(run, true);
  }

  // ---- Files dropped on a row, or things from the context bag, go to that thread's composer ----

  let fileOver = $state<string | null>(null);
  async function dropFiles(e: DragEvent, t: Thread) {
    e.preventDefault();
    fileOver = null;
    // Something dragged out of the context bag.
    const fromBag = e.dataTransfer?.getData(BAG_DRAG);
    if (fromBag) return intoThread(t.id, [fromBag]);
    const files = [...(e.dataTransfer?.files ?? [])];
    if (!files.length) return;
    try {
      const added = await Promise.all(files.map(fromFile));
      dropped[t.id] = [...(dropped[t.id] ?? []), ...added];
      hub.open(t.id);
    } catch (err) {
      hub.fail(err);
    }
  }

  // ---- Custom snooze time ----

  let custom = $state("");
  $effect(() => {
    if (pending.snooze) {
      const d = new Date(Date.now() + 2 * 3_600_000);
      d.setMinutes(0, 0, 0);
      custom = new Date(d.getTime() - d.getTimezoneOffset() * 60_000).toISOString().slice(0, 16);
    }
  });
  function snoozeCustom() {
    const at = new Date(custom).getTime();
    if (pending.snooze && at > Date.now()) hub.snooze(pending.snooze.ids, at);
    pending.snooze = null;
  }

  // Opening a thread from elsewhere (Ctrl J, the palette, a notification) shows it in the list.
  $effect(() => {
    const id = hub.selected;
    if (!id || !list) return;
    queueMicrotask(() => list?.querySelector(`[data-key="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" }));
  });

  // ---- The hover card (ThreadPeek): after a short rest on a row, gone a moment after leaving both ----

  let peek = $state<{ id: string; el: HTMLElement } | null>(null);
  let peekTimer = 0;
  const peeked = $derived(peek ? hub.threads.find((t) => t.id === peek!.id) : undefined);

  function peekEnter(t: Thread, el: HTMLElement) {
    clearTimeout(peekTimer);
    if (drag?.active || hub.picked.length) return;
    if (peek) {
      // Moving down the list swaps the card at once, like tips along a row of buttons.
      if (peek.id !== t.id) peek = { id: t.id, el };
      return;
    }
    peekTimer = window.setTimeout(() => {
      if (!drag?.active && el.isConnected) peek = { id: t.id, el };
    }, 550);
  }

  function peekLeave() {
    clearTimeout(peekTimer);
    // Long enough to cross the gap to the card.
    peekTimer = window.setTimeout(() => (peek = null), 260);
  }

  function peekKeep() {
    clearTimeout(peekTimer);
  }

  function peekClose() {
    clearTimeout(peekTimer);
    peek = null;
  }

  // A row that settles, snoozes or otherwise leaves its place takes no `pointerleave` with it, so
  // the card would hang on, measuring a row that is gone (and so landing in the top-left corner).
  // It closes at once when its row is no longer in the list.
  $effect(() => {
    void items;
    if (!peek) return;
    if (!peeked || peeked.settled_at || !peek.el.isConnected) peekClose();
  });

  function newIn(cwd: string, host: string | null) {
    hub.startDraft(cwd, "", host);
  }
</script>

<svelte:window onkeydown={keydown} onkeyup={(e) => (e.key === "Control" || e.key === "Meta") && (hints = false)} onblur={() => (hints = false)} onpointermove={move} onpointerup={up} />

{#snippet row(t: Thread, section: Section, index: number)}
  {@const m = mark(t)}
  {@const branch = hub.branches[t.id]}
  {@const pr = hub.prs[t.id]}
  {@const run = hub.workerRuns[t.id]}
  {@const fix = hub.fixCards[t.id]}
  {@const draft = !!drafts[t.id]}
  {@const kids = hub.childrenOf(t.id)}
  {@const provider = providerOf(t.engine)}
  {@const quiet = (section === "inbox" || section === "working") && (!m || m.tone === "green")}
  {@const dragging = drag?.active && drag.id === t.id}
  {@const look = looks.project(threadProject(t), t.host, project(t))}
  {@const at = Math.max(t.returned_at ?? t.updated_at, t.snoozed_until ?? 0)}
  {@const cloud = cloudOf(t)}
  {@const hostName = hub.hostOf(t)}
  {@const hostRaw = hostName ? hub.hostInfo(hostName) : undefined}
  {@const host = hostName && !cloud ? { name: hostName, local: !t.host || t.host === hub.localHost, state: hostRaw?.state ?? "online", mac: looks.isMac(hostName, !t.host || t.host === hub.localHost) } : null}
  <div role="presentation"
    class="thread"
    class:active={hub.selected === t.id && hub.view === "thread"}
    class:quiet
    class:compact={section === "pinned" || prefs.cards === "dense"}
    class:cards-compact={section !== "pinned" && prefs.cards === "compact"}
    class:picked={hub.picked.includes(t.id)}
    class:swept={swept.has(t.id)}
    class:dragging
    class:armed={drag?.armed && !drag.active && drag.id === t.id}
    class:fileover={fileOver === t.id}
    data-key={t.id}
    data-section={section}
    data-pin={t.pinned_at ?? ""}
    style:--pc="var(--p-{look.color})"
    style:transform={dragging ? `translateY(${drag!.dy}px)` : undefined}
    use:menu={() => threadMenu(t)}
    onpointerenter={(e) => peekEnter(t, e.currentTarget as HTMLElement)}
    onpointerleave={peekLeave}
    onpointerdown={(e) => {
      peekClose();
      down(e, t, section);
    }}
    ondragover={(e) => {
      if (e.dataTransfer?.types.includes("Files") || e.dataTransfer?.types.includes(BAG_DRAG)) {
        e.preventDefault();
        fileOver = t.id;
      }
    }}
    ondragleave={() => fileOver === t.id && (fileOver = null)}
    ondrop={(e) => dropFiles(e, t)}
  >
    <button class="open" onclick={(e) => click(e, t)}>
      <!-- Three lines, so nothing is mixed up. First what and where: the project and branch, who
           started it, and the machine it runs on. Then the title, clean, with its pull request and
           side jobs at the end. Then how it stands: a sign and the status word with the time, and
           at the end which agent runs it. A Worker run or a fix thread adds a line of its own. -->
      {#if section === "pinned" || prefs.cards === "dense"}
        <!-- A pin is one line: the pin, the title, how it stands and who runs it. What the full row
             carries (project, branch, owner, draft) is in the hover and one click away; a run or a
             fix line still shows under it. -->
        <span class="line one-line">
          {#if hints && index < 9}<kbd class="hint">{index + 1}</kbd>{:else if section === "pinned"}<span class="pinmark" data-tip="Pinned: {project(t)}{branch && branch !== 'main' && branch !== 'master' ? ` · ${branch}` : ''}"><Icon name="pin" size={12} /></span>{:else}<span class="pdot" data-tip="{project(t)}{branch && branch !== 'main' && branch !== 'master' ? ` · ${branch}` : ''}"></span>{/if}
          <span class="grow ellipsis ptitle">{t.title}</span>
          {#if draft}<span class="draft" data-tip="Unsent draft"><Icon name="pen" size={11} /></span>{/if}
          {#if pr}
            <span class="pr is-{pr.state}" role="link" tabindex="-1" data-tip="{pr.title} ({pr.state})" onclick={(e) => { e.stopPropagation(); openUrl(pr.url); }} onkeydown={(e) => { if (e.key === "Enter") { e.stopPropagation(); openUrl(pr.url); } }}><Icon name="pr" size={11} />#{pr.number}</span>
          {/if}
          {#if kids.length}<span class="kids" data-tip={kids.map((k) => k.title).join("\n")}>{#if kids.some((k) => k.status === "running")}<Working size={10} />{/if}{kids.length}</span>{/if}
          <span class="tail">
            <StateSign kind={m?.sign ?? "idle"} />
            {#if t.status === "running"}<span class="since green">{duration(now - (t.sent_at ?? t.updated_at))}</span>
            {:else if m && (m.tone === "amber" || m.tone === "red" || m.tone === "limited")}<span class="word {m.tone}">{m.label}</span>
            {:else}<span class="since" data-tip={fullTime(at)}>{listTime(at, now, prefs.listTime)}</span>{/if}
            {#if cloud}<span class="machine"><CloudMark {cloud} size={13} /></span>{:else if host}<span class="machine" class:offline={host.state !== "online"} data-tip={host.state === "online" ? `Runs on ${host.name}` : `${host.name} is offline`}>
              {#if host.mac}<svg class="apple" viewBox="0 0 24 24" aria-hidden="true"><path d={APPLE} /></svg>{:else}<svg class="kind" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[machineGlyph(host.name, host.local)]} /></svg>{/if}
            </span>{/if}
            <span class="agent icon engine" data-tip="{harnessLabel(t.engine)} on {provider.name}"><ProviderMark id={provider.id} size={11} /></span>
          </span>
        </span>
      {:else if prefs.cards === "compact"}
      <!-- Two lines: the project (left out when the list is already one project) and the title with
           who runs it, then how it stands with the side jobs at the end. -->
      <span class="line c1">
        {#if hints && index < 9}<kbd class="hint">{index + 1}</kbd>{/if}
        {#if !hub.workScope}<span class="pname cname" style:color={isGuardChat(t) ? undefined : `var(--p-${look.color})`} class:sys={isGuardChat(t)} data-tip="{project(t)}{branch && branch !== 'main' && branch !== 'master' ? ` · ${branch}` : ''}">{isGuardChat(t) ? "Hub" : project(t)}</span>{/if}
        <span class="grow ellipsis ctitle">{t.title}</span>
        {#if draft}<span class="draft" data-tip="Unsent draft"><Icon name="pen" size={11} /></span>{/if}
        {#if pr}
          <span class="pr is-{pr.state}" role="link" tabindex="-1" data-tip="{pr.title} ({pr.state})" onclick={(e) => { e.stopPropagation(); openUrl(pr.url); }} onkeydown={(e) => { if (e.key === "Enter") { e.stopPropagation(); openUrl(pr.url); } }}><Icon name="pr" size={11} />#{pr.number}</span>
        {/if}
        {#if cloud}<span class="machine"><CloudMark {cloud} size={13} /></span>{:else if host}<span class="machine" class:offline={host.state !== "online"} data-tip={host.state === "online" ? `Runs on ${host.name}` : `${host.name} is offline`}>
          {#if host.mac}<svg class="apple" viewBox="0 0 24 24" aria-hidden="true"><path d={APPLE} /></svg>{:else}<svg class="kind" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[machineGlyph(host.name, host.local)]} /></svg>{/if}
        </span>{/if}
        <span class="agent icon engine" data-tip="{harnessLabel(t.engine)} on {provider.name}"><ProviderMark id={provider.id} size={11} /></span>
      </span>
      <span class="line state c2">
        <StateSign kind={m?.sign ?? "idle"} />
        {#if t.status === "running"}
          <span class="word green">Working</span><span class="since green">{duration(now - (t.sent_at ?? t.updated_at))}</span>
        {:else if m}
          <span class="word {m.tone}">{m.label}</span><span class="since" data-tip={fullTime(at)}>{listTime(at, now, prefs.listTime)}</span>
        {:else}
          <span class="since" data-tip={fullTime(at)}>{listTime(at, now, prefs.listTime)}</span>
        {/if}
        {#if kids.length}<span class="kids end" data-tip={kids.map((k) => k.title).join("\n")}>{#if kids.some((k) => k.status === "running")}<Working size={10} />{/if}{kids.length} side {kids.length === 1 ? "job" : "jobs"}</span>{/if}
      </span>
      {:else}
      <span class="line top">
        {#if hints && index < 9}<kbd class="hint">{index + 1}</kbd>{:else if isGuardChat(t)}<span class="pi sys"><svg viewBox="0 0 24 24" aria-hidden="true" width="13" height="13"><path d={GLYPHS.shield} /></svg></span>{:else}<span class="pi"><ProjectIcon path={threadProject(t)} host={t.host} name={project(t)} size={13} /></span>{/if}
        <span class="grow ellipsis">{#if isGuardChat(t)}<span class="pname sys" data-tip="Hub's own guard of Problems, not a project">Hub</span>{:else}<span class="pname" style:color="var(--p-{look.color})">{project(t)}</span>{#if branch && branch !== "main" && branch !== "master"}<span class="branch" data-tip="Branch {branch}"> · {branch}</span>{/if}{/if}</span>
        {#if t.owner && !isGuardChat(t)}<OwnerMark owner={t.owner} />{/if}
        {#if draft}<span class="draft" data-tip="Unsent draft"><Icon name="pen" size={11} /></span>{/if}
        {#if cloud}
          <span class="machine"><CloudMark {cloud} size={13} /></span>
        {:else if host}
          <!-- The machine it runs on, always, as its icon alone (a Mac gets its apple, an offline one is dimmed); the name is in the hover. -->
          <span class="machine" class:offline={host.state !== "online"} data-tip={host.state === "online" ? `Runs on ${host.name}` : `${host.name} is offline`}>
            {#if host.mac}<svg class="apple" viewBox="0 0 24 24" aria-hidden="true"><path d={APPLE} /></svg>{:else}<svg class="kind" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS[machineGlyph(host.name, host.local)]} /></svg>{/if}
          </span>
        {/if}
        <span class="agent icon engine" data-tip="{harnessLabel(t.engine)} on {provider.name}"><ProviderMark id={provider.id} size={11} /></span>
      </span>
      <span class="line title"><span class="grow ellipsis">{t.title}</span>
        {#if pr}
          <span class="pr is-{pr.state}" role="link" tabindex="-1" data-tip="{pr.title} ({pr.state})" onclick={(e) => { e.stopPropagation(); openUrl(pr.url); }} onkeydown={(e) => { if (e.key === "Enter") { e.stopPropagation(); openUrl(pr.url); } }}>
            <Icon name="pr" size={11} />#{pr.number}<span class="prs">{pr.state}</span>
          </span>
        {/if}
        {#if kids.length}<span class="kids" data-tip={kids.map((k) => k.title).join("\n")}>{#if kids.some((k) => k.status === "running")}<Working size={10} />{/if}{kids.length} side {kids.length === 1 ? "job" : "jobs"}</span>{/if}
      </span>
      <span class="line state">
        <StateSign kind={m?.sign ?? "idle"} />
        {#if t.status === "running"}
          <span class="word green">Working</span><span class="since green">{duration(now - (t.sent_at ?? t.updated_at))}</span>
        {:else if m}
          <span class="word {m.tone}">{m.label}</span><span class="since" data-tip={fullTime(at)}>{listTime(at, now, prefs.listTime)}</span>
        {:else}
          <span class="since" data-tip={fullTime(at)}>{listTime(at, now, prefs.listTime)}</span>
        {/if}
      </span>
      {/if}
      {#if run}
        <!-- A run of one of your Workers (Knegarn) in this thread: its chain, the step going on lit. -->
        <span class="line runline" data-tip="{run.tool}: {run.title}">{#if run.state === "running" && !run.paused}<Working size={10} />{/if}<WorkerChain {run} small />{#if run.round}<span class="round">r{run.round}</span>{/if}</span>
      {/if}
      {#if fix}
        <!-- A fix thread from Problems says where it stands, so nobody has to open it to know. -->
        <span class="line fix is-{fix.state}" data-tip={fix.what ? `Problem #${fix.problem}: ${fix.what}` : `Problem #${fix.problem}`}>
          {#if fix.state === "working"}<Working size={10} />{:else}<span class="fdot"></span>{/if}<span class="grow ellipsis">{fix.says}</span>
        </span>
      {/if}
    </button>
    {#if dragging && drag?.drop && verb(drag.from, drag.drop.section)}
      <span class="badge">{verb(drag.from, drag.drop.section)}</span>
    {/if}
    {#if section !== "working"}
      <span class="actions">
        <button
          class="act"
          data-use="home.snooze"
          data-tip="Snooze"
          onclick={(e) => {
            const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
            contextMenu.show(r.left, r.bottom + 4, snoozeMenu([t.id]));
          }}><Icon name="clock" size={13} /></button>
        <button
          class="act"
          data-use="home.settle"
          data-tip="Settle: done with it. Drag up or down to settle a run of threads."
          onpointerdown={(e) => sweepDown(e, t, section)}
          onclick={(e) => {
            peekClose();
            if (e.detail === 0) hub.settle(t.id, true);
          }}><Icon name="check" size={13} /></button>
      </span>
    {/if}
  </div>
{/snippet}

{#snippet line(t: Thread, section: "snoozed" | "settled")}
  <div role="presentation"
    class="thread one"
    class:active={hub.selected === t.id && hub.view === "thread"}
    class:picked={hub.picked.includes(t.id)}
    class:swept={swept.has(t.id)}
    class:dragging={drag?.active && drag.id === t.id}
    class:armed={drag?.armed && !drag.active && drag.id === t.id}
    data-key={t.id}
    data-section={section}
    style:transform={drag?.active && drag.id === t.id ? `translateY(${drag.dy}px)` : undefined}
    use:menu={() => threadMenu(t)}
    onpointerdown={(e) => down(e, t, section)}
  >
    <button class="open" onclick={(e) => click(e, t)} data-tip={t.title}>
      <span class="dot {t.status}" class:unread={unread(t)}></span>
      <span class="grow ellipsis">{t.title}</span>
      {#if t.owner}<OwnerMark owner={t.owner} name={false} />{/if}
      <span class="when" data-tip={fullTime(section === "snoozed" ? t.snoozed_until! : t.settled_at!)}>{section === "snoozed" ? wakeLabel(t.snoozed_until!, now) : listTime(t.settled_at!, now, prefs.listTime)}</span>
    </button>
    {#if drag?.active && drag.id === t.id && drag.drop && verb(drag.from, drag.drop.section)}
      <span class="badge">{verb(drag.from, drag.drop.section)}</span>
    {/if}
    <span class="actions">
      <button
        class="act"
        data-tip={section === "snoozed" ? "Wake now. Drag to wake a run." : "Back in the list. Drag to bring back a run."}
        onpointerdown={(e) => sweepDown(e, t, section)}
        onclick={(e) => e.detail === 0 && (section === "snoozed" ? hub.snooze([t.id], 0) : hub.settle(t.id, false))}
      ><Icon name="undo" size={13} /></button>
    </span>
  </div>
{/snippet}

{#if hub.picked.length}
  <div class="bulk">
    <span class="grow">{hub.picked.length} picked</span>
    <button data-tip="Settle" onclick={() => hub.settleMany(hub.picked, true)}><Icon name="check" size={13} /></button>
    <button data-tip="Snooze" onclick={(e) => { const r = (e.currentTarget as HTMLElement).getBoundingClientRect(); contextMenu.show(r.left, r.bottom + 4, snoozeMenu([...hub.picked])); }}><Icon name="clock" size={13} /></button>
    <button data-tip="Pin" onclick={() => hub.pin([...hub.picked], true)}><Icon name="pin" size={13} /></button>
    <button data-tip="Clear (Esc)" onclick={() => (hub.picked = [])}><Icon name="x" size={13} /></button>
  </div>
{/if}

{#if peek && peeked}
  {#key peek.id}
    <ThreadPeek thread={peeked} anchor={peek.el} onkeep={peekKeep} onleave={peekLeave} onclose={peekClose} />
  {/key}
{/if}

<div class="wrap" class:dragging={drag?.active}>
  <div class="activity" use:motion bind:this={list}>
    {#each items as item (item.key)}
      {#if item.kind === "row"}
        {@render row(item.t, item.section, visible.indexOf(item.t))}
      {:else if item.kind === "line"}
        {@render line(item.t, item.section)}
      {:else if item.kind === "drawer"}
        <div class="drawer" data-key={item.key} data-motion="snap">
          {#if item.open}
            <div class="box" in:clipBox out:clipBox>
              <div class="rows" class:stagger={unfolding === item.section} class:steps={prefs.groupMotion === "cascade"} use:motion in:shift out:shift>
                {#each item.rows as r (r.key)}
                  {#if r.kind === "row"}
                    {@render row(r.t, r.section, visible.indexOf(r.t))}
                  {:else if r.kind === "line"}
                    {@render line(r.t, r.section)}
                  {:else if r.kind === "link"}
                    <button class="link" onclick={(e) => { const b = e.currentTarget.getBoundingClientRect(); changeWorld("chat", { x: b.left + 20, y: b.top + 10 }); }}>{r.label}<Icon name="chevron" size={11} /></button>
                  {/if}
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {:else if item.kind === "head"}
        {#if item.toggle}
          <button class="divider" class:over={drag?.drop?.section === item.section && drag.from !== item.section} data-section={item.section} data-head="true" data-key={item.key} data-tip={item.tip} onclick={(e) => openGroup(e.currentTarget, item)}>
            {#if item.robot}<svg class="robot" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.robot} /></svg>{/if}{#if item.shield}<svg class="robot" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.shield} /></svg>{/if}{item.label} <span class="n">{item.n}</span>{#if item.ask}<span class="askdot" data-tip="A chat has a new answer"></span>{/if}<span class="rule"></span>
            <span class="chev" class:open={item.open}><Icon name="chevron" size={12} /></span>
          </button>
        {:else}
          <div class="divider still" class:hubasks={item.section === "hubasks"} class:over={drag?.drop?.section === item.section && drag.from !== item.section} data-section={item.section} data-head="true" data-key={item.key} data-tip={item.tip}>
            {#if item.robot}<svg class="robot" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.robot} /></svg>{/if}{#if item.shield}<svg class="robot" viewBox="0 0 24 24" aria-hidden="true"><path d={GLYPHS.shield} /></svg>{/if}{item.label} {#if item.n}<span class="n">{item.n}</span>{:else}<span class="n">drop to pin</span>{/if}<span class="rule"></span>
          </div>
        {/if}
      {:else if item.kind === "folder"}
        {@const k = `${item.host ?? ""}\u0000${item.cwd}`}
        {@const needs = item.threads.some(needsYou)}
        <div class="folder" data-section="inbox" data-key={item.key}>
          <button class="fold" onclick={() => (closedFolders[k] = !closedFolders[k])}>
            <span class="chev" class:open={!closedFolders[k]}><Icon name="chevron" size={12} /></span>
            <span class="pi"><ProjectIcon path={item.cwd} host={item.host} name={item.name} size={13} /></span><span class="grow ellipsis">{item.name}</span>
            {#if closedFolders[k]}{#if needs}<span class="dot"></span>{/if}<span class="n">{item.threads.length}</span>{/if}
          </button>
          <button class="act" data-tip="Go into {item.name}: only it shows, with its desk to steer its threads" onclick={() => hub.goIn(item.cwd, item.host)}><Icon name="spark" size={12} /></button>
          <button class="act" data-tip="New thread in {item.name}" onclick={() => newIn(item.cwd, item.host)}><Icon name="pen" size={12} /></button>
        </div>
      {:else if item.kind === "gap"}
        <div class="gap" data-key={item.key} data-motion="snap"></div>
      {:else}
        <!-- One line that stays the same while threads come and go below it. -->
        <p class="empty" data-key={item.key} data-motion="snap">
          {hub.workScope ? `Nothing in ${hub.workScope.name} needs you.` : hub.active.length ? "Nothing needs you." : "No threads yet. Start one with New thread."}
        </p>
      {/if}
    {/each}
  </div>
  {#if drag?.active && drag.drop?.section === "pinned" && drag.drop.y != null}
    <div class="insert" style:top="{drag.drop.y}px"></div>
  {/if}
</div>

{#if pending.snooze}
  <div role="presentation" class="scrim" onpointerdown={() => (pending.snooze = null)}></div>
  <form class="custom" transition:pop|global style:left="{Math.min(pending.snooze.x, innerWidth - 240)}px" style:top="{Math.min(pending.snooze.y, innerHeight - 120)}px" onsubmit={(e) => { e.preventDefault(); snoozeCustom(); }}>
    <span class="label">Snooze until</span>
    <input type="datetime-local" bind:value={custom} use:autofocus onkeydown={(e) => e.key === "Escape" && (pending.snooze = null)} />
    <span class="row">
      {#each snoozeTimes().slice(0, 3) as s (s.label)}
        <button type="button" class="preset" onclick={() => { hub.snooze(pending.snooze!.ids, s.at); pending.snooze = null; }}>{s.label}</button>
      {/each}
      <button class="go">Snooze</button>
    </span>
  </form>
{/if}

<style>
  /* At least as tall as the sidebar, so the gap can push the folded groups to its bottom. */
  .wrap { position: relative; min-height: 100%; display: flex; flex-direction: column; }
  .wrap > .activity { flex: 1; }
  .gap { flex: 1; min-height: 8px; }
  .wrap.dragging { user-select: none; }
  .activity { display: flex; flex-direction: column; gap: 1px; padding-top: 4px; padding-bottom: 4px; }
  .thread { position: relative; border-radius: var(--r-md); isolation: isolate; }
  /* Colour says where a thread is from (the project's name) and how it stands (the sign and the
     word on the third line); no bar, wash or ring competes with them. The open thread is a calm
     grey step lighter than the hover (.active below). */
  .pname { opacity: 0.95; }
  .pname.sys, .pi.sys { color: var(--muted); }
  .pi.sys svg { fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  /* The machine, on every row: plain, so it reads as where it runs and not as one more button. */
  .machine {
    display: inline-flex; align-items: center; flex: none; color: var(--text-3);
  }
  .machine.offline { opacity: 0.55; }
  .machine .kind { width: 11px; height: 11px; flex: none; fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .machine .apple { width: 10px; height: 10px; flex: none; fill: currentColor; margin-top: -1px; }
  .thread:hover { background: var(--surface); }
  .thread.active { background: var(--surface-hover); }
  .thread.active .title { font-weight: 500; }
  .thread.picked { background: var(--accent-bg); }
  .thread.swept { background: var(--accent-bg); }
  .thread.fileover { box-shadow: inset 0 0 0 1px var(--accent); }
  /* Held long enough: the row is lifted a little and follows the pointer from here. */
  .thread.armed { background: var(--surface-hover); box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3); }
  .thread.dragging { z-index: 2; background: var(--surface-hover); box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35); pointer-events: none; }
  .thread.dragging .actions, .thread.dragging .machine, .thread.dragging .when { visibility: hidden; }
  .badge {
    position: absolute; right: 8px; top: 7px; font-size: 11px; padding: 1px 8px; border-radius: var(--r-sm);
    background: var(--accent-bg); color: var(--accent);
  }
  .open { display: flex; flex-direction: column; width: 100%; text-align: left; padding: 8px 10px 8px; gap: 1px; color: var(--text-3); }
  .line { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .top { font-size: 11px; color: var(--muted); }
  .title { color: var(--text); font-size: 12px; }
  .quiet .title { color: var(--text-3); }
  .branch { display: inline-flex; align-items: center; gap: 4px; font-family: var(--mono); font-size: 11px; opacity: 0.85; min-width: 0; }
  .branch :global(svg), .pr :global(svg) { flex: none; }
  .draft { display: grid; color: var(--amber); flex: none; }
  .runline { display: flex; align-items: center; gap: 6px; padding-left: 14px; min-width: 0; color: var(--muted); }
  .runline .round { font-size: 11px; color: var(--muted); flex: none; }
  .pr { display: inline-flex; align-items: center; gap: 3px; flex: none; font-size: 11px; border-radius: var(--r-sm); padding: 0 4px; cursor: pointer; }
  .pr:hover { background: var(--surface-hover); }
  /* State classes carry an is- prefix: a bare .open or .draft would also pick up the row's .open
     button and the pen's .draft styles, which stretched the chip onto its own centred line. */
  .pr.is-open { color: var(--green); }
  .pr.is-draft { color: var(--muted); }
  .pr.is-merged { color: var(--accent); }
  .pr.is-closed { color: var(--red); }
  .line.fix { font-size: 11px; color: var(--muted); gap: 6px; padding-left: 1px; }
  .line.fix .fdot { width: 6px; height: 6px; border-radius: 50%; flex: none; background: var(--muted); }
  .line.fix.is-review .fdot { background: var(--green); }
  .line.fix.is-merged .fdot, .line.fix.is-fixed .fdot { background: var(--accent); }
  .line.fix.is-needs_you { color: var(--amber); }
  .line.fix.is-needs_you .fdot { background: var(--amber); }
  .line.fix.is-stuck, .line.fix.is-failed, .line.fix.is-dropped { color: var(--red); }
  .line.fix.is-stuck .fdot, .line.fix.is-failed .fdot, .line.fix.is-dropped .fdot { background: var(--red); }
  .pinmark { display: grid; color: var(--accent); flex: none; }
  /* A pinned thread is one line, so the pins read as a short shelf and not as more inbox. */
  .compact .open { padding: 8px 10px; }
  .pdot { width: 6px; height: 6px; border-radius: 50%; flex: none; background: var(--pc); margin: 0 2px; }
  /* Compact: two lines. Project and title on top, how it stands below, side jobs at the end. */
  .cards-compact .open { padding: 4px 10px; gap: 0; }
  .c1 { font-size: 12px; color: var(--text); gap: 8px; }
  .quiet .c1 { color: var(--text-3); }
  .cname { flex: none; max-width: 64px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; font-weight: 600; }
  .c2 { padding-top: 0; }
  .c2 .kids.end { margin-left: auto; }
  .cards-compact:hover .c2 .kids { visibility: hidden; }
  .one-line { gap: 8px; font-size: 12px; color: var(--text); }
  .ptitle { color: var(--text); }
  .compact .tail { display: inline-flex; align-items: center; gap: 6px; flex: none; font-size: 11px; color: var(--muted); }
  .compact .tail .word { font-weight: 500; }
  .compact .actions { bottom: auto; top: 5px; }
  .compact:hover .tail { visibility: hidden; }
  .pi { display: inline-flex; flex: none; margin: 0 1px 0 1px; }
  .title .dot, .one .dot { margin-right: 1px; }
  .hint {
    width: 15px; height: 15px; border-radius: var(--r-xs); flex: none;
    display: grid; place-items: center; --keep-size: 1; font-size: 10px; font-weight: 600;
    background: var(--surface-hover); color: var(--text-2);
  }
  .hint { background: var(--accent-bg); color: var(--accent); font-family: var(--font); margin: 0; }
  .when { flex: none; font-size: 11px; font-variant-numeric: tabular-nums; }
  .line.state { font-size: 11px; color: var(--muted); gap: 6px; padding-top: 1px; }
  .word { flex: none; font-weight: 500; }
  .word.amber { color: var(--amber); }
  .word.limited { color: var(--amber); opacity: 0.75; }
  .word.red { color: var(--red); }
  .word.accent, .word.ring { color: var(--accent); }
  .word.green, .since.green { color: var(--green); }
  .since { flex: none; font-variant-numeric: tabular-nums; }
  .word + .since::before { content: "· "; }
  .agent { margin-left: 2px; }
  .prs { margin-left: 3px; opacity: 0.8; }
  .kids { flex: none; display: inline-flex; align-items: center; gap: 4px; font-size: 11px; color: var(--muted); margin-left: 6px; }
  .icon { display: grid; flex: none; color: var(--muted); }
  .icon.engine { opacity: 0.8; }
  .top .branch { display: inline; font-family: var(--mono); font-size: 11px; opacity: 0.85; }
  /* Hover buttons sit at the end of the third line, which is empty there, so nothing steps aside. */
  .actions { position: absolute; right: 8px; bottom: 6px; display: none; gap: 2px; }
  .thread:hover .actions, .thread:focus-within .actions { display: flex; }
  .wrap.dragging .thread:hover .actions { display: none; }
  .act {
    width: 22px; height: 20px; border-radius: var(--r-sm); display: grid; place-items: center;
    background: var(--surface-hover); color: var(--text-3); touch-action: none;
  }
  .act:hover { color: var(--text); background: var(--line-strong); }
  .one .open { flex-direction: row; align-items: center; gap: 8px; padding: 6px 10px; color: var(--muted); font-size: 12px; }
  .one .dot { opacity: 0.7; }
  .one .actions { top: 5px; bottom: auto; }
  .one:hover .when { visibility: hidden; }
  .divider { display: flex; align-items: center; gap: 6px; width: 100%; padding: 12px 10px 4px; color: var(--muted); font-size: 12px; border-radius: var(--r-md); }
  .divider:hover { color: var(--text-2); }
  .divider.still:hover { color: var(--muted); }
  .divider.over { color: var(--accent); }
  .divider.hubasks, .divider.hubasks:hover { color: var(--amber); }
  .divider.over .rule { background: var(--accent); }
  .divider .n { font-size: 12px; }
  .link { display: flex; align-items: center; gap: 4px; margin: 2px 0 4px 34px; padding: 3px 4px; font-size: 13px; color: var(--accent); border-radius: var(--r-sm); }
  .link:hover { background: var(--surface-hover); }
  .divider .askdot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); margin-left: 6px; flex: none; }
  .rule { flex: 1; height: 1px; background: var(--line); margin-left: 4px; }
  .chev { display: grid; place-items: center; flex: none; width: 14px; height: 14px; transition: transform 0.12s var(--ease); }
  .activity:global([data-fold]) .chev { transition: transform 0.3s var(--ease); }
  /* A group's rows: the box clips them while it opens and folds (fold.ts). */
  .drawer, .rows { display: flex; flex-direction: column; }
  .rows { gap: 1px; }
  /* Fold out and Steps: each row of a group that opens comes in after the one above it. */
  .stagger > :global(*) { animation: row-in 240ms var(--ease) backwards; }
  .stagger.steps > :global(*) { animation-name: row-step; animation-duration: 300ms; }
  @keyframes row-in { from { opacity: 0; transform: translateY(-6px); } }
  @keyframes row-step { from { opacity: 0; transform: translateY(-16px); } }
  .stagger > :global(:nth-child(2)) { animation-delay: var(--stagger); }
  .stagger > :global(:nth-child(3)) { animation-delay: calc(var(--stagger) * 2); }
  .stagger > :global(:nth-child(4)) { animation-delay: calc(var(--stagger) * 3); }
  .stagger > :global(:nth-child(5)) { animation-delay: calc(var(--stagger) * 4); }
  .stagger > :global(:nth-child(6)) { animation-delay: calc(var(--stagger) * 5); }
  .stagger > :global(:nth-child(n + 7)) { animation-delay: calc(var(--stagger) * 6); }
  .stagger.steps > :global(:nth-child(2)) { animation-delay: 40ms; }
  .stagger.steps > :global(:nth-child(3)) { animation-delay: 80ms; }
  .stagger.steps > :global(:nth-child(4)) { animation-delay: 120ms; }
  .stagger.steps > :global(:nth-child(5)) { animation-delay: 160ms; }
  .stagger.steps > :global(:nth-child(6)) { animation-delay: 200ms; }
  .stagger.steps > :global(:nth-child(7)) { animation-delay: 240ms; }
  .stagger.steps > :global(:nth-child(n + 8)) { animation-delay: 280ms; }
  .chev.open { transform: rotate(90deg); }
  .folder { display: flex; align-items: center; gap: 2px; padding: 8px 6px 2px 4px; }
  .fold { flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; color: var(--text-3); font-size: 13px; padding: 3px 4px; border-radius: var(--r-sm); text-align: left; }
  .fold:hover { color: var(--text); }
  .fold .n { color: var(--muted); font-size: 12px; }
  .fold .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--amber); flex: none; }
  .folder .act { background: none; opacity: 0; }
  .folder:hover .act { opacity: 1; }
  .insert { position: absolute; left: 8px; right: 8px; height: 2px; border-radius: 1px; background: var(--accent); pointer-events: none; }
  .bulk {
    display: flex; align-items: center; gap: 4px; margin: 4px 0; padding: 4px 6px 4px 10px;
    background: var(--accent-bg); color: var(--accent); border-radius: var(--r-md); font-size: 12px;
  }
  .bulk button { width: 24px; height: 22px; display: grid; place-items: center; border-radius: var(--r-sm); color: var(--accent); }
  .bulk button:hover { background: rgba(165, 171, 238, 0.2); }
  .scrim { position: fixed; inset: 0; z-index: 40; }
  .custom {
    position: fixed; z-index: 41; width: 228px; display: flex; flex-direction: column; gap: 8px; padding: 10px;
    background: var(--surface-raised, var(--surface)); border: 1px solid var(--line-strong); border-radius: var(--r-md);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4); font-size: 13px;
  }
  .custom .label { color: var(--muted); font-size: 12px; }
  .custom input { background: var(--bg); color: var(--text); border: 1px solid var(--line); border-radius: var(--r-sm); padding: 4px 8px; font: inherit; color-scheme: dark; }
  .custom .row { display: flex; flex-wrap: wrap; gap: 4px; }
  .preset { font-size: 12px; color: var(--text-3); padding: 2px 6px; border-radius: var(--r-sm); background: var(--surface-hover); }
  .preset:hover { color: var(--text); }
  .go { margin-left: auto; font-size: 12px; color: var(--accent); padding: 2px 8px; border-radius: var(--r-sm); background: var(--accent-bg); }
  .grow { flex: 1; min-width: 0; }
  /* The project name keeps its room and the machine badge gives way first, down to its icon. */
  .top > .grow { flex: 1 1 auto; min-width: 2.5em; }
  .ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .divider .robot { width: 12px; height: 12px; flex: none; fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .empty { color: var(--muted); font-size: 13px; padding: 8px 10px; margin: 0; }
</style>
