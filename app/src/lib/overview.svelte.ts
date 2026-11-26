// The Overview page is a board of cards you pick, place and size yourself (board.ts does the
// arithmetic). This holds the catalogue, the saved board, the day the map has picked (the Day
// card shows it), and the one Pulse answer the playful cards share.
import { api } from "./api";
import type { LinesReport } from "./bindings/LinesReport";
import type { Pulse } from "./bindings/Pulse";
import type { Widget } from "./bindings/Widget";
import { SIZE_COLS, clampSpot, fromList, type Size, type Spot } from "./board";
import type { Snippet } from "svelte";
import { recall, remember } from "./lastSeen";
import { prefs, setPref, type ClockKind } from "./prefs.svelte";

export type CardId = "now" | "pulse" | "limit" | "projects" | "timeline" | "machines" | "recent" | "map" | "day" | "records" | "clock" | "daylight" | "time" | "lines" | "devlog" | "health" | "stats" | "customs" | "uselog";
export type { Spot };

/** The two pages that hold a board: each keeps its own layout, and shares the looks, links and
 *  knobs of a card by its id, so the same card looks alike wherever it is put. */
export type Page = "overview" | "home";

export type Group = "Right now" | "Your work" | "Code" | "Projects and models" | "Fun";

// A card's knobs: what its right-click menu lets you set (the frame draws them under Settings,
// the card never draws them itself). A choice is one of a few options, a switch is on or off, and
// an action is a button (Check now, Play the last year). The card reads its values with `knob`.
export type Knob =
  | { id: string; kind: "choice"; label: string; options: { value: string; label: string }[]; default: string }
  | { id: string; kind: "switch"; label: string; default: boolean }
  | { id: string; kind: "action"; label: string };
export type Card = { id: CardId; title: string; blurb: string; group: Group; size: Size; knobs?: Knob[] };

const RANGE = (options: [string, string][], fallback: string): Knob => ({ id: "range", kind: "choice", label: "Period", options: options.map(([value, label]) => ({ value, label })), default: fallback });

