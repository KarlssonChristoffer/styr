// Simulating: made-up threads, asks and limits, so the live parts of Home (the chart, the lanes,
// the limit bars) show what they look like when things happen. It lives only in this window's
// memory: it is off when Hub starts, writes nothing to the core and touches no real thread, pull
// request or setting. The clock runs ahead (a tick is a minute and a half) so a few seconds show
// threads starting, asking you, failing and finishing.
import { endOf, type Turn } from "./workmap";
import { poll } from "./poll";

export type SimLimit = { label: string; percent: number; resets: string | null };

export const sim = $state({
  on: false,
  /** The simulated clock: starts at the real time and runs ahead. */
  now: Date.now(),
  turns: [] as Turn[],
  limits: [] as SimLimit[],
});

const PROJECTS = [
  { project: "Hub (dev)", cwd: "D:\\Dev\\hub", titles: ["Make Home its own page", "Fix PC and MacBook sessions hanging", "Brand colours in the model picker", "Merge queue conflicts", "Dialog alerts blocked by app permissions"] },
  { project: "Atlas", cwd: "D:\\Dev\\atlas", titles: ["Refresh stale cache", "Phone server for more than one person", "Index the new docs", "Retry the failed sync"] },
  { project: "pr-336", cwd: "D:\\Dev\\pr-336", titles: ["Resolve the conflict in #336", "Rebase on main", "Check the failing test"] },
  { project: "Spelet", cwd: "D:\\Dev\\spelet", titles: ["Hej", "Test #2", "Level loader"] },
  { project: "Knegarn", cwd: "D:\\Dev\\knegarn", titles: ["Plugin handoff", "Contract linter", "Doctor warnings"] },
];

const TICK_MS = 1500;
const STEP = 90_000;
const HOUR = 3_600_000;
let stop: (() => void) | null = null;
let seed = 7;
const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
let made = 0;
/** How long a running turn works and how long a waiting one waits before you answer, by turn id. */
const plan = new Map<string, { dur: number; patience: number }>();

function make(p: (typeof PROJECTS)[number], start: number): Turn {
  const id = `sim-${made++}`;
  plan.set(id, { dur: (4 + rnd() * 22) * 60_000, patience: (5 + rnd() * 15) * 60_000 });
  return { id, title: p.titles[Math.floor(rnd() * p.titles.length)], key: `sim:${p.cwd}`, project: p.project, cwd: p.cwd, host: null, start, work: start, kind: "run", waiting: false, fake: true };
}

function seedTurns(now: number): Turn[] {
  const out: Turn[] = [];
  for (const p of PROJECTS) {
    for (let i = 0; i < 1 + Math.floor(rnd() * 3); i++) {
      const start = now - (0.4 + rnd() * 5.2) * HOUR;
      const t = make(p, start);
      const dur = plan.get(t.id)!.dur;
      out.push({ ...t, kind: "done", work: Math.min(now - 60_000, start + dur) });
    }
  }
  for (const [i, p] of [PROJECTS[0], PROJECTS[0], PROJECTS[1]].entries()) out.push(make(p, now - (6 + i * 9) * 60_000));
  return out;
}

function tick() {
  const now = sim.now + STEP;
  sim.now = now;
  let turns = sim.turns.map((t): Turn => {
    const p = plan.get(t.id);
    if (!p) return t;
    if (t.kind === "run" && now - t.start > p.dur) {
      const r = rnd();
      return { ...t, kind: r < 0.14 ? "fail" : "done", work: now, waiting: r >= 0.14 && r < 0.45 };
    }
    if (t.waiting && now - t.work > p.patience) return { ...t, waiting: false };
    return t;
  });
  if (turns.filter((t) => t.kind === "run").length < 6 && rnd() < 0.34) turns.push(make(PROJECTS[Math.floor(rnd() * PROJECTS.length)], now));
  turns = turns.filter((t) => endOf(t, now) > now - 6 * HOUR || t.waiting);
  sim.turns = turns;
  sim.limits = sim.limits.map((l, i) => {
    const up = [0.12, 0.45, 0.08][i] ?? 0.1;
    const percent = l.percent + up + rnd() * up;
    return { ...l, percent: percent >= 100 ? 3 : Math.round(percent * 10) / 10 };
  });
}

export function startSim() {
  if (sim.on) return;
  seed = 7;
  made = 0;
  plan.clear();
  sim.now = Date.now();
  sim.turns = seedTurns(sim.now);
  sim.limits = [
    { label: "Claude weekly", percent: 42, resets: "Thursday" },
    { label: "Claude 5-hour", percent: 38, resets: "in 2 h 10 min" },
    { label: "Codex", percent: 12, resets: null },
  ];
  sim.on = true;
  stop = poll(tick, TICK_MS);
}

export function stopSim() {
  stop?.();
  stop = null;
  sim.on = false;
  sim.turns = [];
  sim.limits = [];
}

export const toggleSim = () => (sim.on ? stopSim() : startSim());
