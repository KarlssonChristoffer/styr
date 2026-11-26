// Keeps dictated words in the message box while they are still being heard. Each phrase owns a
// stretch of the text: its live guess is written there and redone as the speaker goes on, and its
// final text replaces it in place. Typing elsewhere in the box moves the stretches along; typing
// inside one hands it to the user, so a late result never overwrites what they wrote.

export type Box = { text: string; caret: number };

type Piece = { id: number; at: number; len: number };

/** Names whisper writes with a capital in the middle of a sentence. */
const NAMES = /^(Hub|Claude|GitHub|Git|Windows|Svelte|Rust|Tauri|README|API|Opus|Sonnet|Haiku|Codex|I)\b/;

export class Dictation {
  private pieces: Piece[] = [];
  /** The text as this class last saw it, to tell the user's edits from its own. */
  private seen = "";
  /** Where the last final phrase ended, so the next one can mend the sentence across a pause. */
  private finalTo = -1;
  private box: Box;

  constructor(box: Box) {
    this.box = box;
    this.seen = box.text;
  }

  /** Starts a stretch for a phrase at the caret. */
  open(id: number) {
    this.follow();
    if (!this.pieces.some((p) => p.id === id)) this.pieces.push({ id, at: this.box.caret, len: 0 });
  }

  /** Whether a phrase still owns its stretch (the user has not typed into it). */
  has(id: number) {
    this.follow();
    return this.pieces.some((p) => p.id === id);
  }

  /** The text before a phrase, for whisper's prompt. */
  before(id: number) {
    this.follow();
    const p = this.pieces.find((p) => p.id === id);
    return this.box.text.slice(0, p ? p.at : this.box.caret);
  }

  /** Writes the live guess for a phrase. */
  live(id: number, said: string) {
    this.follow();
    const p = this.pieces.find((p) => p.id === id);
    if (!p) return;
    const before = this.box.text.slice(0, p.at);
    said = said.trim();
    const end = before.trimEnd();
    if (said && (!end || /[.!?]$/.test(end))) said = cap(said);
    this.write(p, p.at, said ? lead(before) + said + trail(this.after(p)) : "");
  }

  /** Writes a phrase's final text and lets go of its stretch. Empty text removes the guess. */
  final(id: number, said: string) {
    this.follow();
    const p = this.pieces.find((p) => p.id === id);
    if (!p) return;
    said = said.trim();
    const before = this.box.text.slice(0, p.at);
    if (!said) {
      this.write(p, p.at, "");
    } else {
      const [mended, words] = punctuate(before, said, p.at === this.finalTo);
      // The seam may change the end of the text before (a full stop added or taken off).
      let cut = 0;
      while (cut < before.length && cut < mended.length && before[cut] === mended[cut]) cut++;
      const piece = mended.slice(cut) + lead(mended) + words + trail(this.after(p));
      this.write(p, cut, piece);
      this.finalTo = cut + piece.length - trail(this.after(p)).length;
    }
    this.pieces = this.pieces.filter((q) => q !== p);
  }

  /** The stretches still being heard or waiting for their final text, as [from, to) in the text. */
  ranges(): [number, number][] {
    this.follow();
    return this.pieces.filter((p) => p.len > 0).map((p) => [p.at, p.at + p.len]);
  }

  /** Lets go of every stretch, leaving their text as it is. */
  release() {
    this.follow();
    this.pieces = [];
  }

  private after(p: Piece) {
    return this.box.text.slice(p.at + p.len);
  }

  /** Replaces from `from` to the end of `p` with `insert`, keeping the caret and later pieces in place. */
  private write(p: Piece, from: number, insert: string) {
    const to = p.at + p.len;
    const old = this.box.text;
    const delta = insert.length - (to - from);
    this.box.text = old.slice(0, from) + insert + old.slice(to);
    const caret = this.box.caret;
    if (caret >= to) this.box.caret = caret + delta;
    else if (caret > from) this.box.caret = from + insert.length;
    for (const q of this.pieces) if (q !== p && q.at >= to) q.at += delta;
    p.at = from;
    p.len = insert.length;
    this.seen = this.box.text;
  }

  /** Accounts for edits made by anyone else since this class last wrote. */
  private follow() {
    const now = this.box.text;
    const was = this.seen;
    if (now === was) return;
    this.seen = now;
    let start = 0;
    const most = Math.min(now.length, was.length);
    while (start < most && now[start] === was[start]) start++;
    let tail = 0;
    while (tail < most - start && now[now.length - 1 - tail] === was[was.length - 1 - tail]) tail++;
    const oldEnd = was.length - tail;
    const delta = now.length - was.length;
    this.pieces = this.pieces.filter((p) => {
      if (p.len === 0) {
        // Nothing heard yet: it just goes where the caret's edit leaves it.
        if (p.at >= oldEnd) p.at += delta;
        else if (p.at > start) p.at = now.length - tail;
        return true;
      }
      if (p.at + p.len <= start) return true;
      if (p.at >= oldEnd) {
        p.at += delta;
        return true;
      }
      // Typed into, or the box was cleared: the text there is the user's now.
      return false;
    });
    if (this.finalTo > start) this.finalTo = -1;
  }
}

function cap(s: string) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function lead(before: string) {
  return before && !/\s$/.test(before) ? " " : "";
}

function trail(after: string) {
  return after && !/^\s/.test(after) ? " " : "";
}

/**
 * Whisper hears each phrase on its own, so the seam between two is guessed from how the new one
 * starts: a capital begins a sentence, so the one before gets its full stop; a small letter
 * goes on with it, so a full stop whisper put at the pause comes off again.
 */
export function punctuate(before: string, said: string, ourLast: boolean): [string, string] {
  const end = before.trimEnd();
  if (!end) return [before, cap(said)];
  const upper = /^\p{Lu}/u.test(said) && !NAMES.test(said);
  const lower = /^\p{Ll}/u.test(said);
  if (/[.!?:]$/.test(end)) {
    if (lower && ourLast && /[^.]\.$/.test(end)) return [end.slice(0, -1) + before.slice(end.length), said];
    if (lower && /[.!?]$/.test(end)) return [before, cap(said)];
    return [before, said];
  }
  if (upper && /[\p{L}\p{N})"]$/u.test(end)) return [end + "." + before.slice(end.length), said];
  return [before, said];
}

/**
 * Keeps the start of a phrase's live guess still. Whisper hears the whole phrase again for every
 * guess and may word its start differently each time, which made the text jump. Words two guesses
 * in a row agree on are locked (as whisper_streaming's LocalAgreement does), and later guesses
 * only change what comes after them. The phrase's final text still replaces it all.
 */
export class Steady {
  private locked: string[] = [];
  private last: string[] = [];

  /** The text to show for a new guess. */
  next(guess: string): string {
    const words = guess.trim().split(/\s+/).filter(Boolean);
    let n = 0;
    while (n < words.length && n < this.last.length && same(words[n], this.last[n])) n++;
    if (n > this.locked.length) this.locked = words.slice(0, n);
    this.last = words;
    const k = this.locked.length;
    return [...this.locked, ...words.slice(k)].join(" ");
  }
}

function same(a: string, b: string) {
  const bare = (w: string) => w.toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");
  return bare(a) === bare(b);
}
