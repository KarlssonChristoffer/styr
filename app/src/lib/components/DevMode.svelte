<!-- Dev mode: work on Hub from inside Hub. A thread in the "Hub (dev)" project changes the clone, and the dev window shows the change as it is saved. Ship opens a pull request; Hub's merge queue merges it and the updater brings it here. The dev core lets core changes be tried too. The preview makes the clone follow the threads' open pull requests instead, so their work shows here as soon as it is pushed, long before it is released. Threads started in Hub (dev) each work in a worktree of their own, and the dev window can show any one of them live.
     Used by Labs.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  // Dev mode: work on Hub from inside Hub. A thread in the "Hub (dev)" project changes the clone,
  // and the dev window shows the change as it is saved. Ship opens a pull request; Hub's merge
  // queue merges it and the updater brings it here. The dev core lets core changes be tried too.
  // The preview makes the clone follow the threads' open pull requests instead, so their work
  // shows here as soon as it is pushed, long before it is released. Threads started in Hub (dev)
  // each work in a worktree of their own, and the dev window can show any one of them live.
  import { onMount } from "svelte";
  import { api } from "../api";
  import { hub } from "../state.svelte";
  import { openDevWindow } from "../native";
  import type { DevStatus } from "../bindings/DevStatus";
  import Select from "./Select.svelte";
  import Switch from "./Switch.svelte";

  let status = $state<DevStatus | null>(null);
  let failed = $state<string | null>(null);
  let acting = $state(false);
  let message = $state("");
  let shipped = $state<{ text: string; pr: string | null } | null>(null);
  /** Bumped when an action fails, so the pull request list shows what is saved, not the failed pick. */
  let failures = $state(0);

  async function load() {
    try {
      status = await api.dev();
      failed = null;
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    }
  }

  async function act(f: () => Promise<unknown>) {
    acting = true;
    try {
      await f();
      failed = null;
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
      failures++;
    } finally {
      acting = false;
      await load();
    }
  }

  const ship = () =>
    act(async () => {
      shipped = null;
      const done = await api.devShip(message.trim());
      shipped = {
        text: `${done.commit} ${done.message} is on ${done.branch}. ${done.updated ? "Added to its pull request" : "Pull request opened"}; the update comes once it is merged.`,
        pr: done.pr,
      };
      message = "";
    });

  onMount(() => {
    load();
    const timer = poll(load, 2000);
    return () => timer();
  });

  const words: Record<DevStatus["state"], string> = {
    missing: "Not set up",
    setting_up: "Setting up",
    stopped: "Live UI stopped",
    starting: "Starting the live UI",
    running: "Live",
    failed: "Stopped with an error",
  };
  const dot = (s: DevStatus["state"]) =>
    s === "running" ? "running" : s === "failed" ? "error" : s === "setting_up" || s === "starting" ? "waiting_on_you" : "idle";
  const coreWords: Record<DevStatus["core"], string> = {
    off: "The dev window uses this core",
    building: "Building the dev core…",
    running: "Dev core on its own copy of the data, rebuilt when Rust code is saved",
    failed: "The dev core stopped",
  };
  const coreDot = (s: DevStatus["core"]) =>
    s === "running" ? "running" : s === "failed" ? "error" : s === "building" ? "waiting_on_you" : "idle";
  const preview = $derived(status?.preview ?? null);
  // Deliberately not format.ts `ago`: this one counts seconds, for a build that just started.
  const ago = (ms: number | null) => {
    if (ms === null) return "";
    const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
    return s < 60 ? `${s} s ago` : `${Math.round(s / 60)} min ago`;
  };
  const shownThread = $derived(status?.threads.find((t) => t.thread_id === status?.shown) ?? null);
  const gb = (n: number) => `${(n / 2 ** 30).toFixed(1)} GB`;
  const unshipped = $derived(
    status
      ? [
          status.changed ? `${status.changed} file${status.changed === 1 ? "" : "s"} changed` : "",
          status.ahead ? `${status.ahead} commit${status.ahead === 1 ? "" : "s"} not shipped` : "",
        ].filter(Boolean)
      : [],
  );
</script>

