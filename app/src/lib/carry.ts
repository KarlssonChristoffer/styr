// What one page hands the next when the window switches between the installed pages and Live
// mode's (`live.svelte.ts`, the shell's `live.rs`). The two come from different origins, so each
// has its own storage: the page that leaves sends its storage along (drafts, zoom, where it was),
// and the one that comes puts it in its own before anything reads it. After a fall back, the
// parcel also says why. Kept free of the app's other modules, since it runs before them.

export const NOTE_KEY = "hub.liveNote";

/** The desktop window this page is in (`main`, `dev`, `viewer-…`); none outside the shell. */
export function windowLabel(): string | undefined {
  return (window as { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } } }).__TAURI_INTERNALS__?.metadata?.currentWindow?.label;
}

/** The main window showing Live mode's pages: dev mode's live UI, on its port. */
export const showingLive = windowLabel() === "main" && location.port === "1421";

/** This window's storage, to hand over. */
export function storage(): Record<string, string> {
  const out: Record<string, string> = {};
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      if (key !== null) out[key] = localStorage.getItem(key) ?? "";
    }
  } catch {
    // Without storage there is nothing to hand over.
  }
  return out;
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

/** Takes over what the page before handed this one, when it did (`?carry` in the address). */
export async function arrive() {
  const query = new URLSearchParams(location.search);
  if (!query.has("carry")) return;
  query.delete("carry");
  const rest = query.toString();
  history.replaceState(history.state, "", location.pathname + (rest ? `?${rest}` : "") + location.hash);
  if (!("__TAURI_INTERNALS__" in window)) return;
  try {
    const parcel = await invoke<string | null>("live_take");
    if (!parcel) return;
    const { storage, note } = JSON.parse(parcel) as { storage?: Record<string, string>; note?: string };
    for (const [key, value] of Object.entries(storage ?? {})) {
      try {
        localStorage.setItem(key, value);
      } catch {
        // A full storage keeps what it has.
      }
    }
    if (note) sessionStorage.setItem(NOTE_KEY, note);
  } catch {
    // An older shell has no Live mode; nothing was handed over.
  }
}

/** Live pages that broke send the window back to the installed ones, saying why. */
export async function leaveBroken(note: string) {
  await invoke("live_leave", { parcel: JSON.stringify({ storage: storage(), note }) });
}
