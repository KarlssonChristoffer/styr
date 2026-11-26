// Hub's patch notes (notes.rs) as items for the Updates page. A bullet may end with
// `{show: place}`, the page to take you to, and `{test: step; step}`, how to try it by hand.
// Notes written before those tags were asked for have a "Try this" checklist instead; each of its
// items becomes a test of its own.

import type { SettingsSection, View } from "./state.svelte";

export type NoteKind = "new" | "fixed" | "try" | "other";
export type NoteItem = { text: string; kind: NoteKind; show: Place | null; test: string[] };
export type NoteSection = { title: string; kind: NoteKind; items: NoteItem[] };
export type Place = { view: View } | { settings: SettingsSection };

const VIEWS: View[] = ["home", "overview", "workers", "machines", "services", "routines", "recipes", "library", "plugins", "problems", "labs"];
const SECTIONS: SettingsSection[] = [
  "account", "appearance", "notifications", "dictation", "chat", "engines", "safety", "customs", "projects",
  "extensions", "apis", "vault", "machines", "access", "updates", "whatsnew", "privacy", "about",
];

/** A `{show: …}` place, or null when it names no page Hub has. */
export function place(name: string): Place | null {
  const n = name.trim().toLowerCase();
  if (n.startsWith("settings/")) {
    const s = n.slice(9) as SettingsSection;
    return SECTIONS.includes(s) ? { settings: s } : null;
  }
  return VIEWS.includes(n as View) ? { view: n as View } : null;
}

const TAG = /\{(show|test):\s*([^{}]*)\}/gi;

/** The text without its tags, for showing notes as plain Markdown. */
export function stripTags(text: string): string {
  return text.replace(TAG, "").replace(/[ \t]+$/gm, "");
}

function kindOf(title: string): NoteKind {
  const t = title.toLowerCase();
  if (t.startsWith("new")) return "new";
  if (t.includes("fix") || t.includes("chang")) return "fixed";
  if (t.startsWith("try")) return "try";
  return "other";
}

function item(raw: string, kind: NoteKind): NoteItem {
  let show: Place | null = null;
  let test: string[] = [];
  const text = raw
    .replace(TAG, (_, tag: string, value: string) => {
      if (tag.toLowerCase() === "show") show = place(value);
      else test = value.split(";").map((s) => s.trim()).filter(Boolean);
      return "";
    })
    .replace(/^\[[ xX]\]\s*/, "")
    .trim();
  // An old "Try this" line is itself the thing to test.
  if (kind === "try" && !test.length) test = [text];
  return { text, kind, show, test };
}

/** The notes as sections of items: `## Heading` starts a section, `- ` or `* ` an item, and an
 *  indented line carries on the item above it. */
export function parseNotes(text: string): NoteSection[] {
  const out: NoteSection[] = [];
  let lines: string[] = [];
  const flush = () => {
    const s = out[out.length - 1];
    if (s && lines.length) s.items.push(item(lines.join(" "), s.kind));
    lines = [];
  };
  for (const line of text.split(/\r?\n/)) {
    const head = /^#{1,4}\s+(.*)$/.exec(line);
    const bullet = /^\s{0,3}[-*]\s+(.*)$/.exec(line);
    if (head) {
      flush();
      out.push({ title: head[1].trim(), kind: kindOf(head[1]), items: [] });
    } else if (bullet) {
      flush();
      if (!out.length) out.push({ title: "", kind: "other", items: [] });
      lines = [bullet[1]];
    } else if (line.trim() && lines.length) {
      lines.push(line.trim());
    } else if (!line.trim()) {
      flush();
    }
  }
  flush();
  return out.filter((s) => s.items.length);
}

/** A short key for an item, to remember that you tried it. */
export function noteKey(version: string, text: string): string {
  let h = 0;
  for (let i = 0; i < text.length; i++) h = (Math.imul(h, 31) + text.charCodeAt(i)) | 0;
  return `${version.slice(0, 7)}:${(h >>> 0).toString(36)}`;
}
