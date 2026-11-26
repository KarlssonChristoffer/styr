// Small words the transcript's rows share: counts, times and "what the user was looking for".
import { clip, clock } from "./format";

export const plural = (n: number, one: string, many = one + "s") => `${n} ${n === 1 ? one : many}`;

export const short = (text: string) => clip(text, 48) || "your message";

/** "at 14:50", or with the day when it is not today. */
export const when = (at?: number) => {
  if (!at) return "later";
  const d = new Date(at);
  const today = d.toDateString() === new Date().toDateString();
  return today ? `at ${clock(at)}` : `${d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })} at ${clock(at)}`;
};

/** When you last looked, as on the "New" break: "3 min ago" (not format.ts `ago`). */
export const ago = (at: number) => {
  const m = Math.round((Date.now() - at) / 60_000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  return h < 36 ? `${h} h ago` : `${Math.round(h / 24)} days ago`;
};

/** How long a thread sat before a message: "2 h later", "3 days later". */
export const later = (ms: number) => {
  const h = Math.round(ms / 3_600_000);
  return h < 36 ? `${h} h later` : `${Math.round(h / 24)} days later`;
};

/** Whether `s` holds what Ctrl F looks for (`needle`, lower case; empty looks for nothing). */
export const hits = (needle: string, s?: string | null) => !!needle && !!s && s.toLowerCase().includes(needle);
