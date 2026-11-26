// The app's own errors go to the Problems page: anything thrown that nothing caught, and promises
// that failed with nobody waiting. Each goes with the last clicks before it, as breadcrumbs, so a
// thread started on it can see what was done. The same message is sent at most once a minute.
import { api } from "./api";
import type { Crumb } from "./bindings/Crumb";
import { quietly } from "./quiet";

/** What sent a problem, in words. */
export const SOURCES: Record<string, string> = {
  hubd: "Hub's core",
  app: "app window",
  thread: "thread",
  sentry: "Sentry SDK",
  otlp: "OpenTelemetry",
  monitor: "quiet source",
  metrics: "baseline",
  tool: "tool call",
  code_analysis: "code analysis",
};

const crumbs: Crumb[] = [];
const sent = new Map<string, number>();

function crumb(category: string, message: string) {
  crumbs.push({ ts: Date.now(), level: "info", category, message: message.slice(0, 200) });
  if (crumbs.length > 15) crumbs.shift();
}

/** Browser noise that is not a fault in the app. */
const NOISE = [/^ResizeObserver loop/];

export function report(error: unknown, how: string) {
  const e = error instanceof Error ? error : new Error(typeof error === "string" ? error : JSON.stringify(error) ?? String(error));
  if (NOISE.some((re) => re.test(e.message))) return;
  const key = `${e.name}:${e.message}`;
  const now = Date.now();
  if ((sent.get(key) ?? 0) > now - 60_000) return;
  sent.set(key, now);
  api
    .reportProblem({
      project: "Hub",
      source: "app",
      level: "error",
      message: e.message || String(error),
      kind: e.name && e.name !== "Error" ? e.name : null,
      trace: e.stack ?? null,
      frames: [],
      crumbs: [...crumbs],
      release: null,
      environment: null,
      extra: { how, page: location.hash || location.pathname, agent: navigator.userAgent },
      ts: now,
    })
    // Reporting must never make a second error; the core may simply be down.
    .catch(quietly("problem report"));
}

export function installProblemReports() {
  window.addEventListener("error", (ev) => report(ev.error ?? ev.message, "uncaught"));
  window.addEventListener("unhandledrejection", (ev) => report(ev.reason, "unhandled promise"));
  // What was clicked, by its visible name, as what came before.
  document.addEventListener(
    "click",
    (ev) => {
      const el = (ev.target as HTMLElement | null)?.closest("button, a, [role=button], [role=tab]") as HTMLElement | null;
      const name = el?.getAttribute("aria-label") ?? el?.dataset.tip ?? el?.textContent?.trim();
      if (name) crumb("click", name);
    },
    { capture: true, passive: true },
  );
}
