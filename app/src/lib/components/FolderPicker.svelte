<!-- The projects found on each machine, kept while Hub is open so the picker opens with them at once; it looks again in the background every time it opens.
     Used by Home.svelte, ProjectView.svelte. -->
<script lang="ts" module>
  // Where a thread that is about no project runs: a fresh folder (`folderPicker.ts`).
  export { scratchFolder, chatFolder } from "../folderPicker";
</script>

<script lang="ts">
  import { quiet } from "../quiet";
  import { poll } from "../poll";
  import type { ProjectFolders } from "../bindings/ProjectFolders";
  import { found, scratchFolder, lastSep, norm, parentOf, baseName, fuzzy, searchRows, away, type Entry, type Row } from "../folderPicker";
  import { pop } from "../popover";

  import Quiet from "./Quiet.svelte";
  import { tick, untrack } from "svelte";
  import { api } from "../api";
  import type { FolderListing } from "../bindings/FolderListing";
  import type { ProjectFolder } from "../bindings/ProjectFolder";
  import type { UpdateStatus } from "../bindings/UpdateStatus";
  import { isChatThread, isScratchRun, threadProject } from "../format";
  import { hub } from "../state.svelte";
  import type { GithubRepo } from "../bindings/GithubRepo";
  import { cloneUrlOf, connect as ghConnect, gh, keyOf, load as ghLoad } from "../github.svelte";
  import HostChip from "./HostChip.svelte";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import ProjectIcon from "./ProjectIcon.svelte";
  import ProviderLogo from "./ProviderLogo.svelte";
  import FolderConnect from "./FolderConnect.svelte";
  import FolderOld from "./FolderOld.svelte";
  import FolderRows from "./FolderRows.svelte";
  import FolderRail from "./FolderRail.svelte";

  // Where a new thread runs. A button that shows the folder; open, it lists projects: Recent (the
  // folders threads ran in), Repositories and Folders across every machine, or the ones on one
  // machine picked in the rail. Each row says where it is, on which machine and on which branch.
  // Under the list sit the ways out that never scroll away: a new project, no project (a scratch
  // folder), browsing folders by hand, the system's dialog. A fuzzy search covers everything on
  // every machine; typing a path (`D:\`, `~/`, `/srv`) browses folders. With GitHub connected
  // (`github.svelte.ts`) the repositories of the account are listed too: one that is cloned
  // somewhere opens that clone, one that is not has "Clone to <machine>".
  let {
    value = $bindable(""),
    host = null,
    placeholder = "",
    system,
    onhost,
  }: {
    value?: string;
    /** The machine the thread runs on now: null for this one. */
    host?: string | null;
    placeholder?: string;
    system?: () => void;
    /** A folder on another machine was picked: the thread runs there. */
    onhost?: (host: string | null) => void;
  } = $props();

  let input = $state<HTMLInputElement>();
  let picker = $state<HTMLElement>();
  let open = $state(false);
  // Where the list opens and how tall it may be: the side of the box with more room (above, as
  // long as it fits), measured against the nearest box that clips it, so a small window or a
  // composer high up shrinks the list (it scrolls inside) instead of cutting it off.
  let room = $state<{ up: boolean; h: number }>({ up: true, h: 460 });
  const GAP = 12;
  const EDGE = 8;
  function measure() {
    if (!picker) return;
    const r = picker.getBoundingClientRect();
    let top = 0;
    let bottom = window.innerHeight;
    for (let el = picker.parentElement; el && el !== document.documentElement; el = el.parentElement) {
      const o = getComputedStyle(el);
      if (o.overflowY === "visible") continue;
      const b = el.getBoundingClientRect();
      top = Math.max(top, b.top);
      bottom = Math.min(bottom, b.bottom);
    }
    const above = r.top - top - GAP - EDGE;
    const below = bottom - r.bottom - GAP - EDGE;
    const up = above >= 460 || above >= below;
    room = { up, h: Math.max(160, Math.floor(up ? above : below)) };
  }
  $effect(() => {
    if (open) untrack(measure);
  });
  let query = $state("");
  let at = $state(0);
  let listing = $state<FolderListing | null>(null);
  let looking = $state(0);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let hidden = $state(false);
  // The machine runs a hubd from before folder listing: its version, this PC's, and how its own
  // updater is doing, so the note can say what happens next and offer what moves it along.
  let old = $state<{ theirs: string | null; ours: string | null; update: UpdateStatus | null } | null>(null);
  let again = $state(0);
  // Typing the name of a new project, after "New project" was picked.
  let naming = $state(false);
  let working = $state<string | null>(null);
  // What the rail shows: a list across all machines, or one machine.
  let view = $state<"recent" | "repos" | "folders" | "github" | { machine: string }>("recent");
  // The machine the buttons below the list act on (new project, scratch, browse): the thread's
  // own when it opens, then the one picked in the rail.
  let target = $state<string | null>(null);
  $effect(() => {
    if (open) untrack(() => { target = host; view = "recent"; });
  });
  // The projects found on each machine (by host, "" for this one) and the ones whose hubd is too
  // old to list them.
  let data = $state<Record<string, ProjectFolders>>({});
  const tooOld = new Set<string>();

  const hk = (h: string | null | undefined) => h ?? "";
  const machine = $derived(target ?? hub.localHost);
  const projects = $derived(data[hk(target)] ?? null);
  const nameOf = (h: string | null) => h ?? hub.localHost;
  /** A path rather than a search: `~`, `D:`, `/x`, `./x` or anything with a slash in it. */
  const browsing = $derived(/^(~|[a-zA-Z]:|[\\/.])/.test(query) || lastSep(query) >= 0);
  /** A repository to clone: an https or scp-style git URL. */
  const cloneUrl = $derived(/^(https?:\/\/[^\s/]+\/[^\s/]+\/[^\s/]+|git@[^\s:]+:[^\s/]+\/[^\s/]+)$/.test(query.trim()) ? query.trim() : null);
  const dir = $derived(browsing ? (query.trim() === "~" ? "~" : query.slice(0, lastSep(query) + 1)) : "");
  const filter = $derived(browsing ? (query.trim() === "~" ? "" : query.slice(lastSep(query) + 1)) : query.trim());
  const sep = $derived(listing?.sep ?? projects?.sep ?? (value.includes("\\") ? "\\" : "/"));
  const home = $derived(listing?.home ?? projects?.home ?? null);
  /** The two-pane list: not while browsing folders, naming a project or cloning. */
  const panes = $derived(!browsing && !naming && !cloneUrl);

  // Projects on every machine that is online: what was found last time at once, then a fresh look.
  $effect(() => {
    if (!open) return;
    void again;
    const here = untrack(() => target);
    const names = untrack(() => hub.hosts.filter((h) => h.state === "online").map((h) => (h.local ? null : h.name)));
    if (!names.includes(here)) names.push(here);
    const seeded: Record<string, ProjectFolders> = {};
    for (const n of names) {
      const d = found.get(hk(n));
      if (d) seeded[hk(n)] = d;
    }
    untrack(() => (data = { ...seeded, ...data }));
    for (const n of names) {
      untrack(() => looking++);
      api
        .projectFolders(n)
        .then((d) => {
          found.set(hk(n), d);
          tooOld.delete(hk(n));
          data = { ...data, [hk(n)]: d };
        })
        .catch((e: Error) => {
          if (!/not found/i.test(e.message) || !n) return;
          tooOld.add(n);
          if (untrack(() => target) === n) checkOld(n);
        })
        .finally(() => looking--);
    }
  });

  // The GitHub account's repositories, asked from the machines that are online.
  const ghHosts = () => {
    const names = hub.hosts.filter((h) => h.state === "online").map((h) => (h.local ? null : h.name));
    return names.includes(null) ? names : [null, ...names];
  };
  $effect(() => {
    if (open) untrack(() => void ghLoad(ghHosts()));
  });

  // One request per folder and machine while browsing; typing more only filters.
  const cache = new Map<string, FolderListing>();
  let asked = "";
  $effect(() => {
    if (!open || !browsing || cloneUrl) return;
    void again;
    const key = `${target ?? ""}|${hidden}|${dir}`;
    if (key === asked) return;
    asked = key;
    const hit = cache.get(key);
    if (hit) {
      listing = hit;
      error = null;
      return;
    }
    const timer = setTimeout(() => {
      busy = true;
      api
        .folders(dir, target, hidden || filter.startsWith("."))
        .then((l) => {
          cache.set(key, l);
          if (asked === key) {
            listing = l;
            error = null;
            old = null;
          }
        })
        .catch((e: Error) => {
          if (asked !== key) return;
          listing = null;
          if (/not found/i.test(e.message) && target) {
            error = null;
            checkOld(target);
          } else error = e.message;
        })
        .finally(() => (busy = false));
    }, 90);
    return () => clearTimeout(timer);
  });

  async function checkOld(name: string) {
    const [theirs, ours] = await Promise.all([quiet(api.version(name), "version", null), quiet(api.version(), "version", null)]);
    if (target !== name) return;
    old = { theirs: theirs?.version ?? hub.hosts.find((h) => h.name === name)?.version ?? null, ours: ours?.version ?? null, update: theirs };
  }



  /** "/home/pi/srv" as "~/srv" on that machine. */
  function pretty(path: string, on: string | null = target) {
    const h = data[hk(on)]?.home ?? (on === target ? home : null);
    if (!h) return path;
    return path === h ? "~" : path.startsWith(h) && /[\\/]/.test(path[h.length] ?? "") ? "~" + path.slice(h.length) : path;
  }



  const here = $derived(machine === hub.localHost || !machine ? "this machine" : machine);
  const entryOf = (p: ProjectFolder, h: string | null): Entry => ({
    path: p.path,
    name: p.name,
    where: pretty(p.path, h),
    host: h,
    machine: nameOf(h),
    repo: true,
    branch: p.branch,
    main: p.main ?? null,
    behind: p.behind ?? null,
    key: p.key,
    touched: p.touched,
    worktrees: p.worktrees ?? 0,
    worktreeOf: p.worktree_of ?? null,
  });

  /** Every repository found, on every machine, a linked worktree counting as its repository. */
  const repos = $derived.by(() => {
    const all = Object.entries(data).flatMap(([k, d]) => d.projects.map((p) => entryOf(p, k || null)));
    const at = new Set(all.map((e) => `${hk(e.host)}|${norm(e.path)}`));
    return all
      .filter((e) => !(e.worktreeOf && at.has(`${hk(e.host)}|${norm(e.worktreeOf)}`)))
      .sort((a, b) => b.touched - a.touched || a.name.localeCompare(b.name));
  });

  /** The folders threads ran in, newest first, each once: a repository found, or a plain folder. */
  const used = $derived.by(() => {
    const latest = new Map<string, { path: string; host: string | null; at: number }>();
    for (const t of hub.threads) {
      const cwd = threadProject(t);
      if (!cwd || isScratchRun(cwd) || isChatThread(t)) continue;
      const h = t.host ?? null;
      const k = `${hk(h)}|${norm(cwd)}`;
      const at = Math.max(t.sent_at ?? 0, t.created_at);
      if (!latest.has(k) || latest.get(k)!.at < at) latest.set(k, { path: cwd, host: h, at });
    }
    const byPath = new Map<string, Entry>();
    for (const e of Object.entries(data).flatMap(([k, d]) => d.projects.map((p) => entryOf(p, k || null)))) byPath.set(`${hk(e.host)}|${norm(e.path)}`, e);
    const out: Entry[] = [];
    const seen = new Set<string>();
    for (const [k, r] of [...latest].sort((a, b) => b[1].at - a[1].at)) {
      // A machine that is not online is not there to start a thread on.
      const info = r.host ? hub.hostInfo(r.host) : null;
      if (r.host && info?.state !== "online") continue;
      let e = byPath.get(k);
      // A worktree of a repository found counts as the repository.
      if (e?.worktreeOf) e = byPath.get(`${hk(e.host)}|${norm(e.worktreeOf)}`) ?? e;
      e ??= {
        path: r.path, name: baseName(r.path), where: pretty(r.path, r.host), host: r.host, machine: nameOf(r.host), repo: false,
        branch: null, main: null, behind: null, key: null, touched: 0, worktrees: 0, worktreeOf: null,
      };
      const id = `${hk(e.host)}|${norm(e.path)}`;
      if (seen.has(id)) continue;
      seen.add(id);
      out.push({ ...e, at: r.at });
    }
    return out;
  });
  const recents = $derived(used.slice(0, 8));
  const plain = $derived(used.filter((e) => !e.repo));
  const onMachine = (h: string | null) => [...repos.filter((e) => e.host === h), ...plain.filter((e) => e.host === h)];

  const counts = $derived({ recent: recents.length, repos: repos.length, folders: plain.length });
  /** The clones of each GitHub repository, on every machine, by the key of their origin. */
  const clones = $derived.by(() => {
    const m = new Map<string, Entry[]>();
    for (const e of repos) if (e.key) m.set(e.key.toLowerCase(), [...(m.get(e.key.toLowerCase()) ?? []), e]);
    return m;
  });
  const cloned = (r: GithubRepo) => clones.get(keyOf(r)) ?? [];
  /** The clone to open: on the thread's machine when there is one, else the most recently touched. */
  const bestClone = (es: Entry[]) => es.find((e) => e.host === target) ?? [...es].sort((a, b) => b.touched - a.touched)[0];
  const ghRows = $derived(gh.repos.filter((r) => !r.archived).map((r): Row => ({ kind: "github", r, local: cloned(r) })));
  const machineCount = (h: string) => onMachine(h === hub.localHost ? null : h).length;
  const viewMachine = $derived(typeof view === "object" ? view.machine : null);

  /** One list: the rows of the rail's pick, or what the search matches on every machine. */
  const list = $derived.by((): Row[] => {
    const as = (es: Entry[], section?: string): Row[] => es.map((e) => ({ kind: "project", e, section }));
    if (viewMachine) {
      const h = viewMachine === hub.localHost ? null : viewMachine;
      return [...as(repos.filter((e) => e.host === h), "Repositories"), ...as(plain.filter((e) => e.host === h), "Folders")];
    }
    if (view === "github") return ghRows;
    if (view === "repos") return as(repos);
    if (view === "folders") return as(plain);
    return as(recents);
  });

  const rows = $derived.by((): Row[] => {
    if (naming) {
      const name = query.trim();
      const root = projects?.root;
      return name && root && !/[<>:"|?*\\/]/.test(name) ? [{ kind: "make", name, parent: root }] : [];
    }
    if (cloneUrl) {
      const name = cloneUrl.replace(/\.git$/, "").split(/[/:]/).pop() || "repo";
      const root = projects?.root;
      return root ? [{ kind: "clone", url: cloneUrl, target: root.replace(/[\\/]+$/, "") + sep + name }] : [];
    }
    if (browsing) {
      if (!listing) return [];
      const q = filter;
      const all = listing.entries.map((e) => ({ kind: "dir" as const, path: e.path, name: e.name, repo: e.repo }));
      const out: Row[] = !q
        ? all
        : all
            .map((r) => ({ r, m: fuzzy(q, r.name) }))
            .filter((x) => x.m)
            .sort((a, b) => b.m!.score - a.m!.score)
            .map((x) => ({ ...x.r, hits: x.m!.hits }));
      const exact = listing.entries.some((e) => e.name.toLowerCase() === q.toLowerCase());
      if (q && !exact && !/[<>:"|?*]/.test(q)) out.push({ kind: "make", name: q, parent: listing.path });
      return out;
    }
    if (!filter) return list;
    return searchRows(filter, [...recents, ...repos, ...plain], gh.repos, clones, target, projects?.root ?? null);
  });
  $effect(() => {
    if (at >= rows.length) at = Math.max(0, rows.length - 1);
  });
  /** Which machines the rows are from, so the machine column only shows when it tells something. */
  const showMachine = $derived(hub.hosts.length > 1 && (!viewMachine || !!filter));
  const empty = $derived(
    !rows.length && panes && !filter
      ? view === "recent" ? "Nothing yet. Pick a project below, or start one." : "Nothing here."
      : "",
  );


  let scroller = $state<HTMLDivElement>();
  $effect(() => {
    void at;
    scroller?.querySelector(".row.on")?.scrollIntoView({ block: "nearest" });
  });

  async function show() {
    if (open) return;
    open = true;
    query = "";
    naming = false;
    error = null;
    at = 0;
    await tick();
    input?.focus();
  }

  export async function browse() {
    // Show what is inside the picked folder, or the projects folder.
    const from = value.trim() || projects?.root || "~";
    query = from.replace(/[\\/]+$/, "") + (from.includes("\\") ? "\\" : from === "~" ? "/" : sep);
    open = true;
    at = 0;
    await tick();
    input?.focus();
  }

  function into(path: string) {
    query = path.replace(/[\\/]+$/, "") + sep;
    at = 0;
    input?.focus();
  }
  /** Start in `path`: on the machine it was found on, when that is another one. */
  function pick(path: string, on: string | null = target) {
    if (on !== host) onhost?.(on);
    value = path;
    open = false;
    naming = false;
    query = "";
    input?.blur();
  }
  function up() {
    if (listing?.parent) into(listing.parent);
    else query = "";
  }
  function rail(next: typeof view) {
    view = next;
    query = "";
    at = 0;
    error = null;
    if (typeof next === "object") {
      const h = next.machine === hub.localHost ? null : next.machine;
      target = h;
      old = null;
      if (h && tooOld.has(h)) checkOld(h);
    }
    input?.focus();
  }

  async function make(parent: string, name: string) {
    busy = true;
    try {
      pick(await api.makeFolder(parent.replace(/[\\/]+$/, "") + sep + name, true, target));
      cache.clear();
      asked = "";
      found.delete(hk(target));
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
  async function cloneInto(url: string, to: string) {
    busy = true;
    error = null;
    try {
      pick(await api.cloneRepo(url, to, target));
      found.delete(hk(target));
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }

  /** A repository from another machine: carried to the machine the thread runs on, then picked. */
  async function bring(e: Entry) {
    if (working) return;
    const id = `${hk(e.host)}|${e.path}`;
    working = id;
    error = null;
    try {
      // Carried from the machine it is on (git bundle + .env files, `carry.rs`), so this machine
      // needs no GitHub login for a private repository.
      const root = (data[hk(host)] ?? (await api.projectFolders(host))).root;
      const to = root.includes("\\") ? "\\" : "/";
      const b = await api.bringProject({ from: e.host, path: e.path, to: host, target: root.replace(/[\\/]+$/, "") + to + e.name });
      pick(b.folder, host);
      found.delete(hk(host));
    } catch (err) {
      error = (err as Error).message;
    } finally {
      working = null;
    }
  }

  async function scratch() {
    busy = true;
    error = null;
    try {
      pick(await scratchFolder(target, api));
    } catch (e) {
      error = (e as Error).message;
    } finally {
      busy = false;
    }
  }
  function startNew() {
    naming = true;
    query = "";
    at = 0;
    input?.focus();
  }

  /** A repository from GitHub: open the clone there is, or clone it to the machine the thread runs on. */
  async function openGithub(r: GithubRepo, local: Entry[]) {
    if (working) return;
    if (local.length) {
      const e = bestClone(local);
      pick(e.path, e.host);
      return;
    }
    const root = projects?.root;
    if (!root) {
      error = "Looking for the projects folder on that machine…";
      return;
    }
    working = r.full;
    error = null;
    try {
      pick(await api.cloneRepo(cloneUrlOf(r), root.replace(/[\\/]+$/, "") + sep + r.name, target));
      found.delete(hk(target));
    } catch (e) {
      error = (e as Error).message;
    } finally {
      working = null;
    }
  }

  // ---- Connecting GitHub ----
  let connecting = $state(false);
  const machines = $derived(hub.hosts.filter((h) => h.state === "online").map((h) => ({ name: h.name, host: h.local ? null : h.name })));
  const stateOf = (host: string | null) => gh.machines[hk(host)]?.state ?? "unknown";
  const waitingOn = $derived(machines.find((m) => stateOf(m.host) === "waiting") ?? null);
  let started = $state<string | null | undefined>(undefined);
  async function startConnect(host: string | null) {
    connecting = true;
    started = host;
    await ghConnect(host);
  }
  /** The banner's button: sign in on this machine, the one the window is on and its browser. */
  const openConnect = () => startConnect(null);
  function run(row: Row) {
    if (row.kind === "github") openGithub(row.r, row.local);
    else if (row.kind === "project") pick(row.e.path, row.e.host);
    else if (row.kind === "dir" || row.kind === "path") pick(row.path);
    else if (row.kind === "make") make(row.parent, row.name);
    else if (row.kind === "clone") cloneInto(row.url, row.target);
  }

  function key(e: KeyboardEvent) {
    if (!open) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        show();
        e.preventDefault();
      }
      return;
    }
    if (connecting && e.key === "Escape") {
      connecting = false;
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    const row = rows[at];
    const caretAtEnd = input?.selectionStart === query.length;
    if (e.key === "ArrowDown") at = Math.min(at + 1, rows.length - 1);
    else if (e.key === "ArrowUp") at = Math.max(at - 1, 0);
    else if ((e.key === "Tab" && !e.shiftKey) || (e.key === "ArrowRight" && caretAtEnd && row)) {
      // Into the highlighted folder, to pick one inside it.
      if (row?.kind === "project" || row?.kind === "dir") {
        if (row.kind === "project" && row.e.host !== target) {
          target = row.e.host;
          tick().then(() => into(row.e.path));
        } else into(row.kind === "project" ? row.e.path : row.path);
      } else if (e.key === "Tab" && !row) return;
    } else if (e.key === "Backspace" && browsing && !filter && caretAtEnd && query.length) up();
    else if (e.key === "ArrowLeft" && e.altKey && browsing) up();
    else if (e.key === "Enter") {
      if (row && (filter || !browsing)) run(row);
      else if (browsing && listing && !filter) pick(listing.path);
      else if (browsing && query.trim()) pick(query.trim());
    } else if (e.key === "o" && (e.ctrlKey || e.metaKey) && system) {
      open = false;
      system();
    } else if (e.key === "Escape") {
      if (naming) {
        naming = false;
        query = "";
      } else if (query) query = "";
      else {
        open = false;
        input?.blur();
      }
    } else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<svelte:window onresize={() => open && measure()} onpointerdown={(e) => { if (open && !(e.target as Element).closest?.(".picker")) { open = false; query = ""; } }} />

<span class="picker" bind:this={picker}>
  {#if open}
    <input
      class="search"
      bind:this={input}
      value={query}
      placeholder={naming
        ? `Name of the new project in ${projects ? pretty(projects.root) : "the projects folder"}`
        : "Search projects on every machine, or type a path"}
      spellcheck="false"
      autocomplete="off"
      onclick={() => (open = true)}
      oninput={(e) => { query = e.currentTarget.value; open = true; at = 0; }}
      onkeydown={key}
    />
  {:else}
    <button class="field" class:empty={!value} data-tip={value || undefined} onclick={show} onkeydown={key}>
      {#if value}
        <ProjectIcon path={value} host={host} name={baseName(value)} size={15} />
        <span class="fname">{baseName(value)}</span>
        <span class="fpath">{pretty(value, host)}</span>
      {:else}
        <Icon name="folder" size={14} />
        <span class="fpath">{placeholder}</span>
      {/if}
      <span class="grow"></span>
      <Icon name="down" size={12} />
    </button>
  {/if}
  {#if open}
    <div class="pop" transition:pop|global class:down={!room.up} style:max-height="{room.h}px" role="listbox">
      <div class="body" class:flat={!panes}>
        {#if panes}
          <FolderRail {counts} {filter} {view} {viewMachine} {rail} {machineCount} {openConnect} />
        {/if}
        <div class="pane">
          <div class="head">
            {#if browsing && !cloneUrl}
              <button class="icon" data-tip="Up one folder (Backspace)" onclick={(e) => { e.preventDefault(); up(); }}>
                <Icon name="arrowUp" size={13} />
              </button>
              <span class="where" data-tip={listing?.path}>{listing ? pretty(listing.path) : dir}</span>
              <span class="on-host">on <HostChip name={machine} /></span>
            {:else if naming}
              <span class="where">New project</span><span class="on-host">on <HostChip name={machine} /></span>
            {:else if cloneUrl}
              <span class="where">Clone a repository</span><span class="on-host">to <HostChip name={machine} /></span>
            {:else if filter}
              <span class="where">Matches on every machine{gh.state === "ready" ? " and GitHub" : ""}</span>
            {:else if viewMachine}
              <span class="where">On {viewMachine}</span><span class="muted">repositories first</span>
            {:else if view === "github"}
              <span class="where">GitHub</span><span class="muted">{gh.login ?? ""} · cloned ones open where they are</span>
            {:else}
              <span class="where">{view === "recent" ? "Recent" : view === "repos" ? "Repositories" : "Folders"}</span>
              <span class="muted">{view === "recent" ? "where threads ran last" : view === "repos" ? "on every machine" : "that are not repositories"}</span>
            {/if}
            {#if busy || looking}<span class="muted">…</span>{/if}
            <span class="grow"></span>
            {#if browsing && !cloneUrl}
              <button class="use" class:set={hidden} data-tip="Also show hidden, system and build folders" onclick={(e) => { e.preventDefault(); hidden = !hidden; }}>
                <Icon name="eye" size={12} /> Hidden
              </button>
              {#if listing && !filter}
                <button class="use" data-tip="Run the thread in this folder (Enter)" onclick={(e) => { e.preventDefault(); pick(listing!.path); }}>
                  <Icon name="check" size={12} /> Use this folder
                </button>
              {/if}
            {/if}
          </div>
          {#if error && !old}<p class="note err">{error}</p>{/if}
          <div class="rows" bind:this={scroller}>
            <FolderRows {rows} bind:at {host} {filter} {showMachine} {here} {machine} {working} {pick} {into} {bring} {openGithub} {run} {pretty} {bestClone} {nameOf} />
            {#if panes && !old && (gh.state === "none" || gh.state === "old") && (filter || view === "repos" || view === "recent")}
              <div class="ghnote">
                <ProviderLogo id="github" size={16} />
                <div class="grow">
                  <div class="t">Only clones on your machines are listed</div>
                  <div class="muted">{gh.state === "old" ? "GitHub repos arrive with the next version of Hub." : "Connect GitHub to find every repo, also those not cloned yet, and clone them from here."}</div>
                </div>
                {#if gh.state === "none"}<button class="use go-on" onclick={(e) => { e.preventDefault(); openConnect(); }}>Connect GitHub</button>{/if}
              </div>
            {/if}
            {#if old}
              <FolderOld bind:old {target} {open} onclose={() => (open = false)} onnewer={() => { asked = ""; cache.clear(); again++; }} />
            {:else if cloneUrl && !projects}
              <Quiet kind="loading" />
            {:else if !rows.length && browsing && listing}
              <p class="note">No folders in here{hidden ? "" : " (hidden ones left out)"}.</p>
            {:else if !rows.length && panes && looking && !filter}
              <Quiet kind="loading" />
            {:else if !rows.length && panes && filter}
              <p class="note">Nothing matches "{filter}".</p>
            {:else if empty}
              <p class="note">{empty}</p>
            {:else if naming && !rows.length}
              <p class="note">Type a name; Enter makes the folder with git in {projects ? pretty(projects.root) : "the projects folder"}.</p>
            {/if}
            {#if browsing && listing?.more && !filter}<p class="note">More folders than shown; type to narrow.</p>{/if}
          </div>
        </div>
      </div>
      <div class="ways">
        <button class="way" class:on={naming} data-tip="A new folder with git in {projects ? pretty(projects.root) : 'the projects folder'} on {machine}; you name it next" onclick={(e) => { e.preventDefault(); startNew(); }}>
          <Icon name="plus" size={13} /> New project
        </button>
        <button class="way" data-tip="For a question or a one-off: the thread gets a fresh empty folder in {projects ? pretty(projects.root) : 'the projects folder'}/scratch on {machine}" onclick={(e) => { e.preventDefault(); scratch(); }}>
          <Icon name="spark" size={13} /> No project
        </button>
        <button class="way" class:on={browsing} data-tip="Pick any folder on {machine} by hand" onclick={(e) => { e.preventDefault(); browse(); }}>
          <Icon name="folder" size={13} /> Browse {here === "this machine" ? "" : `on ${here}`}…
        </button>
        <span class="grow"></span>
        {#if system}
          <button class="way" data-tip="Pick with Windows' own folder dialog (Ctrl+O)" onclick={(e) => { e.preventDefault(); open = false; system(); }}>System dialog…</button>
        {/if}
      </div>
      <div class="foot">
        <span><kbd>↑↓</kbd> move</span><span><kbd>Enter</kbd> pick</span><span><kbd>Tab</kbd> look inside</span>
        {#if naming}<span><kbd>Esc</kbd> back</span>{:else}
          {#if cloneUrl}{:else if browsing}<span><kbd>⌫</kbd> up</span>{:else}<span>type {#if sep === "\\"}<kbd>D:\</kbd> or {/if}<kbd>~/</kbd> for a path</span>{/if}
          <span><kbd>Esc</kbd> {query ? "clear" : "close"}</span>
        {/if}
      </div>
      {#if connecting}<FolderConnect {machines} {waitingOn} {stateOf} {started} {startConnect} {ghHosts} onclose={() => (connecting = false)} />{/if}
    </div>
  {/if}
</span>

<style>
  .picker { position: relative; flex: 1; min-width: 0; display: flex; }
  input { flex: 1; min-width: 0; background: none; border: 0; outline: none; font-family: var(--mono); font-size: 13px; color: var(--text-3); }
  input::placeholder { color: var(--muted); }
  input.search { border: 1px solid var(--accent); border-radius: var(--r-md); padding: 3px 8px; background: var(--surface-hover); }
  .field {
    display: flex; align-items: center; gap: 8px; flex: 1; min-width: 0; padding: 3px 8px; border-radius: var(--r-md);
    border: 1px solid var(--line-strong); background: var(--surface-hover); color: var(--text-2); text-align: left;
  }
  .field:hover { border-color: var(--muted); color: var(--text); }
  .field :global(svg) { color: var(--muted); flex: none; }
  .fname { font-size: 13px; color: var(--text); flex: none; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .fpath { font-family: var(--mono); font-size: 12px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
  .field.empty .fpath { font-family: var(--font); font-size: 13px; }
  .pop {
    position: absolute; z-index: 30; left: -24px; width: min(740px, calc(100vw - 64px)); bottom: calc(100% + 12px);
    display: flex; flex-direction: column;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    padding: 4px; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45); font-family: var(--font);
  }
  .pop.down { bottom: auto; top: calc(100% + 12px); }
  .body { display: grid; grid-template-columns: 176px minmax(0, 1fr); grid-template-rows: minmax(0, 1fr); gap: 4px; flex: 1 1 auto; min-height: 0; }
  .ways, .foot { flex: none; }
  .body.flat { grid-template-columns: minmax(0, 1fr); }
  .pane { min-width: 0; min-height: 0; display: flex; flex-direction: column; }
  .head { flex: none; display: flex; align-items: center; gap: 6px; padding: 4px 6px 6px; min-height: 30px; }
  .where { font-size: 13px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .on-host { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; flex: none; }
  .icon { display: grid; place-items: center; width: 22px; height: 22px; border-radius: var(--r-sm); color: var(--muted); }
  .icon:hover { background: var(--surface-hover); color: var(--text); }
  .use { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-3); padding: 2px 8px; border-radius: var(--r-sm); background: var(--surface-hover); flex: none; }
  .use:hover, .use.set { color: var(--text); }
  .rows { max-height: 300px; flex: 1 1 auto; min-height: 0; overflow-y: auto; padding: 0 2px; }
  .ways { display: flex; align-items: center; gap: 6px; padding: 6px 4px 4px; margin-top: 3px; border-top: 1px solid var(--line); }
  .way { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: var(--text-2); padding: 4px 10px; border-radius: var(--r-md); border: 1px solid var(--line-strong); }
  .way :global(svg) { color: var(--muted); }
  .way:hover, .way.on { background: var(--surface-hover); color: var(--text); }
  .way.on { border-color: var(--accent); }
  .foot { display: flex; flex-wrap: wrap; gap: 4px 12px; padding: 4px 8px 2px; color: var(--muted); font-size: 12px; }
  kbd { font-family: var(--mono); font-size: 11px; color: var(--text-3); }
  .go-on { color: var(--accent); background: var(--accent-bg); border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent); padding: 4px 10px; }
  .grow { flex: 1; }
  .muted { color: var(--muted); font-size: 12px; }
  .note { color: var(--muted); font-size: 13px; margin: 6px 8px; }
  .note.err { color: var(--red); }
  .ghnote { display: flex; align-items: center; gap: 12px; margin: 10px 6px 4px; padding: 10px 12px; border: 1px solid var(--line-strong); border-radius: var(--r-md); background: var(--surface); }
  .ghnote .t { color: var(--text); font-size: 13px; }
  .ghnote .muted { margin-top: 2px; }
</style>
