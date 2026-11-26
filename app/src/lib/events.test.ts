import { describe, expect, it } from "vitest";
import type { Event } from "./bindings/Event";
import { payload, typed } from "./events";

const event = (kind: Event["kind"], data: Event["data"]): Event => ({ thread_id: "t", seq: 1, ts: 0, kind, data });

describe("an event's payload by its kind", () => {
  it("is the payload when the kind is the one asked for", () => {
    const e = event("queue", { seq: 4, action: "steered" });
    expect(payload(e, "queue")?.action).toBe("steered");
    expect(payload(e, "restart")).toBeNull();
  });

  it("narrows in a switch on the kind", () => {
    const { kind, d } = typed(event("checkpoint", { commit: "abc", root: "/r" }));
    expect(kind === "checkpoint" ? d.commit : "").toBe("abc");
  });

  it("reads a payload that is not an object as empty", () => {
    expect(payload(event("reaction", null), "reaction")).toEqual({});
    expect(payload(event("reaction", ["x"]), "reaction")).toEqual({});
    expect(typed(event("unknown", "x")).d).toEqual({});
  });
});
