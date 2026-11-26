// What right-clicking a thread, project, file or server offers. The same lists back the ··· menus.
import { api, BASE } from "./api";
import { isChatThread } from "./format";
import type { Listener } from "./bindings/Listener";
import { payload } from "./events";
import type { Thread } from "./bindings/Thread";
import type { WorkerProcess } from "./bindings/WorkerProcess";
import { contextMenu, copy, type MenuEntry } from "./menu.svelte";
import type { IconName } from "./components/Icon.svelte";
import { machineGlyph } from "./refs";
import { attachCommand, takeBack, takeOver, takeOverOutside } from "./terminal";
import { letForeground, openUrl } from "./native";
import { preview } from "./preview";
import { DETAILS, detailOf, prefs, setThreadDetail } from "./prefs.svelte";
import { hub, isSnoozed, unread } from "./state.svelte";
import { handing, takers } from "./ownTools.svelte";
import { threadInBag } from "./bag.svelte";
import { looks, MACHINE_KINDS, projectKey } from "./looks.svelte";

/** A rollback waiting for a yes at its checkpoint in the thread: back to snapshot `to`, described by `label`. */
export type UndoAsk = { thread: string; to: string; label: string };

/**
 * Threads a menu asked something of: the thread view starts renaming, shows the handoff being
 * written, or the panel asks before rolling back.
 */
export const pending = $state<{
  rename: string | null;
  handoff: string | null;
  undo: UndoAsk | null;
  /** Threads the custom snooze picker is open for, and where. */
  snooze: { ids: string[]; x: number; y: number } | null;
}>({
  rename: null,
  handoff: null,
  undo: null,
  snooze: null,
});

/** Today or the next day at `hour`:00, local time. */
function at(hour: number, daysAhead = 0): number {
  const d = new Date();
  d.setDate(d.getDate() + daysAhead);
  d.setHours(hour, 0, 0, 0);
  return d.getTime();
}

/** The times a snooze offers, soonest first; an evening that has passed is left out. */
export function snoozeTimes(now = Date.now()): { label: string; at: number; icon: IconName }[] {
  const monday = (8 - new Date(now).getDay()) % 7 || 7;
  const times: { label: string; at: number; icon: IconName }[] = [
    { label: "1 hour", at: now + 3_600_000, icon: "clock" },
    { label: "3 hours", at: now + 3 * 3_600_000, icon: "clock" },
    { label: "This evening", at: at(18), icon: "moon" },
    { label: "Tomorrow morning", at: at(9, 1), icon: "sun" },
    { label: "Next week", at: at(9, monday), icon: "calendar" },
  ];
  return times.filter((t) => t.at > now + 30 * 60_000).sort((a, b) => a.at - b.at);
}

/** The Chat detail choices for a thread: its own level, or following the default (Settings). */
export function detailMenu(id: string): MenuEntry[] {
  const own = prefs.threadDetail[id];
  const fallback = DETAILS.find((d) => d.value === prefs.chatDetail)?.name ?? "Quiet";
  return [
    { label: `Follow the default (${fallback})`, checked: own === undefined, run: () => setThreadDetail(id, null) },
    null,
    ...DETAILS.map((d) => ({ label: d.name, checked: own === d.value, run: () => setThreadDetail(id, d.value) })),
  ];
}

/** The snooze choices for some threads, as menu items. */
export function snoozeMenu(ids: string[]): MenuEntry[] {
  return [
    { label: "Snooze until", heading: true },
    // "In 1 hour", "This evening": the menu already says snooze, so the rows do not repeat it.
    ...snoozeTimes().map((t) => ({
      label: /^\d/.test(t.label) ? `In ${t.label}` : t.label,
      icon: t.icon,
      hint: wake(t.at),
      run: () => hub.snooze(ids, t.at),
    })),
    null,
    {
      label: "Pick date and time…",
      icon: "calendarPlus" as const,
      run: () => {
        const { x, y } = contextMenu.last;
        pending.snooze = { ids, x, y };
      },
    },
  ];
}

