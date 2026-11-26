// Tests for bench.ts: a BenchRun turned into the Labs > Bench card's progress, rows and flags (from
// the server's summary, or worked out here for an older hubd), and a suite's labels and estimate.
import { describe, expect, it } from "vitest";
import type { BenchRun } from "./bindings/BenchRun";
import type { BenchRow } from "./bindings/BenchRow";
import type { BenchSuiteInfo } from "./bindings/BenchSuiteInfo";
import type { BenchCaseSummary } from "./bindings/BenchCaseSummary";
import {
  caseRows,
  comparedWith,
  costNote,
  engineChoices,
  estimateFor,
  estimateText,
  flagClass,
  flagsFromReport,
  progress,
  refusal,
  repeatChoices,
  repeatTip,
  runLine,
  stateWords,
  totalCost,
} from "./bench";

const row = (c: string, repeat: number, pass: boolean, measured: Record<string, unknown> = {}, extra: Partial<BenchRow> = {}): BenchRow => ({
  case: c,
  repeat,
  state: "done",
  pass,
  checks: [],
  measured,
  turn_usage: [],
  ...extra,
});

const run = (over: Partial<BenchRun> = {}): BenchRun => ({
  id: "1-smoke",
  suite: "smoke",
  suite_hash: "abc",
  engine: "claude",
  model_asked: "haiku",
  machine: "pc",
  hub_version: "1.0",
  started: 0,
  state: "done",
  repeats: 2,
  cases: ["edit", "ask"],
  rows: [],
  ...over,
});

const caseSummary = (over: Partial<BenchCaseSummary> & { case: string }): BenchCaseSummary => ({
  passed: 0,
  runs: 0,
  invalid: 0,
  wall_ms: null,
  first_word_ms: null,
  tokens: null,
  cost_usd: null,
  before: null,
  flag: null,
  ...over,
});

const suite = (over: Partial<BenchSuiteInfo> = {}): BenchSuiteInfo => ({
  name: "features",
  hash: "h",
  cases: ["files-in-chat", "side-job", "handoff", "branch-commit"],
  engine: "claude",
  model: "fast",
  repeats: 3,
  timeout_s: 600,
  cost_cap_usd: 5,
  estimates: [],
  ...over,
});

const REPORT = `# Bench: smoke on claude

| Case | Passed | Before | Wall (median, min-max) | First word | Tokens | Flag |
|---|---|---|---|---|---|---|
| edit | 2/2 | 2/2 | 1.0 s (1.0 s-1.0 s) | - | - | green |
| ask | 1/2 | 2/2 | 1.0 s (1.0 s-1.0 s) | - | - | amber: passed 1 of 2, was 2 of 2 |
| gone | not run | | | | | |

Cost: $0.12.
`;

describe("progress", () => {
  it("counts the repeats that ran and names the case in hand", () => {
    const r = run({ state: "running", rows: [row("edit", 1, true), row("edit", 2, false)] });
    expect(progress(r)).toEqual({ done: 2, total: 4, text: "Case 2 of 2 (ask) · 2 of 4 repeats done" });
  });
  it("leaves out an interrupted repeat, which runs again", () => {
    const r = run({ state: "running", rows: [row("edit", 1, true, {}, { state: "interrupted" })] });
    expect(progress(r).done).toBe(0);
    expect(progress(r).text).toBe("Case 1 of 2 (edit) · 0 of 4 repeats done");
  });
});

