<!-- Models and cost: which models answered over the period picked in the card's menu, as a band of their share of the tokens in the colours of their makers (Anthropic violet, deeper for the bigger model; OpenAI teal; Google sky; DeepSeek blue; open models lime), then a row per model with tokens, API-price cost and its proven changes, the projects and threads that cost the most, and one line with the totals. Read from Claude Code's own session files. A model's "per proven change" is what its changes in Hub threads cost, failures included, divided by the ones that showed proof: the evidence for picking a model.
     Used by CardBody.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // Models and cost: which models answered over the period picked in the card's menu, as a band
  // of their share of the tokens in the colours of their makers (Anthropic violet, deeper for
  // the bigger model; OpenAI teal; Google sky; DeepSeek blue; open models lime), then a row per
  // model with tokens, API-price cost and its proven changes, the projects and threads that
  // cost the most, and one line with the totals. Read from Claude Code's own session files. A
  // model's "per proven change" is what its changes in Hub threads cost, failures included,
  // divided by the ones that showed proof: the evidence for picking a model.
  import { api } from "../api";
  import type { ModelStat } from "../bindings/ModelStat";
  import type { Stats } from "../bindings/Stats";
  import { folderName } from "../format";
  import { knob, pickRange, setKnob, takeRange } from "../overview.svelte";
  import { prefs } from "../prefs.svelte";
  import { recall, remember } from "../lastSeen";
  import { hub } from "../state.svelte";
  import HowCounted from "./HowCounted.svelte";
  import Ref from "./Ref.svelte";

  const tz = -new Date().getTimezoneOffset();
  const RANGES = [7, 30, 365];

  const days = $derived.by(() => {
    void prefs.overviewFrames;
    return Number(knob<string>("stats", "range"));
  });
  let stats = $state<Stats | null>(null);
  let error = $state("");

  // A range picked in a linked card moves this one to its nearest range (overview.svelte.ts);
  // one picked in the menu reaches the linked cards, and does not go back.
  let handed = false;
  takeRange("stats", RANGES, (d) => {
    handed = true;
    setKnob("stats", "range", String(d));
  });
  let last: number | null = null;
  $effect(() => {
    if (last !== null && days !== last && !handed) pickRange("stats", days);
    handed = false;
    last = days;
  });

  $effect(() => {
    const asked = days;
    const key = `stats.${asked}.${tz}`;
    // The last answer for this range shows at once; the new one replaces it when it comes.
    stats = recall<Stats>(key);
    api.stats(asked, tz).then(
      (s) => {
        if (asked !== days) return;
        stats = s;
        error = "";
        remember(key, s);
      },
      (e) => {
        if (asked === days) error = String(e);
      },
    );
  });

  const tokens = (n: number) =>
    n >= 1e9 ? `${(n / 1e9).toFixed(1)}B` : n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${Math.round(n / 1e3)}k` : `${n}`;
  const dollars = (n: number) => (n >= 100 ? `$${Math.round(n)}` : n >= 1 ? `$${n.toFixed(1)}` : n > 0 ? `$${n.toFixed(2)}` : "$0");
  const perProven = (m: ModelStat) => (m.proven ? dollars(m.result_cost / m.proven) : "–");
  const period = $derived(days === 7 ? "this week" : days === 30 ? "this month" : "this year");
  /** A model's colour: its maker's hue, and within Anthropic's family a deeper shade for the bigger model. */
  function colour(model: string): string {
    const m = model.toLowerCase();
    if (/opus/.test(m)) return "var(--p-violet)";
    if (/sonnet/.test(m)) return "color-mix(in srgb, var(--p-violet) 62%, var(--surface))";
    if (/haiku/.test(m)) return "color-mix(in srgb, var(--p-violet) 38%, var(--surface))";
    if (/claude/.test(m)) return "var(--p-violet)";
    if (/gpt|codex|^o\d/.test(m)) return "var(--p-teal)";
    if (/gemini/.test(m)) return "var(--p-sky)";
    if (/deepseek/.test(m)) return "var(--p-blue)";
    if (/hermes|llama|mistral|qwen|kimi|glm/.test(m)) return "var(--p-lime)";
    return "var(--p-slate)";
  }
</script>

<HowCounted card="stats">
  <p>Tokens and API-price cost from Claude Code's session files. The band is each model's share.</p>
</HowCounted>

{#if !stats && error}
  <p class="quiet">Could not read the stats: {error}</p>
{:else if !stats}
  <Quiet kind="loading" />
{:else if !stats.answers}
  <p class="quiet">No Claude Code answers {period} yet.</p>
{:else}
  <div class="share" aria-hidden="true">
    {#each stats.models as m (m.model)}
      <i style:flex={m.tokens} style:background={colour(m.model)} data-tip="{m.model} · {tokens(m.tokens)} tokens"></i>
    {/each}
  </div>
  <div class="models">
    {#each stats.models as m (m.model)}
      <div class="row">
        <i class="dot" style:background={colour(m.model)}></i>
        <span class="name">{m.model}</span>
        <span class="val">{tokens(m.tokens)}</span>
        <span class="val cost" data-tip={m.priced ? "" : "No price known for this model"}>{m.priced ? dollars(m.cost) : "–"}</span>
        <span class="val proof" data-tip={m.results ? `${m.proven} of ${m.results} changes in Hub threads showed proof; each proven one cost about ${perProven(m)}, failures included` : "No checked changes in Hub threads"}>
          {m.results ? `${m.proven}/${m.results} · ${perProven(m)}` : ""}
        </span>
      </div>
    {/each}
  </div>
  {#if stats.projects.length || stats.threads.length}
    <div class="lists">
      {#if stats.projects.length}
        <div>
          {#each stats.projects.slice(0, 6) as p (p.name + (p.path ?? ""))}
            <div class="row small">
              <span class="name">
                {#if p.path}<Ref kind="project" label={p.name || folderName(p.path)} title="Open {p.path}" onopen={() => hub.openProject(p.path!)} />{:else}{p.name}{/if}
              </span>
              <span class="val">{tokens(p.tokens)}</span>
              <span class="val cost">{dollars(p.cost)}</span>
            </div>
          {/each}
        </div>
      {/if}
      {#if stats.threads.length}
        <div>
          {#each stats.threads.slice(0, 6) as t (t.id)}
            <button class="row small thread" onclick={() => hub.open(t.id)}>
              <span class="name">{t.title}</span>
              <span class="val">{tokens(t.tokens)}</span>
              <span class="val cost">{dollars(t.cost)}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
  <p class="caption"><em>{tokens(stats.tokens)}</em> tokens · {stats.answers.toLocaleString()} answers · <em>≈ {dollars(stats.cost)}</em> at API prices · {period}</p>
{/if}

<style>
  .share { display: flex; gap: 2px; height: 14px; border-radius: var(--r-xs); overflow: hidden; margin-bottom: 12px; }
  .share i { display: block; min-width: 2px; opacity: 0.92; }
  .models { display: flex; flex-direction: column; gap: 8px; }
  .row { display: flex; align-items: center; gap: 10px; min-width: 0; font-size: 13px; }
  .dot { width: 10px; height: 10px; border-radius: 3px; flex: none; }
  .name { flex: 1; min-width: 0; display: flex; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); }
  .val { min-width: 44px; text-align: right; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .cost { color: var(--muted); }
  .proof { min-width: 84px; color: var(--muted); font-size: 12px; }
  /* The projects and threads that cost the most: side by side when the card is wide. */
  .lists { display: grid; grid-template-columns: 1fr; gap: 6px 24px; margin-top: 12px; padding-top: 10px; border-top: 1px solid var(--line); }
  @container (min-width: 560px) { .lists { grid-template-columns: 1fr 1fr; } }
  .lists > div { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .row.small { font-size: 12px; }
  .row.small .name { color: var(--text-3); }
  .row.thread { width: 100%; text-align: left; padding: 0; }
  .row.thread:hover .name { color: var(--text); }
</style>
