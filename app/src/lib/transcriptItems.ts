// The item types a thread's transcript is made of, and the small pure functions over them.
// `transcript.ts` folds the event log into these and re-exports everything here.
import type { PluginRepo } from "./bindings/PluginRepo";
import type { Attachment } from "./bindings/Attachment";
import type { Event } from "./bindings/Event";
import { payload } from "./events";
import type { WorkerLine } from "./bindings/WorkerLine";
import type { WorkerRun } from "./bindings/WorkerRun";
import type { Risk } from "./risk";


export type ToolItem = {
  kind: "tool";
  id: string;
  title: string;
  status: string;
  toolKind?: string;
  detail?: string;
  /** A command run in a terminal: its output streams in as `terminal_output_delta` chunks. */
  terminal?: boolean;
  /** The command line, when the engine sent it. */
  command?: string;
  /** What the command printed so far, ANSI escapes and all; only the end of a very long one. */
  output?: string;
  /** The output so far is what Hub read while the command ran, until the engine sends its own. */
  live?: boolean;
  /** The output was longer than Hub keeps, so its start is gone. */
  cut?: boolean;
  /** Its exit code once it ended; null when it ended without one (stopped, or a background task). */
  exit?: number | null;
  /** When it started and ended. */
  since?: number;
  until?: number;
};

export type PermissionItem = {
  kind: "permission";
  id: string;
  title: string;
  options: { optionId: string; name: string; kind: string }[];
  answer?: string | null;
  resolved: boolean;
  /** Set when the call would destroy work. */
  risk: Risk | null;
  /** The command line asked about, when there is one. */
  command?: string;
};

/** One edit of a file: the text before and after (a snippet for small edits, the file for writes). */
export type FileEdit = { old: string; new: string };
export type FileChange = { path: string; edits: FileEdit[] };

/** What a model switch hands over, counted from the thread as it stood: what went along as words
 *  (`messages`, and the name of each tool, `tools`) and the attachments that did not (`files`). */
export type Carried = { messages: number; tools: number; files: number };

