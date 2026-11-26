<!-- The top of Settings > Engines: two knobs, what new threads start with (engine by its mark, model, effort) and when agents compact, each with the projects that differ folded under it ("Except in"). The default engine is saved in hub.json (by the page, through `onengine`); its start model and effort and each project's start live in this window (`startDefaults` in `providers.svelte.ts`) and Home reads them; compaction, everywhere and per project, is kept by the core and handed to each agent it starts (`agent_prefs.rs`).
     Used by Settings.svelte. -->
<script lang="ts">
  // The top of Settings > Engines: two knobs, what new threads start with (engine by its mark,
  // model, effort) and when agents compact, each with the projects that differ folded under it
  // ("Except in"). The default engine is saved in hub.json (by the page, through `onengine`); its
  // start model and effort and each project's start live in this window (`startDefaults` in
  // `providers.svelte.ts`) and Home reads them; compaction, everywhere and per project, is kept
  // by the core and handed to each agent it starts (`agent_prefs.rs`).
  import { untrack } from "svelte";
  import { agent, COMPACT, compactWord, loadAgentPrefs, saveAgentPrefs, setProjectPrefs } from "../agentPrefs.svelte";
  import type { SettingValue } from "../bindings/SettingValue";
  import { engineProvider, engines, knownModels, setProjectStart, setStartDefault, startDefaults } from "../providers.svelte";
  import { knownOptions, modelLabel } from "../session";
  import Exceptions from "./Exceptions.svelte";
  import ProviderMark from "./ProviderMark.svelte";
  import Select from "./Select.svelte";

  let { engine, onengine }: { engine: string; onengine: (name: string) => void } = $props();

  $effect(() => {
    untrack(loadAgentPrefs);
  });
  const prefs = $derived(agent.prefs);

  const OWN: Record<string, string> = { claude: "Claude Code", codex: "Codex" };
  const label = (name: string) => engines.labels[name] ?? OWN[name] ?? engineProvider(name).name;

  // The default engine's model and effort, as its last thread reported them.
  const model = $derived(knownModels(engine));
  const effort = $derived(knownOptions(engine).find((o) => o.category === "thought_level"));
  const LAST = "__last";
  const pinned = (id: string | undefined): string =>
    id && startDefaults.by[engine]?.[id] !== undefined ? String(startDefaults.by[engine][id]) : LAST;
  const pin = (id: string, value: string) => setStartDefault(engine, id, value === LAST ? undefined : (value as SettingValue));

  const compact = $derived(compactWord(prefs));
  /** How full the dial is drawn: the percent, all of it for Auto, none for Off. */
  const fill = $derived(compact === "never" ? 0 : compact === "engine" ? 95 : Number(compact));
  function setCompact(v: string) {
    saveAgentPrefs(v === "never" ? { auto_compact_off: true, compact_at: null } : { auto_compact_off: false, compact_at: v === "engine" ? null : Number(v) });
  }
  const compactLabel = (v: string) => COMPACT.find((c) => c.value === v)?.label ?? v;

  const ONLY = "Claude Code follows this. Codex decides for itself; Hermes has its own under Settings on its row.";

  // ---- Projects that start differently ----
  const SAME = "__same";
  const startOf = (path: string) => startDefaults.projects[path] ?? {};
  /** The engine a project starts on: its own, else the default. */
  const engineOf = (path: string) => startOf(path).engine ?? engine;
  function startSummary(path: string): string {
    const s = startOf(path);
    const e = engineOf(path);
    const m = knownModels(e);
    const f = knownOptions(e).find((o) => o.category === "thought_level");
    const pick = (o: typeof m | typeof f) => {
      const v = o && s.options?.[o.id];
      return v === undefined || !o ? null : (o.choices.find((c) => c.value === v)?.name ?? String(v));
    };
    return [s.engine ? label(s.engine) : null, pick(m), pick(f)].filter(Boolean).join(" · ") || "as the default";
  }
  const startItems = $derived(Object.keys(startDefaults.projects).map((path) => ({ path, summary: startSummary(path) })));
  function setOwnEngine(path: string, name: string) {
    // Another engine has other models and efforts: they start over.
    setProjectStart(path, name === engine ? {} : { engine: name });
  }
  function setOwnOption(path: string, id: string, value: string) {
    const s = startOf(path);
    setProjectStart(path, { ...s, options: { ...(s.options ?? {}), [id]: value === SAME ? (undefined as unknown as SettingValue) : (value as SettingValue) } });
  }

  const compactItems = $derived(Object.entries(prefs?.projects ?? {}).filter(([, p]) => p?.compact != null).map(([path, p]) => ({ path, summary: compactLabel(p!.compact!) })));
