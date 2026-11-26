// Enter and exit for things that sit above the page: modals with their backdrop, the lightbox, the
// find bar, the bottom panel and the toast. One small set of Svelte transitions so each place is a
// one-line change: `in:modal|global out:modal|global={{ out: true }}`.
//
// design/ANIMATION.md: ease-out in, no bounce, exits about 20% shorter than entrances, the backdrop
// fades, the card starts at 0.96 and never from 0. Reduced motion (the system setting or Settings >
// Appearance > Reduce motion) removes the movement and the wait.
import { quintOut } from "svelte/easing";

/** Entrance length; the exit is 20% shorter. Matches the guide's `pop` token for the small things. */
const IN = 180;
const OUT = Math.round(IN * 0.8);

type Opts = {
  /** This is the exit. */
  out?: boolean;
  /** Selector of the part that rises with the backdrop; the first child when left out. */
  card?: string;
  /** Where the card grows from, as a CSS transform-origin (the palette hangs from the top). */
  origin?: string;
};

export const still = () =>
  typeof document === "undefined" ||
  document.documentElement.classList.contains("calm") ||
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

const length = (o: Opts) => (still() ? 0 : o.out ? OUT : IN);

/** A modal: the backdrop fades, the card eases up from 0.96 and a few pixels below. */
export function modal(node: HTMLElement, o: Opts = {}) {
  const card = (o.card ? node.querySelector<HTMLElement>(o.card) : node.firstElementChild) as HTMLElement | null;
  if (card && o.origin) card.style.transformOrigin = o.origin;
  return {
    duration: length(o),
    easing: quintOut,
    tick(t: number) {
      node.style.opacity = String(t);
      if (card) card.style.transform = t === 1 ? "" : `translateY(${(1 - t) * 6}px) scale(${0.96 + 0.04 * t})`;
    },
  };
}

/** Something that slides in from an edge: `y` is where it starts (negative: above its place). */
export function slide(node: HTMLElement, o: Opts & { y?: number } = {}) {
  const y = o.y ?? -8;
  return {
    duration: length(o),
    easing: quintOut,
    tick(t: number) {
      node.style.opacity = String(t);
      node.style.transform = t === 1 ? "" : `translateY(${(1 - t) * y}px)`;
    },
  };
}

/** The toast keeps its own centring (`translateX(-50%)`) and rises 8px. */
export function toast(node: HTMLElement, o: Opts = {}) {
  return {
    duration: length(o),
    easing: quintOut,
    tick(t: number) {
      node.style.opacity = String(t);
      node.style.transform = t === 1 ? "" : `translateX(-50%) translateY(${(1 - t) * 8}px)`;
    },
  };
}
