// Settings that belong to this window rather than the core, remembered in the webview.
import type { Theme } from "./themes";
import type { CardTheme, PickerTheme, SwitchStyle } from "./cardSwitch";
import { TEXT_MIN, textGrow, type TextSize } from "./textSize";
import { CHAT_SIZE, CHAT_WIDTHS, CODE_SIZE, fontStack, loadFont, type ChatWidth } from "./fonts";

const KEY = "hub.prefs";

/** How much of the agent's work the chat shows, quietest first. "all" is called Full. */
/** The shapes the Clock card can take. */
export type ClockKind = "ring" | "boxes" | "grids" | "rails";

export type WorkDetail = "minimal" | "quiet" | "normal" | "all";
export const DETAILS: { value: WorkDetail; name: string; text: string }[] = [
  { value: "minimal", name: "Minimal", text: "One playful word while it works. Just the answer when it is done." },
  { value: "quiet", name: "Quiet", text: "What it is doing now, in one line. Files changed under the answer." },
  { value: "normal", name: "Normal", text: "Its latest note and the command output, the rest folded." },
  { value: "all", name: "Full", text: "Every note, edit with its diff and command output, in place." },
];

/** The one project focus mode shows, and when you went into it. */
export type FocusOn = { cwd: string; host: string | null; name: string; since: number };
/** What a notice can be about, for a thread (`notices.ts`). */
export type NoticeSort = "waiting" | "failed" | "done";
/** One of your own notice lines: "tell me when <sort> in <project> as <how>, after <minutes>". */
export type NoticeLine = {
  id: string;
  sort: NoticeSort;
  /** The project's folder (`projectFolder`), or null for every project. */
  project: string | null;
  how: "notice" | "flash" | "sound";
  /** Minutes the thread must still be in that state before it speaks; 0 is at once. */
  after: number;
};
/** A notification from another project, held back while you were in focus mode. */
export type Held = { id: string; title: string; what: "waiting" | "done" | "failed"; at: number };

