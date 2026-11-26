// A "To test" line of docs/FOCUS.md as a script: "Title (aside): what to do. Look for: what to see.
// Prompt: text to give an agent. Go: Settings > Updates, Machines." Everything after the title is
// optional, so lines other threads add in the old one-sentence shape still show.
import type { GoTarget } from "./state.svelte";

export type Script = { title: string; todo: string; look: string; prompt: string; go: { label: string; to: GoTarget }[] };

/** The places a Go line can name, by the words used for them. */
const PLACES: Record<string, GoTarget> = {
  home: { view: "home" },
  machines: { view: "machines" },
  problems: { view: "problems" },
  overview: { view: "overview" },
  plugins: { view: "plugins" },
  library: { view: "library" },
  labs: { view: "labs" },
  workers: { view: "workers" },
};
const SECTIONS: Record<string, GoTarget> = Object.fromEntries(
  ["account", "spend", "appearance", "notifications", "dictation", "chat", "engines", "handoffs", "personality", "safety", "customs", "projects", "extensions", "apis", "vault", "machines", "access", "updates", "about"].map((k) => [k, { settings: k } as GoTarget]),
);
SECTIONS["key vault"] = { settings: "vault" };
SECTIONS["api connections"] = { settings: "apis" };

function place(word: string): { label: string; to: GoTarget } | null {
  const w = word.trim();
  const m = w.match(/^settings\s*>\s*(.+)$/i);
  const to = m ? SECTIONS[m[1].trim().toLowerCase()] : PLACES[w.toLowerCase()];
  return to ? { label: w, to } : null;
}

export function parseScript(text: string): Script {
  const marks = [/\bLook for:/i, /\bPrompt:/i, /\bGo:/i];
  const at = marks.map((m) => text.search(m));
  const part = (i: number) => {
    if (at[i] < 0) return "";
    const next = at.filter((x) => x > at[i]).sort((a, b) => a - b)[0] ?? text.length;
    return text.slice(at[i], next).replace(/^[A-Za-z ]+:\s*/, "").trim();
  };
  const firstMark = at.filter((x) => x >= 0).sort((a, b) => a - b)[0] ?? text.length;
  const i = text.indexOf(": ");
  const head = i > 0 && i < 120 && i < firstMark ? text.slice(0, i) : "";
  const todo = text.slice(head ? i + 2 : 0, firstMark).trim();
  const prompt = part(1).replace(/^["“]|["”]\.?$/g, "").trim();
  const go = part(2).replace(/\.$/, "").split(/,\s*/).map(place).filter((x): x is NonNullable<typeof x> => !!x);
  return { title: head.replace(/\s*\([^()]*\)\s*$/, "") || todo.slice(0, 60), todo, look: part(0), prompt, go };
}
