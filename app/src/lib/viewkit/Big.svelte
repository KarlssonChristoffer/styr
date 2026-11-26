<!-- A page big over the window: the same live page as on its card, in a wide panel on a dark veil. ← → step through its versions, Esc or a click beside it closes. The page is not loaded again when it opens big; the frame is the same page served by the same address.
     Used by App.svelte. -->
<script lang="ts">
  import { modal } from "../pop";
  // A page big over the window: the same live page as on its card, in a wide panel on a dark
  // veil. ← → step through its versions, Esc or a click beside it closes. The page is not loaded
  // again when it opens big; the frame is the same page served by the same address.
  import { pageUrl } from "../api";
  import { copy } from "../menu.svelte";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import { fileKind, placeOf } from "../filePreview";
  import { folded, pageVersions, shownIn, type PageItem } from "../transcript";
  import Icon from "../components/Icon.svelte";
  import FilePreview from "../components/FilePreview.svelte";
  import PageFrame from "../components/PageFrame.svelte";
  import { bigView } from "./bigview.svelte";

  const open = $derived(bigView.open);
  const shown = $derived(open ? shownIn(folded(hub.events[open.thread] ?? [])) : []);
  const page = $derived(open?.id ? shown.find((i): i is PageItem => i.kind === "page" && i.id === open.id) : undefined);
  // A file opened big: as it reads, from the thread's folder on whichever machine has it.
  const thread = $derived(open ? hub.threads.find((t) => t.id === open.thread) : undefined);
  const file = $derived(open?.file && thread ? { ...placeOf(open.file, thread.cwd), host: thread.host ?? null, kind: fileKind(open.file) } : null);
  const versions = $derived(page ? pageVersions(shown, page) : []);
  const at = $derived(page ? versions.findIndex((v) => v.id === page.id) : -1);

  let frame = $state<PageFrame>();
  const close = () => (bigView.open = null);
  const go = (by: number) => {
    const next = versions[at + by];
    if (open && next) bigView.open = { thread: open.thread, id: next.id };
  };
  const beside = () => {
    if (!open) return;
    if (page) hub.viewing = { thread: open.thread, kind: "page", id: page.id };
    else if (open.file) hub.viewing = { thread: open.thread, kind: "file", id: open.file };
    else return;
    close();
  };

  $effect(() => {
    if (!open) return;
    const key = (e: KeyboardEvent) => {
      if (e.altKey || e.metaKey || e.ctrlKey) return;
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        close();
      } else if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
        e.preventDefault();
        e.stopPropagation();
        go(e.key === "ArrowLeft" ? -1 : 1);
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  });
  // A page that is gone (the thread was cleared or deleted) closes it.
  $effect(() => {
    if (open && hub.events[open.thread] && !page && !file) close();
  });
</script>

{#if open && (page || file)}
  {@const url = page ? pageUrl(open.thread, page.id) : ""}
  {@const title = page ? page.title : file!.rel}
  <div class="veil" in:modal|global out:modal|global={{ out: true }} role="dialog" aria-modal="true" aria-label={title} tabindex="-1" onclick={(e) => e.target === e.currentTarget && close()} onkeydown={() => {}}>
    <div class="panel">
      <div class="top">
        <span class="what"><Icon name={page ? "page" : "file"} size={12} />{page ? "Page" : "File"}</span>
        <span class="name" data-tip={title}>{title}</span>
        {#if page && versions.length > 1}
          <button class="b" aria-label="Older version" data-tip="Older version (←)" disabled={at <= 0} onclick={() => go(-1)}><Icon name="back" size={13} /></button>
          <span class="count">v{page.version} of {page.of}</span>
          <button class="b" aria-label="Newer version" data-tip="Newer version (→)" disabled={at >= versions.length - 1} onclick={() => go(1)}><Icon name="chevron" size={13} /></button>
        {/if}
        <span class="sp"></span>
        {#if page}<button class="b" aria-label="Load again" data-tip="Load again" onclick={() => frame?.reload()}><Icon name="restart" size={14} /></button>{/if}
        <button class="b" aria-label="Open beside the chat" data-tip="Open beside the chat" onclick={beside}><Icon name="panel" size={14} /></button>
        {#if page}
          <button class="b" aria-label="Open in the browser" data-tip="Open in the browser" onclick={() => openUrl(url).catch((e) => hub.fail(e))}><Icon name="external" size={14} /></button>
          <button class="b" aria-label="Copy the link" data-tip="Copy the link" onclick={() => copy(url)}><Icon name="copy" size={14} /></button>
        {/if}
        <button class="b" aria-label="Close" data-tip="Close (Esc)" onclick={close}><Icon name="x" size={15} /></button>
      </div>
      <div class="frame" class:doc={!!file}>
        {#if page}
          {#key page.id}<PageFrame bind:this={frame} full grow threadId={open.thread} id={page.id} title={page.title} />{/key}
        {:else if file}
          {#key file.rel}<FilePreview root={file.root} rel={file.rel} host={file.host} text={open.text ?? null} fill />{/key}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; z-index: 94; display: grid; place-items: center; background: rgba(9, 9, 8, 0.78); backdrop-filter: blur(3px); }
  .panel { display: flex; flex-direction: column; width: min(1240px, calc(100vw - 96px)); height: calc(100vh - 96px); background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg); overflow: hidden; box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5); }
  .top { flex: none; display: flex; align-items: center; gap: 6px; height: 42px; padding: 0 8px 0 14px; color: var(--muted); font-size: 12px; }
  .what { display: inline-flex; align-items: center; gap: 4px; margin-right: 2px; }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); font-weight: 500; font-size: 13px; }
  .count { color: var(--muted); font-variant-numeric: tabular-nums; }
  .sp { flex: 1; }
  .b { display: grid; place-items: center; width: 28px; height: 28px; border-radius: var(--r-md); color: var(--text-3); }
  .b:hover:not(:disabled) { background: var(--surface-hover); color: var(--text); }
  .b:disabled { opacity: 0.3; }
  /* The page fits the panel and the panel scrolls when it is longer; no scrolling box inside it. */
  .frame { flex: 1; min-height: 0; display: flex; flex-direction: column; overflow-y: auto; background: var(--bg); }
  .frame.doc { padding: 12px 16px; }
</style>
