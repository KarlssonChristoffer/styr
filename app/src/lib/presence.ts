// Tells the machine a thread runs on that you are at the computer with that thread open, so the
// phone gets no notice about what you see in front of you (`POST /api/presence`, `ntfy.rs`).
// Said again every 30 seconds while the window is in front, the moment you switch thread or
// window, and "away" when it leaves. Nothing here is shown and a machine that does not know the
// call (an older Hub) is skipped (compat-until: 2026-12-03).
import { quiet } from "./quiet";
import { api } from "./api";
import { hubdHas } from "./compat";
import { poll } from "./poll";

const BEAT_MS = 30_000;
const client = `desktop-${crypto.randomUUID().slice(0, 8)}`;

export type Seen = { id: string; host: string | null } | null;

let lastInput = Date.now();
let shown: Seen = null;
/** The machine last told you are here, so it is told when you leave. */
let told: { host: string | null; thread: string } | null = null;

/** How long ago the last key or mouse movement in this window was, for the quiet rule "while I'm at the PC". */
export const idleMs = () => Date.now() - lastInput;

const inFront = () => document.hasFocus() && !document.hidden;

/** Whether this machine's hubd knows the call; until it does (an update on the way) nothing is sent. */
let known: Promise<boolean> | null = null;

function send(host: string | null, active: boolean, thread: string | null) {
  known ??= hubdHas("presence.desktop"); // compat-until: 2026-12-03
  void known.then((ok) => {
    if (ok) quiet(api.presence(host, { client, active, thread, idle_ms: Date.now() - lastInput }), "presence");
  });
}

function report() {
  const here = shown && inFront() ? shown : null;
  if (told && (!here || here.id !== told.thread || here.host !== told.host)) {
    send(told.host, false, null);
    told = null;
  }
  if (here) {
    send(here.host, true, here.id);
    told = { host: here.host, thread: here.id };
  }
}

/** The thread open in the window now, or null when a page other than a thread is. */
export function seeing(now: Seen) {
  if (now?.id === shown?.id && now?.host === shown?.host) return;
  shown = now;
  report();
}

/** Starts listening; returns what stops it. Call once, from the main window only. */
export function startPresence(): () => void {
  const used = () => (lastInput = Date.now());
  const events = ["keydown", "pointerdown", "pointermove", "wheel"] as const;
  for (const e of events) window.addEventListener(e, used, { passive: true });
  const change = () => report();
  // Coming back into view is `poll`'s: it says it at once then.
  const hiding = () => document.hidden && report();
  window.addEventListener("focus", change);
  window.addEventListener("blur", change);
  document.addEventListener("visibilitychange", hiding);
  const stop = poll(report, BEAT_MS);
  return () => {
    for (const e of events) window.removeEventListener(e, used);
    window.removeEventListener("focus", change);
    window.removeEventListener("blur", change);
    document.removeEventListener("visibilitychange", hiding);
    stop();
    shown = null;
    report();
  };
}
