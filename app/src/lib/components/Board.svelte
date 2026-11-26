<!-- The board of cards you pick, place and size yourself (Customize), shared by Overview and Home: each page has its own cards and layout (`page`), the page around it holds the heading and the Customize button. While customizing, a card moves by dragging its body and resizes from its edges and corners; the others make room, and nothing overlaps. Dropping a card on the middle of another combines them into one frame (Split in the menu takes them apart). overview.svelte.ts holds the catalogue, the saved layouts, looks, links and knobs; board.ts the arithmetic.
     Used by Home.svelte, Overview.svelte. -->
<script lang="ts">
  import { untrack } from "svelte";
  // The board of cards you pick, place and size yourself (Customize), shared by Overview and Home:
  // each page has its own cards and layout (`page`), the page around it holds the heading and the
  // Customize button. While customizing, a card moves by dragging its body and resizes from its
  // edges and corners; the others make room, and nothing overlaps. Dropping a card on the middle
  // of another combines them into one frame (Split in the menu takes them apart). overview.svelte.ts
  // holds the catalogue, the saved layouts, looks, links and knobs; board.ts the arithmetic.
  import type { Snippet } from "svelte";
  import { COLS, GAP, MIN_H, ROW, SIZE_COLS, clampSpot, freeSpot, settle, type Placed, type Spot } from "../board";
  import { CARDS, changeWidget, defaultBoard, isWidget, layout, links, loadWidgets, lookOf, makeWidget, saveLayout, sharable, sharedBy, toggleLink, toggleShare, widgetOf, type Page } from "../overview.svelte";
  import { prefs, setPref } from "../prefs.svelte";
  import CardFrame from "./CardFrame.svelte";
  import CardMenu from "./CardMenu.svelte";
  import Icon from "./Icon.svelte";
  import OverviewShelf from "./OverviewShelf.svelte";

  let {
    page,
    content,
    editing = false,
    undoable = $bindable(false),
    empty = "No cards on the page. Press Customize and add some.",
  }: {
    page: Page;
    /** Draws the card with this id (or widget id). */
    content: Snippet<[string]>;
    /** Whether Customize is on: the page around the board has the button. */
    editing?: boolean;
    /** Whether there is a change to undo. */
    undoable?: boolean;
    empty?: string;
  } = $props();

  // A board belongs to one page for as long as it lives, so `page` is read once.
  let spots = $state<Spot[]>(untrack(() => layout(page)));
  /** How tall each fit-to-content card is now, in pixels. */
  let measured = $state<Record<string, number>>({});
  let board = $state<HTMLElement>();
  let boardWidth = $state(0);
  /** Too narrow for a board: the cards stack in reading order. */
  const narrow = $derived(boardWidth > 0 && boardWidth < 760);

  type Edge = "move" | "e" | "w" | "s" | "se" | "sw";
  /** `target` is the card the held one is over the middle of: dropping combines them. */
  type Hold = { id: string; edge: Edge; x0: number; y0: number; start: Spot; origin: Placed; now: Spot; dx: number; dy: number; moved: boolean; target: string | null };
  let hold = $state<Hold | null>(null);
  /** A card being dragged in from the shelf, and where it would land. */
  let incoming = $state<Spot | null>(null);
  let undo = $state<Spot[][]>([]);
  /** The card whose right-click menu is open, and where. */
  let menu = $state<{ id: string; x: number; y: number } | null>(null);
  /** The card whose How it's counted text is open (the menu opens it). */
  let how = $state<string | null>(null);
  /** The lines drawn between linked cards, in board pixels. */
  let wires = $state<{ key: string; d: string; a: [number, number]; b: [number, number]; lookOnly: boolean }[]>([]);

  const pairs = $derived.by(() => {
    void prefs.overviewLinks;
    return links().filter(([a, b]) => ids.has(a) && ids.has(b));
  });
  const linkCount = (id: string) => pairs.filter(([a, b]) => a === id || b === id).length;
  function partnersOf(id: string) {
    void prefs.overviewLinks;
    const own = spots.find((s) => s.id === id)?.with ?? [];
    return [...ids].filter((o) => o !== id && !own.includes(o)).map((o) => ({ id: o, title: titleOf(o), can: sharable(id, o), shared: sharedBy(id, o) }));
  }

  // Lines between linked cards, only while customizing: outside it the page shows just the cards.
  $effect(() => {
    void placed;
    void boardWidth;
    const shown = editing ? pairs : [];
    if (!board || narrow || !shown.length || hold) {
      wires = [];
      return;
    }
    const frame = requestAnimationFrame(() => {
      if (!board) return;
      const z = zoom();
      const r0 = board.getBoundingClientRect();
      const box = (id: string) => {
        const r = (board!.querySelector(`[data-part='${id}']`) ?? board!.querySelector(`[data-card='${id}'] .body`))?.getBoundingClientRect();
        return r && { x: (r.left - r0.left) / z, y: (r.top - r0.top) / z, w: r.width / z, h: r.height / z };
      };
      const out: typeof wires = [];
      for (const [a, b, shares] of shown) {
        const p = box(a), q = box(b);
        if (!p || !q) continue;
        // From the nearer sides: left/right when side by side, else top/bottom.
        const pc = [p.x + p.w / 2, p.y + p.h / 2], qc = [q.x + q.w / 2, q.y + q.h / 2];
        const across = Math.abs(pc[0] - qc[0]) > Math.abs(pc[1] - qc[1]);
        let s: [number, number], e: [number, number], d: string;
        if (across) {
          const right = qc[0] > pc[0];
          s = [right ? p.x + p.w : p.x, pc[1]];
          e = [right ? q.x : q.x + q.w, qc[1]];
          const m = (s[0] + e[0]) / 2;
          d = `M${s[0]} ${s[1]} C ${m} ${s[1]}, ${m} ${e[1]}, ${e[0]} ${e[1]}`;
        } else {
          const down = qc[1] > pc[1];
          s = [pc[0], down ? p.y + p.h : p.y];
          e = [qc[0], down ? q.y : q.y + q.h];
          const m = (s[1] + e[1]) / 2;
          d = `M${s[0]} ${s[1]} C ${s[0]} ${m}, ${e[0]} ${m}, ${e[0]} ${e[1]}`;
        }
        out.push({ key: `${a}-${b}`, d, a: s, b: e, lookOnly: !shares.includes("time") });
      }
      wires = out;
    });
    return () => cancelAnimationFrame(frame);
  });

  const titleOf = (id: string) => CARDS.find((c) => c.id === id)?.title ?? widgetOf(id)?.title ?? (isWidget(id) ? "Widget" : id);
  /** A combined card is named by its parts: "Clock and Daylight". */
  const titleOfSpot = (p: Spot) => [p.id, ...(p.with ?? [])].map(titleOf).join(" and ");
  loadWidgets();
  /** Every card on the page, the ones combined into another too. */
  const ids = $derived(new Set(spots.flatMap((s) => [s.id, ...(s.with ?? [])])));
  const live = $derived.by(() => {
    // Over the middle of another card the held one leaves the flow: the board shows itself
    // without it, and nothing slides away from under the pointer.
    let list = hold ? (hold.target ? spots.filter((s) => s.id !== hold!.id) : spots.map((s) => (s.id === hold!.id ? hold!.now : s))) : spots;
    if (incoming) list = [...list.filter((s) => s.id !== incoming!.id), incoming];
    return list;
  });
  const placed = $derived.by(() => {
    const out = settle(live, measured, incoming?.id ?? (hold?.edge === "move" ? hold.id : undefined));
    // The lifted card still draws, where it was (area) and moved by its transform.
    if (hold?.target) out.push({ ...hold.origin });
    return out;
  });
  const at = $derived(new Map(placed.map((p) => [p.id, p])));
  /** Reading order, for the narrow page and for tabbing. */
  const ordered = $derived([...placed].filter((p) => p.id !== incoming?.id).sort((a, b) => a.y - b.y || a.x - b.x));

  function commit(next: Spot[]) {
    undo = [...undo.slice(-49), spots.map((s) => ({ ...s }))];
    spots = next.map(clampSpot);
    saveLayout(page, spots);
  }
  /** The board as laid out now, so what is saved is what is shown. */
  const settled = (list: Placed[]): Spot[] => list.map(({ id, x, y, w, h, with: parts }) => (parts?.length ? { id, x, y, w, h, with: parts } : { id, x, y, w, h }));

  function back() {
    const last = undo.at(-1);
    if (!last) return;
    undo = undo.slice(0, -1);
    spots = last;
    saveLayout(page, spots);
  }
  /** Takes a card off the page; a combined one takes its parts with it, and a part leaves its
   *  frame alone. */
  function remove(id: string) {
    commit(spots.filter((s) => s.id !== id).map((s) => (s.with?.includes(id) ? { ...s, with: s.with.filter((w) => w !== id) } : s)));
  }
  /** Puts the held card into the frame of `into`, after what is there already. */
  function combine(id: string, into: string) {
    const held = spots.find((s) => s.id === id);
    if (!held || id === into) return;
    const parts = [id, ...(held.with ?? [])];
    commit(spots.filter((s) => s.id !== id).map((s) => (s.id === into ? { ...s, with: [...(s.with ?? []), ...parts] } : s)));
  }
  /** Takes a combined card apart: each part gets a spot of its own, the first place it fits. */
  function split(id: string) {
    const group = spots.find((s) => s.id === id);
    if (!group?.with?.length) return;
    let list: Spot[] = spots.map((s) => (s.id === id ? { ...s, with: undefined } : s));
    for (const part of group.with) {
      const spot = freeSpot(settle(list, measured), group.w);
      list = settled(settle([...list, { id: part, x: spot.x, y: spot.y, w: group.w, h: null }], measured, part));
    }
    commit(list);
  }
  function addAt(id: string, where?: { x: number; y: number }) {
    const size = CARDS.find((c) => c.id === id)?.size ?? "s";
    const w = SIZE_COLS[size];
    const spot = where ?? freeSpot(placed, w);
    commit(settled(settle([...spots, { id, x: spot.x, y: spot.y, w, h: null }], measured, id)));
  }
  function toggle(id: string) {
    if (ids.has(id)) remove(id);
    else addAt(id);
  }
  function reset() {
    undo = [...undo, spots.map((s) => ({ ...s }))];
    spots = defaultBoard(page);
    saveLayout(page, null);
    // Looks, links and titles are kept by card, shared by the pages that show it: only the page
    // that owns the whole set (Overview) starts them over too.
    if (page === "overview") {
      setPref("overviewLooks", {});
      setPref("overviewLinks", null);
      setPref("overviewFrames", {});
    }
  }

  // Each fit-to-content card reports how tall it is, so the board gives it that many rows.
  // The first size is taken at once, so a card never shows at the wrong height. Later sizes wait
  // for the next frame: changing the board inside the observer resizes the cards it watches again,
  // and the browser reports that as "ResizeObserver loop completed".
  function measure(node: HTMLElement, id: string) {
    let current = id;
    let frame = 0;
    const take = () => {
      const h = node.offsetHeight;
      if (h && measured[current] !== h) measured[current] = h;
    };
    take();
    const watch = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(take);
    });
    watch.observe(node);
    return {
      update: (next: string) => (current = next),
      destroy: () => {
        cancelAnimationFrame(frame);
        watch.disconnect();
      },
    };
  }
  $effect(() => {
    if (!board) return;
    boardWidth = board.offsetWidth;
    let frame = 0;
    const watch = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => (boardWidth = board!.offsetWidth));
    });
    watch.observe(board);
    return () => {
      cancelAnimationFrame(frame);
      watch.disconnect();
    };
  });

  const zoom = () => Number(document.documentElement.style.zoom) || 1;
  /** One column and its gap, in pixels on the page. */
  const step = () => (boardWidth + GAP) / COLS;

  function grab(e: PointerEvent, id: string, edge: Edge) {
    if (e.button !== 0 || narrow) return;
    const origin = at.get(id);
    const start = spots.find((s) => s.id === id);
    if (!origin || !start) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    hold = { id, edge, x0: e.clientX, y0: e.clientY, start, origin, now: start, dx: 0, dy: 0, moved: false, target: null };
  }
  /** The card whose middle the pointer is over, on the board without the held card (the one
   *  shown while over a card, so it holds still): the inner part of each card, well away from
   *  its edges, so a drop near an edge still places the card there. */
  function under(e: PointerEvent, except: string): string | null {
    if (!board) return null;
    const r = board.getBoundingClientRect();
    const z = zoom();
    const col = (e.clientX - r.left) / z / step();
    const row = (e.clientY - r.top) / z / ROW;
    for (const p of settle(spots.filter((s) => s.id !== except), measured)) {
      const ix = p.w * 0.22, iy = p.rows * 0.22;
      if (col >= p.x + ix && col <= p.x + p.w - ix && row >= p.y + iy && row <= p.y + p.rows - iy) return p.id;
    }
    return null;
  }
  function drag(e: PointerEvent) {
    if (!hold) return;
    const z = zoom();
    const dx = (e.clientX - hold.x0) / z;
    const dy = (e.clientY - hold.y0) / z;
    const cols = Math.round(dx / step());
    const rows = Math.round(dy / ROW);
    const { start, origin, edge } = hold;
    let next: Spot = { ...start, y: origin.y };
    if (edge === "move") next = { ...next, x: start.x + cols, y: Math.max(0, origin.y + rows) };
    if (edge === "e" || edge === "se") next.w = Math.min(COLS - start.x, start.w + cols);
    if (edge === "w" || edge === "sw") {
      const d = Math.max(-start.x, Math.min(start.w - 6, cols));
      next = { ...next, x: start.x + d, w: start.w - d };
    }
    if (edge === "s" || edge === "se" || edge === "sw") next.h = Math.max(MIN_H, origin.rows + rows);
    const moved = hold.moved || Math.abs(dx) + Math.abs(dy) > 3;
    // Over the middle of another card the board holds still (the spot stays where it was), so
    // the card under the pointer does not slide away from it.
    const target = edge === "move" && moved ? under(e, hold.id) : null;
    hold = target ? { ...hold, dx, dy, moved, target } : { ...hold, now: clampSpot(next), dx, dy, moved, target: null };
  }
  function drop() {
    if (!hold) return;
    const { moved, target, id } = hold;
    const after = settled(placed);
    hold = null;
    if (target) combine(id, target);
    else if (moved) commit(after);
  }
  /** Sets a card's width, keeping it on the board. */
  function widen(id: string, w: number) {
    commit(spots.map((s) => (s.id === id ? { ...s, w, x: Math.min(s.x, COLS - w) } : s)));
  }
  function openMenu(e: MouseEvent, id: string) {
    if (e.target instanceof Element && e.target.closest("input, textarea, a[href]")) return;
    e.preventDefault();
    const z = zoom();
    menu = { id, x: e.clientX / z, y: e.clientY / z };
  }
  /** A card's colour, see-through background and frame, as CSS the cards' sections pick up. */
  function looks(id: string) {
    void prefs.overviewLooks;
    const l = lookOf(id);
    const parts = [];
    if (l.tint) parts.push(`--tint: var(--p-${l.tint})`);
    if (l.alpha != null && l.alpha !== 1) parts.push(`--card-alpha: ${Math.round(l.alpha * 100)}%`);
    return { style: parts.join("; "), tinted: !!l.tint, clear: l.alpha != null && l.alpha !== 1, bare: !!l.bare };
  }

  /** Back to following what the card holds. */
  function fit(id: string) {
    commit(spots.map((s) => (s.id === id ? { ...s, h: null } : s)));
  }

  // From the shelf: where the card would land under the pointer.
  function cardOver(e: DragEvent) {
    const id = e.dataTransfer?.types.includes("application/x-hub-card") ? shelfCard : null;
    if (!id || !board || narrow) return;
    e.preventDefault();
    const r = board.getBoundingClientRect();
    const z = zoom();
    const w = SIZE_COLS[CARDS.find((c) => c.id === id)?.size ?? "s"];
    const x = Math.round(((e.clientX - r.left) / z) / step() - w / 2);
    const y = Math.max(0, Math.round(((e.clientY - r.top) / z) / ROW - 8));
    const next = clampSpot({ id, x, y, w, h: null });
    if (!incoming || incoming.x !== next.x || incoming.y !== next.y) incoming = next;
  }
  function cardDrop(e: DragEvent) {
    if (!incoming) return;
    e.preventDefault();
    const spot = incoming;
    incoming = null;
    commit(settled(settle([...spots.filter((s) => s.id !== spot.id), spot], measured, spot.id)));
  }
  let shelfCard = $state<string | null>(null);

  function keys(e: KeyboardEvent) {
    if (!editing) return;
    if (e.key === "Escape" && hold) {
      hold = null;
      e.preventDefault();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) {
      e.preventDefault();
      back();
    }
  }

  function area(p: Placed) {
    const src = hold?.edge === "move" && hold.id === p.id ? hold.origin : p;
    return `grid-column: ${src.x + 1} / span ${src.w}; grid-row: ${src.y + 1} / span ${src.rows};`;
  }
  /** The cards on the page, for the page around the board (its Customize bar and its search). */
  export const has = (id: string) => ids.has(id);
  export { addAt, back, reset };
  $effect(() => {
    undoable = undo.length > 0;
  });

