// Who does which kind of task (`handoff.rs`): one table for Settings > Handoffs. Asked of the hubd
// this window uses; one from before it (`handoff.tasks` in compat::FEATURES) has no table, and the
// page says it needs the next version instead of failing.
import { api } from "./api";
import type { HandoffPick } from "./bindings/HandoffPick";
import type { HandoffPrefs } from "./bindings/HandoffPrefs";
import { hubdHas } from "./compat";
import { hub } from "./state.svelte";

/** The task types, as `handoff::TASKS` names them, with a line on what each one covers. */
export const TASKS: { id: string; label: string; hint: string }[] = [
  { id: "pictures", label: "Pictures", hint: "Drawing and editing images" },
  { id: "vetting", label: "Plugin vetting", hint: "Customs reviewing what an agent wants to use" },
  { id: "research", label: "Research", hint: "Looking things up and reading sources" },
  { id: "code_review", label: "Code review", hint: "Reviewing code or a change" },
  { id: "quick", label: "Quick questions", hint: "Short questions that need no context" },
  { id: "housekeeping", label: "Titles and summaries", hint: "Hub's own tidying: titles, the TL;DR, commit text" },
];

export const handoffs = $state<{ prefs: HandoffPrefs | null; has: boolean | null }>({ prefs: null, has: null });

let loading: Promise<void> | null = null;
/** Reads the table once; later calls wait for the same read. */
export function loadHandoffs(): Promise<void> {
  loading ??= hubdHas("handoff.tasks") // compat-until: 2026-12-04
    .then(async (has) => {
      handoffs.has = has;
      if (has) handoffs.prefs = await api.handoffPrefs();
    })
    .catch((e) => {
      loading = null;
      hub.fail(e);
    });
  return loading;
}

/** Sets or clears (`null`) the pick for one task in chats or in dev threads. */
export async function setPick(task: string, side: "chat" | "dev", pick: HandoffPick | null) {
  if (!handoffs.prefs) return;
  const before = handoffs.prefs;
  const row = { chat: null, dev: null, ...before.tasks[task], [side]: pick };
  const tasks = { ...before.tasks, [task]: row };
  if (!row.chat && !row.dev) delete tasks[task];
  handoffs.prefs = { tasks };
  try {
    handoffs.prefs = await api.setHandoffPrefs(handoffs.prefs);
  } catch (e) {
    handoffs.prefs = before;
    hub.fail(e);
  }
}

const SEP = "\u0001";
/** A pick as one value for a Select, and back. */
export const encode = (p: HandoffPick | null | undefined): string => (p ? `${p.engine}${SEP}${p.model ?? ""}` : "");
export function decode(value: string): HandoffPick | null {
  if (!value) return null;
  const [engine, model] = value.split(SEP);
  return { engine, model: model || null };
}
