// A new version of Hub is on probation until it has shown it is healthy, and when it breaks in
// that time Hub goes back to the last working one (D7, `hub_install::probation`). The core tells
// the window in the update status; these are the optional fields it adds, read here so an older
// hubd without them simply shows nothing. The window confirms with `POST /api/update/alive`
// (feature `update.alive`) once the new pages have rendered.

import { hubdHas } from "./compat";
import type { UpdateStatus } from "./bindings/UpdateStatus";

/** The running version is on probation. */
export type Trial = {
  /** What Hub goes back to when the version fails; null when there is nothing to go back to. */
  previous?: string | null;
  /** When the version counts as healthy, ms since the epoch. */
  ends_at?: number | null;
};

/** The last time a version was rolled back (`rolled-back.json`), shown until it is dismissed. */
export type WentBack = {
  from: string;
  to: string;
  reason: string;
  at_ms: number;
  /** The database as it was when the version failed, kept because the older one could not open it. */
  db_set_aside?: string | null;
};

type Extra = { probation?: Trial | null; rolled_back?: WentBack | null };

export const trial = (u: UpdateStatus | null): Trial | null => (u as (UpdateStatus & Extra) | null)?.probation ?? null;
export const wentBack = (u: UpdateStatus | null): WentBack | null => (u as (UpdateStatus & Extra) | null)?.rolled_back ?? null;

let pinged = "";

/** Tells the core the pages of the version on probation have rendered. Once per version; a core
 * without `update.alive` is not asked. */
export async function confirmPages(u: UpdateStatus | null, send: (version: string) => Promise<unknown>): Promise<void> {
  if (!u || u.version === "dev" || !trial(u) || pinged === u.version) return;
  pinged = u.version;
  const forget = () => {
    pinged = "";
  };
  if (!(await hubdHas("update.alive"))) return forget(); // compat-until: 2026-12-06
  await send(u.version).catch(forget);
}
