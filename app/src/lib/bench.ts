// The Labs > Bench card's pure parts: a `BenchRun` (`GET /api/bench/runs/{id}`) turned into its
// progress line and one row per case (each repeat's verdict and reason, k/n, median tokens and wall
// time, summed cost, the flag with its reasons), and a suite (`GET /api/bench/suites`) turned into
// the start choices' labels and the estimate from earlier runs. The numbers per case come from the
// run's `summary` (`bench::results::summary`, feature `bench.summary`); a run without one (an older
// hubd) gets them worked out here and the flags read from its markdown report.
import type { BenchCaseSummary } from "./bindings/BenchCaseSummary";
import type { BenchEstimate } from "./bindings/BenchEstimate";
import type { BenchLevel } from "./bindings/BenchLevel";
import type { BenchRow } from "./bindings/BenchRow";
import type { BenchRun } from "./bindings/BenchRun";
import type { BenchSuiteInfo } from "./bindings/BenchSuiteInfo";
import type { TurnUsage } from "./bindings/TurnUsage";
import { dollars } from "./format";
import { tokens } from "./session";

export type Level = BenchLevel;
export type Flag = { level: Level; why: string[] };
/** One repeat's verdict as the table shows it; "pending" has not run yet. */
export type Verdict = "pass" | "fail" | "invalid" | "pending";
/** A repeat in the table: its verdict and, when the row says one, why (not counted, not run). */
export type Repeat = { verdict: Verdict; why?: string };
export type CaseRow = {
  case: string;
  repeats: Repeat[];
  /** "k/n" over the valid repeats, with the invalid ones counted apart; "not run" before any. */
  passed: string;
  /** "k/n" of the same case in the run compared with; null when there is none. */
  before: string | null;
  /** Medians over the valid repeats, as the report takes them. */
  tokens: number | null;
  wallMs: number | null;
  /** Summed over every repeat that said one. */
  cost: number | null;
  flag?: Flag;
};

/** A repeat cut by a restart runs again, so it is not one that ran. */
const ran = (r: BenchRow) => r.state !== "interrupted";

/** "Case 2 of 5 (edit) · 4 of 15 repeats done". */
export function progress(run: BenchRun): { done: number; total: number; text: string } {
  const total = run.cases.length * run.repeats;
  const done = run.rows.filter(ran).length;
  const at = Math.min(Math.floor(done / Math.max(run.repeats, 1)), run.cases.length - 1);
  const where = run.cases.length ? `Case ${at + 1} of ${run.cases.length} (${run.cases[at]}) · ` : "";
  return { done, total, text: `${where}${done} of ${total} repeats done` };
}

/** The run's state in words, as the report says it. */
export function stateWords(run: BenchRun): string {
  const state = run.state === "running" ? "still running" : run.state === "done" ? "finished" : run.state.replace(/_/g, " ");
  return run.why ? `${state}: ${run.why}` : state;
}

/** A listed run on one line (the list comes without rows). */
export function runLine(run: BenchRun): string {
  return `${run.suite} on ${run.engine} · ${run.model_answered ?? run.model_asked} · ${run.cases.length} cases × ${run.repeats} · ${stateWords(run)}`;
}

/** Each repeat of a case in order, from the rows (the summary has no per-repeat verdicts). */
function repeatsOf(run: BenchRun, c: string): Repeat[] {
  const rows = run.rows.filter((r) => r.case === c && ran(r));
  return Array.from({ length: run.repeats }, (_, i) => {
    const r = rows.find((r) => r.repeat === i + 1);
    if (!r) return { verdict: "pending" };
    const verdict: Verdict = r.invalid ? "invalid" : r.pass ? "pass" : "fail";
    const why = r.invalid ?? r.note;
    return why ? { verdict, why } : { verdict };
  });
}

const passedText = (passed: number, runs: number, invalid: number) =>
  runs + invalid === 0 ? "not run" : `${passed}/${runs}${invalid ? ` (${invalid} invalid)` : ""}`;

/**
 * One row per case of the run, in the suite's order: from `run.summary` when the server sent one,
 * else worked out here from the rows and the report's Flag column. The one place that decides.
 */
export function caseRows(run: BenchRun, report: string | null | undefined): CaseRow[] {
  if (run.summary) {
    const by = new Map(run.summary.cases.map((c) => [c.case, c]));
    return run.cases.map((c) => fromSummary(c, repeatsOf(run, c), by.get(c)));
  }
  // compat-until: 2026-12-12 (an older hubd sends no `summary`; drop this path and `flagsFromReport`)
  const flags = flagsFromReport(report);
  return run.cases.map((c) => worked(run, c, flags[c]));
}

function fromSummary(c: string, repeats: Repeat[], s: BenchCaseSummary | undefined): CaseRow {
  if (!s) return { case: c, repeats, passed: "not run", before: null, tokens: null, wallMs: null, cost: null };
  return {
    case: c,
    repeats,
    passed: passedText(s.passed, s.runs, s.invalid),
    before: s.before ? `${s.before.passed}/${s.before.runs}` : null,
    tokens: s.tokens?.median ?? null,
    wallMs: s.wall_ms?.median ?? null,
    cost: s.cost_usd,
    ...(s.flag ? { flag: { level: s.flag.level, why: s.flag.why } } : {}),
  };
}

// compat-until: 2026-12-12 — from here to `flagsFromReport`, the numbers for a run without `summary`.
const sum = (u: TurnUsage) => (u.input ?? 0) + (u.output ?? 0) + (u.cache_read ?? 0) + (u.cache_write ?? 0);
const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : null);
/** The median the report uses (`timings::spread`): the middle value, the upper one of two. */
function median(values: number[]): number | null {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.round((sorted.length - 1) * 0.5)];
}

