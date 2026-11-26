// Lists that reorder slide instead of jumping. After each change the rows that moved are drawn
// back where they were and glide into place, new rows fade in from the row above them and rows
// that left fade out where they were. A move that interrupts another starts from wherever the
// row is on screen, so a burst of updates reads as one motion.
// Adapted from T3 Code's Sidebar.motion.ts (MIT, (c) 2026 T3 Tools Inc), see THIRD_PARTY_NOTICES.md.

const timing: KeyframeAnimationOptions = { duration: 150, easing: "cubic-bezier(0.22, 1, 0.36, 1)" };
const travelFor = (height: number) => Math.min(height, 40);
// Every fade clones a row and every layout read reflows; past this many it just snaps.
const MAX_FADES = 40;

type Box = { top: number; left: number; width: number; height: number };

function progress(a: Animation) {
  return a.playState === "finished" ? 1 : (a.effect?.getComputedTiming().progress ?? 0);
}

/** Rows marked `data-motion="snap"` (a note that stands in for an empty list) appear and go at
 *  once: fading them in and out each time a thread passes through reads as a twitch. Rows around
 *  them still slide. */
const snaps = (node: HTMLElement) => node.dataset.motion === "snap";

/** Animates the direct children of `parent` whenever they are added, removed, moved or resized. */
export function createListMotion(parent: HTMLElement) {
  let boxes: Map<HTMLElement, Box> | null = null;
  const reduced = parent.ownerDocument.defaultView?.matchMedia("(prefers-reduced-motion: reduce)");
  const moving = new Map<HTMLElement, { animation: Animation; offset: number }>();
  const entering = new Map<HTMLElement, { animation: Animation; travel: number }>();
  const exiting = new Map<HTMLElement, Animation>();

  /** How far a row still is from its place, from the animations it is in the middle of. */
  const left = (node: HTMLElement) => {
    const m = moving.get(node);
    const e = entering.get(node);
    return (m ? m.offset * (1 - progress(m.animation)) : 0) + (e ? e.travel * (1 - progress(e.animation)) : 0);
  };

  const clearFades = () => {
    for (const e of entering.values()) e.animation.cancel();
    for (const [node, a] of exiting) {
      a.cancel();
      node.remove();
    }
    entering.clear();
    exiting.clear();
  };

  // The row is gone from the DOM, so a copy of it fades out in its old place.
  const fadeOut = (node: HTMLElement, box: Box, travel: number) => {
    const clone = node.cloneNode(true) as HTMLElement;
    for (const el of [clone, ...clone.querySelectorAll("[id]")]) if (!(el instanceof SVGElement)) el.removeAttribute("id");
    clone.setAttribute("aria-hidden", "true");
    clone.inert = true;
    Object.assign(clone.style, {
      position: "absolute",
      top: `${box.top + left(node)}px`,
      left: `${box.left}px`,
      width: `${box.width}px`,
      height: `${box.height}px`,
      margin: "0",
      boxSizing: "border-box",
      transform: "none",
      transition: "none",
      pointerEvents: "none",
    });
    parent.append(clone);
    const from = entering.get(node);
    const a = clone.animate(
      [
        { opacity: from ? progress(from.animation) : 1, transform: "translateY(0px)" },
        { opacity: 0, transform: `translateY(${travel}px)` },
      ],
      timing,
    );
    exiting.set(clone, a);
    a.addEventListener("finish", () => {
      clone.remove();
      exiting.delete(clone);
    }, { once: true });
  };

  const cancel = (node: HTMLElement) => {
    moving.get(node)?.animation.cancel();
    moving.delete(node);
  };

  const move = (node: HTMLElement, offset: number) => {
    cancel(node);
    const e = entering.get(node);
    if (e) e.travel = 0;
    if (offset === 0 && !e) return;
    const animation = node.animate([{ transform: `translateY(${offset}px)` }, { transform: "translateY(0px)" }], timing);
    moving.set(node, { animation, offset });
    animation.addEventListener("finish", () => {
      if (moving.get(node)?.animation === animation) moving.delete(node);
    }, { once: true });
  };

  function update(animate: boolean) {
    const next = new Map<HTMLElement, Box>();
    for (const node of parent.children) {
      if (!(node instanceof HTMLElement) || exiting.has(node)) continue;
      next.set(node, { top: node.offsetTop, left: node.offsetLeft, width: node.offsetWidth, height: node.offsetHeight });
    }
    const before = boxes;
    let fades = 0;
    if (before) {
      for (const [node, box] of before) if (!next.has(node) && box.height > 0) fades++;
      for (const [node, box] of next) if (!before.has(node) && box.height > 0) fades++;
    }
    const go = animate && before !== null && !reduced?.matches && !parent.ownerDocument.documentElement.classList.contains("calm") && fades <= MAX_FADES;

    const delta = new Map<HTMLElement, number>();
    const order = [...next.keys()];
    const oldOrder = before ? [...before.keys()] : [];
    // A row that comes or goes travels with the nearest row above it that stays, so a section
    // opening or closing slides as one piece.
    const riding = (list: HTMLElement[], index: number, kept: (n: HTMLElement) => boolean) => {
      for (let i = index - 1; i >= 0; i--) {
        const d = delta.get(list[i]);
        if (d !== undefined) return d;
        if (kept(list[i])) return left(list[i]);
      }
      return undefined;
    };

    if (!go) clearFades();
    else {
      for (const [node, box] of next) {
        const was = before!.get(node)?.top;
        if (was !== undefined && was !== box.top) delta.set(node, was + left(node) - box.top);
      }
      for (const [node, box] of before!) {
        if (next.has(node) || box.height === 0 || snaps(node)) continue;
        const d = riding(oldOrder, oldOrder.indexOf(node), (n) => next.has(n));
        fadeOut(node, box, d === undefined ? travelFor(box.height) : -d);
      }
    }
    for (const [node, e] of entering) {
      if (!next.has(node)) {
        e.animation.cancel();
        entering.delete(node);
      }
    }
    for (const node of moving.keys()) if (!go || !next.has(node)) cancel(node);

    if (go) {
      for (const [index, node] of order.entries()) {
        const box = next.get(node)!;
        if (!before!.has(node)) {
          if (box.height === 0 || snaps(node)) continue;
          const d = riding(order, index, (n) => before!.has(n));
          const travel = d === undefined ? -travelFor(box.height) : d;
          const animation = node.animate(
            [{ opacity: 0, transform: `translateY(${travel}px)` }, { opacity: 1, transform: "translateY(0px)" }],
            timing,
          );
          entering.set(node, { animation, travel });
          animation.addEventListener("finish", () => {
            if (entering.get(node)?.animation === animation) entering.delete(node);
          }, { once: true });
          continue;
        }
        const d = delta.get(node);
        if (d !== undefined) move(node, d);
      }
    }
    boxes = next;
  }

  return {
    update,
    dispose() {
      for (const node of moving.keys()) cancel(node);
      clearFades();
      boxes = null;
    },
  };
}

