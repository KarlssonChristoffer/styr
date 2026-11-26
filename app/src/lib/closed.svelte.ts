// What was closed in the right panel, newest last, so Ctrl Shift T brings it back as in a browser
// (after T3 Code's ReopenClosedViewShortcut). Only what is closed counts: switching to another
// picture in the viewer is not closing it.
import { hub, type Viewing } from "./state.svelte";

type Pull = NonNullable<typeof hub.pull>;
type Closed = { viewing: Viewing } | { pull: Pull };

const stack: Closed[] = [];

export function closed(what: Closed) {
  stack.push(what);
  if (stack.length > 20) stack.shift();
}

/** Opens the last closed thing again, in its thread. False when there is nothing to bring back. */
export function reopen(): boolean {
  for (let what = stack.pop(); what; what = stack.pop()) {
    const thread = "viewing" in what ? what.viewing.thread : what.pull.thread;
    if (!hub.threads.some((t) => t.id === thread)) continue;
    if (hub.view !== "thread" || hub.selected !== thread) hub.open(thread);
    if ("viewing" in what) hub.viewing = what.viewing;
    else hub.pull = what.pull;
    return true;
  }
  return false;
}
