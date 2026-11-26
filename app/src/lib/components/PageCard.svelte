<!-- A page an agent showed, as it sits in the chat: the live page inside a Viewkit card, in the column like the rest of the answer, with its name above and expand beside it. It carries no toolbar. An older version shrinks to one grey line that opens that version big; the newest version is the card.
     Used by Transcript.svelte. -->
<script lang="ts">
  // A page an agent showed, as it sits in the chat: the live page inside a Viewkit card, in the
  // column like the rest of the answer, with its name above and expand beside it. It carries no
  // toolbar. An older version shrinks to one grey line that opens that version big; the newest
  // version is the card.
  import { pageUrl } from "../api";
  import { copy } from "../menu.svelte";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import type { PageItem } from "../transcript";
  import { grown, showPage, toggleGrown, unfolded, unfoldKey } from "../viewkit/openfile.svelte";
  import Card from "../viewkit/Card.svelte";
  import Icon from "./Icon.svelte";
  import PageFrame from "./PageFrame.svelte";

  let { threadId, page }: { threadId: string; page: PageItem } = $props();

  const older = $derived(page.version < page.of);
  const big = () => showPage(threadId, page.id, { how: "modal" });
  const url = $derived(pageUrl(threadId, page.id));
  const items = () => [
    { label: "Open in the browser", icon: "external" as const, run: () => openUrl(url).catch((e) => hub.fail(e)) },
    { label: "Copy the link", icon: "copy" as const, run: () => copy(url) },
  ];
</script>

{#if older}
  <button class="old" onclick={big} data-tip="Open version {page.version}">
    <Icon name="page" size={12} />
    <span class="name">{page.title}</span>
    <span>v{page.version} · v{page.of} is further down</span>
  </button>
{:else}
  <Card kind="page" name={page.title} meta={page.of > 1 ? `v${page.version} of ${page.of}` : ""} {items}
    onexpand={() => showPage(threadId, page.id)} onpanel={() => showPage(threadId, page.id, { how: "panel" })} onmodal={big}
    unfolded={!!unfolded[unfoldKey(threadId, page.id)]} flat
    grown={!!grown[unfoldKey(threadId, page.id)]} ongrow={() => toggleGrown(threadId, page.id)}>
    {#snippet full()}<PageFrame flush grow flow {threadId} id={page.id} title={page.title} />{/snippet}
    <div class="still"><PageFrame preview {threadId} id={page.id} title={page.title} /></div>
  </Card>
{/if}

<style>
  .still { position: relative; height: 236px; margin: -6px -12px; }
  .old { display: inline-flex; align-items: center; gap: 8px; max-width: 100%; margin: 4px 0 10px; padding: 4px 10px 4px 8px; border-radius: var(--r-md); background: var(--surface); color: var(--muted); font-size: 13px; }
  .old:hover { background: var(--surface-hover); color: var(--text-3); }
  .old .name { color: var(--text-3); font-size: 13px; font-weight: 400; }
</style>
