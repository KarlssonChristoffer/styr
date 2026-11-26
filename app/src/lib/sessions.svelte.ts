// Every agent session that is live right now, on every online machine: the Machines pages and the
// status bar's item (SessionRow.svelte on the Machines pages, StatusBar.svelte). Built from what the Workers view
// already asks each machine for (`/api/workers`, `?host=` for another one) and each machine's
// own load (`/api/host`), so nothing new is needed from the core.
//
// A session is live while its thread has an engine, or while a thread that is settled, archived
// or gone still has processes left running. The state is what the page shows by icon and colour.
import { quiet } from "./quiet";
import { api } from "./api";
import type { Thread } from "./bindings/Thread";
import type { ThreadWork } from "./bindings/ThreadWork";
import { hub } from "./state.svelte";
import { poll } from "./poll";

export type SessionState = "working" | "asks" | "idle" | "lingering" | "asleep";

export type Session = {
  id: string;
  thread: Thread | undefined;
  /** The machine's name, and null for this one (for calls with `?host=`). */
  machine: string;
  host: string | null;
  state: SessionState;
  /** When it got into this state, as far as the thread knows. */
  since: number;
  /** An idle engine stops at this time (`IDLE_TIMEOUT` in `engine/acp.rs`), by the thread's last activity. */
  stopsAt: number | null;
  mem: number;
  /** Share of the whole machine, 0 to 100. */
  cpu: number;
  processes: number;
  ports: number[];
};

export type Machine = {
  name: string;
  host: string | null;
  cpu: number;
  memUsed: number;
  memTotal: number;
  /** What the agents (every thread's engine and tools) take of it. */
  memAgents: number;
  online: boolean;
};

/** An engine with nothing to do stops after this long (`IDLE_TIMEOUT` in `engine/acp.rs`). */
export const IDLE_STOP_MS = 15 * 60 * 1000;
/** A thread-less engine younger than this is a warm one (`warm.rs` stops them after 3 minutes). */
const WARM_GRACE_MS = 5 * 60 * 1000;

const ORDER: Record<SessionState, number> = { lingering: 0, asks: 1, working: 2, idle: 3, asleep: 4 };

class Sessions {
  sessions = $state<Session[]>([]);
  machines = $state<Machine[]>([]);
  /** False until the first answer. */
  loaded = $state(false);
  failed = $state<string | null>(null);

  private timer: (() => void) | null = null;
  private asking: number[] = [];
  private busy = false;

  count = (state: SessionState) => this.sessions.filter((s) => s.state === state).length;
  get memory() {
    return this.sessions.reduce((n, s) => n + s.mem, 0);
  }

  /** Keeps asking every `ms` while something shows it; the quickest asker sets the pace. */
  watch(ms: number): () => void {
    this.asking.push(ms);
    void this.load();
    this.pace();
    return () => {
      this.asking.splice(this.asking.indexOf(ms), 1);
      this.pace();
    };
  }

  private pace() {
    if (this.timer) this.timer();
    this.timer = this.asking.length ? poll(() => void this.load(), Math.min(...this.asking)) : null;
  }

