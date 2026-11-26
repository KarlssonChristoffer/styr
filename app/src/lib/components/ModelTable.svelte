<!-- What each model the engines offer can do, from LiteLLM's public model list. Folded shut in Settings > Engines; open, one compact table per engine with a column per ability. A model that was matched wrong, or not at all, is corrected in the overrides file.
     Used by Settings.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // What each model the engines offer can do, from LiteLLM's public model list. Folded shut in
  // Settings > Engines; open, one compact table per engine with a column per ability. A model
  // that was matched wrong, or not at all, is corrected in the overrides file.
  import { api } from "../api";
  import { ago } from "../format";
  import { hub } from "../state.svelte";
  import { engineProvider, engines } from "../providers.svelte";
  import { openUrl } from "../native";
  import type { ModelTable } from "../bindings/ModelTable";
  import type { ModelCaps } from "../bindings/ModelCaps";
  import ProviderMark from "./ProviderMark.svelte";
  import Icon from "./Icon.svelte";

  let table = $state<ModelTable | null>(null);
  let busy = $state(false);
  let open = $state(false);

  /** The file Hub reads, as a page to look at. */
  const SOURCE = "https://github.com/BerriAI/litellm/blob/main/model_prices_and_context_window.json";

  const load = async (refresh = false) => {
    busy = true;
    try {
      table = await api.models(refresh);
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  };
  $effect(() => {
    load();
  });

  const tokens = (n: number | null) => (n == null ? "" : n >= 1_000_000 ? `${+(n / 1_000_000).toFixed(2)}M` : `${Math.round(n / 1000)}k`);
  const ABILITIES: [keyof ModelCaps, string, string][] = [
    ["image_in", "Images", "Sees images"],
    ["pdf_in", "PDFs", "Reads PDFs"],
    ["tools", "Tools", "Calls tools"],
    ["reasoning", "Thinks", "Thinks before it answers"],
    ["web_search", "Web", "Searches the web"],
  ];
  const folder = (path: string) => path.replace(/[\\/][^\\/]*$/, "");
  /** As the rack above names them. */
  const OWN: Record<string, string> = { claude: "Claude Code", codex: "Codex" };
  // The simulator is not an engine to look at.
  const shown = $derived(table?.engines.filter((e) => e.engine !== "sim") ?? []);
  const count = $derived(shown.reduce((n, e) => n + e.models.length, 0));
  const missing = $derived(shown.reduce((n, e) => n + e.models.filter((m) => !m.found).length, 0));
</script>

<button class="fold" aria-expanded={open} onclick={() => (open = !open)}>
  <span class="title">What the models can do</span>
  <span class="chev" class:open><Icon name="chevron" size={13} /></span>
  {#if table && count}
    <span class="sum">{count} models from {shown.length} engines{#if missing}{` · ${missing} not in the list`}{/if}</span>
  {/if}
</button>

{#if open}
  {#if table}
    {#each shown as e (e.engine)}
      {@const p = engineProvider(e.engine)}
      <section>
        <div class="head">
          <ProviderMark id={p.id} size={14} />
          <span class="name">{engines.labels[e.engine] ?? OWN[e.engine] ?? p.name}</span><span class="mono dim">{e.engine}</span>
          {#if e.draws}<span class="pill" data-tip="Side jobs that make pictures go to this engine (side_jobs.image in hub.json)">Draws pictures</span>{/if}
          {#if !e.models.length}<span class="dim">models show once it has run a thread here</span>{/if}
        </div>
        {#if e.models.length}
          <div class="grid">
            <span class="th">Model</span><span class="th">In the list as</span><span class="th num">Context</span>
            {#each ABILITIES as [, short, long] (short)}<span class="th c" data-tip={long}>{short}</span>{/each}
            {#each e.models as m (m.value)}
              <span class="model" data-tip={m.value}>{m.name}</span>
              <span class="mono dim" class:missing={!m.found}>{m.found ? m.catalog : "not found"}{#if m.corrected}<span class="fixed" data-tip="Set in the overrides file"> · corrected</span>{/if}</span>
              <span class="num dim">{tokens(m.context)}</span>
              {#each ABILITIES as [key, , long] (key)}
                <span class="c">{#if m[key]}<span class="yes" data-tip={long}></span>{:else if m.found}<span class="no"></span>{/if}</span>
              {/each}
            {/each}
          </div>
        {/if}
      </section>
    {/each}
    <div class="foot">
      From <button class="link" onclick={() => openUrl(SOURCE)}>LiteLLM's public model list</button>, {table.known} models{#if table.fetched_at}, fetched {ago(table.fetched_at)}{/if}.
      <button class="link" disabled={busy} onclick={() => load(true)}>{busy ? "Fetching…" : "Fetch again"}</button>
      · Wrong match? <button class="link mono" data-tip={`Correct it there, like { "claude/default": { "catalog": "claude-opus-5-5" } }`} onclick={() => api.openFolder(folder(table!.overrides), "files").catch((e) => hub.fail(e))}>overrides.json</button>
      {#if table.error}<div class="bad">The last fetch failed: {table.error}</div>{/if}
    </div>
  {:else}
    <Quiet kind="loading" />
  {/if}
{/if}

<style>
  .fold { display: flex; align-items: center; gap: 8px; width: 100%; padding: 0; margin: 34px 0 4px; text-align: left; color: var(--text); }
  .fold:hover .title { color: var(--text); }
  .title { font-size: 15px; font-weight: 600; color: var(--text); }
  .sum { color: var(--muted); font-size: 13px; }
  .chev { display: inline-flex; color: var(--muted); transition: transform var(--dur-fast) var(--ease);  }
  .chev.open { transform: rotate(90deg); }
  section { padding: 8px 0; }
  section + section { border-top: 1px solid var(--line); }
  .head { display: flex; align-items: center; gap: 8px; }
  .head + .grid { margin-top: 6px; }
  .name { color: var(--text); font-size: 14px; }
  .dim { color: var(--muted); font-size: 12px; }
  .grid { display: grid; grid-template-columns: minmax(110px, 1fr) minmax(140px, 1.3fr) 56px repeat(5, 52px); gap: 3px 12px; align-items: center; font-size: 13px; }
  .th { color: var(--muted); font-size: 11px; padding-bottom: 2px; }
  .num { text-align: right; }
  .c { text-align: center; }
  .model { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grid > .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .missing { color: var(--amber); }
  .fixed { color: var(--accent); }
  .yes, .no { display: inline-block; width: 6px; height: 6px; border-radius: 50%; vertical-align: middle; }
  .yes { background: var(--text-3); }
  .no { border: 1px solid var(--line-strong); }
  .pill { font-size: 12px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 8px; white-space: nowrap; }
  .foot { color: var(--muted); font-size: 13px; line-height: 1.7; padding: 8px 0 4px; }
  .bad { color: var(--red); }
  .link { color: var(--accent); font-size: inherit; padding: 0 2px; }
  .link:hover { text-decoration: underline; }
  @media (max-width: 700px) {
    .grid { grid-template-columns: 1fr 1fr; }
    .th, .c { display: none; }
  }
</style>