export const CARDS: Card[] = [
  {
    id: "map",
    title: "Activity map",
    blurb: "The year, day by day",
    group: "Your work",
    size: "l",
    knobs: [
      { id: "hub", kind: "switch", label: "Hub", default: true },
      { id: "claude", kind: "switch", label: "Claude Code", default: true },
      { id: "git", kind: "switch", label: "Git and GitHub", default: true },
    ],
  },
  { id: "now", title: "Now", blurb: "Working and waiting threads", group: "Right now", size: "m" },
  {
    id: "projects",
    title: "Projects",
    blurb: "Your projects and what runs",
    group: "Right now",
    size: "m",
    knobs: [{ id: "pick", kind: "action", label: "Choose projects…" }],
  },
  {
    id: "timeline",
    title: "Timeline",
    blurb: "Recent hours, a lane per project",
    group: "Right now",
    size: "l",
    knobs: [{ id: "hours", kind: "choice", label: "Period", options: [["6", "6 hours"], ["12", "12 hours"], ["24", "24 hours"], ["72", "3 days"]].map(([value, label]) => ({ value, label })), default: "12" }],
  },
  { id: "machines", title: "Machines", blurb: "Machines and their threads", group: "Right now", size: "s" },
  { id: "recent", title: "Recent threads", blurb: "Latest threads", group: "Right now", size: "s" },
  { id: "pulse", title: "Pulse", blurb: "How busy the agents are", group: "Right now", size: "s" },
  { id: "limit", title: "Limit", blurb: "Plan limit left", group: "Right now", size: "s" },
  { id: "day", title: "The day", blurb: "The day picked in the map", group: "Your work", size: "s" },
  {
    id: "clock",
    title: "When you work",
    blurb: "When you write, by hour",
    group: "Your work",
    size: "s",
    knobs: [RANGE([["30", "Month"], ["90", "90 days"], ["371", "Year"]], "90"), { id: "daylight", kind: "switch", label: "Colour by daylight", default: true }, { id: "play", kind: "action", label: "Play the last year" }],
  },
  { id: "daylight", title: "Daylight", blurb: "Today's hours of sun", group: "Your work", size: "s" },
  {
    id: "time",
    title: "Clock",
    blurb: "The time, in daylight",
    group: "Your work",
    size: "s",
    knobs: [{ id: "shape", kind: "choice", label: "Shape", options: [{ value: "ring", label: "Ring" }, { value: "boxes", label: "Boxes" }, { value: "grids", label: "Grids" }, { value: "rails", label: "Rails" }], default: "ring" }],
  },
  { id: "devlog", title: "Devlog", blurb: "A note on each day", group: "Your work", size: "s" },
  { id: "lines", title: "Code in and out", blurb: "Lines added and removed", group: "Code", size: "s", knobs: [RANGE([["14", "2 weeks"], ["30", "Month"], ["90", "90 days"]], "30")] },
  { id: "health", title: "Health check", blurb: "Project checks, worst first", group: "Projects and models", size: "s", knobs: [{ id: "check", kind: "action", label: "Check now" }] },
  { id: "customs", title: "Customs", blurb: "Customs today", group: "Projects and models", size: "s", knobs: [{ id: "open", kind: "action", label: "Open Settings > Customs" }] },
  { id: "stats", title: "Models and cost", blurb: "Models, tokens and cost", group: "Projects and models", size: "s", knobs: [RANGE([["7", "Week"], ["30", "Month"], ["365", "Year"]], "30")] },
  { id: "uselog", title: "Usage map", blurb: "Which parts of Hub you use", group: "Fun", size: "m", knobs: [RANGE([["7", "Week"], ["30", "Month"], ["90", "90 days"]], "30"), { id: "off", kind: "switch", label: "Stop counting", default: false }, { id: "clear", kind: "action", label: "Clear the log" }] },
  { id: "records", title: "Records and badges", blurb: "Best days and badges", group: "Fun", size: "s" },
];

export const cardOf = (id: string) => CARDS.find((c) => c.id === id);

// The frame around a card (`CardFrame.svelte`): its title, on or off and in your own words, and
// its knobs' values, kept per card in `prefs.overviewFrames`. A card is just its content; the
// title and the knobs are the board's, set in the right-click menu.

/** The old places a few knobs were kept: read when the knob is unset, written along with it. */
function oldKnob(card: string, knob: string): string | boolean | undefined {
  if (card === "time" && knob === "shape") return prefs.clockKind;
  if (card === "clock" && knob === "daylight") return prefs.overviewDaylight;
  if (card === "map") {
    try {
      const v = JSON.parse(localStorage.getItem("hub.activity.sources") ?? "{}")[knob];
      return typeof v === "boolean" ? v : undefined;
    } catch {
      return undefined;
    }
  }
  return undefined;
}
function writeOldKnob(card: string, knob: string, value: string | boolean) {
  if (card === "time" && knob === "shape" && typeof value === "string") setPref("clockKind", value as ClockKind);
  if (card === "clock" && knob === "daylight" && typeof value === "boolean") setPref("overviewDaylight", value);
  if (card === "map" && typeof value === "boolean") {
    try {
      const all = JSON.parse(localStorage.getItem("hub.activity.sources") ?? "{}");
      localStorage.setItem("hub.activity.sources", JSON.stringify({ ...all, [knob]: value }));
    } catch {}
  }
}

