import { describe, expect, it } from "vitest";
import { projectCounts } from "./threadFlags";
import type { Thread } from "./bindings/Thread";

const t = (o: Partial<Thread>) => ({ status: "idle", settled_at: null, ...o }) as Thread;

describe("projectCounts", () => {
  it("counts running, asking and failed threads, and skips a settled asker", () => {
    const c = projectCounts([
      t({ status: "running" }),
      t({ status: "waiting_on_you" }),
      t({ display: "asks" }),
      t({ display: "asks", settled_at: 5 }),
      t({ status: "error" }),
      t({}),
    ]);
    expect(c).toEqual({ running: 1, asks: 2, failed: 1 });
  });
});
