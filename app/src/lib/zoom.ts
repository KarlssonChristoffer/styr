// Ctrl/Cmd with + - 0 or the mouse wheel zooms the whole app. The level is remembered.
import { quiet } from "./quiet";
import { getCurrentWebview } from "@tauri-apps/api/webview";

const KEY = "hub.zoom";
export const STEPS = [0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3];
const inTauri = "__TAURI_INTERNALS__" in window;

let level = load();

function load(): number {
  try {
    const saved = Number(localStorage.getItem(KEY));
    return STEPS.includes(saved) ? saved : 1;
  } catch {
    return 1;
  }
}

function apply() {
  if (inTauri) {
    // Native webview zoom keeps text crisp and layout correct.
    quiet(getCurrentWebview().setZoom(level), "get current webview");
  } else {
    document.documentElement.style.zoom = String(level);
  }
  // CSS reads the level to keep a few things, like the window buttons, at their screen size.
  document.documentElement.style.setProperty("--zoom", String(level));
  window.dispatchEvent(new CustomEvent("hub-zoom", { detail: level }));
  try {
    localStorage.setItem(KEY, String(level));
  } catch {
    // Not remembering the level is fine.
  }
}

export function zoomLevel(): number {
  return level;
}

/** Sets the zoom from the settings page; Ctrl and the wheel keep working from there. */
export function setZoom(value: number) {
  level = STEPS.includes(value) ? value : 1;
  apply();
}

function step(direction: 1 | -1) {
  const i = STEPS.indexOf(level);
  level = STEPS[Math.min(STEPS.length - 1, Math.max(0, i + direction))];
  apply();
}

export function installZoom(): () => void {
  apply();
  const onKey = (e: KeyboardEvent) => {
    if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
    if (e.key === "+" || e.key === "=" || e.code === "NumpadAdd") step(1);
    else if (e.key === "-" || e.code === "NumpadSubtract") step(-1);
    else if (e.key === "0" || e.code === "Numpad0") {
      level = 1;
      apply();
    } else return;
    e.preventDefault();
  };
  // Trackpad pinch also arrives as ctrl+wheel; small deltas are summed so it feels smooth.
  let pending = 0;
  const onWheel = (e: WheelEvent) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    pending += e.deltaY;
    if (Math.abs(pending) >= 50) {
      step(pending < 0 ? 1 : -1);
      pending = 0;
    }
  };
  window.addEventListener("keydown", onKey);
  window.addEventListener("wheel", onWheel, { passive: false });
  return () => {
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("wheel", onWheel);
  };
}
