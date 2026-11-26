/**
 * Closes a tab when it is clicked with the middle button (the scroll wheel), like in a browser.
 * The press itself is cancelled, so the page does not start autoscroll (Windows) or paste the
 * selection (Linux) while the tab goes away.
 */
export function middleClose(node: HTMLElement, close: () => void) {
  let current = close;
  const down = (e: MouseEvent) => {
    if (e.button === 1) e.preventDefault();
  };
  const up = (e: MouseEvent) => {
    if (e.button !== 1) return;
    e.preventDefault();
    e.stopPropagation();
    current();
  };
  node.addEventListener("mousedown", down);
  node.addEventListener("auxclick", up);
  return {
    update(next: () => void) {
      current = next;
    },
    destroy() {
      node.removeEventListener("mousedown", down);
      node.removeEventListener("auxclick", up);
    },
  };
}
