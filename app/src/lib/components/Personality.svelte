<!-- Settings > Personality: how the agents write to you, everywhere and per project. Kept by the core with compaction (`agent_prefs.rs`) and handed to each agent it starts; only Claude Code can be told so far (its output styles).
     Used by Settings.svelte. -->
<script lang="ts">
  // Settings > Personality: how the agents write to you, everywhere and per project. Kept by the
  // core with compaction (`agent_prefs.rs`) and handed to each agent it starts; only Claude Code
  // can be told so far (its output styles).
  import { untrack } from "svelte";
  import { agent, loadAgentPrefs, saveAgentPrefs, setProjectPrefs, STYLES } from "../agentPrefs.svelte";
  import Exceptions from "./Exceptions.svelte";

  $effect(() => {
    untrack(loadAgentPrefs);
  });
  const prefs = $derived(agent.prefs);
  const style = $derived(prefs?.output_style ?? "");
  const labelOf = (v: string) => STYLES.find((s) => s.value === v)?.label ?? v;
  const items = $derived(Object.entries(prefs?.projects ?? {}).filter(([, p]) => p?.output_style != null).map(([path, p]) => ({ path, summary: labelOf(p!.output_style!) })));
</script>

<div class="knob">
  <span class="l" data-tip="Claude Code follows this. Codex and Hermes keep their own way of writing.">Writing style</span>
  <div class="seg" role="radiogroup" aria-label="Writing style">
    {#each STYLES as st (st.value)}
      <button role="radio" aria-checked={style === st.value} class:on={style === st.value} disabled={!prefs} onclick={() => saveAgentPrefs({ output_style: st.value || null })}>{st.label}</button>
    {/each}
  </div>
  <span class="dim">{STYLES.find((st) => st.value === style)?.hint}</span>
  {#if prefs}
    <Exceptions {items} onremove={(p) => setProjectPrefs(p, { output_style: null })}
      onadd={(p) => setProjectPrefs(p, { output_style: style === "Explanatory" ? "" : "Explanatory" })}>
      {#snippet editor(path)}
        {@const cur = prefs?.projects?.[path]?.output_style ?? style}
        <div class="seg" role="radiogroup" aria-label="Writing style there">
          {#each STYLES as st (st.value)}
            <button role="radio" aria-checked={cur === st.value} class:on={cur === st.value} data-tip={st.hint} onclick={() => setProjectPrefs(path, { output_style: st.value })}>{st.label}</button>
          {/each}
        </div>
      {/snippet}
    </Exceptions>
  {/if}
</div>

<style>
  .knob { border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 12px 14px; display: flex; flex-direction: column; gap: 8px; max-width: 460px; margin-top: 18px; }
  .l { font-size: 12px; color: var(--muted); }
  .seg { display: flex; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 2px; gap: 2px; }
  .seg button { flex: 1; padding: 4px 6px; border-radius: var(--r-sm); font-size: 12px; color: var(--text-3); white-space: nowrap; }
  .seg button:hover:not(:disabled) { color: var(--text); }
  .seg button.on { background: var(--surface-hover); color: var(--text); }
  .dim { color: var(--muted); font-size: 13px; }
</style>