describe("caseRows from the server's summary", () => {
  const rows = [
    row("edit", 2, false, { wall_ms: 3000 }),
    row("edit", 1, true, { wall_ms: 1000 }),
    row("ask", 1, false, {}, { invalid: "needs one of your tools that takes handoffs", note: "not run: needs one of your tools that takes handoffs" }),
  ];
  const summary = {
    before: { id: "0-smoke", hub_version: "0.9" },
    cases: [
      caseSummary({
        case: "edit",
        passed: 1,
        runs: 2,
        wall_ms: { median: 2500, min: 1000, max: 3000 },
        tokens: { median: 1200, min: 15, max: 1200 },
        cost_usd: 0.04,
        before: { passed: 2, runs: 2 },
        flag: { level: "amber", why: ["passed 1 of 2, was 2 of 2", "wall time up"] },
      }),
      caseSummary({ case: "ask", invalid: 1 }),
    ],
  };
  const [edit, ask] = caseRows(run({ rows, summary }), null);

  it("takes k/n, medians, cost and the flag from the summary, not the rows", () => {
    expect(edit.passed).toBe("1/2");
    expect(edit.tokens).toBe(1200);
    expect(edit.wallMs).toBe(2500);
    expect(edit.cost).toBeCloseTo(0.04);
    expect(edit.flag).toEqual({ level: "amber", why: ["passed 1 of 2, was 2 of 2", "wall time up"] });
    expect(edit.before).toBe("2/2");
  });
  it("still takes each repeat's verdict and reason from the rows", () => {
    expect(edit.repeats.map((r) => r.verdict)).toEqual(["pass", "fail"]);
    expect(ask.repeats.map((r) => r.verdict)).toEqual(["invalid", "pending"]);
    expect(ask.repeats[0].why).toBe("needs one of your tools that takes handoffs");
  });
  it("counts invalid repeats apart and has no flag for a case without one", () => {
    expect(ask.passed).toBe("0/0 (1 invalid)");
    expect(ask.flag).toBeUndefined();
    expect(ask.before).toBeNull();
    expect(ask.cost).toBeNull();
  });
  it("says not run for a case with no repeat yet", () => {
    const [, r] = caseRows(run({ summary: { before: null, cases: [caseSummary({ case: "edit" }), caseSummary({ case: "ask" })] } }), null);
    expect(r.passed).toBe("not run");
  });
});

// compat-until: 2026-12-12 (an older hubd sends no `summary`)
describe("caseRows worked out here, for a run without a summary", () => {
  it("gives the same row shape: verdicts, medians, summed cost and the report's flag", () => {
    const r = run({
      rows: [
        row("edit", 2, false, { wall_ms: 3000, tokens: { input: 10, output: 5 }, cost_usd: 0.02 }),
        row("edit", 1, true, { wall_ms: 1000, tokens: { input: 100, cache_read: 900 }, cost_usd: 0.01 }),
        row("ask", 1, true, {}, { invalid: "another model answered" }),
      ],
    });
    const [edit, ask] = caseRows(r, REPORT);
    expect(edit.repeats.map((r) => r.verdict)).toEqual(["pass", "fail"]);
    expect(edit.passed).toBe("1/2");
    expect(edit.tokens).toBe(1000);
    expect(edit.wallMs).toBe(3000);
    expect(edit.cost).toBeCloseTo(0.03);
    expect(edit.flag).toEqual({ level: "green", why: [] });
    expect(edit.before).toBeNull();
    expect(ask.repeats.map((r) => r.verdict)).toEqual(["invalid", "pending"]);
    expect(ask.repeats[0].why).toBe("another model answered");
    expect(ask.passed).toBe("0/0 (1 invalid)");
    expect(ask.tokens).toBeNull();
    expect(ask.cost).toBeNull();
    expect(ask.flag).toEqual({ level: "amber", why: ["passed 1 of 2, was 2 of 2"] });
  });
  it("reads the report's Flag column, skipping cases not run or a new series", () => {
    const flags = flagsFromReport(REPORT);
    expect(flags.edit).toEqual({ level: "green", why: [] });
    expect(flags.ask).toEqual({ level: "amber", why: ["passed 1 of 2, was 2 of 2"] });
    expect(flags.gone).toBeUndefined();
    expect(flagsFromReport("| a | 1/1 | new | - | - | - | new series |").a).toBeUndefined();
    expect(flagsFromReport(null)).toEqual({});
  });
});

