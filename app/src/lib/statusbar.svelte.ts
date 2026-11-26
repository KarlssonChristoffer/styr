// The status bar at the foot of the main window, and the panel above it (StatusBar.svelte,
// BottomPanel.svelte). Like VS Code's, but with Hub's own things: where you are (machine, branch,
// changed files), what needs you (Guard's problems), what runs (terminals, open ports) and how
// Hub's own development goes (the update, the merge queue, live mode). What it shows follows the
// page: a thread's machine, folder and branch; a project's; elsewhere only the machine.
import { quiet } from "./quiet";
import { api } from "./api";
import type { GitInfo } from "./bindings/GitInfo";
import type { Listener } from "./bindings/Listener";
import type { TerminalSession } from "./bindings/TerminalSession";
import { hub } from "./state.svelte";

export type PanelTab = "terminal" | "ports" | "problems";
export type BarCard = "ports" | "update" | "jev" | "guard" | "sessions";

/** Where the bar is about: the machine, and the folder when the page has one. */
export type Place = {
  /** The machine's name, as the machine page knows it. */
  machine: string;
  /** For calls about another machine; null for this one. */
  host: string | null;
  cwd: string | null;
  thread: string | null;
  /** What the page is, in words: the thread's or project's name, else the machine's. */
  label: string;
};

const TERMS = "hub.bar.terminals";
const HEIGHT = "hub.bar.panel";

function read<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}
function keep(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // A window without storage just forgets.
  }
}

class Bar {
  /** The open tab of the panel above the bar, or null when it is shut. */
  panel = $state<PanelTab | null>(null);
  /** The card open above the bar. */
  card = $state<BarCard | null>(null);
  /** Where the open card's item starts, so the card opens above it. */
  cardX = $state(0);
  height = $state<number>(read(HEIGHT, 280));

  /** Terminals opened from the bar, by id, oldest first. */
  mine = $state<string[]>(read(TERMS, []));
  terminals = $state<TerminalSession[]>([]);
  /** The terminal shown in the panel. */
  terminal = $state<string | null>(null);
  /** What a terminal just opened types first: a `cd` into the page's folder. */
  typeFirst = $state<Record<string, string>>({});

  git = $state<GitInfo | null>(null);
  ports = $state<Listener[] | null>(null);
  portsFailed = $state(false);

  /** The page's place: a thread's, a project's, or this machine. */
  place = $derived.by((): Place => {
    const local = hub.localHost || "this computer";
    const t = hub.view === "thread" ? hub.current : null;
    if (t) {
      const host = t.host && t.host !== hub.localHost ? t.host : null;
      return { machine: host ?? local, host, cwd: t.cwd, thread: t.id, label: t.title };
    }
    if (hub.view === "project" && hub.project) {
      const host = hub.projectHost && hub.projectHost !== hub.localHost ? hub.projectHost : null;
      return { machine: host ?? local, host, cwd: hub.project, thread: null, label: hub.project.split(/[\\/]/).filter(Boolean).pop() ?? hub.project };
    }
    if (hub.view === "machine" && hub.machine) {
      const host = hub.machine !== hub.localHost ? hub.machine : null;
      return { machine: hub.machine, host, cwd: null, thread: null, label: hub.machine };
    }
    return { machine: local, host: null, cwd: null, thread: null, label: local };
  });

  /** The bar's terminals that still run, for the place's machine first. */
  get shells(): TerminalSession[] {
    return this.mine.map((id) => this.terminals.find((t) => t.id === id)).filter((t): t is TerminalSession => !!t && t.exit === null);
  }

  /** The ports listening on the place's machine that are worth a look: not the system's own. */
  get openPorts(): Listener[] {
    const seen = new Set<number>();
    return (this.ports ?? [])
      .filter((l) => !l.system && l.protocol.toLowerCase().startsWith("tcp"))
      .filter((l) => (seen.has(l.port) ? false : (seen.add(l.port), true)))
      .sort((a, b) => Number(a.is_hub) - Number(b.is_hub) || a.port - b.port);
  }

  toggleCard(card: BarCard, x = this.cardX) {
    this.cardX = x;
    this.card = this.card === card ? null : card;
    if (this.card === "ports") void this.loadPorts();
  }

