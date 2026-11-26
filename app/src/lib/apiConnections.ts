// The plain parts of Settings > API connections: names, labels and the look of each service.
import type { ApiAuth } from "./bindings/ApiAuth";
import { BRANDS } from "./providerLogos";

export const apiSlug = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "api"; // not docs.ts `slug`: ASCII only, "api" fallback
export const host = (url: string) => url.replace(/^https?:\/\//, "");
export const tokens = (n: number) => (n >= 10_000 ? `${Math.round(n / 1000)}k` : `${n}`);
/** How a connection shows its key, in words (`ApiAuth::says`). */
export function says(a: ApiAuth): string {
  switch (a.type) {
    case "bearer": return "as a Bearer token";
    case "header": return `in the ${a.name} header`;
    case "query": return `in the address as ?${a.name}=`;
    case "basic": return "as a username and password";
    case "login": return "by logging in once on its own page";
  }
}
export const envName = (name: string) => `${name.toUpperCase().replace(/-/g, "_")}_BASE_URL`;
/** How a known service's header looks, a small copy of its own site: a line above the name. The
 *  rest is CSS per `look-<kind>`. */
export const LOOKS: Record<string, { kicker?: string }> = {
  typesafe: { kicker: "TypeSafe AI ··········· Intelligence beyond chat" },
  spotify: {},
  github: { kicker: "api.github.com" },
  openweather: { kicker: "OpenWeather ☀ current · forecast" },
};
/** GitHub's contribution graph, fixed so the header doesn't change between visits. */
export const CONTRIB = Array.from({ length: 7 * 22 }, (_, i) => [0, 0, 1, 0, 2, 1, 3, 0, 4, 2, 1, 0, 3][(i * 7 + (i >> 3)) % 13]);
/** A connection's own colour on its page: Hub's accent takes it, so buttons and switches follow. */
export function brandStyle(kind: string): string {
  const b = BRANDS[kind];
  return b ? `--brand: ${b}; --accent: ${b}; --accent-bg: color-mix(in srgb, ${b} 16%, transparent);` : "--brand: var(--accent);";
}
export const FEATURE_NAMES: Record<string, string> = { auto: "Auto", turns: "Turn ends", focus: "Focus check", proof: "Test output", topic: "Same task?", judge: "While it works" };
export const SAID: Record<string, string> = { asks: "asks you", partway: "half way", goal1: "goal 1", none: "off the stage", new: "new task", same: "same task", failed: "shows failure", passed: "passed" };
export const pct = (n: number, of: number) => (of ? Math.round((100 * n) / of) : 0);
export const KINDS: Record<string, string> = { context: "Context", wording: "Wording", threshold: "Threshold" };
