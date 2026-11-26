<!-- The grip on the edge of a pane: drag to resize it, double-click for its default width. The parent must be `position: relative`; the grip is wider than the line it lights, so it is easy to catch. The parent owns the width: this reports it live while dragging, and once on release.
     Used by App.svelte, Sidebar.svelte. -->
<script lang="ts">
  // The grip on the edge of a pane: drag to resize it, double-click for its default width. The
  // parent must be `position: relative`; the grip is wider than the line it lights, so it is easy
  // to catch. The parent owns the width: this reports it live while dragging, and once on release.
  let {
    side,
    min,
    max = Infinity,
    keep = 420,
    scale = () => 1,
    inside = false,
    label,
    onlive,
    oncommit,
    onreset,
  }: {
    /** Which edge of the parent this sits on: "left" for a panel on the right, "right" for the sidebar. */
    side: "left" | "right";
    min: number;
    max?: number;
    /** Room the page beside it keeps however far you drag. */
    keep?: number;
    /** How much bigger than the window the parent is drawn (the sidebar is drawn larger). */
    scale?: () => number;
    /** Inside the parent's edge, for a parent that clips what sticks out. */
    inside?: boolean;
    label: string;
    onlive: (width: number | null) => void;
    oncommit: (width: number) => void;
    onreset: () => void;
  } = $props();

  let dragging = $state(false);

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    const grip = e.currentTarget as HTMLElement;
    const zoom = Number(document.documentElement.style.zoom) || 1;
    const k = zoom * (scale() || 1);
    const start = e.clientX;
    const width = grip.parentElement!.getBoundingClientRect().width / k;
    const mainWidth = (document.querySelector("main")?.getBoundingClientRect().width ?? 0) / zoom;
    // The page beside it gives up what this takes, down to `keep`.
    const room = Math.min(max, width + Math.max(0, mainWidth - keep) * zoom / k);
    let last: number | null = null;
    grip.setPointerCapture(e.pointerId);
    dragging = true;
    const move = (m: PointerEvent) => {
      const delta = (m.clientX - start) / k;
      last = Math.round(Math.min(Math.max(min, room), Math.max(min, width + (side === "left" ? -delta : delta))));
      onlive(last);
    };
    const up = () => {
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", up);
      grip.removeEventListener("pointercancel", up);
      dragging = false;
      if (last != null) oncommit(last);
      onlive(null);
    };
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", up);
    grip.addEventListener("pointercancel", up);
  }
</script>

<div class="edge {side}" class:inside class:dragging role="separator" aria-orientation="vertical" aria-label={label}
  data-tip="Drag to resize · double-click for the default width" onpointerdown={down} ondblclick={onreset}></div>

<style>
  .edge { position: absolute; top: 0; bottom: 0; width: 10px; z-index: 5; cursor: col-resize; touch-action: none; }
  .edge.left { left: -5px; }
  .edge.right { right: -5px; }
  .edge.left.inside { left: 0; }
  .edge.right.inside { right: 0; }
  .edge::after { content: ""; position: absolute; top: 0; bottom: 0; width: 2px; background: transparent; transition: background 0.12s var(--ease); }
  .edge.left::after { left: 4px; }
  .edge.right::after { right: 4px; }
  .edge.left.inside::after { left: 0; }
  .edge.right.inside::after { right: 0; }
  .edge:hover::after, .edge.dragging::after { background: var(--accent); }
  /* Too narrow for panels the viewer covers the window, and there is nothing to drag. */
  @media (max-width: 1100px) { .edge.left { display: none; } }
</style>