const wake = (t: number) => new Date(t).toLocaleString([], { weekday: "short", hour: "2-digit", minute: "2-digit" });

/** Opens the checkpoint marker in the thread, which asks before rolling its repository back. */
export function askUndo(ask: UndoAsk) {
  pending.undo = ask;
}

/** The first checkpoint of a thread: its repository before the thread did anything. */
export function firstCheckpoint(threadId: string): string | undefined {
  const first = hub.events[threadId]?.find((e) => e.kind === "checkpoint");
  return (first && payload(first, "checkpoint"))?.commit;
}

const attempt = (f: () => Promise<unknown>) => () => f().catch((e) => hub.fail(e));

/** Shows a file or folder in Explorer on the computer Hub runs on. */
export const reveal = (path: string) => attempt(async () => {
  await letForeground();
  await api.reveal(path);
});

/** Opens a folder in VS Code, a terminal or Explorer. */
const openFolder = (path: string, how: "editor" | "terminal" | "files") => attempt(() => api.openFolder(path, how));
const folderItems = (path: string): MenuEntry[] => [
  { label: "Open in editor", run: openFolder(path, "editor") },
  { label: "Open terminal here", run: openFolder(path, "terminal") },
  { label: "Show folder", run: openFolder(path, "files") },
];

/** Joins a project folder and a path inside it, with the folder's own separators. */
export function join(base: string, rel: string) {
  const sep = base.includes("\\") ? "\\" : "/";
  return rel ? base.replace(/[\\/]+$/, "") + sep + rel.replace(/[\\/]+/g, sep) : base;
}

/** Stops the thread's engine and starts it again on the same conversation, so skills, MCP servers
 *  or plugins added since are picked up (T3 Code's "Restart agent session"). A turn in progress
 *  is stopped. */
export async function restartAgent(t: Thread) {
  await api.shutdown(t.id, false);
  await api.warm(t.id);
  hub.tell("Agent restarted; the conversation carries on");
}

/** A link that opens Hub with `text` ready to send in `cwd` (`promptlink.rs`); it never runs anything. */
export function promptLink(text: string, cwd: string): string {
  const q = new URLSearchParams({ prompt: text.slice(0, 8000), folder: cwd });
  return `${BASE}/new?${q}`;
}

/** The icon a machine gets in a list: what kind of computer Hub found there. */
const hostIcon = (name: string, local: boolean): IconName => {
  const g = machineGlyph(name, local);
  return g === "monitor" || g === "laptop" || g === "cloud" ? g : "server";
};

/** A chat's menu: what an everyday chat can do. Nothing of the code side (machines, terminals,
 *  folders, projects), and nothing that would start a thread in Dev. */
function chatMenu(t: Thread): MenuEntry[] {
  const snoozed = isSnoozed(t, hub.now);
  return [
    !(hub.view === "thread" && hub.selected === t.id) && { label: "Open", icon: "threads", run: () => hub.open(t.id) },
    {
      label: "Rename",
      icon: "pen",
      hint: "F2",
      run: () => {
        hub.open(t.id);
        pending.rename = t.id;
      },
    },
    !t.archived && { label: t.pinned_at != null ? "Unpin" : "Pin", icon: "pin", run: () => hub.pin([t.id], t.pinned_at == null) },
    !t.archived && (snoozed
      ? { label: "Wake chat", icon: "sun", hint: wake(t.snoozed_until!), run: () => hub.snooze([t.id], 0) }
      : { label: "Snooze", icon: "clock", children: snoozeMenu([t.id]) }),
    !t.archived && t.returned_at != null && !unread(t) && t.status !== "running" && { label: "Mark unread", icon: "eye", run: () => hub.markUnread(t.id) },
    null,
    {
      label: "Continue in a new chat",
      icon: "arrowRight",
      run: attempt(async () => {
        pending.handoff = t.id;
        try {
          hub.startDraft(t.cwd, (await api.handoff(t.id)).text, t.host ?? null);
        } finally {
          pending.handoff = null;
        }
      }),
    },
    { label: "Copy", icon: "copy", children: [
      { label: "As @ reference", icon: "at", hint: "Ctrl Shift C", run: () => copy(`@${t.title} `) },
      { label: "Thread id", icon: "hash", run: () => copy(t.id) },
    ] },
    { label: "Regenerate title", icon: "wand", hint: "From the conversation", run: () => hub.retitle(t.id) },
    null,
    !t.archived && { label: t.settled_at ? "Back in the list" : "Settle", icon: "check", run: () => hub.settle(t.id, !t.settled_at) },
    { label: t.archived ? "Unarchive" : "Archive", icon: "archive", run: () => hub.setArchived(t.id, !t.archived) },
    { label: "Delete…", icon: "trash", confirm: "Delete for good?", danger: true, run: () => hub.remove(t.id) },
  ];
}

