// Back and forward through Hub, as in a browser: the mouse's side buttons and Alt ← / Alt →.
// Every place you go is an entry in the window's own history, so the webview's own back and
// forward (also over a live page's frame) land here too. Opening the place you are on adds
// nothing, and dialogs, menus and the big page panel are not places.
import { untrack } from "svelte";
import { hub, type Route } from "./state.svelte";
import { stepInModal } from "./modalHistory.svelte";

type Entry = { hub: 1; idx: number; route: Route };

const key = (r: Route) => JSON.stringify(r);
const ours = (s: unknown): s is Entry => !!s && typeof s === "object" && (s as Entry).hub === 1;

/** The entry on screen, and the newest one there is. */
let shown = 0;
let last = 0;
/** Steps we asked the webview for, so a step it also takes on its own can be told apart. */
let owed = 0;
let owedUntil = 0;
/** A press of back or forward just moved us; a second step that way right after is its echo. */
let pressed: { dir: number; until: number } | null = null;

function travel(delta: number) {
  if (!delta) return;
  owed++;
  owedUntil = performance.now() + 1000;
  history.go(delta);
}

/** Back (-1) or forward (1), when there is somewhere to go. */
export function step(dir: -1 | 1) {
  // A window over the page (Settings) takes the step itself.
  if (stepInModal(dir)) return;
  const target = shown + dir;
  if (target < 0 || target > last) return;
  pressed = { dir, until: performance.now() + 400 };
  travel(dir);
}

/** A thread that is gone is skipped, as is a place that looks like the one you are on. */
function skip(r: Route) {
  if (key(r) === key(hub.route)) return true;
  return r.view === "thread" && hub.loaded && !hub.threads.some((t) => t.id === r.thread);
}

function onPop(e: PopStateEvent) {
  if (!ours(e.state)) return;
  const at = e.state.idx;
  const now = performance.now();
  const asked = owed > 0 && now < owedUntil;
  owed = asked ? owed - 1 : 0;
  if (at === shown) return;
  const dir = Math.sign(at - shown);
  // The webview took the same step a button press already took: go back to where it landed.
  if (!asked && pressed && pressed.dir === dir && now < pressed.until) {
    pressed = null;
    return travel(shown - at);
  }
  if (skip(e.state.route)) {
    const next = at + dir;
    return travel(next >= 0 && next <= last ? dir : shown - at);
  }
  shown = at;
  hub.show(e.state.route);
}

function record(r: Route) {
  const here = history.state;
  if (ours(here) && key(here.route) === key(r)) return;
  shown = (ours(here) ? here.idx : shown) + 1;
  last = shown;
  history.pushState({ hub: 1, idx: shown, route: $state.snapshot(r) } satisfies Entry, "");
}

const typing = (el: EventTarget | null) =>
  el instanceof HTMLElement && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName));

function onMouse(e: MouseEvent) {
  // 3 is the back button and 4 forward. The webview would act on them itself; we do it once.
  if (e.button !== 3 && e.button !== 4) return;
  e.preventDefault();
  if (e.type === "mouseup") step(e.button === 3 ? -1 : 1);
}

function onKey(e: KeyboardEvent) {
  if (!e.altKey || e.ctrlKey || e.shiftKey || e.metaKey) return;
  if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
  e.preventDefault();
  // In a text field the keys stay there, so a half written message is not left behind.
  if (typing(e.target) || e.repeat) return;
  step(e.key === "ArrowLeft" ? -1 : 1);
}

/** Where you were when the app restarted for an update, so the new app opens there. A reload
 *  keeps the window's own history; a new app does not. */
const PLACE_KEY = "hub.place";

export function keepPlace(r: Route) {
  try {
    localStorage.setItem(PLACE_KEY, JSON.stringify({ route: $state.snapshot(r), at: Date.now() }));
  } catch {
    // Without storage the app opens Home, as it did.
  }
}

function keptPlace(): Route | null {
  try {
    const kept = JSON.parse(localStorage.getItem(PLACE_KEY) ?? "null");
    localStorage.removeItem(PLACE_KEY);
    // Only right after an update; a later start opens where it always does.
    return kept && Date.now() - kept.at < 5 * 60_000 && kept.route?.view ? kept.route : null;
  } catch {
    return null;
  }
}

/** Keeps the window's history in step with where you are. Returns how to stop. */
export function installHistory() {
  const here = history.state;
  const kept = keptPlace();
  if (ours(here)) {
    // A reload keeps the entries; carry on from the one it is on.
    shown = last = here.idx;
    hub.show(here.route);
  } else if (kept) {
    hub.show(kept);
    history.replaceState({ hub: 1, idx: 0, route: $state.snapshot(hub.route) } satisfies Entry, "");
  } else {
    history.replaceState({ hub: 1, idx: 0, route: $state.snapshot(hub.route) } satisfies Entry, "");
  }
  const stop = $effect.root(() => {
    $effect(() => {
      const r = hub.route;
      untrack(() => record(r));
    });
  });
  addEventListener("popstate", onPop);
  addEventListener("mousedown", onMouse, true);
  addEventListener("mouseup", onMouse, true);
  addEventListener("auxclick", onMouse, true);
  addEventListener("keydown", onKey, true);
  return () => {
    stop();
    removeEventListener("popstate", onPop);
    removeEventListener("mousedown", onMouse, true);
    removeEventListener("mouseup", onMouse, true);
    removeEventListener("auxclick", onMouse, true);
    removeEventListener("keydown", onKey, true);
  };
}
