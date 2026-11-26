// The thread lists the rail and Home show (newest, waiting, running), derived from the thread map.
import { isGuard, isGuardErrand, isSystem } from "./system";
import { prefs } from "./prefs.svelte";
import { isChatThread } from "./format";
import { asksYou, isSnoozed, needsYou } from "./threadFlags";
import type { Thread } from "./bindings/Thread";
import type { HostSnapshot } from "./bindings/HostSnapshot";
import type { PlanUsage } from "./bindings/PlanUsage";
import type { GuardPrefs } from "./bindings/GuardPrefs";

/** How long a finished thread may keep a server running before Home suggests stopping it. */
const STALE_MS = 2 * 60 * 60 * 1000;

/** What the lists read from the hub; `hub` itself passes `this`. */
export interface ThreadListsSource {
  threads: Thread[];
  now: number;
  host: HostSnapshot | null;
  plan: PlanUsage | null;
  guard: GuardPrefs | null;
  isDesk(t: Thread): boolean;
  inFocus(t: Thread): boolean;
  guardHandles(t: Thread): boolean;
}

/** Newest first by `at`, and by id when two tie, so equal times never swap places. */
export function newest(threads: Thread[], at: (t: Thread) => number): Thread[] {
  return threads.sort((a, b) => at(b) - at(a) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

/** Every list of threads the app draws (Inbox, sidebar, focus bar, quota strip), derived from the
 *  hub's threads. `hub` re-exposes each as a getter, so callers keep writing `hub.inbox`. */
export class ThreadLists {
  private h: ThreadListsSource = null as unknown as ThreadListsSource;
  constructor(h: ThreadListsSource) {
    this.h = h;
  }

  /** Needs you and is still in front of you: not settled away, not snoozed. Every "needs you" count
   *  counts these, so a count never points at a thread the lists have put away. */
  waits = (t: Thread): boolean => needsYou(t) && !t.settled_at && !isSnoozed(t, this.h.now) && !this.h.guardHandles(t);

  /** Threads that are not archived, in every project. A side job belongs to the thread that
   *  started it, like a subagent: it shows there (`ChildLine`, which says when it needs you) and
   *  never as a row of its own in a list or in Inbox. */
  everything = $derived(
    this.h.threads.filter(
      (t) => !t.archived && !t.parent && !isChatThread(t) && ((!this.h.isDesk(t) && !isSystem(t)) || t.status === "waiting_on_you" || t.status === "error" || (isSystem(t) && asksYou(t))),
    ),
  );
  /** Hub's own threads (`system.ts`): Vakten, routine runs, the merge queue, widget builds, on
   *  every machine. The inbox keeps them folded under "From Hub", out of the way; one that needs
   *  you goes to "Hub needs you" on top instead. */
  systemThreads = $derived(this.h.threads.filter((t) => !t.archived && isSystem(t)));

  /** The iPhone's chats (`chats/` folders), on every machine. They are not work: Inbox keeps them
   *  folded under "Chats", out of its counts, and they have the Chats project under Temporary. */
  chats = $derived(
    newest(
      this.h.threads.filter((t) => !t.archived && !t.parent && isChatThread(t) && !isSystem(t)),
      (t) => t.returned_at ?? t.updated_at,
    ),
  );

  /** What every list shows: `everything`, or in focus mode only the project in focus. */
  active = $derived(prefs.focusOn ? this.everything.filter((t) => this.h.inFocus(t)) : this.everything);
  /** What the project lists draw their threads from: the work, and only the work. A chat is the
   *  Chat world's and never a project's thread, so Dev's lists have no Chats project. */
  withChats = $derived.by(() => {
    // Hub's own threads (the Guard's chat lives in Hub's data folder) are not a project's work: a
    // folder that only they use must not show up as a project of its own.
    return this.active.filter((t) => !isSystem(t));
  });
  archived = $derived(this.h.threads.filter((t) => t.archived));
  /** Hub's own threads that wait on you, the newest first: the inbox's "Hub needs you". */
  hubAsks = $derived(newest(this.active.filter((t) => isSystem(t) && !isGuard(t) && this.waits(t)), (t) => t.returned_at ?? t.updated_at));
  /** The Guard's chat or one of its fix threads, waiting on you: the inbox's "Guard needs you". The
   *  Guard handles its own fix threads first, so only a real stop gets here. */
  guardAsks = $derived(newest(this.active.filter((t) => isGuard(t) && this.waits(t)), (t) => t.returned_at ?? t.updated_at));
  /** The Guard's fix threads that are not put away: the Problems page and the status bar list them. */
  guardThreads = $derived(newest(this.h.threads.filter((t) => !t.archived && !t.settled_at && isGuardErrand(t)), (t) => t.updated_at));
  /** The rest of Hub's own threads, the latest first: the inbox's folded "From Hub". */
  fromHub = $derived(
    newest(
      (prefs.focusOn ? this.systemThreads.filter((t) => this.h.inFocus(t)) : this.systemThreads).filter((t) => !this.waits(t) && !isGuard(t)),
      (t) => t.returned_at ?? t.updated_at,
    ),
  );
  waiting = $derived(newest(this.active.filter((t) => t.status === "waiting_on_you"), (t) => t.returned_at ?? t.updated_at));
  /** Threads whose last reply asks you something or got stuck, as a decision model read it. */
  asking = $derived(newest(this.active.filter((t) => asksYou(t) && !t.settled_at), (t) => t.returned_at ?? t.updated_at));
  /** Working threads, the one you last sent work to first. Their own activity never moves them. */
  running = $derived(newest(this.active.filter((t) => t.status === "running"), (t) => t.sent_at ?? t.created_at));
  /** Pinned threads, on top of the activity list in the order you put them. */
  pinned = $derived(
    newest(this.active.filter((t) => t.pinned_at != null && !t.settled_at && !isSystem(t)), (t) => t.pinned_at!),
  );
  /** Hidden until they wake, soonest first. */
  snoozed = $derived(
    newest(
      this.active.filter((t) => isSnoozed(t, this.h.now) && t.pinned_at == null && !t.settled_at),
      (t) => -t.snoozed_until!,
    ),
  );
  /** Working threads folded below the inbox, the one you last sent work to first. */
  working = $derived(
    prefs.workingSection
      ? this.running.filter((t) => t.pinned_at == null && !isSnoozed(t, this.h.now) && !isGuard(t))
      : [],
  );
  /** The activity list: threads that handed the ball back, latest first. A snooze that ran out
   *  counts as coming back when it woke. */
  inbox = $derived(
    newest(
      this.active.filter(
        (t) =>
          !t.settled_at &&
          t.pinned_at == null &&
          !isSnoozed(t, this.h.now) &&
          (t.status !== "running" || !prefs.workingSection) &&
          // Hub's own threads have sections of their own (`hubAsks`, `fromHub`), and the Guard's
          // (its chat and its fix threads) come in only when they need you (`guardAsks`).
          !isSystem(t) &&
          !isGuard(t) &&
          // An errand Hub started (a fix thread, say) comes to the inbox when it hands back, not
          // the moment it starts; meanwhile it is in its project's list.
          !(t.owner && t.status === "running" && t.returned_at == null),
      ),
      (t) => Math.max(t.returned_at ?? t.created_at, t.snoozed_until ?? 0),
    ),
  );
  settled = $derived(
    newest(this.active.filter((t) => t.settled_at && t.status !== "running" && t.pinned_at == null), (t) => t.settled_at!),
  );
  /** Ports still held for threads that are done: archived, deleted, or idle for a couple of hours. */
  strays = $derived(
    (this.h.host?.listeners ?? []).filter((l) => {
      if (l.origin !== "thread" || !l.thread_id || l.is_hub || l.pid == null) return false;
      const t = this.h.threads.find((t) => t.id === l.thread_id);
      return !t || t.archived || (t.status !== "running" && t.status !== "waiting_on_you" && Date.now() - t.updated_at > STALE_MS);
    }),
  );
  /** The plan limit closest to running out, once it is past the guard's warning line. */
  quota = $derived.by(() => {
    const top = [...(this.h.plan?.limits ?? [])].sort((a, b) => b.percent - a.percent)[0];
    return top && top.percent >= (this.h.guard?.warn_at ?? 80) ? top : null;
  });
}
