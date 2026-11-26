<!-- Settings page: engines. Used by Settings.svelte. -->
<script lang="ts">
  import { sc, edit, reloadCore, openFolder, parent } from "./core.svelte";
  import Quiet from "../Quiet.svelte";
  import { hub } from "../../state.svelte";
  import Field from "../Field.svelte";
  import ModelTable from "../ModelTable.svelte";
  import EngineList from "../EngineList.svelte";
  import EngineKnobs from "../EngineKnobs.svelte";
  import LocalModels from "../LocalModels.svelte";
  import ProviderList from "../ProviderList.svelte";

  const core = $derived(sc.core);
  const saved = $derived(sc.core?.saved);


</script>

<h1>Engines</h1>
<p class="lead">
  Who does the work, what new threads start with, and where the models come from. Threads on a schedule are
  <button class="link" onclick={() => ((hub.settingsOpen = false), (hub.view = "routines"))}>Routines</button>.
</p>
{#if saved && core}
  <EngineKnobs engine={saved.default_engine} onengine={(name) => edit("default_engine", name)} />

  <h2>Harnesses</h2>
  <EngineList onchange={reloadCore} />

  <h2>Providers</h2>
  <ProviderList onchange={reloadCore}>
    {#snippet after()}<LocalModels />{/snippet}
  </ProviderList>

  <div class="tidy">
    <span class="name">Model for tidying</span>
    <span class="hint" data-tip="Tidying a long message and writing handoffs use the first model whose name contains this">tidies long messages, writes handoffs</span>
    <span class="grow"></span>
    <Field value={saved.tidy_model ?? ""} placeholder="haiku" width={140} onsave={(v) => edit("tidy.model", v)} />
    <button class="link mono small" data-tip={`Open the folder of ${core.path}; other settings there can be edited by hand`} onclick={() => openFolder(parent(core!.path))}>hub.json</button>
  </div>
{:else}
  <section><Quiet kind="loading" /></section>
{/if}

<ModelTable />

<style>
  /* @kit settings */
</style>
