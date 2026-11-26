<!-- A plugin the agent found and suggested in the thread (`suggest_plugin`): the repository as it was when suggested, the agent's reason, and a way to open it in Plugins.
     Used by Transcript.svelte. -->
<script lang="ts">
  // A plugin the agent found and suggested in the thread (`suggest_plugin`): the repository as it
  // was when suggested, the agent's reason, and a way to open it in Plugins.
  import type { PluginItem } from "../transcript";
  import { count, customs, kindLabel, weekLabel } from "../plugins";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { item }: { item: PluginItem } = $props();
  const r = $derived(item.repo);
  const week = $derived(weekLabel(r));
  const stamp = $derived(customs(r));
</script>

<div class="found">
  <div class="head"><Icon name="puzzle" size={12} />Found a plugin</div>
  <div class="card">
    {#if r.avatar}<img class="av" src={r.avatar} alt="" />{:else}<span class="av"></span>{/if}
    <div class="body">
      <div class="t"><button class="nm" onclick={() => hub.openPlugins(r.full_name)}>{r.name}</button><span class="kind">{kindLabel(r.kind)}</span><span class="own">{r.owner}</span></div>
      <div class="why">{item.why || r.description}</div>
      <div class="meta">
        {#if week}<span class="gain">{week} this week</span>{/if}
        <span>★ {count(r.stars)}</span>
        <span class="st {stamp.tone}"><i></i>{stamp.text}</span>
      </div>
    </div>
    <div class="acts">
      <button class="btn" onclick={() => hub.openPlugins(r.full_name)}>Open</button>
      <button class="btn primary" disabled data-tip="Trying a plugin in a thread comes with installing, the next step">Try it here</button>
    </div>
  </div>
</div>

<style>
  .found { margin: 6px 0 10px; }
  .head { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; margin-bottom: 4px; }
  .card { display: flex; gap: 12px; align-items: flex-start; border: 1px solid var(--line); background: var(--surface); border-radius: var(--r-lg); padding: 11px 12px; }
  .av { width: 34px; height: 34px; border-radius: var(--r-md); flex: none; border: 1px solid var(--line-strong); background: var(--surface-hover); object-fit: cover; }
  .body { flex: 1; min-width: 0; }
  .t { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .nm { color: var(--text); font-weight: 500; font-size: 14px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .nm:hover { text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--line-strong); }
  .kind { font-size: 11px; border: 1px solid var(--line-strong); border-radius: var(--r-xs); padding: 0 4px; color: var(--muted); flex: none; }
  .own { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .why { color: var(--text-3); font-size: 13px; margin-top: 2px; line-height: 1.5; }
  .meta { display: flex; gap: 12px; color: var(--muted); font-size: 12px; margin-top: 6px; align-items: center; flex-wrap: wrap; }
  .gain { color: var(--green); }
  .st { display: inline-flex; gap: 4px; align-items: center; }
  .st i { width: 7px; height: 7px; border-radius: 50%; border: 1.5px solid currentColor; }
  .st.ok { color: var(--green); }
  .st.held { color: var(--amber); }
  .st.no { color: var(--red); }
  .st.ok i, .st.held i, .st.no i { background: currentColor; }
  .acts { display: flex; gap: 6px; flex: none; align-self: center; }
  .btn { height: 26px; padding: 0 11px; font-size: 13px; }
</style>
