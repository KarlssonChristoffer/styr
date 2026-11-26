// Reactions on messages (docs/ARCHITECTURE.md, "Reactions"): what each message carries, the
// user's own emojis and the quick row, and the one picker a thread shows at a time.

import { api } from "./api";
import { hubdHas } from "./compat";
import { hub } from "./state.svelte";
import { payload } from "./events";
import type { CustomEmoji } from "./bindings/CustomEmoji";
import type { Event } from "./bindings/Event";

/** One emoji on one message: who put it there, so the chip can say whether it is yours. */
export type Chip = { emoji: string; you: boolean; agent: boolean; count: number };

/** What every message carries, by the seq of the message (a user message, or the turn end of an
 * agent reply). The latest `reaction` event per seq, emoji and who is the state; chips keep the
 * order the emojis were first put on. */
export function reactionsOf(events: Event[]): Map<number, Chip[]> {
  const cached = memo.get(events);
  if (cached) return cached;
  const by = new Map<number, Map<string, { you: boolean; agent: boolean }>>();
  for (const e of events) {
    const d = payload(e, "reaction");
    if (!d) continue;
    if (typeof d.seq !== "number" || !d.emoji) continue;
    let chips = by.get(d.seq);
    if (!chips) by.set(d.seq, (chips = new Map()));
    let c = chips.get(d.emoji);
    if (!c) chips.set(d.emoji, (c = { you: false, agent: false }));
    const on = d.on !== false;
    if (d.by === "agent") c.agent = on;
    else if (d.by === "you") c.you = on;
  }
  const out = new Map<number, Chip[]>();
  for (const [seq, chips] of by) {
    const list = [...chips].filter(([, c]) => c.you || c.agent).map(([emoji, c]) => ({ emoji, ...c, count: Number(c.you) + Number(c.agent) }));
    if (list.length) out.set(seq, list);
  }
  memo.set(events, out);
  return out;
}
const memo = new WeakMap<Event[], Map<number, Chip[]>>();

const STARTERS = ["👍", "❤️", "🎉", "👀", "🚀", "🙏"];

class Emojis {
  /** The hubd has reactions (`chat.reactions`); until it does, nothing is drawn or offered. */
  on = $state(false);
  custom = $state<Record<string, CustomEmoji>>({});
  /** The ones used most, most used first. */
  quick = $state<string[]>(STARTERS);
  #asked: Promise<void> | null = null;

  /** Asks the hubd once what it has. */
  ensure(): Promise<void> {
    return (this.#asked ??= (async () => {
      try {
        this.on = await hubdHas("chat.reactions"); // compat-until: 2026-12-03
        if (this.on) await this.reload();
      } catch {
        this.#asked = null;
      }
    })());
  }

  async reload() {
    const s = await api.emoji();
    this.custom = Object.fromEntries(s.custom.map((c) => [c.name, c]));
    if (s.quick.length) this.quick = s.quick;
  }

  /** Counts a use the way the core does, so the quick row moves before the next reload. */
  used(emoji: string) {
    this.quick = [emoji, ...this.quick.filter((q) => q !== emoji)].slice(0, 7);
  }

  /** The picture of a `:name:` emoji, or null when it is plain Unicode or not known here. */
  src(emoji: string): string | null {
    const m = /^:([a-z0-9_-]+):$/.exec(emoji);
    const c = m && this.custom[m[1]];
    return c ? `data:${c.mime};base64,${c.data}` : null;
  }

  async add(name: string, mime: string, data: string) {
    await api.putEmoji(name, mime, data);
    this.custom = { ...this.custom, [name]: { name, mime, data } };
  }

  async remove(name: string) {
    await api.deleteEmoji(name);
    const { [name]: _gone, ...rest } = this.custom;
    this.custom = rest;
  }
}
export const emojis = new Emojis();

/** Puts your emoji on a message, or takes it away. The event comes back on the stream. */
export function react(thread: string, seq: number, emoji: string, on: boolean) {
  if (on) emojis.used(emoji);
  return api.react(thread, { seq, emoji, on }).catch((e) => hub.fail(e));
}

/** Where the picker is open: under the button that opened it. */
export const picker = $state<{ at: { thread: string; seq: number; x: number; y: number; up: boolean; right: boolean } | null }>({ at: null });

/** Opens the picker for a message, next to `anchor` (the button), or closes it when it is open there. */
export function togglePicker(anchor: HTMLElement, thread: string, seq: number) {
  const cur = picker.at;
  if (cur && cur.thread === thread && cur.seq === seq) {
    picker.at = null;
    return;
  }
  const r = anchor.getBoundingClientRect();
  const zoom = Number(document.documentElement.style.zoom) || 1;
  const h = window.innerHeight / zoom;
  const w = window.innerWidth / zoom;
  const room = h - r.bottom / zoom;
  const right = r.left / zoom > w / 2;
  picker.at = { thread, seq, x: right ? r.right / zoom : r.left / zoom, y: room < 340 && r.top / zoom > room ? r.top / zoom - 4 : r.bottom / zoom + 4, up: room < 340 && r.top / zoom > room, right };
}
