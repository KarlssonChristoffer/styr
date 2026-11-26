<!-- The root component: the window frame, the page switch and the global overlays. Mounted by boot.ts. -->
<script lang="ts">
  import { quiet } from "./lib/quiet";
  import { applyTheme } from "./lib/themes";
  import { onMount, untrack } from "svelte";
  import { closed, reopen } from "./lib/closed.svelte";
  import { copy } from "./lib/menu.svelte";
  import CommandPalette from "./lib/components/CommandPalette.svelte";
  import Composer from "./lib/components/Composer.svelte";
  import ContextMenu from "./lib/components/ContextMenu.svelte";
  import BagDrop from "./lib/components/BagDrop.svelte";
  import Home from "./lib/components/Home.svelte";
  import NewProject from "./lib/components/NewProject.svelte";
  import IdeaBox from "./lib/components/IdeaBox.svelte";
  import ProjectView from "./lib/components/ProjectView.svelte";
  import { toast } from "./lib/pop";
  import Settings from "./lib/components/Settings.svelte";
  import Labs from "./lib/components/Labs.svelte";
  import Welcome from "./lib/components/Welcome.svelte";
  import { track, watchUse } from "./lib/uselog.svelte";
  import Overview from "./lib/components/Overview.svelte";
  import RoutinesView from "./lib/components/RoutinesView.svelte";
  import ProblemsView from "./lib/components/ProblemsView.svelte";
  import RecipesView from "./lib/components/RecipesView.svelte";
  import MachinesView from "./lib/components/MachinesView.svelte";
  import MachineView from "./lib/components/MachineView.svelte";
  import LibraryView from "./lib/components/LibraryView.svelte";
  import PluginsView from "./lib/components/PluginsView.svelte";
  import HomePanel from "./lib/components/HomePanel.svelte";
  import HostPanel from "./lib/components/HostPanel.svelte";
  import WorkersView from "./lib/components/WorkersView.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Rail from "./lib/components/Rail.svelte";
  import ResizeEdge from "./lib/components/ResizeEdge.svelte";
  import ThreadPanel from "./lib/components/ThreadPanel.svelte";
  import PullView from "./lib/components/PullView.svelte";
  import Viewer from "./lib/components/Viewer.svelte";
  import SimFloat from "./lib/components/SimFloat.svelte";
  import SimPanel from "./lib/components/SimPanel.svelte";
  import { sim } from "./lib/simview.svelte";
  import Lightbox from "./lib/components/Lightbox.svelte";
  import Big from "./lib/viewkit/Big.svelte";
  import ThreadView from "./lib/components/ThreadView.svelte";
  import DeskPanel from "./lib/components/DeskPanel.svelte";
  import TourCard from "./lib/components/TourCard.svelte";
  import HandoffDialog from "./lib/components/HandoffDialog.svelte";
  import { handing, loadTakers } from "./lib/ownTools.svelte";
  import { installHistory } from "./lib/history.svelte";
  import { installAutoInstall } from "./lib/autoInstall";
  import WindowControls from "./lib/components/WindowControls.svelte";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import BottomPanel from "./lib/components/BottomPanel.svelte";
  import { bar } from "./lib/statusbar.svelte";
  import { drag, focusWindow, inTauri, isDevWindow, ownTitleBar, poppedViewing, setTitle } from "./lib/native";
  import { seeing, startPresence } from "./lib/presence";
  import { live as liveMode, showingLive } from "./lib/live.svelte";
  import LiveNote from "./lib/components/LiveNote.svelte";
  import Inspector from "./lib/components/Inspector.svelte";
  import InspectOverlay from "./lib/components/InspectOverlay.svelte";
  import InspectCard from "./lib/components/InspectCard.svelte";
  import { inspect } from "./lib/inspect.svelte";
  import InstallBox from "./lib/components/InstallBox.svelte";
  import { hub } from "./lib/state.svelte";
  import { prefs, setPref } from "./lib/prefs.svelte";
  import { installZoom } from "./lib/zoom";
  import { watchScreens } from "./lib/screens.svelte";
  import { changeWorld } from "./lib/worldSwitch";
  import Icon from "./lib/components/Icon.svelte";
  import ProjectIconPicker from "./lib/components/ProjectIconPicker.svelte";
  import { looks } from "./lib/looks.svelte";
  import { clock } from "./lib/format";

  let homeComposer = $state<Composer>();
  let paletteOpen = $state(false);

  onMount(() => {
    const stopHub = hub.start();
    looks.load();
    const stopZoom = installZoom();
    const stopScreens = watchScreens();
    const stopHistory = installHistory();
    const stopPopped = hub.fromPopped(() => quiet(focusWindow(), "focus window"));
    // Only the app's real window installs versions of Hub; not the dev window or a browser tab.
    const stopAuto = inTauri && !isDevWindow ? installAutoInstall() : () => {};
    // Which of your tools take a thread's work, for the thread menu; the hosts are known a moment later.
    setTimeout(() => loadTakers(), 3000);
    // The main window tells the machine which thread you are at, so the phone stays quiet about it.
    const stopPresence = !poppedViewing() && !isDevWindow ? startPresence() : () => {};
    let stopLive = () => {};
    liveMode.start().then((stop) => (stopLive = stop), () => {});
    return () => {
      stopPresence();
      stopLive();
      stopPopped();
      stopAuto();
      stopHub();
      stopZoom();
      stopScreens();
      stopHistory();
    };
  });

  // Which parts of Hub are used, names only (uselog.svelte.ts).
  $effect(() => watchUse());
  $effect(() => {
    track(`page.${hub.view}`);
  });
  $effect(() => {
    if (hub.settingsOpen) track(`settings.${hub.settingsSection}`);
  });

  $effect(() => {
    const t = hub.view === "thread" ? hub.current : null;
    const seen = t ? { id: t.id, host: t.host ?? null } : null;
    untrack(() => seeing(seen));
  });

  // Room for the window buttons is only kept where Hub draws them.
  if (ownTitleBar) document.documentElement.classList.add("own-titlebar");

  // The theme is an attribute on <html>, so app.css can swap the tokens.
  $effect(() => {
    applyTheme(prefs.theme);
  });

  // The world (Dev or Chat) is another attribute, for the warm tokens, and follows the page: a
  // chat puts the window in Chat, any other page in Dev.
  $effect(() => {
    document.documentElement.dataset.world = hub.world;
  });
  $effect(() => {
    void hub.view;
    void hub.current;
    untrack(() => hub.syncWorld());
  });

  // Closing the viewer or a pull request in the right panel is remembered for Ctrl Shift T.
  let lastViewing = untrack(() => hub.viewing);
  let lastPull = untrack(() => hub.pull);
  $effect(() => {
    const v = hub.viewing;
    untrack(() => {
      if (lastViewing && !v) closed({ viewing: lastViewing });
      lastViewing = v;
    });
  });
  $effect(() => {
    const p = hub.pull;
    untrack(() => {
      if (lastPull && !p) closed({ pull: lastPull });
      lastPull = p;
    });
  });

  // Settings are a window over the page; going somewhere else (a thread linked from them, back,
  // a click in the sidebar) closes them.
  $effect(() => {
    void hub.route;
    hub.settingsOpen = false;
  });

  $effect(() => {
    const n = hub.waiting.length;
    quiet(setTitle((n ? `(${n}) ` : "") + (isDevWindow ? "Hub · dev" : "Hub")), "set title");
  });

  // The right panel follows what you are looking at. The project page has its own. A picture a
  // side job made shows in its parent's viewer too.
  const viewingHere = $derived(
    !!hub.current && !!hub.viewing &&
      (hub.viewing.thread === hub.current.id || hub.threads.find((t) => t.id === hub.viewing!.thread)?.parent === hub.current.id),
  );
  const panel = $derived(
    hub.view === "thread" && viewingHere ? "viewer"
    : hub.view === "thread" && hub.current && hub.pull?.thread === hub.current.id ? "pull"
    : hub.panelOpen && hub.view === "thread" && hub.current && hub.world !== "chat" ? "thread"
    : hub.panelOpen && hub.view === "home" && (hub.strays.length || hub.strained) ? "home"
    : null,
  );

  // Every pane's edge drags to resize it; the width is remembered when you let go. The viewer and
  // pull panel keep `viewerWidth`, the rest `widths` by name ("sidebar", "column", "side").
  let live = $state<Record<string, number | null>>({});
  const viewerWidth = $derived(live.viewer ?? prefs.viewerWidth);
  const widthOf = (key: string) => live[key] ?? prefs.widths[key] ?? 0;
  /** The sidebar's own width: the one dragged, else 256px grown by as much as the text, so large text has room. */
  const sidebarW = $derived(widthOf("sidebar") ? `max(${widthOf("sidebar")}px, calc(240px * var(--text-grow, 1)))` : "calc(256px * var(--text-grow, 1))");
  const keepWidth = (key: string, w: number | null) => {
    if (w == null) return;
    setPref("widths", { ...prefs.widths, [key]: w });
  };
  const forget = (key: string) => {
    const { [key]: _, ...rest } = prefs.widths;
    setPref("widths", rest);
  };
  const px = (w: number) => (w ? `${w}px` : undefined);
  /** A dragged panel gives way before the chat does: the sidebar and rail keep theirs, the chat keeps 420px. */
  const cap = (w: number) => (w ? "calc(100vw - 480px - var(--sidebar-w, 0px))" : undefined);

  function newThread() {
    hub.goNew();
    queueMicrotask(() => homeComposer?.focus());
  }

  const typing = (el: EventTarget | null) =>
    el instanceof HTMLElement && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName));

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape" && hub.viewing && !e.defaultPrevented) {
      hub.viewing = null;
      return;
    }
    // Alt 0, 1, 2: Home, Inbox, System. Alt 3: the other world, Dev or Chat (the rail's first button).
    if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && ["Digit0", "Digit1", "Digit2", "Digit3"].includes(e.code)) {
      e.preventDefault();
      if (e.code === "Digit3") changeWorld();
      else if (e.code === "Digit0") hub.goHome();
      else {
        hub.enterRoom(e.code === "Digit1" ? "work" : "control");
        hub.sidebarOpen = true;
      }
      return;
    }
    if (!(e.ctrlKey || e.metaKey)) return;
    if (e.shiftKey && !e.altKey && e.code === "KeyL" && liveMode.offered) {
      // Ctrl Shift L: Live mode on or off, as the rail's button.
      e.preventDefault();
      liveMode.toggle();
      return;
    }
    if (e.shiftKey && !e.altKey && e.code === "KeyI") {
      // Ctrl Shift I: Inspect mode, to mark things in Hub itself and start a thread from them.
      e.preventDefault();
      inspect.toggle();
      return;
    }
    if (e.shiftKey && !e.altKey && e.code === "KeyT") {
      e.preventDefault();
      if (!reopen()) hub.fail(new Error("Nothing closed to bring back"));
      return;
    }
    if (e.shiftKey && !e.altKey && (e.code === "KeyC" || e.code === "KeyK") && hub.view === "thread" && hub.current) {
      // Ctrl Shift C: the thread's pull request link, or the thread as an @ reference to paste
      // in a message. Ctrl Shift K: the pull request as #123.
      const t = hub.current;
      const pr = hub.prs[t.id];
      e.preventDefault();
      if (e.code === "KeyK" && !pr) return hub.fail(new Error("This thread has no pull request"));
      const what = e.code === "KeyK" ? `#${pr.number}` : pr ? pr.url : `@${t.title} `;
      copy(what).then(() => hub.tell(`Copied ${e.code === "KeyK" ? what : pr ? "the pull request link" : "the thread as @" + t.title}`));
      return;
    }
    if (e.key === "k") {
      e.preventDefault();
      paletteOpen = !paletteOpen;
    } else if (e.key === "i" && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      hub.ideaOpen = !hub.ideaOpen;
    } else if (e.key === "n") {
      e.preventDefault();
      newThread();
    } else if (e.key === "j") {
      e.preventDefault();
      if (!hub.nextWaiting()) hub.fail(new Error("Nothing is waiting on you"));
    } else if (e.key === "b") {
      e.preventDefault();
      hub.sidebarOpen = !hub.sidebarOpen;
    } else if (e.key === "e") {
      e.preventDefault();
      hub.switchRoom();
      hub.sidebarOpen = true;
    } else if (e.key === "z" && !e.shiftKey && hub.undo && !typing(e.target)) {
      // Outside a text field, Ctrl Z takes back the last settle.
      e.preventDefault();
      hub.undo.run();
    } else if (e.key === ",") {
      e.preventDefault();
      if (hub.settingsOpen) hub.settingsOpen = false;
      else hub.openSettings();
    } else if (e.key === ".") {
      e.preventDefault();
      hub.panelOpen = !hub.panelOpen;
    } else if (e.code === "Backquote" && e.shiftKey) {
      // With Shift: the terminal in the panel above the status bar.
      e.preventDefault();
      bar.togglePanel("terminal");
    } else if (e.code === "Backquote") {
      // The key under Esc: § on a Nordic keyboard, ` on a US one.
      e.preventDefault();
      hub.toggleTerminal();
    }
  }
