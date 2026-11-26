// How machines and projects look. A machine gets the icon of the kind of computer it is (its hubd
// tells, `host.rs`); a project gets a coloured line icon picked from its name, the way Claude
// gives its projects one. The person can pick another icon, colour or a picture of their own;
// those choices live in Hub's store (`GET`/`PUT /api/looks`, `looks.rs`).
import { api } from "./api";
import { hub } from "./state.svelte";
import type { Looks } from "./bindings/Looks";
import type { MachineKind } from "./bindings/MachineKind";
import type { ProjectLook } from "./bindings/ProjectLook";

/** Project icons on a 24 grid, drawn as strokes like the rest of Hub's glyphs. */
export const PROJECT_ICONS = {
  wrench: "M14.7 6.3a4 4 0 0 0-5.4 5.1L4 16.7 7.3 20l5.3-5.3a4 4 0 0 0 5.1-5.4l-2.4 2.4-2.6-.5-.5-2.6z",
  rocket: "M5 15c-1.5 1.3-2 4-2 6 2 0 4.7-.5 6-2M9 18l-3-3M14.5 4.5C17 3 20 3 21 3c0 1 0 4-1.5 6.5L13 16l-5-5zM15 9h.01M9 11l-3.5-.5L8 8h4M13 15l.5 3.5L16 16v-4",
  bolt: "M13 3 5 13.5h6L10 21l8-10.5h-6z",
  leaf: "M5 19c0-8 5-13 15-14-1 10-6 15-14 15M5 19l7-7",
  flame: "M12 21a6 6 0 0 0 6-6c0-4-3-6-4-10-2 2-2.5 4-2.5 6-1-.5-2-1.5-2.3-3C7.5 9.5 6 12 6 15a6 6 0 0 0 6 6z",
  star: "M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.7-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z",
  heart: "M12 20s-7.5-4.5-7.5-10A4.3 4.3 0 0 1 12 7.3 4.3 4.3 0 0 1 19.5 10c0 5.5-7.5 10-7.5 10z",
  cube: "M12 3l8 4.5v9L12 21l-8-4.5v-9zM4 7.5l8 4.5 8-4.5M12 12v9",
  layers: "M12 3 3 8l9 5 9-5zM3 12.5l9 5 9-5M3 17l9 5 9-5",
  book: "M4 5.5A1.5 1.5 0 0 1 5.5 4H11v16H5.5A1.5 1.5 0 0 1 4 18.5zM20 5.5A1.5 1.5 0 0 0 18.5 4H13v16h5.5a1.5 1.5 0 0 0 1.5-1.5z",
  code: "M8.5 7 3.5 12l5 5M15.5 7l5 5-5 5M13.5 4l-3 16",
  terminal: "M3.5 5h17v14h-17zM7 10l3 2-3 2M12.5 15h4",
  globe: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c2.5 2.5 3.8 5.5 3.8 9s-1.3 6.5-3.8 9c-2.5-2.5-3.8-5.5-3.8-9S9.5 5.5 12 3z",
  planet: "M12 6.5a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11zM17.2 9.3c2.9-.9 4.8-.8 5.1.1.5 1.5-3.8 4.2-9.6 6.1S2.3 18 1.8 16.5c-.3-.9 1-2.3 3.4-3.6",
  music: "M9 18V5l11-2v13M9 18a3 3 0 1 1-6 0 3 3 0 0 1 6 0zM20 16a3 3 0 1 1-6 0 3 3 0 0 1 6 0z",
  camera: "M4 8h3.5L9 5.5h6L16.5 8H20v11H4zM12 10.5a3.2 3.2 0 1 0 0 6.4 3.2 3.2 0 0 0 0-6.4z",
  mic: "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3zM5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21",
  chat: "M4 5h16v11H9l-5 4z M8 9.5h8M8 12.5h5",
  gamepad: "M7 8h10a4 4 0 0 1 4 4.5l-.6 3.5a2.4 2.4 0 0 1-4.1 1.2L14.5 15h-5l-1.8 2.2a2.4 2.4 0 0 1-4.1-1.2L3 12.5A4 4 0 0 1 7 8zM8 10.5v3M6.5 12h3M15.5 11h.01M17.5 13h.01",
  robot: "M5 9h14v10H5zM12 5v4M12 4h.01M9 13h.01M15 13h.01M9.5 16h5M2.5 13v3M21.5 13v3",
  palette: "M12 3a9 9 0 0 0 0 18c1.2 0 1.8-.8 1.8-1.7 0-1.3-1-1.6-1-2.6 0-.9.7-1.5 1.7-1.5H17a4 4 0 0 0 4-4c0-4.5-4-8.2-9-8.2zM7.5 11h.01M10 7.5h.01M14.5 7.5h.01",
  chart: "M4 20h16M7 16v-5M12 16V6M17 16v-8",
  shield: "M12 3l7 3v5.5c0 4.4-3 8-7 9.5-4-1.5-7-5.1-7-9.5V6z",
  key: "M14.5 9.5a4.5 4.5 0 1 0-4.2 4.5L9 15.3V18H6.3v2.7H3.5V18l6-6a4.5 4.5 0 0 0 5-2.5zM16 8h.01",
  flask: "M9 3h6M10 3v6l-5.2 9.1A2 2 0 0 0 6.5 21h11a2 2 0 0 0 1.7-2.9L14 9V3M7.5 15h9",
  bug: "M9 7.5V6a3 3 0 0 1 6 0v1.5M7 9.5h10v5a5 5 0 0 1-10 0zM12 9.5V19.5M3.5 13H7M17 13h3.5M4.5 8.5 7 10M19.5 8.5 17 10M4.5 18.5 7 16.5M19.5 18.5 17 16.5",
  compass: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM15.5 8.5l-2 5-5 2 2-5z",
  mountain: "M3 19 9.5 8l4 6.5 2-3L21 19z",
  sun: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4",
  moon: "M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z",
  coffee: "M5 9h11v6a4 4 0 0 1-4 4H9a4 4 0 0 1-4-4zM16 10.5h1.5a2.5 2.5 0 0 1 0 5H16M8 3.5v2.5M11 3.5v2.5M14 3.5v2.5",
  calendar: "M4 6h16v14H4zM4 10h16M8 3.5V7M16 3.5V7",
  clock: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7.5V12l3 2",
  mail: "M3.5 6h17v12h-17zM3.5 7l8.5 6.5L20.5 7",
  cart: "M3 4h2.5l2.2 11h10.6l2-7.5H6.6M9.5 19.5h.01M17 19.5h.01",
  map: "M12 21s-6.5-5.6-6.5-11a6.5 6.5 0 0 1 13 0c0 5.4-6.5 11-6.5 11zM12 7.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5z",
  hub: "M12 4a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM6 16a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM18 16a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM12 8v4M12 12l-5 4.5M12 12l5 4.5",
  box: "M4 8h16v12H4zM3 4h18v4H3zM10 12h4",
  anchor: "M12 4.5a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM12 8.5V21M8 12h8M4.5 13a7.5 7.5 0 0 0 15 0",
  puzzle: "M9 4.5a2 2 0 0 1 4 0V6h4v4h-1.5a2 2 0 0 0 0 4H17v4h-4v-1.5a2 2 0 0 0-4 0V18H5v-4h1.5a2 2 0 0 0 0-4H5V6h4z",
} as const;
export type ProjectIcon = keyof typeof PROJECT_ICONS;

