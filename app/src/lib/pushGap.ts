// How often another machine takes a new Hub version from this one (`push_quiet_mins` on the host,
// set with `PATCH /api/hosts/{name}`): the choices of the row on its page (PushGap.svelte).

/** The fixed choices, in minutes; 0 is "as they come" (no gap). */
export const PUSH_GAPS = [0, 15, 30, 60, 120] as const;

/** The words for a gap. */
export function pushGapLabel(mins: number): string {
  if (mins <= 0) return "As they come";
  if (mins % 60 === 0) return mins === 60 ? "At most every hour" : `At most every ${mins / 60} hours`;
  return `At most every ${mins} min`;
}

/** The choices for a select, with the current value among them even when it was set to another
 * number by hand. */
export function pushGapChoices(current: number): { value: number; label: string }[] {
  const all: number[] = [...PUSH_GAPS];
  if (current > 0 && !all.includes(current)) all.push(current);
  return all.sort((a, b) => a - b).map((value) => ({ value, label: pushGapLabel(value) }));
}

/** The gap a host's info carries, read defensively: a hubd from before the field sends none,
 * which reads as no gap. */
export function pushGapOf(host: unknown): number {
  const mins = (host as { push_quiet_mins?: unknown } | null | undefined)?.push_quiet_mins;
  return typeof mins === "number" && mins > 0 ? mins : 0;
}
