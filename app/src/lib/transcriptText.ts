// Reading plain text the transcript shows: rules in a message, line diffs, and the question a reply ends on.
import type { Item, Offer } from "./transcriptItems";

const RULE = /(^|[^\p{L}])(always|never|from now on|going forward|every time|alltid|aldrig|från och med nu|framöver|i fortsättningen|hädanefter|varje gång)([^\p{L}]|$)/iu;

/** The first sentence of a message that reads like a rule to keep. */
export function ruleIn(text: string): string | undefined {
  if (text.trimStart().startsWith("/")) return undefined;
  const sentences = text.split(/(?<=[.!?])\s+|\n+/).map((s) => s.trim());
  return sentences.find((s) => RULE.test(s) && s.length >= 12 && s.length <= 300);
}

export type DiffLine = { op: " " | "+" | "-"; text: string };

/** Line diff of two texts, trimmed to the changed lines with a little context. */
export function diffLines(before: string, after: string, context = 2): DiffLine[] {
  const a = before ? before.split("\n") : [];
  const b = after ? after.split("\n") : [];
  let out: DiffLine[];
  if (a.length * b.length > 400_000) {
    out = [...a.map((text) => ({ op: "-" as const, text })), ...b.map((text) => ({ op: "+" as const, text }))];
  } else {
    // Longest common subsequence, walked from the start.
    const n = a.length, m = b.length;
    const lcs = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
    for (let i = n - 1; i >= 0; i--)
      for (let j = m - 1; j >= 0; j--) lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    out = [];
    let i = 0, j = 0;
    while (i < n || j < m) {
      if (i < n && j < m && a[i] === b[j]) out.push({ op: " ", text: a[i++] }), j++;
      else if (i < n && (j === m || lcs[i + 1][j] >= lcs[i][j + 1])) out.push({ op: "-", text: a[i++] });
      else out.push({ op: "+", text: b[j++] });
    }
  }
  const keep = out.map((l, k) => out.slice(Math.max(0, k - context), k + context + 1).some((x) => x.op !== " "));
  const trimmed: DiffLine[] = [];
  out.forEach((l, k) => {
    if (keep[k]) trimmed.push(l);
    else if (trimmed.at(-1)?.text !== "…" || trimmed.at(-1)?.op !== " ") trimmed.push({ op: " ", text: "…" });
  });
  return trimmed;
}

/** A question the agent ended its reply on, with choices to answer it by. `messages` is what each
 *  choice sends, when the agent said so or Hub offers a plain go-ahead; else "1. <choice>". */
export type OpenQuestion = { seq: number; question: string; options: string[]; messages?: string[] };

/** The question the last reply ended on, when nothing was said since: the replies the agent
 *  offered, or else numbered (or lettered) choices in its text, or else a proposal it asked
 *  about ("Should I…?"), answered by going ahead or not. Other questions are left to the composer. */
export function openQuestion(items: Item[]): OpenQuestion | null {
  const end = items.findLastIndex((i) => i.kind === "turn");
  if (end < 0 || items.slice(end).some((i) => i.kind === "user")) return null;
  const turn = items[end] as Extract<Item, { kind: "turn" }>;
  if (turn.replies) {
    const r = turn.replies;
    return { seq: turn.seq, question: r.question, options: r.options.map((o) => o.label), messages: r.options.map((o) => o.message) };
  }
  let reply: string | undefined;
  for (let k = end - 1; k >= 0 && !reply; k--) {
    const it = items[k];
    if (it.kind === "user") return null;
    if (it.kind === "assistant") reply = it.text;
  }
  if (!reply) return null;
  const found = questionIn(reply);
  if (found) return { seq: turn.seq, ...found };
  const proposal = proposalIn(reply);
  if (!proposal) return null;
  return { seq: turn.seq, question: proposal, options: GO_AHEAD.map((o) => o.label), messages: GO_AHEAD.map((o) => o.message) };
}

/** What Hub offers under a proposal the agent asked about without offering replies itself. */
const GO_AHEAD = [
  { label: "Go ahead", message: "Go ahead." },
  { label: "Go ahead, then close this thread", message: "Go ahead, then close this thread." },
  { label: "No", message: "No." },
];

/** "Should I…?", "Want me to…?", "Shall we…?": a reply that ends on asking to go ahead with
 *  something. The question, or null. Only its last paragraph, a single short question. */
const PROPOSAL = /^(?:so,?\s+)?(?:should|shall|can|may) (?:i|we)\b|^(?:do you )?want me to\b|^would you like me to\b|^(?:is it )?ok(?:ay)? (?:if i|to)\b|^(?:ready|good) to go\b/i;
export function proposalIn(text: string): string | null {
  const last = text.trimEnd().split(/\n\s*\n/).pop() ?? "";
  const lines = last.split("\n").map((l) => clean(l.replace(/^\s*(?:>\s*)+/, ""))).filter(Boolean);
  if (lines.length !== 1) return null;
  const line = lines[0];
  if (line.length > 240 || !line.endsWith("?")) return null;
  // The question, after a lead-in sentence on the same line ("Done. Should I…?").
  const asked = line.split(/(?<=[.!])\s+/).pop() ?? line;
  if (!PROPOSAL.test(asked) || (asked.match(/\?/g) ?? []).length !== 1) return null;
  // "Should I do A or B?" is a choice, not a go-ahead.
  if (/\bor\b/i.test(asked) && !/\bor not\b/i.test(asked)) return null;
  return asked;
}

const CHOICE = /^\s{0,3}(?:[-*]\s+)?(?:\*\*)?(?:(\d)|([A-Ia-i]))[.):](?:\*\*)?\s+(.+)$/;

/** Finds the last list of choices in a reply that asks something: the line before it or the text
 *  after it ends in a question mark. */
export function questionIn(text: string): { question: string; options: string[] } | null {
  const lines = text.trimEnd().split("\n");
  let k = lines.length - 1;
  const after: string[] = [];
  // At most a short closing question after the list ("Which one?").
  while (k >= 0 && !CHOICE.test(lines[k])) {
    if (lines[k].trim()) after.unshift(lines[k].trim());
    if (after.length > 2) return null;
    k--;
  }
  const options: string[] = [];
  while (k >= 0) {
    const m = CHOICE.exec(lines[k]);
    if (m) options.unshift(clean(m[3]));
    else if (lines[k].trim() && !/^\s{2,}\S/.test(lines[k])) break;
    k--;
  }
  if (options.length < 2 || options.length > 9) return null;
  let before = "";
  while (k >= 0 && !before) before = lines[k--].trim();
  const asks = (s: string) => /\?\**\s*$/.test(s);
  const closing = after.find(asks);
  if (!closing && !asks(before)) return null;
  // A lone "?" after the list leaves the question to the line that led into it.
  const question = closing && closing.replace(/[\s?*]/g, "") ? closing : before;
  return { question: clean(question), options };
}

/** A line as it reads, without Markdown's marks. */
function clean(s: string) {
  return s.replace(/\*\*|__|`/g, "").replace(/^#+\s*/, "").trim();
}

