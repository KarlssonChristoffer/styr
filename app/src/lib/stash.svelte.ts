// Prompts put away with Ctrl S to send later, after T3 Code's prompt stash. One list for all of Hub,
// so something written in one thread can be taken out in another. Kept in the browser's storage;
// when big pictures do not fit there, the text is kept without them.
import type { Attachment } from "./bindings/Attachment";

export type Stashed = { text: string; attachments: Attachment[]; at: number };

const KEY = "hub.stash";

function load(): Stashed[] {
  try {
    const list = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(list) ? list : [];
  } catch {
    return [];
  }
}

export const stash = $state<{ list: Stashed[] }>({ list: load() });

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify(stash.list));
  } catch {
    try {
      const light = stash.list.map((s) => ({ ...s, attachments: s.attachments.filter((a) => !a.data) }));
      localStorage.setItem(KEY, JSON.stringify(light));
    } catch {
      // Still there until the window closes.
    }
  }
}

/** Newest first. */
export function putAway(text: string, attachments: Attachment[]) {
  stash.list = [{ text, attachments, at: Date.now() }, ...stash.list].slice(0, 30);
  save();
}

export function takeOut(i: number): Stashed | undefined {
  const s = stash.list[i];
  if (!s) return;
  stash.list = stash.list.filter((_, k) => k !== i);
  save();
  return s;
}