/** The event that makes `use:motion` take its rows' places as they are now, without animating. */
export const REMEASURE = "motion:remeasure";

/**
 * `use:motion` on a list: its direct children slide when they reorder. It watches the DOM itself,
 * so it runs after Svelte has put every row in place and before the frame is painted.
 */
export function motion(node: HTMLElement) {
  const list = createListMotion(node);
  if (getComputedStyle(node).position === "static") node.style.position = "relative";
  list.update(false);
  // Our own fading copies come and go too; they are skipped when measuring, so they move nothing.
  // While a group folds open or shut (`data-fold`), its own animation moves what is below it.
  const watch = new MutationObserver(() => list.update(node.isConnected && node.offsetParent !== null && !node.dataset.fold));
  watch.observe(node, { childList: true, subtree: true, attributes: true, attributeFilter: ["class"], characterData: true });
  // A new width rewraps rows; take their new places without animating them. Height changes are
  // the rows coming and going, which the mutations already cover.
  let width = node.offsetWidth;
  const resized = new ResizeObserver(() => {
    if (node.offsetWidth === width) return;
    width = node.offsetWidth;
    list.update(false);
  });
  resized.observe(node);
  // Something that moved rows without a mutation (a group's height animating open, `fold.ts`)
  // asks for its new places to be taken without animating them.
  const remeasure = () => list.update(false);
  node.addEventListener(REMEASURE, remeasure);
  return {
    destroy() {
      node.removeEventListener(REMEASURE, remeasure);
      watch.disconnect();
      resized.disconnect();
      list.dispose();
    },
  };
}