export function threadMenu(t: Thread): MenuEntry[] {
  if (isChatThread(t)) return chatMenu(t);
  const home = t.host ?? hub.localHost;
  const busy = t.status === "running" || t.status === "waiting_on_you";
  const pr = hub.prs[t.id];
  const snoozed = isSnoozed(t, hub.now);
  const focused = prefs.focusOn?.cwd === t.cwd && prefs.focusOn.host === (t.host && t.host !== hub.localHost ? t.host : null);
  // Where this thread's work could go: the other machines that are on.
  const others = hub.hosts.filter((h) => h.state === "online" && h.name !== home);
  const movable = !t.archived && t.status !== "in_terminal";
  return [
    !(hub.view === "thread" && hub.selected === t.id) && { label: "Open", icon: "threads", run: () => hub.open(t.id) },
    { label: "New thread here", icon: "plus", run: () => hub.startDraft(t.cwd, "", t.host ?? null) },
    {
      label: "Rename",
      icon: "pen",
      hint: "F2",
      run: () => {
        hub.open(t.id);
        pending.rename = t.id;
      },
    },
    !t.archived && { label: t.pinned_at != null ? "Unpin" : "Pin", icon: "pin", run: () => hub.pin([t.id], t.pinned_at == null) },
    !t.archived && (snoozed
      ? { label: "Wake thread", icon: "sun", hint: wake(t.snoozed_until!), run: () => hub.snooze([t.id], 0) }
      : { label: "Snooze", icon: "clock", children: snoozeMenu([t.id]) }),
    !t.archived && snoozed && { label: "Snooze again", icon: "clock", children: snoozeMenu([t.id]) },
    !t.archived && t.returned_at != null && !unread(t) && t.status !== "running" && { label: "Mark unread", icon: "eye", run: () => hub.markUnread(t.id) },
    !t.archived && { label: "Chat detail", icon: "detail", hint: DETAILS.find((d) => d.value === detailOf(t.id))?.name, children: detailMenu(t.id) },
    null,
    {
      label: "Continue in a new thread",
      icon: "arrowRight",
      run: attempt(async () => {
        pending.handoff = t.id;
        try {
          hub.startDraft(t.cwd, (await api.handoff(t.id)).text, t.host ?? null);
        } finally {
          pending.handoff = null;
        }
      }),
    },
    // The same thread, carried on over there with its code and uncommitted changes.
    movable && {
      label: "Move to",
      icon: "swap",
      children: [
        { label: "Same thread, code too", heading: true },
        ...others.map((h) => ({
          label: h.name,
          icon: hostIcon(h.name, !!h.local),
          hint: busy ? "Stops this turn" : undefined,
          // It leaves this list and shows on `h` (the move logs a line in the thread).
          run: attempt(async () => {
            await hub.moveThread(t, h.name, busy);
          }),
        })),
      ],
    },
    {
      label: "Take over",
      icon: "terminal",
      children: [
        t.status === "in_terminal"
          ? { label: "Take back from the terminal", icon: "back", run: attempt(() => takeBack(t)) }
          : !!t.engine_session_id && { label: "In Hub's terminal", icon: "terminal", run: attempt(() => takeOver(t)) },
        t.status !== "in_terminal" && !!t.engine_session_id && {
          label: "In a window outside Hub",
          icon: "external",
          run: attempt(async () => hub.tell(await takeOverOutside(t))),
        },
        !!t.engine_session_id && { label: "Copy attach command", icon: "copy", hint: "hubd attach", run: () => copy(attachCommand(t)) },
      ],
    },
    // Your own tools that take work, such as "Hand to Knegarn", on the thread's machine.
    ...takers.list
      .filter((k) => (k.host ?? hub.localHost) === home && !t.archived)
      .map((k) => ({ label: `${k.label}…`, icon: "share" as const, hint: k.about ?? undefined, run: () => (handing.to = { thread: t.id, taker: k }) })),
    null,
    {
      label: "Open in",
      icon: "folder",
      children: [
        { label: "Project page", icon: "page", run: () => hub.openProject(t.cwd, t.host ?? null) },
        pr && { label: `Pull request #${pr.number}`, icon: "pr", run: () => openUrl(pr.url) },
        // Folders on another machine cannot be opened here.
        ...(t.host ? [] : [null, { label: "Editor", icon: "code" as const, run: openFolder(t.cwd, "editor") }, { label: "Terminal here", icon: "terminal" as const, run: openFolder(t.cwd, "terminal") }, { label: "Show folder", icon: "folder" as const, run: openFolder(t.cwd, "files") }]),
      ],
    },
    {
      label: "Copy",
      icon: "copy",
      children: [
        { label: "Folder path", icon: "folder", run: () => copy(t.cwd) },
        { label: "As @ reference", icon: "at", hint: "Ctrl Shift C", run: () => copy(`@${t.title} `) },
        { label: "Thread id", icon: "hash", run: () => copy(t.id) },
        pr && { label: "Pull request link", icon: "pr", run: () => copy(pr.url) },
      ],
    },
    focused
      ? { label: "Show all projects", icon: "target", run: () => hub.leaveFocus() }
      : { label: "Show only this project", icon: "target", run: () => hub.enterFocus(t.cwd, t.host ?? null) },
    {
      label: "More",
      icon: "more",
      children: [
        { label: "Regenerate title", icon: "wand", hint: "From the conversation", run: () => hub.retitle(t.id) },
        !t.archived && {
          label: "Settle by itself",
          icon: "stamp",
          hint: "Idle or PR merged",
          checked: !!t.auto_settle,
          run: () => hub.setAutoSettle(t.id, !t.auto_settle),
        },
        null,
        { label: "Put in the bag", icon: "bag", hint: "Take it into another thread", run: () => threadInBag(t.id) },
        !t.host && t.status !== "in_terminal" && {
          label: "Restart the agent",
          icon: "restart",
          hint: "New skills, MCP servers",
          confirm: t.status === "running" ? "Stop the turn and restart?" : undefined,
          run: attempt(() => restartAgent(t)),
        },
        firstCheckpoint(t.id) && { label: "Get a second opinion", icon: "spark", hint: "Another model reviews", run: attempt(() => api.review(t.id)) },
        firstCheckpoint(t.id) && {
          label: "Roll back everything…",
          icon: "undo",
          hint: "Files and commits",
          run: () => {
            hub.open(t.id);
            askUndo({ thread: t.id, to: firstCheckpoint(t.id)!, label: "the thread started" });
          },
        },
      ],
    },
    null,
    !t.archived && { label: t.settled_at ? "Back in the list" : "Settle", icon: "check", run: () => hub.settle(t.id, !t.settled_at) },
    { label: t.archived ? "Unarchive" : "Archive", icon: "archive", run: () => hub.setArchived(t.id, !t.archived) },
    { label: "Delete…", icon: "trash", confirm: "Delete for good?", danger: true, run: () => hub.remove(t.id) },
  ];
}

