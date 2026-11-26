// Inspect mode: mark elements in Hub's own pages, write a note on each, group some, and start a
// thread that gets them with a screenshot (`Inspector.svelte`, `InspectOverlay.svelte`,
// `InspectCard.svelte`). The marks live in this window only; nothing is stored.
import { describe, inUI, type ElInfo } from "./inspectInfo";

export type Mark = {
  id: number;
  el: Element;
  /** What the element was when it was marked, for when it is gone from the page by the time it is sent. */
  info: ElInfo;
  note: string;
  /** "A", "B"…: marks that share a letter are one group. */
  group: string | null;
};

class Inspect {
  on = $state(false);
  /** The element under the pointer, or one a tree row is over. */
  hover = $state.raw<Element | null>(null);
  marks = $state.raw<Mark[]>([]);
  /** The note of each group, by letter. */
  groups = $state<Record<string, string>>({});
  /** The mark whose note box is open at its element. */
  editing = $state<number | null>(null);
  /** The thread card (screenshot, text, Start) is open. */
  sending = $state(false);
  /** A magnifier follows the pointer and shows the element under it larger. */
  magnify = $state(false);
  /** Where the pointer is, for the magnifier. */
  pointer = $state({ x: 0, y: 0 });
  /** Bumped now and then so the tree and the frames follow a page that changes. */
  rev = $state(0);
  private next = 1;
  /** The mark last clicked: a Shift-click joins it. */
  last: number | null = null;

  get focus(): Element | null {
    return this.hover ?? this.marks.find((m) => m.id === this.editing)?.el ?? null;
  }

  toggle() {
    this.on ? this.stop() : this.start();
  }
  start() {
    this.on = true;
  }
  stop() {
    this.on = false;
    this.hover = null;
    this.editing = null;
    this.sending = false;
    this.clear();
  }
  clear() {
    this.marks = [];
    this.groups = {};
    this.last = null;
  }

  markOf(el: Element): Mark | undefined {
    return this.marks.find((m) => m.el === el);
  }

  /** A click on an element: a first click marks it and opens its note, another opens the note again.
   *  With Shift it goes into a group with the mark clicked before it. */
  pick(el: Element, shift = false) {
    if (inUI(el)) return;
    let m = this.markOf(el);
    if (!m) {
      m = { id: this.next++, el, info: describe(el), note: "", group: null };
      this.marks = [...this.marks, m];
    }
    if (shift && this.last != null && this.last !== m.id) {
      const prev = this.marks.find((x) => x.id === this.last);
      if (prev) {
        const g = prev.group ?? this.freeLetter();
        this.groups[g] ??= "";
        this.setGroup(prev.id, g);
        this.setGroup(m.id, g);
      }
    }
    this.last = m.id;
    this.editing = m.id;
    this.rev++;
  }

  private freeLetter(): string {
    for (const l of "ABCDEFGHIJKLMNOPQRSTUVWXYZ") if (!(l in this.groups)) return l;
    return "Z";
  }
  setGroup(id: number, g: string | null) {
    this.marks = this.marks.map((m) => (m.id === id ? { ...m, group: g } : m));
  }
  setNote(id: number, note: string) {
    this.marks = this.marks.map((m) => (m.id === id ? { ...m, note } : m));
  }
  remove(id: number) {
    this.marks = this.marks.filter((m) => m.id !== id);
    if (this.editing === id) this.editing = null;
    if (this.last === id) this.last = null;
    // A group of one is not a group.
    const count: Record<string, number> = {};
    for (const m of this.marks) if (m.group) count[m.group] = (count[m.group] ?? 0) + 1;
    for (const m of this.marks) if (m.group && count[m.group] < 2) this.setGroup(m.id, null);
    for (const g of Object.keys(this.groups)) if ((count[g] ?? 0) < 2) delete this.groups[g];
  }
  /** The label shown on a mark: its number, with its group's letter when it has one. */
  tag(m: Mark): string {
    const n = this.marks.indexOf(m) + 1;
    return m.group ? `${m.group}${n}` : String(n);
  }
}

export const inspect = new Inspect();
