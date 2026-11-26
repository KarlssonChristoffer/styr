// Tests for compat.ts: telling "this hubd lacks the call" from a real error, and the feature list.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("./home", () => ({ BASE: "http://hub.test" }));

const json = (body: unknown, ok = true, status = 200) => ({ ok, status, json: async () => body }) as Response;

describe("ApiError and tooOld", () => {
  it("carries the status and a name", async () => {
    const { ApiError } = await import("./compat");
    const e = new ApiError("nope", 418);
    expect(e).toBeInstanceOf(Error);
    expect(e.status).toBe(418);
    expect(e.name).toBe("ApiError");
    expect(e.message).toBe("nope");
  });

  it("is true for 404 and 405 only", async () => {
    const { ApiError, tooOld } = await import("./compat");
    expect(tooOld(new ApiError("x", 404))).toBe(true);
    expect(tooOld(new ApiError("x", 405))).toBe(true);
    for (const s of [400, 403, 500]) expect(tooOld(new ApiError("x", s))).toBe(false);
  });

  it("is false for anything that is not an ApiError", async () => {
    const { tooOld } = await import("./compat");
    expect(tooOld(new Error("404"))).toBe(false);
    expect(tooOld({ status: 404 })).toBe(false);
    expect(tooOld(null)).toBe(false);
  });
});

describe("hubdFeatures and hubdHas", () => {
  beforeEach(() => vi.resetModules());
  afterEach(() => vi.unstubAllGlobals());

  it("reads the feature list from /api/health once and keeps it", async () => {
    const fetchMock = vi.fn().mockResolvedValue(json({ features: ["a", "b"] }));
    vi.stubGlobal("fetch", fetchMock);
    const { hubdFeatures, hubdHas } = await import("./compat");
    expect([...(await hubdFeatures())!]).toEqual(["a", "b"]);
    expect(await hubdHas("a")).toBe(true);
    expect(await hubdHas("zzz")).toBe(false);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe("http://hub.test/api/health");
  });

  it("ignores non-string entries", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(json({ features: ["a", 3, null, "b"] })));
    const { hubdFeatures } = await import("./compat");
    expect([...(await hubdFeatures())!]).toEqual(["a", "b"]);
  });

  it("is an empty set (core only) for a hubd from before the list", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(json({})));
    const { hubdFeatures, hubdHas } = await import("./compat");
    expect((await hubdFeatures())!.size).toBe(0);
    expect(await hubdHas("anything")).toBe(false);
  });

  it("is null and counts as present when it could not ask, and asks again next time", async () => {
    const fetchMock = vi.fn().mockRejectedValueOnce(new Error("down")).mockResolvedValueOnce(json({ features: ["a"] }));
    vi.stubGlobal("fetch", fetchMock);
    const { hubdFeatures, hubdHas } = await import("./compat");
    expect(await hubdFeatures()).toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    // The next ask goes to the hubd again (the failure was not remembered) and now has an answer.
    expect(await hubdHas("whatever")).toBe(false);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("treats a non-ok answer like not being able to ask, and counts every feature as present", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(json({}, false, 500)));
    const { hubdFeatures, hubdHas } = await import("./compat");
    expect(await hubdFeatures()).toBeNull();
    expect(await hubdHas("whatever")).toBe(true);
  });
});
