// Ctrl F in a thread, after T3 Code's thread find: every place the words show in the conversation
// is marked with the CSS Custom Highlight API, so the page itself is not changed, and the current
// one is marked brighter and scrolled to. The transcript opens folded turns that hold a match.

/** Every place `query` shows in the text under `root`, case insensitive, in reading order. */
export function findRanges(root: HTMLElement, query: string): Range[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return [];
  const ranges: Range[] = [];
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => {
      const el = n.parentElement;
      // Text that is not on screen: closed details, hidden blocks, the find box itself.
      if (!el || el.closest("input, textarea, script, style, [data-find-skip]")) return NodeFilter.FILTER_REJECT;
      if (!el.checkVisibility?.() && el.offsetParent === null) return NodeFilter.FILTER_REJECT;
      return NodeFilter.FILTER_ACCEPT;
    },
  });
  for (let node = walk.nextNode(); node; node = walk.nextNode()) {
    const text = (node.nodeValue ?? "").toLowerCase();
    for (let at = text.indexOf(needle); at >= 0; at = text.indexOf(needle, at + needle.length)) {
      const r = document.createRange();
      r.setStart(node, at);
      r.setEnd(node, at + needle.length);
      ranges.push(r);
    }
  }
  return ranges;
}

type Highlights = { set(name: string, h: unknown): void; delete(name: string): void };
const registry = (): Highlights | undefined => (globalThis.CSS as unknown as { highlights?: Highlights })?.highlights;
const Highlight = (globalThis as unknown as { Highlight?: new (...r: Range[]) => unknown }).Highlight;

/** Marks all matches, and `current` brighter; nothing at all clears them. */
export function mark(ranges: Range[], current: number) {
  const reg = registry();
  if (!reg || !Highlight) return;
  if (!ranges.length) {
    reg.delete("find");
    reg.delete("find-current");
    return;
  }
  reg.set("find", new Highlight(...ranges.filter((_, i) => i !== current)));
  if (ranges[current]) reg.set("find-current", new Highlight(ranges[current]));
  else reg.delete("find-current");
}

/** Scrolls `scroller` so `range` sits a third of the way down, unless it is already in view. */
export function reveal(scroller: HTMLElement, range: Range) {
  const r = range.getBoundingClientRect();
  const box = scroller.getBoundingClientRect();
  if (r.top >= box.top + 40 && r.bottom <= box.bottom - 40) return;
  scroller.scrollTop += r.top - box.top - box.height / 3;
}