</script>

<svelte:window onkeydown={keydown} />

{#if hub.settingsOpen}<Settings onclose={() => (hub.settingsOpen = false)} />{/if}
{#if hub.newProjectOpen}<NewProject onclose={() => (hub.newProjectOpen = false)} />{/if}
{#if hub.ideaOpen}<IdeaBox onclose={() => (hub.ideaOpen = false)} />{/if}
{#if paletteOpen}<CommandPalette onclose={() => (paletteOpen = false)} onnew={newThread} />{/if}
<Lightbox />
<Big />
<ContextMenu />
<ProjectIconPicker />
<BagDrop />
{#if handing.to && hub.connected}{#key handing.to}<HandoffDialog thread={handing.to.thread} taker={handing.to.taker} />{/key}{/if}

{#if ownTitleBar}<WindowControls />{/if}

<!-- Without a right panel, the window buttons sit over the top of main, which leaves room for them:
     the thread header keeps them clear on the right, the other pages start below them. -->
<div class="app" class:inspecting={inspect.on} style:--below="{bar.panel ? bar.height : 0}px" class:titlebar={ownTitleBar} class:bare={!panel && !inspect.on} class:live={showingLive} style:--sidebar-w={hub.sidebarOpen ? `calc(${sidebarW} * var(--side-scale, 1))` : "0px"}>
  <Rail />
  <Sidebar onnew={newThread} onsearch={() => (paletteOpen = true)} width={widthOf("sidebar")}
    onlive={(w) => (live.sidebar = w)} oncommit={(w) => keepWidth("sidebar", w)} onreset={() => forget("sidebar")} />
  <main>
    {#if ownTitleBar}<div class="grip" use:drag></div>{/if}
    {#if hub.connected && hub.halt}
      {@const h = hub.halt}
      {@const missed = [...h.hosts, ...h.tools].filter((r) => !r.ok)}
      <!-- While paused, every page says so and how to undo it, in a strip above the page. -->
      <div class="halt" role="status">
        <Icon name="pause" size={13} />
        <span>
          Every agent is paused since {clock(h.since)}{h.by !== hub.localHost ? ` (from ${h.by})` : ""}.
          {h.threads.length ? `${h.threads.length} ${h.threads.length === 1 ? "turn" : "turns"} carry on when you resume;` : ""} messages you send wait until then.
          {#if missed.length}<span class="missed">Not reached: {missed.map((r) => `${r.name} (${r.detail ?? "failed"})`).join(", ")}.</span>{/if}
        </span>
        <button disabled={hub.halting} onclick={() => hub.resumeAll()}><Icon name="play" size={12} />Resume all</button>
      </div>
    {/if}
    {#if hub.view === "thread" && hub.current}
      {#key hub.current.id}<ThreadView thread={hub.current} />{/key}
    {:else if hub.view === "project" && hub.project}
      {#key `${hub.projectHost ?? ""}\u0000${hub.project}`}<ProjectView path={hub.project} host={hub.projectHost} />{/key}
    {:else if hub.view === "welcome"}
      <Welcome />
    {:else if hub.view === "labs"}
      <Labs />
    {:else if hub.view === "overview"}
      <Overview />
    {:else if hub.view === "problems"}
      <ProblemsView />
    {:else if hub.view === "routines"}
      <RoutinesView />
    {:else if hub.view === "recipes"}
      <RecipesView />
    {:else if hub.view === "services"}
      <HostPanel full />
    {:else if hub.view === "machines"}
      <MachinesView />
    {:else if hub.view === "machine" && hub.machine}
      {#key hub.machine}<MachineView name={hub.machine} />{/key}
    {:else if hub.view === "library"}
      <LibraryView />
    {:else if hub.view === "plugins"}
      <PluginsView />
    {:else if hub.view === "workers"}
      <WorkersView />
    {:else}
      <Home bind:composer={homeComposer} mode={hub.view === "new" ? "new" : "home"} />
    {/if}
    {#if !hub.connected}
      <div class="banner">The core is not running. Start <code>hubd</code>, Hub reconnects on its own.</div>
    {/if}
    {#if hub.error}
      <div class="toast" in:toast out:toast={{ out: true }}>
        {hub.error}
        {#if hub.errorAction}
          {@const action = hub.errorAction}
          <button class="toast-action" onclick={() => { hub.error = null; hub.errorAction = null; action.run(); }}>{action.label}</button>
        {/if}
      </div>
    {/if}
    {#if hub.notice && !hub.error}<div class="toast quiet" in:toast out:toast={{ out: true }}>{hub.notice}</div>{/if}
    {#if sim.shown && sim.shown.thread === hub.current?.id && hub.viewing?.kind !== "sim"}<SimFloat udid={sim.shown.udid} host={sim.shown.host} />{/if}
    {#if liveMode.note}<LiveNote />{/if}
    {#if hub.tour}<TourCard />{/if}
    {#if hub.installing}<InstallBox />{/if}
  </main>
  {#if inspect.on && panel !== "viewer" && panel !== "pull"}
    <Inspector />
  {:else if panel === "viewer" && hub.viewing}
    <aside class="right wide" style:--viewer-w={viewerWidth ? `${viewerWidth}px` : undefined}>
      <ResizeEdge side="left" min={360} label="Resize the viewer" onlive={(w) => (live.viewer = w)} oncommit={(w) => setPref("viewerWidth", w)} onreset={() => setPref("viewerWidth", 0)} />
      {#if ownTitleBar}<div class="caption" use:drag></div>{/if}{#if hub.viewing.kind === "sim" && sim.shown}<SimPanel udid={sim.shown.udid} host={sim.shown.host} />{:else}<Viewer viewing={hub.viewing} />{/if}
    </aside>
  {:else if panel === "pull" && hub.pull}
    {@const open = hub.pull}
    <aside class="right wide" style:--viewer-w={viewerWidth ? `${viewerWidth}px` : undefined}>
      <ResizeEdge side="left" min={360} label="Resize the panel" onlive={(w) => (live.viewer = w)} oncommit={(w) => setPref("viewerWidth", w)} onreset={() => setPref("viewerWidth", 0)} />
      {#if ownTitleBar}<div class="caption" use:drag></div>{/if}
      <div class="pull-panel">
        {#key open.number}<PullView path={open.path} host={open.host} number={open.number} narrow onclose={() => (hub.pull = null)}
          onthread={(text) => hub.send(open.thread, text)} />{/key}
      </div>
    </aside>
  {:else if panel === "thread" && hub.current && hub.isDesk(hub.current)}
    <aside class="right" style:--panel-w={px(widthOf("side"))} style:--cap={cap(widthOf("side"))}>{@render sideEdge("side")}{#if ownTitleBar}<div class="caption" use:drag></div>{/if}{#key hub.current.id}<DeskPanel desk={hub.current} />{/key}</aside>
  {:else if panel === "thread" && hub.current}
    <aside class="right column" style:--col-w={px(widthOf("column"))} style:--cap={cap(widthOf("column"))}>{@render sideEdge("column")}{#if ownTitleBar}<div class="caption" use:drag></div>{/if}{#key hub.current.id}<ThreadPanel thread={hub.current} />{/key}</aside>
  {:else if panel === "home"}
    <aside class="right" style:--panel-w={px(widthOf("side"))} style:--cap={cap(widthOf("side"))}>{@render sideEdge("side")}{#if ownTitleBar}<div class="caption" use:drag></div>{/if}<HomePanel /></aside>
  {/if}
  {#if inspect.on && (panel === "viewer" || panel === "pull")}<Inspector />{/if}
</div>
{#if inspect.on}<InspectOverlay />{#if inspect.sending}<InspectCard />{/if}{/if}
{#if bar.panel}<BottomPanel />{/if}
<StatusBar />

{#snippet sideEdge(key: string)}
  <ResizeEdge side="left" min={key === "column" ? 300 : 260} label="Resize the panel" onlive={(w) => (live[key] = w)} oncommit={(w) => keepWidth(key, w)} onreset={() => forget(key)} />
{/snippet}

<style>
  /* The status bar (and the panel above it, when open) sit under the app, as in VS Code. */
  .app { display: flex; height: calc(100% - var(--bar-h) - var(--below, 0px)); }
  /* Live mode: a thin frame in the accent round the whole window, so it is never mistaken for
     the installed pages. Drawn over everything, it takes no room and no clicks. */
  .app.live::after { content: ""; position: fixed; inset: 0; z-index: 10000; pointer-events: none; box-shadow: inset 0 0 0 2px var(--accent); }
  main { flex: 1; min-width: 0; display: flex; flex-direction: column; position: relative; }
  .right.wide { position: relative; width: var(--viewer-w, min(52vw, 900px)); max-width: calc(100vw - 420px); border-left: 1px solid var(--line-strong); box-shadow: -10px 0 30px rgba(0, 0, 0, 0.28); }
  .right { position: relative; width: var(--panel-w); max-width: var(--cap, none); flex: none; background: var(--bg-sidebar); display: flex; flex-direction: column; min-height: 0; }
  /* A thread's side column is wider: what runs, what changed and its output share it. With large
     text near the narrow end it gives way, so the sidebar (256px) and the chat keep their room. */
  .right.column { width: var(--col-w, calc(clamp(340px, 27vw, 560px) * var(--text-grow))); max-width: var(--cap, calc(100vw - 720px)); border-left: 1px solid var(--line); }
  /* Too narrow for a panel: the viewer covers the window instead, and Esc brings the thread back. */
  @media (max-width: 1100px) { .right { display: none; } .right.wide { display: flex; position: fixed; inset: 0; z-index: 80; width: auto; max-width: none; } .titlebar { --caption-room: var(--controls-w); --page-top: max(28px, calc(var(--controls-h) + 10px)); } }
  .titlebar.bare { --caption-room: var(--controls-w); --page-top: max(28px, calc(var(--controls-h) + 10px)); }
  .grip { position: absolute; top: 0; left: 0; right: 0; height: 10px; z-index: 5; }
  .caption { height: var(--controls-h); flex: none; }
  .banner { position: absolute; top: 12px; left: 50%; transform: translateX(-50%); background: var(--surface-2); border: 1px solid var(--line-strong); color: var(--text-2); padding: 8px 14px; border-radius: var(--r-md); font-size: 13px; }
  .halt { flex: none; display: flex; align-items: center; gap: 10px; padding: 8px 16px; padding-right: calc(16px + var(--caption-room, 0px)); background: var(--amber-bg); border-bottom: 1px solid var(--line); color: var(--text-2); font-size: 13px; }
  .halt > :global(svg) { color: var(--amber); flex: none; }
  .halt > span { flex: 1; min-width: 0; }
  .halt .missed { color: var(--amber); }
  .halt button { flex: none; display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: var(--r-md); color: var(--amber); box-shadow: inset 0 0 0 1px var(--amber); font-weight: 500; }
  .halt button:hover { color: var(--text); }
  .toast { position: absolute; bottom: 110px; left: 50%; transform: translateX(-50%); background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--red-bg); color: var(--red); padding: 8px 14px; border-radius: var(--r-md); font-size: 13px; max-width: 80%; }
  .toast.quiet { color: var(--text-2); box-shadow: inset 0 0 0 1px var(--line-strong); }
  .toast-action { margin-left: 10px; padding: 2px 8px; border-radius: var(--r-sm); background: var(--surface-hover); color: var(--text); font-size: 13px; }
  .toast-action:hover { background: var(--line-strong); }
  .pull-panel { flex: 1; overflow-y: auto; padding: 18px 20px; min-height: 0; }
</style>
