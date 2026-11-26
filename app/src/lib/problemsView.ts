// The plain parts of the Problems view: its row types, tabs, sorting and small formatters.
import type { ComponentProps } from "svelte";
import type { MetricSummary } from "./bindings/MetricSummary";
import type { Problem } from "./bindings/Problem";
import type { ToolRate } from "./bindings/ToolRate";
import type { View } from "./bindings/View";
import { ago, clock } from "./format";
import type Icon from "./components/Icon.svelte";

export type Row = Problem & { host: string | null };
export type ToolRow = ToolRate & { host: string | null };
/** Problems with the same title, source and level in several places, as one row. */
export type Group = { key: string; rows: Row[]; first: Row; count: number; last: number; days: number[] };
export type Sort = { by: string; up: boolean };
export const DAY = 24 * 3600_000;
export const TABS: [View, string][] = [
  ["needs_you", "Needs you"],
  ["vakten", "Guard is on it"],
  ["open", "All open"],
  ["fixed", "Fixed this week"],
  ["back", "Came back"],
  ["quiet", "Quiet sources"],
  ["slower", "Slower"],
  ["archived", "Archived"],
];
export function inView(p: Row, v: View): boolean {
  const open = p.status === "open" && p.merged_into == null;
  switch (v) {
    case "needs_you": return p.needs_you;
    case "vakten": return open && !!p.thread && !p.needs_you;
    case "open": return open;
    case "fixed": return p.status === "resolved" && (p.resolved_at ?? 0) >= Date.now() - 7 * DAY;
    case "back": return open && p.regressed;
    case "quiet": return open && p.source === "monitor";
    case "slower": return open && p.source === "metrics";
    case "archived": return p.status === "archived" && p.merged_into == null;
    case "resolved": return p.status === "resolved";
    case "all": return p.merged_into == null;
  }
}
export const pct = (failed: number, calls: number) => (!calls ? "–" : !failed ? "0%" : `${((failed / calls) * 100).toFixed(failed / calls < 0.1 ? 1 : 0)}%`);
export const toolPeak = (t: ToolRate) => Math.max(1, ...t.days);
export const CAUSES: Record<string, string> = { agent: "agent misuse", environment: "machine setup", hub: "Hub bug", external: "outside service", noise: "noise" };
export const PRIO_RANK = (p: Row) => (p.prio ? Number(p.prio[1]) : 9);
export const sum = (d: number[]) => d.reduce((a, b) => a + b, 0);
export function sortBy(by: string, s: Sort): Sort {
  // Text sorts A to Z first, numbers biggest first.
  return s.by === by ? { by, up: !s.up } : { by, up: by === "problem" || by === "tool" };
}
export const RING_TIP: Record<string, string> = {
  "idle unread": "Fixed",
  idle: "Left as it is",
  waiting_on_you: "Waits for you",
  error: "Went wrong again, or urgent with nobody on it",
  running: "Being fixed",
};
/** Each source's grey glyph. */
export const SOURCE_ICON: Record<string, ComponentProps<typeof Icon>["name"]> = {
  hubd: "hub", app: "monitor", thread: "threads", tool: "terminal", monitor: "clock", metrics: "gauge", sentry: "puzzle", otlp: "puzzle", code_analysis: "terminal",
};
export const when = (ms: number) => (Date.now() - ms < 20 * 3600_000 ? clock(ms) : ago(ms));
export const peak = (p: Problem) => Math.max(1, ...p.days);
export const fmt = (m: MetricSummary, v: number) => (m.unit === "MB" ? `${v.toFixed(0)} MB` : v >= 1000 ? `${(v / 1000).toFixed(1)} s` : `${v.toFixed(0)} ms`);
export const SIGN_IN = /authentication required|not logged in|please log ?in|run \/login|invalid api key|token has expired|sign in again/i;
