// "While you were away": Hub notes when you last used it, and after a long enough gap Home
// shows what changed meanwhile, until you close it.
const KEY = "hub.lastActive";
/** A gap shorter than this is a coffee, not being away. */
const AWAY_MS = 20 * 60 * 1000;

function stored(): number {
  try {
    return Number(localStorage.getItem(KEY)) || 0;
  } catch {
    return 0;
  }
}

class Away {
  /** When you left, while Home should tell you what happened since; null otherwise. */
  since = $state<number | null>(null);
  private last = stored();

  constructor() {
    if (this.last && Date.now() - this.last > AWAY_MS) this.since = this.last;
    this.mark(Date.now());
    const seen = () => this.seen();
    // Only your hands count: a window left open on an empty desk is not you being here.
    for (const e of ["pointermove", "pointerdown", "keydown", "wheel"]) addEventListener(e, seen, { passive: true });
  }

  private seen() {
    const now = Date.now();
    if (now - this.last < 30_000) return;
    if (now - this.last > AWAY_MS && this.since === null) this.since = this.last;
    this.mark(now);
  }

  private mark(now: number) {
    this.last = now;
    try {
      localStorage.setItem(KEY, String(now));
    } catch {}
  }

  dismiss() {
    this.since = null;
  }
}

export const away = new Away();
