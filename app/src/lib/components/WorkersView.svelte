<!-- Everything Hub has running in the background: each thread's engine and what it started, processes left behind by finished threads, containers, and Hub's own helpers.
     Used by App.svelte. -->
<script lang="ts">
  import Page from "./Page.svelte";
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  import Ref from "./Ref.svelte";
  import ThreadMeta from "./ThreadMeta.svelte";
  // Everything Hub has running in the background: each thread's engine and what it started,
  // processes left behind by finished threads, containers, and Hub's own helpers.
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { Thread } from "../bindings/Thread";
  import type { ThreadWork } from "../bindings/ThreadWork";
  import type { WorkerProcess } from "../bindings/WorkerProcess";
  import type { Workers } from "../bindings/Workers";
  import { ago, bytes, folderName, shortVersion } from "../format";
  import { menu } from "../menu.svelte";
  import { processMenu, threadMenu } from "../menus.svelte";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import OwnTools from "./OwnTools.svelte";

  let workers = $state<Workers | null>(null);
  let failed = $state<string | null>(null);
  let open = $state<Record<string, boolean>>({});
  let busy = $state<string | null>(null);
  /** A "stop everything" waiting for its second click. */
  let armed = $state<string | null>(null);

  async function load() {
    try {
      workers = await api.workers();
      failed = null;
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    }
  }

  onMount(() => {
    load();
    const timer = poll(load, 3000);
    return () => timer();
  });

  type Row = { work: ThreadWork; thread: Thread | undefined };
  const rows = $derived(
    (workers?.threads ?? []).map((work) => ({ work, thread: hub.threads.find((t) => t.id === work.thread_id) })),
  );
  // Threads that are gone or archived but still have processes running.
  const gone = (r: Row) => !r.thread || r.thread.archived;
  const order = (r: Row) => (r.thread?.status === "running" ? 0 : r.thread?.status === "waiting_on_you" ? 1 : r.work.engine_running ? 2 : 3);
  const active = $derived(
    rows.filter((r) => !gone(r) && (r.work.engine_running || r.work.processes.length)).sort((a, b) => order(a) - order(b) || b.work.cpu_percent - a.work.cpu_percent),
  );
  const behind = $derived(rows.filter((r) => gone(r) && r.work.processes.length));
  const all = $derived([...rows.flatMap((r) => r.work.processes), ...(workers?.hub ?? [])]);
  const cpu = $derived(all.reduce((n, p) => n + p.cpu_percent, 0));
  const mem = $derived(all.reduce((n, p) => n + p.mem, 0));
  const engines = $derived(rows.filter((r) => r.work.engine_running).length);
  const started = (w: ThreadWork) => w.processes.filter((p) => p.pid !== w.engine);
  const servers = (w: ThreadWork) => w.processes.flatMap((p) => p.ports);

  function engineState(r: Row): string {
    const t = r.thread;
    if (r.work.engine_running && t?.status === "running") return "Working";
    if (t?.status === "waiting_on_you") return "Waiting on you";
    if (r.work.engine_running) return "Engine idle";
    if (t?.status === "error") return "Engine failed";
    return "Engine stopped";
  }

  function describe(r: Row, processes: number): string {
    const t = r.thread;
    const parts = t ? [t.archived ? "Archived" : engineState(r), ago(t.updated_at)] : [engineState(r)];
    if (processes) parts.push(`${processes} ${processes === 1 ? "process" : "processes"}`);
    return parts.join(" · ");
  }

  async function run(key: string, f: () => Promise<unknown>) {
    busy = key;
    armed = null;
    try {
      await f();
      await load();
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = null;
    }
  }

  function stopEverything(id: string) {
    if (armed !== id) {
      armed = id;
      setTimeout(() => armed === id && (armed = null), 4000);
      return;
    }
    run(id, () => api.shutdown(id, true));
  }

  const pct = (n: number) => (n >= 10 ? `${Math.round(n)}%` : `${n.toFixed(1)}%`);
</script>