export type Item =
  | {
      kind: "user";
      seq: number;
      text: string;
      ts: number;
      attachments: Attachment[];
      original?: string;
      /** A sentence that reads like a standing rule ("always…", "never…"). */
      rule?: string;
      /** The rule as saved to CLAUDE.md. */
      saved?: string;
      /** The snapshot of the repository taken before this message reached the engine. */
      checkpoint?: string;
      /** Hub sent this with the findings of second opinion `review`; shown as part of it. */
      review?: string;
      /** Hub sent this with the findings of design review `designReview`; shown as part of it. */
      designReview?: string;
      /** Hub sent this itself: the leak guard's findings, a request for proof, or carrying on
       *  after a plan limit reset or a restart. */
      fromHub?: "leak" | "customs" | "proof" | "docs" | "quota" | "restart" | "terminal" | "watch" | "child" | "routine" | "recipe" | "vakten" | "widget";
      /** The routine that started this run. */
      routine?: string;
      /** What the user asked an Overview widget to be, or to change (`widgets.rs`). */
      widget?: { ask: string; change: boolean };
      /** The recipe this run is, and the agent that started it when it was not the user. */
      recipe?: { name: string; by?: string };
      /** The pull request Hub watches for the thread, when `fromHub` is "watch". */
      watch?: number;
      /** Typed in a terminal while the thread was taken over, copied in when it came back. */
      terminal?: boolean;
      /** Sent by an agent through Hub's tools: another thread (its id and title) or an outside agent. */
      sentBy?: { thread?: string; name: string };
      /** Changed while it waited in the queue; `text` is what the engine got. */
      edited?: boolean;
      /** Asked to go next while it waited: the running turn is stopping for it. */
      steered?: boolean;
      /** Read receipts, as times: Hub has it and it is next (`got`, at once, while the engine may
       *  still be starting), the engine took it (its turn started), the agent first said or did
       *  something about it, and its turn ended. `outcome` when it did not end normally. */
      receipt?: { got: number; taken?: number; seen?: number; done?: number; outcome?: "stopped" | "failed" };
    }
  | { kind: "assistant"; text: string }
  | { kind: "thought"; text: string }
  | ToolItem
  | {
      kind: "plan";
      entries: { content: string; status: string }[];
      /** When each entry started and finished, as the log said it (same order as `entries`). */
      marks?: { from?: number; to?: number }[];
    }
  | PermissionItem
  | {
      kind: "changes";
      files: FileChange[];
      /** Where the reply ended in the log; a review that saw this far covers it. */
      seq: number;
      /** Whether the reply showed that it works. */
      proof?: ProofState;
      /** Whether it kept the docs that cover its code in step. */
      docs?: DocsState;
    }
  | ({ kind: "proof"; reply: number; docs?: DocsState } & ProofState)
  | { kind: "clash"; other: string; otherTitle: string; paths: string[] }
  /** The thread's first message does not belong to the project's stage now (`docs/FOCUS.md`). */
  | { kind: "focus"; stage: string; why: string; seq: number }
  /** The "Delivered when" line of the stage the thread works towards, as a decision model read it. */
  | { kind: "aim"; stage: string; text: string; by: string }
  /** The message at `userSeq` starts a different task than the thread has been on, as a decision model read it. */
  | { kind: "topic"; userSeq: number; text: string; by: string }
  | { kind: "leak"; findings: LeakFinding[]; sent: boolean }
  | { kind: "customs"; found: CustomsFinding[]; sent: boolean; declared: boolean }
  | { kind: "attempt"; state: "picked" | "dropped"; files: number; cwd?: string; safety?: string; by?: string }
  | RestoredItem
  | ReviewItem
  | DesignReviewItem
  /** Design mode turned on or off; `by` "start" when the thread was started in it. */
  | { kind: "design"; on: boolean; by: string; ts: number }
  /** A turn in design mode changed files outside docs/design/. */
  | { kind: "design_strayed"; files: string[] }
  | { kind: "quota"; limit: string; percent: number; to: string }
  | AutoItem
  | WaitItem
  | IdleItem
  | NewItem
  | ChildItem
  | TaskItem
  /** The thread was moved here from another machine (`moved`) or left to carry on on another. */
  | { kind: "moved"; from: string; folder: string; dirty: number; stopped: boolean; ts: number }
  | { kind: "moved_away"; to: string; ts: number }
  /** The thread moved to another engine; the next message hands the conversation over. */
  | { kind: "switch"; from: string; to: string; ts: number; carried?: Carried }
  /** "Edit from here": what followed message `from` was left behind, kept here to look back at. */
  | { kind: "rewound"; from: number; files: boolean; ts: number; cut: Item[] }
  | { kind: "handed_off"; tool: string; title: string; ok: boolean; output: string; ts: number }
  | { kind: "handoff_result"; tool: string; title: string; state: string; proven: boolean | null; cost: number | null; said?: string; link?: string; ts: number }
  | HandoffOfferItem
  | AskItem
  /** A run of one of your Workers: every `worker_run` of the same run folds into one card where
   *  the run first showed, holding the newest state and all the lines. */
  | { kind: "worker_run"; run: WorkerRun; lines: WorkerLine[]; ts: number }
  | { kind: "terminal"; state: "taken" | "imported"; turns: number; carryOn: boolean; ts: number }
  | RestartItem
  | PausedItem
  /** A live HTML page the agent showed (`show_page`), served sandboxed by the core. */
  | PageItem
  /** In a project's desk: another thread of the project finished, needs you or failed (`desk.rs`). */
  | { kind: "desk"; thread: string; title: string; state: "done" | "asks" | "failed"; text: string; permission: boolean; ts: number }
  | PluginItem
  /** Where a turn ended; the reply's footer stands here. */
  | { kind: "turn"; seq: number; ts: number; replies?: Offer }
  | { kind: "end"; reason: string }
  | { kind: "refused"; title: string; why: string }
  /** Something the engine told the user that is not part of its reply, e.g. Codex's
   *  "Exceeded skills context budget" (an ACP `notice`). */
  | { kind: "notice"; severity: string; title: string; description?: string; ts: number }
  | ErrorItem
  | SignInItem
  | ImageItem;

