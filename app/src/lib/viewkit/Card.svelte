<!-- What an output looks like inline in the chat: not a frame laid on top but the thing itself, part of the answer. The preview sits straight on the chat (a still, no scrollbar, faded where it is cut) and the whole of it is one click that opens the output big. Under it one quiet line says what it is: its glyph, its name, the kind and where it stands among its versions, and ⋮. A answer with many outputs draws each as a row with a miniature instead (`compact`). The frame is Hub's, drawn with app.css tokens; what is inside (a live page in HubKit's styles, a table, a picture) is the kind's own.
     Used by Changes.svelte, NamedFile.svelte, PageCard.svelte. -->
<script lang="ts">
  // What an output looks like inline in the chat: not a frame laid on top but the thing itself,
  // part of the answer. The preview sits straight on the chat (a still, no scrollbar, faded where it
  // is cut) and the whole of it is one click that opens the output big. Under it one quiet line says
  // what it is: its glyph, its name, the kind and where it stands among its versions, and ⋮. A
  // answer with many outputs draws each as a row with a miniature instead (`compact`). The frame is
  // Hub's, drawn with app.css tokens; what is inside (a live page in HubKit's styles, a table, a
  // picture) is the kind's own.
  import type { Snippet } from "svelte";
  import { contextMenu, menu, type MenuEntry } from "../menu.svelte";
  import { KINDS, type ViewKind } from "./kinds";
  import Icon from "../components/Icon.svelte";

  let {
    kind,
    name,
    meta = "",
    items,
    onexpand,
    onpanel,
    onmodal,
    unfolded = false,
    grown = false,
    ongrow,
    flat = false,
    full,
    compact = false,
    children,
  }: {
    kind: ViewKind;
    name: string;
    /** Quiet text after the kind, such as "v2 of 3". */
    meta?: string;
    /** What the ⋮ menu offers; none, no button. */
    items?: () => MenuEntry[];
    /** Opens it big; the whole preview is the button. */
    onexpand?: () => void;
    /** The caption's own ways to open it once: in the right panel, in the modal. */
    onpanel?: () => void;
    onmodal?: () => void;
    /** Unfolded in the chat: `full` is drawn whole instead of the cut preview. */
    unfolded?: boolean;
    /** Grown in place: the card leaves the chat column, takes the window's room and zooms. */
    grown?: boolean;
    /** Grows it, or folds it back when it is grown. */
    ongrow?: () => void;
    /** Unfolded, the body does not scroll inside itself (a live page sizes itself and the chat scrolls). */
    flat?: boolean;
    full?: Snippet;
    /** One row with a miniature instead of the preview. */
    compact?: boolean;
    children: Snippet;
  } = $props();

  const CUT = 236;
  let inner = $state<HTMLElement>();
  let over = $state(false);
  $effect(() => {
    if (!inner || compact) return;
    const ro = new ResizeObserver(() => (over = (inner?.offsetHeight ?? 0) > CUT));
    ro.observe(inner);
    return () => ro.disconnect();
  });

  // The ways to open it that have no button of their own sit in ⋮ with the kind's own entries.
  const all = (): MenuEntry[] => [onmodal && { label: "Open in a window", icon: "popout" as const, run: onmodal }, ...(items?.() ?? [])];
  function more(e: MouseEvent) {
    e.stopPropagation();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const k = Number(document.documentElement.style.zoom) || 1;
    contextMenu.show(r.left / k, r.bottom / k + 4, all());
  }
  function key(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onexpand?.();
    }
  }
  // ---- Grown in place (design/DESIGN.md, "Opening a file or page") ----
  // The card leaves the column for the room the thread's scroller has: as wide as it is (at most
  // GROW_W, centred) and as tall as it is less a gap, so a big screen gets air at the sides and a
  // small window gets all it has. The height is animated from the card's own to that one; the chat
  // stays above, scrolled out of sight, and folding it back puts the scroll where it was.
  const GROW_W = 1280;
  let wrap = $state<HTMLElement>();
  let tile = $state<HTMLElement>();
  let body = $state<HTMLElement>();
  let room = $state({ w: 0, h: 0 });
  /** How tall the content is at the zoom it is at, once known; the card is no taller than this needs. */
  let need = $state(0);
  let zin = $state<HTMLElement>();
  const MIN_H = 160;
  const BAR_H = 39;
  const target = $derived(Math.max(MIN_H, Math.min(room.h - 40, need > 0 ? need + BAR_H : Infinity)));
  let height = $state<number | null>(null);
  let startH = 236;
  let from = 0;
  let z = $state(1);
  let hold = $state(false);
  const still = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const scroller = () => wrap?.closest<HTMLElement>(".scroll") ?? null;
  $effect(() => {
    if (!tile || grown) return;
    const ro = new ResizeObserver(() => (startH = tile?.offsetHeight ?? startH));
    ro.observe(tile);
    return () => ro.disconnect();
  });
  $effect(() => {
    if (!grown || !wrap) return;
    const sc = scroller();
    if (!sc) return;
    const measure = () => (room = { w: sc.clientWidth, h: sc.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(sc);
    from = sc.scrollTop;
    height = startH + 40;
    z = 1;
    const go = requestAnimationFrame(() => {
      height = target;
      wrap?.scrollIntoView({ block: "start", behavior: still ? "auto" : "smooth" });
    });
    return () => {
      cancelAnimationFrame(go);
      ro.disconnect();
    };
  });
  $effect(() => {
    if (grown && height !== null && height > startH + 40 || (grown && height !== null && need > 0)) height = target;
  });
  function shut() {
    if (!grown) return;
    if (still) return ongrow?.();
    height = startH + 40;
    scroller()?.scrollTo({ top: from, behavior: "smooth" });
    setTimeout(() => ongrow?.(), 230);
  }
  $effect(() => {
    if (!grown || !zin) return;
    const k = Number(document.documentElement.style.zoom) || 1;
    const ro = new ResizeObserver(() => (need = Math.ceil((zin?.getBoundingClientRect().height ?? 0) / k)));
    ro.observe(zin);
    return () => {
      ro.disconnect();
      need = 0;
    };
  });
  $effect(() => {
    if (!grown) return;
    const key = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      const typing = !!t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      if (e.key === "Escape" && !typing) {
        e.preventDefault();
        shut();
      } else if (!typing && (e.key === "Control" || e.key === " ")) hold = true;
    };
    const up = (e: KeyboardEvent) => (e.key === "Control" || e.key === " ") && (hold = false);
    const blur = () => (hold = false);
    window.addEventListener("keydown", key);
    window.addEventListener("keyup", up);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("keydown", key);
      window.removeEventListener("keyup", up);
      window.removeEventListener("blur", blur);
      hold = false;
    };
  });
  /** Zoom to `next`, keeping the point under (x, y) where it is. */
  function zoomTo(next: number, x?: number, y?: number) {
    if (!body) return;
    const n = Math.min(4, Math.max(0.25, next));
    const r = body.getBoundingClientRect();
    const k = Number(document.documentElement.style.zoom) || 1;
    const px = x === undefined ? r.width / 2 : x / k - r.left;
    const py = y === undefined ? r.height / 2 : y / k - r.top;
    const cx = (body.scrollLeft + px) / z;
    const cy = (body.scrollTop + py) / z;
    z = n;
    requestAnimationFrame(() => {
      if (!body) return;
      body.scrollLeft = cx * n - px;
      body.scrollTop = cy * n - py;
    });
  }
  function wheel(e: WheelEvent) {
    if (e.ctrlKey) {
      e.preventDefault();
      zoomTo(z * Math.exp(-e.deltaY * 0.0015), e.clientX, e.clientY);
    } else if (body) {
      e.preventDefault();
      body.scrollBy(e.deltaX, e.deltaY);
    }
  }
  let drag: { x: number; y: number; l: number; t: number } | null = null;
  const down = (e: PointerEvent) => {
    if (!body) return;
    drag = { x: e.clientX, y: e.clientY, l: body.scrollLeft, t: body.scrollTop };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const move = (e: PointerEvent) => {
    if (!drag || !body) return;
    body.scrollLeft = drag.l - (e.clientX - drag.x);
    body.scrollTop = drag.t - (e.clientY - drag.y);
  };
  const detail = $derived([KINDS[kind].label, meta].filter(Boolean).join(" · "));
</script>

{#if compact}
  <div class="row" class:go={!!onexpand} role="button" tabindex="0" data-tip="Open big" onclick={onexpand} onkeydown={key} use:menu={() => all()}>
    <div class="th"><div class="mini" inert>{@render children()}</div></div>
    <div class="tx"><b>{name}</b><span>{detail}</span></div>
    {#if onexpand}<span class="open">Open <Icon name="chevron" size={12} /></span>{/if}
  </div>
{:else}
  <div class="card" class:grown={grown && !!full} use:menu={() => all()}>
    {#if grown && full}
      <div class="grow" bind:this={wrap} style:--xw="{Math.min(GROW_W, Math.max(280, room.w - 2 * 20))}px" style:height={height === null ? undefined : `${height}px`} class:still>
        <div class="xbar">
          <Icon name={KINDS[kind].icon} size={12} />
          <span class="name" data-tip={name}>{name}</span>
          <span class="meta">{detail}</span>
          <span class="sp"></span>
          <span class="zoom">
            <button class="b" aria-label="Zoom out" data-tip="Zoom out" onclick={() => zoomTo(z / 1.25)}><Icon name="minus" size={13} /></button>
            <button class="pct" aria-label="Reset zoom" data-tip="Back to 100 %" onclick={() => zoomTo(1)}>{Math.round(z * 100)}%</button>
            <button class="b" aria-label="Zoom in" data-tip="Zoom in" onclick={() => zoomTo(z * 1.25)}><Icon name="plus" size={13} /></button>
          </span>
          {#if onpanel}<button class="b" aria-label="Open in the panel" data-tip="Open in the panel" onclick={onpanel}><Icon name="panel" size={13} /></button>{/if}
          <button class="b" aria-label="More" data-tip="More" onclick={more}><Icon name="more" size={14} /></button>
          <button class="b on" aria-label="Back to the chat" data-tip="Back to the chat (Esc)" onclick={shut}><Icon name="x" size={13} /></button>
        </div>
        <div role="presentation" class="xbody" bind:this={body} onwheel={wheel}>
          <div class="zin" bind:this={zin} style:zoom={z}>{@render full()}</div>
          {#if hold}<div role="presentation" class="catch" onwheel={wheel} onpointerdown={down} onpointermove={move} onpointerup={() => (drag = null)}></div>{/if}
        </div>
      </div>
    {:else if unfolded && full}
      <div class="whole" class:flat>{@render full()}</div>
    {:else}
    <div class="tile" bind:this={tile} class:go={!!onexpand} class:over role="button" tabindex={onexpand ? 0 : -1} aria-label="Open {name} big" onclick={onexpand} onkeydown={key}>
      <div class="pv" inert><div bind:this={inner}>{@render children()}</div></div>
      {#if onexpand}<span class="pill"><Icon name="expand" size={12} />Open</span>{/if}
    </div>
    {/if}
    {#if !(grown && full)}
    <div class="cap">
      <Icon name={KINDS[kind].icon} size={12} />
      <span class="name" data-tip={name}>{name}</span>
      <span class="meta">{detail}</span>
      <span class="sp"></span>
      {#if unfolded}<button class="b w" aria-label="Hide" data-tip="Hide" onclick={onexpand}><Icon name="x" size={13} /></button>{/if}
      {#if onpanel}<button class="b" aria-label="Open in the panel" data-tip="Open in the panel" onclick={onpanel}><Icon name="panel" size={13} /></button>{/if}
      {#if ongrow && full}<button class="b" aria-label="Expand" data-tip="Expand" onclick={ongrow}><Icon name="expand" size={13} /></button>{/if}
      {#if items || onmodal}<button class="b" aria-label="More" data-tip="More" onclick={more}><Icon name="more" size={14} /></button>{/if}
    </div>
    {/if}
  </div>
{/if}

<style>
  .card { margin: 8px 0 14px; }
  .tile { position: relative; border-radius: var(--r-lg); background: var(--surface); overflow: hidden; }
  .whole { border-radius: var(--r-lg); background: var(--surface); overflow: auto; max-height: min(70vh, 640px); padding: 6px 12px; }
  .whole.flat { max-height: none; overflow: visible; padding: 0; }
  .tile.go { cursor: zoom-in; }
  .tile.go:hover, .tile.go:focus-visible { background: var(--surface-hover); outline: 1px solid var(--line-strong); }
  .pv { max-height: 236px; overflow: hidden; padding: 6px 12px; pointer-events: none; user-select: none; }
  .over::after { content: ""; position: absolute; inset: auto 0 0 0; height: 64px; background: linear-gradient(transparent, var(--surface)); pointer-events: none; }
  .go.over:hover::after { background: linear-gradient(transparent, var(--surface-hover)); }
  .pill { position: absolute; top: 10px; right: 10px; display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px; border-radius: var(--r-md); background: color-mix(in srgb, var(--bg) 80%, transparent); color: var(--text-2); font-size: 12px; opacity: 0; transition: opacity 0.12s var(--ease); }
  .tile:hover .pill, .tile:focus-visible .pill { opacity: 1; }
  .cap { display: flex; align-items: center; gap: 8px; min-width: 0; margin-top: 4px; color: var(--muted); font-size: 12px; }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-3); font-weight: 500; }
  .meta { flex: none; white-space: nowrap; font-variant-numeric: tabular-nums; }
  .sp { flex: 1; }
  .b { display: grid; place-items: center; width: 24px; height: 22px; border-radius: var(--r-md); color: var(--muted); }
  .b:hover { background: var(--surface-hover); color: var(--text); }
  /* Many outputs in one answer: a row each, with a miniature. */
  .row { display: flex; align-items: center; gap: 12px; max-width: 560px; margin: 4px 0 8px; padding: 8px 14px 8px 8px; border-radius: var(--r-lg); background: var(--surface); }
  .row.go { cursor: zoom-in; }
  .row.go:hover { background: var(--surface-hover); }
  .th { position: relative; flex: none; width: 92px; height: 60px; border-radius: var(--r-md); overflow: hidden; background: var(--bg); }
  .mini { position: absolute; left: 0; top: 0; width: 400px; padding: 8px; transform: scale(0.23); transform-origin: 0 0; pointer-events: none; }
  .tx { display: flex; flex-direction: column; gap: 3px; min-width: 0; flex: 1; }
  .tx b { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text); font-size: 13px; font-weight: 500; }
  .tx span { color: var(--muted); font-size: 12px; }
  .open { flex: none; display: inline-flex; align-items: center; gap: 4px; color: var(--text-3); font-size: 12px; }
  /* Grown: out of the column, as wide as the room (at most 1280 px), centred. */
  .grow { position: relative; display: flex; flex-direction: column; width: var(--xw); margin-inline: calc((100% - var(--xw)) / 2); min-height: 160px; border-radius: var(--r-lg); background: var(--surface); border: 1px solid var(--line-strong); overflow: hidden; scroll-margin-top: 12px; transition: height 0.26s var(--ease); }
  .grow.still { transition: none; }
  .xbar { flex: none; display: flex; align-items: center; gap: 8px; height: 38px; padding: 0 8px 0 14px; border-bottom: 1px solid var(--line); color: var(--muted); font-size: 12px; }
  .xbar .name { font-size: 13px; color: var(--text); }
  .zoom { display: inline-flex; align-items: center; gap: 1px; padding: 2px; border-radius: var(--r-md); background: var(--surface-2); }
  .pct { min-width: 46px; height: 22px; border-radius: var(--r-md); color: var(--text-2); font-size: 12px; font-variant-numeric: tabular-nums; }
  .pct:hover { background: var(--surface-hover); color: var(--text); }
  .b.on { background: var(--surface-hover); color: var(--text); }
  .xbody { position: relative; flex: 1; min-height: 0; overflow: auto; background: var(--bg); overscroll-behavior: contain; }
  .catch { position: absolute; inset: 0; z-index: 2; cursor: grab; }
  .catch:active { cursor: grabbing; }
</style>
