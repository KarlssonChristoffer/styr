// How a group at the bottom of the activity list (From Hub, Snoozed, Settled, Working) opens and
// folds, as Svelte transitions on the box that holds its rows. The box grows from nothing under
// the heading with its overflow clipped, so a row can never show over the heading or the rows
// around it; the groups below and the spacer above give way as it grows. All of it is CSS
// keyframes, so nothing runs per frame in script except the sidebar following a group that
// opens past its bottom (`follow`).
import type { TransitionConfig } from "svelte/transition";
import { prefs } from "./prefs.svelte";

export type GroupMotion = "unfold" | "cascade" | "drawer" | "now";

const SOFT = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2); // ease-in-out cubic
const OUT = (t: number) => 1 - (1 - t) ** 3; // ease-out cubic

/** The motion picked in Settings > Sidebar, or none under Reduce motion. A value from a newer
 *  build reads as Fold out. */
export function groupMotion(): GroupMotion | null {
  if (prefs.reduceMotion) return null;
  return (["unfold", "cascade", "drawer", "now"] as const).find((m) => m === prefs.groupMotion) ?? "unfold";
}

const TIME: Record<GroupMotion, { open: number; shut: number }> = {
  unfold: { open: 320, shut: 240 },
  cascade: { open: 360, shut: 260 },
  drawer: { open: 340, shut: 260 },
  now: { open: 160, shut: 140 },
};

/** How long a group of this motion takes to open, for `follow`. */
export const openTime = (m: GroupMotion) => TIME[m].open;

const reach = (node: HTMLElement) => node.closest<HTMLElement>(".scroll")?.clientHeight || innerHeight;

/** The box around a group's rows: its height grows from 0 or shrinks to 0, clipped. */
export function clipBox(node: HTMLElement, _: unknown, { direction }: { direction: "in" | "out" | "both" }): TransitionConfig {
  const m = groupMotion();
  if (!m) return { duration: 0 };
  // A long group (Settled) grows only as far as the sidebar is tall and takes the rest at the
  // end, below the edge: growing to its whole height in the same time would rush past.
  const h = Math.min(node.scrollHeight, reach(node));
  const opening = direction === "in";
  return {
    duration: opening ? TIME[m].open : TIME[m].shut,
    easing: m === "now" ? OUT : SOFT,
    css: (t) => `overflow: hidden; height: ${t * h}px;`,
  };
}

/** The rows inside the box. Drawer slides them out from under the heading as one piece; the
 *  others fade them, while each row's own entrance (`.stagger` in the list) folds them out. */
export function shift(node: HTMLElement, _: unknown, { direction }: { direction: "in" | "out" | "both" }): TransitionConfig {
  const m = groupMotion();
  if (!m) return { duration: 0 };
  const h = Math.min(node.offsetHeight, reach(node));
  const opening = direction === "in";
  const duration = opening ? TIME[m].open : TIME[m].shut;
  if (m === "drawer") return { duration, easing: SOFT, css: (t, u) => `transform: translateY(${-u * h}px);` };
  // Fading in is the rows' own job when opening; folding fades the whole group a little ahead
  // of the box, so no half-cut row is left in the last frames.
  if (opening) return { duration: 0 };
  return { duration, easing: OUT, css: (t) => `opacity: ${Math.min(1, t * 1.6)};` };
}

let following: (() => void) | null = null;

/** While a group opens, keeps the sidebar scrolled so its heading rises to the top when its rows
 *  run past the bottom, in step with the box growing. A wheel, finger or key stops it. */
export function follow(view: HTMLElement, head: HTMLElement, ms: number) {
  following?.();
  // Where the heading is in the scrolled content. Nothing moves it by transform meanwhile: the
  // list's own motion stands aside while a group folds.
  const top = () => head.getBoundingClientRect().top - view.getBoundingClientRect().top + view.scrollTop;
  let frame = 0;
  const end = performance.now() + ms + 40;
  const stop = () => {
    cancelAnimationFrame(frame);
    for (const e of ["wheel", "pointerdown", "keydown"]) view.removeEventListener(e, stop);
    if (following === stop) following = null;
  };
  following = stop;
  for (const e of ["wheel", "pointerdown", "keydown"]) view.addEventListener(e, stop, { passive: true });
  const step = (now: number) => {
    const max = view.scrollHeight - view.clientHeight;
    if (max > 0) view.scrollTop = Math.min(Math.max(0, top() - 6), max);
    if (now < end) frame = requestAnimationFrame(step);
    else stop();
  };
  frame = requestAnimationFrame(step);
}
