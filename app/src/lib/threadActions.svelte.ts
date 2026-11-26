// What you can do to a thread from a menu: rename, archive, snooze, stop, delete; each calls the api and updates state.
import { api } from "./api";
import type { ThreadPatch } from "./bindings/ThreadPatch";
import type { Thread } from "./bindings/Thread";
import { unread, wakeLabel } from "./threadFlags";
import type { HubState } from "./state.svelte";

/** What a person does to a thread: rename, archive, pin, snooze, settle, mark seen, delete.
 *  `hub` keeps a one-line method for each, so nothing that calls them changes. */
export class ThreadActions {
  constructor(private h: HubState) {}

  /** Moves a thread to another machine. The row and the chat stay as they are while it moves, and
   *  when it is done only the machine on the row changes: it is the same thread, so it is not a
   *  reason to leave the page or to redraw it. */
  async moveThread(t: Thread, to: string, stop: boolean) {
    this.h.holding.set(t.id, { ...t });
    this.h.unheard.add(t.id);
    try {
      await api.moveThread({ thread: t.id, from: t.host ?? undefined, to, stop });
    } catch (e) {
      // It may have stopped the turn before it failed; show what is true.
      this.h.holding.delete(t.id);
      this.h.unheard.delete(t.id);
      await this.h.resync();
      throw e;
    }
    this.h.holding.delete(t.id);
    // Its log is kept; what the move added to it is read from the new machine.
    await this.h.resync();
    if (!this.h.events[t.id] && this.h.selected === t.id && this.h.view === "thread") this.h.follow(t.id);
    setTimeout(() => {
      this.h.unheard.delete(t.id);
      this.h.resync();
    }, 4000);
  }

  async rename(id: string, title: string) {
    try {
      this.upsert(await api.patchThread(id, { title }));
    } catch (e) {
      this.h.fail(e);
    }
  }

  async setArchived(id: string, archived: boolean) {
    try {
      const t = this.h.threads.find((t) => t.id === id);
      this.upsert(await api.patchThread(id, { archived }));
      if (archived && this.h.selected === id) this.h.goHome();
      if (t && archived) this.offerUndo(`“${t.title}” archived`, () => this.h.setArchived(id, false));
    } catch (e) {
      this.h.fail(e);
    }
  }

  /** Changes marks on one or more threads at once, with one way back for all of them. */
  async mark(ids: string[], patch: ThreadPatch, label: string) {
    const before = ids.map((id) => this.h.threads.find((t) => t.id === id)).filter((t): t is Thread => !!t);
    if (!before.length) return;
    // Leaving the thread you are in moves on to the next one, as working through an inbox.
    const leaving = this.h.view === "thread" && this.h.selected && ids.includes(this.h.selected) && (patch.settled || (patch.snooze_until ?? 0) > Date.now());
    // The next thread comes from the world you are in: settling a chat never lands in Dev.
    const list = this.h.world === "chat" ? this.h.chats : this.h.inbox;
    const at = list.findIndex((x) => x.id === this.h.selected);
    const next = list.slice(at + 1).concat(list.slice(0, Math.max(at, 0)).reverse()).find((t) => !ids.includes(t.id));
    try {
      for (const t of await Promise.all(ids.map((id) => api.patchThread(id, patch)))) this.upsert(t);
      if (leaving) {
        if (next) this.h.open(next.id);
        else this.h.goHome();
      }
      this.h.picked = [];
      const name = before.length === 1 ? `“${before[0].title}”` : `${before.length} threads`;
      this.offerUndo(`${name} ${label}`, async () => {
        for (const t of before) this.upsert(await api.patchThread(t.id, restore(t, patch)));
      });
    } catch (e) {
      this.h.fail(e);
    }
  }

  pin(ids: string[], pinned: boolean) {
    return this.mark(ids, { pinned }, pinned ? "pinned" : "unpinned");
  }

  /** Pins a thread between two others: `above` and `below` are the neighbours' pins. */
  pinAt(id: string, above: number | null, below: number | null) {
    const order =
      above != null && below != null ? Math.round((above + below) / 2)
      : above != null ? above - 1000
      : below != null ? below + 1000
      : Date.now();
    return this.mark([id], { pin_order: order }, "pinned");
  }

  snooze(ids: string[], until: number) {
    return this.mark(ids, { snooze_until: until }, until > Date.now() ? `snoozed until ${wakeLabel(until)}` : "woken");
  }

  markUnread(id: string) {
    return this.mark([id], { seen: false }, "marked unread");
  }

  async setAutoSettle(id: string, auto: boolean) {
    try {
      this.upsert(await api.patchThread(id, { auto_settle: auto }));
    } catch (e) {
      this.h.fail(e);
    }
  }

  async retitle(id: string) {
    try {
      this.upsert(await api.retitle(id));
    } catch (e) {
      this.h.fail(e);
    }
  }

  /** Marks threads done, or back in play, with a way back for a few seconds. */
  settleMany(ids: string[], settled: boolean) {
    return this.mark(ids, { settled }, settled ? "settled" : "back in the list");
  }

  /** Marks a thread done, or back in play, with a way back for a few seconds. */
  settle(id: string, settled: boolean) {
    return this.settleMany([id], settled);
  }

  /** Shows a way back for a few seconds, also on Ctrl Z. */
  private offerUndo(label: string, back: () => Promise<void>) {
    const undo = { label, run: () => void back().then(() => (this.h.undo = null)), at: Date.now() };
    this.h.undo = undo;
    setTimeout(() => this.h.undo?.at === undo.at && (this.h.undo = null), 5000);
  }

  private seeing = new Set<string>();

  async markSeen(id: string) {
    if (this.seeing.has(id)) return;
    this.seeing.add(id);
    try {
      this.upsert(await api.patchThread(id, { seen: true }));
    } catch {
      // Staying unread is harmless; the next open tries again.
    } finally {
      this.seeing.delete(id);
    }
  }

  async remove(id: string) {
    try {
      await api.deleteThread(id);
      this.h.forget(id);
    } catch (e) {
      this.h.fail(e);
    }
  }

  /** Takes a thread as a request answered it, unless the stream has brought a newer one since:
   *  a turn can start and end while an answer is on its way, and the older snapshot would leave
   *  the thread shown as Running for good. */
  upsert(thread: Thread) {
    const i = this.h.threads.findIndex((t) => t.id === thread.id);
    if (i >= 0 && this.h.threads[i].updated_at <= thread.updated_at) this.h.threads[i] = thread;
  }
}

/** The patch that puts back what `patch` changed on `t`. */
function restore(t: Thread, patch: ThreadPatch): ThreadPatch {
  const back: ThreadPatch = {};
  if (patch.settled != null) back.settled = t.settled_at != null;
  if (patch.settled != null || patch.pinned != null || patch.pin_order != null) {
    if (t.pinned_at != null) back.pin_order = t.pinned_at;
    else back.pinned = false;
  }
  if (patch.settled != null || patch.snooze_until != null) back.snooze_until = t.snoozed_until ?? 0;
  if (patch.seen != null) back.seen = !unread(t);
  return back;
}
