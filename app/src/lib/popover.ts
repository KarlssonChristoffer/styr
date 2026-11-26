// The one entrance and exit for a floating layer (menu, select list, picker, sheet, dialog card).
// Use as `transition:pop` on the layer's own element. It grows from `scale(0.95)` and fades in over
// 160 ms with `--ease`, and leaves in 130 ms, from and back to the trigger: the origin is the spot
// the user last pressed (or the focused control, for the keyboard), clamped onto the layer's box.
// `{ origin: "top left" }` overrides that for a layer that has a fixed corner, `{ origin: "50% 0" }` etc.
// It animates the `scale` property, never `transform`, so a layer that positions itself with a
// transform (a list above its trigger) keeps working. Reduced motion: no animation.
// Rules: design/DESIGN.md "Animation rules", design/ANIMATION.md.

import type { TransitionConfig } from "svelte/transition";

export const POP_IN_MS = 160;
export const POP_OUT_MS = 130;
const START = 0.95;

/** `--ease` as a function: cubic-bezier(0.22, 1, 0.36, 1). */
function ease(t: number): number {
  const [x1, y1, x2, y2] = [0.22, 1, 0.36, 1];
  const bez = (a: number, b: number, s: number) => 3 * a * s * (1 - s) ** 2 + 3 * b * s ** 2 * (1 - s) + s ** 3;
  let lo = 0, hi = 1, s = t;
  for (let i = 0; i < 20; i++) {
    s = (lo + hi) / 2;
    if (bez(x1, x2, s) < t) lo = s; else hi = s;
  }
  return bez(y1, y2, s);
}

let last: { x: number; y: number; at: number } | null = null;
if (typeof window !== "undefined") {
  // Capture phase: the press is noted before the click handler that opens the layer runs.
  window.addEventListener("pointerdown", (e) => (last = { x: e.clientX, y: e.clientY, at: performance.now() }), true);
}

function reduced() {
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Where on `node` the layer grows from, as percentages of its own box (so page zoom does not matter). */
function originOf(node: Element): string {
  const r = node.getBoundingClientRect();
  if (!r.width || !r.height) return "top left";
  let x: number, y: number;
  if (last && performance.now() - last.at < 400) {
    ({ x, y } = last);
  } else {
    const a = document.activeElement;
    const f = a && a !== document.body && !node.contains(a) ? a.getBoundingClientRect() : null;
    if (!f) return "50% 0";
    x = f.left + f.width / 2;
    y = f.top + f.height / 2;
  }
  const pct = (v: number, from: number, size: number) => Math.round(Math.min(1, Math.max(0, (v - from) / size)) * 100);
  return `${pct(x, r.left, r.width)}% ${pct(y, r.top, r.height)}%`;
}

/** The origin found on the way in, so the layer goes back where it came from even though the
 *  press that closes it is somewhere else. */
const came = new WeakMap<Element, string>();

export function pop(node: Element, params: { origin?: string | ((node: Element) => string) } = {}) {
  // Svelte 5 calls a returned function with the direction ("in" or "out"); its types only know the plain form.
  const config = ({ direction }: { direction: "in" | "out" }): TransitionConfig => {
    if (reduced()) return { duration: 0 };
    if (direction === "in" || !came.has(node)) came.set(node, typeof params.origin === "function" ? params.origin(node) : (params.origin ?? originOf(node)));
    const origin = came.get(node)!;
    return {
      duration: direction === "in" ? POP_IN_MS : POP_OUT_MS,
      css: (t: number) => {
        const e = ease(t);
        return `transform-origin: ${origin}; scale: ${START + (1 - START) * e}; opacity: ${Math.min(1, e * 1.4)};`;
      },
    };
  };
  return config as unknown as () => TransitionConfig;
}