export function projectMenu(cwd: string, host: string | null = null): MenuEntry[] {
  const added = !host && hub.projects.some((p) => p.path === cwd);
  const open = hub.view === "project" && hub.project === cwd && hub.projectHost === host;
  const focused = prefs.focusOn?.cwd === cwd && prefs.focusOn.host === (host && host !== hub.localHost ? host : null);
  return [
    !open && { label: "Open project page", run: () => hub.openProject(cwd, host) },
    { label: "New thread here", run: () => hub.startDraft(cwd, "", host) },
    { label: "Go in", hint: "Focus with its desk", run: () => hub.goIn(cwd, host) },
    focused
      ? { label: "Leave focus", run: () => hub.leaveFocus() }
      : { label: "Focus on this project", hint: "Only it shows", run: () => hub.enterFocus(cwd, host) },
    { label: "Change icon…", run: () => {
      const name = hub.projects.find((p) => p.path === cwd)?.name ?? cwd.split(/[\\/]/).filter(Boolean).pop() ?? cwd;
      looks.picking = { key: projectKey(cwd, host), name, ...contextMenu.last };
    } },
    null,
    ...(host ? [] : folderItems(cwd)),
    { label: "Copy path", run: () => copy(cwd) },
    null,
    added && { label: "Remove from Hub", hint: "Folder stays", run: () => hub.removeProject(cwd) },
  ];
}

