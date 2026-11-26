// The rules tab's pure parts: what the search finds, the theme filter, the week's changes and the
// groups, from the rules `GET /api/project/rules` lists.
import type { StandingRule } from "./bindings/StandingRule";

export const THEMES: { id: string; label: string }[] = [
  { id: "merge", label: "Merge" },
  { id: "test", label: "Tests" },
  { id: "ui", label: "UI" },
  { id: "docs", label: "Docs" },
  { id: "code", label: "Code" },
  { id: "other", label: "Other" },
];

const WEEK = 7 * 24 * 3600;

/** The rules that match what is typed (every word, in the text or the file name) and the theme picked. */
export function pick(rules: StandingRule[], query: string, theme: string): StandingRule[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  return rules.filter((r) => {
    if (theme !== "all" && r.theme !== theme) return false;
    const hay = `${r.text} ${r.file} ${r.why} ${r.thread}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** Rules changed within the last week, newest first. `now` is in seconds. */
export function recent(rules: StandingRule[], now: number): StandingRule[] {
  return rules.filter((r) => r.since !== null && now - r.since < WEEK).sort((a, b) => (b.since ?? 0) - (a.since ?? 0));
}

/** The rules by theme, in the order of `THEMES`; a theme with none is left out. */
export function byTheme(rules: StandingRule[]): { id: string; label: string; rules: StandingRule[] }[] {
  return THEMES.map((t) => ({ ...t, rules: rules.filter((r) => (THEMES.some((x) => x.id === r.theme) ? r.theme : "other") === t.id) })).filter((g) => g.rules.length);
}

/** Where a rule comes from: the file, or for one said in a thread that thread's name. */
export function origin(r: StandingRule): string {
  return r.source === "hub" ? r.thread || "Said in chat" : r.file;
}