{#snippet usage(cpuPct: number, memBytes: number)}
  <span class="use mono"><span data-tip="Share of the whole machine">{pct(cpuPct)}</span><span>{bytes(memBytes)}</span></span>
{/snippet}

{#snippet tree(list: WorkerProcess[], opts: { thread?: string; engine?: number | null; hubPid?: number })}
  {#each list as p (p.pid)}
    {@const isEngine = p.pid === opts.engine}
    {@const isHub = p.pid === opts.hubPid}
    <div class="proc" class:paused={p.paused} style:padding-left="{12 + p.depth * 16}px" data-tip={p.command}
      use:menu={() => processMenu(p, { thread: opts.thread, engine: isEngine, hub: isHub, done: load })}>
      <span class="name"><Ref kind="process" label={p.name} /></span>
      {#if isEngine}<span class="tag">engine</span>{/if}
      {#if isHub}<span class="tag">core</span>{/if}
      {#if p.paused}<span class="tag paused">paused</span>{/if}
      {#each p.ports as port (port)}
        <button class="port mono" data-tip="Open in browser" onclick={() => openUrl(`http://localhost:${port}`).catch((e) => hub.fail(e))}><Ref kind="port" label=":{port}" /></button>
      {/each}
      <span class="cmd mono">{p.command}</span>
      <span class="age">{ago(p.started_at)}</span>
      {@render usage(p.cpu_percent, p.mem)}
      <span class="acts">
        {#if !isEngine && !isHub}
          {#if p.paused}
            <button class="btn" disabled={busy === `p${p.pid}`} onclick={() => run(`p${p.pid}`, () => api.resumeProcess(p.pid))}>Resume</button>
          {:else}
            <button class="btn" disabled={busy === `p${p.pid}`} data-tip="Pause it and what it started" onclick={() => run(`p${p.pid}`, () => api.pauseProcess(p.pid))}>Pause</button>
          {/if}
          <button class="btn danger" disabled={busy === `p${p.pid}`} data-tip="Stop it and what it started" onclick={() => run(`p${p.pid}`, () => api.stopProcess(p.pid))}>Stop</button>
        {/if}
      </span>
    </div>
  {/each}
{/snippet}

{#snippet threadRow(r: Row)}
  {@const w = r.work}
  {@const t = r.thread}
  {@const extra = started(w)}
  {@const ports = servers(w)}
  <div class="work" class:open={open[w.thread_id]}>
    <div class="head" use:menu={() => (t ? threadMenu(t) : [])}>
      <button class="chev" class:open={open[w.thread_id]} data-tip="Show processes" onclick={() => (open[w.thread_id] = !open[w.thread_id])}>
        <Icon name="chevron" size={13} />
      </button>
      <span class="dot {t && w.engine_running ? t.status : 'idle'}"></span>
      <span class="title">
        {#if t && !t.archived}
          <button class="link" onclick={() => hub.open(t.id)}>{t.title}</button>
        {:else}
          <span class="gone">{t ? t.title : "Deleted thread"}</span>
        {/if}
        <span class="sub">{#if t}<ThreadMeta {t} engine when={false} extra={describe(r, extra.length)} />{:else}{describe(r, extra.length)}{/if}</span>
      </span>
      {#each ports.slice(0, 3) as port (port)}
        <button class="port mono" data-tip="Open in browser" onclick={() => openUrl(`http://localhost:${port}`).catch((e) => hub.fail(e))}><Ref kind="port" label=":{port}" /></button>
      {/each}
      {@render usage(w.cpu_percent, w.mem)}
      <span class="acts always">
        {#if t?.status === "running" && w.engine_running}
          <button class="btn" disabled={busy === w.thread_id} onclick={() => run(w.thread_id, () => api.cancel(w.thread_id))}>
            <Icon name="stop" size={13} />Stop turn
          </button>
        {/if}
        {#if w.engine_running}
          <button class="btn" disabled={busy === w.thread_id} data-tip="Frees its memory; the next message starts it again" onclick={() => run(w.thread_id, () => api.shutdown(w.thread_id, false))}>Shut down</button>
        {/if}
        {#if extra.length}
          <button class="btn danger" class:armed={armed === w.thread_id} disabled={busy === w.thread_id} data-tip="The engine and every process the thread started" onclick={() => stopEverything(w.thread_id)}>
            {armed === w.thread_id ? `Stop ${w.processes.length}?` : "Stop all"}
          </button>
        {/if}
      </span>
    </div>
    {#if open[w.thread_id]}
      <div class="procs">
        {#if w.processes.length}
          {@render tree(w.processes, { thread: w.thread_id, engine: w.engine })}
        {:else}
          <p class="label none">Process not found.</p>
        {/if}
      </div>
    {/if}
  </div>
{/snippet}

<Page title="Workers">
  {#snippet head()}
    <span class="host" data-tip="Where these run"><Ref kind="monitor" label={workers?.host ?? hub.host?.hostname ?? "This computer"} online={hub.connected} /></span>
  {/snippet}
  {#if workers}
    <p class="summary">
      {engines} {engines === 1 ? "engine" : "engines"} running{#if hub.running.length}, {hub.running.length} working{/if}
      · {rows.reduce((n, r) => n + started(r.work).length, 0)} processes started by threads
      {#if workers.containers.length}· {workers.containers.length} {workers.containers.length === 1 ? "container" : "containers"}{/if}
      <span class="total">{@render usage(cpu, mem)}</span>
    </p>
  {/if}
  {#if failed && !workers}
    <p class="label">Could not read what is running: {failed}</p>
  {:else if !workers}
    <Quiet kind="loading" />
  {:else}
    <h4>Threads <span class="label">{active.length}</span></h4>
    {#if active.length}
      {#each active as r (r.work.thread_id)}{@render threadRow(r)}{/each}
    {:else}
      <p class="label">No engine running.</p>
    {/if}

    <OwnTools />

    {#if behind.length}
      <h4>Left behind <span class="label">by archived and deleted threads</span></h4>
      {#each behind as r (r.work.thread_id)}{@render threadRow(r)}{/each}
    {/if}

    {#if workers.containers.length}
      <h4>Containers <span class="label">{workers.containers.length}</span></h4>
      {#each workers.containers as c (c.id)}
        <div class="row" data-tip={c.image}>
          <span class="name">{c.name}</span>
          <span class="cmd mono">{c.image}</span>
          {#if c.project}<button class="link sub" onclick={() => hub.openProject(c.project!)}>{folderName(c.project)}</button>{/if}
          <span class="sub">{c.status}</span>
        </div>
      {/each}
    {/if}

    <h4>Hub <span class="label">the core and its helpers</span></h4>
    {#if hub.update && hub.update.state !== "idle" && hub.update.state !== "disabled"}
      <p class="say">Update {hub.update.state}{#if hub.update.target} to {shortVersion(hub.update.target)}{/if}{#if hub.update.message}: {hub.update.message}{/if}</p>
    {/if}
    <div class="procs flat">{@render tree(workers.hub, { hubPid: workers.hub[0]?.pid })}</div>
  {/if}
</Page>

<style>
  .host { display: inline-flex; align-items: center; color: var(--text-2); font-size: 13px; }
  .summary { color: var(--text-3); font-size: 13px; margin: 6px 0 0; display: flex; gap: 6px; flex-wrap: wrap; align-items: baseline; }
  .total { margin-left: auto; }
  h4 { color: var(--text); font-size: 13px; font-weight: 500; margin: 26px 0 8px; display: flex; gap: 8px; align-items: baseline; }
  .work { border-bottom: 1px solid var(--line); }
  .head { display: flex; align-items: center; gap: 10px; min-height: var(--row-min); padding: var(--row-pad) 0; }
  .chev { display: grid; color: var(--muted); padding: 2px; border-radius: var(--r-sm); transition: transform 0.12s var(--ease); }
  .chev:hover { color: var(--text); background: var(--surface-hover); }
  .chev.open { transform: rotate(90deg); }
  .title { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: var(--row-gap); }
  .title .link, .gone { color: var(--text); font-size: 14px; text-align: left; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .title .link:hover { text-decoration: underline; text-underline-offset: 3px; }
  .gone { color: var(--text-3); }
  .sub { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  button.sub:hover { color: var(--text-2); }
  .use { display: inline-flex; gap: 10px; color: var(--text-3); font-size: 12px; white-space: nowrap; }
  .use span { min-width: 44px; text-align: right; }
  .acts { display: inline-flex; justify-content: flex-end; gap: 6px; min-width: 104px; visibility: hidden; }
  .head .acts { min-width: 236px; }
  .acts.always, .proc:hover .acts { visibility: visible; }
  .btn { height: 24px; padding: 0 8px; font-size: 12px; }
  .btn.armed { background: var(--red-bg); color: var(--red); }
  .port { color: var(--text); background: none; padding: 1px 4px; border-radius: var(--r-sm); font-size: 12px; }
  .port:hover { background: var(--surface-hover); }
  .procs { padding: 0 0 8px 22px; }
  .procs.flat { padding-left: 0; }
  .proc, .row { display: flex; align-items: center; gap: 8px; min-height: 30px; padding-right: 0; border-radius: var(--r-sm); }
  .proc:hover, .row:hover { background: var(--surface); }
  .row { padding: 0 12px; }
  .proc.paused .name, .proc.paused .cmd { opacity: 0.6; }
  .name { color: var(--text-2); font-size: 13px; white-space: nowrap; }
  .cmd { flex: 1; min-width: 0; color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .age { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .tag { color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 0 6px; font-size: 11px; }
  .tag.paused { color: var(--amber); background: var(--amber-bg); }
  .say { color: var(--text-3); font-size: 13px; margin: 0 0 6px; }
  .none { margin: 4px 12px 8px; }
  p.label { margin: 4px 0; }
</style>
