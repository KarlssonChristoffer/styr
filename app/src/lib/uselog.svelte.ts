// The usage log, client side: which parts of Hub you use, as names and times only. A page view or
// a click on something marked `data-use="area.feature"` is counted; nothing typed, opened or
// named ever goes in (the core also drops any name that is not a short lower-case id). Hits wait
// a few seconds and go to the core in one call, only when it has `usage.log`, and not at all when
// the log is switched off. docs/ARCHITECTURE.md, "The usage log".
import { api } from "./api";
import type { UseHit } from "./bindings/UseHit";
import type { UseReport } from "./bindings/UseReport";
import { hubdHas } from "./compat";

/** The parts of Hub the map knows by name, so one you never use shows as a dashed square. A name
 *  the core has counted that is not here still shows, under its own id. */
export const USE_FEATURES: Record<string, { label: string; area: string }> = {
  "page.home": { label: "Home", area: "pages" },
  "page.thread": { label: "A thread", area: "pages" },
  "page.new": { label: "New thread", area: "pages" },
  "page.overview": { label: "Overview", area: "pages" },
  "page.library": { label: "Library", area: "pages" },
  "page.machines": { label: "Machines", area: "pages" },
  "page.project": { label: "A project", area: "pages" },
  "page.services": { label: "Services", area: "pages" },
  "page.problems": { label: "Problems", area: "pages" },
  "page.routines": { label: "Routines", area: "pages" },
  "page.plugins": { label: "Plugins", area: "pages" },
  "page.welcome": { label: "Välkommen", area: "pages" },
  "page.labs": { label: "Labs", area: "pages" },
  "thread.send": { label: "Send", area: "thread" },
  "thread.steps": { label: "Steps card", area: "thread" },
  "home.snooze": { label: "Snooze", area: "home" },
  "home.settle": { label: "Settle", area: "home" },
  "update.install": { label: "Install update", area: "update" },
  "update.news": { label: "What's new", area: "update" },
  "settings.account": { label: "Account", area: "settings" },
  "settings.appearance": { label: "Appearance", area: "settings" },
  "settings.engines": { label: "Engines", area: "settings" },
  "settings.notifications": { label: "Notifications", area: "settings" },
  "settings.dictation": { label: "Dictation", area: "settings" },
  "settings.updates": { label: "Updates", area: "settings" },
  "settings.customs": { label: "Customs", area: "settings" },
  "settings.safety": { label: "Safety", area: "settings" },
  "settings.apis": { label: "API connections", area: "settings" },
};

const NAME = /^[a-z0-9._-]{1,48}$/;
let queue: UseHit[] = [];
let prev = "";
let timer: ReturnType<typeof setTimeout> | undefined;
let sending = false;
let off = false;

/** Counts one use. A feature said twice in a row within a second counts once. */
export function track(feature: string, tookMs?: number) {
  if (off || !NAME.test(feature)) return;
  const last = queue[queue.length - 1];
  const now = Date.now();
  if (last && last.feature === feature && now - last.ts < 1000) return;
  queue.push({ ts: now, feature, area: feature.split(".")[0] || "app", prev: prev !== feature ? prev : "", took_ms: tookMs ?? null });
  prev = feature;
  timer ??= setTimeout(flush, 8000);
}

async function flush() {
  timer = undefined;
  if (sending || !queue.length) return;
  sending = true;
  const batch = queue.splice(0, 100);
  try {
    if (await hubdHas("usage.log")) await api.useRecord(batch); // compat-until: 2026-12-04
  } catch {
    // Dropped: a log that cannot be written must never get in the way.
  }
  sending = false;
  if (queue.length) timer = setTimeout(flush, 8000);
}

/** Switches the counting off or on in this window at once; the core keeps the same switch. */
export function useOff(v: boolean) {
  off = v;
  if (v) queue = [];
}

/** Counts a click on anything inside an element marked `data-use`. Called once from the app. */
export function watchUse(): () => void {
  const onClick = (e: MouseEvent) => {
    const el = (e.target as Element | null)?.closest?.("[data-use]");
    const name = el?.getAttribute("data-use");
    if (name) track(name);
  };
  const onHide = () => {
    if (document.visibilityState === "hidden" && queue.length) void flush();
  };
  document.addEventListener("click", onClick, true);
  document.addEventListener("visibilitychange", onHide);
  return () => {
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("visibilitychange", onHide);
  };
}

/** The report the Usage map card shows. */
export const uselog = $state<{ data: UseReport | null; error: string; old: boolean }>({ data: null, error: "", old: false });

export async function loadUse(days: number) {
  if (!(await hubdHas("usage.log"))) { // compat-until: 2026-12-04
    uselog.old = true;
    return;
  }
  uselog.old = false;
  try {
    uselog.data = await api.useLog(days);
    uselog.error = "";
    useOff(uselog.data.off);
  } catch (e) {
    uselog.error = String(e);
  }
}
