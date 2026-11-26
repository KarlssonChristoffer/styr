<!-- The project's rules in force: what its rules files tell every agent, one line each, with the file and how long ago it last changed. Search and a theme filter on top; the week's changes first, so "what did I say three days ago" is the first thing seen.
     Used by ProjectView.svelte. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { tooOld } from "../compat";
  import { quiet } from "../quiet";
  import { ago } from "../format";
  import { THEMES, byTheme, origin, pick, recent } from "../rulesView";
  import type { Standing } from "../bindings/Standing";
  import type { StandingRule } from "../bindings/StandingRule";
  import Icon from "./Icon.svelte";
  import Quiet from "./Quiet.svelte";

  // The project's rules in force: what its rules files tell every agent, one line each, with the
  // file and how long ago it last changed (git blame). Read only.
  let { path, host = null }: { path: string; host?: string | null } = $props();
  let data = $state<Standing | null>(null);
  let newer = $state(false);
  let failed = $state<string | null>(null);
  let query = $state("");
  let theme = $state("all");

  onMount(() => {
    quiet(
      api
        .projectRules(path, host)
        .then((d) => (data = d))
        .catch((e) => {
          if (tooOld(e)) newer = true;
          else failed = e instanceof Error ? e.message : String(e);
        }),
      "rules",
    );
  });

  const now = Math.floor(Date.now() / 1000);
  const found = $derived(data ? pick(data.rules, query, theme) : []);
  const fresh = $derived(query || theme !== "all" ? [] : recent(found, now));
  const groups = $derived(byTheme(found));
  const count = (id: string) => data?.rules.filter((r) => r.theme === id).length ?? 0;
  const tip = (r: StandingRule) => (r.thread ? `Said in “${r.thread}”` : r.commit ? `${r.commit} · ${r.why}` : `${r.file} line ${r.line}, not committed`);
  const age = (r: StandingRule) => (r.since === null ? "" : ago(r.since * 1000));
</script>

{#snippet row(r: StandingRule)}
  <li data-tip={tip(r)}>
    <span class="text">{r.text}</span>
    <span class="src" class:hub={r.source === "hub"}>{origin(r)}</span>
    <span class="age">{age(r)}</span>
  </li>
{/snippet}

<div class="rules">
  {#if newer}
    <Quiet kind="newer" />
  {:else if failed}
    <p class="error">{failed}</p>
  {:else if !data}
    <Quiet kind="loading" />
  {:else if !data.rules.length}
    <Quiet text="No rules file" />
  {:else}
    <div class="bar">
      <button class="f" class:on={theme === "all"} onclick={() => (theme = "all")}>All <span class="n">{data.rules.length}</span></button>
      {#each THEMES as t (t.id)}
        {#if count(t.id)}
          <button class="f" class:on={theme === t.id} onclick={() => (theme = theme === t.id ? "all" : t.id)}>{t.label} <span class="n">{count(t.id)}</span></button>
        {/if}
      {/each}
      <label class="search"><Icon name="search" size={13} /><input bind:value={query} placeholder="Find a rule…" spellcheck="false" /></label>
    </div>
    <div class="scroll">
      {#if fresh.length}
        <h3>Changed this week</h3>
        <ul>{#each fresh as r (r.id)}{@render row(r)}{/each}</ul>
      {/if}
      {#each groups as g (g.id)}
        <h3>{g.label}</h3>
        <ul>{#each g.rules as r (r.id)}{@render row(r)}{/each}</ul>
      {:else}
        <Quiet text="No rule matches" />
      {/each}
    </div>
  {/if}
</div>

<style>
  .rules { display: flex; flex-direction: column; min-height: 0; height: 100%; padding-top: 12px; }
  .bar { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; padding-bottom: 8px; }
  .f { padding: 4px 10px; border-radius: var(--r-md); color: var(--muted); font-size: 13px; }
  .f:hover { color: var(--text); }
  .f.on { background: var(--surface-2); color: var(--text); }
  .n { color: var(--muted); font-size: 12px; margin-left: 2px; }
  .search { margin-left: auto; display: flex; align-items: center; gap: 8px; padding: 4px 10px; border-radius: var(--r-md); box-shadow: inset 0 0 0 1px var(--line-strong); color: var(--muted); width: min(280px, 34%); }
  .search input { flex: 1; min-width: 0; background: none; border: 0; color: var(--text); font: inherit; font-size: 13px; }
  .search input:focus { outline: none; }
  .scroll { flex: 1; min-height: 0; overflow-y: auto; padding-bottom: 20px; }
  h3 { color: var(--muted); font-size: 12px; font-weight: 500; margin: 16px 0 4px; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; align-items: baseline; gap: 12px; padding: 7px 0; border-bottom: 1px solid var(--line); font-size: 13px; line-height: 1.45; }
  .text { flex: 1; min-width: 0; color: var(--text); overflow-wrap: anywhere; }
  .src { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .src.hub { color: var(--text); }
  .age { color: var(--muted); font-size: 12px; width: 44px; text-align: right; white-space: nowrap; }
  .error { color: var(--red); font-size: 13px; }
</style>
