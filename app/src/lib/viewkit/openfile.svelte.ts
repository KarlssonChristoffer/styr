// Where a file or page card opens: unfolded in the chat, in the right panel, or in the modal. A
// click follows `prefs.fileOpen`; the card's own buttons pick another way for that once. Inline
// needs a card to unfold, so a caller without one (a chip in the text, a list) passes `card: false`
// and gets the panel instead.
import { hub } from "../state.svelte";
import { prefs } from "../prefs.svelte";
import { openBig, openBigFile } from "./bigview.svelte";

export type FileOpen = "inline" | "panel" | "modal";

/** The cards that are unfolded, by `unfoldKey`. */
export const unfolded = $state<Record<string, boolean>>({});
export const unfoldKey = (thread: string, ref: string) => `${thread}\n${ref}`;

/** The cards that are grown in place (out of the chat column, big and zoomable), by `unfoldKey`. */
export const grown = $state<Record<string, boolean>>({});
export const toggleGrown = (thread: string, ref: string) => (grown[unfoldKey(thread, ref)] = !grown[unfoldKey(thread, ref)]);

type Opts = { how?: FileOpen; card?: boolean };
const way = (o: Opts): FileOpen => {
  const how = o.how ?? prefs.fileOpen ?? "inline";
  return how === "inline" && o.card === false ? "panel" : how;
};

export function showFile(thread: string, path: string, text: string | null = null, o: Opts = {}) {
  const how = way(o);
  if (how === "inline") unfolded[unfoldKey(thread, path)] = !unfolded[unfoldKey(thread, path)];
  else if (how === "modal") openBigFile(thread, path, text);
  else hub.viewing = { thread, kind: "file", id: path };
}

export function showPage(thread: string, id: string, o: Opts = {}) {
  const how = way(o);
  if (how === "inline") unfolded[unfoldKey(thread, id)] = !unfolded[unfoldKey(thread, id)];
  else if (how === "modal") openBig(thread, id);
  else hub.viewing = { thread, kind: "page", id };
}
