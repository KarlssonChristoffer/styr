// How many thread logs the app keeps in memory. Opening a thread loads its whole log and
// reconnecting fetches the news of every log held, so without a limit a long session grows with
// every thread ever opened. `toDrop` says which to let go of: the least recently opened, never one
// that is open, working or asked for by a panel.

/** Logs kept beyond the ones that must stay. */
export const KEEP_LOGS = 8;

/**
 * `held` are the thread ids with a log, `recent` the ids in the order they were opened (oldest
 * first), `must` the ones that cannot go. Returns the ids to drop, oldest first.
 */
export function toDrop(held: string[], recent: string[], must: Set<string>, keep = KEEP_LOGS): string[] {
  const optional = held.filter((id) => !must.has(id));
  const over = optional.length - keep;
  if (over <= 0) return [];
  const rank = new Map(recent.map((id, i) => [id, i]));
  // Never opened (followed by a panel) counts as oldest.
  return optional.sort((a, b) => (rank.get(a) ?? -1) - (rank.get(b) ?? -1)).slice(0, over);
}
