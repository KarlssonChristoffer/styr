// The pull request card shown when a PR is hovered or tapped in a chat (`PrCard.svelte`). The
// facts come from the hubd of the machine the project is on (`GET /api/github/card`, through its
// gh), and are kept a minute so moving over the same number again shows at once.

import { api } from "./api";
import { tooOld } from "./compat";
import type { PullCard } from "./bindings/PullCard";

export type CardKey = { path: string; repo: string; number: number; host: string | null };

const KEEP_MS = 60_000;
const seen = new Map<string, { at: number; card: PullCard }>();
const asked = new Map<string, Promise<PullCard>>();

const id = (k: CardKey) => `${k.host ?? ""}|${k.repo || k.path}|${k.number}`;

/** What is known without asking: the last card of this pull request, however old. */
export function lastCard(k: CardKey): PullCard | null {
  return seen.get(id(k))?.card ?? null;
}

/** The card, fresh enough or asked for. A card already being asked for is not asked twice. */
export function loadCard(k: CardKey): Promise<PullCard> {
  const key = id(k);
  const hit = seen.get(key);
  if (hit && Date.now() - hit.at < KEEP_MS) return Promise.resolve(hit.card);
  let pending = asked.get(key);
  if (!pending) {
    pending = api
      .pullCard(k.path, k.repo, k.number, k.host)
      .then((card) => {
        seen.set(key, { at: Date.now(), card });
        return card;
      })
      .finally(() => asked.delete(key));
    asked.set(key, pending);
  }
  return pending;
}

/** The hubd does not have the call yet (an older Hub on that machine). */
export const needsNewer = tooOld;
