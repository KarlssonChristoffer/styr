// How a machine is doing, in the state colours: grey when fine, amber when something wants a look,
// red when it is about to go wrong or already has. The Machines page and each machine's page use
// these, so a ring, a number and a card's one-line verdict always agree.
import type { HostInfo } from "./bindings/HostInfo";
import type { HostSnapshot } from "./bindings/HostSnapshot";
import type { MachineAbout } from "./bindings/MachineAbout";
import type { MachineReady } from "./bindings/MachineReady";

/** ok is grey (or green where it says ready/working), warn amber, bad red. */
export type Level = "ok" | "warn" | "bad";

const GB = 2 ** 30;

export function memLevel(used: number, total: number): Level {
  const pct = (used / Math.max(total, 1)) * 100;
  return pct >= 92 ? "bad" : pct >= 80 ? "warn" : "ok";
}

export function diskLevel(free: number, total: number): Level {
  const pct = (free / Math.max(total, 1)) * 100;
  return pct < 5 || free < 5 * GB ? "bad" : pct < 15 ? "warn" : "ok";
}

/** A chip's temperature. What is hot depends on the machine: a Raspberry Pi starts to slow down at
 *  80 °C, while a Mac's chip sits in the 70s and 80s under a short load and the case is only warm,
 *  and an older hubd reports its hottest core, which can read 95 °C then (compat-until: 2026-12-03). */
export function tempLevel(c: number | null | undefined, about?: { os?: string; kind?: string | null } | null): Level {
  if (c == null) return "ok";
  const [warn, bad] = about?.kind === "board" ? [75, 85] : /mac/i.test(about?.os ?? "") ? [90, 100] : [85, 95];
  return c >= bad ? "bad" : c >= warn ? "warn" : "ok";
}

/** CPU is read every few seconds and spikes, so it only ever goes amber. */
export function cpuLevel(pct: number): Level {
  return pct >= 90 ? "warn" : "ok";
}

const RANK: Record<Level, number> = { ok: 0, warn: 1, bad: 2 };
export const worst = (...l: Level[]): Level => l.reduce((a, b) => (RANK[b] > RANK[a] ? b : a), "ok");

/** One thing worth saying about a machine. `good` marks green news (ready, working). */
export type Signal = { level: Level; text: string; good?: boolean };

/** Everything a machine's card can say, worst first. */
export function signals(o: {
  h: HostInfo | undefined;
  online: boolean;
  snap: HostSnapshot | null;
  about: MachineAbout | null;
  ready: MachineReady | null;
  behind: boolean;
  asks: number;
  working: number;
}): Signal[] {
  const out: Signal[] = [];
  if (!o.online) {
    if (o.h?.state === "connecting") return [{ level: "warn", text: "Connecting…" }];
    return [{ level: "bad", text: o.h?.error ?? "Offline" }];
  }
  for (const c of o.ready?.checks ?? []) {
    if (c.state === "ok") continue;
    out.push({ level: c.state === "missing" ? "bad" : "warn", text: `${c.label}: ${c.detail}` });
  }
  if (o.snap) {
    const m = memLevel(o.snap.mem_used, o.snap.mem_total);
    if (m !== "ok") out.push({ level: m, text: m === "bad" ? "Memory almost full" : "Memory running high" });
    if (cpuLevel(o.snap.cpu_percent) !== "ok") out.push({ level: "warn", text: "CPU busy" });
  }
  const d = o.about?.disk;
  if (d) {
    const l = diskLevel(d.free, d.total);
    if (l !== "ok") out.push({ level: l, text: l === "bad" ? "Disk almost full" : "Disk getting full" });
  }
  const t = tempLevel(o.about?.temp_c, o.about);
  if (t !== "ok") out.push({ level: t, text: t === "bad" ? "Running very hot" : "Running hot" });
  if (o.asks) out.push({ level: "warn", text: `${o.asks} ${o.asks === 1 ? "thread needs" : "threads need"} you` });
  if (o.behind) out.push({ level: "warn", text: "hubd update ready" });
  out.sort((a, b) => RANK[b.level] - RANK[a.level]);
  if (!out.length) {
    if (o.working) out.push({ level: "ok", good: true, text: `${o.working} ${o.working === 1 ? "thread" : "threads"} working` });
    else out.push({ level: "ok", good: true, text: o.ready ? "Ready" : "Online" });
  }
  return out;
}

/** The card's one line: the worst signal, and how many more there are. */
export function verdict(all: Signal[]): Signal {
  const [first, ...rest] = all;
  const more = rest.filter((s) => s.level !== "ok").length;
  return more ? { ...first, text: `${first.text} · ${more} more` } : first;
}
