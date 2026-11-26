// Hover tips. Anything with `data-tip` gets Hub's own tip instead of the system's yellow box: dark,
// after a short rest, gone the moment you click, scroll or type. Once one has shown, the next
// shows at once while you move along a row of buttons, like the system's own. A plain `title`
// (Markdown links, anything added later) is moved to `data-tip` on first hover.
//
// A key combination at the end in brackets, "Search (Ctrl K)", is drawn as keys. Or name them
// with `data-keys="Ctrl K"`. Lines break where the text has a newline.
//
// The tip is ordinary page text, so it grows with Text size and zoom like everything else, and it
// is placed inside the window, below the thing or above it when there is no room.
import { poll } from "./poll";

const DELAY = 450;
/** After a tip closes, the next one within this long shows without the delay. */
const WARM = 500;
const GAP = 6;
const EDGE = 6;

const KEY = /^(Ctrl|Alt|Shift|Cmd|Win|Esc|Enter|Tab|Space|Backspace|Delete|Home|End|PgUp|PgDn|F\d{1,2}|[↑↓←→]|[A-Z0-9§.,;/\\[\]=+-])$/;

/** "Search (Ctrl K)" → the text and the keys; anything else is all text. */
export function splitKeys(tip: string): { text: string; keys: string[] } {
  const m = tip.match(/^([\s\S]*?)\s+\(([^()]+)\)$/);
  if (!m) return { text: tip, keys: [] };
  const keys = m[2].trim().split(/(?<=\S)[ +](?=\S)/);
  return keys.every((k) => KEY.test(k)) ? { text: m[1], keys } : { text: tip, keys: [] };
}

const inTauri = "__TAURI_INTERNALS__" in window;

