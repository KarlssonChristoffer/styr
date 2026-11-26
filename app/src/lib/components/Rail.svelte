<!-- The rail: a thin column at the far left that is always there, even with the sidebar closed. Home on top, then the rooms (Inbox, where threads get done, and System, where you organise projects and watch the machines), and at the bottom the idea box, the pause button and you. A room's icon carries what it reads only when it matters, in a part of the icon rather than a dot: Inbox's top row (InboxIcon) when a thread failed, waits on you or works, a tile of System's grid (SystemIcon) when something is broken or left on. Both play a short move on click. Clicking the room you are in opens or closes the sidebar.
     Used by App.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  // The rail: a thin column at the far left that is always there, even with the sidebar closed.
  // Home on top, then the rooms (Inbox, where threads get done, and System, where you organise
  // projects and watch the machines), and at the bottom the idea box, the pause button and you.
  // A room's icon carries what it reads only when it matters, in a part of the icon rather than a
  // dot: Inbox's top row (InboxIcon) when a thread failed, waits on you or works, a tile of
  // System's grid (SystemIcon) when something is broken or left on. Both play a short move on click. Clicking the room you are in opens or closes the sidebar.
  import { byProject, isTemporary } from "../format";
  import { looks, projectKey } from "../looks.svelte";
  import { menu } from "../menu.svelte";
  import { projectMenu } from "../menus.svelte";
  import { drag } from "../native";
  import { prefs } from "../prefs.svelte";
  import { systemTrouble } from "../rooms";
  import { chatMark, hub, waitsOnYou, type Room } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import InboxIcon from "./InboxIcon.svelte";
  import { fixes } from "../fixes.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import SystemIcon from "./SystemIcon.svelte";
  import UserMenu from "./UserMenu.svelte";
  import { live, showingLive } from "../live.svelte";
  import { changeWorld } from "../worldSwitch";

  /** Recent projects shown under the pinned ones. */
  const RECENT = 5;

  const home = $derived(hub.view === "home");
  const chat = $derived(hub.world === "chat");
  // The world button says when the other world has something that waits on you.
  // Dev's word is "waits on you" (amber); a chat only has a new answer (the accent).
  const otherWaits = $derived(chat ? hub.active.some(waitsOnYou) : hub.chats.some((t) => chatMark(t) === "new" || chatMark(t) === "failed"));
  function world(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    changeWorld(undefined, { x: r.left + r.width / 2, y: r.top + r.height / 2 });
  }
  const needs = $derived(hub.active.filter(waitsOnYou).length);
  const trouble = $derived(systemTrouble());
  // Each room's state lives in a part of its icon instead of a dot: Inbox's top square says how
  // the threads are, a tile of System's grid how the machines are.
  const inboxTone = $derived(
    hub.active.some((t) => waitsOnYou(t) && t.status === "error") ? "red" : needs ? "amber" : hub.running.length ? "working" : "",
  );
  const systemTone = $derived(trouble ? "red" : hub.strays.length ? "amber" : "");
  const name = $derived(prefs.displayName || hub.host?.hostname || "You");
  const updateReady = $derived(!!hub.updateReady || hub.freshNotes);

  // Projects: the pinned ones on top in the order you set, then the ones you worked in lately.
  type P = ReturnType<typeof byProject>[number] & { pk: string };
  const all = $derived(byProject(hub.withChats, hub.projects).map((p) => ({ ...p, pk: projectKey(p.cwd, p.host) })));
  const pinned = $derived(looks.saved.pinned.map((k) => all.find((p) => p.pk === k)).filter((p): p is P => !!p));
  const recent = $derived(all.filter((p) => p.threads.length && !looks.pinned(p.pk) && !isTemporary(p.cwd)).slice(0, RECENT));

  /** A project's state as a dot: amber needs you, red failed, green working. */
  function stateOf(p: P): { tone: "" | "amber" | "red" | "green"; text: string } {
    const failed = p.threads.filter((t) => t.status === "error" && waitsOnYou(t)).length;
    const asks = p.threads.filter((t) => waitsOnYou(t) && t.status !== "error").length;
    const working = p.threads.filter((t) => t.status === "running").length;
    const text = [asks && `${asks} need${asks === 1 ? "s" : ""} you`, failed && `${failed} failed`, working && `${working} working`].filter(Boolean).join(" · ");
    return { tone: asks ? "amber" : failed ? "red" : working ? "green" : "", text };
  }

  const scoped = (p: P) =>
    hub.room === "control"
      ? hub.view === "project" && hub.project === p.cwd && hub.projectHost === p.host
      : !!hub.workScope && hub.workScope.cwd === p.cwd && hub.workScope.host === p.host;

  /** In Inbox (and Home), a project narrows the list to its threads; in System it opens its page. */
  function pick(p: P) {
    if (hub.room === "control") return hub.openProject(p.cwd, p.host);
    hub.workScope = scoped(p) ? null : { cwd: p.cwd, host: p.host, name: p.name };
    hub.sidebarOpen = true;
  }

  const pinItems = (p: P) => [
    looks.pinned(p.pk)
      ? { label: "Unpin from the rail", run: () => looks.pin(p.pk, false).catch((e) => hub.fail(e)) }
      : { label: "Pin to the rail", run: () => looks.pin(p.pk, true).catch((e) => hub.fail(e)) },
    null,
    ...projectMenu(p.cwd, p.host),
  ];

  // Dragging a project onto a pinned one puts it there, pinning it if it was not.
  let dragging = $state<string | null>(null);
  function drop(e: DragEvent, before: string | null) {
    e.preventDefault();
    const key = dragging;
    dragging = null;
    if (key && key !== before) looks.movePin(key, before).catch((err) => hub.fail(err));
  }

  /** Clicks on each room, so its icon plays on every one, not only when the room opens. */
  let inboxClicks = $state(0);
  let systemClicks = $state(0);

  function room(to: Room) {
    if (to === "control") systemClicks++;
    else inboxClicks++;
    if (to === hub.room && !home) {
      hub.sidebarOpen = !hub.sidebarOpen;
      return;
    }
    hub.enterRoom(to);
    hub.sidebarOpen = true;
  }