/** The engine failed a turn. `report` is its card once it went to Problems (`problems/fixnow.rs`). */
export type ErrorItem = {
  kind: "error";
  /** The latest failure's seq; the same failure again right after is one item, `times` over. */
  seq: number;
  seqs: number[];
  times: number;
  message: string;
  detail?: string;
  prompt?: number;
  report?: Report;
};

/** Where a failure stands in Problems and in its fix. */
export type Report = {
  problem: number;
  state: "sent" | "fixing" | "fixed" | "stopped" | "failed";
  fix?: string;
  fixTitle?: string;
  text?: string;
  pr?: { number: number; url: string; state: string };
};

/** The turn failed because the engine's sign-in on `machine` is gone; a new one is a click away. */
export type SignInItem = {
  kind: "sign_in";
  engine: string;
  label: string;
  machine: string;
  message: string;
  /** It was the long-lived sign-in Hub keeps that stopped working. */
  kept: boolean;
  /** Hub can sign the engine in there in a way that lasts. */
  canKeep: boolean;
  /** A Mac, where the CLI's own login is in the Keychain that Hub's service cannot read. */
  keychain: boolean;
  ts: number;
};

/** Auto took the thread, let go of it, or moved it to another model for one message. */
export type AutoItem =
  | { kind: "auto"; on: boolean; by: string }
  | { kind: "auto"; ask: string; engine: string; why: string; to?: string; toName?: string; fromName?: string };

/** A picture the agent made or showed, such as Codex's image generation. It stays in sight when
 *  the turn folds, because it is part of the answer rather than of the work. */
export type ImageItem = {
  kind: "image";
  /** Where it is in the log: the event's seq and its place among that event's images. */
  id: string;
  /** A data URL. */
  src: string;
  /** Where the engine saved it, on the machine the thread runs on. */
  path?: string;
  /** The prompt the image model actually used. */
  prompt?: string;
};

/** Things the agent showed in a thread, which the viewer opens: pages and pictures. */
export type ShownItem = PageItem | ImageItem;

export function shownIn(items: Item[]): ShownItem[] {
  return items.filter((i): i is ShownItem => i.kind === "page" || i.kind === "image");
}

/** What a shown thing is called: a page's title, or a picture's file name. */
export function shownName(item: ShownItem, of: ShownItem[] = []): string {
  if (item.kind === "page") return item.title;
  const file = item.path?.split(/[\\/]/).pop();
  if (file) return file;
  const images = of.filter((i) => i.kind === "image");
  return images.length > 1 ? `Image ${images.indexOf(item) + 1}` : "Image";
}

export type QueuedItem = Extract<Item, { kind: "user" }>;

/** A live HTML page the agent showed, by its id under the thread. A page the agent changed is a
 *  new version of it (`replaces`): `family` is the first version's id, `version` counts from 1,
 *  and `of` is how many versions there are. */
/** A plugin the agent suggested with `suggest_plugin`: the repository as it was then, and why. */
export type PluginItem = { kind: "plugin"; repo: PluginRepo; why: string; ts: number };
/** The agent offered to hand the thread's work to one of your tools (`offer_handoff`); `done` once it went over. */
/** A question the agent asked without waiting (`ask_user`): it carried on with `assumed`. The
 *  question stays until it is answered, and can be answered again; `answer` is the latest one. */
export type AskItem = { kind: "ask"; id: string; seq: number; question: string; assumed: string; options: { label: string; message: string }[]; answer?: string; ts: number };

/** What an answer to an `ask` says to the agent: a normal message that names what it asked and
 *  assumed, so it can change course when the answer differs. */
