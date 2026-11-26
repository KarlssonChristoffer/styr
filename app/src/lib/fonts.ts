// Fonts: the ones Hub carries, how a chosen one is loaded, and the looks (presets) for the chat.
// A font is picked by its name. The bundled ones load on first use, as their Latin file only (about
// 50 KB each), so a font nobody picked costs nothing; any other name is taken as a font installed
// on this computer and used as it is.

export type FontKind = "sans" | "serif" | "mono";
type File = () => Promise<{ default: string }>;
type Face = { file: File; italic?: boolean };
type Bundled = { kind: FontKind; family: string; faces: Face[] };

const face = (file: File, italic = false): Face => ({ file, italic });

/** Geist, Geist Mono and Newsreader's upright face come with `boot.ts`; the rest load when picked. */
export const BUNDLED: Record<string, Bundled> = {
  Geist: { kind: "sans", family: "Geist Variable", faces: [] },
  Inter: {
    kind: "sans",
    family: "Inter Variable",
    faces: [face(() => import("@fontsource-variable/inter/files/inter-latin-wght-normal.woff2?url")), face(() => import("@fontsource-variable/inter/files/inter-latin-wght-italic.woff2?url"), true)],
  },
  "Source Sans 3": {
    kind: "sans",
    family: "Source Sans 3 Variable",
    faces: [face(() => import("@fontsource-variable/source-sans-3/files/source-sans-3-latin-wght-normal.woff2?url")), face(() => import("@fontsource-variable/source-sans-3/files/source-sans-3-latin-wght-italic.woff2?url"), true)],
  },
  "IBM Plex Sans": {
    kind: "sans",
    family: "IBM Plex Sans Variable",
    faces: [face(() => import("@fontsource-variable/ibm-plex-sans/files/ibm-plex-sans-latin-wght-normal.woff2?url")), face(() => import("@fontsource-variable/ibm-plex-sans/files/ibm-plex-sans-latin-wght-italic.woff2?url"), true)],
  },
  Figtree: {
    kind: "sans",
    family: "Figtree Variable",
    faces: [face(() => import("@fontsource-variable/figtree/files/figtree-latin-wght-normal.woff2?url")), face(() => import("@fontsource-variable/figtree/files/figtree-latin-wght-italic.woff2?url"), true)],
  },
  Newsreader: {
    kind: "serif",
    family: "Newsreader Variable",
    faces: [face(() => import("@fontsource-variable/newsreader/files/newsreader-latin-wght-italic.woff2?url"), true)],
  },
  Literata: {
    kind: "serif",
    family: "Literata Variable",
    faces: [face(() => import("@fontsource-variable/literata/files/literata-latin-wght-normal.woff2?url")), face(() => import("@fontsource-variable/literata/files/literata-latin-wght-italic.woff2?url"), true)],
  },
  "Geist Mono": { kind: "mono", family: "Geist Mono Variable", faces: [] },
  "JetBrains Mono": {
    kind: "mono",
    family: "JetBrains Mono Variable",
    faces: [face(() => import("@fontsource-variable/jetbrains-mono/files/jetbrains-mono-latin-wght-normal.woff2?url")), face(() => import("@fontsource-variable/jetbrains-mono/files/jetbrains-mono-latin-wght-italic.woff2?url"), true)],
  },
};

const GENERIC: Record<FontKind, string> = {
  sans: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
  serif: '"Iowan Old Style", "Palatino Linotype", Georgia, serif',
  mono: "ui-monospace, Menlo, Consolas, monospace",
};

const loaded = new Map<string, Promise<void>>();

/** Loads a bundled font's files once. A name that is not bundled is an installed font: nothing to load. */
export function loadFont(name: string): Promise<void> {
  const font = BUNDLED[name];
  if (!font || !font.faces.length) return Promise.resolve();
  const known = loaded.get(name);
  if (known) return known;
  const p = Promise.all(
    font.faces.map(async (f) => {
      const url = (await f.file()).default;
      const ff = new FontFace(font.family, `url("${url}") format("woff2")`, { weight: "100 900", style: f.italic ? "italic" : "normal" });
      document.fonts.add(await ff.load());
    }),
  ).then(
    () => undefined,
    () => {
      loaded.delete(name); // tried again at the next pick
    },
  );
  loaded.set(name, p);
  return p;
}

