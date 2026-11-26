// One way to run something every so often: `poll(fn, ms)` is `setInterval` that rests while the
// window is hidden (minimised, another desktop, a locked phone) and, the moment it is seen again,
// runs `fn` once at once and carries on. Returns what stops it. Use it for every refresh and
// clock tick in the app; a plain `setInterval` is only for something that must run unseen
// (the heartbeat in `live.svelte.ts`).
import { quiet } from "./quiet";

type Job = { fn: () => unknown; ms: number; timer: ReturnType<typeof setInterval> | undefined };

const jobs = new Set<Job>();
let listening = false;

const hidden = () => typeof document !== "undefined" && document.hidden;
const run = (j: Job) => {
  try {
    const r = j.fn();
    if (r instanceof Promise) quiet(r, "poll tick");
  } catch {
    // a failing tick must not stop the ones after it
  }
};
const arm = (j: Job) => {
  if (j.timer === undefined) j.timer = setInterval(() => run(j), j.ms);
};
const disarm = (j: Job) => {
  if (j.timer !== undefined) clearInterval(j.timer);
  j.timer = undefined;
};

function onVisibility() {
  for (const j of jobs) {
    if (hidden()) disarm(j);
    else if (j.timer === undefined) {
      run(j);
      arm(j);
    }
  }
}

export function poll(fn: () => unknown, ms: number): () => void {
  const job: Job = { fn, ms, timer: undefined };
  jobs.add(job);
  if (!listening && typeof document !== "undefined") {
    document.addEventListener("visibilitychange", onVisibility);
    listening = true;
  }
  if (!hidden()) arm(job);
  return () => {
    disarm(job);
    jobs.delete(job);
  };
}
