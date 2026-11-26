<!-- What an agent showed, big in the right panel: a live page or a picture, or a dev server the thread runs (a picture opens over the whole window, Lightbox.svelte; it shows here only when popped out). One frame for every kind, with its name, where it is among the others of its kind, full screen and close. Each kind adds its own tools to the bar and fills the body. A page the agent changed has versions, and the bar switches between them. Things in the context bag show here too, and what is shown here can go into the bag. Any file a reply named or changed opens here wide too, read as what it is (FilePreview).
     Used by App.svelte, ViewerWindow.svelte. -->
<script lang="ts">
  // What an agent showed, big in the right panel: a live page or a picture, or a dev server the
  // thread runs (a picture opens over the whole window, Lightbox.svelte; it shows here only when
  // popped out). One frame for every kind, with its name, where it is among the others of its
  // kind, full screen and close. Each kind adds its own tools to the bar and fills the body. A
  // page the agent changed has versions, and the bar switches between them. Things in the context
  // bag show here too, and what is shown here can go into the bag. Any file a reply named or changed
  // opens here wide too, read as what it is (FilePreview).
  import { api, pageUrl } from "../api";
  import { bag, here, intoThread, putInBag, takeFromBag } from "../bag.svelte";
  import type { BagItem } from "../bindings/BagItem";
  import { copyImage, dataBlob, reveal } from "../menus.svelte";
  import { contextMenu, copy } from "../menu.svelte";
  import { closeWindow, openUrl, popOut } from "../native";
  import { hub, type Viewing } from "../state.svelte";
  import { pictures } from "../lightbox.svelte";
  import { previewable, previewUrl } from "../preview";
  import { folded, latestShown, pageVersions, shownIn, shownName, type ImageItem, type ShownItem } from "../transcript";
  import BrowserFrame from "./BrowserFrame.svelte";
  import FilePreview from "./FilePreview.svelte";
  import { placeOf } from "../filePreview";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import PageFrame from "./PageFrame.svelte";
  import ShareSheet from "./ShareSheet.svelte";

  let { viewing, popped = false }: { viewing: Viewing; /** In a window of its own, popped out of the main one. */ popped?: boolean } = $props();

  const shown = $derived(shownIn(folded(hub.events[viewing.thread] ?? [])));
  const item = $derived<ShownItem | undefined>(shown.find((i) => i.kind === viewing.kind && i.id === viewing.id));
  // Pages step from page to page, each at its newest version; the version bar steps within one.
  const same = $derived(
    viewing.kind === "page" ? latestShown(shown).filter((i) => i.kind === "page") : shown.filter((i) => i.kind === viewing.kind),
  );
  const at = $derived(same.findIndex((i) => i.id === viewing.id || (item?.kind === "page" && i.kind === "page" && i.family === item.family)));
  const versions = $derived(item?.kind === "page" ? pageVersions(shown, item) : []);
  // ---- The context bag ----

  // The item with its content, fetched when it is shown.
  let held = $state<BagItem | null>(null);
  let holdError = $state("");
  $effect(() => {
    const id = viewing.kind === "bag" ? viewing.id : null;
    held = null;
    holdError = "";
    if (id) api.bagItem(id).then((b) => viewing.id === id && (held = b)).catch((e) => (holdError = e instanceof Error ? e.message : String(e)));
  });
  const heldImage = $derived(held?.item.data && held.item.mime.startsWith("image/") ? `data:${held.item.mime};base64,${held.item.data}` : null);
  const bagSame = $derived(viewing.kind === "bag" ? bag.items.filter((b) => !b.item.url) : []);
  const from = $derived(here(viewing.thread));
  function putShownInBag() {
    if (item?.kind === "image") {
      const blob = item.src.startsWith("data:") ? item.src : null;
      if (!blob) return;
      const mime = blob.slice(5, blob.indexOf(";"));
      const named = name.includes(".") ? name : `${name}.${mime.split("/")[1] ?? "png"}`;
      return putInBag({ name: named, mime, data: blob.slice(blob.indexOf(",") + 1) }, from);
    }
    if (viewing.kind === "browser") return putInBag({ name: viewing.id.replace(/^https?:\/\//, ""), mime: "text/uri-list", url: viewing.id }, from);
  }

  // ---- Files ----
  const thread = $derived(hub.threads.find((t) => t.id === viewing.thread));
  const place = $derived(viewing.kind === "file" ? placeOf(viewing.id, thread?.cwd ?? "") : null);
  const fullPath = $derived(place ? `${place.root.replace(/[\\/]+$/, "")}/${place.rel}` : "");
  let pathCopied = $state(false);

  const name = $derived(viewing.kind === "file" ? (place?.rel ?? viewing.id) : viewing.kind === "bag" ? (held?.item.name ?? bag.items.find((b) => b.id === viewing.id)?.item.name ?? "") : item ? shownName(item, shown) : "");
  let full = $state(false);
  let page = $state<PageFrame>();
  let browser = $state<BrowserFrame>();

  // ---- Dev servers ----

  // The addresses gone to here, for back and forward; what the server's own links do inside the
  // frame stays its own.
  let trail = $state<string[]>([]);
  let step = $state(0);
  $effect(() => {
    if (viewing.kind !== "browser") return;
    const url = viewing.id;
    if (trail[step] === url) return;
    trail = [...trail.slice(0, step + 1).filter(Boolean), url];
    step = trail.length - 1;
  });
  let address = $state("");
  $effect(() => {
    address = viewing.kind === "browser" ? viewing.id : "";
  });
  function goTo(typed: string) {
    const url = previewUrl(typed);
    if (!url) return;
    if (!previewable(url)) return void openUrl(url).catch((e) => hub.fail(e));
    if (url === viewing.id) browser?.reload();
    else hub.viewing = { ...viewing, id: url };
  }
  function walk(by: number) {
    const url = trail[step + by];
    if (!url) return;
    step += by;
    hub.viewing = { ...viewing, id: url };
  }

  const go = (step: number) => {
    const list = viewing.kind === "bag" ? bagSame : same;
    const k = list.findIndex((i) => i.id === viewing.id);
    const next = list[k + step];
    if (next) hub.viewing = { ...viewing, id: next.id };
  };
  const bagAt = $derived(bagSame.findIndex((b) => b.id === viewing.id));
  // Popped out, closing closes the window; in the panel, the panel goes back to the thread's.
  const close = () => (popped ? void closeWindow() : (hub.viewing = null));
  function popOutHere() {
    popOut($state.snapshot(viewing)).catch((e) => hub.fail(e));
    hub.viewing = null;
  }

  // Arrows step through the others of the kind; Esc leaves full screen before it closes.
  const typing = (el: EventTarget | null) =>
    el instanceof HTMLElement && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName));
  $effect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.altKey || e.ctrlKey || e.metaKey || typing(e.target) || sharing || pictures.open) return;
      if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && viewing.kind !== "file") {
        e.preventDefault();
        go(e.key === "ArrowLeft" ? -1 : 1);
      } else if (e.key === "Escape" && (full || popped)) {
        e.preventDefault();
        if (full) full = false;
        else close();
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  });

  // ---- Pictures ----

  const file = (image: ImageItem) => {
    const blob = dataBlob(image.src);
    return new File([blob], name.includes(".") ? name : `${name}.${blob.type.split("/")[1] ?? "png"}`, { type: blob.type });
  };
  // Where a picture was saved, said in the bar for a while with a way to it. The desktop app has
  // no download bar, so the core saves downloads in Downloads.
  let saved = $state<{ text: string; path: string } | null>(null);
  let savedTimer: ReturnType<typeof setTimeout> | undefined;
  function said(text: string, path: string) {
    saved = { text, path };
    clearTimeout(savedTimer);
    savedTimer = setTimeout(() => (saved = null), 6000);
  }
  async function download(image: ImageItem) {
    const f = file(image);
    said("Saved to Downloads", (await api.download(f.name, f)).path);
  }

  // A long prompt shows its first lines; the rest opens below, scrolling when it is long.
  let more = $state(false);
  $effect(() => {
    void viewing.id;
    more = false;
  });
  // The share sheet, over the picture; it closes when you step to another.
  let sharing = $state(false);
  $effect(() => {
    void viewing.id;
    sharing = false;
  });
  const fail = (f: () => Promise<unknown>) => () => f().catch((e) => hub.fail(e));

  // ---- A page's ⋮: what used to be buttons on its bar. ----
  const pageMenuOpen = $derived(!!contextMenu.open && contextMenu.open.from instanceof HTMLElement && !!contextMenu.open.from.closest(".viewer header"));
  function pageMenu(e: MouseEvent) {
    if (item?.kind !== "page") return;
    const url = pageUrl(viewing.thread, item.id);
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const z = Number(document.documentElement.style.zoom) || 1;
    contextMenu.show(r.right / z - 230, r.bottom / z + 4, [
      { label: "Reload", icon: "restart", run: () => page?.reload() },
      { label: "Open in the browser", icon: "external", run: fail(() => openUrl(url)) },
      { label: "Copy the link", icon: "copy", run: () => copy(url) },
      null,
      !popped && { label: "Open in its own window", icon: "popout", run: popOutHere },
      !popped && { label: full ? "Leave full screen" : "Full screen", icon: "expand", hint: full ? "Esc" : undefined, run: () => (full = !full) },
    ]);
  }