const defaults = {
  /** A desktop notification when a thread asks you something. Off until you choose it (`quietStart`). */
  notifyWaiting: false,
  /** A desktop notification when a thread finishes or fails. Off until you choose it (`quietStart`). */
  notifyDone: false,
  /** Flash the taskbar button when a thread asks you something. Off until you choose it (`quietStart`). */
  flashTaskbar: false,
  /** Set once the three switches above were put back to off: everyone starts quiet, and only what
   *  you switch on after that stays on. */
  quietStart: false,
  /** A desktop notification when a thread or its engine fails. Off until you choose it. */
  notifyFailed: false,
  /** Which project each of the three switches above is limited to: its folder, or null for all. */
  noticeScope: { waiting: null, failed: null, done: null } as Record<NoticeSort, string | null>,
  /** Notice lines you built yourself, on top of the three switches (`notices.ts`). */
  noticeLines: [] as NoticeLine[],
  /** When Hub keeps quiet: at the computer, in hours, or paused (`notices.ts`). */
  noticeQuiet: {
    /** Nothing about what is in front of you: Hub is the window you use, and your keys or mouse moved lately. */
    atPc: true,
    /** Quiet hours on this computer's clock. */
    hours: false,
    from: 23,
    to: 7,
    /** Quiet hours hold back what needs you too (otherwise only done and failed wait). */
    strict: false,
    /** Paused until this time (ms); 0 when not paused. */
    pausedUntil: 0,
  },
  /** The version whose patch notes you have opened. */
  seenNotes: "",
  /** A built version you said to install later; the next one asks again. */
  laterUpdate: "",
  /** Install a ready version by itself after a few minutes in which you did nothing in Hub. */
  autoInstall: true,
  /** Patch-note items you tried from the Updates page (How to test), by `noteKey`. */
  tried: {} as Record<string, "ok" | "broken">,
  /** The room Hub was left in: Work ("activity") or Control ("menu"). Hub opens there again. */
  sidebarMode: "activity" as "menu" | "activity",
  /** The groups of System pages left open in the sidebar (Watch, Automate, Extend by id). */
  systemOpen: ["watch"] as string[],
  /** Colours: the dark default, Cappuccino, or one of the named themes (themes.ts). */
  theme: "dark" as Theme,
  /** The viewer's width in the right panel, dragged at its edge; 0 is the default. */
  viewerWidth: 0,
  /** The sizes that belong to a screen, by the screen's key (`screens.ts`): what you set while the window was on it. A
   *  screen that has none inherits what was last set, with the zoom kept so the window stays wide enough. */
  screens: {} as Record<string, ScreenSizes>,
  /** Widths dragged at the edge of a pane, by name: "sidebar", "column" (a thread's side column), "side" (the home and desk panel). Missing is the default. */
  widths: {} as Record<string, number>,
  /** Your name, shown at the bottom of the sidebar instead of the computer's. */
  displayName: "",
  /** A sound with the notifications. */
  sound: "off" as "off" | "soft" | "bell",
  /** Lists snap instead of sliding, and nothing fades. */
  reduceMotion: false,
  /** The grid that plays wherever something works (`workGrid.ts`): one of its loops by name, or
   *  "random" for a new one each turn. Pour unless you chose (`calmCube`). */
  workIndicator: "pour",
  /** Set once the working grid became the calm, filled Pour by default; a "random" from before
   *  that was the old default, not a choice. */
  calmCube: false,
  /** Text size apart from the zoom: small print grows most, headings hardly. See TEXT_MIN. */
  textSize: "default" as TextSize,
  /** Typeset $$…$$, \[…\] and \(…\) formulas in replies (figures.ts). */
  math: true,
  /** A monospace font for code and the terminal, by name; empty keeps Geist Mono. */
  codeFont: "",
  /** The font of everything that is Hub's own (lists, menus, buttons, your messages), by name; empty keeps Geist. */
  uiFont: "",
  /** The font of the agent's replies; empty follows the interface font. */
  chatFont: "",
  /** The size of the agent's replies and your messages in px (the default is CHAT_SIZE). */
  chatSize: CHAT_SIZE,
  /** Code's size in px (the default is CODE_SIZE). */
  codeSize: CODE_SIZE,
  /** The line height of the agent's replies. */
  chatLine: 1.65,
  /** Where a click on a file or page card opens it: unfolded in the chat, in the right panel, or in a modal. */
  fileOpen: "inline" as "inline" | "panel" | "modal",
  /** How wide the chat and the message box grow on a large screen. */
  chatWidth: "wide" as ChatWidth,
  /** How much of the agent's work a thread shows, for threads that have no level of their own
   *  (`threadDetail`); a new key, so everyone starts on Quiet. minimal: one playful word while it works and the answer when it is done;
   *  quiet: what it does now in one line; normal: also its latest note and a running command's
   *  last lines; all: every note, edit and command, open. See `detailOf`. */
  chatDetail: "quiet" as WorkDetail,
  /** A level a thread was given in its menu (Chat detail), by thread id; the rest follow `chatDetail`. */
  threadDetail: {} as Record<string, WorkDetail>,
  /** A message sent while a turn runs steers (stops the turn and goes next) instead of queueing. */
  steer: false,
  /** The activity list shows Hub's own threads (Guard, routines, ...) under "From Hub". */
  showHubThreads: false,
  /** The activity list shows the iPhone's chats under "Chats", not among the work. */
  showChatThreads: false,
  /** The world the window was in: Dev (work) or Chat (everyday chats, in warm colours). It follows
   *  the thread you open; the world button on the rail changes it (`hub.switchWorld`). */
  world: "dev" as "dev" | "chat",
  /** The activity list folds working threads into a section of their own. */
  workingSection: true,
  /** How the thread list shows when a thread last moved: the time of day ("22:08") or how long
   *  ago ("10 h"). */
  listTime: "clock" as "clock" | "relative",
  /** How much each thread takes in the Inbox: three lines (comfortable), two (compact) or one
   *  (dense). */
  cards: "comfortable" as "comfortable" | "compact" | "dense",
  /** How a group at the bottom of the activity list (From Hub, Settled, ...) opens and folds
   *  (`fold.ts`): rows folding out one by one, stepping down one by one, sliding out like a
   *  drawer, or "now", the same box quickly. */
  groupMotion: "unfold" as "unfold" | "cascade" | "drawer" | "now",
  /** The activity list puts threads in a folder per project. */
  groupByProject: false,
  /** A plan name you gave an engine's subscription (`plan.ts` `shownPlan`), by engine, with the
   *  provider's value it was given for: it stops applying when the provider says something else. */
  planLabels: {} as Record<string, { raw: string; label: string }>,
  /** Whose limits the name menu and the sidebar show: an engine key, empty for Claude. */
  menuAccount: "",
  /** How the limits card changes from one subscription to the next. */
  cardSwitch: "morph" as SwitchStyle,
  /** Sections of a thread's side column that are folded shut, by key. */
  panelShut: ["connections"] as string[],
  /** The backdrop behind the limits card, in the colours of the subscription it shows. */
  cardTheme: "glow" as CardTheme,
  /** The colour in the composer's model picker, following the provider tab. */
  pickerTheme: "edge" as PickerTheme,
  /** Focus mode: only this project shows in the lists, and the others' notifications wait. */
  focusOn: null as FocusOn | null,
  /** The project's Library tab shows cards instead of rows. */
  libraryGrid: false,
  /** What the other projects would have told you while in focus mode, latest per thread. */
  held: [] as Held[],
  /** The Overview's cards in order with their sizes, before the board (overview.svelte.ts). */
  overviewCards: null as { id: string; size: "s" | "m" | "l" | "full" }[] | null,
  /** Home's board, laid out like the Overview's (overview.svelte.ts `layout("home")`). */
  homeBoard: null as { id: string; x: number; y: number; w: number; h: number | null; with?: string[] }[] | null,
  /** The projects Home's Projects card shows, by project key (folder, and machine when it is not
   *  this one); null shows the ones used last. */
  homeProjects: null as string[] | null,
  /** The scene Home and New thread show (`scenes.ts`): "auto" lets Hub pick by what is going on,
   *  anything else is the scene you chose; "custom" is the board you lay out yourself. */
  homeScene: "auto" as "auto" | "needs" | "live" | "away" | "projects" | "today" | "custom",
  /** The Overview's board: each card's place and size in grid units, h null when it follows what
   *  it holds, and `with` the cards combined into it; null is the starting board. Replaces
   *  overviewCards, which is read once to move over. */
  overviewBoard: null as { id: string; x: number; y: number; w: number; h: number | null; with?: string[] }[] | null,
  /** How each Overview card looks, by card: a colour from the palette, how see-through its
   *  background is (1 solid), and whether its frame is hidden. Missing means the plain look. */
  /** Overview cards linked in pairs, and what each pair shares: "time" (a day or range picked in
   *  one reaches the other) and "look" (colour, see-through, frame). A pair saved without the list
   *  shares time. null is the starting set (the map drives the Day card). */
  overviewLinks: null as [string, string, ("time" | "look")[]?][] | null,
  overviewLooks: {} as Record<string, { tint?: string; alpha?: number; bare?: boolean }>,
  /** When you work colours its hours by how light they were at your place (daylight.ts). */
  overviewDaylight: true,
  /** The Clock card's shape (TimeCard.svelte). */
  clockKind: "ring" as ClockKind,
  /** Each Overview card's frame (overview.svelte.ts `frameOf`): whether its title shows and
   *  what it says (empty for the catalogue's name), and the values of its knobs (period, shape,
   *  sources…) by knob id. Missing means the catalogue's defaults. `clockKind`,
   *  `overviewDaylight` and the map's old source switches are read when a knob is unset and
   *  written along with it, so an older Hub on the same window agrees. */
  overviewFrames: {} as Record<string, { titled?: boolean; title?: string; knobs?: Record<string, string | boolean> }>,
  /** Where you are, for the sun: the Overview's daylight colours, Daylight and Clock cards. */
  place: { name: "Stockholm", lat: 59.33, lon: 18.07 } as { name: string; lat: number; lon: number },
};

