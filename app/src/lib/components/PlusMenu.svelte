<!-- The + in the composer: everything you can add to this message or this thread in one menu. Files and pictures, the context bag, another thread, the folder's uncommitted changes and the web switch work with any Hub. What a thread can reach, extra folders and repos and the connections it may use, is set per thread when the running Hub has `thread.reach`; an older one lists them and says the switches need the next version.
     Used by Composer.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // The + in the composer: everything you can add to this message or this thread in one menu.
  // Files and pictures, the context bag, another thread, the folder's uncommitted changes and the
  // web switch work with any Hub. What a thread can reach, extra folders and repos and the
  // connections it may use, is set per thread when the running Hub has `thread.reach`; an older
  // one lists them and says the switches need the next version.
  import { api } from "../api";
  import { hubdHas } from "../compat";
  import type { ProjectFolder } from "../bindings/ProjectFolder";
  import type { ReachFolder } from "../bindings/ReachFolder";
  import type { ThreadReach } from "../bindings/ThreadReach";
  import type { Attachment } from "../bindings/Attachment";
  import type { Connection } from "../bindings/Connection";
  import { bag } from "../bag.svelte";
  import { SOURCE, STATE } from "../connections";
  import { folderName } from "../format";
  import type { GithubRepo } from "../bindings/GithubRepo";
  import { cloneUrlOf, gh, keyOf, load as ghLoad } from "../github.svelte";
  import { hub } from "../state.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";
  import BagMenu from "./BagMenu.svelte";
  import Icon from "./Icon.svelte";
  import Switch from "./Switch.svelte";

  type Sub = "bag" | "repo" | "conn" | "thread";

  let {
    threadId = null,
    engine,
    from,
    images = true,
    web,
    canDiff,
    cwd = "",
    host = null,
    reach = {},
    onreach,
    onfiles,
    onbag,
    onthread,
    ondiff,
    onweb,
    onclose,
  }: {
    /** The open thread; null on a new thread's box. */
    threadId?: string | null;
    engine: string;
    /** What new bag items say they came from. */
    from?: string;
    /** Whether the engine takes pictures. */
    images?: boolean;
    web: boolean;
    /** The folder is known, so its uncommitted changes can go along. */
    canDiff: boolean;
    /** The thread's own folder, which is never listed as an extra one. */
    cwd?: string;
    /** The machine the folders are on, when not this one. */
    host?: string | null;
    /** What the thread reaches now. */
    reach?: ThreadReach;
    /** Sets it; rejects with the reason when Hub refuses (a folder that is not there). */
    onreach?: (next: ThreadReach) => Promise<void> | void;
    onfiles: () => void;
    onbag: (items: Attachment[]) => void;
    onthread: (t: { id: string; title: string }) => void;
    ondiff: () => void;
    onweb: (on: boolean) => void;
    onclose: () => void;
  } = $props();

  let sub = $state<Sub | null>(null);
  let root: HTMLDivElement;
  let fly: HTMLDivElement | undefined = $state();

  const NEXT = "Arrives with the next version of Hub";
  let has = $state<boolean | null>(null);
  hubdHas("thread.reach").then((v) => (has = v)); // compat-until: 2026-12-03
  let problem = $state<string | null>(null);
  async function change(next: ThreadReach) {
    problem = null;
    try {
      await onreach?.(next);
    } catch (e) {
      problem = e instanceof Error ? e.message : String(e);
    }
  }

  // ---- Folders and repos ----
  const added = $derived(reach.folders ?? []);
  const MAX = 8;
  let found = $state<ProjectFolder[] | null>(null);
  let folderQuery = $state("");
  async function loadFolders() {
    void ghLoad(host ? [null, host] : [null]);
    if (found) return;
    try {
      found = (await api.projectFolders(host)).projects;
    } catch {
      found = [];
    }
  }
  const same = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();
  const matches = $derived(
    (found ?? [])
      .filter((p) => !same(p.path, cwd) && !added.some((f) => same(f.path, p.path)))
      .filter((p) => !folderQuery.trim() || `${p.name} ${p.path}`.toLowerCase().includes(folderQuery.trim().toLowerCase()))
      .slice(0, 40),
  );
  // GitHub repositories of the signed-in account that are not cloned on this machine yet: found by
  // typing, added by cloning them next to the other projects first.
  const onGithub = $derived.by(() => {
    const q = folderQuery.trim().toLowerCase();
    if (!q || looksLikePath(q) || gh.state !== "ready") return [] as GithubRepo[];
    const have = new Set((found ?? []).map((p) => p.key?.toLowerCase()).filter((k): k is string => !!k));
    return gh.repos.filter((r) => !r.archived && !have.has(keyOf(r)) && r.full.toLowerCase().includes(q)).slice(0, 8);
  });
  let cloning = $state<string | null>(null);
  async function cloneAndAdd(r: GithubRepo) {
    if (cloning) return;
    cloning = r.full;
    problem = null;
    try {
      const p = await api.projectFolders(host);
      const path = p.root.replace(/[\\/]+$/, "") + (p.sep || "/") + r.name;
      await api.cloneRepo(cloneUrlOf(r), path, host);
      found = null;
      await addFolder(path, r.name);
      loadFolders();
    } catch (e) {
      problem = e instanceof Error ? e.message : String(e);
    } finally {
      cloning = null;
    }
  }
  const looksLikePath = (q: string) => /^([\\/]|[a-z]:[\\/])/i.test(q.trim());
  function addFolder(path: string, name?: string) {
    const folder: ReachFolder = name ? { path, name } : { path };
    folderQuery = "";
    return change({ ...reach, folders: [...added, folder] });
  }
  const dropFolder = (path: string) => change({ ...reach, folders: added.filter((f) => f.path !== path) });
  function folderEnter() {
    const q = folderQuery.trim();
    if (looksLikePath(q)) addFolder(q);
    else if (matches[0]) addFolder(matches[0].path, matches[0].name);
  }

  // ---- Connections per thread ----
  const offHere = (c: Connection) => (reach.connections_off ?? []).includes(c.name);
  function toggleConn(c: Connection, on: boolean) {
    const rest = (reach.connections_off ?? []).filter((n) => n !== c.name);
    return change({ ...reach, connections_off: on ? rest : [...rest, c.name] });
  }

  // ---- Another thread ----
  let query = $state("");
  const threads = $derived(
    hub.threads
      .filter((t) => hub.inWorld(t) && t.id !== threadId && !t.archived && t.title.trim().length > 2 && (!query.trim() || t.title.toLowerCase().includes(query.trim().toLowerCase())))
      .sort((a, b) => b.updated_at - a.updated_at)
      .slice(0, 30),
  );

  // ---- Connections ----
  let conns = $state<Connection[] | null>(null);
  let connError = $state<string | null>(null);
  async function loadConnections() {
    if (conns || connError) return;
    try {
      if (threadId) {
        const t = await api.threadConnections(threadId);
        conns = [...t.used, ...t.available];
      } else {
        conns = (await api.connections()).engines.find((e) => e.engine === engine)?.connections ?? [];
      }
    } catch (e) {
      connError = e instanceof Error ? e.message : String(e);
    }
  }
  const reachable = $derived((conns ?? []).filter((c) => c.source !== "hub"));
  const tone = (c: Connection) => (c.off ? "" : STATE[c.state].tone);

  function open(next: Sub | null) {
    if (next !== sub) problem = null;
    sub = next;
    if (next === "conn") loadConnections();
    if (next === "repo") loadFolders();
    if (next) queueMicrotask(focusFly);
  }
  function focusFly() {
    fly?.querySelector<HTMLElement>("input, button:not([disabled])")?.focus();
  }
  /** Hovering a row with a list opens it; a row without one leaves an open list alone, so the
   *  pointer can cross the menu on its way to the list. */
  const hover = (next: Sub) => {
    if (sub) open(next);
  };

  function rows(): HTMLElement[] {
    return [...root.querySelectorAll<HTMLElement>(".main .row")];
  }
  function key(e: KeyboardEvent) {
    const inFly = !!fly && fly.contains(e.target as Node);
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (sub) {
        const was = sub;
        sub = null;
        root.querySelector<HTMLElement>(`.main [data-sub="${was}"]`)?.focus();
      } else onclose();
      return;
    }
    if (inFly) {
      if (e.key === "ArrowLeft" && (e.target as HTMLElement).tagName !== "INPUT") {
        e.preventDefault();
        const was = sub;
        sub = null;
        root.querySelector<HTMLElement>(`.main [data-sub="${was}"]`)?.focus();
      }
      return;
    }
    const list = rows();
    const at = list.indexOf(document.activeElement as HTMLElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      list[(at + step + list.length) % list.length]?.focus();
    } else if (e.key === "ArrowRight") {
      const s = (document.activeElement as HTMLElement | null)?.dataset.sub as Sub | undefined;
      if (s) {
        e.preventDefault();
        open(s);
      }
    }
  }

  $effect(() => {
    rows()[0]?.focus();
  });
