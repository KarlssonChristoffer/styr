// Sizes per screen. How big things are (zoom, text size, the chat's sizes, how wide the panels are)
// belongs to the screen the window is on, so moving Hub from the ultrawide to a 24-inch screen does
// not carry the ultrawide's widths along. Hub knows the screen by its size and scale, notices when the
// window moves to another and switches by itself. Fonts and the theme are the same on every screen.
//
// A screen keeps only what was set while the window was on it (`prefs.screens`). A screen that has
// set nothing inherits what was last set, with the zoom lowered if it would leave the window too
// narrow, and takes the panels at their default widths.
import { SIZE_KEYS, here, prefs, rememberHere, setPref, sizeDefaults, type ScreenSizes } from "./prefs.svelte";
import { STEPS, setZoom, zoomLevel } from "./zoom";
import { quietly } from "./quiet";

const inTauri = "__TAURI_INTERNALS__" in window;
/** A window narrower than this (in px at zoom 100%, divided by the zoom) is too narrow for the sidebar, a thread and a panel. */
const NARROWEST = 1200;

type Screen = { key: string; /** Width in px at the system's own scale, before any zoom. */ width: number; label: string };

/** The screen the window is on: by its size in device pixels and the system's scale, not by name, so it is known again after a cable change. */
async function whichScreen(): Promise<Screen> {
  if (inTauri) {
    try {
      const { currentMonitor } = await import("@tauri-apps/api/window");
      const m = await currentMonitor();
      if (m) return { key: `${m.size.width}x${m.size.height}@${m.scaleFactor}`, width: m.size.width / m.scaleFactor, label: `${m.size.width} × ${m.size.height}` };
    } catch {
      // Falls back on what the page can see.
    }
  }
  const dpr = window.devicePixelRatio || 1;
  const w = Math.round((screen.width * dpr) / 8) * 8; // a zoomed page rounds its own screen size, so sizes are matched loosely
  const h = Math.round((screen.height * dpr) / 8) * 8;
  const scale = inTauri ? dpr / zoomLevel() : dpr;
  return { key: `${w}x${h}@${Math.round(scale * 100) / 100}`, width: w / scale, label: `${w} × ${h}` };
}

/** The largest zoom step up to `zoom` that still leaves the window `NARROWEST` px wide. */
export function fitZoom(zoom: number, width: number): number {
  for (const z of STEPS.filter((s) => s <= zoom).reverse()) if (width / z >= NARROWEST) return z;
  return STEPS[0];
}

/** What this screen was last known as, for the settings page. */
export const screenNow = $state({ key: "", label: "", known: false });

/** Puts a screen's sizes on the page. */
export function enterScreen(s: Screen) {
  here.applying = true;
  try {
    here.key = s.key;
    const own = prefs.screens[s.key];
    if (!own && !Object.keys(prefs.screens).length) {
      // The first screen Hub knows keeps what is set now, so nothing changes for it.
      const start: Record<string, unknown> = { zoom: zoomLevel() };
      for (const k of SIZE_KEYS) start[k] = prefs[k];
      setPref("screens", { [s.key]: JSON.parse(JSON.stringify(start)) });
    } else {
      const o: ScreenSizes = own ?? {};
      setPref("widths", o.widths ?? {});
      setPref("viewerWidth", o.viewerWidth ?? 0);
      for (const k of ["textSize", "chatSize", "codeSize", "chatLine", "chatWidth"] as const) if (o[k] !== undefined) setPref(k, o[k] as never);
      const z = o.zoom ?? fitZoom(zoomLevel(), s.width);
      if (z !== zoomLevel()) setZoom(z);
    }
    screenNow.key = s.key;
    screenNow.label = s.label;
    screenNow.known = true;
  } finally {
    here.applying = false;
  }
}

/** "Use Hub's sizes on this screen": forgets what was set here. */
export function resetScreen() {
  if (!here.key) return;
  const { [here.key]: _, ...rest } = prefs.screens;
  here.applying = true;
  try {
    setPref("screens", rest);
    const d = sizeDefaults();
    for (const k of SIZE_KEYS) setPref(k, d[k] as never);
    setZoom(1);
  } finally {
    here.applying = false;
  }
}

/** Follows the window: switches sizes when it moves to another screen. Returns what stops it. */
export function watchScreens(): () => void {
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const check = async () => {
    const s = await whichScreen();
    if (!stopped && s.key !== here.key) enterScreen(s);
  };
  const soon = () => {
    clearTimeout(timer);
    timer = setTimeout(check, 400); // a drag across screens sends many events; the last one counts
  };
  const onZoom = (e: Event) => rememberHere("zoom", (e as CustomEvent<number>).detail);
  const stops: (() => void)[] = [];
  window.addEventListener("resize", soon);
  window.addEventListener("focus", soon);
  window.addEventListener("hub-zoom", onZoom);
  stops.push(() => {
    window.removeEventListener("resize", soon);
    window.removeEventListener("focus", soon);
    window.removeEventListener("hub-zoom", onZoom);
  });
  if (inTauri) {
    import("@tauri-apps/api/window")
      .then(async ({ getCurrentWindow }) => {
        const w = getCurrentWindow();
        const off = [await w.onMoved(soon), await w.onScaleChanged(soon)];
        if (stopped) off.forEach((f) => f());
        else stops.push(() => off.forEach((f) => f()));
      })
      .catch(quietly("window listeners"));
  }
  void check();
  return () => {
    stopped = true;
    clearTimeout(timer);
    stops.forEach((f) => f());
  };
}