/** Which icon a machine shows: the kind of computer Hub found, or one picked here. */
export function machineKindMenu(name: string, local: boolean): MenuEntry[] {
  const now = looks.machineKind(name, local);
  const picked = looks.machinePicked(name);
  return [
    ...MACHINE_KINDS.map((k) => ({
      label: k.label,
      hint: k.kind === now ? (picked ? "Picked" : "Found") : undefined,
      run: () => looks.setMachine(name, k.kind),
    })),
    picked && null,
    picked && { label: "Tell by itself", hint: "What the machine says it is", run: () => looks.setMachine(name, null) },
  ];
}

/** A file or folder. `open` shows it inside Hub when the place it was clicked can. */
/** `remote`: the file lives on another machine, where this computer's Explorer cannot show it. */
export function fileMenu(path: string, opts: { rel?: string; open?: () => void; remote?: boolean } = {}): MenuEntry[] {
  return [
    opts.open && { label: "Open", run: opts.open },
    !opts.remote && { label: "Show in folder", run: reveal(path) },
    null,
    { label: "Copy path", run: () => copy(path) },
    opts.rel && { label: "Copy relative path", run: () => copy(opts.rel!.replace(/\\/g, "/")) },
  ];
}

/** A data URL's bytes. Not through fetch: the desktop app's policy lets no data: URL be fetched. */
export function dataBlob(src: string): Blob {
  const bytes = Uint8Array.from(atob(src.slice(src.indexOf(",") + 1)), (c) => c.charCodeAt(0));
  return new Blob([bytes], { type: src.slice(5, src.indexOf(";")) });
}

/** Puts a picture, given as a data URL, on the clipboard. The clipboard takes PNG, so anything
 *  else is drawn into one first. */
export async function copyImage(src: string) {
  let blob = dataBlob(src);
  if (blob.type !== "image/png") {
    const img = new Image();
    img.src = src;
    await img.decode();
    const canvas = Object.assign(document.createElement("canvas"), { width: img.naturalWidth, height: img.naturalHeight });
    canvas.getContext("2d")!.drawImage(img, 0, 0);
    blob = await new Promise<Blob>((ok, fail) => canvas.toBlob((b) => (b ? ok(b) : fail(new Error("Could not copy the picture"))), "image/png"));
  }
  await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
}

