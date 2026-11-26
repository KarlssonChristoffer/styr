// Changing world (Dev and Chat) with one move: the other world grows as a circle from where you
// clicked, over the old one (the View Transitions API, which the app's webview has). Without it,
// or with reduced motion, the change is a cut.
import { flushSync } from "svelte";
import { hub, type World } from "./state.svelte";
import { quietly } from "./quiet";

type Transition = { ready: Promise<void> };
type WithTransition = Document & { startViewTransition?: (update: () => void) => Transition };

/** Goes to the other world (or `to`). `from` is the point the circle grows from. */
export function changeWorld(to?: World, from?: { x: number; y: number }) {
  const go = () => flushSync(() => hub.switchWorld(to));
  const doc = document as WithTransition;
  if (!doc.startViewTransition || matchMedia("(prefers-reduced-motion: reduce)").matches) return go();
  const zoom = Number(document.documentElement.style.zoom) || 1;
  const x = (from?.x ?? 26) / zoom;
  const y = (from?.y ?? 26) / zoom;
  const r = Math.hypot(Math.max(x, innerWidth / zoom - x), Math.max(y, innerHeight / zoom - y));
  const t = doc.startViewTransition(go);
  t.ready
    .then(() =>
      document.documentElement.animate(
        { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${r}px at ${x}px ${y}px)`] },
        { duration: 480, easing: "cubic-bezier(0.65, 0, 0.35, 1)", pseudoElement: "::view-transition-new(root)" },
      ),
    )
    .catch(quietly("world switch animation"));
}
