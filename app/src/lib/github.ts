// What the GitHub tab and the thread's Git button work out from what the core sends: the list's
// groups, a diff split into files, and which Git step comes next. No requests here.
import type { GitState } from "./bindings/GitState";
import type { PullRow } from "./bindings/PullRow";
import type { ThreadPr } from "./bindings/ThreadPr";

export type PatchLine = { op: "+" | "-" | " " | "@"; text: string };
export type PatchFile = { path: string; old?: string; additions: number; deletions: number; lines: PatchLine[]; binary: boolean };

/** `gh pr diff` output split per file. */
export function parsePatch(text: string): PatchFile[] {
  const files: PatchFile[] = [];
  let f: PatchFile | null = null;
  for (const line of text.split("\n")) {
    if (line.startsWith("diff --git ")) {
      const m = /^diff --git a\/(.+) b\/(.+)$/.exec(line);
      f = { path: m?.[2] ?? line.slice(11), old: m && m[1] !== m[2] ? m[1] : undefined, additions: 0, deletions: 0, lines: [], binary: false };
      files.push(f);
    } else if (!f) {
      continue;
    } else if (line.startsWith("@@")) {
      f.lines.push({ op: "@", text: line.replace(/^@@ [^@]* @@ ?/, "") });
    } else if (line.startsWith("Binary files")) {
      f.binary = true;
    } else if (f.lines.length && line.startsWith("+")) {
      f.lines.push({ op: "+", text: line.slice(1) });
      f.additions++;
    } else if (f.lines.length && line.startsWith("-")) {
      f.lines.push({ op: "-", text: line.slice(1) });
      f.deletions++;
    } else if (f.lines.length && line.startsWith(" ")) {
      f.lines.push({ op: " ", text: line.slice(1) });
    }
  }
  return files;
}

export type PullGroup = { key: string; label: string; pulls: PullRow[] };

/** Open pull requests by how close they are to merging; the others by when. */
export function groupPulls(pulls: PullRow[], now = Date.now()): PullGroup[] {
  const ready: PullRow[] = [], work: PullRow[] = [], waiting: PullRow[] = [], drafts: PullRow[] = [];
  const today: PullRow[] = [], earlier: PullRow[] = [];
  for (const p of pulls) {
    if (p.state !== "open") (now - p.updated_at < 86_400_000 ? today : earlier).push(p);
    else if (p.draft) drafts.push(p);
    else if (p.checks === "fail" || p.mergeable === "conflicts" || p.review === "changes_requested") work.push(p);
    else if (p.checks === "pending" || p.review === "review_required") waiting.push(p);
    else ready.push(p);
  }
  const done = pulls[0]?.state === "closed" ? "Closed" : "Merged";
  return [
    { key: "ready", label: "Ready to merge", pulls: ready },
    { key: "work", label: "Needs work", pulls: work },
    { key: "waiting", label: "Waiting", pulls: waiting },
    { key: "drafts", label: "Drafts", pulls: drafts },
    { key: "today", label: `${done} today`, pulls: today },
    { key: "earlier", label: `${done} earlier`, pulls: earlier },
  ].filter((g) => g.pulls.length);
}

/** Words and colour for a pull request's state, for chips. */
export function pullTone(p: Pick<PullRow, "state" | "draft">): { label: string; tone: "open" | "draft" | "merged" | "closed" } {
  if (p.state === "merged") return { label: "Merged", tone: "merged" };
  if (p.state === "closed") return { label: "Closed", tone: "closed" };
  if (p.draft) return { label: "Draft", tone: "draft" };
  return { label: "Open", tone: "open" };
}

export const checksLabel: Record<string, string> = { pass: "checks pass", fail: "checks fail", pending: "checks running", none: "" };
export const reviewLabel: Record<string, string> = { approved: "approved", changes_requested: "changes asked", review_required: "review needed" };

export type GitStep = {
  label: string;
  commit: boolean;
  push: boolean;
  pr: boolean;
  pull?: boolean;
  /** Pushing straight to the default branch: the button asks once more. */
  toDefault?: boolean;
};

/** What the Git button does next, after T3 Code's: commit → push → pull request, or pull. */
export function nextStep(st: GitState, pr: ThreadPr | undefined): GitStep | null {
  const branch = st.branch;
  if (!branch) return null;
  const onDefault = branch === st.default;
  const hasPr = pr && (pr.state === "open" || pr.state === "draft");
  const canPr = st.github && !onDefault && !hasPr;
  if (st.files.length) {
    if (!st.github) return { label: "Commit", commit: true, push: false, pr: false };
    if (canPr) return { label: "Commit, push & PR", commit: true, push: true, pr: true };
    return { label: onDefault ? `Commit & push to ${branch}` : "Commit & push", commit: true, push: true, pr: false, toDefault: onDefault };
  }
  if (st.behind && !st.ahead) return { label: "Pull", commit: false, push: false, pr: false, pull: true };
  if (st.ahead || (!st.upstream && !onDefault && st.github)) {
    if (canPr) return { label: "Push & create PR", commit: false, push: true, pr: true };
    return { label: onDefault ? `Push to ${branch}` : "Push", commit: false, push: true, pr: false, toDefault: onDefault };
  }
  return null;
}