</script>

{#snippet card(id: string)}
  <CardFrame {id} name={titleOf(id)} open={how === id} shut={() => (how = null)}>
    {@render content(id)}
  </CardFrame>
{/snippet}

<svelte:window onkeydown={keys} />

  <div class="layout" class:editing>
    <div
      class="board"
      class:narrow
      class:holding={hold !== null}
      bind:this={board}
      role="list"
      ondragover={cardOver}
      ondragleave={(e) => {
        if (!board?.contains(e.relatedTarget as Node)) incoming = null;
      }}
      ondrop={cardDrop}
    >
      {#each narrow ? ordered : placed as p (p.id)}
        {#if p.id !== incoming?.id}
          {@const held = hold?.id === p.id}
          {@const look = looks(p.id)}
          <div
            class="slot"
            class:tinted={look.tinted}
            class:clear={look.clear}
            class:bare={look.bare}
            class:fixed={p.h != null}
            class:held
            class:lifted={held && hold?.edge === "move" && hold.moved}
            style={narrow ? "" : area(p)}
            style:transform={held && hold?.edge === "move" ? `translate(${hold.dx}px, ${hold.dy}px)` : null}
            style:height={!narrow && p.h != null ? `${p.rows * ROW}px` : null}
            data-card={p.id}
            role="listitem"
            oncontextmenu={(e) => openMenu(e, p.id)}
          >
            <div class="body" use:measure={p.id} style={look.style} class:target={hold?.target === p.id}>
              {#if p.with?.length}
                <div class="group">
                  {#each [p.id, ...p.with] as id (id)}
                    <div class="part" data-part={id}>{@render card(id)}</div>
                  {/each}
                </div>
              {:else}
                {@render card(p.id)}
              {/if}
            </div>
            {#if editing && !narrow}
              <div
                class="grab"
                role="button"
                tabindex="-1"
                aria-label="Move {titleOfSpot(p)}"
                onpointerdown={(e) => grab(e, p.id, "move")}
                onpointermove={drag}
                onpointerup={drop}
                onpointercancel={() => (hold = null)}
              >
                <span class="tag">
                  {titleOfSpot(p)}
                  {#if linkCount(p.id)}<span class="chain" data-tip="Linked with {linkCount(p.id)} · right-click to change what they share">⛓ {linkCount(p.id)}</span>{/if}
                  {#if held && hold}<span class="dims">{hold.now.w} of {COLS} wide{hold.now.h != null ? ` · ${hold.now.h * ROW}px high` : ""}</span>{/if}
                </span>
                <button
                  class="off"
                  onpointerdown={(e) => e.stopPropagation()}
                  onclick={() => remove(p.id)}
                  data-tip="Take off the page"><Icon name="x" size={12} /></button
                >
              </div>
              {#each ["e", "w", "s", "se", "sw"] as const as edge}
                <div
                  class="edge {edge}"
                  role="separator"
                  aria-label="Resize"
                  data-tip={edge === "s" ? (p.h != null ? "Drag to set the height · double-click to fit what it holds" : "Drag to set the height") : undefined}
                  onpointerdown={(e) => grab(e, p.id, edge)}
                  onpointermove={drag}
                  onpointerup={drop}
                  onpointercancel={() => (hold = null)}
                  ondblclick={() => edge.includes("s") && fit(p.id)}
                ></div>
              {/each}
            {/if}
          </div>
        {/if}
      {:else}
        <p class="muted empty">{empty}</p>
      {/each}
      {#if !narrow}
        {#if hold?.edge === "move" && hold.moved && !hold.target}
          {@const p = at.get(hold.id)}
          {#if p}<div class="ghost-spot" style="grid-column: {p.x + 1} / span {p.w}; grid-row: {p.y + 1} / span {p.rows};"></div>{/if}
        {/if}
        {#if hold?.target}
          {@const p = at.get(hold.target)}
          {#if p}<div class="combine" style="grid-column: {p.x + 1} / span {p.w}; grid-row: {p.y + 1} / span {p.rows};"><span>Drop to combine</span></div>{/if}
        {/if}
        {#if incoming}
          {@const p = at.get(incoming.id)}
          {#if p}<div class="ghost-spot" style="grid-column: {p.x + 1} / span {p.w}; grid-row: {p.y + 1} / span {Math.max(p.rows, 40)};"><span>{titleOf(p.id)}</span></div>{/if}
        {/if}
      {/if}
      {#if wires.length}
        <svg class="wires" aria-hidden="true">
          {#each wires as w (w.key)}
            <path d={w.d} class:look={w.lookOnly} />
            <circle cx={w.a[0]} cy={w.a[1]} r="3.5" />
            <circle cx={w.b[0]} cy={w.b[1]} r="3.5" />
          {/each}
        </svg>
      {/if}
    </div>

    {#if menu}
      {@const m = menu}
      {@const p = at.get(m.id)}
      {#if p}
        <CardMenu
          id={m.id}
          title={titleOfSpot(p)}
          x={m.x}
          y={m.y}
          width={p.w}
          fitted={p.h == null}
          close={() => (menu = null)}
          resize={(w) => widen(m.id, w)}
          fit={() => fit(m.id)}
          remove={() => remove(m.id)}
          parts={p.with ?? []}
          split={() => split(m.id)}
          partners={partnersOf(m.id)}
          widget={isWidget(m.id)}
          change={(text) => changeWidget(m.id, text)}
          howOpen={how === m.id}
          how={() => (how = how === m.id ? null : m.id)}
          link={(o) => toggleLink(m.id, o)}
          share={(o, what) => toggleShare(m.id, o, what)}
        />
      {/if}
    {/if}

    {#if editing}
      <OverviewShelf
        on={ids}
        make={async (text) => {
          const w = await makeWidget(text);
          addAt(w.id);
        }}
        {toggle}
        pick={(id) => (shelfCard = id)}
        letGo={() => {
          shelfCard = null;
          incoming = null;
        }}
      />
    {/if}
  </div>

<style>
  /* The board: COLS columns and ROW-pixel rows (board.ts). Every card is placed by its spot;
     a fit-to-content card is as tall as it is and spans the rows that takes (`measure`), a card
     with a chosen height fills it and scrolls inside. Narrow pages stack the cards instead. */
  .layout { display: flex; gap: 14px; align-items: flex-start; }
  .board { flex: 1; min-width: 0; display: grid; grid-template-columns: repeat(48, minmax(0, 1fr)); grid-auto-rows: 4px; column-gap: 10px; row-gap: 0; position: relative; }
  .board.narrow { display: flex; flex-direction: column; gap: 10px; }
  .slot { min-width: 0; display: flex; flex-direction: column; align-self: start; position: relative; padding-bottom: 10px; }
  .narrow .slot { padding-bottom: 0; align-self: stretch; }
  .slot.fixed { align-self: stretch; }
  .slot .body { min-width: 0; display: flex; flex-direction: column; }
  .slot.fixed .body { flex: 1; min-height: 0; overflow: auto; border-radius: var(--r-lg); }
  .slot .body > :global(section), .slot .group { flex: 1; margin: 0; }
  /* Combined cards: one frame around the parts, which lose their own. The parts sit under each
     other, or side by side once the frame is wide enough for two; the gap between them, showing
     the frame's background, is the line that divides them. */
  .group { display: grid; grid-template-columns: minmax(0, 1fr); gap: 1px; background: var(--line); border: 1px solid var(--line); border-radius: var(--r-lg); overflow: hidden; container-type: inline-size; }
  @container (min-width: 520px) { .group { grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); } }
  .part { min-width: 0; display: flex; background: var(--surface); }
  .part > :global(section) { flex: 1; min-width: 0; border: 0; border-radius: 0; background: transparent; }
  .slot .body > :global(section) { min-width: 0; }
  .slot .body.target > :global(section), .slot .body.target > .group { outline: 1.5px solid var(--accent); outline-offset: 2px; }
  /* Over the middle of another card: a veil over it, above the lifted card, saying what a drop does. */
  .combine { margin-bottom: 10px; z-index: 5; display: grid; place-items: center; border-radius: var(--r-lg); background: color-mix(in srgb, var(--accent-bg) 70%, transparent); color: var(--accent); font-size: 13px; pointer-events: none; }
  .combine span { background: var(--bg); border: 1px solid var(--accent); border-radius: var(--r-sm); padding: 3px 10px; }
  /* Looks set from the card's right-click menu: a colour tints the frame, background and
     heading; see-through lets the page show through the background; bare drops the frame. */
  .slot.tinted .body > :global(section) { border-color: color-mix(in srgb, var(--tint) 38%, var(--line)); background: color-mix(in srgb, var(--tint) 7%, var(--surface)); }
  .slot.tinted .body :global(.frame-title) { color: var(--tint); }
  .slot.clear .body > :global(section) { background: color-mix(in srgb, var(--surface) var(--card-alpha), transparent); }
  .slot.tinted.clear .body > :global(section) { background: color-mix(in srgb, color-mix(in srgb, var(--tint) 7%, var(--surface)) var(--card-alpha), transparent); }
  .slot.bare .body > :global(section) { border-color: transparent; background: transparent; }
  /* The same looks on a combined frame: the frame and the parts' backgrounds take them. */
  .slot.tinted .group { border-color: color-mix(in srgb, var(--tint) 38%, var(--line)); background: color-mix(in srgb, var(--tint) 38%, var(--line)); }
  .slot.tinted .part { background: color-mix(in srgb, var(--tint) 7%, var(--surface)); }
  .slot.clear .part { background: color-mix(in srgb, var(--surface) var(--card-alpha), transparent); }
  .slot.tinted.clear .part { background: color-mix(in srgb, color-mix(in srgb, var(--tint) 7%, var(--surface)) var(--card-alpha), transparent); }
  .slot.bare .group { border-color: transparent; background: transparent; }
  .slot.bare .part { background: transparent; }
  .board:not(.holding) .slot { transition: transform var(--dur-fast) var(--ease-move); }
  .slot.held { z-index: 3; }
  .slot.lifted { opacity: 0.9; filter: drop-shadow(0 18px 30px rgba(0, 0, 0, 0.55)); transition: none; }
  .ghost-spot { border: 1.5px dashed var(--accent); background: var(--accent-bg); border-radius: var(--r-lg); margin-bottom: 10px; z-index: 1; pointer-events: none; display: grid; place-items: center; color: var(--accent); font-size: 12px; }

  /* Customizing: dots show the grid, the whole card is a handle, and its edges resize it. */
  .editing .board:not(.narrow) { background-image: radial-gradient(circle, var(--line-strong) 1px, transparent 1.3px); background-size: calc((100% + 10px) / 48) 16px; background-position: -5px 0; border-radius: var(--r-lg); outline: 1px dashed var(--line); outline-offset: 6px; }
  .editing .slot .body { outline: 1px dashed var(--line-strong); outline-offset: 2px; border-radius: var(--r-lg); }
  .editing .slot.held .body { outline: 1.5px solid var(--accent); }
  .grab { position: absolute; inset: 0 0 10px; cursor: grab; border-radius: var(--r-lg); z-index: 2; touch-action: none; }
  .holding .grab, .slot.lifted .grab { cursor: grabbing; }
  .grab:hover { background: rgba(165, 171, 238, 0.04); }
  .tag { position: absolute; left: 10px; top: -9px; max-width: calc(100% - 50px); display: flex; gap: 6px; align-items: center; font-size: 12px; color: var(--text-3); background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-sm); padding: 0 8px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tag .dims, .tag .chain { color: var(--accent); }
  .wires { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; overflow: visible; z-index: 4; }
  .wires path { fill: none; stroke: var(--accent); stroke-width: 1.5; stroke-dasharray: 4 4; opacity: 0.75; }
  .wires path.look { stroke-dasharray: 1 4; stroke-linecap: round; }
  .wires circle { fill: var(--bg); stroke: var(--accent); stroke-width: 1.5; }
  .off { position: absolute; right: 8px; top: -10px; width: 20px; height: 20px; display: grid; place-items: center; border-radius: var(--r-sm); color: var(--muted); background: var(--bg); border: 1px solid var(--line-strong); }
  .off:hover { color: var(--red); }
  .edge { position: absolute; z-index: 3; touch-action: none; }
  .edge.e, .edge.w { top: 14px; bottom: 24px; width: 10px; cursor: ew-resize; }
  .edge.e { right: -5px; }
  .edge.w { left: -5px; }
  .edge.s { left: 14px; right: 14px; bottom: 5px; height: 10px; cursor: ns-resize; }
  .edge.se, .edge.sw { width: 14px; height: 14px; bottom: 3px; }
  .edge.se { right: -5px; cursor: nwse-resize; }
  .edge.sw { left: -5px; cursor: nesw-resize; }
  .edge::after { content: ""; position: absolute; border-radius: 3px; background: var(--accent); opacity: 0; transition: opacity 0.1s var(--ease); }
  .edge.e::after, .edge.w::after { left: 4px; width: 2px; top: 30%; bottom: 30%; }
  .edge.s::after { top: 4px; height: 2px; left: 40%; right: 40%; }
  .edge.se::after, .edge.sw::after { inset: 3px; border-radius: 2px; background: none; border: 2px solid var(--accent); }
  .edge.se::after { border-left: 0; border-top: 0; }
  .edge.sw::after { border-right: 0; border-top: 0; }
  .slot:hover .edge::after, .slot.held .edge::after { opacity: 0.8; }
  .empty { grid-column: 1 / -1; grid-row: 1 / span 20; padding: 20px 0; }
  .muted { color: var(--muted); font-size: 12px; }

</style>
