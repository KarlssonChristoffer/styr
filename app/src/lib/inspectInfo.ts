// What Inspect mode tells about an element on screen: which component draws it (the build puts
// `data-c="lib/components/Rail.svelte"` on every component's root elements, `svelte.config.js`),
// where it sits in the tree, what it says and a few styles. Plain functions on the DOM, no state.

/** Marks Inspect's own pieces (panel, note box, thread card, the overlay), which it never picks. */
export const UI = "[data-inspect-ui]";

export type ElInfo = {
  /** The component's file, relative to app/src, or "" for an element outside any component. */
  file: string;
  /** "ThreadRow", from the file name. */
  name: string;
  tag: string;
  /** `button.take-over`, without Svelte's generated class names. */
  short: string;
  /** `header.thread-head > button.take-over`: the last few steps up to a component's root. */
  path: string;
  /** The nearest components around it, innermost first. */
  chain: string[];
  text: string;
  rect: { x: number; y: number; w: number; h: number };
  styles: Record<string, string>;
};

export const inUI = (el: Element | null) => !!el?.closest?.(UI);

/** The component file of the nearest root element at or above `el`. */
export function fileOf(el: Element | null): string {
  return el?.closest?.("[data-c]")?.getAttribute("data-c") ?? "";
}

export function nameOf(file: string): string {
  return file ? file.replace(/^.*\//, "").replace(/\.svelte$/, "") : "";
}

const classes = (el: Element) =>
  [...el.classList].filter((c) => !/^svelte-/.test(c)).slice(0, 3);

/** `button.take-over`, or `div#id.a.b`. */
export function shortOf(el: Element): string {
  const id = el.id ? `#${el.id}` : "";
  const c = classes(el);
  return `${el.tagName.toLowerCase()}${id}${c.length ? "." + c.join(".") : ""}`;
}

/** The step from a root up: `header.thread-head > button.take-over`, at most four steps. */
export function pathOf(el: Element): string {
  const steps: string[] = [];
  for (let e: Element | null = el; e && e !== document.body && steps.length < 4; e = e.parentElement) {
    steps.unshift(shortOf(e));
    if (e !== el && e.hasAttribute("data-c")) break;
  }
  return steps.join(" > ");
}

/** Up to `n` different components around the element, innermost first. */
export function chainOf(el: Element, n = 4): string[] {
  const out: string[] = [];
  for (let e = el.closest("[data-c]"); e && out.length < n; e = e.parentElement?.closest("[data-c]") ?? null) {
    const name = nameOf(e.getAttribute("data-c") ?? "");
    if (name && out[out.length - 1] !== name) out.push(name);
  }
  return out;
}

export function labelOf(el: Element): string {
  const r = el.getBoundingClientRect();
  const name = nameOf(fileOf(el));
  return `${name || shortOf(el)} · ${Math.round(r.width)} × ${Math.round(r.height)}`;
}

const STYLES = [
  "display", "position", "padding", "margin", "gap", "font-size", "font-weight", "line-height", "color",
  "background-color", "border", "border-radius", "opacity", "overflow", "z-index",
];
const SKIP = new Set(["", "none", "normal", "auto", "static", "visible", "0px", "rgba(0, 0, 0, 0)", "0px none rgb(0, 0, 0)"]);

/** Computed styles worth reading, leaving out what is only the browser's default. */
export function stylesOf(el: Element): Record<string, string> {
  const cs = getComputedStyle(el);
  const out: Record<string, string> = {};
  for (const p of STYLES) {
    const v = cs.getPropertyValue(p).trim();
    if (SKIP.has(v)) continue;
    if (p === "display" && v === "block") continue;
    out[p] = v;
  }
  return out;
}

export function describe(el: Element): ElInfo {
  const r = el.getBoundingClientRect();
  const file = fileOf(el);
  return {
    file,
    name: nameOf(file),
    tag: el.tagName.toLowerCase(),
    short: shortOf(el),
    path: pathOf(el),
    chain: chainOf(el),
    text: (el.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 100),
    rect: { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) },
    styles: stylesOf(el),
  };
}
