<!-- The context bag, opened from the composer: what you carry between threads. Click an item to add it to this message, drag it onto a thread in the sidebar to hand it to that one. Files dropped here, links and text pasted into the box below go into the bag.
     Used by PlusMenu.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  // The context bag, opened from the composer: what you carry between threads. Click an item to
  // add it to this message, drag it onto a thread in the sidebar to hand it to that one. Files
  // dropped here, links and text pasted into the box below go into the bag.
  import { fromFile } from "../attachments";
  import { asLink, bag, BAG_DRAG, bagIcon, bagSize, emptyBag, fromBag, loadBag, note, putDropped, putInBag, takeFromBag, when } from "../bag.svelte";
  import type { Attachment } from "../bindings/Attachment";
  import type { BagItem } from "../bindings/BagItem";
  import { openUrl } from "../native";
  import { previewable } from "../preview";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let {
    onpick,
    onclose,
    from,
    threadId,
    images = true,
  }: {
    /** Adds the items to the message being written. */
    onpick: (items: Attachment[]) => void;
    /** Closes the menu, once something is shown beside the thread. */
    onclose?: () => void;
    /** What new items say they came from. */
    from?: string;
    /** The open thread, whose viewer shows an item. */
    threadId?: string | null;
    /** Whether the engine takes pictures. */
    images?: boolean;
  } = $props();

  $effect(() => {
    loadBag();
  });

  let typed = $state("");
  let over = $state(false);
  let emptying = $state(false);
  let picker: HTMLInputElement;

  const usable = (b: BagItem) => images || !b.item.mime.startsWith("image/");
  const fail = (f: () => Promise<unknown>) => () => f().catch((e) => hub.fail(e));

  async function pick(ids: string[]) {
    try {
      onpick(await Promise.all(ids.map(fromBag)));
    } catch (e) {
      hub.fail(e);
    }
  }

  async function addFiles(files: File[]) {
    for (const f of files) {
      try {
        await putInBag(await fromFile(f), from);
      } catch (e) {
        hub.fail(e);
      }
    }
  }

  /** A link goes in as a link; anything else as a short note. */
  async function addTyped() {
    const value = typed.trim();
    if (!value) return;
    const item = asLink(value) ?? note(value);
    try {
      await putInBag(item, from);
      typed = "";
    } catch (e) {
      hub.fail(e);
    }
  }

  function show(b: BagItem) {
    if (b.item.url) {
      if (threadId && previewable(b.item.url)) hub.viewing = { thread: threadId, kind: "browser", id: b.item.url };
      else openUrl(b.item.url).catch((e) => hub.fail(e));
    } else if (threadId) {
      hub.viewing = { thread: threadId, kind: "bag", id: b.id };
    }
    onclose?.();
  }

  function drop(e: DragEvent) {
    e.preventDefault();
    // The window-wide zone would take it too.
    e.stopPropagation();
    over = false;
    if (!e.dataTransfer || e.dataTransfer.types.includes(BAG_DRAG)) return;
    putDropped(e.dataTransfer, from);
  }

  /** Ctrl V of a screenshot or copied files anywhere in the bag; text pasted in the box stays text. */
  function paste(e: ClipboardEvent) {
    const dt = e.clipboardData;
    if (!dt?.files.length) return;
    e.preventDefault();
    putDropped(dt, from);
  }
</script>

<div
  class="bag"
  transition:pop|global
  class:over
  role="dialog"
  aria-label="Context bag"
  tabindex="-1"
  ondragover={(e) => {
    if (e.dataTransfer?.types.includes(BAG_DRAG)) return;
    e.preventDefault();
    over = true;
  }}
  ondragleave={(e) => !(e.currentTarget as Element).contains(e.relatedTarget as Node) && (over = false)}
  ondrop={drop}
  onpaste={paste}
