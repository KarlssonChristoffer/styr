// Tests for quiet.ts: a failed call gives the fallback and leaves one console.debug line.
import { afterEach, describe, expect, it, vi } from "vitest";
import { quiet } from "./quiet";

afterEach(() => vi.restoreAllMocks());

describe("quiet", () => {
  it("passes a value through without logging", async () => {
    const debug = vi.spyOn(console, "debug").mockImplementation(() => {});
    expect(await quiet(Promise.resolve(3), "x")).toBe(3);
    expect(debug).not.toHaveBeenCalled();
  });

  it("gives undefined when it fails and no fallback is given, and says what failed", async () => {
    const debug = vi.spyOn(console, "debug").mockImplementation(() => {});
    const err = new Error("no");
    expect(await quiet(Promise.reject(err), "refresh")).toBeUndefined();
    expect(debug).toHaveBeenCalledWith("quiet: refresh", err);
  });

  it("gives the fallback when it fails, even a falsy one", async () => {
    vi.spyOn(console, "debug").mockImplementation(() => {});
    expect(await quiet(Promise.reject(new Error("no")), "x", null)).toBeNull();
    expect(await quiet(Promise.reject(new Error("no")), "x", 0)).toBe(0);
  });
});