export function askLead(a: Pick<AskItem, "question" | "assumed">): string {
  return `You asked \u201c${a.question}\u201d and assumed \u201c${a.assumed}\u201d; the person answered: `;
}
export const askMessage = (a: Pick<AskItem, "question" | "assumed">, option: { message: string }) => askLead(a) + option.message;

/** The latest question asked without waiting that has no answer yet; null if none. */
export function openAsk(items: Item[]): AskItem | null {
  const ask = items.findLast((i): i is AskItem => i.kind === "ask" && i.answer == null);
  return ask ?? null;
}

export type HandoffOfferItem = { kind: "handoff_offer"; tool: string; folder: string; label: string; about: string | null; canBuild: boolean; why: string; done: boolean; failedAt?: number; ts: number };
export type PageItem = { kind: "page"; id: string; title: string; height?: number; ts: number; family: string; version: number; of: number };

/** Every version of `page`'s page, oldest first. */
export function pageVersions(shown: ShownItem[], page: PageItem): PageItem[] {
  return shown.filter((i): i is PageItem => i.kind === "page" && i.family === page.family);
}

/** One page per family, its newest version, where that version is; pictures stay as they are. */
export function latestShown(shown: ShownItem[]): ShownItem[] {
  return shown.filter((i) => i.kind !== "page" || i.version === i.of);
}

/** A turn stopped on a plan limit; Hub carries on when it resets. */
export type WaitItem = {
  kind: "wait";
  /** The `quota_wait` event that started it, which later ones name. */
  seq: number;
  state: "waiting" | "resumed" | "cancelled" | "gave_up" | "switched";
  /** When Hub carries on, and whether that is only a guess. */
  at?: number;
  known: boolean;
  limit?: string;
  message?: string;
  /** Who carried on or cancelled: "hub", "you" or "message". */
  by?: string;
  /** The engine the thread went on with, when it did not wait ("switched"), the one it ran out
   *  on, and when it went over. */
  to?: string;
  from?: string;
  switchedAt?: number;
  /** The other subscription Hub could move it to meanwhile (the thread's "ask" choice). */
  offer?: string;
  tries: number;
};

/** How long a thread sits before the next message gets a "New" break above it. */
export const IDLE_BREAK_MS = 60 * 60 * 1000;

/** The thread sat idle for a while before this message: a "New" break (see Break.svelte). */
export type IdleItem = { kind: "idle"; gap: number; ts: number };

/** Where what you have not seen starts: the first thing the agent did after you last looked at
 *  the thread. `since` is when that was. A "New" break (see Break.svelte); one per opening. */
export type NewItem = { kind: "new"; since: number; ts: number };

/** The panic button stopped this turn; it carries on (a restart by "pause") when resumed. */
export type PausedItem = { kind: "paused"; by: string; ts: number };

/** Hub restarted in the middle of a turn: why, and whether it asked the agent to carry on. */
export type RestartItem = {
  kind: "restart";
  by: "update" | "restart" | "stop" | "crash" | "pause" | "move";
  state: "continued" | "stopped";
  /** Why it did not carry on: switched off, or cut off too many times in a row. */
  why?: "off" | "loop";
  /** Messages sent meanwhile, which went to the agent after it. */
  queued: number;
  ts: number;
};

/** Proof before done for one reply: shown, asked for, failed, still missing after asking, or
 * skipped because there was nothing new to show working. */
export type ProofState = {
  state: "shown" | "failed" | "asked" | "missing" | "skipped";
  /** The run exited cleanly but its output read as a failure (`reading.rs`). */
  hidden?: boolean;
  /** What showed it. */
  via?: "tests" | "screenshot";
  command?: string;
  /** Why Hub asked: "missing" or "failed". Why it did not: "waived", "removed" or "reverted". */
  why?: string;
};

/** Docs in step with code for one reply: the doc sections that cover the code it changed, and
 * whether it updated or opened them, was asked to, or put earlier work back. */
export type DocsState = {
  state: "updated" | "checked" | "asked" | "ignored" | "skipped";
  sections: { doc: string; heading?: string | null; first: number; last: number; files: string[] }[];
  why?: string;
};

