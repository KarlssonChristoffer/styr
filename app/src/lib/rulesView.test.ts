// Tests for rulesView: grouping, origin and recency of standing rules.
import { describe, expect, it } from "vitest";
import type { StandingRule } from "./bindings/StandingRule";
import { byTheme, origin, pick, recent } from "./rulesView";

const rule = (text: string, o: Partial<StandingRule> = {}): StandingRule => ({ id: text, text, theme: "other", source: "repo", file: "CLAUDE.md", line: 1, since: null, commit: "", why: "", thread_id: "", thread: "", ...o });
const DAY = 24 * 3600;

describe("rulesView", () => {
  const rules = [rule("Merge it yourself", { theme: "merge", since: 100 * DAY }), rule("Run cargo test", { theme: "test", since: 90 * DAY, file: "AGENTS.md" }), rule("No date")];

  it("finds every typed word in the text or file and filters by theme", () => {
    expect(pick(rules, "merge yourself", "all").map((r) => r.text)).toEqual(["Merge it yourself"]);
    expect(pick(rules, "agents", "all").map((r) => r.text)).toEqual(["Run cargo test"]);
    expect(pick(rules, "", "test")).toHaveLength(1);
    expect(pick(rules, "", "all")).toHaveLength(3);
  });

  it("lists the week's changes newest first and skips undated rules", () => {
    expect(recent(rules, 101 * DAY).map((r) => r.text)).toEqual(["Merge it yourself"]);
    expect(recent(rules, 96 * DAY).map((r) => r.text)).toEqual(["Merge it yourself", "Run cargo test"]);
  });

  it("groups by theme in order and sends an unknown theme to other", () => {
    const g = byTheme([...rules, rule("odd", { theme: "zzz" })]);
    expect(g.map((x) => x.id)).toEqual(["merge", "test", "other"]);
    expect(g[2].rules).toHaveLength(2);
  });

  it("says where a rule comes from", () => {
    expect(origin(rule("a"))).toBe("CLAUDE.md");
    expect(origin(rule("a", { source: "hub" }))).toBe("Said in chat");
    expect(origin(rule("a", { source: "hub", thread: "Fix tabs" }))).toBe("Fix tabs");
  });
});
