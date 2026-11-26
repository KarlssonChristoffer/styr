// Collapses the agent's tool calls and thinking between two pieces of text into one activity
// line, so the transcript reads like Claude Code: the answer, with a quiet line for the work.
import { plain } from "./ansi";
import type { ComponentProps } from "svelte";
import type Icon from "./components/Icon.svelte";
import type { Item, ToolItem } from "./transcript";
import { isWebStep } from "./web";

type IconName = ComponentProps<typeof Icon>["name"];

export type Step = ToolItem | { kind: "thought"; text: string };
/** `web`: the run is the agent searching the web and reading pages, shown as its own row. */
export type Activity = { kind: "activity"; steps: Step[]; web?: boolean };
export type Shown = Exclude<Item, Step> | Activity;

export function group(items: Item[]): Shown[] {
  const shown: Shown[] = [];
  for (const item of items) {
    if (item.kind === "tool" || item.kind === "thought") {
      const last = shown.at(-1);
      const web = isWebStep(item);
      if (last?.kind === "activity" && !!last.web === web) last.steps.push(item);
      else shown.push(web ? { kind: "activity", steps: [item], web } : { kind: "activity", steps: [item] });
    } else {
      shown.push(item as Exclude<Item, Step>);
    }
  }
  return shown;
}

/** "Read README.md" while running reads "Reading README.md…". */
export function live(step: Step): string {
  if (step.kind === "thought") return "Thinking…";
  const title = step.title.replace(/`/g, "");
  const [verb, ...rest] = title.split(" ");
  const ing: Record<string, string> = {
    Read: "Reading", Find: "Finding", Search: "Searching", Grep: "Searching", Edit: "Editing",
    Write: "Writing", Run: "Running", Fetch: "Fetching", List: "Listing", Create: "Creating",
    Delete: "Deleting", Move: "Moving", Glob: "Finding", Bash: "Running", Update: "Updating",
  };
  if (ing[verb]) return `${ing[verb]} ${rest.join(" ")}…`;
  // Codex titles a command with the command line itself.
  return step.toolKind === "execute" ? `Running ${title}…` : `${title}…`;
}

export function running(step: Step | undefined): boolean {
  return !!step && step.kind === "tool" && (step.status === "in_progress" || step.status === "pending");
}

/** "Read 2 files, ran 1 command" style summary of finished work. */
export function summary(steps: Step[]): string {
  const tools = steps.filter((s): s is ToolItem => s.kind === "tool");
  if (!tools.length) return "Thought it through";
  const counts = new Map<string, number>();
  for (const t of tools) {
    const k = t.toolKind ?? "other";
    counts.set(k, (counts.get(k) ?? 0) + 1);
  }
  const words: Record<string, [string, string, string]> = {
    read: ["Read", "file", "files"],
    search: ["Searched", "time", "times"],
    edit: ["Edited", "file", "files"],
    execute: ["Ran", "command", "commands"],
    fetch: ["Fetched", "page", "pages"],
    delete: ["Deleted", "file", "files"],
    move: ["Moved", "file", "files"],
    think: ["Planned", "step", "steps"],
  };
  const parts = [...counts].map(([k, n]) => {
    const [verb, one, many] = words[k] ?? ["Used", "tool", "tools"];
    return `${verb} ${n} ${n === 1 ? one : many}`;
  });
  return parts.map((p, i) => (i ? p[0].toLowerCase() + p.slice(1) : p)).join(", ");
}

/** "42s", "3m 12s", "1h 4m". */
export function duration(ms: number): string {
  const s = Math.max(1, Math.round(ms / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
}

/** The one line a finished turn folds into: "Worked for 42s · read 4 files, edited 1 file".
 *  While the turn runs it reads "So far · …" without a clock: the status line above the composer
 *  is the one place that says how long it has been working. */
export function worked(steps: Step[], ms: number | null, planned: boolean | { done: number; of: number }, notes = 0, running = false): string {
  const parts: string[] = [];
  if (typeof planned === "object") parts.push(planned.done === planned.of ? `${planned.of} steps` : `${planned.done} of ${planned.of} steps`);
  else if (planned) parts.push("made a plan");
  if (notes) parts.push(`${notes} ${notes === 1 ? "note" : "notes"}`);
  if (steps.some((s) => s.kind === "tool")) parts.push(summary(steps));
  const what = parts.map((p) => p[0].toLowerCase() + p.slice(1)).join(", ");
  if (running) return `So far${what ? ` · ${what}` : ""}`;
  if (ms == null) return what ? what[0].toUpperCase() + what.slice(1) : "Worked on it";
  return `Worked for ${duration(ms)}${what ? ` · ${what}` : ""}`;
}

/** The grey glyph of a step, by what kind of work it is (an Icon name). Colour stays for state. */
export function glyph(step: Step): IconName {
  if (step.kind === "thought") return "brain";
  if (step.terminal) return "terminal";
  const glyphs: Record<string, IconName> = {
    read: "file", edit: "pen", delete: "x", move: "swap", search: "search",
    execute: "terminal", fetch: "globe", think: "plan", switch_mode: "swap",
  };
  return glyphs[step.toolKind ?? ""] ?? "puzzle";
}

/** The last line a command printed, colours dropped, or "" when it printed nothing. */
export function lastLine(output: string | undefined): string {
  if (!output) return "";
  const lines = plain(output.length > 4000 ? output.slice(-4000) : output).split("\n");
  return lines.findLast((l) => l.trim())?.trim() ?? "";
}

/** A command that ended badly: it failed, or it exited with something other than 0. */
export function failedStep(step: Step): boolean {
  return step.kind === "tool" && (step.status === "failed" || (step.exit != null && step.exit !== 0));
}

export type Now = {
  /** When the running turn started. */
  since: number;
};

/** When the thread's running turn started, for the status line's clock. What it does right now
 *  is the transcript's live activity line and what it has done is the fold row, so the status
 *  line says neither again. */
export function now(items: Item[]): Now | null {
  const at = items.findLastIndex((i) => i.kind === "user");
  const said = items[at];
  if (!said || said.kind !== "user") return null;
  return { since: said.receipt?.taken ?? said.ts };
}

/** The playful words of the quietest chat level, in the way Claude Code has them: they say that
 *  something is happening without saying what. */
export const WORDS = [
  "Tempering", "Simmering", "Noodling", "Untangling", "Kneading", "Whittling", "Polishing", "Mulling",
  "Stitching", "Sifting", "Brewing", "Tinkering", "Threading", "Weaving", "Distilling", "Sketching",
  "Winnowing", "Percolating", "Smithing", "Gardening", "Folding", "Burnishing", "Sanding", "Kindling",
] as const;

/** The word for the `n`th piece of work a turn has done. It changes when the agent really does
 *  something new, not on a timer, so a word that stands still for long means the turn has. `seed`
 *  (the thread) keeps two threads from saying the same word at once. */
export function playfulWord(seed: string, n: number): string {
  let h = 0;
  for (const c of seed) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return WORDS[(h + n * 7) % WORDS.length];
}

/** The lean line of Minimal and Quiet: how long it worked, how far its plan got and how many files
 *  it changed, and nothing of what it read or ran (that is Normal's). While it runs ("So far") it is
 *  the plan and the changes alone. */
export function workedQuietly(ms: number | null, files: number, plan?: { done: number; of: number } | false, running = false): string {
  const parts: string[] = [];
  if (plan) parts.push(plan.done === plan.of ? `${plan.of} ${plan.of === 1 ? "step" : "steps"}` : `${plan.done} of ${plan.of} steps`);
  if (files) parts.push(`${files} ${files === 1 ? "file" : "files"} changed`);
  const what = parts.join(" · ");
  if (running) return `So far${what ? ` · ${what}` : ""}`;
  if (ms == null) return what ? `Worked on it · ${what}` : "Worked on it";
  return `Worked for ${duration(ms)}${what ? ` · ${what}` : ""}`;
}
