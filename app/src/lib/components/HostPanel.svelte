<!-- Everything listening on this computer. Threads show their own servers in their panel.
     Used by App.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  import { api } from "../api";
  import type { Listener } from "../bindings/Listener";
  import { ago, bytes, clock, folderName } from "../format";
  import { copy, menu } from "../menu.svelte";
  import { reveal, serverMenu } from "../menus.svelte";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";

  // Everything listening on this computer. Threads show their own servers in their panel.
  let { full = false }: { full?: boolean } = $props();

  // Things a developer starts. Everything else (apps, services, the OS) is folded away.
  const DEV = new Set([
    "node", "bun", "deno", "python", "python3", "pythonw", "py", "uvicorn", "gunicorn", "flask",
    "cargo", "rustc", "java", "javaw", "dotnet", "go", "air", "ruby", "rails", "php", "nginx",
    "caddy", "httpd", "postgres", "redis-server", "mysqld", "mongod", "docker", "com.docker.backend",
    "wslrelay", "vite", "esbuild", "godot", "hubd",
  ]);
  const base = (name: string | null) => (name ?? "").toLowerCase().replace(/\.exe$/, "");
  const isDev = (l: Listener) => !l.system && (DEV.has(base(l.process)) || l.origin !== "other");

  const host = $derived(hub.host);
  const listeners = $derived(host?.listeners ?? []);
  const ofThreads = $derived(listeners.filter((l) => l.origin === "thread"));
  const dev = $derived(listeners.filter((l) => l.origin !== "thread" && isDev(l)));
  const apps = $derived(listeners.filter((l) => l.origin !== "thread" && !isDev(l) && !l.system));
  const system = $derived(listeners.filter((l) => l.system));
  const memPct = $derived(host ? Math.round((host.mem_used / Math.max(host.mem_total, 1)) * 100) : 0);
  const threadOf = (id: string | null) => (id ? hub.threads.find((t) => t.id === id) : undefined);
  const threadTitle = (id: string | null) => threadOf(id)?.title ?? "a deleted thread";

  // One row at a time opens to show what started it and whether stopping it is safe.
  const key = (l: Listener) => `${l.port}-${l.pid}`;
  let open = $state<string | null>(null);

  /** Since when, as a time today or a date before that. */
  const since = (ms: number) =>
    new Date(ms).toDateString() === new Date().toDateString()
      ? clock(ms)
      : new Date(ms).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

  /** What the row's thread is doing, in a few words. */
  function threadState(id: string | null): string {
    const t = threadOf(id);
    if (!t) return "deleted";
    if (t.archived) return "archived";
    if (t.status === "running") return "working right now";
    if (t.status === "waiting_on_you") return "waiting on you";
    if (t.status === "in_terminal") return "taken over in a terminal";
    if (t.status === "error") return "stopped with an error";
    const when = ago(t.updated_at);
    return `idle, last active ${when === "now" ? "just now" : `${when} ago`}`;
  }

  /** Whether stopping it is safe, and why. `tone` colours it: ok, careful or neutral. */
  function verdict(l: Listener): { tone: "ok" | "careful" | "neutral"; text: string } {
    if (l.is_hub) return { tone: "careful", text: "This is Hub itself." };
    if (l.system) return { tone: "careful", text: "Part of the operating system. Leave it running." };
    const t = threadOf(l.thread_id);
    switch (l.origin) {
      case "thread":
        if (!t) return { tone: "ok", text: "Its thread is deleted, so nothing in Hub uses it. Safe to stop." };
        if (t.archived) return { tone: "ok", text: "Its thread is archived. Safe to stop." };
        if (t.status === "running" || t.status === "waiting_on_you" || t.status === "in_terminal")
          return { tone: "careful", text: "Its thread is busy and may be using it. Stopping it could break the turn." };
        return { tone: "ok", text: "Its thread is not working on anything. Stopping only ends this server, and the thread can start it again." };
      case "engine":
        return { tone: "careful", text: `The engine of ${t ? `"${t.title}"` : "a thread"} listens here for itself. Stopping it ends the thread's engine; stop its turn instead.` };
      case "dev_mode":
        return { tone: "careful", text: "Dev mode: the live UI or the dev core for working on Hub. Stopping it breaks the dev window until Labs, Work on Hub starts it again." };
      case "hub":
        return { tone: "neutral", text: "A helper Hub started outside any thread. Hub may start it again when it needs it." };
      case "folder":
        return { tone: "neutral", text: `Hub did not start this. It runs in the folder of ${t ? `"${t.title}"` : "a thread"}, so you or a tool there probably did. Check the command before stopping it.` };
      default:
        return { tone: "neutral", text: "Not started by Hub. Check the command and folder before stopping it." };
    }
  }

  /** Who started it, for the detail. */
  function startedBy(l: Listener): string {
    switch (l.origin) {
      case "thread": return "Its thread. Hub tags what a thread starts, so this is certain";
      case "engine": return "Hub, as the thread's engine";
      case "dev_mode": return "Hub, for its live UI";
      case "hub": return "Hub";
      case "folder": return "Not Hub. The thread is only guessed from its folder";
      default: return l.parent ?? "Unknown";
    }
  }

  /** The line under the process name. */
  function sub(l: Listener): string | null {
    switch (l.origin) {
      case "thread": return threadTitle(l.thread_id);
      case "engine": return `Engine of ${threadTitle(l.thread_id)}`;
      case "dev_mode": return "Hub's live UI";
      case "hub": return "Started by Hub";
      case "folder": return `In the folder of ${threadTitle(l.thread_id)}`;
      default: return l.cwd ? folderName(l.cwd) : null;
    }
  }

  let showApps = $state(false);
  let showSystem = $state(false);

  let stopping = $state<number | null>(null);
  async function stop(l: Listener) {
    if (l.pid == null) return;
    stopping = l.pid;
    try {
      await api.stopProcess(l.pid);
      await hub.refreshHost();
    } catch (e) {
      hub.fail(e);
    } finally {
      stopping = null;
    }
  }
