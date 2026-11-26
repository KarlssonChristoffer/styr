<!-- Settings page: whatsnew. Used by Settings.svelte. -->
<script lang="ts">
  import { sc, open } from "./core.svelte";
  import Quiet from "../Quiet.svelte";
  import { api } from "../../api";
  import { ago, shortVersion } from "../../format";
  import { setPref } from "../../prefs.svelte";
  import { hub } from "../../state.svelte";
  import Markdown from "../Markdown.svelte";
  import Icon from "../Icon.svelte";
  import { stripTags } from "../../patchNotes";

  const about = $derived(sc.about);

  let writing = $state(false);

  let showNotes = $state<Record<string, boolean>>({});

  const current = $derived(hub.notes.find((n) => n.version === hub.update?.version) ?? null);

  const earlier = $derived(hub.notes.filter((n) => n !== current));

  async function writeNotes() {
    writing = true;
    try {
      const notes = await api.writeNotes();
      hub.notes = [notes, ...hub.notes.filter((n) => n.version !== notes.version)];
    } catch (e) {
      hub.fail(e);
    } finally {
      writing = false;
    }
  }

  // Opening the page marks the running version's notes as read.
  $effect(() => {
    if (hub.freshNotes) setPref("seenNotes", hub.freshNotes.version);
  });
</script>

<h1>What's new</h1>
<p class="lead">What changed. Version <span class="mono">{shortVersion(hub.update?.version, "") || (about?.version ?? "…")}</span> is running.</p>
<h2 class="with-action">
  {current ? `In ${shortVersion(current.version)}` : "This version"}
  {#if hub.connected}<button class="link" disabled={writing} onclick={writeNotes}>{writing ? "Writing…" : current ? "Write again" : "Write notes for this version"}</button>{/if}
</h2>
{#if current}
  <section class="notes">
    {#if current.text}<Markdown text={stripTags(current.text)} />{:else}<div class="hint">Commits only.</div>{/if}
    <div class="foot">
      {current.commits.length} commit{current.commits.length === 1 ? "" : "s"}{current.from ? ` since ${shortVersion(current.from)}` : ""} · {ago(current.created_at)}
      <button class="link" onclick={() => (showNotes[current!.version] = !showNotes[current!.version])}>{showNotes[current.version] ? "Hide commits" : "Show commits"}</button>
    </div>
    {#if showNotes[current.version] || !current.text}
      <ul class="commits">{#each current.commits as c (c.hash)}<li><span class="mono dim">{c.hash}</span> {c.subject}</li>{/each}</ul>
    {/if}
  </section>
{:else}
  <section><Quiet kind={writing ? "loading" : "none"} text={writing ? undefined : "No notes"} /></section>
{/if}
{#each earlier as n (n.version)}
  <section class="notes earlier">
    <button class="earlier-head" onclick={() => (showNotes[n.version] = !showNotes[n.version])}>
      <span class="mono">{shortVersion(n.version)}</span><span class="dim">{n.commits.length} commits · {ago(n.created_at)}</span>
      <span class="chev" class:open={showNotes[n.version]}><Icon name="chevron" size={13} /></span>
    </button>
    {#if showNotes[n.version]}
      {#if n.text}<Markdown text={stripTags(n.text)} />{/if}
      <ul class="commits">{#each n.commits as c (c.hash)}<li><span class="mono dim">{c.hash}</span> {c.subject}</li>{/each}</ul>
    {/if}
  </section>
{/each}

<h2>Changelog</h2>
<section class="list">
  {#each about?.changelog ?? [] as c (c.hash)}
    {@const running = hub.update?.version?.startsWith(c.hash) || c.hash.startsWith(hub.update?.version ?? "-")}
    <div class="change" class:running>
      {#if about?.repo_url}
        <button class="hash mono" data-tip="Open on GitHub" onclick={() => open(`${about!.repo_url}/commit/${c.hash}`)}>{c.hash}</button>
      {:else}<span class="hash mono">{c.hash}</span>{/if}
      <span class="subject">{c.subject}</span>
      {#if running}<span class="pill">Running</span>{/if}
      <span class="when">{ago(c.ts)}</span>
    </div>
  {:else}
    {#if about}<Quiet text="No history found next to this build" />{:else}<Quiet kind="loading" />{/if}
  {/each}
</section>

<style>
  /* @kit settings */
</style>