/** The `font-family` to use for a pick; `null` when nothing is picked (the default stays). */
export function fontStack(name: string, kind: FontKind): string | null {
  const n = name.trim().replace(/["\\;{}]/g, "");
  if (!n) return null;
  const b = BUNDLED[n];
  return b ? `"${b.family}", ${GENERIC[b.kind]}` : `"${n}", ${GENERIC[kind]}`;
}

/** Bundled fonts of the given kinds, in the order the pickers list them. */
export function bundledNames(...kinds: FontKind[]): string[] {
  return Object.entries(BUNDLED).filter(([, f]) => kinds.includes(f.kind)).map(([n]) => n);
}

type LocalFonts = { queryLocalFonts?: () => Promise<{ family: string }[]> };
let installed: Promise<string[]> | null = null;

/** Whether the webview can list the fonts installed on this computer at all. */
export const canListInstalled = (): boolean => typeof (window as unknown as LocalFonts).queryLocalFonts === "function";

/** Whether the person has already allowed it, so listing them now asks nothing. */
async function listingAllowed(): Promise<boolean> {
  try {
    const st = await navigator.permissions.query({ name: "local-fonts" as PermissionName });
    return st.state === "granted";
  } catch {
    return false;
  }
}

/**
 * The font families installed on this computer. Reading them makes the webview ask "use the fonts on
 * your computer?", so it is only done when `ask` is true (the person pressed "Show installed fonts")
 * or when they have already said yes; otherwise the list is empty and nothing is shown.
 */
export function installedFonts(ask = false): Promise<string[]> {
  if (installed) return installed;
  const run = (async () => {
    try {
      const q = (window as unknown as LocalFonts).queryLocalFonts;
      if (!q || !(ask || (await listingAllowed()))) return null;
      const all = await q.call(window);
      return [...new Set(all.map((f) => f.family))].sort((a, b) => a.localeCompare(b));
    } catch {
      return null; // blocked or failed: asked again at the next press
    }
  })();
  const p = run.then((l) => {
    if (l === null) installed = null;
    return l ?? [];
  });
  installed = p;
  return p;
}

/** What the chat's reading looks like: size, spacing and width. */
export const CHAT_WIDTHS = { narrow: 720, medium: 820, wide: 920 } as const;
export type ChatWidth = keyof typeof CHAT_WIDTHS;
/** The size the chat text and the code were designed at; a pick is a multiple of it. */
export const CHAT_SIZE = 14.5;
export const CODE_SIZE = 12.5;
export const CHAT_SIZES = [13, 14, 14.5, 15, 16, 16.5, 17, 18, 20];
export const CODE_SIZES = [11, 12, 12.5, 13, 14, 15, 16];
export const CHAT_LINES = [1.4, 1.5, 1.6, 1.65, 1.75, 1.9];

export type Look = { chatFont: string; chatSize: number; chatLine: number; chatWidth: ChatWidth };
export const LOOKS: { id: string; label: string; hint: string; look: Look }[] = [
  { id: "hub", label: "Hub", hint: "Geist, 14.5 px, wide", look: { chatFont: "", chatSize: 14.5, chatLine: 1.65, chatWidth: "wide" } },
  { id: "claude", label: "Claude Code", hint: "Serif replies, 16.5 px, medium width", look: { chatFont: "Newsreader", chatSize: 16.5, chatLine: 1.6, chatWidth: "medium" } },
  { id: "compact", label: "Compact", hint: "13 px, tight lines, wide", look: { chatFont: "", chatSize: 13, chatLine: 1.5, chatWidth: "wide" } },
];