/** A knob's value for a card: what was set, else what it was in the old place, else the default. */
export function knob<T extends string | boolean>(card: string, id: string): T {
  const def = cardOf(card)?.knobs?.find((k) => k.id === id);
  const fallback = def && def.kind !== "action" ? def.default : undefined;
  const set = prefs.overviewFrames?.[card]?.knobs?.[id];
  const value = set ?? oldKnob(card, id) ?? fallback;
  // A choice that is no longer offered falls back, so a saved value from another version is safe.
  if (def?.kind === "choice" && !def.options.some((o) => o.value === value)) return def.default as T;
  if (def?.kind === "switch" && typeof value !== "boolean") return def.default as T;
  return value as T;
}
export function setKnob(card: string, id: string, value: string | boolean) {
  const all = { ...(prefs.overviewFrames ?? {}) };
  all[card] = { ...all[card], knobs: { ...all[card]?.knobs, [id]: value } };
  setPref("overviewFrames", all);
  writeOldKnob(card, id, value);
}
/** What a card's title says, if it is on: your words, or the catalogue's name. */
export function frameTitle(card: string, fallback: string): string | null {
  const f = prefs.overviewFrames?.[card];
  if (!f?.titled) return null;
  return f.title?.trim() || fallback;
}
export function setFrameTitle(card: string, titled: boolean, title?: string) {
  const all = { ...(prefs.overviewFrames ?? {}) };
  all[card] = { ...all[card], titled, title: title ?? all[card]?.title ?? "" };
  setPref("overviewFrames", all);
}
/** Something a card asked to be done from its menu (an action knob): the card listens. */
export const actions = $state({ n: 0, card: "", id: "" });
export function act(card: string, id: string) {
  actions.n++;
  actions.card = card;
  actions.id = id;
}
/** Runs `f` each time the menu presses `id` on this card. */
export function onAction(card: string, id: string, f: () => void) {
  let seen = actions.n;
  $effect(() => {
    if (actions.n > seen && actions.card === card && actions.id === id) f();
    seen = actions.n;
  });
}
/** The "How it's counted" text each card registers (`HowCounted.svelte`), shown by the frame
 *  when the menu asks for it. */
export const about = $state<Record<string, Snippet>>({});

/** The cards a new Overview starts with, in the order the old list showed them. */
const DEFAULT_LIST: { id: CardId; size: Size }[] = [
  { id: "map", size: "l" },
  { id: "records", size: "s" },
  { id: "clock", size: "s" },
  { id: "time", size: "s" },
  { id: "daylight", size: "s" },
  { id: "lines", size: "s" },
  { id: "health", size: "s" },
  { id: "day", size: "s" },
  { id: "devlog", size: "s" },
  { id: "stats", size: "s" },
  { id: "customs", size: "s" },
];

/** The cards a new Home starts with: what is going on now first, then where each project stands. */
const HOME_LIST: { id: CardId; size: Size }[] = [
  { id: "now", size: "m" },
  { id: "projects", size: "m" },
  { id: "timeline", size: "l" },
  { id: "limit", size: "s" },
  { id: "machines", size: "s" },
  { id: "pulse", size: "s" },
];

/** The board a new page starts with. */
export function defaultBoard(page: Page = "overview"): Spot[] {
  return fromList(page === "home" ? HOME_LIST : DEFAULT_LIST);
}

/** The first starting set of the old list: a layout still exactly like it was never chosen, so
 *  it moves on to the default (its full-width devlog ran lines across a wide screen). */
const FIRST_DEFAULT = JSON.stringify([
  { id: "map", size: "l" },
  { id: "records", size: "s" },
  { id: "clock", size: "s" },
  { id: "time", size: "s" },
  { id: "daylight", size: "s" },
  { id: "lines", size: "s" },
  { id: "health", size: "s" },
  { id: "day", size: "l" },
  { id: "stats", size: "s" },
  { id: "devlog", size: "full" },
]);

const known = new Set<string>(CARDS.map((c) => c.id));
/** A widget's card id (`widgets.rs`): known before the list of widgets has loaded. */
export const isWidget = (id: string) => /^w[0-9a-f]{10}$/.test(id);
const isCard = (id: string) => known.has(id) || isWidget(id);
const sizes = new Set<string>(Object.keys(SIZE_COLS));

