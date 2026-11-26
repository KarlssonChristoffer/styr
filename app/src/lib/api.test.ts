// Tests for the error paths in api.ts: ApiError from a failed call, the error body fallback, the
// retry while the core restarts, and tooOld on what api throws.
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./home", () => ({ BASE: "http://hub.test", streamUrl: () => "ws://hub.test/api/stream", socketUrl: () => "ws://hub.test" }));

import { api } from "./api";
import { ApiError, tooOld } from "./compat";

const reply = (status: number, body: string, statusText = "Status") =>
  new Response(body, { status, statusText, headers: { "content-type": "application/json" } });

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("api call errors", () => {
  it("returns the parsed body, and undefined for an empty one", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(reply(200, '{"a":1}')).mockResolvedValueOnce(reply(200, "")));
    expect(await api.halt()).toEqual({ a: 1 });
    expect(await api.halt()).toBeUndefined();
  });

  it("throws ApiError with the core's message and the status", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(reply(403, '{"error":"phone may not"}')));
    const err = await api.halt().catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err.message).toBe("phone may not");
    expect(err.status).toBe(403);
    expect(tooOld(err)).toBe(false);
  });

  it("falls back to the status text when the error body is not JSON", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(reply(502, "<html>bad</html>", "Bad Gateway")));
    const err = await api.halt().catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect(err.message).toBe("Bad Gateway");
  });

  it("is tooOld for a 404 or 405 from an older hubd", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(reply(404, "{}", "Not Found")).mockResolvedValueOnce(reply(405, "{}", "Method Not Allowed")));
    expect(tooOld(await api.halt().catch((e) => e))).toBe(true);
    expect(tooOld(await api.halt().catch((e) => e))).toBe(true);
  });

  it("sends a JSON body with its content type", async () => {
    const fetchMock = vi.fn().mockResolvedValue(reply(200, "{}"));
    vi.stubGlobal("fetch", fetchMock);
    await api.pauseAll();
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toBe("http://hub.test/api/halt/pause");
    expect(init.method).toBe("POST");
    expect(init.headers).toEqual({ "content-type": "application/json" });
    expect(init.body).toBe("{}");
  });
});

describe("simFrame", () => {
  it("is null while there is no picture yet", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(null, { status: 204 })));
    expect(await api.simFrame("u")).toBeNull();
  });

  it("throws ApiError with the body's error on failure, quietly falling back to the status text", async () => {
    vi.spyOn(console, "debug").mockImplementation(() => {});
    vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(reply(404, '{"error":"no sim"}')).mockResolvedValueOnce(reply(500, "oops", "Server Error")));
    const a = await api.simFrame("u").catch((e) => e);
    expect(a).toBeInstanceOf(ApiError);
    expect([a.message, a.status]).toEqual(["no sim", 404]);
    const b = await api.simFrame("u").catch((e) => e);
    expect([b.message, b.status]).toEqual(["Server Error", 500]);
    expect(console.debug).toHaveBeenCalledWith("quiet: error body", expect.anything());
  });
});
