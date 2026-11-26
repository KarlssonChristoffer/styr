// Entry point: mounts App.svelte and hooks the window-level error reporting.
import { quiet } from "./lib/quiet";
import { arrive, leaveBroken, showingLive } from "./lib/carry";
import { ownToken } from "./lib/home";

// What the page before this one handed over (Live mode) goes into storage before the app reads
// it, so the app starts in boot.ts, once that is done. Live pages that cannot even start send the
// window back to the installed ones (`lib/live.svelte.ts`), but not at the first failure: while
// dev mode refreshes its clone (a merged pull request, a new push) Vite serves files that are
// being rewritten, so a module can fail once and load a moment later. The page tries again a few
// times, a couple of seconds apart, before it gives up.
const TRIES = 4;
const TRIES_KEY = "hub.liveStartTries";

const tried = (): number => {
  try {
    return Number(sessionStorage.getItem(TRIES_KEY)) || 0;
  } catch {
    return TRIES;
  }
};

arrive()
  .finally(() => ownToken())
  .finally(() => import("./boot"))
  .then(() => {
    try {
      sessionStorage.removeItem(TRIES_KEY);
    } catch {
      // Nothing to clear without storage.
    }
  })
  .catch((e) => {
    if (showingLive) {
      const n = tried();
      if (n < TRIES) {
        let counted = false;
        try {
          sessionStorage.setItem(TRIES_KEY, String(n + 1));
          counted = true;
        } catch {
          // Without storage it cannot count, so it would reload for ever; it goes back instead.
        }
        if (counted) setTimeout(() => location.reload(), 2000);
        else quiet(leaveBroken(`The live pages did not start: ${e instanceof Error ? e.message : e}`), "leave broken");
      } else {
        try {
          sessionStorage.removeItem(TRIES_KEY);
        } catch {
          // As above.
        }
        quiet(leaveBroken(`The live pages did not start: ${e instanceof Error ? e.message : e}`), "leave broken");
      }
    }
    throw e;
  });