</script>

<div class="plus" bind:this={root} role="menu" tabindex="-1" aria-label="Add context" onkeydown={key}>
  <div class="main">
    <button class="row" role="menuitem" onclick={onfiles}>
      <Icon name="clip" size={15} /><span class="grow">Add files or photos</span><span class="hint">Ctrl U</span>
    </button>
    <button class="row" class:on={sub === "bag"} role="menuitem" data-sub="bag" aria-haspopup="true" onclick={() => open("bag")} onmouseenter={() => hover("bag")}>
      <Icon name="bag" size={15} /><span class="grow">Context bag</span>{#if bag.items.length}<span class="hint">{bag.items.length}</span>{/if}<Icon name="chevron" size={13} />
    </button>
    <div class="rule"></div>
    <button class="row" class:on={sub === "repo"} role="menuitem" data-sub="repo" aria-haspopup="true" onclick={() => open("repo")} onmouseenter={() => hover("repo")}>
      <Icon name="branch" size={15} /><span class="grow">Folder or repo</span>{#if added.length}<span class="hint">{added.length}</span>{/if}<Icon name="chevron" size={13} />
    </button>
    <button class="row" class:on={sub === "conn"} role="menuitem" data-sub="conn" aria-haspopup="true" onclick={() => open("conn")} onmouseenter={() => hover("conn")}>
      <Icon name="plug" size={15} /><span class="grow">Connections</span>{#if conns}<span class="hint">{reachable.filter((c) => !c.off && !offHere(c)).length} on</span>{/if}<Icon name="chevron" size={13} />
    </button>
    <button class="row" class:on={sub === "thread"} role="menuitem" data-sub="thread" aria-haspopup="true" onclick={() => open("thread")} onmouseenter={() => hover("thread")}>
      <Icon name="threads" size={15} /><span class="grow">Another thread</span><span class="hint">@</span><Icon name="chevron" size={13} />
    </button>
    {#if canDiff}
      <button class="row" role="menuitem" onclick={ondiff}>
        <Icon name="diff" size={15} /><span class="grow">Uncommitted changes</span><span class="hint">/diff</span>
      </button>
    {/if}
    <div class="rule"></div>
    <div class="row plain" role="menuitem" tabindex="-1">
      <Icon name="globe" size={15} /><span class="grow">Search the web</span>
      <Switch on={web} label="Search the web" onchange={onweb} />
    </div>
  </div>

  {#if sub}
    <div class="fly" class:wide={sub === "bag"} bind:this={fly}>
      {#if sub === "bag"}
        <BagMenu {from} {threadId} {images} onclose={onclose} onpick={onbag} />
      {:else if sub === "repo"}
        <div class="title">Folders and repos the agent can also look in</div>
        {#if has === false}
          <p class="note">{NEXT}.</p>
        {:else}
          {#if added.length}
            <div class="list keep">
              {#each added as f (f.path)}
                <div class="item" data-tip={f.path}>
                  <Icon name="branch" size={13} />
                  <span class="two grow"><span class="name">{f.name ?? folderName(f.path)}</span><span class="sub">{f.path}</span></span>
                  <button class="x" aria-label="Remove {f.name ?? folderName(f.path)}" onclick={() => dropFolder(f.path)}><Icon name="x" size={12} /></button>
                </div>
              {/each}
            </div>
            <div class="rule"></div>
          {/if}
          {#if added.length >= MAX}
            <p class="note">Limit reached.</p>
          {:else}
            <div class="search">
              <Icon name="search" size={13} />
              <input bind:value={folderQuery} placeholder="Find a repo, or paste a folder path" spellcheck="false" onkeydown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  folderEnter();
                }
              }} />
            </div>
            <div class="list">
              {#if looksLikePath(folderQuery)}
                <button class="item pick" onclick={() => addFolder(folderQuery.trim())}>
                  <Icon name="plus" size={13} /><span class="two"><span class="name">Add {folderName(folderQuery.trim()) || folderQuery.trim()}</span><span class="sub">{folderQuery.trim()}</span></span>
                </button>
              {/if}
              {#each matches as p (p.path)}
                <button class="item pick" onclick={() => addFolder(p.path, p.name)}>
                  <Icon name="branch" size={13} /><span class="two"><span class="name">{p.name}</span><span class="sub">{p.branch ? `${p.branch} · ` : ""}{p.path}</span></span>
                </button>
              {:else}
                {#if !looksLikePath(folderQuery) && !onGithub.length}<p class="note">{found ? (folderQuery.trim() ? "No repo by that name." : "No repos found.") : "Looking…"}</p>{/if}
              {/each}
              {#if onGithub.length}
                <div class="note head">On GitHub, not on {host ?? hub.localHost} yet</div>
                {#each onGithub as r (r.full)}
                  <button class="item pick" disabled={!!cloning} onclick={() => cloneAndAdd(r)}>
                    <ProviderLogo id="github" size={13} />
                    <span class="two grow"><span class="name">{r.name}</span><span class="sub one">{r.full}{r.private ? " · private" : ""}</span></span>
                    <span class="cl">{cloning === r.full ? "Cloning…" : "Clone and add"}</span>
                  </button>
                {/each}
              {/if}
              {#if folderQuery.trim() && gh.state === "none" && !looksLikePath(folderQuery)}
                <p class="note">Only clones on this machine. Connect GitHub to find the rest.</p>
              {/if}
            </div>
          {/if}
          {#if problem}<p class="note bad">{problem}</p>{/if}
        {/if}
      {:else if sub === "conn"}
        <div class="title">What this thread's agent can reach</div>
        {#if connError}
          <p class="note">Could not ask: {connError}</p>
        {:else if !conns}
          <p class="note">Asking {engine}…</p>
        {:else if !reachable.length}
          <Quiet text="No connections here" />
        {:else}
          <div class="list">
            {#each reachable as c (c.name)}
              {@const off = c.off || offHere(c)}
              <div class="item" class:off data-tip="{SOURCE[c.source]}{c.target ? ` · ${c.target}` : ''}">
                <span class="ldot {off ? '' : tone(c)}"></span>
                <span class="grow name">{c.label}</span>
                {#if has && !c.off}
                  <span class="state" class:warn={!off && STATE[c.state].tone === "warn"} class:bad={!off && STATE[c.state].tone === "bad"}>{off ? "Off here" : STATE[c.state].label}</span>
                  <Switch on={!off} label="{c.label} in this thread" onchange={(on) => toggleConn(c, on)} />
                {:else}
                  <span class="state" class:warn={!off && STATE[c.state].tone === "warn"} class:bad={!off && STATE[c.state].tone === "bad"}>{c.off ? "Off in Settings" : STATE[c.state].label}</span>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
        {#if problem}<p class="note bad">{problem}</p>{/if}
        {#if has === false}
          <p class="note">Per-thread switches arrive with the next version of Hub. Settings > Extensions switches them for every thread.
            <button class="link" onclick={() => { hub.openSettings("extensions"); onclose(); }}>Open Settings</button></p>
        {:else}
          <p class="note">This thread only. Settings > Extensions switches every thread.
            <button class="link" onclick={() => { hub.openSettings("extensions"); onclose(); }}>Open Settings</button></p>
        {/if}
      {:else if sub === "thread"}
        <div class="search">
          <Icon name="search" size={13} />
          <input bind:value={query} placeholder="Find a thread to take along" spellcheck="false" onkeydown={(e) => {
            if (e.key === "Enter" && threads[0]) {
              e.preventDefault();
              onthread({ id: threads[0].id, title: threads[0].title });
            }
          }} />
        </div>
        <div class="list">
          {#each threads as t (t.id)}
            <button class="item pick" onclick={() => onthread({ id: t.id, title: t.title })}>
              <span class="two"><span class="name">{t.title}</span><span class="sub">{folderName(t.cwd)}</span></span>
            </button>
          {:else}
            <p class="note">{query.trim() ? "No thread by that name." : "No other threads yet."}</p>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .plus { position: relative; color: var(--text-2); font-size: 14px; outline: none; }
  .main, .fly {
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 4px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  }
  .main { min-width: 268px; }
  .fly { position: absolute; left: calc(100% + 6px); bottom: 0; width: 330px; max-width: calc(100vw - 340px); min-width: 240px; max-height: min(440px, 70vh); display: flex; flex-direction: column; overflow: hidden; }
  .fly.wide { width: auto; max-width: none; padding: 0; overflow: visible; }
  .row {
    display: flex; align-items: center; gap: 10px; width: 100%; padding: 6px 10px; border-radius: var(--r-md);
    text-align: left; color: var(--text-2); white-space: nowrap; line-height: 1.3;
  }
  .row > :global(svg) { color: var(--muted); flex: none; }
  .row:hover, .row.on, .row:focus-visible { background: var(--surface-hover); color: var(--text); outline: none; }
  .row:hover > :global(svg), .row.on > :global(svg), .row:focus-visible > :global(svg) { color: var(--text-3); }
  .row.plain:hover { background: none; color: var(--text-2); }
  .grow { flex: 1; min-width: 0; }
  .hint { color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .rule { height: 1px; background: var(--line-strong); margin: 4px 6px; }
  .title { color: var(--muted); font-size: 12px; padding: 6px 10px 4px; }
  .note { color: var(--muted); font-size: 12px; line-height: 1.45; margin: 2px 10px 6px; white-space: normal; }
  .note.bad { color: var(--red); }
  .note.head { margin: 8px 10px 2px; font-size: 12px; }
  .sub.one { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cl { color: var(--accent); font-size: 12px; flex: none; }
  .x { color: var(--muted); padding: 3px; border-radius: var(--r-sm); flex: none; }
  .x:hover { color: var(--text); background: var(--surface-hover); }
  .list.keep { flex: none; max-height: 140px; }
  .link { color: var(--accent); font-size: 12px; margin-left: 4px; }
  .link:hover { text-decoration: underline; }
  .list { overflow-y: auto; min-height: 0; }
  .item { display: flex; align-items: center; gap: 8px; padding: 4px 10px; border-radius: var(--r-md); min-width: 0; text-align: left; width: 100%; }
  .item.off { opacity: 0.55; }
  .item.pick:hover, .item.pick:focus-visible { background: var(--surface-hover); outline: none; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); }
  .two { display: flex; flex-direction: column; min-width: 0; }
  .sub { color: var(--muted); font-size: 12px; }
  .state { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .state.warn { color: var(--amber); }
  .state.bad { color: var(--red); }
  .ldot { width: 6px; height: 6px; border-radius: 50%; flex: none; background: var(--muted); }
  .ldot.ok { background: var(--green); }
  .ldot.warn { background: var(--amber); }
  .ldot.bad { background: var(--red); }
  .search { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line-strong); color: var(--muted); margin-bottom: 2px; }
  .search input { flex: 1; min-width: 0; background: none; border: 0; outline: none; font-size: 13px; color: var(--text); }
</style>