</script>

<div class="viewer" class:full>
  <header>
    <Icon name={viewing.kind === "page" ? "page" : viewing.kind === "browser" ? "globe" : viewing.kind === "bag" ? "bag" : viewing.kind === "file" ? "file" : "image"} size={13} />
    {#if viewing.kind === "browser"}
      <span class="nav">
        <button data-tip="Back" disabled={step <= 0} onclick={() => walk(-1)}><span class="back"><Icon name="chevron" size={13} /></span></button>
        <button data-tip="Forward" disabled={step >= trail.length - 1} onclick={() => walk(1)}><Icon name="chevron" size={13} /></button>
      </span>
      <input
        class="address mono"
        aria-label="Address"
        spellcheck="false"
        bind:value={address}
        onkeydown={(e) => {
          if (e.key === "Enter") goTo(address);
          else if (e.key === "Escape") (address = viewing.id), (e.currentTarget as HTMLInputElement).blur();
        }}
        onfocus={(e) => e.currentTarget.select()}
      />
    {:else}
      <span class="name" data-tip={name}>{name || "Not found"}</span>
    {/if}
    {#if versions.length > 1 && item?.kind === "page"}
      {@const k = versions.findIndex((v) => v.id === item.id)}
      <span class="nav" role="group" aria-label="Versions">
        <button data-tip="The version before" aria-label="The version before" disabled={k <= 0} onclick={() => (hub.viewing = { ...viewing, id: versions[k - 1].id })}><span class="back"><Icon name="chevron" size={13} /></span></button>
        <span class="count" data-tip={k === versions.length - 1 ? "The newest version" : "A newer version is further down the chat"}>v{k + 1} of {versions.length}</span>
        <button data-tip="The version after" aria-label="The version after" disabled={k >= versions.length - 1} onclick={() => (hub.viewing = { ...viewing, id: versions[k + 1].id })}><Icon name="chevron" size={13} /></button>
      </span>
    {/if}
    {#if viewing.kind === "bag" && bagSame.length > 1 && bagAt >= 0}
      <span class="nav">
        <button title="Previous (←)" disabled={bagAt <= 0} onclick={() => go(-1)}><span class="back"><Icon name="chevron" size={13} /></span></button>
        <span class="count">{bagAt + 1} of {bagSame.length}</span>
        <button title="Next (→)" disabled={bagAt >= bagSame.length - 1} onclick={() => go(1)}><Icon name="chevron" size={13} /></button>
      </span>
    {:else if same.length > 1 && viewing.kind !== "browser" && viewing.kind !== "bag" && viewing.kind !== "page"}
      <span class="nav">
        <button data-tip="Previous (←)" disabled={at <= 0} onclick={() => go(-1)}><span class="back"><Icon name="chevron" size={13} /></span></button>
        <span class="count">{at + 1} of {same.length}</span>
        <button data-tip="Next (→)" disabled={at >= same.length - 1} onclick={() => go(1)}><Icon name="chevron" size={13} /></button>
      </span>
    {/if}
    <span class="tools">
      {#if item?.kind === "image"}
        {#if saved}
          <button class="saved" data-tip={saved.path} onclick={reveal(saved.path)}>{saved.text} · Show</button>
        {:else}
          <button data-tip="Download to Downloads" onclick={fail(() => download(item))}><Icon name="download" size={13} /></button>
        {/if}
        <button data-tip="Copy image" onclick={fail(() => copyImage(item.src))}><Icon name="copy" size={13} /></button>
        <button data-tip="Share" data-share class:on={sharing} onclick={() => (sharing = !sharing)}><Icon name="share" size={13} /></button>
        {#if item.path && !thread?.host}<button data-tip="Show in folder" onclick={reveal(item.path)}><Icon name="folder" size={13} /></button>{/if}
        <button data-tip="Put it in the context bag, for other threads" onclick={fail(async () => putShownInBag())}><Icon name="bag" size={13} /></button>
      {:else if item?.kind === "page"}
        <!-- A page keeps its bar to the name, the versions and ⋮, as Claude Code's artifact panel. -->
        <button data-tip="More" aria-label="More" class:on={pageMenuOpen} onclick={pageMenu}><Icon name="more" size={14} /></button>
      {:else if viewing.kind === "browser"}
        <button data-tip="Reload" onclick={() => browser?.reload()}><Icon name="restart" size={13} /></button>
        <button data-tip="Open in your browser" onclick={() => openUrl(viewing.id)}><Icon name="external" size={13} /></button>
        <button data-tip="Put the address in the context bag" onclick={fail(async () => putShownInBag())}><Icon name="bag" size={13} /></button>
      {:else if viewing.kind === "file"}
        <button data-tip={pathCopied ? "Copied" : "Copy the path"} onclick={fail(async () => { await copy(fullPath); pathCopied = true; setTimeout(() => (pathCopied = false), 1200); })}><IconSwap name={pathCopied ? "check" : "copy"} size={13} /></button>
        {#if !thread?.host}<button data-tip="Show in folder" onclick={reveal(fullPath)}><Icon name="folder" size={13} /></button>{/if}
      {:else if viewing.kind === "bag" && held}
        {@const id = held.id}
        <button data-tip="Add it to the message in this thread" onclick={() => intoThread(viewing.thread, [id])}><Icon name="plus" size={13} /></button>
        {#if heldImage}<button data-tip="Copy image" onclick={fail(() => copyImage(heldImage))}><Icon name="copy" size={13} /></button>{/if}
        {#if held.item.path}<button data-tip="Show in folder" onclick={reveal(held.item.path)}><Icon name="folder" size={13} /></button>{/if}
        <button data-tip="Take it out of the bag" onclick={fail(async () => { await takeFromBag(id); close(); })}><Icon name="x" size={13} /></button>
        <span class="sep"></span>
      {/if}
      {#if item?.kind !== "page"}<span class="sep"></span>{/if}
      {#if !popped && item?.kind !== "page"}
        <button data-tip="Pop out into its own window" onclick={popOutHere}><Icon name="popout" size={13} /></button>
        <button data-tip={full ? "Leave full screen (Esc)" : "Full screen"} onclick={() => (full = !full)}><Icon name="expand" size={13} /></button>
      {/if}
      <button data-tip="Close (Esc)" onclick={close}><Icon name="x" size={13} /></button>
    </span>
  </header>

  {#if sharing && item?.kind === "image"}
    <ShareSheet image={item} {name} threadId={viewing.thread} onclose={() => (sharing = false)} onsaved={said} />
  {/if}

  <div class="body">
    {#if viewing.kind === "file" && place}
      <div class="file">{#key viewing.id}<FilePreview root={place.root} rel={place.rel} host={thread?.host ?? null} fill />{/key}</div>
    {:else if viewing.kind === "browser"}
      <BrowserFrame bind:this={browser} url={viewing.id} />
    {:else if viewing.kind === "bag"}
      {#if heldImage}
        <div class="picture"><img src={heldImage} alt={name} /></div>
      {:else if held?.item.text != null}
        <pre class="held mono">{held.item.text}</pre>
      {:else if held}
        <p class="gone">{held.item.path ? `A file on this machine: ${held.item.path}` : `${held.item.name} has no preview.`} Add it to the message to send it along.</p>
      {:else if holdError}
        <p class="gone">This is no longer in the bag.</p>
      {/if}
      {#if held?.from}<p class="from">Put in the bag from {held.from}</p>{/if}
    {:else if item?.kind === "page"}
      {#key item.id}<PageFrame bind:this={page} full threadId={viewing.thread} id={item.id} title={item.title} />{/key}
    {:else if item?.kind === "image"}
      <div class="picture"><img src={item.src} alt={item.prompt ?? name} /></div>
      {#if item.prompt}
        <div class="prompt" class:more>
          <p>{item.prompt}</p>
          {#if item.prompt.length > 160}<button onclick={() => (more = !more)}>{more ? "Less" : "More"}</button>{/if}
        </div>
      {/if}
      {#if same.length > 1}
        <div class="strip">
          {#each same as other, k (other.id)}
            {#if other.kind === "image"}
              <button class:on={k === at} data-tip={shownName(other, shown)} onclick={() => (hub.viewing = { ...viewing, id: other.id })}>
                <img src={other.src} alt="" />
              </button>
            {/if}
          {/each}
        </div>
      {/if}
    {:else}
      <p class="gone">This is no longer in the thread.</p>
    {/if}
  </div>
</div>

<style>
  .viewer { position: relative; flex: 1; display: flex; flex-direction: column; min-height: 0; background: var(--bg); }
  .viewer.full { position: fixed; inset: 0; z-index: 90; }
  header { display: flex; align-items: center; gap: 6px; height: 40px; flex: none; padding: 0 8px 0 14px; color: var(--muted); font-size: 13px; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); }
  .nav, .tools { display: flex; align-items: center; gap: 2px; }
  .address { flex: 1; min-width: 0; height: 24px; padding: 0 8px; border-radius: var(--r-sm); border: 1px solid transparent; background: var(--surface); color: var(--text-2); font-size: 12px; }
  .address:focus { outline: none; border-color: var(--line-strong); color: var(--text); }
  .count { color: var(--text-3); font-variant-numeric: tabular-nums; padding: 0 2px; }
  .back { display: grid; transform: rotate(180deg); }
  .sep { width: 1px; height: 14px; background: var(--line); margin: 0 4px; }
  header button { display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--r-sm); color: var(--muted); }
  header button:hover:not(:disabled) { background: var(--surface-hover); color: var(--text); }
  header button:disabled { opacity: 0.35; }
  header button.on { background: var(--surface-hover); color: var(--text); }
  /* Each kind fills the body; a page's frame takes all the height there is. */
  .body { flex: 1; display: flex; flex-direction: column; min-height: 0; }
  .file { flex: 1; display: flex; flex-direction: column; min-height: 0; padding: 10px 14px; overflow: auto; }
  /* The picture gets what the prompt and strip leave, and is fitted inside it, so nothing overlaps. */
  .picture { flex: 1; min-height: 120px; position: relative; }
  .picture img { position: absolute; inset: 16px; width: calc(100% - 32px); height: calc(100% - 32px); object-fit: contain; }
  .prompt { flex: none; display: flex; align-items: flex-start; gap: 8px; margin: 0 16px 8px; padding: 8px 10px; border-radius: var(--r-md); background: var(--surface); color: var(--text-3); font-size: 13px; line-height: 1.5; }
  .prompt p { flex: 1; margin: 0; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .prompt.more p { display: block; max-height: 30vh; overflow-y: auto; white-space: pre-wrap; }
  .prompt button { flex: none; color: var(--accent); font-size: 12px; padding: 0 2px; }
  .saved { width: auto !important; padding: 0 8px; color: var(--green) !important; font-size: 12px; white-space: nowrap; }
  .strip { display: flex; gap: 6px; justify-content: center; flex: none; padding: 8px 12px 12px; overflow-x: auto; }
  .strip button { flex: none; width: 64px; height: 44px; padding: 0; border-radius: var(--r-sm); overflow: hidden; border: 1px solid var(--line); opacity: 0.6; }
  .strip button:hover { opacity: 1; }
  .strip button.on { opacity: 1; border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent); }
  .strip img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .gone { color: var(--muted); padding: 24px; text-align: center; }
  .held { user-select: text; -webkit-user-select: text; flex: 1; min-height: 0; margin: 0; padding: 14px 16px; overflow: auto; white-space: pre-wrap; word-break: break-word; color: var(--text-2); font-size: 13px; line-height: 1.55; }
  .from { flex: none; margin: 0; padding: 6px 16px 10px; color: var(--text-3); font-size: 12px; }
</style>