export function installTooltips(): () => void {
  const tip = document.createElement("div");
  tip.className = "hub-tip";
  tip.id = "hub-tip";
  tip.setAttribute("role", "tooltip");
  document.body.append(tip);

  let target: HTMLElement | null = null;
  let shown = false;
  let timer = 0;
  let closedAt = 0;
  /** Where the pointer rested, for things too big to point at as a whole. */
  let pointer: { x: number; y: number } | null = null;
  /** Clicked: no tip on this one again until the pointer leaves it. */
  let pressed: HTMLElement | null = null;

  const watch = new MutationObserver(() => {
    if (!target) return;
    adopt(target);
    if (shown) target.dataset.tip ? draw() : hide();
  });

  /** Moves a system `title` over, so the system box never shows. */
  function adopt(el: HTMLElement) {
    const title = el.getAttribute("title");
    if (title == null) return;
    el.removeAttribute("title");
    if (title) el.dataset.tip = title;
    else delete el.dataset.tip;
    name(el);
  }

  /** An icon button's tip is its name for screen readers too, as its title was. */
  function name(el: HTMLElement) {
    const tip = el.dataset.tip;
    if (el.hasAttribute("aria-label") && !el.hasAttribute("data-tip-named")) return;
    if (tip && !el.textContent?.trim()) {
      el.setAttribute("aria-label", tip);
      el.setAttribute("data-tip-named", "");
    } else if (el.hasAttribute("data-tip-named")) {
      el.removeAttribute("aria-label");
      el.removeAttribute("data-tip-named");
    }
  }
  const names = new MutationObserver((records) => {
    for (const r of records) {
      if (r.type === "attributes") name(r.target as HTMLElement);
      else for (const n of r.addedNodes) if (n instanceof HTMLElement) [n, ...n.querySelectorAll<HTMLElement>("[data-tip]")].forEach((el) => el.dataset.tip && name(el));
    }
  });
  names.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ["data-tip"] });
  document.querySelectorAll<HTMLElement>("[data-tip]").forEach(name);

  function owner(node: EventTarget | null): HTMLElement | null {
    if (!(node instanceof Element)) return null;
    const el = node.closest<HTMLElement>("[data-tip], [title]");
    if (!el || el instanceof HTMLIFrameElement || !(el instanceof HTMLElement)) return null;
    adopt(el);
    return el.dataset.tip ? el : null;
  }

  /** A tip that only repeats what the thing already shows in full says nothing. */
  function repeats(el: HTMLElement) {
    if (el.dataset.tip?.trim() !== el.textContent?.trim()) return false;
    return ![el, ...el.querySelectorAll<HTMLElement>("*")].some((n) => n.scrollWidth > n.clientWidth + 1 || n.scrollHeight > n.clientHeight + 1);
  }

  function draw() {
    if (!target) return;
    const { text, keys } = target.dataset.keys
      ? { text: target.dataset.tip!, keys: target.dataset.keys.split(/\s+/) }
      : splitKeys(target.dataset.tip!);
    tip.replaceChildren(Object.assign(document.createElement("span"), { className: "hub-tip-text", textContent: text }));
    if (keys.length) {
      const row = Object.assign(document.createElement("span"), { className: "hub-tip-keys" });
      for (const k of keys) row.append(Object.assign(document.createElement("kbd"), { textContent: k }));
      tip.append(row);
    }
    place();
  }

  function place() {
    if (!target) return;
    // In the browser the zoom is CSS on <html>; boxes are measured zoomed and placed unzoomed.
    const zoom = inTauri ? 1 : Number(document.documentElement.style.zoom) || 1;
    const vw = window.innerWidth / zoom, vh = window.innerHeight / zoom;
    tip.style.left = "0px";
    tip.style.top = "0px";
    const r = target.getBoundingClientRect();
    let box = { left: r.left / zoom, right: r.right / zoom, top: r.top / zoom, bottom: r.bottom / zoom };
    const t = tip.getBoundingClientRect();
    const w = t.width / zoom, h = t.height / zoom;
    // Rows and panels are pointed at where the pointer rests, not at their middle.
    if (pointer && (box.right - box.left > Math.max(2 * w, 320) || box.bottom - box.top > 64)) {
      const x = pointer.x / zoom, y = pointer.y / zoom;
      box = { left: x, right: x, top: y - 4, bottom: y + 18 };
    }
    const below = box.bottom + GAP + h <= vh - EDGE || box.top - GAP - h < EDGE;
    const top = below ? Math.min(box.bottom + GAP, vh - EDGE - h) : box.top - GAP - h;
    const left = Math.min(Math.max(EDGE, (box.left + box.right) / 2 - w / 2), vw - EDGE - w);
    tip.style.left = `${Math.round(left)}px`;
    tip.style.top = `${Math.round(Math.max(EDGE, top))}px`;
    tip.dataset.side = below ? "below" : "above";
  }

  function show() {
    if (!target?.isConnected || repeats(target)) return;
    shown = true;
    draw();
    tip.classList.add("on");
    if (target.getAttribute("aria-label") !== target.dataset.tip) target.setAttribute("aria-describedby", tip.id);
  }

  function hide() {
    clearTimeout(timer);
    if (shown) closedAt = performance.now();
    shown = false;
    tip.classList.remove("on");
    if (target?.getAttribute("aria-describedby") === tip.id) target.removeAttribute("aria-describedby");
    watch.disconnect();
    target = null;
  }

  function aim(el: HTMLElement, at: { x: number; y: number } | null) {
    if (el === target) return;
    hide();
    target = el;
    pointer = at;
    watch.observe(el, { attributes: true, attributeFilter: ["data-tip", "data-keys", "title"] });
    const warm = performance.now() - closedAt < WARM;
    timer = window.setTimeout(show, warm ? 0 : DELAY);
  }

  const over = (e: PointerEvent) => {
    if (e.pointerType === "touch") return;
    const el = owner(e.target);
    if (pressed && pressed !== el) pressed = null;
    if (!el) return target && hide();
    if (el !== pressed) aim(el, { x: e.clientX, y: e.clientY });
  };
  const move = (e: PointerEvent) => {
    // Until it shows, the tip follows where the pointer comes to rest.
    if (target && !shown) pointer = { x: e.clientX, y: e.clientY };
  };
  const out = (e: PointerEvent) => {
    if (!target || (e.relatedTarget instanceof Node && target.contains(e.relatedTarget))) return;
    if (!owner(e.relatedTarget)) hide();
  };
  const down = (e: Event) => {
    pressed = owner(e.target);
    hide();
    closedAt = 0;
  };
  const focus = (e: FocusEvent) => {
    const el = owner(e.target);
    if (el && el === e.target && el.matches(":focus-visible")) aim(el, null);
  };
  const blur = (e: FocusEvent) => {
    if (e.target === target) hide();
  };
  const key = (e: KeyboardEvent) => {
    if (target && !["Shift", "Control", "Alt", "Meta"].includes(e.key)) hide();
  };
  const away = () => target && hide();
  // A thing that goes away under the pointer takes its tip with it.
  const gone = poll(() => target && !target.isConnected && hide(), 300);

  document.addEventListener("pointerover", over, true);
  document.addEventListener("pointermove", move, { capture: true, passive: true });
  document.addEventListener("pointerout", out, true);
  document.addEventListener("pointerdown", down, true);
  document.addEventListener("focusin", focus, true);
  document.addEventListener("focusout", blur, true);
  document.addEventListener("keydown", key, true);
  document.addEventListener("scroll", away, { capture: true, passive: true });
  document.addEventListener("wheel", away, { capture: true, passive: true });
  window.addEventListener("blur", away);
  window.addEventListener("resize", away);
  window.addEventListener("hub-zoom", away);
  return () => {
    hide();
    names.disconnect();
    gone();
    tip.remove();
    document.removeEventListener("pointerover", over, true);
    document.removeEventListener("pointermove", move, true);
    document.removeEventListener("pointerout", out, true);
    document.removeEventListener("pointerdown", down, true);
    document.removeEventListener("focusin", focus, true);
    document.removeEventListener("focusout", blur, true);
    document.removeEventListener("keydown", key, true);
    document.removeEventListener("scroll", away, true);
    document.removeEventListener("wheel", away, true);
    window.removeEventListener("blur", away);
    window.removeEventListener("resize", away);
    window.removeEventListener("hub-zoom", away);
  };
}
