// How full each machine is, now and over the last hours: the Machines page and a machine's own
// page draw their waves from this (Wave.svelte). It asks each online machine for its load
// (`/api/host`, `?host=` for another one) while something shows it and keeps what it heard in
// buckets: 1 h in 30 s, 6 h in 3 min, 24 h in 12 min. It lives as long as the window, so a graph
// fills from the right while the window is open; the core keeping history is a later step.
import { api } from "./api";
import type { HostSnapshot } from "./bindings/HostSnapshot";
import type { MachineAbout } from "./bindings/MachineAbout";
import { recall, remember } from "./lastSeen";
import { hub } from "./state.svelte";
import { poll } from "./poll";

export type Range = "1h" | "6h" | "24h";
export const RANGES: Record<Range, { span: number; label: string }> = {
  "1h": { span: 3_600_000, label: "1 h" },
  "6h": { span: 6 * 3_600_000, label: "6 h" },
  "24h": { span: 24 * 3_600_000, label: "24 h" },
};
const N = 120;
type Slot = { i: number; mem: number; cpu: number; n: number };

/** One machine's readings, as averages per bucket for each range. */
class History {
  private rings: Record<Range, Slot[]> = { "1h": [], "6h": [], "24h": [] };

  add(t: number, mem: number, cpu: number) {
    for (const r of Object.keys(RANGES) as Range[]) {
      const i = Math.floor(t / (RANGES[r].span / N));
      const ring = this.rings[r];
      const s = ring[i % N];
      if (s && s.i === i) {
        s.mem = (s.mem * s.n + mem) / (s.n + 1);
        s.cpu = (s.cpu * s.n + cpu) / (s.n + 1);
        s.n++;
      } else ring[i % N] = { i, mem, cpu, n: 1 };
    }
  }

  /** The last `N` buckets of a range, oldest first; null where nothing was heard. */
  series(range: Range, key: "mem" | "cpu", now: number): (number | null)[] {
    const last = Math.floor(now / (RANGES[range].span / N));
    return Array.from({ length: N }, (_, k) => {
      const i = last - (N - 1) + k;
      const s = this.rings[range][i % N];
      return s && s.i === i ? s[key] : null;
    });
  }
}

class MachineStats {
  /** Each machine's latest load, by its name. */
  snaps = $state<Record<string, HostSnapshot | null>>(recall("machine.snaps") ?? {});
  abouts = $state<Record<string, MachineAbout | null>>(recall("machine.abouts") ?? {});
  /** Bumped by every reading, so a graph that reads a series redraws. */
  private tick = $state(0);
  private history = new Map<string, History>();
  private busy = new Set<string>();
  private aboutAt = new Map<string, number>();

  /** Asks every `ms` (just `only` when given) until the returned function is called. */
  watch(ms: number, only?: () => string | null): () => void {
    const run = () => void this.pull(only?.() ?? null);
    run();
    const timer = poll(run, ms);
    return () => timer();
  }

  /** The machines to ask: those online, or just this computer before the list has come. */
  private targets(only: string | null): { name: string; at: string | null }[] {
    const hosts = hub.hosts.length
      ? hub.hosts.filter((h) => h.local || h.state === "online").map((h) => ({ name: h.name, at: h.local ? null : h.name }))
      : [{ name: hub.host?.hostname ?? "This computer", at: null }];
    return only ? hosts.filter((h) => h.name === only) : hosts;
  }

  async pull(only: string | null = null) {
    await Promise.all(this.targets(only).map((t) => this.sample(t.name, t.at)));
  }

  private async sample(name: string, at: string | null) {
    if (this.busy.has(name)) return;
    this.busy.add(name);
    try {
      const s = await api.host(at);
      this.snaps[name] = s;
      remember("machine.snaps", this.snaps);
      let h = this.history.get(name);
      if (!h) this.history.set(name, (h = new History()));
      h.add(Date.now(), s.mem_used / Math.max(s.mem_total, 1), Math.min(1, s.cpu_percent / 100));
      this.tick++;
      // What the machine is (cores, disk, temperature) changes slowly.
      if (Date.now() - (this.aboutAt.get(name) ?? 0) > 60_000) {
        this.aboutAt.set(name, Date.now());
        this.abouts[name] = await api.hostAbout(at).catch(() => this.abouts[name] ?? null);
        remember("machine.abouts", this.abouts);
      }
    } catch {
      // Offline or too old: the card says so from the host list.
    } finally {
      this.busy.delete(name);
    }
  }

  /** One machine's memory or CPU as shares of its capacity. */
  series(name: string, range: Range, key: "mem" | "cpu"): (number | null)[] {
    void this.tick;
    return this.history.get(name)?.series(range, key, Date.now()) ?? Array(N).fill(null);
  }

  /**
   * All machines together: memory in use of all memory, or cores busy of all cores. Each point
   * counts the machines that were heard at that time.
   */
  network(names: string[], range: Range, key: "mem" | "cpu"): (number | null)[] {
    void this.tick;
    const per = names.map((n) => ({
      s: this.series(n, range, key),
      w: key === "mem" ? (this.snaps[n]?.mem_total ?? 0) : (this.abouts[n]?.cores ?? 1),
    }));
    return Array.from({ length: N }, (_, k) => {
      let sum = 0;
      let weight = 0;
      for (const p of per) {
        const v = p.s[k];
        if (v == null) continue;
        sum += v * p.w;
        weight += p.w;
      }
      return weight ? sum / weight : null;
    });
  }
}

export const machineStats = new MachineStats();
