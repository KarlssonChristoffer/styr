// Dev servers a thread runs, shown in the right panel's browser (`BrowserFrame.svelte`) rather
// than in Chrome. Only this machine's own servers open there: the desktop app may only frame
// localhost, and Hub's own address never, since a page there would be Hub.
import { BASE } from "./api";
import { openUrl } from "./native";
import { hub } from "./state.svelte";

const LOCAL = new Set(["localhost", "127.0.0.1", "0.0.0.0"]);

/** What was typed, as a URL: "5173", ":5173" and "localhost:5173/x" are all localhost. */
export function previewUrl(typed: string): string | null {
  let t = typed.trim();
  if (!t) return null;
  if (/^:?\d{2,5}(\/.*)?$/.test(t)) t = `localhost${t.startsWith(":") ? "" : ":"}${t}`;
  if (!/^https?:\/\//i.test(t)) t = `http://${t}`;
  try {
    const u = new URL(t);
    if (u.hostname === "0.0.0.0") u.hostname = "localhost";
    return u.href;
  } catch {
    return null;
  }
}

/** Whether `url` may open in the panel: a server on this machine that is not Hub itself. */
export function previewable(url: string): boolean {
  try {
    const u = new URL(url);
    if (!/^https?:$/.test(u.protocol) || !LOCAL.has(u.hostname)) return false;
    const own = [new URL(BASE), new URL(location.href)];
    return !own.some((o) => LOCAL.has(o.hostname) && (o.port || "80") === (u.port || (u.protocol === "https:" ? "443" : "80")));
  } catch {
    return false;
  }
}

/** Opens `url` in the side panel beside thread `thread`, or in the browser when it cannot be. */
export function preview(url: string, thread = hub.current?.id) {
  const href = previewUrl(url);
  // A thread on another machine runs its servers there, out of this window's reach.
  const here = !hub.threads.find((t) => t.id === thread)?.host;
  if (href && thread && here && previewable(href)) hub.viewing = { thread, kind: "browser", id: href };
  else openUrl(href ?? url).catch((e) => hub.fail(e));
}