export type Prefs = typeof defaults;

/** The settings that are about how big things are, so they belong to the screen the window is on: how wide
 *  the panels were dragged, the text size and the chat's sizes. The zoom is the same kind (zoom.ts). */
export const SIZE_KEYS = ["textSize", "chatSize", "codeSize", "chatLine", "chatWidth", "widths", "viewerWidth"] as const;
export type ScreenSizes = Partial<{ zoom: number; textSize: TextSize; chatSize: number; codeSize: number; chatLine: number; chatWidth: ChatWidth; widths: Record<string, number>; viewerWidth: number }>;
/** The screen the window is on now, and whether Hub itself is switching sizes (so that is not taken for a pick). */
export const here = { key: "", applying: false };
const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));

/** Remembers a size for the screen the window is on. */
export function rememberHere(key: keyof ScreenSizes, value: unknown) {
  if (!here.key || here.applying) return;
  prefs.screens = { ...prefs.screens, [here.key]: { ...prefs.screens[here.key], [key]: clone(value) } };
  save();
}
/** Hub's own sizes, for "Use Hub's sizes on this screen". */
export const sizeDefaults = (): Required<Omit<ScreenSizes, "zoom">> => clone(Object.fromEntries(SIZE_KEYS.map((k) => [k, defaults[k]])) as never);

