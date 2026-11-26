// What the Live scene draws: every thread's latest turn as a span of time, and from those how many
// threads worked at each moment. The real turns come from the threads the window already has, so
// only the latest turn of each thread is known; the simulation (`sim.svelte.ts`) makes the same
// kind of turns up, so the charts cannot tell the two apart.
import type { Thread } from "./bindings/Thread";
import { byProject, isScratch } from "./format";

export type TurnKind = "run" | "done" | "fail";

/** One thread's latest turn: sent work at `start`, came back (or stopped) at `work`, and if it then
 *  waits on you, `waiting` carries the amber tail from `work` up to now. */
export type Turn = {
  id: string;
  title: string;
  /** Project key, name and place, for the lane the thread sits in. */
  key: string;
  project: string;
  cwd: string;
  host: string | null;
  start: number;
  work: number;
  kind: TurnKind;
  waiting: boolean;
  /** Made up by the simulation: nothing to open. */
  fake?: boolean;
};

/** The real turns in the last `from..now` window, one per thread, scratch folders left out. */
export function turnsOf(threads: Thread[], now: number, from: number): Turn[] {
  const out: Turn[] = [];
  for (const p of byProject(threads, []).filter((p) => !isScratch(p.cwd))) {
    for (const t of p.threads) {
      const start = t.sent_at ?? t.created_at;
      const running = t.status === "running";
      const work = running ? now : Math.min(now, Math.max(start, t.returned_at ?? t.updated_at));
      const waiting = t.status === "waiting_on_you";
      if ((waiting ? now : work) < from) continue;
      out.push({
        id: t.id,
        title: t.title,
        key: p.key,
        project: p.name,
        cwd: p.cwd,
        host: p.host,
        start: Math.max(start, from),
        work: Math.max(work, from),
        kind: running ? "run" : t.status === "error" ? "fail" : "done",
        waiting: waiting && work < now,
      });
    }
  }
  return out;
}

/** The moment a turn stopped working: now while it runs. */
export const endOf = (t: Turn, now: number) => (t.kind === "run" ? now : t.work);

/** Which threads worked in each of `n` slices between `from` and `now`. */
export function concurrency(turns: Turn[], from: number, now: number, n: number): { at: number; titles: string[] }[] {
  const step = (now - from) / n;
  return Array.from({ length: n }, (_, i) => {
    const a = from + i * step;
    const b = a + step;
    // A thread counts in a slice it worked any part of, so a short burst still shows.
    return { at: a + step / 2, titles: turns.filter((t) => t.start < b && endOf(t, now) >= a).map((t) => t.title) };
  });
}

/** Turns grouped by project, the project that moved last first. */
export function lanesOf(turns: Turn[], now: number) {
  const by = new Map<string, { key: string; project: string; cwd: string; host: string | null; turns: Turn[]; at: number }>();
  for (const t of turns) {
    const g = by.get(t.key) ?? { key: t.key, project: t.project, cwd: t.cwd, host: t.host, turns: [], at: 0 };
    g.turns.push(t);
    g.at = Math.max(g.at, t.waiting ? now : endOf(t, now));
    by.set(t.key, g);
  }
  return [...by.values()].map((g) => ({ ...g, turns: g.turns.sort((a, b) => a.start - b.start) })).sort((a, b) => b.at - a.at);
}

/** Whole-hour ticks on an axis `from..now`: label and place in percent. */
export function hourTicks(from: number, now: number, every: number): { at: number; label: string }[] {
  const span = now - from;
  const out: { at: number; label: string }[] = [];
  const first = new Date(from);
  first.setMinutes(0, 0, 0);
  for (let d = first.getTime() + 3_600_000; d < now - span * 0.04; d += 3_600_000) {
    const h = new Date(d).getHours();
    if (h % every !== 0 || d <= from + span * 0.02) continue;
    out.push({ at: ((d - from) / span) * 100, label: String(h).padStart(2, "0") });
  }
  return out;
}

export const hhmm = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