</script>

<div class="knobs">
  <div class="knob">
    <span class="l">New threads start on</span>
    <div class="marks" role="radiogroup" aria-label="Default engine">
      {#each engines.names as name (name)}
        <button role="radio" aria-checked={engine === name} class:on={engine === name} data-tip={`${label(name)} · ${name}`} onclick={() => engine !== name && onengine(name)}>
          <ProviderMark id={engineProvider(name).id} size={20} />
        </button>
      {/each}
      <span class="which">{label(engine)}</span>
    </div>
    {#if model}
      <Select label="Start model" value={pinned(model.id)} onchange={(v) => pin(model.id, v)}
        options={[{ value: LAST, label: "Last used model" }, ...model.choices.map((c) => ({ value: String(c.value), label: modelLabel(c), group: c.group ?? undefined }))]} />
    {:else}
      <span class="dim">Its models show here once it has run a thread.</span>
    {/if}
    {#if effort}
      <div class="seg" role="radiogroup" aria-label="Start effort">
        <button role="radio" aria-checked={pinned(effort.id) === LAST} class:on={pinned(effort.id) === LAST} data-tip="As its last thread" onclick={() => pin(effort.id, LAST)}>Last</button>
        {#each effort.choices.filter((c) => c.value !== "default") as c (c.value)}
          <button role="radio" aria-checked={pinned(effort.id) === String(c.value)} class:on={pinned(effort.id) === String(c.value)} onclick={() => pin(effort.id, String(c.value))}>{c.name}</button>
        {/each}
      </div>
    {/if}
    <Exceptions items={startItems} onremove={(p) => setProjectStart(p, undefined)}
      onadd={(p) => setProjectStart(p, { engine: engines.names.find((n) => n !== engine) ?? engine })}>
      {#snippet editor(path)}
        {@const e = engineOf(path)}
        {@const m = knownModels(e)}
        {@const f = knownOptions(e).find((o) => o.category === "thought_level")}
        {@const s = startOf(path)}
        <div class="marks small" role="radiogroup" aria-label="Engine there">
          {#each engines.names as name (name)}
            <button role="radio" aria-checked={e === name} class:on={e === name} data-tip={name === engine ? `${label(name)}, the default` : label(name)} onclick={() => setOwnEngine(path, name)}>
              <ProviderMark id={engineProvider(name).id} size={16} />
            </button>
          {/each}
          <span class="which">{label(e)}{#if !s.engine}<span class="dim">{" · the default"}</span>{/if}</span>
        </div>
        {#if m}
          <Select size="small" label="Model there" value={s.options?.[m.id] !== undefined ? String(s.options[m.id]) : SAME} onchange={(v) => setOwnOption(path, m.id, v)}
            options={[{ value: SAME, label: "Same as everywhere" }, ...m.choices.map((c) => ({ value: String(c.value), label: modelLabel(c), group: c.group ?? undefined }))]} />
        {/if}
        {#if f}
          {@const cur = s.options?.[f.id] !== undefined ? String(s.options[f.id]) : SAME}
          <div class="seg" role="radiogroup" aria-label="Effort there">
            <button role="radio" aria-checked={cur === SAME} class:on={cur === SAME} data-tip="Same as everywhere" onclick={() => setOwnOption(path, f.id, SAME)}>Same</button>
            {#each f.choices.filter((c) => c.value !== "default") as c (c.value)}
              <button role="radio" aria-checked={cur === String(c.value)} class:on={cur === String(c.value)} onclick={() => setOwnOption(path, f.id, String(c.value))}>{c.name}</button>
            {/each}
          </div>
        {/if}
      {/snippet}
    </Exceptions>
  </div>

  <div class="knob">
    <span class="l" data-tip={ONLY}>Auto-compact</span>
    <div class="dial">
      <svg width="58" height="58" viewBox="0 0 58 58" aria-hidden="true">
        <circle cx="29" cy="29" r="23" fill="none" stroke="var(--surface-hover)" stroke-width="5" />
        {#if fill > 0}
          <circle cx="29" cy="29" r="23" fill="none" stroke="var(--text-3)" stroke-width="5" stroke-linecap="round"
            stroke-dasharray="{(fill / 100) * 144.5} 200" transform="rotate(-90 29 29)" />
        {/if}
      </svg>
      <div>
        <div class="big">{compact === "engine" ? "Auto" : compact === "never" ? "Off" : `${compact}%`}</div>
        <div class="dim">{compact === "engine" ? "when the agent finds it full" : compact === "never" ? "only when you press Compact" : "of the context"}</div>
      </div>
    </div>
    <div class="seg" role="radiogroup" aria-label="Auto-compact">
      {#each COMPACT as c (c.value)}
        <button role="radio" aria-checked={compact === c.value} class:on={compact === c.value} disabled={!prefs} data-tip={c.hint} onclick={() => setCompact(c.value)}>{c.label}</button>
      {/each}
    </div>
    {#if prefs}
      <Exceptions items={compactItems} onremove={(p) => setProjectPrefs(p, { compact: null })}
        onadd={(p) => setProjectPrefs(p, { compact: compact === "never" ? "engine" : "never" })}>
        {#snippet editor(path)}
          {@const cur = prefs?.projects?.[path]?.compact ?? compact}
          <div class="seg" role="radiogroup" aria-label="Auto-compact there">
            {#each COMPACT as c (c.value)}
              <button role="radio" aria-checked={cur === c.value} class:on={cur === c.value} data-tip={c.hint} onclick={() => setProjectPrefs(path, { compact: c.value })}>{c.label}</button>
            {/each}
          </div>
        {/snippet}
      </Exceptions>
    {/if}
  </div>
</div>

<style>
  .knobs { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; margin: 18px 0 10px; }
  .knob { border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .knob :global(.select) { width: 100%; }
  .l { font-size: 12px; color: var(--muted); }
  .marks { display: flex; align-items: center; gap: 4px; }
  .marks button { width: 34px; height: 34px; display: inline-flex; align-items: center; justify-content: center; border-radius: var(--r-md); border: 1px solid transparent; opacity: 0.5; }
  .marks button:hover { opacity: 0.9; background: var(--surface-hover); }
  .marks button.on { opacity: 1; border-color: var(--line-strong); background: var(--surface-hover); }
  .which { margin-left: 6px; color: var(--text); font-size: 14px; }
  .marks.small button { width: 28px; height: 28px; }
  .marks.small .which { font-size: 13px; }
  .seg { display: flex; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 2px; gap: 2px; }
  .seg button { flex: 1; padding: 4px 6px; border-radius: var(--r-sm); font-size: 12px; color: var(--text-3); white-space: nowrap; }
  .seg button:hover:not(:disabled) { color: var(--text); }
  .seg button.on { background: var(--surface-hover); color: var(--text); }
  .dial { display: flex; align-items: center; gap: 14px; }
  .dial svg circle { transition: stroke-dasharray var(--dur-move) var(--ease); }
  .big { font-size: 22px; color: var(--text); line-height: 1.2; font-variant-numeric: tabular-nums; }
  .dim { color: var(--muted); font-size: 13px; }
  @media (max-width: 900px) { .knobs { grid-template-columns: 1fr; } }
</style>