</script>

<nav class="rail" aria-label="Rooms">
  <div class="top" use:drag>
    <!-- The world button, as on the phone: a dashed circle with a speech bubble in Dev (a click goes
         to Chat) and a terminal in Chat (a click goes back). It stands where Home was; Home is the
         house in the sidebar's header (Alt 0). A click does not focus a button here, so a later
         key press cannot draw a focus ring on it. -->
    <button class="place world" data-tip={chat ? "Back to Dev (Alt 3)" : "Chat: everyday chats, in warm colours (Alt 3)"} onmousedown={(e) => e.preventDefault()} onclick={world}>
      <IconSwap name={chat ? "terminal" : "threads"} size={16} />
      {#if otherWaits}<span class="wdot" class:chatdot={!chat} data-tip={chat ? "A thread in Dev waits on you" : "A chat has a new answer"}></span>{/if}
    </button>
  </div>
  {#if chat}
    <div class="rooms" role="tablist" aria-label="Room">
      <button role="tab" aria-selected="true" class="place on" data-tip={hub.sidebarOpen ? "Chats: hide the list" : "Chats: show the list"}
        onmousedown={(e) => e.preventDefault()} onclick={() => (hub.sidebarOpen = !hub.sidebarOpen)}>
        <Icon name="threads" size={19} />
      </button>
    </div>
  {:else}
  <div class="rooms" role="tablist" aria-label="Room">
    <button role="tab" aria-selected={hub.room === "work" && !home} class="place" class:on={hub.room === "work" && !home} data-inbox
      data-tip={needs ? `Inbox: ${needs} need you (Alt 1)` : `Inbox: your threads (Alt 1)`} onmousedown={(e) => e.preventDefault()} onclick={() => room("work")}>
      <InboxIcon open={hub.room === "work" && !home} tone={inboxTone} pulse={inboxClicks} land={fixes.landed} size={19} />
    </button>
    <button role="tab" aria-selected={hub.room === "control"} class="place" class:on={hub.room === "control"}
      data-tip={trouble ? `System: ${trouble} (Alt 2)` : "System: projects and machines (Alt 2)"} onmousedown={(e) => e.preventDefault()} onclick={() => room("control")}>
      <SystemIcon open={hub.room === "control" && !home} tone={systemTone} pulse={systemClicks} size={19} />
    </button>
  </div>
  {/if}

  <!-- Projects: pinned on top, then recent, dimmed. The rest of the rail moves the window. -->
  <div class="mid">
    {#if !chat && (pinned.length || recent.length)}
      <div class="projects">
        {#each pinned as p (p.pk)}
          {@const st = stateOf(p)}
          <button class="project" class:on={scoped(p)} class:over={dragging && dragging !== p.pk}
            data-tip="{p.name}{st.text ? ` · ${st.text}` : ''}" draggable="true"
            ondragstart={() => (dragging = p.pk)} ondragend={() => (dragging = null)}
            ondragover={(e) => e.preventDefault()} ondrop={(e) => drop(e, p.pk)}
            onclick={() => pick(p)} use:menu={() => pinItems(p)}>
            <span class="ic"><ProjectIcon path={p.cwd} host={p.host} name={p.name} size={16} /></span>
            {#if st.tone}<span class="dot {st.tone}"></span>{/if}
          </button>
        {/each}
        {#if pinned.length && recent.length}<span class="line"></span>{/if}
        {#each recent as p (p.pk)}
          {@const st = stateOf(p)}
          <button class="project recent" class:on={scoped(p)}
            data-tip="{p.name}{st.text ? ` · ${st.text}` : ''} · right-click to pin" draggable="true"
            ondragstart={() => (dragging = p.pk)} ondragend={() => (dragging = null)}
            onclick={() => pick(p)} use:menu={() => pinItems(p)}>
            <span class="ic"><ProjectIcon path={p.cwd} host={p.host} name={p.name} size={16} /></span>
            {#if st.tone}<span class="dot {st.tone}"></span>{/if}
          </button>
        {/each}
      </div>
    {/if}
    <!-- Drop a recent project here to pin it at the end. -->
    <div class="grow" use:drag role="presentation" ondragover={(e) => dragging && e.preventDefault()} ondrop={(e) => drop(e, null)}></div>
  </div>

  <div class="bottom">
    {#if live.offered && !chat}
      <!-- Live mode: left click switches the window between the installed pages and the open pull
           requests' live ones; right click lists those pull requests and which of them show. -->
      <button class="tool" class:live={showingLive} class:going={!!live.going}
        data-tip={live.needsUpdate ? "Live mode comes with the next full update of Hub"
          : live.going ? `${live.going}…`
          : showingLive ? "Live: the open pull requests, as they are pushed. Click for the installed pages (Ctrl Shift L), right-click for the list"
          : "Show the open pull requests live (Ctrl Shift L). Right-click for the list"}
        onmouseenter={() => quiet(live.refresh(), "refresh")}
        onclick={() => live.toggle()} use:menu={() => live.menu()}>
        <Icon name="live" size={16} />
      </button>
    {/if}
    {#if !chat}
      <button class="tool" data-tip="An idea (Ctrl I): Hub puts it in the right project's focus" onclick={() => (hub.ideaOpen = !hub.ideaOpen)}>
        <Icon name="bulb" size={16} />
      </button>
    {/if}
    {#if hub.connected}
      <!-- The panic button: one click pauses every agent on every machine, the same button resumes. -->
      <button class="tool" class:halted={!!hub.halt} disabled={hub.halting}
        data-tip={hub.halt ? "Resume every agent: cut-off turns carry on" : "Pause every agent on every machine"}
        onclick={() => (hub.halt ? hub.resumeAll() : hub.pauseAll())}>
        <IconSwap name={hub.halt ? "play" : "pause"} size={16} />
      </button>
    {/if}
    <!-- With the sidebar open, its foot has your account and settings; the avatar is here for when it is shut. -->
    {#if !hub.sidebarOpen || hub.meMenu === "rail"}
    <div class="me">
      {#if hub.meMenu === "rail"}<UserMenu onclose={() => (hub.meMenu = null)} />{/if}
      <button class="who" class:open={!!hub.meMenu || hub.settingsOpen}
        data-tip={hub.connected ? `${name} · account and settings (Ctrl ,)` : "Core not reachable"}
        onclick={() => (hub.meMenu = hub.meMenu === "rail" ? null : "rail")}>
        {name.slice(0, 1).toUpperCase()}
        <span class="dot" class:green={hub.connected} class:red={!hub.connected}></span>
        {#if updateReady}<span class="dot accent top" data-tip="A new version is ready"></span>{/if}
      </button>
    </div>
    {/if}
  </div>
</nav>

<style>
  /* One size for everything on the rail: 34px buttons, 16px icons, 6px state dots in the same
     corner, all drawn --side-scale larger. A dot is the only colour; how many is in the tooltip, not on the icon. */
  .rail {
    zoom: var(--side-scale, 1);
    width: 52px; flex: none; display: flex; flex-direction: column; align-items: center;
    background: var(--bg-sidebar); border-right: 1px solid var(--line); min-height: 0; padding-bottom: 10px;
  }
  .top { width: 100%; display: grid; place-items: center; padding: 8px 0 6px; }
  /* The world button: a dashed circle, as on the phone. */
  .place.world { border: 1.5px dashed var(--line-strong); border-radius: 50%; }
  .place.world:hover { border-color: var(--muted); }
  .wdot.chatdot { background: var(--accent); }
  .wdot { position: absolute; top: -2px; right: -2px; width: 8px; height: 8px; border-radius: 50%; background: var(--amber); box-shadow: 0 0 0 2px var(--bg-sidebar); }
  .rooms { display: flex; flex-direction: column; align-items: center; gap: 2px; padding-top: 6px; border-top: 1px solid var(--line); width: 100%; }
  .place, .project, .tool {
    position: relative; width: 34px; height: 34px; flex: none; border-radius: var(--r-md); display: grid; place-items: center; color: var(--muted);
  }
  .place:hover, .tool:hover { background: var(--surface); color: var(--text-2); }
  .place.on { background: var(--surface-hover); color: var(--text); }
  .dot { position: absolute; right: 7px; bottom: 7px; width: 6px; height: 6px; border-radius: 50%; box-shadow: 0 0 0 2px var(--bg-sidebar); }
  .dot.top { bottom: auto; top: -1px; right: -1px; }
  .dot.amber { background: var(--amber); }
  .dot.red { background: var(--red); }
  .dot.green { background: var(--green); }
  .dot.accent { background: var(--accent); }
  .grow { flex: 1; width: 100%; min-height: 12px; }
  /* Projects scroll when there are many; the rooms and the bottom stay put. */
  .mid { flex: 1; min-height: 0; width: 100%; display: flex; flex-direction: column; overflow-y: auto; overflow-x: hidden; scrollbar-width: none; }
  .mid::-webkit-scrollbar { display: none; }
  .projects { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 8px 0 4px; }
  .project:hover { background: var(--surface); }
  .project.on { background: var(--surface-hover); }
  .project.over { box-shadow: inset 0 0 0 1px var(--line-strong); }
  /* Recent ones are dimmed, the icon only: their dot keeps its full colour. */
  .ic { display: grid; place-items: center; }
  .project.recent .ic { opacity: 0.5; }
  .project.recent:hover .ic, .project.recent.on .ic { opacity: 1; }
  .line { width: 18px; height: 1px; background: var(--line-strong); margin: 4px 0; flex: none; }
  .bottom { display: flex; flex-direction: column; align-items: center; gap: 2px; width: 100%; padding-top: 6px; border-top: 1px solid var(--line); }
  .tool.halted { color: var(--amber); background: var(--amber-bg); }
  .tool.live { color: var(--accent); background: var(--accent-bg); }
  .tool.going { color: var(--accent); animation: going 1.2s var(--ease-move) infinite; }
  @keyframes going { 50% { opacity: 0.45; } }
  @media (prefers-reduced-motion: reduce) { .tool.going { animation: none; } }
  .me { position: relative; margin-top: 4px; }
  .who {
    position: relative; width: 26px; height: 26px; border-radius: 50%; display: grid; place-items: center;
    background: var(--surface-hover); color: var(--text-3); font-size: 11px; font-weight: 500;
  }
  .who:hover, .who.open { color: var(--text); box-shadow: 0 0 0 1px var(--line-strong); }
  .who .dot { right: -1px; bottom: -1px; }
  .who .dot.top { top: -1px; bottom: auto; }
</style>
