// What Guard's fix threads are doing, in a few words, for the Problems page's Guard panel and the
// status bar's Guard card. A thread that waits on you is the only one that reaches the inbox
// ("Guard needs you"); the rest are Guard's own business and are only listed here. With a core
// that sends stuck fix threads back to Guard (`guard.reports`), only a live permission question
// still waits on you.
import type { Thread } from "./bindings/Thread";
import { asksYou, hub, waitsOnYou } from "./state.svelte";

export type GuardTone = "work" | "ask" | "bad" | "";

export function threadSays(t: Thread): [GuardTone, string] {
  if (t.status === "running") return ["work", "fixing"];
  // A fix thread that stopped goes back to Guard, which decides what happens next.
  if (hub.guardHandles(t) && (t.status === "error" || asksYou(t))) return ["", "back with Guard"];
  if (t.status === "error") return ["bad", "failed"];
  if (waitsOnYou(t)) return ["ask", t.reading === "stuck" ? "stuck" : "asks you"];
  if (t.status === "in_terminal") return ["work", "in a terminal"];
  return ["", "idle"];
}