describe("table words", () => {
  it("sums the cost over the cases that said one", () => {
    expect(totalCost(caseRows(run({ summary: { before: null, cases: [caseSummary({ case: "edit", cost_usd: 0.1 }), caseSummary({ case: "ask", cost_usd: 0.2 })] } }), null))).toBeCloseTo(0.3);
    expect(totalCost(caseRows(run(), null))).toBeNull();
  });
  it("says what the run is compared with", () => {
    expect(comparedWith(run({ summary: { before: { id: "x", hub_version: "0.9.1" }, cases: [] } }))).toBe("The flag compares with the run on Hub 0.9.1.");
    expect(comparedWith(run({ summary: { before: null, cases: [] } }))).toBe("New series: no earlier like run to compare with.");
    expect(comparedWith(run())).toBe("The flag compares with the last like run.");
  });
  it("gives a repeat's tooltip with its reason when it has one", () => {
    expect(repeatTip(0, { verdict: "invalid", why: "needs one of your tools that takes handoffs" })).toBe(
      "Repeat 1: not counted: needs one of your tools that takes handoffs",
    );
    expect(repeatTip(1, { verdict: "pass" })).toBe("Repeat 2: passed");
    expect(repeatTip(2, { verdict: "fail", why: "could not start its thread" })).toBe("Repeat 3: failed: could not start its thread");
  });
});

describe("suite choices", () => {
  it("names the suite's own defaults in the default options", () => {
    expect(engineChoices(["claude", "codex"], suite())[0]).toEqual({ value: undefined, label: "claude (suite default)" });
    expect(engineChoices(["claude", "codex"], suite()).slice(1).map((c) => c.value)).toEqual(["claude", "codex"]);
    expect(repeatChoices(suite())[0]).toEqual({ value: undefined, label: "3 repeats (suite default)" });
    expect(repeatChoices(suite({ repeats: 1 }))[0].label).toBe("1 repeat (suite default)");
  });
  // compat-until: 2026-12-12 (an older hubd has no suite list)
  it("falls back to the suite's words without a suite list", () => {
    expect(engineChoices(["claude"], null)[0].label).toBe("Suite's engine");
    expect(repeatChoices(null)[0].label).toBe("Suite's repeats");
    expect(repeatChoices(null).slice(1).map((c) => c.value)).toEqual([1, 2, 3, 5]);
  });
});

describe("estimate", () => {
  const est = (engine: string, repeats: number, over: object = {}) => ({ engine, model: "haiku", repeats, runs: 2, tokens: 120_000, cost_usd: 0.42, ...over });
  const s = suite({ estimates: [est("codex", 3, { runs: 1 }), est("claude", 1), est("claude", 3, { runs: 4 }), est("claude", 3, { runs: 9 })] });

  it("takes the newest one for the chosen engine and repeats, the suite's filling what is left out", () => {
    expect(estimateFor(s, undefined, undefined)?.runs).toBe(4);
    expect(estimateFor(s, "codex", undefined)?.runs).toBe(1);
    expect(estimateFor(s, "claude", 1)?.repeats).toBe(1);
    expect(estimateFor(s, "codex", 5)).toBeNull();
    expect(estimateFor(null, undefined, undefined)).toBeNull();
  });
  it("says it in words, or that there is nothing to go on", () => {
    expect(estimateText(est("claude", 3, { runs: 4 }))).toBe("Earlier runs like this took about 120k tokens, $0.42 (median of 4 runs).");
    expect(estimateText(est("claude", 3, { runs: 1, cost_usd: null }))).toBe("Earlier runs like this took about 120k tokens (median of 1 run).");
    expect(estimateText(est("claude", 3, { tokens: null, cost_usd: null }))).toBe("Earlier runs like this did not say what they took.");
    expect(estimateText(null)).toBe("No earlier run to estimate from.");
  });
  it("names the cost cap of the suite", () => {
    expect(costNote(suite())).toBe("It stops at 80 % of the plan or at the suite's cost cap of $5.00.");
    expect(costNote(null)).toBe("It stops at 80 % of the plan or at the suite's cost cap.");
  });
});

describe("words", () => {
  it("says the state and why", () => {
    expect(stateWords(run({ state: "cost_cap", why: "spent $5" }))).toBe("cost cap: spent $5");
    expect(stateWords(run())).toBe("finished");
  });
  it("puts a listed run (the list has no rows) on one line", () => {
    expect(runLine(run())).toBe("smoke on claude · haiku · 2 cases × 2 · finished");
  });
  it("maps a flag to its colour class", () => {
    expect(flagClass("red")).toBe("red");
    expect(flagClass(undefined)).toBe("");
  });
  it("turns a refusal into a sentence", () => {
    expect(refusal(new Error("a bench run is going already (x); wait for it"))).toBe("A bench run is going already (x); wait for it.");
    expect(refusal("plain")).toBe("Plain.");
  });
});