function worked(run: BenchRun, c: string, flag: Flag | undefined): CaseRow {
  const rows = run.rows.filter((r) => r.case === c && ran(r));
  const valid = rows.filter((r) => !r.invalid);
  const tokensUsed = valid.flatMap((r) => {
    const t = r.measured.tokens as TurnUsage | null | undefined;
    return t ? [sum(t)] : [];
  });
  const costs = rows.flatMap((r) => num(r.measured.cost_usd) ?? []);
  return {
    case: c,
    repeats: repeatsOf(run, c),
    passed: passedText(valid.filter((r) => r.pass).length, valid.length, rows.length - valid.length),
    before: null,
    tokens: median(tokensUsed),
    wallMs: median(valid.flatMap((r) => num(r.measured.wall_ms) ?? [])),
    cost: costs.length ? costs.reduce((a, b) => a + b, 0) : null,
    ...(flag ? { flag } : {}),
  };
}

/** The Flag column of the report's case table, by case. A case not run or in a new series has none. */
export function flagsFromReport(report: string | null | undefined): Record<string, Flag> {
  const out: Record<string, Flag> = {};
  for (const line of (report ?? "").split("\n")) {
    const cells = line.split("|").map((c) => c.trim());
    // "| case | passed | before | wall | first word | tokens | flag |" splits into 9 pieces.
    if (cells.length !== 9) continue;
    const m = /^(green|amber|red)(?::\s*(.*))?$/.exec(cells[7]);
    if (m) out[cells[1]] = { level: m[1] as Level, why: m[2] ? [m[2]] : [] };
  }
  return out;
}

/** The run's cost over its cases; null when no case said one. */
export function totalCost(rows: CaseRow[]): number | null {
  return rows.reduce<number | null>((a, r) => (r.cost === null ? a : (a ?? 0) + r.cost), null);
}

/** What the flags are measured against, for the hint under the table. */
export function comparedWith(run: BenchRun): string {
  // compat-until: 2026-12-12 (no `summary`: the card cannot name the run before)
  if (!run.summary) return "The flag compares with the last like run.";
  const b = run.summary.before;
  return b ? `The flag compares with the run on Hub ${b.hub_version}.` : "New series: no earlier like run to compare with.";
}

const verdictWords: Record<Verdict, string> = { pass: "passed", fail: "failed", invalid: "not counted", pending: "not run yet" };

/** A repeat's mark's tooltip: "Repeat 2: not counted: <why>". */
export function repeatTip(i: number, r: Repeat): string {
  return `Repeat ${i + 1}: ${verdictWords[r.verdict]}${r.why ? `: ${r.why}` : ""}`;
}

/** The colour class for a flag; none for no flag. */
export function flagClass(level: Level | undefined): string {
  return level ?? "";
}

type Choice<T> = { value: T | undefined; label: string };
const repeatsWord = (n: number) => `${n} ${n === 1 ? "repeat" : "repeats"}`;

/** The Engine choice; its first option (left out of the ask) names the suite's engine. */
export function engineChoices(names: string[], suite: BenchSuiteInfo | null): Choice<string>[] {
  // compat-until: 2026-12-12 (null: an older hubd without `bench.suites`)
  const first = suite ? `${suite.engine} (suite default)` : "Suite's engine";
  return [{ value: undefined, label: first }, ...names.map((e) => ({ value: e, label: e }))];
}

/** The Repeats choice; its first option (left out of the ask) names the suite's repeats. */
export function repeatChoices(suite: BenchSuiteInfo | null): Choice<number>[] {
  // compat-until: 2026-12-12 (null: an older hubd without `bench.suites`)
  const first = suite ? `${repeatsWord(suite.repeats)} (suite default)` : "Suite's repeats";
  return [{ value: undefined, label: first }, ...[1, 2, 3, 5].map((n) => ({ value: n, label: repeatsWord(n) }))];
}

/** What earlier runs like the chosen one took: the newest with its engine and repeats, the suite's filling in. */
export function estimateFor(suite: BenchSuiteInfo | null, engine: string | undefined, repeats: number | undefined): BenchEstimate | null {
  if (!suite) return null;
  const e = engine ?? suite.engine;
  const r = repeats ?? suite.repeats;
  return suite.estimates.find((x) => x.engine === e && x.repeats === r) ?? null;
}

export function estimateText(est: BenchEstimate | null): string {
  if (!est) return "No earlier run to estimate from.";
  const parts = [est.tokens === null ? null : `${tokens(est.tokens)} tokens`, est.cost_usd === null ? null : dollars(est.cost_usd)].filter((p) => p !== null);
  if (!parts.length) return "Earlier runs like this did not say what they took.";
  return `Earlier runs like this took about ${parts.join(", ")} (median of ${est.runs} ${est.runs === 1 ? "run" : "runs"}).`;
}

/** Where the run stops on its own; with the suite's cap when the suite is known. */
export function costNote(suite: BenchSuiteInfo | null): string {
  return `It stops at 80 % of the plan or at the suite's cost cap${suite ? ` of $${suite.cost_cap_usd.toFixed(2)}` : ""}.`;
}

/** A start or stop the server refused, as a sentence (its reason is already in plain words). */
export function refusal(err: unknown): string {
  const text = (err instanceof Error ? err.message : String(err)).trim();
  if (!text) return "Hub refused it without saying why.";
  return text[0].toUpperCase() + text.slice(1) + (/[.!?]$/.test(text) ? "" : ".");
}
