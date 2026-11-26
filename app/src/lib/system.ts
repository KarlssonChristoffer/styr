// Threads Hub started on its own (`system.rs` in the core): who owns them, and which are system
// threads. A system thread (the Guard, a routine run, the merge queue, a widget build, a bench
// repeat) is Hub's own work across projects and machines: the inbox folds it under "From Hub", and puts it under "Hub
// needs you" on top only when it needs you. An errand (a fix thread, Customs' update, a thread another thread
// started) is done in a project and listed there, marked with the robot and its owner's name.
import type { Owner } from "./bindings/Owner";
import type { OwnerKind } from "./bindings/OwnerKind";
import type { Thread } from "./bindings/Thread";

// Strings, not `OwnerKind`: "bench" is here before every core sends it (`bench/run.rs`), and an
// older core's bindings do not know it yet.
const SYSTEM: readonly string[] = ["vakten", "routine", "merge", "widget", "bench"];

/** Hub's own thread, not one project's. */
export function isSystem(t: Thread): boolean {
  return !!t.owner && SYSTEM.includes(t.owner.kind);
}

/** What Hub's guard is called in the app. The core still stores and sends "Vakten" (old events,
 *  other versions of Hub), so the app translates it where it shows. */
export const GUARD = "Guard";

/** A word the core says ("Vakten": who stamped a problem, who did something) as the app says it. */
export function guardName(who: string): string {
  return who === "Vakten" ? GUARD : who;
}

/** The guard's own chat thread. */
export function isGuardChat(t: Thread): boolean {
  return t.owner?.kind === "vakten";
}

/** A fix thread the guard started (not one you started with Fix now). */
export function isGuardErrand(t: Thread): boolean {
  return t.owner?.kind === "fixer" && t.owner.name === "Vakten";
}

/** The guard's chat or one of its fix threads. Neither is in the inbox until it needs you. */
export function isGuard(t: Thread): boolean {
  return isGuardChat(t) || isGuardErrand(t);
}

/** The owner's name as lists show it: "Guard" for the guard's chat, "via Guard" for its fix threads. */
export function ownerName(o: Owner): string {
  if (o.kind === "vakten") return GUARD;
  if (o.kind === "fixer" && o.name === "Vakten") return `via ${GUARD}`;
  return o.name;
}

/** What each kind of owner is, for the hover. */
const WHAT: Record<OwnerKind, string> & Record<string, string> = {
  bench: "a bench run, one repeat of one case",
  vakten: "Guard, Hub's own guard of Problems",
  routine: "a routine",
  merge: "Hub's merge queue",
  widget: "an Overview widget build",
  fixer: "a fix for a problem",
  customs: "Customs' monthly update",
  agent: "another thread",
};

/** "Started by Guard" with what that is, for a hover. */
export function ownerTip(o: Owner): string {
  if (o.kind === "fixer") return `Started by ${guardName(o.name)} to fix problem #${o.id ?? "?"}, not by you`;
  if (o.kind === "agent") return `Started by the agent in “${o.name}”, not by you`;
  if (o.kind === "routine") return `A run of the routine ${o.name}, started by Hub`;
  return `Started by ${WHAT[o.kind]}, not by you`;
}
