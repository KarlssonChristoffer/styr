import { describe, expect, it } from "vitest";
import { pushGapChoices, pushGapLabel, pushGapOf } from "./pushGap";

describe("pushGap", () => {
  it("says a gap in words", () => {
    expect(pushGapLabel(0)).toBe("As they come");
    expect(pushGapLabel(30)).toBe("At most every 30 min");
    expect(pushGapLabel(60)).toBe("At most every hour");
    expect(pushGapLabel(120)).toBe("At most every 2 hours");
  });

  it("keeps a value set by hand among the choices, in order", () => {
    expect(pushGapChoices(30).map((c) => c.value)).toEqual([0, 15, 30, 60, 120]);
    expect(pushGapChoices(45).map((c) => c.value)).toEqual([0, 15, 30, 45, 60, 120]);
  });

  it("reads a host from an older hubd as no gap", () => {
    expect(pushGapOf({ name: "tincan" })).toBe(0);
    expect(pushGapOf({ push_quiet_mins: null })).toBe(0);
    expect(pushGapOf({ push_quiet_mins: 30 })).toBe(30);
    expect(pushGapOf(undefined)).toBe(0);
  });
});