/** The saved board, with anything this version does not know left out (a card combined into
 *  another too). A page saved as the old list (cards in order, each S/M/L/Full) is laid onto the
 *  board the way it looked. */
export function layout(page: Page = "overview"): Spot[] {
  const seen = new Set<string>();
  const fresh = (id: string) => isCard(id) && !seen.has(id) && !!seen.add(id);
  const board = page === "home" ? prefs.homeBoard : prefs.overviewBoard;
  if (board) {
    return board
      .filter((s) => fresh(s.id) && [s.x, s.y, s.w].every(Number.isFinite))
      .map((s) => clampSpot({ id: s.id, x: s.x, y: s.y, w: s.w, h: Number.isFinite(s.h) ? s.h : null, with: Array.isArray(s.with) ? s.with.filter((id) => typeof id === "string" && fresh(id)) : undefined }));
  }
  if (page === "home") return defaultBoard("home");
  const list = prefs.overviewCards;
  if (!list || JSON.stringify(list.map((p) => ({ id: p.id, size: p.size }))) === FIRST_DEFAULT) return defaultBoard();
  return fromList(list.filter((p) => sizes.has(p.size) && fresh(p.id)));
}

/** Saves a page's board; null goes back to the starting one. */
export function saveLayout(page: Page, board: Spot[] | null) {
  const spots = board ? board.map((s) => ({ ...s })) : null;
  if (page === "home") return setPref("homeBoard", spots);
  setPref("overviewBoard", spots);
  if (!board) setPref("overviewCards", null);
}

/** The widgets agents wrote on request, asked for again every few seconds while one is being
 *  built, so it fills in as soon as its page is shown. */
export const widgets = $state({ list: [] as Widget[], loaded: false });
let widgetTimer: ReturnType<typeof setTimeout> | null = null;

export async function loadWidgets() {
  try {
    widgets.list = await api.widgets();
  } catch {
    // An older core without widgets: none to show.
  } finally {
    widgets.loaded = true;
  }
  if (widgetTimer) clearTimeout(widgetTimer);
  widgetTimer = widgets.list.some((w) => w.state === "building") ? setTimeout(loadWidgets, 3000) : null;
}

export function widgetOf(id: string): Widget | undefined {
  return widgets.list.find((w) => w.id === id);
}

/** Starts a widget and lists it at once, building. */
export async function makeWidget(text: string): Promise<Widget> {
  const made = await api.createWidget(text);
  widgets.list = [...widgets.list.filter((w) => w.id !== made.id), made];
  void loadWidgets();
  return made;
}

export async function changeWidget(id: string, text: string) {
  const changed = await api.changeWidget(id, text);
  widgets.list = widgets.list.map((w) => (w.id === id ? changed : w));
  void loadWidgets();
}

export async function deleteWidget(id: string) {
  await api.deleteWidget(id);
  widgets.list = widgets.list.filter((w) => w.id !== id);
}

/** Colours a card can take (right-click a card), the palette in app.css. Status colours are
 *  left out: those mean state. */
export const TINTS = ["violet", "blue", "sky", "teal", "lime", "orange", "rose", "pink", "sand", "slate"] as const;
export type Look = { tint?: string; alpha?: number; bare?: boolean };

export function lookOf(id: string): Look {
  const l = prefs.overviewLooks?.[id] ?? {};
  return {
    tint: TINTS.includes(l.tint as (typeof TINTS)[number]) ? l.tint : undefined,
    alpha: typeof l.alpha === "number" ? Math.max(0, Math.min(1, l.alpha)) : undefined,
    bare: l.bare === true || undefined,
  };
}

/** Changes how a card looks; an empty look is dropped. Cards linked to it for their look
 *  (directly or through others) change the same way, so a linked group keeps one colour. */
