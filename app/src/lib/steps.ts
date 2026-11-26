// The steps card: the plan an agent keeps while it works (ACP `plan` updates, which Claude's to-do
// list and Codex's plan both arrive as), shown above the live step of the running turn. Colour
// stays for state: the working grid marks the step now, a grey check the steps done.
import type { Item } from "./transcript";

export type PlanItem = Extract<Item, { kind: "plan" }>;

/** The plan of the turn that is running (after your last message), if the agent made one. */
export function turnPlan(items: readonly { kind: string }[]): PlanItem | null {
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i];
    if (it.kind === "plan") return (it as PlanItem).entries.length ? (it as PlanItem) : null;
    if (it.kind === "user") return null;
  }
  return null;
}

/** Whether the plan shows while a turn runs: as the card, or in Minimal as one line (`stepsLine`),
 *  unless the chat says everything (Full), which shows the plan where it was made. It then holds the
 *  one moving grid of the thread. */
export function stepsShown(items: readonly { kind: string }[], detail: string): PlanItem | null {
  return detail === "all" ? null : turnPlan(items);
}

/** The plan as one line, for Minimal: "Step 2 of 4 · Draw the flow". With every step done it says so. */
export function stepsLine(plan: PlanItem): { text: string; of: string } {
  const total = plan.entries.length;
  const done = plan.entries.filter((e) => e.status === "completed").length;
  const now = plan.entries.findIndex((e) => e.status === "in_progress");
  const at = now >= 0 ? now : Math.min(done, total - 1);
  if (done === total) return { text: "All steps done", of: `${total} of ${total}` };
  return { text: plan.entries[at].content, of: `Step ${at + 1} of ${total}` };
}

/** The steps of a plan with how many are done, the one now and what to hide when it is long. */
export function stepsView(plan: PlanItem, max = 6) {
  const done = plan.entries.filter((e) => e.status === "completed").length;
  const now = plan.entries.findIndex((e) => e.status === "in_progress");
  let hidden = 0;
  const keep = plan.entries.map((_, n) => n);
  if (plan.entries.length > max) {
    // Finished steps fold away, all but the last one, so the card stays about five rows high.
    const lastDone = plan.entries.findLastIndex((e) => e.status === "completed");
    const drop = keep.filter((n) => plan.entries[n].status === "completed" && n !== lastDone);
    hidden = drop.length;
    return { done, now, hidden, shown: keep.filter((n) => !drop.includes(n)) };
  }
  return { done, now, hidden, shown: keep };
}
