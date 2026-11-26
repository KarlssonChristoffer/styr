<!-- A picture big over the whole window, the way Claude Code shows one: no panel and no frame, the picture on a dark veil with its name and a few quiet buttons. ← → and the strip step through the thread's other pictures, − Fit + (or Ctrl and the wheel) zoom, a drag moves a zoomed picture, and Esc or a click beside it closes. The rest (share, show in folder, the context bag) waits behind ⋮.
     Used by App.svelte. -->
<script lang="ts">
  import { modal } from "../pop";
  // A picture big over the whole window, the way Claude Code shows one: no panel and no frame,
  // the picture on a dark veil with its name and a few quiet buttons. ← → and the strip step
  // through the thread's other pictures, − Fit + (or Ctrl and the wheel) zoom, a drag moves a
  // zoomed picture, and Esc or a click beside it closes. The rest (share, show in folder, the
  // context bag) waits behind ⋮.
  import { api } from "../api";
  import { here, putInBag } from "../bag.svelte";
  import { pictures } from "../lightbox.svelte";
  import { contextMenu } from "../menu.svelte";
  import { copyImage, dataBlob, reveal } from "../menus.svelte";
  import { hub } from "../state.svelte";
  import { folded, shownName, type ImageItem } from "../transcript";
  import Icon from "./Icon.svelte";
  import ShareSheet from "./ShareSheet.svelte";

  const open = $derived(pictures.open);
  const all = $derived(open ? folded(hub.events[open.thread] ?? []).filter((i): i is ImageItem => i.kind === "image") : []);
  const at = $derived(open ? all.findIndex((i) => i.id === open.id) : -1);
  const image = $derived(at >= 0 ? all[at] : undefined);
  const name = $derived(image ? shownName(image, all) : "");

  const close = () => (pictures.open = null);
  const go = (by: number) => {
    const next = all[at + by];
    if (open && next) pictures.open = { thread: open.thread, id: next.id };
  };

  // ---- Zoom: null is fitted to the window; a number is that many times its own size. ----
  let scale = $state<number | null>(null);
  let natural = $state({ w: 0, h: 0 });
  let stage = $state<HTMLDivElement>();
  let img = $state<HTMLImageElement>();
  $effect(() => {
    void open?.id;
    scale = null;
    sharing = false;
    saved = null;
  });
  const fitted = () => (img && natural.w ? img.clientWidth / natural.w : 1);
  function zoom(by: number) {
    const from = scale ?? fitted();
    const to = Math.min(8, Math.max(0.1, from * by));
    scale = Math.abs(to - fitted()) < 0.02 ? null : to;
  }
  function wheel(e: WheelEvent) {
    if (!e.ctrlKey) return;
    e.preventDefault();
    zoom(e.deltaY < 0 ? 1.15 : 1 / 1.15);
  }
  // A zoomed picture moves with the mouse held down.
  let drag: { x: number; y: number; left: number; top: number } | null = null;
  function down(e: PointerEvent) {
    if (scale == null || !stage || e.button !== 0) return;
    drag = { x: e.clientX, y: e.clientY, left: stage.scrollLeft, top: stage.scrollTop };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!drag || !stage) return;
    stage.scrollLeft = drag.left - (e.clientX - drag.x);
    stage.scrollTop = drag.top - (e.clientY - drag.y);
  }

  // ---- What can be done with it ----
  let saved = $state<{ text: string; path: string } | null>(null);
  let savedTimer: ReturnType<typeof setTimeout> | undefined;
  function said(text: string, path: string) {
    saved = { text, path };
    clearTimeout(savedTimer);
    savedTimer = setTimeout(() => (saved = null), 6000);
  }
  const fileName = (i: ImageItem) => {
    const type = dataBlob(i.src).type;
    return name.includes(".") ? name : `${name}.${type.split("/")[1] ?? "png"}`;
  };
  async function download(i: ImageItem) {
    const blob = dataBlob(i.src);
    said("Saved to Downloads", (await api.download(fileName(i), new File([blob], fileName(i), { type: blob.type }))).path);
  }
  const fail = (f: () => Promise<unknown>) => () => void f().catch((e) => hub.fail(e));
  let sharing = $state(false);

  function more(e: MouseEvent) {
    if (!image || !open) return;
    const i = image;
    const thread = open.thread;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const z = Number(document.documentElement.style.zoom) || 1;
    contextMenu.show(r.right / z - 220, r.bottom / z + 4, [
      { label: "Share…", icon: "share", run: () => (sharing = true) },
      i.path && !hub.threads.find((t) => t.id === thread)?.host && { label: "Show in folder", icon: "folder", run: reveal(i.path) },
      {
        label: "Put it in the context bag",
        icon: "bag",
        hint: "for other threads",
        run: fail(async () => {
          const mime = dataBlob(i.src).type;
          await putInBag({ name: fileName(i), mime, data: i.src.slice(i.src.indexOf(",") + 1) }, here(thread));
        }),
      },
    ]);
  }

  // Keys while it is open, before anything under it sees them.
  $effect(() => {
    if (!open) return;
    const key = (e: KeyboardEvent) => {
      if (e.altKey || e.metaKey) return;
      if (e.key === "Escape") {
        if (contextMenu.open) return;
        e.preventDefault();
        e.stopPropagation();
        if (sharing) sharing = false;
        else close();
      } else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && !sharing) {
        e.preventDefault();
        e.stopPropagation();
        go(e.key === "ArrowLeft" ? -1 : 1);
      } else if ((e.key === "+" || e.key === "=") && !sharing) {
        e.preventDefault();
        zoom(1.25);
      } else if (e.key === "-" && !sharing) {
        e.preventDefault();
        zoom(1 / 1.25);
      } else if (e.key === "0" && !sharing) {
        e.preventDefault();
        scale = null;
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  });
  // A picture that is gone (the thread was cleared or deleted) closes it.
  $effect(() => {
    if (open && hub.events[open.thread] && !image) close();
  });
</script>

{#if open && image}
  <div class="veil" in:modal|global={{ card: ".stage" }} out:modal|global={{ out: true, card: ".stage" }} role="dialog" aria-modal="true" aria-label={name}>
    <div class="top">
      <span class="name" data-tip={image.path ?? name}>{name}</span>
      {#if all.length > 1}<span class="count">{at + 1} / {all.length}</span>{/if}
      <span class="sp"></span>
      {#if saved}
        <button class="saved" data-tip={saved.path} onclick={reveal(saved.path)}>{saved.text} · Show</button>
      {:else}
        <button data-tip="Download to Downloads" aria-label="Download" onclick={fail(() => download(image))}><Icon name="download" size={15} /></button>
      {/if}
      <button data-tip="Copy image" aria-label="Copy image" onclick={fail(() => copyImage(image.src))}><Icon name="copy" size={15} /></button>
      <button data-tip="More" aria-label="More" onclick={more}><Icon name="more" size={15} /></button>
      <button data-tip="Close (Esc)" aria-label="Close" onclick={close}><Icon name="x" size={15} /></button>
    </div>

    {#if sharing}
      <div class="share"><ShareSheet {image} {name} threadId={open.thread} onclose={() => (sharing = false)} onsaved={said} /></div>
    {/if}

    <!-- A click on the veil beside the picture closes it; on the picture it does not. -->
    <div
      class="stage"
      class:zoomed={scale != null}
      bind:this={stage}
      onwheel={wheel}
      onclick={(e) => e.target === e.currentTarget && close()}
      role="presentation"
    >
      <img
        bind:this={img}
        src={image.src}
        alt={image.prompt ?? name}
        draggable="false"
        style:width={scale != null && natural.w ? `${natural.w * scale}px` : undefined}
        onload={(e) => (natural = { w: (e.currentTarget as HTMLImageElement).naturalWidth, h: (e.currentTarget as HTMLImageElement).naturalHeight })}
        onpointerdown={down}
        onpointermove={move}
        onpointerup={() => (drag = null)}
        ondblclick={() => (scale == null ? zoom(2) : (scale = null))}
      />
    </div>
    {#if image.prompt}<p class="prompt" data-tip={image.prompt}>{image.prompt}</p>{/if}

    {#if all.length > 1}
      <button class="arrow prev" aria-label="Previous" data-tip="Previous (←)" disabled={at <= 0} onclick={() => go(-1)}><Icon name="back" size={18} /></button>
      <button class="arrow next" aria-label="Next" data-tip="Next (→)" disabled={at >= all.length - 1} onclick={() => go(1)}><Icon name="chevron" size={18} /></button>
    {/if}

    <div class="dock">
      {#if all.length > 1}
        {#each all as other, k (other.id)}
          <button class="thumb" class:on={k === at} aria-label="Picture {k + 1}" onclick={() => (pictures.open = { thread: open.thread, id: other.id })}>
            <img src={other.src} alt="" />
          </button>
        {/each}
        <span class="rule"></span>
      {/if}
      <button class="z" data-tip="Smaller (−)" aria-label="Smaller" onclick={() => zoom(1 / 1.25)}>−</button>
      <button class="z fit" class:on={scale == null} data-tip="Fit to the window (0)" onclick={() => (scale = null)}>{scale == null ? "Fit" : `${Math.round(scale * 100)}%`}</button>
      <button class="z" data-tip="Bigger (+)" aria-label="Bigger" onclick={() => zoom(1.25)}>+</button>
    </div>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; z-index: 95; display: flex; flex-direction: column; background: rgba(9, 9, 8, 0.9); backdrop-filter: blur(3px); color: #d3cfc8; }
  /* The veil is dark in every theme, so what sits on it keeps the dark theme's ink (Cappuccino's own is dark and would vanish). */
  .top, .stage, .arrow, .dock { --text: #ece8e2; --text-2: #d3cfc8; --text-3: #a8a299; --muted: #8f8a82; --green: #84c29a; --line: #352e27; }
  .top { flex: none; display: flex; align-items: center; gap: 4px; height: 52px; padding: 0 12px 0 18px; font-size: 13px; }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); }
  .count { flex: none; margin-left: 8px; color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; }
  .sp { flex: 1; }
  .top button { display: grid; place-items: center; width: 32px; height: 32px; border-radius: var(--r-md); color: var(--text-3); }
  .top button:hover { background: rgba(255, 255, 255, 0.08); color: var(--text); }
  .top .saved { width: auto; padding: 0 10px; color: var(--green); font-size: 12px; white-space: nowrap; }
  .share { position: absolute; top: 12px; right: 12px; z-index: 2; width: 360px; }
  /* Fitted, the picture takes what the bars leave; zoomed, the stage scrolls and a drag moves it. */
  .stage { flex: 1; min-height: 0; display: grid; place-items: center; overflow: hidden; padding: 0 72px 84px; }
  .stage img { max-width: calc(100vw - 144px); max-height: calc(100vh - 152px); object-fit: contain; border-radius: var(--r-md); box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5); user-select: none; }
  .stage.zoomed { overflow: auto; place-items: start; }
  .stage.zoomed img { max-width: none; max-height: none; margin: auto; cursor: grab; }
  .stage.zoomed img:active { cursor: grabbing; }
  .prompt {
    position: absolute; left: 50%; bottom: 74px; transform: translateX(-50%); width: min(720px, calc(100% - 160px)); margin: 0;
    color: var(--muted); font-size: 12px; line-height: 1.45; text-align: center;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
  }
  .arrow { position: absolute; top: 50%; transform: translateY(-50%); display: grid; place-items: center; width: 40px; height: 40px; border-radius: 50%; background: rgba(40, 40, 38, 0.72); color: var(--text); }
  .arrow:hover:not(:disabled) { background: rgba(60, 60, 57, 0.9); }
  .arrow:disabled { opacity: 0.25; }
  .prev { left: 18px; }
  .next { right: 18px; }
  .dock {
    position: absolute; left: 50%; bottom: 18px; transform: translateX(-50%); max-width: calc(100% - 48px); overflow-x: auto;
    display: flex; align-items: center; gap: 6px; padding: 6px; border-radius: var(--r-lg); background: rgba(30, 30, 28, 0.88); border: 1px solid var(--line);
  }
  .thumb { flex: none; width: 64px; height: 40px; padding: 0; border-radius: var(--r-sm); overflow: hidden; opacity: 0.45; }
  .thumb:hover { opacity: 0.8; }
  .thumb.on { opacity: 1; box-shadow: 0 0 0 1.5px var(--text-2); }
  .thumb img { display: block; width: 100%; height: 100%; object-fit: cover; object-position: top left; }
  .rule { flex: none; width: 1px; height: 24px; background: var(--line); margin: 0 4px; }
  .z { flex: none; min-width: 30px; height: 30px; padding: 0 6px; border-radius: var(--r-md); color: var(--text-3); font-size: 14px; font-variant-numeric: tabular-nums; }
  .z.fit { font-size: 12px; min-width: 44px; }
  .z:hover { background: rgba(255, 255, 255, 0.08); color: var(--text); }
</style>
