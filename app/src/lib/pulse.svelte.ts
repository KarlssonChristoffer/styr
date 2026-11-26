// The pulse: how many tool calls the agents have made lately, counted from the stream this window
// already listens to. Nothing is stored or asked for; a window that was closed knows nothing of
// the time it was closed. `n` changes with every call so a card that reads it redraws.
import type { Event } from "./bindings/Event";

const KEEP_MS = 10 * 60_000;
let stamps: number[] = [];

export const pulse = $state({ n: 0, since: Date.now() });

/** Counts an event if it is an agent starting a tool call. */
export function notePulse(e: Event) {
  if (e.kind !== "update") return;
  const d = e.data as { sessionUpdate?: string } | null;
  if (d?.sessionUpdate !== "tool_call") return;
  const now = Date.now();
  stamps.push(now);
  if (stamps.length > 5000 || stamps[0] < now - KEEP_MS) stamps = stamps.filter((t) => t >= now - KEEP_MS);
  pulse.n++;
}

/** Tool calls in each of `buckets` slices of `slice` ms, the last one ending now. */
export function rate(now: number, buckets: number, slice: number): number[] {
  void pulse.n;
  const out = new Array<number>(buckets).fill(0);
  for (const t of stamps) {
    const i = buckets - 1 - Math.floor((now - t) / slice);
    if (i >= 0 && i < buckets) out[i]++;
  }
  return out;
}
