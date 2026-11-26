// Stand-in threads: a placeholder row shown at once while the real thread is being created.
import { api } from "./api";
import { quiet } from "./quiet";
import { drafts, setDraft } from "./drafts.svelte";
import { folderName } from "./format";
import type { Attachment } from "./bindings/Attachment";
import type { Event } from "./bindings/Event";
import type { SettingValue } from "./bindings/SettingValue";
import type { Thread } from "./bindings/Thread";
import type { View } from "./viewTypes";

/** Ids of stand-ins for threads the core is still creating (`StandIns.show`). */
const STAND_IN = "starting:";
export const isStandIn = (id: string) => id.startsWith(STAND_IN);

/** What the stand-ins need of the hub they live in (`HubState`, which passes itself). */
export interface StandInHost {
  threads: Thread[];
  events: Record<string, Event[]>;
  view: View;
  selected: string | null;
  notice: string | null;
  localHost: string;
  open(id: string): void;
  catchUp(id: string): Promise<void>;
  startDraft(cwd: string, text: string, host?: string | null, attachments?: Attachment[]): void;
  fail(e: unknown): void;
}

/**
 * Stand-ins for threads the core is still creating, each with the real id once it is made, and the
 * calls that wait for it (`send`, `setSetting`).
 */
export class StandIns {
  private pending = new Map<string, { real: Promise<string | null>; done: (id: string | null) => void }>();

  constructor(private host: StandInHost) {}

  /**
   * Opens a new thread at once, before the core has made it. Creating one can take seconds (a dev
   * worktree, a scratch folder, another machine), and Home sitting empty meanwhile looks like the
   * message was lost. The stand-in shows the message and "Starting" as the real thread will;
   * `newThread` swaps it for that one when the core answers, or takes it away on an error.
   * Returns the stand-in's id.
   */
  show(s: { cwd: string; message: string; attachments?: Attachment[]; engine?: string; host?: string | null }): string {
    const h = this.host;
    const id = `${STAND_IN}${crypto.randomUUID()}`;
    const now = Date.now();
    const line = s.message.split("\n")[0].trim();
    const chars = [...line];
    const title = line ? chars.slice(0, 60).join("") + (chars.length > 60 ? "…" : "") : folderName(s.cwd) || "New thread";
    const host = s.host && s.host !== h.localHost ? s.host : undefined;
    h.threads.unshift({
      id, title, cwd: s.cwd, engine: s.engine ?? "", status: "running", engine_session_id: null, title_locked: false,
      archived: false, host, created_at: now, updated_at: now, returned_at: null, seen_at: now, settled_at: null,
      sent_at: now, pinned_at: null, snoozed_until: null, auto_settle: true, parent: null, reading: null, owner: null,
    });
    const said: Event = { thread_id: id, seq: 1, ts: now, kind: "user_message", data: { text: s.message, attachments: (s.attachments ?? []) as never } };
    h.events = { ...h.events, [id]: [said] };
    let done: (id: string | null) => void = () => {};
    const real = new Promise<string | null>((r) => (done = r));
    this.pending.set(id, { real, done });
    h.open(id);
    return id;
  }

  /** The thread a stand-in became; null if it did not start. Any other id is itself. */
  async realId(id: string): Promise<string | null> {
    return isStandIn(id) ? ((await this.pending.get(id)?.real) ?? null) : id;
  }

  /** Puts the real thread where its stand-in was, with its log loaded first so nothing flickers. */
  async swap(stand: string, real: string) {
    const h = this.host;
    if (!h.events[real]) {
      h.events = { ...h.events, [real]: [] };
      await quiet(h.catchUp(real), "catch up");
    }
    // What was written in the stand-in's box meanwhile moves along with it.
    if (drafts[stand]) {
      setDraft(real, drafts[stand]);
      setDraft(stand, "");
    }
    if (h.view === "thread" && h.selected === stand) h.open(real);
    this.drop(stand, real);
  }

  private drop(stand: string, real: string | null) {
    const h = this.host;
    h.threads = h.threads.filter((t) => t.id !== stand);
    const { [stand]: _, ...rest } = h.events;
    h.events = rest;
    this.pending.get(stand)?.done(real);
    this.pending.delete(stand);
  }

  /** A thread that did not start: its stand-in goes, and if it was on screen the message goes
   *  back into Home's box, so nothing written is lost. */
  fail(stand: string, cwd: string, message: string, attachments: Attachment[] = [], host?: string | null) {
    if (!this.pending.has(stand)) return;
    const h = this.host;
    const looking = h.view === "thread" && h.selected === stand;
    this.drop(stand, null);
    if (looking) h.startDraft(cwd, message, host ?? null, attachments);
  }

  async send(id: string, text: string, attachments: Attachment[] = [], original?: string, steer = false) {
    try {
      // Written while the thread was still being made: it goes once the thread is there.
      const real = await this.realId(id);
      if (!real) throw new Error("The thread did not start, so this message was not sent.");
      id = real;
      await api.send(id, text, attachments, original, steer);
    } catch (e) {
      this.host.fail(e);
      throw e;
    }
  }

  async setSetting(id: string, configId: string, value: SettingValue) {
    try {
      const real = await this.realId(id);
      if (!real) return;
      id = real;
      await api.setSetting(id, configId, value);
    } catch (e) {
      this.host.fail(e);
    }
  }

  tell(text: string) {
    const h = this.host;
    h.notice = text;
    setTimeout(() => {
      if (h.notice === text) h.notice = null;
    }, 2500);
  }
}
