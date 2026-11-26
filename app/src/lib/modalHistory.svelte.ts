// Back and forward inside a window over the page. While one is open, the mouse's side buttons
// and Alt ← / Alt → step through its own places (Settings' pages) and leave the page behind it
// alone. A window with no places of its own just keeps them from reaching the page.
import { untrack } from "svelte";

type Nav = { step(dir: -1 | 1): void };

/** Open windows that have places, the newest last. */
const open: Nav[] = [];

/** Where a press of back or forward goes first. True when a window took it. */
export function stepInModal(dir: -1 | 1): boolean {
  const top = open.at(-1);
  if (top) {
    top.step(dir);
    return true;
  }
  return !!document.querySelector('[aria-modal="true"]');
}

/** Call while a window component starts. `get` says where in the window you are, `set` goes
 *  back there. Every change of place becomes a step, as in the window's own small history. */
export function useModalHistory<T>(get: () => T, set: (place: T) => void) {
  let places: string[] = [];
  let at = -1;
  $effect(() => {
    const here = JSON.stringify(get());
    untrack(() => {
      if (places[at] === here) return;
      places = [...places.slice(0, at + 1), here];
      at = places.length - 1;
    });
  });
  const nav: Nav = {
    step(dir) {
      const to = at + dir;
      if (to < 0 || to >= places.length) return;
      at = to;
      set(JSON.parse(places[to]));
    },
  };
  $effect(() => {
    open.push(nav);
    return () => {
      const i = open.indexOf(nav);
      if (i >= 0) open.splice(i, 1);
    };
  });
}
