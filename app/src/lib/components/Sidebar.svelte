<!-- Two rooms, picked on the rail. Inbox (work) is the activity list, where threads get done. System (control) is where you organise projects and watch the machines; it lists no threads, its counts lead into Inbox instead. Home sits above both and shows Inbox's list.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import { untrack, type Snippet } from "svelte";
  import { api } from "../api";
  import type { FocusSummary } from "../bindings/FocusSummary";
  import { byProject, isChatThread, isScratch, isTemporary, shortVersion, threadProject } from "../format";
  import { looks, projectKey } from "../looks.svelte";
  import { contextMenu, menu } from "../menu.svelte";
  import { motion } from "../motion";
  import { cubicOut } from "svelte/easing";
  import { slide } from "svelte/transition";
  import { projectMenu, threadMenu } from "../menus.svelte";
  import { drag } from "../native";
  import { prefs, setPref } from "../prefs.svelte";
  import { textGrow } from "../textSize";
  import { engineProvider } from "../providers.svelte";
  import { inboxReading, nowItems, ROOM_NAMES, systemReading } from "../rooms";
  import { hub, needsYou, roomOf, unread, waitsOnYou } from "../state.svelte";
  import ActivityList from "./ActivityList.svelte";
  import ChatList from "./ChatList.svelte";
  import FocusBar from "./FocusBar.svelte";
  import HostChip from "./HostChip.svelte";
  import ScopeBar from "./ScopeBar.svelte";
  import Icon from "./Icon.svelte";
  import ResizeEdge from "./ResizeEdge.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import UpdateBadge from "./UpdateBadge.svelte";
  import UserMenu from "./UserMenu.svelte";

  /** `footer` is a slot at the foot of the list, under the scroll (the update badge goes there). */
  let { onnew, onsearch, footer, width = 0, onlive = () => {}, oncommit = () => {}, onreset = () => {} }: {
    onnew: () => void; onsearch: () => void; footer?: Snippet;
    /** A width dragged at its edge, in the sidebar's own units; 0 is the default. */
    width?: number; onlive?: (w: number | null) => void; oncommit?: (w: number) => void; onreset?: () => void;
  } = $props();
  const sideScale = () => Number(getComputedStyle(document.documentElement).getPropertyValue("--side-scale")) || 1;

  // Two rooms, picked on the rail. Inbox (work) is the activity list, where threads get done.
  // System (control) is where you organise projects and watch the machines; it lists no threads,
  // its counts lead into Inbox instead. Home sits above both and shows Inbox's list.
  const work = $derived(hub.room === "work");

  // Hub opens in the room you left it in, and each room remembers the page it had open.
  if (prefs.sidebarMode === "menu" && (hub.view === "home" || hub.view === "new")) hub.view = "overview";
  $effect(() => {
    const r = hub.route;
    untrack(() => {
      // Home is its own place on the rail, and New thread is a moment, so the room keeps the page you had open before them.
      // A chat is the Chat world's page, not Inbox's: Inbox keeps the last thread of work.
      const chat = r.view === "thread" && hub.threads.some((t) => t.id === r.thread && isChatThread(t));
      if (r.view !== "home" && r.view !== "new" && !chat) hub.rooms[roomOf(r.view)] = r;
    });
  });
  $effect(() => {
    const mode = work ? "activity" : "menu";
    if (untrack(() => prefs.sidebarMode) !== mode) setPref("sidebarMode", mode);
  });

  // With more than one machine, threads elsewhere get a host icon and both rooms can be narrowed to one.
  const several = $derived(hub.hosts.length > 1);
  const shown = $derived(hub.hostFilter ? hub.active.filter((t) => hub.hostOf(t) === hub.hostFilter) : hub.active);
  // The project lists also draw the iPhone's chats, which are not work and so not in `shown`.
  const shownWithChats = $derived(hub.hostFilter ? hub.withChats.filter((t) => hub.hostOf(t) === hub.hostFilter) : hub.withChats);
  // In focus mode the one project is all there is, here as in every list, threads or not.
  const projects = $derived.by(() => {
    const all = byProject(shownWithChats, !hub.hostFilter || hub.hostFilter === hub.localHost ? hub.projects : []);
    const f = prefs.focusOn;
    if (!f) return all;
    const one = all.filter((p) => p.cwd === f.cwd && p.host === f.host);
    return one.length ? one : [{ key: f.host ? `${f.host}\u0000${f.cwd}` : f.cwd, name: f.name, cwd: f.cwd, host: f.host, threads: [], added: false, at: 0 }];
  });
  // What focus mode hides from the list, said under it, so a focus you forgot never reads as
  // projects gone.
  const hiddenByFocus = $derived.by(() => {
    if (!prefs.focusOn) return 0;
    const every = hub.hostFilter ? hub.everything.filter((t) => hub.hostOf(t) === hub.hostFilter) : hub.everything;
    return Math.max(0, byProject(every, !hub.hostFilter || hub.hostFilter === hub.localHost ? hub.projects : []).length - projects.length);
  });
  // The projects you keep come first, in the order of the rail (the pinned ones as you set them,
  // then by name), not by when they last moved. What is only there for a while (Scratch, a fix
  // worktree) folds into "Temporary" under them. Focus mode shows its one project as it is.
  const kept = $derived.by(() => {
    if (prefs.focusOn) return projects;
    const rank = (p: (typeof projects)[number]) => {
      const i = looks.saved.pinned.indexOf(projectKey(p.cwd, p.host));
      return i < 0 ? Infinity : i;
    };
    return projects
      .filter((p) => !isTemporary(p.cwd))
      .sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
  });
  const temporary = $derived(prefs.focusOn ? [] : projects.filter((p) => isTemporary(p.cwd) && !isScratch(p.cwd)));
  let showTemporary = $state(false);
  // Quick threads: the ones started with no project, from every machine, as one row fixed under
  // the list so it never scrolls away. It opens Inbox narrowed to them.
  const quick = $derived(prefs.focusOn ? [] : shown.filter((t) => isScratch(threadProject(t))));
  const quickAsks = $derived(quick.filter(waitsOnYou).length);
  const quickWorking = $derived(quick.filter((t) => t.status === "running").length);
  function openQuick() {
    hub.workScope = { cwd: "", host: null, name: "Quick threads", quick: true };
    hub.switchRoom("work");
  }
  const temporaryPrs = $derived(temporary.reduce((n, p) => n + p.threads.filter((t) => hub.prs[t.id]?.state === "open").length, 0));
  let showArchived = $state(false);
  // The foot: one subscription at a time, the one the open thread runs on, else the one last
  // picked in the name menu. The wheel on it steps through the rest until you open another thread.
  const subs = $derived(hub.subscriptions);
  const threadSub = $derived.by(() => {
    const t = hub.view === "thread" ? hub.current : null;
    if (!t) return null;
    const id = engineProvider(t.engine).id;
    return subs.find((s) => s.engine === t.engine) ?? subs.find((s) => engineProvider(s.engine).id === id) ?? null;
  });
  let picked = $state<string | null>(null);
  $effect(() => {
    void hub.selected;
    untrack(() => (picked = null));
  });
  const shownSub = $derived((picked && subs.find((s) => s.engine === picked)) || threadSub || hub.subscription);
  /** Its limit closest to running out. */
  const top = $derived(shownSub.plan?.limits.length ? shownSub.plan.limits.reduce((a, b) => (b.percent > a.percent ? b : a)) : null);
  /** What a screen reader hears for the strip. */
  const footName = $derived(
    subs.map((s) => (s.balance ? `${s.label} ${s.balance} left` : s.plan?.limits.length ? `${s.label} ${Math.round(Math.max(...s.plan.limits.map((l) => l.percent)))}%` : s.label)).join(", "),
  );
  function stepSub(e: WheelEvent) {
    if (subs.length < 2) return;
    e.preventDefault();
    const i = subs.findIndex((s) => s.engine === shownSub.engine);
    picked = subs[(i + (e.deltaY > 0 ? 1 : -1) + subs.length) % subs.length].engine;
  }
  /** Opens the name menu on the subscription the foot shows. */
  function openCard() {
    if (hub.meMenu === "foot") return (hub.meMenu = null);
    setPref("menuAccount", shownSub.engine === "claude" ? "" : shownSub.engine);
    hub.meMenu = "foot";
  }
  /** The Updates row in System says what the version is doing. */
  const update = $derived.by(() => {
    const u = hub.update;
    if (!u) return null;
    if (u.state === "ready") return { text: "Ready", tone: "accent" };
    if (u.state === "restarting") return { text: "Restarting…", tone: "" };
    if (u.state === "failed") return { text: "Failed", tone: "red" };
    return { text: shortVersion(u.version), tone: "mono" };
  });
  const notesWaiting = $derived(!!hub.update && hub.freshNotes && hub.update.state === "idle");
  const level = (pct: number) => (pct >= 95 ? "full" : pct >= 80 ? "warn" : "");
  // Next to the room's name, what it reads right now.
  // In the Chat world the sidebar is the list of chats and nothing else of Dev's.
  const chat = $derived(hub.world === "chat" && work);
  const reading = $derived(chat ? { text: "", tone: "" } : work ? inboxReading() : systemReading());
  const title = $derived(chat ? "Chats" : hub.view === "home" ? "Home" : work ? ROOM_NAMES.work : ROOM_NAMES.control);


  // Each project's stage now from its docs/FOCUS.md, read again when a thread changes state (an
  // agent may have ticked something off) and every minute. Only this machine's folders.
  let focusOf = $state<Record<string, FocusSummary>>({});
  const localPaths = $derived(projects.filter((p) => !p.host || p.host === hub.localHost).map((p) => p.cwd));
  $effect(() => {
    const paths = localPaths;
    void hub.active.map((t) => t.status).join();
    const load = () => (paths.length ? quiet(api.focusSummaries(paths).then((f) => (focusOf = f)), "focus summaries") : undefined);
    load();
    const timer = poll(load, 60_000);
    return () => timer();
  });
  /** "3/6 · 2 to test": how far the stage now has come. */
  function focusLine(p: (typeof projects)[number]) {
    const f = p.host && p.host !== hub.localHost ? undefined : focusOf[p.cwd];
    if (!f) return null;
    if (!f.stage) return { text: "all stages delivered", title: "Every stage in docs/FOCUS.md is delivered" };
    const text = [f.of ? `${f.done}/${f.of}` : `stage ${f.n}`, f.to_test && `${f.to_test} to test`].filter(Boolean).join(" · ");
    return { text, title: `Stage ${f.n} of ${f.stages}: ${f.stage}${f.of ? `, ${f.done} of ${f.of} delivered` : ""}${f.to_test ? `, ${f.to_test} to test` : ""}` };
  }

  /** What a project row says about itself: threads by state, and a second line only when there is
   *  something to say (what is left to test, open pull requests). No times: it is not an inbox. */
  function about(p: (typeof projects)[number]) {
    const asks = p.threads.filter(waitsOnYou).length;
    const working = p.threads.filter((t) => t.status === "running").length;
    const fresh = p.threads.filter((t) => unread(t) && t.status !== "running" && !needsYou(t)).length;
    const prs = p.threads.filter((t) => hub.prs[t.id]?.state === "open").length;
    return { asks, working, fresh, prs, idle: !asks && !working && !fresh };
  }
  /** The second line: "3/14 · 163 to test · 1 PR open", or nothing when there is nothing to say. */
  function sayLine(p: (typeof projects)[number], a: ReturnType<typeof about>) {
    const f = focusLine(p);
    const stage = f && focusOf[p.cwd]?.to_test ? f : null;
    const text = [stage?.text, a.prs && `${a.prs} PR${a.prs > 1 ? "s" : ""} open`].filter(Boolean).join(" · ");
    return text ? { text, title: stage?.title } : null;
  }

  /** Opens Inbox narrowed to one project's threads. */
  function workOn(p: (typeof projects)[number]) {
    hub.workScope = { cwd: p.cwd, host: p.host, name: p.name };
    hub.switchRoom("work");
  }

  function pickMachine(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    contextMenu.show(r.left, r.bottom + 4, [
      { label: "All machines", hint: hub.hostFilter ? "" : "✓", run: () => (hub.hostFilter = null) },
      null,
      ...hub.hosts.map((h) => ({ label: h.name, hint: hub.hostFilter === h.name ? "✓" : h.state === "online" ? "" : h.state, run: () => (hub.hostFilter = h.name) })),
    ]);
  }

  const system = [
    { id: "overview", label: "Overview", icon: "chart" },
    { id: "problems", label: "Problems", icon: "alert" },
    { id: "machines", label: "Machines", icon: "monitor" },
    { id: "routines", label: "Routines", icon: "clock" },
    { id: "recipes", label: "Recipes", icon: "recipe" },
    { id: "library", label: "Library", icon: "brain" },
    { id: "plugins", label: "Plugins", icon: "puzzle" },
    { id: "workers", label: "Workers", icon: "workers" },
    { id: "services", label: "Services", icon: "services" },
    { id: "updates", label: "Updates", icon: "download" },
    { id: "labs", label: "Labs", icon: "flask" },
    { id: "welcome", label: "Välkommen", icon: "help" },
  ] as const;
  type SystemItem = (typeof system)[number];
  const byId = (ids: SystemItem["id"][]) => ids.map((id) => system.find((v) => v.id === id)!);
  // The pages in three groups, by what you do there: look at how things are, set things to run
  // by themselves, and extend Hub. Only Watch is open until you open another; the page you are
  // on keeps its group open, and a folded group says what it holds.
  const groups = [
    { id: "watch", label: "Watch", items: byId(["overview", "problems", "machines"]) },
    { id: "automate", label: "Automate", items: byId(["routines", "recipes"]) },
    { id: "extend", label: "Extend", items: byId(["library", "plugins", "workers", "services", "updates", "labs", "welcome"]) },
  ];
  const isOn = (id: SystemItem["id"]) => hub.view === id || (id === "machines" && hub.view === "machine");
  const groupOpen = (g: (typeof groups)[number]) => g.id === "watch" || prefs.systemOpen.includes(g.id) || g.items.some((v) => isOn(v.id));
  function toggleGroup(g: (typeof groups)[number]) {
    const open = prefs.systemOpen.includes(g.id);
    setPref("systemOpen", open ? prefs.systemOpen.filter((x) => x !== g.id) : [...prefs.systemOpen, g.id]);
  }
  /** What a folded group still says: servers left on, or a failed update. */
  const groupTone = (g: (typeof groups)[number]) =>
    g.id === "extend" ? (update?.tone === "red" ? "red" : hub.strays.length ? "amber" : "") : "";

  function go(id: (typeof system)[number]["id"]) {
    if (id === "updates") hub.openSettings("updates");
    else if (id === "plugins") hub.openPlugins();
    else {
      hub.view = id;
      hub.selected = null;
    }
  }

  // The bell: what wants you now (failed or waiting threads, machines gone offline, problems), a
  // few lines folded out under the header. Only there while something does.
  const LOOK = 3;
  const looking = $derived(hub.world === "chat" ? [] : nowItems());
  let lookOpen = $state(false);
  const lookMs = matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : 160;
  $effect(() => {
    if (!looking.length) lookOpen = false;
  });

  /** A quick slide for opening and closing the sidebar; none with reduced motion. */
  function reveal(node: HTMLElement, { ms }: { ms: number }) {
    const width = node.offsetWidth || 256;
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    return {
      duration: reduce ? 0 : ms,
      easing: cubicOut,
      css: (t: number) => `width: ${t * width}px; opacity: ${0.4 + 0.6 * t};`,
    };
  }
