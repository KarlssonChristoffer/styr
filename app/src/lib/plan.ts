/**
 * How a subscription plan is shown. The name is whatever the provider's own data says (Claude's
 * `/usage` and login file, Codex's `plan_type` and ID token), so a rename on their side shows up
 * here by itself: nothing in Hub lists plan names. This only makes a raw value readable:
 * `max` -> `Max`, `team_plus` -> `Team Plus`, `default_claude_max_20x` -> `Max 20x`. A value it
 * does not understand is still shown, tidied, never dropped.
 */
import { prefs } from "./prefs.svelte";

const NOISE = new Set(["default", "claude", "chatgpt", "plan", "subscription"]);

export function planName(raw: string | null | undefined): string | null {
  const words = (raw ?? "")
    .split(/[\s_\-.]+/)
    .filter(Boolean)
    .filter((w, i, all) => !NOISE.has(w.toLowerCase()) || all.length === 1);
  if (!words.length) return null;
  // A size like `20x` keeps its lower-case x; every other word starts with a capital.
  return words.map((w) => (/^\d+x$/i.test(w) ? w.toLowerCase() : w[0].toUpperCase() + w.slice(1).toLowerCase())).join(" ");
}

/** The key a plan label is tied to: the provider's value as shown, in lower case. */
export const planKey = (raw: string | null | undefined) => planName(raw)?.toLowerCase() ?? null;

/**
 * The plan to show for an engine: the label you gave it in Settings while the provider still says
 * what it said when you gave it, otherwise the provider's own name. A label is a stop-gap for a
 * rename the provider has not made in its data yet (Claude says `max` for what is now sold as
 * Ultra); the moment the provider's value changes the label is ignored and the provider's name
 * shows, so nothing has to be taken away by hand.
 */
export function shownPlan(engine: string, raw: string | null | undefined): string | null {
  const mine = prefs.planLabels[engine];
  if (mine?.label.trim() && mine.raw === planKey(raw)) return mine.label.trim();
  return planName(raw);
}