export type LeakFinding = { path: string; line: number; kind: string; hint: string };
/** Something a turn brought in, and where it stands at Customs after the review. */
export type CustomsFinding = { kind: string; name: string; path: string; status: string; why?: string };

/** The thread is one of several attempts at the same task. */
export type AttemptStart = { group: string; n: number; of: number; threads: string[]; cwd: string };

/** The repository was rolled back to a snapshot. */
export type RestoredItem = {
  kind: "restored";
  /** The message it went back to before, or null when an earlier rollback was undone. */
  seq: number | null;
  text?: string;
  files: number;
  commits: number;
  keptCommits?: string;
  /** The repository just before, to undo the rollback. */
  safety: string;
  ts: number;
};

/** One part of a pile of changes, reviewed on its own or skipped. */
export type ReviewPart = {
  title: string;
  files: string[];
  why: string;
  state: "waiting" | "skipped" | "running" | "done" | "failed";
  verdict?: string;
  text?: string;
  error?: string;
};

/** A side job: the agent handed one piece of work to its own thread, often on another engine. */
export type ChildItem = {
  kind: "child";
  /** The child thread. */
  id: string;
  title: string;
  /** "image", "research", "review", "subtask", "quick" or "other". */
  job: string;
  engine: string;
  state: "working" | "done" | "failed";
  /** When it finished (ms), so the line can say how long it took. Absent while it works. */
  doneTs?: number;
  /** Where the pictures it made were saved. */
  images: string[];
  reply?: string;
  error?: string;
  ts: number;
};

/** Hub ran one task for the thread on the engine picked for it in Settings > Handoffs (`handoff.rs`). */
export type TaskItem = {
  kind: "task";
  /** Pairs the "working" line with the one that ends it. */
  id: string;
  /** A `handoff::TASKS` id, such as "vetting". */
  task: string;
  /** What it was about, say the package Customs looked at. */
  name: string;
  engine: string;
  model?: string;
  state: "working" | "done" | "failed";
  /** For a vetting: "cleared", "held" or "refused". */
  verdict?: string;
  reason?: string;
  ms?: number;
  ts: number;
};

/** A second opinion from another model on what the thread changed. */
export type ReviewItem = {
  kind: "review";
  id: string;
  state: "running" | "done" | "failed";
  model: string;
  files: string[];
  /** How far into the log it saw; replies up to here are reviewed. */
  seq: number;
  /** "agent" when the thread's agent asked for it, with its `reason`. */
  by: string;
  reason?: string;
  parts: ReviewPart[];
  text?: string;
  /** Its findings went back to the agent. */
  sent?: boolean;
  /** The agent has finished the turn that answered those findings. */
  fixed?: boolean;
  error?: string;
};

/** One fresh reader of a design document (`design.rs`). */
export type DesignCheck = {
  /** "explain", "gaps", "fit" or "ready". */
  name: string;
  title: string;
  model?: string;
  state: "running" | "done" | "failed";
  verdict?: string;
  text?: string;
  error?: string;
};

/** Fresh sessions that never saw the conversation reviewed the thread's design document. */
export type DesignReviewItem = {
  kind: "design_review";
  id: string;
  state: "running" | "done" | "failed";
  doc: string;
  checks: DesignCheck[];
  verdict?: string;
  ready?: boolean;
  /** Its findings went back to the agent. */
  sent?: boolean;
  /** The agent has finished the turn that answered those findings. */
  fixed?: boolean;
  /** "agent" when the thread's agent asked for it. */
  by: string;
  error?: string;
};

/** Whether the thread is in design mode: its last design switch says so. */
export function designOn(events: Event[]): boolean {
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i];
    const on = payload(e, "design")?.on;
    if (typeof on === "boolean") return on;
  }
  return false;
}

/** One-click answers the agent offered (`offer_replies`): a label, and what a click sends. */
export type Offer = { question: string; options: { label: string; message: string }[] };
