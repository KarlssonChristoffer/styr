<!-- Plugins: GitHub repositories made for agents. Discover shows what is rising this week, what is new this month and what most people use; search goes over all of GitHub. A repository opens on its own page. Installing and switching one on in a thread come in a later step.
     Used by App.svelte. -->
<script lang="ts">
  import Page from "./Page.svelte";
  // Plugins: GitHub repositories made for agents. Discover shows what is rising this week, what
  // is new this month and what most people use; search goes over all of GitHub. A repository
  // opens on its own page. Installing and switching one on in a thread come in a later step.
  import { api } from "../api";
  import type { PluginDiscover } from "../bindings/PluginDiscover";
  import type { PluginRepo } from "../bindings/PluginRepo";
  import { ago } from "../format";
  import { KIND_SAYS, KINDS, QUICK } from "../plugins";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";
  import Quiet from "./Quiet.svelte";
  import PluginCard from "./PluginCard.svelte";
  import PluginPage from "./PluginPage.svelte";

  let data = $state<PluginDiscover | null>(null);
  let failed = $state<string | null>(null);
  let loading = $state(false);
  let kind = $state("all");
  let query = $state("");
  let found = $state<PluginRepo[] | null>(null);
  let searching = $state(false);
  let searchNote = $state<string | null>(null);
  // On a wide window a repository opens in a panel beside the lists instead of on its own page.
  let width = $state(0);
  const wide = $derived(width >= 1500);
  // Rising shows its first six until asked, so the other lists stay in view.
  let allRising = $state(false);

  // While Hub counts in the background (Rising, or the lists again), the page asks every few seconds.
  let again: ReturnType<typeof setTimeout> | null = null;

  async function load(fresh = false) {
    loading = true;
    if (again) clearTimeout(again);
    again = null;
    try {
      data = await api.pluginsDiscover(fresh);
      failed = null;
      if (data.updating) again = setTimeout(() => load(), 2500);
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  $effect(() => () => {
    if (again) clearTimeout(again);
  });

  async function search(e?: Event) {
    e?.preventDefault();
    const q = query.trim();
    if (!q) return clear();
    // A GitHub link or owner/name goes straight to that repository's page.
    const direct = q.match(/^(?:https?:\/\/github\.com\/)?([\w.-]+\/[\w.-]+?)(?:\.git)?\/?$/);
    if (direct && q.includes("/")) return hub.openPlugins(direct[1]);
    searching = true;
    try {
      const r = await api.pluginsSearch(q);
      found = r.repos;
      searchNote = r.message ?? null;
    } catch (err) {
      searchNote = err instanceof Error ? err.message : String(err);
    } finally {
      searching = false;
    }
  }

  function quick(q: string) {
    query = q;
    search();
  }

  function clear() {
    query = "";
    found = null;
    searchNote = null;
  }

  // The lists load when they are on screen: always, except behind a repository's full page.
  $effect(() => {
    if (!data && !loading && !failed && (!hub.pluginRepo || wide)) load();
  });

  function key(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (e.key === "Escape" && wide && hub.pluginRepo && !t?.closest("input, textarea, [contenteditable]")) hub.openPlugins();
  }

  const pick = (list: PluginRepo[]) => (kind === "all" ? list : list.filter((r) => r.kind === kind));
  const rising = $derived(pick(data?.rising ?? []));
  const fresh = $derived(pick(data?.fresh ?? []));
  const top = $derived(pick(data?.top ?? []));
  const results = $derived(found ? pick(found) : null);
  // How many of each kind the page holds now (search results while searching), each repo once.
  const counts = $derived.by(() => {
    const seen = new Map<string, string>();
    for (const r of found ?? [...(data?.rising ?? []), ...(data?.fresh ?? []), ...(data?.top ?? [])]) seen.set(r.full_name, r.kind);
    const c: Record<string, number> = { all: seen.size };
    for (const k of seen.values()) c[k] = (c[k] ?? 0) + 1;
    return c;
  });
</script>

<svelte:window onkeydown={key} />

<div class="wrap" class:split={wide && !!hub.pluginRepo} bind:clientWidth={width}>
{#if hub.pluginRepo && !wide}
  {#key hub.pluginRepo}<PluginPage name={hub.pluginRepo} />{/key}
{:else}
  <Page title="Plugins" width={wide && hub.pluginRepo ? "full" : "wide"}>
    {#snippet head()}
      {#if data}<span class="label" data-tip="Hub looks again in the background after an hour">Looked {ago(data.fetched_at)} ago{#if data.updating && rising.length} · looking again…{/if}</span>{/if}
      <button class="btn push" disabled={loading || data?.updating} onclick={() => load(true)}><Icon name="restart" size={12} />{loading || data?.updating ? "Looking…" : "Look again"}</button>
    {/snippet}
    <p class="summary">Agent tools from GitHub.</p>

    <form class="search" onsubmit={search}>
      <Icon name="search" size={16} />
      <input bind:value={query} placeholder="Search GitHub or paste a link" aria-label="Search GitHub" />
      {#if query}<button type="button" class="x" aria-label="Clear" onclick={clear}><Icon name="x" size={12} /></button>{/if}
      <button type="submit" class="go" disabled={!query.trim() || searching}>Search</button>
    </form>
    <div class="quick">
      <span>Try</span>
      {#each QUICK as q (q)}<button class:on={found && query === q} onclick={() => quick(q)}>{q}</button>{/each}
    </div>

    <div class="show">
      <span class="lbl">Show</span>
      <div class="seg" role="radiogroup" aria-label="Show kind">
        {#each KINDS as [id, label] (id)}
          <button role="radio" aria-checked={kind === id} class:on={kind === id} onclick={() => (kind = id)}>{label}{#if counts[id]}<i>{counts[id]}</i>{/if}</button>
        {/each}
      </div>
      <span class="says">{KIND_SAYS[kind]}</span>
    </div>

    {#if results || searching || searchNote}
      <div class="h"><b>{searching ? "Searching…" : `Found ${results?.length ?? 0}`}</b>{#if !searching}<span>best match first, all of GitHub</span>{/if}</div>
      {#if searchNote}<p class="say bad">{searchNote}</p>{/if}
      {#if results?.length}
        <div class="grid small">{#each results as r (r.full_name)}<PluginCard repo={r} />{/each}</div>
      {:else if results && !searching}
        <p class="say">Nothing {kind === "all" ? "" : "of that kind "}matched.</p>
      {/if}
    {:else}
      {#if failed}<p class="say bad">{failed}</p>{/if}
      {#if data?.message}<p class="say bad">{data.message}</p>{/if}
      {#if !data && loading}
        <div class="grid">{#each Array(6) as _, i (i)}<div class="ghost"></div>{/each}</div>
      {/if}
      {#if !rising.length && data?.updating && kind === "all"}
        <div class="h"><b>Rising this week</b><span>counting this week's stars…</span></div>
        <div class="grid">{#each Array(3) as _, i (i)}<div class="ghost"></div>{/each}</div>
      {/if}
      {#if rising.length}
        <div class="h"><b>Rising this week</b><span>most stars gained in the last 7 days</span></div>
        <div class="grid">{#each allRising ? rising : rising.slice(0, 6) as r, i (r.full_name)}<PluginCard repo={r} rising rank={i + 1} />{/each}</div>
        {#if rising.length > 6}<button class="more" onclick={() => (allRising = !allRising)}>{allRising ? "Show fewer" : `Show all ${rising.length}`}</button>{/if}
      {/if}
      {#if fresh.length}
        <div class="h"><b>New this month</b><span>made in the last 30 days</span></div>
        <div class="grid small">{#each fresh as r (r.full_name)}<PluginCard repo={r} age />{/each}</div>
      {/if}
      {#if top.length}
        <div class="h"><b>Most stars</b><span>worked on in the last month</span></div>
        <div class="grid small">{#each top as r (r.full_name)}<PluginCard repo={r} />{/each}</div>
      {/if}
      {#if data && !data.updating && !rising.length && !fresh.length && !top.length && !data.message}
        <Quiet text="Nothing of that kind right now" />
      {/if}
    {/if}
  </Page>
  {#if hub.pluginRepo}
    <aside class="side">{#key hub.pluginRepo}<PluginPage name={hub.pluginRepo} side />{/key}</aside>
  {/if}
{/if}
</div>

<style>
  .wrap { flex: 1; min-height: 0; display: flex; width: 100%; min-width: 0; }
  /* Beside the panel the lists use all the room they have left. */
  .split .search, .split .show, .split .h, .split .grid { max-width: none; }
  .side { width: clamp(560px, 44%, 940px); flex: none; display: flex; min-height: 0; border-left: 1px solid var(--line); background: color-mix(in srgb, var(--surface) 35%, var(--bg)); }
  .push { margin-left: auto; }
  .label { color: var(--muted); font-size: 12px; }
  .summary { color: var(--text-3); font-size: 13px; margin: 6px 0 0; max-width: var(--prose); }
  .btn { height: 26px; padding: 0 10px; font-size: 13px; display: inline-flex; align-items: center; gap: 4px; }
  .say { color: var(--text-3); font-size: 13px; margin: 10px 0; }
  .say.bad { color: var(--red); }

  .search { display: flex; align-items: center; gap: 10px; margin-top: 18px; background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 0 6px 0 14px; color: var(--muted); max-width: 1200px; }
  .search:focus-within { border-color: var(--muted); }
  .search input { flex: 1; background: none; border: 0; outline: none; padding: 12px 0; font-size: 15px; min-width: 0; }
  .search input::placeholder { color: var(--muted); }
  .x { display: grid; color: var(--muted); padding: 4px; border-radius: var(--r-sm); }
  .x:hover { color: var(--text); background: var(--surface-hover); }
  .go { height: 30px; padding: 0 12px; border-radius: var(--r-md); font-size: 13px; color: var(--text); background: var(--surface-hover); }
  .go:disabled { color: var(--muted); background: none; }
  .quick { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; margin: 8px 0 0 2px; font-size: 12px; }
  .quick span { color: var(--muted); margin-right: 4px; }
  .quick button { color: var(--text-3); padding: 2px 8px; border-radius: var(--r-sm); }
  .quick button:hover, .quick button.on { color: var(--text); background: var(--surface-hover); }

  .show { display: flex; align-items: center; gap: 12px; margin: 18px 0 4px; max-width: 1200px; flex-wrap: wrap; }
  .lbl { color: var(--muted); font-size: 12px; }
  .seg { display: inline-flex; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-md); padding: 2px; }
  .seg button { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: var(--text-3); padding: 4px 11px; border-radius: var(--r-sm); }
  .seg button:hover { color: var(--text); }
  .seg button.on { background: var(--surface-hover); color: var(--text); }
  .seg i { font-style: normal; font-size: 11px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .says { color: var(--muted); font-size: 12px; min-width: 0; }
  .h { display: flex; align-items: baseline; gap: 10px; margin: 26px 0 10px; max-width: 1200px; }
  .h b { color: var(--text); font-weight: 500; font-size: 14px; }
  .h span { color: var(--muted); font-size: 12px; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(calc(300px * var(--text-grow)), 1fr)); gap: 12px; max-width: 1200px; }
  .grid.small { grid-template-columns: repeat(auto-fill, minmax(calc(230px * var(--text-grow)), 1fr)); gap: 8px; }
  .more { margin-top: 8px; font-size: 12px; color: var(--muted); padding: 2px 0; }
  .more:hover { color: var(--text); }
  .ghost { height: 150px; border-radius: var(--r-md); background: var(--surface); border: 1px solid var(--line); }
</style>
