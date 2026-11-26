// Right-click menus. Anything can carry one with `use:menu={() => items}`; what is in it depends
// on what was clicked. One <ContextMenu /> in App draws whichever is open.

import { openUrl } from "./native";
import type { IconName } from "./components/Icon.svelte";

export type MenuItem = {
  label: string;
  /** What it does. Not needed for a heading or an item that only opens `children`. */
  run?: () => unknown;
  hint?: string;
  /** A small grey glyph before the label, so the kind of action reads at a glance. */
  icon?: IconName;
  danger?: boolean;
  /** Asks again in place before running, for things that cannot be undone. */
  confirm?: string;
  disabled?: boolean;
  /** A setting that is on or off: a tick after the label when on. */
  checked?: boolean;
  /** Opens a submenu beside the item (right arrow, hover or click). */
  children?: MenuEntry[];
  /** A small caps label above a group; it cannot be picked. */
  heading?: boolean;
};
/** null draws a divider; false, "" and undefined are left out, so items can be written as `cond && {…}`. */
export type MenuEntry = MenuItem | null | false | "" | undefined;
/** What the menu draws: entries with the empty ones taken out, submenus too. */
export type MenuNode = Omit<MenuItem, "children"> & { children?: (MenuNode | null)[] };

class MenuState {
  open = $state<{ x: number; y: number; items: (MenuNode | null)[]; from: Element | null } | null>(null);

  /** Where the last menu opened, so an item can open a follow-up menu in the same place. */
  last = { x: 0, y: 0 };

  show(x: number, y: number, entries: MenuEntry[]) {
    const items = tidy(entries);
    if (!items.length) return false;
    this.last = { x, y };
    this.open = { x, y, items, from: document.activeElement };
    return true;
  }

  close() {
    const from = this.open?.from;
    this.open = null;
    if (from instanceof HTMLElement && from.isConnected) from.focus({ preventScroll: true });
  }
}

export const contextMenu = new MenuState();

/** Drops empty entries and dividers at the ends or next to each other. */
function tidy(entries: MenuEntry[]): (MenuNode | null)[] {
  const out: (MenuNode | null)[] = [];
  for (const e of entries) {
    if (e === false || e === "" || e === undefined) continue;
    if (e === null && (!out.length || out.at(-1) === null)) continue;
    if (e === null) {
      out.push(null);
      continue;
    }
    const { children, ...rest } = e;
    if (!children) {
      out.push(rest);
      continue;
    }
    // A submenu with nothing in it is left out rather than shown as a dead end.
    const sub = tidy(children);
    if (sub.some((c) => c && !c.heading)) out.push({ ...rest, children: sub });
  }
  if (out.at(-1) === null) out.pop();
  return out;
}

/** Text fields keep the system menu: cut, copy, paste and spelling live there. */
export function editable(target: EventTarget | null) {
  return target instanceof Element && !!target.closest("input, textarea, select, [contenteditable=''], [contenteditable='true']");
}

export async function copy(text: string) {
  await navigator.clipboard.writeText(text);
}

function selected(within?: Element): string {
  const sel = window.getSelection();
  if (!sel || sel.isCollapsed) return "";
  if (within && !within.contains(sel.anchorNode)) return "";
  return sel.toString();
}

/** What every menu starts with: the link under the pointer and the text you selected. */
export function common(target: EventTarget | null, within?: Element): MenuEntry[] {
  const link = target instanceof Element ? target.closest<HTMLAnchorElement>("a[href]") : null;
  const href = link && /^https?:/i.test(link.href) ? link.href : null;
  const text = selected(within);
  return [
    href && { label: "Open link", run: () => openUrl(href) },
    href && { label: "Copy link", run: () => copy(href) },
    null,
    text && { label: "Copy", hint: "Ctrl C", run: () => copy(text) },
    null,
  ];
}

/** Shift with the right button keeps the WebView's own menu, for inspecting the page. */
const native = (e: MouseEvent) => e.shiftKey && e.button === 2;

/** Puts the menu where the pointer is, or under the element when opened from the keyboard. */
function at(e: MouseEvent, node: Element) {
  // The browser fallback zooms with CSS, which scales fixed positions too.
  const zoom = Number(document.documentElement.style.zoom) || 1;
  if (e.button === 2) return { x: e.clientX / zoom, y: e.clientY / zoom };
  const r = node.getBoundingClientRect();
  return { x: r.left / zoom + 12, y: r.bottom / zoom };
}

/** Opens a menu for the element on right-click, Shift F10 or the menu key. */
export function menu(node: HTMLElement, items: () => MenuEntry[]) {
  let get = items;
  const open = (e: MouseEvent) => {
    // An inner element already opened its own, or this is a text field.
    if (e.defaultPrevented || native(e) || editable(e.target)) return;
    const { x, y } = at(e, node);
    if (contextMenu.show(x, y, [...common(e.target, node), ...get()])) e.preventDefault();
  };
  node.addEventListener("contextmenu", open);
  return {
    update: (next: () => MenuEntry[]) => (get = next),
    destroy: () => node.removeEventListener("contextmenu", open),
  };
}

/** For anything without a menu of its own: links and selected text, and no browser menu. */
export function fallback(e: MouseEvent) {
  if (e.defaultPrevented || native(e) || editable(e.target)) return;
  e.preventDefault();
  const { x, y } = at(e, e.target instanceof Element ? e.target : document.body);
  contextMenu.show(x, y, common(e.target));
}
