<!-- Ctrl K. One box for everything you can jump to or do. Type to search all of it, or start with a sign to search one kind: > actions, @ projects, # pull requests and branches, " what was said, / files in the project you are in, ! what needs you. In a thread or on a project page the actions for that thread and project come first. Alt Enter opens the row's own menu.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { modal } from "../pop";
  // Ctrl K. One box for everything you can jump to or do. Type to search all of it, or start with a
  // sign to search one kind: > actions, @ projects, # pull requests and branches, " what was said,
  // / files in the project you are in, ! what needs you. In a thread or on a project page the
  // actions for that thread and project come first. Alt Enter opens the row's own menu.
  import { ago, byProject, folderName, isChatThread, threadProject } from "../format";
  import { api } from "../api";
  import { inspect } from "../inspect.svelte";
  import type { HistoryHit } from "../bindings/HistoryHit";
  import type { Thread } from "../bindings/Thread";
  import { hub, needsYou, waitsOnYou } from "../state.svelte";
  import { prefs } from "../prefs.svelte";
  import { restartAgent, threadMenu, projectMenu } from "../menus.svelte";
  import { contextMenu, type MenuEntry } from "../menu.svelte";
  import { frecency } from "../visits";
  import { takeBack, takeOver } from "../terminal";
  import { showable } from "../filePreview";
  import { openUrl } from "../native";
  import { machineGlyph } from "../refs";
  import Icon, { type IconName } from "./Icon.svelte";
  import Ref from "./Ref.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";

  let { onclose, onnew }: { onclose: () => void; onnew: () => void } = $props();

  type Mode = "actions" | "projects" | "prs" | "said" | "files" | "needs";
  type Entry = {
    id: string;
    label: string;
    /** Searchable, and shown in grey on an action. */
    detail: string;
    run: () => unknown;
    section: string;
    glyph?: IconName;
    /** A thread's status ring. */
    dot?: string;
    keys?: string[];
    /** A second line under the label. */
    sub?: string;
    chevron?: boolean;
    /** Stays open after running (it changes what the box shows). */
    stay?: boolean;
    thread?: Thread;
    project?: { cwd: string; host: string | null; name: string };
    age?: string;
    state?: string;
    menu?: () => MenuEntry[];
    find?: string;
    rank?: number;
  };

  const MODES: { sign: string; mode: Mode; label: string; placeholder: string }[] = [
    { sign: ">", mode: "actions", label: "Actions", placeholder: "Run an action…" },
    { sign: "@", mode: "projects", label: "Projects", placeholder: "Find a project…" },
    { sign: "#", mode: "prs", label: "PR or branch", placeholder: "A pull request number or a branch…" },
    { sign: '"', mode: "said", label: "What was said", placeholder: "Words from any thread…" },
    { sign: "/", mode: "files", label: "Files", placeholder: "A file in this project…" },
    { sign: "!", mode: "needs", label: "Needs you", placeholder: "Threads waiting on you…" },
  ];

  let query = $state("");
  let mode = $state<Mode | null>(null);
  let drill = $state<"newThread" | null>(null);
  let selected = $state(0);
  let list: HTMLDivElement;

  const modeInfo = $derived(MODES.find((m) => m.mode === mode));
  const placeholder = $derived(drill ? "Which project?" : (modeInfo?.placeholder ?? "Search threads, projects and actions…"));

  // --- where you are ---------------------------------------------------------------------------

  // The two worlds are walled apart here too: Chat searches its chats and has no projects, Dev has
  // no chats.
  const chatWorld = $derived(hub.world === "chat");
  const ctxThread = $derived(hub.view === "thread" && hub.current ? hub.current : null);
  const projectList = $derived(chatWorld ? [] : byProject(hub.everything, hub.projects));
  const ctxProject = $derived.by(() => {
    if (chatWorld) return null;
    const t = ctxThread;
    const cwd = t ? threadProject(t) : hub.view === "project" ? hub.project : null;
    if (!cwd) return null;
    const host = t ? (t.host ?? null) : hub.projectHost;
    const found = projectList.find((p) => p.cwd === cwd && (p.host ?? null) === (host ?? null));
    return { cwd, host: host ?? null, name: found?.name ?? folderName(cwd) };
  });

  function projectOf(t: Thread) {
    const cwd = threadProject(t);
    const found = projectList.find((p) => p.cwd === cwd && (p.host ?? null) === (t.host ?? null));
    return { cwd, host: t.host ?? null, name: found?.name ?? folderName(cwd) };
  }

  // --- the things you can pick -----------------------------------------------------------------

  const KEYS: Record<string, string[]> = {
    "Ctrl N": ["Ctrl", "N"], "Ctrl I": ["Ctrl", "I"], "Ctrl J": ["Ctrl", "J"], "Ctrl B": ["Ctrl", "B"],
    "Ctrl E": ["Ctrl", "E"], "Ctrl ,": ["Ctrl", ","], "Ctrl .": ["Ctrl", "."], "Ctrl §": ["Ctrl", "§"], "Ctrl Shift I": ["Ctrl", "Shift", "I"],
  };
  const keysOf = (shortcut: string) => KEYS[shortcut];

  const everywhere: Entry[] = [
    { id: "a:new", glyph: "plus" as const, label: "New thread", detail: "", keys: keysOf("Ctrl N"), run: () => onnew() },
    { id: "a:project", glyph: "folder" as const, label: "New project", detail: "", run: () => (hub.newProjectOpen = true) },
    { id: "a:idea", glyph: "bulb" as const, label: "An idea: put it in the right project's focus", detail: "", keys: keysOf("Ctrl I"), run: () => (hub.ideaOpen = true) },
    { id: "a:inspect", glyph: "eye" as const, label: "Inspect Hub: mark elements and start a thread from them", detail: "", keys: keysOf("Ctrl Shift I"), run: () => inspect.toggle() },
    { id: "a:waiting", glyph: "bell" as const, label: "Next thread that needs you", detail: "", keys: keysOf("Ctrl J"), run: () => hub.nextWaiting() },
    { id: "a:home", glyph: "home" as const, label: "Go home", detail: "", run: () => hub.goHome() },
    { id: "a:welcome", glyph: "help" as const, label: "Välkommen: en kort guide till Hub", detail: "", run: () => { hub.view = "welcome"; hub.selected = null; } },
    { id: "a:machines", glyph: "monitor" as const, label: "Machines", detail: "", run: () => { hub.view = "machines"; hub.selected = null; } },
    { id: "a:services", glyph: "services" as const, label: "Services on this computer", detail: "", run: () => { hub.view = "services"; hub.selected = null; } },
    { id: "a:sidebar", glyph: "sidebar" as const, label: "Toggle sidebar", detail: "", keys: keysOf("Ctrl B"), run: () => (hub.sidebarOpen = !hub.sidebarOpen) },
    {
      id: "a:activity",
      glyph: "swap" as const,
      label: hub.room === "work" ? "Go to System: projects and machines" : "Go to Inbox: your threads",
      detail: "",
      keys: keysOf("Ctrl E"),
      run: () => {
        hub.switchRoom();
        hub.sidebarOpen = true;
      },
    },
    hub.halt
      ? { id: "a:halt", glyph: "play" as const, label: "Resume every agent", detail: "", run: () => hub.resumeAll() }
      : { id: "a:halt", label: "Pause every agent on every machine", detail: "panic button", run: () => hub.pauseAll() },
    // Leaving: what is working here carries on on a machine that stays on.
    ...hub.hosts
      .filter((h) => !h.local && h.state === "online")
      .map((h) => ({
        id: `a:leaving:${h.name}`,
        label: `Leaving: move my running threads to ${h.name}`,
        detail: "stops their turns; they carry on there",
        run: () => hub.leaving(h.name),
      })),
    ...(prefs.focusOn
      ? [{ id: "a:focus", glyph: "target" as const, label: `Leave focus on ${prefs.focusOn.name}`, detail: prefs.held.length ? `${prefs.held.length} held` : "focus mode", run: () => hub.leaveFocus() }]
      : []),
    { id: "a:settings", label: "Settings", detail: "", keys: keysOf("Ctrl ,"), glyph: "gear" as const, run: () => hub.openSettings() },
    { id: "a:panel", label: "Toggle right panel", detail: "", keys: keysOf("Ctrl ."), glyph: "panel" as const, run: () => (hub.panelOpen = !hub.panelOpen) },
    { id: "a:terminal", label: "Toggle terminal", detail: "", keys: keysOf("Ctrl §"), glyph: "terminal", run: () => hub.toggleTerminal() },
  ]
    // Chat has no machines, no Home, no focus: only what is about chatting.
    .filter((e) => !chatWorld || ["a:new", "a:sidebar", "a:settings"].includes(e.id))
    .map((e) => ({ ...e, section: "Everywhere" }) as Entry);

  // Settings pages: found by typing, not listed when the box is empty.
  const settingsPages: Entry[] = (
    [
      ["account", "Account and usage"],
      ["appearance", "Appearance"],
      ["notifications", "Notifications"],
      ["dictation", "Dictation"],
      ["chat", "Chat and threads"],
      ["engines", "Engines"],
      ["safety", "Safety"],
      ["projects", "Projects and tools"],
      ["extensions", "Extensions"],
      ["machines", "Machines and SSH"],
      ["access", "Access"],
      ["updates", "Updates"],
      ["whatsnew", "What's new"],
      ["privacy", "Privacy and data"],
      ["about", "About and help"],
    ] as const
  ).map(([s, label]) => ({ id: `a:settings:${s}`, label: `Settings: ${label}`, detail: "", glyph: "gear" as const, section: "Settings", run: () => hub.openSettings(s) }));

  /** What you can do with the thread you are in. */
  const threadActions = $derived.by<Entry[]>(() => {
    const t = ctxThread;
    if (!t) return [];
    const section = `In ${t.title}`;
    const home = t.host ?? hub.localHost;
    const busy = t.status === "running" || t.status === "waiting_on_you";
    const pr = hub.prs[t.id];
    const out: Entry[] = [];
    const add = (id: string, label: string, glyph: IconName, run: () => unknown, detail = "") => out.push({ id: `t:${id}`, label, detail, glyph, section, run });
    if (!t.archived) add("settle", t.settled_at ? "Put this thread back in the list" : "Settle this thread", "check", () => hub.settle(t.id, !t.settled_at));
    if (!t.archived) add("pin", t.pinned_at != null ? "Unpin this thread" : "Pin this thread", "pin", () => hub.pin([t.id], t.pinned_at == null));
    if (!t.archived && t.status !== "in_terminal" && !chatWorld) {
      for (const h of hub.hosts.filter((h) => h.state === "online" && h.name !== home)) {
        add(`move:${h.name}`, `Move to ${h.name}…`, "swap", () => hub.moveThread(t, h.name, busy).catch((e) => hub.fail(e)), busy ? "stops this turn" : "same thread, code too");
      }
    }
    if (chatWorld) {
      add("archive", t.archived ? "Unarchive" : "Archive", "archive", () => hub.setArchived(t.id, !t.archived));
      return out;
    }
    if (t.status === "in_terminal") add("back", "Take back from the terminal", "back", () => takeBack(t).catch((e) => hub.fail(e)));
    else if (t.engine_session_id) add("take", "Take over in the terminal", "terminal", () => takeOver(t).catch((e) => hub.fail(e)));
    if (!t.host && t.status !== "in_terminal") add("restart", "Restart the agent", "restart", () => restartAgent(t).catch((e) => hub.fail(e)), "picks up new skills and MCP servers");
    if (pr) add("pr", `Open pull request #${pr.number}`, "pr", () => openUrl(pr.url), pr.state);
    add("archive", t.archived ? "Unarchive" : "Archive", "archive", () => hub.setArchived(t.id, !t.archived));
    return out;
  });

  /** What you can do in the project you are in. */
  const projectActions = $derived.by<Entry[]>(() => {
    const p = ctxProject;
    if (!p) return [];
    const section = p.name;
    return [
      { id: "c:new", label: `New thread in ${p.name}`, detail: "", glyph: "plus", keys: keysOf("Ctrl N"), section, run: () => hub.startDraft(p.cwd, "", p.host) },
      { id: "c:newin", label: "New thread in…", detail: "pick the project", glyph: "plus", chevron: true, stay: true, section, run: () => { drill = "newThread"; query = ""; } },
      { id: "c:files", label: `Go to file in ${p.name}`, detail: "", glyph: "file", section, stay: true, run: () => { mode = "files"; query = ""; } },
      { id: "c:said", label: "Search what was said", detail: "words from any thread", glyph: "search", section, stay: true, run: () => { mode = "said"; query = ""; } },
      { id: "c:page", label: `Open ${p.name}'s page`, detail: "", glyph: "folder", section, run: () => hub.openProject(p.cwd, p.host) },
    ] as Entry[];
  });

  const projects = $derived<Entry[]>(
    // Search reaches every project, in focus mode too: it is something you ask for.
    projectList.map((p) => ({
      id: `p:${p.key}`,
      label: p.name,
      detail: `project ${p.cwd}`,
      section: "Projects",
      project: { cwd: p.cwd, host: p.host, name: p.name },
      age: p.threads[0] ? ago(p.threads[0].updated_at) : undefined,
      menu: () => projectMenu(p.cwd, p.host),
      run: () => hub.openProject(p.cwd, p.host),
    })),
  );

  const projectVerbs = $derived<Entry[]>(
    projectList.flatMap((p) => [
      { id: `g:${p.key}`, label: `Go into ${p.name}`, detail: "its desk · steer its threads from one chat", glyph: "arrowRight" as const, section: "Actions", run: () => hub.goIn(p.cwd, p.host) },
      { id: `f:${p.key}`, label: `Focus on ${p.name}`, detail: "focus mode · only this project shows", glyph: "target" as const, section: "Actions", run: () => hub.enterFocus(p.cwd, p.host) },
    ]),
  );

  const drillProjects = $derived<Entry[]>(
    projectList.map((p) => ({
      id: `n:${p.key}`,
      label: p.name,
      detail: p.cwd,
      section: "New thread in",
      project: { cwd: p.cwd, host: p.host, name: p.name },
      run: () => hub.startDraft(p.cwd, "", p.host),
    })),
  );

  // Threads you open often come first, and a branch or pull request number finds its thread.
  function threadEntry(t: Thread, section: string): Entry {
    const proj = projectOf(t);
    const pr = hub.prs[t.id];
    return {
      id: `${section}:${t.id}`,
      label: t.title,
      detail: [t.owner?.name, proj.name, t.host, t.archived && "archived"].filter(Boolean).join(" "),
      find: [hub.branches[t.id], pr && `#${pr.number} ${pr.title}`].filter(Boolean).join(" "),
      section,
      dot: t.status === "idle" && !t.settled_at ? (t.returned_at != null && (t.seen_at == null || t.seen_at < t.returned_at) ? "idle unread" : "idle") : t.status,
      thread: t,
      project: proj,
      age: ago(t.updated_at),
      rank: frecency(t.id),
      menu: () => threadMenu(t),
      run: () => hub.open(t.id),
    };
  }

  const allThreads = $derived<Thread[]>(
    chatWorld
      ? [...hub.chats, ...hub.archived.filter(isChatThread)]
      : [...hub.everything, ...hub.systemThreads.filter((t) => !hub.everything.includes(t)), ...hub.archived.filter((t) => !isChatThread(t))],
  );
  const threads = $derived<Entry[]>(allThreads.map((t) => threadEntry(t, "Threads")).sort((a, b) => b.rank! - a.rank!));
  const needing = $derived<Entry[]>(
    allThreads
      .filter((t) => !t.archived && waitsOnYou(t))
      .sort((a, b) => b.updated_at - a.updated_at)
      .map((t) => threadEntry(t, "Needs you")),
  );

  const prEntries = $derived.by<Entry[]>(() => {
    const seen = new Set<number>();
    const out: Entry[] = [];
    for (const t of allThreads) {
      const pr = hub.prs[t.id];
      if (pr && !seen.has(pr.number)) {
        seen.add(pr.number);
        out.push({ id: `pr:${pr.number}`, label: `#${pr.number} ${pr.title}`, detail: t.title, sub: t.title, section: "Pull requests", glyph: "pr", state: pr.state, age: ago(t.updated_at), thread: t, run: () => hub.open(t.id), menu: () => threadMenu(t) });
      }
      const branch = hub.branches[t.id];
      if (branch && branch !== "main" && branch !== "master") {
        out.push({ id: `br:${t.id}`, label: branch, detail: t.title, sub: t.title, section: "Branches", glyph: "branch", age: ago(t.updated_at), thread: t, run: () => hub.open(t.id), menu: () => threadMenu(t) });
      }
    }
    return out;
  });

  // Words in what you wrote to threads, from the core's history, once typing pauses.
  let found = $state<Entry[]>([]);
  let asked = 0;
  $effect(() => {
    const q = query.trim();
    const ask = ++asked;
    const wanted = mode === "said" || (mode === null && !drill);
    if (!wanted || q.length < 3) {
      found = [];
      return;
    }
    const timer = setTimeout(async () => {
      const hits: HistoryHit[] = await quiet(api.history(q, -new Date().getTimezoneOffset()), "history", []);
      if (ask !== asked) return;
      const seen = new Set<string>();
      found = hits
        .filter((h) => h.kind === "message" && h.thread_id && allThreads.some((t) => t.id === h.thread_id))
        .filter((h) => !seen.has(h.thread_id!) && !!seen.add(h.thread_id!))
        .slice(0, mode === "said" ? 20 : 6)
        .map((h) => ({
          id: `m:${h.thread_id}`,
          label: h.text,
          detail: "",
          sub: `in ${h.title ?? "a thread"}${h.ts ? ` · ${ago(h.ts)}` : ""}`,
          glyph: "threads" as const,
          section: "What was said",
          run: () => hub.open(h.thread_id!),
        }));
    }, 200);
    return () => clearTimeout(timer);
  });

  // Files in the project you are in.
  let files = $state<Entry[]>([]);
  let fileAsk = 0;
  $effect(() => {
    const q = query.trim();
    const ask = ++fileAsk;
    const p = ctxProject;
    if (mode !== "files" || !p || !q) {
      files = [];
      return;
    }
    const timer = setTimeout(async () => {
      const hits = await quiet(api.filesIn(p.cwd, q, p.host), "files in", []);
      if (ask !== fileAsk) return;
      files = hits
        .filter((f) => !f.dir)
        .slice(0, 30)
        .map((f) => ({
          id: `f:${f.path}`,
          label: f.rel.split("/").pop() ?? f.rel,
          detail: f.rel,
          sub: f.rel.includes("/") ? f.rel.slice(0, f.rel.lastIndexOf("/")) : undefined,
          glyph: "file" as const,
          section: `Files in ${p.name}`,
          run: () => {
            if (ctxThread && showable(f.path)) hub.viewing = { thread: ctxThread.id, kind: "file", id: f.path };
            else hub.openProject(p.cwd, p.host);
          },
        }));
    }, 150);
    return () => clearTimeout(timer);
  });

  // --- finding ---------------------------------------------------------------------------------

  /** Letters in order, closer together and earlier is better. -1 when it does not match. */
  function score(text: string, q: string): number {
    if (!q) return 0;
    const t = text.toLowerCase();
    const direct = t.indexOf(q);
    if (direct >= 0) return 1000 - direct;
    let pos = -1, gaps = 0;
    for (const ch of q) {
      const next = t.indexOf(ch, pos + 1);
      if (next < 0) return -1;
      gaps += next - pos - 1;
      pos = next;
    }
    return 500 - gaps;
  }

  /** The label cut into parts, the letters you typed marked. */
  function marks(text: string, q: string): { s: string; hit: boolean }[] {
    const needle = q.trim().toLowerCase();
    if (!needle) return [{ s: text, hit: false }];
    const t = text.toLowerCase();
    const at = t.indexOf(needle);
    if (at >= 0) return [{ s: text.slice(0, at), hit: false }, { s: text.slice(at, at + needle.length), hit: true }, { s: text.slice(at + needle.length), hit: false }].filter((p) => p.s);
    const picked = new Set<number>();
    let pos = -1;
    for (const ch of needle) {
      pos = t.indexOf(ch, pos + 1);
      if (pos < 0) return [{ s: text, hit: false }];
      picked.add(pos);
    }
    const out: { s: string; hit: boolean }[] = [];
    [...text].forEach((c, i) => {
      const hit = picked.has(i);
      if (out.length && out[out.length - 1].hit === hit) out[out.length - 1].s += c;
      else out.push({ s: c, hit });
    });
    return out;
  }

  function rank(pool: Entry[], q: string): Entry[] {
    return pool
      .map((e) => {
        const r = Math.min(e.rank ?? 0, 40);
        const s = Math.max(score(e.label, q), score(e.detail, q) - 200, e.find ? score(e.find, q) - 100 : -1);
        return { e, s: s < 0 ? s : s + r * 3 };
      })
      .filter((r) => r.s >= 0)
      .sort((a, b) => b.s - a.s)
      .map((r) => r.e);
  }

  /** Items of the same section together, the section with the best match first. */
  function grouped(items: Entry[]): Entry[] {
    const order: string[] = [];
    for (const e of items) if (!order.includes(e.section)) order.push(e.section);
    return order.flatMap((s) => items.filter((e) => e.section === s));
  }

  const results = $derived.by<Entry[]>(() => {
    const q = query.trim().toLowerCase();
    if (drill) return q ? rank(drillProjects, q) : drillProjects;
    if (mode === "needs") return q ? rank(needing, q) : needing;
    if (mode === "projects") return q ? rank(projects, q) : projects;
    if (mode === "prs") return q ? rank(prEntries, q.replace(/^#/, "")) : prEntries;
    if (mode === "said") return found;
    if (mode === "files") return files;
    const acts = [...threadActions, ...projectActions, ...everywhere];
    if (mode === "actions") {
      const pool = [...acts, ...settingsPages, ...projectVerbs];
      return q ? grouped(rank(pool, q)).slice(0, 40) : acts;
    }
    if (!q) {
      return [
        ...needing.slice(0, 3),
        ...threadActions,
        ...projectActions,
        ...threads.filter((t) => !needing.slice(0, 3).some((n) => n.thread === t.thread)).slice(0, 6).map((t) => ({ ...t, section: "Recent" })),
        ...everywhere.slice(0, 6),
      ];
    }
    const best = grouped(rank([...projects, ...threads, ...acts, ...settingsPages, ...projectVerbs], q).slice(0, 30));
    return [...best, ...found];
  });

  $effect(() => {
    void query;
    void mode;
    void drill;
    selected = 0;
  });

  // --- keys ------------------------------------------------------------------------------------

  function run(e: Entry | undefined) {
    if (!e) return;
    if (e.stay) return void e.run();
    onclose();
    e.run();
  }

  function menuFor(e: Entry | undefined) {
    const items = e?.menu?.();
    if (!items) return;
    const el = list?.querySelector<HTMLElement>(`[data-i="${selected}"]`);
    const r = el?.getBoundingClientRect();
    contextMenu.show((r?.right ?? 200) - 220, (r?.bottom ?? 200) - 6, items);
  }

  function setMode(m: Mode | null) {
    mode = m;
    drill = null;
    query = "";
    input?.focus();
  }

  function oninput() {
    if (mode || drill || query.length !== 1) return;
    const hit = MODES.find((m) => m.sign === query);
    if (hit) setMode(hit.mode);
  }

  function keydown(ev: KeyboardEvent) {
    if (ev.key === "ArrowDown") selected = Math.min(results.length - 1, selected + 1);
    else if (ev.key === "ArrowUp") selected = Math.max(0, selected - 1);
    else if (ev.key === "Enter" && ev.altKey) menuFor(results[selected]);
    else if (ev.key === "Enter" && ev.shiftKey) newFromText();
    else if (ev.key === "Enter") run(results[selected]);
    else if (ev.key === "Backspace" && !query && (mode || drill)) setMode(null);
    else if (ev.key === "Escape") onclose();
    else return;
    ev.preventDefault();
    list?.querySelector(`[data-i="${selected}"]`)?.scrollIntoView({ block: "nearest" });
  }

  /** Shift Enter: the words you wrote become the first message of a new thread. */
  function newFromText() {
    const text = query.trim();
    if (!text) return;
    const cwd = ctxProject?.cwd ?? hub.everything[0]?.cwd;
    if (!cwd) return;
    onclose();
    hub.startDraft(cwd, text, ctxProject?.host ?? null);
  }

  let input: HTMLInputElement;
  function autofocus(el: HTMLInputElement) {
    input = el;
    el.focus();
  }

  const machineOf = (t: Thread) => {
    const h = hub.hosts.find((h) => h.name === (t.host ?? hub.localHost));
    return hub.hosts.length > 1 && h ? h : null;
  };
  const sel = $derived(results[selected]);
</script>

<div role="presentation" class="backdrop" onclick={(e) => e.target === e.currentTarget && onclose()} in:modal|global={{ origin: "50% 0" }} out:modal|global={{ out: true, origin: "50% 0" }}>
  <div class="palette" role="dialog" aria-label="Search" tabindex="-1">
    <div class="top">
      <span class="lens"><Icon name="search" size={17} /></span>
      {#if drill}
        <span class="scope"><Icon name="plus" size={12} />New thread in…<button class="x" tabindex="-1" onclick={() => setMode(null)} aria-label="Clear">×</button></span>
      {:else if modeInfo}
        <span class="scope mode"><b>{MODES.find((m) => m.mode === mode)?.sign}</b>{modeInfo.label}<button class="x" tabindex="-1" onclick={() => setMode(null)} aria-label="Clear">×</button></span>
      {/if}
      <input bind:value={query} {oninput} onkeydown={keydown} use:autofocus {placeholder} spellcheck="false" />
    </div>

    {#if !mode && !drill && !query}
      <div class="prefixes">
        {#each MODES as m (m.mode)}
          <button class="prefix" tabindex="-1" onclick={() => setMode(m.mode)}><b>{m.sign}</b>{m.label}</button>
        {/each}
      </div>
    {/if}

    <div class="list" bind:this={list}>
      {#each results as r, i (r.id)}
        {#if i === 0 || results[i - 1].section !== r.section}
          <div class="sec" class:am={r.section === "Needs you"} class:ctx={r.section.startsWith("In ") || r.section === ctxProject?.name}>
            {#if r.section === "Needs you"}<span class="dot waiting_on_you"></span>{/if}
            <span class="name">{r.section}</span>
            {#if r.section === "Needs you"}<span class="n">{needing.length}</span>{/if}
            {#if r.section.startsWith("In ") && ctxProject}
              <span class="where"><Ref kind="project" project={{ path: ctxProject.cwd, host: ctxProject.host }} label={ctxProject.name} /></span>
            {/if}
          </div>
        {/if}
        <button class="item" class:sel={i === selected} class:two={!!r.sub} data-i={i} onclick={() => run(r)} onmousemove={() => (selected = i)}>
          <span class="gl">
            {#if r.dot}<span class="dot {r.dot}"></span>
            {:else if r.project && !r.thread}<ProjectIcon path={r.project.cwd} host={r.project.host} name={r.project.name} size={15} />
            {:else if r.glyph}<Icon name={r.glyph} size={15} />
            {:else}<Icon name="search" size={15} />{/if}
          </span>
          <span class="t">
            <span class="lab">{#each marks(r.label, query) as p}{#if p.hit}<mark>{p.s}</mark>{:else}{p.s}{/if}{/each}</span>
            {#if r.sub}<small>{r.sub}</small>{/if}
          </span>
          {#if r.chevron}
            <span class="chev"><Icon name="chevron" size={14} /></span>
          {:else if r.thread && r.project && !r.sub}
            <span class="wh">
              {#if r.thread.owner}<span class="robot" data-tip="Started by {r.thread.owner.name}"><Icon name="robot" size={13} /></span>{/if}
              <Ref kind="project" project={{ path: r.project.cwd, host: r.project.host }} label={r.project.name} />
              {#if machineOf(r.thread)}{@const m = machineOf(r.thread)!}<Ref kind={machineGlyph(m.name, !!m.local)} label={m.name} online={m.state === "online"} />{/if}
              <span class="age">{r.age}</span>
            </span>
          {:else if r.state || r.age}
            <span class="wh">{#if r.state}<span class="pr {r.state}">{r.state}</span>{/if}<span class="age">{r.age ?? ""}</span></span>
          {:else if r.keys}
            <span class="keys">{#each r.keys as k}<kbd>{k}</kbd>{/each}</span>
          {:else if r.detail && r.section !== "Threads"}
            <span class="detail">{r.detail}</span>
          {/if}
          {#if r.thread && !r.sub}
            <span class="go"><span class="micro">{r.thread.status === "waiting_on_you" ? "Answer" : "Open"}</span><Icon name="more" size={15} /><kbd>↵</kbd></span>
          {/if}
        </button>
      {:else}
        <p class="none">
          {#if mode === "files" && !ctxProject}Open a thread or a project first, then / finds its files.
          {:else if mode === "said" && query.trim().length < 3}Type three letters or more.
          {:else if mode === "files" && !query.trim()}Type part of a file name.
          {:else}Nothing matches.{/if}
        </p>
      {/each}
    </div>

    <div class="bar">
      <span><kbd>Esc</kbd>Close</span>
      <span><kbd>↑</kbd><kbd>↓</kbd>Move</span>
      <span><kbd>↵</kbd>{sel?.thread || sel?.project ? "Open" : "Run"}</span>
      {#if sel?.menu}<span><kbd>Alt</kbd><kbd>↵</kbd>More</span>{/if}
      {#if mode || drill}<span><kbd>⌫</kbd>Back to everything</span>{/if}
      {#if query.trim() && !mode}<span class="rt"><kbd>⇧</kbd><kbd>↵</kbd>New thread from this text</span>{/if}
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.35); z-index: 50; display: flex; justify-content: center; align-items: flex-start; padding-top: 9vh; }
  .palette { width: min(660px, calc(100vw - 32px)); max-height: 80vh; display: flex; flex-direction: column; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.5); overflow: hidden; }
  .top { display: flex; align-items: center; gap: 10px; padding: 0 14px; height: 50px; border-bottom: 1px solid var(--line); flex: none; }
  .lens { color: var(--muted); display: inline-flex; flex: none; }
  input { flex: 1; min-width: 0; background: none; border: 0; outline: none; padding: 0; height: 100%; font-size: 15px; }
  input::placeholder { color: var(--muted); }
  .scope { display: inline-flex; align-items: center; gap: 6px; height: 22px; padding: 0 4px 0 8px; border-radius: var(--r-sm); background: var(--surface-hover); color: var(--text-2); font-size: 12px; flex: none; }
  .scope.mode { background: var(--accent-bg); color: var(--accent); }
  .scope b { font-family: var(--mono); font-weight: 500; }
  .scope .x { color: var(--muted); width: 16px; height: 16px; line-height: 14px; text-align: center; border-radius: var(--r-xs); }
  .scope .x:hover { color: var(--text); background: var(--surface-hover); }
  .prefixes { display: flex; flex-wrap: wrap; gap: 6px; padding: 10px 12px 4px; flex: none; }
  .prefix { display: inline-flex; align-items: center; gap: 6px; height: 24px; padding: 0 8px 0 6px; border-radius: var(--r-sm); border: 1px solid var(--line); color: var(--text-3); font-size: 12px; background: var(--bg-sidebar); }
  .prefix:hover { border-color: var(--line-strong); color: var(--text); }
  .prefix b { font-family: var(--mono); font-weight: 500; font-size: 12px; color: var(--accent); background: var(--accent-bg); border-radius: var(--r-xs); padding: 0 4px; line-height: 17px; }
  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px 6px 8px; }
  .sec { display: flex; align-items: center; gap: 8px; padding: 10px 10px 4px; font-size: 12px; color: var(--muted); }
  .sec.am { color: var(--amber); }
  .sec.ctx .name { color: var(--text-3); font-weight: 500; }
  .sec .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sec .n { font-variant-numeric: tabular-nums; }
  .sec .where { margin-left: auto; display: inline-flex; flex: none; }
  .item { display: flex; align-items: center; gap: 11px; width: 100%; padding: 0 10px; min-height: 34px; border-radius: var(--r-md); text-align: left; }
  .item.two { min-height: 46px; padding-top: 4px; padding-bottom: 4px; }
  .item.sel { background: var(--surface-hover); }
  .gl { width: 16px; display: grid; place-items: center; flex: none; color: var(--muted); }
  .t { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .lab { color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .item.sel .lab { color: var(--text); }
  small { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: 1px; }
  mark { background: none; color: var(--text); font-weight: 600; text-decoration: underline; text-decoration-color: color-mix(in srgb, var(--accent) 70%, transparent); text-underline-offset: 3px; text-decoration-thickness: 1.5px; }
  .wh { display: flex; align-items: center; gap: 12px; color: var(--muted); font-size: 12px; white-space: nowrap; flex: none; }
  .wh .age { min-width: 40px; text-align: right; font-variant-numeric: tabular-nums; }
  .robot { display: inline-flex; color: var(--muted); }
  .pr { text-transform: lowercase; }
  .pr.open { color: var(--green); }
  .pr.merged { color: var(--accent); }
  .detail { color: var(--muted); font-size: 12px; white-space: nowrap; max-width: 45%; overflow: hidden; text-overflow: ellipsis; flex: none; }
  .chev { color: var(--muted); display: inline-flex; flex: none; }
  .keys { display: flex; gap: 2px; flex: none; }
  .go { display: none; align-items: center; gap: 8px; color: var(--muted); flex: none; }
  .micro { font-size: 12px; }
  .item.sel .go { display: flex; }
  .item.sel .wh { display: none; }
  kbd { font-family: var(--mono); font-size: 11px; color: var(--text-3); background: var(--bg); border: 1px solid var(--line-strong); border-bottom-width: 1.5px; border-radius: var(--r-sm); padding: 1px 4px; line-height: 15px; min-width: 19px; text-align: center; display: inline-block; }
  .bar { display: flex; align-items: center; gap: 16px; padding: 0 14px; min-height: 36px; border-top: 1px solid var(--line); color: var(--muted); font-size: 12px; flex: none; flex-wrap: wrap; row-gap: 4px; }
  .bar span { display: inline-flex; align-items: center; gap: 4px; white-space: nowrap; }
  .bar .rt { margin-left: auto; }
  .none { color: var(--muted); padding: 10px; margin: 0; font-size: 13px; }
</style>