/** Project colours, by name. The values are CSS variables in app.css (`--p-violet`, …), lighter
 *  than the status colours and never green, amber or red, so a project's colour does not read
 *  as a state. */
export const PROJECT_COLORS = ["violet", "blue", "sky", "teal", "lime", "orange", "rose", "pink", "sand", "slate"] as const;
export type ProjectColor = (typeof PROJECT_COLORS)[number];

/** Words in a project's name that say which icon fits it. The first match wins. */
const HINTS: [RegExp, ProjectIcon][] = [
  [/^\d{4}-\d{2}-\d{2}/, "clock"],
  [/hub|central|nav/, "hub"],
  [/bot|agent|ai\b|llm|gpt|claude|hermes/, "robot"],
  [/voice|audio|speech|dicta|mic|röst|tal/, "mic"],
  [/music|spotify|sound|song|ljud/, "music"],
  [/game|play|spel|rift|arsenal|switch|steam/, "gamepad"],
  [/chat|discord|slack|messag|meme/, "chat"],
  [/photo|camera|image|bild|foto|pic/, "camera"],
  [/doc|wiki|library|book|notes|bok|lär|learn|course|school/, "book"],
  [/web|site|portf|blog|page|www/, "globe"],
  [/^scratch$|test|lab|experiment|sandbox|prov/, "flask"],
  [/setup|dev|tool|config|dotfile|verkstan|knegarn|forman/, "wrench"],
  [/shop|store|butik|cart|shopp/, "cart"],
  [/api|server|proxy|infra|deploy|ops|cloud/, "layers"],
  [/data|stat|chart|metric|analytic|report|aftermath/, "chart"],
  [/auth|login|lösen|pass|secret|key|vault/, "key"],
  [/secur|guard|safe|shield|customs|tull/, "shield"],
  [/map|travel|trip|geo|resa/, "map"],
  [/mail|inbox|post/, "mail"],
  [/calendar|kalender|plan|schedule|time/, "calendar"],
  [/desk|work|office|kontor/, "coffee"],
  [/surf|wave|sea|boat|sail/, "anchor"],
  [/space|planet|star|astro|clip/, "planet"],
  [/bug|fix|debug/, "bug"],
];

