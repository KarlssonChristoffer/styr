// Fix threads started from the Problems page run in the background (`problems::start_thread`
// makes them unattended: a fitting small model, Auto mode, Hub answers their questions). Starting
// one never opens it: the button answers at once, a dot flies to Inbox's icon on the rail, and
// the row says it is sent. The core may take a while to make the thread (a dev worktree, another
// machine), so nothing waits on it; a failure comes back as an error.
import { api } from "./api";
import { hub } from "./state.svelte";

type State = "sending" | "sent";

/** Problems being handed to a fix thread right now, by `host:id` ("" for this machine). */
export const fixes = $state<{ by: Record<string, State>; landed: number }>({ by: {}, landed: 0 });

const key = (id: number, host?: string | null) => `${host ?? ""}:${id}`;

/** Where the fix of a problem stands while it is handed over: "sending", "sent" or null. */
export function fixState(id: number, host?: string | null): State | null {
  return fixes.by[key(id, host)] ?? null;
}

/** Starts a background fix thread on problem `id`. `from` is the button pressed, for the dot. */
export async function startFix(id: number, plan: string | null, host: string | null | undefined, from?: Element | null) {
  const k = key(id, host);
  if (fixes.by[k]) return;
  fixes.by[k] = "sending";
  fly(from);
  try {
    let r = await api.problemThread(id, plan, host);
    // The machine the problem is on knows no folder for it (Hub's own code is often only here):
    // this machine starts it in its project of the same name.
    if (!r.thread && host && host !== hub.localHost) r = await api.problemFixHere(id, plan, host);
    if (r.thread) {
      hub.upsert(r.thread);
      fixes.by[k] = "sent";
      hub.problemsTick++;
      setTimeout(() => delete fixes.by[k], 4000);
    } else {
      // No machine knows a folder for its project: only then does it need the user, who picks one.
      delete fixes.by[k];
      hub.startDraft(r.folder ?? hub.draftFolder(), r.prompt);
      hub.tell("No machine knows this project's folder: pick it, then send");
    }
  } catch (e) {
    delete fixes.by[k];
    hub.fail(e);
  }
}

/** A dot from the button to Inbox's icon on the rail, which then plays a row landing on top. */
function fly(from?: Element | null) {
  const to = document.querySelector("[data-inbox]");
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (!from || !to || reduced) {
    fixes.landed++;
    return;
  }
  const a = from.getBoundingClientRect();
  const b = to.getBoundingClientRect();
  const dot = document.createElement("div");
  dot.className = "fix-dot";
  document.body.append(dot);
  const x0 = a.left + a.width / 2;
  const y0 = a.top + a.height / 2;
  const dx = b.left + b.width / 2 - x0;
  const dy = b.top + b.height / 2 - y0;
  dot.style.left = `${x0}px`;
  dot.style.top = `${y0}px`;
  // An arc: up first, then down onto the icon.
  const lift = Math.min(120, Math.abs(dx) / 4);
  const run = dot.animate(
    [
      { transform: "translate(-50%, -50%) scale(1)", opacity: 1 },
      { transform: `translate(calc(${dx * 0.5}px - 50%), calc(${dy * 0.5 - lift}px - 50%)) scale(0.9)`, opacity: 1, offset: 0.5 },
      { transform: `translate(calc(${dx}px - 50%), calc(${dy}px - 50%)) scale(0.5)`, opacity: 0.6 },
    ],
    { duration: 520, easing: "cubic-bezier(0.4, 0, 0.6, 1)" },
  );
  run.onfinish = () => {
    dot.remove();
    fixes.landed++;
  };
}
