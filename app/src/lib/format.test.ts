// Tests for the pure helpers in format.ts.
import { describe, expect, it } from "vitest";
import { clip, dollars, shortVersion , threadProject } from "./format";

describe("dollars", () => {
  it("shows cents, fractions of a cent and nothing", () => {
    expect(dollars(0)).toBe("$0");
    expect(dollars(0.00001)).toBe("<$0.0001");
    expect(dollars(0.0042)).toBe("$0.0042");
    expect(dollars(1.234)).toBe("$1.23");
  });
});

describe("shortVersion", () => {
  it("cuts a hash to 7 characters and leaves dev alone", () => {
    expect(shortVersion("0123456789abcdef")).toBe("0123456");
    expect(shortVersion("dev")).toBe("dev");
  });
  it("uses the given text for an empty version", () => {
    expect(shortVersion(null)).toBe("?");
    expect(shortVersion(undefined, "")).toBe("");
  });
});

describe("clip", () => {
  it("keeps short text and cuts long text with an ellipsis", () => {
    expect(clip("abc", 5)).toBe("abc");
    expect(clip("abcdef ghij", 7)).toBe("abcdef…");
    expect(clip("abcde ghij", 6)).toBe("abcde…");
  });
});

describe("threadProject", () => {
  it("prefers what the core worked out and falls back to cwd", () => {
    expect(threadProject({ cwd: "/r/hub/.hub-threads/x", project: "/core/says" })).toBe("/core/says");
    expect(threadProject({ cwd: "/r/hub/.hub-threads/x" })).toBe("/r/hub");
  });
});
