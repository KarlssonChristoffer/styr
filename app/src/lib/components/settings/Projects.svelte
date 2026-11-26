<!-- Settings page: projects. Used by Settings.svelte. -->
<script lang="ts">
  import { quiet } from "../../quiet";
  import { onMount } from "svelte";
  import { sc, edit, open, openFolder } from "./core.svelte";
  import Quiet from "../Quiet.svelte";
  import { api } from "../../api";
  import type { ToolInfo } from "../../bindings/ToolInfo";
  import { hub } from "../../state.svelte";
  import Field from "../Field.svelte";

  const about = $derived(sc.about);
  const saved = $derived(sc.core?.saved);

  let root = $state("");

  let savedRoot = $state("");

  let github = $state(false);

  let tools = $state<ToolInfo[] | null>(null);

  function loadTools() {
    tools = null;
    api.tools().then((t) => (tools = t)).catch((e) => hub.fail(e));
  }

  async function saveRoot() {
    const value = root.trim();
    if (!value || value === savedRoot) return;
    try {
      await api.setProjectRoot(value);
      savedRoot = root = value;
    } catch (e) {
      hub.fail(e);
    }
  }

  function loadDefaults() {
    quiet(api.projectDefaults().then((d) => ((root = savedRoot = d.root), (github = d.github))), "project defaults");
  }
  onMount(() => {
    loadDefaults();
    loadTools();
  });
  // The core may restart; read again once it is back.
  let wasConnected = hub.connected;
  $effect(() => {
    const now = hub.connected;
    if (now && !wasConnected) {
      loadDefaults();
      loadTools();
    }
    wasConnected = now;
  });
</script>

<h1>Projects and tools</h1>
<section>
  <div class="line">
    <div class="what">
      <div class="name">Folder for new projects</div>
      <div class="hint">Where new projects go.</div>
    </div>
    <div class="control">
      <input class="path mono" bind:value={root} placeholder="D:\Dev" spellcheck="false"
        onkeydown={(e) => e.key === "Enter" && saveRoot()} onblur={saveRoot} />
      {#if root.trim() !== savedRoot}<button class="btn primary" onclick={saveRoot}>Save</button>{/if}
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Editor</div><div class="hint">Empty uses VS Code. <span class="mono">{"{dir}"}</span> marks the folder.</div></div>
    <div class="control">
      <Field value={saved?.editor ?? ""} placeholder="code" width={200} onsave={(v) => edit("open.editor", v)} />
      <button class="btn" disabled={!about} data-tip="Open Hub's data folder with it" onclick={() => about && openFolder(about.data_dir, "editor")}>Try</button>
    </div>
  </div>
  <div class="line">
    <div class="what"><div class="name">Terminal</div><div class="hint">Empty uses Windows Terminal.</div></div>
    <div class="control">
      <Field value={saved?.terminal ?? ""} placeholder="wt -d" width={200} onsave={(v) => edit("open.terminal", v)} />
      <button class="btn" disabled={!about} data-tip="Open Hub's data folder with it" onclick={() => about && openFolder(about.data_dir, "terminal")}>Try</button>
    </div>
  </div>
</section>

<h2 class="with-action">Programs <button class="link" onclick={loadTools}>Check again</button></h2>
<section class="list">
  {#each tools ?? [] as t (t.name)}
    <div class="change tool">
      <span class="tool-name">{t.label}</span>
      <span class="subject">
        {#if t.found}<span class="dim mono">{t.version ?? ""}</span>{/if}
        <span class="dim">{t.detail ?? t.purpose}</span>
      </span>
      <span class="pill" class:ok={t.found} class:off={!t.found}>{t.found ? "Found" : "Missing"}</span>
    </div>
  {:else}
    <Quiet kind="loading" />
  {/each}
</section>

<style>
  /* @kit settings */
</style>