  togglePanel(tab: PanelTab) {
    this.card = null;
    if (this.panel === tab) {
      this.panel = null;
      return;
    }
    this.panel = tab;
    if (tab === "terminal" && !this.shells.length) void this.newTerminal();
    if (tab === "ports") void this.loadPorts();
  }

  setHeight(px: number) {
    this.height = Math.max(140, Math.min(px, Math.round(innerHeight * 0.7)));
    keep(HEIGHT, this.height);
  }

  /** A new shell on the place's machine, in the page's folder when it has one. */
  async newTerminal() {
    const p = this.place;
    try {
      const t = await api.machineTerminal(p.machine, { what: "shell", engine: null, thread: null, cols: 120, rows: 24 });
      this.mine = [...this.mine, t.id];
      keep(TERMS, this.mine);
      this.terminals = [...this.terminals, t];
      if (p.cwd) this.typeFirst = { ...this.typeFirst, [t.id]: `cd "${p.cwd}"\r` };
      this.terminal = t.id;
      this.panel = "terminal";
    } catch (e) {
      hub.fail(e);
    }
  }

  async closeTerminal(id: string) {
    await quiet(api.closeTerminal(id), "close terminal");
    this.forget(id);
  }

  forget(id: string) {
    const i = this.mine.indexOf(id);
    this.mine = this.mine.filter((x) => x !== id);
    keep(TERMS, this.mine);
    this.terminals = this.terminals.filter((t) => t.id !== id);
    if (this.terminal === id) this.terminal = this.mine[Math.min(Math.max(i, 0), this.mine.length - 1)] ?? null;
    if (!this.mine.length && this.panel === "terminal") this.panel = null;
  }

  async loadTerminals() {
    try {
      this.terminals = await api.terminals();
      // Terminals the core no longer has (it restarted) are forgotten.
      const alive = new Set(this.terminals.map((t) => t.id));
      if (this.mine.some((id) => !alive.has(id))) {
        this.mine = this.mine.filter((id) => alive.has(id));
        keep(TERMS, this.mine);
      }
      if (!this.terminal || !this.mine.includes(this.terminal)) this.terminal = this.mine.at(-1) ?? null;
    } catch {
      // An older core or a lost connection: the bar just shows none.
    }
  }

  /** The branch and changed files of the page's folder: the thread's, or the project's. */
  async loadGit() {
    const { thread, cwd, host } = this.place;
    const same = () => this.place.thread === thread && this.place.cwd === cwd;
    if (!cwd) return void (this.git = null);
    try {
      const g: GitInfo = thread ? await api.git(thread) : await api.project(cwd, host).then((p) => ({ branch: p.branch, changed: p.changed }));
      if (same()) this.git = g;
    } catch {
      if (same()) this.git = null;
    }
  }

  async loadPorts() {
    const host = this.place.host;
    try {
      const snap = await api.host(host);
      if (this.place.host === host) {
        this.ports = snap.listeners;
        this.portsFailed = false;
      }
    } catch {
      if (this.place.host === host) this.portsFailed = true;
    }
  }
}

export const bar = new Bar();

/** Keeps a bar card inside the window whatever its real width: the text size makes cards wider
 *  than the width they are placed by, so this measures the card and moves it left (or right) by a
 *  margin just enough, again whenever it or the window changes size. */
export function onScreen(node: HTMLElement, _x?: number) {
  const fit = () => {
    // A margin, not a transform: a transformed card would hold its fixed menus (a Select's list)
    // inside itself instead of the window.
    node.style.marginLeft = "";
    const r = node.getBoundingClientRect();
    const over = r.right - (window.innerWidth - 8);
    const under = 6 - (r.left - Math.max(0, over));
    const shift = Math.max(0, over) - Math.max(0, under);
    if (shift) node.style.marginLeft = `${-shift}px`;
  };
  const seen = new ResizeObserver(fit);
  seen.observe(node);
  window.addEventListener("resize", fit);
  fit();
  return {
    update: fit,
    destroy() {
      seen.disconnect();
      window.removeEventListener("resize", fit);
    },
  };
}
