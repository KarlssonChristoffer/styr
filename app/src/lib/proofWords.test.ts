// What the proof stamp says, for a test run, a look at a page and a `via` this app does not know.
import { describe, expect, it } from "vitest";
import { proofWords } from "./proofWords";
import type { ProofState } from "./transcriptItems";

describe("proofWords", () => {
  it("a failed test run reads as failed tests", () => {
    expect(proofWords({ state: "failed", via: "tests", command: "cargo test" })).toEqual({ tone: "bad", text: "tests failed", title: "Failed: cargo test" });
  });

  it("a failed look at a page reads as a page with errors", () => {
    const w = proofWords({ state: "failed", via: "screenshot", command: "page_check index.html" });
    expect(w).toEqual({ tone: "bad", text: "page has errors", title: "Errors: page_check index.html" });
  });

  it("Hub asking to fix a page says so, and a passing look reads as a screenshot", () => {
    expect(proofWords({ state: "asked", why: "failed", via: "screenshot", command: "page_check index.html" }).text).toBe("fixing page errors…");
    expect(proofWords({ state: "asked", why: "failed", via: "tests", command: "npm test" }).text).toBe("fixing failed tests…");
    expect(proofWords({ state: "shown", via: "screenshot", command: "page_check index.html" })).toMatchObject({ tone: "done", text: "screenshot" });
    expect(proofWords({ state: "shown", via: "tests" })).toMatchObject({ tone: "done", text: "tested" });
  });

  it("an unknown or missing via keeps the words for a test run", () => {
    const odd = { state: "failed", via: "video", command: "x" } as unknown as ProofState;
    expect(proofWords(odd).text).toBe("tests failed");
    expect(proofWords({ state: "failed" }).text).toBe("tests failed");
    expect(proofWords({ state: "shown", via: "video" } as unknown as ProofState).text).toBe("tested");
  });

  it("a run that exited cleanly but printed failures stays a test failure", () => {
    expect(proofWords({ state: "failed", via: "tests", hidden: true, command: "npm test" }).title).toBe("npm test exited cleanly but printed failures");
  });

  it("skipped and missing keep their words", () => {
    expect(proofWords({ state: "skipped", why: "waived" })).toMatchObject({ tone: "none", text: "no proof needed" });
    expect(proofWords({ state: "missing" })).toMatchObject({ tone: "none", text: "no proof" });
  });
});
