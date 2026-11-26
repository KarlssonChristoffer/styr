// How a line of Customs' log reads (customs_log.rs), shared by Settings > Customs and the
// Overview card.
import type { CustomsLogEntry } from "./bindings/CustomsLogEntry";

const WHAT: Record<string, string> = {
  declared: "Declared",
  found: "Found after a turn",
  asked: "Asked for through the registry",
  cleared: "Cleared",
  held: "Held",
  refused: "Refused",
  reviewed: "Reviewed",
  advisory: "New advisory",
  added: "Added",
  removed: "Removed",
  imported: "Read a project",
  daily_check: "Daily check",
  monthly: "Monthly update",
};
const BY: Record<string, string> = { you: "by you", review: "by review", agent: "by an agent", check: "by the daily check", registry: "" };

/** "Cleared by review", "Held for you", "Declared by an agent". */
export function headline(e: CustomsLogEntry): string {
  if (e.what === "held") return e.by === "you" ? "Held by you" : "Held for you";
  if (["daily_check", "monthly", "imported", "advisory", "asked", "found"].includes(e.what)) return WHAT[e.what];
  const by = BY[e.by] ?? "";
  return `${WHAT[e.what] ?? e.what}${by ? ` ${by}` : ""}`;
}

/** The state ring a line wears: colour only for what was decided. */
export function ring(e: CustomsLogEntry): "ok" | "warn" | "bad" | "open" | "round" {
  if (e.what === "cleared") return "ok";
  if (e.what === "held" || e.what === "advisory") return "warn";
  if (e.what === "refused") return "bad";
  if (["daily_check", "monthly", "imported"].includes(e.what)) return "round";
  return "open";
}

/** Something that came through Customs: asked for, declared or brought in. */
export const cameThrough = (e: CustomsLogEntry) => ["declared", "found", "asked"].includes(e.what);

const DAY = 86_400_000;
const dayStart = (ms: number) => {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
};

/** What came through on each of the last `days` days, oldest first, and whether any of a day's was held. */
export function perDay(entries: CustomsLogEntry[], days = 7, now = Date.now()): { n: number; held: boolean }[] {
  const today = dayStart(now);
  const out = Array.from({ length: days }, () => ({ n: 0, held: false }));
  for (const e of entries) {
    const i = days - 1 - Math.round((today - dayStart(e.ts)) / DAY);
    if (i < 0 || i >= days) continue;
    if (cameThrough(e)) out[i].n++;
    if (e.what === "held") out[i].held = true;
  }
  return out;
}

/** "Today", "Yesterday" or the date, for grouping the log by day. */
export function dayLabel(ms: number, now = Date.now()): string {
  const days = Math.round((dayStart(now) - dayStart(ms)) / DAY);
  if (days === 0) return "Today";
  if (days === 1) return "Yesterday";
  return new Date(ms).toLocaleDateString([], { weekday: days < 7 ? "long" : undefined, day: "numeric", month: "long" });
}

export const isToday = (ms: number, now = Date.now()) => dayStart(ms) === dayStart(now);

/** How long a review took, in words. */
export const took = (ms: number) => (ms < 1000 ? "under a second" : ms < 60_000 ? `${Math.round(ms / 1000)} s` : `${Math.round(ms / 60_000)} min`);

/** The time of day, 24-hour like the thread list: "22:08". */
export const hm = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
