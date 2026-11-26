<!-- One repository on the Plugins page; a click opens its own page. Two sizes: `rising` is the big, ranked card of what gains the most stars this week, with its gain and star line up front; the compact card (new, most stars, search) leads with the name and who made it, and says `note` (how old it is, or what Customs says) beside the stars.
     Used by PluginsView.svelte. -->
<script lang="ts">
  // One repository on the Plugins page; a click opens its own page. Two sizes: `rising` is the big,
  // ranked card of what gains the most stars this week, with its gain and star line up front;
  // the compact card (new, most stars, search) leads with the name and who made it, and says
  // `note` (how old it is, or what Customs says) beside the stars.
  import type { PluginRepo } from "../bindings/PluginRepo";
  import { count, customs, daysAgo, kindLabel, line, spanLabel, weekLabel } from "../plugins";
  import { hub } from "../state.svelte";

  let { repo, rank = null, rising = false, age = false }: { repo: PluginRepo; rank?: number | null; rising?: boolean; age?: boolean } = $props();

  const week = $derived(weekLabel(repo));
  const spark = $derived(line(repo.spark, rising ? 120 : 54, rising ? 30 : 16, 2));
  const stamp = $derived(customs(repo));
</script>

<button class="card" class:rising class:sel={hub.pluginRepo === repo.full_name} onclick={() => hub.openPlugins(hub.pluginRepo === repo.full_name ? null : repo.full_name)}>
  <span class="top">
    {#if repo.avatar}<img class="av" src={repo.avatar} alt="" />{:else}<span class="av"></span>{/if}
    <span class="who">
      <span class="nm">{repo.name}</span>
      <span class="own">{repo.owner}</span>
    </span>
    {#if rank != null}<span class="rank" aria-label="Rank {rank}">{rank}</span>{:else}<span class="kind">{kindLabel(repo.kind)}</span>{/if}
  </span>
  <span class="desc">{repo.description ?? ""}</span>
  {#if rising}
    <span class="trend">
      <span class="gain" data-tip={repo.week_estimated ? "Stars this week, projected from the newest stars" : "Stars gained in the last 7 days"}>{week ?? "–"}<small>this week</small></span>
      {#if spark}<svg class="spark" data-tip="Stars over the last {spanLabel(repo.spark_span)}" viewBox="0 0 120 30" preserveAspectRatio="none" aria-hidden="true"><polyline points={spark} /></svg>{/if}
    </span>
  {/if}
  <span class="meta">
    <span>★ {count(repo.stars)}</span>
    {#if rising}<span class="kind">{kindLabel(repo.kind)}</span>{/if}
    {#if !rising && !age && week}<span class="gain-s">{week}</span>{/if}
    {#if age}<span>{daysAgo(repo.created_at)} old</span>{/if}
    <span class="st {stamp.tone}" data-tip={stamp.tone === "none" ? "Customs has not looked at it yet" : `Customs: ${stamp.text}`}><span class="ring"></span>{stamp.text}</span>
  </span>
</button>

<style>
  .card { display: flex; flex-direction: column; gap: 8px; text-align: left; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 12px 14px 12px; min-width: 0; transition: border-color 0.12s var(--ease), background 0.12s var(--ease), transform 0.12s var(--ease); }
  .card:hover { border-color: var(--line-strong); background: var(--surface-2); }
  .card.sel { border-color: var(--muted); background: var(--surface-2); }
  .top { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .av { width: 30px; height: 30px; border-radius: var(--r-md); flex: none; background: var(--surface-hover); object-fit: cover; border: 1px solid var(--line); }
  .who { display: flex; flex-direction: column; min-width: 0; flex: 1; line-height: 1.25; }
  .nm { color: var(--text); font-weight: 600; font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .own { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .kind { font-size: 11px; border: 1px solid var(--line-strong); border-radius: var(--r-xs); padding: 0 4px; color: var(--muted); flex: none; align-self: center; }
  .desc { color: var(--text-3); font-size: 13px; line-height: 1.45; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; min-height: 2.9em; }
  .meta { display: flex; align-items: center; gap: 12px; color: var(--muted); font-size: 12px; white-space: nowrap; min-width: 0; margin-top: auto; }
  .gain-s { color: var(--green); }
  .st { display: inline-flex; align-items: center; gap: 4px; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .ring { width: 7px; height: 7px; border-radius: 50%; flex: none; border: 1.5px solid currentColor; }
  .st.ok { color: var(--green); }
  .st.held { color: var(--amber); }
  .st.no { color: var(--red); }
  .st.ok .ring, .st.held .ring, .st.no .ring { background: currentColor; }

  /* Rising: bigger, ranked, the week's gain and its line up front. */
  .rising { padding: 16px 16px 14px; gap: 10px; background: linear-gradient(180deg, color-mix(in srgb, var(--green) 5%, var(--surface)), var(--surface) 55%); }
  .rising .av { width: 38px; height: 38px; border-radius: var(--r-lg); }
  .rising .nm { font-size: 15px; }
  .rank { font: 600 22px/1 var(--mono); color: var(--line-strong); flex: none; align-self: flex-start; }
  .card.rising:hover .rank { color: var(--muted); }
  .trend { display: flex; align-items: flex-end; gap: 14px; }
  .gain { display: flex; align-items: baseline; gap: 6px; color: var(--green); font-size: 20px; font-weight: 600; white-space: nowrap; }
  .gain small { font-size: 12px; font-weight: 400; color: var(--muted); }
  .spark { flex: 1; height: 30px; min-width: 0; }
  .spark polyline { fill: none; stroke: var(--green); stroke-width: 1.6; stroke-linejoin: round; stroke-linecap: round; vector-effect: non-scaling-stroke; }
</style>