</script>

{#snippet rows(list: Listener[])}
  {#each list as l (key(l))}
    {@const shown = open === key(l)}
    <div class="listener" class:shown data-tip={l.exe ?? ""} use:menu={() => serverMenu(l)}>
      <button class="row" aria-expanded={shown} onclick={() => (open = shown ? null : key(l))}>
        <span class="port mono">:{l.port}</span>
        <span class="what">
          <span class="proc">{l.process ?? "unknown"}{#if l.pid != null}<span class="pid"> · pid {l.pid}</span>{/if}</span>
          {#if sub(l)}<span class="sub" class:mono={l.origin === "other"} data-tip={l.cwd ?? ""}>{sub(l)}</span>{/if}
        </span>
      </button>
      {#if l.pid != null && !l.is_hub && !l.system}
        <button class="btn danger" disabled={stopping === l.pid} onclick={() => stop(l)}>Stop</button>
      {/if}
    </div>
    {#if shown}{@render detail(l)}{/if}
  {/each}
{/snippet}

{#snippet detail(l: Listener)}
  {@const v = verdict(l)}
  {@const t = threadOf(l.thread_id)}
  <div class="detail">
    <p class="verdict {v.tone}">{v.text}</p>
    <dl>
      <dt>Started by</dt>
      <dd>{startedBy(l)}</dd>
      {#if l.thread_id && (l.origin === "thread" || l.origin === "folder" || l.origin === "engine")}
        <dt>Thread</dt>
        <dd>{#if t}<button class="link" onclick={() => hub.open(t.id)}>{t.title}</button> · {/if}{threadState(l.thread_id)}</dd>
      {/if}
      {#if l.started_at}
        <dt>Running</dt>
        <dd>since {since(l.started_at)}{#if ago(l.started_at) !== "now"} · {ago(l.started_at)}{/if}</dd>
      {/if}
      {#if l.command}<dt>Command</dt><dd class="mono wrap">{l.command}</dd>{/if}
      {#if l.cwd}<dt>Folder</dt><dd class="mono wrap">{l.cwd}</dd>{/if}
      {#if l.parent}<dt>Parent</dt><dd>{l.parent}</dd>{/if}
      <dt>Address</dt>
      <dd class="mono">{l.address}:{l.port}</dd>
    </dl>
    <div class="actions">
      {#if t}<button class="btn" onclick={() => hub.open(t.id)}>Open thread</button>{/if}
      {#if l.origin === "dev_mode"}<button class="btn" onclick={() => { hub.view = "labs"; hub.selected = null; }}>Open Labs</button>{/if}
      <button class="btn" onclick={() => openUrl(`http://localhost:${l.port}`).catch((e) => hub.fail(e))}>Open in browser</button>
      {#if l.cwd}<button class="btn" onclick={reveal(l.cwd)}>Show folder</button>{/if}
      {#if l.command}<button class="btn" onclick={() => copy(l.command!)}>Copy command</button>{/if}
    </div>
  </div>
{/snippet}

<div class="panel" class:full>
  {#if !host}
    <Quiet kind="loading" />
  {:else}
    <h3>{host.hostname} <span class="label">{host.os}</span></h3>
    <div class="meters">
      <span class="label">CPU</span><span class="bar"><span style:width="{Math.round(host.cpu_percent)}%"></span></span><span class="v">{Math.round(host.cpu_percent)}%</span>
      <span class="label">RAM</span><span class="bar"><span style:width="{memPct}%"></span></span><span class="v">{bytes(host.mem_used)}</span>
    </div>

    <!-- Full page: what threads and you started on the left, the rest of the machine beside it. -->
    <div class:tiles={full} style:--tile="440px">
      <div>
        <h4>Started by threads <span class="label">{ofThreads.length}</span></h4>
        {#if ofThreads.length}{@render rows(ofThreads)}{:else}<p class="label">None.</p>{/if}
        <h4>Dev servers <span class="label">{dev.length}</span></h4>
        {#if dev.length}{@render rows(dev)}{:else}<p class="label">None running.</p>{/if}
      </div>
      <div>
        {#if full}<h4>On this machine</h4>{/if}
        <button class="fold" onclick={() => (showApps = !showApps)}>
          {showApps ? "Hide" : "Show"} apps <span class="label">{apps.length}</span>
        </button>
        {#if showApps}{@render rows(apps)}{/if}
        <button class="fold" onclick={() => (showSystem = !showSystem)}>
          {showSystem ? "Hide" : "Show"} system <span class="label">{system.length}</span>
        </button>
        {#if showSystem}{@render rows(system)}{/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .panel { padding: 16px 18px; overflow-y: auto; min-height: 0; flex: 1; }
  .panel.full { width: 100%; padding: var(--page-top) var(--gutter); }
  .full .tiles { gap: 0 40px; }
  .full .meters { max-width: 640px; }
  h3 { color: var(--text); font-size: 14px; font-weight: 500; margin: 0 0 12px; display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; }
  h4 { color: var(--text); font-size: 13px; font-weight: 500; margin: 22px 0 8px; display: flex; gap: 8px; align-items: baseline; }
  .fold { display: flex; gap: 8px; align-items: baseline; color: var(--text-3); font-size: 13px; margin: 18px 0 6px; }
  .fold:hover { color: var(--text); }
  .meters { display: grid; grid-template-columns: auto 1fr auto; gap: 8px 10px; align-items: center; font-size: 12px; }
  .bar { height: 4px; border-radius: 3px; background: var(--surface-hover); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--text-3); border-radius: 3px; }
  .v { color: var(--text-3); font-size: 12px; text-align: right; min-width: 52px; }
  .listener { display: flex; align-items: center; gap: 10px; padding: 6px 0; border-bottom: 1px solid var(--line); }
  .listener:last-child { border-bottom: 0; }
  .listener.shown { border-bottom-color: transparent; }
  .row { flex: 1; min-width: 0; display: flex; align-items: center; gap: 10px; text-align: left; border-radius: var(--r-sm); }
  .row:hover .proc { color: var(--text); }
  .detail { margin: 0 0 6px 68px; padding: 2px 0 12px; border-bottom: 1px solid var(--line); font-size: 13px; }
  .verdict { margin: 0 0 10px; color: var(--text-2); line-height: 1.45; }
  .verdict::before { content: ""; display: inline-block; width: 6px; height: 6px; border-radius: 50%; margin-right: 8px; vertical-align: 1px; background: var(--muted); }
  .verdict.ok::before { background: var(--green); }
  .verdict.careful::before { background: var(--amber); }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 4px 14px; margin: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; color: var(--text-2); min-width: 0; }
  dd.mono { font-size: 12px; }
  .wrap { overflow-wrap: anywhere; }
  .link { color: var(--text); text-decoration: underline; text-decoration-color: var(--line-strong); text-underline-offset: 2px; }
  .link:hover { text-decoration-color: var(--text-3); }
  .actions { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 12px; }
  .port { color: var(--text); background: var(--surface); padding: 1px 6px; border-radius: var(--r-sm); font-size: 12px; min-width: 58px; text-align: center; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .proc { color: var(--text-2); font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pid { color: var(--muted); font-size: 12px; }
  .sub { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .btn { height: 24px; padding: 0 8px; font-size: 12px; }
  p.label { margin: 4px 0; }
</style>
