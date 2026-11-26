import { describe, expect, it } from "vitest";
import { KEEP_LOGS, toDrop } from "./logCache";

const ids = (n: number) => Array.from({ length: n }, (_, i) => `t${i}`);

describe("toDrop", () => {
  it("drops nothing while the logs fit", () => {
    expect(toDrop(ids(KEEP_LOGS), ids(KEEP_LOGS), new Set())).toEqual([]);
  });

  it("drops the least recently opened beyond the limit", () => {
    const held = ids(KEEP_LOGS + 2);
    expect(toDrop(held, held, new Set())).toEqual(["t0", "t1"]);
  });

  it("lets go of logs nobody opened before those that were", () => {
    const held = ["a", "b", ...ids(KEEP_LOGS)];
    expect(toDrop(held, ids(KEEP_LOGS), new Set())).toEqual(["a", "b"]);
  });

  it("never drops a log that must stay, and does not count it", () => {
    const held = ids(KEEP_LOGS + 1);
    expect(toDrop(held, held, new Set(["t0"]))).toEqual([]);
    expect(toDrop([...held, "x"], [...held, "x"], new Set(["t0"]))).toEqual(["t1"]);
  });
});