>
  <div class="head">
    <Icon name="bag" size={13} />
    <span class="title">Context bag</span>
    <span class="grow"></span>
    {#if bag.items.length > 1}
      <button class="small" onclick={() => pick(bag.items.filter(usable).map((b) => b.id))}>Add all</button>
    {/if}
    {#if bag.items.length}
      {#if emptying}
        <button class="small danger" onclick={fail(async () => { await emptyBag(); emptying = false; })}>Empty it</button>
        <button class="small" onclick={() => (emptying = false)}>Keep</button>
      {:else}
        <button class="small" onclick={() => (emptying = true)}>Empty</button>
      {/if}
    {/if}
  </div>

  {#if bag.items.length}
    <div class="list">
      {#each bag.items as b (b.id)}
        <div
          class="row"
          class:off={!usable(b)}
          role="listitem"
          draggable="true"
          ondragstart={(e) => {
            e.dataTransfer?.setData(BAG_DRAG, b.id);
            if (b.item.url) e.dataTransfer?.setData("text/uri-list", b.item.url);
            if (e.dataTransfer) e.dataTransfer.effectAllowed = "copy";
          }}
        >
          <button class="add" disabled={!usable(b)} title={usable(b) ? `Add ${b.item.name} to this message` : "This engine does not take pictures"} onclick={() => pick([b.id])}>
            {#if b.thumb}<img src={b.thumb} alt="" />{:else}<span class="glyph"><Icon name={bagIcon(b)} size={14} /></span>{/if}
            <span class="text">
              <span class="name" title={b.item.url ?? b.item.path ?? b.item.name}>{b.item.name}</span>
              <span class="detail">{[b.item.url ? new URL(b.item.url).hostname : bagSize(b), b.from, when(b)].filter(Boolean).join(" · ")}</span>
            </span>
          </button>
          {#if b.item.url || threadId}
            <button class="icon" title={b.item.url ? "Open the link" : "Show it in the viewer"} onclick={() => show(b)}><Icon name={b.item.url ? "external" : "eye"} size={13} /></button>
          {/if}
          <button class="icon" title="Remove" onclick={fail(() => takeFromBag(b.id))}><Icon name="x" size={13} /></button>
        </div>
      {/each}
    </div>
  {:else if bag.loaded}
    <p class="empty">
      Empty. Drop files or paste a link.
    </p>
  {/if}

  <div class="foot">
    <input
      bind:value={typed}
      placeholder="Paste a link or a note, Enter puts it in"
      spellcheck="false"
      onkeydown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          addTyped();
        }
      }}
    />
    <button class="small" title="Put files in the bag" onclick={() => picker.click()}><Icon name="plus" size={12} />Files</button>
    <input bind:this={picker} type="file" multiple hidden onchange={() => {
      addFiles([...(picker.files ?? [])]);
      picker.value = "";
    }} />
  </div>
  {#if over}<div class="drop">Drop to put it in the bag</div>{/if}
</div>

<style>
  .bag { position: relative; display: flex; flex-direction: column; width: 380px; max-width: calc(100vw - 32px); max-height: min(460px, 70vh); }
  .head { display: flex; align-items: center; gap: 6px; padding: 6px 6px 6px 8px; color: var(--muted); font-size: 12px; }
  .title { color: var(--text-2); font-weight: 500; }
  .grow { flex: 1; }
  .small { display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border-radius: var(--r-sm); color: var(--muted); font-size: 12px; white-space: nowrap; }
  .small:hover { background: var(--surface-hover); color: var(--text); }
  .small.danger { color: var(--red); }
  .list { overflow-y: auto; min-height: 0; padding: 0 2px; }
  .row { display: flex; align-items: center; gap: 2px; border-radius: var(--r-md); }
  .row:hover { background: var(--surface-hover); }
  .row.off { opacity: 0.5; }
  .add { flex: 1; min-width: 0; display: flex; align-items: center; gap: 8px; padding: 4px 6px; text-align: left; color: var(--text-2); }
  .add:disabled { cursor: default; }
  .row:hover .add { color: var(--text); }
  .add img, .glyph { flex: none; width: 28px; height: 28px; border-radius: var(--r-sm); object-fit: cover; }
  .glyph { display: grid; place-items: center; background: var(--surface); color: var(--muted); }
  .text { min-width: 0; display: flex; flex-direction: column; }
  .name, .detail { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name { font-size: 13px; }
  .detail { color: var(--muted); font-size: 12px; }
  .icon { flex: none; display: grid; place-items: center; width: 24px; height: 24px; border-radius: var(--r-sm); color: var(--muted); opacity: 0; }
  .row:hover .icon, .icon:focus-visible { opacity: 1; }
  .icon:hover { color: var(--text); background: var(--surface); }
  .empty { margin: 4px 8px 8px; color: var(--muted); font-size: 13px; line-height: 1.5; }
  .foot { display: flex; align-items: center; gap: 6px; padding: 6px 4px 2px; border-top: 1px solid var(--line); margin-top: 4px; }
  .foot input:not([hidden]) { flex: 1; min-width: 0; height: 26px; padding: 0 8px; border-radius: var(--r-sm); border: 1px solid var(--line); background: var(--surface); color: var(--text); font-size: 13px; }
  .foot input:focus { outline: none; border-color: var(--line-strong); }
  .drop { position: absolute; inset: 0; display: grid; place-items: center; border-radius: var(--r-md); border: 1px dashed var(--accent); background: color-mix(in srgb, var(--surface-2) 88%, transparent); color: var(--text-2); font-size: 13px; pointer-events: none; }
</style>
