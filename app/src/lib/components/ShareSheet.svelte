<!-- Where a picture can go, opened from the viewer's Share. Hub's own places come first, as a row of round icons; the system's share sheet (Discord, Teams and the like) is one icon of its own; the plain actions sit in a grid. "Send to thread…" turns the sheet to a page of its own, to pick from every thread. Nothing it puts in a message box is sent until you send it.
     Used by Lightbox.svelte, Viewer.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import { pop } from "../popover";
  import Quiet from "./Quiet.svelte";
  // Where a picture can go, opened from the viewer's Share. Hub's own places come first, as a row
  // of round icons; the system's share sheet (Discord, Teams and the like) is one icon of its own;
  // the plain actions sit in a grid. "Send to thread…" turns the sheet to a page of its own, to
  // pick from every thread. Nothing it puts in a message box is sent until you send it.
  import { api } from "../api";
  import type { Attachment } from "../bindings/Attachment";
  import type { Thread } from "../bindings/Thread";
  import { folderName } from "../format";
  import { copy } from "../menu.svelte";
  import { copyImage, dataBlob, reveal } from "../menus.svelte";
  import { hub } from "../state.svelte";
  import type { ImageItem } from "../transcript";
  import Icon from "./Icon.svelte";

  let {
    image,
    name,
    threadId,
    onclose,
    onsaved,
  }: {
    image: ImageItem;
    name: string;
    threadId: string;
    onclose: () => void;
    /** Something was saved on disk: what to say, and where it went. */
    onsaved: (text: string, path: string) => void;
  } = $props();

  const here = $derived(hub.threads.find((t) => t.id === threadId));
  const blob = $derived(dataBlob(image.src));
  const fileName = $derived(name.includes(".") ? name : `${name}.${blob.type.split("/")[1] ?? "png"}`);
  // Its size and shape, for the head of the sheet.
  const size = $derived(blob.size < 1024 * 1024 ? `${Math.round(blob.size / 1024)} KB` : `${(blob.size / 1024 / 1024).toFixed(1)} MB`);
  let dims = $state("");

  const attachment = (): Attachment => ({ name: fileName, mime: blob.type, data: image.src.slice(image.src.indexOf(",") + 1) });
  const file = () => new File([blob], fileName, { type: blob.type });
  const done = (f: () => unknown) => async () => {
    try {
      await f();
      onclose();
    } catch (e) {
      hub.fail(e);
    }
  };

  // The system's sheet, where the system has one.
  const canShare = typeof navigator.canShare === "function" && navigator.canShare({ files: [new File([""], "x.png", { type: "image/png" })] });

  // ---- Send to thread ----

  let page = $state<"share" | "threads">("share");
  let query = $state("");
  let picked = $state<string | null>(null);
  let search = $state<HTMLInputElement>();
  const others = $derived(hub.threads.filter((t) => hub.inWorld(t) && t.id !== threadId && !t.archived));
  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? others.filter((t) => `${t.title} ${folderName(t.cwd)}`.toLowerCase().includes(q)) : others;
  });
  const busy = (t: Thread) => t.status === "running" || t.status === "waiting_on_you";
  const none = $derived(!matches.length);
  const groups = $derived([
    { label: "Working now", threads: matches.filter(busy) },
    { label: query.trim() ? "Others" : "Recent", threads: matches.filter((t) => !busy(t)).slice(0, query.trim() ? 50 : 12) },
  ]);
  function toThreads() {
    page = "threads";
    picked = null;
    queueMicrotask(() => search?.focus());
  }
  const send = (id: string) => {
    hub.stageIn(id, [attachment()]);
    onclose();
  };

  // Esc steps back from the thread page, then closes; a click outside closes.
  let root = $state<HTMLDivElement>();
  $effect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      if (page === "threads") page = "share";
      else onclose();
    };
    // The Share button toggles the sheet itself.
    const outside = (e: PointerEvent) =>
      root && !root.contains(e.target as Node) && !(e.target as Element).closest?.("[data-share]") && onclose();
    window.addEventListener("keydown", key, true);
    window.addEventListener("pointerdown", outside, true);
    return () => {
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("pointerdown", outside, true);
    };
  });