function load(): Prefs {
  try {
    const stored = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    const loaded = { ...defaults, ...stored };
    // Notices used to be on for everyone and a test chat could fill the screen with them.
    if (!stored.quietStart) Object.assign(loaded, { notifyWaiting: false, notifyDone: false, flashTaskbar: false, quietStart: true });
    if (!stored.calmCube) Object.assign(loaded, { workIndicator: stored.workIndicator === "random" || !stored.workIndicator ? "pour" : stored.workIndicator, calmCube: true });
    return loaded;
  } catch {
    return { ...defaults };
  }
}

export const prefs = $state<Prefs>(load());

/** Settings that show as a class on the page, so CSS can follow them. */
function reflect() {
  const root = document.documentElement;
  root.classList.toggle("calm", prefs.reduceMotion);
  root.style.setProperty("--text-min", `${TEXT_MIN[prefs.textSize] ?? 0}px`);
  root.style.setProperty("--text-grow", String(textGrow(prefs.textSize)));
  // Breakpoints follow the text too: the build adds a copy of each for every step (vite.config.ts).
  if (prefs.textSize === "default") delete root.dataset.text;
  else root.dataset.text = prefs.textSize;
  // Fonts: a pick that is not the default is a CSS variable on the page; the default stays the one in app.css.
  const face = (prop: string, name: string, kind: "sans" | "serif" | "mono") => {
    const stack = fontStack(name, kind);
    if (stack) {
      root.style.setProperty(prop, stack);
      loadFont(name.trim());
    } else root.style.removeProperty(prop);
  };
  face("--font", prefs.uiFont, "sans");
  face("--chat-font", prefs.chatFont, "sans");
  face("--mono", prefs.codeFont, "mono");
  root.style.setProperty("--chat-scale", String(prefs.chatSize / CHAT_SIZE));
  root.style.setProperty("--code-scale", String(prefs.codeSize / CODE_SIZE));
  root.style.setProperty("--chat-lh", String(prefs.chatLine));
  root.style.setProperty("--chat", `calc(${CHAT_WIDTHS[prefs.chatWidth] ?? CHAT_WIDTHS.wide}px * var(--text-grow))`);
}
reflect();

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // Not remembering is fine; the setting holds until the window closes.
  }
}

export function setPref<K extends keyof Prefs>(key: K, value: Prefs[K]) {
  prefs[key] = value;
  reflect();
  if ((SIZE_KEYS as readonly string[]).includes(key)) rememberHere(key as (typeof SIZE_KEYS)[number], value);
  save();
}

/** The level a thread shows its work at: the one it was given, else the default. */
export function detailOf(threadId: string): WorkDetail {
  return prefs.threadDetail[threadId] ?? prefs.chatDetail;
}

/** Gives a thread a level of its own; `null` has it follow the default again. */
export function setThreadDetail(threadId: string, level: WorkDetail | null) {
  const next = { ...prefs.threadDetail };
  if (level) next[threadId] = level;
  else delete next[threadId];
  setPref("threadDetail", next);
}