export function setLook(id: string, change: Look) {
  const all = { ...(prefs.overviewLooks ?? {}) };
  for (const card of [id, ...reach(id, "look")]) {
    const next = { ...lookOf(card), ...change };
    if (!next.tint && (next.alpha == null || next.alpha === 1) && !next.bare) delete all[card];
    else all[card] = next;
  }
  setPref("overviewLooks", all);
}

/** The day the map has picked, which the Day card follows, and how lit the map drew each day. */
export const overview = $state({
  day: null as string | null,
  levels: {} as Record<string, number>,
  /** The day each other card has picked or was handed by a card linked to it (map, lines). */
  picked: {} as Record<string, string>,
  /** The range in days a card was handed by a linked card, and a count so the same range
   *  handed twice is still news. */
  ranges: {} as Record<string, { days: number; n: number }>,
});

// Linked cards (right-click a card > Linked with). A link shares one or both of:
//  - time: a day or a range picked in one card is handed to every card linked to it, directly or
//    through others, that can take it. Only cards that pick days or ranges share time.
//  - look: the colour, see-through and frame set on one card are set on the others too. Any two
//    cards, widgets included, can share their look.
// Links go both ways.

export type Share = "time" | "look";
export type Link = [string, string, Share[]?];

/** What each card can hand on and take: a day, a range of days. */
const TAKES: Record<string, { day?: boolean; range?: boolean }> = {
  map: { day: true },
  day: { day: true },
  lines: { day: true, range: true },
  clock: { range: true },
  stats: { range: true },
};
/** The links a new Overview starts with: the map drives the Day card, as it always has. */
const DEFAULT_LINKS: Link[] = [["map", "day", ["time"]]];

/** Whether two cards can share time: both pick a day, or both pick a range. */
export function canShareTime(a: string, b: string): boolean {
  const x = TAKES[a], y = TAKES[b];
  return a !== b && !!x && !!y && ((!!x.day && !!y.day) || (!!x.range && !!y.range));
}
/** What a link between two cards can share: time where both take it, the look always. */
export function sharable(a: string, b: string): Share[] {
  if (a === b) return [];
  return canShareTime(a, b) ? ["time", "look"] : ["look"];
}
/** What a saved link shares. Links saved before they had a list shared only time. */
function sharesOf(l: Link): Share[] {
  const ok = sharable(l[0], l[1]);
  return (l[2] ?? ["time"]).filter((s) => ok.includes(s));
}
/** Every link, each with what it shares; a link that shares nothing is left out. */
export function links(): [string, string, Share[]][] {
  const saved = (prefs.overviewLinks ?? DEFAULT_LINKS) as Link[];
  return saved.map((l): [string, string, Share[]] => [l[0], l[1], sharesOf(l)]).filter((l) => l[2].length > 0);
}
const pairIs = (l: Link, a: string, b: string) => (l[0] === a && l[1] === b) || (l[0] === b && l[1] === a);
/** What the link between two cards shares; empty when they are not linked. */
export function sharedBy(a: string, b: string): Share[] {
  return links().find((l) => pairIs(l, a, b))?.[2] ?? [];
}
export function isLinked(a: string, b: string): boolean {
  return sharedBy(a, b).length > 0;
}
/** Sets what two cards share; sharing nothing unlinks them. */
export function setShared(a: string, b: string, shares: Share[]) {
  const ok = sharable(a, b);
  const keep = ok.filter((s) => shares.includes(s));
  const hadLook = sharedBy(a, b).includes("look");
  const rest = links().filter((l) => !pairIs(l, a, b));
  setPref("overviewLinks", keep.length ? [...rest, [a, b, keep]] : rest);
  // Linking looks makes the new partner take this card's look at once.
  if (keep.includes("look") && !hadLook) setLook(a, lookOf(a));
}
/** Links two cards with all they can share, or unlinks them. */
export function toggleLink(a: string, b: string) {
  setShared(a, b, isLinked(a, b) ? [] : sharable(a, b));
}
/** Turns one thing two linked cards share on or off. */
export function toggleShare(a: string, b: string, share: Share) {
  const now = sharedBy(a, b);
  setShared(a, b, now.includes(share) ? now.filter((s) => s !== share) : [...now, share]);
}
/** Every card reached from `id` through links that share `what`, itself left out. */
export function reach(id: string, what: Share = "time"): string[] {
  const all = links().filter((l) => l[2].includes(what));
  const seen = new Set([id]);
  const queue = [id];
  while (queue.length) {
    const at = queue.shift()!;
    for (const [a, b] of all) {
      const next = a === at ? b : b === at ? a : null;
      if (next && !seen.has(next)) {
        seen.add(next);
        queue.push(next);
      }
    }
  }
  seen.delete(id);
  return [...seen];
}