<div class="dev">
  {#if status}
    <div class="row">
      <span class="dot {dot(status.state)}"></span>
      <span class="state">{words[status.state]}</span>
      <span class="dir" data-tip="Where Hub (dev) threads work">{status.dir}</span>
    </div>

    {#if status.state === "missing"}
      <p class="hint">
        Clones Hub here and adds <b>Hub (dev)</b> as a project. The dev window shows its threads' changes live.
      </p>
      <div class="actions"><button class="btn primary" disabled={acting} onclick={() => act(() => api.devSetup())}>Set up</button></div>
    {:else if status.state === "setting_up" || status.state === "starting"}
      {#if status.message}<p class="hint">{status.message}…</p>{/if}
    {:else}
      <div class="actions">
        {#if status.state === "running"}
          <button class="btn primary" onclick={() => openDevWindow(status!.url)}>Open dev window</button>
        {:else}
          <button class="btn primary" disabled={acting} onclick={() => act(() => (status!.branch === null ? api.devSetup() : api.devStart()))}>
            {status.state === "failed" ? "Try again" : "Start"}
          </button>
        {/if}
        {#if status.branch !== null}
          <button class="btn" onclick={() => hub.openProject(status!.dir)}>Open project</button>
        {/if}
        {#if status.state === "running"}
          <button class="btn" disabled={acting} onclick={() => act(api.devStop)}>Stop</button>
        {/if}
      </div>
      {#if status.state === "failed" && status.message}<pre class="error">{status.message}</pre>{/if}

      {#if status.branch !== null}
        <div class="preview">
          <span class="dot {preview ? (preview.refreshing ? 'waiting_on_you' : preview.skipped.length ? 'error' : 'running') : 'idle'}"></span>
          {#if preview}
            <span class="changes">
              {#if preview.refreshing}Updating the preview…{:else}
                {preview.only === null ? "Open pull requests" : `#${preview.only}`} on main{preview.base ? ` ${preview.base}` : ""}
                · {preview.included.length} in{preview.skipped.length ? ` · ${preview.skipped.length} left out` : ""}
                {#if preview.refreshed_at}· {ago(preview.refreshed_at)}{/if}
              {/if}
            </span>
            {#key failures}
              <Select
                tip="Which pull requests the dev window shows"
                disabled={acting}
                value={preview.only}
                onchange={(only) => act(() => api.devPreview(true, only))}
                options={[{ value: null, label: "All open" }, ...preview.open.map((pr) => ({ value: pr.number, label: `#${pr.number} ${pr.title}` }))]}
              />
            {/key}
            <button class="btn" disabled={acting || preview.refreshing} onclick={() => act(() => api.devPreview(true, preview!.only, true))}>Refresh</button>
            <button class="btn" disabled={acting} onclick={() => act(() => api.devPreview(false))} title="Back to main">Stop preview</button>
          {:else}
            <span class="changes">Show the threads' open pull requests here as they are pushed</span>
            <button class="btn" disabled={acting} onclick={() => act(() => api.devPreview(true))}>Preview pull requests</button>
          {/if}
        </div>
        {#if preview}
          {#if preview.message}<p class="hint">{preview.message}</p>{/if}
          {#if preview.included.length || preview.skipped.length}
            <ul class="prs">
              {#each preview.included as pr (pr.number)}
                <li><a href={pr.url} target="_blank" rel="noopener">#{pr.number}</a> {pr.title}</li>
              {/each}
              {#each preview.skipped as pr (pr.number)}
                <li class="out"><a href={pr.url} target="_blank" rel="noopener">#{pr.number}</a> {pr.title} <span>{pr.why}</span></li>
              {/each}
            </ul>
          {/if}
          {#if preview.core_changed && status.core === "off"}
            <p class="hint">Core changes need a dev core.</p>
          {/if}
        {/if}
      {/if}

      {#if status.branch !== null}
        <div class="threads">
          <div class="head">
            <span class="changes">
              {#if shownThread}The dev window shows <b>{shownThread.title}</b>{:else if status.threads.length}Threads on Hub, each in a worktree of its own{:else}Each new thread in Hub (dev) gets a worktree and a branch of its own{/if}
            </span>
            {#if shownThread}
              <button class="btn" disabled={acting} onclick={() => act(() => api.devShow(null))}>Back to the clone</button>
            {/if}
            <label class="toggle" title="Off: work in the clone itself">
              <Switch label="A worktree per thread" on={status.worktrees} disabled={acting} onchange={(on) => act(() => api.devWorktrees(on))} />
              A worktree per thread
            </label>
          </div>
          {#if status.threads.length}
            <ul class="list">
              {#each status.threads as t (t.thread_id)}
                <li class:shown={t.thread_id === status.shown}>
                  <span class="dot {t.status}"></span>
                  <button class="title" onclick={() => hub.open(t.thread_id)} title="Open the thread">{t.title}</button>
                  <span class="meta" title={t.path}>
                    {t.branch}{t.changed ? ` · ${t.changed} changed` : ""}{t.ahead ? ` · ${t.ahead} commit${t.ahead === 1 ? "" : "s"}` : ""}
                  </span>
                  {#if t.thread_id !== status.shown}
                    <button class="btn" disabled={acting} onclick={() => act(() => api.devShow(t.thread_id))} title="Show its work live">Show</button>
                  {/if}
                  <button
                    class="btn"
                    disabled={acting || t.status === "running"}
                    onclick={() => act(() => api.devRemoveWorktree(t.thread_id))}
                    title="Removes the worktree"
                  >Remove</button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      {#if status.branch !== null && !preview}
        <div class="ship">
          <span class="changes">{unshipped.length ? unshipped.join(" · ") : `Nothing to ship on ${status.branch}`}</span>
          {#if status.pr}<a class="pr" href={status.pr} target="_blank" rel="noopener">Pull request</a>{/if}
          {#if unshipped.length}
            <input
              placeholder="What changed (optional)"
              bind:value={message}
              onkeydown={(e) => e.key === "Enter" && !acting && ship()}
            />
            <button class="btn" disabled={acting} onclick={ship} data-tip="Commit and open a pull request">Ship</button>
          {/if}
        </div>
      {/if}
    {/if}
    {#if status.branch !== null && status.state !== "missing" && status.state !== "setting_up"}
      <div class="core">
        <span class="dot {coreDot(status.core)}"></span>
        <span class="changes">{coreWords[status.core]}</span>
        {#if status.core === "off" || status.core === "failed"}
          <button
            class="btn"
            disabled={acting}
            onclick={() => act(() => api.devCore(true))}
            data-tip="Runs the clone's core on a database copy"
          >
            {status.core === "failed" ? "Try again" : "Use a dev core"}
          </button>
        {:else}
          <button class="btn" disabled={acting} onclick={() => act(() => api.devCore(false))}>Back to this core</button>
        {/if}
      </div>
      {#if status.core_message && status.core !== "building"}<pre class="error">{status.core_message}</pre>{/if}
      {#if status.cache_bytes !== null}
        <p class="hint">Build cache {gb(status.cache_bytes)} of {gb(status.cache_cap)}</p>
      {/if}
    {/if}
    {#if shipped}
      <p class="hint">
        {shipped.text}
        {#if shipped.pr}<a href={shipped.pr} target="_blank" rel="noopener">Open it</a>{/if}
      </p>
    {/if}
  {/if}
  {#if failed}<pre class="error">{failed}</pre>{/if}
</div>

<style>
  .dev { border: 1px solid var(--line); border-radius: var(--r-md); padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; }
  .row { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .state { color: var(--text); }
  .dir { color: var(--muted); font-family: var(--mono); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hint { color: var(--text-3); margin: 0; max-width: 640px; }
  .actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .ship { display: flex; align-items: center; gap: 8px; border-top: 1px solid var(--line); padding-top: 10px; }
  .changes { color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .core { display: flex; align-items: center; gap: 8px; border-top: 1px solid var(--line); padding-top: 10px; }
  .core .changes { flex: 1; min-width: 0; }
  .pr { white-space: nowrap; }
  .preview { display: flex; align-items: center; gap: 8px; border-top: 1px solid var(--line); padding-top: 10px; }
  .preview .changes { flex: 1; min-width: 0; }
  .preview :global(.select) { max-width: 220px; }
  .threads { display: flex; flex-direction: column; gap: 6px; border-top: 1px solid var(--line); padding-top: 10px; }
  .threads .head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .threads .head .changes { flex: 1; min-width: 0; white-space: normal; }
  .toggle { display: flex; align-items: center; gap: 6px; color: var(--muted); white-space: nowrap; }
  .list { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 4px; }
  .list li { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-width: 0; padding: 2px 6px; border-radius: var(--r-md); }
  .list li.shown { background: var(--surface); }
  .list .title { background: none; border: 0; padding: 0; color: var(--text); font: inherit; cursor: pointer; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1 1 160px; }
  .list .title:hover { text-decoration: underline; }
  .list .meta { color: var(--muted); font-family: var(--mono); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1 1 140px; }
  .prs { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 2px; font-size: 12px; color: var(--text-3); }
  .prs li { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .prs .out { color: var(--muted); }
  .prs .out span { color: var(--amber, var(--muted)); }
  .ship input {
    flex: 1;
    min-width: 0;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    color: var(--text);
    padding: 4px 8px;
    font: inherit;
  }
  .error { color: var(--red); background: var(--red-bg); border-radius: var(--r-md); padding: 8px 10px; margin: 0; white-space: pre-wrap; font-size: 12px; }
</style>
