// Arrow Up brings back what you sent, Ctrl S puts a prompt away: the Composer's history and stash,
// moved out of Composer.svelte. The box's own state (text, files, caret) stays in the Composer and
// reaches this through `Hooks`, so nothing here owns the draft.
import { THREAD_MIME } from "./attachments";
import type { Attachment } from "./bindings/Attachment";
import { putAway, stash, takeOut } from "./stash.svelte";
import type { Sent } from "./recent";

export type Hooks = {
  text: () => string;
  setText: (t: string) => void;
  attachments: () => Attachment[];
  setAttachments: (a: Attachment[]) => void;
  /** Adds one file to the box without replacing the ones there. */
  addAttachment: (a: Attachment) => void;
  box: () => HTMLTextAreaElement | undefined;
  /** Messages sent before, oldest first: the thread's own, or the recent ones outside a thread. */
  past: () => Sent[];
  clearPeek: () => void;
  clearOriginal: () => void;
  clearSuggestions: () => void;
  flash: (what: string) => void;
};

export class Recall {
  /** How far back Arrow Up went, 0 being the last message; -1 when not recalling. */
  at = -1;
  recalled: string | null = null;
  /** How many files came back with the recalled message; more or fewer means it was changed. */
  files = 0;

  constructor(private h: Hooks) {}

  /** The box was edited: once it no longer holds the recalled text, recalling starts over. */
  edited(text: string) {
    if (text !== this.recalled) this.at = -1;
  }

  step(e: KeyboardEvent): boolean {
    const h = this.h;
    if (e.shiftKey || e.ctrlKey || e.altKey || e.metaKey) return false;
    const past = h.past();
    if (!past.length) return false;
    const box = h.box()!;
    const text = h.text();
    const files = h.attachments();
    const caret = box.selectionStart;
    const ours = this.recalled !== null && text === this.recalled && files.length === this.files;
    if (e.key === "ArrowUp") {
      const atTop = !text.slice(0, caret).includes("\n");
      if (!((!text && !files.length) || (ours && atTop)) || this.at >= past.length - 1) return false;
      this.at++;
    } else {
      const atBottom = !text.slice(box.selectionEnd).includes("\n");
      if (!ours || !atBottom) return false;
      this.at--;
    }
    const back = this.at >= 0 ? past[past.length - 1 - this.at] : null;
    this.recalled = back?.text ?? null;
    h.setText(this.recalled ?? "");
    // The files it went with come back too; a thread named with @ is named again by the text.
    h.setAttachments((back?.attachments ?? []).filter((a) => a.mime !== THREAD_MIME).map((a) => ({ ...a })));
    this.files = h.attachments().length;
    h.clearPeek();
    queueMicrotask(() => (box.selectionStart = box.selectionEnd = e.key === "ArrowUp" ? 0 : h.text().length));
    return true;
  }
}

/** The put-away list: a tab on the box that opens a list above it, after T3 Code's stash. */
export class StashList {
  open = $state(false);
  at = $state(0);

  constructor(private h: Hooks) {}

  /** Ctrl S: close the list, put the draft away, or open the list when the box is empty. */
  key() {
    const h = this.h;
    if (this.open) {
      this.open = false;
    } else if (h.text().trim() || h.attachments().length) {
      putAway(h.text(), $state.snapshot(h.attachments()) as Attachment[]);
      h.setText("");
      h.setAttachments([]);
      h.clearPeek();
      h.clearOriginal();
      h.flash("Put away · Ctrl S in an empty box opens the list");
    } else if (stash.list.length) {
      this.show();
    } else {
      h.flash("Nothing put away · Ctrl S with text puts it away");
    }
  }

  restore(i: number) {
    const h = this.h;
    const s = takeOut(i);
    this.open = false;
    if (!s) return;
    h.setText(s.text);
    for (const a of s.attachments) h.addAttachment(a);
    h.clearSuggestions();
    queueMicrotask(() => {
      const box = h.box();
      box?.focus();
      if (box) box.selectionStart = box.selectionEnd = h.text().length;
    });
  }

  show() {
    if (!stash.list.length) return;
    this.h.clearSuggestions();
    this.at = 0;
    this.open = true;
    queueMicrotask(() => this.h.box()?.focus());
  }

  remove(i: number) {
    takeOut(i);
    if (!stash.list.length) this.open = false;
    else this.at = Math.min(this.at, stash.list.length - 1);
  }

  /** Keys while the list is open; true when one was used. */
  keys(e: KeyboardEvent): boolean {
    const n = stash.list.length;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      this.at = (this.at + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    } else if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey && !e.isComposing) {
      this.restore(this.at);
    } else if (e.key === "Delete") {
      this.remove(this.at);
    } else if (e.key === "Escape") {
      this.open = false;
    } else {
      // Anything else is typing: the list gets out of the way.
      if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) this.open = false;
      return false;
    }
    e.preventDefault();
    return true;
  }
}
