// The clock every working grid on screen plays by (`WorkGrid.svelte`): one frame loop however many
// grids there are, running only while one is shown and moving. Each loop is a pure function of
// this time, so all grids stay in step.
import { LOOP } from "./workGrid";

let watching = 0;
let frame = 0;

export const workClock = $state({ t: 0 });

function step() {
  workClock.t = performance.now() % LOOP;
  frame = requestAnimationFrame(step);
}

/** Keeps the clock running while the caller is shown; returns what stops it again. */
export function watchWorkClock(): () => void {
  if (watching++ === 0) step();
  return () => {
    if (--watching === 0) cancelAnimationFrame(frame);
  };
}
