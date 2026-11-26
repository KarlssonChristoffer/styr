/** Moves an element to the end of the page, out of any parent that clips or masks it. */
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return { destroy: () => node.remove() };
}