</script>

<div class="sheet" transition:pop|global bind:this={root} role="dialog" aria-label="Share">
  {#if page === "share"}
    <div class="head">
      <img src={image.src} alt="" onload={(e) => {
        const img = e.currentTarget as HTMLImageElement;
        dims = `${img.naturalWidth}×${img.naturalHeight}`;
      }} />
      <div class="what">
        <div class="file" data-tip={fileName}>{fileName}</div>
        <div class="meta">{blob.type.split("/")[1]?.toUpperCase()} · {size}{dims ? ` · ${dims}` : ""}</div>
      </div>
      <button class="x" data-tip="Close (Esc)" onclick={onclose}><Icon name="x" size={13} /></button>
    </div>

    <div class="label">In Hub</div>
    <div class="row">
      <button class="target" onclick={toThreads}><span class="icon blue"><Icon name="threads" size={17} /></span>Send to thread…</button>
      <button class="target" onclick={() => (hub.startDraft(here?.cwd ?? "", "", here?.host ?? null, [attachment()]), onclose())}><span class="icon green"><Icon name="plus" size={17} /></span>New thread</button>
      <button class="target" onclick={() => (hub.startDraft("", "", null, [attachment()], true), onclose())}><span class="icon purple"><Icon name="threads" size={17} /></span>Ask about everything</button>
      {#if here && !here.host}
        <button class="target" onclick={done(async () => onsaved("Added to docs/images", (await api.addImage(here.cwd, fileName, file())).path))}><span class="icon amber"><Icon name="folder" size={17} /></span>Add to project</button>
      {/if}
      <button class="target" onclick={() => send(threadId)}><span class="icon"><Icon name="clip" size={17} /></span>Reply here</button>
    </div>

    {#if canShare}
      <div class="rule"></div>
      <div class="label">Other apps</div>
      <div class="row">
        <button class="target" onclick={() => (quiet(navigator.share({ files: [file()] }), "share"), onclose())}><span class="icon square"><Icon name="share" size={17} /></span>Share…</button>
      </div>
    {/if}

    <div class="rule"></div>
    <div class="grid">
      <button class="action" onclick={done(() => copyImage(image.src))}><Icon name="copy" size={14} />Copy image</button>
      <button class="action" onclick={done(async () => onsaved("Saved to Downloads", (await api.download(fileName, file())).path))}><Icon name="download" size={14} />Download</button>
      {#if image.path}
        <button class="action" onclick={done(() => copy(image.path!))}><Icon name="link" size={14} />Copy path</button>
        {#if !here?.host}<button class="action" onclick={done(reveal(image.path!))}><Icon name="folder" size={14} />Show in folder</button>{/if}
      {/if}
    </div>
  {:else}
    <div class="head nav">
      <button class="back" onclick={() => (page = "share")}><span class="flip"><Icon name="chevron" size={13} /></span>Back</button>
      <span class="title">Send to thread</span>
      <button class="x" data-tip="Close (Esc)" onclick={onclose}><Icon name="x" size={13} /></button>
    </div>
    <label class="search"><Icon name="search" size={13} /><input bind:this={search} bind:value={query} placeholder="Search threads and projects…" spellcheck="false" /></label>
    <div class="list">
      {#each groups as g (g.label)}
        {#if g.threads.length}
          <div class="label">{g.label}</div>
          {#each g.threads as t (t.id)}
            <button class="thread" class:on={picked === t.id} onclick={() => (picked = t.id)} ondblclick={() => send(t.id)}>
              <span class="dot" class:live={busy(t)}></span>
              <span class="tname">{t.title}</span>
              <span class="folder">{folderName(t.cwd)}</span>
            </button>
          {/each}
        {/if}
      {/each}
      {#if none}<Quiet text="No thread matches" />{/if}
    </div>
    <button class="go" disabled={!picked} onclick={() => picked && send(picked)}>Put in its message box</button>
    <p class="note">Opens the thread with the picture attached.</p>
  {/if}
</div>

<style>
  .sheet {
    position: absolute; top: 40px; right: 8px; z-index: 20; width: min(340px, calc(100% - 16px));
    display: flex; flex-direction: column; gap: 10px; padding: 14px;
    background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.45);
  }
  .head { display: flex; align-items: center; gap: 10px; }
  .head img { width: 48px; height: 36px; object-fit: cover; border-radius: var(--r-md); flex: none; }
  .what { flex: 1; min-width: 0; }
  .file { color: var(--text); font-size: 13px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { color: var(--muted); font-size: 12px; }
  .x { display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--r-sm); color: var(--muted); flex: none; }
  .x:hover { background: var(--surface-hover); color: var(--text); }
  .label { color: var(--muted); font-size: 12px; margin-bottom: -4px; }
  .row { display: flex; gap: 4px; overflow-x: auto; }
  .target { width: 60px; flex: none; display: grid; justify-items: center; gap: 4px; padding: 4px 0; border-radius: var(--r-lg); color: var(--text-3); font-size: 11px; line-height: 1.2; text-align: center; }
  .target:hover { color: var(--text); }
  .target:hover .icon { border-color: var(--line-strong); filter: brightness(1.15); }
  .icon { width: 44px; height: 44px; border-radius: 50%; display: grid; place-items: center; color: var(--text-2); background: var(--surface-2); border: 1px solid var(--line); }
  .icon.square { border-radius: var(--r-lg); }
  .icon.blue { background: color-mix(in srgb, var(--p-blue) 16%, transparent); color: var(--p-blue); }
  .icon.green { background: color-mix(in srgb, var(--green) 16%, transparent); color: var(--green); }
  .icon.purple { background: color-mix(in srgb, var(--p-violet) 16%, transparent); color: var(--p-violet); }
  .icon.amber { background: color-mix(in srgb, var(--amber) 16%, transparent); color: var(--amber); }
  .rule { height: 1px; background: var(--line); }
  .note { margin: -4px 0 0; color: var(--muted); font-size: 12px; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  .action { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border-radius: var(--r-md); border: 1px solid var(--line); color: var(--text-3); font-size: 13px; text-align: left; }
  .action:hover { background: var(--surface-hover); color: var(--text); }
  .nav .title { flex: 1; text-align: center; color: var(--text); font-size: 13px; font-weight: 500; }
  .back { display: flex; align-items: center; gap: 2px; color: var(--muted); font-size: 12px; }
  .back:hover { color: var(--text); }
  .flip { display: grid; transform: rotate(180deg); }
  .search { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-radius: var(--r-md); border: 1px solid var(--line); color: var(--muted); }
  .search:focus-within { border-color: var(--accent); }
  .search input { flex: 1; min-width: 0; background: none; border: 0; outline: 0; color: var(--text); font-size: 13px; }
  .list { display: flex; flex-direction: column; gap: 2px; max-height: 300px; overflow-y: auto; }
  .list .label { margin: 6px 0 2px; }
  .thread { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: var(--r-md); text-align: left; }
  .thread:hover { background: var(--surface-hover); }
  .thread.on { background: var(--accent-bg); }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--line-strong); flex: none; }
  .dot.live { background: var(--green); }
  .tname { flex: 1; min-width: 0; color: var(--text-2); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .folder { color: var(--muted); font-size: 11px; flex: none; }
  .go { padding: 8px; border-radius: var(--r-md); background: var(--accent); color: var(--bg); font-size: 13px; font-weight: 600; }
  .go:disabled { opacity: 0.4; }
</style>
