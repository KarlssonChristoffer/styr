// Small helpers shared by the Plugins page: how a repository's kind, stars and Customs' word read.
import type { PluginRepo } from "./bindings/PluginRepo";

export const KINDS: [string, string][] = [
  ["all", "All"],
  ["mcp", "MCP servers"],
  ["skill", "Skills"],
  ["tool", "Tools"],
  ["worker", "Workers"],
];

/** What each kind is, in one line, shown under the Show switch. */
export const KIND_SAYS: Record<string, string> = {
  all: "Everything agents can use. Hub guesses each repo's kind from its topics and description.",
  mcp: "Programs that give the agent new tools to call, like driving a browser or reading a PDF.",
  skill: "Written know-how the agent reads before a task, like design rules. Nothing runs.",
  tool: "Command-line programs the agent runs itself, like packing a repo into one file.",
  worker: "Programs that do whole jobs by themselves with several agents, like Knegarn. Each run shows as a thread.",
};

/** Quick searches offered under the search box. */
export const QUICK = ["reverse engineering", "browser", "pdf", "design", "database", "code review"];

export const kindLabel = (kind: string) => (kind === "mcp" ? "MCP" : kind === "skill" ? "Skill" : kind === "worker" ? "Worker" : "Tool");

/** 9800 → "9.8k", 31000 → "31k". */
export function count(n: number): string {
  if (n < 1000) return String(n);
  if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, "")}k`;
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
}

/** "+4.1k" this week, with "~" when it was projected from the newest stars. */
export const weekLabel = (r: PluginRepo) => (r.week == null ? null : `${r.week_estimated ? "~" : ""}+${count(r.week)}`);

/** What Customs says, as the ring class and words the page shows. */
export function customs(r: PluginRepo): { tone: "ok" | "held" | "no" | "none"; text: string } {
  if (r.customs === "cleared") return { tone: "ok", text: "Cleared" };
  if (r.customs === "held") return { tone: "held", text: r.customs_reason ? `Held: ${r.customs_reason}` : "Held" };
  if (r.customs === "refused") return { tone: "no", text: "Refused" };
  return { tone: "none", text: "Not checked" };
}

/** How long a star line spans, in words: "14 days", "40 hours". */
export function spanLabel(ms: number): string {
  const h = Math.round(ms / 3_600_000);
  if (h < 48) return `${Math.max(1, h)} ${h === 1 ? "hour" : "hours"}`;
  return `${Math.round(h / 24)} days`;
}

/** An SVG polyline's points for `values` in a `w` × `h` box, lowest at the bottom. */
export function line(values: number[], w: number, h: number, pad = 1.5): string {
  if (values.length < 2) return "";
  const min = Math.min(...values);
  const span = Math.max(1, Math.max(...values) - min);
  return values
    .map((v, i) => `${((i / (values.length - 1)) * w).toFixed(1)},${(h - pad - ((v - min) / span) * (h - pad * 2)).toFixed(1)}`)
    .join(" ");
}

/** Whole days since a moment, worded the way the page uses it. */
export function daysAgo(ms: number, now = Date.now()): string {
  const d = Math.floor((now - ms) / 86_400_000);
  if (d <= 0) return "today";
  if (d === 1) return "1 day";
  if (d < 60) return `${d} days`;
  if (d < 730) return `${Math.floor(d / 30)} months`;
  return `${Math.floor(d / 365)} years`;
}
