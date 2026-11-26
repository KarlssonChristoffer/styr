// Hub's versions differ: the window may be newer than the hubd it uses (a window on another
// machine's Hub), or older. Core things always work; a brand-new feature may be missing on the
// other side and is then hidden or disabled where it would be used. The rules are in
// docs/ARCHITECTURE.md, "Versions that talk to each other".

import { BASE } from "./home";

/** A failed call, with the status, so a caller can tell "this hubd lacks it" from a real error. */
export class ApiError extends Error {
  constructor(message: string, readonly status: number) {
    super(message);
    this.name = "ApiError";
  }
}

/** The hubd answered 404 or 405: an older hubd has no such call. It is not a failure to show, only
 * a feature that is not there. A 403 is not this: the hubd knows the call and refuses this client
 * (a phone asking for more than it may), which is a real error. To ask whether a feature exists,
 * use `hubdHas(feature)` before calling instead of guessing from the failure. */
export function tooOld(err: unknown): boolean {
  return err instanceof ApiError && (err.status === 404 || err.status === 405);
}

let listed: Promise<Set<string> | null> | null = null;

/** The features the hubd this window uses lists in `/api/health`. An empty set is a hubd from
 * before the list (core only); null is "could not ask", which callers treat as present so a
 * blip never hides anything. */
export function hubdFeatures(): Promise<Set<string> | null> {
  return (listed ??= fetch(`${BASE}/api/health`, { signal: AbortSignal.timeout(10_000) })
    .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
    .then((h: { features?: unknown }) => new Set<string>(Array.isArray(h.features) ? h.features.filter((f): f is string => typeof f === "string") : []))
    .catch(() => {
      listed = null;
      return null;
    }));
}

/** Whether the hubd has a feature (`compat::FEATURES` in the core). */
export async function hubdHas(feature: string): Promise<boolean> {
  const features = await hubdFeatures();
  return features === null || features.has(feature);
}