/** A small stable number for a name. */
function hash(s: string): number {
  let h = 2166136261;
  for (const c of s) h = Math.imul(h ^ c.codePointAt(0)!, 16777619);
  return h >>> 0;
}

const ICON_NAMES = Object.keys(PROJECT_ICONS) as ProjectIcon[];

/** The icon and colour Hub gives a project by itself: an icon its name hints at, or one picked
 *  from the name, and a colour picked from the name. The same name always gets the same. */
export function autoLook(name: string): { icon: ProjectIcon; color: ProjectColor } {
  const n = name.toLowerCase();
  const h = hash(n);
  const hinted = HINTS.find(([re]) => re.test(n))?.[1];
  return { icon: hinted ?? ICON_NAMES[h % ICON_NAMES.length], color: PROJECT_COLORS[(h >>> 8) % PROJECT_COLORS.length] };
}

/** How a project is keyed among the looks: `host|path`, with an empty host for this machine. */
export function projectKey(path: string, host?: string | null): string {
  return `${host && host !== hub.localHost ? host : ""}|${path}`;
}

/** The kinds a machine can be, with the glyph and words each shows. */
export const MACHINE_KINDS: { kind: MachineKind; label: string }[] = [
  { kind: "desktop", label: "Desktop" },
  { kind: "laptop", label: "Laptop" },
  { kind: "board", label: "Single-board computer" },
  { kind: "vps", label: "VPS / cloud" },
  { kind: "server", label: "Server" },
];

const KINDS_KEY = "hub.machineKinds";
const MACS_KEY = "hub.machineMacs";

class LooksState {
  /** What the person picked. */
  saved = $state<Looks>({ machines: {}, projects: {}, pinned: [] });
  /** What each machine's hubd said it is, remembered so an offline machine keeps its icon. */
  reported = $state<Record<string, MachineKind>>(JSON.parse(localStorage.getItem(KINDS_KEY) ?? "{}"));
  /** Machines whose hubd said they run macOS, remembered like `reported`. */
  macs = $state<Record<string, boolean>>(JSON.parse(localStorage.getItem(MACS_KEY) ?? "{}"));
  /** The project the icon picker is open for, and where. */
  picking = $state<{ key: string; name: string; x: number; y: number } | null>(null);
  #asked = new Set<string>();

  async load() {
    try {
      const l = await api.looks();
      this.saved = { machines: l.machines ?? {}, projects: l.projects ?? {}, pinned: l.pinned ?? [] };
    } catch {
      // An older core without looks: everything stays automatic.
    }
  }