function setDay(id: string, date: string) {
  if (id === "day") overview.day = date;
  else overview.picked[id] = date;
}
/** A card picked a day: it and every card linked to it that takes a day show it. */
export function pickDay(from: string, date: string) {
  setDay(from, date);
  for (const id of reach(from)) if (TAKES[id]?.day) setDay(id, date);
}
let handed = 0;
/** A card picked a range: every card linked to it that takes one moves to its nearest range. */
export function pickRange(from: string, days: number) {
  handed++;
  for (const id of reach(from)) if (TAKES[id]?.range) overview.ranges[id] = { days, n: handed };
}
/** Moves a card to its nearest range whenever a linked card hands it one after it mounted; a range
 *  left in `overview.ranges` from before (the card re-added, Overview reopened) is not news. */
export function takeRange(id: string, options: number[], apply: (days: number) => void) {
  const seen = overview.ranges[id]?.n ?? 0;
  $effect(() => {
    const got = overview.ranges[id];
    if (got && got.n > seen) apply(nearest(options, got.days));
  });
}
/** Of a card's own ranges, the one nearest to `days`. */
export function nearest(options: number[], days: number): number {
  return options.reduce((best, d) => (Math.abs(Math.log(d / days)) < Math.abs(Math.log(best / days)) ? d : best), options[0]);
}

const tz = -new Date().getTimezoneOffset();
/** A year and a bit, the same span as the map. */
export const PULSE_DAYS = 371;
const PULSE_KEY = `pulse.${PULSE_DAYS}.${tz}`;

/** The Pulse every playful card reads: shown from last time at once, asked for once per visit. */
export const pulse = $state({ data: recall<Pulse>(PULSE_KEY), loaded: false, error: "" });
let asked: Promise<void> | null = null;

export function loadPulse(): Promise<void> {
  asked ??= api
    .pulse(PULSE_DAYS, tz)
    .then((p) => {
      pulse.data = p;
      pulse.error = "";
      remember(PULSE_KEY, p);
    })
    .catch((e) => {
      pulse.error = String(e);
    })
    .finally(() => {
      pulse.loaded = true;
      // The next visit to the page asks again.
      setTimeout(() => (asked = null), 60_000);
    });
  return asked;
}

const LINES_KEY = `pulse.lines.${PULSE_DAYS}.${tz}`;

/** Lines in and out, from git: slow the first time (every commit is diffed once), quick after.
 *  `since` is when the current ask started, for the wait shown on the cards. */
export const lines = $state({ data: recall<LinesReport>(LINES_KEY), loaded: false, error: "", since: 0 });
let linesAsked: Promise<void> | null = null;

export function loadLines(): Promise<void> {
  if (!linesAsked) lines.since = Date.now();
  linesAsked ??= api
    .pulseLines(PULSE_DAYS, tz)
    .then((l) => {
      lines.data = l;
      lines.error = "";
      remember(LINES_KEY, l);
    })
    .catch((e) => {
      lines.error = String(e);
    })
    .finally(() => {
      lines.loaded = true;
      lines.since = 0;
      setTimeout(() => (linesAsked = null), 60_000);
    });
  return linesAsked;
}

/** `YYYY-MM-DD` of a moment in this computer's clock. */
export function dateKey(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