</script>

<svelte:window onkeydown={(e) => { if (e.key === "Escape" && lookOpen) lookOpen = false; }} />

{#snippet projectRow(p: (typeof projects)[number])}
  {@const a = about(p)}
  {@const line = sayLine(p, a)}
  <div class="project" class:active={hub.view === "project" && hub.project === p.cwd && hub.projectHost === p.host} class:idle={a.idle} use:menu={() => projectMenu(p.cwd, p.host)}>
    <button class="open-project" data-tip={p.host ? `${p.host}: ${p.cwd}` : p.cwd} onclick={() => hub.openProject(p.cwd, p.host)}>
      <span class="name"><ProjectIcon path={p.cwd} host={p.host} name={p.name} size={17} /><span class="ellipsis">{p.name}</span>{#if several && p.host}<HostChip name={p.host} compact />{/if}</span>
      {#if line}<span class="line ellipsis" data-tip={line.title}>{line.text}</span>{/if}
    </button>
    {#if !a.idle}
      <button class="live" data-tip="Open {p.name}'s threads in Inbox" onclick={() => workOn(p)}>
        {#if a.asks}<span class="amber" data-tip="{a.asks} need you"><i class="dot waiting_on_you"></i>{a.asks}</span>{/if}
        {#if a.working}<span class="green" data-tip="{a.working} working"><i class="dot running"></i>{a.working}</span>{/if}
        {#if a.fresh}<span class="accent" data-tip="{a.fresh} done, not seen yet"><i class="dot idle unread"></i>{a.fresh}</span>{/if}
      </button>
    {/if}
  </div>
{/snippet}

{#if hub.sidebarOpen}
<div class="slot" style:--side-w={width ? `max(${width}px, calc(240px * var(--text-grow, 1)))` : undefined} in:reveal={{ ms: 160 }} out:reveal={{ ms: 130 }}>
<ResizeEdge side="right" inside min={Math.round(240 * textGrow(prefs.textSize))} max={Math.round(480 * textGrow(prefs.textSize))} scale={sideScale} label="Resize the sidebar" {onlive} {oncommit} {onreset} />
<aside>
  <!-- The room you are in, what it reads now, and search. The window moves by this row. -->
  <header use:drag>
    <h1>{title}</h1>
    <span class="reading {reading.tone}">{reading.text}</span>
    <span class="grow"></span>
    {#if looking.length}
      <button class="ghost bell" class:on={lookOpen} aria-expanded={lookOpen} aria-label="Needs a look"
        data-tip={looking.length === 1 ? "1 thing needs a look" : `${looking.length} things need a look`}
        onmousedown={(e) => e.preventDefault()} onclick={() => (lookOpen = !lookOpen)}>
        <Icon name="bell" size={15} /><span class="bd {looking.some((n) => n.tone === 'red') ? 'red' : 'amber'}"></span>
      </button>
    {/if}
    <button class="ghost" data-tip="Search (Ctrl K)" onmousedown={(e) => e.preventDefault()} onclick={onsearch}><Icon name="search" size={15} /></button>
    <button class="ghost" data-tip="Hide the sidebar (Ctrl B)" onmousedown={(e) => e.preventDefault()} onclick={() => (hub.sidebarOpen = false)}><Icon name="sidebar" size={15} /></button>
  </header>

  {#if lookOpen && looking.length}
    <div class="look" in:slide={{ duration: lookMs }} out:slide={{ duration: Math.round(lookMs * 0.8) }}>
      {#each looking.slice(0, LOOK) as n, i (i)}
        <button class="look-row" onclick={() => { lookOpen = false; n.open(); }}>
          <span class="look-dot {n.tone}"></span><span class="look-title">{n.title}</span>
        </button>
      {/each}
      {#if looking.length > LOOK}<div class="look-more">+{looking.length - LOOK} more in Inbox</div>{/if}
    </div>
  {/if}

  <div class="start">
    {#if chat}
      <button class="new" class:on={hub.view === "new"} onclick={onnew} data-tip="New chat (Ctrl N)">
        <Icon name="plus" /><span>New chat</span><kbd>Ctrl N</kbd>
      </button>
    {:else if work}
      <button class="new" class:on={hub.view === "new"} onclick={onnew} data-tip="New thread (Ctrl N)">
        <Icon name="plus" /><span>New thread</span><kbd>Ctrl N</kbd>
      </button>
      <!-- Home, which the world button took the rail's place of. -->
      <button class="homebtn" class:on={hub.view === "home"} data-tip="Home (Alt 0)" aria-label="Home" onmousedown={(e) => e.preventDefault()} onclick={() => hub.goHome()}><Icon name="home" size={16} /></button>
    {:else}
      <button class="new" onclick={() => (hub.newProjectOpen = true)} data-tip="New project">
        <Icon name="plus" /><span>New project</span>
      </button>
    {/if}
  </div>

  {#if !chat}<FocusBar />{/if}

  {#if chat}
    <div class="scroll"><ChatList /></div>
  {:else if work}
    <ScopeBar />
    <div class="scroll"><ActivityList /></div>
  {:else}
    <div class="scroll" use:motion>
      {#each groups as g (g.id)}
        {@const open = groupOpen(g)}
        {#if g.id !== "watch"}
        <button class="group" class:open aria-expanded={open} onclick={() => toggleGroup(g)}>
          <span class="chev" class:open><Icon name="chevron" size={12} /></span>
          <span class="gname">{g.label}</span>
          {#if !open}
            {@const tone = groupTone(g)}
            <span class="peek ellipsis">{g.items.map((v) => v.label).join(" · ")}</span>
            {#if tone}<i class="gdot {tone}"></i>{/if}
          {/if}
        </button>
        {/if}
        {#if open}
          {#each g.items as v (v.id)}
            <button class="row" class:active={isOn(v.id)} onclick={() => go(v.id)}>
              <Icon name={v.icon} /><span class="grow">{v.label}</span>
              {#if v.id === "machines" && hub.hosts.length}
                <span class="lights" data-tip={hub.hosts.map((h) => `${h.name}: ${h.state}`).join(", ")}>
                  {#each hub.hosts as h (h.name)}<i class={h.state}></i>{/each}
                </span>
              {:else if v.id === "problems" && hub.problemsNeedYou.length}<span class="read red" data-tip="Problems on every online machine that wait for you: asks first, came back, urgent with nobody on it, or unsorted while Guard is off. The same count as Needs you on the Problems page, per problem, not per folded row">{hub.problemsNeedYou.length} need you</span>
              {:else if v.id === "workers" && hub.running.length}<span class="read green">{hub.running.length} working</span>
              {:else if v.id === "services" && hub.strays.length}<span class="read amber" data-tip="Servers still running after their thread finished">{hub.strays.length} left on</span>
              {:else if v.id === "updates" && update}<span class="read {update.tone}">{update.text}</span>{/if}
            </button>
          {/each}
        {/if}
      {/each}

      <div class="section with-action">
        Projects
        {#if several}<button class="machine" onclick={pickMachine}>{hub.hostFilter ?? "All machines"}<Icon name="down" size={11} /></button>{/if}
        <button class="ghost add" data-tip="New project" onclick={() => (hub.newProjectOpen = true)}><Icon name="plus" size={13} /></button>
      </div>
      {#if !projects.length}
        <button class="row more" onclick={() => (hub.newProjectOpen = true)}>Add a folder or create a project</button>
      {/if}
      {#each kept as p (p.key)}
        {@render projectRow(p)}
      {/each}
      {#if temporary.length}
        <button class="row more" aria-expanded={showTemporary} data-tip="Conflict-fix folders and the like: they come and go" onclick={() => (showTemporary = !showTemporary)}>
          <span class="chev" class:open={showTemporary}><Icon name="chevron" size={13} /></span>
          <span class="grow">Temporary</span>
          {#if temporaryPrs}<span class="amber count">{temporaryPrs} PR{temporaryPrs > 1 ? "s" : ""}</span>{/if}
          <span class="count">{temporary.length}</span>
        </button>
        {#if showTemporary}
          <div class="temporary">
            {#each temporary as p (p.key)}{@render projectRow(p)}{/each}
          </div>
        {/if}
      {/if}
      {#if prefs.focusOn && hiddenByFocus}
        <button class="row more" data-tip="Leave focus on {prefs.focusOn.name}: every project shows again" onclick={() => hub.leaveFocus()}>
          <Icon name="eye" size={13} /><span class="grow ellipsis">{hiddenByFocus} more hidden by focus</span><span class="count">Show all</span>
        </button>
      {/if}

      {#if hub.archived.length}
        <button class="row more" onclick={() => (showArchived = !showArchived)}>
          <span class="chev" class:open={showArchived}><Icon name="chevron" size={13} /></span>
          Archived <span class="count">{hub.archived.length}</span>
        </button>
        {#if showArchived}
          {#each hub.archived as t (t.id)}
            <button class="row child" class:active={hub.selected === t.id} onclick={() => hub.open(t.id)} use:menu={() => threadMenu(t)}>
              <span class="grow ellipsis">{t.title}</span>
            </button>
          {/each}
        {/if}
      {/if}
    </div>
    {#if quick.length}
      <div class="quick">
        <button class="row" onclick={openQuick} data-tip="Threads started with no project, from every machine">
          <Icon name="flask" /><span class="grow">Quick threads</span>
          {#if quickAsks}<span class="amber" data-tip="{quickAsks} need you"><i class="dot waiting_on_you"></i>{quickAsks}</span>{/if}
          {#if quickWorking}<span class="green" data-tip="{quickWorking} working"><i class="dot running"></i>{quickWorking}</span>{/if}
          <span class="count">{quick.length}</span>
        </button>
      </div>
    {/if}
  {/if}

  {#if hub.undo}
    <div class="undo">
      <span class="grow ellipsis">{hub.undo.label}</span>
      <button onclick={hub.undo.run}>Undo <kbd>Ctrl Z</kbd></button>
    </div>
  {/if}

  <!-- The foot: the subscription you are working on, then Account and Settings as their own
       buttons, so where they live is plain to see. The subscription opens its card in the name
       menu, the same menu as the avatar on the rail. -->
  <UpdateBadge />

  <footer>
    {#if footer}{@render footer()}{:else}
      {#if hub.meMenu === "foot"}<UserMenu foot onclose={() => (hub.meMenu = null)} />{/if}
      <button class="me-strip" class:open={hub.meMenu === "foot"} data-tip={hub.connected ? undefined : "Core not reachable"} aria-label={hub.connected ? `Usage: ${footName}. Opens your account card.` : undefined}
        onclick={openCard} onwheel={stepSub}>
        {#if hub.connected}
          <ProviderMark id={engineProvider(shownSub.engine).id} size={14} />
          <span class="sub-name">{shownSub.label}</span>
          {#if shownSub.balance}
            <span class="pct">{shownSub.balance}</span>
          {:else if top}
            <span class="bar"><i class={level(top.percent)} style:width="{Math.max(top.percent, 2)}%"></i></span>
            <span class="pct {level(top.percent)}">{Math.round(top.percent)}%</span>
          {:else}
            <span class="pct">–</span>
          {/if}
          {#if subs.length > 1}
            <span class="others">{#each subs.filter((s) => s.engine !== shownSub.engine) as s (s.engine)}<ProviderMark id={engineProvider(s.engine).id} size={11} />{/each}</span>
          {/if}
        {:else}
          <span class="sub-name">Core not reachable</span>
        {/if}
      </button>
      <button class="foot-btn" class:open={hub.settingsOpen && hub.settingsSection === "account"} data-tip="Account and usage" onclick={() => hub.openSettings("account")}>
        <Icon name="user" size={15} />
      </button>
      <button class="foot-btn" data-tip={notesWaiting ? "Settings · What's new is waiting" : hub.update?.state === "failed" ? "Settings · the update failed" : "Settings (Ctrl ,)"}
        onclick={() => hub.openSettings(notesWaiting ? "whatsnew" : hub.update?.state === "failed" ? "updates" : undefined)}>
        <Icon name="gear" size={15} />
        {#if notesWaiting}<span class="foot-dot accent"></span>{:else if hub.update?.state === "failed"}<span class="foot-dot amber"></span>{/if}
      </button>
    {/if}
  </footer>
</aside>
</div>
{/if}

<style>
  /* Opening and closing, the sidebar slides out from behind the rail: this holder grows or
     shrinks while the sidebar inside keeps its width, so nothing in it rewraps on the way. */
  .slot { position: relative; flex: none; display: flex; overflow: hidden; min-height: 0; }
  /* Drawn --side-scale larger than the rest of the window, like the rail. */
  aside { zoom: var(--side-scale, 1); }
  aside {
    width: var(--side-w, calc(256px * var(--text-grow, 1)));
    flex: none;
    background: var(--bg-sidebar);
    display: flex;
    flex-direction: column;
    padding: 10px 10px 8px;
    gap: 2px;
    min-height: 0;
  }
  /* The top row is where the window title bar used to be, so it moves the window. */
  header { display: flex; align-items: center; gap: 8px; height: 40px; margin: -10px -2px 0 4px; flex: none; }
  h1 { font-size: 15px; font-weight: 600; color: var(--text); letter-spacing: -0.005em; }
  .reading { display: inline-flex; align-items: center; gap: 4px; min-width: 0; font-size: 12px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .reading.amber { color: var(--amber); }
  .reading.green { color: var(--green); }
  .reading.red { color: var(--red); }
  .reading.amber::before, .reading.red::before { content: ""; flex: none; width: 6px; height: 6px; border-radius: 50%; background: currentColor; }
  .reading.green::before { content: ""; flex: none; width: 6px; height: 6px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px currentColor; }
  .start { display: flex; gap: 6px; margin: 4px 0 6px; width: 100%; container-type: inline-size; }
  .new {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 12px; white-space: nowrap; overflow: hidden;
    border-radius: var(--r-md); color: var(--text); font-weight: 500; box-shadow: inset 0 0 0 1px var(--line-strong);
  }
  .new:hover, .new.on { background: var(--surface); }
  .new span { flex: 1; min-width: 0; text-align: left; overflow: hidden; text-overflow: ellipsis; }
  .new kbd { font-weight: 400; flex: none; margin-left: auto; }
  /* A narrow sidebar gives the shortcut up before the name wraps or is cut. */
  @container (max-width: 226px) { .new kbd { display: none; } }
  .homebtn { flex: none; width: 34px; height: 34px; display: grid; place-items: center; border-radius: var(--r-md); color: var(--muted); box-shadow: inset 0 0 0 1px var(--line); }
  .homebtn:hover, .homebtn.on { background: var(--surface); color: var(--text); }
  .undo {
    display: flex; align-items: center; gap: 8px; margin: 4px 0; padding: 6px 10px;
    background: var(--surface); border-radius: var(--r-md); font-size: 12px; color: var(--text-3);
  }
  .undo button { color: var(--accent); flex: none; display: inline-flex; gap: 6px; align-items: center; }
  .undo kbd { margin: 0; font-size: 11px; }
  .ghost { color: var(--muted); display: grid; place-items: center; padding: 4px; border-radius: var(--r-sm); }
  .ghost:hover { color: var(--text); background: var(--surface-hover); }
  .ghost.on { color: var(--text); }
  .bell { position: relative; }
  .bell.on { color: var(--text); background: var(--surface-hover); }
  .bell .bd { position: absolute; top: 2px; right: 2px; width: 6px; height: 6px; border-radius: 50%; box-shadow: 0 0 0 2px var(--bg-sidebar); }
  .bell .bd.amber, .look-dot.amber { background: var(--amber); }
  .bell .bd.red, .look-dot.red { background: var(--red); }
  /* The folded-out list: quiet lines on a raised strip, one line each, pushing the list down. */
  .look { flex: none; margin: 2px 0 6px; padding: 3px; background: var(--surface); border-radius: var(--r-md); overflow: hidden; }
  .look-row { display: flex; align-items: center; gap: 8px; width: 100%; height: 27px; padding: 0 8px; border-radius: var(--r-sm); text-align: left; font-size: 13px; color: var(--text); }
  .look-row:hover { background: var(--surface-hover); }
  .look-title { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .look-dot { flex: none; width: 6px; height: 6px; border-radius: 50%; }
  .look-more { padding: 3px 8px 2px; font-size: 12px; color: var(--muted); }
  .section { color: var(--muted); font-size: 12px; padding: 12px 10px 4px; display: flex; align-items: center; gap: 10px; }
  .section::after { content: ""; flex: 1; height: 1px; background: var(--line); }
  .section.with-action { padding-top: 10px; padding-bottom: 3px; }
  .section.with-action .add { order: 2; }
  .add { padding: 3px; margin: -3px -3px -3px 0; }
  .row {
    display: flex; align-items: center; gap: 10px;
    height: 32px; padding: 0 10px; border-radius: var(--r-md);
    color: var(--text-3); text-align: left; width: 100%; font-size: 14px;
    position: relative;
  }
  .row:hover { background: var(--surface); color: var(--text-2); }
  .row.active { background: var(--surface-hover); color: var(--text); }
  .row.child { height: 30px; padding-left: 30px; font-size: 13px; }
  .row.more { color: var(--muted); font-size: 13px; height: 28px; }
  .row :global(svg) { flex: none; }
  kbd { margin-left: auto; color: var(--muted); font-size: 12px; font-family: var(--font); }
  .badge { margin-left: auto; background: var(--amber-bg); color: var(--amber); font-size: 12px; border-radius: var(--r-sm); padding: 0 6px; line-height: 18px; }
  .count { color: var(--muted); font-size: 12px; }
  .group { display: flex; align-items: center; gap: 8px; width: 100%; height: 30px; padding: 0 10px; margin-top: 4px; border-radius: var(--r-md); color: var(--muted); font-size: 13px; text-align: left; }
  .group:hover { background: var(--surface); color: var(--text-2); }
  .group .gname { flex: none; }
  .group .peek { flex: 1; min-width: 0; text-align: right; font-size: 12px; color: var(--muted); opacity: 0.8; }
  .gdot { width: 6px; height: 6px; border-radius: 50%; flex: none; background: var(--muted); }
  .gdot.amber { background: var(--amber); }
  .gdot.red { background: var(--red); }
  .chev { display: grid; color: var(--muted); transition: transform 0.12s var(--ease); }
  .chev.open { transform: rotate(90deg); }
  .grow { flex: 1; min-width: 0; }
  .ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .scroll { flex: 1; overflow-y: auto; min-height: 0; margin: 0 -4px; padding: 0 4px; }
  /* Work: what the list is narrowed to. */
  .machine { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; padding: 1px 4px; border-radius: var(--r-sm); }
  .machine:hover { color: var(--text-2); background: var(--surface); }
  .section .machine { order: 2; margin: -2px 0; }
  /* Control: each system line ends in what it reads now. */
  .read { font-size: 12px; color: var(--muted); }
  .read.green { color: var(--green); }
  .read.amber { color: var(--amber); }
  .read.red { color: var(--red); }
  .read.accent { color: var(--accent); }
  .read.mono { font-family: var(--mono); }
  .lights { display: inline-flex; gap: 4px; }
  .lights i { width: 7px; height: 7px; border-radius: 50%; box-shadow: inset 0 0 0 1.4px var(--muted); }
  .lights i.online { background: var(--green); box-shadow: none; }
  .lights i.connecting { box-shadow: inset 0 0 0 1.4px var(--amber); }
  .project { display: flex; align-items: center; gap: 6px; border-radius: var(--r-md); padding-right: 6px; }
  .project:hover { background: var(--surface); }
  .project.active { background: var(--surface-hover); }
  .open-project { flex: 1; min-width: 0; display: flex; flex-direction: column; padding: 4px 4px 6px 10px; text-align: left; }
  .open-project .name { display: flex; align-items: center; gap: 8px; color: var(--text-2); font-size: 14px; line-height: 1.35; }
  .project.active .name { color: var(--text); }
  .project.idle .name { color: var(--muted); }
  .open-project .line { color: var(--muted); font-size: 12px; line-height: 1.35; }
  .temporary { opacity: 0.7; }
  .quick { flex: none; border-top: 1px solid var(--line); padding: 4px 0 2px; }
  .quick .row { gap: 8px; }
  .quick .amber, .quick .green { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; }
  .quick .amber { color: var(--amber); }
  .quick .green { color: var(--green); }
  .row.more .amber { color: var(--amber); }
  .live { display: inline-flex; gap: 8px; font-size: 12px; padding: 2px 4px; border-radius: var(--r-sm); font-variant-numeric: tabular-nums; }
  .live:hover { background: var(--surface-hover); }
  .live span { display: inline-flex; align-items: center; gap: 4px; }
  .live .dot { width: 7px; height: 7px; }
  .live .amber { color: var(--amber); }
  .live .green { color: var(--green); }
  .live .accent { color: var(--accent); }
  footer {
    display: flex; align-items: center; gap: 8px;
    border-top: 1px solid var(--line); margin-top: auto; padding: 6px 4px 0 0;
    color: var(--text-3); font-size: 13px; width: 100%; position: relative; min-height: 30px;
  }
  /* The strip: the subscription in use (mark, name, the fullest limit or the balance left), the
     others as small dimmed marks; grey until a limit runs low. */
  .me-strip {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; height: 28px; padding: 0 6px;
    border-radius: var(--r-md); color: var(--muted); font-size: 12px; text-align: left; white-space: nowrap;
  }
  .me-strip:hover, .me-strip.open { background: var(--surface); }
  .sub-name { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .me-strip .bar { width: 22px; height: 3px; flex: none; border-radius: 2px; background: var(--line-strong); overflow: hidden; }
  .me-strip .bar i { display: block; height: 100%; background: var(--text-3); border-radius: 2px; }
  .me-strip .bar i.warn { background: var(--amber); }
  .me-strip .bar i.full { background: var(--red); }
  .me-strip .pct { flex: none; font-variant-numeric: tabular-nums; color: var(--text-3); }
  .me-strip .pct.warn { color: var(--amber); }
  .me-strip .pct.full { color: var(--red); }
  .others { display: inline-flex; gap: 4px; margin-left: 2px; opacity: 0.5; flex: none; }
  .me-strip:hover .others { opacity: 0.8; }
  .foot-btn { position: relative; flex: none; width: 28px; height: 28px; display: grid; place-items: center; border-radius: var(--r-md); color: var(--muted); }
  .foot-btn:hover, .foot-btn.open { background: var(--surface); color: var(--text-2); }
  .foot-dot { position: absolute; right: 5px; top: 5px; width: 6px; height: 6px; border-radius: 50%; box-shadow: 0 0 0 2px var(--bg-sidebar); }
  .foot-dot.accent { background: var(--accent); }
  .foot-dot.amber { background: var(--amber); }
  .version { color: var(--muted); font-family: var(--mono); font-size: 12px; padding: 0 6px; }
  .version:hover { color: var(--text-2); }
  .version.fresh { color: var(--accent); background: var(--accent-bg); border-radius: var(--r-sm); font-family: var(--font); line-height: 20px; }
  .version.warn { color: var(--amber); font-family: var(--font); }
</style>