  /** Asks a machine's hubd once what kind of computer it is. */
  learn(name: string, local: boolean) {
    if (this.#asked.has(name)) return;
    this.#asked.add(name);
    api
      .hostAbout(local ? null : name)
      .then((a) => {
        const mac = /macos|mac os|darwin/i.test(a.os) || /^mac/i.test(a.model ?? "");
        if (this.macs[name] !== mac) {
          this.macs[name] = mac;
          localStorage.setItem(MACS_KEY, JSON.stringify(this.macs));
        }
        const kind = a.kind ?? guessFromAbout(a.model, a.os);
        if (!kind || this.reported[name] === kind) return;
        this.reported[name] = kind;
        localStorage.setItem(KINDS_KEY, JSON.stringify(this.reported));
      })
      // Offline or busy: ask again in a minute, when the icon is drawn next.
      .catch(() => setTimeout(() => this.#asked.delete(name), 60_000));
  }

  machineKind(name: string, local: boolean): MachineKind {
    return this.saved.machines[name] ?? this.reported[name] ?? guessFromName(name, local);
  }

  /** Whether a machine is a Mac: what its hubd said, else whether its name says so. */
  isMac(name: string, local: boolean): boolean {
    this.learn(name, local);
    return this.macs[name] ?? /\bmac|macbook|imac/i.test(name);
  }

  /** Whether the person picked the machine's kind rather than Hub. */
  machinePicked(name: string): boolean {
    return !!this.saved.machines[name];
  }

  project(path: string, host: string | null | undefined, name: string): { icon: ProjectIcon; color: ProjectColor; image: string | null } {
    const auto = autoLook(name);
    const own = this.saved.projects[projectKey(path, host)];
    const icon = own?.icon && own.icon in PROJECT_ICONS ? (own.icon as ProjectIcon) : auto.icon;
    const color = own?.color && (PROJECT_COLORS as readonly string[]).includes(own.color) ? (own.color as ProjectColor) : auto.color;
    return { icon, color, image: own?.image ?? null };
  }

  async setMachine(name: string, kind: MachineKind | null) {
    const machines = { ...this.saved.machines };
    if (kind) machines[name] = kind;
    else delete machines[name];
    await this.#save({ ...this.saved, machines });
  }

  async setProject(key: string, look: ProjectLook | null) {
    const projects = { ...this.saved.projects };
    if (look && (look.icon || look.color || look.image)) projects[key] = look;
    else delete projects[key];
    await this.#save({ ...this.saved, projects });
  }

  /** Whether a project is pinned to the top of the rail. */
  pinned(key: string): boolean {
    return this.saved.pinned.includes(key);
  }

  /** Pins a project to the rail (at the bottom of the pins) or takes it off. */
  async pin(key: string, on: boolean) {
    const rest = this.saved.pinned.filter((k) => k !== key);
    await this.#save({ ...this.saved, pinned: on ? [...rest, key] : rest });
  }

  /** Moves a pinned project to just before another one, or to the end. */
  async movePin(key: string, before: string | null) {
    const rest = this.saved.pinned.filter((k) => k !== key);
    const at = before ? rest.indexOf(before) : -1;
    rest.splice(at < 0 ? rest.length : at, 0, key);
    await this.#save({ ...this.saved, pinned: rest });
  }

  async #save(next: Looks) {
    const before = this.saved;
    this.saved = next;
    try {
      this.saved = await api.setLooks(next);
    } catch (e) {
      this.saved = before;
      throw e;
    }
  }
}

export const looks = new LooksState();

/** A machine whose hubd is too old to say its kind: what its board and system suggest. */
function guessFromAbout(model: string | null, os: string): MachineKind | null {
  const m = `${model ?? ""} ${os}`.toLowerCase();
  if (/raspberry|orange pi|jetson|odroid/.test(m)) return "board";
  if (/qemu|kvm|vmware|virtual|hetzner|droplet|standard pc/.test(m)) return "vps";
  if (/macbook/.test(m)) return "laptop";
  return null;
}

/** Before a machine has said anything: this one is a desktop, a name may say cloud or Pi. */
function guessFromName(name: string, local: boolean): MachineKind {
  if (local) return "desktop";
  const n = name.toLowerCase();
  if (/vps|cloud|moln|droplet|ec2/.test(n)) return "vps";
  if (/macbook|laptop|book/.test(n)) return "laptop";
  if (/\bpi\b|raspberry|rpi/.test(n)) return "board";
  return "server";
}

/** A picture as a small square PNG data URL for an icon: cropped to the middle, 96 pixels. */
export async function iconPicture(file: File): Promise<string> {
  if (!/^image\/(png|jpeg|webp|gif|svg\+xml|bmp)$/.test(file.type)) throw new Error("Pick a PNG, JPEG, WebP, GIF, SVG or BMP picture");
  if (file.size > 10 * 1024 * 1024) throw new Error("That picture is over 10 MB");
  const url = URL.createObjectURL(file);
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    const side = Math.min(img.naturalWidth, img.naturalHeight) || 96;
    const canvas = Object.assign(document.createElement("canvas"), { width: 96, height: 96 });
    const g = canvas.getContext("2d")!;
    g.imageSmoothingQuality = "high";
    g.drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 0, 0, 96, 96);
    return canvas.toDataURL("image/png");
  } finally {
    URL.revokeObjectURL(url);
  }
}