/** A picture in the chat: `src` is a data URL, `path` where the engine saved it. */
export function imageMenu(src: string, path: string | undefined, open: () => void): MenuEntry[] {
  return [
    { label: "Open", run: open },
    { label: "Copy image", run: attempt(() => copyImage(src)) },
    ...(path ? [null, ...fileMenu(path)] : []),
  ];
}

/** A web link shown as a button rather than a link. */
export function linkMenu(url: string): MenuEntry[] {
  return [
    { label: "Open link", run: attempt(() => openUrl(url)) },
    { label: "Copy link", run: () => copy(url) },
  ];
}

type Server = Pick<Listener, "port" | "pid" | "cwd"> & Partial<Pick<Listener, "process" | "thread_id" | "is_hub" | "system">>;

/** A port something is listening on. */
export function serverMenu(l: Server): MenuEntry[] {
  const url = `http://localhost:${l.port}`;
  const thread = l.thread_id ? hub.threads.find((t) => t.id === l.thread_id) : undefined;
  const stoppable = l.pid != null && !l.is_hub && !l.system;
  return [
    thread && { label: "Open beside the thread", run: () => (hub.open(thread.id), preview(url, thread.id)) },
    { label: "Open in browser", run: attempt(() => openUrl(url)) },
    { label: "Copy address", run: () => copy(url) },
    null,
    thread && { label: "Open thread", run: () => hub.open(thread.id) },
    l.cwd && { label: "Show folder", run: reveal(l.cwd) },
    l.pid != null && { label: "Copy process id", hint: String(l.pid), run: () => copy(String(l.pid)) },
    null,
    stoppable && {
      label: "Stop",
      hint: l.process ?? undefined,
      danger: true,
      run: attempt(async () => {
        await api.stopProcess(l.pid!);
        await hub.refreshHost();
      }),
    },
  ];
}

/**
 * A process in the Workers view. `engine` marks a thread's engine, which is shut down with the
 * thread rather than stopped or paused on its own. `done` refreshes the view after an action.
 */
export function processMenu(p: WorkerProcess, opts: { thread?: string; engine?: boolean; hub?: boolean; done: () => unknown }): MenuEntry[] {
  const act = (f: (pid: number) => Promise<void>) => attempt(async () => {
    await f(p.pid);
    await opts.done();
  });
  const touchable = !opts.engine && !opts.hub;
  return [
    ...p.ports.map((port) => ({ label: `Open :${port} in browser`, run: attempt(() => openUrl(`http://localhost:${port}`)) })),
    p.ports.length > 0 && null,
    opts.thread && hub.threads.some((t) => t.id === opts.thread) && { label: "Open thread", run: () => hub.open(opts.thread!) },
    p.cwd && { label: "Show folder", run: reveal(p.cwd) },
    p.command && { label: "Copy command", run: () => copy(p.command) },
    { label: "Copy process id", hint: String(p.pid), run: () => copy(String(p.pid)) },
    touchable && null,
    touchable && (p.paused
      ? { label: "Resume", run: act(api.resumeProcess) }
      : { label: "Pause", hint: "with what it started", run: act(api.pauseProcess) }),
    touchable && { label: "Stop", hint: "with what it started", danger: true, run: act(api.stopProcess) },
  ];
}

/** A message in a thread. `undo` asks to roll back to before it, when there is a checkpoint. */
export function messageMenu(text: string, undo?: () => void, cwd?: string, edit?: () => void): MenuEntry[] {
  return [
    text && { label: "Copy message", run: () => copy(text) },
    edit && { label: "Edit from here…", run: edit },
    text && cwd && { label: "Copy prompt link", hint: "Opens a draft of it", run: () => copy(promptLink(text, cwd)) },
    undo && null,
    undo && { label: "Roll back to before this…", run: undo },
  ];
}
