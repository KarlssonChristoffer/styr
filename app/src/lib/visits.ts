// How often and how lately each thread was opened, so search ranks the threads you live in first.
const KEY = "hub.visits";
const DAY = 86_400_000;

type Visit = { n: number; at: number };

function load(): Record<string, Visit> {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}");
  } catch {
    return {};
  }
}

const visits = load();

export function visited(id: string) {
  const v = visits[id] ?? { n: 0, at: 0 };
  visits[id] = { n: Math.min(v.n + 1, 50), at: Date.now() };
  try {
    localStorage.setItem(KEY, JSON.stringify(visits));
  } catch {
    // Ranking is a convenience.
  }
}

/** Opens weighted by how recent they are: this week counts fully, a month ago a quarter. */
export function frecency(id: string, now = Date.now()): number {
  const v = visits[id];
  if (!v) return 0;
  const age = (now - v.at) / DAY;
  return v.n * (age < 1 ? 4 : age < 7 ? 2 : age < 30 ? 1 : 0.25);
}

// When you last left each thread (switched away, hid the window, closed Hub): everything after it
// is what "New" marks the next time you come back. Kept here so it survives a restart.
const LEFT_KEY = "hub.left";

function loadLeft(): Record<string, number> {
  try {
    return JSON.parse(localStorage.getItem(LEFT_KEY) ?? "{}");
  } catch {
    return {};
  }
}

const lefts = loadLeft();

/** You stopped looking at this thread now. */
export function left(id: string, at = Date.now()) {
  lefts[id] = at;
  const ids = Object.keys(lefts);
  if (ids.length > 300) for (const old of ids.sort((a, b) => lefts[a] - lefts[b]).slice(0, ids.length - 300)) delete lefts[old];
  try {
    localStorage.setItem(LEFT_KEY, JSON.stringify(lefts));
  } catch {
    // Without it "New" starts from when the thread was last opened, on any device.
  }
}

/** Where "New" starts when you open a thread: the later of when you left it here and when any
 *  device last saw it (`seen_at`), or none when neither is known. */
export function newSince(id: string, seenAt: number | null | undefined): number | undefined {
  const at = Math.max(lefts[id] ?? 0, seenAt ?? 0);
  return at > 0 ? at : undefined;
}