  async load() {
    if (this.busy) return;
    this.busy = true;
    try {
      const names: (string | null)[] = [null, ...hub.hosts.filter((h) => !h.local && h.state === "online").map((h) => h.name)];
      const answers = await Promise.all(
        names.map(async (host) => {
          const [work, snap] = await Promise.all([api.workers(host), quiet(api.host(host), "host", null)]);
          return { host, work, snap };
        }).map((p) => quiet(p, "machine answers", null)),
      );
      const sessions: Session[] = [];
      const machines: Machine[] = [];
      for (const a of answers) {
        if (!a) continue;
        const name = a.host ?? (hub.localHost || a.work.host);
        for (const w of a.work.threads) {
          const s = this.session(w, name, a.host);
          if (s) sessions.push(s);
        }
        // A thread waiting on you with no engine is asleep: it wakes with your answer (`sleep.rs`).
        for (const t of hub.threads) {
          if (t.status !== "waiting_on_you" || t.archived || t.settled_at != null) continue;
          if ((t.host ?? hub.localHost) !== name || sessions.some((x) => x.id === t.id)) continue;
          sessions.push({ id: t.id, thread: t, machine: name, host: a.host, state: "asleep", since: t.updated_at, stopsAt: null, mem: 0, cpu: 0, processes: 0, ports: [] });
        }
        const agents = a.work.threads.reduce((n, w) => n + w.mem, 0);
        machines.push({
          name,
          host: a.host,
          cpu: a.snap?.cpu_percent ?? 0,
          memUsed: a.snap?.mem_used ?? 0,
          memTotal: a.snap?.mem_total ?? 0,
          memAgents: agents,
          online: true,
        });
      }
      sessions.sort((a, b) => ORDER[a.state] - ORDER[b.state] || a.since - b.since);
      this.sessions = sessions;
      this.machines = machines;
      this.failed = null;
      this.loaded = true;
    } catch (e) {
      this.failed = e instanceof Error ? e.message : String(e);
    } finally {
      this.busy = false;
    }
  }

  private session(w: ThreadWork, machine: string, host: string | null): Session | null {
    const t = hub.threads.find((x) => x.id === w.thread_id);
    const putAway = !t || t.archived || t.settled_at != null;
    const turn = t?.status === "running" || t?.status === "waiting_on_you";
    const live = w.engine_running || w.processes.length > 0;
    // An engine with no thread row yet is a warm one (`warm.rs`): started while a first message is
    // typed, it waits for its thread and stops by itself after WARM_FOR. Not a leftover.
    if (!t && w.engine_running && !turn) {
      const born = w.processes.reduce((min, p) => Math.min(min, p.started_at), Infinity);
      if (!Number.isFinite(born) || Date.now() - born < WARM_GRACE_MS) return null;
    }
    let state: SessionState;
    if (t?.status === "waiting_on_you" && w.engine_running) state = "asks";
    else if (t?.status === "running" && w.engine_running) state = "working";
    else if (putAway && live && !turn) state = "lingering";
    else if (w.engine_running) state = "idle";
    // Processes a thread that is still in use left behind are the Workers page's business.
    else return null;
    const since = state === "lingering" ? (t?.settled_at ?? t?.updated_at ?? Date.now()) : (t?.updated_at ?? Date.now());
    return {
      id: w.thread_id,
      thread: t,
      machine,
      host,
      state,
      since,
      stopsAt: state === "idle" ? since + IDLE_STOP_MS : null,
      mem: w.mem,
      cpu: w.cpu_percent,
      processes: w.processes.length,
      ports: w.processes.flatMap((p) => p.ports),
    };
  }
}

export const sessions = new Sessions();

/** "now", "4m", "2h", "3d": the short time the page and the bar show. */
export function span(ms: number): string {
  const m = Math.floor(Math.max(0, ms) / 60_000);
  if (m < 1) return "<1m";
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  return h < 24 ? `${h}h` : `${Math.floor(h / 24)}d`;
}

/** What a row says in its time column, and the words for the hover. */
export function timeOf(s: Session, now: number): { text: string; says: string } {
  if (s.state === "idle" && s.stopsAt) {
    const left = s.stopsAt - now;
    return { text: `${span(left)} ↓`, says: `Idle. Its engine stops in about ${span(left)} and starts again with the next message` };
  }
  const t = span(now - s.since);
  const says = {
    working: `Working for ${t}`,
    asks: `Waiting on you for ${t}`,
    lingering: `Settled or archived ${t} ago and still running`,
    idle: "",
    asleep: `Asleep for ${t}. It waits on you and wakes with your answer`,
  }[s.state];
  return { text: t, says };
}
