// Yes/no questions about one thread (asks you, unread, snoozed) and the wake label; pure, used by lists and the rail.
import { isChatThread } from "./format";
import type { Thread } from "./bindings/Thread";

/** The last reply asks you something or got stuck (`reading.rs` in the core). `Thread.display` is the core's
 *  word for it; a thread without one (an older hubd, a stand-in) is read from `status` and `reading` (compat-until: 2026-12-07). */
export function asksYou(t: Thread): boolean {
  if (t.display) return t.display === "asks";
  return t.status === "idle" && (t.reading === "asks" || t.reading === "stuck");
}

/** The thread waits on you: a permission question, a failure, or a reply that asks or is stuck. */
export function needsYou(t: Thread): boolean {
  if (t.display) return t.display === "waiting" || t.display === "error" || t.display === "asks";
  return t.status === "waiting_on_you" || t.status === "error" || asksYou(t);
}

/** What a project's threads say about it: how many run, how many wait on you (a permission question or a
 *  reply that asks, unless settled) and how many failed. One answer for the project page, Home and the Overview. */
export function projectCounts(threads: Thread[]): { running: number; asks: number; failed: number } {
  const n = (f: (t: Thread) => boolean) => threads.filter(f).length;
  return {
    running: n((t) => t.status === "running"),
    asks: n((t) => t.status === "waiting_on_you" || (asksYou(t) && !t.settled_at)),
    failed: n((t) => t.status === "error"),
  };
}

/** A thread came back since you last opened it. */
export function unread(t: Thread): boolean {
  return t.status !== "running" && t.returned_at != null && (t.seen_at == null || t.seen_at < t.returned_at);
}

/** What a chat shows: a chat has no "needs you" (an answer is not a request), only a dot for an
 *  answer you have not read (opening the chat reads it), one for a turn still writing, and one for a turn that failed. */
export function chatMark(t: Thread): "failed" | "writing" | "new" | null {
  if (t.status === "error") return "failed";
  if (t.status === "running") return "writing";
  // A question in the last answer ("asks") stays true until the next turn, so it is not a reason to
  // keep a dot: opening the chat is what reads it. Only a live permission question (waiting) holds one.
  if (unread(t) || t.status === "waiting_on_you") return "new";
  return null;
}

/** Snoozed and not woken yet. */
export function isSnoozed(t: Thread, now: number): boolean {
  return t.snoozed_until != null && t.snoozed_until > now;
}

/** "14:30", "tomorrow 09:00" or "Mon 09:00": when a snooze ends. */
export function wakeLabel(at: number, now = Date.now()): string {
  const d = new Date(at);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(d) - day(new Date(now))) / 86_400_000);
  if (days <= 0) return time;
  if (days === 1) return `tomorrow ${time}`;
  if (days < 7) return `${d.toLocaleDateString([], { weekday: "short" })} ${time}`;
  return `${d.toLocaleDateString([], { day: "numeric", month: "short" })} ${time}`;
}

const FOLDERS_KEY = "hub.folders";

export function recentFolders(): string[] {
  try {
    // A chat's folder is not a place to start work in, even if an older version remembered one.
    return (JSON.parse(localStorage.getItem(FOLDERS_KEY) ?? "[]") as string[]).filter((f) => !isChatThread({ cwd: f }));
  } catch {
    return [];
  }
}

export function rememberFolder(cwd: string) {
  if (isChatThread({ cwd })) return;
  try {
    const list = [cwd, ...recentFolders().filter((f) => f !== cwd)].slice(0, 12);
    localStorage.setItem(FOLDERS_KEY, JSON.stringify(list));
  } catch {
    // Storage can be unavailable; the folder list is only a convenience.
  }
}
