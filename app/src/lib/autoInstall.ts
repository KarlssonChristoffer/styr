// Installs a ready version of Hub by itself once you have left Hub alone for a while. An update
// no longer cuts threads off (engines live under keepers, `keeper.rs`) and the window takes the
// new pages in place (`shell.rs`), so all it costs is a moment's reload: done while nobody looks.
import { quiet } from "./quiet";
import { hub } from "./state.svelte";
import { prefs } from "./prefs.svelte";

/** How long without a key, click or scroll in the window counts as away. */
export const AWAY_MS = 3 * 60_000;

/** Whether now is a good moment, given when you last did something here. */
export function dueNow(at: number, lastInput: number, ready: string | null, later: string, on: boolean, asked: string | null): boolean {
  return on && !!ready && ready !== later && ready !== asked && at - lastInput >= AWAY_MS;
}

/** Watches the window for you; returns how to stop. Only the main window runs it. */
export function installAutoInstall(): () => void {
  let lastInput = Date.now();
  let asked: string | null = null;
  const seen = () => (lastInput = Date.now());
  const kinds = ["keydown", "pointerdown", "wheel", "pointermove"] as const;
  for (const k of kinds) addEventListener(k, seen, { capture: true, passive: true });
  const timer = setInterval(() => {
    const u = hub.update;
    const ready = u?.state === "ready" ? u.target : null;
    if (!dueNow(Date.now(), lastInput, ready, prefs.laterUpdate, prefs.autoInstall, asked)) return;
    asked = ready;
    quiet(hub.installUpdate(), "install update");
  }, 15_000);
  return () => {
    clearInterval(timer);
    for (const k of kinds) removeEventListener(k, seen, { capture: true });
  };
}
